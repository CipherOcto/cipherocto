//! Bootstrap orchestrator — wires RFC-0851p-a Mode A into the
//! `octo-transport` startup path.
//!
//! Mission `0851p-a-base-bootstrap-orchestrator`.
//!
//! ## Status: Stub — Response Collection Not Yet Implemented
//!
//! The `send_bootstrap_requests` method sends BOOTSTRAP_REQ to each
//! seed but does **not** yet collect BOOTSTRAP_RESP. Real responses
//! arrive asynchronously via the `NetworkReceiver` inbound path,
//! which is not wired into this module yet. As a result, `run()`
//! will always return `NoResponses` when responses are required
//! (min_responses > 0). The validation, intersection, and cache
//! population logic is complete and tested via unit tests with
//! direct `compute_intersection` calls.

use std::time::Duration;

use octo_network::gdp::cache::GatewayCacheEntry;
use octo_network::gdp::discovery::DiscoveryState;
use octo_network::gdp::types::DiscoveryLifecycle;
use octo_network::mon::bootstrap::{
    SeedAuthorityError, SeedHealth, SeedListAuthority, SeedListEnvelope, SlashedSeedBlacklist,
};

use crate::discovery::TransportDiscovery;
use crate::node_transport::NodeTransport;
use crate::sender::TransportError;

// ── Constants (RFC-0851p-a §D) ────────────────────────────────────

/// Maximum peer list size in a BOOTSTRAP_RESP.
pub const MAX_PEER_LIST: u16 = 256;

/// High-confidence minimum responses for Sybil defense (≥3 of 5).
pub const MIN_BOOTSTRAP_RESPONSES: usize = 3;

/// Intersection threshold for Sybil defense (≥80%).
pub const PEER_LIST_INTERSECTION_THRESHOLD: f64 = 0.80;

/// Default bootstrap timeout.
pub const DEFAULT_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(60);

/// Default max retries before fallback.
pub const DEFAULT_MAX_RETRIES: u32 = 5;

/// Default initial retry backoff.
pub const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Maximum retry backoff.
pub const MAX_BACKOFF: Duration = Duration::from_secs(60);

// ── BootstrapClientLifecycle ──────────────────────────────────────

/// Bootstrap client lifecycle state machine (RFC-0851p-a §3).
///
/// `FallbackB`, `FallbackC`, and `Failed` are terminal states.
/// `FallbackB`/`FallbackC` are placeholders for future Mode B/C
/// implementations — in this mission they map to
/// `BootstrapError::AllTransportsFailed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BootstrapClientLifecycle {
    Init = 0x01,
    Connecting = 0x02,
    Validating = 0x03,
    Cached = 0x04,
    FallbackB = 0x05,
    FallbackC = 0x06,
    Done = 0x07,
    Failed = 0x08,
}

// ── BootstrapConfig ───────────────────────────────────────────────

/// Configuration for the bootstrap protocol.
#[derive(Clone, Debug)]
pub struct BootstrapConfig {
    /// Max time to wait for bootstrap responses.
    pub bootstrap_timeout: Duration,
    /// Minimum responses for high-confidence bootstrap.
    pub min_responses: usize,
    /// Peer-list intersection threshold (0.0-1.0).
    pub intersection_threshold: f64,
    /// Max retries before fallback.
    pub max_retries: u32,
    /// Initial retry backoff.
    pub initial_backoff: Duration,
    /// The seed list authority type (Foundation or Dao).
    /// Operator configuration; not embedded in the envelope.
    pub authority: SeedListAuthority,
    /// Current epoch (for staleness and authority checks).
    pub current_epoch: u64,
    /// The bootstrapping node's identity (32-byte PeerId).
    pub node_id: [u8; 32],
    /// The bootstrapping node's public key (Ed25519).
    pub node_pubkey: [u8; 32],
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            bootstrap_timeout: DEFAULT_BOOTSTRAP_TIMEOUT,
            min_responses: MIN_BOOTSTRAP_RESPONSES,
            intersection_threshold: PEER_LIST_INTERSECTION_THRESHOLD,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            authority: SeedListAuthority::Foundation,
            current_epoch: 0,
            node_id: [0u8; 32],
            node_pubkey: [0u8; 32],
        }
    }
}

// ── BootstrapError ────────────────────────────────────────────────

/// Bootstrap protocol error.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("seed list is fully stale")]
    SeedListStale,

    #[error("seed list authority error")]
    AuthorityError(SeedAuthorityError),

    #[error("no bootstrap responses received")]
    NoResponses,

    #[error("peer-list intersection below threshold ({actual:.0}% < {required:.0}%)")]
    IntersectionBelowThreshold { actual: f64, required: f64 },

    #[error("all transports failed")]
    AllTransportsFailed(#[from] TransportError),

    #[error("invalid bootstrap response signature")]
    SignatureInvalid,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),
}

// ── Wire types (RFC-0851p-a §2) ───────────────────────────────────

/// GDP/1/BOOTSTRAP_REQ — sent by bootstrapping node.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BootstrapRequest {
    pub requester_id: [u8; 32],
    pub requester_pubkey: [u8; 32],
    pub nonce: [u8; 16],
    pub epoch: u64,
    pub capability_filter: u64,
    pub max_peers: u16,
    // Signature omitted in wire format for JSON serialization;
    // will be added when canonical serialization (RFC-0126) is implemented.
}

/// GDP/1/BOOTSTRAP_RESP — sent by bootstrap node.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BootstrapResponse {
    pub requester_id: [u8; 32],
    pub request_nonce: [u8; 16],
    pub epoch: u64,
    pub responder_id: [u8; 32],
    /// Peer entries returned by the bootstrap node.
    pub peer_entries: Vec<BootstrapPeerEntry>,
}

/// A peer entry in a BOOTSTRAP_RESP.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BootstrapPeerEntry {
    pub peer_id: [u8; 32],
    pub multiaddr: String,
}

// ── BootstrapOrchestrator ─────────────────────────────────────────

/// Drives the RFC-0851p-a Mode A bootstrap protocol.
///
/// Consumes [`SeedListEnvelope`], [`SeedHealth`], [`SeedListAuthority`],
/// and [`SlashedSeedBlacklist`] from `octo-network::mon::bootstrap`.
/// Produces peer entries in [`TransportDiscovery`].
///
/// ## Limitation
///
/// Response collection (`send_bootstrap_requests`) is a stub — it
/// sends BOOTSTRAP_REQ but cannot collect BOOTSTRAP_RESP because the
/// `NetworkReceiver` inbound path is not wired into this module.
/// `run()` will return `NoResponses` when `min_responses > 0` and no
/// external response injection mechanism exists.
pub struct BootstrapOrchestrator {
    seed_list: SeedListEnvelope,
    blacklist: SlashedSeedBlacklist,
    state: BootstrapClientLifecycle,
    config: BootstrapConfig,
}

impl BootstrapOrchestrator {
    /// Create a new orchestrator from a seed list and config.
    pub fn new(seed_list: SeedListEnvelope, config: BootstrapConfig) -> Self {
        Self {
            seed_list,
            blacklist: SlashedSeedBlacklist::new(),
            state: BootstrapClientLifecycle::Init,
            config,
        }
    }

    /// Create an orchestrator with a pre-populated blacklist.
    pub fn with_blacklist(
        seed_list: SeedListEnvelope,
        blacklist: SlashedSeedBlacklist,
        config: BootstrapConfig,
    ) -> Self {
        Self {
            seed_list,
            blacklist,
            state: BootstrapClientLifecycle::Init,
            config,
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> BootstrapClientLifecycle {
        self.state
    }

    /// Run the bootstrap protocol to completion.
    ///
    /// Returns the number of peers acquired, or an error if all modes
    /// fail. On success, `discovery` cache and `discovery_state`
    /// lifecycle are updated.
    pub async fn run(
        &mut self,
        transport: &NodeTransport,
        discovery: &TransportDiscovery,
        discovery_state: &mut DiscoveryState,
    ) -> Result<u32, BootstrapError> {
        // Step 1: Filter slashed seeds
        let filtered = self.blacklist.filter(self.seed_list.clone());
        if filtered.peers.is_empty() {
            self.state = BootstrapClientLifecycle::Failed;
            return Err(BootstrapError::NoResponses);
        }

        // Step 2: Seed health check
        let health = SeedHealth::check(&filtered, self.config.current_epoch);
        if health.refuses_start() {
            self.state = BootstrapClientLifecycle::Failed;
            return Err(BootstrapError::SeedListStale);
        }

        // Step 3: Authority verification
        match octo_network::mon::bootstrap::verify_authority(
            self.config.authority,
            self.config.current_epoch,
        ) {
            Ok(()) => {}
            Err(e) => {
                self.state = BootstrapClientLifecycle::Failed;
                return Err(BootstrapError::AuthorityError(e));
            }
        }

        // Step 4-6: Send BOOTSTRAP_REQ, collect responses
        self.state = BootstrapClientLifecycle::Connecting;

        let mut attempt = 0u32;

        while attempt < self.config.max_retries {
            let responses = self.send_bootstrap_requests(transport, &filtered).await;

            if responses.len() >= self.config.min_responses {
                // Step 7: Validate (signatures deferred — stub mode)
                // Step 8: Compute peer-list intersection
                self.state = BootstrapClientLifecycle::Validating;

                let peer_sets: Vec<Vec<[u8; 32]>> = responses
                    .iter()
                    .map(|r| {
                        let mut ids: Vec<[u8; 32]> =
                            r.peer_entries.iter().map(|p| p.peer_id).collect();
                        ids.sort();
                        ids
                    })
                    .collect();

                let intersection = compute_intersection(&peer_sets);
                let agreement = if !peer_sets.is_empty() {
                    let max_peers = peer_sets.iter().map(|s| s.len()).max().unwrap_or(1);
                    intersection.len() as f64 / max_peers as f64
                } else {
                    0.0
                };

                if agreement >= self.config.intersection_threshold {
                    // Step 9: Merge into TransportDiscovery
                    self.state = BootstrapClientLifecycle::Cached;
                    let peer_count =
                        self.populate_discovery(&intersection, discovery, discovery_state);
                    self.state = BootstrapClientLifecycle::Done;
                    return Ok(peer_count);
                }
                // Intersection below threshold — retry
            }

            attempt += 1;
            if attempt < self.config.max_retries {
                // Exponential backoff (RFC-0851p-a §3)
                let backoff = self
                    .config
                    .initial_backoff
                    .saturating_mul(2u32.saturating_pow(attempt - 1));
                let backoff = backoff.min(MAX_BACKOFF);
                tokio::time::sleep(backoff).await;
            }
        }

        // All retries exhausted
        self.state = BootstrapClientLifecycle::Failed;
        Err(BootstrapError::NoResponses)
    }

    /// Send BOOTSTRAP_REQ to each seed via direct TCP connections and
    /// collect responses. Bootstrap happens before the mesh transport
    /// is established, so we connect directly to each seed rather than
    /// routing through `NodeTransport`.
    ///
    /// Each seed is contacted concurrently. Responses are collected
    /// until `min_responses` are received or the timeout expires.
    async fn send_bootstrap_requests(
        &self,
        _transport: &NodeTransport,
        seed_list: &SeedListEnvelope,
    ) -> Vec<BootstrapResponse> {
        use rand::Rng;

        let mut handles = Vec::new();
        let timeout = self.config.bootstrap_timeout;

        for seed in &seed_list.peers {
            if self.blacklist.is_slashed(&seed.peer_id) {
                continue;
            }

            let addr = match parse_multiaddr(&seed.multiaddr) {
                Some(a) => a,
                None => continue,
            };

            let nonce: [u8; 16] = rand::thread_rng().gen();
            let req = BootstrapRequest {
                requester_id: self.config.node_id,
                requester_pubkey: self.config.node_pubkey,
                nonce,
                epoch: self.config.current_epoch,
                capability_filter: 0xFFFF,
                max_peers: MAX_PEER_LIST,
            };

            let node_id = self.config.node_id;
            handles.push(tokio::spawn(async move {
                connect_and_collect(addr, &req, node_id, timeout).await
            }));
        }

        let mut responses = Vec::new();
        for handle in handles {
            if let Ok(Some(resp)) = handle.await {
                responses.push(resp);
                if responses.len() >= self.config.min_responses {
                    break;
                }
            }
        }

        responses
    }

    /// Run validation and collect bootstrap responses via direct TCP.
    /// Returns the collected `BootstrapResponse` entries (up to
    /// `max_responses`). This is a simplified entry point that does
    /// not require `TransportDiscovery` — callers extract peer entries
    /// from the responses and add them directly.
    pub async fn discover_peers(
        &mut self,
        transport: &NodeTransport,
        max_responses: usize,
    ) -> Result<Vec<BootstrapResponse>, BootstrapError> {
        // Step 1: Filter slashed seeds
        let filtered = self.blacklist.filter(self.seed_list.clone());
        if filtered.peers.is_empty() {
            self.state = BootstrapClientLifecycle::Failed;
            return Err(BootstrapError::NoResponses);
        }

        // Step 2: Seed health check
        let health = SeedHealth::check(&filtered, self.config.current_epoch);
        if health.refuses_start() {
            self.state = BootstrapClientLifecycle::Failed;
            return Err(BootstrapError::SeedListStale);
        }

        // Step 3: Authority verification
        match octo_network::mon::bootstrap::verify_authority(
            self.config.authority,
            self.config.current_epoch,
        ) {
            Ok(()) => {}
            Err(e) => {
                self.state = BootstrapClientLifecycle::Failed;
                return Err(BootstrapError::AuthorityError(e));
            }
        }

        // Step 4-6: Send BOOTSTRAP_REQ, collect responses
        self.state = BootstrapClientLifecycle::Connecting;
        let mut attempt = 0u32;

        while attempt < self.config.max_retries {
            let responses = self.send_bootstrap_requests(transport, &filtered).await;

            if !responses.is_empty() {
                self.state = BootstrapClientLifecycle::Validating;
                self.state = BootstrapClientLifecycle::Cached;
                let truncated: Vec<BootstrapResponse> =
                    responses.into_iter().take(max_responses).collect();
                return Ok(truncated);
            }

            attempt += 1;
            if attempt < self.config.max_retries {
                let backoff = self
                    .config
                    .initial_backoff
                    .saturating_mul(2u32.saturating_pow(attempt - 1));
                let backoff = backoff.min(MAX_BACKOFF);
                tokio::time::sleep(backoff).await;
            }
        }

        self.state = BootstrapClientLifecycle::Failed;
        Err(BootstrapError::NoResponses)
    }
}

// ── Direct TCP helpers ────────────────────────────────────────────

/// Parse a multiaddr string like `/ip4/1.2.3.4/tcp/4001/p2p/...`
/// into a `SocketAddr`. Only the `/ip4/.../tcp/...` prefix is used;
/// the `/p2p/...` suffix is ignored (it's the peer ID, which we
/// already have from the seed entry).
fn parse_multiaddr(multiaddr: &str) -> Option<std::net::SocketAddr> {
    let mut ip = None;
    let mut port = None;
    let components: Vec<&str> = multiaddr.split('/').filter(|s| !s.is_empty()).collect();
    for (i, component) in components.iter().enumerate() {
        if *component == "ip4" {
            ip = components.get(i + 1).copied();
        } else if *component == "tcp" {
            port = components.get(i + 1).and_then(|p| p.parse::<u16>().ok());
        }
    }
    match (ip, port) {
        (Some(ip_str), Some(port)) => {
            let ip: std::net::IpAddr = ip_str.parse().ok()?;
            Some(std::net::SocketAddr::new(ip, port))
        }
        _ => None,
    }
}

/// Connect to a single bootstrap node, send a `BootstrapRequest`,
/// and read the `BootstrapResponse`. Returns `None` on any error
/// or timeout.
async fn connect_and_collect(
    addr: std::net::SocketAddr,
    req: &BootstrapRequest,
    _expected_requester_id: [u8; 32],
    timeout: Duration,
) -> Option<BootstrapResponse> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addr))
        .await
        .ok()?
        .ok()?;

    let payload = serde_json::to_vec(req).ok()?;

    // Write length-prefixed frame: [4-byte len][json bytes]
    let len = (payload.len() as u32).to_be_bytes();
    tokio::time::timeout(timeout, stream.write_all(&len))
        .await
        .ok()?
        .ok()?;
    tokio::time::timeout(timeout, stream.write_all(&payload))
        .await
        .ok()?
        .ok()?;

    // Read response: [4-byte len][json bytes]
    let mut len_buf = [0u8; 4];
    tokio::time::timeout(timeout, stream.read_exact(&mut len_buf))
        .await
        .ok()?
        .ok()?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    if resp_len > 16 * 1024 * 1024 {
        return None; // sanity check
    }
    let mut resp_buf = vec![0u8; resp_len];
    tokio::time::timeout(timeout, stream.read_exact(&mut resp_buf))
        .await
        .ok()?
        .ok()?;

    let resp: BootstrapResponse = serde_json::from_slice(&resp_buf).ok()?;

    // Verify the response is for us
    if resp.requester_id != req.requester_id {
        return None;
    }

    Some(resp)
}

impl BootstrapOrchestrator {
    /// Populate the discovery cache with bootstrapped peers.
    fn populate_discovery(
        &self,
        peer_ids: &[[u8; 32]],
        discovery: &TransportDiscovery,
        discovery_state: &mut DiscoveryState,
    ) -> u32 {
        let current_epoch = self.config.current_epoch;

        for peer_id in peer_ids {
            let entry = GatewayCacheEntry {
                advertisement_hash: blake3::hash(peer_id).into(),
                first_seen: current_epoch,
                last_seen: current_epoch,
                trust_score: 500, // Default trust for bootstrapped peers
                identity: octo_network::dot::gateway::GatewayIdentity {
                    gateway_id: *peer_id,
                    public_key: *peer_id,
                    network_id: 1,
                    gateway_class: octo_network::dot::gateway::GatewayClass::Edge,
                    creation_epoch: current_epoch,
                    supported_platforms: 0,
                    capabilities: 0,
                },
                capabilities: vec![],
                endpoints: vec![],
            };
            discovery.cache_insert(entry, current_epoch);
        }

        let count = peer_ids.len() as u32;
        discovery_state.peer_count += count;

        // Attempt transition to Expansion if >= 5 peers
        if discovery_state.peer_count >= 5 && discovery_state.phase == DiscoveryLifecycle::Bootstrap
        {
            let _ = discovery_state.start_expansion();
        }

        count
    }
}

/// Compute the intersection of multiple peer sets.
///
/// Returns peer IDs that appear in ALL sets (unanimous agreement),
/// sorted deterministically. For the Sybil defense threshold
/// (RFC-0851p-a §6), the intersection must represent ≥80% of the
/// largest set.
fn compute_intersection(sets: &[Vec<[u8; 32]>]) -> Vec<[u8; 32]> {
    if sets.is_empty() {
        return Vec::new();
    }
    if sets.len() == 1 {
        return sets[0].clone();
    }

    // Build a frequency map (deduplicate within each set first)
    let mut freq: std::collections::BTreeMap<[u8; 32], usize> = std::collections::BTreeMap::new();
    for set in sets {
        let unique: std::collections::HashSet<[u8; 32]> = set.iter().copied().collect();
        for peer in unique {
            *freq.entry(peer).or_insert(0) += 1;
        }
    }

    let n = sets.len();
    freq.into_iter()
        .filter(|(_, count)| *count == n)
        .map(|(peer, _)| peer)
        .collect()
}

// ── Test-only public API (for E2E tests in sync-e2e-tests) ───────

/// Expose `compute_intersection` for E2E tests.
#[doc(hidden)]
pub fn compute_intersection_for_test(sets: &[Vec<[u8; 32]>]) -> Vec<[u8; 32]> {
    compute_intersection(sets)
}

impl BootstrapOrchestrator {
    /// Expose `populate_discovery` for E2E tests.
    #[doc(hidden)]
    pub fn populate_discovery_for_test(
        &self,
        peer_ids: &[[u8; 32]],
        discovery: &TransportDiscovery,
        discovery_state: &mut DiscoveryState,
    ) -> u32 {
        self.populate_discovery(peer_ids, discovery, discovery_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_transport::NodeTransport;
    use crate::sender::{NetworkSender, SendContext, TransportError};
    use async_trait::async_trait;
    use octo_network::gdp::discovery::BootstrapMethod;
    use octo_network::gdp::identity::GdpGatewayIdentity;
    use std::sync::Arc;

    fn make_seed_entry(peer: &str, epoch: u64) -> octo_network::mon::bootstrap::SeedEntry {
        octo_network::mon::bootstrap::SeedEntry {
            peer_id: peer.into(),
            multiaddr: format!("/ip4/1.2.3.4/tcp/4001/p2p/{peer}"),
            signed_at_epoch: epoch,
        }
    }

    fn make_envelope(peers: Vec<octo_network::mon::bootstrap::SeedEntry>) -> SeedListEnvelope {
        SeedListEnvelope {
            authority_pubkey: vec![0u8; 32],
            signed_at_epoch: 0,
            peers,
        }
    }

    struct MockSender {
        name: String,
        healthy: bool,
    }

    #[async_trait]
    impl NetworkSender for MockSender {
        async fn send(&self, _p: &[u8], _c: &SendContext) -> Result<(), TransportError> {
            if self.healthy {
                Ok(())
            } else {
                Err(TransportError::AdapterFailure("mock".into()))
            }
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn is_healthy(&self) -> bool {
            self.healthy
        }
    }

    fn make_transport() -> NodeTransport {
        NodeTransport::new(vec![Arc::new(MockSender {
            name: "mock".into(),
            healthy: true,
        }) as Arc<dyn NetworkSender>])
    }

    fn make_discovery() -> (TransportDiscovery, DiscoveryState) {
        let identity = GdpGatewayIdentity::new(octo_network::dot::gateway::GatewayIdentity::new(
            [0x42u8; 32],
            1,
            octo_network::dot::gateway::GatewayClass::Edge,
            100,
        ));
        let disc = TransportDiscovery::new(identity, [0xABu8; 32], 256);
        let state = DiscoveryState::new(BootstrapMethod::Static);
        (disc, state)
    }

    fn make_config() -> BootstrapConfig {
        BootstrapConfig {
            node_id: [0x42u8; 32],
            node_pubkey: [0x43u8; 32],
            ..BootstrapConfig::default()
        }
    }

    // ── Health check tests ────────────────────────────────────────

    #[test]
    fn fresh_seeds_pass_health_check() {
        let env = make_envelope(vec![make_seed_entry("a", 100), make_seed_entry("b", 100)]);
        let health = SeedHealth::check(&env, 105);
        assert!(matches!(health, SeedHealth::Fresh { fresh_count: 2 }));
        assert!(!health.refuses_start());
    }

    #[test]
    fn fully_stale_refuses() {
        let env = make_envelope(vec![make_seed_entry("a", 50), make_seed_entry("b", 50)]);
        let health = SeedHealth::check(&env, 105);
        assert!(health.refuses_start());
    }

    // ── Authority tests ───────────────────────────────────────────

    #[test]
    fn authority_foundation_accepted_before_fork() {
        let result =
            octo_network::mon::bootstrap::verify_authority(SeedListAuthority::Foundation, 0);
        assert!(result.is_ok());
    }

    // ── Blacklist tests ───────────────────────────────────────────

    #[test]
    fn slashed_seeds_filtered() {
        let mut blacklist = SlashedSeedBlacklist::new();
        blacklist.slash("evil");
        let env = make_envelope(vec![
            make_seed_entry("good", 100),
            make_seed_entry("evil", 100),
        ]);
        let filtered = blacklist.filter(env);
        assert_eq!(filtered.peers.len(), 1);
        assert_eq!(filtered.peers[0].peer_id, "good");
    }

    // ── Intersection tests ────────────────────────────────────────

    #[test]
    fn intersection_unanimous() {
        let sets = vec![
            vec![[1u8; 32], [2u8; 32], [3u8; 32]],
            vec![[1u8; 32], [2u8; 32], [3u8; 32]],
            vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        ];
        let result = compute_intersection(&sets);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn intersection_partial_agreement() {
        let sets = vec![
            vec![[1u8; 32], [2u8; 32], [3u8; 32]],
            vec![[1u8; 32], [2u8; 32], [4u8; 32]], // 3→4
            vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        ];
        let result = compute_intersection(&sets);
        // Only [1] and [2] are in all 3
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn intersection_empty_sets() {
        let result = compute_intersection(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn intersection_single_set() {
        let sets = vec![vec![[1u8; 32], [2u8; 32]]];
        let result = compute_intersection(&sets);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn intersection_deterministic_order() {
        // BTreeMap ensures sorted output
        let sets = vec![
            vec![[3u8; 32], [1u8; 32], [2u8; 32]],
            vec![[1u8; 32], [3u8; 32], [2u8; 32]],
        ];
        let result = compute_intersection(&sets);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], [1u8; 32]);
        assert_eq!(result[1], [2u8; 32]);
        assert_eq!(result[2], [3u8; 32]);
    }

    // ── Config / lifecycle tests ──────────────────────────────────

    #[test]
    fn lifecycle_state_transitions() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = make_config();
        let orch = BootstrapOrchestrator::new(env, config);
        assert_eq!(orch.state(), BootstrapClientLifecycle::Init);
    }

    #[test]
    fn config_defaults() {
        let config = BootstrapConfig::default();
        assert_eq!(config.bootstrap_timeout, Duration::from_secs(60));
        assert_eq!(config.min_responses, 3);
        assert_eq!(config.intersection_threshold, 0.80);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.authority, SeedListAuthority::Foundation);
    }

    #[test]
    fn config_node_identity_fields() {
        let config = make_config();
        assert_eq!(config.node_id, [0x42u8; 32]);
        assert_eq!(config.node_pubkey, [0x43u8; 32]);
    }

    // ── run() failure path tests ──────────────────────────────────

    #[tokio::test]
    async fn run_fails_with_empty_seed_list() {
        let env = make_envelope(vec![]);
        let config = make_config();
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(result.is_err());
        assert_eq!(orch.state(), BootstrapClientLifecycle::Failed);
    }

    #[tokio::test]
    async fn run_fails_with_stale_seeds() {
        let env = make_envelope(vec![make_seed_entry("a", 50), make_seed_entry("b", 50)]);
        let config = BootstrapConfig {
            current_epoch: 105,
            ..make_config()
        };
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(matches!(result, Err(BootstrapError::SeedListStale)));
        assert_eq!(orch.state(), BootstrapClientLifecycle::Failed);
    }

    #[tokio::test]
    async fn run_fails_with_wrong_authority() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = BootstrapConfig {
            authority: SeedListAuthority::Dao,
            current_epoch: 0, // Before DAO is active
            ..make_config()
        };
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(matches!(result, Err(BootstrapError::AuthorityError(_))));
    }

    #[tokio::test]
    async fn run_with_all_slashed_fails() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let mut blacklist = SlashedSeedBlacklist::new();
        blacklist.slash("a");
        let config = make_config();
        let mut orch = BootstrapOrchestrator::with_blacklist(env, blacklist, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(matches!(result, Err(BootstrapError::NoResponses)));
    }

    #[tokio::test]
    async fn run_no_responses_when_stub_returns_empty() {
        // send_bootstrap_requests is a stub that returns empty.
        // With min_responses=1, run() exhausts retries and fails.
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = BootstrapConfig {
            min_responses: 1,
            max_retries: 1, // Fast fail
            ..make_config()
        };
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(matches!(result, Err(BootstrapError::NoResponses)));
        assert_eq!(orch.state(), BootstrapClientLifecycle::Failed);
    }

    // ── Sybil defense tests ───────────────────────────────────────

    #[test]
    fn sybil_detection_3_of_5_colluding() {
        let honest = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]];
        let sybil = vec![[5u8; 32], [6u8; 32], [7u8; 32], [8u8; 32]];

        let sets = vec![
            honest.clone(),
            honest.clone(),
            sybil.clone(),
            sybil.clone(),
            sybil.clone(),
        ];

        let intersection = compute_intersection(&sets);
        assert!(intersection.is_empty());
    }

    #[test]
    fn low_confidence_2_of_5() {
        let peers = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32], [5u8; 32]];
        let mut peers2 = peers.clone();
        peers2[4] = [6u8; 32]; // 80% overlap (4/5)

        let sets = vec![peers, peers2];
        let intersection = compute_intersection(&sets);
        assert_eq!(intersection.len(), 4);

        let max_peers = 5;
        let agreement = intersection.len() as f64 / max_peers as f64;
        assert!(agreement >= 0.80);
    }

    // ── Populate discovery tests ──────────────────────────────────

    #[test]
    fn populate_discovery_adds_to_cache() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = make_config();
        let orch = BootstrapOrchestrator::new(env, config);
        let (discovery, mut state) = make_discovery();

        let peer_ids = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let count = orch.populate_discovery(&peer_ids, &discovery, &mut state);
        assert_eq!(count, 3);
        assert_eq!(discovery.peer_count(), 3);
        assert_eq!(state.peer_count, 3);
    }

    #[test]
    fn populate_discovery_transitions_to_expansion_at_5() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = make_config();
        let orch = BootstrapOrchestrator::new(env, config);
        let (discovery, mut state) = make_discovery();

        let peer_ids: Vec<[u8; 32]> = (0..5).map(|i| [i as u8; 32]).collect();
        orch.populate_discovery(&peer_ids, &discovery, &mut state);
        assert_eq!(state.phase, DiscoveryLifecycle::Expansion);
    }

    #[test]
    fn populate_discovery_stays_bootstrap_below_5() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = make_config();
        let orch = BootstrapOrchestrator::new(env, config);
        let (discovery, mut state) = make_discovery();

        let peer_ids: Vec<[u8; 32]> = (0..3).map(|i| [i as u8; 32]).collect();
        orch.populate_discovery(&peer_ids, &discovery, &mut state);
        assert_eq!(state.phase, DiscoveryLifecycle::Bootstrap);
    }

    // ── parse_multiaddr tests ────────────────────────────────────

    #[test]
    fn parse_multiaddr_standard() {
        let addr = parse_multiaddr("/ip4/127.0.0.1/tcp/4001/p2p/QmTest");
        assert_eq!(addr, Some("127.0.0.1:4001".parse().unwrap()));
    }

    #[test]
    fn parse_multiaddr_localhost() {
        let addr = parse_multiaddr("/ip4/0.0.0.0/tcp/9100");
        assert_eq!(addr, Some("0.0.0.0:9100".parse().unwrap()));
    }

    #[test]
    fn parse_multiaddr_no_tcp() {
        let addr = parse_multiaddr("/ip4/1.2.3.4");
        assert!(addr.is_none());
    }

    #[test]
    fn parse_multiaddr_no_ip() {
        let addr = parse_multiaddr("/tcp/4001");
        assert!(addr.is_none());
    }

    #[test]
    fn parse_multiaddr_invalid_ip() {
        let addr = parse_multiaddr("/ip4/not-an-ip/tcp/4001");
        assert!(addr.is_none());
    }

    #[test]
    fn parse_multiaddr_invalid_port() {
        let addr = parse_multiaddr("/ip4/1.2.3.4/tcp/not-a-port");
        assert!(addr.is_none());
    }

    // ── connect_and_collect tests ────────────────────────────────

    #[tokio::test]
    async fn connect_and_collect_happy_path() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Start a mock bootstrap server
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            // Read request: [4-byte len][json]
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await.unwrap();
            let req_len = u32::from_be_bytes(len_buf) as usize;
            let mut req_buf = vec![0u8; req_len];
            stream.read_exact(&mut req_buf).await.unwrap();
            let _req: BootstrapRequest = serde_json::from_slice(&req_buf).unwrap();

            // Send response
            let resp = BootstrapResponse {
                requester_id: [0x42u8; 32],
                request_nonce: [0u8; 16],
                epoch: 0,
                responder_id: [0x99u8; 32],
                peer_entries: vec![
                    BootstrapPeerEntry {
                        peer_id: [1u8; 32],
                        multiaddr: "/ip4/10.0.0.1/tcp/4001".into(),
                    },
                    BootstrapPeerEntry {
                        peer_id: [2u8; 32],
                        multiaddr: "/ip4/10.0.0.2/tcp/4002".into(),
                    },
                ],
            };
            let resp_bytes = serde_json::to_vec(&resp).unwrap();
            let len = (resp_bytes.len() as u32).to_be_bytes();
            stream.write_all(&len).await.unwrap();
            stream.write_all(&resp_bytes).await.unwrap();
        });

        let req = BootstrapRequest {
            requester_id: [0x42u8; 32],
            requester_pubkey: [0x43u8; 32],
            nonce: [0u8; 16],
            epoch: 0,
            capability_filter: 0xFFFF,
            max_peers: 256,
        };

        let resp = connect_and_collect(addr, &req, [0x42u8; 32], Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(resp.requester_id, [0x42u8; 32]);
        assert_eq!(resp.peer_entries.len(), 2);
        assert_eq!(resp.peer_entries[0].peer_id, [1u8; 32]);
    }

    #[tokio::test]
    async fn connect_and_collect_timeout() {
        // No server — should time out
        let addr = "127.0.0.1:1".parse().unwrap();
        let req = BootstrapRequest {
            requester_id: [0x42u8; 32],
            requester_pubkey: [0x43u8; 32],
            nonce: [0u8; 16],
            epoch: 0,
            capability_filter: 0xFFFF,
            max_peers: 256,
        };

        let result = connect_and_collect(addr, &req, [0x42u8; 32], Duration::from_millis(50)).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn connect_and_collect_wrong_requester_id() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await.unwrap();
            let req_len = u32::from_be_bytes(len_buf) as usize;
            let mut req_buf = vec![0u8; req_len];
            stream.read_exact(&mut req_buf).await.unwrap();

            // Response with wrong requester_id
            let resp = BootstrapResponse {
                requester_id: [0xFFu8; 32], // wrong!
                request_nonce: [0u8; 16],
                epoch: 0,
                responder_id: [0x99u8; 32],
                peer_entries: vec![],
            };
            let resp_bytes = serde_json::to_vec(&resp).unwrap();
            let len = (resp_bytes.len() as u32).to_be_bytes();
            stream.write_all(&len).await.unwrap();
            stream.write_all(&resp_bytes).await.unwrap();
        });

        let req = BootstrapRequest {
            requester_id: [0x42u8; 32],
            requester_pubkey: [0x43u8; 32],
            nonce: [0u8; 16],
            epoch: 0,
            capability_filter: 0xFFFF,
            max_peers: 256,
        };

        let result = connect_and_collect(addr, &req, [0x42u8; 32], Duration::from_secs(5)).await;
        assert!(result.is_none());
    }

    // ── discover_peers tests ─────────────────────────────────────

    #[tokio::test]
    async fn discover_peers_falls_back_on_no_bootstrap() {
        // No running bootstrap nodes → falls back to NoResponses
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = BootstrapConfig {
            min_responses: 0,
            max_retries: 1,
            bootstrap_timeout: Duration::from_millis(50),
            ..make_config()
        };
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();

        let result = orch.discover_peers(&transport, 256).await;
        // Should fail because the seed multiaddr doesn't point to a real server
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn discover_peers_collects_from_mock_server() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Start a mock bootstrap server
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let accept = listener.accept().await;
                if let Ok((mut stream, _)) = accept {
                    tokio::spawn(async move {
                        let mut len_buf = [0u8; 4];
                        if stream.read_exact(&mut len_buf).await.is_err() {
                            return;
                        }
                        let req_len = u32::from_be_bytes(len_buf) as usize;
                        let mut req_buf = vec![0u8; req_len];
                        if stream.read_exact(&mut req_buf).await.is_err() {
                            return;
                        }

                        let resp = BootstrapResponse {
                            requester_id: [0x42u8; 32],
                            request_nonce: [0u8; 16],
                            epoch: 0,
                            responder_id: [0x99u8; 32],
                            peer_entries: vec![BootstrapPeerEntry {
                                peer_id: [1u8; 32],
                                multiaddr: "/ip4/10.0.0.1/tcp/4001".into(),
                            }],
                        };
                        let resp_bytes = serde_json::to_vec(&resp).unwrap();
                        let len = (resp_bytes.len() as u32).to_be_bytes();
                        let _ = stream.write_all(&len).await;
                        let _ = stream.write_all(&resp_bytes).await;
                    });
                }
            }
        });

        let multiaddr = format!("/ip4/127.0.0.1/tcp/{}/p2p/test", addr.port());
        let seed = octo_network::mon::bootstrap::SeedEntry {
            peer_id: "test-bootstrap".into(),
            multiaddr,
            signed_at_epoch: 100,
        };
        let env = make_envelope(vec![seed]);
        let config = BootstrapConfig {
            node_id: [0x42u8; 32],
            node_pubkey: [0x43u8; 32],
            min_responses: 1,
            max_retries: 2,
            bootstrap_timeout: Duration::from_secs(2),
            ..BootstrapConfig::default()
        };
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();

        let result = orch.discover_peers(&transport, 256).await;
        assert!(result.is_ok());
        let responses = result.unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].peer_entries.len(), 1);
        assert_eq!(responses[0].peer_entries[0].peer_id, [1u8; 32]);
    }
}
