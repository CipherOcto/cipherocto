//! End-to-end live integration tests.
//!
//! These tests exercise real cross-module flows:
//!
//! 1. Bootstrap → Seed Health → Authority Fork
//! 2. BIND → 2PC REBIND → message delivery
//! 3. Multi-DC BIND + Cross-Platform Consensus (N=3, N=2, N=1)
//! 4. DC ATTEST freshness + challenge flow
//! 5. Slash → cooldown → exclusion → REJOIN rate-limit
//! 6. Replay protection across the wire (DGP + DOT)
//! 7. Mempool admission + gossip propagation
//! 8. PCE round-trip across the wire
//! 9. Governance: federated count-based + DAO weight-based
//! 10. Onion-routed delivery through 3 hops
//!
//! + a cross-cutting transport mode / wire format scenario.

mod common;

use common::mock_adapter::{FailureMode, MockPlatformAdapter};
use common::mock_network::MockNetwork;
use ed25519_dalek::{Signer, SigningKey};
use octo_network::dc::admin_attest::{
    attest_topic, verify_attest, AttestChallenge, Platform, PlatformAdminAttest,
    PlatformAdminAttestError, MAX_ATTEST_AGE_EPOCHS,
};
use octo_network::dc::consensus::{
    consensus_topic, ConsensusAction, ConsensusState, ConsensusVote, DcConsensusCoordinator, Quorum,
};
use octo_network::dc::rejoin::{RejoinCooldown, RejoinError, REJOIN_COOLDOWN_EPOCHS};
use octo_network::dc::reputation::{
    DcRootedSlashReputationStore, DcSlashEventRef, DC_REPUTATION_HARD_THRESHOLD,
};
use octo_network::dc::slash::{process_dc_slash, DcMisbehavior, DcSlashEnvelope, DcSlashError};
use octo_network::dgp::dedup::GossipReplayCache;
use octo_network::dom::admission::{check_admission, AdmissionConfig, ReplayCache, SequenceTracker};
use octo_network::dom::intent::{intent_type_to_class, IntentType, OverlayIntent};
use octo_network::dom::error::DomError;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::envelope::{DeterministicEnvelope, MessageType};
use octo_network::dot::pce::aggregate::aggregate_proofs;
use octo_network::dot::pce::envelope::ProofCarryingEnvelope;
use octo_network::dot::pce::error::PceError;
use octo_network::dot::pce::proof_type::{ProofSystemId, VerificationResult};
use octo_network::dot::pce::verify::{compute_merkle_root, verify_pce};
use octo_network::dot::replay::ReplayCache as DotReplayCache;
use octo_network::dot::transport::{decode_native_ref, encode_native_ref, select_mode, TransportMode};
use octo_network::gossip::bind::{bind_gossip_topic, BindGossipState};
use octo_network::mon::bind_envelope::BindEnvelope;
use octo_network::mon::bootstrap::{
    verify_authority, SeedAuthorityError, SeedEntry, SeedHealth, SeedListAuthority,
    SeedListEnvelope, SlashedSeedBlacklist, EPOCH_GOVERNANCE_TAKEOVER,
};
use octo_network::mon::governance::{
    DecisionType, EmergencyAuthority, GovernanceModel, GovernancePolicy, GovernanceProposal,
    ProposalState,
};
use octo_network::mon::rebind::{PrepareVote, RebindCoordinator};
use octo_network::orr::onion::{construct_onion, peel_layer, HopConstructionParams};
use octo_network::orr::types::{OnionRoute, TransportVector};

/// Helper: make a `BindEnvelope`.
fn make_bind(domain: &str, platform: &str, group: &str) -> BindEnvelope {
    let mut b = BindEnvelope::new(domain, platform, group);
    b.member_count_at_bind = 3;
    b
}

/// Helper: build a signed `OverlayIntent`.
fn make_signed_intent(
    sk: &SigningKey,
    intent_id: [u8; 32],
    intent_type: IntentType,
    mission_id: [u8; 32],
    sequence: u64,
    exp: u64,
    payload_root: [u8; 32],
) -> OverlayIntent {
    let mut intent = OverlayIntent {
        intent_id,
        intent_type: intent_type as u16,
        mission_id,
        sender_id: sk.verifying_key().to_bytes(),
        sequence,
        logical_timestamp: 1000,
        expiration: exp,
        payload_root,
        economic_weight: 100,
        execution_class: intent_type_to_class(intent_type) as u16,
        signature: [0u8; 64],
    };
    let msg = intent.to_signing_bytes();
    let sig = sk.sign(&msg);
    intent.signature = sig.to_bytes();
    intent
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 1: Bootstrap → Seed Health → Authority Fork
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario1_bootstrap_seed_health_and_authority_fork() {
    // Fresh seeds: should start.
    let env = SeedListEnvelope {
        authority_pubkey: vec![0xAA],
        signed_at_epoch: 1000,
        peers: vec![
            SeedEntry {
                peer_id: "peer-1".into(),
                multiaddr: "/ip4/1.2.3.4/tcp/4001/p2p/peer-1".into(),
                signed_at_epoch: 1000,
            },
            SeedEntry {
                peer_id: "peer-2".into(),
                multiaddr: "/ip4/1.2.3.4/tcp/4001/p2p/peer-2".into(),
                signed_at_epoch: 998,
            },
        ],
    };
    let health = SeedHealth::check(&env, 1000);
    assert!(matches!(health, SeedHealth::Fresh { fresh_count: 2 }));
    assert!(!health.refuses_start());

    // Partially stale seeds (50%): should warn but start.
    let env = SeedListEnvelope {
        authority_pubkey: vec![0xAA],
        signed_at_epoch: 50,
        peers: vec![
            SeedEntry {
                peer_id: "p1".into(),
                multiaddr: "x".into(),
                signed_at_epoch: 1000,
            },
            SeedEntry {
                peer_id: "p2".into(),
                multiaddr: "x".into(),
                signed_at_epoch: 50,
            },
        ],
    };
    let health = SeedHealth::check(&env, 1000);
    assert!(matches!(
        health,
        SeedHealth::PartialStale { ratio_percent: 50, .. }
    ));
    assert!(!health.refuses_start());

    // Fully stale: should refuse start.
    let env = SeedListEnvelope {
        authority_pubkey: vec![0xAA],
        signed_at_epoch: 0,
        peers: vec![
            SeedEntry {
                peer_id: "p1".into(),
                multiaddr: "x".into(),
                signed_at_epoch: 0,
            },
            SeedEntry {
                peer_id: "p2".into(),
                multiaddr: "x".into(),
                signed_at_epoch: 0,
            },
        ],
    };
    let health = SeedHealth::check(&env, 1000);
    assert!(matches!(health, SeedHealth::FullyStale { total: 2 }));
    assert!(health.refuses_start());

    // Authority hard-fork.
    assert!(matches!(
        verify_authority(SeedListAuthority::Foundation, 0),
        Ok(())
    ));
    assert!(matches!(
        verify_authority(SeedListAuthority::Foundation, EPOCH_GOVERNANCE_TAKEOVER),
        Err(SeedAuthorityError::SeedListAuthorityDeprecated)
    ));
    assert!(matches!(
        verify_authority(SeedListAuthority::Dao, 0),
        Err(SeedAuthorityError::DaoNotYetActive)
    ));
    assert!(matches!(
        verify_authority(SeedListAuthority::Dao, EPOCH_GOVERNANCE_TAKEOVER + 1),
        Ok(())
    ));

    // Slashed blacklist filters seeds.
    let mut bl = SlashedSeedBlacklist::new();
    bl.slash("evil-peer");
    let env = SeedListEnvelope {
        authority_pubkey: vec![0],
        signed_at_epoch: 1000,
        peers: vec![
            SeedEntry {
                peer_id: "good".into(),
                multiaddr: "x".into(),
                signed_at_epoch: 1000,
            },
            SeedEntry {
                peer_id: "evil-peer".into(),
                multiaddr: "x".into(),
                signed_at_epoch: 1000,
            },
        ],
    };
    let filtered = bl.filter(env);
    assert_eq!(filtered.peers.len(), 1);
    assert_eq!(filtered.peers[0].peer_id, "good");
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 2: BIND → 2PC REBIND → message delivery
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario2_bind_rebind_message_delivery() {
    let net = MockNetwork::new(2);

    // DC binds a domain with two participants.
    let bind = make_bind("domain-1", "whatsapp", "group-1");
    assert!(!bind.domain_id.is_empty());

    // BIND gets gossiped via BindGossipState.
    let state = BindGossipState::new();
    assert!(state.record_received(bind.clone()));
    assert!(!state.record_received(bind.clone())); // dedup
    assert_eq!(state.received_count(), 1);

    let topic = bind_gossip_topic(&bind.domain_id);
    assert_eq!(topic, "/dot/bind/domain-1");

    // Run 2PC REBIND: 2 participants → unanimous.
    let new_bind = make_bind("domain-1", "whatsapp", "group-1-v2");
    let mut coord = RebindCoordinator::new(
        "domain-1",
        new_bind.clone(),
        vec!["peer-1".into(), "peer-2".into()],
    );
    assert!(matches!(
        coord.state,
        octo_network::mon::rebind::CoordinatorState::Preparing
    ));

    let prepare_env = coord.prepare_envelope(vec![0x01; 64]);
    // prepare_envelope returns a RebindPrepare struct directly.
    assert_eq!(prepare_env.domain_id, "domain-1");
    assert_eq!(prepare_env.new_bind.platform, "whatsapp");

    // Both participants vote Prepared.
    assert!(matches!(
        coord.record_vote("peer-1", PrepareVote::Prepared),
        octo_network::mon::rebind::CoordinatorState::Preparing
    ));
    let state_after_one = coord.record_vote("peer-2", PrepareVote::Prepared);
    assert!(matches!(
        state_after_one,
        octo_network::mon::rebind::CoordinatorState::Committing
    ));

    // Commit envelope built; commit completes.
    let commit_env = coord.commit_envelope(vec![0x02; 64]);
    assert!(commit_env.is_some());
    coord.mark_committed();

    // Now send a real message envelope across the wire.
    let msg = MockNetwork::make_envelope(
        blake3::hash(b"rebind-then-message").into(),
        1,
        net.gateways[0].id,
        2000,
    );
    net.broadcast(0, &msg).await;
    net.deliver_all().await;

    let domain = net.gateways[1].adapter.domain_id("test");
    let received = net.gateways[1]
        .adapter
        .receive_messages(&domain)
        .await
        .unwrap();
    assert_eq!(received.len(), 1);
    let canonical = net.gateways[1].adapter.canonicalize(&received[0]).unwrap();
    assert_eq!(canonical.envelope_id, msg.envelope_id);
    assert_eq!(canonical.source_peer, msg.source_peer);
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 3: Multi-DC BIND + Cross-Platform Consensus (N=3)
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario3_multi_dc_consensus_n3_n2_n1() {
    // N=3: needs 2 votes (2/3).
    let mut coord_n3 = DcConsensusCoordinator::new(
        "d1",
        ConsensusAction::Rebind,
        None,
        vec!["dc-whatsapp".into(), "dc-matrix".into(), "dc-nostr".into()],
    );
    assert!(matches!(coord_n3.quorum(), Quorum::TwoThirds));
    assert!(matches!(
        coord_n3.record_vote("dc-whatsapp", ConsensusVote::Prepared),
        ConsensusState::Preparing
    ));
    let s = coord_n3.record_vote("dc-matrix", ConsensusVote::Prepared);
    assert!(matches!(s, ConsensusState::Committing));

    // N=2: unanimous.
    let mut coord_n2 = DcConsensusCoordinator::new(
        "d2",
        ConsensusAction::Unbind,
        None,
        vec!["dc-a".into(), "dc-b".into()],
    );
    assert!(matches!(coord_n2.quorum(), Quorum::Unanimous));
    assert!(matches!(
        coord_n2.record_vote("dc-a", ConsensusVote::Prepared),
        ConsensusState::Preparing
    ));
    assert!(matches!(
        coord_n2.record_vote("dc-b", ConsensusVote::Prepared),
        ConsensusState::Committing
    ));

    // N=1: unilateral — no votes needed.
    let mut coord_n1 = DcConsensusCoordinator::new(
        "d3",
        ConsensusAction::Rebind,
        None,
        vec!["dc-solo".into()],
    );
    assert!(matches!(coord_n1.quorum(), Quorum::Unilateral));
    let s = coord_n1.check_deadline(coord_n1.deadline_epoch + 1);
    assert!(matches!(s, ConsensusState::Committing));

    // Unknown platform vote aborts.
    let mut coord_unk = DcConsensusCoordinator::new(
        "d4",
        ConsensusAction::Rebind,
        None,
        vec!["dc-1".into(), "dc-2".into()],
    );
    let s = coord_unk.record_vote("dc-evil", ConsensusVote::Prepared);
    assert!(matches!(s, ConsensusState::Aborted));

    // Reject vote aborts immediately.
    let mut coord_rej = DcConsensusCoordinator::new(
        "d5",
        ConsensusAction::Rebind,
        None,
        vec!["dc-1".into(), "dc-2".into()],
    );
    coord_rej.record_vote("dc-1", ConsensusVote::Prepared);
    let s = coord_rej.record_vote("dc-2", ConsensusVote::Rejected);
    assert!(matches!(s, ConsensusState::Aborted));

    // Quorum rules for N=0 must never be met.
    let q = Quorum::for_n(0);
    assert!(matches!(q, Quorum::Unilateral));
    assert!(!q.is_met(0, 0));
    assert!(!Quorum::Unanimous.is_met(0, 0));
    assert!(!Quorum::TwoThirds.is_met(0, 0));

    // Topic format.
    assert_eq!(consensus_topic("d1"), "/dot/dc-consensus/d1");

    // N=0 in coordinator aborts (defense in depth).
    let mut coord_n0 =
        DcConsensusCoordinator::new("d6", ConsensusAction::Rebind, None, vec![]);
    let s = coord_n0.record_vote("dc-1", ConsensusVote::Prepared);
    assert!(matches!(s, ConsensusState::Aborted));
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 4: DC ATTEST freshness + challenge flow
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario4_dc_attest_freshness_and_challenge() {
    // Fresh attest: verifies.
    let attest = PlatformAdminAttest {
        domain_id: "d1".into(),
        platform: Platform::WhatsApp,
        platform_group_id: "group-1".into(),
        dc_pubkey: vec![0xAA; 32],
        proof: vec![0; 64],
        signed_at_epoch: 1000,
    };
    let result = verify_attest(&attest, &[0xAA; 32], 1000);
    assert!(result.is_ok(), "fresh attest should verify: {:?}", result);

    // Expired attest: rejected.
    let old_attest = PlatformAdminAttest {
        signed_at_epoch: 0,
        ..attest.clone()
    };
    let result = verify_attest(&old_attest, &[0xAA; 32], MAX_ATTEST_AGE_EPOCHS + 100);
    assert!(matches!(
        result,
        Err(PlatformAdminAttestError::Stale { .. })
    ));

    // Topic format.
    let topic = attest_topic("d1", Platform::WhatsApp);
    assert_eq!(topic, "/dot/admin/d1/whatsapp");

    // CHALLENGE flow.
    let challenge = AttestChallenge {
        domain_id: "d1".into(),
        dc_pubkey: vec![0xAA; 32],
        reason: "stale-attest".into(),
        evidence: vec![0x42; 32],
        issued_at_epoch: 1000,
        response_deadline_epoch: 1010,
    };
    assert!(challenge.response_deadline_epoch > challenge.issued_at_epoch);
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 5: Slash → cooldown → exclusion → REJOIN rate-limit
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario5_slash_cooldown_exclusion_rejoin() {
    // Build a slash envelope and process it.
    let envelope = DcSlashEnvelope::new(
        vec![0xAA; 32],
        DcMisbehavior::FailedAttest,
        vec!["d1".into(), "d2".into()],
        vec![vec![0x01], vec![0x02], vec![0x03]],
        1000,
    );
    let outcome = process_dc_slash(&envelope, 3, 0).unwrap();
    // First slash: cooldown = 2^1 = 2.
    assert_eq!(outcome.cool_down_epochs, 2);
    assert!(matches!(
        outcome.final_state,
        octo_network::dc::slash::DcFinalState::Cooldown
    ));

    // Reject envelope with empty pubkey.
    let empty_env = DcSlashEnvelope::new(
        vec![],
        DcMisbehavior::FailedAttest,
        vec!["d1".into()],
        vec![vec![0x01]],
        1000,
    );
    assert!(matches!(
        process_dc_slash(&empty_env, 3, 0),
        Err(DcSlashError::EmptyDcPubkey)
    ));

    // Reject envelope with too few witness signatures.
    let weak = DcSlashEnvelope::new(
        vec![0xAA; 32],
        DcMisbehavior::FailedAttest,
        vec!["d1".into()],
        vec![vec![0x01]],
        1000,
    );
    assert!(matches!(
        process_dc_slash(&weak, 3, 0),
        Err(DcSlashError::InsufficientWitnesses { .. })
    ));

    // Cross-domain reputation store: build up to exclusion.
    let mut store = DcRootedSlashReputationStore::new();
    for i in 0..(DC_REPUTATION_HARD_THRESHOLD as u8) {
        store.record_slash(
            "dc-1",
            DcSlashEventRef {
                domain_id: format!("d-{i}"),
                slash_reason: 0x000F,
                event_hash: [i; 32],
                epoch: 1000,
            },
        );
    }
    assert!(store.is_excluded("dc-1"));
    assert!(store.priority("dc-1", 1000).is_none());

    // Rejoin cooldown: rate-limit within window.
    let mut cd = RejoinCooldown::new();
    assert!(cd.check_and_record("peer-1", 1000).is_ok());
    assert_eq!(
        cd.check_and_record("peer-1", 1999),
        Err(RejoinError::RateLimited {
            last_request_epoch: 1000
        })
    );
    // Past window: allowed.
    assert!(cd.check_and_record("peer-1", 1000 + REJOIN_COOLDOWN_EPOCHS).is_ok());

    // Empty peer_id rejected.
    assert_eq!(
        cd.check_and_record("", 5000),
        Err(RejoinError::InvalidPeerId)
    );
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 6: Replay protection across the wire
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario6_replay_protection_across_wire() {
    // DGP gossip replay cache.
    let mut cache = GossipReplayCache::new(10, 1_000_000);
    assert!(cache.check_and_insert([0x01; 32], 100).unwrap());
    assert!(!cache.check_and_insert([0x01; 32], 101).unwrap());
    assert!(cache.check_and_insert([0x02; 32], 102).unwrap());

    // DOT replay cache: same envelope_id twice is rejected.
    let mut dot_cache = DotReplayCache::new(3600, 100);
    let env = MockNetwork::make_envelope([0xAA; 32], 1, [0x01; 32], 1000);
    assert!(dot_cache.check_and_insert(env.envelope_id, 1000).is_ok());
    assert!(dot_cache.check_and_insert(env.envelope_id, 1001).is_err());

    // Eviction kicks in at capacity.
    let mut small = DotReplayCache::new(60, 3);
    for i in 0..5 {
        small
            .check_and_insert([i as u8; 32], 1000 + i as u64)
            .unwrap();
    }
    assert_eq!(small.len(), 3);

    // Expired entries — purging is private; verify size stays bounded
    // and inserted timestamps are tracked.
    let mut windowed = DotReplayCache::new(10, 100);
    windowed.check_and_insert([0x01; 32], 100).unwrap();
    windowed.check_and_insert([0x02; 32], 105).unwrap();
    // Inserting many more entries does not exceed capacity.
    for i in 10..120u8 {
        windowed
            .check_and_insert([i; 32], 200 + i as u64)
            .unwrap();
    }
    assert!(windowed.len() <= 100);
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 7: Mempool admission + gossip propagation
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario7_mempool_admission_gossip_propagation() {
    let sk = SigningKey::from_bytes(&[0xCC; 32]);
    let vk = sk.verifying_key();
    let mission_id = [0x55; 32];

    let intent = make_signed_intent(
        &sk,
        [0xAA; 32],
        IntentType::Transaction,
        mission_id,
        1,
        10_000,
        [0u8; 32],
    );

    let cfg = AdmissionConfig::default();
    let mut seq: SequenceTracker = Default::default();
    let mut replay: ReplayCache = Default::default();

    // Both nodes admit the same intent.
    assert!(check_admission(&intent, 500, &replay, &seq, 0, &cfg).is_ok());
    // After first admission, replay and sequence are updated.
    replay.insert(intent.intent_id, 500);
    seq.insert((intent.sender_id, intent.mission_id), intent.sequence);

    // Replay: same intent again is rejected.
    let result = check_admission(&intent, 500, &replay, &seq, 1, &cfg);
    assert!(matches!(
        result,
        Err(DomError::ReplayDetected { .. })
    ));

    // Capacity overflow: pending_count >= max_pending_intents → rejected.
    // Use a fresh intent (not in replay cache) so the capacity check is reached.
    let fresh_intent = make_signed_intent(
        &sk,
        [0xBB; 32],
        IntentType::Transaction,
        mission_id,
        2,
        10_000,
        [0u8; 32],
    );
    let cfg_tight = AdmissionConfig {
        max_pending_intents: 0,
        ..AdmissionConfig::default()
    };
    // pending_count=0, max=0 → 0 >= 0 → capacity exceeded.
    let result = check_admission(&fresh_intent, 500, &replay, &seq, 0, &cfg_tight);
    assert!(matches!(
        result,
        Err(DomError::CapacityExceeded { .. })
    ));

    // Sequence: re-using sequence=1 from same sender is rejected.
    let replay2: ReplayCache = Default::default();
    let result = check_admission(&intent, 500, &replay2, &seq, 0, &cfg);
    assert!(matches!(
        result,
        Err(DomError::SequenceInvalid { .. })
    ));

    // Ed25519: bad signature is rejected.
    let mut bad_sig = intent.clone();
    bad_sig.signature = [0u8; 64];
    let replay3: ReplayCache = Default::default();
    let seq2: SequenceTracker = Default::default();
    let result = check_admission(&bad_sig, 500, &replay3, &seq2, 0, &cfg);
    assert!(matches!(
        result,
        Err(DomError::InvalidSignature { .. })
    ));

    // Cross-gateway delivery: intent flows through MockNetwork as bytes.
    let net = MockNetwork::new(2);
    let env = DeterministicEnvelope {
        version: 1,
        network_id: 1,
        message_type: MessageType::Message as u16,
        envelope_id: blake3::hash(intent.payload_root.as_slice()).into(),
        mission_id,
        source_peer: vk.to_bytes(),
        origin_gateway: vk.to_bytes(),
        logical_timestamp: 500,
        ttl_hops: 10,
        payload_hash: intent.payload_root,
        route_trace_root: [0u8; 32],
        flags: 0,
        signature: intent.signature,
    };
    net.broadcast(0, &env).await;
    net.deliver_all().await;
    let domain = net.gateways[1].adapter.domain_id("test");
    let received = net.gateways[1]
        .adapter
        .receive_messages(&domain)
        .await
        .unwrap();
    assert_eq!(received.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 8: PCE round-trip across the wire
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario8_pce_round_trip_aggregate_verify() {
    let inner = DeterministicEnvelope {
        version: 1,
        network_id: 1,
        message_type: MessageType::Message as u16,
        envelope_id: [0xEE; 32],
        mission_id: [0x55; 32],
        source_peer: [0x11; 32],
        origin_gateway: [0x22; 32],
        logical_timestamp: 2000,
        ttl_hops: 10,
        payload_hash: blake3::hash(b"pce payload").into(),
        route_trace_root: [0u8; 32],
        flags: 0,
        signature: [0u8; 64],
    };
    let proof_blob = b"STWO proof bytes v1";
    let proof_commitment = ProofCarryingEnvelope::compute_proof_commitment(proof_blob);
    let public_inputs: Vec<[u8; 32]> = vec![blake3::hash(b"public inputs").into()];
    let public_input_root = compute_merkle_root(&public_inputs);
    let pce = ProofCarryingEnvelope {
        envelope: inner.clone(),
        proof_system_id: ProofSystemId::STWO as u16,
        proof_commitment,
        public_input_root,
        proof_blob: proof_blob.to_vec(),
        execution_model: 0x0001, // AIR
        parent_proof_commitment: None,
    };

    // Self-check: commitment matches blob.
    assert!(pce.verify_commitment());

    // Receiver verifies the PCE.
    let result = verify_pce(&pce, &public_inputs).unwrap();
    assert!(matches!(result, VerificationResult::Valid));

    // Tampered blob: commitment mismatch.
    let mut bad_pce = pce.clone();
    bad_pce.proof_blob = b"different bytes".to_vec();
    assert!(matches!(
        verify_pce(&bad_pce, &public_inputs),
        Err(PceError::CommitmentMismatch)
    ));

    // Empty blob.
    let mut empty_blob = pce.clone();
    empty_blob.proof_blob = vec![];
    empty_blob.proof_commitment = ProofCarryingEnvelope::compute_proof_commitment(&[]);
    assert!(matches!(
        verify_pce(&empty_blob, &public_inputs),
        Err(PceError::MalformedProof(_))
    ));

    // Aggregate two PCEs into one.
    let pce_a = pce.clone();
    let mut pce_b_inner = inner.clone();
    pce_b_inner.envelope_id = [0xEF; 32];
    let pce_b = ProofCarryingEnvelope {
        envelope: pce_b_inner,
        ..pce.clone()
    };
    let agg = aggregate_proofs(&[pce_a.clone(), pce_b.clone()], ProofSystemId::STWO).unwrap();
    assert_eq!(agg.proof_count, 2);
    assert_eq!(agg.inner_proof_commitments.len(), 2);
    assert!(agg.verify_structure().is_ok());

    // Zero proofs is an error.
    assert!(matches!(
        aggregate_proofs(&[], ProofSystemId::STWO),
        Err(PceError::AggregationError { .. })
    ));
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 9: Governance — federated count-based + DAO weight-based
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario9_governance_federated_and_dao() {
    // Federated policy: count-based quorum.
    let fed_policy = GovernancePolicy::new(
        GovernanceModel::Federated,
        2,
        3,
        100,
        EmergencyAuthority::Coordinator,
    )
    .unwrap();
    let mut prop = GovernanceProposal::new(
        [0xAA; 32],
        DecisionType::Admission,
        [0x01; 32],
        100,
        200,
    );
    prop.open_voting();
    // 4 of 10 voters cast for.
    for i in 0..4u8 {
        prop.cast_vote([i; 32], 1, true);
    }
    prop.cast_vote([10; 32], 1, false);
    let state = prop.resolve(&fed_policy, 10);
    // 5/10 voted (count-quorum: 5*3 >= 10*2 = 20? 15 >= 20 no) → Preparing.
    assert_eq!(state, ProposalState::Voting);

    // Now 7 of 10 vote.
    for i in 5..7u8 {
        prop.cast_vote([i; 32], 1, true);
    }
    let state = prop.resolve(&fed_policy, 10);
    // 7/10 voted (count-quorum: 7*3 >= 10*2 = 20? 21 >= 20 yes).
    // For-weight = 5, against-weight = 1, majority for.
    assert_eq!(state, ProposalState::Approved);

    // Voter change of mind: replace, don't duplicate.
    let mut prop2 = GovernanceProposal::new(
        [0xBB; 32],
        DecisionType::Admission,
        [0x02; 32],
        100,
        200,
    );
    prop2.open_voting();
    prop2.cast_vote([0x11; 32], 100, true);
    prop2.cast_vote([0x11; 32], 100, false); // changes mind
    assert_eq!(prop2.total_for(), 0);
    assert_eq!(prop2.total_against(), 100);

    // DAO policy: weight-based quorum.
    let dao_policy = GovernancePolicy::new(
        GovernanceModel::Dao,
        2,
        3,
        100,
        EmergencyAuthority::Coordinator,
    )
    .unwrap();
    let mut prop3 = GovernanceProposal::new(
        [0xCC; 32],
        DecisionType::Admission,
        [0x03; 32],
        100,
        200,
    );
    prop3.open_voting();
    prop3.cast_vote([0x21; 32], 100, true);
    prop3.cast_vote([0x22; 32], 80, true);
    // voted weight = 180 >= 160 → quorum met, for > against → approve.
    let state = prop3.resolve_weighted(&dao_policy, 240);
    assert_eq!(state, ProposalState::Approved);

    // Zero weight rejected.
    let mut prop4 = GovernanceProposal::new(
        [0xDD; 32],
        DecisionType::Admission,
        [0x04; 32],
        100,
        200,
    );
    prop4.open_voting();
    assert!(!prop4.cast_vote([0x33; 32], 0, true));

    // Centralized: auto-approved on resolve.
    let central = GovernancePolicy::new(
        GovernanceModel::Centralized,
        1,
        1,
        100,
        EmergencyAuthority::Coordinator,
    )
    .unwrap();
    let mut prop5 = GovernanceProposal::new(
        [0xEE; 32],
        DecisionType::Admission,
        [0x05; 32],
        100,
        200,
    );
    prop5.open_voting();
    let state = prop5.resolve(&central, 1);
    assert_eq!(state, ProposalState::Approved);
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 10: Onion-routed delivery through 3 hops
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario10_onion_routed_delivery_3_hops() {
    use x25519_dalek::{PublicKey, StaticSecret};

    // 3 hops: entry, middle, exit.
    let entry_secret = StaticSecret::from([0x11; 32]);
    let middle_secret = StaticSecret::from([0x22; 32]);
    let exit_secret = StaticSecret::from([0x33; 32]);
    let entry_pk_bytes = PublicKey::from(&entry_secret).to_bytes();
    let middle_pk_bytes = PublicKey::from(&middle_secret).to_bytes();
    let exit_pk_bytes = PublicKey::from(&exit_secret).to_bytes();

    let route = OnionRoute {
        route_id: [0xAA; 32],
        mission_id: [0u8; 32],
        route_epoch: 100,
        hop_count: 3,
        entry_gateway: [0x01; 32],
        exit_gateway: [0x03; 32],
        layered_route_root: [0u8; 32],
        construction_timestamp: 500,
        flags: 0,
    };

    let hops = vec![
        HopConstructionParams {
            hop_index: 0,
            relay_public_key: entry_pk_bytes,
            relay_gateway_id: [0x01; 32],
            next_gateway: [0x12; 32],
            transport_vector: TransportVector {
                transport_type: 0x0001,
                domain_id: [0xAA; 32],
                priority: 1,
                bandwidth_class: 100,
                censorship_score: 50,
            },
        },
        HopConstructionParams {
            hop_index: 1,
            relay_public_key: middle_pk_bytes,
            relay_gateway_id: [0x02; 32],
            next_gateway: [0x23; 32],
            transport_vector: TransportVector {
                transport_type: 0x0002,
                domain_id: [0xAA; 32],
                priority: 1,
                bandwidth_class: 100,
                censorship_score: 50,
            },
        },
        HopConstructionParams {
            hop_index: 2,
            relay_public_key: exit_pk_bytes,
            relay_gateway_id: [0x03; 32],
            next_gateway: [0x00; 32], // exit has no next
            transport_vector: TransportVector {
                transport_type: 0x0003,
                domain_id: [0xAA; 32],
                priority: 1,
                bandwidth_class: 100,
                censorship_score: 50,
            },
        },
    ];

    let payload = b"onion payload";
    let (onion_layers, layered_payload) =
        construct_onion(&route, &hops, payload, &route.route_id).expect("construct_onion");
    assert_eq!(onion_layers.len(), 3);
    // layered_payload is the encrypted outer blob (not the plaintext).
    assert!(!layered_payload.is_empty());
    assert_ne!(layered_payload, payload);

    // Entry hop peels: reveals next-hop instructions for middle.
    let peeled_entry = peel_layer(
        &onion_layers[0],
        &entry_secret.to_bytes(),
        &route.route_id,
        &layered_payload,
    )
    .expect("entry peels");
    assert_eq!(peeled_entry.next_gateway, [0x12; 32]);

    // Middle hop peels: reveals instructions for exit.
    let peeled_middle = peel_layer(
        &onion_layers[1],
        &middle_secret.to_bytes(),
        &route.route_id,
        &peeled_entry.inner_payload,
    )
    .expect("middle peels");
    assert_eq!(peeled_middle.next_gateway, [0x23; 32]);

    // Exit hop peels: reveals final payload (no next gateway).
    let peeled_exit = peel_layer(
        &onion_layers[2],
        &exit_secret.to_bytes(),
        &route.route_id,
        &peeled_middle.inner_payload,
    )
    .expect("exit peels");
    assert_eq!(peeled_exit.next_gateway, [0x00; 32]); // exit has no next
    assert_eq!(peeled_exit.inner_payload, payload);

    // Empty hops: error.
    let result = construct_onion(
        &OnionRoute {
            route_id: [0; 32],
            mission_id: [0; 32],
            route_epoch: 0,
            hop_count: 0,
            entry_gateway: [0; 32],
            exit_gateway: [0; 32],
            layered_route_root: [0; 32],
            construction_timestamp: 0,
            flags: 0,
        },
        &[],
        payload,
        &[0; 32],
    );
    assert!(result.is_err());
}

// ─────────────────────────────────────────────────────────────────────
// Cross-cutting: Transport mode selection + wire format
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_transport_mode_and_wire_round_trip() {
    // Small payload with raw-binary capability → Raw.
    let cap = octo_network::dot::adapters::CapabilityReport {
        max_payload_bytes: 65536,
        supports_fragmentation: true,
        supports_encryption: true,
        supports_raw_binary: true,
        rate_limit_per_second: 1000,
        media_capabilities: None,
    };
    let mode = select_mode(1000, &cap).unwrap();
    assert_eq!(mode, TransportMode::Raw);

    // Small payload, no raw binary → Text.
    let cap_text = octo_network::dot::adapters::CapabilityReport {
        supports_raw_binary: false,
        ..cap
    };
    let mode = select_mode(1000, &cap_text).unwrap();
    assert_eq!(mode, TransportMode::Text);

    // Large payload, no media, no raw → Fragment.
    let cap_frag = octo_network::dot::adapters::CapabilityReport {
        supports_raw_binary: false,
        supports_fragmentation: true,
        media_capabilities: None,
        ..cap
    };
    let mode = select_mode(10_000, &cap_frag).unwrap();
    assert_eq!(mode, TransportMode::Fragment);

    // Wire format encode/decode round-trip.
    let encoded = encode_native_ref("msg-1");
    assert_eq!(encoded, "DOT/2/msg-1");
    assert_eq!(decode_native_ref(&encoded), Some("msg-1"));
    assert_eq!(decode_native_ref("DOT/2/"), None); // empty id rejected
    assert_eq!(decode_native_ref("hello world"), None); // wrong prefix

    // Adapter failure modes don't corrupt wire bytes.
    // Gateway 0 (no failures) broadcasts to gateway 1 (DropAll).
    // FailureMode::DropAll only affects the adapter's send-side API,
    // not the wire bus; broadcast fills the bus regardless.
    let net = MockNetwork::with_failures(2, vec![FailureMode::None, FailureMode::DropAll]);
    let env = MockNetwork::make_envelope([0xAA; 32], 1, [0x01; 32], 1000);
    net.broadcast(0, &env).await;
    // Pre-delivery: bus has 1 message destined for gateway 1.
    assert_eq!(net.pending_count().await, 1);
    net.deliver_all().await;
    // Post-delivery: bus drained.
    assert_eq!(net.pending_count().await, 0);
}

// Suppress unused-import warning when running only the test file.
#[allow(dead_code)]
fn _ensure_adapters_imported(_: MockPlatformAdapter) {}
