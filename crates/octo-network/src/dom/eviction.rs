//! Deterministic Eviction — RFC-0857 §9
//!
//! Eviction order: lowest class → lowest weight → oldest timestamp.

use crate::dom::intent::OverlayIntent;

/// Eviction key: highest execution_class (lowest priority) evicted first.
/// Uses inverted class so min_by_key picks the right target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvictionKey {
    /// Inverted: higher class value = lower numeric = evicted first
    pub execution_class_inv: u16,
    pub economic_weight: u64,
    pub logical_timestamp: u64,
    pub intent_id: [u8; 32],
}

impl EvictionKey {
    pub fn from_intent(intent: &OverlayIntent) -> Self {
        Self {
            execution_class_inv: u16::MAX.saturating_sub(intent.execution_class),
            economic_weight: intent.economic_weight,
            logical_timestamp: intent.logical_timestamp,
            intent_id: intent.intent_id,
        }
    }
}

/// Find the worst intent for eviction (lowest priority).
pub fn find_eviction_target(intents: &[OverlayIntent]) -> Option<usize> {
    intents
        .iter()
        .enumerate()
        .min_by_key(|(_, i)| EvictionKey::from_intent(i))
        .map(|(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::intent::ExecutionClass;

    fn make_intent(class: ExecutionClass, weight: u64, ts: u64, id: u8) -> OverlayIntent {
        OverlayIntent {
            intent_id: {
                let mut arr = [0u8; 32];
                arr[0] = id;
                arr
            },
            intent_type: 0x0001,
            mission_id: [0u8; 32],
            sender_id: [0u8; 32],
            sequence: 1,
            logical_timestamp: ts,
            expiration: ts + 100,
            payload_root: [0u8; 32],
            economic_weight: weight,
            execution_class: class as u16,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_evict_lowest_class() {
        let intents = vec![
            make_intent(ExecutionClass::Consensus, 100, 10, 0x01),
            make_intent(ExecutionClass::Archive, 100, 10, 0x02),
        ];
        let idx = find_eviction_target(&intents).unwrap();
        assert_eq!(intents[idx].execution_class, ExecutionClass::Archive as u16);
    }

    #[test]
    fn test_evict_lowest_weight() {
        let intents = vec![
            make_intent(ExecutionClass::Standard, 500, 10, 0x01),
            make_intent(ExecutionClass::Standard, 100, 10, 0x02),
        ];
        let idx = find_eviction_target(&intents).unwrap();
        assert_eq!(intents[idx].economic_weight, 100);
    }

    #[test]
    fn test_evict_oldest() {
        let intents = vec![
            make_intent(ExecutionClass::Standard, 100, 200, 0x01),
            make_intent(ExecutionClass::Standard, 100, 50, 0x02),
        ];
        let idx = find_eviction_target(&intents).unwrap();
        assert_eq!(intents[idx].logical_timestamp, 50);
    }

    #[test]
    fn test_evict_empty() {
        let intents: Vec<OverlayIntent> = vec![];
        assert!(find_eviction_target(&intents).is_none());
    }
}
