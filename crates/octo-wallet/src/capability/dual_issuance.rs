// RFC-0969 §Phase 2: dual-issuance mint.
//
// `mint_dual` atomically issues both bearer + capability via
// `txn.insert_dual`. This is the load-bearing algorithm for RFC-0969
// §Adversary A9 (race between bearer and capability mint).
//
// Mission 0969-b's full implementation requires:
// 1. RFC-0957-A1 4-arg persistence-free mint signature (DEFERRED — current
//    `CapabilityToken::mint` is 5-arg with catalog parameter).
// 2. BearerCapsule mint with X25519 + ChaCha20-Poly1305 (deferred to 0959-b
//    algorithm body).
// 3. Live `Transaction::insert_dual` impl (deferred stub in 0957-c; full impl
//    sticks to atomicity guarantee).
//
// This stub ships the types + entry point + MintError so 0969-b downstream
// consumers can wire references. The full algorithm body is a follow-up.

use thiserror::Error;

use super::bearer_capsule_re_export::BearerCapsule;
use super::CapabilityToken;

/// MintError (RFC-0969 §Phase 2).
#[derive(Error)]
pub enum MintError {
    #[error("ask expired: ask_id=<redacted 32 bytes>, expired_at_unix={expired_at_unix}")]
    AskExpired {
        ask_id: [u8; 32],
        expired_at_unix: u64,
    },
    #[error("root secret missing: ask_id=<redacted 32 bytes>")]
    RootSecretMissing { ask_id: [u8; 32] },
    #[error("holder key invalid: {reason}")]
    HolderKeyInvalid { reason: String },
    #[error("dual insert failed: ask_id=<redacted 32 bytes>")]
    DualInsertFailed {
        ask_id: [u8; 32],
        bearer_err: Option<String>,
        cap_err: Option<String>,
    },
}

impl std::fmt::Debug for MintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AskExpired {
                ask_id: _,
                expired_at_unix,
            } => f
                .debug_struct("AskExpired")
                .field("ask_id", &"<redacted 32 bytes>")
                .field("expired_at_unix", expired_at_unix)
                .finish(),
            Self::RootSecretMissing { .. } => f
                .debug_struct("RootSecretMissing")
                .field("ask_id", &"<redacted 32 bytes>")
                .finish(),
            Self::HolderKeyInvalid { reason } => f
                .debug_struct("HolderKeyInvalid")
                .field("reason", reason)
                .finish(),
            Self::DualInsertFailed {
                bearer_err,
                cap_err,
                ..
            } => f
                .debug_struct("DualInsertFailed")
                .field("ask_id", &"<redacted 32 bytes>")
                .field("bearer_err", bearer_err)
                .field("cap_err", cap_err)
                .finish(),
        }
    }
}

/// `mint_dual` entry point (RFC-0969 §Phase 2).
///
/// Full impl wired to:
/// 1. RFC-0957-A1 4-arg persistence-free mint (deferred — see 0969-b notes).
/// 2. `Transaction::insert_dual(bearer, cap)` atomic (deferred stub in 0957-c).
///
/// For now this returns a structured error so callers can wire references.
pub fn mint_dual(
    ask_id: [u8; 32],
    _ask_ttl_unix: u64,
) -> Result<(BearerCapsule, CapabilityToken), MintError> {
    // Full impl follows RFC-0969 §Algorithms:mint_dual body — see
    // mission 0969-b Notes for the deferral rationale.
    Err(MintError::RootSecretMissing { ask_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_dual_returns_root_secret_missing_stub() {
        let r = mint_dual([0x33; 32], 1_700_000_000_000);
        assert!(matches!(r, Err(MintError::RootSecretMissing { .. })));
    }

    #[test]
    fn mint_error_variants_present() {
        let _ = MintError::AskExpired {
            ask_id: [0x33; 32],
            expired_at_unix: 1_700_000_000_000,
        };
        let _ = MintError::HolderKeyInvalid {
            reason: "test".into(),
        };
        let _ = MintError::DualInsertFailed {
            ask_id: [0x33; 32],
            bearer_err: Some("test".into()),
            cap_err: None,
        };
    }

    #[test]
    fn mint_error_debug_redacts_ask_id() {
        let e = MintError::AskExpired {
            ask_id: [0x42; 32],
            expired_at_unix: 1_700_000_000_000,
        };
        let s = format!("{:?}", e);
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("4242"), "leaked ask_id bytes: {s}");
    }
}
