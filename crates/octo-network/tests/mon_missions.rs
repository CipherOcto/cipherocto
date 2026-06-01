//! Integration tests for Mission Overlay Networks (MON).
//!
//! Tests the full mission lifecycle: MissionId creation → membership →
//! role validation → governance → route table → lifecycle state machine.

use octo_network::mon::governance::{
    EmergencyAuthority, GovernanceModel, GovernancePolicy,
};
use octo_network::mon::lifecycle::{is_valid_transition, tolerance_threshold, MissionState};
use octo_network::mon::membership::{
    compute_membership_commitment, is_valid_role_combination, validate_role_assignment,
    ROLE_COORDINATOR, ROLE_EXECUTOR, ROLE_OBSERVER,
    ROLE_PROVER, ROLE_RELAY, ROLE_VALIDATOR,
};
use octo_network::mon::mission_id::{MissionId, MissionType};
use octo_network::mon::routing::{
    compute_route_commitment, compute_route_table_merkle,
    RouteEntry, RouteIsolationGuard, MissionRouteTable,
};

// ── MissionId lifecycle ──

#[test]
fn test_mission_id_deterministic_creation() {
    let peer = [0x42; 32];
    let nonce = [0x99; 32];

    let id1 = MissionId::new(1, &peer, 100, &nonce, 1);
    let id2 = MissionId::new(1, &peer, 100, &nonce, 1);
    assert_eq!(id1, id2);
}

#[test]
fn test_mission_id_serialization_roundtrip() {
    let peer = [0x42; 32];
    let nonce = [0x99; 32];
    let id = MissionId::new(7, &peer, 500, &nonce, 2);

    let bytes = id.to_canonical_bytes();
    assert_eq!(bytes.len(), MissionId::SIZE);

    let recovered = MissionId::from_canonical_bytes(&bytes).unwrap();
    assert_eq!(id, recovered);
}

#[test]
fn test_mission_id_different_genesis_different_hash() {
    let nonce = [0x99; 32];

    let id1 = MissionId::new(1, &[0x01; 32], 100, &nonce, 1);
    let id2 = MissionId::new(1, &[0x02; 32], 100, &nonce, 1);
    assert_ne!(id1.mission_hash, id2.mission_hash);
}

#[test]
fn test_mission_id_bad_bytes_rejected() {
    assert!(MissionId::from_canonical_bytes(&[0u8; 10]).is_err());
    assert!(MissionId::from_canonical_bytes(&[0u8; 40]).is_err());
}

// ── Mission types ──

#[test]
fn test_mission_type_repr() {
    assert_eq!(MissionType::AiSwarm as u16, 0x0001);
    assert_eq!(MissionType::Custom as u16, 0xFFFF);
}

// ── Lifecycle state machine ──

#[test]
fn test_valid_lifecycle_progression() {
    assert!(is_valid_transition(MissionState::Created, MissionState::Discovering));
    assert!(is_valid_transition(MissionState::Discovering, MissionState::Forming));
    assert!(is_valid_transition(MissionState::Forming, MissionState::Active));
    assert!(is_valid_transition(MissionState::Active, MissionState::Degraded));
    assert!(is_valid_transition(MissionState::Degraded, MissionState::Recovering));
    assert!(is_valid_transition(MissionState::Recovering, MissionState::Active));
    assert!(is_valid_transition(MissionState::Active, MissionState::Terminated));
    assert!(is_valid_transition(MissionState::Terminated, MissionState::Archived));
}

#[test]
fn test_invalid_lifecycle_transitions() {
    assert!(!is_valid_transition(MissionState::Created, MissionState::Active));
    assert!(!is_valid_transition(MissionState::Active, MissionState::Created));
    assert!(!is_valid_transition(MissionState::Archived, MissionState::Active));
    assert!(!is_valid_transition(MissionState::Terminated, MissionState::Active));
}

#[test]
fn test_tolerance_threshold() {
    assert_eq!(tolerance_threshold(9), 3);
    assert_eq!(tolerance_threshold(10), 3);
    assert_eq!(tolerance_threshold(0), 0);
}

// ── Membership ──

#[test]
fn test_valid_role_combinations() {
    assert!(is_valid_role_combination(ROLE_COORDINATOR));
    assert!(is_valid_role_combination(ROLE_EXECUTOR | ROLE_RELAY));

    // Forbidden: Coordinator + Prover
    assert!(!is_valid_role_combination(ROLE_COORDINATOR | ROLE_PROVER));
    // Forbidden: Coordinator + Observer
    assert!(!is_valid_role_combination(ROLE_COORDINATOR | ROLE_OBSERVER));
    // No roles
    assert!(!is_valid_role_combination(0));
}

#[test]
fn test_role_assignment_trust_requirements() {
    // Coordinator needs trust >= 500
    assert!(validate_role_assignment(ROLE_COORDINATOR, 500).is_ok());
    assert!(validate_role_assignment(ROLE_COORDINATOR, 499).is_err());

    // Validator needs trust >= 300
    assert!(validate_role_assignment(ROLE_VALIDATOR, 300).is_ok());
    assert!(validate_role_assignment(ROLE_VALIDATOR, 299).is_err());

    // Executor has no minimum
    assert!(validate_role_assignment(ROLE_EXECUTOR, 0).is_ok());
}

#[test]
fn test_membership_commitment_deterministic() {
    let c1 = compute_membership_commitment(&[0xAA; 32], &[0xBB; 32], ROLE_EXECUTOR, 100);
    let c2 = compute_membership_commitment(&[0xAA; 32], &[0xBB; 32], ROLE_EXECUTOR, 100);
    assert_eq!(c1, c2);
    assert_ne!(c1, [0u8; 32]);
}

// ── Governance ──

#[test]
fn test_governance_policy_validation() {
    let valid = GovernancePolicy::new(
        GovernanceModel::Dao,
        2,
        3,
        10,
        EmergencyAuthority::Coordinator,
    );
    assert!(valid.is_ok());

    // Zero denominator
    let invalid = GovernancePolicy::new(
        GovernanceModel::Dao,
        2,
        0,
        10,
        EmergencyAuthority::Coordinator,
    );
    assert!(invalid.is_err());

    // numerator > denominator
    let invalid = GovernancePolicy::new(
        GovernanceModel::Dao,
        3,
        2,
        10,
        EmergencyAuthority::Coordinator,
    );
    assert!(invalid.is_err());
}

#[test]
fn test_governance_quorum() {
    let policy = GovernancePolicy::default_dao();
    assert_eq!(policy.model, GovernanceModel::Dao);

    // 2/3 quorum: 2 out of 3 should pass
    assert!(policy.is_quorum_met(2, 3));
    assert!(policy.is_quorum_met(3, 3));
    assert!(!policy.is_quorum_met(1, 3));

    // Edge case: 0 total
    assert!(!policy.is_quorum_met(0, 0));
}

#[test]
fn test_governance_model_enum() {
    assert_eq!(GovernanceModel::from_u16(0x0001), Some(GovernanceModel::Centralized));
    assert_eq!(GovernanceModel::from_u16(0x0005), Some(GovernanceModel::Autonomous));
    assert!(GovernanceModel::from_u16(0x0006).is_none());
}

// ── Mission Route Table ──

#[test]
fn test_route_table_full_lifecycle() {
    let mut table = MissionRouteTable::new([0xAA; 32]);

    // Insert
    table.upsert(RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x03; 32],
        cost: 100,
        sequence: 1,
    });
    assert_eq!(table.len(), 1);

    // Stale update rejected
    table.upsert(RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x04; 32],
        cost: 50,
        sequence: 1,
    });
    assert_eq!(table.lookup(&[0x02; 32]).unwrap().next_hop, [0x03; 32]);

    // Fresh update accepted
    table.upsert(RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x04; 32],
        cost: 50,
        sequence: 2,
    });
    assert_eq!(table.lookup(&[0x02; 32]).unwrap().next_hop, [0x04; 32]);

    // Remove
    table.remove(&[0x02; 32]);
    assert!(table.is_empty());
}

#[test]
fn test_route_table_merkle_root_deterministic() {
    let mut table = MissionRouteTable::new([0xAA; 32]);
    table.upsert(RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x03; 32],
        cost: 100,
        sequence: 1,
    });

    let r1 = compute_route_table_merkle(&table.routes);
    let r2 = compute_route_table_merkle(&table.routes);
    assert_eq!(r1, r2);
}

#[test]
fn test_route_isolation_guard() {
    let guard = RouteIsolationGuard::new(
        [0xAA; 32],
        vec![[0x01; 32], [0x02; 32]],
    );

    assert!(guard.is_authorized(&[0xAA; 32], &[0x01; 32]));
    assert!(!guard.is_authorized(&[0xAA; 32], &[0x03; 32]));
    assert!(!guard.is_authorized(&[0xBB; 32], &[0x01; 32]));
}

#[test]
fn test_route_commitment_deterministic() {
    let c1 = compute_route_commitment(&[0xAA; 32], 5, 100);
    let c2 = compute_route_commitment(&[0xAA; 32], 5, 100);
    assert_eq!(c1, c2);
    assert_ne!(c1, [0u8; 32]);
}
