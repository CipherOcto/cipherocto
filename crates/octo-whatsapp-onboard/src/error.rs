//! Error type for `octo-whatsapp-onboard`.
//!
//! Mirrors `octo-matrix-onboard/src/error.rs` (mission AC §Binary
//! structure). R3-C2: explicit `From<CoreError>` for stable 1-to-1
//! variant mapping.

use std::process::ExitCode;

use octo_whatsapp_onboard_core::CoreError;

/// R1-L2: `AuthRejected` and `RateLimited` are reserved for future
/// error states (e.g., when the adapter exposes more event-driven
/// failure modes). The mission AC requires all 7 variants to
/// support the 7 exit codes (0-7), so they're kept but unused.
///
/// Mission 0850p-a-symlink-check: `SymlinkAttack` exits 5 (bad config).
/// Mission 0850p-a-ws-url-release-guard: `WsUrlReleaseForbidden` exits 5.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum OnboardError {
    #[error("{0}")]
    Generic(#[from] anyhow::Error),

    #[error("unreachable")]
    Unreachable(String),

    #[error("cancelled")]
    Cancelled(String),

    #[error("bad config")]
    BadConfig(String),

    #[error("session expired")]
    SessionExpired(String),

    /// Mission 0850p-a-symlink-check: session_path is a symlink
    /// whose target is outside the user-requested parent directory.
    #[error("symlink attack: {0}")]
    SymlinkAttack(String),

    /// Mission 0850p-a-ws-url-release-guard: --ws-url is forbidden
    /// in release builds unless OCTO_WHATSAPP_ALLOW_WS_URL=1.
    #[error("--ws-url is forbidden in release builds unless OCTO_WHATSAPP_ALLOW_WS_URL=1 is set")]
    WsUrlReleaseForbidden,
}

impl OnboardError {
    /// R1-M6: the `Display` impl renders ONLY the short kind label.
    /// The inner message is reachable via the `inner()` accessor for
    /// log enrichment, but is NOT shown to the operator by default.
    pub fn inner(&self) -> Option<&str> {
        match self {
            OnboardError::Generic(_) => None,
            OnboardError::Unreachable(s)
            | OnboardError::Cancelled(s)
            | OnboardError::BadConfig(s)
            | OnboardError::SessionExpired(s)
            | OnboardError::SymlinkAttack(s) => Some(s.as_str()),
            OnboardError::WsUrlReleaseForbidden => Some("--ws-url forbidden in release"),
        }
    }

    /// Exit code (mission AC §Error Types).
    pub fn exit_code(&self) -> u8 {
        match self {
            OnboardError::Generic(_) => 1,
            OnboardError::Unreachable(_) => 3,
            OnboardError::Cancelled(_) => 4,
            OnboardError::BadConfig(_) => 5,
            OnboardError::SessionExpired(_) => 7,
            OnboardError::SymlinkAttack(_) => 5,
            OnboardError::WsUrlReleaseForbidden => 5,
        }
    }

    pub fn as_exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit_code())
    }
}

/// R3-C2: convert `CoreError` -> `OnboardError` via stable 1-to-1
/// mapping.
impl From<CoreError> for OnboardError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::Adapter(source) => OnboardError::Generic(source),
            CoreError::ClientBuild => OnboardError::Unreachable("client build failed".into()),
            CoreError::InvalidPhone { value, reason } => {
                OnboardError::BadConfig(format!("invalid phone {value:?}: {reason}"))
            }
            CoreError::InvalidSessionPath { path, reason } => {
                OnboardError::BadConfig(format!("invalid session_path {path:?}: {reason}"))
            }
            CoreError::Parse { path, source } => {
                OnboardError::BadConfig(format!("parse {path:?}: {source}"))
            }
            CoreError::Read { path, source } => {
                OnboardError::BadConfig(format!("read {path:?}: {source}"))
            }
            CoreError::SessionExpired => {
                OnboardError::SessionExpired("Event::LoggedOut after a successful link".into())
            }
            CoreError::Timeout { secs } => OnboardError::Cancelled(format!(
                "timed out after {secs}s waiting for Event::Connected"
            )),
            CoreError::SessionPathSymlink {
                requested,
                resolved,
            } => OnboardError::SymlinkAttack(format!(
                "{requested:?} is a symlink to {resolved:?} outside the requested parent"
            )),
            CoreError::InvalidBundle { path, reason } => {
                OnboardError::BadConfig(format!("invalid bundle {path:?}: {reason}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_mapping_is_stable() {
        assert_eq!(OnboardError::Generic(anyhow::anyhow!("x")).exit_code(), 1);
        assert_eq!(OnboardError::Unreachable("x".into()).exit_code(), 3);
        assert_eq!(OnboardError::Cancelled("x".into()).exit_code(), 4);
        assert_eq!(OnboardError::BadConfig("x".into()).exit_code(), 5);
        assert_eq!(OnboardError::SessionExpired("x".into()).exit_code(), 7);
    }

    #[test]
    fn from_core_error_timeout_to_cancelled() {
        let e: OnboardError = CoreError::Timeout { secs: 5 }.into();
        match e {
            OnboardError::Cancelled(msg) => assert!(msg.contains("5s")),
            other => panic!("expected OnboardError::Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn from_core_error_session_expired() {
        let e: OnboardError = CoreError::SessionExpired.into();
        match e {
            OnboardError::SessionExpired(_) => {}
            other => panic!("expected OnboardError::SessionExpired, got {other:?}"),
        }
    }

    #[test]
    fn from_core_error_invalid_phone_to_bad_config() {
        let e: OnboardError = CoreError::InvalidPhone {
            value: "5551234".into(),
            reason: "missing +".into(),
        }
        .into();
        match e {
            OnboardError::BadConfig(_) => {}
            other => panic!("expected OnboardError::BadConfig, got {other:?}"),
        }
    }

    #[test]
    fn inner_accessor_returns_message() {
        let e = OnboardError::Cancelled("details".into());
        assert_eq!(e.inner(), Some("details"));
    }
}
