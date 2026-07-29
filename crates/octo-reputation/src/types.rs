//! Core reputation types — RFC-0968 §3, §21, §22, §10 amendments.
//!
//! All score fields are `octo_determin::Dfp` per RFC-0104. Wire form is the
//! 24-byte `DfpEncoding::from_dfp(&d).to_bytes()`.
//!
//! All identifiers (`RecorderDid`, `ControllerId`, `EventId`) are newtypes
//! over fixed-width byte arrays. Constructors validate length and refuse
//! malformed input. Cross-mission keying (gossip, slash-binding) uses these
//! canonical types — never raw `String` or coordinator public keys.

use octo_determin::{Dfp, DfpEncoding};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Identifier newtypes
// ---------------------------------------------------------------------------

/// 52-byte CID-style DID, e.g. `did:octo:b<base32>`. Validated by constructor.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecorderDid(#[serde(with = "hex::serde")] [u8; 52]);

impl RecorderDid {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::ReputationError> {
        if bytes.len() != 52 {
            return Err(crate::error::ReputationError::RecorderDidMalformed(
                "recorder did must be 52 bytes",
            ));
        }
        let mut arr = [0u8; 52];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    pub const fn from_array(arr: [u8; 52]) -> Self {
        Self(arr)
    }

    pub fn as_bytes(&self) -> &[u8; 52] {
        &self.0
    }

    /// Translate to canonical wire form (`did:octo:z<base58btc>` per
    /// RFC-0010 §Specification §`raw_to_wire`).
    ///
    /// **Round 2 review C3 — pending RFC-0968 §2 vs RFC-0010 reconcile.**
    /// RFC-0968 §2 mandates `did:octo:b<52-char base32-lowercase-no-padding>`
    /// over `blake3(pubkey)` (the §2 Round 2 finding M15 explicitly
    /// REJECTED the `z`/base58btc multibase prefix in favour of `b`/
    /// base32-lowercase). The implementation takes the RFC-0010 §
    /// path. The two RFCs are incompatible on the wire form. Until
    /// the reconcile amendment lands, the wire form emitted here is
    /// the RFC-0010 form (`did:octo:z<base58btc>`); cross-protocol
    /// bridges that interpret `did:octo:` per RFC-0968 §2 will need
    /// a translation layer. The on-disk raw bytes (`self.0`) are
    /// 52 bytes of `blake3(pubkey) || [0;20]`; that part does NOT
    /// change between the two RFCs and is the stable interop key.
    /// See RFC-0968-A2 amendment proposal (post-C3 review).
    pub fn to_wire(&self) -> Result<String, crate::error::ReputationError> {
        use octo_ident::{CanonicalCodec, RawDid};
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&self.0[..32]);
        let mut disc = [0u8; 20];
        disc.copy_from_slice(&self.0[32..]);
        let raw = RawDid {
            hash,
            version_discriminator: disc,
        };
        let wire = CanonicalCodec::raw_to_wire(&raw).map_err(|_e| {
            crate::error::ReputationError::RecorderDidMalformed("did wire encode failed")
        })?;
        Ok(wire.as_str().to_owned())
    }
}

impl std::fmt::Debug for RecorderDid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RecorderDid({})", hex::encode(self.0))
    }
}

/// 32-byte controller identifier, e.g. `blake3(governance_pubkey)` per
/// RFC-0968 amendment 44 default. Coalesces candidates per election.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ControllerId(#[serde(with = "hex::serde")] [u8; 32]);

impl ControllerId {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::ReputationError> {
        if bytes.len() != 32 {
            return Err(crate::error::ReputationError::RecorderDidMalformed(
                "controller id must be 32 bytes",
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    pub const fn from_array(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for ControllerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ControllerId({})", hex::encode(self.0))
    }
}

/// 64-bit monotonically-increasing event identifier. Storage layer assigns;
/// callers MUST NOT mint these.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(#[serde(with = "hex::serde")] [u8; 8]);

impl EventId {
    pub const fn from_u64(value: u64) -> Self {
        Self(value.to_be_bytes())
    }

    pub const fn to_u64(self) -> u64 {
        u64::from_be_bytes(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl std::fmt::Debug for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EventId({})", self.to_u64())
    }
}

/// 64-bit recorder identifier (database primary key).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecorderId(#[serde(with = "hex::serde")] [u8; 8]);

impl RecorderId {
    pub const fn from_u64(value: u64) -> Self {
        Self(value.to_be_bytes())
    }

    pub const fn to_u64(self) -> u64 {
        u64::from_be_bytes(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl std::fmt::Debug for RecorderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RecorderId({})", self.to_u64())
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Type of signal being recorded. RFC-0968 §3.
///
/// **Round 2 review C4 — pending RFC-0968-A2 realignment.** The
/// canonical RFC-0968 §10 + RFC-0955-R1 signal-kind set is
/// `{Slash, Outcome, Latency, Capacity, Discovery, Rotation}` —
/// the implementation declares `{Outcome, Latency, Slash,
/// Suspension, Anchor, CrossLayer}` because the mission-critical
/// consumers (the slash store, the anchor job, mission 0855p-c
/// cross-domain reputation) predate the §10 canonicalisation and
/// carry their own variant names. The discriminant values are
/// STABLE for the existing consumers; renaming risks
/// cross-replica protocol drift until every consumer is migrated in
/// lockstep. Pending an explicit RFC-0968-A2 amendment that
/// simultaneously:
/// - renames each variant to the §10 canonical name,
/// - maintains `discriminant()` backwards-compat for the legacy
///   code, AND
/// - bumps the migration slot for `kind_weights` (RFC-0955-R1)
///   to read the new column.
///   **Do NOT change `discriminant()` values until RFC-0968-A2 lands.**
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalKind {
    Outcome = 0x01,
    Latency = 0x02,
    Slash = 0x03,
    Suspension = 0x04,
    Anchor = 0x05,
    CrossLayer = 0x06,
}

impl SignalKind {
    pub const fn discriminant(self) -> u8 {
        self as u8
    }

    pub fn from_discriminant(d: u8) -> Result<Self, crate::error::ReputationError> {
        Ok(match d {
            0x01 => Self::Outcome,
            0x02 => Self::Latency,
            0x03 => Self::Slash,
            0x04 => Self::Suspension,
            0x05 => Self::Anchor,
            0x06 => Self::CrossLayer,
            other => return Err(crate::error::ReputationError::SignalKindInvalid(other)),
        })
    }
}

/// Layer in which a signal is recorded. RFC-0968 §3 + amendment.
///
/// **Round 2 review C4 — pending RFC-0968-A2 realignment.** The
/// canonical RFC-0968 §10 + RFC-0955-R1 layer set is `{Mon, Dc,
/// Marketplace, TaskMarket, Retrieval, ProofMarket}`. The
/// implementation declares `{Consensus, Market, Coordinator,
/// Slash, Governance}` because the canonical names overlap with
/// the MON/DCN mission-layer modules (`crates/octo-network/src/mon/`
/// for `Mon`, `crates/octo-network/src/dc/` for `Dc`) and the
/// implementation chose non-overlapping names to keep module paths
/// distinct. Mission 0855p-c wires use `ReputationLayer::Coordinator`
/// (see `crates/octo-network/src/reputation/dc_store.rs:48, 193`)
/// because the DC layer maps onto `Coordinator` in the
/// implementation's vocabulary, NOT the canonical RFC name `Dc`.
///
/// **Same migration gate as `SignalKind`:** do NOT rename
/// discriminants until RFC-0968-A2 lands with a coordinated
/// migration. The current discriminant values are stable for
/// existing consumers.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReputationLayer {
    Consensus = 0x01,
    Market = 0x02,
    Coordinator = 0x03,
    Slash = 0x04,
    Governance = 0x05,
}

impl ReputationLayer {
    pub const fn discriminant(self) -> u8 {
        self as u8
    }

    pub fn from_discriminant(d: u8) -> Result<Self, crate::error::ReputationError> {
        Ok(match d {
            0x01 => Self::Consensus,
            0x02 => Self::Market,
            0x03 => Self::Coordinator,
            0x04 => Self::Slash,
            0x05 => Self::Governance,
            other => return Err(crate::error::ReputationError::ReputationLayerInvalid(other)),
        })
    }
}

// ---------------------------------------------------------------------------
// Event + aggregate
// ---------------------------------------------------------------------------

/// A single reputation signal. Persisted as one row in `reputation_events`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalEvent {
    pub event_id: EventId,
    pub recorder_did: RecorderDid,
    pub controller_id: ControllerId,
    pub signal_kind: SignalKind,
    pub layer: ReputationLayer,
    /// `octo_determin::Dfp`. Wire form: 24-byte `DfpEncoding`.
    pub score_delta: Dfp,
    /// Unix seconds. Set by storage layer.
    pub recorded_at_unix: u64,
    /// Optional. Tombstoned DIDs MUST carry provenance before replay.
    pub rotation_provenance: Option<RotationProvenance>,
    /// Optional. Audit metadata (signatures, references).
    pub audit_ref: Option<Vec<u8>>,
    /// Optional. Anchor tx hash (32-byte BLAKE3) populated by
    /// `ReputationStore::anchor_pending` once the event is committed to
    /// the anchoring chain (RFC-0955-R1). `None` until the anchor job
    /// runs and writes back via `set_event_anchor_tx_hash`.
    pub anchor_tx_hash: Option<[u8; 32]>,
}

impl SignalEvent {
    /// Test-only constructor; never use in production.
    #[cfg(test)]
    pub fn dummy_for_test(seed: u64, ts: u64, score: f64) -> Self {
        Self {
            event_id: EventId::from_u64(seed),
            recorder_did: RecorderDid::from_array([0u8; 52]),
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(score),
            recorded_at_unix: ts,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        }
    }

    /// Canonical byte serialisation for digest computation.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + 52 + 32 + 1 + 1 + 24 + 8 + 1 + 32);
        buf.extend_from_slice(self.event_id.as_bytes());
        buf.extend_from_slice(self.recorder_did.as_bytes());
        buf.extend_from_slice(self.controller_id.as_bytes());
        buf.push(self.signal_kind.discriminant());
        buf.push(self.layer.discriminant());
        buf.extend_from_slice(&DfpEncoding::from_dfp(&self.score_delta).to_bytes());
        buf.extend_from_slice(&self.recorded_at_unix.to_be_bytes());
        match &self.rotation_provenance {
            None => buf.push(0),
            Some(rp) => {
                buf.push(1);
                buf.extend_from_slice(rp.new_did.as_bytes());
                buf.extend_from_slice(&rp.consumed_at_unix.to_be_bytes());
                buf.extend_from_slice(&rp.rotation_id.to_be_bytes());
            }
        }
        // Neither `anchor_tx_hash` NOR `audit_ref` is included in
        // `canonical_bytes` — both are post-event sidecars that
        // mutate independently of the event envelope digest, so
        // folding them into the digest would break federation /
        // audit replay stability. The anchor provenance lives on
        // `reputation_anchors` + `query_anchors_by_controller_id`;
        // the audit metadata is intentionally opaque to peers
        // that don't have the same audit ACL. Cross-replica wire-
        // format stability is preserved for the pre-RFC-0955-R1
        // gossip + audit contract. Future RFC-0968-A2 amendment
        // can fold either field into a version-tagged preimage
        // (`0x00` legacy, `0x01` anchor-aware, `0x02` audit-
        // aware). For now both fields exist on the struct so
        // callers can carry the values, but neither participates
        // in digest computation. (Round 1 review F3; doc tightened
        // in Round 4 audit-ref nit.)
        buf
    }
}

/// Per-(did, kind, layer) aggregate row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationAggregate {
    pub recorder_did: RecorderDid,
    pub signal_kind: SignalKind,
    pub layer: ReputationLayer,
    pub score_ewma: Dfp,
    pub samples: u64,
    pub severity_total: u64,
    pub last_event_id: EventId,
    pub last_event_unix: u64,
    pub updated_at_unix: u64,
}

// ---------------------------------------------------------------------------
// Dfp BLOB helpers
// ---------------------------------------------------------------------------

/// Encode a `Dfp` as the canonical 24-byte wire form (RFC-0968 §22 +
/// RFC-0104). Every persistence layer that writes a `score_ewma` /
/// `score_delta` BLOB column uses this function.
pub fn dfp_to_blob(d: &Dfp) -> [u8; 24] {
    DfpEncoding::from_dfp(d).to_bytes()
}

/// Decode a 24-byte BLOB back into a `Dfp`. Returns
/// `ReputationError::ScoreEncodingInvalid` on length mismatch. The wire
/// form is bit-deterministic so a malformed blob indicates corruption,
/// not a recoverable state.
pub fn dfp_from_blob(bytes: &[u8]) -> Result<Dfp, crate::error::ReputationError> {
    if bytes.len() != 24 {
        return Err(crate::error::ReputationError::ScoreEncodingInvalid);
    }
    let mut arr = [0u8; 24];
    arr.copy_from_slice(bytes);
    Ok(DfpEncoding::from_bytes(arr).to_dfp())
}

impl ReputationAggregate {
    /// Test-only constructor.
    #[cfg(test)]
    pub fn dummy_for_test(seed: u64, ts: u64, score: f64, samples: u64) -> Self {
        Self {
            recorder_did: RecorderDid::from_array([0u8; 52]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_ewma: Dfp::from_f64(score),
            samples,
            severity_total: 0,
            last_event_id: EventId::from_u64(seed),
            last_event_unix: ts,
            updated_at_unix: ts,
        }
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(52 + 1 + 1 + 24 + 8 + 8 + 8 + 8 + 8);
        buf.extend_from_slice(self.recorder_did.as_bytes());
        buf.push(self.signal_kind.discriminant());
        buf.push(self.layer.discriminant());
        buf.extend_from_slice(&DfpEncoding::from_dfp(&self.score_ewma).to_bytes());
        buf.extend_from_slice(&self.samples.to_be_bytes());
        buf.extend_from_slice(&self.severity_total.to_be_bytes());
        buf.extend_from_slice(self.last_event_id.as_bytes());
        buf.extend_from_slice(&self.last_event_unix.to_be_bytes());
        buf.extend_from_slice(&self.updated_at_unix.to_be_bytes());
        buf
    }
}

// ---------------------------------------------------------------------------
// Provenance + retirement
// ---------------------------------------------------------------------------

/// Provenance record for a DID rotation. Required for replaying events on
/// tombstoned DIDs (RFC-0968 amendment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationProvenance {
    pub new_did: RecorderDid,
    pub consumed_at_unix: u64,
    pub rotation_id: u64,
}

/// Per-adapter evidence used to declare retirement eligibility (Phase 2.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParityEvidence {
    /// Adapter for which retirement is being declared.
    pub adapter: u8,
    /// Per-(did, kind, layer) match rate across the rolling 24h window.
    pub parity_score: u32, // basis points × 10000 (e.g. 9999 = 0.9999)
    /// Number of distinct (did, kind, layer) triples observed in window.
    pub bucket_count: u64,
    /// Earliest bucket boundary Unix seconds.
    pub first_bucket_unix: u64,
    /// Latest bucket boundary Unix seconds.
    pub last_bucket_unix: u64,
    /// `BLAKE3(BLAKE3_REPUTATION_PARITY_DOMAIN || adapter || parity_score_be ||
    /// bucket_count_be || first_bucket_unix_be || last_bucket_unix_be)`.
    pub evidence_hash: [u8; 32],
}

/// Outcome of `declare_retirement_eligible` — succeeds only when the
/// governance proof is well-formed and the parity threshold holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetirementEligibility {
    pub eligible: bool,
    pub since_unix: u64,
    pub evidence_hash: [u8; 32],
    pub adapter: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_kind_round_trip() {
        for k in [
            SignalKind::Outcome,
            SignalKind::Latency,
            SignalKind::Slash,
            SignalKind::Suspension,
            SignalKind::Anchor,
            SignalKind::CrossLayer,
        ] {
            let d = k.discriminant();
            assert_eq!(SignalKind::from_discriminant(d).unwrap(), k);
        }
    }

    #[test]
    fn reputation_layer_round_trip() {
        for l in [
            ReputationLayer::Consensus,
            ReputationLayer::Market,
            ReputationLayer::Coordinator,
            ReputationLayer::Slash,
            ReputationLayer::Governance,
        ] {
            let d = l.discriminant();
            assert_eq!(ReputationLayer::from_discriminant(d).unwrap(), l);
        }
    }

    #[test]
    fn signal_kind_invalid_discriminant_returns_err() {
        let err = SignalKind::from_discriminant(0xFF).unwrap_err();
        assert_eq!(err.discriminant(), 0x01);
    }

    #[test]
    fn reputation_layer_invalid_discriminant_returns_err() {
        let err = ReputationLayer::from_discriminant(0xFF).unwrap_err();
        assert_eq!(err.discriminant(), 0x02);
    }

    #[test]
    fn recorder_did_rejects_wrong_length() {
        let err = RecorderDid::from_bytes(&[0u8; 51]).unwrap_err();
        assert_eq!(err.discriminant(), 0x05);
    }

    #[test]
    fn controller_id_rejects_wrong_length() {
        let err = ControllerId::from_bytes(&[0u8; 31]).unwrap_err();
        assert_eq!(err.discriminant(), 0x05);
    }

    #[test]
    fn event_id_be_round_trip() {
        let id = EventId::from_u64(0xDEADBEEF_CAFEBABE);
        assert_eq!(id.to_u64(), 0xDEADBEEF_CAFEBABE);
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let e1 = SignalEvent::dummy_for_test(7, 1_700_000_000, 0.75);
        let e2 = SignalEvent::dummy_for_test(7, 1_700_000_000, 0.75);
        assert_eq!(e1.canonical_bytes(), e2.canonical_bytes());
    }

    /// Round 3 review F4: pin the canonical-bytes length. The pre-F3
    /// canonical form was 127 bytes (event_id 8 + recorder_did 52 +
    /// controller_id 32 + signal_kind 1 + layer 1 + score_delta 24 +
    /// recorded_at_unix 8 + rotation_provenance tag 1). The
    /// anchor_tx_hash field is intentionally NOT included in the
    /// preimage (Round 1 review F3 fix). A future drift that
    /// accidentally folds anchor_tx_hash into the digest is caught
    /// here.
    #[test]
    fn canonical_bytes_length_is_127_for_unanchored_event() {
        let e = SignalEvent::dummy_for_test(0, 1_000_000, 0.5);
        // rotation_provenance: None → 1 byte tag, no body.
        // anchor_tx_hash: excluded by design (Round 1 F3 fix).
        // audit_ref: also excluded by design (pre-existing).
        assert_eq!(
            e.canonical_bytes().len(),
            127,
            "canonical_bytes length drifted from pre-anchor baseline"
        );
    }
}
