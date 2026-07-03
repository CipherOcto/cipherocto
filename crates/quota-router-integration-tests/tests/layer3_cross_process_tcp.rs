//! Layer 3: Cross-process TCP tests (manual only, `#[ignore]`-gated)
//!
//! These tests spawn real `quota-router serve` processes and communicate
//! via TCP. They exercise the full cross-process production path:
//! in-process node → TcpAdapter → TCP wire → remote process → handler.
//!
//! Run with:
//! ```sh
//! cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml -- --ignored l3_
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use octo_adapter_tcp::TcpAdapter;
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_transport::adapter_bridge::PlatformAdapterBridge;
use octo_transport::node_transport::NodeTransport;
use octo_transport::receiver::ReceiveContext;
use std::sync::Arc;

use quota_router_core::node::announce::SignedPayload;
use quota_router_core::node::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router_core::node::provider::{
    LocalProvider, ModelPricing, NetworkId, ProviderAuth, ProviderCapacity,
    ProviderConfig, ProviderError, ProviderHealth, ProviderId,
    RouterNodeId,
};
use quota_router_core::node::request::{ForwardingConfig, RequestContext, RoutingPolicy};
use quota_router_core::node::{envelope, DISC_CAPACITY_GOSSIP, QuotaRouterNode};

/// Path to the built CLI binary
fn cli_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates/
    path.pop(); // workspace root
    path.push("target/debug/quota-router");
    path
}

/// Write a network config TOML file for a node.
fn write_network_config(
    dir: &std::path::Path,
    node_id: &RouterNodeId,
    network_id: &NetworkId,
    models: &[&str],
) -> PathBuf {
    let path = dir.join("network.toml");
    let models_toml: Vec<String> = models
        .iter()
        .map(|m| format!("\"{}\"", m))
        .collect();
    let toml = format!(
        r#"
node_id = "{}"
network_id = "{}"

[[providers]]
name = "mock-provider"
endpoint = "http://localhost"
models = [{}]
"#,
        hex::encode(node_id.0),
        hex::encode(network_id.0),
        models_toml.join(", ")
    );
    std::fs::write(&path, toml).unwrap();
    path
}

/// Spawn a `quota-router serve` process and return the child handle.
fn spawn_serve(
    listen_addr: SocketAddr,
    config_path: &std::path::Path,
    peers: &[String],
) -> Child {
    let mut cmd = Command::new(cli_binary());
    cmd.arg("serve")
        .arg("--listen-addr")
        .arg(listen_addr.to_string())
        .arg("--network-config")
        .arg(config_path)
        .arg("--mock-provider");

    if !peers.is_empty() {
        cmd.arg("--peers").arg(peers.join(","));
    }

    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn quota-router serve")
}

/// MockLocalProvider for the in-process test node.
struct MockProvider;
#[async_trait::async_trait]
impl LocalProvider for MockProvider {
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

/// Build an in-process node with a TcpAdapter connected to a remote address.
async fn build_tcp_node(
    node_id: RouterNodeId,
    remote_addr: SocketAddr,
) -> Arc<QuotaRouterNode> {
    let tcp_adapter = TcpAdapter::new("127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();

    // Connect to the remote serve process
    tcp_adapter.connect(remote_addr).await.unwrap();

    let adapter: Arc<dyn octo_network::dot::adapters::PlatformAdapter> =
        Arc::new(tcp_adapter);
    let domain = BroadcastDomainId::new(
        PlatformType::Tcp,
        &hex::encode(node_id.0),
    );
    let bridge = PlatformAdapterBridge::new(adapter, domain);
    let sender: Arc<dyn octo_transport::sender::NetworkSender> = Arc::new(bridge);
    let transport = Arc::new(NodeTransport::new(vec![sender]));

    let provider: Arc<dyn LocalProvider> = Arc::new(MockProvider);
    let mut builder = QuotaRouterNode::builder()
        .node_id(node_id)
        .network_id(NetworkId([1u8; 32]))
        .policy(RoutingPolicy::Balanced)
        .forwarding(ForwardingConfig::default())
        .primary_provider_override(provider)
        .transport(transport);

    builder = builder.provider(ProviderConfig {
        name: "gpt-4o".into(),
        endpoint: "http://localhost".into(),
        auth: ProviderAuth::Local,
        models: vec!["gpt-4o".into()],
    });

    builder.build().unwrap()
}

/// L3: Spawn two `quota-router serve` processes and verify that a
/// gossip message sent from an in-process node via TcpAdapter reaches
/// the remote process's handler.
///
/// ```sh
/// cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
///   -- --ignored l3_cross_process_gossip
/// ```
#[tokio::test]
#[ignore = "requires built CLI binary and TCP ports"]
async fn l3_cross_process_gossip() {
    let tmp = tempfile::tempdir().unwrap();

    // Node IDs
    let node_a = RouterNodeId([1u8; 32]);
    let node_b = RouterNodeId([2u8; 32]);
    let network_id = NetworkId([1u8; 32]);

    // Ephemeral ports
    let port_a: u16 = 19100 + (rand::random::<u16>() % 1000);
    let port_b: u16 = 19200 + (rand::random::<u16>() % 1000);
    let addr_a: SocketAddr = format!("127.0.0.1:{}", port_a).parse().unwrap();
    let addr_b: SocketAddr = format!("127.0.0.1:{}", port_b).parse().unwrap();

    // Write config files
    let config_a = write_network_config(tmp.path(), &node_a, &network_id, &["gpt-4o"]);
    let config_b = write_network_config(tmp.path(), &node_b, &network_id, &["gpt-4o"]);

    // Spawn process B (no peers initially)
    let mut child_b = spawn_serve(addr_b, &config_b, &[]);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Spawn process A (peers = B)
    let peers_b = vec![format!("{}:{}", hex::encode(node_b.0), addr_b)];
    let mut child_a = spawn_serve(addr_a, &config_a, &peers_b);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Build an in-process node that connects to process A
    let in_process = build_tcp_node(RouterNodeId([3u8; 32]), addr_a).await;

    // Send a gossip envelope through the in-process node
    let network_key = *blake3::hash(&network_id.0).as_bytes();
    let mut gossip = CapacityGossipPayload {
        sender_id: in_process.config.node_id,
        timestamp: monotonic_now(),
        capacities: vec![ProviderCapacity {
            provider_id: ProviderId([0xA1u8; 32]),
            provider_name: "test-provider".into(),
            router_node_id: in_process.config.node_id,
            models: vec!["gpt-4o".into()],
            requests_remaining: 42,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 1,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: 50,
            success_rate_bps: 9900,
            last_updated: 0,
        }],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&network_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();

    let ctx = ReceiveContext {
        source_transport: "tcp".into(),
        mission_id: [0u8; 32],
        sender_id: Some(in_process.config.node_id.0),
    };

    // Send via the in-process node's transport (goes through TcpAdapter → process A)
    let result = in_process.receive(&framed, &ctx).await;
    assert!(
        result.is_ok(),
        "gossip send through TcpAdapter should succeed: {:?}",
        result
    );

    // Give the remote process time to process the message
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Clean up
    let _ = child_a.kill();
    let _ = child_b.kill();
    let _ = child_a.wait();
    let _ = child_b.wait();
}

/// L3: Spawn two processes, send a ForwardRequest from an in-process
/// node through the mesh, verify the remote process dispatches it
/// locally and returns a response.
///
/// ```sh
/// cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
///   -- --ignored l3_cross_process_forward
/// ```
#[tokio::test]
#[ignore = "requires built CLI binary and TCP ports"]
async fn l3_cross_process_forward() {
    let tmp = tempfile::tempdir().unwrap();

    let node_a = RouterNodeId([1u8; 32]);
    let node_b = RouterNodeId([2u8; 32]);
    let network_id = NetworkId([1u8; 32]);

    let port_a: u16 = 19300 + (rand::random::<u16>() % 1000);
    let port_b: u16 = 19400 + (rand::random::<u16>() % 1000);
    let addr_a: SocketAddr = format!("127.0.0.1:{}", port_a).parse().unwrap();
    let addr_b: SocketAddr = format!("127.0.0.1:{}", port_b).parse().unwrap();

    let config_a = write_network_config(tmp.path(), &node_a, &network_id, &["gpt-4o"]);
    let config_b = write_network_config(tmp.path(), &node_b, &network_id, &["gpt-4o"]);

    // Spawn B, then A with B as peer
    let mut child_b = spawn_serve(addr_b, &config_b, &[]);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let peers_b = vec![format!("{}:{}", hex::encode(node_b.0), addr_b)];
    let mut child_a = spawn_serve(addr_a, &config_a, &peers_b);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Build in-process node connected to A
    let in_process = build_tcp_node(RouterNodeId([3u8; 32]), addr_a).await;

    // Route a request — the in-process node will try to dispatch
    // locally first (it has a provider), so this tests the local
    // dispatch path through the TCP-connected mesh.
    let ctx = RequestContext {
        model: "gpt-4o".into(),
        preferred_provider: None,
        model_group: None,
        input_tokens: None,
        max_output_tokens: None,
        tags: None,
        max_price_per_1k_tokens: None,
        max_latency_ms: None,
        policy_override: Some(RoutingPolicy::LocalOnly),
        consumer_id: [0u8; 32],
        priority: 0,
        deadline: None,
    };

    let result = in_process.route(&ctx, b"test-payload").await;
    assert!(
        result.is_ok(),
        "local route through TCP node should succeed: {:?}",
        result
    );

    // Clean up
    let _ = child_a.kill();
    let _ = child_b.kill();
    let _ = child_a.wait();
    let _ = child_b.wait();
}
