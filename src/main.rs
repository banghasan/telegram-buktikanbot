use std::error::Error;
use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::update_listeners::webhooks;

mod captcha;
mod config;
mod handlers;
mod logging;
mod utils;

use crate::captcha::SharedState;
use crate::config::{Config, LogLevel, RunMode};
use crate::handlers::{
    on_callback_query, on_chat_member_updated, on_left_member, on_new_members, on_non_text, on_text,
};
use crate::logging::{log_system, log_system_block, log_system_level};

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

    let version_line = format!("(system) version: {}", env!("CARGO_PKG_VERSION"));
    let config_line = format!(
        "(system) config: captcha_len={} timeout={}s update={}s size={}x{} options={} attempts={} delete_join_message={} delete_left_message={} log_json={} log_level={} captcha_log_enabled={} captcha_log_chat_id={} timezone={} run_mode={}",
        config.captcha_len,
        config.captcha_timeout_secs,
        config.captcha_caption_update_secs,
        config.captcha_width,
        config.captcha_height,
        config.captcha_option_count,
        config.captcha_attempts,
        config.delete_join_message,
        config.delete_left_message,
        config.log_json,
        config.log_level.as_str(),
        config.captcha_log_enabled,
        config
            .captcha_log_chat_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string()),
        config.timezone,
        match config.run_mode {
            RunMode::Polling => "polling",
            RunMode::Webhook => "webhook",
        }
    );
    let bot_username = match bot.get_me().await {
        Ok(me) => me.username.as_deref().unwrap_or("unknown").to_string(),
        Err(err) => {
            log_system_level(&config, LogLevel::Warn, &format!("getMe failed: {err}"));
            "unknown".to_string()
        }
    };
    let started_line = format!("(system) bot started @{}", bot_username);
    log_system_block(
        &config,
        LogLevel::Info,
        &[started_line, version_line, config_line],
    );
    for warning in &config.config_warnings {
        log_system_level(&config, LogLevel::Warn, warning);
    }

    let state: SharedState = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .branch(
                    dptree::filter(|msg: teloxide::types::Message| {
                        msg.new_chat_members().is_some()
                    })
                    .endpoint({
                        let state = state.clone();
                        let config = config.clone();
                        move |bot: Bot, msg: teloxide::types::Message| {
                            on_new_members(bot, msg, state.clone(), config.clone())
                        }
                    }),
                )
                .branch(
                    dptree::filter(|msg: teloxide::types::Message| {
                        msg.left_chat_member().is_some()
                    })
                    .endpoint({
                        let config = config.clone();
                        move |bot: Bot, msg: teloxide::types::Message| {
                            on_left_member(bot, msg, config.clone())
                        }
                    }),
                )
                .branch(
                    dptree::filter(|msg: teloxide::types::Message| msg.text().is_some()).endpoint(
                        {
                            let state = state.clone();
                            let config = config.clone();
                            move |bot: Bot, msg: teloxide::types::Message| {
                                on_text(bot, msg, state.clone(), config.clone())
                            }
                        },
                    ),
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
        }))
        .branch(Update::filter_callback_query().endpoint({
            let state = state.clone();
            let config = config.clone();
            move |bot: Bot, query: teloxide::types::CallbackQuery| {
                on_callback_query(bot, query, state.clone(), config.clone())
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

    match config.run_mode {
        RunMode::Polling => {
            if let Err(err) = bot.delete_webhook().send().await {
                log_system_level(
                    &config,
                    LogLevel::Warn,
                    &format!("delete webhook failed: {err}"),
                );
            }
            Dispatcher::builder(bot, handler).build().dispatch().await;
        }
        RunMode::Webhook => {
            let Some(url) = config.webhook_url.clone() else {
                return Err("WEBHOOK_URL is required for webhook mode".into());
            };
            let mut options = webhooks::Options::new(config.webhook_listen_addr, url);
            if let Some(secret) = config.webhook_secret_token.clone() {
                options = options.secret_token(secret);
            }
            log_system_level(
                &config,
                LogLevel::Info,
                &format!(
                    "webhook: listen={} url={}",
                    config.webhook_listen_addr, options.url
                ),
            );
            let listener = webhooks::axum(bot.clone(), options)
                .await
                .map_err(|err| format!("failed to setup webhook: {err}"))?;
            Dispatcher::builder(bot, handler)
                .build()
                .dispatch_with_listener(
                    listener,
                    LoggingErrorHandler::with_custom_text("update listener error"),
                )
                .await;
        }
    }
    Ok(())
}
