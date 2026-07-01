//! L2 inbound TTL exceeded — direct `node.receive()` with ttl=0 triggers
//! a `ForwardReject` with reason `TtlExpired`.
//!
//! Verifies the production inbound reject path:
//!   - `node.receive(...)` → `transport.dispatch(...)` →
//!     `handler.on_receive(...)` → `handle_forward_request(...)`.
//!   - When the inbound `ForwardRequestPayload::ttl == 0`, the handler
//!     calls `send_forward_reject(request_id, TtlExpired)` which
//!     produces a `ForwardRejectPayload` envelope (disc `0xC5`) and
//!     emits it via the transport's `send_best`.
//!
//! We observe the wire-level reject envelope by registering a
//! `TestObserver` on the RECEIVER node's transport. The receiver's
//! transport runs `dispatch()` when its driver drains its inbox;
//! the observer captures every payload dispatched.
//!
//! Topology: 2 nodes. Forward with ttl=0 is injected into node 0's
//! inbox. Node 0's handler sees ttl=0, emits reject via send_best
//! → broadcast → lands in node 1's inbox. The observer on node 1's
//! transport captures the reject when node 1's driver drains it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::TransportError;
use quota_router::announce::SignedPayload;
use quota_router::forward::{ForwardRejectPayload, ForwardRejectReason, ForwardRequestPayload};
use quota_router::provider::{NetworkId, RouterNodeId};
use quota_router::request::RequestContext;
use quota_router::{envelope, DISC_FORWARD_REQUEST};
use quota_router_e2e_tests::TestCluster;

pub struct TestObserver {
    pub captured: Mutex<Vec<Vec<u8>>>,
    pub call_count: AtomicUsize,
}

impl TestObserver {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            captured: Mutex::new(Vec::new()),
            call_count: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl NetworkReceiver for TestObserver {
    async fn on_receive(
        &self,
        payload: &[u8],
        _ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        self.captured.lock().unwrap().push(payload.to_vec());
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn name(&self) -> &str {
        "test-observer"
    }
}

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
        created_at: quota_router::gossip::monotonic_now(),
        hmac: [0u8; 32],
    };
    fwd.hmac = fwd.compute_hmac(network_key);
    fwd
}

/// Direct `node.receive()` (via the harness inbox) with a
/// `ForwardRequest` payload whose `ttl == 0` causes the production
/// handler to emit a `ForwardReject` envelope with reason `TtlExpired`.
/// The wire-level reject envelope is captured by a `TestObserver`
/// registered on the RECEIVER's transport.
#[tokio::test]
async fn l2_inbound_ttl_exceeded_emits_ttl_reject() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);

    // Register an observer on node 1 (the receiver of the reject).
    // The production handler at node 0 emits the reject via send_best
    // which broadcasts through the InProcessSender → node 1's inbox.
    // When node 1's driver drains the inbox and dispatches via
    // transport.dispatch, the observer captures the reject envelope.
    let observer = TestObserver::new();
    cluster
        .nodes[1]
        .node
        .transport
        .register_receiver(observer.clone());

    let request_id = [0x55u8; 32];
    let phantom_origin = RouterNodeId([0xEEu8; 32]);
    let fwd = forward_request_with_ttl(
        &cluster.network_key,
        NetworkId([1u8; 32]),
        request_id,
        "gpt-4o",
        0, // ttl=0 — handler must reject
        phantom_origin,
    );
    let framed = envelope(DISC_FORWARD_REQUEST, &fwd).expect("envelope");

    // Inject the forward into node 0's inbox. The harness's
    // background driver drains it and calls `node.receive()` →
    // handler.handle_forward_request → ttl=0 → send_forward_reject.
    cluster.inject(RouterNodeId([1u8; 32]), phantom_origin, framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    // The observer on node 1 captured the reject envelope when
    // node 1's driver dispatched it.
    let captured = observer.captured.lock().unwrap().clone();
    let reject_env = captured
        .iter()
        .find(|e| !e.is_empty() && e[0] == quota_router::DISC_FORWARD_REJECT)
        .unwrap_or_else(|| {
            panic!(
                "forward-reject envelope should be in captured (observer count={}, len={})",
                observer.call_count.load(Ordering::SeqCst),
                captured.len()
            )
        });
    let reject: ForwardRejectPayload =
        bincode::deserialize(&reject_env[1..]).expect("deserialize reject body");
    assert_eq!(reject.request_id, request_id);
    assert!(
        matches!(reject.reason, ForwardRejectReason::TtlExpired),
        "expected TtlExpired reason, got {:?}",
        reject.reason
    );
}

/// Integration: with `max_ttl=0` on node 0's forwarding config, a real
/// `route()` from node 0 sends a forward with `ttl=0` to node 1. The
/// handler at node 1 emits a `TtlExpired` reject which is picked up by
/// node 0's `handle_forward_reject`, resolving the pending oneshot.
/// `route()` then returns `ForwardRejected`. (Note: `route()` maps
/// `ForwardOutcome::Rejected(_)` to `RouterNodeError::ForwardRejected(NoProvider)`
/// regardless of the underlying reason — the wire-level reason
/// verification is covered by the test above.)
#[tokio::test]
async fn l2_inbound_ttl_exceeded_via_route() {
    let mut cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Seed node 0's gossip cache with node 1's gpt-4o capability.
    {
        let node = &*cluster.nodes[0].node;
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([2u8; 32]),
            vec![quota_router::provider::ProviderCapacity {
                provider_id: quota_router::provider::ProviderId([2u8; 32]),
                provider_name: "peer1".into(),
                router_node_id: RouterNodeId([2u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![quota_router::provider::ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 5,
                }],
                status: quota_router::provider::ProviderHealth::Healthy,
                latency_ms: 200,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
    }

    // Set TTL=0 on node 0 — the forward envelope it sends will have
    // ttl=0, which the receiving handler rejects with TtlExpired.
    cluster.node_mut(0).await.config.forwarding.max_ttl = 0;
    cluster.node_mut(0).await.config.forwarding.forward_timeout = Duration::from_millis(200);

    let ctx = quota_router_e2e_tests::make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(
        matches!(
            result,
            Err(quota_router::RouterNodeError::ForwardRejected(_))
        ),
        "expected ForwardRejected from ttl=0 chain, got {:?}",
        result
    );
}