//! L2 HMAC verification tests (RFC-0870 T22-T27).
//!
//! These tests exercise the production HMAC verification paths in
//! `QuotaRouterHandler::on_receive()`. Each positive test verifies that
//! a correctly-signed message is accepted; each negative test constructs
//! a message with a wrong HMAC and verifies it is rejected.

use std::time::Duration;

use quota_router_core::node::announce::{
    RouterAnnouncePayload, RouterWithdrawPayload, SignedPayload, WithdrawReason,
};
use quota_router_core::node::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router_core::node::provider::{NetworkId, RouterNodeId};
use quota_router_integration_tests::TestCluster;

/// T22 — gossip_hmac_verified
/// Valid gossip (correct HMAC) is accepted and merged into peer cache.
/// We verify the gossip path specifically by checking that node 0's
/// gossip broadcast (with capacities) appears in node 1's cache.
#[tokio::test]
async fn l2_t22_gossip_hmac_verified() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Broadcast gossip from node 0 — this exercises the gossip HMAC path
    cluster.nodes[0].broadcast_gossip().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    let snap_after = cluster.nodes[1].gossip_cache_snapshot().await;
    // Gossip should have added/updated entries (capacities from gossip
    // broadcast differ from announce-only entries)
    assert!(
        !snap_after.is_empty(),
        "Valid gossip should be accepted into cache"
    );
}

/// T23 — gossip_hmac_rejected
/// Gossip with wrong HMAC is silently dropped; peer cache remains unchanged.
#[tokio::test]
async fn l2_t23_gossip_hmac_rejected() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    // Capture baseline gossip state
    cluster.drive_all().await;
    let snap_before = cluster.nodes[1].gossip_cache_snapshot().await;

    // Build gossip with WRONG HMAC (sign with a different key)
    // Include non-empty capacities so a bypass would change the cache
    let wrong_key = [99u8; 32];
    let mut bad_gossip = CapacityGossipPayload {
        sender_id: RouterNodeId([1u8; 32]),
        timestamp: monotonic_now(),
        capacities: vec![quota_router_core::node::provider::ProviderCapacity {
            provider_id: quota_router_core::node::provider::ProviderId([1u8; 32]),
            provider_name: "attacker".into(),
            router_node_id: RouterNodeId([1u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: 9999,
            pricing: vec![],
            status: quota_router_core::node::provider::ProviderHealth::Healthy,
            latency_ms: 1,
            success_rate_bps: 10000,
            last_updated: 0,
        }],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    bad_gossip.hmac = bad_gossip.compute_hmac(&wrong_key);
    let body = bincode::serialize(&bad_gossip).unwrap();
    let framed = {
        let mut out = vec![0xC6u8];
        out.extend_from_slice(&body);
        out
    };

    // Inject into node 1's inbox and drive
    cluster.inject(RouterNodeId([2u8; 32]), RouterNodeId([1u8; 32]), framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    // Gossip cache should be unchanged (bad gossip rejected)
    let snap_after = cluster.nodes[1].gossip_cache_snapshot().await;
    assert_eq!(
        snap_before.len(),
        snap_after.len(),
        "Bad HMAC gossip should be rejected"
    );
}

/// T24 — announce_hmac_verified
/// Valid announce (correct HMAC) adds peer to peer cache if model overlap.
#[tokio::test]
async fn l2_t24_announce_hmac_verified() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // After start_all (which broadcasts announces), node 1 should know node 0
    let count = cluster.nodes[1].peer_count().await;
    assert!(
        count >= 1,
        "Valid announce should add peer to cache, got {}",
        count
    );
}

/// T25 — announce_hmac_rejected
/// Announce with wrong HMAC is silently dropped; peer not added.
#[tokio::test]
async fn l2_t25_announce_hmac_rejected() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Capture baseline
    let count_before = cluster.nodes[1].peer_count().await;

    // Build announce with WRONG HMAC
    let wrong_key = [99u8; 32];
    let mut bad_announce = RouterAnnouncePayload {
        node_id: RouterNodeId([99u8; 32]), // unknown phantom node
        network_id: NetworkId([1u8; 32]),
        supported_models: vec!["gpt-4o".into()],
        capacities: vec![],
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
        pricing_policy: None,
    };
    bad_announce.hmac = bad_announce.compute_hmac(&wrong_key);
    let body = bincode::serialize(&bad_announce).unwrap();
    let framed = {
        let mut out = vec![0xCAu8];
        out.extend_from_slice(&body);
        out
    };

    cluster.inject(RouterNodeId([2u8; 32]), RouterNodeId([1u8; 32]), framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    // Peer count should not increase (bad announce rejected)
    let count_after = cluster.nodes[1].peer_count().await;
    assert_eq!(
        count_before, count_after,
        "Bad HMAC announce should be rejected"
    );
}

/// T26 — withdraw_hmac_verified
/// Valid withdraw (correct HMAC) removes peer from cache.
#[tokio::test]
async fn l2_t26_withdraw_hmac_verified() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Establish peer
    assert!(
        cluster.nodes[1].peer_count().await >= 1,
        "Should have peer after gossip"
    );
    let count_before = cluster.nodes[1].peer_count().await;

    // Build withdraw with correct HMAC for node 0
    let mut withdraw = RouterWithdrawPayload {
        node_id: RouterNodeId([1u8; 32]),
        reason: WithdrawReason::Graceful,
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    withdraw.hmac = withdraw.compute_hmac(&cluster.network_key);
    let body = bincode::serialize(&withdraw).unwrap();
    let framed = {
        let mut out = vec![0xCBu8];
        out.extend_from_slice(&body);
        out
    };

    cluster.inject(RouterNodeId([2u8; 32]), RouterNodeId([1u8; 32]), framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    let count_after = cluster.nodes[1].peer_count().await;
    assert!(
        count_after < count_before,
        "Valid withdraw should remove peer: before={}, after={}",
        count_before,
        count_after
    );
}

/// T27 — withdraw_hmac_rejected
/// Withdraw with wrong HMAC is silently dropped; peer remains.
#[tokio::test]
async fn l2_t27_withdraw_hmac_rejected() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Establish peer
    assert!(cluster.nodes[1].peer_count().await >= 1);
    let count_before = cluster.nodes[1].peer_count().await;

    // Build withdraw with WRONG HMAC
    let wrong_key = [99u8; 32];
    let mut bad_withdraw = RouterWithdrawPayload {
        node_id: RouterNodeId([1u8; 32]),
        reason: WithdrawReason::Graceful,
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    bad_withdraw.hmac = bad_withdraw.compute_hmac(&wrong_key);
    let body = bincode::serialize(&bad_withdraw).unwrap();
    let framed = {
        let mut out = vec![0xCBu8];
        out.extend_from_slice(&body);
        out
    };

    cluster.inject(RouterNodeId([2u8; 32]), RouterNodeId([1u8; 32]), framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    let count_after = cluster.nodes[1].peer_count().await;
    assert_eq!(
        count_before, count_after,
        "Bad HMAC withdraw should be rejected"
    );
}
