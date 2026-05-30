//! Partition Resilience (RFC-0855 §13)

use serde::{Deserialize, Serialize};

use crate::mon::mission_id::MissionId;

/// Partition event — a domain isolation or failure event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartitionEvent {
    pub domain_hash: [u8; 32],
    pub epoch: u64,
    pub affected_peers: Vec<[u8; 32]>,
}

/// Reconciliation state tracking.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ReconciliationState {
    pub mission_id: MissionId,
    pub partition_epoch: u64,
    pub local_state_root: [u8; 32],
    pub remote_state_root: [u8; 32],
    pub converged: bool,
    pub verified: bool,
}

impl ReconciliationState {
    pub fn new(mission_id: MissionId, partition_epoch: u64) -> Self {
        Self {
            mission_id,
            partition_epoch,
            local_state_root: [0xFF; 32],
            remote_state_root: [0x00; 32],
            converged: false,
            verified: false,
        }
    }

    /// Check if local and remote roots match (convergence).
    /// Only returns true if roots match AND both have been verified.
    pub fn check_convergence(&mut self) -> bool {
        self.converged = self.verified && self.local_state_root == self.remote_state_root;
        self.converged
    }

    /// Mark state roots as verified (both sides have been set to real values).
    pub fn verify(&mut self) {
        self.verified = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::mission_id::MissionId;

    fn test_mission_id() -> MissionId {
        MissionId::new(1, &[1u8; 32], 100, &[2u8; 32], 1)
    }

    #[test]
    fn test_reconciliation_new() {
        let rs = ReconciliationState::new(test_mission_id(), 100);
        assert_eq!(rs.partition_epoch, 100);
        assert!(!rs.converged);
        assert!(!rs.verified);
    }

    #[test]
    fn test_reconciliation_no_false_convergence() {
        let rs = ReconciliationState::new(test_mission_id(), 100);
        // Without verify(), convergence must be false even though sentinels differ
        assert!(!rs.converged);
    }

    #[test]
    fn test_reconciliation_convergence() {
        let mut rs = ReconciliationState::new(test_mission_id(), 100);
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xAA; 32];
        rs.verify();
        assert!(rs.check_convergence());
    }

    #[test]
    fn test_reconciliation_no_convergence() {
        let mut rs = ReconciliationState::new(test_mission_id(), 100);
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xBB; 32];
        rs.verify();
        assert!(!rs.check_convergence());
    }

    #[test]
    fn test_reconciliation_requires_verify() {
        let mut rs = ReconciliationState::new(test_mission_id(), 100);
        // Set matching roots but don't verify
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xAA; 32];
        // Should still be false without verify()
        assert!(!rs.check_convergence());
    }
}
