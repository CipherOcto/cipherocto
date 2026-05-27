//! Overlay sequence numbers (RFC-0850 §5)

use serde::{Deserialize, Serialize};

/// Logical sequence number for deterministic ordering
///
/// Order: (epoch, monotonic_counter, gateway_id)
/// This ensures deterministic ordering independent of wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct OverlaySequence {
    /// Network epoch (consensus-derived)
    pub epoch: u64,
    /// Gateway that generated the sequence
    pub gateway: [u8; 32],
    /// Monotonically increasing counter per gateway per epoch
    pub monotonic_counter: u64,
}

impl OverlaySequence {
    /// Create a new sequence
    pub fn new(epoch: u64, gateway: [u8; 32], counter: u64) -> Self {
        Self {
            epoch,
            gateway,
            monotonic_counter: counter,
        }
    }

    /// Compare two sequences deterministically.
    /// Order: (epoch, monotonic_counter, gateway_id)
    pub fn canonical_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.epoch
            .cmp(&other.epoch)
            .then(self.monotonic_counter.cmp(&other.monotonic_counter))
            .then(self.gateway.cmp(&other.gateway))
    }
}

impl PartialOrd for OverlaySequence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}

impl Ord for OverlaySequence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.canonical_cmp(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_ordering_by_epoch() {
        let a = OverlaySequence::new(1, [0x01; 32], 100);
        let b = OverlaySequence::new(2, [0x01; 32], 100);
        assert!(a.canonical_cmp(&b) == std::cmp::Ordering::Less);
    }

    #[test]
    fn test_sequence_ordering_by_counter() {
        let a = OverlaySequence::new(1, [0x01; 32], 100);
        let b = OverlaySequence::new(1, [0x01; 32], 200);
        assert!(a.canonical_cmp(&b) == std::cmp::Ordering::Less);
    }

    #[test]
    fn test_sequence_ordering_by_gateway() {
        let a = OverlaySequence::new(1, [0x01; 32], 100);
        let b = OverlaySequence::new(1, [0x02; 32], 100);
        assert!(a.canonical_cmp(&b) == std::cmp::Ordering::Less);
    }

    #[test]
    fn test_sequence_equal() {
        let a = OverlaySequence::new(1, [0x01; 32], 100);
        let b = OverlaySequence::new(1, [0x01; 32], 100);
        assert!(a.canonical_cmp(&b) == std::cmp::Ordering::Equal);
    }
}
