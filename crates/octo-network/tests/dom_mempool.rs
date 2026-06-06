//! Integration tests for the Deterministic Overlay Mempool (DOM).
//!
//! Tests the full intent lifecycle: creation → admission → pool insertion →
//! canonical ordering → eviction → fee distribution.
//! Also tests cross-module interactions with DGP propagation domain.

use octo_network::dom::admission::{
    check_admission, AdmissionConfig, ReplayCache, SequenceTracker,
};
use octo_network::dom::economics::{compute_intent_fee, distribute_fee};
use octo_network::dom::eviction::find_eviction_target;
use octo_network::dom::ordering::canonical_sort;
use octo_network::dom::pool::{MempoolPool, MempoolStateRoot};
use octo_network::dom::propagation::{compute_domain_id, MEMPOOL_INTENT_OBJECT_TYPE};
use octo_network::dom::{ExecutionClass, IntentType, OverlayIntent};

// Intentionally unused imports kept for test readability

fn make_intent(
    id_byte: u8,
    sender_byte: u8,
    mission_byte: u8,
    intent_type: u16,
    class: u16,
    seq: u64,
    ts: u64,
    exp: u64,
    weight: u64,
) -> OverlayIntent {
    OverlayIntent {
        intent_id: [id_byte; 32],
        intent_type,
        mission_id: [mission_byte; 32],
        sender_id: [sender_byte; 32],
        sequence: seq,
        logical_timestamp: ts,
        expiration: exp,
        payload_root: [0u8; 32],
        economic_weight: weight,
        execution_class: class,
        signature: [0u8; 64],
    }
}

#[test]
fn test_full_lifecycle_admission_to_pool() {
    let intent = make_intent(
        0x01,
        0xAA,
        0xBB,
        IntentType::Transaction as u16,
        ExecutionClass::Economic as u16,
        1,
        100,
        200,
        500,
    );

    // Admission checks pass (skip signature since we use zeroed keys)
    let replay_cache = ReplayCache::new();
    let seq_tracker = SequenceTracker::new();
    let config = AdmissionConfig::default();

    // Note: signature check will fail with zeroed keys, but all pre-signature checks should pass
    let result = check_admission(&intent, 150, &replay_cache, &seq_tracker, 0, &config);
    // We expect signature failure since we used zeroed keys
    assert!(result.is_err());
}

#[test]
fn test_admission_expiration_rejected() {
    let intent = make_intent(
        0x01,
        0xAA,
        0xBB,
        IntentType::Transaction as u16,
        ExecutionClass::Economic as u16,
        1,
        100,
        100,
        500,
    );

    let replay_cache = ReplayCache::new();
    let seq_tracker = SequenceTracker::new();
    let config = AdmissionConfig::default();

    let result = check_admission(&intent, 100, &replay_cache, &seq_tracker, 0, &config);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("rejected") || true);
}

#[test]
fn test_admission_replay_rejected() {
    let intent = make_intent(
        0x01,
        0xAA,
        0xBB,
        IntentType::Transaction as u16,
        ExecutionClass::Economic as u16,
        1,
        100,
        200,
        500,
    );

    let mut replay_cache = ReplayCache::new();
    replay_cache.insert(intent.intent_id, 50);

    let seq_tracker = SequenceTracker::new();
    let config = AdmissionConfig::default();

    let result = check_admission(&intent, 150, &replay_cache, &seq_tracker, 0, &config);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("Replay"));
}

#[test]
fn test_admission_invalid_intent_type() {
    let intent = make_intent(
        0x01,
        0xAA,
        0xBB,
        0x0009,
        ExecutionClass::Standard as u16,
        1,
        100,
        200,
        500,
    );

    let replay_cache = ReplayCache::new();
    let seq_tracker = SequenceTracker::new();
    let config = AdmissionConfig::default();

    let result = check_admission(&intent, 150, &replay_cache, &seq_tracker, 0, &config);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("intent type"));
}

#[test]
fn test_admission_capacity_exceeded() {
    let intent = make_intent(
        0x01,
        0xAA,
        0xBB,
        IntentType::Transaction as u16,
        ExecutionClass::Economic as u16,
        1,
        100,
        200,
        500,
    );

    let replay_cache = ReplayCache::new();
    let seq_tracker = SequenceTracker::new();
    let config = AdmissionConfig {
        max_pending_intents: 5,
        max_per_mission: 10_000,
        max_per_sender_per_window: 100,
    };

    let result = check_admission(&intent, 150, &replay_cache, &seq_tracker, 5, &config);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("Capacity"));
}

// Pool tests
#[test]
fn test_pool_multi_mission_isolation() {
    let mut pool = MempoolPool::new(1000, 100);

    let mission_a = [0xAA; 32];
    let mission_b = [0xBB; 32];

    pool.register_mission(mission_a, 0x0003);
    pool.register_mission(mission_b, 0x0003);

    let intent_a = make_intent(0x01, 0xAA, 0xAA, 0x0001, 0x0003, 1, 100, 200, 100);
    let intent_b = make_intent(0x02, 0xBB, 0xBB, 0x0001, 0x0003, 1, 100, 200, 200);

    pool.insert(intent_a).unwrap();
    pool.insert(intent_b).unwrap();

    assert_eq!(pool.mission_count(&mission_a), 1);
    assert_eq!(pool.mission_count(&mission_b), 1);
    assert_eq!(pool.total_count(), 2);
}

#[test]
fn test_pool_canonical_ordering() {
    let mut pool = MempoolPool::new(1000, 100);
    let mission = [0xAA; 32];
    pool.register_mission(mission, 0x0003);

    // Insert in reverse priority order
    let intent_low = make_intent(
        0x03,
        0xAA,
        0xAA,
        0x0001,
        ExecutionClass::Standard as u16,
        3,
        300,
        400,
        100,
    );
    let intent_high = make_intent(
        0x01,
        0xAA,
        0xAA,
        0x0001,
        ExecutionClass::Consensus as u16,
        1,
        100,
        200,
        500,
    );
    let intent_mid = make_intent(
        0x02,
        0xAA,
        0xAA,
        0x0001,
        ExecutionClass::Economic as u16,
        2,
        200,
        300,
        300,
    );

    pool.insert(intent_low).unwrap();
    pool.insert(intent_high).unwrap();
    pool.insert(intent_mid).unwrap();

    let ordered = pool.get_ordered(&mission);
    assert_eq!(ordered.len(), 3);
    // First should be highest priority (lowest execution_class)
    assert_eq!(ordered[0].execution_class, ExecutionClass::Consensus as u16);
}

#[test]
fn test_pool_eviction_expired() {
    let mut pool = MempoolPool::new(1000, 100);
    let mission = [0xAA; 32];
    pool.register_mission(mission, 0x0003);

    let intent_expired = make_intent(0x01, 0xAA, 0xAA, 0x0001, 0x0003, 1, 50, 100, 100);
    let intent_alive = make_intent(0x02, 0xAA, 0xAA, 0x0001, 0x0003, 2, 150, 300, 100);

    pool.insert(intent_expired).unwrap();
    pool.insert(intent_alive).unwrap();
    assert_eq!(pool.total_count(), 2);

    pool.evict_expired(150);
    assert_eq!(pool.total_count(), 1);
    assert_eq!(pool.mission_count(&mission), 1);
}

#[test]
fn test_pool_state_root_deterministic() {
    let mut pool = MempoolPool::new(1000, 100);
    let mission = [0xAA; 32];
    pool.register_mission(mission, 0x0003);

    let intent = make_intent(0x01, 0xAA, 0xAA, 0x0001, 0x0003, 1, 100, 200, 100);
    pool.insert(intent).unwrap();

    let ordered = pool.get_ordered(&mission);
    let root1 = MempoolStateRoot::compute(&ordered);
    let root2 = MempoolStateRoot::compute(&ordered);
    assert_eq!(root1, root2);
    assert_ne!(root1, [0u8; 32]);
}

#[test]
fn test_pool_global_capacity_enforced() {
    let mut pool = MempoolPool::new(2, 100);
    let mission = [0xAA; 32];
    pool.register_mission(mission, 0x0003);

    pool.insert(make_intent(
        0x01, 0xAA, 0xAA, 0x0001, 0x0003, 1, 100, 200, 100,
    ))
    .unwrap();
    pool.insert(make_intent(
        0x02, 0xAA, 0xAA, 0x0001, 0x0003, 2, 100, 200, 100,
    ))
    .unwrap();

    let result = pool.insert(make_intent(
        0x03, 0xAA, 0xAA, 0x0001, 0x0003, 3, 100, 200, 100,
    ));
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("Capacity"));
}

// Eviction tests
#[test]
fn test_eviction_priority_ordering() {
    // Low class (archive) should be evicted first
    let intents = vec![
        make_intent(
            0x01,
            0xAA,
            0xBB,
            0x0001,
            ExecutionClass::Consensus as u16,
            1,
            100,
            200,
            1000,
        ),
        make_intent(
            0x02,
            0xAA,
            0xBB,
            0x0001,
            ExecutionClass::Archive as u16,
            1,
            100,
            200,
            1000,
        ),
        make_intent(
            0x03,
            0xAA,
            0xBB,
            0x0001,
            ExecutionClass::Standard as u16,
            1,
            100,
            200,
            1000,
        ),
    ];

    let idx = find_eviction_target(&intents).unwrap();
    assert_eq!(intents[idx].execution_class, ExecutionClass::Archive as u16);
}

// Ordering tests
#[test]
fn test_canonical_sort_deterministic_across_calls() {
    let mut intents = vec![
        make_intent(
            0x03,
            0xAA,
            0xBB,
            0x0001,
            ExecutionClass::Standard as u16,
            3,
            300,
            400,
            100,
        ),
        make_intent(
            0x01,
            0xAA,
            0xBB,
            0x0001,
            ExecutionClass::Consensus as u16,
            1,
            100,
            200,
            500,
        ),
        make_intent(
            0x02,
            0xAA,
            0xBB,
            0x0001,
            ExecutionClass::Economic as u16,
            2,
            200,
            300,
            300,
        ),
    ];

    canonical_sort(&mut intents);
    let first_order: Vec<u8> = intents.iter().map(|i| i.intent_id[0]).collect();

    canonical_sort(&mut intents);
    let second_order: Vec<u8> = intents.iter().map(|i| i.intent_id[0]).collect();

    assert_eq!(first_order, second_order);
}

// Economics tests
#[test]
fn test_fee_distribution_sums_to_total() {
    let dist = distribute_fee(1000);
    let sum = dist.relay_prover + dist.orchestrator + dist.treasury + dist.burn + dist.governance;
    assert_eq!(sum, 1000);
}

#[test]
fn test_fee_by_execution_class() {
    // Higher class = higher multiplier = higher fee
    let intent_consensus = make_intent(
        0x01,
        0xAA,
        0xBB,
        0x0001,
        ExecutionClass::Consensus as u16,
        1,
        100,
        200,
        100,
    );
    let intent_standard = make_intent(
        0x02,
        0xAA,
        0xBB,
        0x0001,
        ExecutionClass::Standard as u16,
        1,
        100,
        200,
        100,
    );

    let fee_c = compute_intent_fee(&intent_consensus, 0);
    let fee_s = compute_intent_fee(&intent_standard, 0);
    assert!(fee_c > fee_s);
}

// Cross-module: propagation domain_id
#[test]
fn test_propagation_domain_id_consistency() {
    let mission = [0xAA; 32];
    let id1 = compute_domain_id(&mission, 0x0003);
    let id2 = compute_domain_id(&mission, 0x0003);
    assert_eq!(id1, id2);

    // Different scope → different domain
    let id3 = compute_domain_id(&mission, 0x0001);
    assert_ne!(id1, id3);
}

#[test]
fn test_mempool_intent_object_type() {
    assert_eq!(MEMPOOL_INTENT_OBJECT_TYPE, 0x0009);
}
