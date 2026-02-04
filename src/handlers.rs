use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, ChatMemberStatus, ChatMemberUpdated, ChatPermissions, InlineKeyboardButton,
    InlineKeyboardMarkup, InputFile, InputMedia, InputMediaPhoto, Message, ParseMode, UserId,
};

use crate::ban_release::BanReleaseStore;
use crate::captcha::{
    CaptchaCheck, SharedState, captcha_caption, check_captcha_answer, generate_captcha,
    generate_captcha_options, make_pending_captcha,
};
use crate::config::{Config, LogLevel};
use crate::logging::{
    chat_context, log_message, log_system_level, log_telegram_error, log_user_event_by_display,
    log_user_event_with_chat,
};
use crate::utils::{escape_html, sanitize_log_text};

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
    for member in members {
        start_captcha_for_user(
            &bot,
            msg.chat.id,
            chat_title.clone(),
            chat_username.clone(),
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
    start_captcha_for_user(
        &bot,
        update.chat.id,
        chat_title,
        chat_username,
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
    chat_title: Option<String>,
    chat_username: Option<String>,
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

    let no_permissions = ChatPermissions::empty();
    if let Err(err) = bot
        .restrict_chat_member(chat_id, user.id, no_permissions)
        .await
    {
        log_telegram_error(
            config,
            LogLevel::Error,
            chat_id,
            chat_title.as_deref(),
            chat_username.as_deref(),
            "failed to restrict user",
            &err,
        );
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

    let pending = make_pending_captcha(
        code,
        sent.id,
        options,
        config.captcha_attempts,
        config.captcha_timeout_secs,
        &user,
        chat_title.clone(),
        chat_username.clone(),
    );

    {
        let mut guard = state.lock().await;
        guard.insert((chat_id, user.id), pending);
    }
    log_user_event_with_chat(
        config,
        &user,
        chat_id,
        chat_title.as_deref(),
        chat_username.as_deref(),
        "-> ⏳ captcha sent",
    );

    let bot_clone = bot.clone();
    let state_clone = state.clone();
    let config_clone = config.clone();
    let ban_release_store_clone = ban_release_store.clone();
    let user_clone = user.clone();
    let user_id = user.id;
    let timeout = config.captcha_timeout_secs;
    let update_secs = config.captcha_caption_update_secs.max(1);
    let captcha_message_id = sent.id;

    tokio::spawn(async move {
        let mut remaining = timeout;
        while remaining > 0 {
            tokio::time::sleep(Duration::from_secs(update_secs)).await;
            remaining = remaining.saturating_sub(update_secs);

            let still_pending = {
                let guard = state_clone.lock().await;
                guard.contains_key(&(chat_id, user_id))
            };
            if !still_pending {
                return;
            }

            let options_state = {
                let mut guard = state_clone.lock().await;
                guard.get_mut(&(chat_id, user_id)).map(|pending| {
                    pending.remaining_secs = remaining;
                    (
                        pending.options.clone(),
                        pending.attempts_left,
                        pending.attempts_total,
                    )
                })
            };
            if let Some((options, attempts_left, attempts_total)) = options_state {
                let caption =
                    captcha_caption(&user_clone, remaining, attempts_left, attempts_total);
                let _ = bot_clone
                    .edit_message_caption(chat_id, captcha_message_id)
                    .caption(caption)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(build_captcha_keyboard(
                        &options,
                        config_clone.captcha_option_digits_to_emoji,
                    ))
                    .await;
            }
        }

        let pending = {
            let mut guard = state_clone.lock().await;
            guard.remove(&(chat_id, user_id))
        };

        if let Some(pending) = pending {
            ban_user_and_maybe_release(
                &bot_clone,
                &config_clone,
                chat_id,
                user_id,
                pending.chat_title.as_deref(),
                pending.chat_username.as_deref(),
                ban_release_store_clone.clone(),
                "failed to ban user on timeout",
            )
            .await;
            if let Err(err) = bot_clone
                .delete_message(chat_id, pending.captcha_message_id)
                .await
            {
                log_telegram_error(
                    &config_clone,
                    LogLevel::Error,
                    chat_id,
                    pending.chat_title.as_deref(),
                    pending.chat_username.as_deref(),
                    "failed to delete captcha message on timeout",
                    &err,
                );
            }
            log_user_event_by_display(
                &config_clone,
                user_id,
                chat_id,
                pending.chat_title.as_deref(),
                pending.chat_username.as_deref(),
                &pending.user_display,
                "-> 🏌🏻‍♂️captcha timeout, user banned",
            );
            send_captcha_log_if_enabled(
                &bot_clone,
                &config_clone,
                &user_clone,
                chat_id,
                pending.chat_title.as_deref(),
                pending.chat_username.as_deref(),
                false,
            )
            .await;
        }
    });

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
        check_captcha_answer(&mut guard, key, selected)
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
                    pending.options = options.clone();
                    (
                        options,
                        updated_png,
                        pending.attempts_left,
                        pending.attempts_total,
                        pending.remaining_secs,
                    )
                })
            };
            if let Some((options, updated_png, attempts_left, attempts_total, remaining_secs)) =
                updated
            {
                if attempts_left == 0 {
                    let pending = {
                        let mut guard = state.lock().await;
                        guard.remove(&key)
                    };
                    if let Some(pending) = pending {
                        ban_user_and_maybe_release(
                            &bot,
                            &config,
                            chat_id,
                            from.id,
                            pending.chat_title.as_deref(),
                            pending.chat_username.as_deref(),
                            ban_release_store.clone(),
                            "failed to ban user on attempts exceeded",
                        )
                        .await;
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
                        send_captcha_log_if_enabled(
                            &bot,
                            &config,
                            &from,
                            chat_id,
                            pending.chat_title.as_deref(),
                            pending.chat_username.as_deref(),
                            false,
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
                let caption = captcha_caption(&from, remaining_secs, attempts_left, attempts_total);
                if let Some(png) = updated_png {
                    let media = InputMedia::Photo(
                        InputMediaPhoto::new(InputFile::memory(png))
                            .caption(caption)
                            .parse_mode(ParseMode::Html),
                    );
                    let _ = bot
                        .edit_message_media(chat_id, message.id, media)
                        .reply_markup(build_captcha_keyboard(
                            &options,
                            config.captcha_option_digits_to_emoji,
                        ))
                        .await;
                } else {
                    let _ = bot
                        .edit_message_caption(chat_id, message.id)
                        .caption(caption)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(build_captcha_keyboard(
                            &options,
                            config.captcha_option_digits_to_emoji,
                        ))
                        .await;
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
            let _ = bot
                .delete_message(chat_id, pending.captcha_message_id)
                .await;
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
                &from,
                chat_id,
                chat_title.as_deref(),
                chat_username.as_deref(),
                true,
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
    chat_id: ChatId,
    user_id: UserId,
    chat_title: Option<&str>,
    chat_username: Option<&str>,
    ban_release_store: Option<Arc<BanReleaseStore>>,
    error_context: &str,
) {
    if let Err(err) = bot.ban_chat_member(chat_id, user_id).await {
        log_telegram_error(
            config,
            LogLevel::Error,
            chat_id,
            chat_title,
            chat_username,
            error_context,
            &err,
        );
        return;
    }

    if !config.ban_release_enabled {
        return;
    }
    let Some(store) = ban_release_store else {
        return;
    };
    let release_at = Utc::now().timestamp() + config.ban_release_after_secs as i64;
    let Ok(user_id_i64) = i64::try_from(user_id.0) else {
        let err = "user id out of range";
        log_telegram_error(
            config,
            LogLevel::Warn,
            chat_id,
            chat_title,
            chat_username,
            "failed to store ban release job (user id out of range)",
            &err,
        );
        return;
    };
    if let Err(err) = store.upsert_job(chat_id.0, user_id_i64, release_at).await {
        log_telegram_error(
            config,
            LogLevel::Warn,
            chat_id,
            chat_title,
            chat_username,
            "failed to store ban release job",
            &err,
        );
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
                    out.push_str("🆎");
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

async fn send_captcha_log_if_enabled(
    bot: &Bot,
    config: &Config,
    user: &teloxide::types::User,
    chat_id: ChatId,
    chat_title: Option<&str>,
    chat_username: Option<&str>,
    success: bool,
) {
    if !config.captcha_log_enabled {
        return;
    }
    let Some(target_id) = config.captcha_log_chat_id else {
        return;
    };

    let tz_now = Utc::now().with_timezone(&config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S").to_string();

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

    let result = if success { "✅ sukses" } else { "🚫 gagal" };

    let mut lines = Vec::with_capacity(6);
    lines.push("🪵 Captcha Log".to_string());
    lines.push(format!(" ├⏱️ <code>{}</code>", escape_html(&ts)));
    lines.push(format!(" ├🙋🏽 {}", full_name));
    if let Some(line) = username_line {
        lines.push(line);
    }
    lines.push(format!(" ├👥 {}", group_label));
    lines.push(format!(" └{}", result));
    let message = lines.join("\n");

    if let Err(err) = bot
        .send_message(ChatId(target_id), message)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(true)
        .await
    {
        log_telegram_error(
            config,
            LogLevel::Warn,
            chat_id,
            chat_title,
            chat_username,
            "failed to send captcha log",
            &err,
        );
    }
}
