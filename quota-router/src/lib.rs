pub mod announce;
pub mod forward;
pub mod gossip;
pub mod handler;
pub mod metrics;
pub mod provider;
pub mod ratelimit;
pub mod request;
pub mod scorer;

use std::sync::Arc;

use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext};

use announce::{RouterAnnouncePayload, SignedPayload};
use forward::{ForwardOutcome, ForwardRequestPayload, PendingRequests};
use gossip::{monotonic_now, CapacityGossipPayload, GossipCache};
use provider::{
    LocalProvider, LocalProviderSender, NetworkId, PeerConfig, PeerTrust, ProviderCapacity,
    ProviderConfig, ProviderError, ProviderId, RouterNodeId,
};
use request::{ForwardingConfig, RequestContext, RoutingPolicy};
use scorer::{select_destinations, Destination};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouterNodeLifecycle {
    Init = 0x00,
    Bootstrapping = 0x01,
    Discovering = 0x02,
    Active = 0x03,
    Degraded = 0x04,
    Draining = 0x05,
    Terminated = 0x06,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RouterNodeConfig {
    pub node_id: RouterNodeId,
    pub network_id: NetworkId,
    pub providers: Vec<ProviderConfig>,
    pub peers: Vec<PeerConfig>,
    pub policy: RoutingPolicy,
    pub forwarding: ForwardingConfig,
    pub gossip_interval: std::time::Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterNodeError {
    #[error("node_id is required")]
    MissingNodeId,
    #[error("network_id is required")]
    MissingNetworkId,
    #[error("no providers configured")]
    NoProviders,
    #[error("no destination supports request")]
    NoProvider,
    #[error("forwarded request was rejected: {0:?}")]
    ForwardRejected(forward::ForwardRejectReason),
    #[error("forwarded request timed out")]
    ForwardTimeout,
    #[error("rate limit exceeded for consumer")]
    RateLimited,
    #[error("provider dispatch failed: {0}")]
    Provider(#[from] ProviderError),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
}

pub struct QuotaRouterNode {
    pub config: RouterNodeConfig,
    pub state: RouterNodeLifecycle,
    pub transport: NodeTransport,
    pub gossip_cache: GossipCache,
    pub peer_cache: PeerCache,
    pub(crate) pending: PendingRequests,
    pub identity_key: [u8; 32],
    #[allow(dead_code)]
    primary_provider: Arc<dyn LocalProvider>,
    pub rate_limiter: ratelimit::RateLimiter,
    pub metrics: Option<metrics::QuotaRouterMetrics>,
}

pub struct PeerCache {
    direct: std::collections::BTreeMap<RouterNodeId, PeerInfo>,
    discovered: std::collections::BTreeMap<RouterNodeId, PeerInfo>,
    max_peers: usize,
}

pub struct PeerInfo {
    pub node_id: RouterNodeId,
    pub trust_level: PeerTrust,
    pub discovered: bool,
    pub last_seen: u64,
}

impl Default for PeerCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerCache {
    pub fn new() -> Self {
        Self::with_max_peers(128)
    }

    pub fn with_max_peers(max_peers: usize) -> Self {
        Self {
            direct: std::collections::BTreeMap::new(),
            discovered: std::collections::BTreeMap::new(),
            max_peers,
        }
    }

    pub fn max_peers(&self) -> usize {
        self.max_peers
    }

    pub fn add_direct(&mut self, node_id: RouterNodeId, _capacities: Vec<ProviderCapacity>) {
        self.direct.insert(
            node_id,
            PeerInfo {
                node_id,
                trust_level: PeerTrust::Verified,
                discovered: false,
                last_seen: monotonic_now(),
            },
        );
    }

    pub fn try_add(&mut self, node_id: RouterNodeId) {
        if !self.direct.contains_key(&node_id) && !self.discovered.contains_key(&node_id) {
            if self.total() >= self.max_peers {
                if let Some(oldest) = self
                    .discovered
                    .iter()
                    .min_by_key(|(_, info)| info.last_seen)
                    .map(|(k, _)| *k)
                {
                    self.discovered.remove(&oldest);
                }
            }
            self.discovered.insert(
                node_id,
                PeerInfo {
                    node_id,
                    trust_level: PeerTrust::Untrusted,
                    discovered: true,
                    last_seen: monotonic_now(),
                },
            );
        }
    }

    pub fn remove(&mut self, node_id: RouterNodeId) {
        self.direct.remove(&node_id);
        self.discovered.remove(&node_id);
    }

    pub fn total(&self) -> usize {
        self.direct.len() + self.discovered.len()
    }

    pub fn direct_ids(&self) -> Vec<RouterNodeId> {
        self.direct.keys().copied().collect()
    }
}

impl QuotaRouterNode {
    pub fn builder() -> QuotaRouterNodeBuilder {
        QuotaRouterNodeBuilder::default()
    }

    pub fn peer_count(&self) -> usize {
        self.peer_cache.total()
    }

    pub fn local_provider_models(&self) -> Vec<String> {
        self.config
            .providers
            .iter()
            .flat_map(|p| p.models.iter().cloned())
            .collect()
    }

    pub fn add_peer(&mut self, peer: PeerConfig) {
        self.peer_cache.add_direct(peer.node_id, vec![]);
        self.config.peers.push(peer);
    }

    pub fn select_destinations(
        &self,
        request: &RequestContext,
        local_providers: &[ProviderCapacity],
        peer_capabilities: &[(RouterNodeId, Vec<ProviderCapacity>)],
        policy: &RoutingPolicy,
    ) -> Vec<Destination> {
        select_destinations(request, local_providers, peer_capabilities, policy)
    }

    pub fn pending_origin(&self, request_id: [u8; 32]) -> Option<RouterNodeId> {
        self.pending.origin(request_id)
    }

    pub fn primary_provider_id(&self) -> ProviderId {
        ProviderId(
            *blake3::hash(
                format!(
                    "{}|{}",
                    self.config.providers[0].name,
                    hex::encode(self.config.node_id.0)
                )
                .as_bytes(),
            )
            .as_bytes(),
        )
    }

    pub fn build_capacity_gossip(&self) -> CapacityGossipPayload {
        let capacities: Vec<ProviderCapacity> = self
            .config
            .providers
            .iter()
            .map(|p| ProviderCapacity::from_config(p, self.config.node_id))
            .collect();
        let known_peers: Vec<RouterNodeId> =
            self.peer_cache.direct_ids().into_iter().take(32).collect();
        let mut payload = CapacityGossipPayload {
            sender_id: self.config.node_id,
            timestamp: monotonic_now(),
            capacities,
            known_peers,
            hmac: [0u8; 32],
        };
        payload.hmac = payload.compute_hmac(&self.network_key());
        payload
    }

    pub fn request_capacity_from(&self, peer_id: RouterNodeId) {
        let _ = peer_id;
    }

    pub async fn broadcast_gossip(&self) -> Result<usize, octo_transport::sender::TransportError> {
        let gossip = self.build_capacity_gossip();
        let payload = bincode::serialize(&gossip).map_err(|e| {
            octo_transport::sender::TransportError::EnvelopeConstruction(e.to_string())
        })?;
        if let Some(m) = &self.metrics {
            m.add_gossip_bytes(payload.len());
        }
        let ctx = SendContext::default();
        Ok(self.transport.broadcast(&payload, &ctx).await)
    }

    pub async fn broadcast_announce(
        &self,
    ) -> Result<usize, octo_transport::sender::TransportError> {
        let mut announce = RouterAnnouncePayload {
            node_id: self.config.node_id,
            network_id: self.config.network_id,
            supported_models: self.local_provider_models(),
            capacities: self
                .config
                .providers
                .iter()
                .map(|p| ProviderCapacity::from_config(p, self.config.node_id))
                .collect(),
            timestamp: monotonic_now(),
            hmac: [0u8; 32],
        };
        announce.hmac = announce.compute_hmac(&self.network_key());
        let payload = bincode::serialize(&announce).map_err(|e| {
            octo_transport::sender::TransportError::EnvelopeConstruction(e.to_string())
        })?;
        if let Some(m) = &self.metrics {
            m.add_gossip_bytes(payload.len());
        }
        let ctx = SendContext::default();
        Ok(self.transport.broadcast(&payload, &ctx).await)
    }

    fn network_key(&self) -> [u8; 32] {
        *blake3::hash(self.config.network_id.0.as_ref()).as_bytes()
    }

    pub async fn route(
        &self,
        context: &RequestContext,
        payload: &[u8],
    ) -> Result<Vec<u8>, RouterNodeError> {
        let started = std::time::Instant::now();
        if !self.rate_limiter.check_consumer(&context.consumer_id) {
            if let Some(m) = &self.metrics {
                m.record_outcome("rate_limited");
            }
            return Err(RouterNodeError::RateLimited);
        }

        let local: Vec<ProviderCapacity> = self
            .config
            .providers
            .iter()
            .map(|p| ProviderCapacity::from_config(p, self.config.node_id))
            .collect();
        let peer_caps = self.gossip_cache.snapshot();
        let effective_policy = context
            .policy_override
            .as_ref()
            .unwrap_or(&self.config.policy);
        let destinations = self.select_destinations(context, &local, &peer_caps, effective_policy);
        if destinations.is_empty() {
            if let Some(m) = &self.metrics {
                m.record_outcome("no_provider");
            }
            return Err(RouterNodeError::NoProvider);
        }

        let outcome_label: &'static str;
        let result = match &destinations[0] {
            Destination::Local { provider, .. } => {
                outcome_label = "local_success";
                self.primary_provider
                    .completion(&context.model, payload, provider)
                    .await
                    .map_err(RouterNodeError::Provider)
            }
            Destination::Remote { .. } => {
                let request_id = {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&context.consumer_id);
                    hasher.update(&monotonic_now().to_le_bytes());
                    *hasher.finalize().as_bytes()
                };
                let mut fwd = ForwardRequestPayload {
                    request_id,
                    network_id: self.config.network_id,
                    context: context.clone(),
                    payload: payload.to_vec(),
                    ttl: self.config.forwarding.max_ttl,
                    origin_node: self.config.node_id,
                    hop_count: 0,
                    created_at: monotonic_now(),
                    hmac: [0u8; 32],
                };
                fwd.hmac = fwd.compute_hmac(&self.network_key());
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.pending.insert(request_id, tx, self.config.node_id);
                let fwd_bytes = bincode::serialize(&fwd)
                    .map_err(|e| RouterNodeError::Serialization(e.to_string()))?;
                if let Some(m) = &self.metrics {
                    m.active_forwards.inc();
                }
                let send_result = self
                    .transport
                    .send_best(&fwd_bytes, &SendContext::default())
                    .await;
                if let Err(e) = send_result {
                    self.pending.cancel(request_id);
                    if let Some(m) = &self.metrics {
                        m.active_forwards.dec();
                        m.record_outcome("send_failed");
                    }
                    return Err(RouterNodeError::Transport(e.to_string()));
                }
                let outcome =
                    tokio::time::timeout(self.config.forwarding.forward_timeout, rx).await;
                if let Some(m) = &self.metrics {
                    m.active_forwards.dec();
                }
                match outcome {
                    Ok(Ok(ForwardOutcome::Completed(bytes))) => {
                        outcome_label = "remote_success";
                        Ok(bytes)
                    }
                    Ok(Ok(ForwardOutcome::Rejected(_))) => {
                        outcome_label = "rejected";
                        Err(RouterNodeError::ForwardRejected(
                            forward::ForwardRejectReason::NoProvider,
                        ))
                    }
                    Ok(Ok(ForwardOutcome::Timeout)) | Ok(Err(_)) | Err(_) => {
                        outcome_label = "timeout";
                        Err(RouterNodeError::ForwardTimeout)
                    }
                }
            }
        };

        if let Some(m) = &self.metrics {
            m.observe_forwarding_latency(started.elapsed().as_secs_f64());
            m.record_outcome(outcome_label);
        }
        result
    }

    pub async fn build_with_bootstrap(
        config: RouterNodeConfig,
        bootstrap: QuotaRouterBootstrap,
    ) -> Result<Self, RouterNodeError> {
        let mut builder = QuotaRouterNode::builder()
            .node_id(config.node_id)
            .network_id(config.network_id)
            .policy(config.policy)
            .forwarding(config.forwarding)
            .gossip_interval(config.gossip_interval);
        for p in &config.providers {
            builder = builder.provider(p.clone());
        }
        let mut node = builder.build()?;

        if let Some(seed_path) = bootstrap.seed_list_path.as_ref() {
            if let Ok(seed_envelope) = load_seed_envelope(seed_path) {
                let bootstrap_cfg = octo_transport::bootstrap::BootstrapConfig {
                    node_id: node.config.node_id.0,
                    node_pubkey: node.identity_key,
                    bootstrap_timeout: bootstrap.timeout,
                    ..Default::default()
                };
                let _orch = octo_transport::bootstrap::BootstrapOrchestrator::new(
                    seed_envelope.clone(),
                    bootstrap_cfg,
                );
                for entry in &seed_envelope.peers {
                    if let Ok(endpoint) = entry.multiaddr.parse::<std::net::SocketAddr>() {
                        let hash = blake3::hash(entry.peer_id.as_bytes());
                        let peer_bytes = hash.as_bytes();
                        let mut node_id = [0u8; 32];
                        node_id.copy_from_slice(&peer_bytes[..32]);
                        node.add_peer(PeerConfig {
                            node_id: RouterNodeId(node_id),
                            endpoint,
                            trust_level: PeerTrust::Trusted,
                        });
                    }
                }
            }
        }

        for peer in &bootstrap.static_peers {
            node.add_peer(peer.clone());
        }

        if node.peer_count() >= bootstrap.min_peers {
            node.state = RouterNodeLifecycle::Active;
        } else {
            node.state = RouterNodeLifecycle::Discovering;
        }

        Ok(node)
    }
}

fn load_seed_envelope(
    path: &std::path::Path,
) -> Result<octo_network::mon::bootstrap::SeedListEnvelope, RouterNodeError> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| RouterNodeError::Serialization(format!("seed list: {e}")))
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct QuotaRouterBootstrap {
    pub seed_list_path: Option<std::path::PathBuf>,
    pub static_peers: Vec<PeerConfig>,
    pub timeout: std::time::Duration,
    pub min_peers: usize,
}

pub struct QuotaRouterNodeBuilder {
    node_id: Option<RouterNodeId>,
    network_id: Option<NetworkId>,
    providers: Vec<ProviderConfig>,
    peers: Vec<PeerConfig>,
    policy: RoutingPolicy,
    forwarding: ForwardingConfig,
    gossip_interval: std::time::Duration,
}

impl Default for QuotaRouterNodeBuilder {
    fn default() -> Self {
        Self {
            node_id: None,
            network_id: None,
            providers: Vec::new(),
            peers: Vec::new(),
            policy: RoutingPolicy::Balanced,
            forwarding: ForwardingConfig::default(),
            gossip_interval: std::time::Duration::from_secs(10),
        }
    }
}

impl QuotaRouterNodeBuilder {
    pub fn node_id(mut self, id: RouterNodeId) -> Self {
        self.node_id = Some(id);
        self
    }
    pub fn network_id(mut self, id: NetworkId) -> Self {
        self.network_id = Some(id);
        self
    }
    pub fn provider(mut self, p: ProviderConfig) -> Self {
        self.providers.push(p);
        self
    }
    pub fn peer(mut self, p: PeerConfig) -> Self {
        self.peers.push(p);
        self
    }
    pub fn policy(mut self, p: RoutingPolicy) -> Self {
        self.policy = p;
        self
    }
    pub fn forwarding(mut self, f: ForwardingConfig) -> Self {
        self.forwarding = f;
        self
    }
    pub fn gossip_interval(mut self, d: std::time::Duration) -> Self {
        self.gossip_interval = d;
        self
    }

    pub fn build(self) -> Result<QuotaRouterNode, RouterNodeError> {
        let node_id = self.node_id.ok_or(RouterNodeError::MissingNodeId)?;
        let network_id = self.network_id.ok_or(RouterNodeError::MissingNetworkId)?;
        if self.providers.is_empty() {
            return Err(RouterNodeError::NoProviders);
        }

        let senders: Vec<Arc<dyn NetworkSender>> = self
            .providers
            .iter()
            .map(|_| Arc::new(LocalProviderSender) as Arc<dyn NetworkSender>)
            .collect();
        let transport = NodeTransport::new(senders);

        let mut identity_key = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut identity_key);

        let primary_provider: Arc<dyn LocalProvider> =
            Arc::new(provider::HttpLocalProvider::new(self.providers[0].clone()));

        Ok(QuotaRouterNode {
            config: RouterNodeConfig {
                node_id,
                network_id,
                providers: self.providers,
                peers: self.peers,
                policy: self.policy,
                forwarding: self.forwarding,
                gossip_interval: self.gossip_interval,
            },
            state: RouterNodeLifecycle::Init,
            transport,
            gossip_cache: GossipCache::new(),
            peer_cache: PeerCache::new(),
            pending: PendingRequests::new(),
            identity_key,
            primary_provider,
            rate_limiter: ratelimit::RateLimiter::new(100, 500),
            metrics: Some(metrics::QuotaRouterMetrics::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::provider::ProviderAuth;
    use super::*;

    fn test_config() -> RouterNodeConfig {
        RouterNodeConfig {
            node_id: RouterNodeId([1u8; 32]),
            network_id: NetworkId([2u8; 32]),
            providers: vec![ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            }],
            peers: vec![],
            policy: RoutingPolicy::Balanced,
            forwarding: ForwardingConfig::default(),
            gossip_interval: std::time::Duration::from_secs(10),
        }
    }

    #[test]
    fn builder_creates_node() {
        let node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .build();
        assert!(node.is_ok());
        let node = node.unwrap();
        assert_eq!(node.config.node_id, RouterNodeId([1u8; 32]));
        assert_eq!(node.state, RouterNodeLifecycle::Init);
    }

    #[test]
    fn builder_rejects_empty_providers() {
        let result = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .build();
        assert!(matches!(result, Err(RouterNodeError::NoProviders)));
    }

    #[test]
    fn builder_rejects_missing_node_id() {
        let result = QuotaRouterNode::builder()
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .build();
        assert!(matches!(result, Err(RouterNodeError::MissingNodeId)));
    }

    #[test]
    fn peer_cache_lru_eviction() {
        let mut cache = PeerCache::with_max_peers(3);
        for i in 0..5u8 {
            cache.try_add(RouterNodeId([i; 32]));
        }
        assert_eq!(cache.total(), 3);
    }

    #[test]
    fn peer_cache_add_direct() {
        let mut cache = PeerCache::new();
        cache.add_direct(RouterNodeId([1u8; 32]), vec![]);
        assert_eq!(cache.total(), 1);
        assert_eq!(cache.direct_ids(), vec![RouterNodeId([1u8; 32])]);
    }

    #[test]
    fn gossip_cache_staleness() {
        let mut cache = GossipCache::new();
        let id = RouterNodeId([1u8; 32]);
        cache.merge(id, vec![]);
        let snap = cache.snapshot();
        assert_eq!(snap.len(), 1);
    }

    #[tokio::test]
    async fn build_with_static_peers() {
        let config = test_config();
        let bootstrap = QuotaRouterBootstrap {
            seed_list_path: None,
            static_peers: vec![PeerConfig {
                node_id: RouterNodeId([3u8; 32]),
                endpoint: "127.0.0.1:9000".parse().unwrap(),
                trust_level: PeerTrust::Trusted,
            }],
            timeout: std::time::Duration::from_secs(5),
            min_peers: 1,
        };
        let node = QuotaRouterNode::build_with_bootstrap(config, bootstrap).await;
        assert!(node.is_ok());
        let node = node.unwrap();
        assert_eq!(node.state, RouterNodeLifecycle::Active);
        assert_eq!(node.peer_count(), 1);
    }

    #[tokio::test]
    async fn build_with_insufficient_peers() {
        let config = test_config();
        let bootstrap = QuotaRouterBootstrap {
            seed_list_path: None,
            static_peers: vec![],
            timeout: std::time::Duration::from_secs(5),
            min_peers: 3,
        };
        let node = QuotaRouterNode::build_with_bootstrap(config, bootstrap).await;
        assert!(node.is_ok());
        let node = node.unwrap();
        assert_eq!(node.state, RouterNodeLifecycle::Discovering);
    }

    #[test]
    fn primary_provider_id_deterministic() {
        let node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .build()
            .unwrap();
        let id1 = node.primary_provider_id();
        let id2 = node.primary_provider_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn add_peer_increases_count() {
        let mut node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .build()
            .unwrap();
        assert_eq!(node.peer_count(), 0);
        node.add_peer(PeerConfig {
            node_id: RouterNodeId([3u8; 32]),
            endpoint: "127.0.0.1:9000".parse().unwrap(),
            trust_level: PeerTrust::Trusted,
        });
        assert_eq!(node.peer_count(), 1);
        assert!(node
            .config
            .peers
            .iter()
            .any(|p| p.node_id == RouterNodeId([3u8; 32])));
    }

    #[test]
    fn local_provider_models() {
        let node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into(), "gpt-3.5-turbo".into()],
            })
            .build()
            .unwrap();
        let models = node.local_provider_models();
        assert_eq!(models.len(), 2);
        assert!(models.contains(&"gpt-4o".to_string()));
        assert!(models.contains(&"gpt-3.5-turbo".to_string()));
    }

    #[tokio::test]
    async fn route_local_dispatch() {
        let node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .build()
            .unwrap();
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
        // HttpLocalProvider returns b"{}" for any request
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"{}".to_vec());
    }

    #[tokio::test]
    async fn route_rate_limited() {
        let mut node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .build()
            .unwrap();
        // Override rate limiter with very small burst
        node.rate_limiter = ratelimit::RateLimiter::new(100, 1);
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
        // First request succeeds
        assert!(node.route(&ctx, b"test").await.is_ok());
        // Second request is rate-limited
        let result = node.route(&ctx, b"test").await;
        assert!(matches!(result, Err(RouterNodeError::RateLimited)));
    }

    #[tokio::test]
    async fn route_local_only_no_forwarding() {
        let mut node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .policy(RoutingPolicy::LocalOnly)
            .build()
            .unwrap();
        // Add a peer with gpt-4o to verify LocalOnly doesn't forward
        node.add_peer(PeerConfig {
            node_id: RouterNodeId([3u8; 32]),
            endpoint: "127.0.0.1:9000".parse().unwrap(),
            trust_level: PeerTrust::Trusted,
        });
        // Inject gossip data for the peer
        node.gossip_cache.merge(
            RouterNodeId([3u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([4u8; 32]),
                provider_name: "anthropic".into(),
                router_node_id: RouterNodeId([3u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![],
                status: provider::ProviderHealth::Healthy,
                latency_ms: 100,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
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
        // LocalOnly with local provider → dispatches locally
        let result = node.route(&ctx, b"test").await;
        assert!(result.is_ok());
    }
}
