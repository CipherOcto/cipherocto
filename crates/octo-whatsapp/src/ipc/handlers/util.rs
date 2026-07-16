//! Cross-handler utilities. Phase 6.12.4 introduced
//! [`rpc_for_bot_state`] so handlers can opt into the precise
//! `RpcErrorCode::SessionLost*` codes (added with the 7-variant
//! `BotStateMirror`) instead of the generic `NotConnected`.
//!
//! Existing handlers continue to return `NotConnected` for a
//! disconnected bot; this module is the migration path. New handlers
//! — or future patches to existing ones — should call
//! [`rpc_for_bot_state`] right after the bot-state check.

use crate::daemon::BotStateMirror;

use super::super::protocol::{RpcError, RpcErrorCode};

/// Translate a non-`Connected` `BotStateMirror` into the matching
/// `SessionLost*` RpcError (or fall back to `NotConnected` for the
/// "transient" non-Connected variants like `Disconnected`/`PairingQr`/
/// `PairingCode`).
///
/// Caller contract: only invoke this when the bot is genuinely
/// non-`Connected`. Passing `BotStateMirror::Connected` is a logic
/// bug and panics via `unreachable!()`.
///
/// Error code mapping:
/// - `LoggedOut`        → `SessionLostLoggedOut`  (-32000)
/// - `Replaced`         → `SessionLostReplaced`   (-32001)
/// - `SessionExpired`   → `SessionLostExpired`    (-31999)
/// - `Disconnected` | `PairingQr` | `PairingCode` | `AwaitingUserAction`
///   | `AwaitingPasskey` → `NotConnected` (-32012)
///
/// `AwaitingUserAction` is mapped to `NotConnected` because the bot
/// is in a non-actionable-from-the-CLI state: the operator must
/// complete a phone-side prompt (WebAuthn, 2FA PIN, etc.) before any
/// further RPCs can succeed. The status handler surfaces the
/// operator-facing hint; this helper just keeps the RPC contract
/// stable. `AwaitingPasskey` follows the same rule (the bot is mid-
/// SHORTCAKE_PASSKEY assertion — sends will fail until the phone
/// scans the QR / a registered authenticator completes).
pub fn rpc_for_bot_state(bs: BotStateMirror) -> RpcError {
    let code = match bs {
        BotStateMirror::Connected => {
            unreachable!("rpc_for_bot_state called with Connected; short-circuit first")
        }
        BotStateMirror::LoggedOut => RpcErrorCode::SessionLostLoggedOut,
        BotStateMirror::Replaced => RpcErrorCode::SessionLostReplaced,
        BotStateMirror::SessionExpired => RpcErrorCode::SessionLostExpired,
        BotStateMirror::Disconnected
        | BotStateMirror::PairingQr
        | BotStateMirror::PairingCode
        | BotStateMirror::AwaitingUserAction
        | BotStateMirror::AwaitingPasskey => RpcErrorCode::NotConnected,
    };
    RpcError {
        code: code.as_i32(),
        message: format!("bot_state={}", bot_state_label(bs)),
        data: None,
    }
}

/// Mirror of the label table in `ipc/handlers/status.rs::bot_state_label`.
/// Kept local to avoid leaking the `status` handler module's internals.
fn bot_state_label(bs: BotStateMirror) -> &'static str {
    use BotStateMirror::*;
    match bs {
        Disconnected => "Disconnected",
        PairingQr => "PairingQr",
        PairingCode => "PairingCode",
        Connected => "Connected",
        Replaced => "Replaced",
        LoggedOut => "LoggedOut",
        SessionExpired => "SessionExpired",
        AwaitingUserAction => "AwaitingUserAction",
        AwaitingPasskey => "AwaitingPasskey",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_for_bot_state_maps_terminal_variants_to_session_lost() {
        let r = rpc_for_bot_state(BotStateMirror::LoggedOut);
        assert_eq!(r.code, -32000);
        assert!(r.message.contains("LoggedOut"));

        let r = rpc_for_bot_state(BotStateMirror::Replaced);
        assert_eq!(r.code, -32001);

        let r = rpc_for_bot_state(BotStateMirror::SessionExpired);
        assert_eq!(r.code, -31999);
    }

    #[test]
    fn rpc_for_bot_state_maps_transient_variants_to_not_connected() {
        for bs in [
            BotStateMirror::Disconnected,
            BotStateMirror::PairingQr,
            BotStateMirror::PairingCode,
            BotStateMirror::AwaitingUserAction,
            BotStateMirror::AwaitingPasskey,
        ] {
            let r = rpc_for_bot_state(bs);
            assert_eq!(r.code, -32012, "expected NotConnected for {bs:?}");
        }
    }

    #[test]
    #[should_panic(expected = "rpc_for_bot_state called with Connected")]
    fn rpc_for_bot_state_panics_on_connected() {
        let _ = rpc_for_bot_state(BotStateMirror::Connected);
    }
}
