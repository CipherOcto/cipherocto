// RFC-0969 §Phase 2: dual-issuance mint.
//
// `mint_dual` atomically issues both bearer + capability via
// `txn.insert_dual`. This is the load-bearing algorithm for RFC-0969
// §Adversary A9 (race between bearer and capability mint).
//
// ## What is actually still blocking this (corrected 2026-10-12)
//
// This header previously listed **three** deferrals. Measured, **two of them
// were resolved by other work and the rationale went stale** — nobody
// re-evaluated whether the stub is still blocked, because the note read as
// current:
//
// 1. ~~"RFC-0957-A1 4-arg persistence-free mint (DEFERRED — current
//    `CapabilityToken::mint` is 5-arg with catalog parameter)."~~
//    **Resolved.** `CapabilityToken::mint` now takes exactly the 4 RFC-0957-A1
//    arguments — `(root_secret, holder, holder_did, initial_caveats)` — with no
//    catalog parameter.
// 2. "BearerCapsule mint with X25519 + ChaCha20-Poly1305 (deferred)."
//    **Still true.** `BearerCapsule` is a structural stub with the correct
//    3-field wire shape and a `new()` constructor, but no cryptography:
//    `encrypted_capsule` is arbitrary bytes and `seller_signature` is an
//    unverified 64-byte array. Nothing in the workspace verifies that
//    signature, and nothing constructs a `BearerCapsule` outside tests. The
//    real type is owned by mission 0959-a1.
// 3. ~~"Live `Transaction::insert_dual` impl (deferred stub in 0957-c)."~~
//    **Resolved.** `HolderRegistry::insert_dual` is implemented over a real
//    Stoolap transaction, with drop-triggered rollback and a defence-in-depth
//    commit path.
//
// So the remaining blocker is a single one — the `BearerCapsule` type has no
// cryptographic semantics yet.
//
// This stub ships the types + entry point + `MintError` so downstream consumers
// can wire references. The full algorithm body is a follow-up.
//
// ## Why this matters beyond the stub
//
// `mint_dual` is where a capability would receive its `Caveat::AskBinding` —
// the RFC's `mint_dual(buyer_did, buyer_holder_pub, ask_id, …)` signature takes
// the ask id for exactly that reason. That makes this stub the single origin of
// three separate gaps recorded elsewhere:
//
// - RFC-0969 TV9 (Dual-Issuance Atomicity) has no test, and cannot have one.
// - No production mint attaches an ask binding, so every producible capability
//   reports `Ok(None)` from `CapabilityToken::ask_binding()`.
// - The gateway's `CapabilityVerifier` has nothing ask-bound to verify.
//
// See mission `0969-c`.

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
/// Still a stub, blocked on **one** precondition — `BearerCapsule` has no
/// cryptographic semantics. The other two deferrals this function was written
/// against were resolved by other work; see the module header for which.
///
/// Returns a structured error so callers can wire references.
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
        let s = format!("{e:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("4242"), "leaked ask_id bytes: {s}");
    }
}
