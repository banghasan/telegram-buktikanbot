use std::error::Error;
use std::sync::Arc;

use teloxide::prelude::*;

mod captcha;
mod config;
mod handlers;
mod logging;
mod utils;

use crate::captcha::SharedState;
use crate::config::{Config, LogLevel};
use crate::handlers::{on_chat_member_updated, on_new_members, on_non_text, on_text};
use crate::logging::{log_system, log_system_level};

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

    let state: SharedState = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .branch(
                    dptree::filter(|msg: teloxide::types::Message| msg.new_chat_members().is_some())
                        .endpoint({
                            let state = state.clone();
                            let config = config.clone();
                            move |bot: Bot, msg: teloxide::types::Message| {
                                on_new_members(bot, msg, state.clone(), config.clone())
                            }
                        }),
                )
                .branch(
                    dptree::filter(|msg: teloxide::types::Message| msg.text().is_some()).endpoint({
                        let state = state.clone();
                        let config = config.clone();
                        move |bot: Bot, msg: teloxide::types::Message| {
                            on_text(bot, msg, state.clone(), config.clone())
                        }
                    }),
                )
                .branch(dptree::endpoint({
                    let config = config.clone();
                    move |msg: teloxide::types::Message| on_non_text(msg, config.clone())
                })),
        )
        .branch(Update::filter_chat_member().endpoint({
            let state = state.clone();
            let config = config.clone();
            move |bot: Bot, update: teloxide::types::ChatMemberUpdated| {
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
