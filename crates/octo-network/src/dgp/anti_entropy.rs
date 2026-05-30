//! Anti-entropy synchronization (RFC-0852 §7)
//!
//! Anti-entropy reconciliation uses Merkle state summaries to efficiently
//! detect and resolve state divergence between peers.

use super::domain::GossipDomainId;
use super::error::DgpError;
use super::object::GossipObject;

/// State summary for a gossip domain, used for anti-entropy reconciliation.
///
/// Peers exchange summaries to detect divergence. If state roots differ,
/// binary Merkle descent locates the specific divergent objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GossipStateSummary {
    /// The gossip domain this summary covers
    pub domain_id: GossipDomainId,
    /// Merkle root of all object hashes in this domain (BLAKE3-256)
    pub state_root: [u8; 32],
    /// Total number of objects in this domain
    pub object_count: u64,
    /// Highest logical timestamp in this domain
    pub watermark: u64,
}

impl GossipStateSummary {
    /// Compute a state summary from a set of objects in a domain.
    ///
    /// Objects are sorted by canonical order before computing the Merkle root
    /// to ensure deterministic summaries regardless of insertion order.
    pub fn compute(domain_id: &GossipDomainId, objects: &[GossipObject]) -> Self {
        let mut domain_objects: Vec<&GossipObject> = objects
            .iter()
            .filter(|o| o.domain_id == *domain_id)
            .collect();

        // Sort by canonical order for deterministic Merkle root
        domain_objects.sort_by_key(|a| a.ordering_key());

        let state_root = compute_merkle_root(
            &domain_objects
                .iter()
                .map(|o| o.object_hash)
                .collect::<Vec<_>>(),
        );

        let watermark = domain_objects
            .iter()
            .map(|o| o.logical_timestamp)
            .max()
            .unwrap_or(0);

        Self {
            domain_id: *domain_id,
            state_root,
            object_count: domain_objects.len() as u64,
            watermark,
        }
    }

    /// Check if two summaries indicate identical state.
    pub fn matches(&self, other: &Self) -> bool {
        self.state_root == other.state_root && self.object_count == other.object_count
    }
}

/// Result of anti-entropy reconciliation between two peers.
#[derive(Debug, Clone)]
pub struct ReconciliationResult {
    /// Objects we have that the peer is missing
    pub missing_from_peer: Vec<[u8; 32]>,
    /// Objects the peer has that we are missing
    pub missing_from_us: Vec<[u8; 32]>,
}

/// Anti-entropy reconciler.
///
/// Given local and remote state summaries, determines which objects
/// need to be exchanged to achieve convergence.
pub struct AntiEntropyReconciler;

impl AntiEntropyReconciler {
    /// Reconcile local and remote summaries.
    ///
    /// If state roots match, no action needed. If they differ, returns
    /// the set of object hashes that need to be exchanged.
    pub fn reconcile(
        local_summary: &GossipStateSummary,
        remote_summary: &GossipStateSummary,
        local_objects: &[GossipObject],
        remote_hashes: &[[u8; 32]],
    ) -> Result<ReconciliationResult, DgpError> {
        // If roots match, state is identical
        if local_summary.matches(remote_summary) {
            return Ok(ReconciliationResult {
                missing_from_peer: Vec::new(),
                missing_from_us: Vec::new(),
            });
        }

        // Compute diff: objects we have that peer doesn't
        let local_hashes: std::collections::BTreeSet<[u8; 32]> = local_objects
            .iter()
            .filter(|o| o.domain_id == local_summary.domain_id)
            .map(|o| o.object_hash)
            .collect();

        let remote_set: std::collections::BTreeSet<[u8; 32]> =
            remote_hashes.iter().copied().collect();

        let missing_from_peer: Vec<[u8; 32]> =
            local_hashes.difference(&remote_set).copied().collect();

        let missing_from_us: Vec<[u8; 32]> =
            remote_set.difference(&local_hashes).copied().collect();

        Ok(ReconciliationResult {
            missing_from_peer,
            missing_from_us,
        })
    }
}

/// Compute a BLAKE3-256 Merkle root from a list of hashes.
///
/// Uses domain separation per RFC 6962:
/// - Leaf hash: BLAKE3(0x00 || hash)
/// - Internal hash: BLAKE3(0x01 || left || right)
///
/// If the number of leaves is odd, the last element is duplicated for pairing.
/// Returns zero hash for empty input.
pub fn compute_merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
    if hashes.is_empty() {
        return [0u8; 32];
    }

    // Compute leaf hashes with domain separation
    let mut level: Vec<[u8; 32]> = hashes
        .iter()
        .map(|h| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&[0x00]);
            hasher.update(h);
            *hasher.finalize().as_bytes()
        })
        .collect();

    // Build tree bottom-up
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() {
                level[i + 1]
            } else {
                // Duplicate last element for odd count
                level[i]
            };
            let mut hasher = blake3::Hasher::new();
            hasher.update(&[0x01]);
            hasher.update(&left);
            hasher.update(&right);
            next.push(*hasher.finalize().as_bytes());
            i += 2;
        }
        level = next;
    }

    level[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::{GossipDomainId, GossipScope};
    use crate::dgp::object::{GossipObjectType, FLAG_ANTI_ENTROPY};

    fn make_obj(domain_net: u32, ts: u64, hash_byte: u8) -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::Envelope as u16,
            object_hash: [hash_byte; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(domain_net, [0u8; 32], GossipScope::GLOBAL),
            logical_timestamp: ts,
            origin_gateway: [1u8; 32],
            ttl_hops: 20,
            propagation_flags: FLAG_ANTI_ENTROPY,
            payload_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_state_summary_matches() {
        let domain = GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL);
        let objects = vec![make_obj(1, 100, 0xAA), make_obj(1, 200, 0xBB)];
        let summary_a = GossipStateSummary::compute(&domain, &objects);
        let summary_b = GossipStateSummary::compute(&domain, &objects);
        assert!(summary_a.matches(&summary_b));
    }

    #[test]
    fn test_state_summary_deterministic() {
        let domain = GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL);
        // Insert in different order - should produce same summary
        let objects_a = vec![make_obj(1, 100, 0xAA), make_obj(1, 200, 0xBB)];
        let objects_b = vec![make_obj(1, 200, 0xBB), make_obj(1, 100, 0xAA)];
        let summary_a = GossipStateSummary::compute(&domain, &objects_a);
        let summary_b = GossipStateSummary::compute(&domain, &objects_b);
        assert_eq!(summary_a.state_root, summary_b.state_root);
        assert!(summary_a.matches(&summary_b));
    }

    #[test]
    fn test_state_summary_divergence() {
        let domain = GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL);
        let objects_a = vec![make_obj(1, 100, 0xAA)];
        let objects_b = vec![make_obj(1, 100, 0xBB)];
        let summary_a = GossipStateSummary::compute(&domain, &objects_a);
        let summary_b = GossipStateSummary::compute(&domain, &objects_b);
        assert!(!summary_a.matches(&summary_b));
    }

    #[test]
    fn test_reconcile_identical() {
        let domain = GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL);
        let objects = vec![make_obj(1, 100, 0xAA), make_obj(1, 200, 0xBB)];
        let summary = GossipStateSummary::compute(&domain, &objects);
        let remote_hashes = vec![[0xAA; 32], [0xBB; 32]];

        let result =
            AntiEntropyReconciler::reconcile(&summary, &summary, &objects, &remote_hashes).unwrap();
        assert!(result.missing_from_peer.is_empty());
        assert!(result.missing_from_us.is_empty());
    }

    #[test]
    fn test_reconcile_divergent() {
        let domain = GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL);
        let local_objects = vec![make_obj(1, 100, 0xAA), make_obj(1, 200, 0xBB)];
        let remote_objects = vec![make_obj(1, 100, 0xAA), make_obj(1, 200, 0xCC)];

        let local_summary = GossipStateSummary::compute(&domain, &local_objects);
        let remote_summary = GossipStateSummary::compute(&domain, &remote_objects);
        let remote_hashes: Vec<[u8; 32]> = remote_objects.iter().map(|o| o.object_hash).collect();

        let result = AntiEntropyReconciler::reconcile(
            &local_summary,
            &remote_summary,
            &local_objects,
            &remote_hashes,
        )
        .unwrap();

        // We have BB, peer has CC
        assert!(result.missing_from_peer.contains(&[0xBB; 32]));
        assert!(result.missing_from_us.contains(&[0xCC; 32]));
    }

    #[test]
    fn test_merkle_root_single() {
        let root = compute_merkle_root(&[[0xAA; 32]]);
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_two() {
        let root = compute_merkle_root(&[[0xAA; 32], [0xBB; 32]]);
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let a = compute_merkle_root(&[[0xAA; 32], [0xBB; 32]]);
        let b = compute_merkle_root(&[[0xAA; 32], [0xBB; 32]]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_merkle_root_order_dependent() {
        let a = compute_merkle_root(&[[0xAA; 32], [0xBB; 32]]);
        let b = compute_merkle_root(&[[0xBB; 32], [0xAA; 32]]);
        assert_ne!(a, b); // Merkle root depends on order
    }

    #[test]
    fn test_merkle_root_empty() {
        let root = compute_merkle_root(&[]);
        assert_eq!(root, [0u8; 32]);
    }
}
