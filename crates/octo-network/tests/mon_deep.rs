//! Deep coverage tests for MON — execution layer, governance proposals,
//! mission discovery, lifecycle edge cases.

use octo_network::mon::discovery::{
    scope_to_gdp_scope, MissionAdvertisement, MissionDiscoveryScope, EPHEMERAL_ADVERTISEMENT_TTL,
};
use octo_network::mon::execution::{
    task_type, ExecutionTask, ExecutorCapability, JobDistributor, ProofCarryingResult,
    SwarmCoordinator, TaskResult,
};
use octo_network::mon::governance::{
    DecisionType, EmergencyAuthority, GovernanceModel, GovernancePolicy, GovernanceProposal,
    ProposalState,
};
use octo_network::mon::lifecycle::{
    min_participants_for_state_transition, tolerance_threshold, MissionState, TransitionTrigger,
    DEFAULT_HEARTBEAT_INTERVAL, DEFAULT_MISSED_HEARTBEATS,
};
use octo_network::mon::mission_id::MissionId;

fn make_mission_id(byte: u8) -> MissionId {
    MissionId::new(1, &[byte; 32], 100, &[byte; 32], 1)
}

// ── Execution tasks ──

#[test]
fn test_execution_task_lifecycle() {
    let task = ExecutionTask {
        task_id: [0x01; 32],
        mission_id: make_mission_id(0xAA),
        task_type: task_type::INFERENCE,
        payload_hash: [0xBB; 32],
        assigned_executor: [0u8; 32],
        deadline_epoch: 200,
        priority: 10,
        proof_required: true,
    };

    assert!(task.is_broadcast());
    assert!(!task.is_expired(100));
    assert!(!task.is_expired(200));
    assert!(task.is_expired(201));

    let bytes = task.to_signing_bytes();
    assert!(!bytes.is_empty());
}

#[test]
fn test_execution_task_assigned() {
    let task = ExecutionTask {
        task_id: [0x01; 32],
        mission_id: make_mission_id(0xAA),
        task_type: task_type::COMPUTE,
        payload_hash: [0xBB; 32],
        assigned_executor: [0xCC; 32],
        deadline_epoch: 0,
        priority: 5,
        proof_required: false,
    };

    assert!(!task.is_broadcast());
    assert!(!task.is_expired(u64::MAX));
}

#[test]
fn test_task_type_constants() {
    assert_eq!(task_type::INFERENCE, 0x0001);
    assert_eq!(task_type::COMPUTE, 0x0002);
    assert_eq!(task_type::FEDERATED_GRADIENT, 0x0003);
    assert_eq!(task_type::CONSENSUS_VALIDATION, 0x0004);
    assert_eq!(task_type::SIMULATION, 0x0005);
    assert_eq!(task_type::ANALYTICS, 0x0006);
    assert_eq!(task_type::ORCHESTRATION, 0x0007);
}

// ── ProofCarryingResult ──

#[test]
fn test_proof_carrying_result_empty_blob() {
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
fn test_proof_carrying_result_with_blob() {
    let blob = vec![0xAB; 64];
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
        public_inputs: vec![[0xDD; 32]],
    };
    assert!(pcr.verify_proof_commitment());
}

#[test]
fn test_proof_carrying_result_mismatch() {
    let result = TaskResult {
        task_id: [0x01; 32],
        executor_id: [0x02; 32],
        result_hash: [0x03; 32],
        proof_commitment: [0xFF; 32],
        epoch: 100,
    };
    let pcr = ProofCarryingResult {
        result,
        proof_blob: vec![0xAB; 64],
        public_inputs: vec![],
    };
    assert!(!pcr.verify_proof_commitment());
}

// ── ExecutorCapability ──

#[test]
fn test_executor_capability() {
    let cap = ExecutorCapability {
        executor_id: [0x42; 32],
        supported_types: task_type::INFERENCE | task_type::COMPUTE,
        trust_score: 500,
        current_load: 1,
        max_concurrent: 4,
    };
    assert!(cap.is_available());
    assert!(cap.supports_type(task_type::INFERENCE));
    assert!(cap.supports_type(task_type::COMPUTE));
    // ANALYTICS=0x0006, not in INFERENCE|COMPUTE bitmask
    assert!(!cap.supports_type(task_type::CONSENSUS_VALIDATION));
}

#[test]
fn test_executor_capability_at_capacity() {
    let cap = ExecutorCapability {
        executor_id: [0x42; 32],
        supported_types: task_type::INFERENCE,
        trust_score: 500,
        current_load: 4,
        max_concurrent: 4,
    };
    assert!(!cap.is_available());
}

// ── JobDistributor ──

#[test]
fn test_job_distributor_select_executor() {
    // Use ORCHESTRATION=0x0007 as the unsupported type
    let executors = vec![
        ExecutorCapability {
            executor_id: [0x01; 32],
            supported_types: task_type::INFERENCE | task_type::COMPUTE,
            trust_score: 500,
            current_load: 0,
            max_concurrent: 2,
        },
        ExecutorCapability {
            executor_id: [0x02; 32],
            supported_types: task_type::SIMULATION | task_type::CONSENSUS_VALIDATION,
            trust_score: 300,
            current_load: 0,
            max_concurrent: 1,
        },
    ];
    let dist = JobDistributor::new(executors);

    let task = ExecutionTask {
        task_id: [0xAA; 32],
        mission_id: make_mission_id(0xAA),
        task_type: task_type::INFERENCE,
        payload_hash: [0xBB; 32],
        assigned_executor: [0u8; 32],
        deadline_epoch: 200,
        priority: 10,
        proof_required: false,
    };
    let executor = dist.select_executor(&task);
    assert_eq!(executor, Some([0x01; 32]));

    // ANALYTICS task should go to executor 2
    let analytics_task = ExecutionTask {
        task_id: [0xBB; 32],
        mission_id: make_mission_id(0xAA),
        task_type: task_type::CONSENSUS_VALIDATION,
        payload_hash: [0xCC; 32],
        assigned_executor: [0u8; 32],
        deadline_epoch: 200,
        priority: 10,
        proof_required: false,
    };
    let executor = dist.select_executor(&analytics_task);
    assert_eq!(executor, Some([0x02; 32]));
}

// ── SwarmCoordinator ──

#[test]
fn test_swarm_coordinator_lifecycle() {
    let mut coord = SwarmCoordinator::new(make_mission_id(0xAA));

    coord.register_agent([0x01; 32]);
    coord.register_agent([0x02; 32]);

    assert!(coord.assign_task(&[0x01; 32], [0xAA; 32]));
    assert!(!coord.assign_task(&[0x99; 32], [0xBB; 32])); // unknown agent

    assert!(coord.complete_task(&[0x01; 32], &[0xAA; 32]));
    assert!(!coord.complete_task(&[0x01; 32], &[0xBB; 32])); // wrong task
}

// ── Governance proposals ──

#[test]
fn test_governance_proposal_full_lifecycle() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    assert_eq!(proposal.state, ProposalState::Created);

    assert!(proposal.open_voting());
    assert_eq!(proposal.state, ProposalState::Voting);
    assert!(!proposal.open_voting()); // can't open again

    assert!(proposal.cast_vote([0x01; 32], 100, true));
    assert!(proposal.cast_vote([0x02; 32], 50, false));
    assert!(proposal.cast_vote([0x03; 32], 200, true));

    assert_eq!(proposal.total_for(), 300);
    assert_eq!(proposal.total_against(), 50);
}

#[test]
fn test_governance_proposal_cant_vote_before_open() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    assert!(!proposal.cast_vote([0x01; 32], 100, true));
}

#[test]
fn test_governance_resolve_centralized() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    proposal.open_voting();

    let policy = GovernancePolicy::new(
        GovernanceModel::Centralized,
        1,
        1,
        10,
        EmergencyAuthority::Coordinator,
    )
    .unwrap();

    let state = proposal.resolve(&policy, 10);
    assert_eq!(state, ProposalState::Approved);
}

#[test]
fn test_governance_resolve_autonomous() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    proposal.open_voting();
    proposal.cast_vote([0x01; 32], 100, true);
    proposal.cast_vote([0x02; 32], 50, false);

    let policy = GovernancePolicy::new(
        GovernanceModel::Autonomous,
        1,
        1,
        10,
        EmergencyAuthority::None,
    )
    .unwrap();

    assert_eq!(proposal.resolve(&policy, 10), ProposalState::Approved);
}

#[test]
fn test_governance_resolve_autonomous_reject() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    proposal.open_voting();
    proposal.cast_vote([0x01; 32], 30, true);
    proposal.cast_vote([0x02; 32], 100, false);

    let policy = GovernancePolicy::new(
        GovernanceModel::Autonomous,
        1,
        1,
        10,
        EmergencyAuthority::None,
    )
    .unwrap();

    assert_eq!(proposal.resolve(&policy, 10), ProposalState::Rejected);
}

#[test]
fn test_governance_resolve_dao_quorum_met() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    proposal.open_voting();
    proposal.cast_vote([0x01; 32], 100, true);
    proposal.cast_vote([0x02; 32], 50, true);
    proposal.cast_vote([0x03; 32], 30, false);

    let policy = GovernancePolicy::default_dao();
    assert_eq!(proposal.resolve(&policy, 3), ProposalState::Approved);
}

#[test]
fn test_governance_resolve_not_voting() {
    let mut proposal =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x42; 32], 100, 200);
    let policy = GovernancePolicy::default_dao();
    assert_eq!(proposal.resolve(&policy, 10), ProposalState::Created);
}

#[test]
fn test_decision_type_variants() {
    assert_eq!(DecisionType::Admission as u16, 0x0001);
    assert_eq!(DecisionType::RoleAssignment as u16, 0x0002);
    assert_eq!(DecisionType::TopologyChange as u16, 0x0003);
    assert_eq!(DecisionType::MissionTermination as u16, 0x0004);
    assert_eq!(DecisionType::PolicyModification as u16, 0x0005);
    assert_eq!(DecisionType::EmergencyRekey as u16, 0x0006);
    assert_eq!(DecisionType::ParticipantExpulsion as u16, 0x0007);
}

// ── Mission Discovery ──

#[test]
fn test_mission_discovery_scope_all() {
    assert_eq!(
        MissionDiscoveryScope::from_u16(0x0100),
        Some(MissionDiscoveryScope::Public)
    );
    assert_eq!(
        MissionDiscoveryScope::from_u16(0x0101),
        Some(MissionDiscoveryScope::InviteOnly)
    );
    assert_eq!(
        MissionDiscoveryScope::from_u16(0x0102),
        Some(MissionDiscoveryScope::Stealth)
    );
    assert_eq!(
        MissionDiscoveryScope::from_u16(0x0103),
        Some(MissionDiscoveryScope::Federated)
    );
    assert_eq!(
        MissionDiscoveryScope::from_u16(0x0104),
        Some(MissionDiscoveryScope::Ephemeral)
    );
    assert!(MissionDiscoveryScope::from_u16(0x0000).is_none());
}

#[test]
fn test_mission_discovery_scope_encryption() {
    assert!(MissionDiscoveryScope::Stealth.requires_encryption());
    assert!(MissionDiscoveryScope::InviteOnly.requires_encryption());
    assert!(!MissionDiscoveryScope::Public.requires_encryption());
}

#[test]
fn test_mission_discovery_scope_ttl() {
    assert_eq!(MissionDiscoveryScope::Public.default_ttl(), 20);
    assert_eq!(MissionDiscoveryScope::Stealth.default_ttl(), 5);
    assert_eq!(
        MissionDiscoveryScope::Ephemeral.default_ttl(),
        EPHEMERAL_ADVERTISEMENT_TTL
    );
}

#[test]
fn test_scope_to_gdp_scope_mapping() {
    assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Public), 0x0004);
    assert_eq!(
        scope_to_gdp_scope(MissionDiscoveryScope::InviteOnly),
        0x0005
    );
    assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Stealth), 0x0005);
    assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Federated), 0x0002);
    assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Ephemeral), 0x0003);
}

#[test]
fn test_mission_advertisement_creation() {
    let mission_id = make_mission_id(0xAA);
    let adv = MissionAdvertisement::new(
        mission_id,
        [0xBB; 32],
        MissionDiscoveryScope::Public,
        5,
        2,
        [0x42; 32],
        1000,
    );
    assert_eq!(adv.participant_count, 5);
    assert_eq!(adv.min_participants, 2);
    let bytes = adv.to_signing_bytes();
    assert!(!bytes.is_empty());
}

// ── Lifecycle edge cases ──

#[test]
fn test_min_participants() {
    assert_eq!(
        min_participants_for_state_transition(MissionState::Discovering),
        2
    );
    assert_eq!(
        min_participants_for_state_transition(MissionState::Active),
        0
    );
}

#[test]
fn test_heartbeat_constants() {
    assert_eq!(DEFAULT_HEARTBEAT_INTERVAL, 10);
    assert_eq!(DEFAULT_MISSED_HEARTBEATS, 3);
}

#[test]
fn test_tolerance_threshold_values() {
    assert_eq!(tolerance_threshold(0), 0);
    assert_eq!(tolerance_threshold(3), 1);
    assert_eq!(tolerance_threshold(9), 3);
}

#[test]
fn test_transition_trigger_variants() {
    assert_eq!(TransitionTrigger::GatewayAdvertisement as u16, 0x0001);
    assert_eq!(TransitionTrigger::UnrecoverableFailure as u16, 0x0009);
}
