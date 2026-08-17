//! Signing primitives (RFC-0871 §Algorithms).
//!
//!
// `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`. Signature
//! preimage: `blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE",
//! envelope_id || from_did_wire || payload).as_bytes()` (32 bytes,
//! domain-separated).

use crate::envelope::NodeEnvelope;

/// Domain-separated context for `envelope_id` derivation.
///
/// Per RFC-0871 §Algorithms: `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`.
/// The envelope_id is computed over the borsh serialization of the envelope
/// with `envelope_id` zeroed out — this is the canonical idempotent
/// derivation.
pub const DOMAIN_ENVELOPE_ID: &str = "OCTO_NODEENVELOPE_V1_ID";

/// Domain-separated context for signature preimage derivation.
///
/// Per RFC-0871 §Algorithms: `preimage = blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE",
/// envelope_id || from_did_wire || payload).as_bytes()` (32 bytes).
pub const DOMAIN_SIGNATURE: &str = "OCTO_NODEENVELOPE_V1_SIGNATURE";

/// Compute `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`.
///
/// Implementation: serialize the envelope with `envelope_id` zeroed (the
/// build-time placeholder), then BLAKE3-256 hash. Per RFC-0871 §Algorithms
/// step 2.
#[must_use]
pub fn compute_envelope_id(envelope: &NodeEnvelope) -> [u8; 32] {
    let mut env_without_id = envelope.clone();
    env_without_id.envelope_id = [0u8; 32];
    let bytes =
        borsh::to_vec(&env_without_id).expect("NodeEnvelope borsh serialization is infallible");
    *blake3::hash(&bytes).as_bytes()
}

/// Compute the signature preimage: 32-byte domain-separated digest over
/// `envelope_id || from_did_wire || payload`.
///
/// Per RFC-0871 §Algorithms step 3. Output is `blake3::derive_key(ctx, input)`
/// per RFC-0853 §Domain Separation.
#[must_use]
pub fn signature_preimage(envelope_id: &[u8; 32], from_did_wire: &str, payload: &[u8]) -> [u8; 32] {
    let mut input = Vec::with_capacity(32 + from_did_wire.len() + payload.len());
    input.extend_from_slice(envelope_id);
    input.extend_from_slice(from_did_wire.as_bytes());
    input.extend_from_slice(payload);
    blake3::derive_key(DOMAIN_SIGNATURE, &input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload_kind::IDENTITY_RESOLVE;
    use crate::recipient::RecipientRef;
    use octo_ident::CanonicalCodec;
    use octo_ident::DidCodec;
    use octo_ident::WireDid;

    fn sample_did(seed: u8) -> WireDid {
        let mut pk = [0u8; 32];
        for (i, byte) in pk.iter_mut().enumerate() {
            *byte = seed.wrapping_add(i as u8);
        }
        let raw = CanonicalCodec::mint(&pk);
        CanonicalCodec::raw_to_wire(&raw).unwrap()
    }

    #[test]
    fn envelope_id_is_deterministic() {
        let did = sample_did(7);
        let env_a = NodeEnvelope::build(
            did.clone(),
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![0x01, 0x02, 0x03],
            vec![],
            [0xff; 32],
            1_735_689_600_000,
            crate::envelope::VERSION_TAG_V2,
        )
        .unwrap();
        let env_b = NodeEnvelope::build(
            did,
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![0x01, 0x02, 0x03],
            vec![],
            [0xff; 32],
            1_735_689_600_000,
            crate::envelope::VERSION_TAG_V2,
        )
        .unwrap();
        assert_eq!(env_a.envelope_id, env_b.envelope_id);
    }

    #[test]
    fn envelope_id_changes_with_payload() {
        let did = sample_did(7);
        let env_a = NodeEnvelope::build(
            did.clone(),
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![0x01],
            vec![],
            [0; 32],
            1_735_689_600_000,
            crate::envelope::VERSION_TAG_V2,
        )
        .unwrap();
        let env_b = NodeEnvelope::build(
            did,
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![0x02],
            vec![],
            [0; 32],
            1_735_689_600_000,
            crate::envelope::VERSION_TAG_V2,
        )
        .unwrap();
        assert_ne!(env_a.envelope_id, env_b.envelope_id);
    }

    #[test]
    fn signature_preimage_is_32_bytes() {
        let preimage = signature_preimage(&[0xab; 32], "did:octo:zTest", b"abc");
        assert_eq!(preimage.len(), 32);
    }

    #[test]
    fn signature_preimage_is_deterministic() {
        let a = signature_preimage(&[0xab; 32], "did:octo:zTest", b"abc");
        let b = signature_preimage(&[0xab; 32], "did:octo:zTest", b"abc");
        assert_eq!(a, b);
    }

    #[test]
    fn signature_preimage_changes_with_input() {
        let a = signature_preimage(&[0xab; 32], "did:octo:zTest", b"abc");
        let b = signature_preimage(&[0xab; 32], "did:octo:zTest", b"abd");
        assert_ne!(a, b);
    }
}
