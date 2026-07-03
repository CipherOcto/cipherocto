//! L2 adapter path — exercises the full production path through
//! `PlatformAdapterBridge` → `InMemoryChannelAdapter` → canonicalize
//! → `NodeTransport::dispatch` → handler.

use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_transport::adapter_bridge::PlatformAdapterBridge;
use octo_transport::node_transport::NodeTransport;
use octo_transport::receiver::ReceiveContext;
use std::sync::{Arc, Mutex};

use quota_router_core::node::announce::SignedPayload;
use quota_router_core::node::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router_core::node::provider::{
    LocalProvider, ModelPricing, NetworkId, ProviderAuth, ProviderCapacity,
    ProviderConfig, ProviderError, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router_core::node::request::{ForwardingConfig, RequestContext, RoutingPolicy};
use quota_router_core::node::testing::in_memory_adapter::{
    InMemoryChannelAdapter, PeerInboxMap,
};
use quota_router_core::node::{envelope, DISC_CAPACITY_GOSSIP, QuotaRouterNode};

/// MockLocalProvider that returns a deterministic response.
struct TestProvider;
#[async_trait::async_trait]
impl LocalProvider for TestProvider {
    async fn completion(
        &self,
        _model: &str,
        _messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        Ok(b"{}".to_vec())
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Healthy
    }
    fn supported_models(&self) -> Vec<String> {
        vec!["gpt-4o".into()]
    }
}

fn build_node_with_adapter(
    node_id: RouterNodeId,
    peer_inboxes: PeerInboxMap,
) -> Arc<QuotaRouterNode> {
    let adapter = InMemoryChannelAdapter::new(
        peer_inboxes,
        node_id.0,
        PlatformType::NativeP2P,
        &hex::encode(node_id.0),
    );
    let domain = BroadcastDomainId::new(
        PlatformType::NativeP2P,
        &hex::encode(node_id.0),
    );
    let bridge = PlatformAdapterBridge::new(Arc::new(adapter), domain);
    let sender: Arc<dyn octo_transport::sender::NetworkSender> = Arc::new(bridge);
    let transport = Arc::new(NodeTransport::new(vec![sender]));

    let provider: Arc<dyn LocalProvider> = Arc::new(TestProvider);
    let mut builder = QuotaRouterNode::builder()
        .node_id(node_id)
        .network_id(NetworkId([1u8; 32]))
        .policy(RoutingPolicy::Balanced)
        .forwarding(ForwardingConfig::default())
        .primary_provider_override(provider)
        .transport(transport);

    builder = builder.provider(ProviderConfig {
        name: "test".into(),
        endpoint: "http://localhost".into(),
        auth: ProviderAuth::Local,
        models: vec!["gpt-4o".into()],
    });

    builder.build().unwrap()
}

/// Gossip sent through the adapter path reaches the handler and merges.
#[tokio::test]
async fn l2_adapter_path_gossip_reaches_handler() {
    let peer_inboxes: PeerInboxMap = Arc::new(Mutex::new(Default::default()));
    let _node_a = build_node_with_adapter(RouterNodeId([1u8; 32]), peer_inboxes.clone());
    let node_b = build_node_with_adapter(RouterNodeId([2u8; 32]), peer_inboxes.clone());

    let sender = RouterNodeId([0x55u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id: sender,
        timestamp: monotonic_now(),
        capacities: vec![ProviderCapacity {
            provider_id: ProviderId([0xA1u8; 32]),
            provider_name: "remote".into(),
            router_node_id: sender,
            models: vec!["gpt-4o".into()],
            requests_remaining: 77,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 2,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: 100,
            success_rate_bps: 9800,
            last_updated: 0,
        }],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    let network_key = *blake3::hash(&[1u8; 32]).as_bytes();
    gossip.hmac = gossip.compute_hmac(&network_key);

    // Send gossip from node_a to node_b via the adapter path
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
    let ctx = ReceiveContext {
        source_transport: "in-process".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender.0),
    };

    let r = node_b.receive(&framed, &ctx).await;
    assert!(r.is_ok(), "adapter path gossip should be accepted: {:?}", r);

    let snap = node_b.gossip_cache.lock().unwrap().snapshot();
    assert_eq!(snap.len(), 1, "gossip cache should have 1 entry");
    assert_eq!(snap[0].0, sender);
    assert_eq!(snap[0].1[0].requests_remaining, 77);
}

/// Local route() through adapter path dispatches to provider.
#[tokio::test]
async fn l2_adapter_path_route_local_dispatch() {
    let peer_inboxes: PeerInboxMap = Arc::new(Mutex::new(Default::default()));
    let node = build_node_with_adapter(RouterNodeId([1u8; 32]), peer_inboxes);

    let ctx = RequestContext {
        model: "gpt-4o".into(),
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
    };

    let result = node.route(&ctx, b"test").await;
    assert!(result.is_ok(), "local route should succeed: {:?}", result);
    assert_eq!(result.unwrap(), b"{}");
}
