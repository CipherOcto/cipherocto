//! Forward Envelope Builder (RFC-0855 §Wire Format)
//!
//! Forward envelope builder for peer-to-peer propagation.
//! Phase 13 G16b per RFC-0011-u §Substrate Mapping Table.
//! Operates on the in-memory snapshot; real network propagation
//! OUT OF SCOPE for Phase 13. Per-extension impl crates (Layer D)
//! provide real network adapters in follow-on missions.

/// Forward envelope for peer-to-peer propagation (RFC-0855 §Wire Format).
///
/// Phase 13 G16b per RFC-0011-u §Substrate Mapping Table. Operates
/// on the in-memory snapshot; real network propagation OUT OF
/// SCOPE for Phase 13. Per-extension impl crates (Layer D) provide
/// real network adapters in follow-on missions. Scalar field-order
/// determinism preserved per RFC-0011-h §Output Envelope
/// determinism (zero collection dependencies; `wire_bytes()`
/// returns a flat 80-byte scalar layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardEnvelope {
    source_envelope_id: [u8; 32],
    destination_peer_id: [u8; 32],
    ttl_epochs: u64,
    construction_epoch: u64,
}

/// Maximum TTL epochs accepted by `ForwardEnvelope::build`.
///
/// Phase 13 G16b substrate contract per RFC-0011-u §Substrate
/// extensions. TTL values exceeding this limit return
/// `ForwardEnvelopeError::TtlOverflow`. Set to a deliberately
/// bounded value (not `u64::MAX`) so the overflow path is
/// unit-testable.
pub const FORWARD_ENVELOPE_MAX_TTL_EPOCHS: u64 = u64::MAX / 2;

/// Forward envelope construction error.
///
/// Phase 13 G16b per RFC-0011-u §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForwardEnvelopeError {
    /// TTL epochs exceeds `FORWARD_ENVELOPE_MAX_TTL_EPOCHS`.
    TtlOverflow,
    /// Destination peer_id is all-zero (invalid).
    InvalidPeerId,
    /// Internal error (reserved for follow-on Layer D adapter
    /// missions).
    Internal(String),
}

impl ForwardEnvelope {
    /// Build a new `ForwardEnvelope` from explicit fields.
    ///
    /// Returns `Err(InvalidPeerId)` if `destination_peer_id` is
    /// all-zero. Returns `Err(TtlOverflow)` if `ttl_epochs`
    /// exceeds `FORWARD_ENVELOPE_MAX_TTL_EPOCHS`.
    ///
    /// `construction_epoch` is plumbed through the substrate API
    /// but caller-stubbed at 0 in this additive-type-only phase per
    /// RFC-0011-u §Substrate-faithfulness. The real epoch source
    /// (RFC-0855 §Wire Format) is OUT OF SCOPE for Phase 13 and
    /// lands in a follow-on Layer D adapter mission; until then
    /// CLI callers MUST pass `0` for this slot so the substrate
    /// contract remains substrate-faithful.
    pub fn build(
        source_envelope_id: [u8; 32],
        destination_peer_id: [u8; 32],
        ttl_epochs: u64,
        construction_epoch: u64,
    ) -> Result<Self, ForwardEnvelopeError> {
        if destination_peer_id == [0u8; 32] {
            return Err(ForwardEnvelopeError::InvalidPeerId);
        }
        if ttl_epochs > FORWARD_ENVELOPE_MAX_TTL_EPOCHS {
            return Err(ForwardEnvelopeError::TtlOverflow);
        }
        Ok(Self {
            source_envelope_id,
            destination_peer_id,
            ttl_epochs,
            construction_epoch,
        })
    }

    /// Serialize the envelope to canonical wire bytes per
    /// RFC-0855 §Wire Format canonical encoding. Layout:
    /// `source_envelope_id (32) || destination_peer_id (32) ||
    /// ttl_epochs_be (8) || construction_epoch_be (8)` = 80 bytes.
    /// Scalar field-order determinism preserved per RFC-0011-h
    /// §Output Envelope determinism (no HashMap/BTreeMap iteration;
    /// flat scalar concatenation).
    pub fn wire_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 32 + 8 + 8);
        out.extend_from_slice(&self.source_envelope_id);
        out.extend_from_slice(&self.destination_peer_id);
        out.extend_from_slice(&self.ttl_epochs.to_be_bytes());
        out.extend_from_slice(&self.construction_epoch.to_be_bytes());
        out
    }

    /// Source envelope_id accessor.
    pub fn source_envelope_id(&self) -> [u8; 32] {
        self.source_envelope_id
    }

    /// Destination peer_id accessor.
    pub fn destination_peer_id(&self) -> [u8; 32] {
        self.destination_peer_id
    }

    /// TTL epochs accessor.
    pub fn ttl_epochs(&self) -> u64 {
        self.ttl_epochs
    }

    /// Construction epoch accessor.
    pub fn construction_epoch(&self) -> u64 {
        self.construction_epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv_phase13_substrate_5_forward_envelope_build_succeeds() {
        let src = [0xAA; 32];
        let dst = [0xBB; 32];
        let fe = ForwardEnvelope::build(src, dst, 100, 1000).expect("build ok");
        assert_eq!(fe.source_envelope_id(), src);
        assert_eq!(fe.destination_peer_id(), dst);
        assert_eq!(fe.ttl_epochs(), 100);
        assert_eq!(fe.construction_epoch(), 1000);
    }

    #[test]
    fn tv_phase13_substrate_6_forward_envelope_invalid_peer_id_rejected() {
        let src = [0xAA; 32];
        let result = ForwardEnvelope::build(src, [0u8; 32], 100, 1000);
        assert_eq!(result.err(), Some(ForwardEnvelopeError::InvalidPeerId));
    }

    #[test]
    fn tv_phase13_substrate_7_forward_envelope_wire_bytes_canonical() {
        let src = [0xAA; 32];
        let dst = [0xBB; 32];
        let fe = ForwardEnvelope::build(src, dst, 100, 1000).expect("build ok");
        let bytes = fe.wire_bytes();
        // Layout: 32 + 32 + 8 + 8 = 80 bytes per RFC-0855 §Wire Format.
        assert_eq!(bytes.len(), 80);
        // Source envelope_id at offset 0..32.
        assert_eq!(&bytes[0..32], &src[..]);
        // Destination peer_id at offset 32..64.
        assert_eq!(&bytes[32..64], &dst[..]);
        // ttl_epochs_be at offset 64..72.
        assert_eq!(&bytes[64..72], &100u64.to_be_bytes());
        // construction_epoch_be at offset 72..80.
        assert_eq!(&bytes[72..80], &1000u64.to_be_bytes());
    }

    #[test]
    fn tv_phase13_substrate_8_forward_envelope_wire_bytes_deterministic() {
        let src = [0xAA; 32];
        let dst = [0xBB; 32];
        let fe1 = ForwardEnvelope::build(src, dst, 100, 1000).expect("build ok");
        let fe2 = ForwardEnvelope::build(src, dst, 100, 1000).expect("build ok");
        assert_eq!(fe1.wire_bytes(), fe2.wire_bytes());
    }

    #[test]
    fn tv_phase13_substrate_9_forward_envelope_ttl_overflow_rejected() {
        let src = [0xAA; 32];
        let dst = [0xBB; 32];
        // Boundary: ttl_epochs == FORWARD_ENVELOPE_MAX_TTL_EPOCHS accepted.
        let fe_max = ForwardEnvelope::build(src, dst, FORWARD_ENVELOPE_MAX_TTL_EPOCHS, 0)
            .expect("build at MAX_TTL ok");
        assert_eq!(fe_max.ttl_epochs(), FORWARD_ENVELOPE_MAX_TTL_EPOCHS);
        // Overflow: ttl_epochs > FORWARD_ENVELOPE_MAX_TTL_EPOCHS rejected.
        let overflow = FORWARD_ENVELOPE_MAX_TTL_EPOCHS + 1;
        let result = ForwardEnvelope::build(src, dst, overflow, 0);
        assert_eq!(result.err(), Some(ForwardEnvelopeError::TtlOverflow));
        // Verify TtlOverflow discriminant distinct from other variants.
        let err = ForwardEnvelopeError::TtlOverflow;
        assert_ne!(err, ForwardEnvelopeError::InvalidPeerId);
        let internal = ForwardEnvelopeError::Internal("x".to_string());
        assert_ne!(err, internal);
    }
}
