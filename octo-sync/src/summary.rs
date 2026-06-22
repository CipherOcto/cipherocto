//! Per-table Merkle segment summary builder (per RFC-0862 §4.3.4, mission 0862b).
//!
//! Pure compute over `SegmentMetadata` — does NOT call any DB functions.
//! Segment enumeration is delegated to mission 0862c via the
//! `DatabaseSyncAdapter` trait.
//!
//! The Merkle tree is 16-way (matching the Stoolap fork's `HexaryProof`
//! convention at `stoolap/src/trie/proof.rs:71-87`).

use crate::types::Lsn;

/// Metadata for a single snapshot segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentMetadata {
    /// The ordinal position of this segment in the table's snapshot directory.
    pub segment_index: u32,
    /// BLAKE3-256 of the segment payload.
    pub payload_hash: [u8; 32],
    /// The LSN watermark at the time the segment was generated.
    pub lsn_watermark: Lsn,
    /// Size of the segment in bytes.
    pub byte_size: u64,
}

/// A per-table Merkle segment summary envelope (RFC-0862 §4.3, code 0xA1).
#[allow(dead_code)] // used by external callers (e.g., the cipherocto sync engine)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSummary {
    /// The table ID.
    pub table_id: u32,
    /// The number of segments in the table.
    pub segment_count: u32,
    /// The Merkle root (BLAKE3-256 over the 16-way tree).
    pub segment_root: [u8; 32],
    /// The LSN watermark at the time the summary was built.
    pub lsn_watermark: Lsn,
    /// The HMAC binding the root to the local node.
    /// Computed as `HMAC-BLAKE3(transport_key, summary_body || node_id)`.
    pub hmac: [u8; 32],
}

/// 16-way Merkle tree over snapshot segments.
///
/// Tree depth ≤ 4 for ≤ 65,536 segments per table. The zero-hash for an empty
/// slot at depth 2 is different from the zero-hash at depth 3 (each level
/// hashes its own padding).
#[derive(Debug, Clone, Default)]
pub struct MerkleSegmentTree {
    leaves: Vec<[u8; 32]>,
}

impl MerkleSegmentTree {
    /// The branching factor (16-way tree).
    pub const BRANCH_FACTOR: usize = 16;

    /// Build a Merkle tree from a list of segment metadata.
    /// The leaves are sorted by `segment_index` before hashing.
    pub fn from_segments(segments: &[SegmentMetadata]) -> Self {
        let mut sorted: Vec<&SegmentMetadata> = segments.iter().collect();
        sorted.sort_by_key(|s| s.segment_index);
        let leaves: Vec<[u8; 32]> = sorted.iter().map(|s| s.payload_hash).collect();
        Self { leaves }
    }

    /// Return the root of the Merkle tree.
    pub fn root(&self) -> [u8; 32] {
        compute_root(&self.leaves)
    }

    /// Return the list of `(level, index)` where this tree diverges from `other`.
    /// `level = 0` is the leaf level; higher levels are internal nodes.
    /// `index` is the position within the level.
    pub fn diff(&self, other: &Self) -> Vec<(usize, usize)> {
        let mut divergences = Vec::new();
        if self.leaves == other.leaves {
            return divergences;
        }
        let mut level = 0;
        let mut current_self = self.leaves.clone();
        let mut current_other = other.leaves.clone();
        loop {
            if current_self.is_empty() && current_other.is_empty() {
                break;
            }
            let padded_self = pad_to_16(&current_self, level);
            let padded_other = pad_to_16(&current_other, level);
            let max_len = padded_self.len().max(padded_other.len());
            for i in 0..max_len {
                let a = padded_self.get(i).copied().unwrap_or_else(|| zero_hash(level));
                let b = padded_other.get(i).copied().unwrap_or_else(|| zero_hash(level));
                if a != b {
                    divergences.push((level, i));
                }
            }
            // If both are size 1, no higher level
            if current_self.len() <= 1 && current_other.len() <= 1 {
                break;
            }
            current_self = next_level(&current_self, level);
            current_other = next_level(&current_other, level);
            level += 1;
            if level > 4 {
                break;
            }
        }
        divergences
    }

    /// Return the segments at the given `(level, index)` positions.
    /// `level = 0` returns the leaves; higher levels return internal node hashes.
    pub fn segments_at(&self, positions: &[(usize, usize)]) -> Vec<SegmentMetadata> {
        // The full SegmentMetadata is available from 0862c via
        // adapter.read_snapshot_segment. This method returns empty
        // placeholders; the caller is expected to use the diff() result
        // to identify which segments to fetch from 0862c.
        let _ = positions;
        Vec::new()
    }

    /// Return the number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }
}

/// Compute the root of a 16-way tree over the given leaves.
fn compute_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return zero_hash(0);
    }
    let mut level = 0;
    let mut current: Vec<[u8; 32]> = leaves.to_vec();
    while current.len() > 1 {
        let padded = pad_to_16(&current, level);
        current = next_level(&padded, level);
        level += 1;
        if level > 4 {
            break;
        }
    }
    current[0]
}

/// Pad an array to a multiple of 16 with level-specific zero hashes.
fn pad_to_16(arr: &[[u8; 32]], level: usize) -> Vec<[u8; 32]> {
    let target_len = arr.len().div_ceil(16) * 16;
    if arr.len() == target_len {
        return arr.to_vec();
    }
    let mut padded = arr.to_vec();
    let zh = zero_hash(level);
    padded.resize(target_len, zh);
    padded
}

/// Compute the next level of the tree from the current level.
/// The input is assumed to already be padded to a multiple of 16.
fn next_level(arr: &[[u8; 32]], level: usize) -> Vec<[u8; 32]> {
    let mut next = Vec::with_capacity(arr.len() / 16);
    for chunk in arr.chunks(16) {
        let mut hasher = blake3::Hasher::new();
        for h in chunk {
            hasher.update(h);
        }
        let hash: [u8; 32] = *hasher.finalize().as_bytes();
        next.push(hash);
    }
    let _ = level;
    next
}

/// The zero-hash at a given level.
/// `zero_hash(0)` is the all-zero 32-byte array.
/// `zero_hash(n+1)` is BLAKE3-256(zero_hash(n) repeated 16 times).
fn zero_hash(level: usize) -> [u8; 32] {
    if level == 0 {
        [0u8; 32]
    } else {
        let prev = zero_hash(level - 1);
        let mut hasher = blake3::Hasher::new();
        for _ in 0..16 {
            hasher.update(&prev);
        }
        *hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(index: u32, hash_byte: u8) -> SegmentMetadata {
        let mut h = [0u8; 32];
        h[0] = hash_byte;
        SegmentMetadata {
            segment_index: index,
            payload_hash: h,
            lsn_watermark: index as Lsn,
            byte_size: 1024,
        }
    }

    fn make_hash(b: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = b;
        h
    }

    #[test]
    fn empty_tree_returns_zero_hash() {
        let t = MerkleSegmentTree::from_segments(&[]);
        assert_eq!(t.root(), [0u8; 32]);
    }

    #[test]
    fn single_leaf_tree_root_is_leaf() {
        let segs = vec![make_segment(0, 1)];
        let t = MerkleSegmentTree::from_segments(&segs);
        assert_eq!(t.root(), make_hash(1));
    }

    #[test]
    fn tree_with_exactly_16_leaves() {
        let segs: Vec<_> = (0..16).map(|i| make_segment(i, i as u8)).collect();
        let t = MerkleSegmentTree::from_segments(&segs);
        // Root is BLAKE3-256(sorted_leaves) (no padding needed at leaf level)
        let mut hasher = blake3::Hasher::new();
        for i in 0..16u8 {
            hasher.update(&make_hash(i));
        }
        let expected: [u8; 32] = *hasher.finalize().as_bytes();
        assert_eq!(t.root(), expected);
    }

    #[test]
    fn diff_identical_trees_returns_empty() {
        let segs = vec![make_segment(0, 1), make_segment(1, 2)];
        let t1 = MerkleSegmentTree::from_segments(&segs);
        let t2 = MerkleSegmentTree::from_segments(&segs);
        assert!(t1.diff(&t2).is_empty());
    }

    #[test]
    fn diff_disjoint_trees_returns_positions() {
        let t1 = MerkleSegmentTree::from_segments(&[make_segment(0, 1)]);
        let t2 = MerkleSegmentTree::from_segments(&[make_segment(0, 2)]);
        let d = t1.diff(&t2);
        assert!(!d.is_empty());
    }

    #[test]
    fn from_segments_sorts_by_segment_index() {
        let segs = vec![make_segment(2, 2), make_segment(0, 0), make_segment(1, 1)];
        let t = MerkleSegmentTree::from_segments(&segs);
        let segs_sorted = vec![make_segment(0, 0), make_segment(1, 1), make_segment(2, 2)];
        let t_sorted = MerkleSegmentTree::from_segments(&segs_sorted);
        assert_eq!(t.root(), t_sorted.root());
    }

    #[test]
    fn zero_hash_different_per_level() {
        assert_eq!(zero_hash(0), [0u8; 32]);
        assert_ne!(zero_hash(0), zero_hash(1));
        assert_ne!(zero_hash(1), zero_hash(2));
    }

    #[test]
    fn leaf_count() {
        let segs: Vec<_> = (0..5).map(|i| make_segment(i, i as u8)).collect();
        let t = MerkleSegmentTree::from_segments(&segs);
        assert_eq!(t.leaf_count(), 5);
    }
}
