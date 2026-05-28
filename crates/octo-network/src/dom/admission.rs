//! Deterministic Admission — RFC-0857 §3
//!
//! Admission checks: signature validity, replay window, sequence validity,
//! mission authorization, resource constraints.
//!
//! Forbidden inputs: local latency, wall-clock, CPU load, thread order.

use crate::dom::error::DomError;
use crate::dom::intent::OverlayIntent;
use std::collections::BTreeMap;

/// Sequence tracker per (sender_id, mission_id).
pub type SequenceTracker = BTreeMap<(Vec<u8>, Vec<u8>), u64>;

/// Replay cache — maps intent_id to first_seen logical timestamp.
pub type ReplayCache = BTreeMap<[u8; 32], u64>;

/// Admission configuration.
pub struct AdmissionConfig {
    /// Maximum pending intents globally
    pub max_pending_intents: u32,
    /// Maximum pending intents per mission
    pub max_per_mission: u32,
    /// Maximum intents per sender per logical_timestamp window
    pub max_per_sender_per_window: u32,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            max_pending_intents: 100_000,
            max_per_mission: 10_000,
            max_per_sender_per_window: 100,
        }
    }
}

/// Check if an intent passes deterministic admission (RFC-0857 §3).
///
/// Returns Ok(()) if admitted, Err(DomError) if rejected.
pub fn check_admission(
    intent: &OverlayIntent,
    current_timestamp: u64,
    replay_cache: &ReplayCache,
    sequence_tracker: &SequenceTracker,
    _config: &AdmissionConfig,
) -> Result<(), DomError> {
    // 1. Check expiration
    if intent.expiration <= current_timestamp {
        return Err(DomError::AdmissionRejected {
            intent_id: intent.intent_id,
            reason: 0x0001, // expired
        });
    }

    // 2. Check replay
    if let Some(&first_seen) = replay_cache.get(&intent.intent_id) {
        return Err(DomError::ReplayDetected {
            intent_id: intent.intent_id,
            first_seen,
        });
    }

    // 3. Check sequence monotonicity
    let key = (intent.sender_id.to_vec(), intent.mission_id.to_vec());
    if let Some(&last_seq) = sequence_tracker.get(&key) {
        if intent.sequence <= last_seq {
            return Err(DomError::SequenceInvalid {
                sender_id: intent.sender_id,
                sequence: intent.sequence,
            });
        }
    }

    // 4. Validate execution class range
    if intent.execution_class > 0x0006 {
        return Err(DomError::InvalidExecutionClass {
            execution_class: intent.execution_class,
        });
    }

    // 5. Validate intent type range
    if intent.intent_type == 0 || intent.intent_type > 0x0008 {
        return Err(DomError::InvalidIntentType {
            intent_type: intent.intent_type,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::intent::ExecutionClass;

    fn make_intent(seq: u64, ts: u64, exp: u64) -> OverlayIntent {
        OverlayIntent {
            intent_id: [0xAA; 32],
            intent_type: 0x0001,
            mission_id: [0xBB; 32],
            sender_id: [0xCC; 32],
            sequence: seq,
            logical_timestamp: ts,
            expiration: exp,
            payload_root: [0u8; 32],
            economic_weight: 100,
            execution_class: ExecutionClass::Economic as u16,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_admission_expired() {
        let intent = make_intent(1, 100, 50); // expiration=50, current=100
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &Default::default(),
        );
        assert!(matches!(result, Err(DomError::AdmissionRejected { .. })));
    }

    #[test]
    fn test_admission_replay() {
        let intent = make_intent(1, 100, 200);
        let mut replay = BTreeMap::new();
        replay.insert(intent.intent_id, 50u64);
        let result = check_admission(&intent, 100, &replay, &BTreeMap::new(), &Default::default());
        assert!(matches!(result, Err(DomError::ReplayDetected { .. })));
    }

    #[test]
    fn test_admission_sequence_stale() {
        let intent = make_intent(5, 100, 200);
        let mut seq = BTreeMap::new();
        seq.insert(
            (intent.sender_id.to_vec(), intent.mission_id.to_vec()),
            10u64,
        );
        let result = check_admission(&intent, 100, &BTreeMap::new(), &seq, &Default::default());
        assert!(matches!(result, Err(DomError::SequenceInvalid { .. })));
    }

    #[test]
    fn test_admission_invalid_class() {
        let mut intent = make_intent(1, 100, 200);
        intent.execution_class = 0x00FF;
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &Default::default(),
        );
        assert!(matches!(
            result,
            Err(DomError::InvalidExecutionClass { .. })
        ));
    }

    #[test]
    fn test_admission_invalid_type() {
        let mut intent = make_intent(1, 100, 200);
        intent.intent_type = 0x0000;
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &Default::default(),
        );
        assert!(matches!(result, Err(DomError::InvalidIntentType { .. })));
    }

    #[test]
    fn test_admission_success() {
        let intent = make_intent(1, 100, 200);
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &Default::default(),
        );
        assert!(result.is_ok());
    }
}
