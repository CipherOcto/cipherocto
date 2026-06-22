//! `tracing`-based logging setup for the MTProto Telegram
//! onboard CLI.
//!
//! Mirrors the shape of the TDLib `octo-telegram-onboard`
//! crate's `logging` module so operators get a familiar
//! experience: `RUST_LOG`-controlled env-filter with a
//! sensible default of `info,octo_telegram_mtproto_onboard=debug`.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialise the global tracing subscriber. Idempotent — a
/// no-op on subsequent calls. Returns `true` if the subscriber
/// was installed by this call, `false` if one was already
/// present.
pub fn init(verbose: u8) -> bool {
    let default = match verbose {
        0 => "info,octo_telegram_mtproto_onboard=debug",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false)
        .with_file(false);
    // `try_init` returns Err if a subscriber is already set;
    // we treat that as a no-op success.
    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .try_init()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        // First call may or may not succeed depending on
        // whether a previous test in the same process set
        // a subscriber. Both outcomes are acceptable; the
        // important property is that the *second* call
        // does not panic.
        let _ = init(0);
        let _ = init(1);
    }
}
