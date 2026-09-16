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
    use octo_wallet::ed25519_dalek::{Signature as DalekSignature, Verifier, VerifyingKey};

    let vk = VerifyingKey::from_bytes(holder_pubkey).map_err(|e| AttachError::BadSignature {
        reason: format!("invalid public key: {e}"),
    })?;
    let dalek_sig = DalekSignature::from_bytes(&sig.0);
    let msg = canonical_payload_bytes(session_id, payload, mint_timestamp_unix, ttl_unix);
    vk.verify(&msg, &dalek_sig)
        .map_err(|e| AttachError::BadSignature {
            reason: format!("ed25519 verify: {e}"),
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
}
