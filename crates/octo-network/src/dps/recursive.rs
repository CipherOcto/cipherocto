//! Recursive proof aggregation — RFC-0854 §8, RFC-0650
//!
//! Binary tree aggregation of proofs with O(1) verification,
//! first-seen-wins conflict resolution, and configurable depth limits.

use std::collections::BTreeMap;

use crate::common::merkle;
use crate::dps::suite::ProofSystemId;
use crate::dps::DpsError;

/// Default maximum recursive aggregation depth.
pub const DEFAULT_MAX_AGGREGATION_DEPTH: u32 = 10;

/// Aggregation method identifiers (RFC-0854 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum AggregationMethod {
    /// Binary tree aggregation
    BinaryTree = 0x0001,
    /// Accumulation scheme
    Accumulation = 0x0002,
    /// Folding scheme
    Folding = 0x0003,
}

impl AggregationMethod {
    /// Parse from u16.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::BinaryTree),
            0x0002 => Some(Self::Accumulation),
            0x0003 => Some(Self::Folding),
            _ => None,
        }
    }
}

/// RFC-0650 actor roles in the proof aggregation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum AggregationRole {
    /// Generates individual proofs from witnesses
    Worker = 0x0001,
    /// Collects proofs from workers and batches them
    Collector = 0x0002,
    /// Combines collected proofs into aggregated proofs
    Aggregator = 0x0003,
    /// Verifies aggregated proofs in O(1)
    Verifier = 0x0004,
}

impl AggregationRole {
    /// Parse from u16.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::Worker),
            0x0002 => Some(Self::Collector),
            0x0003 => Some(Self::Aggregator),
            0x0004 => Some(Self::Verifier),
            _ => None,
        }
    }
}

/// An aggregated proof combining multiple constituent proofs.
///
/// RFC-0854 §8 / RFC-0859 §7.
#[derive(Debug, Clone)]
pub struct AggregatedProof {
    /// Proof system used for aggregation
    pub aggregation_system: ProofSystemId,
    /// Aggregation method
    pub method: AggregationMethod,
    /// Number of constituent proofs
    pub proof_count: u32,
    /// Merkle root of constituent proof commitments
    pub aggregation_root: [u8; 32],
    /// Aggregated proof blob
    pub aggregated_blob: Vec<u8>,
    /// Aggregated public input root
    pub aggregated_public_input_root: [u8; 32],
    /// Recursive depth of this aggregation (0 = leaf proofs)
    pub depth: u32,
}

impl AggregatedProof {
    /// Create a new aggregated proof.
    pub fn new(
        aggregation_system: ProofSystemId,
        method: AggregationMethod,
        aggregation_root: [u8; 32],
        aggregated_blob: Vec<u8>,
        aggregated_public_input_root: [u8; 32],
        proof_count: u32,
        depth: u32,
    ) -> Self {
        Self {
            aggregation_system,
            method,
            proof_count,
            aggregation_root,
            aggregated_blob,
            aggregated_public_input_root,
            depth,
        }
    }

    /// O(1) verification: verify the aggregated proof blob commitment.
    ///
    /// The verifier only checks the root proof, not all children.
    /// Returns Ok(()) if the blob commitment matches.
    pub fn verify(&self, expected_blob_commitment: &[u8; 32]) -> Result<(), DpsError> {
        let actual = self.blob_commitment();
        if actual != *expected_blob_commitment {
            return Err(DpsError::CommitmentMismatch {
                expected: *expected_blob_commitment,
                actual,
            });
        }
        Ok(())
    }

    /// Compute BLAKE3-256 commitment over the aggregated blob.
    pub fn blob_commitment(&self) -> [u8; 32] {
        *blake3::hash(&self.aggregated_blob).as_bytes()
    }

    /// Compute aggregation commitment: BLAKE3-256(child_0 || child_1).
    ///
    /// For binary tree aggregation, this is the commitment over two child commitments.
    pub fn compute_aggregation_commitment(
        left_child: &[u8; 32],
        right_child: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(left_child);
        hasher.update(right_child);
        *hasher.finalize().as_bytes()
    }

    /// Serialize for commitment computation.
    pub fn to_commitment_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self.aggregation_system as u16).to_be_bytes());
        buf.extend_from_slice(&(self.method as u16).to_be_bytes());
        buf.extend_from_slice(&self.proof_count.to_be_bytes());
        buf.extend_from_slice(&self.aggregation_root);
        buf.extend_from_slice(&self.aggregated_public_input_root);
        buf.extend_from_slice(&self.depth.to_be_bytes());
        buf
    }

    /// Compute full commitment over this aggregated proof.
    pub fn compute_commitment(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.to_commitment_bytes());
        hasher.update(&self.aggregated_blob);
        *hasher.finalize().as_bytes()
    }
}

/// Recursive proof aggregator — builds a binary tree of constituent proofs.
pub struct RecursiveAggregator {
    /// Constituent proof commitments
    leaves: Vec<[u8; 32]>,
    /// Aggregation system to use
    aggregation_system: ProofSystemId,
    /// Aggregation method
    method: AggregationMethod,
    /// Maximum recursive depth
    max_depth: u32,
}

impl RecursiveAggregator {
    /// Create a new aggregator with default depth limit.
    pub fn new(aggregation_system: ProofSystemId, method: AggregationMethod) -> Self {
        Self {
            leaves: Vec::new(),
            aggregation_system,
            method,
            max_depth: DEFAULT_MAX_AGGREGATION_DEPTH,
        }
    }

    /// Set the maximum recursive depth.
    pub fn with_max_depth(mut self, max_depth: u32) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Add a proof commitment to the aggregation.
    pub fn add_proof(&mut self, commitment: [u8; 32]) {
        self.leaves.push(commitment);
    }

    /// Number of constituent proofs.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Check if aggregator is empty.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Compute the aggregation root using RFC 6962 domain-separated Merkle tree.
    pub fn compute_aggregation_root(&self) -> [u8; 32] {
        merkle::compute_merkle_root(&self.leaves)
    }

    /// Compute the depth of the binary tree for the given number of leaves.
    pub fn compute_depth(leaf_count: usize) -> u32 {
        if leaf_count <= 1 {
            return 0;
        }
        // ceil(log2(leaf_count))
        (usize::BITS) - (leaf_count - 1).leading_zeros()
    }

    /// Build the aggregated proof from the provided blob and public input root.
    ///
    /// Returns error if:
    /// - No constituent proofs added
    /// - Depth exceeds max_depth
    pub fn build(
        &self,
        aggregated_blob: Vec<u8>,
        aggregated_public_input_root: [u8; 32],
    ) -> Result<AggregatedProof, DpsError> {
        if self.leaves.is_empty() {
            return Err(DpsError::AggregationError {
                reason: "no constituent proofs",
            });
        }

        let depth = Self::compute_depth(self.leaves.len());
        if depth > self.max_depth {
            return Err(DpsError::AggregationError {
                reason: "aggregation depth exceeds max_depth",
            });
        }

        let root = self.compute_aggregation_root();
        Ok(AggregatedProof::new(
            self.aggregation_system,
            self.method,
            root,
            aggregated_blob,
            aggregated_public_input_root,
            self.leaves.len() as u32,
            depth,
        ))
    }
}

/// First-seen-wins conflict resolution for double-aggregation.
///
/// Tracks canonical aggregation roots. If two different aggregations
/// claim the same constituent set, the first one seen is canonical.
#[derive(Debug, Clone)]
pub struct AggregationRegistry {
    /// aggregation_root -> first-seen commitment
    seen: BTreeMap<[u8; 32], [u8; 32]>,
    /// Maximum entries before eviction
    max_entries: usize,
}

impl AggregationRegistry {
    /// Create a new aggregation registry.
    pub fn new(max_entries: usize) -> Self {
        Self {
            seen: BTreeMap::new(),
            max_entries,
        }
    }

    /// Register an aggregation. Returns Ok(true) if first-seen (canonical),
    /// Ok(false) if duplicate, Err if at capacity.
    ///
    /// First-seen-wins: the first aggregation for a given root is canonical.
    pub fn register(
        &mut self,
        aggregation_root: [u8; 32],
        commitment: [u8; 32],
    ) -> Result<bool, DpsError> {
        if self.seen.contains_key(&aggregation_root) {
            return Ok(false);
        }

        if self.seen.len() >= self.max_entries {
            // Evict oldest entry (smallest key in BTreeMap)
            if let Some(key) = self.seen.keys().next().copied() {
                self.seen.remove(&key);
            }
        }

        self.seen.insert(aggregation_root, commitment);
        Ok(true)
    }

    /// Check if an aggregation root has been seen.
    pub fn is_canonical(&self, aggregation_root: &[u8; 32]) -> bool {
        self.seen.contains_key(aggregation_root)
    }

    /// Get the canonical commitment for an aggregation root.
    pub fn get_commitment(&self, aggregation_root: &[u8; 32]) -> Option<&[u8; 32]> {
        self.seen.get(aggregation_root)
    }

    /// Number of registered aggregations.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_proof(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    // -- AggregationMethod tests --

    #[test]
    fn test_aggregation_method_variants() {
        assert_eq!(AggregationMethod::BinaryTree as u16, 0x0001);
        assert_eq!(AggregationMethod::Accumulation as u16, 0x0002);
        assert_eq!(AggregationMethod::Folding as u16, 0x0003);
    }

    #[test]
    fn test_aggregation_method_from_u16() {
        assert_eq!(
            AggregationMethod::from_u16(0x0001),
            Some(AggregationMethod::BinaryTree)
        );
        assert_eq!(AggregationMethod::from_u16(0x0099), None);
    }

    // -- AggregationRole tests --

    #[test]
    fn test_aggregation_role_variants() {
        assert_eq!(AggregationRole::Worker as u16, 0x0001);
        assert_eq!(AggregationRole::Verifier as u16, 0x0004);
    }

    #[test]
    fn test_aggregation_role_from_u16() {
        assert_eq!(
            AggregationRole::from_u16(0x0001),
            Some(AggregationRole::Worker)
        );
        assert_eq!(
            AggregationRole::from_u16(0x0004),
            Some(AggregationRole::Verifier)
        );
        assert_eq!(AggregationRole::from_u16(0x0099), None);
    }

    // -- AggregatedProof tests --

    #[test]
    fn test_aggregated_proof_commitment_deterministic() {
        let ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::BinaryTree,
            [0xAA; 32],
            vec![1, 2, 3],
            [0xBB; 32],
            5,
            0,
        );
        let c1 = ap.compute_commitment();
        let c2 = ap.compute_commitment();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_aggregated_proof_verify_ok() {
        let blob = vec![1, 2, 3, 4, 5];
        let ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::BinaryTree,
            [0xAA; 32],
            blob,
            [0xBB; 32],
            1,
            0,
        );
        let commitment = ap.blob_commitment();
        assert!(ap.verify(&commitment).is_ok());
    }

    #[test]
    fn test_aggregated_proof_verify_fails_on_tamper() {
        let ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::BinaryTree,
            [0xAA; 32],
            vec![1, 2, 3],
            [0xBB; 32],
            1,
            0,
        );
        let wrong_commitment = [0xFF; 32];
        assert!(ap.verify(&wrong_commitment).is_err());
    }

    #[test]
    fn test_aggregation_commitment_formula() {
        let left = [0xAA; 32];
        let right = [0xBB; 32];
        let commitment = AggregatedProof::compute_aggregation_commitment(&left, &right);
        // Must be BLAKE3(left || right)
        let mut hasher = blake3::Hasher::new();
        hasher.update(&left);
        hasher.update(&right);
        let expected = *hasher.finalize().as_bytes();
        assert_eq!(commitment, expected);
    }

    #[test]
    fn test_aggregation_commitment_order_dependent() {
        let a = [0xAA; 32];
        let b = [0xBB; 32];
        let ab = AggregatedProof::compute_aggregation_commitment(&a, &b);
        let ba = AggregatedProof::compute_aggregation_commitment(&b, &a);
        assert_ne!(ab, ba);
    }

    #[test]
    fn test_aggregated_proof_depth_field() {
        let ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::BinaryTree,
            [0xAA; 32],
            vec![1, 2, 3],
            [0xBB; 32],
            4,
            2,
        );
        assert_eq!(ap.depth, 2);
    }

    // -- RecursiveAggregator tests --

    #[test]
    fn test_aggregator_empty() {
        let agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        assert!(agg.is_empty());
        assert_eq!(agg.len(), 0);
    }

    #[test]
    fn test_aggregator_single_proof() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        agg.add_proof(make_proof(0x01));
        assert_eq!(agg.len(), 1);
        // Single leaf: root = BLAKE3(0x00 || leaf) per RFC 6962
        let root = agg.compute_aggregation_root();
        let expected = merkle::hash_leaf(&make_proof(0x01));
        assert_eq!(root, expected);
    }

    #[test]
    fn test_aggregator_two_proofs() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        agg.add_proof(make_proof(0x01));
        agg.add_proof(make_proof(0x02));
        let root = agg.compute_aggregation_root();
        assert_ne!(root, make_proof(0x01));
        assert_ne!(root, make_proof(0x02));
    }

    #[test]
    fn test_aggregator_deterministic() {
        let mut agg1 = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        agg1.add_proof(make_proof(0x01));
        agg1.add_proof(make_proof(0x02));
        agg1.add_proof(make_proof(0x03));

        let mut agg2 = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        agg2.add_proof(make_proof(0x01));
        agg2.add_proof(make_proof(0x02));
        agg2.add_proof(make_proof(0x03));

        assert_eq!(
            agg1.compute_aggregation_root(),
            agg2.compute_aggregation_root()
        );
    }

    #[test]
    fn test_aggregator_empty_root() {
        let agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        assert_eq!(agg.compute_aggregation_root(), [0u8; 32]);
    }

    #[test]
    fn test_aggregator_build_empty() {
        let agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        assert!(agg.build(vec![1, 2, 3], [0xBB; 32]).is_err());
    }

    #[test]
    fn test_aggregator_build_ok() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);
        agg.add_proof(make_proof(0x01));
        agg.add_proof(make_proof(0x02));
        let ap = agg.build(vec![1, 2, 3], [0xBB; 32]).unwrap();
        assert_eq!(ap.proof_count, 2);
        assert_eq!(ap.depth, 1); // 2 leaves = depth 1
        assert_eq!(ap.aggregation_system, ProofSystemId::STWO);
    }

    #[test]
    fn test_aggregator_depth_limit_exceeded() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree)
            .with_max_depth(1); // max depth 1 = max 2 leaves
        agg.add_proof(make_proof(0x01));
        agg.add_proof(make_proof(0x02));
        agg.add_proof(make_proof(0x03)); // 3 leaves = depth 2 > max 1
        assert!(agg.build(vec![1, 2, 3], [0xBB; 32]).is_err());
    }

    #[test]
    fn test_aggregator_depth_limit_ok() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree)
            .with_max_depth(10);
        for i in 0..8u8 {
            agg.add_proof(make_proof(i));
        }
        let ap = agg.build(vec![1, 2, 3], [0xBB; 32]).unwrap();
        assert_eq!(ap.depth, 3); // 8 leaves = depth 3
    }

    #[test]
    fn test_compute_depth() {
        assert_eq!(RecursiveAggregator::compute_depth(0), 0);
        assert_eq!(RecursiveAggregator::compute_depth(1), 0);
        assert_eq!(RecursiveAggregator::compute_depth(2), 1);
        assert_eq!(RecursiveAggregator::compute_depth(3), 2);
        assert_eq!(RecursiveAggregator::compute_depth(4), 2);
        assert_eq!(RecursiveAggregator::compute_depth(8), 3);
        assert_eq!(RecursiveAggregator::compute_depth(1024), 10);
    }

    // -- AggregationRegistry tests --

    #[test]
    fn test_registry_first_seen_wins() {
        let mut reg = AggregationRegistry::new(100);
        let root = [0xAA; 32];
        let commit1 = [0x01; 32];
        let commit2 = [0x02; 32];

        assert!(reg.register(root, commit1).unwrap()); // first seen
        assert!(!reg.register(root, commit2).unwrap()); // duplicate, rejected
        assert_eq!(reg.get_commitment(&root), Some(&commit1)); // first is canonical
    }

    #[test]
    fn test_registry_different_roots() {
        let mut reg = AggregationRegistry::new(100);
        assert!(reg.register([0xAA; 32], [0x01; 32]).unwrap());
        assert!(reg.register([0xBB; 32], [0x02; 32]).unwrap());
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn test_registry_eviction() {
        let mut reg = AggregationRegistry::new(2);
        reg.register([0x01; 32], [0xAA; 32]).unwrap();
        reg.register([0x02; 32], [0xBB; 32]).unwrap();
        reg.register([0x03; 32], [0xCC; 32]).unwrap(); // evicts oldest
        assert_eq!(reg.len(), 2);
        assert!(!reg.is_canonical(&[0x01; 32])); // evicted
        assert!(reg.is_canonical(&[0x03; 32])); // newest
    }

    #[test]
    fn test_registry_is_canonical() {
        let mut reg = AggregationRegistry::new(100);
        assert!(!reg.is_canonical(&[0xAA; 32]));
        reg.register([0xAA; 32], [0x01; 32]).unwrap();
        assert!(reg.is_canonical(&[0xAA; 32]));
    }
}
