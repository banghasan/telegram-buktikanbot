use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use captcha::Captcha;
use captcha::filters::Noise;
use chrono::DateTime;
use chrono_tz::Tz;
use teloxide::prelude::*;
use teloxide::types::{
    Chat, ChatMemberStatus, ChatMemberUpdated, ChatPermissions, InputFile, Message, MessageId,
    ParseMode, UserId,
};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct Config {
    token: String,
    captcha_len: usize,
    captcha_timeout_secs: u64,
    captcha_caption_update_secs: u64,
    captcha_width: u32,
    captcha_height: u32,
    log_enabled: bool,
    log_json: bool,
    log_level: LogLevel,
    timezone: Tz,
    config_warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    fn color(self) -> &'static str {
        match self {
            LogLevel::Info => color_green(),
            LogLevel::Warn => color_yellow(),
            LogLevel::Error => color_red(),
        }
    }
}

#[derive(Clone, Debug)]
struct PendingCaptcha {
    code: String,
    captcha_message_id: MessageId,
    user_display: String,
    chat_title: Option<String>,
    chat_username: Option<String>,
}

type CaptchaKey = (ChatId, UserId);
type SharedState = Arc<Mutex<HashMap<CaptchaKey, PendingCaptcha>>>;

enum CaptchaCheck {
    NoPending,
    Wrong,
    Verified(PendingCaptcha),
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("fatal error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env()?);
    let bot = Bot::new(config.token.clone());

    log_system_level(
        &config,
        LogLevel::Info,
        &format!(
            "config: captcha_len={} timeout={}s update={}s size={}x{} log_json={} log_level={} timezone={}",
            config.captcha_len,
            config.captcha_timeout_secs,
            config.captcha_caption_update_secs,
            config.captcha_width,
            config.captcha_height,
            config.log_json,
            config.log_level.as_str(),
            config.timezone
        ),
    );
    for warning in &config.config_warnings {
        log_system_level(&config, LogLevel::Warn, warning);
    }
    log_system(&config, "bot started");

    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .branch(
                    dptree::filter(|msg: Message| msg.new_chat_members().is_some()).endpoint({
                        let state = state.clone();
                        let config = config.clone();
                        move |bot: Bot, msg: Message| {
                            on_new_members(bot, msg, state.clone(), config.clone())
                        }
                    }),
                )
                .branch(
                    dptree::filter(|msg: Message| msg.text().is_some()).endpoint({
                        let state = state.clone();
                        let config = config.clone();
                        move |bot: Bot, msg: Message| {
                            on_text(bot, msg, state.clone(), config.clone())
                        }
                    }),
                )
                .branch(dptree::endpoint({
                    let config = config.clone();
                    move |msg: Message| on_non_text(msg, config.clone())
                })),
        )
        .branch(Update::filter_chat_member().endpoint({
            let state = state.clone();
            let config = config.clone();
            move |bot: Bot, update: ChatMemberUpdated| {
                on_chat_member_updated(bot, update, state.clone(), config.clone())
            }
        }));

    {
        let config = config.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                log_system(&config, "bot terminated (Ctrl+C)");
                std::process::exit(0);
            }
        });
    }

    Dispatcher::builder(bot, handler).build().dispatch().await;
    Ok(())
}

impl Config {
    fn from_env() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let mut warnings = Vec::new();
        let token = env::var("BOT_TOKEN")
            .or_else(|_| env::var("TELOXIDE_TOKEN"))
            .map_err(|_| "BOT_TOKEN or TELOXIDE_TOKEN is required")?;
        let captcha_len = parse_env_usize("CAPTCHA_LEN", 6, 4, 12, &mut warnings);
        let captcha_timeout_secs =
            parse_env_u64("CAPTCHA_TIMEOUT_SECONDS", 120, 30, 600, &mut warnings);
        let captcha_caption_update_secs =
            parse_env_u64("CAPTCHA_CAPTION_UPDATE_SECONDS", 10, 2, 30, &mut warnings);
        let captcha_width = parse_env_u32("CAPTCHA_WIDTH", 220, 160, 400, &mut warnings);
        let captcha_height = parse_env_u32("CAPTCHA_HEIGHT", 100, 60, 200, &mut warnings);
        let log_enabled = parse_env_bool("LOG_ENABLED", true, &mut warnings);
        let log_json = parse_env_bool("LOG_JSON", false, &mut warnings);
        let log_level = env::var("LOG_LEVEL")
            .ok()
            .and_then(|v| {
                parse_log_level(&v).or_else(|| {
                    warnings.push(format!(
                        "LOG_LEVEL invalid ('{}'), using INFO",
                        sanitize_log_text(&v)
                    ));
                    None
                })
            })
            .unwrap_or(LogLevel::Info);
        let timezone = env::var("TIMEZONE")
            .ok()
            .and_then(|v| Tz::from_str(v.trim()).ok())
            .unwrap_or(chrono_tz::Asia::Jakarta);

        Ok(Self {
            token,
            captcha_len,
            captcha_timeout_secs,
            captcha_caption_update_secs,
            captcha_width,
            captcha_height,
            log_enabled,
            log_json,
            log_level,
            timezone,
            config_warnings: warnings,
        })
    }
}

async fn on_new_members(
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

async fn on_chat_member_updated(
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

    let pending = PendingCaptcha {
        code,
        captcha_message_id: sent.id,
        user_display: format_user_display(&user),
        chat_title: chat_title.clone(),
        chat_username: chat_username.clone(),
    };

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

fn captcha_caption(user: &teloxide::types::User, remaining_secs: u64) -> String {
    let name = escape_html(&user.first_name);
    let mention = format!("<a href=\"tg://user?id={}\">{}</a>", user.id.0, name);
    format!(
        "🖐🏼 Hi, {mention}\n\n\
🙏🏼 Please solve this captcha within <code>{remaining_secs}</code> seconds.\n\
💁🏻‍♂️ Mohon ketik teks pada gambar ini, dalam <code>{remaining_secs}</code> detik.\n\n
🗒 <i>Setiap ketikan akan terhapus hingga kamu terverifikasi</i>.
"
    )
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
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

async fn on_text(
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
            let version = env!("CARGO_PKG_VERSION");
            let text = format!(
                "📦 *Version*\nApp: `{}`\nSource: [github](https://github.com/banghasan/telegram-buktikanbot)\nGroup: @botindonesia",
                version
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

async fn on_non_text(
    msg: Message,
    config: Arc<Config>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log_message(&config, &msg);
    Ok(())
}

fn generate_captcha(
    length: usize,
    width: u32,
    height: u32,
) -> Result<(String, Vec<u8>), Box<dyn Error + Send + Sync>> {
    let mut captcha = Captcha::new();
    captcha
        .add_chars(length as u32)
        .apply_filter(Noise::new(0.4))
        .view(width, height);

    let code = captcha.chars_as_string();
    let png = captcha.as_png().ok_or("failed to render captcha")?;
    Ok((code, png))
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

fn parse_log_level(input: &str) -> Option<LogLevel> {
    match input.trim().to_ascii_lowercase().as_str() {
        "info" => Some(LogLevel::Info),
        "warn" | "warning" => Some(LogLevel::Warn),
        "error" | "err" => Some(LogLevel::Error),
        _ => None,
    }
}

fn parse_env_usize(
    name: &str,
    default: usize,
    min: usize,
    max: usize,
    warnings: &mut Vec<String>,
) -> usize {
    let Some(raw) = env::var(name).ok() else {
        return default;
    };
    let Ok(value) = raw.trim().parse::<usize>() else {
        warnings.push(format!(
            "{} invalid ('{}'), using default {} (range {}..={})",
            name,
            sanitize_log_text(&raw),
            default,
            min,
            max
        ));
        return default;
    };
    if !(min..=max).contains(&value) {
        warnings.push(format!(
            "{} out of range ({}), using default {} (range {}..={})",
            name, value, default, min, max
        ));
        return default;
    }
    value
}

fn parse_env_u64(name: &str, default: u64, min: u64, max: u64, warnings: &mut Vec<String>) -> u64 {
    let Some(raw) = env::var(name).ok() else {
        return default;
    };
    let Ok(value) = raw.trim().parse::<u64>() else {
        warnings.push(format!(
            "{} invalid ('{}'), using default {} (range {}..={})",
            name,
            sanitize_log_text(&raw),
            default,
            min,
            max
        ));
        return default;
    };
    if !(min..=max).contains(&value) {
        warnings.push(format!(
            "{} out of range ({}), using default {} (range {}..={})",
            name, value, default, min, max
        ));
        return default;
    }
    value
}

fn parse_env_u32(name: &str, default: u32, min: u32, max: u32, warnings: &mut Vec<String>) -> u32 {
    let Some(raw) = env::var(name).ok() else {
        return default;
    };
    let Ok(value) = raw.trim().parse::<u32>() else {
        warnings.push(format!(
            "{} invalid ('{}'), using default {} (range {}..={})",
            name,
            sanitize_log_text(&raw),
            default,
            min,
            max
        ));
        return default;
    };
    if !(min..=max).contains(&value) {
        warnings.push(format!(
            "{} out of range ({}), using default {} (range {}..={})",
            name, value, default, min, max
        ));
        return default;
    }
    value
}

fn parse_env_bool(name: &str, default: bool, warnings: &mut Vec<String>) -> bool {
    let Some(raw) = env::var(name).ok() else {
        return default;
    };
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "true" | "1" | "yes" | "y" => true,
        "false" | "0" | "no" | "n" => false,
        _ => {
            warnings.push(format!(
                "{} invalid ('{}'), using default {}",
                name,
                sanitize_log_text(&raw),
                default
            ));
            default
        }
    }
}

fn log_enabled_at(config: &Config, level: LogLevel) -> bool {
    config.log_enabled && level >= config.log_level
}

fn check_captcha_answer(
    state: &mut HashMap<CaptchaKey, PendingCaptcha>,
    key: CaptchaKey,
    text: &str,
) -> CaptchaCheck {
    let Some(pending) = state.get(&key).cloned() else {
        return CaptchaCheck::NoPending;
    };
    if text.eq_ignore_ascii_case(&pending.code) {
        state.remove(&key);
        CaptchaCheck::Verified(pending)
    } else {
        CaptchaCheck::Wrong
    }
}

fn log_message(config: &Config, msg: &Message) {
    if !log_enabled_at(config, LogLevel::Info) {
        return;
    }
    let Some(user) = msg.from() else {
        return;
    };
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    let content = sanitize_log_text(&message_content_label(msg));
    let (title, chat_username) = chat_context(&msg.chat);
    let user_context = format_user_context(user);
    log_line(
        LogLevel::Info,
        config.log_json,
        &ts,
        msg.chat.id,
        title.as_deref(),
        chat_username.as_deref(),
        Some(&user_context),
        &content,
    );
}

fn chat_context(chat: &Chat) -> (Option<String>, Option<String>) {
    let title = chat.title().map(|t| sanitize_log_text(t.trim()));
    let chat_username = if title.is_some() {
        chat.username().map(|u| sanitize_log_text(u.trim()))
    } else {
        None
    };
    (title, chat_username)
}

fn log_system(config: &Config, text: &str) {
    log_system_level(config, LogLevel::Info, text);
}

fn log_system_level(config: &Config, level: LogLevel, text: &str) {
    if !log_enabled_at(config, level) {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    log_line(
        level,
        config.log_json,
        &ts,
        "system",
        None,
        None,
        Some("system"),
        &sanitize_log_text(text),
    );
}

fn log_user_event_with_chat(
    config: &Config,
    user: &teloxide::types::User,
    chat_id: ChatId,
    title: Option<&str>,
    chat_username: Option<&str>,
    text: &str,
) {
    if !log_enabled_at(config, LogLevel::Info) {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    let user_context = format_user_context(user);
    log_line(
        LogLevel::Info,
        config.log_json,
        &ts,
        chat_id,
        title,
        chat_username,
        Some(&user_context),
        &sanitize_log_text(text),
    );
}

fn log_user_event_by_display(
    config: &Config,
    user_id: UserId,
    chat_id: ChatId,
    title: Option<&str>,
    chat_username: Option<&str>,
    _user_display: &str,
    text: &str,
) {
    if !log_enabled_at(config, LogLevel::Info) {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    let user_context = format!("{}:{}", user_id.0, sanitize_log_text(_user_display));
    log_line(
        LogLevel::Info,
        config.log_json,
        &ts,
        chat_id,
        title,
        chat_username,
        Some(&user_context),
        &sanitize_log_text(text),
    );
}

fn log_telegram_error(
    config: &Config,
    level: LogLevel,
    chat_id: ChatId,
    title: Option<&str>,
    chat_username: Option<&str>,
    context: &str,
    err: &dyn std::fmt::Display,
) {
    if !log_enabled_at(config, level) {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    let summary = summarize_telegram_error(err);
    let message = format!("{}: {}", sanitize_log_text(context), summary);
    log_line(
        level,
        config.log_json,
        &ts,
        chat_id,
        title,
        chat_username,
        None,
        &message,
    );
}

fn log_line<T: std::fmt::Display>(
    level: LogLevel,
    log_json: bool,
    ts: &str,
    chat_id: T,
    title: Option<&str>,
    chat_username: Option<&str>,
    user_context: Option<&str>,
    message: &str,
) {
    if log_json {
        let payload = serde_json::json!({
            "ts": ts,
            "level": level.as_str(),
            "chat_id": chat_id.to_string(),
            "title": title,
            "chat_username": chat_username,
            "user_context": user_context,
            "message": message,
        });
        println!("{payload}");
        return;
    }
    println!(
        "{}",
        render_log_header(level, ts, &chat_id.to_string(), title, chat_username)
    );
    if let Some(user_context) = user_context {
        log_sub_line(&format!("({}) {}", user_context, message));
    } else {
        log_sub_line(message);
    }
}

fn log_sub_line(message: &str) {
    println!("{}", render_log_sub_line(message));
}

fn render_log_header(
    level: LogLevel,
    ts: &str,
    chat_id: &str,
    title: Option<&str>,
    chat_username: Option<&str>,
) -> String {
    let mut detail = String::new();
    if let Some(title) = title {
        detail.push_str(" : ");
        detail.push_str(color_magenta());
        detail.push_str(title);
        detail.push_str(color_reset());
    }
    if let Some(username) = chat_username {
        detail.push(' ');
        detail.push_str(color_blue());
        detail.push('@');
        detail.push_str(username);
        detail.push_str(color_reset());
    }
    let level_color = level.color();
    let level_label = level.as_str();
    if detail.is_empty() {
        format!(
            "{}[{}]{} {}{}{} {}{}{}",
            color_cyan(),
            ts,
            color_reset(),
            level_color,
            level_label,
            color_reset(),
            color_yellow(),
            chat_id,
            color_reset()
        )
    } else {
        format!(
            "{}[{}]{} {}{}{} {}{}{} {}{}",
            color_cyan(),
            ts,
            color_reset(),
            level_color,
            level_label,
            color_reset(),
            color_yellow(),
            chat_id,
            color_reset(),
            detail,
            color_reset()
        )
    }
}

fn render_log_sub_line(message: &str) -> String {
    if let Some(close_idx) = message.find(')') {
        let (context_with_paren, rest) = message.split_at(close_idx + 1);
        let rest = rest.trim_start();
        let context = context_with_paren
            .trim_start()
            .trim_start_matches('(')
            .trim_end_matches(')');
        let mut context_rendered = String::new();
        let mut remaining = context;
        if let Some(colon_idx) = remaining.find(':') {
            let (user_id, after_colon) = remaining.split_at(colon_idx);
            context_rendered.push_str(color_gray());
            context_rendered.push_str(user_id.trim());
            context_rendered.push_str(color_reset());
            context_rendered.push(':');
            remaining = after_colon[1..].trim_start();
        }
        let (name_part, username_part) = if let Some(at_idx) = remaining.find(" @") {
            let (name, username) = remaining.split_at(at_idx);
            (name.trim_end(), username.trim())
        } else if remaining.starts_with('@') {
            ("", remaining)
        } else {
            (remaining, "")
        };
        if !name_part.is_empty() {
            if !context_rendered.is_empty() && !context_rendered.ends_with(':') {
                context_rendered.push(' ');
            }
            context_rendered.push_str(color_magenta());
            context_rendered.push_str(name_part.trim());
            context_rendered.push_str(color_reset());
        }
        if !username_part.is_empty() {
            if !context_rendered.is_empty() {
                context_rendered.push(' ');
            }
            context_rendered.push_str(color_blue());
            context_rendered.push_str(username_part.trim());
            context_rendered.push_str(color_reset());
        }
        return format!(
            "{}  └ ({}) {}{}{}",
            color_white(),
            context_rendered,
            color_white(),
            rest,
            color_reset()
        );
    }
    format!("{}  └ {}{}", color_white(), message, color_reset())
}

fn now_in_timezone(tz: Tz) -> DateTime<Tz> {
    chrono::Utc::now().with_timezone(&tz)
}

fn summarize_telegram_error(err: &dyn std::fmt::Display) -> String {
    let mut msg = err
        .to_string()
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string();
    if let Some(idx) = msg.find(" (caused by ") {
        msg.truncate(idx);
    } else if let Some(idx) = msg.find(" caused by ") {
        msg.truncate(idx);
    }
    if msg.len() > 220 {
        msg.truncate(220);
        msg.push_str("...");
    }
    sanitize_log_text(&msg)
}

fn format_user_display(user: &teloxide::types::User) -> String {
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    let username = sanitize_log_text(user.username.as_deref().unwrap_or("-"));
    let username_fmt = format!("@{}", username);
    if last_name.is_empty() {
        format!("{first_name} {username_fmt}")
    } else {
        format!("{first_name} {last_name} {username_fmt}")
    }
}

fn format_user_context(user: &teloxide::types::User) -> String {
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    let username = sanitize_log_text(user.username.as_deref().unwrap_or(""));
    let name = if last_name.is_empty() {
        first_name.to_string()
    } else {
        format!("{first_name} {last_name}")
    };
    if username.is_empty() {
        format!("{}:{}", user.id.0, name)
    } else {
        format!("{}:{} @{}", user.id.0, name, username)
    }
}

fn message_content_label(msg: &Message) -> String {
    if let Some(text) = msg.text() {
        return text.to_string();
    }
    if msg.photo().is_some() {
        return "-image-".to_string();
    }
    if msg.document().is_some() {
        return "-document-".to_string();
    }
    if msg.sticker().is_some() {
        return "-sticker-".to_string();
    }
    if msg.video().is_some() {
        return "-video-".to_string();
    }
    if msg.audio().is_some() {
        return "-audio-".to_string();
    }
    if msg.voice().is_some() {
        return "-voice-".to_string();
    }
    if msg.animation().is_some() {
        return "-animation-".to_string();
    }
    if msg.video_note().is_some() {
        return "-video_note-".to_string();
    }
    if msg.contact().is_some() {
        return "-contact-".to_string();
    }
    if msg.location().is_some() {
        return "-location-".to_string();
    }
    if msg.venue().is_some() {
        return "-venue-".to_string();
    }
    if msg.poll().is_some() {
        return "-poll-".to_string();
    }
    if msg.dice().is_some() {
        return "-dice-".to_string();
    }
    if msg.game().is_some() {
        return "-game-".to_string();
    }
    if msg.invoice().is_some() {
        return "-invoice-".to_string();
    }
    if msg.successful_payment().is_some() {
        return "-payment-".to_string();
    }
    "-non-text-".to_string()
}

fn sanitize_log_text(input: &str) -> String {
    input
        .chars()
        .filter(|ch| !is_invisible_or_control(*ch))
        .collect::<String>()
}

fn is_invisible_or_control(ch: char) -> bool {
    if ch.is_control() {
        return true;
    }
    matches!(
        ch,
        '\u{00AD}' // SHY
            | '\u{061C}' // ALM
            | '\u{180E}' // MVS
            | '\u{200B}' // ZWSP
            | '\u{200C}' // ZWNJ
            | '\u{200D}' // ZWJ
            | '\u{200E}' // LRM
            | '\u{200F}' // RLM
            | '\u{202A}' // LRE
            | '\u{202B}' // RLE
            | '\u{202C}' // PDF
            | '\u{202D}' // LRO
            | '\u{202E}' // RLO
            | '\u{2060}' // WJ
            | '\u{2061}' // function application
            | '\u{2062}' // invisible times
            | '\u{2063}' // invisible separator
            | '\u{2064}' // invisible plus
            | '\u{2066}' // LRI
            | '\u{2067}' // RLI
            | '\u{2068}' // FSI
            | '\u{2069}' // PDI
            | '\u{206A}' // deprecated
            | '\u{206B}' // deprecated
            | '\u{206C}' // deprecated
            | '\u{206D}' // deprecated
            | '\u{206E}' // deprecated
            | '\u{206F}' // deprecated
            | '\u{FEFF}' // BOM/ZWNBSP
    )
}

fn color_cyan() -> &'static str {
    "\x1b[36m"
}

fn color_green() -> &'static str {
    "\x1b[32m"
}

fn color_yellow() -> &'static str {
    "\x1b[33m"
}

fn color_red() -> &'static str {
    "\x1b[31m"
}

fn color_magenta() -> &'static str {
    "\x1b[35m"
}

fn color_blue() -> &'static str {
    "\x1b[34m"
}

fn color_white() -> &'static str {
    "\x1b[37m"
}

fn color_gray() -> &'static str {
    "\x1b[90m"
}

fn color_reset() -> &'static str {
    "\x1b[0m"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_log_text_removes_control_chars() {
        let input = "hi\nthere\u{200B}";
        let out = sanitize_log_text(input);
        assert_eq!(out, "hithere");
    }

    #[test]
    fn render_log_header_includes_colors_and_context() {
        let out = render_log_header(
            LogLevel::Warn,
            "2020-01-01 00:00:00.000000",
            "123",
            Some("Group"),
            Some("groupname"),
        );
        assert!(out.contains(color_magenta()));
        assert!(out.contains(color_blue()));
        assert!(out.contains("WARN"));
        assert!(out.contains("Group"));
        assert!(out.contains("@groupname"));
    }

    #[test]
    fn render_log_sub_line_formats_user_context() {
        let out = render_log_sub_line("(123:Name @user) hello");
        assert!(out.contains(color_gray()));
        assert!(out.contains(color_magenta()));
        assert!(out.contains(color_blue()));
        assert!(out.contains("hello"));
    }

    #[test]
    fn check_captcha_answer_marks_verified_and_removes() {
        let mut state: HashMap<CaptchaKey, PendingCaptcha> = HashMap::new();
        let key = (ChatId(1), UserId(2));
        state.insert(
            key,
            PendingCaptcha {
                code: "AbC".to_string(),
                captcha_message_id: MessageId(10),
                user_display: "User @user".to_string(),
                chat_title: Some("Group".to_string()),
                chat_username: Some("groupname".to_string()),
            },
        );
        let wrong = check_captcha_answer(&mut state, key, "nope");
        assert!(matches!(wrong, CaptchaCheck::Wrong));
        assert!(state.contains_key(&key));

        let verified = check_captcha_answer(&mut state, key, "aBc");
        assert!(matches!(verified, CaptchaCheck::Verified(_)));
        assert!(!state.contains_key(&key));
    }
}
