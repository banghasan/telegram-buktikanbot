use std::env;
use std::error::Error;
use std::str::FromStr;

use chrono_tz::Tz;

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

#[derive(Clone, Debug)]
pub struct Config {
    pub token: String,
    pub captcha_len: usize,
    pub captcha_timeout_secs: u64,
    pub captcha_caption_update_secs: u64,
    pub captcha_width: u32,
    pub captcha_height: u32,
    pub log_enabled: bool,
    pub log_json: bool,
    pub log_level: LogLevel,
    pub timezone: Tz,
    pub config_warnings: Vec<String>,
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

pub fn parse_log_level(input: &str) -> Option<LogLevel> {
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
