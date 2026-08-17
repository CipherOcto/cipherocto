//! `NodeEnvelope` — the canonical envelope every CipherOcto specialized node
//! uses for inter-node communication (RFC-0871 §Data Structures).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::authorization::Authorization;
use crate::error::ProtocolError;
use crate::payload_kind::PayloadKindId;
use crate::recipient::RecipientRef;
use crate::signing::compute_envelope_id;

// WireDid is re-exported from octo-protocol at the crate root for downstream
// convenience; use it through the crate path so we don't need a private
// re-export here.
use crate::WireDid;

/// `NodeEnvelope` wire version 1 (RFC-0871 §14.1). Pre-cutover envelopes
/// carry this tag; receivers MUST hard-reject V1 receipts (per
/// `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
/// §14.1 — "V1 receipt drain requires coordinated consumer rebuild;
/// silent fallback would replay cross-cutover attacks").
pub const VERSION_TAG_V1: u8 = 0xA0;

/// `NodeEnvelope` wire version 2 (RFC-0871 §14.1 amendment). All
/// post-cutover envelopes carry this tag. Distinct from V1 by exactly
/// one byte; receivers MUST reject V1 deterministically.
pub const VERSION_TAG_V2: u8 = 0xA1;

/// Canonical envelope for specialized-node communication (RFC-0871).
///
/// Wire form: borsh-serialized. `envelope_id` is `BLAKE3-256` of
/// `canonical_ser(self_without_envelope_id)` per RFC-0871 §Algorithms. Fields:
///
/// - `envelope_id` — replay-defense identifier
/// - `version_tag` — wire version discriminator (`VERSION_TAG_V2` for
///   post-cutover envelopes; V1 rejected at verify time per §14.1)
/// - `from_did` — sender canonical DID (validated via `CanonicalCodec::parse`)
/// - `to_node_id` — recipient reference (direct / domain / broadcast)
/// - `payload_kind` — 128-bit UUID discriminator
/// - `payload` — borsh-serialized payload body
/// - `authorization` — `Vec<Authorization>` (logical AND verification)
/// - `nonce` — per-sender unique 32-byte nonce
/// - `expires_at_unix_ms` — TTL ceiling (millisecond resolution, RFC-0970 §TV11)
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NodeEnvelope {
    /// BLAKE3-256 hash of canonical_ser(all other fields). Replay defense.
    pub envelope_id: [u8; 32],
    /// Wire version discriminator (RFC-0871 §14.1). `VERSION_TAG_V2` for
    /// post-cutover envelopes. Receivers MUST reject V1 deterministically
    /// via [`NodeEnvelope::verify_version`].
    pub version_tag: u8,
    /// Sender canonical DID. Validated via `octo_ident::CanonicalCodec::parse()`.
    pub from_did: WireDid,
    /// Recipient reference.
    pub to_node_id: RecipientRef,
    /// Payload discriminator (128-bit UUID, RFC-allocated).
    pub payload_kind: PayloadKindId,
    /// Borsh-encoded payload body.
    pub payload: Vec<u8>,
    /// Authorization(s). Capability IS one.
    pub authorization: Vec<Authorization>,
    /// Per-sender unique nonce.
    pub nonce: [u8; 32],
    /// TTL in unix milliseconds (RFC-0970 §TV11).
    pub expires_at_unix_ms: u64,
}

impl NodeEnvelope {
    /// Build a fresh envelope from typed fields, computing `envelope_id` per
    /// RFC-0871 §Algorithms step 2.
    ///
    /// Validates `from_did` shape via `octo_ident::CanonicalCodec::parse()` —
    /// legacy `did:octo:b<base32>` is rejected post-deprecation window.
    /// Also validates `version_tag` — only `VERSION_TAG_V1` / `VERSION_TAG_V2`
    /// accepted; any other value returns `ProtocolError::UnsupportedVersion`.
    /// (Build-time validation catches typos loud; verify-time
    /// `verify_version` is the runtime gate for incoming receipts.)
    #[allow(clippy::too_many_arguments)]
    // Parameter ordering is pinned by RFC-0871 §14.1 (wire-form canonical
    // signature). The `version_tag` argument was added per RFC-0870 §14.1
    // (S5, commit d007de54).
    pub fn build(
        from_did: WireDid,
        to_node_id: RecipientRef,
        payload_kind: PayloadKindId,
        payload: Vec<u8>,
        authorization: Vec<Authorization>,
        nonce: [u8; 32],
        expires_at_unix_ms: u64,
        version_tag: u8,
    ) -> Result<Self, ProtocolError> {
        if version_tag != VERSION_TAG_V1 && version_tag != VERSION_TAG_V2 {
            return Err(ProtocolError::UnsupportedVersion(version_tag));
        }
        // Validate canonical DID shape at the envelope boundary per
        // RFC-0871 §Adversary Analysis A7 + RFC-0010 v1.2 amendment.
        crate::validate_canonical_did(from_did.as_str())?;
        let mut envelope = Self {
            envelope_id: [0u8; 32], // placeholder; computed below
            version_tag,
            from_did,
            to_node_id,
            payload_kind,
            payload,
            authorization,
            nonce,
            expires_at_unix_ms,
        };
        envelope.envelope_id = compute_envelope_id(&envelope);
        Ok(envelope)
    }

    /// Verify-time wire-version gate (RFC-0871 §14.1).
    ///
    /// - Returns `Ok(())` if `self.version_tag == VERSION_TAG_V2`.
    /// - Returns `Err(ProtocolError::UnsupportedVersion(V1))` if V1 —
    ///   pre-cutover receipts are hard-rejected (no silent fallback).
    /// - Returns `Err(ProtocolError::UnsupportedVersion(observed))` for
    ///   any other value (forward-compat: unknown tags fail-closed).
    ///
    /// Wire-format break per §14.1; consumers must rebuild against V2.
    pub fn verify_version(&self) -> Result<(), ProtocolError> {
        match self.version_tag {
            VERSION_TAG_V2 => Ok(()),
            VERSION_TAG_V1 => Err(ProtocolError::UnsupportedVersion(VERSION_TAG_V1)),
            observed => Err(ProtocolError::UnsupportedVersion(observed)),
        }
    }

    /// True if `now_unix_ms > expires_at_unix_ms` (TTL exceeded).
    #[must_use]
    pub fn is_expired(&self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.expires_at_unix_ms
    }

    /// True if `now_unix_ms + max_ttl_secs * 1000 > expires_at_unix_ms`
    /// (TTL exceeds the per-node-type ceiling from `RouterAnnouncePayload`).
    #[must_use]
    pub fn exceeds_ttl_ceiling(&self, now_unix_ms: u64, max_ttl_secs: u64) -> bool {
        let max_expires = now_unix_ms.saturating_add(max_ttl_secs.saturating_mul(1000));
        self.expires_at_unix_ms > max_expires
    }
}

// Re-export WireDid at the envelope module boundary for downstream crates
// that already import `octo_protocol::WireDid`.
pub use octo_ident::WireDid as _WireDid;

/// Type alias for ergonomic imports: `octo_protocol::WireDid`.
pub type WireDidAlias = WireDid;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload_kind::IDENTITY_RESOLVE;
    use crate::recipient::RecipientRef;
    use octo_ident::DidCodec;

    fn sample_did_str(seed: u8) -> String {
        // Mint a canonical DID via the codec, then take the wire form.
        let mut pk = [0u8; 32];
        for (i, byte) in pk.iter_mut().enumerate() {
            *byte = seed.wrapping_add(i as u8);
        }
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        wire.as_str().to_owned()
    }

    #[test]
    fn build_assigns_envelope_id() {
        let did_str = sample_did_str(7);
        let did = octo_ident::WireDid::new(did_str);
        let env = NodeEnvelope::build(
            did,
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![0x01, 0x02, 0x03],
            vec![],
            [0xff; 32],
            1_735_689_600_000,
            VERSION_TAG_V2,
        )
        .unwrap();
        assert_ne!(env.envelope_id, [0u8; 32]);
    }

    #[test]
    fn build_rejects_non_canonical_did() {
        let did = octo_ident::WireDid::new("did:octo:bad".to_owned());
        let r = NodeEnvelope::build(
            did,
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![],
            vec![],
            [0; 32],
            1_735_689_600_000,
            VERSION_TAG_V2,
        );
        assert!(matches!(r, Err(ProtocolError::InvalidDid(_))));
    }

    #[test]
    fn is_expired_check() {
        let did_str = sample_did_str(9);
        let did = octo_ident::WireDid::new(did_str);
        let env = NodeEnvelope::build(
            did,
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![],
            vec![],
            [0; 32],
            1_735_689_600_000,
            VERSION_TAG_V2,
        )
        .unwrap();
        assert!(!env.is_expired(1_735_689_500_000));
        assert!(env.is_expired(1_735_689_600_000));
        assert!(env.is_expired(1_735_689_700_000));
    }

    #[test]
    fn ttl_ceiling_check() {
        let did_str = sample_did_str(11);
        let did = octo_ident::WireDid::new(did_str);
        let now = 1_735_689_500_000u64;
        // expires = now + 600s; ceiling = 300s → exceeded.
        let env = NodeEnvelope::build(
            did,
            RecipientRef::Direct([0x01; 32]),
            IDENTITY_RESOLVE,
            vec![],
            vec![],
            [0; 32],
            now + 600_000,
            VERSION_TAG_V2,
        )
        .unwrap();
        assert!(env.exceeds_ttl_ceiling(now, 300));
        assert!(!env.exceeds_ttl_ceiling(now, 600));
        assert!(!env.exceeds_ttl_ceiling(now, 1200));
    }
}
