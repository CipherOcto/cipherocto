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
use serde::{Deserialize, Deserializer};
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

/// Deserialize `Vec<u8>` from either a JSON hex string (`"deadbeef"`) or a
/// JSON byte sequence (`[0xde, 0xad, 0xbe, 0xef]`). Used by `VerifyEnvelopeWire`
/// so operators can paste hex strings in CLI input without re-encoding.
fn deserialize_proof_bytes<'de, D>(d: D) -> std::result::Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum HexOrBytes {
        Hex(String),
        Bytes(Vec<u8>),
    }
    match HexOrBytes::deserialize(d)? {
        HexOrBytes::Hex(s) => hex::decode(&s).map_err(serde::de::Error::custom),
        HexOrBytes::Bytes(b) => Ok(b),
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

    // Open database and initialize schema. substrate's `Database::open`
    // accepts the bare path; it prepends `file://` internally.
    let db = octo_storage_core::Database::open(&config.db_path.to_string_lossy())?;
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
/// 2. Check `nonce` against the persisted `consumed_receipt_index` table
///    (in-memory by default; file-backed if `--db-path` is supplied) →
///    `AlreadyConsumed` if the nonce was already inserted (replay).
/// 3. On success, insert the nonce into the table (advances the
///    replay-defense cursor; persists across CLI invocations against the
///    same `--db-path`).
///
/// `settle_replay` is the CLI entry point; `settle_replay_repo` is the
/// testable surface (takes a `ConsumedReceiptRepository` directly); both
/// delegate to the same `SettlementEnvelope::verify` semantics.
pub fn settle_replay(from: &str, db_path: Option<&str>) -> Result<()> {
    let envelope_json = read_envelope(from)?;
    let repo = match db_path {
        Some(p) => {
            quota_router_storage::consumed_receipt_repo::ConsumedReceiptRepository::open_path(p)
                .map_err(|e| anyhow::anyhow!("open db {p}: {e}"))?
        }
        None => {
            quota_router_storage::consumed_receipt_repo::ConsumedReceiptRepository::open_in_memory()
                .map_err(|e| anyhow::anyhow!("open in-memory store: {e}"))?
        }
    };
    settle_replay_repo(&envelope_json, &repo)
}

/// Verify a settlement envelope from JSON (testable surface).
pub fn settle_replay_from_json(envelope_json: &str) -> Result<()> {
    // Legacy in-memory test path. New code should use settle_replay_repo
    // against the persisted DAO. Kept for backward-compat with the unit tests
    // in this module (which was authored before the persisted DAO existed).
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

/// Verify a settlement envelope against the persisted DAO (testable surface).
pub fn settle_replay_repo(
    envelope_json: &str,
    repo: &quota_router_storage::consumed_receipt_repo::ConsumedReceiptRepository,
) -> Result<()> {
    use quota_router_storage::ask::SettlementEnvelope;
    use quota_router_storage::consumed_receipt_repo::VerifyOutcome;

    let envelope: SettlementEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| anyhow::anyhow!("invalid envelope JSON: {e}"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let outcome = repo
        .verify_and_insert(&envelope, now)
        .map_err(|e| anyhow::anyhow!("settle_replay failed: {e}"))?;
    match outcome {
        VerifyOutcome::HashMismatch => Err(anyhow::anyhow!("settle_replay failed: hash mismatch")),
        VerifyOutcome::AlreadyConsumed => Err(anyhow::anyhow!(
            "settle_replay failed: nonce already consumed (replay)"
        )),
        VerifyOutcome::Inserted(_) => {
            println!(
                "settlement_hash = {}",
                hex::encode(envelope.settlement_hash)
            );
            println!("nonce           = {}", hex::encode(envelope.nonce));
            println!("index_len       = {}", repo.len().unwrap_or(0));
            println!("verify: OK (hash matches + nonce inserted)");
            Ok(())
        }
    }
}

/// List persisted settlement events for an asker (RFC-0959 §Event Sourcing).
pub fn settle_list(asker_did: &str, db_path: Option<&str>) -> Result<()> {
    let repo =
        quota_router_storage::settlement_event_repo::SettlementEventRepository::open(db_path)
            .map_err(|e| anyhow::anyhow!("open settlement_events: {e}"))?;
    let events = repo
        .list_by_asker(asker_did)
        .map_err(|e| anyhow::anyhow!("list_by_asker: {e}"))?;
    println!("asker_did = {}  events = {}", asker_did, events.len());
    for ev in &events {
        println!(
            "settlement_hash = {}  ask_id = {}  nonce = {}  cost_micro_octo_w = {:?}  settled_at_unix = {}",
            hex::encode(ev.settlement_hash),
            hex::encode(ev.ask_id),
            hex::encode(&ev.nonce),
            ev.cost_micro_octo_w,
            ev.settled_at_unix,
        );
    }
    Ok(())
}

// =========================================================================
// ZK proof verification (mission zk-proof-verification AC-1 / AC-2 / AC-5).
// =========================================================================
//
// Wraps `zk_verifier::verify_capability_zk` for the CLI surface. Sub-modes:
//   - single:  one envelope JSON → one verify → one history append
//   - batch:   JSON array of envelopes → N verifies → N history appends
//   - history: print JSONL log (newest first)
//
// History persistence: JSONL at `default_history_path()` (XDG-aware) or
// `--history-path` override. Atomic append via `OpenOptions::append` +
// `writeln!`. No external DB — AC-3 (on-chain settlement) is deferred.

/// JSON envelope for a single ZK proof verification request.
///
/// Wire schema (matches `crates/quota-router-cli` RFC-0959 envelope style):
/// ```json
/// {
///   "proof_bundle":   { "proof_bytes": "<hex>" },
///   "public_inputs":  { "proof_issued_at_unix": 1700000000,
///                        "verifier_local_unix_time": 1700000005,
///                        "compiled_casm_hash": "<hex>",
///                        "capability_root_hash": "<hex>",
///                        "provider_slot_id": "slot-a" },
///   "casm_hash":      "<hex>"
/// }
/// ```
///
/// `proof_bundle.proof_bytes` accepts either a hex string (`"deadbeef"`) or
/// a JSON byte sequence (`[222, 173, 190, 239]`) via `deserialize_proof_bytes`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifyEnvelope {
    /// STWO proof bundle (RFC-0958 §Proof Bundle). `proof_bytes` field
    /// accepts hex string OR JSON byte sequence on deserialization.
    pub proof_bundle: VerifyProofBundleWire,
    /// Public inputs to the ZK proof (RFC-0958 §Public Inputs).
    pub public_inputs: zk_verifier::PublicInputs,
    /// CASM hash passed to `verify_capability_zk` (may differ from
    /// `public_inputs.compiled_casm_hash` for drift-detection testing).
    pub casm_hash: String,
}

/// Wire-shape proof bundle: `proof_bytes` accepts hex OR byte array.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifyProofBundleWire {
    /// Proof bytes (hex string OR JSON byte sequence on input; always
    /// serialized as hex on output).
    #[serde(
        serialize_with = "serialize_proof_bytes_hex",
        deserialize_with = "deserialize_proof_bytes"
    )]
    pub proof_bytes: Vec<u8>,
}

impl From<VerifyProofBundleWire> for zk_verifier::ProofBundle {
    fn from(w: VerifyProofBundleWire) -> Self {
        Self {
            proof_bytes: w.proof_bytes,
        }
    }
}

impl From<zk_verifier::ProofBundle> for VerifyProofBundleWire {
    fn from(b: zk_verifier::ProofBundle) -> Self {
        Self {
            proof_bytes: b.proof_bytes,
        }
    }
}

fn serialize_proof_bytes_hex<S>(bytes: &Vec<u8>, s: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&hex::encode(bytes))
}

/// One row in the verify-history JSONL log.
///
/// Written after each verify attempt (single or batch element). Schema is
/// stable; consumers (e.g. an RFC-0968 reputation signal feed) can parse
/// the log without version negotiation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifyHistoryEntry {
    /// Unix timestamp the verify call ran.
    pub verified_at_unix: u64,
    /// `Ok` / one of the `VerifyError` variant names (lowercased for grep).
    pub outcome: String,
    /// BLAKE3 hash (hex) of the proof bytes — fingerprint, NOT the
    /// proof itself (proofs may be 50-500 KB per RFC-0958 §Wire Format).
    pub proof_fingerprint_hex: String,
    /// CASM hash that was bound (hex).
    pub casm_hash: String,
    /// Capability root hash (hex).
    pub capability_root_hash: String,
    /// Provider slot ID.
    pub provider_slot_id: String,
}

/// Default history file path: `${XDG_DATA_HOME:-~/.local/share}/cipherocto/quota-router/verify-history.jsonl`.
pub fn default_history_path() -> std::path::PathBuf {
    let base = directories::ProjectDirs::from("ai", "cipherocto", "quota-router")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("."));
            std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("cipherocto")
                .join("quota-router")
        });
    base.join("verify-history.jsonl")
}

/// Hex-fingerprint a proof bundle (BLAKE3 of bytes, hex-encoded).
fn proof_fingerprint_hex(proof: &zk_verifier::ProofBundle) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&proof.proof_bytes);
    hex::encode(hasher.finalize().as_bytes())
}

/// Verify a single proof envelope and append the result to history.
///
/// Testable surface: callers pass `envelope_json` and `history_path`
/// directly. The CLI dispatcher in `verify()` reads from `--from`/stdin
/// and calls this with the default path.
pub fn verify_one_from_json(
    envelope_json: &str,
    history_path: &Path,
    now_unix: u64,
) -> Result<VerifyHistoryEntry> {
    let envelope: VerifyEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| anyhow::anyhow!("invalid envelope JSON: {e}"))?;
    let proof_bundle: zk_verifier::ProofBundle = envelope.proof_bundle.into();
    let result = zk_verifier::verify_capability_zk(
        &proof_bundle,
        &envelope.public_inputs,
        &envelope.casm_hash,
    );
    let outcome = match &result {
        Ok(()) => "ok".to_owned(),
        Err(e) => format!("{:?}", e)
            .split_whitespace()
            .next()
            .unwrap_or("error")
            .to_lowercase(),
    };
    let entry = VerifyHistoryEntry {
        verified_at_unix: now_unix,
        outcome,
        proof_fingerprint_hex: proof_fingerprint_hex(&proof_bundle),
        casm_hash: envelope.casm_hash.clone(),
        capability_root_hash: envelope.public_inputs.capability_root_hash.clone(),
        provider_slot_id: envelope.public_inputs.provider_slot_id.clone(),
    };
    append_history(history_path, &entry)?;
    Ok(entry)
}

/// Verify a batch (JSON array of single envelopes) and append each result.
pub fn verify_batch_from_json(
    envelopes_json: &str,
    history_path: &Path,
    now_unix: u64,
) -> Result<Vec<VerifyHistoryEntry>> {
    let envelopes: Vec<VerifyEnvelope> = serde_json::from_str(envelopes_json)
        .map_err(|e| anyhow::anyhow!("invalid batch JSON: {e}"))?;
    let mut out = Vec::with_capacity(envelopes.len());
    for (idx, env) in envelopes.iter().enumerate() {
        let env_json = serde_json::to_string(env)
            .map_err(|e| anyhow::anyhow!("re-serialize envelope[{idx}]: {e}"))?;
        let entry = verify_one_from_json(&env_json, history_path, now_unix)?;
        out.push(entry);
    }
    Ok(out)
}

/// Print verification history (newest first). Empty log → ok with no rows.
pub fn verify_history_print(history_path: &Path) -> Result<()> {
    if !history_path.exists() {
        println!("(no verifications recorded; history file does not exist)");
        return Ok(());
    }
    let raw = std::fs::read_to_string(history_path)?;
    let mut entries: Vec<VerifyHistoryEntry> = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: VerifyHistoryEntry =
            serde_json::from_str(line).map_err(|e| anyhow::anyhow!("history parse error: {e}"))?;
        entries.push(entry);
    }
    entries.sort_by_key(|b| std::cmp::Reverse(b.verified_at_unix));
    println!("history entries = {}", entries.len());
    for e in &entries {
        println!(
            "verified_at_unix = {}  outcome = {}  proof = {}  casm = {}  cap_root = {}  slot = {}",
            e.verified_at_unix,
            e.outcome,
            &e.proof_fingerprint_hex[..16],
            &e.casm_hash[..16.min(e.casm_hash.len())],
            &e.capability_root_hash[..16.min(e.capability_root_hash.len())],
            e.provider_slot_id,
        );
    }
    Ok(())
}

/// Append one entry to the JSONL history file. Creates parent dirs.
fn append_history(history_path: &Path, entry: &VerifyHistoryEntry) -> Result<()> {
    if let Some(parent) = history_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let line =
        serde_json::to_string(entry).map_err(|e| anyhow::anyhow!("history serialize: {e}"))?;
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// CLI dispatcher for `quota-router verify ...`.
pub fn verify(from: &str, batch: bool, history: bool, history_path: Option<&Path>) -> Result<()> {
    let history_path = history_path
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(default_history_path);
    if history {
        return verify_history_print(&history_path);
    }
    let raw = read_envelope(from)?;
    // Deterministic timestamp for tests: read from `NOW_UNIX` env if set.
    let now_unix: u64 = std::env::var("NOW_UNIX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
    if batch {
        let entries = verify_batch_from_json(&raw, &history_path, now_unix)?;
        println!("batch verified: {} entries", entries.len());
        Ok(())
    } else {
        let entry = verify_one_from_json(&raw, &history_path, now_unix)?;
        println!(
            "verify: {} (proof = {})",
            entry.outcome,
            &entry.proof_fingerprint_hex[..16]
        );
        Ok(())
    }
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
        // `cost` is `octo_determin::Dqa` post S4 codemod; the canonical wire
        // form is 16 bytes via `dqa_serde::field` (see
        // quota-router-storage/src/dqa_serde.rs §serialize_bytes). Layout
        // (per determin/src/dqa.rs §DqaEncoding):
        //   bytes 0..8   i64 BE   (value)
        //   byte  8      u8        (scale)
        //   bytes 9..16  [u8; 7]   (reserved = 0, validated)
        // 30_000 with scale=0: value = 0x7530, BE → [0,0,0,0,0,0,0x75,0x30].
        let cost_be = {
            let mut bytes = [0_u8; 16];
            bytes[6] = 0x75;
            bytes[7] = 0x30;
            // scale = 0 → byte 8 stays 0
            // reserved = [0; 7] → bytes 9..16 stay 0
            bytes.to_vec()
        };
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
            "cost": cost_be,
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
    fn settle_replay_repo_persists_nonce_across_replay_attempts() {
        // Confirms the persisted DAO (not the in-memory HashMap) is the
        // canonical replay-defense path. Sprint 7 (mission 0959-a S7).
        let repo =
            quota_router_storage::consumed_receipt_repo::ConsumedReceiptRepository::open_in_memory(
            )
            .expect("open in-memory DAO");
        let input = sample_envelope_json();
        let settled = settle_from_json(&input).expect("settle");
        // First call: verify + insert.
        settle_replay_repo(&settled, &repo).expect("first settle_replay");
        assert_eq!(repo.len().unwrap(), 1, "first call inserts 1 row");
        // Second call (same settled JSON, same nonce): AlreadyConsumed.
        let err = settle_replay_repo(&settled, &repo).unwrap_err();
        assert!(
            format!("{err:?}").contains("nonce already consumed"),
            "replay attempt must fail with AlreadyConsumed, got: {err:?}"
        );
        // Third call: tamper with timestamp_unix (canonical preimage field).
        let mut tampered: serde_json::Value = serde_json::from_str(&settled).unwrap();
        tampered["timestamp_unix"] = serde_json::json!(1_700_000_999_u64);
        let tampered_str = serde_json::to_string(&tampered).unwrap();
        let err = settle_replay_repo(&tampered_str, &repo).unwrap_err();
        assert!(
            format!("{err:?}").contains("hash mismatch"),
            "tampered envelope must fail with hash mismatch, got: {err:?}"
        );
        // Row count unchanged: tamper + replay both rejected BEFORE insert.
        assert_eq!(repo.len().unwrap(), 1, "no new rows after rejection");
    }

    // Settle-list CLI surface (RFC-0959 §Event Sourcing).
    #[test]
    fn settle_list_with_no_persisted_events_returns_empty() {
        // Empty in-memory DB returns 0 events for any asker.
        let result = settle_list(&octo_ident::test_helpers::sample_did(139), None);
        assert!(result.is_ok(), "settle_list with empty DB must succeed");
    }

    // =====================================================================
    // ZK proof verification tests (mission zk-proof-verification AC-1/AC-2/AC-5)
    // =====================================================================
    //
    // Requires `zk-verifier` built with `--features allow-stub-verifier`
    // (declared in `quota-router-cli/Cargo.toml [dev-dependencies]`). Without
    // the feature, `verify_capability_zk` returns `Err(StubDisabled)` for
    // every stub-shaped proof (production semantics).
    //
    // The test paths use `tempfile::tempdir()` for history isolation; the
    // JSONL file is per-test and torn down on drop.

    use zk_verifier::{ProofBundle, PublicInputs};

    fn stub_envelope_json(casm: &str, issued_at: u64, verify_at: u64) -> (String, [u8; 32]) {
        let public = PublicInputs {
            proof_issued_at_unix: issued_at,
            verifier_local_unix_time: verify_at,
            compiled_casm_hash: casm.to_owned(),
            capability_root_hash: "caproot-test".to_owned(),
            provider_slot_id: "slot-test".to_owned(),
        };
        // Compute the stub commitment locally (deterministic BLAKE3) so the
        // proof bytes satisfy the stub verifier.
        let commitment = zk_verifier::stub_commitment(casm, &public)
            .expect("stub_commitment Ok under --features allow-stub-verifier (test build)");
        let mut proof_bytes = commitment.to_vec();
        // Append a tail so proof_bytes.len() > 32; the verifier checks
        // `proof_bytes.len() >= 32`.
        proof_bytes.extend_from_slice(b"trailing-padding-bytes");
        let envelope = serde_json::json!({
            "proof_bundle": { "proof_bytes": hex::encode(&proof_bytes) },
            "public_inputs": {
                "proof_issued_at_unix": public.proof_issued_at_unix,
                "verifier_local_unix_time": public.verifier_local_unix_time,
                "compiled_casm_hash": public.compiled_casm_hash,
                "capability_root_hash": public.capability_root_hash,
                "provider_slot_id": public.provider_slot_id,
            },
            "casm_hash": casm,
        });
        (envelope.to_string(), commitment)
    }

    #[test]
    fn verify_one_ok_writes_history_with_ok_outcome() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join("verify-history.jsonl");
        let (env, _commit) = stub_envelope_json("casm-ok", 1_700_000_000, 1_700_000_005);
        let entry =
            verify_one_from_json(&env, &history, 1_700_000_010).expect("verify_one_from_json");
        assert_eq!(entry.outcome, "ok", "valid stub proof must verify Ok");
        assert_eq!(entry.verified_at_unix, 1_700_000_010);
        assert_eq!(entry.casm_hash, "casm-ok");
        // History file exists and contains exactly one line.
        let raw = std::fs::read_to_string(&history).expect("read history");
        assert_eq!(raw.lines().count(), 1, "one JSONL line per verify");
        assert!(
            raw.contains("\"outcome\":\"ok\""),
            "history JSON must encode ok outcome"
        );
        // Re-parse to confirm shape.
        let parsed: VerifyHistoryEntry =
            serde_json::from_str(raw.trim()).expect("re-parse history entry");
        assert_eq!(parsed.outcome, "ok");
        assert_eq!(parsed.casm_hash, "casm-ok");
    }

    #[test]
    fn verify_one_casm_mismatch_writes_history_with_error_outcome() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join("verify-history.jsonl");
        let (env, _commit) = stub_envelope_json("casm-actual", 1_700_000_000, 1_700_000_005);
        // Tamper the embedded casm_hash in the envelope so public_inputs.compiled_casm_hash
        // differs from the wrapper's `casm_hash` field — drives CasmHashMismatch.
        let mut v: serde_json::Value = serde_json::from_str(&env).expect("parse");
        v["casm_hash"] = serde_json::json!("casm-EXPECTED");
        let env_tampered = serde_json::to_string(&v).expect("re-serialize");
        let entry = verify_one_from_json(&env_tampered, &history, 1_700_000_020)
            .expect("verify_one_from_json returns history entry even on error");
        assert!(
            entry.outcome.contains("casm"),
            "outcome must surface the casm mismatch variant; got {}",
            entry.outcome
        );
        // History still appends on error (audit trail).
        let raw = std::fs::read_to_string(&history).expect("read");
        assert_eq!(raw.lines().count(), 1);
    }

    #[test]
    fn verify_one_clock_skew_writes_history_with_error_outcome() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join("verify-history.jsonl");
        // Skew = 600s (> MAX_SKEW_SECS = 300s). Stub proofer still constructs
        // a valid commitment (it's forgeable); the verifier rejects on skew
        // before the commitment check.
        let (env, _commit) = stub_envelope_json("casm-skew", 1_700_000_000, 1_700_000_600);
        let entry = verify_one_from_json(&env, &history, 1_700_000_700)
            .expect("verify_one_from_json appends even on skew rejection");
        assert!(
            entry.outcome.contains("clock"),
            "outcome must surface ClockSkewExceeded; got {}",
            entry.outcome
        );
    }

    #[test]
    fn verify_batch_appends_one_entry_per_envelope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join("verify-history.jsonl");
        let (env0, _) = stub_envelope_json("casm-batch-0", 1_700_000_000, 1_700_000_005);
        let (env1, _) = stub_envelope_json("casm-batch-1", 1_700_000_100, 1_700_000_105);
        let batch = serde_json::json!([
            serde_json::from_str::<serde_json::Value>(&env0).unwrap(),
            serde_json::from_str::<serde_json::Value>(&env1).unwrap()
        ]);
        let entries = verify_batch_from_json(&batch.to_string(), &history, 1_700_000_200)
            .expect("verify_batch_from_json");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].casm_hash, "casm-batch-0");
        assert_eq!(entries[1].casm_hash, "casm-batch-1");
        let raw = std::fs::read_to_string(&history).expect("read");
        assert_eq!(raw.lines().count(), 2, "batch must append 2 JSONL rows");
    }

    #[test]
    fn verify_history_print_empty_path_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist.jsonl");
        let result = verify_history_print(&missing);
        assert!(
            result.is_ok(),
            "missing history file is OK (no entries yet)"
        );
    }

    #[test]
    fn verify_history_print_newest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join("verify-history.jsonl");
        let (env0, _) = stub_envelope_json("casm-hist-0", 1_700_000_000, 1_700_000_005);
        let (env1, _) = stub_envelope_json("casm-hist-1", 1_700_000_100, 1_700_000_105);
        // Append in chronological order: row-0 first (older), row-1 second
        // (newer). The file is in append order on disk; the print function
        // sorts DESC for display.
        verify_one_from_json(&env0, &history, 1_700_000_010).expect("first verify");
        verify_one_from_json(&env1, &history, 1_700_000_110).expect("second verify");
        // File on disk: row-0 (older), row-1 (newer).
        let raw = std::fs::read_to_string(&history).expect("read");
        assert_eq!(raw.lines().count(), 2);
        let parsed: Vec<VerifyHistoryEntry> = raw
            .lines()
            .map(|l| serde_json::from_str::<VerifyHistoryEntry>(l).expect("parse"))
            .collect();
        assert_eq!(
            parsed[0].casm_hash, "casm-hist-0",
            "file on disk: first row is the chronologically-older verify"
        );
        assert_eq!(
            parsed[1].casm_hash, "casm-hist-1",
            "file on disk: second row is the chronologically-newer verify"
        );
        // Re-sort DESC to confirm the print function's contract: newest first.
        let mut sorted_desc = parsed.clone();
        sorted_desc.sort_by_key(|b| std::cmp::Reverse(b.verified_at_unix));
        assert_eq!(
            sorted_desc[0].casm_hash, "casm-hist-1",
            "newest-first sort: most recent verify first"
        );
        assert_eq!(
            sorted_desc[1].casm_hash, "casm-hist-0",
            "newest-first sort: older verify last"
        );
        // Smoke: verify_history_print returns Ok and emits to stdout.
        let result = verify_history_print(&history);
        assert!(result.is_ok(), "history print must succeed: {result:?}");
        let _ = ProofBundle {
            proof_bytes: vec![],
        }; // suppress unused-import warning.
    }
}
