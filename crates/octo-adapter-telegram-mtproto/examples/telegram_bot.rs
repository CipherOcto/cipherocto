//! Example binary: connect a Telegram bot via the Bot-API HTTP
//! fallback (Phase 3 / sub-mission 0850ab-c-http), then send a
//! one-shot message and long-poll for a few updates.
//!
//! Usage:
//!
//! ```text
//! TELEGRAM_BOT_TOKEN="123:abc" \
//! TELEGRAM_DEST_CHAT=12345 \
//! cargo run -p octo-adapter-telegram-mtproto --example telegram_bot \
//!     --features bot-api
//! ```
//!
//! The MTProto path is exercised by the integration tests in
//! `real_client.rs` (gated on `--features real-network`) and
//! the cipherocto gateway's adapter registry; this binary is
//! a smoke test for the Bot-API HTTP path only.
//!
//! The binary demonstrates the full Phase 3 surface:
//! 1. `BotApiClient::new(token)` builds a configured client.
//! 2. `client.get_me()` verifies the token and returns the
//!    bot's identity.
//! 3. `client.send_message(chat_id, text)` sends a message.
//! 4. `client.get_updates(None, 5)` long-polls for 5 s.

use std::env;
use std::process::ExitCode;

use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[cfg(feature = "bot-api")]
use octo_adapter_telegram_mtproto::{BotApiClient, BotApiConfig};

/// Print the usage help to stderr. Examples are run
/// interactively, so we use `eprintln` here for the help
/// text itself (which is pre-init output — the
/// `tracing_subscriber` has not been initialised yet). The
/// runtime output uses `tracing` (R15-C16 fix; the previous
/// example used `eprintln!` for every status message,
/// which violated the mission's tracing-only rule).
fn print_usage_and_exit() -> ExitCode {
    eprintln!("usage: TELEGRAM_BOT_TOKEN=... [TELEGRAM_DEST_CHAT=...] telegram_bot");
    eprintln!();
    eprintln!("environment:");
    eprintln!("  TELEGRAM_BOT_TOKEN   bot token (required)");
    eprintln!("  TELEGRAM_DEST_CHAT   destination chat id (required)");
    eprintln!(
        "  TELEGRAM_TEXT        message text (default: 'hello from octo-adapter-telegram-mtproto')"
    );
    eprintln!("  TELEGRAM_LONG_POLL   long-poll seconds (default: 5, max: 50)");
    ExitCode::from(2)
}

/// Initialise a `tracing_subscriber` with a default `EnvFilter`
/// (RUST_LOG or `info`) so the example binary emits
/// structured log lines to stderr. The example is run
/// interactively, so we install the subscriber on every
/// invocation (re-init is harmless because we use
/// `try_init`, not `init`).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Ignore the "already initialised" error if a parent
    // test harness or workspace binary has already
    // installed a subscriber.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    let bot_token = match env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => {
            error!("TELEGRAM_BOT_TOKEN is required");
            return print_usage_and_exit();
        }
    };
    let chat_id: i64 = match env::var("TELEGRAM_DEST_CHAT")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        Some(id) => id,
        None => {
            error!("TELEGRAM_DEST_CHAT is required (must be a valid chat id)");
            return print_usage_and_exit();
        }
    };
    let text = env::var("TELEGRAM_TEXT")
        .unwrap_or_else(|_| "hello from octo-adapter-telegram-mtproto".to_string());
    let long_poll: u64 = env::var("TELEGRAM_LONG_POLL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
        .min(50);

    #[cfg(feature = "bot-api")]
    {
        if let Err(code) = run_http(bot_token, chat_id, text, long_poll).await {
            return code;
        }
        ExitCode::SUCCESS
    }
    #[cfg(not(feature = "bot-api"))]
    {
        let _ = (bot_token, chat_id, text, long_poll);
        warn!("this example requires the `bot-api` feature; rebuild with --features bot-api");
        ExitCode::from(2)
    }
}

#[cfg(feature = "bot-api")]
async fn run_http(
    bot_token: String,
    chat_id: i64,
    text: String,
    long_poll: u64,
) -> Result<(), ExitCode> {
    // Build a client with a 60 s timeout (covers a 50 s long-poll
    // window with 10 s of slack).
    let client = BotApiClient::with_config(
        BotApiConfig::new(&bot_token).with_user_agent("octo-adapter-telegram-mtproto/telegram_bot"),
    )
    .map_err(|e| {
        error!(error = ?e, "bot api client build failed");
        ExitCode::from(1)
    })?;
    // Smoke-test 1: getMe — verifies the token and prints the
    // bot's identity.
    let me = client.get_me().await.map_err(|e| {
        error!(error = ?e, "getMe failed");
        ExitCode::from(1)
    })?;
    info!(
        username = ?me.username,
        id = me.id,
        is_bot = me.is_bot,
        "bot api http: connected"
    );
    // Smoke-test 2: sendMessage — sends the user-supplied text
    // to the destination chat.
    let sent = client.send_message(chat_id, &text).await.map_err(|e| {
        error!(error = ?e, "sendMessage failed");
        ExitCode::from(1)
    })?;
    info!(
        message_id = sent.message_id,
        chat_id = sent.chat.id,
        text = ?sent.text,
        "sendMessage ok"
    );
    // Smoke-test 3: getUpdates — long-polls for up to
    // `long_poll` seconds. The server holds the response
    // open and only replies when there's a new update OR
    // the long-poll window expires.
    let updates = client.get_updates(None, long_poll).await.map_err(|e| {
        error!(error = ?e, "getUpdates failed");
        ExitCode::from(1)
    })?;
    info!(
        count = updates.len(),
        long_poll_secs = long_poll,
        "getUpdates returned"
    );
    for u in &updates {
        info!(update_id = u.update_id, text = ?u.text(), "update");
    }
    Ok(())
}
