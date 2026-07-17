//! Adapter lifecycle state introspection.
//!
//! `octo-adapter-telegram-mtproto` exposes the current
//! `AdapterLifecycle` (e.g. `Init`, `Connecting`, `WaitCode`,
//! `WaitPassword`, `Ready`, `Failed`, `Stopped`) on the adapter.
//! The onboard CLI needs a stable string name for logs and for the
//! `OnboardError::Lifecycle { state }` field, so we centralize
//! the conversion here.

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};

/// Best-effort human-readable name of the adapter's current
/// lifecycle state.
///
/// R26-ARCH-2: the prior implementation used
/// `format!("{:?}", adapter.lifecycle().state())` which
/// returned the Debug form (e.g. `Uninitialised`). The
/// adapter now exposes `AdapterLifecycle::state_name()`
/// which returns a kebab-cased `&'static str` (e.g.
/// `uninitialised`, `shutting-down`) that is more
/// operator-friendly in error messages.
///
/// Generic over the client impl so this works for both
/// production (`RealTelegramMtprotoClient`) and tests
/// (`MockTelegramMtprotoClient`).
pub fn auth_state_name<C: MtprotoTelegramClient>(adapter: &MtprotoTelegramAdapter<C>) -> String {
    // The match in `state_name()` is exhaustive on the
    // current `AdapterLifecycle` variants; if a future
    // variant is added the adapter's maintainers will
    // update the match there. The onboard crate's
    // `auth_state_name` is a thin wrapper so it doesn't
    // need its own enum copy.
    adapter.lifecycle().state().state_name().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::mock_adapter_for_test;
    use octo_adapter_telegram_mtproto::AdapterLifecycle;
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

    /// R26-ARCH-2: `state_name()` returns kebab-cased
    /// names so the CLI can render them in error
    /// messages. Pin a few of the labels here so
    /// `OnboardError::Lifecycle { state }` is stable
    /// across releases.
    #[test]
    fn state_name_is_kebab_case() {
        assert_eq!(
            AdapterLifecycle::Uninitialised.state_name(),
            "uninitialised"
        );
        assert_eq!(AdapterLifecycle::Connecting.state_name(), "connecting");
        assert_eq!(AdapterLifecycle::Connected.state_name(), "connected");
        assert_eq!(
            AdapterLifecycle::Authenticating.state_name(),
            "authenticating"
        );
        assert_eq!(AdapterLifecycle::Ready.state_name(), "ready");
        assert_eq!(AdapterLifecycle::ShuttingDown.state_name(), "shutting-down");
        assert_eq!(AdapterLifecycle::Stopped.state_name(), "stopped");
        assert_eq!(AdapterLifecycle::Failed.state_name(), "failed");
    }
}
