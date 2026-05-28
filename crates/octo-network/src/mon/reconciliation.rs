//! Partition Resilience (RFC-0855 §13)

use serde::{Deserialize, Serialize};

/// Partition event — a domain isolation or failure event.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct PartitionEvent {
    pub domain_hash: [u8; 32],
    pub epoch: u64,
    pub affected_peers: Vec<[u8; 32]>,
}

/// Reconciliation state tracking.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ReconciliationState {
    pub mission_id: [u8; 32],
    pub partition_epoch: u64,
    pub local_state_root: [u8; 32],
    pub remote_state_root: [u8; 32],
    pub converged: bool,
}

impl ReconciliationState {
    pub fn new(mission_id: [u8; 32], partition_epoch: u64) -> Self {
        Self {
            mission_id,
            partition_epoch,
            local_state_root: [0u8; 32],
            remote_state_root: [0u8; 32],
            converged: false,
        }
    }

    /// Check if local and remote roots match (convergence).
    pub fn check_convergence(&mut self) -> bool {
        self.converged = self.local_state_root == self.remote_state_root;
        self.converged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconciliation_new() {
        let rs = ReconciliationState::new([1u8; 32], 100);
        assert_eq!(rs.partition_epoch, 100);
        assert!(!rs.converged);
    }

    #[test]
    fn test_reconciliation_convergence() {
        let mut rs = ReconciliationState::new([1u8; 32], 100);
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xAA; 32];
        assert!(rs.check_convergence());
    }

    #[test]
    fn test_reconciliation_no_convergence() {
        let mut rs = ReconciliationState::new([1u8; 32], 100);
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xBB; 32];
        assert!(!rs.check_convergence());
    }
}
