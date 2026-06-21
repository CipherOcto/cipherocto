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

#[cfg(feature = "bot-api")]
use octo_adapter_telegram_mtproto::{BotApiClient, BotApiConfig};

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

#[tokio::main]
async fn main() -> ExitCode {
    let bot_token = match env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!("TELEGRAM_BOT_TOKEN is required");
            return print_usage_and_exit();
        }
    };
    let chat_id: i64 = match env::var("TELEGRAM_DEST_CHAT")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        Some(id) => id,
        None => {
            eprintln!("TELEGRAM_DEST_CHAT is required (must be a valid chat id)");
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
        eprintln!("this example requires the `bot-api` feature; rebuild with --features bot-api");
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
        eprintln!("bot api client build failed: {:?}", e);
        ExitCode::from(1)
    })?;
    // Smoke-test 1: getMe — verifies the token and prints the
    // bot's identity.
    let me = client.get_me().await.map_err(|e| {
        eprintln!("getMe failed: {:?}", e);
        ExitCode::from(1)
    })?;
    eprintln!(
        "bot api http: connected as @{:?} (id={}, is_bot={})",
        me.username, me.id, me.is_bot
    );
    // Smoke-test 2: sendMessage — sends the user-supplied text
    // to the destination chat.
    let sent = client.send_message(chat_id, &text).await.map_err(|e| {
        eprintln!("sendMessage failed: {:?}", e);
        ExitCode::from(1)
    })?;
    eprintln!(
        "sendMessage ok: message_id={} chat_id={} text={:?}",
        sent.message_id, sent.chat.id, sent.text
    );
    // Smoke-test 3: getUpdates — long-polls for up to
    // `long_poll` seconds. The server holds the response
    // open and only replies when there's a new update OR
    // the long-poll window expires.
    let updates = client.get_updates(None, long_poll).await.map_err(|e| {
        eprintln!("getUpdates failed: {:?}", e);
        ExitCode::from(1)
    })?;
    eprintln!(
        "getUpdates returned {} update(s) after up to {} s long-poll",
        updates.len(),
        long_poll
    );
    for u in &updates {
        eprintln!("  update_id={} text={:?}", u.update_id, u.text());
    }
    Ok(())
}
