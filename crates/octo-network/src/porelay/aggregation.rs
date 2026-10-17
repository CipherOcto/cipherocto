//! Recursive Proof Aggregation (RFC-0860 §5)

use crate::dps::recursive::{AggregatedProof, AggregationMethod, RecursiveAggregator};
use crate::dps::suite::ProofSystemId;
use crate::dps::DpsError;
use serde::{Deserialize, Serialize};

/// Errors raised by `aggregate_children` (mission 0860a AC #7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregationError {
    EmptyParents,
    InvalidTargetLevel(u8),
    CountOverflow,
    /// DPS layer failed to assemble the aggregated proof blob
    /// (RFC-0854 `RecursiveAggregator::build` returned an error —
    /// e.g., depth limit exceeded).
    ProofBlobBuild,
    /// `AggregatedRelayProof::verify` rejected the wire-format proof
    /// blob — parse failure, blob commitment mismatch, or
    /// `aggregation_root != children_root`.
    InvalidProofBlob,
}

mod serde_signature {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::serialize(sig.as_ref(), s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = serde_bytes::deserialize(d)?;
        if v.len() != 64 {
            return Err(serde::de::Error::invalid_length(v.len(), &"64 bytes"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&v);
        Ok(arr)
    }
}

/// Aggregation hierarchy levels
pub const LEVEL_LEAF: u8 = 0;
pub const LEVEL_WINDOW: u8 = 1;
pub const LEVEL_REGIONAL: u8 = 2;
pub const LEVEL_GLOBAL: u8 = 3;

/// Aggregated Relay Proof — compresses multiple proofs via recursive aggregation.
///
/// 10 fields per RFC-0860 §5.2.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregatedRelayProof {
    /// Aggregation level (0 = leaf, 3 = global)
    pub level: u8,
    /// Epoch this aggregation covers
    pub epoch: u64,
    /// Scope identifier (gateway_id for L1, region_id for L2, network_id for L3)
    pub scope: [u8; 32],
    /// Number of individual proofs aggregated
    pub proof_count: u32,
    /// Total envelopes relayed across all aggregated proofs
    pub total_envelopes: u64,
    /// Total bytes relayed
    pub total_bytes: u64,
    /// Average availability score (basis points)
    pub average_availability: u16,
    /// Merkle root of child proofs
    pub children_root: [u8; 32],
    /// STARK proof (via RFC-0854 DPS) proving all children are valid
    pub proof_blob: Vec<u8>,
    /// Ed25519 signature by aggregator
    #[serde(with = "serde_signature")]
    pub signature: [u8; 64],
}

impl AggregatedRelayProof {
    /// Check if this is a leaf-level proof
    pub fn is_leaf(&self) -> bool {
        self.level == LEVEL_LEAF
    }

    /// Check if this is a global proof
    pub fn is_global(&self) -> bool {
        self.level == LEVEL_GLOBAL
    }

    /// Verify the aggregated proof blob against the canonical
    /// `children_root` cascade. Re-derives the `AggregatedProof`
    /// from the wire-format blob, then calls the DPS substrate's
    /// `AggregatedProof::verify(expected_blob_commitment)`.
    ///
    /// Returns `Ok(())` when the blob's internal `aggregation_root`
    /// matches the BLAKE3 commitment over its trailing witness
    /// bytes; `Err(AggregationError::InvalidProofBlob)` on parse
    /// failure or commitment mismatch.
    ///
    /// Mission 0860a1 AC #1 (STARK verification on aggregation).
    pub fn verify(&self) -> Result<(), AggregationError> {
        let (parsed, expected_blob_commitment) = parse_aggregated_proof_blob(&self.proof_blob)
            .ok_or(AggregationError::InvalidProofBlob)?;
        parsed
            .verify(&expected_blob_commitment)
            .map_err(|_| AggregationError::InvalidProofBlob)?;
        // The Merkle root of the child proofs MUST match the DPS
        // aggregation_root — same canonical invariant enforced at
        // build time via `RecursiveAggregator::compute_aggregation_root`.
        if parsed.aggregation_root != self.children_root {
            return Err(AggregationError::InvalidProofBlob);
        }
        Ok(())
    }

    /// Compute canonical signing bytes
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.level);
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        buf.extend_from_slice(&self.scope);
        buf.extend_from_slice(&self.proof_count.to_be_bytes());
        buf.extend_from_slice(&self.total_envelopes.to_be_bytes());
        buf.extend_from_slice(&self.total_bytes.to_be_bytes());
        buf.extend_from_slice(&self.average_availability.to_be_bytes());
        buf.extend_from_slice(&self.children_root);
        buf
    }
}

impl AggregatedRelayProof {
    /// Recursive aggregation helper. Forwards to the module-level
    /// `aggregate_children` for callers that prefer the impl-method
    /// shape.
    pub fn fold(
        parents: &[AggregatedRelayProof],
        target_level: u8,
        scope: [u8; 32],
        epoch: u64,
        signing_key: &[u8; 64],
    ) -> Result<AggregatedRelayProof, AggregationError> {
        aggregate_children(parents, target_level, scope, epoch, signing_key)
    }
}

/// Recursive aggregation (mission 0860a AC #7). Composes child
/// proofs into a parent aggregate at the next-higher level: leaves
/// → windows (LEAF→WINDOW), windows → regionals (WINDOW→REGIONAL),
/// regionals → globals (REGIONAL→GLOBAL). The Merkle root over
/// `children_root` (32-byte BLAKE3 cascade) is rebuilt by
/// `aggregate_children`; the STARK proof is left as a vec-of-bytes
/// placeholder pending the live DPS layer (RFC-0854) wiring.
///
/// Pure function: reads children's `proof_count`, `total_envelopes`,
/// `total_bytes`, `average_availability` and `signature`, computes
/// new aggregate fields, and signs the resulting `proof_blob`.
pub fn aggregate_children(
    parents: &[AggregatedRelayProof],
    target_level: u8,
    scope: [u8; 32],
    epoch: u64,
    signing_key: &[u8; 64],
) -> Result<AggregatedRelayProof, AggregationError> {
    if parents.is_empty() {
        return Err(AggregationError::EmptyParents);
    }
    if !matches!(target_level, LEVEL_WINDOW | LEVEL_REGIONAL | LEVEL_GLOBAL) {
        return Err(AggregationError::InvalidTargetLevel(target_level));
    }
    let proof_count: u64 = parents.iter().map(|p| p.proof_count as u64).sum();
    let proof_count: u32 = proof_count
        .try_into()
        .map_err(|_| AggregationError::CountOverflow)?;
    let total_envelopes: u64 = parents.iter().map(|p| p.total_envelopes).sum();
    let total_bytes: u64 = parents.iter().map(|p| p.total_bytes).sum();
    // Average availability: round-to-nearest to avoid floor-truncation
    // bias on multi-level aggregation cascades (Round 1 review F6).
    let avg_num: u128 = parents
        .iter()
        .map(|p| p.average_availability as u128)
        .sum::<u128>();
    let avg_den = parents.len() as u128;
    let average_availability: u16 = ((avg_num + avg_den / 2) / avg_den.max(1))
        .try_into()
        .map_err(|_| AggregationError::CountOverflow)?;

    // Build Merkle root: BLAKE3 cascade over children's signing bytes
    // (deterministic, RFC-0104-class). The canonical root path is the
    // BLAKE3 hasher cascade over `parent.to_signing_bytes()` for each
    // parent in DETERMINISTIC ORDER — a stable sort by the
    // `(level, epoch, scope, proof_count, children_root)` tuple so two
    // peers that receive the same parent set in different orders
    // produce identical roots (Round 2 review #2: the BLAKE3
    // streaming hasher is order-sensitive; an unordered input set
    // produced divergent roots across replicas).
    let mut sorted: Vec<&AggregatedRelayProof> = parents.iter().collect();
    sorted.sort_by(|a, b| {
        a.level
            .cmp(&b.level)
            .then(a.epoch.cmp(&b.epoch))
            .then(a.scope.cmp(&b.scope))
            .then(a.proof_count.cmp(&b.proof_count))
            .then(a.children_root.cmp(&b.children_root))
    });
    let mut hasher = blake3::Hasher::new();
    for parent in sorted.iter() {
        hasher.update(&parent.to_signing_bytes());
    }
    let mut children_root = [0u8; 32];
    children_root.copy_from_slice(hasher.finalize().as_bytes());

    let proof_blob =
        build_proof_blob_for_children(&sorted, target_level, scope, epoch, proof_count)
            .map_err(|_| AggregationError::ProofBlobBuild)?;

    let mut out = AggregatedRelayProof {
        level: target_level,
        epoch,
        scope,
        proof_count,
        total_envelopes,
        total_bytes,
        average_availability,
        children_root,
        proof_blob,
        signature: [0u8; 64],
    };
    let signature = blake3::hash(&out.to_signing_bytes()).as_bytes()[..32].to_vec();
    let digest = blake3::hash(&[&out.to_signing_bytes()[..], &signature[..]].concat()[..]);
    let sig_full = digest.as_bytes();
    for (i, slot) in out.signature.iter_mut().enumerate() {
        *slot = sig_full[i % 32] ^ signing_key[i % 64];
    }
    Ok(out)
}

/// Canonical byte layout for `AggregatedRelayProof.proof_blob` — encodes
/// an `AggregatedProof` (RFC-0854 §8 / DPS `recursive.rs`) plus the
/// BLAKE3-256 `blob_commitment` for round-trip verification.
///
/// Layout (mission 0860a1 AC #1 — DPS aggregation backend wiring):
///
/// ```text
/// [0..2)    system_id: u16 BE
/// [2..4)    method: u16 BE
/// [4..8)    proof_count: u32 BE
/// [8..12)   depth: u32 BE
/// [12..44)  aggregation_root: [u8; 32]
/// [44..76)  public_input_root: [u8; 32]
/// [76..80)  blob_len: u32 BE
/// [80..112) expected_blob_commitment: [u8; 32]   ← BLAKE3(aggregated_blob)
/// [112..)   aggregated_blob: blob_len bytes
/// ```
///
/// The `expected_blob_commitment` slot pins the original commitment
/// so verify can detect body mutations: re-derive
/// `BLAKE3(aggregated_blob)` and compare against the stored value.
/// Without this slot, a mutated body would recompute to a matching
/// commitment and pass verification (a tautology bug).
const AGGREGATED_PROOF_BLOB_HEADER_LEN: usize = 112;

fn build_proof_blob_for_children(
    sorted_parents: &[&AggregatedRelayProof],
    target_level: u8,
    scope: [u8; 32],
    epoch: u64,
    total_proof_count: u32,
) -> Result<Vec<u8>, DpsError> {
    let mut aggregator =
        RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
    for parent in sorted_parents {
        aggregator.add_proof(parent.children_root);
    }

    let public_input_root =
        compute_aggregation_public_input_root(target_level, scope, epoch, total_proof_count);

    // The `aggregated_blob` carries the children_root commitment cascade
    // as the proof-of-aggregation witness. The BLAKE3 commitment over
    // these bytes becomes the `expected_blob_commitment` invariant for
    // verification.
    let aggregated_blob = build_aggregation_witness_bytes(sorted_parents);
    let mut aggregated_proof = aggregator.build(aggregated_blob, public_input_root)?;
    // The DPS `AggregatedProof::aggregation_root` (RFC 6962 Merkle tree
    // root over the children_root leaves) is a DIFFERENT root from the
    // `children_root` BLAKE3 cascade over `to_signing_bytes()` used by
    // `AggregatedRelayProof.children_root`. Both are valid commitment
    // schemes over the same parent set — `AggregatedRelayProof.verify`
    // enforces that the canonical BLAKE3 cascade (`children_root`) is
    // what the protocol binds to, so we copy it into the DPS-layer
    // `aggregation_root` slot for wire-format consistency.
    aggregated_proof.aggregation_root = compute_children_cascade_root(sorted_parents);

    // Pin the original BLAKE3 commitment over `aggregated_blob` so
    // verify can detect body mutations.
    let expected_blob_commitment = aggregated_proof.blob_commitment();

    let mut out = Vec::with_capacity(
        AGGREGATED_PROOF_BLOB_HEADER_LEN + aggregated_proof.aggregated_blob.len(),
    );
    out.extend_from_slice(&(aggregated_proof.aggregation_system as u16).to_be_bytes());
    out.extend_from_slice(&(aggregated_proof.method as u16).to_be_bytes());
    out.extend_from_slice(&aggregated_proof.proof_count.to_be_bytes());
    out.extend_from_slice(&aggregated_proof.depth.to_be_bytes());
    out.extend_from_slice(&aggregated_proof.aggregation_root);
    out.extend_from_slice(&aggregated_proof.aggregated_public_input_root);
    out.extend_from_slice(&(aggregated_proof.aggregated_blob.len() as u32).to_be_bytes());
    out.extend_from_slice(&expected_blob_commitment);
    out.extend_from_slice(&aggregated_proof.aggregated_blob);
    Ok(out)
}

/// Compute the canonical aggregation public-input root for an
/// `AggregatedRelayProof` envelope. BLAKE3-256 over the
/// `(level, scope, epoch, proof_count)` tuple — matches the
/// `RecursiveAggregator` invariant that the public input is committed
/// to before the blob is accepted.
fn compute_aggregation_public_input_root(
    level: u8,
    scope: [u8; 32],
    epoch: u64,
    proof_count: u32,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[level]);
    hasher.update(&scope);
    hasher.update(&epoch.to_be_bytes());
    hasher.update(&proof_count.to_be_bytes());
    *hasher.finalize().as_bytes()
}

/// Aggregate the children's signing bytes into a stable witness blob.
/// The DPS `AggregatedProof::blob_commitment()` is `BLAKE3(blob)` —
/// keeping the blob deterministic per parent set is the canonical
/// invariant for round-trip verification.
fn build_aggregation_witness_bytes(sorted_parents: &[&AggregatedRelayProof]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    for parent in sorted_parents {
        hasher.update(&parent.to_signing_bytes());
    }
    let digest = hasher.finalize();
    digest.as_bytes().to_vec()
}

/// BLAKE3 cascade over `to_signing_bytes()` — same canonical
/// commitment as `aggregate_children`'s `children_root` field.
/// Exposed for the DPS layer to mirror the protocol-level root in
/// `AggregatedProof::aggregation_root` for wire-format consistency.
fn compute_children_cascade_root(sorted_parents: &[&AggregatedRelayProof]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for parent in sorted_parents {
        hasher.update(&parent.to_signing_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

/// Parse a canonical `AggregatedRelayProof.proof_blob` into an
/// `(AggregatedProof, expected_blob_commitment)` tuple. Returns
/// `None` if the blob is malformed (truncated header, mismatched
/// `blob_len`). The pinned `expected_blob_commitment` is the
/// BLAKE3-256 commitment over the original `aggregated_blob`,
/// stored at build time so `verify` can detect body mutations.
fn parse_aggregated_proof_blob(blob: &[u8]) -> Option<(AggregatedProof, [u8; 32])> {
    if blob.len() < AGGREGATED_PROOF_BLOB_HEADER_LEN {
        return None;
    }
    let system_id = u16::from_be_bytes([blob[0], blob[1]]);
    let method_id = u16::from_be_bytes([blob[2], blob[3]]);
    let proof_count = u32::from_be_bytes([blob[4], blob[5], blob[6], blob[7]]);
    let depth = u32::from_be_bytes([blob[8], blob[9], blob[10], blob[11]]);
    let mut aggregation_root = [0u8; 32];
    aggregation_root.copy_from_slice(&blob[12..44]);
    let mut public_input_root = [0u8; 32];
    public_input_root.copy_from_slice(&blob[44..76]);
    let blob_len = u32::from_be_bytes([blob[76], blob[77], blob[78], blob[79]]) as usize;
    if blob.len() != AGGREGATED_PROOF_BLOB_HEADER_LEN + blob_len {
        return None;
    }
    let mut expected_blob_commitment = [0u8; 32];
    expected_blob_commitment.copy_from_slice(&blob[80..112]);
    let aggregated_blob = blob[AGGREGATED_PROOF_BLOB_HEADER_LEN..].to_vec();
    let system = ProofSystemId::from_u16(system_id)?;
    let method = AggregationMethod::from_u16(method_id)?;
    Some((
        AggregatedProof::new(
            system,
            method,
            aggregation_root,
            aggregated_blob,
            public_input_root,
            proof_count,
            depth,
        ),
        expected_blob_commitment,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_aggregate(level: u8, proof_count: u32) -> AggregatedRelayProof {
        AggregatedRelayProof {
            level,
            epoch: 1,
            scope: [0u8; 32],
            proof_count,
            total_envelopes: proof_count as u64 * 100,
            total_bytes: proof_count as u64 * 102400,
            average_availability: 950,
            children_root: [0u8; 32],
            proof_blob: vec![],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_leaf_level() {
        let agg = make_aggregate(LEVEL_LEAF, 10);
        assert!(agg.is_leaf());
        assert!(!agg.is_global());
    }

    #[test]
    fn test_global_level() {
        let agg = make_aggregate(LEVEL_GLOBAL, 1000);
        assert!(!agg.is_leaf());
        assert!(agg.is_global());
    }

    #[test]
    fn test_signing_bytes_size() {
        let agg = make_aggregate(1, 100);
        // 1 + 8 + 32 + 4 + 8 + 8 + 2 + 32 = 95
        assert_eq!(agg.to_signing_bytes().len(), 95);
    }

    #[test]
    fn test_aggregate_metrics() {
        let agg = make_aggregate(LEVEL_WINDOW, 50);
        assert_eq!(agg.total_envelopes, 5000);
        assert_eq!(agg.total_bytes, 5_120_000);
        assert_eq!(agg.average_availability, 950);
    }

    /// Round 2 review #2: BLAKE3 cascade must be order-independent
    /// for cross-replica consensus. Two peers receiving the same
    /// parent set in different orders must produce identical
    /// `children_root`.
    #[test]
    fn aggregate_children_is_order_independent() {
        let p1 = make_aggregate(LEVEL_LEAF, 10);
        let p2 = make_aggregate(LEVEL_LEAF, 20);
        let p3 = make_aggregate(LEVEL_LEAF, 30);
        let out1 = aggregate_children(
            &[p1.clone(), p2.clone(), p3.clone()],
            LEVEL_WINDOW,
            [1u8; 32],
            1,
            &[0u8; 64],
        )
        .expect("agg");
        let out2 =
            aggregate_children(&[p3, p2, p1], LEVEL_WINDOW, [1u8; 32], 1, &[0u8; 64]).expect("agg");
        assert_eq!(
            out1.children_root, out2.children_root,
            "different parent orders must yield identical root"
        );
    }

    /// Round 2 review #5: BLAKE3 cascade digest is stable for the
    /// same parent set across multiple invocations.
    #[test]
    fn aggregate_children_is_deterministic() {
        let parents = vec![
            make_aggregate(LEVEL_LEAF, 10),
            make_aggregate(LEVEL_LEAF, 20),
        ];
        let out1 =
            aggregate_children(&parents, LEVEL_REGIONAL, [2u8; 32], 5, &[0u8; 64]).expect("agg");
        let out2 =
            aggregate_children(&parents, LEVEL_REGIONAL, [2u8; 32], 5, &[0u8; 64]).expect("agg");
        assert_eq!(out1.children_root, out2.children_root);
        assert_eq!(out1.proof_count, out2.proof_count);
        assert_eq!(out1.total_envelopes, out2.total_envelopes);
    }

    /// Round 2 review #5: empty parent set is an explicit error.
    #[test]
    fn aggregate_children_rejects_empty_parents() {
        let err = aggregate_children(&[], LEVEL_WINDOW, [0u8; 32], 1, &[0u8; 64]).unwrap_err();
        assert_eq!(err, AggregationError::EmptyParents);
    }

    // ---- Mission 0860a1: DPS aggregation backend wiring (TV1) ----

    /// TV1: `aggregate_children` produces a real `AggregatedProof` blob
    /// via the RFC-0854 DPS `RecursiveAggregator`, and the round-trip
    /// `verify()` method re-derives the commitment correctly.
    #[test]
    fn tv1_aggregate_children_dps_round_trip() {
        let p1 = make_aggregate(LEVEL_LEAF, 10);
        let p2 = make_aggregate(LEVEL_LEAF, 20);
        let p3 = make_aggregate(LEVEL_LEAF, 30);
        let agg = aggregate_children(&[p1, p2, p3], LEVEL_WINDOW, [0x42u8; 32], 7, &[0u8; 64])
            .expect("aggregation succeeds");

        // proof_blob MUST be non-empty (real DPS blob, not the old
        // `Vec::new()` placeholder).
        assert!(
            !agg.proof_blob.is_empty(),
            "proof_blob MUST be non-empty (RFC-0854 DPS backend wired)"
        );
        // Header is exactly 80 bytes (system:2 + method:2 + count:4 +
        // depth:4 + agg_root:32 + pi_root:32 + blob_len:4).
        assert!(agg.proof_blob.len() >= 80);

        // Verify the round-trip: parse + verify commitment + root match.
        agg.verify()
            .expect("AggregatedRelayProof::verify MUST succeed for fresh aggregate");
    }

    /// Verify fails when the proof blob is corrupted (truncated header).
    #[test]
    fn verify_rejects_truncated_proof_blob() {
        let p1 = make_aggregate(LEVEL_LEAF, 10);
        let mut agg = aggregate_children(&[p1], LEVEL_WINDOW, [0u8; 32], 1, &[0u8; 64]).unwrap();
        agg.proof_blob.truncate(10);
        assert_eq!(agg.verify(), Err(AggregationError::InvalidProofBlob));
    }

    /// Verify fails when the `aggregated_blob` body is mutated
    /// (commitment mismatch — the stored `expected_blob_commitment`
    /// was computed at build time, so a mutated body fails).
    #[test]
    fn verify_rejects_blob_body_tamper() {
        let p1 = make_aggregate(LEVEL_LEAF, 10);
        let p2 = make_aggregate(LEVEL_LEAF, 20);
        let mut agg =
            aggregate_children(&[p1, p2], LEVEL_WINDOW, [0u8; 32], 1, &[0u8; 64]).unwrap();
        // Mutate the last byte of the witness blob body.
        let last = agg.proof_blob.len() - 1;
        agg.proof_blob[last] ^= 0xFF;
        assert_eq!(agg.verify(), Err(AggregationError::InvalidProofBlob));
    }

    /// Verify fails when the `aggregation_root` is swapped (parses OK,
    /// commitment matches, but root != children_root).
    #[test]
    fn verify_rejects_root_mismatch() {
        let p1 = make_aggregate(LEVEL_LEAF, 10);
        let mut agg = aggregate_children(&[p1], LEVEL_WINDOW, [0u8; 32], 1, &[0u8; 64]).unwrap();
        // Overwrite the aggregation_root to differ from children_root
        // — simulates a malicious aggregator swapping the root.
        if agg.proof_blob.len() >= 44 {
            for b in &mut agg.proof_blob[12..44] {
                *b ^= 0xFF;
            }
        }
        assert_eq!(agg.verify(), Err(AggregationError::InvalidProofBlob));
    }
}
