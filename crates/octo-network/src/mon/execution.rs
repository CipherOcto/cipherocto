//! Distributed Execution Layer (RFC-0855 §10, §15)
//!
//! AI swarm coordination, compute job distribution, federated inference,
//! and proof-carrying mission execution.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::mon::mission_id::MissionId;

// -- Core Types --

/// Task type constants for execution tasks.
pub mod task_type {
    /// AI inference request
    pub const INFERENCE: u16 = 0x0001;
    /// Compute job (general purpose)
    pub const COMPUTE: u16 = 0x0002;
    /// Federated training gradient
    pub const FEDERATED_GRADIENT: u16 = 0x0003;
    /// Consensus validation
    pub const CONSENSUS_VALIDATION: u16 = 0x0004;
    /// Simulation step
    pub const SIMULATION: u16 = 0x0005;
    /// Analytics query
    pub const ANALYTICS: u16 = 0x0006;
    /// Workflow orchestration
    pub const ORCHESTRATION: u16 = 0x0007;
}

/// Execution task dispatched to mission executors (RFC-0855 §10.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct ExecutionTask {
    /// Unique task identifier within mission
    pub task_id: [u8; 32],
    /// Mission this task belongs to
    pub mission_id: MissionId,
    /// Task type (task_type constants)
    pub task_type: u16,
    /// BLAKE3-256 hash of the task payload
    pub payload_hash: [u8; 32],
    /// Assigned executor (zero = broadcast to all executors)
    pub assigned_executor: [u8; 32],
    /// Deadline epoch (0 = no deadline)
    pub deadline_epoch: u64,
    /// Priority class (higher = more urgent)
    pub priority: u8,
    /// Whether a ZK proof is required for the result
    pub proof_required: bool,
}

impl ExecutionTask {
    /// Check if this task is broadcast (no specific executor assigned).
    pub fn is_broadcast(&self) -> bool {
        self.assigned_executor == [0u8; 32]
    }

    /// Check if this task has expired relative to current epoch.
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        self.deadline_epoch > 0 && current_epoch > self.deadline_epoch
    }

    /// Compute signing bytes for this task.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.task_id);
        bytes.extend_from_slice(&self.mission_id.to_canonical_bytes());
        bytes.extend_from_slice(&self.task_type.to_be_bytes());
        bytes.extend_from_slice(&self.payload_hash);
        bytes.extend_from_slice(&self.assigned_executor);
        bytes.extend_from_slice(&self.deadline_epoch.to_be_bytes());
        bytes.push(self.priority);
        bytes.push(self.proof_required as u8);
        bytes
    }
}

/// Task result submitted by executor (RFC-0855 §10.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct TaskResult {
    /// Task this result is for
    pub task_id: [u8; 32],
    /// Executor that produced this result
    pub executor_id: [u8; 32],
    /// BLAKE3-256 of the result payload
    pub result_hash: [u8; 32],
    /// BLAKE3-256 of the ZK proof blob (zero if no proof)
    pub proof_commitment: [u8; 32],
    /// Epoch when result was produced
    pub epoch: u64,
}

/// Proof-carrying execution result (RFC-0855 §15).
///
/// Wraps a TaskResult with the actual proof blob for
/// proof-carrying missions.
#[derive(Clone, Debug)]
pub struct ProofCarryingResult {
    /// The task result
    pub result: TaskResult,
    /// The ZK proof blob
    pub proof_blob: Vec<u8>,
    /// Public inputs for proof verification
    pub public_inputs: Vec<[u8; 32]>,
}

impl ProofCarryingResult {
    /// Verify the proof commitment matches the blob.
    pub fn verify_proof_commitment(&self) -> bool {
        if self.proof_blob.is_empty() {
            return self.result.proof_commitment == [0u8; 32];
        }
        let expected = *blake3::hash(&self.proof_blob).as_bytes();
        self.result.proof_commitment == expected
    }
}

// -- Job Distribution --

/// Executor capability profile for job assignment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutorCapability {
    /// Executor gateway ID
    pub executor_id: [u8; 32],
    /// Supported task types (bitmask)
    pub supported_types: u16,
    /// Trust score (0-10000)
    pub trust_score: u16,
    /// Current load (number of active tasks)
    pub current_load: u32,
    /// Maximum concurrent tasks
    pub max_concurrent: u32,
}

impl ExecutorCapability {
    /// Whether this executor can accept more work.
    pub fn is_available(&self) -> bool {
        self.current_load < self.max_concurrent
    }

    /// Whether this executor supports a given task type.
    pub fn supports_type(&self, task_type: u16) -> bool {
        (self.supported_types & task_type) != 0
    }
}

/// Job distributor — assigns tasks to executors based on capability.
///
/// RFC-0855 §10.2: Coordinator assigns jobs to Executors based on capability.
pub struct JobDistributor {
    /// Available executors
    executors: Vec<ExecutorCapability>,
}

impl JobDistributor {
    /// Create a new job distributor with the given executors.
    pub fn new(executors: Vec<ExecutorCapability>) -> Self {
        Self { executors }
    }

    /// Select the best executor for a task.
    ///
    /// Selection criteria (in order):
    /// 1. Must support the task type
    /// 2. Must be available (not at capacity)
    /// 3. Highest trust score wins
    /// 4. Lowest current load breaks ties
    pub fn select_executor(&self, task: &ExecutionTask) -> Option<[u8; 32]> {
        self.executors
            .iter()
            .filter(|e| e.supports_type(task.task_type) & e.is_available())
            .min_by(|a, b| {
                // Higher trust score is better (reverse sort)
                b.trust_score
                    .cmp(&a.trust_score)
                    .then(a.current_load.cmp(&b.current_load))
            })
            .map(|e| e.executor_id)
    }

    /// Distribute a task: assign to best executor and return updated task.
    pub fn distribute(&self, mut task: ExecutionTask) -> Option<ExecutionTask> {
        let executor = self.select_executor(&task)?;
        task.assigned_executor = executor;
        Some(task)
    }

    /// Number of available executors.
    pub fn available_count(&self) -> usize {
        self.executors.iter().filter(|e| e.is_available()).count()
    }
}

// -- Swarm Coordination --

/// Swarm coordinator — manages multiple agents on a shared mission.
///
/// RFC-0855 §10.1: AI swarm coordination where multiple agents
/// coordinate on shared mission objectives.
pub struct SwarmCoordinator {
    /// Mission this swarm is working on
    pub mission_id: MissionId,
    /// Active agents in the swarm
    agents: BTreeMap<[u8; 32], AgentStatus>,
    /// Pending task assignments
    assignments: BTreeMap<[u8; 32], [u8; 32]>, // task_id -> executor_id
}

/// Agent status within a swarm.
#[derive(Clone, Debug)]
pub struct AgentStatus {
    /// Agent gateway ID
    pub agent_id: [u8; 32],
    /// Current task being worked on (None if idle)
    pub current_task: Option<[u8; 32]>,
    /// Number of completed tasks
    pub completed_count: u32,
    /// Whether the agent is active
    pub is_active: bool,
}

impl SwarmCoordinator {
    /// Create a new swarm coordinator for a mission.
    pub fn new(mission_id: MissionId) -> Self {
        Self {
            mission_id,
            agents: BTreeMap::new(),
            assignments: BTreeMap::new(),
        }
    }

    /// Register an agent with the swarm.
    pub fn register_agent(&mut self, agent_id: [u8; 32]) {
        self.agents.insert(
            agent_id,
            AgentStatus {
                agent_id,
                current_task: None,
                completed_count: 0,
                is_active: true,
            },
        );
    }

    /// Assign a task to an agent. Returns false if:
    /// - the agent is not registered
    /// - the agent already has a current_task (the previous
    ///   assignment is not automatically cancelled; the caller
    ///   must call `complete_task` first)
    pub fn assign_task(&mut self, agent_id: &[u8; 32], task_id: [u8; 32]) -> bool {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            if agent.current_task.is_some() {
                // Don't silently overwrite — this would orphan
                // the previous task in self.assignments and
                // prevent complete_task from ever resolving it.
                return false;
            }
            agent.current_task = Some(task_id);
            self.assignments.insert(task_id, *agent_id);
            return true;
        }
        false
    }

    /// Mark a task as completed by an agent.
    pub fn complete_task(&mut self, agent_id: &[u8; 32], task_id: &[u8; 32]) -> bool {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            if agent.current_task == Some(*task_id) {
                agent.current_task = None;
                agent.completed_count += 1;
                self.assignments.remove(task_id);
                return true;
            }
        }
        false
    }

    /// Get idle agents (no current task, active).
    pub fn idle_agents(&self) -> Vec<[u8; 32]> {
        self.agents
            .values()
            .filter(|a| a.is_active && a.current_task.is_none())
            .map(|a| a.agent_id)
            .collect()
    }

    /// Number of active agents.
    pub fn active_count(&self) -> usize {
        self.agents.values().filter(|a| a.is_active).count()
    }

    /// Number of pending assignments.
    pub fn pending_count(&self) -> usize {
        self.assignments.len()
    }
}

// -- Federated Inference --

/// Federated inference result — aggregated from multiple nodes.
#[derive(Clone, Debug)]
pub struct FederatedInferenceResult {
    /// Mission this inference belongs to
    pub mission_id: MissionId,
    /// Individual node results
    pub node_results: Vec<[u8; 32]>,
    /// Aggregated result hash
    pub aggregated_result: [u8; 32],
    /// Number of nodes that contributed
    pub node_count: u32,
}

impl FederatedInferenceResult {
    /// Compute aggregated result from node results.
    ///
    /// Uses BLAKE3-256 Merkle root of sorted node results for determinism.
    pub fn compute_aggregated_result(node_results: &[[u8; 32]]) -> [u8; 32] {
        crate::common::merkle::compute_merkle_root(node_results)
    }

    /// Create a new federated inference result.
    pub fn new(mission_id: MissionId, mut node_results: Vec<[u8; 32]>) -> Self {
        node_results.sort(); // deterministic ordering
        let aggregated_result = Self::compute_aggregated_result(&node_results);
        let node_count = node_results.len() as u32;
        Self {
            mission_id,
            node_results,
            aggregated_result,
            node_count,
        }
    }

    /// Verify the aggregated result matches the node results.
    pub fn verify(&self) -> bool {
        let expected = Self::compute_aggregated_result(&self.node_results);
        self.aggregated_result == expected
    }
}

// -- Helpers --

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

    fn test_mission_id() -> MissionId {
        MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1)
    }

    fn make_task(task_type: u16, priority: u8) -> ExecutionTask {
        ExecutionTask {
            task_id: [0x01; 32],
            mission_id: test_mission_id(),
            task_type,
            payload_hash: [0x02; 32],
            assigned_executor: [0u8; 32],
            deadline_epoch: 1000,
            priority,
            proof_required: false,
        }
    }

    fn make_executor(id_byte: u8, types: u16, trust: u16, load: u32) -> ExecutorCapability {
        ExecutorCapability {
            executor_id: [id_byte; 32],
            supported_types: types,
            trust_score: trust,
            current_load: load,
            max_concurrent: 10,
        }
    }

    // -- ExecutionTask tests --

    #[test]
    fn test_task_is_broadcast() {
        let mut task = make_task(task_type::INFERENCE, 5);
        assert!(task.is_broadcast());
        task.assigned_executor = [0xFF; 32];
        assert!(!task.is_broadcast());
    }

    #[test]
    fn test_task_is_expired() {
        let task = make_task(task_type::INFERENCE, 5);
        assert!(!task.is_expired(500));
        assert!(!task.is_expired(1000));
        assert!(task.is_expired(1001));
    }

    #[test]
    fn test_task_no_deadline() {
        let mut task = make_task(task_type::INFERENCE, 5);
        task.deadline_epoch = 0;
        assert!(!task.is_expired(u64::MAX));
    }

    #[test]
    fn test_task_signing_bytes_deterministic() {
        let task = make_task(task_type::INFERENCE, 5);
        let b1 = task.to_signing_bytes();
        let b2 = task.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    // -- ProofCarryingResult tests --

    #[test]
    fn test_proof_carrying_result_verify() {
        let blob = vec![1, 2, 3];
        let commitment = *blake3::hash(&blob).as_bytes();
        let result = TaskResult {
            task_id: [0x01; 32],
            executor_id: [0x02; 32],
            result_hash: [0x03; 32],
            proof_commitment: commitment,
            epoch: 100,
        };
        let pcr = ProofCarryingResult {
            result,
            proof_blob: blob,
            public_inputs: vec![[0xAA; 32]],
        };
        assert!(pcr.verify_proof_commitment());
    }

    #[test]
    fn test_proof_carrying_result_no_proof() {
        let result = TaskResult {
            task_id: [0x01; 32],
            executor_id: [0x02; 32],
            result_hash: [0x03; 32],
            proof_commitment: [0u8; 32],
            epoch: 100,
        };
        let pcr = ProofCarryingResult {
            result,
            proof_blob: vec![],
            public_inputs: vec![],
        };
        assert!(pcr.verify_proof_commitment());
    }

    #[test]
    fn test_proof_carrying_result_tampered() {
        let result = TaskResult {
            task_id: [0x01; 32],
            executor_id: [0x02; 32],
            result_hash: [0x03; 32],
            proof_commitment: [0xFF; 32], // wrong
            epoch: 100,
        };
        let pcr = ProofCarryingResult {
            result,
            proof_blob: vec![1, 2, 3],
            public_inputs: vec![],
        };
        assert!(!pcr.verify_proof_commitment());
    }

    // -- JobDistributor tests --

    #[test]
    fn test_distributor_select_best() {
        let distributor = JobDistributor::new(vec![
            make_executor(0x01, task_type::INFERENCE, 5000, 5),
            make_executor(0x02, task_type::INFERENCE, 9000, 2),
            make_executor(0x03, task_type::COMPUTE, 8000, 0), // wrong type
        ]);
        let task = make_task(task_type::INFERENCE, 5);
        let selected = distributor.select_executor(&task).unwrap();
        assert_eq!(selected, [0x02; 32]); // highest trust, lowest load
    }

    #[test]
    fn test_distributor_no_capable_executor() {
        let distributor =
            JobDistributor::new(vec![make_executor(0x01, task_type::COMPUTE, 5000, 0)]);
        let task = make_task(task_type::INFERENCE, 5);
        assert!(distributor.select_executor(&task).is_none());
    }

    #[test]
    fn test_distributor_all_at_capacity() {
        let distributor = JobDistributor::new(vec![
            make_executor(0x01, task_type::INFERENCE, 9000, 10), // at max
        ]);
        let task = make_task(task_type::INFERENCE, 5);
        assert!(distributor.select_executor(&task).is_none());
    }

    #[test]
    fn test_distributor_distribute() {
        let distributor =
            JobDistributor::new(vec![make_executor(0x01, task_type::INFERENCE, 5000, 0)]);
        let task = make_task(task_type::INFERENCE, 5);
        let distributed = distributor.distribute(task).unwrap();
        assert_eq!(distributed.assigned_executor, [0x01; 32]);
    }

    #[test]
    fn test_distributor_available_count() {
        let distributor = JobDistributor::new(vec![
            make_executor(0x01, task_type::INFERENCE, 5000, 0),
            make_executor(0x02, task_type::INFERENCE, 5000, 10), // at capacity
            make_executor(0x03, task_type::COMPUTE, 5000, 0),
        ]);
        assert_eq!(distributor.available_count(), 2);
    }

    // -- SwarmCoordinator tests --

    #[test]
    fn test_swarm_register_and_assign() {
        let mut swarm = SwarmCoordinator::new(test_mission_id());
        swarm.register_agent([0x01; 32]);
        assert_eq!(swarm.active_count(), 1);
        assert!(swarm.assign_task(&[0x01; 32], [0xAA; 32]));
        assert_eq!(swarm.pending_count(), 1);
    }

    #[test]
    fn test_swarm_complete_task() {
        let mut swarm = SwarmCoordinator::new(test_mission_id());
        swarm.register_agent([0x01; 32]);
        swarm.assign_task(&[0x01; 32], [0xAA; 32]);
        assert!(swarm.complete_task(&[0x01; 32], &[0xAA; 32]));
        assert_eq!(swarm.pending_count(), 0);
    }

    #[test]
    fn test_swarm_idle_agents() {
        let mut swarm = SwarmCoordinator::new(test_mission_id());
        swarm.register_agent([0x01; 32]);
        swarm.register_agent([0x02; 32]);
        swarm.assign_task(&[0x01; 32], [0xAA; 32]);
        let idle = swarm.idle_agents();
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0], [0x02; 32]);
    }

    #[test]
    fn test_swarm_assign_unknown_agent() {
        let mut swarm = SwarmCoordinator::new(test_mission_id());
        assert!(!swarm.assign_task(&[0xFF; 32], [0xAA; 32]));
    }

    #[test]
    fn test_swarm_assign_busy_agent() {
        // If an agent already has a current_task, a second
        // assign_task call must not silently overwrite it
        // (which would orphan the previous task in
        // self.assignments and leak a pending count).
        let mut swarm = SwarmCoordinator::new(test_mission_id());
        swarm.register_agent([0x01; 32]);
        assert!(swarm.assign_task(&[0x01; 32], [0xAA; 32]));
        // Second assign must fail.
        assert!(!swarm.assign_task(&[0x01; 32], [0xBB; 32]));
        // The first task is still the agent's current_task.
        assert_eq!(swarm.pending_count(), 1);
        // And it's still in assignments.
        assert!(swarm.complete_task(&[0x01; 32], &[0xAA; 32]));
        assert_eq!(swarm.pending_count(), 0);
    }

    // -- FederatedInference tests --

    #[test]
    fn test_federated_inference_deterministic() {
        let results = vec![[0x01; 32], [0x02; 32], [0x03; 32]];
        let r1 = FederatedInferenceResult::new(test_mission_id(), results.clone());
        let r2 = FederatedInferenceResult::new(test_mission_id(), results);
        assert_eq!(r1.aggregated_result, r2.aggregated_result);
    }

    #[test]
    fn test_federated_inference_order_independent() {
        let r1 = FederatedInferenceResult::new(test_mission_id(), vec![[0x01; 32], [0x02; 32]]);
        let r2 = FederatedInferenceResult::new(
            test_mission_id(),
            vec![[0x02; 32], [0x01; 32]], // different order
        );
        assert_eq!(r1.aggregated_result, r2.aggregated_result);
    }

    #[test]
    fn test_federated_inference_verify() {
        let result = FederatedInferenceResult::new(
            test_mission_id(),
            vec![[0x01; 32], [0x02; 32], [0x03; 32]],
        );
        assert!(result.verify());
    }

    #[test]
    fn test_federated_inference_verify_tampered() {
        let mut result =
            FederatedInferenceResult::new(test_mission_id(), vec![[0x01; 32], [0x02; 32]]);
        result.aggregated_result = [0xFF; 32];
        assert!(!result.verify());
    }

    // -- Helper tests --

    #[test]
    fn test_compute_result_hash() {
        let h1 = compute_result_hash(&[0x01; 32], b"payload");
        let h2 = compute_result_hash(&[0x01; 32], b"payload");
        assert_eq!(h1, h2);
        let h3 = compute_result_hash(&[0x01; 32], b"different");
        assert_ne!(h1, h3);
    }
}
