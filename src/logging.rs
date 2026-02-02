use chrono::DateTime;
use chrono_tz::Tz;
use teloxide::types::{Chat, ChatId, Message, UserId};

use crate::config::{Config, LogLevel};
use crate::utils::{format_user_context, message_content_label, sanitize_log_text};

pub fn log_enabled_at(config: &Config, level: LogLevel) -> bool {
    config.log_enabled && level >= config.log_level
}

pub fn log_message(config: &Config, msg: &Message) {
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

pub fn chat_context(chat: &Chat) -> (Option<String>, Option<String>) {
    let title = chat.title().map(|t| sanitize_log_text(t.trim()));
    let chat_username = if title.is_some() {
        chat.username().map(|u| sanitize_log_text(u.trim()))
    } else {
        None
    };
    (title, chat_username)
}

pub fn log_system(config: &Config, text: &str) {
    log_system_level(config, LogLevel::Info, text);
}

pub fn log_system_level(config: &Config, level: LogLevel, text: &str) {
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

pub fn log_user_event_with_chat(
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

pub fn log_user_event_by_display(
    config: &Config,
    user_id: UserId,
    chat_id: ChatId,
    title: Option<&str>,
    chat_username: Option<&str>,
    user_display: &str,
    text: &str,
) {
    if !log_enabled_at(config, LogLevel::Info) {
        return;
    }
    let tz_now = now_in_timezone(config.timezone);
    let ts = tz_now.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    let user_context = format!("{}:{}", user_id.0, sanitize_log_text(user_display));
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

pub fn log_telegram_error(
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
    let level_color = level_color(level);
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

fn level_color(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Info => color_green(),
        LogLevel::Warn => color_yellow(),
        LogLevel::Error => color_red(),
    }
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
}
