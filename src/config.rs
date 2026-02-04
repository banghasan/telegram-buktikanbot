use std::env;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use chrono_tz::Tz;
use url::Url;

use crate::utils::sanitize_log_text;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Polling,
    Webhook,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub token: String,
    pub captcha_len: usize,
    pub captcha_timeout_secs: u64,
    pub captcha_caption_update_secs: u64,
    pub captcha_width: u32,
    pub captcha_height: u32,
    pub captcha_option_count: usize,
    pub captcha_attempts: usize,
    pub captcha_option_digits_to_emoji: bool,
    pub delete_join_message: bool,
    pub delete_left_message: bool,
    pub ban_release_enabled: bool,
    pub ban_release_after_secs: u64,
    pub ban_release_db_path: String,
    pub log_enabled: bool,
    pub log_json: bool,
    pub log_level: LogLevel,
    pub captcha_log_enabled: bool,
    pub captcha_log_chat_id: Option<i64>,
    pub timezone: Tz,
    pub config_warnings: Vec<String>,
    pub run_mode: RunMode,
    pub webhook_url: Option<Url>,
    pub webhook_listen_addr: SocketAddr,
    pub webhook_secret_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let mut warnings = Vec::new();
        let token = env::var("BOT_TOKEN")
            .or_else(|_| env::var("TELOXIDE_TOKEN"))
            .map_err(|_| "BOT_TOKEN or TELOXIDE_TOKEN is required")?;
        let captcha_len = parse_env_usize("CAPTCHA_LEN", 6, 4, 12, &mut warnings);
        let captcha_timeout_secs =
            parse_env_u64("CAPTCHA_TIMEOUT_SECONDS", 120, 30, 600, &mut warnings);
        let captcha_caption_update_secs =
            parse_env_u64("CAPTCHA_CAPTION_UPDATE_SECONDS", 10, 2, 30, &mut warnings);
        let captcha_width = parse_env_u32("CAPTCHA_WIDTH", 320, 160, 400, &mut warnings);
        let captcha_height = parse_env_u32("CAPTCHA_HEIGHT", 100, 60, 200, &mut warnings);
        let captcha_option_count = parse_env_usize("CAPTCHA_OPTION_COUNT", 6, 3, 12, &mut warnings);
        let captcha_attempts = parse_env_usize("CAPTCHA_ATTEMPTS", 3, 1, 10, &mut warnings);
        let captcha_option_digits_to_emoji =
            parse_env_bool("CAPTCHA_OPTION_DIGITS_TO_EMOJI", true, &mut warnings);
        let delete_join_message = parse_env_bool("DELETE_JOIN_MESSAGE", true, &mut warnings);
        let delete_left_message = parse_env_bool("DELETE_LEFT_MESSAGE", true, &mut warnings);
        let ban_release_enabled = parse_env_bool("BAN_RELEASE_ENABLED", false, &mut warnings);
        let ban_release_after_secs = parse_env_u64(
            "BAN_RELEASE_AFTER_SECONDS",
            21600,
            60,
            2_592_000,
            &mut warnings,
        );
        let ban_release_db_path =
            env::var("BAN_RELEASE_DB_PATH").unwrap_or_else(|_| "buktikan.sqlite".to_string());
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
        let mut captcha_log_enabled = parse_env_bool("CAPTCHA_LOG_ENABLED", false, &mut warnings);
        let captcha_log_chat_id = parse_env_i64("CAPTCHA_LOG_CHAT_ID", &mut warnings);
        let run_mode = match env::var("RUN_MODE").ok() {
            Some(raw) => parse_run_mode(&raw).ok_or_else(|| {
                format!(
                    "RUN_MODE invalid ('{}'), expected polling|webhook",
                    sanitize_log_text(&raw)
                )
            })?,
            None => RunMode::Polling,
        };
        let webhook_path = normalize_webhook_path(
            env::var("WEBHOOK_PATH").unwrap_or_else(|_| "/telegram".to_string()),
        );
        let webhook_url = env::var("WEBHOOK_URL")
            .ok()
            .map(|raw| parse_webhook_url(&raw, &webhook_path))
            .transpose()?;
        let webhook_listen_addr = parse_webhook_listen_addr(&mut warnings);
        let webhook_secret_token = env::var("WEBHOOK_SECRET_TOKEN")
            .ok()
            .map(|raw| validate_webhook_secret(&raw))
            .transpose()?;
        let timezone = env::var("TIMEZONE")
            .ok()
            .and_then(|v| Tz::from_str(v.trim()).ok())
            .unwrap_or(chrono_tz::Asia::Jakarta);

        if captcha_log_enabled && captcha_log_chat_id.is_none() {
            captcha_log_enabled = false;
            warnings.push(
                "CAPTCHA_LOG_ENABLED true but CAPTCHA_LOG_CHAT_ID is missing or invalid; disabled"
                    .to_string(),
            );
        }

        if matches!(run_mode, RunMode::Webhook) && webhook_url.is_none() {
            return Err("WEBHOOK_URL is required for webhook mode".into());
        }

        Ok(Self {
            token,
            captcha_len,
            captcha_timeout_secs,
            captcha_caption_update_secs,
            captcha_width,
            captcha_height,
            captcha_option_count,
            captcha_attempts,
            captcha_option_digits_to_emoji,
            delete_join_message,
            delete_left_message,
            ban_release_enabled,
            ban_release_after_secs,
            ban_release_db_path,
            log_enabled,
            log_json,
            log_level,
            captcha_log_enabled,
            captcha_log_chat_id,
            timezone,
            config_warnings: warnings,
            run_mode,
            webhook_url,
            webhook_listen_addr,
            webhook_secret_token,
        })
    }
}

pub fn parse_log_level(input: &str) -> Option<LogLevel> {
    match input.trim().to_ascii_lowercase().as_str() {
        "info" => Some(LogLevel::Info),
        "warn" | "warning" => Some(LogLevel::Warn),
        "error" | "err" => Some(LogLevel::Error),
        _ => None,
    }
}

fn parse_run_mode(input: &str) -> Option<RunMode> {
    match input.trim().to_ascii_lowercase().as_str() {
        "polling" | "poll" => Some(RunMode::Polling),
        "webhook" | "webhooks" => Some(RunMode::Webhook),
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

fn parse_env_i64(name: &str, warnings: &mut Vec<String>) -> Option<i64> {
    let Some(raw) = env::var(name).ok() else {
        return None;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        warnings.push(format!("{} empty, ignoring", name));
        return None;
    }
    match trimmed.parse::<i64>() {
        Ok(value) => {
            if value == 0 {
                warnings.push(format!("{} invalid (0), ignoring", name));
                None
            } else {
                Some(value)
            }
        }
        Err(_) => {
            warnings.push(format!(
                "{} invalid ('{}'), ignoring",
                name,
                sanitize_log_text(&raw)
            ));
            None
        }
    }
}

fn parse_webhook_listen_addr(warnings: &mut Vec<String>) -> SocketAddr {
    let addr = env::var("WEBHOOK_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("WEBHOOK_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(8080);
    let ip = addr.trim().parse::<IpAddr>().unwrap_or_else(|_| {
        warnings.push(format!(
            "WEBHOOK_LISTEN_ADDR invalid ('{}'), using 0.0.0.0",
            sanitize_log_text(&addr)
        ));
        IpAddr::from([0, 0, 0, 0])
    });
    SocketAddr::new(ip, port)
}

fn normalize_webhook_path(path: String) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/telegram".to_string();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn parse_webhook_url(raw: &str, path: &str) -> Result<Url, String> {
    match Url::parse(raw.trim()) {
        Ok(mut url) => {
            url.set_path(path);
            Ok(url)
        }
        Err(_) => Err(format!(
            "WEBHOOK_URL invalid ('{}')",
            sanitize_log_text(raw)
        )),
    }
}

fn validate_webhook_secret(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    let len = token.len();
    if !(1..=256).contains(&len) {
        return Err(format!("WEBHOOK_SECRET_TOKEN length invalid ({})", len));
    }
    if token
        .as_bytes()
        .iter()
        .any(|c| !matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-'))
    {
        return Err("WEBHOOK_SECRET_TOKEN has invalid characters".to_string());
    }
    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard(Vec<(String, Option<String>)>);

    impl EnvGuard {
        fn set(vars: &[(&str, &str)], clears: &[&str]) -> Self {
            let mut saved = Vec::new();
            for key in clears {
                saved.push((key.to_string(), env::var(key).ok()));
                unsafe {
                    env::remove_var(key);
                }
            }
            for (key, value) in vars {
                saved.push((key.to_string(), env::var(key).ok()));
                unsafe {
                    env::set_var(key, value);
                }
            }
            Self(saved)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(val) => unsafe {
                        env::set_var(key, val);
                    },
                    None => unsafe {
                        env::remove_var(key);
                    },
                }
            }
        }
    }

    fn base_required_env() -> Vec<(&'static str, &'static str)> {
        vec![("BOT_TOKEN", "test-token")]
    }

    #[test]
    fn run_mode_invalid_fails_fast() {
        let _lock = ENV_LOCK.lock().unwrap();
        let mut vars = base_required_env();
        vars.push(("RUN_MODE", "invalid"));
        let _guard = EnvGuard::set(&vars, &["WEBHOOK_URL", "WEBHOOK_SECRET_TOKEN"]);
        let err = Config::from_env().unwrap_err().to_string();
        assert!(err.contains("RUN_MODE invalid"));
    }

    #[test]
    fn webhook_url_required_in_webhook_mode() {
        let _lock = ENV_LOCK.lock().unwrap();
        let mut vars = base_required_env();
        vars.push(("RUN_MODE", "webhook"));
        let _guard = EnvGuard::set(&vars, &["WEBHOOK_URL"]);
        let err = Config::from_env().unwrap_err().to_string();
        assert!(err.contains("WEBHOOK_URL is required"));
    }

    #[test]
    fn webhook_path_normalizes_and_applies_to_url() {
        let _lock = ENV_LOCK.lock().unwrap();
        let mut vars = base_required_env();
        vars.push(("RUN_MODE", "webhook"));
        vars.push(("WEBHOOK_URL", "https://example.com"));
        vars.push(("WEBHOOK_PATH", "tg"));
        let _guard = EnvGuard::set(&vars, &[]);
        let cfg = Config::from_env().unwrap();
        let url = cfg.webhook_url.unwrap();
        assert_eq!(url.as_str(), "https://example.com/tg");
    }

    #[test]
    fn webhook_secret_token_validation() {
        let _lock = ENV_LOCK.lock().unwrap();
        let mut vars = base_required_env();
        vars.push(("RUN_MODE", "webhook"));
        vars.push(("WEBHOOK_URL", "https://example.com"));
        vars.push(("WEBHOOK_SECRET_TOKEN", "bad token"));
        let _guard = EnvGuard::set(&vars, &[]);
        let err = Config::from_env().unwrap_err().to_string();
        assert!(err.contains("WEBHOOK_SECRET_TOKEN has invalid characters"));
    }
}
