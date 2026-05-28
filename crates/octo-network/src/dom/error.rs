//! DOM Error Types — RFC-0857 §10

use thiserror::Error;

/// Deterministic Overlay Mempool error enum.
#[derive(Error, Debug)]
pub enum DomError {
    #[error("Invalid signature for intent {intent_id:?}")]
    InvalidSignature { intent_id: [u8; 32] },

    #[error("Replay detected for intent {intent_id:?}, first seen at {first_seen}")]
    ReplayDetected {
        intent_id: [u8; 32],
        first_seen: u64,
    },

    #[error("Sequence invalid for sender {sender_id:?}: got {sequence}")]
    SequenceInvalid { sender_id: [u8; 32], sequence: u64 },

    #[error("Mission {mission_id:?} unauthorized for sender {sender_id:?}")]
    MissionUnauthorized {
        mission_id: [u8; 32],
        sender_id: [u8; 32],
    },

    #[error("Capacity exceeded for scope {scope:#x}: max {max_entries}")]
    CapacityExceeded { scope: u16, max_entries: u32 },

    #[error("Invalid intent type: {intent_type:#x}")]
    InvalidIntentType { intent_type: u16 },

    #[error("Invalid execution class: {execution_class:#x}")]
    InvalidExecutionClass { execution_class: u16 },

    #[error("Fee insufficient: required {required}, provided {provided}")]
    FeeInsufficient { required: u64, provided: u64 },

    #[error("Serialization error: {reason}")]
    SerializationError { reason: String },

    #[error("Admission rejected for intent {intent_id:?}: reason {reason:#x}")]
    AdmissionRejected { intent_id: [u8; 32], reason: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_error_display() {
        let err = DomError::InvalidSignature {
            intent_id: [0xAA; 32],
        };
        assert!(err.to_string().contains("Invalid signature"));

        let err = DomError::ReplayDetected {
            intent_id: [0xBB; 32],
            first_seen: 100,
        };
        assert!(err.to_string().contains("Replay detected"));

        let err = DomError::FeeInsufficient {
            required: 100,
            provided: 50,
        };
        assert!(err.to_string().contains("Fee insufficient"));
    }
}
