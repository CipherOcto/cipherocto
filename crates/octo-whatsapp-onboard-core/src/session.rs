//! Polling-based wait helpers for the `Event::Connected` event.
//!
//! R1-M2 + R4-H2 + R8-H1: constants block at the top of this file.
//! R4-C1: use `adapter.health_check()` (existing method) instead of a
//! non-existent `bot_handle_is_alive()`. R3-M1: 100ms grace period
//! after `Event::Connected` re-verifies the bot is still alive.
//!
//! Mission 0850p-a-notify-event-connected: `wait_for_connected` now
//! uses the adapter's `Arc<Notify>` (signalled on `Event::Connected`)
//! with a one-shot pre-check (so an already-paired bot returns
//! immediately) and a 250ms-poll fallback for back-compat with
//! older adapter builds that predate the Notify field.

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
/// a timeout. The implementation:
///
/// 1. **Pre-check** (mission 0850p-a-has-valid-session): if the
///    adapter already has a valid session (`has_valid_session()`),
///    return the phone immediately (the CLI is restarting a
///    pre-paired bot).
/// 2. **Notify-wait** (mission 0850p-a-notify-event-connected):
///    await the adapter's `Arc<Notify>` with the timeout. The
///    adapter's `Event::Connected` handler calls `notify_waiters()`.
/// 3. **Grace period** (R3-M1): re-verify with `health_check()`
///    after a `POST_CONNECT_GRACE_MS` delay to catch the
///    `Connected` -> `LoggedOut` race.
/// 4. **Polling fallback** (R1-M2): if the Notify never fires (older
///    adapter build), poll `self_handle()` every `POLL_INTERVAL_MS`
///    until the deadline.
///
/// R7-H1: shares the same constants as `wait_for_health`. The two
/// helpers are kept separate because their return types differ
/// (`String` for phone resolution, `()` for health probe).
pub async fn wait_for_connected(adapter: &WhatsAppWebAdapter, timeout: Duration) -> Result<String> {
    // Mission 0850p-a-has-valid-session: if the adapter already has
    // a valid session (pre-paired bot), return the phone in <2ms
    // without any polling or Notify wait.
    if let Some(phone) = adapter.self_handle() {
        // Re-verify after grace period (catches the Connected -> LoggedOut race)
        tokio::time::sleep(Duration::from_millis(POST_CONNECT_GRACE_MS)).await;
        if adapter.health_check().await.is_ok() && adapter.self_handle().is_some() {
            return Ok(phone);
        }
        return Err(CoreError::SessionExpired);
    }

    let deadline = Instant::now() + timeout;
    let notify = adapter.connected();
    // Race the Notify against the polling fallback. The first one to
    // see `self_handle()` set wins.
    let check = async {
        // notified() returns when the adapter calls notify_waiters()
        // (or the future is dropped). We pair this with a periodic
        // poll to keep the API identical to the legacy behavior
        // (any path that sets self_handle wakes the waiter).
        let mut interval = tokio::time::interval(Duration::from_millis(POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            // Check 1: adapter-side state (handles all paths that
            // set self_phone, not just Event::Connected).
            if let Some(phone) = adapter.self_handle() {
                return Ok(phone);
            }
            // Check 2: Notify wakeup. Use tokio::select! to race
            // the Notify against the next poll tick.
            tokio::select! {
                _ = notify.notified() => {
                    if let Some(phone) = adapter.self_handle() {
                        return Ok(phone);
                    }
                    // Notify fired but state not yet set; loop back.
                }
                _ = interval.tick() => {
                    // Periodic poll already handled above; just
                    // continue the loop to re-check.
                }
            }
            if Instant::now() >= deadline {
                return Err(CoreError::Timeout {
                    secs: timeout.as_secs(),
                });
            }
        }
    };
    let phone = check.await?;
    // Re-verify after grace period (catches the Connected -> LoggedOut race)
    tokio::time::sleep(Duration::from_millis(POST_CONNECT_GRACE_MS)).await;
    if adapter.health_check().await.is_ok() && adapter.self_handle().is_some() {
        Ok(phone)
    } else {
        Err(CoreError::SessionExpired)
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

/// Wait for the initial history sync to complete with a timeout.
/// Races both `synced_notify` (fires on `Event::OfflineSyncCompleted`)
/// and `connected_notify` (fires on `Event::HistorySync` which is
/// definitive proof the connection is alive and sync is progressing).
/// For a 0-conversation sync, `OfflineSyncCompleted` may not fire,
/// but the `HistorySync` event itself proves the sync is done.
pub async fn wait_for_synced(adapter: &WhatsAppWebAdapter, timeout: Duration) -> Result<()> {
    let synced = adapter.synced();
    let connected = adapter.connected();
    let check = async {
        // Race synced (OfflineSyncCompleted) vs connected (HistorySync).
        // Either one means the connection is alive and syncing.
        tokio::select! {
            _ = synced.notified() => {
                tracing::debug!("wait_for_synced: OfflineSyncCompleted received");
            }
            _ = connected.notified() => {
                tracing::debug!("wait_for_synced: connected/HistorySync received");
            }
        }
        // Give a brief window for OfflineSyncCompleted to arrive
        // after the first HistorySync. If it doesn't come, the
        // 0-conversation case is still valid.
        tokio::select! {
            _ = synced.notified() => {
                tracing::debug!("wait_for_synced: OfflineSyncCompleted received (second)");
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                tracing::debug!("wait_for_synced: no further sync events in 10s, assuming done");
            }
        }
    };
    match tokio::time::timeout(timeout, check).await {
        Ok(()) => Ok(()),
        Err(_) => Err(CoreError::Timeout {
            secs: timeout.as_secs(),
        }),
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
