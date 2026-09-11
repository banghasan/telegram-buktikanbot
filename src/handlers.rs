use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Datelike, Duration as ChronoDuration, TimeZone, Utc};
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, CallbackQueryId, ChatId, ChatMemberStatus, ChatMemberUpdated, ChatPermissions,
    InlineKeyboardButton, InlineKeyboardMarkup, InputFile, InputMedia, InputMediaPhoto,
    LinkPreviewOptions, MaybeInaccessibleMessage, Message, MessageId, ParseMode, ReplyParameters,
    ThreadId, User, UserId,
};
use url::Url;

use crate::ban_release::{
    BanReleaseJob, BanReleaseStore, StatsEvent, StatsEventType, StatsGroup, StatsReport,
};
use crate::captcha::{
    CaptchaChatContext, CaptchaCheck, CaptchaSession, PendingCaptcha, SharedState, captcha_caption,
    check_captcha_answer_for_message_at, generate_captcha, generate_captcha_options,
    make_pending_captcha,
};
use crate::config::{Config, LogLevel};
use crate::logging::{
    chat_context, log_message, log_system_level, log_telegram_error, log_user_event_by_display,
    log_user_event_with_chat,
};
use crate::utils::{escape_html, sanitize_log_text};

const ADMIN_PENDING_PAGE_SIZE: i64 = 5;

struct BanRequest {
    chat_id: ChatId,
    user_id: UserId,
    chat_title: Option<String>,
    chat_username: Option<String>,
    user_name: String,
    user_username: Option<String>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
    error_context: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptchaFailureReason {
    SetupFailed,
    Timeout,
    AttemptsExceeded,
}

impl CaptchaFailureReason {
    fn code(self) -> &'static str {
        match self {
            Self::SetupFailed => "setup_failed",
            Self::Timeout => "timeout",
            Self::AttemptsExceeded => "attempts_exceeded",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::SetupFailed => "verifikasi tidak dapat dimulai",
            Self::Timeout => "waktu habis",
            Self::AttemptsExceeded => "jumlah percobaan habis",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptchaLogOutcome {
    Success {
        attempts_used: usize,
        attempts_total: usize,
    },
    Failure {
        reason: CaptchaFailureReason,
        attempts_used: usize,
        attempts_total: usize,
        ban_release_at: Option<i64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CaptchaLogReference {
    chat_id: i64,
    message_thread_id: Option<i32>,
    message_id: i32,
}

#[derive(Clone, Copy, Debug)]
struct CaptchaLogContext<'a> {
    user: &'a teloxide::types::User,
    chat_id: ChatId,
    chat_title: Option<&'a str>,
    chat_username: Option<&'a str>,
}

struct CaptchaFailureLog<'a> {
    context: CaptchaLogContext<'a>,
    reason: CaptchaFailureReason,
    attempts_used: usize,
    attempts_total: usize,
    ban_release_at: Option<i64>,
    store: Option<&'a BanReleaseStore>,
}

async fn record_stats_event_if_available(
    config: &Config,
    store: Option<&BanReleaseStore>,
    event: StatsEvent,
    error_context: &str,
) {
    let Some(store) = store else {
        return;
    };
    if let Err(err) = store.record_stats_event(event).await {
        log_system_level(config, LogLevel::Warn, &format!("{error_context}: {err}"));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BanResult {
    banned: bool,
    release_at: Option<i64>,
}

impl BanRequest {
    fn from_pending(
        chat_id: ChatId,
        user_id: UserId,
        pending: &PendingCaptcha,
        ban_release_store: Option<Arc<BanReleaseStore>>,
        error_context: &'static str,
    ) -> Self {
        Self {
            chat_id,
            user_id,
            chat_title: pending.chat_title.clone(),
            chat_username: pending.chat_username.clone(),
            user_name: pending.user_name.clone(),
            user_username: pending.user_username.clone(),
            ban_release_store,
            error_context,
        }
    }
}

pub async fn on_new_members(
    bot: Bot,
    msg: Message,
    state: SharedState,
    config: Arc<Config>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(members) = msg.new_chat_members() else {
        return Ok(());
    };

    if config.delete_join_message {
        let _ = bot.delete_message(msg.chat.id, msg.id).await;
    }

    log_message(&config, &msg);

    let (chat_title, chat_username) = chat_context(&msg.chat);
    let chat = CaptchaChatContext::new(chat_title, chat_username);
    for member in members {
        start_captcha_for_user(
            &bot,
            msg.chat.id,
            chat.clone(),
            member.clone(),
            &state,
            &config,
            &ban_release_store,
        )
        .await?;
    }

    Ok(())
}

pub async fn on_left_member(
    bot: Bot,
    msg: Message,
    config: Arc<Config>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if msg.left_chat_member().is_none() {
        return Ok(());
    }
    if config.delete_left_message {
        let _ = bot.delete_message(msg.chat.id, msg.id).await;
    }
    log_message(&config, &msg);
    Ok(())
}

pub async fn on_chat_member_updated(
    bot: Bot,
    update: ChatMemberUpdated,
    state: SharedState,
    config: Arc<Config>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let old_status = update.old_chat_member.status();
    let new_status = update.new_chat_member.status();
    let joined = matches!(
        old_status,
        ChatMemberStatus::Left | ChatMemberStatus::Banned
    ) && matches!(
        new_status,
        ChatMemberStatus::Member | ChatMemberStatus::Restricted | ChatMemberStatus::Administrator
    );
    if !joined {
        return Ok(());
    }

    let user = update.new_chat_member.user;
    let (chat_title, chat_username) = chat_context(&update.chat);
    let chat = CaptchaChatContext::new(chat_title, chat_username);
    start_captcha_for_user(
        &bot,
        update.chat.id,
        chat,
        user,
        &state,
        &config,
        &ban_release_store,
    )
    .await?;
    Ok(())
}

async fn start_captcha_for_user(
    bot: &Bot,
    chat_id: ChatId,
    chat: CaptchaChatContext,
    user: teloxide::types::User,
    state: &SharedState,
    config: &Arc<Config>,
    ban_release_store: &Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if user.is_bot {
        return Ok(());
    }

    {
        let guard = state.lock().await;
        if guard.contains_key(&(chat_id, user.id)) {
            return Ok(());
        }
    }

    let Some(store) = ban_release_store.as_ref() else {
        return Err("captcha state store unavailable".into());
    };

    let no_permissions = ChatPermissions::empty();
    if let Err(err) = bot
        .restrict_chat_member(chat_id, user.id, no_permissions)
        .await
    {
        log_telegram_error(
            config,
            LogLevel::Error,
            chat_id,
            chat.title.as_deref(),
            chat.username.as_deref(),
            "failed to restrict user",
            &err,
        );
        let ban_result = ban_user_and_maybe_release(
            bot,
            config,
            BanRequest {
                chat_id,
                user_id: user.id,
                chat_title: chat.title.clone(),
                chat_username: chat.username.clone(),
                user_name: crate::utils::format_user_name(&user),
                user_username: user.username.clone(),
                ban_release_store: ban_release_store.clone(),
                error_context: "failed to restrict user; fallback ban failed",
            },
        )
        .await;
        if ban_result.banned {
            log_captcha_failure_and_attach(
                bot,
                config,
                CaptchaFailureLog {
                    context: CaptchaLogContext {
                        user: &user,
                        chat_id,
                        chat_title: chat.title.as_deref(),
                        chat_username: chat.username.as_deref(),
                    },
                    reason: CaptchaFailureReason::SetupFailed,
                    attempts_used: 0,
                    attempts_total: config.captcha_attempts,
                    ban_release_at: ban_result.release_at,
                    store: ban_release_store.as_deref(),
                },
            )
            .await;
        }
        return Ok(());
    }

    let (code, png) = generate_captcha(
        config.captcha_len,
        config.captcha_width,
        config.captcha_height,
    )?;

    let caption = captcha_caption(
        &user,
        config.captcha_timeout_secs,
        config.captcha_attempts,
        config.captcha_attempts,
    );
    let options = generate_captcha_options(&code, config.captcha_option_count);
    let keyboard = build_captcha_keyboard(&options, config.captcha_option_digits_to_emoji);
    let sent = bot
        .send_photo(chat_id, InputFile::memory(png))
        .caption(caption)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    let mut pending = make_pending_captcha(
        code,
        sent.id,
        options,
        config.captcha_attempts,
        config.captcha_timeout_secs,
        &user,
        &chat,
    );
    pending.expires_at = Utc::now().timestamp() + config.captcha_timeout_secs as i64;

    let session = match CaptchaSession::from_pending(chat_id, &user, &pending) {
        Ok(session) => session,
        Err(err) => {
            log_system_level(
                config,
                LogLevel::Error,
                &format!("failed to encode captcha session: {err}"),
            );
            if let Err(delete_err) = bot.delete_message(chat_id, sent.id).await {
                log_telegram_error(
                    config,
                    LogLevel::Warn,
                    chat_id,
                    chat.title.as_deref(),
                    chat.username.as_deref(),
                    "failed to delete captcha message after encoding failure",
                    &delete_err,
                );
            }
            if let Err(restore_err) = restore_chat_permissions(bot, chat_id, user.id).await {
                log_telegram_error(
                    config,
                    LogLevel::Error,
                    chat_id,
                    chat.title.as_deref(),
                    chat.username.as_deref(),
                    "failed to restore permissions after encoding failure",
                    &restore_err,
                );
            }
            return Err(err);
        }
    };
    let stats_event = StatsEvent {
        occurred_at: Utc::now().timestamp(),
        event_type: StatsEventType::CaptchaStarted,
        chat_id: session.chat_id,
        user_id: session.user_id,
        chat_title: session.chat_title.clone(),
        chat_username: session.chat_username.clone(),
        attempts_used: Some(0),
        attempts_total: Some(session.attempts_total),
        failure_reason: None,
    };
    if let Err(err) = store.save_captcha_session(session).await {
        log_system_level(
            config,
            LogLevel::Error,
            &format!("failed to persist captcha session: {err}"),
        );
        if let Err(delete_err) = bot.delete_message(chat_id, sent.id).await {
            log_telegram_error(
                config,
                LogLevel::Warn,
                chat_id,
                chat.title.as_deref(),
                chat.username.as_deref(),
                "failed to delete captcha message after persistence failure",
                &delete_err,
            );
        }
        if let Err(restore_err) = restore_chat_permissions(bot, chat_id, user.id).await {
            log_telegram_error(
                config,
                LogLevel::Error,
                chat_id,
                chat.title.as_deref(),
                chat.username.as_deref(),
                "failed to restore permissions after captcha persistence failure",
                &restore_err,
            );
        }
        return Err(format!("failed to persist captcha session: {err}").into());
    }

    {
        let mut guard = state.lock().await;
        guard.insert((chat_id, user.id), pending);
    }
    record_stats_event_if_available(
        config,
        Some(store.as_ref()),
        stats_event,
        "failed to record captcha start statistic",
    )
    .await;
    log_user_event_with_chat(
        config,
        &user,
        chat_id,
        chat.title.as_deref(),
        chat.username.as_deref(),
        "-> ⏳ captcha sent",
    );
    spawn_captcha_lifecycle(bot, state, config, store.clone(), chat_id, user, sent.id);

    Ok(())
}

fn spawn_captcha_lifecycle(
    bot: &Bot,
    state: &SharedState,
    config: &Arc<Config>,
    store: Arc<BanReleaseStore>,
    chat_id: ChatId,
    user: teloxide::types::User,
    captcha_message_id: teloxide::types::MessageId,
) {
    let bot = bot.clone();
    let state = state.clone();
    let config = config.clone();
    let user_id = user.id;
    let update_secs = config.captcha_caption_update_secs.max(1);

    tokio::spawn(async move {
        loop {
            let Some(expires_at) = ({
                let guard = state.lock().await;
                guard
                    .get(&(chat_id, user_id))
                    .map(|pending| pending.expires_at)
            }) else {
                return;
            };
            let remaining = expires_at.saturating_sub(Utc::now().timestamp()) as u64;
            if remaining > 0 {
                tokio::time::sleep(Duration::from_secs(update_secs.min(remaining))).await;
            }

            let options_state = {
                let mut guard = state.lock().await;
                let Some(pending) = guard.get_mut(&(chat_id, user_id)) else {
                    return;
                };
                let remaining = pending.expires_at.saturating_sub(Utc::now().timestamp()) as u64;
                pending.remaining_secs = remaining;
                (
                    pending.options.clone(),
                    pending.attempts_left,
                    pending.attempts_total,
                    remaining,
                )
            };
            let (options, attempts_left, attempts_total, remaining) = options_state;
            let caption = captcha_caption(&user, remaining, attempts_left, attempts_total);
            let _ = bot
                .edit_message_caption(chat_id, captcha_message_id)
                .caption(caption)
                .parse_mode(ParseMode::Html)
                .reply_markup(build_captcha_keyboard(
                    &options,
                    config.captcha_option_digits_to_emoji,
                ))
                .await;
            if remaining == 0 {
                break;
            }
        }

        let pending = {
            let mut guard = state.lock().await;
            guard.remove(&(chat_id, user_id))
        };

        if let Some(pending) = pending {
            let ban_result = ban_user_and_maybe_release(
                &bot,
                &config,
                BanRequest::from_pending(
                    chat_id,
                    user_id,
                    &pending,
                    Some(store.clone()),
                    "failed to ban user on timeout",
                ),
            )
            .await;
            if !ban_result.banned {
                let mut guard = state.lock().await;
                guard.insert((chat_id, user_id), pending);
                return;
            }
            if let Ok(user_id_i64) = i64::try_from(user_id.0)
                && let Err(err) = store.delete_captcha_session(chat_id.0, user_id_i64).await
            {
                log_system_level(
                    &config,
                    LogLevel::Error,
                    &format!("failed to delete expired captcha session: {err}"),
                );
            }
            if let Err(err) = bot
                .delete_message(chat_id, pending.captcha_message_id)
                .await
            {
                log_telegram_error(
                    &config,
                    LogLevel::Error,
                    chat_id,
                    pending.chat_title.as_deref(),
                    pending.chat_username.as_deref(),
                    "failed to delete captcha message on timeout",
                    &err,
                );
            }
            log_user_event_by_display(
                &config,
                user_id,
                chat_id,
                pending.chat_title.as_deref(),
                pending.chat_username.as_deref(),
                &pending.user_display,
                "-> 🏌🏻‍♂️captcha timeout, user banned",
            );
            log_captcha_failure_and_attach(
                &bot,
                &config,
                CaptchaFailureLog {
                    context: CaptchaLogContext {
                        user: &user,
                        chat_id,
                        chat_title: pending.chat_title.as_deref(),
                        chat_username: pending.chat_username.as_deref(),
                    },
                    reason: CaptchaFailureReason::Timeout,
                    attempts_used: pending.attempts_total.saturating_sub(pending.attempts_left),
                    attempts_total: pending.attempts_total,
                    ban_release_at: ban_result.release_at,
                    store: Some(store.as_ref()),
                },
            )
            .await;
        }
    });
}

pub async fn restore_pending_captchas(
    bot: &Bot,
    state: &SharedState,
    config: &Arc<Config>,
    store: Arc<BanReleaseStore>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let now = Utc::now().timestamp();
    let sessions = store.fetch_captcha_sessions().await?;
    log_system_level(
        config,
        LogLevel::Info,
        &format!("loaded {} persisted captcha session(s)", sessions.len()),
    );
    let mut restored = 0;

    for session in sessions {
        let (chat_id, user_id, user, pending) = match session.clone().into_runtime(now) {
            Ok(runtime) => runtime,
            Err(err) => {
                log_system_level(
                    config,
                    LogLevel::Error,
                    &format!(
                        "discarding invalid persisted captcha session chat={} user={}: {err}",
                        session.chat_id, session.user_id
                    ),
                );
                store
                    .delete_captcha_session(session.chat_id, session.user_id)
                    .await?;
                continue;
            }
        };

        let member = match bot.get_chat_member(chat_id, user_id).await {
            Ok(member) => member,
            Err(err) => {
                log_telegram_error(
                    config,
                    LogLevel::Warn,
                    chat_id,
                    pending.chat_title.as_deref(),
                    pending.chat_username.as_deref(),
                    "failed to inspect persisted captcha member during startup recovery",
                    &err,
                );
                continue;
            }
        };
        match member.status() {
            ChatMemberStatus::Left
            | ChatMemberStatus::Banned
            | ChatMemberStatus::Owner
            | ChatMemberStatus::Administrator => {
                store
                    .delete_captcha_session(session.chat_id, session.user_id)
                    .await?;
                continue;
            }
            ChatMemberStatus::Member | ChatMemberStatus::Restricted => {}
        }

        if pending.expires_at <= now {
            let ban_result = ban_user_and_maybe_release(
                bot,
                config,
                BanRequest::from_pending(
                    chat_id,
                    user_id,
                    &pending,
                    Some(store.clone()),
                    "failed to ban user during captcha recovery",
                ),
            )
            .await;
            if !ban_result.banned {
                continue;
            }
            store
                .delete_captcha_session(session.chat_id, session.user_id)
                .await?;
            let _ = bot
                .delete_message(chat_id, pending.captcha_message_id)
                .await;
            log_captcha_failure_and_attach(
                bot,
                config,
                CaptchaFailureLog {
                    context: CaptchaLogContext {
                        user: &user,
                        chat_id,
                        chat_title: pending.chat_title.as_deref(),
                        chat_username: pending.chat_username.as_deref(),
                    },
                    reason: CaptchaFailureReason::Timeout,
                    attempts_used: pending.attempts_total.saturating_sub(pending.attempts_left),
                    attempts_total: pending.attempts_total,
                    ban_release_at: ban_result.release_at,
                    store: Some(&store),
                },
            )
            .await;
            continue;
        }

        if let Err(err) = bot
            .delete_message(chat_id, pending.captcha_message_id)
            .await
        {
            log_telegram_error(
                config,
                LogLevel::Warn,
                chat_id,
                pending.chat_title.as_deref(),
                pending.chat_username.as_deref(),
                "failed to delete stale captcha message during startup recovery",
                &err,
            );
        }
        if let Err(err) = start_captcha_for_user(
            bot,
            chat_id,
            CaptchaChatContext::new(pending.chat_title.clone(), pending.chat_username.clone()),
            user,
            state,
            config,
            &Some(store.clone()),
        )
        .await
        {
            log_telegram_error(
                config,
                LogLevel::Warn,
                chat_id,
                pending.chat_title.as_deref(),
                pending.chat_username.as_deref(),
                "failed to reissue captcha during startup recovery",
                &err,
            );
            continue;
        }
        restored += 1;
    }

    if restored > 0 {
        log_system_level(
            config,
            LogLevel::Info,
            &format!("restored {restored} pending captcha session(s) after startup"),
        );
    }
    Ok(())
}

pub async fn on_text(
    bot: Bot,
    msg: Message,
    state: SharedState,
    config: Arc<Config>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(user) = msg.from.as_ref() else {
        return Ok(());
    };
    if user.is_bot {
        return Ok(());
    }

    if msg.text().is_some() {
        let key = (msg.chat.id, user.id);
        let pending = {
            let guard = state.lock().await;
            guard.contains_key(&key)
        };
        if pending {
            let _ = bot.delete_message(msg.chat.id, msg.id).await;
            let (chat_title, chat_username) = chat_context(&msg.chat);
            log_user_event_with_chat(
                &config,
                user,
                msg.chat.id,
                chat_title.as_deref(),
                chat_username.as_deref(),
                "<- 🚫 captcha text blocked",
            );
            return Ok(());
        }
    }

    let text = match msg.text() {
        Some(text) => text.trim().to_string(),
        None => return Ok(()),
    };

    log_message(&config, &msg);

    if msg.chat.is_private() {
        let command = text.split_whitespace().next().unwrap_or("");

        if is_admin_pending_command(command) {
            if !config.is_admin(user.id.0) {
                bot.send_message(msg.chat.id, "⛔ Perintah ini khusus admin.")
                    .await?;
                return Ok(());
            }
            let Some(store) = ban_release_store.as_ref() else {
                bot.send_message(msg.chat.id, "⚠️ Database jadwal ban belum tersedia.")
                    .await?;
                return Ok(());
            };
            send_admin_pending_panel(&bot, msg.chat.id, store, &config, 0, None).await?;
            return Ok(());
        }

        if is_admin_stats_command(command) {
            if !config.is_admin(user.id.0) {
                bot.send_message(msg.chat.id, "⛔ Perintah ini khusus admin.")
                    .await?;
                return Ok(());
            }
            let Some(store) = ban_release_store.as_ref() else {
                bot.send_message(msg.chat.id, "⚠️ Database statistik belum tersedia.")
                    .await?;
                return Ok(());
            };
            send_admin_stats_report(&bot, msg.chat.id, store, &config, StatsPeriod::Today, None)
                .await?;
            return Ok(());
        }

        if is_command(command, "ping") {
            let start = Instant::now();
            let sent = bot
                .send_message(msg.chat.id, "🏓 *Pong\\!*\n⏰ Response time: `...` ms")
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
            let elapsed = start.elapsed().as_millis();
            if let Err(err) = bot
                .edit_message_text(
                    msg.chat.id,
                    sent.id,
                    format!("🏓 *Pong\\!*\n⏰ Response time: `{}` ms", elapsed),
                )
                .parse_mode(ParseMode::MarkdownV2)
                .await
            {
                let (chat_title, chat_username) = chat_context(&msg.chat);
                log_telegram_error(
                    &config,
                    LogLevel::Warn,
                    msg.chat.id,
                    chat_title.as_deref(),
                    chat_username.as_deref(),
                    "failed to edit ping response",
                    &err,
                );
            }
            return Ok(());
        }

        if is_command(command, "start") || is_command(command, "help") {
            let text = render_start_message(
                config.captcha_timeout_secs,
                config.captcha_attempts,
                config.is_admin(user.id.0),
            );
            bot.send_message(msg.chat.id, text)
                .parse_mode(ParseMode::Html)
                .link_preview_options(no_link_preview())
                .reply_markup(build_start_keyboard())
                .await?;
            return Ok(());
        }

        if is_version_command(command) {
            let run_mode = match config.run_mode {
                crate::config::RunMode::Polling => "polling",
                crate::config::RunMode::Webhook => "webhook",
            };
            let timezone = config.timezone.to_string();
            let text = render_version_message(VersionInfo {
                app_version: env!("CARGO_PKG_VERSION"),
                run_mode,
                log_enabled: config.log_enabled,
                log_level: config.log_level.as_str(),
                ban_release_enabled: config.ban_release_enabled,
                ban_release_after_secs: config.ban_release_after_secs,
                timezone: &timezone,
                is_admin: config.is_admin(user.id.0),
            });
            if let Err(err) = bot
                .send_message(msg.chat.id, text)
                .parse_mode(ParseMode::Html)
                .link_preview_options(no_link_preview())
                .await
            {
                let (chat_title, chat_username) = chat_context(&msg.chat);
                log_telegram_error(
                    &config,
                    LogLevel::Warn,
                    msg.chat.id,
                    chat_title.as_deref(),
                    chat_username.as_deref(),
                    "failed to send version response",
                    &err,
                );
            }
            return Ok(());
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatsPeriod {
    Today,
    SevenDays,
    ThirtyDays,
    All,
}

impl StatsPeriod {
    fn callback_key(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::SevenDays => "7d",
            Self::ThirtyDays => "30d",
            Self::All => "all",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Today => "Hari ini",
            Self::SevenDays => "7 hari terakhir",
            Self::ThirtyDays => "30 hari terakhir",
            Self::All => "Semua data",
        }
    }

    fn from_callback_key(key: &str) -> Option<Self> {
        match key {
            "today" => Some(Self::Today),
            "7d" => Some(Self::SevenDays),
            "30d" => Some(Self::ThirtyDays),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

fn stats_period_bounds(config: &Config, period: StatsPeriod, now: i64) -> (i64, i64) {
    let start = match period {
        StatsPeriod::Today => {
            let local_now = config.timezone.timestamp_opt(now, 0).single();
            local_now
                .and_then(|value| {
                    config
                        .timezone
                        .with_ymd_and_hms(value.year(), value.month(), value.day(), 0, 0, 0)
                        .single()
                })
                .map(|value| value.timestamp())
                .unwrap_or_else(|| now.saturating_sub(24 * 60 * 60))
        }
        StatsPeriod::SevenDays => now.saturating_sub(7 * 24 * 60 * 60),
        StatsPeriod::ThirtyDays => now.saturating_sub(30 * 24 * 60 * 60),
        StatsPeriod::All => 0,
    };
    (start, now)
}

fn render_stats_period(config: &Config, period: StatsPeriod, start_at: i64, end_at: i64) -> String {
    match period {
        StatsPeriod::All => "Semua data yang tersimpan".to_string(),
        _ => format!(
            "{} ({} s.d. {})",
            period.label(),
            format_log_timestamp(config, start_at),
            format_log_timestamp(config, end_at)
        ),
    }
}

async fn send_admin_stats_report(
    bot: &Bot,
    chat_id: ChatId,
    store: &BanReleaseStore,
    config: &Config,
    period: StatsPeriod,
    message_id: Option<MessageId>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let generated_at = Utc::now().timestamp();
    let (start_at, end_at) = stats_period_bounds(config, period, generated_at);
    let report = store.fetch_stats(start_at, end_at).await?;
    let period_label = render_stats_period(config, period, start_at, end_at);
    let summary = render_admin_stats_summary(config, &report, &period_label, generated_at);
    let keyboard = build_admin_stats_keyboard(period);

    if let Some(message_id) = message_id {
        bot.edit_message_text(chat_id, message_id, summary)
            .parse_mode(ParseMode::Html)
            .link_preview_options(no_link_preview())
            .reply_markup(keyboard)
            .await?;
    } else {
        bot.send_message(chat_id, summary)
            .parse_mode(ParseMode::Html)
            .link_preview_options(no_link_preview())
            .reply_markup(keyboard)
            .await?;
    }

    let markdown = render_admin_stats_markdown(
        config,
        &report,
        &period_label,
        generated_at,
        start_at,
        end_at,
    );
    let filename = format_stats_filename(config, generated_at);
    let caption = format!(
        "📊 <b>Laporan statistik BuktikanBot</b>\n\
         Periode: {}\n\
         Dicetak: {}",
        escape_html(&period_label),
        escape_html(&format_log_timestamp(config, generated_at))
    );
    bot.send_document(
        chat_id,
        InputFile::memory(markdown.into_bytes()).file_name(filename),
    )
    .caption(caption)
    .parse_mode(ParseMode::Html)
    .await?;
    Ok(())
}

fn render_admin_stats_summary(
    config: &Config,
    report: &StatsReport,
    period_label: &str,
    generated_at: i64,
) -> String {
    let completed = report.successes.saturating_add(report.failures);
    let success_rate = percentage(report.successes, completed);
    [
        "<b>📊 Statistik BuktikanBot</b>".to_string(),
        format!("📅 Periode: {}", escape_html(period_label)),
        format!(
            "🖨️ Dicetak: <code>{}</code>",
            escape_html(&format_log_timestamp(config, generated_at))
        ),
        String::new(),
        format!(
            "👥 User unik dilayani: <code>{}</code>",
            report.unique_users
        ),
        format!(
            "🧩 Sesi verifikasi: <code>{}</code>",
            report.verification_sessions
        ),
        format!("✅ Berhasil: <code>{}</code>", report.successes),
        format!("❌ Gagal: <code>{}</code>", report.failures),
        format!("📈 Tingkat sukses: <code>{}</code>", success_rate),
        format!("  ├⏰ Timeout: <code>{}</code>", report.timeouts),
        format!(
            "  ├🔁 Percobaan habis: <code>{}</code>",
            report.attempts_exceeded
        ),
        format!(
            "  └⚙️ Gagal menyiapkan: <code>{}</code>",
            report.setup_failures
        ),
        format!("🚫 Ban dibuat: <code>{}</code>", report.bans_created),
        format!("♻️ Auto-release: <code>{}</code>", report.auto_releases),
        format!("🛠️ Manual release: <code>{}</code>", report.manual_releases),
        String::new(),
        format!("⏳ Pending saat ini: <code>{}</code>", report.pending_bans),
        format!(
            "🔐 CAPTCHA aktif saat ini: <code>{}</code>",
            report.active_captchas
        ),
        format!("👥 Grup terdata: <code>{}</code>", report.groups.len()),
    ]
    .join("\n")
}

fn build_admin_stats_keyboard(period: StatsPeriod) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                "📅 Hari ini",
                format!("stats:{}", StatsPeriod::Today.callback_key()),
            ),
            InlineKeyboardButton::callback(
                "7 hari",
                format!("stats:{}", StatsPeriod::SevenDays.callback_key()),
            ),
            InlineKeyboardButton::callback(
                "30 hari",
                format!("stats:{}", StatsPeriod::ThirtyDays.callback_key()),
            ),
            InlineKeyboardButton::callback(
                "Semua",
                format!("stats:{}", StatsPeriod::All.callback_key()),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!("🔄 Refresh ({})", period.label()),
            format!("stats:{}", period.callback_key()),
        )],
    ])
}

fn render_admin_stats_markdown(
    config: &Config,
    report: &StatsReport,
    period_label: &str,
    generated_at: i64,
    start_at: i64,
    end_at: i64,
) -> String {
    let completed = report.successes.saturating_add(report.failures);
    let success_rate = percentage(report.successes, completed);
    let mut lines = vec![
        "# 📊 Statistik BuktikanBot".to_string(),
        String::new(),
        format!(
            "- Dicetak pada: {}",
            format_log_timestamp(config, generated_at)
        ),
        format!("- Zona waktu: {}", config.timezone),
        format!("- Periode: {period_label}"),
        format!(
            "- Data dihitung sampai: {}",
            format_log_timestamp(config, end_at)
        ),
        String::new(),
        "## Ringkasan".to_string(),
        String::new(),
        "| Metrik | Jumlah |".to_string(),
        "|---|---:|".to_string(),
        format!("| User unik dilayani | {} |", report.unique_users),
        format!("| Sesi verifikasi | {} |", report.verification_sessions),
        format!("| Berhasil | {} |", report.successes),
        format!("| Gagal | {} |", report.failures),
        format!("| Tingkat keberhasilan | {success_rate} |"),
        format!("| Timeout | {} |", report.timeouts),
        format!("| Percobaan habis | {} |", report.attempts_exceeded),
        format!("| Gagal menyiapkan | {} |", report.setup_failures),
        format!("| Ban dibuat | {} |", report.bans_created),
        format!("| Auto-release | {} |", report.auto_releases),
        format!("| Manual release | {} |", report.manual_releases),
        format!("| Pending saat ini | {} |", report.pending_bans),
        format!("| CAPTCHA aktif saat ini | {} |", report.active_captchas),
        String::new(),
        "## Statistik Per Grup".to_string(),
        String::new(),
    ];

    if report.groups.is_empty() {
        lines.push("_Belum ada data verifikasi pada periode ini._".to_string());
    } else {
        lines.extend([
            "| Grup | Sesi | Sukses | Gagal | Ban |".to_string(),
            "|---|---:|---:|---:|---:|".to_string(),
        ]);
        for group in &report.groups {
            let label = markdown_group_label(group);
            lines.push(format!(
                "| {} | {} | {} | {} | {} |",
                markdown_table_cell(&label),
                group.verification_sessions,
                group.successes,
                group.failures,
                group.bans_created
            ));
        }
        if report.groups.len() == 100 {
            lines.push(String::new());
            lines.push("_Catatan: laporan menampilkan maksimal 100 grup._".to_string());
        }
    }

    lines.extend([
        String::new(),
        "## Catatan".to_string(),
        String::new(),
        format!("- Rentang data Unix timestamp: {start_at} sampai {end_at}."),
        "- Statistik historis dicatat sejak fitur statistik diaktifkan.".to_string(),
        "- Pending ban dan CAPTCHA aktif menunjukkan kondisi saat laporan dibuat.".to_string(),
    ]);
    lines.join("\n")
}

fn markdown_group_label(group: &StatsGroup) -> String {
    match (group.chat_username.as_deref(), group.chat_title.as_deref()) {
        (Some(username), Some(title)) => format!("@{} : {}", username.trim(), title.trim()),
        (Some(username), None) => format!("@{}", username.trim()),
        (None, Some(title)) => title.trim().to_string(),
        (None, None) => format!("chat {}", group.chat_id),
    }
}

fn markdown_table_cell(input: &str) -> String {
    sanitize_log_text(input)
        .replace('\\', "\\\\")
        .replace('|', "\\|")
}

fn percentage(numerator: i64, denominator: i64) -> String {
    if denominator <= 0 {
        return "0.0%".to_string();
    }
    format!("{:.1}%", numerator as f64 * 100.0 / denominator as f64)
}

fn format_stats_filename(config: &Config, timestamp: i64) -> String {
    let stamp = config
        .timezone
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d-%H%M%S").to_string())
        .unwrap_or_else(|| timestamp.to_string());
    format!("buktikanbot-statistik-{stamp}.md")
}

async fn send_admin_pending_panel(
    bot: &Bot,
    chat_id: ChatId,
    store: &BanReleaseStore,
    config: &Config,
    requested_page: usize,
    message_id: Option<MessageId>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let total = store.count_pending().await?;
    let total_usize = usize::try_from(total).unwrap_or(0);
    let page_size = usize::try_from(ADMIN_PENDING_PAGE_SIZE).unwrap_or(5);
    let total_pages = total_usize.div_ceil(page_size).max(1);
    let page = requested_page.min(total_pages - 1);
    let offset = i64::try_from(page)
        .unwrap_or(i64::MAX / ADMIN_PENDING_PAGE_SIZE)
        .saturating_mul(ADMIN_PENDING_PAGE_SIZE);
    let jobs = store.fetch_pending(offset, ADMIN_PENDING_PAGE_SIZE).await?;
    let text = render_admin_pending_panel(config, &jobs, total, page, total_pages);
    let keyboard = build_admin_pending_keyboard(&jobs, page, total_pages);

    if let Some(message_id) = message_id {
        bot.edit_message_text(chat_id, message_id, text)
            .parse_mode(ParseMode::Html)
            .link_preview_options(no_link_preview())
            .reply_markup(keyboard)
            .await?;
    } else {
        bot.send_message(chat_id, text)
            .parse_mode(ParseMode::Html)
            .link_preview_options(no_link_preview())
            .reply_markup(keyboard)
            .await?;
    }
    Ok(())
}

fn render_admin_pending_panel(
    config: &Config,
    jobs: &[BanReleaseJob],
    total: i64,
    page: usize,
    total_pages: usize,
) -> String {
    let now = Utc::now().timestamp();
    let mut lines = vec![
        format!("🛡️ <b>Pending Ban Release ({total})</b>"),
        "Daftar ban yang masih tersimpan dan dapat dilepas manual:".to_string(),
    ];

    if jobs.is_empty() {
        lines.push(String::new());
        lines.push("✅ Tidak ada user yang menunggu release.".to_string());
    } else {
        for (index, job) in jobs.iter().enumerate() {
            let name = escape_html(&sanitize_log_text(job.user_name.trim()));
            let username = job
                .user_username
                .as_deref()
                .map(|value| format!(" (@{})", escape_html(&sanitize_log_text(value.trim()))));
            let group = format_admin_group_label(job);
            let schedule = format_log_timestamp(config, job.release_at);
            let release_status = if job.release_at <= now {
                "⚠️ lewat jadwal"
            } else {
                "⏳ jadwal unban"
            };
            lines.push(String::new());
            lines.push(format!(
                "<b>{}.</b> 🙋🏽 {}{}",
                index + 1,
                name,
                username.unwrap_or_default()
            ));
            lines.push(format!("   ├👥 {}", group));
            lines.push(format!("   ├🆔 user: <code>{}</code>", job.user_id));
            lines.push(format!("   ├🆔 chat: <code>{}</code>", job.chat_id));
            lines.push(format!(
                "   └{}: <code>{}</code>",
                release_status,
                escape_html(&schedule)
            ));
        }
    }

    lines.push(String::new());
    lines.push(format!("Halaman {} dari {}", page + 1, total_pages));
    lines.join("\n")
}

fn format_admin_group_label(job: &BanReleaseJob) -> String {
    let label = match (job.chat_username.as_deref(), job.chat_title.as_deref()) {
        (Some(username), Some(title)) => format!("@{} : {}", username.trim(), title.trim()),
        (Some(username), None) => format!("@{}", username.trim()),
        (None, Some(title)) => title.trim().to_string(),
        (None, None) => "unknown".to_string(),
    };
    escape_html(&sanitize_log_text(&label))
}

fn build_admin_pending_keyboard(
    jobs: &[BanReleaseJob],
    page: usize,
    total_pages: usize,
) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| {
            vec![InlineKeyboardButton::callback(
                format!("✅ Lepas #{}", index + 1),
                format!("admin:confirm:{}:{}:{}", job.chat_id, job.user_id, page),
            )]
        })
        .collect();

    let mut navigation = vec![InlineKeyboardButton::callback(
        "🔄 Refresh",
        format!("admin:refresh:{page}"),
    )];
    if page > 0 {
        navigation.insert(
            0,
            InlineKeyboardButton::callback("⬅️", format!("admin:refresh:{}", page - 1)),
        );
    }
    if page + 1 < total_pages {
        navigation.push(InlineKeyboardButton::callback(
            "➡️",
            format!("admin:refresh:{}", page + 1),
        ));
    }
    rows.push(navigation);
    InlineKeyboardMarkup::new(rows)
}

fn render_admin_confirmation(config: &Config, job: &BanReleaseJob) -> String {
    format!(
        "⚠️ <b>Konfirmasi release ban</b>\n\n\
         🙋🏽 {}{}\n\
         ├👥 {}\n\
         ├🆔 user: <code>{}</code>\n\
         ├🆔 chat: <code>{}</code>\n\
         └📅 jadwal unban: <code>{}</code>\n\n\
         Lepaskan ban user ini sekarang?",
        escape_html(&sanitize_log_text(job.user_name.trim())),
        job.user_username
            .as_deref()
            .map(|value| format!(" (@{})", escape_html(&sanitize_log_text(value.trim()))))
            .unwrap_or_default(),
        format_admin_group_label(job),
        job.user_id,
        job.chat_id,
        escape_html(&format_log_timestamp(config, job.release_at)),
    )
}

fn build_admin_confirmation_keyboard(job: &BanReleaseJob, page: usize) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(
            "✅ Ya, lepaskan",
            format!("admin:release:{}:{}:{}", job.chat_id, job.user_id, page),
        ),
        InlineKeyboardButton::callback("↩️ Batal", format!("admin:cancel:{page}")),
    ]])
}

pub async fn on_callback_query(
    bot: Bot,
    query: CallbackQuery,
    state: SharedState,
    config: Arc<Config>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let CallbackQuery {
        id,
        from,
        data,
        message,
        ..
    } = query;
    let Some(data) = data.as_deref() else {
        return Ok(());
    };
    if data.starts_with("admin:") || data.starts_with("stats:") {
        return on_admin_callback(&bot, id, &from, data, message, &config, ban_release_store).await;
    }
    if !data.starts_with("captcha:") {
        return Ok(());
    }
    let Some(message) = message.and_then(|message| message.regular_message().cloned()) else {
        return Ok(());
    };
    let chat_id = message.chat.id;
    let key = (chat_id, from.id);
    let selected = data.trim_start_matches("captcha:");

    let check = {
        let mut guard = state.lock().await;
        check_captcha_answer_for_message_at(
            &mut guard,
            key,
            message.id,
            Utc::now().timestamp(),
            selected,
        )
    };

    match check {
        CaptchaCheck::NoPending => {
            let _ = bot
                .answer_callback_query(id)
                .text("🚫 Captcha sudah selesai atau bukan untukmu.")
                .show_alert(true)
                .await;
        }
        CaptchaCheck::Wrong => {
            let updated = {
                let mut guard = state.lock().await;
                guard.get_mut(&key).map(|pending| {
                    let previous = pending.clone();
                    pending.attempts_left = pending.attempts_left.saturating_sub(1);
                    let mut updated_png = None;
                    let options = if pending.attempts_left == 0 {
                        generate_captcha_options(&pending.code, config.captcha_option_count)
                    } else {
                        match generate_captcha(
                            config.captcha_len,
                            config.captcha_width,
                            config.captcha_height,
                        ) {
                            Ok((code, png)) => {
                                pending.code = code.clone();
                                updated_png = Some(png);
                                generate_captcha_options(&code, config.captcha_option_count)
                            }
                            Err(err) => {
                                log_system_level(
                                    &config,
                                    LogLevel::Error,
                                    &format!("failed to regenerate captcha: {err}"),
                                );
                                generate_captcha_options(&pending.code, config.captcha_option_count)
                            }
                        }
                    };
                    pending.options = options;
                    pending.remaining_secs =
                        pending.expires_at.saturating_sub(Utc::now().timestamp()) as u64;
                    (previous, pending.clone(), updated_png)
                })
            };
            if let Some((previous, pending_state, updated_png)) = updated {
                if pending_state.attempts_left == 0 {
                    let pending = {
                        let mut guard = state.lock().await;
                        guard.remove(&key)
                    };
                    if let Some(pending) = pending {
                        let ban_result = ban_user_and_maybe_release(
                            &bot,
                            &config,
                            BanRequest::from_pending(
                                chat_id,
                                from.id,
                                &pending,
                                ban_release_store.clone(),
                                "failed to ban user on attempts exceeded",
                            ),
                        )
                        .await;
                        if !ban_result.banned {
                            let mut guard = state.lock().await;
                            guard.insert(key, pending);
                            let _ = bot
                                .answer_callback_query(id)
                                .text("⚠️ Sistem belum bisa mengeluarkanmu. Coba lagi sebentar.")
                                .show_alert(true)
                                .await;
                            return Ok(());
                        }
                        if let Some(store) = ban_release_store.as_ref()
                            && let Ok(user_id_i64) = i64::try_from(from.id.0)
                            && let Err(err) =
                                store.delete_captcha_session(chat_id.0, user_id_i64).await
                        {
                            log_system_level(
                                &config,
                                LogLevel::Error,
                                &format!("failed to delete captcha session: {err}"),
                            );
                        }
                        if let Err(err) = bot
                            .delete_message(chat_id, pending.captcha_message_id)
                            .await
                        {
                            log_telegram_error(
                                &config,
                                LogLevel::Error,
                                chat_id,
                                pending.chat_title.as_deref(),
                                pending.chat_username.as_deref(),
                                "failed to delete captcha message on attempts exceeded",
                                &err,
                            );
                        }
                        log_user_event_by_display(
                            &config,
                            from.id,
                            chat_id,
                            pending.chat_title.as_deref(),
                            pending.chat_username.as_deref(),
                            &pending.user_display,
                            "-> 🧨 captcha attempts exceeded, user banned",
                        );
                        log_captcha_failure_and_attach(
                            &bot,
                            &config,
                            CaptchaFailureLog {
                                context: CaptchaLogContext {
                                    user: &from,
                                    chat_id,
                                    chat_title: pending.chat_title.as_deref(),
                                    chat_username: pending.chat_username.as_deref(),
                                },
                                reason: CaptchaFailureReason::AttemptsExceeded,
                                attempts_used: pending.attempts_total,
                                attempts_total: pending.attempts_total,
                                ban_release_at: ban_result.release_at,
                                store: ban_release_store.as_deref(),
                            },
                        )
                        .await;
                    }
                    let _ = bot
                        .answer_callback_query(id)
                        .text("❌ Kesempatan habis. Kamu dikeluarkan.")
                        .show_alert(true)
                        .await;
                    return Ok(());
                }
                let persistence_result = match ban_release_store.as_ref() {
                    Some(store) => {
                        match CaptchaSession::from_pending(chat_id, &from, &pending_state) {
                            Ok(session) => store.save_captcha_session(session).await,
                            Err(err) => Err(err),
                        }
                    }
                    None => Err("captcha state store unavailable".into()),
                };
                if let Err(err) = persistence_result {
                    log_system_level(
                        &config,
                        LogLevel::Error,
                        &format!("failed to update captcha session: {err}"),
                    );
                    let mut guard = state.lock().await;
                    guard.insert(key, previous);
                    let _ = bot
                        .answer_callback_query(id)
                        .text("⚠️ Sistem sedang sibuk. Silakan tekan tombol lagi.")
                        .show_alert(true)
                        .await;
                    return Ok(());
                }
                let options = &pending_state.options;
                let caption = captcha_caption(
                    &from,
                    pending_state.remaining_secs,
                    pending_state.attempts_left,
                    pending_state.attempts_total,
                );
                let edit_result = if let Some(png) = updated_png {
                    let media = InputMedia::Photo(
                        InputMediaPhoto::new(InputFile::memory(png))
                            .caption(caption)
                            .parse_mode(ParseMode::Html),
                    );
                    bot.edit_message_media(chat_id, message.id, media)
                        .reply_markup(build_captcha_keyboard(
                            options,
                            config.captcha_option_digits_to_emoji,
                        ))
                        .await
                } else {
                    bot.edit_message_caption(chat_id, message.id)
                        .caption(caption)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(build_captcha_keyboard(
                            options,
                            config.captcha_option_digits_to_emoji,
                        ))
                        .await
                };
                if let Err(err) = edit_result {
                    let (chat_title, chat_username) = chat_context(&message.chat);
                    log_telegram_error(
                        &config,
                        LogLevel::Warn,
                        chat_id,
                        chat_title.as_deref(),
                        chat_username.as_deref(),
                        "failed to update captcha message",
                        &err,
                    );
                    if let Some(store) = ban_release_store.as_ref() {
                        match CaptchaSession::from_pending(chat_id, &from, &previous) {
                            Ok(session) => {
                                if let Err(rollback_err) = store.save_captcha_session(session).await
                                {
                                    log_system_level(
                                        &config,
                                        LogLevel::Error,
                                        &format!(
                                            "failed to roll back captcha session after message update failure: {rollback_err}"
                                        ),
                                    );
                                }
                            }
                            Err(rollback_err) => {
                                log_system_level(
                                    &config,
                                    LogLevel::Error,
                                    &format!(
                                        "failed to encode captcha rollback session: {rollback_err}"
                                    ),
                                );
                            }
                        }
                    }
                    let mut guard = state.lock().await;
                    guard.insert(key, previous);
                    let _ = bot
                        .answer_callback_query(id)
                        .text("⚠️ Captcha belum diperbarui. Silakan tekan tombol lagi.")
                        .show_alert(true)
                        .await;
                    return Ok(());
                }
                let _ = bot
                    .answer_callback_query(id)
                    .text("❌ Jawaban salah, coba lagi.")
                    .show_alert(false)
                    .await;
                let (chat_title, chat_username) = chat_context(&message.chat);
                log_user_event_with_chat(
                    &config,
                    &from,
                    chat_id,
                    chat_title.as_deref(),
                    chat_username.as_deref(),
                    "<- 🚫 captcha wrong (button)",
                );
            }
        }
        CaptchaCheck::Verified(pending) => {
            if let Err(err) = restore_chat_permissions(&bot, chat_id, from.id).await {
                let (chat_title, chat_username) = chat_context(&message.chat);
                log_telegram_error(
                    &config,
                    LogLevel::Error,
                    chat_id,
                    chat_title.as_deref(),
                    chat_username.as_deref(),
                    "failed to restore user permissions",
                    &err,
                );
                let mut guard = state.lock().await;
                guard.insert(key, *pending);
                let _ = bot
                    .answer_callback_query(id)
                    .text("⚠️ Verifikasi belum selesai. Coba lagi sebentar.")
                    .show_alert(true)
                    .await;
                return Ok(());
            }
            let Some(store) = ban_release_store.as_ref() else {
                if let Err(err) = bot
                    .restrict_chat_member(
                        chat_id,
                        from.id,
                        teloxide::types::ChatPermissions::empty(),
                    )
                    .await
                {
                    let (chat_title, chat_username) = chat_context(&message.chat);
                    log_telegram_error(
                        &config,
                        LogLevel::Error,
                        chat_id,
                        chat_title.as_deref(),
                        chat_username.as_deref(),
                        "failed to re-restrict user because captcha state store is unavailable",
                        &err,
                    );
                }
                let mut guard = state.lock().await;
                guard.insert(key, *pending);
                let _ = bot
                    .answer_callback_query(id)
                    .text("⚠️ Sistem belum siap. Coba lagi sebentar.")
                    .show_alert(true)
                    .await;
                return Ok(());
            };
            let user_id_i64 = i64::try_from(from.id.0).map_err(|_| "user id out of range")?;
            if let Err(err) = store.delete_captcha_session(chat_id.0, user_id_i64).await {
                log_system_level(
                    &config,
                    LogLevel::Error,
                    &format!("failed to delete verified captcha session: {err}"),
                );
                if let Err(restrict_err) = bot
                    .restrict_chat_member(
                        chat_id,
                        from.id,
                        teloxide::types::ChatPermissions::empty(),
                    )
                    .await
                {
                    let (chat_title, chat_username) = chat_context(&message.chat);
                    log_telegram_error(
                        &config,
                        LogLevel::Error,
                        chat_id,
                        chat_title.as_deref(),
                        chat_username.as_deref(),
                        "failed to re-restrict user after captcha session cleanup failure",
                        &restrict_err,
                    );
                }
                let mut guard = state.lock().await;
                guard.insert(key, *pending);
                let _ = bot
                    .answer_callback_query(id)
                    .text("⚠️ Sistem belum menyelesaikan verifikasi. Coba lagi sebentar.")
                    .show_alert(true)
                    .await;
                return Ok(());
            }
            if let Err(err) = bot
                .delete_message(chat_id, pending.captcha_message_id)
                .await
            {
                let (chat_title, chat_username) = chat_context(&message.chat);
                log_telegram_error(
                    &config,
                    LogLevel::Warn,
                    chat_id,
                    chat_title.as_deref(),
                    chat_username.as_deref(),
                    "failed to delete captcha message after verification",
                    &err,
                );
            }
            let _ = bot
                .answer_callback_query(id)
                .text("✅ Captcha benar. Terima kasih!")
                .show_alert(false)
                .await;
            let (chat_title, chat_username) = chat_context(&message.chat);
            log_user_event_with_chat(
                &config,
                &from,
                chat_id,
                chat_title.as_deref(),
                chat_username.as_deref(),
                "==> ✅ captcha verified (button)",
            );
            send_captcha_log_if_enabled(
                &bot,
                &config,
                CaptchaLogContext {
                    user: &from,
                    chat_id,
                    chat_title: chat_title.as_deref(),
                    chat_username: chat_username.as_deref(),
                },
                CaptchaLogOutcome::Success {
                    attempts_used: pending
                        .attempts_total
                        .saturating_sub(pending.attempts_left)
                        .saturating_add(1)
                        .min(pending.attempts_total),
                    attempts_total: pending.attempts_total,
                },
            )
            .await;
            record_stats_event_if_available(
                &config,
                Some(store.as_ref()),
                StatsEvent {
                    occurred_at: Utc::now().timestamp(),
                    event_type: StatsEventType::CaptchaSuccess,
                    chat_id: chat_id.0,
                    user_id: user_id_i64,
                    chat_title,
                    chat_username,
                    attempts_used: Some(
                        i64::try_from(
                            pending
                                .attempts_total
                                .saturating_sub(pending.attempts_left)
                                .saturating_add(1)
                                .min(pending.attempts_total),
                        )
                        .unwrap_or(i64::MAX),
                    ),
                    attempts_total: Some(i64::try_from(pending.attempts_total).unwrap_or(i64::MAX)),
                    failure_reason: None,
                },
                "failed to record captcha success statistic",
            )
            .await;
        }
    }

    Ok(())
}

async fn on_admin_callback(
    bot: &Bot,
    callback_id: CallbackQueryId,
    from: &User,
    data: &str,
    message: Option<MaybeInaccessibleMessage>,
    config: &Arc<Config>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !config.is_admin(from.id.0) {
        let _ = bot
            .answer_callback_query(callback_id)
            .text("⛔ Perintah ini khusus admin.")
            .show_alert(true)
            .await;
        return Ok(());
    }

    let Some(message) = message.and_then(|message| message.regular_message().cloned()) else {
        let _ = bot
            .answer_callback_query(callback_id)
            .text("⚠️ Panel admin sudah tidak tersedia.")
            .show_alert(true)
            .await;
        return Ok(());
    };
    if !message.chat.is_private() {
        let _ = bot
            .answer_callback_query(callback_id)
            .text("⚠️ Panel admin hanya dapat digunakan di private chat.")
            .show_alert(true)
            .await;
        return Ok(());
    }

    let Some(store) = ban_release_store.as_ref() else {
        let _ = bot
            .answer_callback_query(callback_id)
            .text("⚠️ Database jadwal ban belum tersedia.")
            .show_alert(true)
            .await;
        return Ok(());
    };

    if let Some(period) = parse_stats_callback(data) {
        send_admin_stats_report(
            bot,
            message.chat.id,
            store,
            config,
            period,
            Some(message.id),
        )
        .await?;
        let _ = bot.answer_callback_query(callback_id).await;
        return Ok(());
    }

    if let Some(page) = parse_admin_page_callback(data, "refresh") {
        send_admin_pending_panel(bot, message.chat.id, store, config, page, Some(message.id))
            .await?;
        let _ = bot.answer_callback_query(callback_id).await;
        return Ok(());
    }

    if let Some((chat_id, user_id, page)) = parse_admin_job_callback(data, "confirm") {
        let Some(job) = store.get_job(chat_id, user_id).await? else {
            let _ = bot
                .answer_callback_query(callback_id)
                .text("ℹ️ Job sudah tidak tersedia. Daftar akan diperbarui.")
                .show_alert(true)
                .await;
            send_admin_pending_panel(bot, message.chat.id, store, config, page, Some(message.id))
                .await?;
            return Ok(());
        };
        bot.edit_message_text(
            message.chat.id,
            message.id,
            render_admin_confirmation(config, &job),
        )
        .parse_mode(ParseMode::Html)
        .link_preview_options(no_link_preview())
        .reply_markup(build_admin_confirmation_keyboard(&job, page))
        .await?;
        let _ = bot.answer_callback_query(callback_id).await;
        return Ok(());
    }

    if let Some(page) = parse_admin_page_callback(data, "cancel") {
        send_admin_pending_panel(bot, message.chat.id, store, config, page, Some(message.id))
            .await?;
        let _ = bot.answer_callback_query(callback_id).await;
        return Ok(());
    }

    if let Some((chat_id, user_id, page)) = parse_admin_job_callback(data, "release") {
        let Some(job) = store.get_job(chat_id, user_id).await? else {
            let _ = bot
                .answer_callback_query(callback_id)
                .text("ℹ️ Job sudah tidak tersedia. Daftar akan diperbarui.")
                .show_alert(true)
                .await;
            send_admin_pending_panel(bot, message.chat.id, store, config, page, Some(message.id))
                .await?;
            return Ok(());
        };

        match release_ban_job(bot, config, store, &job, Some(from.id)).await {
            Ok(()) => {
                let _ = bot
                    .answer_callback_query(callback_id)
                    .text("✅ Ban berhasil dilepas.")
                    .show_alert(false)
                    .await;
                send_admin_pending_panel(
                    bot,
                    message.chat.id,
                    store,
                    config,
                    page,
                    Some(message.id),
                )
                .await?;
            }
            Err(err) => {
                log_system_level(
                    config,
                    LogLevel::Warn,
                    &format!(
                        "manual ban release failed; chat={} user={}: {err}",
                        job.chat_id, job.user_id
                    ),
                );
                let _ = bot
                    .answer_callback_query(callback_id)
                    .text("⚠️ Ban belum dapat dilepas. Pastikan bot masih admin di grup.")
                    .show_alert(true)
                    .await;
            }
        }
        return Ok(());
    }

    let _ = bot
        .answer_callback_query(callback_id)
        .text("⚠️ Tombol admin tidak dikenali.")
        .show_alert(true)
        .await;
    Ok(())
}

fn is_admin_pending_command(input: &str) -> bool {
    is_command(input, "pending") || is_command(input, "bans")
}

fn is_admin_stats_command(input: &str) -> bool {
    is_command(input, "stats") || is_command(input, "statistic") || is_command(input, "statistik")
}

fn parse_stats_callback(data: &str) -> Option<StatsPeriod> {
    let mut parts = data.split(':');
    if parts.next() != Some("stats") {
        return None;
    }
    let period = StatsPeriod::from_callback_key(parts.next()?)?;
    parts.next().is_none().then_some(period)
}

const SUPPORT_GROUP_URL: &str = "https://t.me/botindonesia";

fn render_start_message(timeout_secs: u64, attempts: usize, is_admin: bool) -> String {
    let mut lines = vec![
        "<b>🤖 BuktikanBot</b>".to_string(),
        "Bot untuk membantu memverifikasi anggota baru grup menggunakan CAPTCHA.".to_string(),
        String::new(),
        "<b>📋 Cara kerja</b>".to_string(),
        "1. Setelah bergabung, bot mengirim CAPTCHA bergambar.".to_string(),
        "2. Pilih jawaban melalui tombol yang tersedia.".to_string(),
        "3. Jawaban benar memulihkan akses ke grup.".to_string(),
        "4. Jawaban salah berulang atau waktu habis mengeluarkan user dari grup.".to_string(),
        format!("⏱️ Waktu verifikasi: <code>{timeout_secs} detik</code>."),
        format!("🔁 Kesempatan menjawab: <code>{attempts}</code>."),
        "🔒 Selama verifikasi, user tidak dapat mengirim pesan; cukup gunakan tombol CAPTCHA."
            .to_string(),
        String::new(),
        "<b>📚 Perintah</b>".to_string(),
        "• <code>/start</code> atau <code>/help</code> — tampilkan panduan ini.".to_string(),
        "• <code>/ping</code> — cek respons bot.".to_string(),
        "• <code>/ver</code> — lihat versi aplikasi.".to_string(),
    ];

    if is_admin {
        lines.extend([
            String::new(),
            "<b>🛡️ Perintah Admin</b>".to_string(),
            "• <code>/pending</code> atau <code>/bans</code> — lihat ban yang menunggu release dan lepaskan melalui tombol.".to_string(),
            "• <code>/stats</code>, <code>/statistic</code>, atau <code>/statistik</code> — kirim ringkasan dan laporan statistik dalam file Markdown.".to_string(),
        ]);
    }

    lines.extend([
        String::new(),
        "👤 Pengembang: <b>bangHasan</b> @hasanudinhs".to_string(),
        "💬 Bantuan tersedia melalui tombol grup support di bawah.".to_string(),
    ]);
    lines.join("\n")
}

fn build_start_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::url(
        "💬 Grup Support @botindonesia",
        Url::parse(SUPPORT_GROUP_URL).expect("support group URL must be valid"),
    )]])
}

struct VersionInfo<'a> {
    app_version: &'a str,
    run_mode: &'a str,
    log_enabled: bool,
    log_level: &'a str,
    ban_release_enabled: bool,
    ban_release_after_secs: u64,
    timezone: &'a str,
    is_admin: bool,
}

fn render_version_message(info: VersionInfo<'_>) -> String {
    let mut lines = vec![
        "<b>🧩 BuktikanBot</b>".to_string(),
        String::new(),
        format!("📦 Versi: <code>{}</code>", escape_html(info.app_version)),
        "✅ Status: aktif".to_string(),
    ];

    if info.is_admin {
        let auto_unban = if info.ban_release_enabled {
            format!(
                "aktif — {}",
                escape_html(&format_duration(info.ban_release_after_secs))
            )
        } else {
            "nonaktif".to_string()
        };
        let logging = if info.log_enabled {
            format!("aktif — {}", escape_html(info.log_level))
        } else {
            "nonaktif".to_string()
        };
        lines.extend([
            String::new(),
            "<b>🛠️ Informasi runtime</b>".to_string(),
            format!("⚙️ Mode: <code>{}</code>", escape_html(info.run_mode)),
            format!("🔓 Auto-unban: <code>{auto_unban}</code>"),
            format!("🪵 Logging: <code>{logging}</code>"),
            format!("🕒 Zona waktu: <code>{}</code>", escape_html(info.timezone)),
        ]);
    }

    lines.extend([
        String::new(),
        "Gunakan <code>/help</code> untuk melihat panduan penggunaan.".to_string(),
    ]);
    lines.join("\n")
}

fn format_duration(seconds: u64) -> String {
    const DAY: u64 = 24 * 60 * 60;
    const HOUR: u64 = 60 * 60;
    const MINUTE: u64 = 60;

    if seconds >= DAY && seconds.is_multiple_of(DAY) {
        format!("{} hari", seconds / DAY)
    } else if seconds >= HOUR && seconds.is_multiple_of(HOUR) {
        format!("{} jam", seconds / HOUR)
    } else if seconds >= MINUTE && seconds.is_multiple_of(MINUTE) {
        format!("{} menit", seconds / MINUTE)
    } else {
        format!("{seconds} detik")
    }
}

fn parse_admin_page_callback(data: &str, action: &str) -> Option<usize> {
    let mut parts = data.split(':');
    (parts.next() == Some("admin") && parts.next() == Some(action))
        .then(|| parts.next()?.parse::<usize>().ok())?
        .filter(|_| parts.next().is_none())
}

fn parse_admin_job_callback(data: &str, action: &str) -> Option<(i64, i64, usize)> {
    let mut parts = data.split(':');
    if parts.next() != Some("admin") || parts.next() != Some(action) {
        return None;
    }
    let chat_id = parts.next()?.parse::<i64>().ok()?;
    let user_id = parts.next()?.parse::<i64>().ok()?;
    let page = parts.next()?.parse::<usize>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((chat_id, user_id, page))
}

pub async fn on_non_text(
    msg: Message,
    config: Arc<Config>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log_message(&config, &msg);
    Ok(())
}

async fn restore_chat_permissions(
    bot: &Bot,
    chat_id: ChatId,
    user_id: UserId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let chat = bot.get_chat(chat_id).await?;
    let Some(permissions) = chat.permissions() else {
        return Err("chat permissions unavailable".into());
    };
    bot.restrict_chat_member(chat_id, user_id, permissions)
        .await?;
    Ok(())
}

async fn ban_user_and_maybe_release(
    bot: &Bot,
    config: &Arc<Config>,
    request: BanRequest,
) -> BanResult {
    let BanRequest {
        chat_id,
        user_id,
        chat_title,
        chat_username,
        user_name,
        user_username,
        ban_release_store,
        error_context,
    } = request;

    let release_time = Utc::now() + ChronoDuration::seconds(config.ban_release_after_secs as i64);
    let mut ban_request = bot.ban_chat_member(chat_id, user_id);
    if config.ban_release_enabled {
        ban_request = ban_request.until_date(release_time);
    }
    if let Err(err) = ban_request.await {
        log_telegram_error(
            config,
            LogLevel::Error,
            chat_id,
            chat_title.as_deref(),
            chat_username.as_deref(),
            error_context,
            &err,
        );
        return BanResult::default();
    }

    if let Ok(user_id) = i64::try_from(user_id.0) {
        record_stats_event_if_available(
            config,
            ban_release_store.as_deref(),
            StatsEvent {
                occurred_at: Utc::now().timestamp(),
                event_type: StatsEventType::BanCreated,
                chat_id: chat_id.0,
                user_id,
                chat_title: chat_title.clone(),
                chat_username: chat_username.clone(),
                attempts_used: None,
                attempts_total: None,
                failure_reason: None,
            },
            "failed to record ban statistic",
        )
        .await;
    }

    let release_at = config
        .ban_release_enabled
        .then_some(release_time.timestamp());
    if !config.ban_release_enabled {
        return BanResult {
            banned: true,
            release_at,
        };
    }
    let Some(store) = ban_release_store else {
        log_telegram_error(
            config,
            LogLevel::Error,
            chat_id,
            chat_title.as_deref(),
            chat_username.as_deref(),
            "ban release store unavailable after user ban",
            &"state store unavailable",
        );
        return BanResult {
            banned: true,
            release_at,
        };
    };
    let Ok(user_id_i64) = i64::try_from(user_id.0) else {
        let err = "user id out of range";
        log_telegram_error(
            config,
            LogLevel::Warn,
            chat_id,
            chat_title.as_deref(),
            chat_username.as_deref(),
            "failed to store ban release job (user id out of range)",
            &err,
        );
        return BanResult {
            banned: true,
            release_at,
        };
    };
    if let Err(err) = store
        .upsert_job(BanReleaseJob {
            chat_id: chat_id.0,
            user_id: user_id_i64,
            release_at: release_time.timestamp(),
            user_name,
            user_username,
            chat_title: chat_title.clone(),
            chat_username: chat_username.clone(),
            log_chat_id: config
                .captcha_log_enabled
                .then_some(config.captcha_log_chat_id)
                .flatten(),
            log_message_thread_id: config
                .captcha_log_enabled
                .then_some(config.captcha_log_message_thread_id)
                .flatten(),
            log_message_id: None,
        })
        .await
    {
        log_telegram_error(
            config,
            LogLevel::Warn,
            chat_id,
            chat_title.as_deref(),
            chat_username.as_deref(),
            "failed to store ban release job",
            &err,
        );
    }
    BanResult {
        banned: true,
        release_at,
    }
}

fn is_command(input: &str, cmd: &str) -> bool {
    let lowered = input.trim().to_ascii_lowercase();
    let cmd = format!("/{}", cmd);
    lowered == cmd || lowered.starts_with(&(cmd + "@"))
}

fn is_version_command(input: &str) -> bool {
    let lowered = input.trim().to_ascii_lowercase();
    let cmd = lowered.split('@').next().unwrap_or(&lowered);
    matches!(cmd, "/ver" | "/versi" | "/version")
}

fn option_to_display(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            'A' | 'a' => {
                if matches!(chars.peek(), Some('B') | Some('b')) {
                    chars.next();
                    out.push('🆎');
                } else {
                    out.push_str("🅰️");
                }
            }
            'B' | 'b' => out.push_str("🅱️"),
            '0' => out.push_str("0️⃣"),
            '1' => out.push_str("1️⃣"),
            '2' => out.push_str("2️⃣"),
            '3' => out.push_str("3️⃣"),
            '4' => out.push_str("4️⃣"),
            '5' => out.push_str("5️⃣"),
            '6' => out.push_str("6️⃣"),
            '7' => out.push_str("7️⃣"),
            '8' => out.push_str("8️⃣"),
            '9' => out.push_str("9️⃣"),
            _ => out.push(ch),
        }
    }
    out
}

fn build_captcha_keyboard(options: &[String], digits_to_emoji: bool) -> InlineKeyboardMarkup {
    let rows: Vec<Vec<InlineKeyboardButton>> = options
        .chunks(3)
        .map(|chunk| {
            chunk
                .iter()
                .map(|option| {
                    let display = if digits_to_emoji
                        && option
                            .chars()
                            .any(|ch| ch.is_ascii_digit() || matches!(ch, 'A' | 'a' | 'B' | 'b'))
                    {
                        option_to_display(option)
                    } else {
                        option.to_string()
                    };
                    InlineKeyboardButton::callback(display, format!("captcha:{option}"))
                })
                .collect()
        })
        .collect();
    InlineKeyboardMarkup::new(rows)
}

async fn log_captcha_failure_and_attach(
    bot: &Bot,
    config: &Config,
    failure: CaptchaFailureLog<'_>,
) {
    let CaptchaFailureLog {
        context,
        reason,
        attempts_used,
        attempts_total,
        ban_release_at,
        store,
    } = failure;
    if let Ok(user_id) = i64::try_from(context.user.id.0) {
        record_stats_event_if_available(
            config,
            store,
            StatsEvent {
                occurred_at: Utc::now().timestamp(),
                event_type: StatsEventType::CaptchaFailed,
                chat_id: context.chat_id.0,
                user_id,
                chat_title: context.chat_title.map(str::to_string),
                chat_username: context.chat_username.map(str::to_string),
                attempts_used: Some(i64::try_from(attempts_used).unwrap_or(i64::MAX)),
                attempts_total: Some(i64::try_from(attempts_total).unwrap_or(i64::MAX)),
                failure_reason: Some(reason.code().to_string()),
            },
            "failed to record captcha failure statistic",
        )
        .await;
    }
    let reference = send_captcha_log_if_enabled(
        bot,
        config,
        context,
        CaptchaLogOutcome::Failure {
            reason,
            attempts_used,
            attempts_total,
            ban_release_at,
        },
    )
    .await;

    if ban_release_at.is_none() {
        return;
    }
    let Some(reference) = reference else {
        return;
    };
    let Some(store) = store else {
        return;
    };
    let Ok(user_id) = i64::try_from(context.user.id.0) else {
        log_system_level(
            config,
            LogLevel::Warn,
            "failed to attach captcha log: user id out of range",
        );
        return;
    };
    if let Err(err) = store
        .attach_log_message(
            context.chat_id.0,
            user_id,
            reference.chat_id,
            reference.message_thread_id,
            reference.message_id,
        )
        .await
    {
        log_system_level(
            config,
            LogLevel::Warn,
            &format!("failed to attach captcha log message to ban release job: {err}"),
        );
    }
}

async fn send_captcha_log_if_enabled(
    bot: &Bot,
    config: &Config,
    context: CaptchaLogContext<'_>,
    outcome: CaptchaLogOutcome,
) -> Option<CaptchaLogReference> {
    if !config.captcha_log_enabled {
        return None;
    }
    let target_id = config.captcha_log_chat_id?;

    let ts = format_log_timestamp(config, Utc::now().timestamp());

    let user = context.user;
    let chat_id = context.chat_id;
    let chat_title = context.chat_title;
    let chat_username = context.chat_username;
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or("").trim());
    let full_name = if last_name.is_empty() {
        first_name
    } else {
        format!("{first_name} {last_name}")
    };
    let full_name = escape_html(&full_name);

    let username_line = user.username.as_deref().map(|raw| {
        let username = escape_html(&sanitize_log_text(raw.trim()));
        format!(" ├👤 @{username}")
    });

    let group_label = match (chat_username, chat_title) {
        (Some(username), Some(title)) => format!("@{} : {}", username.trim(), title.trim()),
        (Some(username), None) => format!("@{}", username.trim()),
        (None, Some(title)) => title.trim().to_string(),
        (None, None) => "unknown".to_string(),
    };
    let group_label = escape_html(&sanitize_log_text(&group_label));

    let mut lines = Vec::with_capacity(12);
    let (heading, attempts_used, attempts_total) = match outcome {
        CaptchaLogOutcome::Success {
            attempts_used,
            attempts_total,
        } => ("🔐 CAPTCHA — BERHASIL", attempts_used, attempts_total),
        CaptchaLogOutcome::Failure {
            attempts_used,
            attempts_total,
            ..
        } => ("🔐 CAPTCHA — GAGAL", attempts_used, attempts_total),
    };
    lines.push(heading.to_string());
    lines.push(format!(" ├🕒 kejadian: <code>{}</code>", escape_html(&ts)));
    lines.push(format!(" ├🙋🏽 {}", full_name));
    if let Some(line) = username_line {
        lines.push(line);
    }
    lines.push(format!(" ├👥 {}", group_label));
    lines.push(format!(" ├🆔 user: <code>{}</code>", user.id.0));
    lines.push(format!(" ├🆔 chat: <code>{}</code>", chat_id.0));
    lines.push(format!(
        " ├🎯 percobaan: <code>{}</code>/<code>{}</code>",
        attempts_used, attempts_total
    ));
    match outcome {
        CaptchaLogOutcome::Success { .. } => {
            lines.push(" └✅ user terverifikasi.".to_string());
        }
        CaptchaLogOutcome::Failure {
            reason,
            ban_release_at,
            ..
        } => {
            lines.push(format!(" ├⚠️ alasan: {}", reason.as_str()));
            if let Some(release_at) = ban_release_at {
                lines.push(" ├🔒 tindakan: ban sementara.".to_string());
                lines.push(format!(
                    " └📅 unban otomatis: <code>{}</code>",
                    escape_html(&format_log_timestamp(config, release_at))
                ));
            } else {
                lines.push(" └🔒 tindakan: ban diterapkan.".to_string());
            }
        }
    }
    let message = lines.join("\n");

    let mut request = bot
        .send_message(ChatId(target_id), message)
        .parse_mode(ParseMode::Html)
        .link_preview_options(no_link_preview());
    if let Some(thread_id) = config.captcha_log_message_thread_id {
        request = request.message_thread_id(ThreadId(MessageId(thread_id)));
    }
    match request.await {
        Ok(message) => Some(CaptchaLogReference {
            chat_id: target_id,
            message_thread_id: config.captcha_log_message_thread_id,
            message_id: message.id.0,
        }),
        Err(err) => {
            log_telegram_error(
                config,
                LogLevel::Warn,
                chat_id,
                chat_title,
                chat_username,
                "failed to send captcha log",
                &err,
            );
            None
        }
    }
}

pub async fn release_ban_job(
    bot: &Bot,
    config: &Arc<Config>,
    store: &BanReleaseStore,
    job: &BanReleaseJob,
    released_by: Option<UserId>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_id = u64::try_from(job.user_id).map_err(|_| "user ID out of range")?;
    bot.unban_chat_member(ChatId(job.chat_id), UserId(user_id))
        .only_if_banned(true)
        .await?;
    if store.delete_job(job.chat_id, job.user_id).await? {
        let event_type = released_by
            .map(|_| StatsEventType::BanManualReleased)
            .unwrap_or(StatsEventType::BanAutoReleased);
        record_stats_event_if_available(
            config,
            Some(store),
            StatsEvent {
                occurred_at: Utc::now().timestamp(),
                event_type,
                chat_id: job.chat_id,
                user_id: job.user_id,
                chat_title: job.chat_title.clone(),
                chat_username: job.chat_username.clone(),
                attempts_used: None,
                attempts_total: None,
                failure_reason: None,
            },
            "failed to record ban release statistic",
        )
        .await;
        send_ban_release_log_if_enabled(bot, config, job, released_by).await;
    }
    Ok(())
}

async fn send_ban_release_log_if_enabled(
    bot: &Bot,
    config: &Arc<Config>,
    job: &BanReleaseJob,
    released_by: Option<UserId>,
) {
    if !config.captcha_log_enabled {
        return;
    }
    let Some(target_id) = job.log_chat_id.or(config.captcha_log_chat_id) else {
        return;
    };

    let ts = format_log_timestamp(config, Utc::now().timestamp());
    let full_name = escape_html(&sanitize_log_text(job.user_name.trim()));
    let username_line = job.user_username.as_deref().map(|raw| {
        let username = escape_html(&sanitize_log_text(raw.trim()));
        format!(" ├👤 @{username}")
    });

    let group_label = match (job.chat_username.as_deref(), job.chat_title.as_deref()) {
        (Some(username), Some(title)) => format!("@{} : {}", username.trim(), title.trim()),
        (Some(username), None) => format!("@{}", username.trim()),
        (None, Some(title)) => title.trim().to_string(),
        (None, None) => "unknown".to_string(),
    };
    let group_label = escape_html(&sanitize_log_text(&group_label));

    let mut lines = Vec::with_capacity(13);
    lines.push("♻️ BAN — DILEPAS".to_string());
    lines.push(format!(" ├🕒 kejadian: <code>{}</code>", escape_html(&ts)));
    lines.push(format!(" ├🙋🏽 {}", full_name));
    if let Some(line) = username_line {
        lines.push(line);
    }
    lines.push(format!(" ├👥 {}", group_label));
    lines.push(format!(" ├🆔 user: <code>{}</code>", job.user_id));
    lines.push(format!(" ├🆔 chat: <code>{}</code>", job.chat_id));
    lines.push(format!(
        " ├📅 jadwal unban: <code>{}</code>",
        escape_html(&format_log_timestamp(config, job.release_at))
    ));
    if let Some(released_by) = released_by {
        lines.push(format!(
            " ├🛠️ dilepas oleh admin: <code>{}</code>",
            released_by.0
        ));
    }
    lines.push(" └✅ ban sementara telah dilepas.".to_string());
    let message = lines.join("\n");

    let mut request = bot
        .send_message(ChatId(target_id), message)
        .parse_mode(ParseMode::Html)
        .link_preview_options(no_link_preview());
    let thread_id = if job.log_message_id.is_some() {
        job.log_message_thread_id
    } else {
        job.log_message_thread_id
            .or(config.captcha_log_message_thread_id)
    };
    if let Some(thread_id) = thread_id {
        request = request.message_thread_id(ThreadId(MessageId(thread_id)));
    }
    if let Some(message_id) = job.log_message_id {
        request = request.reply_parameters(
            ReplyParameters::new(MessageId(message_id)).allow_sending_without_reply(),
        );
    }
    if let Err(err) = request.await {
        log_system_level(
            config,
            LogLevel::Warn,
            &format!("failed to send ban release log: {err}"),
        );
    }
}

fn format_log_timestamp(config: &Config, timestamp: i64) -> String {
    match config.timezone.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(value) => value.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
        _ => format!("invalid timestamp ({timestamp})"),
    }
}

fn no_link_preview() -> LinkPreviewOptions {
    LinkPreviewOptions {
        is_disabled: true,
        url: None,
        prefer_small_media: false,
        prefer_large_media: false,
        show_above_text: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use teloxide::types::InlineKeyboardButtonKind;

    #[test]
    fn admin_pending_commands_accept_the_documented_aliases() {
        assert!(is_admin_pending_command("/pending"));
        assert!(is_admin_pending_command("/bans@buktikanbot"));
        assert!(!is_admin_pending_command("/pending-now"));
    }

    #[test]
    fn admin_stats_commands_accept_the_documented_aliases() {
        assert!(is_admin_stats_command("/stats"));
        assert!(is_admin_stats_command("/statistic@buktikanbot"));
        assert!(is_admin_stats_command("/statistik"));
        assert!(!is_admin_stats_command("/stats-now"));
    }

    #[test]
    fn stats_callback_parser_rejects_unknown_or_extra_data() {
        assert_eq!(
            parse_stats_callback("stats:7d"),
            Some(StatsPeriod::SevenDays)
        );
        assert_eq!(parse_stats_callback("stats:unknown"), None);
        assert_eq!(parse_stats_callback("stats:today:extra"), None);
        assert_eq!(parse_stats_callback("admin:today"), None);
    }

    #[test]
    fn start_and_help_commands_share_the_onboarding_message() {
        assert!(is_command("/start", "start"));
        assert!(is_command("/help@buktikanbot", "help"));
        assert!(!is_command("/health", "help"));

        let regular = render_start_message(100, 3, false);
        assert!(regular.contains("/help"));
        assert!(!regular.contains("/pending"));
    }

    #[test]
    fn start_message_includes_admin_commands_for_registered_admin() {
        let admin = render_start_message(100, 3, true);
        assert!(admin.contains("/pending"));
        assert!(admin.contains("/bans"));
    }

    #[test]
    fn start_keyboard_links_to_support_group() {
        let keyboard = build_start_keyboard();
        assert_eq!(keyboard.inline_keyboard.len(), 1);
        assert_eq!(keyboard.inline_keyboard[0].len(), 1);
        let button = &keyboard.inline_keyboard[0][0];
        assert_eq!(button.text, "💬 Grup Support @botindonesia");
        assert!(matches!(
            &button.kind,
            InlineKeyboardButtonKind::Url(url) if url.as_str() == SUPPORT_GROUP_URL
        ));
    }

    #[test]
    fn version_message_hides_runtime_details_for_regular_users() {
        let text = render_version_message(VersionInfo {
            app_version: "1.10.0",
            run_mode: "webhook",
            log_enabled: true,
            log_level: "INFO",
            ban_release_enabled: true,
            ban_release_after_secs: 14_400,
            timezone: "Asia/Jakarta",
            is_admin: false,
        });
        assert!(text.contains("Versi: <code>1.10.0</code>"));
        assert!(text.contains("Status: aktif"));
        assert!(text.contains("/help"));
        assert!(!text.contains("Mode:"));
        assert!(!text.contains("Auto-unban:"));
        assert!(!text.contains("Logging:"));
        assert!(!text.contains("Asia/Jakarta"));
    }

    #[test]
    fn version_message_shows_runtime_details_for_admins() {
        let text = render_version_message(VersionInfo {
            app_version: "1.10.0",
            run_mode: "webhook",
            log_enabled: true,
            log_level: "INFO",
            ban_release_enabled: true,
            ban_release_after_secs: 14_400,
            timezone: "Asia/Jakarta",
            is_admin: true,
        });
        assert!(text.contains("Mode: <code>webhook</code>"));
        assert!(text.contains("Auto-unban: <code>aktif — 4 jam</code>"));
        assert!(text.contains("Logging: <code>aktif — INFO</code>"));
        assert!(text.contains("Zona waktu: <code>Asia/Jakarta</code>"));
    }

    #[test]
    fn version_duration_uses_readable_units() {
        assert_eq!(format_duration(60), "1 menit");
        assert_eq!(format_duration(14_400), "4 jam");
        assert_eq!(format_duration(172_800), "2 hari");
        assert_eq!(format_duration(90), "90 detik");
    }

    #[test]
    fn admin_page_callback_parser_rejects_malformed_data() {
        assert_eq!(
            parse_admin_page_callback("admin:refresh:2", "refresh"),
            Some(2)
        );
        assert_eq!(
            parse_admin_page_callback("admin:refresh:2:extra", "refresh"),
            None
        );
        assert_eq!(
            parse_admin_page_callback("admin:refresh:nope", "refresh"),
            None
        );
    }

    #[test]
    fn admin_job_callback_parser_supports_negative_chat_ids() {
        assert_eq!(
            parse_admin_job_callback("admin:release:-100123:456:3", "release"),
            Some((-100123, 456, 3))
        );
        assert_eq!(
            parse_admin_job_callback("admin:release:-100123:456", "release"),
            None
        );
    }
}
