use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use teloxide::prelude::*;
use teloxide::types::{
    ChatMemberStatus, ChatMemberUpdated, ChatPermissions, InputFile, Message, ParseMode, UserId,
};

use crate::captcha::{
    CaptchaCheck, SharedState, captcha_caption, check_captcha_answer, generate_captcha,
    make_pending_captcha,
};
use crate::config::{Config, LogLevel};
use crate::logging::{
    chat_context, log_message, log_telegram_error, log_user_event_by_display,
    log_user_event_with_chat,
};

pub async fn on_new_members(
    bot: Bot,
    msg: Message,
    state: SharedState,
    config: Arc<Config>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(members) = msg.new_chat_members() else {
        return Ok(());
    };

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
        )
        .await?;
    }

    Ok(())
}

pub async fn on_chat_member_updated(
    bot: Bot,
    update: ChatMemberUpdated,
    state: SharedState,
    config: Arc<Config>,
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

    let text_only = ChatPermissions::SEND_MESSAGES;
    if let Err(err) = bot.restrict_chat_member(chat_id, user.id, text_only).await {
        log_telegram_error(
            config,
            LogLevel::Error,
            chat_id,
            chat_title.as_deref(),
            chat_username.as_deref(),
            "failed to restrict user to text-only",
            &err,
        );
    }

    let (code, png) = generate_captcha(
        config.captcha_len,
        config.captcha_width,
        config.captcha_height,
    )?;

    let caption = captcha_caption(&user, config.captcha_timeout_secs);
    let sent = bot
        .send_photo(chat_id, InputFile::memory(png))
        .caption(caption)
        .parse_mode(ParseMode::Html)
        .await?;

    let pending = make_pending_captcha(
        code,
        sent.id,
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

            let caption = captcha_caption(&user_clone, remaining);
            let _ = bot_clone
                .edit_message_caption(chat_id, captcha_message_id)
                .caption(caption)
                .parse_mode(ParseMode::Html)
                .await;
        }

        let pending = {
            let mut guard = state_clone.lock().await;
            guard.remove(&(chat_id, user_id))
        };

        if let Some(pending) = pending {
            if let Err(err) = bot_clone.ban_chat_member(chat_id, user_id).await {
                log_telegram_error(
                    &config_clone,
                    LogLevel::Error,
                    chat_id,
                    pending.chat_title.as_deref(),
                    pending.chat_username.as_deref(),
                    "failed to ban user on timeout",
                    &err,
                );
            }
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
        }
    });

    Ok(())
}

pub async fn on_text(
    bot: Bot,
    msg: Message,
    state: SharedState,
    config: Arc<Config>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(user) = msg.from() else {
        return Ok(());
    };
    if user.is_bot {
        return Ok(());
    }

    let text = match msg.text() {
        Some(text) => text.trim().to_string(),
        None => return Ok(()),
    };

    let key = (msg.chat.id, user.id);
    let check = {
        let mut guard = state.lock().await;
        check_captcha_answer(&mut guard, key, &text)
    };

    match check {
        CaptchaCheck::NoPending => {}
        CaptchaCheck::Wrong => {
            let _ = bot.delete_message(msg.chat.id, msg.id).await;
            let (chat_title, chat_username) = chat_context(&msg.chat);
            log_user_event_with_chat(
                &config,
                user,
                msg.chat.id,
                chat_title.as_deref(),
                chat_username.as_deref(),
                "<- 🚫 captcha wrong",
            );
            return Ok(());
        }
        CaptchaCheck::Verified(pending) => {
            let _ = bot.delete_message(msg.chat.id, msg.id).await;
            let _ = bot
                .delete_message(msg.chat.id, pending.captcha_message_id)
                .await;
            if let Err(err) = restore_chat_permissions(&bot, msg.chat.id, user.id).await {
                let (chat_title, chat_username) = chat_context(&msg.chat);
                log_telegram_error(
                    &config,
                    LogLevel::Error,
                    msg.chat.id,
                    chat_title.as_deref(),
                    chat_username.as_deref(),
                    "failed to restore user permissions",
                    &err,
                );
            }
            let (chat_title, chat_username) = chat_context(&msg.chat);
            log_user_event_with_chat(
                &config,
                user,
                msg.chat.id,
                chat_title.as_deref(),
                chat_username.as_deref(),
                "==> ✅ captcha verified",
            );
            return Ok(());
        }
    }

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
            let text = "🤖 *Bot Verifikasi User*\n👤 oleh *bangHasan* \\(@hasanudinhs\\)\n👥 Group @botindonesia";
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
