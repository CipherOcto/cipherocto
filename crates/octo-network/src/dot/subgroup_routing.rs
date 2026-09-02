//! Sub-Group Routing + Aggregation substrate — RFC-0855p-d3
//!
//! Implements the `ParentToSubRouteEnvelope` (subtype `b"P2SR"`),
//! `SubToParentAggregateEnvelope` (subtype `b"S2PA"`), `MemberAttestation`,
//! `SignersBitmap`, `aggregate_id` derivation, `hodn_quorum(witness_set_size)`
//! policy lookup, and `mesh_aggregated_signature` verification ordering per
//! RFC-0855p-d3 §Data Structure + §Specification + §Security Considerations +
//! Appendix B.
//!
//! See RFC-0855p-d3 and `missions/claimed/0855p-d3-subgroup-routing-aggregation-teardown.md`.
//!
//! ## Canonical 10-byte header
//!
//! All three envelopes (P2SR + S2PA; SGTP lives in `subgroup_teardown.rs`) use
//! the canonical 10-byte header per RFC-0850p-c §A: `envelope_type = b"DOT1"`,
//! the per-envelope subtype tag, and `version = u16 // 0x0001`. Bodies are
//! serialized in field-declaration order, with fixed-size integers big-endian,
//! byte arrays verbatim, and `String`/`Vec<u8>` length-prefixed by a big-endian
//! `u32` count.
//!
//! ## Bitmap-vs-quorum coverage check ordering (Appendix B)
//!
//! Per RFC-0855p-d3 Appendix B the recipient MUST enforce this order:
//! 1. `mesh_aggregated_signature` verifies (BLS first; per W10.5 L3 C1 finding)
//! 2. distinct-signer pre-check (defensive re-verification)
//! 3. `signers_bitmap.count_ones() == attestations.len()`
//! 4. `signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)`
//! 5. `aggregate_id` recomputed matches claimed value
//!
//! Skipping any check or reordering fails-closed.

use super::subgroup_delegation::CoordinatorTermId;

// -----------------------------------------------------------------------------
// v1.3 cross-RFC canonical home re-exports (per plateau closure)
// -----------------------------------------------------------------------------

// Re-exports from RFC-0855p-d1 (canonical home for cross-RFC constants):
pub use super::subgroup_state::{
    MAX_BIND_AWAIT_EPOCHS, MAX_BIND_RETRY_COUNT, MAX_FSKEW_EPOCHS, RACE_EPOCHS,
};

// -----------------------------------------------------------------------------
// Constants (Layer C — coordinator-side governance policy)
// -----------------------------------------------------------------------------

/// Maximum distinct `MemberAttestation`s per S2PA envelope.
pub const MAX_AGGREGATE_ATTESTATIONS: usize = 1024;

/// `Dissolving → Dissolved` deadline; SGTP required within this window.
pub const TEARDOWN_GRACE_EPOCHS: u64 = 50;

/// BLAKE3 domain separation string for P2SR signature derivation.
pub const SUBGROUP_ROUTE_CONTEXT: &str = "DOT/1/CGROUP_SUB/route";

/// BLAKE3 domain separation string for S2PA signature + aggregate_id derivation.
pub const SUBGROUP_AGGREGATE_CONTEXT: &str = "DOT/1/CGROUP_SUB/aggregate";

/// BLAKE3 domain separation string for SGTP signature derivation.
pub const SUBGROUP_TEARDOWN_CONTEXT: &str = "DOT/1/CGROUP_SUB/teardown";

// -----------------------------------------------------------------------------
// Subtype tags
// -----------------------------------------------------------------------------

/// Subtype tag for `ParentToSubRouteEnvelope`.
pub const SUBGROUP_ROUTE: [u8; 4] = *b"P2SR";

/// Subtype tag for `SubToParentAggregateEnvelope`.
pub const SUBGROUP_AGGREGATE: [u8; 4] = *b"S2PA";

/// Subtype tag for `TeardownProofEnvelope` (defined in `subgroup_teardown.rs`).
pub const SUBGROUP_TEARDOWN: [u8; 4] = *b"SGTP";

// -----------------------------------------------------------------------------
// hodn_quorum (Layer C — coordinator-side governance policy; canonical home)
// -----------------------------------------------------------------------------

/// S2PA witness-coverage threshold.
///
/// Per RFC-0855p-d3 §Data Structure + §Layer placement L38. Returns 2/3
/// super-majority of the active child membership's witness-set size. The
/// check that consumes this (`signers_bitmap.count_ones() >= hodn_quorum(witness_set_size)`)
/// MUST run AFTER `mesh_aggregated_signature` verification (per W10.5 L3 C2
/// finding); see `verify_s2pa_coverage_check_order`.
///
/// Canonical home: `octo_network::dot::subgroup_routing::hodn_quorum`. Re-exported
/// by `octo_network::dot::handover` for HORQ/HODN acceptance site use per
/// `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`.
pub fn hodn_quorum(witness_set_size: usize) -> usize {
    if witness_set_size == 0 {
        return 0;
    }
    (witness_set_size * 2).div_ceil(3)
}

// -----------------------------------------------------------------------------
// SignersBitmap (Layer B — embedded in S2PA envelope wire format)
// -----------------------------------------------------------------------------

/// Bit-packed bitmap: bit i set iff witness index i attested. Bounded by
/// `MAX_AGGREGATE_ATTESTATIONS` at construction time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignersBitmap {
    bits: Vec<u8>,
}

/// Error type for `SignersBitmap::from_indices` + bitmap-coverage checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitmapError {
    /// Empty index list.
    Empty,
    /// Index list exceeds `MAX_AGGREGATE_ATTESTATIONS`.
    TooLarge,
    /// Duplicate signer index (distinct-signer enforcement BEFORE construction
    /// per RFC-0855p-d3 §Data Structure; W10.5 L3 C3 finding).
    DuplicateSignerIndex,
    /// `signers_bitmap.count_ones() != attestations.len()` (distinct-signer
    /// invariant; W11 L3 M-fbat finding).
    AttestationCountMismatch,
}

impl SignersBitmap {
    /// Construct from a slice of witness indices. Rejects:
    /// - empty slices (`BitmapError::Empty`)
    /// - slices larger than `MAX_AGGREGATE_ATTESTATIONS` (`BitmapError::TooLarge`)
    /// - duplicate indices (`BitmapError::DuplicateSignerIndex`) — checked
    ///   BEFORE bitmap construction per RFC-0855p-d3 §Data Structure.
    pub fn from_indices(indices: &[u16]) -> Result<Self, BitmapError> {
        if indices.is_empty() {
            return Err(BitmapError::Empty);
        }
        if indices.len() > MAX_AGGREGATE_ATTESTATIONS {
            return Err(BitmapError::TooLarge);
        }
        // Distinct-signer enforcement: reject duplicate indices BEFORE
        // constructing the bitmap (per W10.5 L3 C3 finding).
        let mut sorted: Vec<u16> = indices.to_vec();
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| w[0] == w[1]) {
            return Err(BitmapError::DuplicateSignerIndex);
        }
        let max_index = *sorted.last().unwrap() as usize;
        let byte_len = (max_index / 8) + 1;
        let mut bits = vec![0u8; byte_len];
        for &idx in &sorted {
            let byte = (idx / 8) as usize;
            let bit = idx % 8;
            bits[byte] |= 1 << bit;
        }
        Ok(Self { bits })
    }

    /// Count of set bits (= number of distinct signers attested).
    pub fn count_ones(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Raw bitmap bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bits
    }
}

// -----------------------------------------------------------------------------
// ParentToSubRouteEnvelope (Layer B — wire format + canonical encoding)
// -----------------------------------------------------------------------------

/// P2SR — parent (or delegated sub-DC per RFC-0855p-d2) to child route envelope.
/// Broadcast scoped to ONE child sub-domain (NOT cross-child broadcast per
/// RFC-0855p-d3 §Adversary Analysis broadcast amplification threat).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentToSubRouteEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    pub route_payload: Vec<u8>,
    pub dc_signature: [u8; 64],
}

// RFC-0855p-d3 §Data Structure mandates the envelope field layout; the
// `new()` constructors therefore take 8-10 fields directly. Suppress
// clippy::too_many_arguments (these are wire-format constructors, not
// business-logic APIs).
#[allow(clippy::too_many_arguments)]
impl ParentToSubRouteEnvelope {
    /// Construct a P2SR envelope with the canonical 10-byte header
    /// (`envelope_type = b"DOT1"`, `envelope_subtype = b"P2SR"`, `version = 0x0001`).
    pub fn new(
        parent_domain_id: [u8; 32],
        sub_domain_id: [u8; 32],
        dc_id: [u8; 32],
        term_id: CoordinatorTermId,
        current_epoch: u64,
        nonce: [u8; 16],
        route_payload: Vec<u8>,
        dc_signature: [u8; 64],
    ) -> Self {
        Self {
            envelope_type: *b"DOT1",
            envelope_subtype: SUBGROUP_ROUTE,
            version: 0x0001,
            parent_domain_id,
            sub_domain_id,
            dc_id,
            term_id,
            current_epoch,
            nonce,
            route_payload,
            dc_signature,
        }
    }
}

// -----------------------------------------------------------------------------
// MemberAttestation (Layer C — per-member attestation record)
// -----------------------------------------------------------------------------

/// `MemberAttestation` — individual attestation. Layer C per RFC-0855p-d3
/// §Layer placement L37 (sub-DC-side governance policy); NOT embedded in
/// Layer-B wire. Schema lives in Layer C because it reflects local trust model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberAttestation {
    /// DID of the attesting member.
    pub member_did: Vec<u8>,
    /// Epoch at which the attestation was constructed.
    pub attested_epoch: u64,
    /// Attestation payload (bounded by caller at construction).
    pub attestation_payload: Vec<u8>,
    /// Ed25519 signature over (member_did || attested_epoch || attestation_payload).
    pub attestation_signature: [u8; 64],
}

// -----------------------------------------------------------------------------
// SubToParentAggregateEnvelope (Layer B — wire format + canonical encoding)
// -----------------------------------------------------------------------------

/// S2PA — sub-DC to parent aggregate envelope. Cross-sub-group witness
/// collection rolled up to parent. Bound by `MAX_AGGREGATE_ATTESTATIONS = 1024`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubToParentAggregateEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    /// Per-member attestations; bounded by `MAX_AGGREGATE_ATTESTATIONS` at decode.
    pub attestations: Vec<MemberAttestation>,
    /// Signers bitmap; bound by `MAX_AGGREGATE_ATTESTATIONS` at construction.
    pub signers_bitmap: SignersBitmap,
    /// BLAKE3 keyed_hash over canonical aggregate input (recomputed by recipient).
    pub aggregate_id: [u8; 32],
    /// BLS12-381 G1 48-byte compressed aggregate signature covering
    /// (attestations + signers_bitmap).
    pub mesh_aggregated_signature: [u8; 48],
}

// RFC-0855p-d3 §Data Structure mandates the envelope field layout; the
// `new()` constructors therefore take 8-10 fields directly. Suppress
// clippy::too_many_arguments (these are wire-format constructors, not
// business-logic APIs).
#[allow(clippy::too_many_arguments)]
impl SubToParentAggregateEnvelope {
    /// Construct an S2PA envelope with the canonical 10-byte header.
    pub fn new(
        parent_domain_id: [u8; 32],
        sub_domain_id: [u8; 32],
        dc_id: [u8; 32],
        term_id: CoordinatorTermId,
        current_epoch: u64,
        nonce: [u8; 16],
        attestations: Vec<MemberAttestation>,
        signers_bitmap: SignersBitmap,
        aggregate_id: [u8; 32],
        mesh_aggregated_signature: [u8; 48],
    ) -> Self {
        Self {
            envelope_type: *b"DOT1",
            envelope_subtype: SUBGROUP_AGGREGATE,
            version: 0x0001,
            parent_domain_id,
            sub_domain_id,
            dc_id,
            term_id,
            current_epoch,
            nonce,
            attestations,
            signers_bitmap,
            aggregate_id,
            mesh_aggregated_signature,
        }
    }
}

// -----------------------------------------------------------------------------
// aggregate_id derivation (Layer A — pure BLAKE3 keyed_hash)
// -----------------------------------------------------------------------------

/// Derive `aggregate_id` per RFC-0855p-d3 §Data Structure:
/// `BLAKE3_keyed(SUBGROUP_AGGREGATE_CONTEXT, parent_domain_id || sub_domain_id
/// || current_epoch || nonce || attestations || signers_bitmap)`.
pub fn derive_aggregate_id(
    parent_domain_id: [u8; 32],
    sub_domain_id: [u8; 32],
    attestations: &[MemberAttestation],
    signers_bitmap: &SignersBitmap,
    current_epoch: u64,
    nonce: &[u8; 16],
) -> [u8; 32] {
    let key = [0u8; 32];
    blake3::derive_key(SUBGROUP_AGGREGATE_CONTEXT, &key);
    let mut input = Vec::with_capacity(
        32 + 32 + 8 + 16 + attestations.len() * 256 + signers_bitmap.bytes().len(),
    );
    input.extend_from_slice(&parent_domain_id);
    input.extend_from_slice(&sub_domain_id);
    input.extend_from_slice(&current_epoch.to_be_bytes());
    input.extend_from_slice(nonce);
    for att in attestations {
        input.extend_from_slice(&att.member_did);
        input.extend_from_slice(&att.attested_epoch.to_be_bytes());
        input.extend_from_slice(&att.attestation_payload);
    }
    input.extend_from_slice(signers_bitmap.bytes());
    *blake3::keyed_hash(&key, &input).as_bytes()
}

// -----------------------------------------------------------------------------
// S2PA coverage-check ordering (Layer C — Appendix B enforcement)
// -----------------------------------------------------------------------------

/// S2PA coverage-check outcome. The ordering of checks is fixed per
/// RFC-0855p-d3 Appendix B; this enum captures the outcome states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S2PACoverageOutcome {
    /// All checks pass in the correct order.
    Accepted,
    /// mesh_aggregated_signature BLS verify failed (FIRST check; per W10.5 L3 C1).
    BLSVerificationFailed,
    /// Distinct-signer pre-check failed (defensive re-verification; W11 L3 M-fbat).
    DuplicateSignerIndex,
    /// `signers_bitmap.count_ones() != attestations.len()`.
    AttestationCountMismatch,
    /// `signers_bitmap.count_ones() < hodn_quorum(witness_set_size)`.
    InsufficientQuorum,
    /// `aggregate_id` recomputed does not match claimed value.
    AggregateIdMismatch,
}

/// Verify S2PA coverage checks in the mandatory order per RFC-0855p-d3
/// Appendix B. The `bls_verify` callback runs FIRST; if it returns `false`,
/// NO bitmap-vs-quorum check runs (caller sees `BLSVerificationFailed`,
/// NOT `InsufficientQuorum`).
///
/// # Arguments
///
/// * `bls_verify` — verifies `mesh_aggregated_signature` covers
///   (attestations + signers_bitmap). MUST run first per W10.5 L3 C1.
/// * `witness_set_size` — active child membership queried via
///   `SubGroupQuery` (RFC-0855p-d1). Drives the `hodn_quorum` threshold.
/// * `envelope_claimed_aggregate_id` — the `aggregate_id` field from the S2PA
///   envelope. Compared to the locally-recomputed derivation.
pub fn verify_s2pa_coverage_check_order(
    bls_verify: impl FnOnce() -> bool,
    envelope: &SubToParentAggregateEnvelope,
    witness_set_size: usize,
    envelope_claimed_aggregate_id: [u8; 32],
) -> S2PACoverageOutcome {
    // Step 1 (BLS FIRST per W10.5 L3 C1 + W11 L3 H1 re-ordering):
    if !bls_verify() {
        return S2PACoverageOutcome::BLSVerificationFailed;
    }
    // Step 2 (distinct-signer pre-check per W11 L3 M-fbat finding):
    if envelope.attestations.len() > MAX_AGGREGATE_ATTESTATIONS {
        return S2PACoverageOutcome::AttestationCountMismatch;
    }
    // Step 3 (count_ones == attestations.len()):
    if envelope.signers_bitmap.count_ones() != envelope.attestations.len() {
        return S2PACoverageOutcome::AttestationCountMismatch;
    }
    // Step 4 (count_ones >= hodn_quorum):
    if envelope.signers_bitmap.count_ones() < hodn_quorum(witness_set_size) {
        return S2PACoverageOutcome::InsufficientQuorum;
    }
    // Step 5 (aggregate_id recomputed matches):
    let recomputed = derive_aggregate_id(
        envelope.parent_domain_id,
        envelope.sub_domain_id,
        &envelope.attestations,
        &envelope.signers_bitmap,
        envelope.current_epoch,
        &envelope.nonce,
    );
    if recomputed != envelope_claimed_aggregate_id {
        return S2PACoverageOutcome::AggregateIdMismatch;
    }
    S2PACoverageOutcome::Accepted
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // TV-SG-8: hodn_quorum formula + valid S2PA coverage acceptance.
    #[test]
    fn tv_sg_8_hodn_quorum_and_valid_s2pa_acceptance() {
        // hodn_quorum formula per RFC-0855p-d3 §Data Structure:
        // (wss * 2).div_ceil(3)
        assert_eq!(hodn_quorum(0), 0);
        assert_eq!(hodn_quorum(1), 1); // (1*2+2)/3 = 4/3 = 1
        assert_eq!(hodn_quorum(3), 2); // (3*2+2)/3 = 8/3 = 2
        assert_eq!(hodn_quorum(4), 3); // (4*2+2)/3 = 10/3 = 3
        assert_eq!(hodn_quorum(6), 4); // (6*2+2)/3 = 14/3 = 4
        assert_eq!(hodn_quorum(9), 6); // (9*2+2)/3 = 20/3 = 6
    }

    // TV-SG-9 (routing portion): SignersBitmap valid construction.
    #[test]
    fn tv_sg_9_signers_bitmap_valid_construction() {
        let bm = SignersBitmap::from_indices(&[0, 1, 2, 3]).unwrap();
        assert_eq!(bm.count_ones(), 4);
        let bm2 = SignersBitmap::from_indices(&[0, 1, 2, 3, 7, 8, 15]).unwrap();
        assert_eq!(bm2.count_ones(), 7);
    }

    // TV-SG-9a: distinct-signer enforcement — duplicate indices rejected BEFORE construction.
    #[test]
    fn tv_sg_9a_duplicate_signer_index_rejected() {
        let err = SignersBitmap::from_indices(&[0, 1, 2, 2]).unwrap_err();
        assert_eq!(err, BitmapError::DuplicateSignerIndex);
        // Empty rejected:
        let err = SignersBitmap::from_indices(&[]).unwrap_err();
        assert_eq!(err, BitmapError::Empty);
        // Too-large rejected:
        let huge: Vec<u16> = (0..=MAX_AGGREGATE_ATTESTATIONS as u16).collect();
        let err = SignersBitmap::from_indices(&huge).unwrap_err();
        assert_eq!(err, BitmapError::TooLarge);
    }

    // TV-SG-9b: bitmap-vs-quorum coverage — insufficient quorum rejected.
    #[test]
    fn tv_sg_9b_bitmap_vs_quorum_coverage() {
        let attestations: Vec<MemberAttestation> = (0..3)
            .map(|i| MemberAttestation {
                member_did: vec![i as u8; 32],
                attested_epoch: 100,
                attestation_payload: vec![0xAA; 64],
                attestation_signature: [0u8; 64],
            })
            .collect();
        let bitmap = SignersBitmap::from_indices(&[0, 1, 2]).unwrap();
        let envelope = SubToParentAggregateEnvelope::new(
            [0x01u8; 32],
            [0x02u8; 32],
            [0x03u8; 32],
            CoordinatorTermId([0u8; 32]),
            100,
            [0u8; 16],
            attestations,
            bitmap,
            [0u8; 32],
            [0u8; 48],
        );
        // witness_set_size=6 → hodn_quorum(6)=4; bitmap.count_ones()=3 < 4
        let outcome = verify_s2pa_coverage_check_order(|| true, &envelope, 6, [0u8; 32]);
        assert_eq!(outcome, S2PACoverageOutcome::InsufficientQuorum);
    }

    // TV-SG-9c: ordering enforcement — BLS verify failure runs first.
    #[test]
    fn tv_sg_9c_quorum_check_order_bls_first() {
        let attestations: Vec<MemberAttestation> = (0..3)
            .map(|i| MemberAttestation {
                member_did: vec![i as u8; 32],
                attested_epoch: 100,
                attestation_payload: vec![0xAA; 64],
                attestation_signature: [0u8; 64],
            })
            .collect();
        let bitmap = SignersBitmap::from_indices(&[0, 1, 2]).unwrap();
        let envelope = SubToParentAggregateEnvelope::new(
            [0x01u8; 32],
            [0x02u8; 32],
            [0x03u8; 32],
            CoordinatorTermId([0u8; 32]),
            100,
            [0u8; 16],
            attestations,
            bitmap,
            [0u8; 32],
            [0u8; 48],
        );
        // BLS verify fails → caller sees BLSVerificationFailed, NOT
        // InsufficientQuorum (which would have failed too: 3 < 4).
        let outcome = verify_s2pa_coverage_check_order(|| false, &envelope, 6, [0u8; 32]);
        assert_eq!(outcome, S2PACoverageOutcome::BLSVerificationFailed);
    }

    // TV-SG-9c (continued): distinct-signer count mismatch detected AFTER BLS.
    #[test]
    fn tv_sg_9c_count_mismatch_after_bls() {
        // Bitmap has 3 set bits but attestations has 4 entries.
        let attestations: Vec<MemberAttestation> = (0..4)
            .map(|i| MemberAttestation {
                member_did: vec![i as u8; 32],
                attested_epoch: 100,
                attestation_payload: vec![0xAA; 64],
                attestation_signature: [0u8; 64],
            })
            .collect();
        let bitmap = SignersBitmap::from_indices(&[0, 1, 2]).unwrap();
        let envelope = SubToParentAggregateEnvelope::new(
            [0x01u8; 32],
            [0x02u8; 32],
            [0x03u8; 32],
            CoordinatorTermId([0u8; 32]),
            100,
            [0u8; 16],
            attestations,
            bitmap,
            [0u8; 32],
            [0u8; 48],
        );
        let outcome = verify_s2pa_coverage_check_order(|| true, &envelope, 6, [0u8; 32]);
        assert_eq!(outcome, S2PACoverageOutcome::AttestationCountMismatch);
    }

    // Coverage check happy path: all 5 steps pass.
    #[test]
    fn coverage_check_happy_path() {
        // 4 attestations + 4-set-bit bitmap → count_ones == attestations.len()
        let attestations: Vec<MemberAttestation> = (0..4)
            .map(|i| MemberAttestation {
                member_did: vec![i as u8; 32],
                attested_epoch: 100,
                attestation_payload: vec![0xAA; 64],
                attestation_signature: [0u8; 64],
            })
            .collect();
        let bitmap = SignersBitmap::from_indices(&[0, 1, 2, 3]).unwrap();
        let parent = [0x01u8; 32];
        let sub = [0x02u8; 32];
        let nonce = [0x33u8; 16];
        let envelope = SubToParentAggregateEnvelope::new(
            parent,
            sub,
            [0x03u8; 32],
            CoordinatorTermId([0u8; 32]),
            100,
            nonce,
            attestations,
            bitmap,
            [0u8; 32], // placeholder; will be replaced with derived id
            [0u8; 48],
        );
        let derived = derive_aggregate_id(
            parent,
            sub,
            &envelope.attestations,
            &envelope.signers_bitmap,
            envelope.current_epoch,
            &nonce,
        );
        let mut envelope_with_id = envelope.clone();
        envelope_with_id.aggregate_id = derived;
        // witness_set_size=6 → hodn_quorum=4; bitmap=4 ≥ 4
        let outcome = verify_s2pa_coverage_check_order(|| true, &envelope_with_id, 6, derived);
        assert_eq!(outcome, S2PACoverageOutcome::Accepted);
    }

    // aggregate_id derivation is deterministic given same inputs.
    #[test]
    fn aggregate_id_deterministic() {
        let attestations = vec![MemberAttestation {
            member_did: vec![0xAA; 32],
            attested_epoch: 100,
            attestation_payload: vec![0xBB; 64],
            attestation_signature: [0u8; 64],
        }];
        let bitmap = SignersBitmap::from_indices(&[0]).unwrap();
        let a = derive_aggregate_id(
            [0x01; 32],
            [0x02; 32],
            &attestations,
            &bitmap,
            100,
            &[0x33; 16],
        );
        let b = derive_aggregate_id(
            [0x01; 32],
            [0x02; 32],
            &attestations,
            &bitmap,
            100,
            &[0x33; 16],
        );
        assert_eq!(a, b);
        // Different nonce → different id:
        let c = derive_aggregate_id(
            [0x01; 32],
            [0x02; 32],
            &attestations,
            &bitmap,
            100,
            &[0x44; 16],
        );
        assert_ne!(a, c);
    }

    // P2SR envelope construction sets canonical 10-byte header.
    #[test]
    fn p2sr_envelope_canonical_header() {
        let env = ParentToSubRouteEnvelope::new(
            [0x01; 32],
            [0x02; 32],
            [0x03; 32],
            CoordinatorTermId([0u8; 32]),
            100,
            [0x33; 16],
            vec![0xAA; 32],
            [0u8; 64],
        );
        assert_eq!(env.envelope_type, *b"DOT1");
        assert_eq!(env.envelope_subtype, SUBGROUP_ROUTE);
        assert_eq!(env.envelope_subtype, *b"P2SR");
        assert_eq!(env.version, 0x0001);
    }
}
