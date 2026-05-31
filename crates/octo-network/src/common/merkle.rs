//! RFC 6962 domain-separated Merkle root computation.
//!
//! All Merkle root computations in octo-network MUST use this module
//! to ensure consistent domain separation across all modules.
//!
//! Leaf hash: BLAKE3(0x00 || data)
//! Internal hash: BLAKE3(0x01 || left || right)
//! Odd leaves: last element duplicated for pairing.

/// Compute a BLAKE3-256 Merkle root with RFC 6962 domain separation.
///
/// - Empty input: returns `[0u8; 32]`
/// - Single leaf: returns `BLAKE3(0x00 || leaf)`
/// - Multiple leaves: binary tree with `BLAKE3(0x01 || left || right)` internal nodes
/// - Odd count: last element duplicated for pairing
pub fn compute_merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    // Compute leaf hashes with domain separation
    let mut level: Vec<[u8; 32]> = leaves.iter().map(|h| hash_leaf(h)).collect();

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
            next.push(hash_internal(&left, &right));
            i += 2;
        }
        level = next;
    }

    level[0]
}

/// Hash a leaf with RFC 6962 domain separation: BLAKE3(0x00 || data).
pub fn hash_leaf(data: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[0x00]);
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

/// Hash two children with RFC 6962 domain separation: BLAKE3(0x01 || left || right).
pub fn hash_internal(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[0x01]);
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_root() {
        assert_eq!(compute_merkle_root(&[]), [0u8; 32]);
    }

    #[test]
    fn test_single_leaf_uses_domain_separation() {
        let leaf = [0xAA; 32];
        let root = compute_merkle_root(&[leaf]);
        // Must be BLAKE3(0x00 || leaf), NOT raw leaf
        let expected = hash_leaf(&leaf);
        assert_eq!(root, expected);
        assert_ne!(root, leaf, "single leaf must be hashed, not raw");
    }

    #[test]
    fn test_two_leaves() {
        let a = [0xAA; 32];
        let b = [0xBB; 32];
        let root = compute_merkle_root(&[a, b]);
        let expected = hash_internal(&hash_leaf(&a), &hash_leaf(&b));
        assert_eq!(root, expected);
    }

    #[test]
    fn test_three_leaves_duplicates_last() {
        let a = [0xAA; 32];
        let b = [0xBB; 32];
        let c = [0xCC; 32];
        let root = compute_merkle_root(&[a, b, c]);
        // Last leaf duplicated: [hash(a), hash(b), hash(c)] -> pairs: [ab, cc] -> root
        let ha = hash_leaf(&a);
        let hb = hash_leaf(&b);
        let hc = hash_leaf(&c);
        let ab = hash_internal(&ha, &hb);
        let cc = hash_internal(&hc, &hc);
        let expected = hash_internal(&ab, &cc);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_deterministic() {
        let leaves = [[0x01; 32], [0x02; 32], [0x03; 32]];
        let r1 = compute_merkle_root(&leaves);
        let r2 = compute_merkle_root(&leaves);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_order_dependent() {
        let a = [[0x01; 32], [0x02; 32]];
        let b = [[0x02; 32], [0x01; 32]];
        assert_ne!(compute_merkle_root(&a), compute_merkle_root(&b));
    }

    #[test]
    fn test_leaf_hash_differs_from_raw() {
        let data = [0x42; 32];
        assert_ne!(hash_leaf(&data), data);
    }

    #[test]
    fn test_internal_hash_differs_from_leaf() {
        let data = [0x42; 32];
        assert_ne!(hash_internal(&data, &data), hash_leaf(&data));
    }
}
