//! Distributed Execution Layer (RFC-0855 §10)

use serde::{Deserialize, Serialize};

/// Execution task dispatched to mission executors.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ExecutionTask {
    pub task_id: [u8; 32],
    pub mission_id: [u8; 32],
    pub task_type: u16,
    pub payload_hash: [u8; 32],
    pub assigned_executor: [u8; 32],
    pub deadline_epoch: u64,
    pub priority: u8,
}

/// Task result submitted by executor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct TaskResult {
    pub task_id: [u8; 32],
    pub executor_id: [u8; 32],
    pub result_hash: [u8; 32],
    pub proof_commitment: [u8; 32],
    pub epoch: u64,
}

/// Compute result hash: BLAKE3-256(task_id || result_payload)
pub fn compute_result_hash(task_id: &[u8; 32], result_payload: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(task_id);
    hasher.update(result_payload);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_result_hash_deterministic() {
        let task_id = [1u8; 32];
        let payload = b"result data";
        let h1 = compute_result_hash(&task_id, payload);
        let h2 = compute_result_hash(&task_id, payload);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_result_hash_different_payloads() {
        let task_id = [1u8; 32];
        let h1 = compute_result_hash(&task_id, b"payload A");
        let h2 = compute_result_hash(&task_id, b"payload B");
        assert_ne!(h1, h2);
    }
}
