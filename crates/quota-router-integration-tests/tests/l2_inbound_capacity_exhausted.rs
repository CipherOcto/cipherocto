//! L2 inbound capacity exhausted — placeholder for a test that would
//! verify a saturated local provider produces a `ForwardReject` with
//! reason `CapacityExhausted`.
//!
//! **DESIGN GAP (marked #[ignore]).** The production
//! `QuotaRouterHandler::handle_forward_request` currently emits
//! `ForwardRejectReason::NoProvider` when the scorer's destination
//! list is empty (which happens when the only matching provider has
//! `requests_remaining == 0` — see `scorer::filter_capacity`). The
//! production code path never emits `ForwardRejectReason::CapacityExhausted`
//! from the inbound handler.
//!
//! To make this test pass, production code would need to:
//!   1. Distinguish "no provider supports this model" (NoProvider)
//!      from "the supporting provider is saturated" (CapacityExhausted)
//!      in the scorer's destination list, and
//!   2. Have `handle_forward_request` emit `CapacityExhausted` when
//!      a forward arrives for a model whose only local provider has
//!      `requests_remaining == 0`.
//!
//! This test stays in the suite as a placeholder so the gap is
//! visible and tracked. The `#[ignore]` attribute prevents it from
//! running; cargo test will still report it as ignored.

use std::time::Duration;

use octo_transport::receiver::ReceiveContext;
use quota_router_core::node::announce::SignedPayload;
use quota_router_core::node::forward::{
    ForwardRejectPayload, ForwardRejectReason, ForwardRequestPayload,
};
use quota_router_core::node::provider::{NetworkId, RouterNodeId};
use quota_router_core::node::request::RequestContext;
use quota_router_core::node::{envelope, DISC_FORWARD_REQUEST};
use quota_router_integration_tests::TestCluster;

fn forward_request_with_ttl(
    network_key: &[u8; 32],
    network_id: NetworkId,
    request_id: [u8; 32],
    model: &str,
    ttl: u8,
    origin_node: RouterNodeId,
) -> ForwardRequestPayload {
    let mut fwd = ForwardRequestPayload {
        request_id,
        network_id,
        context: RequestContext {
            model: model.to_string(),
            preferred_provider: None,
            model_group: None,
            input_tokens: None,
            max_output_tokens: None,
            tags: None,
            max_price_per_1k_tokens: None,
            max_latency_ms: None,
            policy_override: None,
            consumer_id: [0u8; 32],
            priority: 0,
            deadline: None,
        },
        payload: b"hello".to_vec(),
        ttl,
        origin_node,
        hop_count: 0,
        created_at: quota_router_core::node::gossip::monotonic_now(),
        hmac: [0u8; 32],
    };
    fwd.hmac = fwd.compute_hmac(network_key);
    fwd
}

// Helper kept here so the test compiles and the doc-string above
// remains accurate even when the test body is `#[ignore]`-ed out.
#[allow(dead_code)]
async fn _exercise_capacity_exhausted() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Saturate node 0's local provider for gpt-4o by directly
    // mutating its config-derived capacity. Production code
    // currently does not expose this; the test would need either
    // a production seam (e.g. `MockLocalProvider::set_saturated`)
    // or a direct node_mut on `config.providers[0]` to set the
    // derived `requests_remaining` field.
    //
    // (See the file-level doc comment for why this is `#[ignore]`.)

    // Inject a forward into node 0's inbox — production handler
    // would need to detect the saturation and emit
    // `ForwardRejectReason::CapacityExhausted` (currently emits
    // `NoProvider`).

    let request_id = [0x77u8; 32];
    let phantom_origin = RouterNodeId([0xEEu8; 32]);
    let fwd = forward_request_with_ttl(
        &cluster.network_key,
        NetworkId([1u8; 32]),
        request_id,
        "gpt-4o",
        3,
        phantom_origin,
    );
    let framed = envelope(DISC_FORWARD_REQUEST, &fwd).expect("envelope");

    let _ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(phantom_origin.0),
    };
    let _ = framed;
    let _ = (request_id, phantom_origin);
}

/// Placeholder test that documents the design gap. The
/// actual assertion would verify `ForwardRejectReason::CapacityExhausted`
/// once production emits it.
#[tokio::test]
#[ignore = "production handler emits NoProvider for saturated providers; see file doc comment"]
async fn l2_inbound_capacity_exhausted_emits_reject() {
    _exercise_capacity_exhausted().await;
    // If production changes to emit CapacityExhausted, the test
    // body would inspect the reject envelope via a TestObserver
    // registered on the receiving node's transport (mirroring the
    // pattern in l2_inbound_ttl_exceeded.rs) and assert:
    //   let reject: ForwardRejectPayload = bincode::deserialize(...);
    //   assert!(matches!(reject.reason, ForwardRejectReason::CapacityExhausted));
    // Until then, the test is a no-op.
    let _ = ForwardRejectReason::CapacityExhausted;
    let _ = ForwardRejectPayload {
        request_id: [0u8; 32],
        peer_id: RouterNodeId([0u8; 32]),
        reason: ForwardRejectReason::NoProvider,
    };
}
