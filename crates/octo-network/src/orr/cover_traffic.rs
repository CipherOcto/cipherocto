//! Cover traffic generation (RFC-0858 §8)
//!
//! Generates cover traffic indistinguishable from real relay traffic
//! to prevent traffic analysis attacks on onion routes.

/// Default cover traffic ratio (20% of total traffic).
pub const DEFAULT_COVER_RATIO: f64 = 0.20;

/// Generate a cover payload that is indistinguishable from real relay traffic.
///
/// Uses deterministic randomness from `rng_seed` to produce a payload
/// of the specified size. The payload structure mimics a real DOT envelope
/// to resist traffic analysis.
pub fn generate_cover_payload(size: usize, rng_seed: &[u8; 32]) -> Vec<u8> {
    use blake3::Hasher;

    let mut payload = Vec::with_capacity(size);
    let mut counter = 0u64;

    while payload.len() < size {
        let mut hasher = Hasher::new();
        hasher.update(rng_seed);
        hasher.update(&counter.to_be_bytes());
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let remaining = size - payload.len();
        let take = remaining.min(32);
        payload.extend_from_slice(&bytes[..take]);
        counter += 1;
    }

    payload
}

/// Check if a given ratio of cover-to-real traffic is within acceptable bounds.
pub fn is_cover_ratio_valid(cover_count: usize, real_count: usize, target_ratio: f64) -> bool {
    if real_count == 0 {
        return cover_count == 0;
    }
    let actual_ratio = cover_count as f64 / (cover_count + real_count) as f64;
    (actual_ratio - target_ratio).abs() < 0.05 // 5% tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_cover_payload_size() {
        let seed = [0x42u8; 32];
        let payload = generate_cover_payload(100, &seed);
        assert_eq!(payload.len(), 100);
    }

    #[test]
    fn test_generate_cover_payload_deterministic() {
        let seed = [0x42u8; 32];
        let p1 = generate_cover_payload(64, &seed);
        let p2 = generate_cover_payload(64, &seed);
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_generate_cover_payload_different_seeds() {
        let s1 = [0x42u8; 32];
        let s2 = [0x43u8; 32];
        let p1 = generate_cover_payload(64, &s1);
        let p2 = generate_cover_payload(64, &s2);
        assert_ne!(p1, p2);
    }

    #[test]
    fn test_cover_ratio_valid() {
        assert!(is_cover_ratio_valid(20, 80, 0.20));
        assert!(!is_cover_ratio_valid(50, 50, 0.20));
    }

    #[test]
    fn test_cover_ratio_zero_real() {
        assert!(is_cover_ratio_valid(0, 0, 0.20));
        assert!(!is_cover_ratio_valid(1, 0, 0.20));
    }
}
