//! Integration tests for the Deterministic Proof Substrate (DPS).
//!
//! Tests the full proof lifecycle: witness → prove → proof envelope →
//! verify commitment → verifier registry → recursive aggregation.
//! Also tests cross-module interactions with DOT envelope wrapping.

use octo_network::dps::envelope::ProofCarryingEnvelope;
use octo_network::dps::recursive::{AggregatedProof, AggregationMethod, RecursiveAggregator};
use octo_network::dps::suite::{
    ProofCircuitModel, ProofExecutionClass, ProofSuite, ProofSuiteId, ProofSystemId,
};
use octo_network::dps::verifier::{VerifierEntry, VerifierRegistry};
use octo_network::dps::witness::{Witness, WitnessInput};

use octo_network::dot::envelope::{DeterministicEnvelope, MessageType};

fn make_envelope(id_byte: u32) -> DeterministicEnvelope {
    DeterministicEnvelope {
        version: 1,
        network_id: id_byte,
        message_type: MessageType::Message as u16,
        envelope_id: [id_byte as u8; 32],
        mission_id: [0u8; 32],
        source_peer: [0x01; 32],
        origin_gateway: [0x01; 32],
        logical_timestamp: 1000,
        ttl_hops: 10,
        payload_hash: blake3::hash(b"test").into(),
        route_trace_root: [0u8; 32],
        flags: 0,
        signature: [0u8; 64],
    }
}

// ── ProofCarryingEnvelope lifecycle ──

#[test]
fn test_proof_envelope_creation_and_validation() {
    let envelope = make_envelope(1);
    let proof_blob = vec![0xAB; 128];

    let pce = ProofCarryingEnvelope::new(
        envelope,
        ProofSystemId::STWO,
        ProofCircuitModel::AIR,
        proof_blob,
    )
    .unwrap();

    assert_eq!(pce.proof_system(), Some(ProofSystemId::STWO));
    assert_eq!(pce.circuit_model(), Some(ProofCircuitModel::AIR));
    assert!(pce.verify_commitment().is_ok());
}

#[test]
fn test_proof_envelope_with_public_input_root() {
    let envelope = make_envelope(1);
    let pce = ProofCarryingEnvelope::new(
        envelope,
        ProofSystemId::PLONK,
        ProofCircuitModel::PLONKISH,
        vec![0xCD; 64],
    )
    .unwrap()
    .with_public_input_root([0xEE; 32]);

    assert!(pce.validate().is_ok());
    assert_eq!(pce.public_input_root, [0xEE; 32]);
}

#[test]
fn test_proof_envelope_rejects_zero_public_input_root() {
    let envelope = make_envelope(1);
    let pce = ProofCarryingEnvelope::new(
        envelope,
        ProofSystemId::STWO,
        ProofCircuitModel::AIR,
        vec![0xAB; 32],
    )
    .unwrap();

    // public_input_root is zero by default
    assert!(pce.validate().is_err());
}

#[test]
fn test_proof_envelope_commitment_mismatch_detected() {
    let envelope = make_envelope(1);
    let mut pce = ProofCarryingEnvelope::new(
        envelope,
        ProofSystemId::STWO,
        ProofCircuitModel::AIR,
        vec![0xAB; 32],
    )
    .unwrap();

    // Tamper with commitment
    pce.proof_commitment[0] ^= 0xFF;
    assert!(pce.verify_commitment().is_err());
}

#[test]
fn test_proof_envelope_parent_chain() {
    let envelope = make_envelope(1);
    let parent = [0xAA; 32];

    let pce = ProofCarryingEnvelope::new(
        envelope,
        ProofSystemId::Halo2,
        ProofCircuitModel::Recursive,
        vec![0x01; 32],
    )
    .unwrap()
    .with_parent(parent)
    .with_public_input_root([0xBB; 32]);

    assert_eq!(pce.parent_proof_commitment, Some(parent));
}

#[test]
fn test_proof_envelope_rejects_oversized_blob() {
    let envelope = make_envelope(1);
    let huge_blob = vec![0u8; 2_000_000]; // 2MB > 1MB limit

    let result = ProofCarryingEnvelope::new(
        envelope,
        ProofSystemId::STWO,
        ProofCircuitModel::AIR,
        huge_blob,
    );
    assert!(result.is_err());
}

// ── Witness lifecycle ──

#[test]
fn test_witness_input_full_lifecycle() {
    let witness = WitnessInput::new([0xAA; 32], vec![1, 2, 3, 4], vec![5, 6, 7, 8]);

    assert!(witness.validate().is_ok());

    let bytes = witness.to_canonical_bytes();
    assert!(!bytes.is_empty());

    let hash = witness.commitment_hash();
    assert_ne!(hash, [0u8; 32]);

    // Deterministic
    assert_eq!(hash, witness.commitment_hash());
}

#[test]
fn test_witness_input_empty_private_rejected() {
    let witness = WitnessInput::new([0xAA; 32], vec![], vec![5, 6]);
    assert!(witness.validate().is_err());
}

#[test]
fn test_witness_input_empty_public_rejected() {
    let witness = WitnessInput::new([0xAA; 32], vec![1, 2], vec![]);
    assert!(witness.validate().is_err());
}

// ── Verifier Registry ──

#[test]
fn test_verifier_registry_full_lifecycle() {
    let mut reg = VerifierRegistry::new();
    assert!(reg.is_empty());

    let suite_id = ProofSuiteId::new(ProofSystemId::STWO.as_u16(), 0x0001, 0x0001, 0x0001);

    let entry = VerifierEntry {
        suite_id: suite_id.clone(),
        proof_suite: ProofSuite::new(
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            ProofExecutionClass::ClassA,
        ),
        verification_key: vec![0xAA; 64],
        registered_at: 100,
        expires_at: None,
    };

    reg.register(entry);
    assert_eq!(reg.len(), 1);
    assert!(reg.contains(&suite_id));

    let retrieved = reg.get(&suite_id).unwrap();
    assert_eq!(retrieved.suite_id, suite_id);
    assert_eq!(retrieved.verification_key, vec![0xAA; 64]);
}

#[test]
fn test_verifier_registry_deterministic_iteration() {
    let mut reg = VerifierRegistry::new();

    for i in 0..5u16 {
        let sid = ProofSuiteId::new(i + 1, 0x0001, 0x0001, 0x0001);
        let system = ProofSystemId::from_u16(i + 1).unwrap();
        let entry = VerifierEntry {
            suite_id: sid,
            proof_suite: ProofSuite::new(
                system,
                ProofCircuitModel::AIR,
                ProofExecutionClass::ClassA,
            ),
            verification_key: vec![i as u8; 32],
            registered_at: 100,
            expires_at: None,
        };
        reg.register(entry);
    }

    let keys1: Vec<[u8; 32]> = reg.iter().map(|(k, _)| *k).collect();
    let keys2: Vec<[u8; 32]> = reg.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys1, keys2);
}

#[test]
fn test_verifier_registry_eviction() {
    let mut reg = VerifierRegistry::new();

    let sid = ProofSuiteId::new(ProofSystemId::STWO.as_u16(), 0x0001, 0x0001, 0x0001);
    let entry = VerifierEntry {
        suite_id: sid,
        proof_suite: ProofSuite::new(
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            ProofExecutionClass::ClassA,
        ),
        verification_key: vec![0xAA; 32],
        registered_at: 100,
        expires_at: Some(200),
    };

    reg.register(entry);
    assert_eq!(reg.len(), 1);

    let removed = reg.evict_expired(200);
    assert_eq!(removed, 1);
    assert!(reg.is_empty());
}

// ── Recursive Aggregation ──

#[test]
fn test_aggregation_binary_tree_pipeline() {
    let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::BinaryTree);

    // Add proof commitments
    let proof_a = [0xAA; 32];
    let proof_b = [0xBB; 32];

    agg.add_proof(proof_a);
    agg.add_proof(proof_b);

    assert_eq!(agg.len(), 2);

    let root = agg.compute_aggregation_root();
    assert_ne!(root, [0u8; 32]);

    let result = agg.build(vec![0xCC; 64], [0xDD; 32]);
    assert!(result.is_ok());

    let agg_proof = result.unwrap();
    assert_eq!(agg_proof.proof_count, 2);
    assert_eq!(agg_proof.method, AggregationMethod::BinaryTree);
    assert_eq!(agg_proof.aggregation_system, ProofSystemId::STWO);
}

#[test]
fn test_aggregation_commitment_deterministic() {
    let left = [0xAA; 32];
    let right = [0xBB; 32];

    let c1 = AggregatedProof::compute_aggregation_commitment(&left, &right);
    let c2 = AggregatedProof::compute_aggregation_commitment(&left, &right);
    assert_eq!(c1, c2);
    assert_ne!(c1, [0u8; 32]);
}

#[test]
fn test_aggregation_commitment_order_dependent() {
    let a = [0xAA; 32];
    let b = [0xBB; 32];

    let c_ab = AggregatedProof::compute_aggregation_commitment(&a, &b);
    let c_ba = AggregatedProof::compute_aggregation_commitment(&b, &a);
    assert_ne!(c_ab, c_ba);
}

#[test]
fn test_aggregated_proof_blob_commitment() {
    let agg = AggregatedProof::new(
        ProofSystemId::STWO,
        AggregationMethod::BinaryTree,
        [0xCC; 32],
        vec![0xDD; 128],
        [0xEE; 32],
        4,
        2,
    );

    let blob_commitment = agg.blob_commitment();
    assert_ne!(blob_commitment, [0u8; 32]);

    assert!(agg.verify(&blob_commitment).is_ok());

    let wrong = [0xFF; 32];
    assert!(agg.verify(&wrong).is_err());
}

// ── ProofSystemId enum coverage ──

#[test]
fn test_proof_system_id_roundtrip() {
    for id in 1..=8u16 {
        let system = ProofSystemId::from_u16(id).unwrap();
        assert_eq!(system.as_u16(), id);
    }
    assert!(ProofSystemId::from_u16(0x0009).is_none());
}

#[test]
fn test_proof_suite_id_hash_deterministic() {
    let sid = ProofSuiteId::new(0x0001, 0x0002, 0x0003, 0x0004);
    let h1 = sid.to_hash();
    let h2 = sid.to_hash();
    assert_eq!(h1, h2);
}
