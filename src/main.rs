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
    ChatMemberStatus, ChatMemberUpdated, ChatPermissions, InputFile, Message, MessageId, ParseMode,
    UserId,
};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct Config {
    token: String,
    captcha_len: usize,
    captcha_timeout_secs: u64,
    captcha_width: u32,
    captcha_height: u32,
    log_enabled: bool,
    timezone: Tz,
}

#[derive(Clone, Debug)]
struct PendingCaptcha {
    code: String,
    captcha_message_id: MessageId,
    user_display: String,
}

type CaptchaKey = (ChatId, UserId);
type SharedState = Arc<Mutex<HashMap<CaptchaKey, PendingCaptcha>>>;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("fatal error: {err}");
    }
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env()?);
    let bot = Bot::new(config.token.clone());

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
        let token = env::var("BOT_TOKEN")
            .or_else(|_| env::var("TELOXIDE_TOKEN"))
            .map_err(|_| "BOT_TOKEN or TELOXIDE_TOKEN is required")?;
        let captcha_len = env::var("CAPTCHA_LEN")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(6);
        let captcha_timeout_secs = env::var("CAPTCHA_TIMEOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(120);
        let captcha_width = env::var("CAPTCHA_WIDTH")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(220);
        let captcha_height = env::var("CAPTCHA_HEIGHT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(100);
        let log_enabled = env::var("LOG_ENABLED")
            .ok()
            .map(|v| v.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(true);
        let timezone = env::var("TIMEZONE")
            .ok()
            .and_then(|v| Tz::from_str(v.trim()).ok())
            .unwrap_or(chrono_tz::Asia::Jakarta);

        Ok(Self {
            token,
            captcha_len,
            captcha_timeout_secs,
            captcha_width,
            captcha_height,
            log_enabled,
            timezone,
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

    for member in members {
        start_captcha_for_user(&bot, msg.chat.id, member.clone(), &state, &config).await?;
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
    start_captcha_for_user(&bot, update.chat.id, user, &state, &config).await?;
    Ok(())
}

async fn start_captcha_for_user(
    bot: &Bot,
    chat_id: ChatId,
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
        eprintln!("failed to restrict user to text-only: {err}");
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
    };

    {
        let mut guard = state.lock().await;
        guard.insert((chat_id, user.id), pending);
    }
    log_user_event(config, &user, chat_id, "-> ⏳ captcha sent");

    let bot_clone = bot.clone();
    let state_clone = state.clone();
    let config_clone = config.clone();
    let user_clone = user.clone();
    let user_id = user.id;
    let timeout = config.captcha_timeout_secs;
    let captcha_message_id = sent.id;

    tokio::spawn(async move {
        let mut remaining = timeout;
        while remaining > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            remaining = remaining.saturating_sub(1);

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
                eprintln!("failed to ban user on timeout: {err}");
            }
            if let Err(err) = bot_clone
                .delete_message(chat_id, pending.captcha_message_id)
                .await
            {
                eprintln!("failed to delete captcha message on timeout: {err}");
            }
            log_user_event_by_display(
                &config_clone,
                user_id,
                chat_id,
                &pending.user_display,
                "captcha timeout, user banned",
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
🗒 Setiap ketikan akan terhapus hingga kamu terverifikasi.
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
    let pending = {
        let guard = state.lock().await;
        guard.get(&key).cloned()
    };

    if let Some(pending) = pending {
        let is_correct = text.eq_ignore_ascii_case(&pending.code);

        let _ = bot.delete_message(msg.chat.id, msg.id).await;

        if !is_correct {
            log_user_event(&config, user, msg.chat.id, "<- 🚫 captcha wrong");
            return Ok(());
        }

        {
            let mut guard = state.lock().await;
            guard.remove(&key);
        }

        let _ = bot
            .delete_message(msg.chat.id, pending.captcha_message_id)
            .await;
        if let Err(err) = restore_chat_permissions(&bot, msg.chat.id, user.id).await {
            eprintln!("failed to restore user permissions: {err}");
        }
        log_user_event(&config, user, msg.chat.id, "==> ✅ captcha verified");
        return Ok(());
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
                eprintln!("failed to edit ping response: {err}");
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
                eprintln!("failed to send version response: {err}");
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

fn log_message(config: &Config, msg: &Message) {
    if !config.log_enabled {
        return;
    }
    let Some(user) = msg.from() else {
        return;
    };
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f");
    let user_id = user.id.0;
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    let username = sanitize_log_text(user.username.as_deref().unwrap_or("-"));
    let content = sanitize_log_text(&message_content_label(msg));
    let username_fmt = format!("@{}", username);
    let name = if last_name.is_empty() {
        first_name.to_string()
    } else {
        format!("{first_name} {last_name}")
    };
    println!(
        "{}[{}]{} {}INFO{} {}[{}@{}]{} {}({} {}){}: {}{}{}",
        color_cyan(),
        ts,
        color_reset(),
        color_green(),
        color_reset(),
        color_yellow(),
        user_id,
        msg.chat.id,
        color_reset(),
        color_magenta(),
        name,
        username_fmt,
        color_reset(),
        color_white(),
        content,
        color_reset()
    );
}

fn log_system(config: &Config, text: &str) {
    if !config.log_enabled {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f");
    println!(
        "{}[{}]{} {}INFO{} {}[system@system]{} {}(system){}: {}{}{}",
        color_cyan(),
        ts,
        color_reset(),
        color_green(),
        color_reset(),
        color_yellow(),
        color_reset(),
        color_magenta(),
        color_reset(),
        color_white(),
        text,
        color_reset()
    );
}

fn log_user_event(config: &Config, user: &teloxide::types::User, chat_id: ChatId, text: &str) {
    if !config.log_enabled {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f");
    let user_id = user.id.0;
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    let username = sanitize_log_text(user.username.as_deref().unwrap_or("-"));
    let username_fmt = format!("@{}", username);
    let name = if last_name.is_empty() {
        first_name.to_string()
    } else {
        format!("{first_name} {last_name}")
    };
    println!(
        "{}[{}]{} {}INFO{} {}[{}@{}]{} {}({} {}){}: {}{}{}",
        color_cyan(),
        ts,
        color_reset(),
        color_green(),
        color_reset(),
        color_yellow(),
        user_id,
        chat_id,
        color_reset(),
        color_magenta(),
        name,
        username_fmt,
        color_reset(),
        color_white(),
        text,
        color_reset()
    );
}

fn log_user_event_by_display(
    config: &Config,
    user_id: UserId,
    chat_id: ChatId,
    user_display: &str,
    text: &str,
) {
    if !config.log_enabled {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f");
    println!(
        "{}[{}]{} {}INFO{} {}[{}@{}]{} {}({}){}: {}{}{}",
        color_cyan(),
        ts,
        color_reset(),
        color_green(),
        color_reset(),
        color_yellow(),
        user_id.0,
        chat_id,
        color_reset(),
        color_magenta(),
        user_display,
        color_reset(),
        color_white(),
        sanitize_log_text(text),
        color_reset()
    );
}

fn now_in_timezone(tz: Tz) -> DateTime<Tz> {
    chrono::Utc::now().with_timezone(&tz)
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

fn color_magenta() -> &'static str {
    "\x1b[35m"
}

fn color_white() -> &'static str {
    "\x1b[37m"
}

fn color_reset() -> &'static str {
    "\x1b[0m"
}
