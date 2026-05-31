//! Cover Traffic Generation and Replay Protection (RFC-0858 §6)
//!
//! Cover traffic is indistinguishable from real relay traffic to prevent
//! traffic analysis attacks. Replay protection uses a BTreeSet of
//! (route_id, sequence) pairs for deterministic checking.
//!
//! # RFC-0008 Execution Class Mapping (RFC-0858 §2)
//!
//! | Operation                    | Class |
//! |------------------------------|-------|
//! | Onions construction          | C     |
//! | Onion verification           | A     |
//! | Cover traffic generation     | C     |
//! | Replay check                 | A     |
//! | Session key derivation       | A     |
//! | Hop MAC computation          | A     |

use std::collections::BTreeSet;

/// Generate indistinguishable cover traffic using deterministic randomness.
///
/// Produces a payload of exactly `size` bytes that is computationally
/// indistinguishable from real encrypted relay traffic. The output is
/// deterministic given the same `rng_seed`, ensuring reproducibility
/// across nodes for testing and auditing.
///
/// Uses BLAKE3 in XOF (extensible output function) mode keyed by the seed
/// to generate pseudorandom bytes.
///
/// # Arguments
/// * `size` - Desired payload size in bytes
/// * `rng_seed` - 32-byte deterministic seed for the PRNG
pub fn generate_cover_payload(size: usize, rng_seed: &[u8; 32]) -> Vec<u8> {
    let mut output = vec![0u8; size];
    let mut hasher = blake3::Hasher::new();
    hasher.update(rng_seed);
    hasher.update(b"orr:cover_traffic:v1");
    let mut xof = hasher.finalize_xof();
    xof.fill(&mut output);
    output
}

/// Check if a (route_id, sequence) pair has been seen before (replay detection).
///
/// Returns `true` if the pair is already in the seen set (replay detected),
/// `false` if it is new.
///
/// # Arguments
/// * `route_id` - The 32-byte route identifier
/// * `sequence` - The monotonic sequence number for this route
/// * `seen` - The set of previously observed (route_id, sequence) pairs
pub fn check_replay(route_id: &[u8; 32], sequence: u64, seen: &BTreeSet<([u8; 32], u64)>) -> bool {
    seen.contains(&(*route_id, sequence))
}

/// RFC-0008 Execution Class Mapping for ORR operations.
///
/// Maps ORR operation types to their execution classes per RFC-0008.
pub const ORR_EXECUTION_CLASS_TABLE: &[(&str, &str)] = &[
    ("Onion construction", "Class C"),
    ("Onion verification", "Class A"),
    ("Cover traffic generation", "Class C"),
    ("Replay check", "Class A"),
    ("Session key derivation", "Class A"),
    ("Hop MAC computation", "Class A"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cover_traffic_deterministic() {
        let seed = [0xAA; 32];
        let p1 = generate_cover_payload(256, &seed);
        let p2 = generate_cover_payload(256, &seed);
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_cover_traffic_correct_size() {
        let seed = [0xBB; 32];
        for size in [0, 1, 32, 128, 1024, 4096] {
            let payload = generate_cover_payload(size, &seed);
            assert_eq!(payload.len(), size);
        }
    }

    #[test]
    fn test_cover_traffic_different_seeds() {
        let seed_a = [0xAA; 32];
        let seed_b = [0xBB; 32];
        let p1 = generate_cover_payload(256, &seed_a);
        let p2 = generate_cover_payload(256, &seed_b);
        assert_ne!(p1, p2);
    }

    #[test]
    fn test_cover_traffic_not_all_zeros() {
        let seed = [0xCC; 32];
        let payload = generate_cover_payload(256, &seed);
        assert!(payload.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_replay_check_new_entry() {
        let seen = BTreeSet::new();
        let route_id = [0xAA; 32];
        assert!(!check_replay(&route_id, 1, &seen));
    }

    #[test]
    fn test_replay_check_existing_entry() {
        let mut seen = BTreeSet::new();
        let route_id = [0xAA; 32];
        seen.insert((route_id, 1));
        assert!(check_replay(&route_id, 1, &seen));
    }

    #[test]
    fn test_replay_check_different_sequence() {
        let mut seen = BTreeSet::new();
        let route_id = [0xAA; 32];
        seen.insert((route_id, 1));
        assert!(!check_replay(&route_id, 2, &seen));
    }

    #[test]
    fn test_replay_check_different_route() {
        let mut seen = BTreeSet::new();
        let route_a = [0xAA; 32];
        let route_b = [0xBB; 32];
        seen.insert((route_a, 1));
        assert!(!check_replay(&route_b, 1, &seen));
    }

    #[test]
    fn test_execution_class_table() {
        assert_eq!(ORR_EXECUTION_CLASS_TABLE.len(), 6);
        assert_eq!(
            ORR_EXECUTION_CLASS_TABLE[0],
            ("Onion construction", "Class C")
        );
        assert_eq!(
            ORR_EXECUTION_CLASS_TABLE[1],
            ("Onion verification", "Class A")
        );
        assert_eq!(
            ORR_EXECUTION_CLASS_TABLE[2],
            ("Cover traffic generation", "Class C")
        );
    }
}
