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
use crate::handle::KeySet;
use crate::handle::{AttachHandle, AttachPayload, SessionId, Signature, Transport};

/// The canonical bytes an `AttachHandle` signature actually covers:
/// the RFC-0009 prefix ‖ DCS(canonical payload bytes).
///
/// Single source of truth for both sign and verify. The prefix is carried on
/// the token as [`AttachHandle::signable_prefix`], so the verifier rebuilds
/// these bytes from the token rather than from its own configuration — which
/// is what lets the algorithm come from the message instead of from a
/// hardcoded preference.
fn signable_bytes(
    prefix: &octo_wallet::SignablePrefix,
    canonical: &[u8],
) -> Result<Vec<u8>, AttachError> {
    octo_wallet::SignedMessage::new(*prefix, canonical.to_vec())
        .signable(octo_wallet::SignableForm::Raw)
        .map_err(|e| AttachError::BadSignature {
            reason: format!("prefix construction: {e}"),
        })
}

/// Parse the prefix carried on a token, mapping the failure into
/// `AttachError` so callers on the decode path stay in one error space.
fn prefix_from_token(bytes: &[u8; 32]) -> Result<octo_wallet::SignablePrefix, AttachError> {
    octo_wallet::SignablePrefix::from_bytes(bytes).map_err(|e| AttachError::BadSignature {
        reason: format!("signable prefix: {e}"),
    })
}

/// Sign the canonical payload bytes for an `AttachHandle` token.
///
/// Composes `IdentityKey::sign` (Layer B substrate per RFC-0015-a
/// Appendix A) over the RFC-0009 signable bytes — the prefix ‖ DCS of the
/// canonical payload — so the signature commits to the holder's algorithm and
/// verification method, not just to the session fields.
///
/// # Errors
/// Returns `AttachError::BadSignature` when the holder carries no
/// `key_id` (nothing may be signed without a key context), or when the
/// underlying HSM adapter surfaces a `WalletError` (lifecycle gate, HSM
/// transport, or user rejection).
pub fn sign_attach_handle_payload(
    holder: &IdentityKey,
    session_id: SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
) -> Result<Signature, AttachError> {
    let prefix = holder
        .signable_prefix()
        .map_err(|e| AttachError::BadSignature {
            reason: format!("IdentityKey signable prefix: {e}"),
        })?;
    let canonical = canonical_payload_bytes(&session_id, payload, mint_timestamp_unix, ttl_unix);
    let msg = signable_bytes(&prefix, &canonical)?;
    let sig_bytes = holder.sign(&msg).map_err(|e| AttachError::BadSignature {
        reason: format!("IdentityKey::sign: {e}"),
    })?;
    Ok(Signature(sig_bytes))
}

/// Verify the canonical payload bytes for an `AttachHandle` token
/// against an arbitrary public key (pure helper mirroring
/// `verify_revocation_proof` shape).
///
/// `signable_prefix` is the 32-byte prefix carried on the token. It is the
/// source of truth for the algorithm: verification dispatches on the one the
/// prefix names, never on a hardcoded preference. A verifier that expected
/// Ed25519 and is handed a token naming something else fails closed rather
/// than misreading the message.
///
/// Consumed by `attach_with_token` per §F.2 validation chain step (a);
/// `decode_token` per §F.1 also invokes verify-on-decode.
///
/// # Errors
/// Returns `AttachError::BadSignature` on a malformed prefix, signature
/// mismatch, or malformed public key bytes.
pub fn verify_attach_handle_payload(
    holder_pubkey: &[u8],
    signable_prefix: &[u8; 32],
    session_id: &SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
    sig: &Signature,
) -> Result<(), AttachError> {
    let prefix = prefix_from_token(signable_prefix)?;
    let canonical = canonical_payload_bytes(session_id, payload, mint_timestamp_unix, ttl_unix);
    let msg = signable_bytes(&prefix, &canonical)?;
    verify_bytes(&prefix.algorithm_id().to_owned(), holder_pubkey, &msg, sig)
}

/// Shared verify helper: verify `sig` over precomputed `msg` against
/// the pubkey bytes `pk`. Used by both the v1 single-pubkey verifier
/// (which computes `msg` inline via `canonical_payload_bytes`) and
/// the v2 multi-pubkey verifier (which uses the v2 canonical bytes
/// already augmented with the `key_id` suffix). Lives in the
/// non-feature-gated module path so the v1 baseline stays a single
/// canonical helper.
fn verify_bytes(
    algorithm_id: &octo_wallet::AlgorithmId,
    pk: &[u8],
    msg: &[u8],
    sig: &Signature,
) -> Result<(), AttachError> {
    octo_wallet::verify_dispatch(algorithm_id, pk, msg, &sig.0).map_err(|e| {
        AttachError::BadSignature {
            reason: format!("dispatched verify: {e}"),
        }
    })
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
/// Sign an `AttachHandle` token to be verified against the holder's `KeySet`
/// (RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge).
///
/// The bytes signed are exactly the ones v1 signs — the RFC-0009 prefix ‖ DCS
/// of the canonical payload. There is **no `key_id` parameter and no
/// `key_id` suffix**: the prefix already carries the verification method id,
/// and `0009-e4` put it on the token as a field so a verifier can read it.
/// The suffix was a second, parallel binding of the same fact whose only real
/// effect was forcing the verifier to *supply* the key id instead of reading
/// it.
///
/// # Errors
/// Returns `AttachError::BadSignature` when the holder carries no key
/// context, or when the underlying HSM adapter surfaces a `WalletError`.
#[cfg(feature = "octo-attach-key-rotation")]
pub fn sign_attach_handle_payload_v2(
    holder: &IdentityKey,
    session_id: SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
) -> Result<Signature, AttachError> {
    sign_attach_handle_payload(holder, session_id, payload, mint_timestamp_unix, ttl_unix)
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
/// Verify an `AttachHandle` token against the holder's `KeySet`
/// (RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge).
///
/// The key comes from **the token's own prefix**, not from a caller argument.
/// That is the whole difference from the pre-`0009-e4` shape: a verifier
/// holding only a token used to have to be handed the key id, which meant the
/// binding it enforced was one it chose itself. Now the token names its key and
/// the verifier reads it.
///
/// Lookup order:
/// 1. Active `KeySet::lookup(key_id)` from the prefix.
/// 2. Otherwise `KeySet::grace_key(key_id)`. Only the grace entry matching the
///    token's own `key_id` — never an arbitrary grace key. Iterating every
///    grace pubkey against a token naming `key_id` X would let any holder of
///    *any* grace key's private material forge tokens claiming X.
/// 3. Otherwise `AttachError::UnknownKeyId`.
///
/// The grace window accepts the standard rotation case: the holder keeps one
/// `key_id` across a pubkey change, so a token signed under the rotated-out
/// pubkey still names the same `key_id` and resolves through step 2.
///
/// # Errors
/// Returns `AttachError::BadSignature` on a malformed prefix or signature
/// mismatch against the matched pubkey, or `AttachError::UnknownKeyId` when
/// neither the active nor the grace set holds the token's key.
#[cfg(feature = "octo-attach-key-rotation")]
pub fn verify_attach_handle_payload_v2(
    key_set: &KeySet,
    signable_prefix: &[u8; 32],
    session_id: &SessionId,
    payload: &AttachPayload,
    mint_timestamp_unix: u64,
    ttl_unix: u64,
    sig: &Signature,
) -> Result<(), AttachError> {
    let prefix = prefix_from_token(signable_prefix)?;
    let key_id = prefix.key_id();
    let canonical = canonical_payload_bytes(session_id, payload, mint_timestamp_unix, ttl_unix);
    let msg = signable_bytes(&prefix, &canonical)?;
    let algorithm = prefix.algorithm_id();

    // 1. Active lookup. A mismatch here does NOT end the story — the whole
    //    point of the grace window is that a token signed before a rotation
    //    verifies against the retired key. The previous code returned the
    //    error from this step, so `grace_key` was unreachable and the grace
    //    window never accepted anything: a rotation silently invalidated
    //    every in-flight token, which is the failure mode the window exists
    //    to prevent.
    let mut last_err = None;
    if let Some(pk) = key_set.lookup(key_id) {
        match verify_bytes(&algorithm.to_owned(), pk, &msg, sig) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }

    // 2. Grace entries for the token's OWN key id only. Every retired pubkey
    //    under that one id is tried — a key id can have more than one, since
    //    a rotation under a stable verification method id changes the key and
    //    keeps the id. Walking grace pubkeys across *other* ids is what would
    //    enable forgery, and is never done.
    for pk in key_set.grace_keys_for(key_id) {
        match verify_bytes(&algorithm.to_owned(), pk, &msg, sig) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    if let Some(e) = last_err {
        return Err(e);
    }

    // 3. Exhausted: the id is known to neither map.
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
    let prefix = holder
        .signable_prefix()
        .map_err(|e| AttachError::BadSignature {
            reason: format!("IdentityKey signable prefix: {e}"),
        })?;
    let signature =
        sign_attach_handle_payload(holder, session_id, &payload, mint_timestamp_unix, ttl_unix)?;
    Ok(AttachHandle {
        session_id,
        mint_timestamp_unix,
        ttl_unix,
        signature,
        signable_prefix: prefix.to_bytes(),
        payload,
        transport,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_wallet::IdentityKey;

    fn activated_holder() -> IdentityKey {
        // The `key_id` is not optional here: `sign_attach_handle_payload`
        // refuses an identity with no key context, because a token that names
        // no key is exactly the unbound token this migration removes.
        let mut k = IdentityKey::generate()
            .expect("generate")
            .with_verification_method_id(octo_wallet::VerificationMethodId::derive(
                "attach-holder",
            ));
        k.activate(1_700_000_000).expect("activate");
        k
    }

    /// The prefix the holder embeds in a token, for tests that call the verify
    /// helper directly rather than going through `mint_attach_handle`.
    fn holder_prefix(holder: &IdentityKey) -> [u8; 32] {
        holder.signable_prefix().expect("key context").to_bytes()
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
        verify_attach_handle_payload(
            &pk,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        )
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
        let result = verify_attach_handle_payload(
            &pk,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        );
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
        let result = verify_attach_handle_payload(
            &wrong_pk,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        );
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
        let result = verify_attach_handle_payload(
            &pk,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            9_999,
            2_000,
            &sig,
        );
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
            &token.signable_prefix,
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
    // ── The prefix is carried, and the algorithm comes from it ──────────

    #[test]
    fn the_token_carries_the_prefix_in_band() {
        let holder = activated_holder();
        let token = mint_attach_handle(
            &holder,
            Uuid::from_bytes([0x77; 16]),
            [0x5a; 32],
            0,
            1_700_003_600,
            Transport::IN_PROCESS,
        )
        .expect("mint");

        // Not merely signed over — transported as a field, so a verifier
        // holding only the token can read the binding without being told it.
        assert_eq!(token.signable_prefix, holder_prefix(&holder));

        // And it survives the wire form, which is the only reason carrying it
        // is worth anything.
        let encoded = crate::handle::encoding::encode_token(&token).expect("encode");
        let decoded = crate::handle::encoding::decode_token(&encoded, &holder.public_key_bytes())
            .expect("decode");
        assert_eq!(
            decoded.signable_prefix, token.signable_prefix,
            "the prefix must survive encode/decode"
        );
    }

    #[test]
    fn the_verifier_dispatches_on_the_named_algorithm() {
        let holder = activated_holder();
        let session_id = [0x61; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0x62; 16]),
            since_cursor: 5,
        };
        let canonical = canonical_payload_bytes(&session_id, &payload, 1_000, 2_000);
        let prefix =
            octo_wallet::SignablePrefix::from_bytes(&holder_prefix(&holder)).expect("prefix bytes");
        let msg = signable_bytes(&prefix, &canonical).expect("signable");
        let sig = Signature(holder.sign(&msg).expect("sign"));

        // Proof the verifier is reading the algorithm from the prefix rather
        // than assuming Ed25519: hand it a token whose prefix names a
        // different algorithm. There is no such registered algorithm under
        // that name, so dispatch must fail closed.
        let foreign = octo_wallet::SignablePrefix::new(
            octo_wallet::AlgorithmId::derive("not-a-registered-algorithm"),
            octo_wallet::VerificationMethodId::derive("attach-holder"),
        )
        .to_bytes();

        let result = verify_attach_handle_payload(
            &holder.public_key_bytes(),
            &foreign,
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        );
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "an unknown algorithm must fail closed, got {result:?}"
        );

        // Control: the same signature verifies under its own prefix. Without
        // this the assertion above could pass merely because the helper is
        // broken for every input.
        verify_attach_handle_payload(
            &holder.public_key_bytes(),
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        )
        .expect("the real prefix must still verify");
    }

    #[test]
    fn a_token_naming_another_key_id_is_refused() {
        let holder = activated_holder();
        let session_id = [0x71; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0x72; 16]),
            since_cursor: 3,
        };
        let canonical = canonical_payload_bytes(&session_id, &payload, 1_000, 2_000);
        let real =
            octo_wallet::SignablePrefix::from_bytes(&holder_prefix(&holder)).expect("prefix bytes");
        let msg = signable_bytes(&real, &canonical).expect("signable");
        let sig = Signature(holder.sign(&msg).expect("sign"));

        // Same key, same algorithm, different `key_id` in the prefix. This is
        // the substitution the prefix exists to refuse: an attacker who can
        // rewrite the token's key_id must not be able to keep the signature.
        let swapped = octo_wallet::SignablePrefix::new(
            real.algorithm_id(),
            octo_wallet::VerificationMethodId::derive("some-other-key"),
        )
        .to_bytes();

        let result = verify_attach_handle_payload(
            &holder.public_key_bytes(),
            &swapped,
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        );
        assert!(
            matches!(result, Err(AttachError::BadSignature { .. })),
            "a swapped key_id must refuse, got {result:?}"
        );
    }

    #[test]
    fn minting_refuses_a_holder_with_no_key_context() {
        // A token that names no key is exactly the unbound token this
        // migration removes, so minting must fail rather than emit one.
        let mut holder = IdentityKey::generate().expect("generate");
        holder.activate(1_700_000_000).expect("activate");
        assert!(
            holder.verification_method_id().is_none(),
            "control: this identity really has no key context"
        );

        let err = mint_attach_handle(
            &holder,
            Uuid::from_bytes([0x81; 16]),
            [0x82; 32],
            0,
            1_700_003_600,
            Transport::IN_PROCESS,
        )
        .expect_err("a holder with no key_id must not mint a token");
        let AttachError::BadSignature { reason } = &err else {
            panic!("expected BadSignature, got {err:?}");
        };
        assert!(
            reason.to_lowercase().contains("key"),
            "the refusal must be the missing key context, not another error: {reason}"
        );
    }

    // ── The KeySet path: the key comes from the token, not the caller ───

    #[cfg(feature = "octo-attach-key-rotation")]
    /// A key id derived from a name, so a fixture can stand in for "a
    /// different verification method on the same holder".
    fn vmid(name: &str) -> octo_wallet::VerificationMethodId {
        octo_wallet::VerificationMethodId::derive(name)
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    fn v2_fixture() -> (IdentityKey, KeySet, SessionId, AttachPayload) {
        let holder = activated_holder();
        let session_id = [0xab; 32];
        let payload = AttachPayload {
            agent_id: Uuid::from_bytes([0xcd; 16]),
            since_cursor: 42,
        };
        let mut key_set = KeySet::new();
        key_set.insert(
            holder.signable_prefix().expect("key").key_id(),
            holder.public_key_bytes(),
        );
        (holder, key_set, session_id, payload)
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn sign_v2_binds_the_prefix_not_a_key_id_suffix() {
        let (holder, key_set, session_id, payload) = v2_fixture();

        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000)
            .expect("sign v2");
        verify_attach_handle_payload_v2(
            &key_set,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        )
        .expect("verify v2 active lookup");
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn the_key_is_read_from_the_token_not_supplied_by_the_caller() {
        let (holder, key_set, session_id, payload) = v2_fixture();
        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000)
            .expect("sign v2");

        // A token naming a key the KeySet does not hold must be rejected on
        // the token's own claim. Before the prefix landed on the token, the
        // verifier was *told* which key to try, so this state was not
        // expressible: the caller simply named a key that existed.
        let forged_prefix = octo_wallet::SignablePrefix::new(
            holder.signable_prefix().expect("key").algorithm_id(),
            vmid("a-key-this-set-does-not-hold"),
        )
        .to_bytes();

        match verify_attach_handle_payload_v2(
            &key_set,
            &forged_prefix,
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        ) {
            Err(AttachError::UnknownKeyId { key_id, .. }) => {
                assert_eq!(key_id, vmid("a-key-this-set-does-not-hold"));
            }
            other => panic!("a token naming an unknown key must not verify, got {other:?}"),
        }
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn verify_v2_grace_period_accepts_rotated_key() {
        // This test builds its own rotated set, so the fixture's set is unused.
        let (holder, _key_set, session_id, payload) = v2_fixture();
        let key_id = holder.signable_prefix().expect("key").key_id();
        // A real rotation under a stable verification method id: the old
        // pubkey goes to grace, the new one becomes active, and the id is
        // unchanged. The token was signed by the OLD key.
        let mut rotated = KeySet::new();
        rotated.insert(key_id, holder.public_key_bytes());
        rotated.move_to_grace(key_id);
        rotated.insert(key_id, [0xee; 32]);
        assert_eq!(
            rotated.grace_keys_for(key_id),
            &[holder.public_key_bytes()],
            "control: the rotated-out pubkey must still be retained"
        );

        let sig = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000)
            .expect("sign under the rotated-out key");

        verify_attach_handle_payload_v2(
            &rotated,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        )
        .expect("a token signed before rotation must survive the grace window");
    }

    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn verify_v2_grace_key_does_not_vouch_for_another_keys_token() {
        // Regression, carried over from the pre-prefix shape. A grace key must
        // NOT vouch for a token naming a DIFFERENT key id: iterating every
        // grace pubkey would let any holder of any grace key's private material
        // forge tokens claiming someone else's key.
        let (holder, _key_set, session_id, payload) = v2_fixture();
        let grace_key_id = holder.signable_prefix().expect("key").key_id();
        let mut rotated = KeySet::new();
        rotated.insert(grace_key_id, holder.public_key_bytes());
        rotated.move_to_grace(grace_key_id);
        rotated.insert(vmid("someone-elses-active-key"), [0xee; 32]);

        // The attacker holds the grace key's private material but signs a token
        // claiming a different verification method.
        let forged = octo_wallet::SignablePrefix::new(
            holder.signable_prefix().expect("key").algorithm_id(),
            vmid("someone-elses-active-key"),
        )
        .to_bytes();
        let canonical = canonical_payload_bytes(&session_id, &payload, 1_000, 2_000);
        let msg = signable_bytes(
            &octo_wallet::SignablePrefix::from_bytes(&forged).expect("prefix"),
            &canonical,
        )
        .expect("signable");
        let sig = Signature(holder.sign(&msg).expect("sign"));

        match verify_attach_handle_payload_v2(
            &rotated,
            &forged,
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        ) {
            // `BadSignature`, not `UnknownKeyId`: the claimed id IS in the
            // active set — it just does not match this signature. The grace
            // window was consulted only under the claimed id and holds no
            // entry for it, so the grace key did not vouch. Returning
            // `UnknownKeyId` here would be the wrong diagnosis and would also
            // mislead an operator about what to rotate.
            Err(AttachError::BadSignature { .. }) => {}
            other => panic!("a grace key must not vouch for another key id, got {other:?}"),
        }

        // The other branch: a token naming an id neither map holds. That is
        // the genuine unknown-key case, and it must be distinguished from the
        // mismatch above.
        let unknown = octo_wallet::SignablePrefix::new(
            holder.signable_prefix().expect("key").algorithm_id(),
            vmid("an-id-nobody-has"),
        )
        .to_bytes();
        match verify_attach_handle_payload_v2(
            &rotated,
            &unknown,
            &session_id,
            &payload,
            1_000,
            2_000,
            &sig,
        ) {
            Err(AttachError::UnknownKeyId { key_id, .. }) => {
                assert_eq!(key_id, vmid("an-id-nobody-has"));
            }
            other => panic!("an unknown key id must say so, got {other:?}"),
        }
        // Control: the real token does verify against the same set, so the
        // rejection above is about the key id and not a broken fixture.
        let real = sign_attach_handle_payload_v2(&holder, session_id, &payload, 1_000, 2_000)
            .expect("sign real");
        verify_attach_handle_payload_v2(
            &rotated,
            &holder_prefix(&holder),
            &session_id,
            &payload,
            1_000,
            2_000,
            &real,
        )
        .expect("control: the genuine token verifies");
    }
}
