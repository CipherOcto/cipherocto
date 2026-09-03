//! Canonical test vectors for §Phase 5 slashing substrate
//! (RFC-0855p-b §Implementation Phase 5 + §Appendix B Slash Offense Codes).

use ed25519_dalek::Signer;
use octo_coordinator_types::slashing::{apply_slash, verify_slash_proof, CoolDownTracker};
use octo_coordinator_types::state::{
    CoordinatorError, CoordinatorLifecycle, CoordinatorRecord, CoordinatorSource, SlashProof,
};

fn keypair(seed: [u8; 32]) -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&seed)
}

fn sample_record() -> CoordinatorRecord {
    CoordinatorRecord {
        coordinator_peer_id: [0xAAu8; 32],
        state: CoordinatorLifecycle::Active,
        term_start_epoch: 100,
        term_end_epoch: 200,
        source: CoordinatorSource::Election,
        coordinator_term_id: [0x33u8; 32],
        slash_count: 0,
        octo_o_stake_locked: 10_000,
        last_heartbeat_epoch: 100,
        heartbeat_interval: 50,
    }
}

fn sample_signed_proof(offense: u16, penalty: u64) -> SlashProof {
    let mut proof = SlashProof {
        slash_id: [0x99u8; 32],
        coordinator: [0xAAu8; 32],
        coordinator_term_id: [0x33u8; 32],
        offense,
        evidence: vec![0x01, 0x02, 0x03],
        penalty,
        adjudicator: [0x77u8; 32],
        adjudicator_signature: [0u8; 64],
    };
    let payload = octo_coordinator_types::slashing::slash_proof_payload(&proof);
    let sk = keypair([0x77u8; 32]);
    let sig = sk.sign(&payload);
    proof.adjudicator_signature = sig.to_bytes();
    proof
}

// TV-SL-1: Valid slash (Active → Demoting → Inactive).
#[test]
fn tv_sl_1_valid_slash_progression() {
    let proof = sample_signed_proof(0x0001, 1_000); // DoubleSign
    let sk = keypair([0x77u8; 32]);
    let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
    let mut record = sample_record();
    let mut cool_down = CoolDownTracker::new();

    verify_slash_proof(&proof, &record, &governance_set).unwrap();
    let update = apply_slash(&mut record, &proof, &mut cool_down, 100).unwrap();
    assert_eq!(record.state, CoordinatorLifecycle::Inactive);
    assert_eq!(record.octo_o_stake_locked, 9_000);
    assert_eq!(record.slash_count, 1);
    assert_eq!(update.slash_reason_code, 0x0001);
    // Exponential backoff: 2^1 = 2 epochs from current_epoch 100 → end 102.
    assert!(!cool_down.is_eligible(&[0xAAu8; 32], 101));
    assert!(cool_down.is_eligible(&[0xAAu8; 32], 102));
}

// TV-SL-2: Demoting → Inactive with cool-down registered.
#[test]
fn tv_sl_2_demoting_to_inactive_with_cool_down() {
    let proof = sample_signed_proof(0x0009, 500); // GenesisCompromise
    let sk = keypair([0x77u8; 32]);
    let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
    let mut record = sample_record();
    let mut cool_down = CoolDownTracker::new();

    verify_slash_proof(&proof, &record, &governance_set).unwrap();
    apply_slash(&mut record, &proof, &mut cool_down, 100).unwrap();
    // 2^1 = 2 epochs cool-down from epoch 100 → eligible at epoch ≥102.
    assert!(!cool_down.is_eligible(&[0xAAu8; 32], 101));
    assert!(cool_down.is_eligible(&[0xAAu8; 32], 102));
}

// TV-SL-3: Invalid signature → InvalidAdjudicatorSignature.
#[test]
fn tv_sl_3_invalid_adjudicator_signature_rejected() {
    let mut proof = sample_signed_proof(0x0001, 1_000);
    proof.adjudicator_signature[0] = 0xFF;
    let sk = keypair([0x77u8; 32]);
    let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
    let record = sample_record();
    let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
    assert_eq!(err, CoordinatorError::InvalidAdjudicatorSignature);
}

// TV-SL-4: Unknown offense code (reserved 0x000C) → UnknownOffenseCode.
#[test]
fn tv_sl_4_unknown_offense_code_rejected() {
    let proof = sample_signed_proof(0x000C, 100);
    let sk = keypair([0x77u8; 32]);
    let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
    let record = sample_record();
    let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
    assert_eq!(err, CoordinatorError::UnknownOffenseCode { got: 0x000C });
}
