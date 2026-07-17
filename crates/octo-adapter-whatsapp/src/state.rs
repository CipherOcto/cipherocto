//! Bot lifecycle state machine (mission 0850p-a-replaced-state).
//!
//! `BotState` is the high-level state the bot is in. The state is
//! derived from the sequence of `Event`s emitted by `whatsapp-rust`.
//!
//! ## States
//!
//! - `Disconnected` — initial state; the bot has not yet been started
//!   or has shut down.
//! - `PairingQr` — the bot is showing a QR code to the operator for
//!   device pairing.
//! - `PairingCode` — the bot is showing a pair code for device pairing.
//! - `Connected` — the bot is paired and connected to the WhatsApp
//!   Web server.
//! - `Replaced` — the bot's session was replaced by another device
//!   (mission 0850p-a-replaced-state; previously collapsed into
//!   `LoggedOut`, which lost information — the operator needs to
//!   re-pair, not just reconnect).
//! - `LoggedOut` — the bot's session was logged out for an
//!   "intentional" reason (operator-initiated, expired, etc.).
//! - `SessionExpired` — the bot's session is invalid; a new pairing
//!   is required.
//!
//! ## Why a `Replaced` state?
//!
//! When the operator pairs a competing device (e.g., a new tablet)
//! from the same phone, the adapter receives `Event::LoggedOut` and
//! the CLI would exit 2 with "session logged out" — but the actual
//! cause is "replaced by another device". Distinguishing the two
//! allows recovery automation (re-pair) to react differently than
//! a true logout (operator must investigate). See D-WA-7.

use serde::{Deserialize, Serialize};

/// The high-level state of a paired bot. See module docs for the
/// state diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BotState {
    /// The bot has not yet been started or has shut down.
    #[default]
    Disconnected,
    /// Showing a QR code to the operator for pairing.
    PairingQr,
    /// Showing a pair code to the operator for pairing.
    PairingCode,
    /// Paired and connected to the WhatsApp Web server.
    Connected,
    /// Session replaced by another device (mission 0850p-a-replaced-state).
    Replaced,
    /// Session logged out (operator-initiated, expired, etc.).
    LoggedOut,
    /// Session expired; a new pairing is required.
    SessionExpired,
}

impl BotState {
    /// The reason code an exit handler should return for this state.
    /// Mission 0850p-a-replaced-state: `Replaced` is exit code 8.
    /// `SessionExpired` is 7 (existing). Other `LoggedOut` is 2.
    /// `Disconnected`/`PairingQr`/`PairingCode`/`Connected` are 0.
    pub fn exit_code(&self) -> u8 {
        match self {
            BotState::Disconnected
            | BotState::PairingQr
            | BotState::PairingCode
            | BotState::Connected => 0,
            BotState::LoggedOut => 2,
            BotState::SessionExpired => 7,
            // Mission 0850p-a-replaced-state: distinct from LoggedOut (2)
            BotState::Replaced => 8,
        }
    }

    /// True if this state indicates the bot is "alive" (i.e., paired
    /// and processing messages).
    pub fn is_alive(&self) -> bool {
        matches!(self, BotState::Connected)
    }
}

/// The cause of an `Event::LoggedOut` from the WhatsApp Web server.
/// whatsapp-rust exposes this as part of the event; we normalize it
/// into this enum for the state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoggedOutCause {
    /// Session was replaced by another device (mission
    /// 0850p-a-replaced-state: maps to `BotState::Replaced`).
    Replaced,
    /// Session was logged out (operator-initiated, expired, etc.;
    /// maps to `BotState::LoggedOut`).
    LoggedOut,
    /// Other / unknown cause; maps to `BotState::LoggedOut` for safety.
    Other,
}

impl LoggedOutCause {
    /// Translate a cause to the corresponding `BotState`.
    pub fn to_bot_state(&self) -> BotState {
        match self {
            LoggedOutCause::Replaced => BotState::Replaced,
            LoggedOutCause::LoggedOut | LoggedOutCause::Other => BotState::LoggedOut,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disconnected() {
        assert_eq!(BotState::default(), BotState::Disconnected);
    }

    #[test]
    fn replaced_exits_8() {
        // Mission 0850p-a-replaced-state: distinct exit code from
        // LoggedOut (2).
        assert_eq!(BotState::Replaced.exit_code(), 8);
        assert_ne!(
            BotState::Replaced.exit_code(),
            BotState::LoggedOut.exit_code()
        );
    }

    #[test]
    fn logged_out_exits_2() {
        assert_eq!(BotState::LoggedOut.exit_code(), 2);
    }

    #[test]
    fn session_expired_exits_7() {
        assert_eq!(BotState::SessionExpired.exit_code(), 7);
    }

    #[test]
    fn replaced_cause_maps_to_replaced_state() {
        assert_eq!(LoggedOutCause::Replaced.to_bot_state(), BotState::Replaced);
    }

    #[test]
    fn logged_out_cause_maps_to_logged_out_state() {
        assert_eq!(
            LoggedOutCause::LoggedOut.to_bot_state(),
            BotState::LoggedOut
        );
    }

    #[test]
    fn other_cause_maps_to_logged_out_for_safety() {
        // Unknown causes default to LoggedOut (not Replaced) because
        // a false-positive on Replaced would trigger an unnecessary
        // re-pair. Better to be safe.
        assert_eq!(LoggedOutCause::Other.to_bot_state(), BotState::LoggedOut);
    }

    #[test]
    fn only_connected_is_alive() {
        assert!(BotState::Connected.is_alive());
        for state in [
            BotState::Disconnected,
            BotState::PairingQr,
            BotState::PairingCode,
            BotState::Replaced,
            BotState::LoggedOut,
            BotState::SessionExpired,
        ] {
            assert!(!state.is_alive(), "{state:?} should not be alive");
        }
    }
}
