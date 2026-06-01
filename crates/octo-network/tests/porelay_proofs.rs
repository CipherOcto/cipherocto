//! Integration tests for Proof-of-Relay (PoRelay).
//!
//! Tests the full relay score lifecycle: relay score computation → trust
//! registry → decay → reward distribution → slashing → availability/bandwidth proofs.

use octo_network::porelay::availability::AvailabilityProof;
use octo_network::porelay::bandwidth::BandwidthProof;
use octo_network::porelay::economics::{
    compute_archival_cost, RewardDistribution, SlashingCondition,
    SLASH_CONSENSUS_VIOLATION, SLASH_INVALID_PROOF, SLASH_PROOF_REPLAY,
};
use octo_network::porelay::registry::TrustRegistry;
use octo_network::porelay::score::{
    RelayScore, DEFAULT_STAKE_MULTIPLIER, MAX_STAKE_MULTIPLIER,
    WEIGHT_AVAILABILITY, WEIGHT_BANDWIDTH, WEIGHT_DIVERSITY, WEIGHT_FORWARDING, WEIGHT_UPTIME,
};

fn make_score(id_byte: u8, fwd: u16, avail: u16, bw: u16, uptime: u16, diversity: u16, stake: u32) -> RelayScore {
    let mut s = RelayScore {
        gateway_id: [id_byte; 32],
        epoch: 1,
        forwarding_score: fwd,
        availability_score: avail,
        bandwidth_score: bw,
        uptime_score: uptime,
        diversity_bonus: diversity,
        stake_multiplier: stake,
        composite: 0,
    };
    s.compute_composite();
    s
}

// ── Relay Score computation ──

#[test]
fn test_relay_score_composite_formula() {
    let mut score = RelayScore {
        gateway_id: [0x42; 32],
        epoch: 1,
        forwarding_score: 800,
        availability_score: 950,
        bandwidth_score: 700,
        uptime_score: 900,
        diversity_bonus: 400,
        stake_multiplier: 1500,
        composite: 0,
    };
    score.compute_composite();

    // raw = 800*300 + 950*250 + 700*200 + 900*150 + 400*100
    //     = 240000 + 237500 + 140000 + 135000 + 40000 = 792500
    // composite = 792500 * 1500 / 1000 = 1188750
    assert_eq!(score.composite, 1_188_750);
}

#[test]
fn test_relay_score_clamps_values() {
    let mut score = RelayScore {
        gateway_id: [0x42; 32],
        epoch: 1,
        forwarding_score: 1500,  // over 1000 — should be clamped
        availability_score: 2000, // over 1000
        bandwidth_score: 700,
        uptime_score: 900,
        diversity_bonus: 600,    // over 500 — should be clamped
        stake_multiplier: DEFAULT_STAKE_MULTIPLIER,
        composite: 0,
    };
    score.compute_composite();

    // Clamped: fwd=1000, avail=1000, diversity=500
    let expected_raw = 1000 * 300 + 1000 * 250 + 700 * 200 + 900 * 150 + 500 * 100;
    // = 300000 + 250000 + 140000 + 135000 + 50000 = 875000
    // composite = 875000 * 1000 / 1000 = 875000
    assert_eq!(score.composite, expected_raw);
}

#[test]
fn test_relay_score_stake_multiplier_cap() {
    let mut score = RelayScore {
        gateway_id: [0x42; 32],
        epoch: 1,
        forwarding_score: 500,
        availability_score: 500,
        bandwidth_score: 500,
        uptime_score: 500,
        diversity_bonus: 250,
        stake_multiplier: 50000, // way over cap
        composite: 0,
    };
    score.compute_composite();

    // Should be capped at MAX_STAKE_MULTIPLIER = 10000
    let raw = 500 * 300 + 500 * 250 + 500 * 200 + 500 * 150 + 250 * 100;
    // = 150000 + 125000 + 100000 + 75000 + 25000 = 475000
    // composite = 475000 * 10000 / 1000 = 4750000
    let expected = (raw as u64) * 10000 / 1000;
    assert_eq!(score.composite, expected);
}

// ── Score decay ──

#[test]
fn test_score_decay_no_inactive() {
    assert_eq!(RelayScore::decay_score(1_000_000, 0), 1_000_000);
}

#[test]
fn test_score_decay_reduces_over_time() {
    let initial = 1_000_000u64;
    let after_1 = RelayScore::decay_score(initial, 1);
    let after_10 = RelayScore::decay_score(initial, 10);

    assert!(after_1 < initial);
    assert!(after_10 < after_1);
    assert!(after_10 > 0);
}

#[test]
fn test_score_decay_converges_to_zero() {
    let initial = 1_000_000u64;
    let after_300 = RelayScore::decay_score(initial, 300);
    assert_eq!(after_300, 0); // should decay to zero after many epochs
}

// ── Stake multiplier ──

#[test]
fn test_stake_multiplier_no_stake() {
    assert_eq!(RelayScore::compute_stake_multiplier(0, 1000, 5000), DEFAULT_STAKE_MULTIPLIER);
}

#[test]
fn test_stake_multiplier_zero_unit() {
    assert_eq!(RelayScore::compute_stake_multiplier(1000, 0, 5000), DEFAULT_STAKE_MULTIPLIER);
}

#[test]
fn test_stake_multiplier_increases_with_stake() {
    let m1 = RelayScore::compute_stake_multiplier(1000, 1000, 5000);
    let m2 = RelayScore::compute_stake_multiplier(5000, 1000, 5000);
    assert!(m2 > m1);
}

#[test]
fn test_stake_multiplier_capped() {
    let m = RelayScore::compute_stake_multiplier(1_000_000, 1, 5000);
    assert!(m <= MAX_STAKE_MULTIPLIER);
}

// ── Trust Registry ──

#[test]
fn test_registry_full_lifecycle() {
    let mut reg = TrustRegistry::new(100);

    let gw1 = make_score(0x01, 800, 900, 700, 850, 300, 1000);
    let gw2 = make_score(0x02, 600, 700, 500, 600, 200, 1000);
    let gw3 = make_score(0x03, 900, 950, 800, 900, 400, 1000);

    reg.update_score(gw1);
    reg.update_score(gw2);
    reg.update_score(gw3);

    assert_eq!(reg.len(), 3);

    // Set stakes
    reg.set_stake([0x01; 32], 5000);
    reg.set_stake([0x02; 32], 1000);
    assert_eq!(reg.get_stake(&[0x01; 32]), 5000);
    assert_eq!(reg.get_stake(&[0x03; 32]), 0); // no stake set

    // Top gateways
    let top = reg.top_gateways(2);
    assert_eq!(top.len(), 2);
    // gw3 has highest composite
    assert_eq!(top[0].gateway_id, [0x03; 32]);
}

#[test]
fn test_registry_deterministic_ordering() {
    let mut reg = TrustRegistry::new(100);

    // Insert in random order
    for i in (0..10u8).rev() {
        reg.update_score(make_score(i, 500, 500, 500, 500, 250, 1000));
    }

    let top1: Vec<[u8; 32]> = reg.top_gateways(5).iter().map(|s| s.gateway_id).collect();
    let top2: Vec<[u8; 32]> = reg.top_gateways(5).iter().map(|s| s.gateway_id).collect();
    assert_eq!(top1, top2);
}

#[test]
fn test_registry_apply_decay() {
    let mut reg = TrustRegistry::new(100);
    reg.current_epoch = 20;

    let mut score = make_score(0x01, 800, 900, 700, 850, 300, 1000);
    score.epoch = 1; // 19 epochs old
    let before = score.composite;
    reg.update_score(score);

    reg.apply_decay(10); // threshold = 10, excess = 9
    let after = reg.get_score(&[0x01; 32]).unwrap().composite;

    assert!(after < before);
}

// ── Availability Proof ──

#[test]
fn test_availability_proof_scoring() {
    let proof = AvailabilityProof {
        gateway_id: [0x42; 32],
        window_start: 0,
        window_end: 3000,
        heartbeat_count: 100,
        heartbeat_root: [0u8; 32],
        peer_diversity: 10,
        signature: [0u8; 64],
    };

    assert_eq!(proof.availability_score(), 1000);
    assert!(proof.is_highly_available());
}

#[test]
fn test_availability_proof_partial() {
    let proof = AvailabilityProof {
        gateway_id: [0x42; 32],
        window_start: 0,
        window_end: 3000,
        heartbeat_count: 90,
        heartbeat_root: [0u8; 32],
        peer_diversity: 5,
        signature: [0u8; 64],
    };

    assert_eq!(proof.availability_score(), 900);
    assert!(!proof.is_highly_available());
}

#[test]
fn test_availability_proof_signing_bytes() {
    let proof = AvailabilityProof {
        gateway_id: [0x42; 32],
        window_start: 0,
        window_end: 3000,
        heartbeat_count: 100,
        heartbeat_root: [0u8; 32],
        peer_diversity: 10,
        signature: [0u8; 64],
    };

    let bytes = proof.to_signing_bytes();
    assert_eq!(bytes.len(), 32 + 8 + 8 + 4 + 32 + 2);
}

// ── Bandwidth Proof ──

#[test]
fn test_bandwidth_proof_efficiency() {
    let proof = BandwidthProof {
        gateway_id: [0x42; 32],
        window_start: 0,
        window_end: 100,
        envelope_count: 100,
        bytes_relayed: 102400, // 1KB per envelope
        source_diversity: 5,
        destination_diversity: 3,
        relay_merkle_root: [0u8; 32],
        signature: [0u8; 64],
    };

    assert_eq!(proof.efficiency_score(), 1000);
}

#[test]
fn test_bandwidth_proof_zero_envelopes() {
    let proof = BandwidthProof {
        gateway_id: [0x42; 32],
        window_start: 0,
        window_end: 100,
        envelope_count: 0,
        bytes_relayed: 0,
        source_diversity: 0,
        destination_diversity: 0,
        relay_merkle_root: [0u8; 32],
        signature: [0u8; 64],
    };

    assert_eq!(proof.efficiency_score(), 0);
}

#[test]
fn test_bandwidth_proof_signing_bytes() {
    let proof = BandwidthProof {
        gateway_id: [0u8; 32],
        window_start: 0,
        window_end: 100,
        envelope_count: 50,
        bytes_relayed: 51200,
        source_diversity: 5,
        destination_diversity: 3,
        relay_merkle_root: [0u8; 32],
        signature: [0u8; 64],
    };

    assert_eq!(proof.to_signing_bytes().len(), 104);
}

// ── Economics: Reward Distribution ──

#[test]
fn test_reward_forwarding() {
    let rewards = RewardDistribution {
        octo_b_per_envelope: 10,
        octo_n_per_hour: 5,
        octo_b_per_byte: 1,
        octo_n_per_window: 100,
    };

    assert_eq!(rewards.forwarding_reward(50), 500);
}

#[test]
fn test_reward_availability_with_score() {
    let rewards = RewardDistribution {
        octo_b_per_envelope: 10,
        octo_n_per_hour: 5,
        octo_b_per_byte: 1,
        octo_n_per_window: 100,
    };

    // Full availability
    let full = rewards.availability_reward(10, 1000);
    assert_eq!(full, 50); // 10 * 5 * 1000/1000

    // 50% availability
    let half = rewards.availability_reward(10, 500);
    assert_eq!(half, 25); // 10 * 5 * 500/1000
}

#[test]
fn test_reward_bandwidth() {
    let rewards = RewardDistribution {
        octo_b_per_envelope: 10,
        octo_n_per_hour: 5,
        octo_b_per_byte: 2,
        octo_n_per_window: 100,
    };

    assert_eq!(rewards.bandwidth_reward(1000), 2000);
}

// ── Economics: Slashing ──

#[test]
fn test_slashing_amounts() {
    let stake = 10_000u64;

    assert_eq!(
        RewardDistribution::slashing_amount(stake, SlashingCondition::InvalidProof),
        stake * SLASH_INVALID_PROOF / 10000
    );
    assert_eq!(
        RewardDistribution::slashing_amount(stake, SlashingCondition::ProofReplay),
        stake * SLASH_PROOF_REPLAY / 10000
    );
    assert_eq!(
        RewardDistribution::slashing_amount(stake, SlashingCondition::ConsensusViolation),
        stake * SLASH_CONSENSUS_VIOLATION / 10000
    );
    assert_eq!(
        RewardDistribution::slashing_amount(stake, SlashingCondition::LowAvailability),
        0 // reward reduction, not slashing
    );
}

#[test]
fn test_reward_reduction_low_availability() {
    // Below 500 threshold
    let reduced = RewardDistribution::reward_reduction(1000, 250);
    assert_eq!(reduced, 500); // 1000 * 250 / 500

    // Above 500 threshold
    let full = RewardDistribution::reward_reduction(1000, 750);
    assert_eq!(full, 1000);
}

// ── Economics: Archival ──

#[test]
fn test_archival_cost() {
    assert_eq!(compute_archival_cost(1000), 1000);
    assert_eq!(compute_archival_cost(0), 0);
}

// ── Score weights sum ──

#[test]
fn test_score_weights_sum() {
    let sum = WEIGHT_FORWARDING + WEIGHT_AVAILABILITY + WEIGHT_BANDWIDTH + WEIGHT_UPTIME + WEIGHT_DIVERSITY;
    assert_eq!(sum, 1000);
}
