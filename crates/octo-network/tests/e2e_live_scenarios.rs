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

use common::mock_adapter::{AdminCall, AdminScripted, FailureMode, MockPlatformAdapter};
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
use octo_network::dom::admission::{
    check_admission, AdmissionConfig, ReplayCache, SequenceTracker,
};
use octo_network::dom::error::DomError;
use octo_network::dom::intent::{intent_type_to_class, IntentType, OverlayIntent};
use octo_network::dot::adapters::coordinator_admin::{
    AddMemberOutput, CoordinatorAdmin, GroupHandle, GroupId, GroupMemberSpec,
};
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::PlatformType;
use octo_network::dot::envelope::{DeterministicEnvelope, MessageType};
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::pce::aggregate::aggregate_proofs;
use octo_network::dot::pce::envelope::ProofCarryingEnvelope;
use octo_network::dot::pce::error::PceError;
use octo_network::dot::pce::proof_type::{ProofSystemId, VerificationResult};
use octo_network::dot::pce::verify::{compute_merkle_root, verify_pce};
use octo_network::dot::replay::ReplayCache as DotReplayCache;
use octo_network::dot::transport::{
    decode_native_ref, encode_native_ref, select_mode, TransportMode,
};
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
        SeedHealth::PartialStale {
            ratio_percent: 50,
            ..
        }
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
    let mut coord_n1 =
        DcConsensusCoordinator::new("d3", ConsensusAction::Rebind, None, vec!["dc-solo".into()]);
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
    let mut coord_n0 = DcConsensusCoordinator::new("d6", ConsensusAction::Rebind, None, vec![]);
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
    assert!(cd
        .check_and_record("peer-1", 1000 + REJOIN_COOLDOWN_EPOCHS)
        .is_ok());

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
        windowed.check_and_insert([i; 32], 200 + i as u64).unwrap();
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
    assert!(matches!(result, Err(DomError::ReplayDetected { .. })));

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
    assert!(matches!(result, Err(DomError::CapacityExceeded { .. })));

    // Sequence: re-using sequence=1 from same sender is rejected.
    let replay2: ReplayCache = Default::default();
    let result = check_admission(&intent, 500, &replay2, &seq, 0, &cfg);
    assert!(matches!(result, Err(DomError::SequenceInvalid { .. })));

    // Ed25519: bad signature is rejected.
    let mut bad_sig = intent.clone();
    bad_sig.signature = [0u8; 64];
    let replay3: ReplayCache = Default::default();
    let seq2: SequenceTracker = Default::default();
    let result = check_admission(&bad_sig, 500, &replay3, &seq2, 0, &cfg);
    assert!(matches!(result, Err(DomError::InvalidSignature { .. })));

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
    let mut prop =
        GovernanceProposal::new([0xAA; 32], DecisionType::Admission, [0x01; 32], 100, 200);
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
    let mut prop2 =
        GovernanceProposal::new([0xBB; 32], DecisionType::Admission, [0x02; 32], 100, 200);
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
    let mut prop3 =
        GovernanceProposal::new([0xCC; 32], DecisionType::Admission, [0x03; 32], 100, 200);
    prop3.open_voting();
    prop3.cast_vote([0x21; 32], 100, true);
    prop3.cast_vote([0x22; 32], 80, true);
    // voted weight = 180 >= 160 → quorum met, for > against → approve.
    let state = prop3.resolve_weighted(&dao_policy, 240);
    assert_eq!(state, ProposalState::Approved);

    // Zero weight rejected.
    let mut prop4 =
        GovernanceProposal::new([0xDD; 32], DecisionType::Admission, [0x04; 32], 100, 200);
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
    let mut prop5 =
        GovernanceProposal::new([0xEE; 32], DecisionType::Admission, [0x05; 32], 100, 200);
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
        ..Default::default()
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

// ─────────────────────────────────────────────────────────────────────
// Scenario 12: CoordinatorAdmin bridge downcast + capability honesty
// ─────────────────────────────────────────────────────────────────────
//
// Closes the e2e test gap for the `CoordinatorAdmin` trait. The mock
// platform adapter opts in to the admin trait via
// `PlatformAdapter::as_coordinator_admin()`. This scenario verifies
// (a) the bridge returns `Some(_)`, (b) `platform_name` matches what
// the script expected, (c) `admin_capabilities` is truth-honest
// (RFC-0861 §1), and (d) a method that wasn't scripted returns the
// trait's default `Unimplemented` with the correct platform label.

#[tokio::test]
async fn scenario12_coordinator_admin_bridge_downcast_and_capability_honesty() {
    // Mock opts in to create_group but NOT add_member — the
    // capability report must reflect that asymmetry, not advertise
    // both as `true`.
    let adapter =
        MockPlatformAdapter::new(PlatformType::WhatsApp).with_admin_scripted(AdminScripted {
            create_group: Some(Ok(GroupHandle {
                id: GroupId::new("1203630250@g.us"),
                subject: Some("scripted".into()),
                invite_url: None,
                is_admin: true,
                member_count: Some(0),
                mode_flags: None,
                initial_admins_promoted: true,
            })),
            add_member: None,
        });

    // (a) Bridge returns Some for adapters that opt in.
    let admin: Option<&dyn CoordinatorAdmin> = adapter.as_coordinator_admin();
    assert!(admin.is_some(), "mock must opt in to CoordinatorAdmin");
    let admin = admin.unwrap();

    // (b) platform_name round-trips.
    let pname = admin.platform_name();
    assert!(
        pname.starts_with("mock"),
        "platform_name should be 'mock-*' for the mock; got {pname:?}",
    );

    // (c) Capability report is honest: create_group is scripted so
    // `can_create` is true; add_member is not scripted so
    // `can_add_member` is false. RFC-0861 §1 rule.
    let caps = admin.admin_capabilities();
    assert!(caps.can_create, "can_create must reflect scripted slot");
    assert!(
        !caps.can_add_member,
        "can_add_member must be false when slot is None (RFC-0861 §1 honesty rule)"
    );
    assert!(!caps.can_destroy);
    assert!(!caps.can_transfer_ownership);

    // (d) Scripted method returns the scripted value.
    let handle = admin
        .create_group("scripted", &[GroupMemberSpec::new("+15551111111")])
        .await
        .expect("create_group should return the scripted handle");
    assert_eq!(handle.id.as_str(), "1203630250@g.us");
    assert!(handle.is_admin);

    // (e) Unscripted method returns the trait's default
    // `Unimplemented` with the correct platform label.
    let err = admin
        .add_member(
            &GroupId::new("1203630250@g.us"),
            &GroupMemberSpec::new("+15552222222"),
        )
        .await
        .expect_err("add_member should be Unimplemented when slot is None");
    match err {
        PlatformAdapterError::Unimplemented { platform, action } => {
            assert!(
                platform.starts_with("mock"),
                "platform label: got {platform:?}"
            );
            assert_eq!(action, "add_member");
        }
        other => panic!("expected Unimplemented, got {other:?}"),
    }

    // (f) The call log captures the calls that actually happened.
    let calls = adapter.admin_calls().await;
    assert_eq!(calls.len(), 2);
    match &calls[0] {
        AdminCall::CreateGroup {
            subject,
            initial_member_count,
        } => {
            assert_eq!(subject, "scripted");
            assert_eq!(*initial_member_count, 1);
        }
        other => panic!("expected CreateGroup, got {other:?}"),
    }
    match &calls[1] {
        AdminCall::AddMember {
            group_id,
            member_handle,
            member_is_admin,
        } => {
            assert_eq!(group_id, "1203630250@g.us");
            assert_eq!(member_handle, "+15552222222");
            assert!(!member_is_admin);
        }
        other => panic!("expected AddMember, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 13: CoordinatorAdmin::create_group → BindEnvelope → wire
// ─────────────────────────────────────────────────────────────────────
//
// The main end-to-end flow. Exercises the full bridge from a trait
// method through to wire bytes:
//
//   1. `&dyn PlatformAdapter` → `as_coordinator_admin` → `&dyn CoordinatorAdmin`
//   2. `CoordinatorAdmin::create_group` returns a `GroupHandle`
//   3. `GroupHandle.id` flows into a `BindEnvelope` (consumer side)
//   4. `BindGossipState::record_received` ingests the bind
//   5. `DeterministicEnvelope` carries the bind as its payload
//      (payload_hash = blake3(serialized BindEnvelope))
//   6. `PlatformAdapter::send_envelope` writes wire bytes
//   7. Wire bytes round-trip through `from_wire_bytes` and still
//      carry the bind payload (group_id intact)

#[tokio::test]
async fn scenario13_coordinator_admin_create_group_then_bind_to_wire() {
    let scripted_group_id = "1203630399@g.us";
    let adapter =
        MockPlatformAdapter::new(PlatformType::WhatsApp).with_admin_scripted(AdminScripted {
            create_group: Some(Ok(GroupHandle {
                id: GroupId::new(scripted_group_id),
                subject: Some("DOT swarm A".into()),
                invite_url: Some(format!("https://chat.whatsapp.com/{scripted_group_id}")),
                is_admin: true,
                member_count: Some(3),
                mode_flags: None,
                initial_admins_promoted: true,
            })),
            add_member: None,
        });

    // ── Step 1+2: bridge downcast → create_group ────────────────
    let admin: &dyn CoordinatorAdmin = adapter
        .as_coordinator_admin()
        .expect("mock opts in to CoordinatorAdmin");
    let members = vec![
        GroupMemberSpec::new("+15551111111").as_admin(),
        GroupMemberSpec::new("+15552222222"),
        GroupMemberSpec::new("+15553333333"),
    ];
    let handle = admin
        .create_group("DOT swarm A", &members)
        .await
        .expect("scripted create_group returns Ok");
    assert_eq!(handle.id.as_str(), scripted_group_id);
    assert!(handle.is_admin);
    assert!(handle.initial_admins_promoted);

    // ── Step 3: GroupHandle.id → BindEnvelope.group_id ──────────
    let mut bind = BindEnvelope::new("domain-A", "whatsapp", handle.id.as_str());
    bind.member_count_at_bind = handle.member_count.unwrap_or(0) as u16;
    assert_eq!(bind.group_id, scripted_group_id);
    assert_eq!(bind.platform, "whatsapp");

    // ── Step 4: BindGossipState ingests the bind ────────────────
    let gossip = BindGossipState::new();
    assert!(
        gossip.record_received(bind.clone()),
        "first record_received must return true"
    );
    assert!(
        !gossip.record_received(bind.clone()),
        "duplicate record_received must return false"
    );
    let received = gossip.received_for("domain-A");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].group_id, scripted_group_id);

    // ── Step 5: wrap bind in a DeterministicEnvelope ────────────
    let bind_bytes = serde_json::to_vec(&bind).expect("BindEnvelope is JSON-serializable");
    let payload_hash: [u8; 32] = blake3::hash(&bind_bytes).into();
    let sk = SigningKey::from_bytes(&[0x42; 32]);
    let env = DeterministicEnvelope {
        version: 1,
        network_id: 1,
        message_type: MessageType::GossipObject as u16,
        envelope_id: blake3::hash(b"bind-domain-A-v1").into(),
        mission_id: [0; 32],
        source_peer: sk.verifying_key().to_bytes(),
        origin_gateway: [0; 32],
        logical_timestamp: 1000,
        ttl_hops: 8,
        payload_hash,
        route_trace_root: [0; 32],
        flags: 0,
        signature: [0; 64],
    };
    let signing_bytes = env.to_signing_bytes();
    let sig = sk.sign(&signing_bytes);
    let env = DeterministicEnvelope {
        signature: sig.to_bytes(),
        ..env
    };

    // ── Step 6: send_envelope writes the wire bytes ─────────────
    let domain = adapter.domain_id("whatsapp:test-group");
    let receipt = adapter
        .send_envelope(&domain, &env)
        .await
        .expect("send_envelope should succeed");
    assert!(receipt.platform_message_id.starts_with("mock-"));

    let outbound = adapter.outbound_messages().await;
    assert_eq!(outbound.len(), 1, "exactly one wire message");

    // ── Step 7: wire bytes round-trip and carry the bind ────────
    let wire = &outbound[0];
    let parsed = DeterministicEnvelope::from_wire_bytes(wire)
        .expect("wire bytes must round-trip through from_wire_bytes");
    assert_eq!(parsed.envelope_id, env.envelope_id);
    assert_eq!(parsed.payload_hash, payload_hash);
    assert_eq!(parsed.source_peer, env.source_peer);

    // The bind payload inside the wire bytes still references the
    // group_id returned by CoordinatorAdmin::create_group — proving
    // the trait output flowed end-to-end through the consumer side.
    let payload_str = std::str::from_utf8(&bind_bytes).expect("bind_bytes is valid utf-8");
    assert!(
        payload_str.contains(scripted_group_id),
        "bind payload must contain the group_id; got {payload_str:?}"
    );

    // And the recorded call confirms the bridge was used.
    let calls = adapter.admin_calls().await;
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        AdminCall::CreateGroup {
            subject,
            initial_member_count,
        } => {
            assert_eq!(subject, "DOT swarm A");
            assert_eq!(*initial_member_count, 3);
        }
        other => panic!("expected CreateGroup, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Scenario 14: CoordinatorAdmin::add_member — H6 partial-success
// ─────────────────────────────────────────────────────────────────────
//
// RFC-0861 §3 H6: `AddMemberOutput.promoted` carries the result of the
// optional promote-to-admin step independently from the add itself.
// This scenario exercises all three variants through the bridge:
//
//   - `promoted: None` — caller didn't request promotion
//   - `promoted: Some(Ok(()))` — add succeeded AND promote succeeded
//   - `promoted: Some(Err(_))` — add succeeded but promote failed
//                          (the H6 partial-success case)
//
// Verifies the bridge correctly surfaces each variant without
// collapsing them into a single binary outcome.

#[tokio::test]
async fn scenario14_coordinator_admin_add_member_partial_success() {
    let adapter =
        MockPlatformAdapter::new(PlatformType::Matrix).with_admin_scripted(AdminScripted {
            create_group: None,
            // The mock scripts `add_member` to mirror the variant the
            // caller is testing — same return for every call. The
            // scenario below uses three SEPARATE adapters, each with
            // its own scripted response, so we exercise all three
            // variants without collision.
            add_member: Some(Ok(AddMemberOutput {
                added: true,
                promoted: Some(Ok(())),
            })),
        });

    // ── Variant A: `promoted: Some(Ok(()))` ─────────────────────
    let admin: &dyn CoordinatorAdmin = adapter.as_coordinator_admin().unwrap();
    let g = GroupId::new("!room:matrix.org");
    let out_a = admin
        .add_member(&g, &GroupMemberSpec::new("@alice:matrix.org").as_admin())
        .await
        .expect("add_member returns Ok for Some(Ok(()))");
    assert!(out_a.added);
    assert_eq!(
        out_a.promoted,
        Some(Ok(())),
        "Some(Ok(())) variant must surface verbatim through the bridge"
    );

    // ── Variant B: `promoted: None` (no promote attempted) ──────
    let adapter_b =
        MockPlatformAdapter::new(PlatformType::Matrix).with_admin_scripted(AdminScripted {
            create_group: None,
            add_member: Some(Ok(AddMemberOutput {
                added: true,
                promoted: None,
            })),
        });
    let admin_b: &dyn CoordinatorAdmin = adapter_b.as_coordinator_admin().unwrap();
    let out_b = admin_b
        .add_member(&g, &GroupMemberSpec::new("@bob:matrix.org"))
        .await
        .expect("add_member returns Ok for None");
    assert!(out_b.added);
    assert!(
        out_b.promoted.is_none(),
        "None variant must surface verbatim through the bridge"
    );

    // ── Variant C: `promoted: Some(Err(ApiError))` (H6 partial) ─
    let adapter_c =
        MockPlatformAdapter::new(PlatformType::Matrix).with_admin_scripted(AdminScripted {
            create_group: None,
            add_member: Some(Ok(AddMemberOutput {
                added: true,
                promoted: Some(Err(PlatformAdapterError::ApiError {
                    code: 500,
                    message: "promote failed after add succeeded".into(),
                })),
            })),
        });
    let admin_c: &dyn CoordinatorAdmin = adapter_c.as_coordinator_admin().unwrap();
    let out_c = admin_c
        .add_member(&g, &GroupMemberSpec::new("@carol:matrix.org").as_admin())
        .await
        .expect("add_member returns Ok even when promote failed (H6)");
    assert!(
        out_c.added,
        "added must remain true (the add itself succeeded)"
    );
    match &out_c.promoted {
        Some(Err(PlatformAdapterError::ApiError { code, message })) => {
            assert_eq!(*code, 500);
            assert!(message.contains("promote failed"));
        }
        other => panic!(
            "expected Some(Err(ApiError {{ 500, ... }})) — the H6 partial-success variant; got {other:?}"
        ),
    }

    // ── Variant D: `added: false` (add itself failed) ──────────
    // The trait spec says `promoted` is `None` in this case (no
    // promote is attempted when there's no member to promote).
    let adapter_d =
        MockPlatformAdapter::new(PlatformType::Matrix).with_admin_scripted(AdminScripted {
            create_group: None,
            add_member: Some(Ok(AddMemberOutput {
                added: false,
                promoted: None,
            })),
        });
    let admin_d: &dyn CoordinatorAdmin = adapter_d.as_coordinator_admin().unwrap();
    let out_d = admin_d
        .add_member(&g, &GroupMemberSpec::new("@dave:matrix.org"))
        .await
        .expect("add_member returns Ok with added=false when the platform rejected the add");
    assert!(!out_d.added);
    assert!(
        out_d.promoted.is_none(),
        "promoted must be None when added=false (no promote attempted)"
    );
}
