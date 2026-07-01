//! L2 inbound multi-receiver — verifies that additional receivers
//! registered via `node.transport.register_receiver(...)` after the
//! builder runs receive payloads alongside the built-in handler.
//!
//! The production `NodeTransport::dispatch()` iterates all registered
//! receivers in registration order. The builder registers the
//! internal `QuotaRouterHandler` first; a subsequent call to
//! `register_receiver(observer)` appends the observer. Both should
//! fire on every inbound payload.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::TransportError;
use quota_router::announce::SignedPayload;
use quota_router::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router::provider::{
    ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router::{envelope, DISC_CAPACITY_GOSSIP};
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
        "test-observer-multi-receiver"
    }
}

fn make_capacity(name: &str, model: &str, remaining: u64) -> ProviderCapacity {
    ProviderCapacity {
        provider_id: ProviderId([0xA1u8; 32]),
        provider_name: name.to_string(),
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

/// Registering an additional receiver on the node's transport causes
/// that receiver to receive payloads alongside the production handler.
#[tokio::test]
async fn l2_inbound_multi_receiver_observer_fires() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    // Sanity: the handler should already be registered by the builder.
    // (This is verified by the in-tree unit test
    // `node_has_internal_handler_after_build`; we don't re-check it
    // here.)

    // Register the observer after the builder ran.
    let observer = TestObserver::new();
    cluster
        .nodes[0]
        .node
        .transport
        .register_receiver(observer.clone());
    assert_eq!(
        observer.call_count.load(Ordering::SeqCst),
        0,
        "observer should not have fired before any payload"
    );

    // Send a valid gossip envelope via the production public inbound
    // API. This routes through transport.dispatch → both the built-in
    // handler AND the observer should receive the payload.
    let sender_id = RouterNodeId([0x99u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![make_capacity("multi-rx-provider", "gpt-4o", 7)],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).expect("envelope");

    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    let result = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(result.is_ok(), "node.receive should Ok: {:?}", result);

    // Both the built-in handler AND the observer must have received
    // the payload. The handler's effect is observable via the gossip
    // cache mutation; the observer's effect is observable via its
    // call_count.
    assert!(
        observer.call_count.load(Ordering::SeqCst) >= 1,
        "observer should fire on dispatch, got {}",
        observer.call_count.load(Ordering::SeqCst)
    );
    let captured = observer.captured.lock().unwrap().clone();
    assert_eq!(captured.len(), 1, "observer should have 1 captured payload");
    assert_eq!(captured[0], framed, "captured bytes should match the original envelope");

    // The built-in handler must have merged the gossiped capacity.
    let snap = cluster.nodes[0].gossip_cache_snapshot().await;
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].0, sender_id);
    assert_eq!(snap[0].1[0].provider_name, "multi-rx-provider");
}

/// Multiple additional receivers all fire on every payload.
#[tokio::test]
async fn l2_inbound_multi_receiver_two_observers() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let obs_a = TestObserver::new();
    let obs_b = TestObserver::new();
    cluster.nodes[0].node.transport.register_receiver(obs_a.clone());
    cluster.nodes[0].node.transport.register_receiver(obs_b.clone());

    let sender_id = RouterNodeId([0x88u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![make_capacity("two-rx", "gpt-4o", 5)],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).expect("envelope");
    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    cluster.nodes[0].node.receive(&framed, &ctx).await.unwrap();

    assert!(obs_a.call_count.load(Ordering::SeqCst) >= 1, "obs_a should fire");
    assert!(obs_b.call_count.load(Ordering::SeqCst) >= 1, "obs_b should fire");
}