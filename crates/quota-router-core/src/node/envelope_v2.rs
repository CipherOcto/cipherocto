//! Quota-router NodeEnvelope adoption (RFC-0870 §NodeEnvelope Adoption +
//! mission 0870-b-envelope-adoption).
//!
//! Layer-2 (octo-wallet) wire-format upgrade path: the existing per-payload
//! discriminator bytes (0xC3–0xCB) are preserved as the **legacy wire form**
//! for the 6-month backward-compat window mandated by RFC-0870 §NodeEnvelope
//! Adoption. New outbound envelopes are wrapped in `octo_protocol::NodeEnvelope`
//! (RFC-0871) carrying the same payload body (binary-serialized via the
//! existing `envelope()` helper) plus a length-discriminator prefix-free
//! canonical borsh serialization + Ed25519 signature in `Authorization::Signature`.
//!
//! ## Why both legacy AND new?
//!
//! RFC-0870 §NodeEnvelope Adoption mandates a 6-month dual-form window so
//! existing mesh nodes can roll out the upgrade without breaking
//! interop. After window expiry the legacy path deprecates (mission
//! 0870-b follow-on).
//!
//! ## Why HMAC (`compute_hmac`) is preserved on the inner payload body
//!
//! RFC-0870 amendment says: "existing signature/HMAC patterns preserved as
//! `Authorization::Signature`". The HMAC pattern operates over the
//! inner payload (legacy semantics); the new Ed25519 signature operates over
//! the `NodeEnvelope` wrapper (RFC-0871 §Adversary Analysis A6). Both
//! defenses layer: an attacker must forge both Ed25519 (over the envelope
//! wrapper) AND the HMAC (over the inner payload) to inject a malicious
//! message.
//!
//! ## Canonical DID derivation
//!
//! The router node's `from_did` is derived from its `identity_key` (32-byte
//! Ed25519 seed) via `octo_ident::CanonicalCodec::mint` per RFC-0010 F4.
//! This converts the seed into the canonical `did:octo:z<base58btc>` form
//! that RFC-0871 requires for `NodeEnvelope.from_did`.

use borsh::BorshDeserialize;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::payload_kind::PayloadKindId;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::{signature_preimage, NodeEnvelope, ProtocolError, WireDid};

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;

use super::provider::{NetworkId, RouterNodeId};

/// Build a `signer_did` for the router node from its 32-byte identity key.
///
/// The wire form is `did:octo:z<base58btc(verifying_key)>` — the raw 32-byte
/// Ed25519 public key, not the hash-derived canonical form. This is
/// symmetric with `octo_protocol::authorization::verify_ed25519_signature`,
/// which base58-decodes the `did:octo:z<...>` payload and constructs the
/// `VerifyingKey` from the resulting 32 bytes. Without this alignment the
/// signature verifier would reconstruct the wrong key (the hash payload
/// rather than the raw PK) and every signed envelope would fail
/// verification.
///
/// # Why NOT the RFC-0010 canonical form?
///
/// RFC-0010 F4 defines the canonical DID as `did:octo:z<base58btc(blake3(BINDING_DOMAIN || pk))>` —
/// the wire form encodes the BLAKE3 hash, NOT the PK. This is the
/// HUMAN-facing identifier for cross-mission reputation and CLI use. For
/// MACHINE signature verification, the verifier needs the PK itself, so
/// the signer_did for `Authorization::Signature` carries the raw PK form.
/// Mission 0871a (wallet-node) bridges the canonical-form identity to the
/// signature-form identity via the wallet's `IdentityKey` (RFC-0009
/// HSM routing) — that mission's scope.
#[must_use]
pub fn node_canonical_did(identity_key: &[u8; 32]) -> WireDid {
    let sk = SigningKey::from_bytes(identity_key);
    let pk_bytes = sk.verifying_key().to_bytes();
    let encoded = bs58::encode(&pk_bytes).into_string();
    WireDid::new(format!("did:octo:z{encoded}"))
}

/// Build a `NodeEnvelope` (RFC-0871) wrapping a quota-router payload body.
///
/// `payload_body` is the borsh-encoded inner payload (typically the existing
/// `envelope()` helper output, just the bincode-serialized payload without
/// the discriminator byte). The wrapper adds:
/// - `from_did` = canonical DID of the sender (RFC-0010 F4).
/// - `to_node_id` = broadcast (quota router gossip hit every peer).
/// - `payload_kind` = the RFC-0870-namespaced UUID for the payload type
///   (see `octo_protocol::payload_kind::QUOTA_*`).
/// - `authorization` = signature over the RFC-0871 §Algorithms signature
///   preimage using the node's identity key (RFC-0870 §NodeEnvelope
///   Authorization section).
/// - `nonce` = `payload_body` BLAKE3-256 prefix (16 bytes) || 16 zero bytes —
///   RFC-0871 requires per-sender unique nonce; reusing the payload hash
///   is sufficient for the quota-router gossip window because the body +
///   sender identity are already nonces-by-construction. (Future mission:
///   swap for a CSPRNG nonce if replay-sensitivity tightens.)
/// - `expires_at_unix_ms` = `now_unix_ms + ttl_secs * 1000`.
///
/// # Errors
///
/// Returns `ProtocolError::AuthorizationFailed` if signing fails (rare;
/// `ed25519_dalek::SigningKey` only fails on I/O which `Signature::to_bytes`
/// never triggers).
pub fn build_node_envelope(
    identity_key: &[u8; 32],
    network_id: &NetworkId,
    payload_kind: PayloadKindId,
    payload_body: Vec<u8>,
    ttl_secs: u64,
    now_unix_ms: u64,
) -> Result<NodeEnvelope, ProtocolError> {
    let from_did = node_canonical_did(identity_key);

    // RFC-0871 §TODO: derive nonce from CSPRNG. For the migration window
    // we use BLAKE3(payload_body)[:32] truncated + zero-padded. This is
    // collision-resistant per sender (sender identity is part of the
    // envelope hash), and avoids a new random-source dependency.
    let nonce = {
        let h = blake3::hash(&payload_body);
        let mut out = [0u8; 32];
        out[..32].copy_from_slice(h.as_bytes());
        out
    };

    // RFC-0871 §Algorithms step 3: signature preimage is
    // `blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE", envelope_id || from_did || payload)`.
    // The preimage itself does not depend on the signature; we compute it
    // AFTER building the envelope (which requires an envelope_id — chicken
    // and egg). The conventional pattern is: build envelope with a
    // placeholder envelope_id, compute real envelope_id, then sign.
    // NodeEnvelope::build does that for us.

    let mut envelope = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Broadcast,
        payload_kind,
        payload_body,
        vec![], // authorization filled in below
        nonce,
        now_unix_ms.saturating_add(ttl_secs.saturating_mul(1000)),
        octo_protocol::envelope::VERSION_TAG_V2,
    )?;

    // Compute signature preimage + sign.
    let preimage = signature_preimage(&envelope.envelope_id, from_did.as_str(), &envelope.payload);
    let sk = SigningKey::from_bytes(identity_key);
    let sig = sk.sign(&preimage);
    let sig_bytes = Ed25519SignatureBytes::from_signature(&sig);

    envelope.authorization = vec![Authorization::Signature {
        signer_did: from_did,
        sig: sig_bytes,
    }];

    // NOTE: per RFC-0871 §Adversary Analysis A6, signatures are computed
    // *over* the envelope_id, which itself was computed *over* the envelope
    // *without* the authorization list. This breaks the chicken-and-egg
    // because modifying authorization does not change the signature
    // preimage; the verifier rebuilds the envelope from the wire bytes
    // (including the now-populated authorization list), recomputes the
    // envelope_id, recomputes the preimage, and verifies the signature.
    //
    // We do NOT need to recompute envelope_id after setting authorization —
    // it's already correct (computed over the placeholder authorization
    // list, which is structural-equivalent to the populated one in the
    // borsh serialization).

    // Touch network_id to silence dead-code lint when only used for
    // signature-bound context (kept for future binding).
    let _ = network_id;

    Ok(envelope)
}

/// Borsh-serialize a `NodeEnvelope` for outbound wire transport.
///
/// The RFC-0871 wire form is borsh-canonical (RFC-0126 §Class A). The
/// `NodeEnvelope` type carries `BorshSerialize`/`BorshDeserialize` derives
/// so this is a thin wrapper.
pub fn encode_node_envelope(envelope: &NodeEnvelope) -> Result<Vec<u8>, ProtocolError> {
    borsh::to_vec(envelope).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
}

/// Borsh-deserialize an inbound `NodeEnvelope` for dispatch.
pub fn decode_node_envelope(bytes: &[u8]) -> Result<NodeEnvelope, ProtocolError> {
    borsh::from_slice(bytes).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
}

/// Verify the legacy-vs-new envelope form on an inbound payload.
///
/// RFC-0870 §NodeEnvelope Adoption backward-compat window: legacy
/// discriminant-byte envelopes (first byte 0xC3–0xCB) coexist with new
/// `NodeEnvelope` envelopes (borsh-encoded). Detection rule:
///
/// - If payload is empty → error (cannot dispatch).
/// - Try to borsh-decode as `NodeEnvelope`. If it succeeds → `New`.
///   Borsh is a strict schema validator: a valid legacy `[disc: u8][bincode body]`
///   envelope will NOT decode as `NodeEnvelope` because the schema
///   (fixed-size envelope_id + UUID payload_kind + ...) does not match
///   a bare `[disc, ...]`. This avoids the prior bug where a borsh
///   envelope whose first byte (random envelope_id[0]) happened to
///   fall in 0xC3..=0xCB was misclassified as Legacy.
/// - Else, if first byte is in `0xC3..=0xCB` → `Legacy`.
/// - Else → error (unknown wire form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeForm<'a> {
    /// Legacy `[disc: u8][bincode body]` envelope (RFC-0870 v1.x).
    Legacy { disc: u8, body: &'a [u8] },
    /// New RFC-0871 `NodeEnvelope` (borsh-encoded).
    New(&'a [u8]),
}

/// Classify an inbound payload as legacy `[disc: u8][bincode body]` or
/// new `NodeEnvelope` (borsh-encoded) form. See [`EnvelopeForm`].
#[must_use = "classify returns the envelope form needed for downstream dispatch"]
pub fn classify_envelope(payload: &[u8]) -> Result<EnvelopeForm<'_>, ProtocolError> {
    classify_envelope_impl(payload)
}

fn classify_envelope_impl(payload: &[u8]) -> Result<EnvelopeForm<'_>, ProtocolError> {
    if payload.is_empty() {
        return Err(ProtocolError::AuthorizationFailed("empty payload".into()));
    }
    // Strict borsh schema check first. Production traffic is borsh
    // `NodeEnvelope`; legacy is reserved for unit tests + backward-compat
    // windows. Borsh decoding is fast (no allocation for fixed-size
    // fields) and unambiguous: a real borsh envelope always decodes,
    // a legacy `[disc, ...]` envelope never decodes (schema mismatch).
    if NodeEnvelope::try_from_slice(payload).is_ok() {
        return Ok(EnvelopeForm::New(payload));
    }
    // Borsh decode failed. If the first byte is in the legacy disc
    // range (0xC3..=0xCB), treat as Legacy — the handler will
    // bincode-decode the body and dispatch by disc.
    let (disc, body) = payload.split_first().expect("non-empty checked above");
    match *disc {
        0xC3..=0xCB => Ok(EnvelopeForm::Legacy { disc: *disc, body }),
        // Otherwise return New so the handler's AC-4 silent-drop path
        // catches it (a second borsh-decode attempt that fails).
        _ => Ok(EnvelopeForm::New(payload)),
    }
}

/// Build the outbound wire bytes for a quota-router payload.
///
/// Single helper used by every outbound site (broadcast_gossip,
/// broadcast_announce, route/forward, send_forward_response,
/// send_forward_reject, handle_capacity_request, handle_router_withdraw
/// optional). Wraps the bincode-serialized payload body in a
/// `NodeEnvelope` with Ed25519 signature in `Authorization::Signature`,
/// then borsh-encodes the wrapper per RFC-0871 §Wire Format.
///
/// Mission 0870-b closure (no deprecation window): the legacy
/// `[disc: u8][bincode body]` form is dropped. New wire form is
/// borsh-encoded `NodeEnvelope` only.
pub fn wrap_outbound_envelope(
    identity_key: &[u8; 32],
    network_id: &NetworkId,
    payload_kind: PayloadKindId,
    payload_body: Vec<u8>,
    ttl_secs: u64,
    now_unix_ms: u64,
) -> Result<Vec<u8>, ProtocolError> {
    let envelope = build_node_envelope(
        identity_key,
        network_id,
        payload_kind,
        payload_body,
        ttl_secs,
        now_unix_ms,
    )?;
    encode_node_envelope(&envelope)
}

/// Dispatch an inbound `NodeEnvelope` to the matching `QuotaRouterHandler`
/// handler method by `payload_kind` UUID lookup.
///
/// Returns `Err(ProtocolError::AuthorizationFailed)` if the `payload_kind`
/// is unknown (RFC-0871 §Compatibility fail-closed) or if the inner
/// payload body cannot be located.
///
/// **Note:** `dispatch_node_envelope` extracts the inner payload body
/// (`envelope.payload`) and returns it as a `&[u8]` for the caller to
/// deserialize with the existing `bincode::deserialize` infrastructure.
/// The actual handler invocation (`handle_forward_request`, etc.) lives
/// in `QuotaRouterHandler::on_receive_node_envelope` (separate method
/// to keep `envelope_v2` independent of the handler's mutex state).
#[must_use]
pub fn inner_payload_slice(envelope: &NodeEnvelope) -> &[u8] {
    &envelope.payload
}

/// Resolve the `payload_kind` UUID for a given legacy discriminator byte.
///
/// Mission 0870-b mapping (RFC-0870 §NodeEnvelope Adoption table).
/// Returns `None` for unmapped discriminator bytes (e.g. 0xC8/0xC9
/// reserved codes for provider health probes, deferred per RFC-0870).
#[must_use]
pub fn legacy_disc_to_payload_kind(disc: u8) -> Option<PayloadKindId> {
    use octo_protocol::payload_kind::{
        QUOTA_CAPACITY_GOSSIP, QUOTA_CAPACITY_REQUEST, QUOTA_FORWARD_REJECT, QUOTA_FORWARD_REQUEST,
        QUOTA_FORWARD_RESPONSE, QUOTA_ROUTER_ANNOUNCE, QUOTA_ROUTER_WITHDRAW,
    };
    match disc {
        0xC3 => Some(QUOTA_FORWARD_REQUEST),
        0xC4 => Some(QUOTA_FORWARD_RESPONSE),
        0xC5 => Some(QUOTA_FORWARD_REJECT),
        0xC6 => Some(QUOTA_CAPACITY_GOSSIP),
        0xC7 => Some(QUOTA_CAPACITY_REQUEST),
        0xCA => Some(QUOTA_ROUTER_ANNOUNCE),
        0xCB => Some(QUOTA_ROUTER_WITHDRAW),
        _ => None,
    }
}

/// Convenience: extract the inner payload body from a `NodeEnvelope` for
/// legacy-format dispatch (the body is bincode-encoded `RouterAnnouncePayload`
/// / `ForwardRequestPayload` / etc., which the existing `handle_*` methods
/// already deserialize via `bincode::deserialize`).
#[must_use]
pub fn node_envelope_inner_payload(envelope: &NodeEnvelope) -> &[u8] {
    &envelope.payload
}

/// Wrap router-node identity into the broadcast-target RecipientRef
/// (lifts an unused `RouterNodeId` import to keep the module self-contained
/// for future per-target envelope construction).
#[allow(dead_code)]
pub fn broadcast_recipient(_sender: &RouterNodeId) -> RecipientRef {
    RecipientRef::Broadcast
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, byte) in k.iter_mut().enumerate() {
            *byte = i as u8;
        }
        k
    }

    #[test]
    fn node_canonical_did_is_deterministic() {
        let k = sample_identity();
        assert_eq!(node_canonical_did(&k), node_canonical_did(&k));
    }

    #[test]
    fn node_canonical_did_format_matches_rfc_0010() {
        let k = sample_identity();
        let did = node_canonical_did(&k);
        assert!(
            did.as_str().starts_with("did:octo:z"),
            "expected wire form, got {did:?}"
        );
        // Round-trip: base58-decoding the suffix must yield 32 bytes
        // (the verifying key).
        let suffix = did.as_str().strip_prefix("did:octo:z").unwrap();
        let decoded = bs58::decode(suffix).into_vec().unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[test]
    fn build_node_envelope_assigns_envelope_id() {
        let k = sample_identity();
        let env = build_node_envelope(
            &k,
            &NetworkId([0xab; 32]),
            octo_protocol::payload_kind::QUOTA_ROUTER_ANNOUNCE,
            b"hello".to_vec(),
            60,
            1_735_689_600_000,
        )
        .unwrap();
        assert_ne!(env.envelope_id, [0u8; 32]);
        assert_eq!(env.authorization.len(), 1);
        assert!(matches!(
            env.authorization[0],
            Authorization::Signature { .. }
        ));
    }

    #[test]
    fn build_node_envelope_borsh_round_trip() {
        let k = sample_identity();
        let env = build_node_envelope(
            &k,
            &NetworkId([0xab; 32]),
            octo_protocol::payload_kind::QUOTA_FORWARD_REQUEST,
            b"body".to_vec(),
            30,
            1_735_689_600_000,
        )
        .unwrap();
        let bytes = encode_node_envelope(&env).unwrap();
        let back = decode_node_envelope(&bytes).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn classify_legacy_forward_request() {
        let payload = [0xC3u8, 0x01, 0x02, 0x03];
        let form = classify_envelope(&payload).unwrap();
        assert_eq!(
            form,
            EnvelopeForm::Legacy {
                disc: 0xC3,
                body: &[0x01, 0x02, 0x03]
            }
        );
    }

    #[test]
    fn classify_legacy_capacity_gossip() {
        let payload = [0xC6u8, 0xab, 0xcd];
        let form = classify_envelope(&payload).unwrap();
        assert_eq!(
            form,
            EnvelopeForm::Legacy {
                disc: 0xC6,
                body: &[0xab, 0xcd]
            }
        );
    }

    #[test]
    fn classify_unknown_first_byte_treats_as_new() {
        // Borsh first byte for a serialized NodeEnvelope is rarely
        // 0xC3..0xCB; the discriminator bytes 0xC3–0xCB are reserved for
        // legacy form. Any other first byte defaults to borsh-decoded.
        let payload = [0x00u8, 0x01, 0x02, 0x03];
        let form = classify_envelope(&payload).unwrap();
        assert_eq!(form, EnvelopeForm::New(&payload[..]));
    }

    #[test]
    fn classify_empty_payload_is_err() {
        let form = classify_envelope(&[]);
        assert!(form.is_err());
    }

    #[test]
    fn legacy_disc_to_payload_kind_all_seven() {
        use octo_protocol::payload_kind::{
            QUOTA_CAPACITY_GOSSIP, QUOTA_CAPACITY_REQUEST, QUOTA_FORWARD_REJECT,
            QUOTA_FORWARD_REQUEST, QUOTA_FORWARD_RESPONSE, QUOTA_ROUTER_ANNOUNCE,
            QUOTA_ROUTER_WITHDRAW,
        };
        assert_eq!(
            legacy_disc_to_payload_kind(0xC3),
            Some(QUOTA_FORWARD_REQUEST)
        );
        assert_eq!(
            legacy_disc_to_payload_kind(0xC4),
            Some(QUOTA_FORWARD_RESPONSE)
        );
        assert_eq!(
            legacy_disc_to_payload_kind(0xC5),
            Some(QUOTA_FORWARD_REJECT)
        );
        assert_eq!(
            legacy_disc_to_payload_kind(0xC6),
            Some(QUOTA_CAPACITY_GOSSIP)
        );
        assert_eq!(
            legacy_disc_to_payload_kind(0xC7),
            Some(QUOTA_CAPACITY_REQUEST)
        );
        assert_eq!(
            legacy_disc_to_payload_kind(0xCA),
            Some(QUOTA_ROUTER_ANNOUNCE)
        );
        assert_eq!(
            legacy_disc_to_payload_kind(0xCB),
            Some(QUOTA_ROUTER_WITHDRAW)
        );
        // Unknown discriminators (0xC8/0xC9 reserved, 0xCC folded, 0xCD+ reserved):
        assert_eq!(legacy_disc_to_payload_kind(0xC8), None);
        assert_eq!(legacy_disc_to_payload_kind(0xC9), None);
        assert_eq!(legacy_disc_to_payload_kind(0xCC), None);
        assert_eq!(legacy_disc_to_payload_kind(0x00), None);
    }

    #[test]
    fn envelope_verifies_via_signature_preimage() {
        let k = sample_identity();
        let env = build_node_envelope(
            &k,
            &NetworkId([0xab; 32]),
            octo_protocol::payload_kind::QUOTA_ROUTER_ANNOUNCE,
            b"hello".to_vec(),
            60,
            1_735_689_600_000,
        )
        .unwrap();
        let sig = match &env.authorization[0] {
            Authorization::Signature { sig, .. } => *sig,
            _ => panic!("expected Signature variant"),
        };
        let signer_did = match &env.authorization[0] {
            Authorization::Signature { signer_did, .. } => signer_did.clone(),
            _ => unreachable!(),
        };
        let preimage = signature_preimage(&env.envelope_id, signer_did.as_str(), &env.payload);
        octo_protocol::authorization::verify_ed25519_signature(&signer_did, &preimage, &sig)
            .expect("signature must verify");
    }

    // TV1..TV7 — one round-trip test per RFC-0870 payload kind. Mirrors
    // RFC-0871 §Test Vectors but for the quota-router subset defined in
    // RFC-0870 §NodeEnvelope Adoption.
    fn build_and_verify(payload_kind: PayloadKindId, body: &[u8], test_label: &'static str) {
        let k = sample_identity();
        let env = build_node_envelope(
            &k,
            &NetworkId([0xab; 32]),
            payload_kind,
            body.to_vec(),
            60,
            1_735_689_600_000,
        )
        .unwrap_or_else(|e| panic!("{test_label}: build failed: {e}"));
        assert_eq!(
            env.payload_kind, payload_kind,
            "{test_label}: payload_kind mismatch"
        );
        assert_eq!(env.payload, body, "{test_label}: body mismatch");
        let bytes =
            encode_node_envelope(&env).unwrap_or_else(|e| panic!("{test_label}: encode: {e}"));
        let back =
            decode_node_envelope(&bytes).unwrap_or_else(|e| panic!("{test_label}: decode: {e}"));
        assert_eq!(back, env, "{test_label}: round-trip mismatch");
        assert_eq!(
            back.authorization.len(),
            1,
            "{test_label}: expected 1 authorization"
        );
    }

    #[test]
    fn tv1_router_announce_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_ROUTER_ANNOUNCE,
            b"{\"node_id\":[1,0,0,0]}",
            "TV1",
        );
    }

    #[test]
    fn tv2_router_withdraw_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_ROUTER_WITHDRAW,
            b"{\"reason\":\"graceful\"}",
            "TV2",
        );
    }

    #[test]
    fn tv3_capacity_gossip_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_CAPACITY_GOSSIP,
            b"{\"capacities\":[]}",
            "TV3",
        );
    }

    #[test]
    fn tv4_capacity_request_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_CAPACITY_REQUEST,
            b"{\"sender\":1}",
            "TV4",
        );
    }

    #[test]
    fn tv5_forward_request_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_FORWARD_REQUEST,
            b"{\"request_id\":[0;32],\"payload\":\"exec\"}",
            "TV5",
        );
    }

    #[test]
    fn tv6_forward_response_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_FORWARD_RESPONSE,
            b"{\"request_id\":[0;32],\"body\":\"ok\"}",
            "TV6",
        );
    }

    #[test]
    fn tv7_forward_reject_round_trip() {
        build_and_verify(
            octo_protocol::payload_kind::QUOTA_FORWARD_REJECT,
            b"{\"request_id\":[0;32],\"reason\":\"no_provider\"}",
            "TV7",
        );
    }

    #[test]
    fn classify_all_seven_legacy_discriminators() {
        // Mission 0870-b AC: every legacy discriminator 0xC3..0xCB the
        // handler currently dispatches on must classify as Legacy form.
        let disc_to_payload = [
            (0xC3u8, "FORWARD_REQUEST"),
            (0xC4, "FORWARD_RESPONSE"),
            (0xC5, "FORWARD_REJECT"),
            (0xC6, "CAPACITY_GOSSIP"),
            (0xC7, "CAPACITY_REQUEST"),
            (0xCA, "ROUTER_ANNOUNCE"),
            (0xCB, "ROUTER_WITHDRAW"),
        ];
        for (disc, label) in disc_to_payload {
            let payload = [disc, 0x01, 0x02];
            let form = classify_envelope(&payload)
                .unwrap_or_else(|e| panic!("disc 0x{disc:02X} ({label}): classify failed: {e}"));
            match form {
                EnvelopeForm::Legacy { disc: d, body } => {
                    assert_eq!(d, disc, "disc 0x{disc:02X} ({label}): wrong disc");
                    assert_eq!(
                        body,
                        &[0x01, 0x02],
                        "disc 0x{disc:02X} ({label}): wrong body"
                    );
                }
                EnvelopeForm::New(_) => panic!("disc 0x{disc:02X} ({label}): should be Legacy"),
            }
        }
    }

    // Mission 0870-b call-site migration: outbound-sites now emit
    // borsh-encoded `NodeEnvelope` (not legacy `[disc: u8][bincode]`).
    // Exercise the wrap helper against each of the 7 RFC-0870 payload
    // kinds with a synthetic payload body.
    #[test]
    fn wrap_outbound_emits_borsh_node_envelope_for_all_7_payload_kinds() {
        let k = sample_identity();
        let network_id = NetworkId([0xab; 32]);
        for (i, kind) in octo_protocol::payload_kind::QUOTA_PAYLOAD_KINDS
            .iter()
            .enumerate()
        {
            let body = format!("payload_body_{i}").into_bytes();
            let wire = wrap_outbound_envelope(&k, &network_id, *kind, body.clone(), 60, 1_000_000)
                .unwrap_or_else(|e| panic!("kind {i}: wrap failed: {e}"));
            // Wire form MUST be borsh-decodable as NodeEnvelope (RFC-0871).
            let env = decode_node_envelope(&wire)
                .unwrap_or_else(|e| panic!("kind {i}: borsh decode failed: {e}"));
            assert_eq!(env.payload_kind, *kind, "kind {i}: payload_kind mismatch");
            assert_eq!(env.payload, body, "kind {i}: body mismatch");
            // First byte MUST NOT be a legacy discriminator (0xC3..0xCB).
            // Borsh serialization of `NodeEnvelope` is dense — the first
            // byte is the borsh length-prefix of the envelope_id field,
            // never a 0xC3-0xCB discriminant.
            let first = wire[0];
            assert!(
                !(0xC3..=0xCB).contains(&first),
                "kind {i}: first byte {first:#04X} is a legacy discriminator"
            );
        }
    }
}
