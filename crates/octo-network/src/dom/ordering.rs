//! Canonical Intent Ordering — RFC-0857 §4
//!
//! Sort order: (execution_class ASC, economic_weight DESC,
//!              logical_timestamp ASC, sequence ASC, intent_id ASC)

use crate::dom::intent::OverlayIntent;

/// Canonical sort key for deterministic ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalOrderKey {
    /// Lower class = higher priority
    pub execution_class: u16,
    /// Higher weight = higher priority (inverted for ascending sort)
    pub economic_weight_inv: u64,
    /// Older intents first
    pub logical_timestamp: u64,
    /// Lower sequence first
    pub sequence: u64,
    /// Lexicographic tiebreaker
    pub intent_id: [u8; 32],
}

impl CanonicalOrderKey {
    /// Build from an OverlayIntent.
    pub fn from_intent(intent: &OverlayIntent) -> Self {
        Self {
            execution_class: intent.execution_class,
            economic_weight_inv: u64::MAX.saturating_sub(intent.economic_weight),
            logical_timestamp: intent.logical_timestamp,
            sequence: intent.sequence,
            intent_id: intent.intent_id,
        }
    }
}

/// Sort intents in canonical order (RFC-0857 §4).
///
/// Returns a new Vec sorted by:
/// execution_class ASC, economic_weight DESC, logical_timestamp ASC,
/// sequence ASC, intent_id ASC.
pub fn canonical_sort(intents: &mut [OverlayIntent]) {
    intents.sort_by(|a, b| {
        let ka = CanonicalOrderKey::from_intent(a);
        let kb = CanonicalOrderKey::from_intent(b);
        ka.cmp(&kb)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::intent::ExecutionClass;

    fn make_intent(
        class: ExecutionClass,
        weight: u64,
        ts: u64,
        seq: u64,
        id_first: u8,
    ) -> OverlayIntent {
        OverlayIntent {
            intent_id: {
                let mut id = [0u8; 32];
                id[0] = id_first;
                id
            },
            intent_type: 0x0001,
            mission_id: [0u8; 32],
            sender_id: [0u8; 32],
            sequence: seq,
            logical_timestamp: ts,
            expiration: ts + 100,
            payload_root: [0u8; 32],
            economic_weight: weight,
            execution_class: class as u16,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_canonical_sort_by_class() {
        let mut intents = vec![
            make_intent(ExecutionClass::Standard, 100, 1, 1, 0x01),
            make_intent(ExecutionClass::Consensus, 100, 1, 1, 0x02),
        ];
        canonical_sort(&mut intents);
        assert_eq!(intents[0].execution_class, ExecutionClass::Consensus as u16);
        assert_eq!(intents[1].execution_class, ExecutionClass::Standard as u16);
    }

    #[test]
    fn test_canonical_sort_by_weight_desc() {
        let mut intents = vec![
            make_intent(ExecutionClass::Consensus, 500, 1, 1, 0x01),
            make_intent(ExecutionClass::Consensus, 1000, 1, 1, 0x02),
        ];
        canonical_sort(&mut intents);
        // Higher weight first
        assert_eq!(intents[0].economic_weight, 1000);
        assert_eq!(intents[1].economic_weight, 500);
    }

    #[test]
    fn test_canonical_sort_by_timestamp() {
        let mut intents = vec![
            make_intent(ExecutionClass::Consensus, 500, 200, 1, 0x01),
            make_intent(ExecutionClass::Consensus, 500, 100, 1, 0x02),
        ];
        canonical_sort(&mut intents);
        // Older timestamp first
        assert_eq!(intents[0].logical_timestamp, 100);
    }

    #[test]
    fn test_canonical_sort_by_id_tiebreak() {
        let mut intents = vec![
            make_intent(ExecutionClass::Consensus, 500, 100, 1, 0x02),
            make_intent(ExecutionClass::Consensus, 500, 100, 1, 0x01),
        ];
        canonical_sort(&mut intents);
        // Lower ID wins
        assert_eq!(intents[0].intent_id[0], 0x01);
    }

    #[test]
    fn test_canonical_sort_complex() {
        // Test vector from RFC-0857 §4:
        // A: class=1, weight=500, ts=100, seq=1, id=[0x01;32]
        // B: class=1, weight=1000, ts=200, seq=2, id=[0x02;32]
        // C: class=4, weight=500, ts=50, seq=1, id=[0x03;32]
        // D: class=1, weight=500, ts=100, seq=1, id=[0x00;32]
        // Canonical order: class ASC, weight DESC, timestamp ASC, seq ASC, id ASC
        // Consensus (class=1): B(weight=1000), D(weight=500,ts=100,id=0x00), A(weight=500,ts=100,id=0x01)
        // Standard (class=4): C
        // Expected: B, D, A, C
        let mut intents = vec![
            make_intent(ExecutionClass::Consensus, 500, 100, 1, 0x01), // A
            make_intent(ExecutionClass::Consensus, 1000, 200, 2, 0x02), // B
            make_intent(ExecutionClass::Standard, 500, 50, 1, 0x03),   // C
            make_intent(ExecutionClass::Consensus, 500, 100, 1, 0x00), // D
        ];
        canonical_sort(&mut intents);
        assert_eq!(intents[0].intent_id[0], 0x02); // B (highest weight in Consensus)
        assert_eq!(intents[1].intent_id[0], 0x00); // D (same weight/ts as A, lower id)
        assert_eq!(intents[2].intent_id[0], 0x01); // A
        assert_eq!(intents[3].intent_id[0], 0x03); // C (Standard class)
    }

    #[test]
    fn test_canonical_sort_empty() {
        let mut intents: Vec<OverlayIntent> = vec![];
        canonical_sort(&mut intents);
        assert!(intents.is_empty());
    }
}
