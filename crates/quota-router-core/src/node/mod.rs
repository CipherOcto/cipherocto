pub mod announce;
pub mod forward;
pub mod gossip;
pub mod handler;
pub mod metrics;
pub mod provider;
pub mod ratelimit;
pub mod request;
pub mod scorer;
#[cfg(any(test, feature = "test-helpers"))]
pub mod testing;

use std::sync::{Arc, Mutex};

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
use scorer::{select_destinations, select_destinations_with_state, Destination, SelectionState};

/// Discriminator byte prepended to every outbound DOT envelope.
///
/// The handler (`QuotaRouterHandler::on_receive`) reads the first byte
/// of an inbound payload and dispatches to the matching `handle_*`
/// method. Every outbound site MUST prepend one of these bytes —
/// otherwise the peer drops the message as "unknown discriminator".
pub const DISC_FORWARD_REQUEST: u8 = 0xC3;
pub const DISC_FORWARD_RESPONSE: u8 = 0xC4;
pub const DISC_FORWARD_REJECT: u8 = 0xC5;
pub const DISC_CAPACITY_GOSSIP: u8 = 0xC6;
pub const DISC_CAPACITY_REQUEST: u8 = 0xC7;
pub const DISC_ROUTER_ANNOUNCE: u8 = 0xCA;
pub const DISC_ROUTER_WITHDRAW: u8 = 0xCB;

/// Wrap a serializable payload in a DOT envelope with the given
/// discriminator byte. Wire format: `[discriminator: u8][body: bincode(payload)]`.
///
/// Used by every outbound site so peers can dispatch on the discriminator
/// without trying to interpret bincode framing bytes as message kinds.
pub fn envelope<T: serde::Serialize>(
    discriminator: u8,
    payload: &T,
) -> Result<Vec<u8>, octo_transport::sender::TransportError> {
    let mut out = Vec::with_capacity(1 + 64);
    out.push(discriminator);
    let body = bincode::serialize(payload)
        .map_err(|e| octo_transport::sender::TransportError::EnvelopeConstruction(e.to_string()))?;
    out.extend_from_slice(&body);
    Ok(out)
}

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
    pub state: std::sync::Mutex<RouterNodeLifecycle>,
    pub transport: Arc<NodeTransport>,
    /// Cached capacities learned via gossip. Wrapped in `Mutex` so the
    /// inbound handler can merge entries through the `Arc<QuotaRouterNode>`
    /// shared via `Weak`.
    pub gossip_cache: Mutex<GossipCache>,
    /// Discovered/configured peer table. Wrapped in `Mutex` so the
    /// inbound handler can add/remove entries through the
    /// `Arc<QuotaRouterNode>` shared via `Weak`.
    pub peer_cache: Mutex<PeerCache>,
    pub(crate) pending: PendingRequests,
    pub identity_key: [u8; 32],
    #[allow(dead_code)]
    primary_provider: Arc<dyn LocalProvider>,
    pub rate_limiter: std::sync::Mutex<ratelimit::RateLimiter>,
    pub metrics: Option<metrics::QuotaRouterMetrics>,
    /// Live count of in-flight remote forwards. Incremented when a
    /// forward is dispatched, decremented when its oneshot resolves
    /// or the request times out. Used to enforce
    /// `config.forwarding.max_concurrent_forwards` in `route()`.
    pub active_forwards: std::sync::atomic::AtomicUsize,
    /// Internal inbound handler. Owned by the node and registered
    /// with `self.transport` by the builder so inbound envelopes
    /// are dispatched into the handler without callers having to
    /// wire it up manually.
    pub(crate) handler: Arc<handler::QuotaRouterHandler>,
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

    pub async fn receive(
        &self,
        payload: &[u8],
        ctx: &octo_transport::receiver::ReceiveContext,
    ) -> Result<(), octo_transport::sender::TransportError> {
        self.transport.dispatch(payload, ctx).await
    }

    /// Register an additional inbound receiver on the underlying
    /// transport. Used by the e2e test harness after swapping
    /// `node.transport` to an in-process implementation — the
    /// builder-registered handler stays attached to the OLD transport
    /// and must be re-attached to the new one. Production code paths
    /// do not need this method because they construct the transport
    /// once and never replace it.
    pub fn register_receiver(&self, receiver: Arc<dyn octo_transport::receiver::NetworkReceiver>) {
        self.transport.register_receiver(receiver);
    }

    /// Re-register the internal `QuotaRouterHandler` on the current
    /// `node.transport`. The builder registers the handler on the
    /// transport at construction time; if the transport is later
    /// swapped (e.g. the e2e harness swapping in an `InProcessSender`-
    /// backed transport), the handler remains attached to the OLD
    /// transport. Calling this method re-attaches it to the current
    /// transport. Production code paths never swap the transport.
    pub fn reattach_internal_handler(&self) {
        self.transport.register_receiver(
            self.handler.clone() as Arc<dyn octo_transport::receiver::NetworkReceiver>
        );
    }

    /// Temporarily clear the handler's back-reference so the inner
    /// `Arc<QuotaRouterNode>` becomes uniquely owned. Callers must
    /// follow up with `restore_handler_back_ref(...)` once mutation
    /// is done so inbound dispatch still resolves the node.
    ///
    /// This is a test-only escape hatch — production code does not
    /// need to mutate the node post-construction because it sets up
    /// all state through the builder.
    pub fn release_handler_back_ref(&self) -> std::sync::Weak<QuotaRouterNode> {
        self.handler.release_back_ref()
    }

    /// Restore the handler's back-reference after
    /// `release_handler_back_ref`. Pass back the weak returned by
    /// the release call.
    pub fn restore_handler_back_ref(&self, weak: std::sync::Weak<QuotaRouterNode>) {
        self.handler.restore_back_ref(weak);
    }

    pub fn peer_count(&self) -> usize {
        // Peer table sources, potentially overlapping:
        //   1. `config.peers` — populated by the builder and by
        //      `add_peer` (which mirrors into `peer_cache.direct`).
        //   2. `peer_cache.discovered` — populated by gossip when a
        //      previously-unknown peer announces itself.
        //   3. `peer_cache.direct` — runtime entries added via
        //      `add_peer` (also in `config.peers`) and via
        //      `handle_router_announce` (peer cache only, NOT in
        //      `config.peers`).
        // To avoid double-counting we sum the disjoint sources:
        //   - `config.peers` (the configured set)
        //   - `peer_cache.discovered.len()` (the gossip-discovered set)
        //   - `peer_cache.direct` entries whose node_id is NOT in
        //     `config.peers` (announce-added set).
        let cache = self.peer_cache.lock().unwrap();
        let mut count = self.config.peers.len() + cache.discovered.len();
        let configured: std::collections::BTreeSet<RouterNodeId> =
            self.config.peers.iter().map(|p| p.node_id).collect();
        for id in cache.direct.keys() {
            if !configured.contains(id) {
                count += 1;
            }
        }
        count
    }

    /// Set the lifecycle state. Public for bootstrap/init flows that
    /// mutate the node post-construction without holding a unique
    /// reference. Used by `build_with_bootstrap` after staging peers.
    pub fn set_lifecycle(&self, next: RouterNodeLifecycle) {
        *self.state.lock().unwrap() = next;
    }

    pub fn local_provider_models(&self) -> Vec<String> {
        self.config
            .providers
            .iter()
            .flat_map(|p| p.models.iter().cloned())
            .collect()
    }

    pub fn add_peer(&mut self, peer: PeerConfig) {
        self.peer_cache
            .lock()
            .unwrap()
            .add_direct(peer.node_id, vec![]);
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

    pub fn select_destinations_with_state(
        &self,
        request: &RequestContext,
        local_providers: &[ProviderCapacity],
        peer_capabilities: &[(RouterNodeId, Vec<ProviderCapacity>)],
        policy: &RoutingPolicy,
    ) -> SelectionState {
        select_destinations_with_state(request, local_providers, peer_capabilities, policy)
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
        let known_peers: Vec<RouterNodeId> = self
            .peer_cache
            .lock()
            .unwrap()
            .direct_ids()
            .into_iter()
            .take(32)
            .collect();
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
        let payload = envelope(DISC_CAPACITY_GOSSIP, &gossip)?;
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
        let payload = envelope(DISC_ROUTER_ANNOUNCE, &announce)?;
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
        if !self
            .rate_limiter
            .lock()
            .unwrap()
            .check_consumer(&context.consumer_id)
        {
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
        let peer_caps = self.gossip_cache.lock().unwrap().snapshot();
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
                // Concurrent-forward gate. If we've hit the configured
                // cap, refuse the forward rather than queuing — keeps the
                // in-flight set bounded so backpressure is observable to
                // callers. Pre-check (load) is a hint; the post-add
                // fetch_add ensures the counter is correct under races.
                if self
                    .active_forwards
                    .load(std::sync::atomic::Ordering::SeqCst)
                    >= self.config.forwarding.max_concurrent_forwards as usize
                {
                    if let Some(m) = &self.metrics {
                        m.record_outcome("rate_limited");
                    }
                    return Err(RouterNodeError::RateLimited);
                }
                self.active_forwards
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
                let fwd_bytes = envelope(DISC_FORWARD_REQUEST, &fwd)
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
                    self.active_forwards
                        .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(RouterNodeError::Transport(e.to_string()));
                }
                let outcome =
                    tokio::time::timeout(self.config.forwarding.forward_timeout, rx).await;
                if let Some(m) = &self.metrics {
                    m.active_forwards.dec();
                }
                self.active_forwards
                    .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
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
    ) -> Result<Arc<QuotaRouterNode>, RouterNodeError> {
        let mut builder = QuotaRouterNode::builder()
            .node_id(config.node_id)
            .network_id(config.network_id)
            .policy(config.policy)
            .forwarding(config.forwarding)
            .gossip_interval(config.gossip_interval);
        for p in &config.providers {
            builder = builder.provider(p.clone());
        }

        // Stage peers on the builder before invoking `build()`. The
        // builder returns an `Arc<QuotaRouterNode>` whose inner value
        // is shared with the handler's Weak back-pointer, so we cannot
        // mutate fields via `Arc::get_mut` (the Weak blocks it). All
        // peer additions therefore happen up-front here.
        if let Some(seed_path) = bootstrap.seed_list_path.as_ref() {
            if let Ok(seed_envelope) = load_seed_envelope(seed_path) {
                let bootstrap_cfg = octo_transport::bootstrap::BootstrapConfig {
                    node_id: config.node_id.0,
                    node_pubkey: [0u8; 32],
                    bootstrap_timeout: bootstrap.timeout,
                    min_responses: 0,
                    max_retries: 1,
                    ..Default::default()
                };
                let mut orch = octo_transport::bootstrap::BootstrapOrchestrator::new(
                    seed_envelope.clone(),
                    bootstrap_cfg,
                );

                // Run the orchestrator to validate the seed list and
                // attempt to collect responses from bootstrap nodes.
                // On success, use the response peer entries. On failure
                // (e.g. bootstrap nodes unreachable), fall back to the
                // seed list entries directly.
                let peer_configs = match orch.discover_peers(&dummy_transport(), 256).await {
                    Ok(responses) if !responses.is_empty() => {
                        // Build peer configs from bootstrap responses
                        let mut configs = Vec::new();
                        for resp in &responses {
                            for entry in &resp.peer_entries {
                                let hash = blake3::hash(&entry.peer_id);
                                let peer_bytes = hash.as_bytes();
                                let mut node_id = [0u8; 32];
                                node_id.copy_from_slice(&peer_bytes[..32]);
                                if let Ok(endpoint) =
                                    entry.multiaddr.parse::<std::net::SocketAddr>()
                                {
                                    configs.push(PeerConfig {
                                        node_id: RouterNodeId(node_id),
                                        endpoint,
                                        trust_level: PeerTrust::Trusted,
                                    });
                                }
                            }
                        }
                        configs
                    }
                    _ => {
                        // Fallback: parse seed list entries directly
                        let mut configs = Vec::new();
                        for entry in &seed_envelope.peers {
                            if let Ok(endpoint) = entry.multiaddr.parse::<std::net::SocketAddr>() {
                                let hash = blake3::hash(entry.peer_id.as_bytes());
                                let peer_bytes = hash.as_bytes();
                                let mut node_id = [0u8; 32];
                                node_id.copy_from_slice(&peer_bytes[..32]);
                                configs.push(PeerConfig {
                                    node_id: RouterNodeId(node_id),
                                    endpoint,
                                    trust_level: PeerTrust::Trusted,
                                });
                            }
                        }
                        configs
                    }
                };

                for cfg in peer_configs {
                    builder = builder.peer(cfg);
                }
            }
        }

        for peer in &bootstrap.static_peers {
            builder = builder.peer(peer.clone());
        }

        let node = builder.build()?;
        // Promote lifecycle state based on the configured peer count.
        // The `state` field is pub but mutating through Arc would require
        // deref-mut which Rust does not implement; expose a setter that
        // goes through interior mutability instead.
        let next_state = if node.peer_count() >= bootstrap.min_peers {
            RouterNodeLifecycle::Active
        } else {
            RouterNodeLifecycle::Discovering
        };
        node.set_lifecycle(next_state);
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

/// Create a minimal `NodeTransport` for bootstrap orchestrator calls.
/// The orchestrator uses direct TCP connections for seed communication
/// and does not route through the transport, so this only needs to
/// satisfy the type signature.
fn dummy_transport() -> octo_transport::node_transport::NodeTransport {
    use octo_transport::sender::NetworkSender;
    struct DummySender;
    #[async_trait::async_trait]
    impl NetworkSender for DummySender {
        async fn send(
            &self,
            _: &[u8],
            _: &octo_transport::sender::SendContext,
        ) -> Result<(), octo_transport::sender::TransportError> {
            Ok(())
        }
        fn name(&self) -> &str {
            "dummy"
        }
        fn is_healthy(&self) -> bool {
            true
        }
    }
    octo_transport::node_transport::NodeTransport::new(vec![std::sync::Arc::new(DummySender)])
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
    /// Optional `LocalProvider` injected for tests instead of the default
    /// `HttpLocalProvider`. The handler and node both receive this
    /// provider so inbound dispatch reaches it.
    primary_provider_override: Option<Arc<dyn LocalProvider>>,
    /// Optional caller-provided transport. When set, the builder uses
    /// this transport instead of the auto-constructed one wrapping
    /// `LocalProviderSender` placeholders. The handler is registered on
    /// this transport during build, so the caller does not need to
    /// swap transports post-build.
    transport_override: Option<Arc<NodeTransport>>,
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
            primary_provider_override: None,
            transport_override: None,
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

    /// Inject a custom `LocalProvider` (e.g. `MockLocalProvider`) for
    /// tests. Replaces the default `HttpLocalProvider` that the builder
    /// would otherwise construct from `providers[0]`. The override is
    /// passed to both the node's `primary_provider` field and the
    /// internal `QuotaRouterHandler` so local dispatch and inbound
    /// forwarding reach the same provider instance.
    pub fn primary_provider_override(mut self, p: Arc<dyn LocalProvider>) -> Self {
        self.primary_provider_override = Some(p);
        self
    }

    /// Provide the `NodeTransport` the builder should install on the
    /// node. Used by integration tests to swap in transports backed
    /// by `InProcessSender` without post-build mutation. When this is
    /// set, the builder skips constructing the default
    /// `LocalProviderSender`-only transport.
    pub fn transport(mut self, transport: Arc<NodeTransport>) -> Self {
        self.transport_override = Some(transport);
        self
    }

    pub fn build(self) -> Result<Arc<QuotaRouterNode>, RouterNodeError> {
        let node_id = self.node_id.ok_or(RouterNodeError::MissingNodeId)?;
        let network_id = self.network_id.ok_or(RouterNodeError::MissingNetworkId)?;
        if self.providers.is_empty() {
            return Err(RouterNodeError::NoProviders);
        }

        // Resolve the transport: either the caller's override (tests)
        // or a freshly constructed `LocalProviderSender`-only transport
        // (production). The handler is registered on whichever one
        // we end up using.
        let transport = match self.transport_override {
            Some(t) => t,
            None => {
                let senders: Vec<Arc<dyn NetworkSender>> = self
                    .providers
                    .iter()
                    .map(|_| Arc::new(LocalProviderSender) as Arc<dyn NetworkSender>)
                    .collect();
                Arc::new(NodeTransport::new(senders))
            }
        };

        let mut identity_key = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut identity_key);

        let primary_provider: Arc<dyn LocalProvider> = match self.primary_provider_override {
            Some(p) => p,
            None => Arc::new(provider::HttpLocalProvider::new(self.providers[0].clone())),
        };

        // Network key: BLAKE3 hash of the network_id — used to HMAC
        // outbound gossip / forward envelopes and to verify inbound
        // ones. Derived here so handler and node agree on it.
        let network_key = *blake3::hash(network_id.0.as_ref()).as_bytes();

        // Build the node via `Arc::new_cyclic` so the handler can hold
        // a `Weak` back-pointer to the node. The closure receives the
        // `Weak` already, hands a clone to the handler, and stores the
        // resulting `Arc<QuotaRouterHandler>` on the node.
        //
        // We then RETURN the `Arc<QuotaRouterNode>` directly rather than
        // `Arc::try_unwrap`'ing it. Unwrapping would drop the allocation
        // the handler's `Weak` references, causing inbound dispatch to
        // fail with "node dropped" when `upgrade()` returns None.
        let node = Arc::new_cyclic(|weak: &std::sync::Weak<QuotaRouterNode>| {
            let handler = Arc::new(handler::QuotaRouterHandler::new(
                weak.clone(),
                primary_provider.clone(),
                network_key,
            ));
            QuotaRouterNode {
                config: RouterNodeConfig {
                    node_id,
                    network_id,
                    providers: self.providers,
                    peers: self.peers,
                    policy: self.policy,
                    forwarding: self.forwarding,
                    gossip_interval: self.gossip_interval,
                },
                state: std::sync::Mutex::new(RouterNodeLifecycle::Init),
                transport: transport.clone(),
                gossip_cache: Mutex::new(GossipCache::new()),
                peer_cache: Mutex::new(PeerCache::new()),
                pending: PendingRequests::new(),
                identity_key,
                primary_provider: primary_provider.clone(),
                rate_limiter: std::sync::Mutex::new(ratelimit::RateLimiter::new(100, 500)),
                metrics: Some(metrics::QuotaRouterMetrics::new()),
                active_forwards: std::sync::atomic::AtomicUsize::new(0),
                handler,
            }
        });

        node.transport.register_receiver(
            node.handler.clone() as Arc<dyn octo_transport::receiver::NetworkReceiver>
        );

        Ok(node)
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
    fn node_has_internal_handler_after_build() {
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
        assert!(
            std::sync::Arc::strong_count(&node.handler) >= 1,
            "QuotaRouterNode must own its handler"
        );
        let handler_as_receiver: Arc<dyn octo_transport::receiver::NetworkReceiver> =
            node.handler.clone();
        assert_eq!(handler_as_receiver.name(), "quota-router-handler");
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
        assert_eq!(*node.state.lock().unwrap(), RouterNodeLifecycle::Init);
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
        let cache = GossipCache::new();
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
        assert_eq!(*node.state.lock().unwrap(), RouterNodeLifecycle::Active);
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
        assert_eq!(
            *node.state.lock().unwrap(),
            RouterNodeLifecycle::Discovering
        );
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
        let node = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .peer(PeerConfig {
                node_id: RouterNodeId([3u8; 32]),
                endpoint: "127.0.0.1:9000".parse().unwrap(),
                trust_level: PeerTrust::Trusted,
            })
            .build()
            .unwrap();
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
        let arc = QuotaRouterNode::builder()
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
        // Override rate limiter with very small burst. The `rate_limiter`
        // is behind a Mutex so the swap works through the Arc.
        *arc.rate_limiter.lock().unwrap() = ratelimit::RateLimiter::new(100, 1);
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
        assert!(arc.route(&ctx, b"test").await.is_ok());
        // Second request is rate-limited
        let result = arc.route(&ctx, b"test").await;
        assert!(matches!(result, Err(RouterNodeError::RateLimited)));
    }

    #[tokio::test]
    async fn route_local_only_no_forwarding() {
        let arc = QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .policy(RoutingPolicy::LocalOnly)
            .peer(PeerConfig {
                node_id: RouterNodeId([3u8; 32]),
                endpoint: "127.0.0.1:9000".parse().unwrap(),
                trust_level: PeerTrust::Trusted,
            })
            .build()
            .unwrap();
        // Inject gossip data for the peer (gossip_cache is behind a Mutex).
        arc.gossip_cache.lock().unwrap().merge(
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
        let result = arc.route(&ctx, b"test").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn receive_delegates_to_transport_dispatch() {
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
        let ctx = octo_transport::receiver::ReceiveContext {
            source_transport: "test".into(),
            mission_id: [0u8; 32],
            sender_id: None,
        };
        // Handler is auto-registered by the builder. Unknown
        // discriminator 0xFF should be silently accepted (handler
        // returns Ok for unknown discriminators).
        let r = node.receive(&[0xFF], &ctx).await;
        assert!(r.is_ok(), "expected Ok for unknown discriminator: {:?}", r);
    }

    #[tokio::test]
    async fn broadcast_gossip_does_not_panic() {
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
        // broadcast_gossip sends to LocalProviderSender (no-op), should not panic
        let _ = node.broadcast_gossip().await;
    }

    #[tokio::test]
    async fn broadcast_announce_does_not_panic() {
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
        let _ = node.broadcast_announce().await;
    }

    #[test]
    fn select_destinations_with_state_capacity_exhausted() {
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
        let req = super::request::RequestContext {
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
        // Provider with 0 remaining → CapacityExhausted
        let local = vec![super::provider::ProviderCapacity {
            provider_id: super::provider::ProviderId([1u8; 32]),
            provider_name: "openai".into(),
            router_node_id: RouterNodeId([1u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: 0,
            pricing: vec![super::provider::ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 3,
            }],
            status: super::provider::ProviderHealth::Healthy,
            latency_ms: 200,
            success_rate_bps: 9500,
            last_updated: 0,
        }];
        let state =
            node.select_destinations_with_state(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(matches!(
            state,
            super::scorer::SelectionState::CapacityExhausted
        ));
    }

    #[test]
    fn build_capacity_gossip_includes_known_peers() {
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
        // Add a peer so gossip includes it
        node.peer_cache
            .lock()
            .unwrap()
            .add_direct(RouterNodeId([3u8; 32]), vec![]);
        let gossip = node.build_capacity_gossip();
        assert_eq!(gossip.sender_id, RouterNodeId([1u8; 32]));
        assert!(gossip.known_peers.contains(&RouterNodeId([3u8; 32])));
    }

    #[test]
    fn pending_origin_returns_none_for_unknown() {
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
        assert!(node.pending_origin([99u8; 32]).is_none());
    }

    #[test]
    fn set_lifecycle_transitions() {
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
        assert_eq!(*node.state.lock().unwrap(), RouterNodeLifecycle::Init);
        node.set_lifecycle(RouterNodeLifecycle::Active);
        assert_eq!(*node.state.lock().unwrap(), RouterNodeLifecycle::Active);
        node.set_lifecycle(RouterNodeLifecycle::Draining);
        assert_eq!(*node.state.lock().unwrap(), RouterNodeLifecycle::Draining);
    }
}
