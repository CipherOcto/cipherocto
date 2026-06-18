//! Mission Lifecycle State Machine (RFC-0855 §3)

use serde::{Deserialize, Serialize};

/// Mission lifecycle states (RFC-0855 §3.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum MissionState {
    Created = 0x0001,
    Discovering = 0x0002,
    Forming = 0x0003,
    Active = 0x0004,
    Degraded = 0x0005,
    Recovering = 0x0006,
    Terminated = 0x0007,
    Archived = 0x0008,
}

/// State transition record for deterministic replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct StateTransition {
    pub from: MissionState,
    pub to: MissionState,
    pub trigger: TransitionTrigger,
    pub epoch: u64,
}

/// Transition trigger types.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(u16)]
pub enum TransitionTrigger {
    GatewayAdvertisement = 0x0001,
    MinParticipantsReached = 0x0002,
    TopologyCommitted = 0x0003,
    FailureThresholdExceeded = 0x0004,
    ReconciliationInitiated = 0x0005,
    StateConvergenceVerified = 0x0006,
    MissionComplete = 0x0007,
    TtlExpired = 0x0008,
    UnrecoverableFailure = 0x0009,
    StateSnapshotCommitted = 0x000A,
}

/// Determine if a state transition is valid per RFC-0855 §3.2.
pub fn is_valid_transition(from: MissionState, to: MissionState) -> bool {
    matches!(
        (from, to),
        (MissionState::Created, MissionState::Discovering)
            | (MissionState::Discovering, MissionState::Forming)
            | (MissionState::Forming, MissionState::Active)
            | (MissionState::Active, MissionState::Degraded)
            | (MissionState::Active, MissionState::Terminated)
            | (MissionState::Degraded, MissionState::Recovering)
            | (MissionState::Degraded, MissionState::Terminated)
            | (MissionState::Recovering, MissionState::Active)
            | (MissionState::Terminated, MissionState::Archived)
    )
}

/// Minimum participants per topology model.
pub fn min_participants_for_state_transition(from: MissionState) -> u32 {
    match from {
        MissionState::Discovering => 2, // min for any topology
        _ => 0,
    }
}

/// Heartbeat configuration.
pub const DEFAULT_HEARTBEAT_INTERVAL: u64 = 10;
pub const DEFAULT_MISSED_HEARTBEATS: u64 = 3;

/// Compute tolerance threshold: floor(active_participants / 3)
pub fn tolerance_threshold(active_participants: u32) -> u32 {
    active_participants / 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert!(is_valid_transition(
            MissionState::Created,
            MissionState::Discovering
        ));
        assert!(is_valid_transition(
            MissionState::Discovering,
            MissionState::Forming
        ));
        assert!(is_valid_transition(
            MissionState::Forming,
            MissionState::Active
        ));
        assert!(is_valid_transition(
            MissionState::Active,
            MissionState::Degraded
        ));
        assert!(is_valid_transition(
            MissionState::Active,
            MissionState::Terminated
        ));
        assert!(is_valid_transition(
            MissionState::Degraded,
            MissionState::Recovering
        ));
        assert!(is_valid_transition(
            MissionState::Recovering,
            MissionState::Active
        ));
        assert!(is_valid_transition(
            MissionState::Terminated,
            MissionState::Archived
        ));
        assert!(is_valid_transition(
            MissionState::Degraded,
            MissionState::Terminated
        ));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!is_valid_transition(
            MissionState::Created,
            MissionState::Active
        ));
        assert!(!is_valid_transition(
            MissionState::Active,
            MissionState::Created
        ));
        assert!(!is_valid_transition(
            MissionState::Archived,
            MissionState::Active
        ));
        assert!(!is_valid_transition(
            MissionState::Terminated,
            MissionState::Active
        ));
        assert!(!is_valid_transition(
            MissionState::Forming,
            MissionState::Degraded
        ));
    }

    #[test]
    fn test_tolerance_threshold() {
        assert_eq!(tolerance_threshold(9), 3);
        assert_eq!(tolerance_threshold(10), 3);
        assert_eq!(tolerance_threshold(3), 1);
        assert_eq!(tolerance_threshold(2), 0);
        assert_eq!(tolerance_threshold(0), 0);
    }

    #[test]
    fn test_state_repr_values() {
        assert_eq!(MissionState::Created as u16, 0x0001);
        assert_eq!(MissionState::Archived as u16, 0x0008);
    }

    #[test]
    fn test_state_transition_record() {
        let t = StateTransition {
            from: MissionState::Created,
            to: MissionState::Discovering,
            trigger: TransitionTrigger::GatewayAdvertisement,
            epoch: 100,
        };
        assert_eq!(t.from, MissionState::Created);
        assert_eq!(t.to, MissionState::Discovering);
        assert_eq!(t.epoch, 100);
    }
}
