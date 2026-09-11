use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, ChatMemberStatus, ChatMemberUpdated, ChatPermissions, InlineKeyboardButton,
    InlineKeyboardMarkup, InputFile, InputMedia, InputMediaPhoto, Message, ParseMode, UserId,
};

use crate::ban_release::{BanReleaseJob, BanReleaseStore};
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
    _ban_release_store: Option<Arc<BanReleaseStore>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(user) = msg.from() else {
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

        if is_command(command, "start") {
            let text = "🤖 *Verification Bot User*\n👤 by *bangHasan* @hasanudinhs\n👥 Support: @botindonesia";
            bot.send_message(msg.chat.id, text)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
            return Ok(());
        }

        if is_version_command(command) {
            let run_mode = match config.run_mode {
                crate::config::RunMode::Polling => "polling",
                crate::config::RunMode::Webhook => "webhook",
            };
            let log_info = if config.log_enabled {
                format!(
                    "enabled (level: {})",
                    config.log_level.as_str().to_ascii_lowercase()
                )
            } else {
                "disabled".to_string()
            };
            let timezone = config.timezone.to_string();
            let text = format!(
                "🧩 *BuktikanBot*\n\
📦 Version: `{}`\n\
⚙️ Mode: `{}`\n\
🪵 Log: `{}`\n\
🕒 Timezone: `{}`",
                escape_markdown_v2(env!("CARGO_PKG_VERSION")),
                escape_markdown_v2(run_mode),
                escape_markdown_v2(&log_info),
                escape_markdown_v2(&timezone)
            );
            if let Err(err) = bot
                .send_message(msg.chat.id, text)
                .parse_mode(ParseMode::MarkdownV2)
                .disable_web_page_preview(true)
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
    if !data.starts_with("captcha:") {
        return Ok(());
    }
    let Some(message) = message else {
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
        }
    }

    Ok(())
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

fn escape_markdown_v2(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|'
            | '{' | '}' | '.' | '!' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
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
        .disable_web_page_preview(true);
    if let Some(thread_id) = config.captcha_log_message_thread_id {
        request = request.message_thread_id(thread_id);
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

fn format_log_timestamp(config: &Config, timestamp: i64) -> String {
    match config.timezone.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(value) => value.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
        _ => format!("invalid timestamp ({timestamp})"),
    }
}
