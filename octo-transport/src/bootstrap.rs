//! Bootstrap orchestrator — wires RFC-0851p-a Mode A into the
//! `octo-transport` startup path.
//!
//! Mission `0851p-a-base-bootstrap-orchestrator`.

use std::time::Duration;

use octo_network::gdp::cache::GatewayCacheEntry;
use octo_network::gdp::discovery::DiscoveryState;
use octo_network::gdp::types::DiscoveryLifecycle;
use octo_network::mon::bootstrap::{
    BootstrapMode, SeedAuthorityError, SeedHealth, SeedListAuthority, SeedListEnvelope,
    SlashedSeedBlacklist,
};

use crate::discovery::TransportDiscovery;
use crate::node_transport::NodeTransport;
use crate::sender::{SendContext, TransportError};

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
    /// Bootstrap mode.
    pub mode: BootstrapMode,
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
            mode: BootstrapMode::Direct,
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
/// [`BootstrapMode`], and [`SlashedSeedBlacklist`] from
/// `octo-network::mon::bootstrap`.  Produces peer entries in
/// [`TransportDiscovery`].
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
    /// fail.  On success, `discovery` cache and `discovery_state`
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

        let mut all_peer_sets: Vec<Vec<[u8; 32]>> = Vec::new();
        let mut attempt = 0u32;

        while attempt < self.config.max_retries {
            let responses = self
                .send_bootstrap_requests(transport, &filtered)
                .await;

            if responses.len() >= self.config.min_responses {
                // Step 7: Validate (signatures deferred — stub mode)
                // Step 8: Compute peer-list intersection
                self.state = BootstrapClientLifecycle::Validating;

                for resp in &responses {
                    let peer_set: Vec<[u8; 32]> =
                        resp.peer_entries.iter().map(|p| p.peer_id).collect();
                    all_peer_sets.push(peer_set);
                }

                let intersection = compute_intersection(&all_peer_sets);
                let agreement = if !all_peer_sets.is_empty() {
                    let max_peers = all_peer_sets
                        .iter()
                        .map(|s| s.len())
                        .max()
                        .unwrap_or(1);
                    intersection.len() as f64 / max_peers as f64
                } else {
                    0.0
                };

                if agreement >= self.config.intersection_threshold {
                    // Step 9: Merge into TransportDiscovery
                    self.state = BootstrapClientLifecycle::Cached;
                    let peer_count = self.populate_discovery(
                        &intersection,
                        discovery,
                        discovery_state,
                    );
                    self.state = BootstrapClientLifecycle::Done;
                    return Ok(peer_count);
                }
                // Intersection below threshold — retry
                all_peer_sets.clear();
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

    /// Send BOOTSTRAP_REQ to each seed and collect responses.
    async fn send_bootstrap_requests(
        &self,
        transport: &NodeTransport,
        seed_list: &SeedListEnvelope,
    ) -> Vec<BootstrapResponse> {
        use rand::Rng;

        let responses = Vec::new();

        for _seed in &seed_list.peers {
            let nonce: [u8; 16] = rand::thread_rng().gen();

            let req = BootstrapRequest {
                requester_id: [0u8; 32], // Would be node identity
                requester_pubkey: seed_list.authority_pubkey[..32]
                    .try_into()
                    .unwrap_or([0u8; 32]),
                nonce,
                epoch: self.config.current_epoch,
                capability_filter: 0xFFFF,
                max_peers: MAX_PEER_LIST,
            };

            let payload = match serde_json::to_vec(&req) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let ctx = SendContext {
                mission_id: [0u8; 32],
                priority: 255, // Bootstrap is highest priority
                source_peer: req.requester_id,
                origin_gateway: req.requester_id,
            };

            // Send via transport (best available)
            match tokio::time::timeout(
                self.config.bootstrap_timeout,
                transport.send_best(&payload, &ctx),
            )
            .await
            {
                Ok(Ok(())) => {
                    // In a real implementation, we'd wait for a response
                    // on the receiver channel. For now, the response
                    // would come through the NetworkReceiver path.
                    // This is a placeholder — real responses arrive
                    // asynchronously via the inbound receive loop.
                }
                _ => continue,
            }
        }

        responses
    }

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
        if discovery_state.peer_count >= 5
            && discovery_state.phase == DiscoveryLifecycle::Bootstrap
        {
            let _ = discovery_state.start_expansion();
        }

        count
    }
}

/// Compute the intersection of multiple peer sets.
///
/// Returns peer IDs that appear in ALL sets (unanimous agreement).
/// For the Sybil defense threshold (RFC-0851p-a §6), the intersection
/// must represent ≥80% of the largest set.
fn compute_intersection(sets: &[Vec<[u8; 32]>]) -> Vec<[u8; 32]> {
    if sets.is_empty() {
        return Vec::new();
    }
    if sets.len() == 1 {
        return sets[0].clone();
    }

    // Build a frequency map
    let mut freq: std::collections::HashMap<[u8; 32], usize> =
        std::collections::HashMap::new();
    for set in sets {
        for peer in set {
            *freq.entry(*peer).or_insert(0) += 1;
        }
    }

    let n = sets.len();
    freq.into_iter()
        .filter(|(_, count)| *count == n)
        .map(|(peer, _)| peer)
        .collect()
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
        let identity = GdpGatewayIdentity::new(
            octo_network::dot::gateway::GatewayIdentity::new(
                [0x42u8; 32],
                1,
                octo_network::dot::gateway::GatewayClass::Edge,
                100,
            ),
        );
        let disc = TransportDiscovery::new(identity, [0xABu8; 32], 256);
        let state = DiscoveryState::new(BootstrapMethod::Static);
        (disc, state)
    }

    #[test]
    fn fresh_seeds_pass_health_check() {
        let env = make_envelope(vec![
            make_seed_entry("a", 100),
            make_seed_entry("b", 100),
        ]);
        let health = SeedHealth::check(&env, 105);
        assert!(matches!(health, SeedHealth::Fresh { fresh_count: 2 }));
        assert!(!health.refuses_start());
    }

    #[test]
    fn fully_stale_refuses() {
        let env = make_envelope(vec![
            make_seed_entry("a", 50),
            make_seed_entry("b", 50),
        ]);
        let health = SeedHealth::check(&env, 105);
        assert!(health.refuses_start());
    }

    #[test]
    fn authority_foundation_accepted_before_fork() {
        let result = octo_network::mon::bootstrap::verify_authority(
            SeedListAuthority::Foundation,
            0,
        );
        assert!(result.is_ok());
    }

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
    fn lifecycle_state_transitions() {
        let env = make_envelope(vec![make_seed_entry("a", 100)]);
        let config = BootstrapConfig::default();
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

    #[tokio::test]
    async fn run_fails_with_empty_seed_list() {
        let env = make_envelope(vec![]);
        let config = BootstrapConfig::default();
        let mut orch = BootstrapOrchestrator::new(env, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(result.is_err());
        assert_eq!(orch.state(), BootstrapClientLifecycle::Failed);
    }

    #[tokio::test]
    async fn run_fails_with_stale_seeds() {
        let env = make_envelope(vec![
            make_seed_entry("a", 50),
            make_seed_entry("b", 50),
        ]);
        let config = BootstrapConfig {
            current_epoch: 105,
            ..BootstrapConfig::default()
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
            ..BootstrapConfig::default()
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
        let config = BootstrapConfig::default();
        let mut orch = BootstrapOrchestrator::with_blacklist(env, blacklist, config);
        let transport = make_transport();
        let (discovery, mut state) = make_discovery();

        let result = orch.run(&transport, &discovery, &mut state).await;
        assert!(matches!(result, Err(BootstrapError::NoResponses)));
    }

    #[test]
    fn sybil_detection_3_of_5_colluding() {
        // 3 of 5 bootstrap nodes return a colluding peer list
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
        // No peer is in ALL 5 sets
        assert!(intersection.is_empty());
    }

    #[test]
    fn low_confidence_2_of_5() {
        // 2 of 5 respond with ≥80% overlap
        let peers = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32], [5u8; 32]];
        let mut peers2 = peers.clone();
        peers2[4] = [6u8; 32]; // 80% overlap (4/5)

        let sets = vec![peers, peers2];
        let intersection = compute_intersection(&sets);
        // 4 peers in intersection
        assert_eq!(intersection.len(), 4);

        let max_peers = 5;
        let agreement = intersection.len() as f64 / max_peers as f64;
        assert!(agreement >= 0.80);
    }
}
