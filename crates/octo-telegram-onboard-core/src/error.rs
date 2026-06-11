//! Error type for `octo-telegram-onboard-core`.
//!
//! Exit-code table (matches `octo-matrix-onboard`):
//!
//! | Code | Meaning                                    |
//! |------|--------------------------------------------|
//! |  0   | Success                                    |
//! |  1   | Generic (catch-all)                        |
//! |  2   | Auth rejected (bad token, 2FA fail, etc.)  |
//! |  3   | Telegram unreachable / DNS / TLS           |
//! |  4   | User cancelled (timeout, Ctrl-C)           |
//! |  5   | Bad config (unwritable path, etc.)         |
//! |  6   | Rate-limited (TDLib flood-wait)            |

use std::process::ExitCode;

#[derive(Debug, thiserror::Error)]
pub enum OnboardError {
    #[error("{0}")]
    Generic(#[from] anyhow::Error),

    #[error("auth rejected")]
    AuthRejected(String),

    #[error("telegram unreachable")]
    TelegramUnreachable(String),

    #[error("cancelled")]
    Cancelled(String),

    #[error("bad config")]
    BadConfig(String),

    #[error("rate limited")]
    RateLimited(String),
}

impl OnboardError {
    /// Inner message for log enrichment. Display impl shows only the kind label.
    pub fn inner(&self) -> Option<&str> {
        match self {
            OnboardError::Generic(_) => None,
            OnboardError::AuthRejected(s)
            | OnboardError::TelegramUnreachable(s)
            | OnboardError::Cancelled(s)
            | OnboardError::BadConfig(s)
            | OnboardError::RateLimited(s) => Some(s.as_str()),
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            OnboardError::Generic(_) => 1,
            OnboardError::AuthRejected(_) => 2,
            OnboardError::TelegramUnreachable(_) => 3,
            OnboardError::Cancelled(_) => 4,
            OnboardError::BadConfig(_) => 5,
            OnboardError::RateLimited(_) => 6,
        }
    }

    pub fn as_exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit_code())
    }
}

pub type Result<T> = std::result::Result<T, OnboardError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_table() {
        assert_eq!(OnboardError::Generic(anyhow::anyhow!("x")).exit_code(), 1);
        assert_eq!(OnboardError::AuthRejected("x".into()).exit_code(), 2);
        assert_eq!(OnboardError::TelegramUnreachable("x".into()).exit_code(), 3);
        assert_eq!(OnboardError::Cancelled("x".into()).exit_code(), 4);
        assert_eq!(OnboardError::BadConfig("x".into()).exit_code(), 5);
        assert_eq!(OnboardError::RateLimited("x".into()).exit_code(), 6);
    }

    #[test]
    fn inner_returns_message_for_typed_variants() {
        let e = OnboardError::AuthRejected("bad token".into());
        assert_eq!(e.inner(), Some("bad token"));
    }

    #[test]
    fn inner_returns_none_for_generic() {
        let e = OnboardError::Generic(anyhow::anyhow!("x"));
        assert_eq!(e.inner(), None);
    }
}
