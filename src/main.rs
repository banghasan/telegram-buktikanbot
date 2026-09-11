use std::error::Error;
use std::sync::Arc;

use chrono::TimeZone;
use teloxide::prelude::*;
use teloxide::types::{ChatId, MessageId, ParseMode, UserId};
use teloxide::update_listeners::webhooks;

mod ban_release;
mod captcha;
mod captcha_quotes;
mod config;
mod handlers;
mod logging;
mod utils;

use crate::ban_release::{BanReleaseJob, BanReleaseStore, worker_interval};
use crate::captcha::SharedState;
use crate::config::{Config, LogLevel, RunMode};
use crate::handlers::{
    on_callback_query, on_chat_member_updated, on_left_member, on_new_members, on_non_text,
    on_text, restore_pending_captchas,
};
use crate::logging::{log_system, log_system_block, log_system_level};
use crate::utils::{escape_html, sanitize_log_text};

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
        "(system) config: captcha_len={} timeout={}s update={}s size={}x{} options={} attempts={} option_digits_to_emoji={} delete_join_message={} delete_left_message={} ban_release_enabled={} ban_release_after_secs={} ban_release_db_path={} log_json={} log_level={} captcha_log_enabled={} captcha_log_chat_id={} captcha_log_message_thread_id={} timezone={} run_mode={}",
        config.captcha_len,
        config.captcha_timeout_secs,
        config.captcha_caption_update_secs,
        config.captcha_width,
        config.captcha_height,
        config.captcha_option_count,
        config.captcha_attempts,
        config.captcha_option_digits_to_emoji,
        config.delete_join_message,
        config.delete_left_message,
        config.ban_release_enabled,
        config.ban_release_after_secs,
        config.ban_release_db_path,
        config.log_json,
        config.log_level.as_str(),
        config.captcha_log_enabled,
        config
            .captcha_log_chat_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string()),
        config
            .captcha_log_message_thread_id
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
    let state_store = match BanReleaseStore::init(config.ban_release_db_path.clone()).await {
        Ok(store) => Arc::new(store),
        Err(err) => {
            log_system_level(
                &config,
                LogLevel::Error,
                &format!("state store init failed: {err}"),
            );
            return Err(format!("state store init failed: {err}").into());
        }
    };
    log_system_level(
        &config,
        LogLevel::Info,
        &format!(
            "state store initialized path={}",
            config.ban_release_db_path
        ),
    );
    restore_pending_captchas(&bot, &state, &config, state_store.clone()).await?;
    let ban_release_store = Some(state_store);

    if config.ban_release_enabled
        && let Some(store) = ban_release_store.clone()
    {
        let bot = bot.clone();
        let config = config.clone();
        tokio::spawn(async move {
            run_ban_release_worker(bot, config, store).await;
        });
    }

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
                        let ban_release_store = ban_release_store.clone();
                        move |bot: Bot, msg: teloxide::types::Message| {
                            on_new_members(
                                bot,
                                msg,
                                state.clone(),
                                config.clone(),
                                ban_release_store.clone(),
                            )
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
                            let ban_release_store = ban_release_store.clone();
                            move |bot: Bot, msg: teloxide::types::Message| {
                                on_text(
                                    bot,
                                    msg,
                                    state.clone(),
                                    config.clone(),
                                    ban_release_store.clone(),
                                )
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
            let ban_release_store = ban_release_store.clone();
            move |bot: Bot, update: teloxide::types::ChatMemberUpdated| {
                on_chat_member_updated(
                    bot,
                    update,
                    state.clone(),
                    config.clone(),
                    ban_release_store.clone(),
                )
            }
        }))
        .branch(Update::filter_callback_query().endpoint({
            let state = state.clone();
            let config = config.clone();
            let ban_release_store = ban_release_store.clone();
            move |bot: Bot, query: teloxide::types::CallbackQuery| {
                on_callback_query(
                    bot,
                    query,
                    state.clone(),
                    config.clone(),
                    ban_release_store.clone(),
                )
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

async fn run_ban_release_worker(bot: Bot, config: Arc<Config>, store: Arc<BanReleaseStore>) {
    log_system_level(
        &config,
        LogLevel::Info,
        "ban release worker started (interval 60s)",
    );
    loop {
        if let Err(err) = process_due_releases(&bot, &config, &store).await {
            log_system_level(
                &config,
                LogLevel::Warn,
                &format!("ban release worker error: {err}"),
            );
        }
        tokio::time::sleep(worker_interval()).await;
    }
}

async fn process_due_releases(
    bot: &Bot,
    config: &Arc<Config>,
    store: &Arc<BanReleaseStore>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let now = chrono::Utc::now().timestamp();
    let due = store.fetch_due(now).await?;
    for job in due {
        let Ok(user_id_u64) = u64::try_from(job.user_id) else {
            log_system_level(
                config,
                LogLevel::Warn,
                &format!(
                    "invalid user_id in ban release store; chat={} user_id={}",
                    job.chat_id, job.user_id
                ),
            );
            store.delete_job(job.chat_id, job.user_id).await?;
            continue;
        };
        if let Err(err) = bot
            .unban_chat_member(ChatId(job.chat_id), UserId(user_id_u64))
            .await
        {
            log_system_level(
                config,
                LogLevel::Warn,
                &format!(
                    "failed to unban user {} in chat {}: {err}",
                    job.user_id, job.chat_id
                ),
            );
            continue;
        }
        store.delete_job(job.chat_id, job.user_id).await?;
        send_ban_release_log_if_enabled(bot, config, &job).await;
    }
    Ok(())
}

async fn send_ban_release_log_if_enabled(bot: &Bot, config: &Arc<Config>, job: &BanReleaseJob) {
    if !config.captcha_log_enabled {
        return;
    }
    let Some(target_id) = job.log_chat_id.or(config.captcha_log_chat_id) else {
        return;
    };

    let ts = format_log_timestamp(config, chrono::Utc::now().timestamp());
    let full_name = escape_html(&sanitize_log_text(job.user_name.trim()));
    let username_line = job.user_username.as_deref().map(|raw| {
        let username = escape_html(&sanitize_log_text(raw.trim()));
        format!(" ├👤 @{username}")
    });

    let group_label = match (job.chat_username.as_deref(), job.chat_title.as_deref()) {
        (Some(username), Some(title)) => format!("@{} : {}", username.trim(), title.trim()),
        (Some(username), None) => format!("@{}", username.trim()),
        (None, Some(title)) => title.trim().to_string(),
        (None, None) => "unknown".to_string(),
    };
    let group_label = escape_html(&sanitize_log_text(&group_label));

    let mut lines = Vec::with_capacity(12);
    lines.push("♻️ BAN — DILEPAS".to_string());
    lines.push(format!(" ├🕒 kejadian: <code>{}</code>", escape_html(&ts)));
    lines.push(format!(" ├🙋🏽 {}", full_name));
    if let Some(line) = username_line {
        lines.push(line);
    }
    lines.push(format!(" ├👥 {}", group_label));
    lines.push(format!(" ├🆔 user: <code>{}</code>", job.user_id));
    lines.push(format!(" ├🆔 chat: <code>{}</code>", job.chat_id));
    lines.push(format!(
        " ├📅 jadwal unban: <code>{}</code>",
        escape_html(&format_log_timestamp(config, job.release_at))
    ));
    lines.push(" └✅ ban sementara telah dilepas.".to_string());
    let message = lines.join("\n");

    let mut request = bot
        .send_message(ChatId(target_id), message)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(true);
    let thread_id = if job.log_message_id.is_some() {
        job.log_message_thread_id
    } else {
        job.log_message_thread_id
            .or(config.captcha_log_message_thread_id)
    };
    if let Some(thread_id) = thread_id {
        request = request.message_thread_id(thread_id);
    }
    if let Some(message_id) = job.log_message_id {
        request = request
            .reply_to_message_id(MessageId(message_id))
            .allow_sending_without_reply(true);
    }
    if let Err(err) = request.await {
        log_system_level(
            config,
            LogLevel::Warn,
            &format!("failed to send ban release log: {err}"),
        );
    }
}

fn format_log_timestamp(config: &Config, timestamp: i64) -> String {
    match config.timezone.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(value) => value.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
        _ => format!("invalid timestamp ({timestamp})"),
    }
}
