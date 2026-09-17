//! `octo-runtime::handle::signing` — sign / verify wrappers for the
//! `AttachHandle` token (RFC-0011-c §F.5).
//!
//! Wrappers compose substrate helpers on `octo_wallet::identity::IdentityKey`
//! per RFC-0015-a Appendix A (the existing operative signing surface).
//! The substrate follows the `verify_successor_proof` /
//! `verify_revocation_proof` static-helper pattern (pure helpers that
//! verify against arbitrary `pubkey: &[u8; 32]` rather than binding
//! to a `&self` receiver).
//!
//! No new Layer A types are introduced; the local `Signature` newtype
//! in `octo_runtime::handle` wraps `ed25519_dalek::Signature` at the
//! substrate boundary (Layer A primitive re-exported via the
//! `octo-wallet` path dep).

use uuid::Uuid;

use octo_wallet::IdentityKey;

use crate::handle::encoding::canonical_payload_bytes;
use crate::handle::error::AttachError;
#[cfg(feature = "octo-attach-key-rotation")]
use crate::handle::key_id::KeyId;
#[cfg(feature = "octo-attach-key-rotation")]
use crate::handle::KeySet;
use crate::handle::{AttachHandle, AttachPayload, SessionId, Signature, Transport};

/// Sign the canonical payload bytes for an `AttachHandle` token.
///
/// Composes `IdentityKey::sign` (Layer B substrate per RFC-0015-a
/// Appendix A) over the canonical signed bytes (single source of
/// truth per [`crate::handle::encoding::canonical_payload_bytes`]).
/// The returned `Signature` is the substrate-visible newtype — it
/// wraps `ed25519_dalek::Signature` at the boundary so Layer A
/// primitive types don't leak across the Layer A/B seam.
///
/// # Errors
/// Returns `AttachError::BadSignature` when the underlying HSM
/// adapter surfaces a `WalletError` (lifecycle gate, HSM transport,
/// or user rejection).
pub fn sign_attach_handle_payload(
    holder: &IdentityKey,
    session_id: SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
) -> Result<Signature, AttachError> {
    let msg = canonical_payload_bytes(&session_id, payload, mint_timestamp_unix, ttl_unix);
    let dalek_sig = holder.sign(&msg).map_err(|e| AttachError::BadSignature {
        reason: format!("IdentityKey::sign: {e}"),
    })?;
    Ok(Signature(dalek_sig.to_bytes()))
}

/// Verify the canonical payload bytes for an `AttachHandle` token
/// against an arbitrary public key (pure helper mirroring
/// `verify_revocation_proof` shape).
///
/// Consumed by `attach_with_token` per §F.2 validation chain step (a);
/// `decode_token` per §F.1 also invokes verify-on-decode.
///
/// # Errors
/// Returns `AttachError::BadSignature` on signature mismatch or
/// malformed public key bytes.
pub fn verify_attach_handle_payload(
    holder_pubkey: &[u8; 32],
    session_id: &SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
    sig: &Signature,
) -> Result<(), AttachError> {
    let msg = canonical_payload_bytes(session_id, payload, mint_timestamp_unix, ttl_unix);
    verify_bytes(holder_pubkey, &msg, sig)
}

/// Shared verify helper: verify `sig` over precomputed `msg` against
/// the pubkey bytes `pk`. Used by both the v1 single-pubkey verifier
/// (which computes `msg` inline via `canonical_payload_bytes`) and
/// the v2 multi-pubkey verifier (which uses the v2 canonical bytes
/// already augmented with the `key_id` suffix). Lives in the
/// non-feature-gated module path so the v1 baseline stays a single
/// canonical helper.
fn verify_bytes(pk: &[u8; 32], msg: &[u8], sig: &Signature) -> Result<(), AttachError> {
    use octo_wallet::ed25519_dalek::{Signature as DalekSignature, Verifier, VerifyingKey};

    let vk = VerifyingKey::from_bytes(pk).map_err(|e| AttachError::BadSignature {
        reason: format!("invalid public key: {e}"),
    })?;
    let dalek_sig = DalekSignature::from_bytes(&sig.0);
    vk.verify(msg, &dalek_sig)
        .map_err(|e| AttachError::BadSignature {
            reason: format!("ed25519 verify: {e}"),
        })
}

/// Additive v2 canonical payload bytes for an `AttachHandle` token
/// (RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge).
///
/// `v1_bytes || key_id_be` — the `key_id` suffix is the
/// discriminator that lets a verifier know which holder pubkey to
/// consult from the holder's `KeySet`. Single source of truth for
/// both `sign_attach_handle_payload_v2` +
/// `verify_attach_handle_payload_v2`. When the
/// `octo-attach-key-rotation` feature is OFF, this function is
/// hidden and `canonical_payload_bytes` (v1) is the only canonical
/// helper.
#[cfg(feature = "octo-attach-key-rotation")]
#[must_use]
pub fn canonical_payload_bytes_v2(
    session_id: &SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
    key_id: KeyId,
) -> Vec<u8> {
    let mut out = canonical_payload_bytes(session_id, payload, mint_timestamp_unix, ttl_unix);
    out.extend_from_slice(&key_id.to_be_bytes());
    out
}

/// Sign the canonical v2 payload bytes for an `AttachHandle` token
/// (RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge +
/// RFC-0015-a Appendix A `IdentityKey` substrate).
///
/// Composes `IdentityKey::sign` over `canonical_payload_bytes_v2`.
/// The `key_id` discriminator is bound into the signed bytes so a
/// verifier cannot silently swap the key_id claim.
///
/// # Errors
/// Returns `AttachError::BadSignature` when the underlying HSM
/// adapter surfaces a `WalletError` (lifecycle gate, HSM transport,
/// or user rejection).
#[cfg(feature = "octo-attach-key-rotation")]
pub fn sign_attach_handle_payload_v2(
    holder: &IdentityKey,
    session_id: SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
    key_id: KeyId,
) -> Result<Signature, AttachError> {
    let msg =
        canonical_payload_bytes_v2(&session_id, payload, mint_timestamp_unix, ttl_unix, key_id);
    let dalek_sig = holder.sign(&msg).map_err(|e| AttachError::BadSignature {
        reason: format!("IdentityKey::sign v2: {e}"),
    })?;
    Ok(Signature(dalek_sig.to_bytes()))
}

/// Verify the canonical v2 payload bytes for an `AttachHandle` token
/// against the holder's `KeySet` (RFC-0011-c §F.5.1 + RFC-0015-a §6.5
/// paired-acceptance bridge).
///
/// Lookup order:
/// 1. Active `KeySet::lookup(key_id)` — return Ok or BadSignature.
/// 2. Otherwise try `KeySet::grace_key(key_id)` (the grace fallback
///    only consults the grace entry matching the CLAIMED `key_id`,
///    not every grace key — the `key_id` discriminator binds the
///    signature to a specific key, so a token claiming key_id X
///    must verify against the rotated-out pubkey registered under
///    key_id X, never against an arbitrary grace key).
/// 3. Otherwise return `AttachError::UnknownKeyId { key_id,
///    known_keys }`.
///
/// The grace fallback accepts the standard rotation case: the
/// holder rotates key_id X from pubkey P1 to P2 (so P1 moves into
/// the grace window). A token signed under P1 with the key_id X
/// suffix verifies via step 2 — the same key_id, the rotated-out
/// pubkey. Active lookup returns the new P2 (verify fails), then
/// grace_key(X) returns P1 (verify succeeds).
///
/// # Errors
/// Returns `AttachError::BadSignature` on signature mismatch
/// against the matched pubkey, or `AttachError::UnknownKeyId`
/// when neither the active nor the grace set contains a key
/// matching `key_id`.
#[cfg(feature = "octo-attach-key-rotation")]
pub fn verify_attach_handle_payload_v2(
    key_set: &KeySet,
    session_id: &SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
    key_id: KeyId,
    sig: &Signature,
) -> Result<(), AttachError> {
    let msg =
        canonical_payload_bytes_v2(session_id, payload, mint_timestamp_unix, ttl_unix, key_id);

    // 1. Try the active lookup.
    if let Some(pk) = key_set.lookup(key_id) {
        return verify_bytes(pk, &msg, sig);
    }

    // 2. Try the grace entry for the CLAIMED key_id only. Iterating
    //    every grace pubkey against a msg keyed by `key_id` X would
    //    let any holder of a grace key's private material forge
    //    tokens claiming arbitrary key_ids — a forgery vulnerability.
    //    Standard rotation keeps the same key_id across pubkey
    //    changes, so the rotated-out pubkey is recovered by looking
    //    up the claimed key_id in the grace map.
    if let Some(pk) = key_set.grace_key(key_id) {
        return verify_bytes(pk, &msg, sig);
    }

    // 3. Exhausted.
    Err(AttachError::UnknownKeyId {
        key_id,
        known_keys: key_set.known_key_ids(),
    })
}

/// Mint a fully-signed `AttachHandle` token.
///
/// Convenience entry point used by the CLI's `octo agent run --detach`
/// path: composes `sign_attach_handle_payload` over a fresh
/// `AttachPayload` populated with the operator-supplied `agent_id`,
/// `since_cursor`, TTL, and transport.
///
/// # Errors
/// Returns `AttachError::BadSignature` when the underlying signing
/// surface rejects (HSM transport, lifecycle gate, user rejection).
pub fn mint_attach_handle(
    holder: &IdentityKey,
    agent_id: Uuid,
    session_id: SessionId,
    since_cursor: u64,
    ttl_unix: u64,
    transport: Transport,
) -> Result<AttachHandle, AttachError> {
    let payload = AttachPayload {
        agent_id,
        since_cursor,
    };
    // The mint timestamp is the substrate-visible wall-clock at
    // mint time; `SystemTime::now()` is the canonical substrate
    // clock per RFC-0009 §Time Authority. CLI callers cannot
    // back-date the token.
    let mint_timestamp_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let signature =
        sign_attach_handle_payload(holder, session_id, &payload, mint_timestamp_unix, ttl_unix)?;
    Ok(AttachHandle {
        session_id,
        mint_timestamp_unix,
        ttl_unix,
        signature,
        payload,
        transport,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_wallet::IdentityKey;

    fn activated_holder() -> IdentityKey {
        let mut k = IdentityKey::generate().expect("generate");
        k.activate(1_700_000_000).expect("activate");
        k
    }

    #[test]
    fn sign_verify_happy_path() {
        let holder = activated_holder();
        let pk = holder.public_key_bytes();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 42,
        };
        let sig =
            sign_attach_handle_payload(&holder, session_id, &payload, 1_000, 2_000).expect("sign");
        verify_attach_handle_payload(&pk, &session_id, &payload, 1_000, 2_000, &sig)
            .expect("verify");
    }

    #[test]
    fn verify_rejects_tampered_signature() {
        let holder = activated_holder();
        let pk = holder.public_key_bytes();
        let session_id = [0x11; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0x22; 16]),
            since_cursor: 0,
        };
        let mut sig =
            sign_attach_handle_payload(&holder, session_id, &payload, 1_000, 2_000).expect("sign");
        sig.0[0] ^= 0x01;
        let result = verify_attach_handle_payload(&pk, &session_id, &payload, 1_000, 2_000, &sig);
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "tampered sig must reject, got {result:?}"
        );
    }

    #[test]
    fn verify_rejects_wrong_pubkey() {
        let holder = activated_holder();
        let session_id = [0x33; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0x44; 16]),
            since_cursor: 0,
        };
        let sig =
            sign_attach_handle_payload(&holder, session_id, &payload, 1_000, 2_000).expect("sign");
        let wrong_pk = [0xee; 32];
        let result =
            verify_attach_handle_payload(&wrong_pk, &session_id, &payload, 1_000, 2_000, &sig);
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "wrong pubkey must reject, got {result:?}"
        );
    }

    #[test]
    fn verify_rejects_tampered_mint_timestamp() {
        let holder = activated_holder();
        let pk = holder.public_key_bytes();
        let session_id = [0x55; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0x66; 16]),
            since_cursor: 0,
        };
        let sig =
            sign_attach_handle_payload(&holder, session_id, &payload, 1_000, 2_000).expect("sign");
        let result = verify_attach_handle_payload(&pk, &session_id, &payload, 9_999, 2_000, &sig);
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "tampered mint must reject, got {result:?}"
        );
    }

    #[test]
    fn mint_attach_handle_happy_path() {
        let holder = activated_holder();
        let pk = holder.public_key_bytes();
        let session_id = [0x77; 32];
        let agent_id = Uuid::from_bytes([0x88; 16]);
        let token = mint_attach_handle(
            &holder,
            agent_id,
            session_id,
            5,
            1_700_003_600,
            Transport::IN_PROCESS,
        )
        .expect("mint");
        // Token is signed; verify using the static helper.
        verify_attach_handle_payload(
            &pk,
            &token.session_id,
            &token.payload,
            token.mint_timestamp_unix,
            token.ttl_unix,
            &token.signature,
        )
        .expect("verify minted");
    }

    #[test]
    fn mint_attach_handle_rejects_inactive_lifecycle() {
        // IdentityKey::generate yields a Designated (not Active) key.
        // sign() must fail at the lifecycle gate per RFC-0009
        // §Lifecycle Requirements.
        let holder = IdentityKey::generate().expect("generate");
        let session_id = [0x99; 32];
        let agent_id = Uuid::from_bytes([0xaa; 16]);
        let result = mint_attach_handle(
            &holder,
            agent_id,
            session_id,
            0,
            1_700_003_600,
            Transport::IN_PROCESS,
        );
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "inactive holder must reject, got {result:?}"
        );
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn sign_v2_includes_key_id_in_canonical_bytes() {
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 42,
        };
        let v1 = canonical_payload_bytes(&session_id, &payload, 1_000, 2_000);
        let v2 = canonical_payload_bytes_v2(&session_id, &payload, 1_000, 2_000, 7);
        // v2 = v1 ++ key_id_be(7) == 4 trailing zero bytes plus the high 3 zero bytes.
        assert_eq!(v2.len(), v1.len() + 4, "v2 must append exactly 4 bytes");
        assert_eq!(&v2[..v1.len()], &v1[..], "v2 prefix must equal v1 bytes");
        assert_eq!(
            &v2[v1.len()..],
            &[0, 0, 0, 7],
            "key_id=7 trailing bytes must be big-endian"
        );
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn sign_v2_happy_path() {
        let holder = activated_holder();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 42,
        };
        let mut key_set = KeySet::new();
        key_set.insert(7, holder.public_key_bytes());
        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000, 7)
            .expect("sign v2");
        verify_attach_handle_payload_v2(&key_set, &session_id, &payload, 1_000, 2_000, 7, &sig)
            .expect("verify v2 active lookup");
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn verify_v2_grace_period_accepts_rotated_key() {
        let holder = activated_holder();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 42,
        };
        let mut key_set = KeySet::new();
        key_set.insert(1, holder.public_key_bytes());
        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000, 1)
            .expect("sign v2");
        // Rotate key_id=1 out of the active set into the grace window.
        key_set.move_to_grace(1);
        assert_eq!(key_set.lookup(1), None);
        // Verify still succeeds via the grace fallback.
        verify_attach_handle_payload_v2(&key_set, &session_id, &payload, 1_000, 2_000, 1, &sig)
            .expect("verify v2 grace fallback");
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn verify_v2_unknown_key_id_returns_error() {
        let holder = activated_holder();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 0,
        };
        let mut key_set = KeySet::new();
        key_set.insert(1, holder.public_key_bytes());
        key_set.insert(2, [0xee; 32]);
        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000, 99)
            .expect("sign v2");
        let result = verify_attach_handle_payload_v2(
            &key_set,
            &session_id,
            &payload,
            1_000,
            2_000,
            99,
            &sig,
        );
        match result {
            Err(AttachError::UnknownKeyId { key_id, known_keys }) => {
                assert_eq!(key_id, 99);
                assert!(known_keys.contains(&1));
                assert!(known_keys.contains(&2));
            }
            other => panic!("expected UnknownKeyId, got {other:?}"),
        }
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn verify_v2_active_lookup_mismatch_returns_bad_signature() {
        // Two keys in active: key_a (id=1, sign here) and key_b (id=2, mismatch).
        // Verify with key_id=2 must hit active lookup of key_b's pubkey
        // and reject (sig was signed by key_a over key_id=1 bytes — msg
        // differs from the key_id=2 bytes).
        let holder_a = activated_holder();
        let holder_b = activated_holder();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 0,
        };
        let mut key_set = KeySet::new();
        key_set.insert(1, holder_a.public_key_bytes());
        key_set.insert(2, holder_b.public_key_bytes());
        let sig = sign_attach_handle_payload_v2(&holder_a, session_id, &payload, 1_000, 2_000, 1)
            .expect("sign v2 by holder_a with key_id=1");
        let result =
            verify_attach_handle_payload_v2(&key_set, &session_id, &payload, 1_000, 2_000, 2, &sig);
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "active lookup of key_id=2 must fail BadSignature, got {result:?}"
        );
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn verify_v2_grace_key_does_not_vouch_for_arbitrary_key_id() {
        // Regression: a grace key must NOT vouch for a token that
        // claims a DIFFERENT key_id. Holder rotates key_id=1 to
        // grace (so key_set has active={2}, grace={1}). Attacker
        // (with holder's old key_id=1 private material) signs a
        // token claiming key_id=99; verifier must reject via
        // UnknownKeyId, NOT accept via grace fallback.
        //
        // Pre-fix behavior (R1 CRIT): grace_period() iteration
        // tried every grace pubkey against the msg, so any grace
        // key holder could forge tokens claiming any key_id.
        let holder = activated_holder();
        let other_holder = activated_holder();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 0,
        };
        let mut key_set = KeySet::new();
        key_set.insert(2, other_holder.public_key_bytes());
        key_set.insert(1, holder.public_key_bytes());
        key_set.move_to_grace(1);
        // Attacker signs msg with key_id=99 suffix using holder's
        // (now grace) key_id=1 private key.
        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000, 99)
            .expect("sign v2");
        let result = verify_attach_handle_payload_v2(
            &key_set,
            &session_id,
            &payload,
            1_000,
            2_000,
            99,
            &sig,
        );
        match result {
            Err(AttachError::UnknownKeyId { key_id, .. }) => assert_eq!(key_id, 99),
            other => panic!("grace key must NOT vouch for claimed key_id=99, got {other:?}"),
        }
    }
}
