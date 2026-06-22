//! Adapter lifecycle state introspection.
//!
//! `octo-adapter-telegram-mtproto` exposes the current
//! `AdapterLifecycle` (e.g. `Init`, `Connecting`, `WaitCode`,
//! `WaitPassword`, `Ready`, `Failed`, `Stopped`) on the adapter.
//! The onboard CLI needs a stable string name for logs and for the
//! `OnboardError::NotReady { last_state }` field, so we centralize
//! the conversion here.

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};

/// Best-effort human-readable name of the adapter's current
/// lifecycle state.
///
/// We re-use the upstream enum's `Debug` impl rather than
/// hardcoding the variant set here so the two crates stay
/// loosely coupled (a future adapter version adding a new variant
/// will degrade gracefully, not panic).
///
/// Generic over the client impl so this works for both
/// production (`RealTelegramMtprotoClient`) and tests
/// (`MockTelegramMtprotoClient`).
pub fn auth_state_name<C: MtprotoTelegramClient>(adapter: &MtprotoTelegramAdapter<C>) -> String {
    format!("{:?}", adapter.lifecycle().state())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::mock_adapter_for_test;
    use tempfile::tempdir;

    // Smoke test: build a real mock adapter and confirm the
    // helper produces a non-empty state name. The lifecycle
    // starts at `Init` per `lifecycle.rs`, but we do not
    // hardcode that here — the test only asserts
    // non-emptiness so it survives future upstream changes.
    #[tokio::test(flavor = "current_thread")]
    async fn auth_state_name_does_not_panic_on_fresh_adapter() {
        let tmp = tempdir().expect("tempdir");
        let adapter = mock_adapter_for_test(tmp.path());
        let name = auth_state_name(&adapter);
        assert!(!name.is_empty(), "auth_state_name should not be empty");
    }
}
