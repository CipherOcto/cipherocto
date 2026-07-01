//! L2 inbound HMAC failure — tampered envelope returns Err, no mutation.
//!
//! Verifies that `node.receive()` with a tampered `CapacityGossip`
//! envelope returns a transport error and does NOT mutate the receiver's
//! gossip cache. The handler must verify the HMAC before mutating any
//! state (production code path).
//!
//! Production paths exercised:
//!   - `node.receive(payload, &ctx)` → `transport.dispatch(...)` →
//!     `handler.on_receive(...)` → `handle_capacity_gossip(...)`.
//! The handler returns `TransportError::AdapterFailure("capacity gossip
//! HMAC mismatch")` when the HMAC fails to verify, and that propagates
//! through `dispatch()` (which fails fast on the first receiver error).

use octo_transport::receiver::ReceiveContext;
use quota_router::announce::SignedPayload;
use quota_router::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router::provider::{
    ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router::{envelope, DISC_CAPACITY_GOSSIP};
use quota_router_e2e_tests::TestCluster;

fn make_capacity(provider_name: &str, model: &str, remaining: u64) -> ProviderCapacity {
    ProviderCapacity {
        provider_id: ProviderId([0xA1u8; 32]),
        provider_name: provider_name.to_string(),
        router_node_id: RouterNodeId([0xB1u8; 32]),
        models: vec![model.to_string()],
        requests_remaining: remaining,
        pricing: vec![ModelPricing {
            model: model.to_string(),
            price_per_1k_tokens: 3,
        }],
        status: ProviderHealth::Healthy,
        latency_ms: 200,
        success_rate_bps: 9500,
        last_updated: 0,
    }
}

/// Direct `node.receive()` with a gossip envelope whose HMAC was signed
/// with a WRONG key returns an error and the receiver's gossip cache
/// remains unchanged.
#[tokio::test]
async fn l2_inbound_hmac_failure_wrong_key() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    // Build gossip signed with the wrong key. Include a clearly
    // observable capacity so any cache mutation would be visible.
    let wrong_key = [0x77u8; 32];
    let sender_id = RouterNodeId([0x33u8; 32]);
    let mut bad_gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![make_capacity("attacker-provider", "gpt-4o", 9999)],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    bad_gossip.hmac = bad_gossip.compute_hmac(&wrong_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &bad_gossip).expect("envelope");

    // Capture baseline gossip cache.
    let snap_before = cluster.nodes[0].gossip_cache_snapshot().await;
    assert!(snap_before.is_empty(), "gossip cache starts empty");

    // Send via the production public inbound API.
    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    let result = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(
        result.is_err(),
        "tampered HMAC must return Err, got {:?}",
        result
    );

    // Cache must be unchanged — the handler returns the error BEFORE
    // mutating gossip_cache or peer_cache.
    let snap_after = cluster.nodes[0].gossip_cache_snapshot().await;
    assert_eq!(
        snap_before.len(),
        snap_after.len(),
        "tampered gossip must not mutate cache: before={:?}, after={:?}",
        snap_before,
        snap_after
    );
}

/// Direct `node.receive()` with a gossip envelope whose body bytes
/// are tampered AFTER signing returns an error and does not mutate
/// the gossip cache. This is the "wire-level tampering" case: a valid
/// HMAC over payload P, then the wire bytes are mutated to P'.
#[tokio::test]
async fn l2_inbound_hmac_failure_body_tamper() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    // Build a correctly-signed gossip, then flip the last byte of the
    // framed envelope (still inside the bincode body). This breaks
    // the HMAC match without affecting the discriminator.
    let sender_id = RouterNodeId([0x44u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![make_capacity("honest-provider", "gpt-4o", 100)],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);
    let mut framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).expect("envelope");
    assert!(framed.len() > 1);
    let last = framed.len() - 1;
    framed[last] ^= 0xFF; // tamper the last body byte

    let snap_before = cluster.nodes[0].gossip_cache_snapshot().await;

    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    let result = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(
        result.is_err(),
        "body-tampered envelope must return Err, got {:?}",
        result
    );

    let snap_after = cluster.nodes[0].gossip_cache_snapshot().await;
    assert_eq!(
        snap_before.len(),
        snap_after.len(),
        "body-tampered gossip must not mutate cache"
    );
}

/// Direct `node.receive()` with an all-zero HMAC returns an error and
/// does not mutate the gossip cache. All-zero is the "no signature"
/// sentinel and must not pass verification.
#[tokio::test]
async fn l2_inbound_hmac_failure_zero_hmac() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let sender_id = RouterNodeId([0x55u8; 32]);
    let bad_gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![make_capacity("zero-sig-provider", "gpt-4o", 50)],
        known_peers: vec![],
        hmac: [0u8; 32], // zero HMAC — never valid
    };
    let framed = envelope(DISC_CAPACITY_GOSSIP, &bad_gossip).expect("envelope");

    let snap_before = cluster.nodes[0].gossip_cache_snapshot().await;

    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    let result = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(
        result.is_err(),
        "zero-HMAC envelope must return Err, got {:?}",
        result
    );

    let snap_after = cluster.nodes[0].gossip_cache_snapshot().await;
    assert_eq!(
        snap_before.len(),
        snap_after.len(),
        "zero-HMAC gossip must not mutate cache"
    );
}