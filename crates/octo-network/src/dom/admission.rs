//! Deterministic Admission — RFC-0857 §3
//!
//! Admission checks: signature validity, replay window, sequence validity,
//! mission authorization, resource constraints.
//!
//! Forbidden inputs: local latency, wall-clock, CPU load, thread order.

use crate::dom::error::DomError;
use crate::dom::intent::OverlayIntent;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::collections::BTreeMap;

/// Sequence tracker per (sender_id, mission_id).
/// Uses [u8; 32] keys to avoid heap allocation on the hot path.
pub type SequenceTracker = BTreeMap<([u8; 32], [u8; 32]), u64>;

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
///
/// Check ordering: cheap checks first, expensive signature verification last.
pub fn check_admission(
    intent: &OverlayIntent,
    current_timestamp: u64,
    replay_cache: &ReplayCache,
    sequence_tracker: &SequenceTracker,
    pending_count: u32,
    config: &AdmissionConfig,
) -> Result<(), DomError> {
    // 1. Check expiration (O(1), cheapest)
    if intent.expiration <= current_timestamp {
        return Err(DomError::AdmissionRejected {
            intent_id: intent.intent_id,
            reason: 0x0001, // expired
        });
    }

    // 2. Check replay (O(log n) BTreeMap lookup)
    if replay_cache.contains_key(&intent.intent_id) {
        return Err(DomError::ReplayDetected {
            intent_id: intent.intent_id,
            first_seen: 0,
        });
    }

    // 3. Validate execution class range (O(1))
    if intent.execution_class > 0x0006 {
        return Err(DomError::InvalidExecutionClass {
            execution_class: intent.execution_class,
        });
    }

    // 4. Validate intent type range (O(1))
    if intent.intent_type == 0 || intent.intent_type > 0x0008 {
        return Err(DomError::InvalidIntentType {
            intent_type: intent.intent_type,
        });
    }

    // 5. Check global capacity using actual pending count (O(1))
    if pending_count >= config.max_pending_intents {
        return Err(DomError::CapacityExceeded {
            scope: 0x0001, // GLOBAL
            max_entries: config.max_pending_intents,
        });
    }

    // 6. Check sequence monotonicity (O(log n))
    let key = (intent.sender_id, intent.mission_id);
    if let Some(&last_seq) = sequence_tracker.get(&key) {
        if intent.sequence <= last_seq {
            return Err(DomError::SequenceInvalid {
                sender_id: intent.sender_id,
                sequence: intent.sequence,
            });
        }
    }

    // 7. Ed25519 signature verification (expensive, ~50-100us, done last)
    let vk =
        VerifyingKey::from_bytes(&intent.sender_id).map_err(|_| DomError::InvalidSignature {
            intent_id: intent.intent_id,
        })?;
    let sig = Signature::from_bytes(&intent.signature);
    let msg = intent.to_signing_bytes();
    vk.verify(&msg, &sig)
        .map_err(|_| DomError::InvalidSignature {
            intent_id: intent.intent_id,
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::intent::ExecutionClass;
    use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

    fn make_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0xCCu8; 32])
    }

    fn make_signed_intent(seq: u64, ts: u64, exp: u64) -> (OverlayIntent, VerifyingKey) {
        let sk = make_signing_key();
        let vk = sk.verifying_key();
        let intent = OverlayIntent {
            intent_id: [0xAA; 32],
            intent_type: 0x0001,
            mission_id: [0xBB; 32],
            sender_id: vk.to_bytes(),
            sequence: seq,
            logical_timestamp: ts,
            expiration: exp,
            payload_root: [0u8; 32],
            economic_weight: 100,
            execution_class: ExecutionClass::Economic as u16,
            signature: [0u8; 64], // placeholder
        };
        let msg = intent.to_signing_bytes();
        let sig = sk.sign(&msg);
        let mut signed = intent;
        signed.signature = sig.to_bytes();
        (signed, vk)
    }

    fn default_config() -> AdmissionConfig {
        AdmissionConfig::default()
    }

    #[test]
    fn test_admission_signature_valid() {
        let (intent, _vk) = make_signed_intent(1, 100, 200);
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_admission_signature_invalid() {
        let (mut intent, _vk) = make_signed_intent(1, 100, 200);
        intent.signature[0] ^= 0xFF; // corrupt signature
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(matches!(result, Err(DomError::InvalidSignature { .. })));
    }

    #[test]
    fn test_admission_expired() {
        let (intent, _vk) = make_signed_intent(1, 100, 50); // expiration=50, current=100
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(matches!(result, Err(DomError::AdmissionRejected { .. })));
    }

    #[test]
    fn test_admission_replay() {
        let (intent, _vk) = make_signed_intent(1, 100, 200);
        let mut replay = BTreeMap::new();
        replay.insert(intent.intent_id, 50u64);
        let result = check_admission(
            &intent,
            100,
            &replay,
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(matches!(result, Err(DomError::ReplayDetected { .. })));
    }

    #[test]
    fn test_admission_sequence_stale() {
        let (intent, _vk) = make_signed_intent(5, 100, 200);
        let mut seq: SequenceTracker = BTreeMap::new();
        seq.insert((intent.sender_id, intent.mission_id), 10u64);
        let result = check_admission(&intent, 100, &BTreeMap::new(), &seq, 0, &default_config());
        assert!(matches!(result, Err(DomError::SequenceInvalid { .. })));
    }

    #[test]
    fn test_admission_invalid_class() {
        let (mut intent, _vk) = make_signed_intent(1, 100, 200);
        intent.execution_class = 0x00FF;
        // Re-sign after mutation
        let sk = make_signing_key();
        let msg = intent.to_signing_bytes();
        intent.signature = sk.sign(&msg).to_bytes();
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(matches!(
            result,
            Err(DomError::InvalidExecutionClass { .. })
        ));
    }

    #[test]
    fn test_admission_invalid_type() {
        let (mut intent, _vk) = make_signed_intent(1, 100, 200);
        intent.intent_type = 0x0000;
        // Re-sign after mutation
        let sk = make_signing_key();
        let msg = intent.to_signing_bytes();
        intent.signature = sk.sign(&msg).to_bytes();
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(matches!(result, Err(DomError::InvalidIntentType { .. })));
    }

    #[test]
    fn test_admission_success() {
        let (intent, _vk) = make_signed_intent(1, 100, 200);
        let result = check_admission(
            &intent,
            100,
            &BTreeMap::new(),
            &BTreeMap::new(),
            0,
            &default_config(),
        );
        assert!(result.is_ok());
    }
}
