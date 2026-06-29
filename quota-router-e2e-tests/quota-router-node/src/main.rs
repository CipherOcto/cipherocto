use std::sync::Arc;

use async_trait::async_trait;
use clap::Parser;
use octo_adapter_tcp::TcpAdapter;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::BroadcastDomainId;
use octo_network::dot::PlatformType;
use octo_transport::adapter_bridge::PlatformAdapterBridge;
use octo_transport::node_transport::NodeTransport;
use quota_router::handler::QuotaRouterHandler;
use quota_router::provider::{
    LocalProvider, NetworkId, PeerConfig, PeerTrust, ProviderAuth, ProviderCapacity,
    ProviderConfig, ProviderError, ProviderHealth, RouterNodeId,
};
use quota_router::request::RoutingPolicy;
use quota_router::QuotaRouterNode;

#[derive(Parser)]
#[command(name = "quota-router-node")]
struct CliArgs {
    #[arg(long)]
    node_id: String,
    #[arg(long)]
    listen_addr: String,
    #[arg(long, value_delimiter = ',')]
    peers: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    providers: Vec<String>,
    #[arg(long)]
    network_key: String,
    #[arg(long, default_value = "10000")]
    gossip_interval: u64,
}

fn decode_hex(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).expect("invalid hex");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes[..32]);
    arr
}

struct LocalMockProvider {
    models: Vec<String>,
}

#[async_trait]
impl LocalProvider for LocalMockProvider {
    async fn completion(
        &self,
        model: &str,
        _messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        Ok(format!("response-{}", model).into_bytes())
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Healthy
    }
    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = CliArgs::parse();

    let node_id = RouterNodeId(decode_hex(&args.node_id));
    let network_key = decode_hex(&args.network_key);

    let mut builder = QuotaRouterNode::builder()
        .node_id(node_id)
        .network_id(NetworkId([1u8; 32]))
        .policy(RoutingPolicy::Balanced)
        .gossip_interval(std::time::Duration::from_millis(args.gossip_interval));

    let models: Vec<String> = args.providers.iter().cloned().collect();
    for model in &args.providers {
        builder = builder.provider(ProviderConfig {
            name: model.clone(),
            endpoint: "http://localhost".into(),
            auth: ProviderAuth::Local,
            models: vec![model.clone()],
        });
    }

    for peer_addr in &args.peers {
        if let Ok(addr) = peer_addr.parse::<std::net::SocketAddr>() {
            let hash = blake3::hash(peer_addr.as_bytes());
            let mut peer_id = [0u8; 32];
            peer_id.copy_from_slice(hash.as_bytes());
            builder = builder.peer(PeerConfig {
                node_id: RouterNodeId(peer_id),
                endpoint: addr,
                trust_level: PeerTrust::Trusted,
            });
        }
    }

    let mut node = builder.build().expect("failed to build node");

    // Create TCP adapter
    let listen_addr: std::net::SocketAddr = args.listen_addr.parse().expect("invalid listen_addr");
    let tcp_adapter = TcpAdapter::new(listen_addr)
        .await
        .expect("failed to create TCP adapter");
    let actual_addr = tcp_adapter.local_addr();

    // Connect to peers
    for peer_addr in &args.peers {
        if let Ok(addr) = peer_addr.parse::<std::net::SocketAddr>() {
            if let Err(e) = tcp_adapter.connect(addr).await {
                tracing::warn!("Failed to connect to {}: {}", addr, e);
            }
        }
    }

    // Create PlatformAdapterBridge wrapping TcpAdapter
    let domain = BroadcastDomainId::new(PlatformType::Tcp, &actual_addr.to_string());
    let adapter: Arc<dyn PlatformAdapter> = Arc::new(tcp_adapter);

    // Create two NodeTransport instances from the same adapter (NodeTransport is not Clone)
    let bridge1 = Arc::new(PlatformAdapterBridge::new(adapter.clone(), domain.clone()));
    let bridge2 = Arc::new(PlatformAdapterBridge::new(adapter, domain));

    node.transport = NodeTransport::new(vec![bridge1]);

    // Create handler
    let transport = Arc::new(NodeTransport::new(vec![bridge2]));
    let primary_provider: Arc<dyn LocalProvider> = Arc::new(LocalMockProvider {
        models: models.clone(),
    });
    let handler = Arc::new(QuotaRouterHandler::new(
        Arc::new(std::sync::Mutex::new(node)),
        primary_provider,
        network_key,
        transport,
    ));

    tracing::info!("Node {:?} listening on {}", node_id, actual_addr);

    // Main loop: receive messages and dispatch to handler
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("Shutting down node {:?}", node_id);
                break;
            }
            _ = async {
                // Poll for incoming messages from TCP adapter
                // and dispatch to handler.on_receive()
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            } => {}
        }
    }
}
