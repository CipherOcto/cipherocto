use crate::balance::Balance;
use crate::config::Config;
use crate::providers::{default_endpoint, Provider};
use crate::proxy::ProxyServer;
use anyhow::Result;
use octo_determin::Dfp;
use octo_reputation::reputation_score_0_100;
use octo_reputation::store::{InMemoryReputationStore, ReputationStore};
use octo_reputation::types::{ReputationLayer, SignalKind};
use quota_router_core::admin::AdminServer;
use quota_router_core::marketplace::reputation_compat::parse_canonical_did;
use quota_router_core::node::provider::{
    MockLocalProvider, PeerConfig, PeerTrust, ProviderAuth, ProviderConfig, RouterNodeId,
};
use quota_router_core::node::{QuotaRouterNode, RouterNodeLifecycle};
use quota_router_core::{init_database, StoolapKeyStorage};
use std::collections::HashMap;
use std::io::Read as _;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// Read envelope JSON from stdin (`-`) or a file path.
fn read_envelope(from: &str) -> Result<String> {
    if from == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        Ok(std::fs::read_to_string(from)?)
    }
}

pub async fn init() -> Result<()> {
    let config = Config::load()?;
    config.save()?;
    info!("Initialized quota-router config");
    println!("Initialized quota-router config");
    Ok(())
}

pub async fn add_provider(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let endpoint = default_endpoint(name).unwrap_or_else(|| "https://api.example.com".to_string());
    config.providers.push(Provider::new(name, &endpoint));
    config.save()?;
    info!("Added provider: {}", name);
    println!("Added provider: {}", name);
    Ok(())
}

pub async fn balance() -> Result<()> {
    let config = Config::load()?;
    println!("OCTO-W Balance: {}", config.balance);
    Ok(())
}

pub async fn list(prompts: u64, price: u64) -> Result<()> {
    info!("Listed {} prompts at {} OCTO-W each", prompts, price);
    println!("Listed {} prompts at {} OCTO-W each", prompts, price);
    Ok(())
}

pub async fn proxy(proxy_port: u16, admin_port: u16) -> Result<()> {
    let config = Config::load()?;

    // Ensure db_path parent directory exists
    if let Some(parent) = config.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Open database and initialize schema
    let db = stoolap::Database::open(&format!("file://{}", config.db_path.display()))?;
    init_database(&db)?;

    // Create storage and admin server
    let storage = StoolapKeyStorage::new(db);
    let mut admin_server = AdminServer::new(storage, admin_port);

    // Get provider for proxy
    let provider = config
        .providers
        .first()
        .cloned()
        .unwrap_or_else(|| Provider::new("openai", "https://api.openai.com/v1"));
    let balance = Balance::new(config.balance);
    // Dispatch map is empty when using simple CLI config (no GatewayConfig).
    // GatewayConfig integration (to_provider_map) comes in a later mission.
    let dispatch_map = HashMap::new();
    let mut proxy_server = ProxyServer::new(balance, provider, proxy_port, dispatch_map);

    // Run both servers
    tokio::spawn(async move {
        if let Err(e) = admin_server.run().await {
            eprintln!("Admin server error: {}", e);
        }
    });

    info!("Starting proxy server on port {}", proxy_port);
    info!("Starting admin API server on port {}", admin_port);

    proxy_server
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("Proxy error: {}", e))?;
    Ok(())
}

pub async fn route(provider: &str, prompt: &str) -> Result<()> {
    info!("Routing test request to {}: {}", provider, prompt);
    println!("Routed to {}: {}", provider, prompt);
    Ok(())
}

/// Network config file format (TOML).
#[derive(serde::Deserialize)]
struct NetworkConfig {
    node_id: String,
    network_id: String,
    #[serde(default)]
    providers: Vec<TomlProviderConfig>,
}

#[derive(serde::Deserialize)]
struct TomlProviderConfig {
    name: String,
    #[serde(default = "default_endpoint_for_toml")]
    endpoint: String,
    #[serde(default)]
    models: Vec<String>,
}

fn default_endpoint_for_toml() -> String {
    "https://api.openai.com".into()
}

pub async fn serve(
    listen_addr: SocketAddr,
    network_config: &Path,
    mock_provider: bool,
    peers: &[String],
) -> Result<()> {
    let toml_str = std::fs::read_to_string(network_config)
        .map_err(|e| anyhow::anyhow!("Failed to read network config: {e}"))?;
    let net_cfg: NetworkConfig = toml::from_str(&toml_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse network config: {e}"))?;

    let node_id = parse_node_id(&net_cfg.node_id)?;
    let network_id = parse_network_id(&net_cfg.network_id)?;

    // Build provider configs
    let provider_configs: Vec<ProviderConfig> = net_cfg
        .providers
        .iter()
        .map(|p| ProviderConfig {
            name: p.name.clone(),
            endpoint: p.endpoint.clone(),
            auth: ProviderAuth::Local,
            models: if p.models.is_empty() {
                vec!["gpt-4o".into()]
            } else {
                p.models.clone()
            },
        })
        .collect();

    if provider_configs.is_empty() {
        return Err(anyhow::anyhow!(
            "At least one provider must be configured in network config"
        ));
    }

    // Parse peer configs from CLI args
    let peer_configs: Vec<PeerConfig> = peers.iter().filter_map(|p| parse_peer_arg(p)).collect();

    // Build the node
    let mut builder = QuotaRouterNode::builder()
        .node_id(node_id)
        .network_id(network_id)
        .policy(quota_router_core::node::request::RoutingPolicy::Balanced);

    for pc in &provider_configs {
        builder = builder.provider(pc.clone());
    }
    for pc in &peer_configs {
        builder = builder.peer(pc.clone());
    }

    // If mock-provider, inject a MockLocalProvider
    if mock_provider {
        let models: Vec<String> = provider_configs
            .iter()
            .flat_map(|p| p.models.iter().cloned())
            .collect();
        let mock = Arc::new(MockLocalProvider::new(models));
        builder = builder.primary_provider_override(mock);
    }

    let node = builder.build()?;

    // Transition to Active if we have peers
    if node.peer_count() > 0 {
        node.set_lifecycle(RouterNodeLifecycle::Active);
    }

    info!(
        "QuotaRouterNode started: node_id={} listen={} peers={} mock_provider={}",
        hex::encode(node_id.0),
        listen_addr,
        node.peer_count(),
        mock_provider,
    );

    // Build TcpAdapter for mesh transport
    let tcp_adapter = octo_adapter_tcp::TcpAdapter::new(listen_addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind TcpAdapter: {e}"))?;

    info!("TcpAdapter listening on {}", tcp_adapter.local_addr());

    // Connect to configured peers
    for pc in &peer_configs {
        if let Err(e) = tcp_adapter.connect(pc.endpoint).await {
            tracing::warn!("Failed to connect to peer {}: {}", pc.endpoint, e);
        }
    }

    // Wrap adapter in PlatformAdapterBridge for NodeTransport
    let adapter: Arc<dyn octo_network::dot::adapters::PlatformAdapter> = Arc::new(tcp_adapter);
    let domain = octo_network::dot::BroadcastDomainId::new(
        octo_network::dot::domain::PlatformType::Tcp,
        &hex::encode(node_id.0),
    );
    let bridge =
        octo_transport::adapter_bridge::PlatformAdapterBridge::new(adapter.clone(), domain);
    let sender: Arc<dyn octo_transport::sender::NetworkSender> = Arc::new(bridge);

    // Create a new transport with the TCP sender
    let mesh_transport = Arc::new(octo_transport::node_transport::NodeTransport::new(vec![
        sender,
    ]));
    // Re-register the handler on the new transport
    node.reattach_internal_handler();

    // Spawn the PlatformAdapterPoller for inbound messages
    let poller =
        octo_transport::adapter_poller::PlatformAdapterPoller::new(adapter, domain, mesh_transport);
    tokio::spawn(async move { poller.run().await });

    // Start gossip loop
    let gossip_node = Arc::clone(&node);
    let gossip_interval = node.config.gossip_interval;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(gossip_interval).await;
            if let Err(e) = gossip_node.broadcast_gossip().await {
                tracing::warn!("Gossip broadcast failed: {}", e);
            }
        }
    });

    // Start announce loop
    let announce_node = Arc::clone(&node);
    tokio::spawn(async move {
        if let Err(e) = announce_node.broadcast_announce().await {
            tracing::warn!("Announce broadcast failed: {}", e);
        }
    });

    // Run inbound receive loop until SIGTERM
    info!("Mesh daemon running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    // Graceful shutdown: the peer will time out the node via gossip
    // staleness detection. A full RouterWithdraw broadcast would
    // require HMAC signing (via network_key) — deferred to a future
    // mission when the key management surface is stabilized.

    Ok(())
}

fn parse_node_id(s: &str) -> Result<RouterNodeId> {
    let bytes = hex::decode(s.trim_start_matches("0x"))
        .map_err(|e| anyhow::anyhow!("Invalid node_id hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "node_id must be 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(RouterNodeId(arr))
}

fn parse_network_id(s: &str) -> Result<quota_router_core::node::provider::NetworkId> {
    let bytes = hex::decode(s.trim_start_matches("0x"))
        .map_err(|e| anyhow::anyhow!("Invalid network_id hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "network_id must be 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(quota_router_core::node::provider::NetworkId(arr))
}

fn parse_peer_arg(s: &str) -> Option<PeerConfig> {
    // Format: "node_id_hex:ip:port" or "node_id_hex@ip:port"
    let (id_str, addr_str) = if let Some(pos) = s.find('@') {
        (&s[..pos], &s[pos + 1..])
    } else if let Some(pos) = s.rfind(':') {
        // Find the LAST colon to separate ID from address
        // but ID itself might contain colons if it's not hex...
        // Use a simpler heuristic: split at the first colon after position 64
        // (hex node_id is 64 chars)
        if s.len() > 64 && s.as_bytes()[64] == b':' {
            (&s[..64], &s[65..])
        } else {
            (&s[..pos], &s[pos + 1..])
        }
    } else {
        return None;
    };

    let id_bytes = hex::decode(id_str.trim_start_matches("0x")).ok()?;
    if id_bytes.len() != 32 {
        return None;
    }
    let mut node_id = [0u8; 32];
    node_id.copy_from_slice(&id_bytes);

    let endpoint: SocketAddr = addr_str.parse().ok()?;

    Some(PeerConfig {
        node_id: RouterNodeId(node_id),
        endpoint,
        trust_level: PeerTrust::Trusted,
    })
}

/// W2 replay tool: verify a settlement hash reproduces a deterministic
/// `Receipt` from the sm-engine state machines. Reads an ask_id (hex) +
/// receipt payload from the args, runs the canonical_ser pipeline, and
/// prints the resulting settlement_hash + receipt_id.
/// CLI: `quota-router reputation-show --did <canonical_did>`
/// (mission 0968-b Phase E).
///
/// Reads the persisted RFC-0968 aggregate for `--did`, prints
/// `score_ewma` (Dfp → f64 for display), the 0-100 presentation score,
/// samples, and last_signal_at_unix. Replaces the legacy `provider
/// --name` / `seller --wallet` / `leaderboard` / `multiplier` subcommands.
///
/// `--backend {memory,stoolap}` selects the read-side store. The
/// default `memory` is hermetic (always empty) — useful for the parse
/// test path; the production path requires `--backend stoolap` with
/// `--db-path` pointing at a CipherOcto-fork stoolap DB (per
/// `feedback_stoolap-persistence.md`).
///
/// `--strict-deprecation` is honoured in the retire-PR gate (mission
/// 0968-b Phase D): the retire gate fails closed while set. The
/// intent is "refuse while legacy CLI is retired" (so well-formed
/// canonical DIDs are accepted, and only legacy callers get the
/// error). Set the flag to refuse the entire subcommand.
pub async fn reputation_show(
    did: &str,
    backend: &str,
    db_path: Option<&Path>,
    strict_deprecation: bool,
) -> Result<()> {
    if strict_deprecation {
        return Err(anyhow::anyhow!(
            "strict-deprecation active: reputation-show CLI retired per retirement gate"
        ));
    }
    if backend != "memory" && backend != "stoolap" {
        return Err(anyhow::anyhow!(
            "unsupported backend {backend:?}; expected 'memory' or 'stoolap'"
        ));
    }
    let recorder_did = parse_canonical_did(did)?;
    println!("did: {did}");
    match backend {
        "memory" => {
            let store = InMemoryReputationStore::new();
            let outcome = store
                .read_aggregate(&recorder_did, SignalKind::Outcome, ReputationLayer::Market)
                .await;
            print_outcome(outcome)
        }
        "stoolap" => {
            let db_path = db_path
                .ok_or_else(|| anyhow::anyhow!("--db-path required when --backend stoolap"))?;
            // Mission 0010-b Phase F (0968-b Phase F): the cipherocto-fork
            // stoolap backend is now wired. Opens the file-backed DB, applies
            // migrations on first run, and reads the canonical aggregate.
            #[cfg(feature = "stoolap")]
            {
                let dsn = format!("file://{}", db_path.display());
                let store = octo_reputation::StoolapReputationStore::open(&dsn).await?;
                let outcome = store
                    .read_aggregate(&recorder_did, SignalKind::Outcome, ReputationLayer::Market)
                    .await;
                print_outcome(outcome)
            }
            #[cfg(not(feature = "stoolap"))]
            {
                Err(anyhow::anyhow!(
                    "--backend stoolap requires --features stoolap at build time; \
                     current binary was built without it (db_path={})",
                    db_path.display()
                ))
            }
        }
        _ => unreachable!("backend checked above"),
    }
}

fn print_outcome(
    outcome: Result<
        octo_reputation::types::ReputationAggregate,
        octo_reputation::error::ReputationError,
    >,
) -> Result<()> {
    use octo_reputation::error::ReputationError;
    match outcome {
        Ok(agg) => {
            let score = agg.score_ewma.to_f64();
            let presentation = reputation_score_0_100(agg.score_ewma)?;
            println!("score_ewma: {score:.6} (Dfp)");
            println!("presentation_0_100: {presentation}");
            println!("samples: {}", agg.samples);
            println!("last_signal_at_unix: {}", agg.last_event_unix);
            Ok(())
        }
        // Distinguish "no data" (canonical AggregateNotFound) from
        // "connectivity blip" or "schema drift" (any other Err).
        // The dual-read parity contract (mission 0968-b Phase D)
        // requires this distinction so a fault doesn't get rendered
        // as an empty read-side.
        Err(ReputationError::AggregateNotFound { .. }) => {
            let default_presentation = reputation_score_0_100(Dfp::from_f64(0.0))?;
            println!("score_ewma: <unknown> (no aggregate)");
            println!("presentation_0_100: {default_presentation} (default for unknown providers)");
            println!("samples: 0");
            println!("last_signal_at_unix: 0");
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!(
            "read_aggregate failed; CLI cannot distinguish data-absence from store fault: {e:?}"
        )),
    }
}

/// Compute settlement hash from a partial envelope JSON (RFC-0959 §CLI).
///
/// Input: JSON serialized `SettlementEnvelope` (the `settlement_hash` field is
/// ignored/overwritten). Output: JSON with `settlement_hash` filled in by
/// `SettlementEnvelope::compute_settlement_hash()`.
///
/// Used by routers to compute canonical settlement hashes before signing the
/// receipt + emitting to the consumed-receipt index.
pub fn settle(from: &str) -> Result<String> {
    let envelope_json = read_envelope(from)?;
    settle_from_json(&envelope_json)
}

/// Compute settlement hash from a parsed envelope JSON (testable surface).
pub fn settle_from_json(envelope_json: &str) -> Result<String> {
    use quota_router_storage::ask::SettlementEnvelope;

    let mut envelope: SettlementEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| anyhow::anyhow!("invalid envelope JSON: {e}"))?;
    envelope.settlement_hash = envelope.compute_settlement_hash();
    let out = serde_json::to_string(&envelope)
        .map_err(|e| anyhow::anyhow!("re-serialize envelope: {e}"))?;
    println!(
        "settlement_hash = {}",
        hex::encode(envelope.settlement_hash)
    );
    println!("ask_id          = {}", hex::encode(envelope.ask_id));
    println!("nonce           = {}", hex::encode(envelope.nonce));
    Ok(out)
}

/// Verify a settlement envelope against replay defense (RFC-0959 §CLI).
///
/// Input: JSON `SettlementEnvelope` (with `settlement_hash` already filled).
/// Steps:
/// 1. Recompute `settlement_hash` from canonical fields → `HashMismatch` if
///    envelope fields were tampered with.
/// 2. Check `nonce` against the in-memory `ConsumedReceiptIndex` →
///    `AlreadyConsumed` if the nonce was already inserted (replay).
/// 3. On success, insert the nonce into the index (advances the
///    replay-defense cursor).
///
/// The CLI holds a per-process in-memory index; production deployments
/// back the index with the stoolap-backed `consumed_receipt_index` table
/// (out of scope for this CLI; see `quota-router-storage` schema).
pub fn settle_replay(from: &str) -> Result<()> {
    let envelope_json = read_envelope(from)?;
    settle_replay_from_json(&envelope_json)
}

/// Verify a settlement envelope from JSON (testable surface).
pub fn settle_replay_from_json(envelope_json: &str) -> Result<()> {
    use quota_router_storage::ask::{ConsumedReceiptIndex, SettlementEnvelope};

    let envelope: SettlementEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| anyhow::anyhow!("invalid envelope JSON: {e}"))?;
    let mut index = ConsumedReceiptIndex::new();
    envelope
        .verify(&mut index)
        .map_err(|e| anyhow::anyhow!("settle_replay failed: {e}"))?;
    println!(
        "settlement_hash = {}",
        hex::encode(envelope.settlement_hash)
    );
    println!("nonce           = {}", hex::encode(envelope.nonce));
    println!("index_len       = {}", index.len());
    println!("verify: OK (hash matches + nonce inserted)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_node_id_valid() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let result = parse_node_id(hex_id);
        assert!(result.is_ok());
        let id = result.unwrap();
        assert_eq!(id.0[0], 0x01);
        assert_eq!(id.0[31], 0xef);
    }

    #[test]
    fn parse_node_id_with_0x_prefix() {
        let hex_id = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let result = parse_node_id(hex_id);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_node_id_too_short() {
        let result = parse_node_id("001122");
        assert!(result.is_err());
    }

    #[test]
    fn parse_node_id_invalid_hex() {
        let result = parse_node_id("zzzz");
        assert!(result.is_err());
    }

    #[test]
    fn parse_network_id_valid() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let result = parse_network_id(hex_id);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_peer_arg_valid() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let arg = format!("{}:127.0.0.1:9100", hex_id);
        let result = parse_peer_arg(&arg);
        assert!(result.is_some());
        let peer = result.unwrap();
        assert_eq!(peer.endpoint, "127.0.0.1:9100".parse().unwrap());
        assert_eq!(peer.trust_level, PeerTrust::Trusted);
    }

    #[test]
    fn parse_peer_arg_at_separator() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let arg = format!("{}@10.0.0.1:9200", hex_id);
        let result = parse_peer_arg(&arg);
        assert!(result.is_some());
        let peer = result.unwrap();
        assert_eq!(peer.endpoint, "10.0.0.1:9200".parse().unwrap());
    }

    #[test]
    fn parse_peer_arg_invalid_addr() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let arg = format!("{}:not-an-addr", hex_id);
        let result = parse_peer_arg(&arg);
        assert!(result.is_none());
    }

    #[test]
    fn parse_peer_arg_no_colon() {
        let result = parse_peer_arg("no-separator");
        assert!(result.is_none());
    }

    #[test]
    fn parse_peer_arg_invalid_id() {
        let result = parse_peer_arg("zzzz:127.0.0.1:9100");
        assert!(result.is_none());
    }

    #[test]
    fn network_config_deserialize_minimal() {
        let toml = r#"
node_id = "0101010101010101010101010101010101010101010101010101010101010101"
network_id = "0202020202020202020202020202020202020202020202020202020202020202"

[[providers]]
name = "openai"
"#;
        let cfg: NetworkConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.node_id.len(), 64);
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.providers[0].name, "openai");
        assert_eq!(cfg.providers[0].endpoint, "https://api.openai.com");
        assert!(cfg.providers[0].models.is_empty());
    }

    #[test]
    fn network_config_deserialize_with_models() {
        let toml = r#"
node_id = "0101010101010101010101010101010101010101010101010101010101010101"
network_id = "0202020202020202020202020202020202020202020202020202020202020202"

[[providers]]
name = "anthropic"
endpoint = "https://api.anthropic.com"
models = ["claude-3-opus", "claude-3-sonnet"]
"#;
        let cfg: NetworkConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.providers[0].endpoint, "https://api.anthropic.com");
        assert_eq!(cfg.providers[0].models.len(), 2);
    }

    #[test]
    fn network_config_deserialize_multiple_providers() {
        let toml = r#"
node_id = "0101010101010101010101010101010101010101010101010101010101010101"
network_id = "0202020202020202020202020202020202020202020202020202020202020202"

[[providers]]
name = "openai"

[[providers]]
name = "anthropic"
"#;
        let cfg: NetworkConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.providers.len(), 2);
    }

    #[test]
    fn network_config_deserialize_missing_node_id() {
        let toml = r#"
network_id = "0202020202020202020202020202020202020202020202020202020202020202"

[[providers]]
name = "openai"
"#;
        let result: Result<NetworkConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn network_config_deserialize_missing_network_id() {
        let toml = r#"
node_id = "0101010101010101010101010101010101010101010101010101010101010101"

[[providers]]
name = "openai"
"#;
        let result: Result<NetworkConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn network_config_deserialize_empty_providers() {
        let toml = r#"
node_id = "0101010101010101010101010101010101010101010101010101010101010101"
network_id = "0202020202020202020202020202020202020202020202020202020202020202"
"#;
        let cfg: NetworkConfig = toml::from_str(toml).unwrap();
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn parse_node_id_empty() {
        let result = parse_node_id("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_node_id_64_chars() {
        let hex_id = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = parse_node_id(hex_id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, [0u8; 32]);
    }

    #[test]
    fn parse_network_id_invalid_hex() {
        let result = parse_network_id("not-hex");
        assert!(result.is_err());
    }

    #[test]
    fn parse_peer_arg_empty() {
        assert!(parse_peer_arg("").is_none());
    }

    #[test]
    fn parse_peer_arg_long_hex_with_colon() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let arg = format!("{}:10.0.0.1:9200", hex_id);
        let result = parse_peer_arg(&arg);
        assert!(result.is_some());
        let peer = result.unwrap();
        assert_eq!(peer.endpoint, "10.0.0.1:9200".parse().unwrap());
    }

    // RFC-0959 §CLI: settle + settle-replay round-trip tests.
    use quota_router_storage::ask::ModelRef;

    fn sample_envelope_json() -> String {
        serde_json::json!({
            "settlement_hash": vec![0_u8; 32],
            "asker_did": "did:octo:asker1",
            "holder_did": "did:octo:holder-1",
            "model": {
                "namespace": "openai",
                "family": "gpt-4",
                "version": null,
            },
            "axes_consumed": [["input_tokens_per_1k", 1000]],
            "ask_id": vec![0x42_u8; 32],
            "nonce": vec![0x55_u8; 32],
            "timestamp_unix": 1_700_000_000_u64,
            "cost": 30_000_u128,
        })
        .to_string()
    }

    #[test]
    fn settle_fills_settlement_hash_deterministically() {
        let input = sample_envelope_json();
        let out = settle_from_json(&input).expect("settle");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("re-parse");
        let hash_hex = parsed["settlement_hash"]
            .as_array()
            .expect("settlement_hash array")
            .iter()
            .map(|v| format!("{:02x}", v.as_u64().unwrap()))
            .collect::<String>();
        // 32 zero bytes serialize as [0,0,...]; hash is now non-zero.
        assert_eq!(hash_hex.len(), 64);
        assert!(
            hash_hex.chars().any(|c| c != '0'),
            "computed hash must be non-zero"
        );
        // Idempotency: re-running settle on the same envelope must yield
        // the same hash (since canonical inputs are unchanged).
        let out2 = settle_from_json(&input).expect("settle again");
        assert_eq!(out, out2, "settle must be deterministically idempotent");
        // Reference shape: ModelRef was implicitly constructed from "openai/gpt-4".
        let model: ModelRef = "openai/gpt-4".parse().expect("parse model");
        assert_eq!(model.to_wire(), "openai/gpt-4");
    }

    #[test]
    fn settle_replay_passes_after_settle_then_canonicalizes_hash() {
        let input = sample_envelope_json();
        let settled = settle_from_json(&input).expect("settle");
        // Now settled JSON has the embedded settlement_hash.
        // settle_replay should verify and insert the nonce.
        let out = settle_replay_from_json(&settled);
        assert!(
            out.is_ok(),
            "settle_replay after settle must succeed: {out:?}"
        );
    }

    #[test]
    fn settle_replay_rejects_tampered_envelope() {
        let input = sample_envelope_json();
        let mut settled: serde_json::Value =
            serde_json::from_str(&settle_from_json(&input).unwrap()).unwrap();
        // Tamper: flip a byte in timestamp_unix. This IS part of the canonical
        // preimage (see SettlementEnvelope::compute_settlement_hash), so the
        // embedded settlement_hash will no longer match. `cost` is NOT part of
        // the hash preimage, so it would silently pass — confirmed by an
        // earlier round of this test.
        settled["timestamp_unix"] = serde_json::json!(1_700_000_999_u64);
        let tampered = serde_json::to_string(&settled).unwrap();
        let result = settle_replay_from_json(&tampered);
        assert!(result.is_err(), "tampered envelope must fail verify");
        let err = format!("{result:?}");
        assert!(
            err.contains("settlement hash mismatch") || err.contains("HashMismatch"),
            "error must indicate hash mismatch, got: {err}"
        );
    }
}
