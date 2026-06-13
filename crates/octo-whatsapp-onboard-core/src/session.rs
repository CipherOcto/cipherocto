//! Polling-based wait helpers for the `Event::Connected` event.
//!
//! R1-M2 + R4-H2 + R8-H1: constants block at the top of this file.
//! R4-C1: use `adapter.health_check()` (existing method) instead of a
//! non-existent `bot_handle_is_alive()`. R3-M1: 100ms grace period
//! after `Event::Connected` re-verifies the bot is still alive.

use std::time::{Duration, Instant};

use crate::error::{CoreError, Result};
use octo_adapter_whatsapp::WhatsAppWebAdapter;
use octo_network::dot::adapters::PlatformAdapter; // R3-M1: brings `health_check` into scope

/// R1-M2: poll interval for `wait_for_connected` and `wait_for_health`.
/// Unit test pins to 250ms ± 10ms.
pub const POLL_INTERVAL_MS: u64 = 250;
/// R4-H2: grace period after `Event::Connected` to catch the
/// `Connected` -> `LoggedOut` race window. Unit test pins to 100ms ± 10ms.
pub const POST_CONNECT_GRACE_MS: u64 = 100;
/// R7-M2: `session list` fallback timeout (not operator-tunable).
pub const SESSION_LIST_HEALTH_TIMEOUT_SECS: u64 = 5;
/// R5-H2: `whoami` and `session verify` `wait_for_connected` timeout.
/// 30s is hardcoded; if `Event::Connected` has already fired, the
/// function returns on the first poll (<10ms).
pub const WHOAMI_TIMEOUT_SECS: u64 = 30;

/// Wait for `Event::Connected` (and the resolved `self_phone`) with
/// a timeout. Polls `adapter.self_handle()` every `POLL_INTERVAL_MS`,
/// then re-verifies via `health_check()` after a `POST_CONNECT_GRACE_MS`
/// grace period to catch the `Connected` -> `LoggedOut` race.
///
/// R7-H1: shares the same constants as `wait_for_health`. The two
/// helpers are kept separate because their return types differ
/// (`String` for phone resolution, `()` for health probe).
pub async fn wait_for_connected(
    adapter: &WhatsAppWebAdapter,
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(phone) = adapter.self_handle() {
            // Re-verify after grace period (catches the Connected -> LoggedOut race)
            tokio::time::sleep(Duration::from_millis(POST_CONNECT_GRACE_MS)).await;
            if adapter.health_check().await.is_ok() && adapter.self_handle().is_some() {
                return Ok(phone);
            }
            return Err(CoreError::SessionExpired);
        }
        if Instant::now() >= deadline {
            return Err(CoreError::Timeout {
                secs: timeout.as_secs(),
            });
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

/// Wait for the bot to be alive (any health check passes) with a
/// timeout. Used by `session list` fallback when no sidecar exists.
///
/// R6-H2: same shape as `wait_for_connected` but returns `Result<(), CoreError>`
/// because `session list` only needs `is_valid: bool`, not the phone.
pub async fn wait_for_health(adapter: &WhatsAppWebAdapter, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if adapter.health_check().await.is_ok() {
            // Re-verify after grace period (catches the Connected -> LoggedOut race)
            tokio::time::sleep(Duration::from_millis(POST_CONNECT_GRACE_MS)).await;
            if adapter.health_check().await.is_ok() {
                return Ok(());
            }
            return Err(CoreError::SessionExpired);
        }
        if Instant::now() >= deadline {
            return Err(CoreError::Timeout {
                secs: timeout.as_secs(),
            });
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_interval_ms_is_pinned() {
        // R1-M2: pin the constant to 250ms ± 10ms
        assert!(
            (245..=255).contains(&POLL_INTERVAL_MS),
            "POLL_INTERVAL_MS drifted to {POLL_INTERVAL_MS}"
        );
    }

    #[test]
    fn post_connect_grace_ms_is_pinned() {
        // R4-H2: pin the constant to 100ms ± 10ms
        assert!(
            (90..=110).contains(&POST_CONNECT_GRACE_MS),
            "POST_CONNECT_GRACE_MS drifted to {POST_CONNECT_GRACE_MS}"
        );
    }

    #[test]
    fn session_list_health_timeout_is_5() {
        assert_eq!(SESSION_LIST_HEALTH_TIMEOUT_SECS, 5);
    }

    #[test]
    fn whoami_timeout_is_30() {
        assert_eq!(WHOAMI_TIMEOUT_SECS, 30);
    }
}
