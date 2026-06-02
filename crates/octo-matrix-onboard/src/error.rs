//! Error type for `octo-matrix-onboard`.
//!
//! Wraps `anyhow::Error` with an exit code so the binary can return
//! the right process status without each call site deciding on its
//! own. The exit-code table is in `docs/plans/2026-06-02-matrix-auth-
//! onboarding-design.md` §5:
//!
//! | Code | Meaning                                       |
//! |------|-----------------------------------------------|
//! |  0   | Success                                       |
//! |  1   | Generic (catch-all)                           |
//! |  2   | Auth rejected (wrong password, OAuth denied)  |
//! |  3   | Homeserver unreachable / DNS / TLS            |
//! |  4   | User cancelled (Ctrl-C, QR timeout, etc.)     |
//! |  5   | Bad config (output path unwritable, etc.)     |

use std::process::ExitCode;

#[derive(Debug, thiserror::Error)]
pub enum OnboardError {
    #[error("{0}")]
    Generic(#[from] anyhow::Error),

    #[error("auth rejected: {0}")]
    AuthRejected(String),

    #[error("homeserver unreachable: {0}")]
    Unreachable(String),

    #[error("cancelled: {0}")]
    Cancelled(String),

    #[error("bad config: {0}")]
    BadConfig(String),
}

impl OnboardError {
    pub fn exit_code(&self) -> u8 {
        match self {
            OnboardError::Generic(_) => 1,
            OnboardError::AuthRejected(_) => 2,
            OnboardError::Unreachable(_) => 3,
            OnboardError::Cancelled(_) => 4,
            OnboardError::BadConfig(_) => 5,
        }
    }

    pub fn as_exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit_code())
    }
}

impl From<std::io::Error> for OnboardError {
    fn from(e: std::io::Error) -> Self {
        OnboardError::Generic(e.into())
    }
}

pub type Result<T> = std::result::Result<T, OnboardError>;
