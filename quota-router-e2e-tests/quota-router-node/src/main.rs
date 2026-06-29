use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use clap::Parser;
use octo_adapter_tcp::TcpAdapter;
use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};
use quota_router::handler::QuotaRouterHandler;
use quota_router::provider::{
    LocalProvider, NetworkId, PeerConfig, PeerTrust, ProviderAuth, ProviderConfig, ProviderError,
    ProviderHealth, ProviderCapacity, RouterNodeId,
};
use quota_router::request::RoutingPolicy;
use quota_router::QuotaRouterNode;
use tokio::sync::RwLock;

/// Raw TCP sender — sends length-prefixed frames directly over TCP.
/// Lives in the binary (not the adapter crate) because it implements
/// `NetworkSender` from `octo-transport`, and adapters only depend on `octo-network`.
struct TcpRawSender {
    peers: Arc<RwLock<BTreeMap<[u8; 32], SocketAddr>>>,
}

#[async_trait]
impl NetworkSender for TcpRawSender {
    async fn send(&self, payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        let len = (payload.len() as u32).to_be_bytes();
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&len);
        frame.extend_from_slice(payload);

        let peers = self.peers.read().await;
        if peers.is_empty() {
            return Err(TransportError::AllTransportsFailed);
        }

        let mut sent = false;
        for (_id, addr) in peers.iter() {
            if let Ok(mut stream) = tokio::net::TcpStream::connect(addr).await {
                if stream.write_all(&frame).await.is_ok() {
                    sent = true;
                }
            }
        }
        if sent { Ok(()) } else { Err(TransportError::AllTransportsFailed) }
    }

    fn name(&self) -> &str { "tcp-raw" }
    fn is_healthy(&self) -> bool { true }
}

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

    // Create TCP adapter and track peer addresses
    let listen_addr: std::net::SocketAddr = args.listen_addr.parse().expect("invalid listen_addr");
    let tcp_adapter = TcpAdapter::new(listen_addr)
        .await
        .expect("failed to create TCP adapter");
    let actual_addr = tcp_adapter.local_addr();

    // Collect peer addresses for the raw sender
    let peer_addrs: BTreeMap<[u8; 32], SocketAddr> = args.peers.iter().filter_map(|p| {
        let addr: std::net::SocketAddr = p.parse().ok()?;
        let hash = blake3::hash(p.as_bytes());
        let mut id = [0u8; 32];
        id.copy_from_slice(hash.as_bytes());
        Some((id, addr))
    }).collect();

    // Connect to peers
    for addr in peer_addrs.values() {
        if let Err(e) = tcp_adapter.connect(*addr).await {
            tracing::warn!("Failed to connect to {}: {}", addr, e);
        }
    }

    // Create raw TCP sender for outbound messages (bypasses DOT envelope wrapping)
    let raw_sender = Arc::new(TcpRawSender {
        peers: Arc::new(RwLock::new(peer_addrs)),
    });

    node.transport = NodeTransport::new(vec![raw_sender]);

    // Create handler
    let transport = Arc::new(NodeTransport::new(vec![])); // handler transport (unused for now)
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
