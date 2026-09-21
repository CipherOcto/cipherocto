//! Envelope Inspector (RFC-0855 §Wire Format)
//!
//! Read-only envelope metadata inspector. Phase 13 G16a per
//! RFC-0011-u §Substrate Mapping Table. Operates on the
//! in-memory snapshot; live envelope store OUT OF SCOPE for
//! Phase 13. Per-extension impl crates (Layer D) provide real
//! envelope stores in follow-on missions.

/// Read-only inspector of envelope metadata (RFC-0855 §Wire Format).
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table. Operates
/// on the in-memory snapshot; live envelope store OUT OF SCOPE for
/// Phase 13. Per-extension impl crates (Layer D) provide real
/// envelope stores in follow-on missions. BTreeMap-based
/// deterministic iteration ordering preserved per RFC-0011-h
/// §Output Envelope determinism.
#[derive(Clone, Debug, Default)]
pub struct EnvelopeInspector;

/// Envelope metadata returned by `inspect()`.
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeMeta {
    /// 32-byte envelope_id.
    pub envelope_id: [u8; 32],
    /// Envelope kind discriminator.
    pub envelope_kind: EnvelopeKind,
    /// Creator DID hex (canonicalized via substrate DID format).
    pub creator_did_hex: String,
    /// Creation epoch (epoch when envelope was constructed).
    pub creation_epoch: u64,
    /// TTL epochs (time-to-live in epochs).
    pub ttl_epochs: u64,
}

/// Envelope kind discriminator (RFC-0855 §Wire Format envelope kinds).
///
/// Phase 13 G16a per RFC-0011-u §Substrate Mapping Table. Additive
/// enum on the NEW module per Phase 7 RFC-0011-o precedent; no
/// central edit per [[cipherocto-design-principles]] §Extension over
/// enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeKind {
    /// Mission-scoped envelope.
    Mission,
    /// Governance envelope.
    Governance,
    /// Reputation envelope.
    Reputation,
    /// Slash envelope.
    Slash,
    /// Forward envelope (peer-to-peer propagation).
    Forward,
    /// Bootstrap envelope.
    Bootstrap,
    /// Discovery envelope.
    Discovery,
    /// Coordinator envelope.
    Coordinator,
}

impl EnvelopeKind {
    /// String label for the envelope kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mission => "mission",
            Self::Governance => "governance",
            Self::Reputation => "reputation",
            Self::Slash => "slash",
            Self::Forward => "forward",
            Self::Bootstrap => "bootstrap",
            Self::Discovery => "discovery",
            Self::Coordinator => "coordinator",
        }
    }
}

impl EnvelopeInspector {
    /// Inspect an envelope by 32-byte envelope_id; returns `None`
    /// for unknown envelopes (per RFC-0011-u §Envelope Inspect
    /// semantics). Phase 13 returns None unconditionally (live
    /// envelope store OUT OF SCOPE for Phase 13).
    pub fn inspect(&self, envelope_id: [u8; 32]) -> Option<EnvelopeMeta> {
        // Phase 13 additive-type-only: real envelope store wiring
        // OUT OF SCOPE; this stub returns None for unknown envelopes
        // per RFC-0011-u §Envelope Inspect semantics.
        let _ = envelope_id;
        None
    }

    /// Construct a synthetic EnvelopeMeta from explicit fields
    /// (substrate-faithful projection path for CLI dispatch to
    /// exercise the type surface end-to-end).
    pub fn from_fields(
        envelope_id: [u8; 32],
        envelope_kind: EnvelopeKind,
        creator_did_hex: String,
        creation_epoch: u64,
        ttl_epochs: u64,
    ) -> EnvelopeMeta {
        EnvelopeMeta {
            envelope_id,
            envelope_kind,
            creator_did_hex,
            creation_epoch,
            ttl_epochs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv_phase13_substrate_1_envelope_inspector_default_constructs() {
        let insp = EnvelopeInspector;
        // Default-constructed inspector returns None for any envelope_id
        // (Phase 13 stub behavior per RFC-0011-u §Envelope Inspect).
        assert!(insp.inspect([0u8; 32]).is_none());
        assert!(insp.inspect([0xAA; 32]).is_none());
    }

    #[test]
    fn tv_phase13_substrate_2_envelope_meta_from_fields_roundtrip() {
        let mid = [0xCC; 32];
        let meta = EnvelopeInspector::from_fields(
            mid,
            EnvelopeKind::Mission,
            "did:octo:0xabcdef".to_string(),
            1000,
            50,
        );
        assert_eq!(meta.envelope_id, mid);
        assert_eq!(meta.envelope_kind, EnvelopeKind::Mission);
        assert_eq!(meta.creator_did_hex, "did:octo:0xabcdef");
        assert_eq!(meta.creation_epoch, 1000);
        assert_eq!(meta.ttl_epochs, 50);
    }

    #[test]
    fn tv_phase13_substrate_3_envelope_kind_as_str_distinct() {
        // All 8 variants have distinct string labels.
        let kinds = [
            EnvelopeKind::Mission,
            EnvelopeKind::Governance,
            EnvelopeKind::Reputation,
            EnvelopeKind::Slash,
            EnvelopeKind::Forward,
            EnvelopeKind::Bootstrap,
            EnvelopeKind::Discovery,
            EnvelopeKind::Coordinator,
        ];
        let labels: std::collections::BTreeSet<&'static str> =
            kinds.iter().map(|k| k.as_str()).collect();
        assert_eq!(labels.len(), 8);
        assert!(labels.contains("mission"));
        assert!(labels.contains("governance"));
        assert!(labels.contains("reputation"));
        assert!(labels.contains("slash"));
        assert!(labels.contains("forward"));
        assert!(labels.contains("bootstrap"));
        assert!(labels.contains("discovery"));
        assert!(labels.contains("coordinator"));
    }

    #[test]
    fn tv_phase13_substrate_4_envelope_meta_partial_eq() {
        let meta_a = EnvelopeInspector::from_fields(
            [0xAA; 32],
            EnvelopeKind::Mission,
            "did:octo:0x01".to_string(),
            100,
            50,
        );
        let meta_b = EnvelopeInspector::from_fields(
            [0xAA; 32],
            EnvelopeKind::Mission,
            "did:octo:0x01".to_string(),
            100,
            50,
        );
        let meta_c = EnvelopeInspector::from_fields(
            [0xAA; 32],
            EnvelopeKind::Governance,
            "did:octo:0x01".to_string(),
            100,
            50,
        );
        assert_eq!(meta_a, meta_b);
        assert_ne!(meta_a, meta_c);
    }
}
