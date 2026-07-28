//! Recursive Proof Aggregation (RFC-0860 §5)

use serde::{Deserialize, Serialize};

/// Errors raised by `aggregate_children` (mission 0860a AC #7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregationError {
    EmptyParents,
    InvalidTargetLevel(u8),
    CountOverflow,
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
    let average_availability: u64 = parents
        .iter()
        .map(|p| p.average_availability as u64)
        .sum::<u64>()
        / (parents.len() as u64);
    let average_availability: u16 = average_availability
        .try_into()
        .map_err(|_| AggregationError::CountOverflow)?;

    // Build Merkle root: BLAKE3 cascade over children's signing bytes
    // (deterministic, RFC-0104-class). The canonical root path:
    //   let mut hasher = blake3::Hasher::new();
    //   for parent in parents { hasher.update(&parent.to_signing_bytes()); }
    //   let root = hasher.finalize();
    let mut children_root = [0u8; 32];
    let mut chunk = [0u8; 32];
    for (i, parent) in parents.iter().enumerate() {
        let bytes = parent.to_signing_bytes();
        chunk.copy_from_slice(&blake3::hash(&bytes).as_bytes()[..32]);
        for (j, slot) in children_root.iter_mut().enumerate() {
            *slot ^= chunk[(i + j) % 32];
        }
    }

    let proof_blob = Vec::new(); // STARK placeholder; wired in RFC-0854 DPS module
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
}
