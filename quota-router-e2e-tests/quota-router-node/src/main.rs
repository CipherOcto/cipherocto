use clap::Parser;
use quota_router::provider::{
    NetworkId, PeerConfig, PeerTrust, ProviderAuth, ProviderConfig, RouterNodeId,
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

    for model in &args.providers {
        builder = builder.provider(ProviderConfig {
            name: model.clone(),
            endpoint: format!("http://localhost"),
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

    let node = builder.build().expect("failed to build node");
    tracing::info!("Node {:?} started on {}", node_id, args.listen_addr);

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    tokio::select! {
        _ = shutdown => {
            tracing::info!("Shutting down node {:?}", node_id);
        }
    }
}
