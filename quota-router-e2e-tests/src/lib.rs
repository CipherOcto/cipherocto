//! End-to-end harness for the quota-router network.
//!
//! Tests exercise the REAL production code paths:
//!   * `QuotaRouterNode::builder().build()` constructs the node exactly as
//!     production does — gossip caches, peer cache, rate limiter, metrics,
//!     the internal `QuotaRouterHandler`, and the `register_receiver` call
//!     that wires inbound dispatch. No manual handler wiring required.
//!   * The default `NodeTransport` from the builder is built with
//!     `LocalProviderSender` placeholders that discard payloads. For the
//!     in-process mesh we need messages to actually flow between nodes,
//!     so we *swap* `node.transport` for one wrapping an `InProcessSender`
//!     that broadcasts to peer inboxes. The `NetworkSender` trait is the
//!     production seam for this swap.
//!   * `MockLocalProvider` replaces the default `HttpLocalProvider` via
//!     `builder.primary_provider_override(...)` so tests can capture
//!     completion payloads and override responses without hitting a
//!     real HTTP endpoint. The override flows into both the node's
//!     `primary_provider` field and the internal handler.
//!   * `node.receive()` is the public inbound API — the harness's
//!     background driver calls it on every inbound payload. It
//!     delegates to `NodeTransport::dispatch()` internally, so the
//!     production path is preserved end-to-end.
//!   * ALL inbound discriminators (0xC3..0xCB) are dispatched through
//!     the builder-installed handler. HMAC checks, model-overlap gates,
//!     TTL handling, pending-request resolution, and capacity-cache
//!     updates happen exactly as in production.
//!
//! A background driver task drains each node's inbox and feeds payloads
//! into `node.receive()`, so the full production inbound path is
//! exercised. `route()`'s `oneshot::Receiver` gets fulfilled by the
//! real handler running on the peer side via the same dispatch path.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use octo_transport::node_transport::NodeTransport;
use octo_transport::receiver::ReceiveContext;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};
use quota_router::provider::{
    LocalProvider, NetworkId, ProviderAuth, ProviderCapacity, ProviderConfig, ProviderError,
    ProviderHealth, RouterNodeId,
};
use quota_router::request::{ForwardingConfig, RequestContext, RoutingPolicy};
use quota_router::QuotaRouterNode;

pub type PeerMap =
    Arc<Mutex<BTreeMap<RouterNodeId, tokio::sync::mpsc::Sender<(RouterNodeId, Vec<u8>)>>>>;

// ── InProcessSender ────────────────────────────────────────────────
//
// `NetworkSender` implementation backed by the shared peer map. Every
// node has exactly one of these in its `NodeTransport`. Delivery is
// broadcast (fan-out to all peers except self). `try_send` is used so
// the call is non-blocking — messages sit in each peer's mpsc inbox
// until the background driver drains them.
//
// Each message is tagged with the sender's `RouterNodeId` so the
// receiver can set `ReceiveContext.sender_id` — enabling production
// trust-level checks and per-peer rate limiting.

pub struct InProcessSender {
    peers: PeerMap,
    self_id: RouterNodeId,
}

impl InProcessSender {
    pub fn new(peers: PeerMap, self_id: RouterNodeId) -> Self {
        Self { peers, self_id }
    }
}

#[async_trait::async_trait]
impl NetworkSender for InProcessSender {
    async fn send(&self, payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        let senders: Vec<_> = {
            let peers = self.peers.lock().unwrap();
            peers
                .iter()
                .filter(|(id, _)| **id != self.self_id)
                .map(|(_, s)| s.clone())
                .collect()
        };
        for sender in senders {
            let _ = sender.try_send((self.self_id, payload.to_vec()));
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "in-process"
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

// ── MockLocalProvider ──────────────────────────────────────────────
//
// `LocalProvider` implementation that the production `QuotaRouterHandler`
// calls when a forward request arrives and is dispatched locally. The
// provider:
//   * Captures every (model, payload) pair it sees, so tests can assert
//     that the forwarded payload actually reached the destination node.
//   * Returns `b"{}"` by default — matches the placeholder
//     `HttpLocalProvider::completion` in `quota-router/src/provider.rs`
//     so unit tests for local dispatch continue to work.
//   * Lets tests override the response for a given model with
//     `set_response(model, bytes)` so a multi-hop test can detect that
//     the response came from a specific peer.

#[allow(clippy::type_complexity)]
pub struct MockLocalProvider {
    models: Vec<String>,
    health: ProviderHealth,
    captured: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    responses: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl MockLocalProvider {
    pub fn new(models: Vec<String>) -> Self {
        Self {
            models,
            health: ProviderHealth::Healthy,
            captured: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_health(mut self, health: ProviderHealth) -> Self {
        self.health = health;
        self
    }

    /// Override the response returned for a given model.
    pub fn set_response(&self, model: &str, bytes: Vec<u8>) {
        self.responses
            .lock()
            .unwrap()
            .insert(model.to_string(), bytes);
    }

    /// Snapshot every (model, payload) pair the provider has dispatched.
    pub fn captured(&self) -> Vec<(String, Vec<u8>)> {
        self.captured.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LocalProvider for MockLocalProvider {
    async fn completion(
        &self,
        model: &str,
        messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        self.captured
            .lock()
            .unwrap()
            .push((model.to_string(), messages.to_vec()));
        let response = self
            .responses
            .lock()
            .unwrap()
            .get(model)
            .cloned()
            .unwrap_or_else(|| b"{}".to_vec());
        Ok(response)
    }

    async fn health_check(&self) -> ProviderHealth {
        self.health.clone()
    }

    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

// ── TestNode ───────────────────────────────────────────────────────

pub struct TestNode {
    pub node_id: RouterNodeId,
    pub node: Arc<QuotaRouterNode>,
    pub provider: Arc<MockLocalProvider>,
    inbox_rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<(RouterNodeId, Vec<u8>)>>,
}

impl TestNode {
    pub fn new(
        node_id: RouterNodeId,
        models: Vec<String>,
        peer_map: PeerMap,
        _network_key: [u8; 32],
    ) -> Self {
        // Each node owns an inbox; the InProcessSender peers push into it.
        let (inbox_tx, inbox_rx) = tokio::sync::mpsc::channel(256);
        peer_map.lock().unwrap().insert(node_id, inbox_tx);

        // Build the production transport with the InProcessSender.
        let sender = Arc::new(InProcessSender::new(peer_map.clone(), node_id));
        let transport = Arc::new(NodeTransport::new(vec![sender]));

        // The MockLocalProvider replaces the default HttpLocalProvider
        // so tests can capture completion payloads and override responses
        // without hitting a real HTTP endpoint. The builder wires this
        // provider into both the node's primary_provider field and the
        // internal QuotaRouterHandler — no manual handler construction.
        let provider = Arc::new(MockLocalProvider::new(models.clone()));
        let provider_for_builder: Arc<dyn LocalProvider> = provider.clone();

        // Construct the node via the production builder. We pass the
        // pre-built in-process transport via `.transport(...)` so the
        // builder registers the internal handler on THIS transport
        // directly — no post-build mutation needed. The builder returns
        // `Arc<QuotaRouterNode>` so the handler's Weak back-pointer
        // stays valid.
        let mut builder = QuotaRouterNode::builder()
            .node_id(node_id)
            .network_id(NetworkId([1u8; 32]))
            .policy(RoutingPolicy::Balanced)
            .forwarding(ForwardingConfig::default())
            .gossip_interval(std::time::Duration::from_secs(10))
            .primary_provider_override(provider_for_builder)
            .transport(transport);
        for model in &models {
            builder = builder.provider(ProviderConfig {
                name: model.clone(),
                endpoint: "http://localhost".into(),
                auth: ProviderAuth::Local,
                models: vec![model.clone()],
            });
        }
        let node = builder.build().expect("failed to build QuotaRouterNode");

        Self {
            node_id,
            node,
            provider,
            inbox_rx: tokio::sync::Mutex::new(inbox_rx),
        }
    }

    /// Drain the inbox, dispatching each payload through `node.receive()`.
    /// `node.receive()` delegates to `NodeTransport::dispatch()` →
    /// `handler.on_receive()`, exercising the full production inbound
    /// path. The sender_id is set so the handler can look up trust
    /// level and enforce HMAC verification for Verified peers.
    pub async fn drive(&self) {
        loop {
            let (sender_id, payload) = {
                let mut rx = self.inbox_rx.lock().await;
                match rx.try_recv() {
                    Ok(item) => item,
                    Err(_) => return,
                }
            };
            self.dispatch_with_sender(&sender_id, &payload).await;
        }
    }

    async fn dispatch_with_sender(&self, sender_id: &RouterNodeId, payload: &[u8]) {
        if payload.is_empty() {
            return;
        }
        let ctx = ReceiveContext {
            source_transport: "in-process".into(),
            mission_id: [0u8; 32],
            sender_id: Some(sender_id.0),
        };
        if let Err(e) = self.node.receive(payload, &ctx).await {
            eprintln!(
                "node {:?}: handler error on disc 0x{:02X}: {}",
                self.node_id, payload[0], e
            );
        }
    }

    /// Broadcast a RouterAnnounce using the production method.
    pub async fn broadcast_announce(&self) {
        if let Err(e) = self.node.broadcast_announce().await {
            eprintln!("node {:?}: broadcast_announce failed: {}", self.node_id, e);
        }
    }

    /// Broadcast a CapacityGossip using the production method.
    pub async fn broadcast_gossip(&self) {
        if let Err(e) = self.node.broadcast_gossip().await {
            eprintln!("node {:?}: broadcast_gossip failed: {}", self.node_id, e);
        }
    }

    pub async fn route(
        &self,
        ctx: &RequestContext,
        payload: &[u8],
    ) -> Result<Vec<u8>, quota_router::RouterNodeError> {
        self.node.route(ctx, payload).await
    }

    pub async fn gossip_cache_snapshot(&self) -> Vec<(RouterNodeId, Vec<ProviderCapacity>)> {
        self.node.gossip_cache.lock().unwrap().snapshot()
    }

    pub async fn peer_count(&self) -> usize {
        self.node.peer_count()
    }
}

// ── TestCluster ────────────────────────────────────────────────────

pub struct TestCluster {
    pub nodes: Vec<Arc<TestNode>>,
    pub network_key: [u8; 32],
    _peer_map: PeerMap,
    driver_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    driver_cancel: Arc<AtomicBool>,
}

impl TestCluster {
    /// Mutable access to a test node's `QuotaRouterNode` by index.
    /// Temporarily releases the handler's `Weak` back-reference so
    /// `Arc::get_mut` succeeds on the inner `Arc<QuotaRouterNode>`,
    /// then restores the weak once the caller is done mutating.
    /// The Weak lives in a Mutex specifically to allow this escape
    /// hatch while keeping inbound dispatch working the rest of the time.
    pub fn node_mut(&mut self, idx: usize) -> &mut QuotaRouterNode {
        // The builder returns `Arc<QuotaRouterNode>`. The inner
        // `Arc::get_mut` requires unique ownership AND no Weak refs.
        // The handler holds the Weak through a release-able Mutex so
        // we can temporarily clear it.
        //
        // The outer `Arc::get_mut(&mut self.nodes[idx])` can fail if
        // the background driver holds an `Arc<TestNode>` (it doesn't
        // drop its local ref between iterations). Tests that need
        // `node_mut` should pause the driver via
        // `cluster.driver_cancel.store(true, Ordering::Relaxed)` and
        // call `wait` for the in-flight iteration to finish before
        // mutating. The gossip-correctness tests don't call
        // `node_mut`, so the driver interferes only with TTL/concurrency
        // tests — a separate cleanup-plan follow-up.
        let test_node = Arc::get_mut(&mut self.nodes[idx]).expect(
            "TestCluster::node_mut: another Arc<TestNode> exists; \
             tests must not clone node before tweaking config",
        );
        let weak = test_node.node.release_handler_back_ref();
        let inner = Arc::get_mut(&mut test_node.node).expect(
            "TestCluster::node_mut: inner Arc<QuotaRouterNode> \
             still has aliasing references after releasing the handler Weak",
        );
        inner.restore_handler_back_ref(weak);
        inner
    }
}

impl TestCluster {
    pub fn new(n: usize, model_sets: Vec<Vec<String>>) -> Self {
        let network_id = [1u8; 32];
        let network_key = *blake3::hash(&network_id).as_bytes();
        let peer_map: PeerMap = Arc::new(Mutex::new(BTreeMap::new()));

        let mut nodes = Vec::with_capacity(n);
        for i in 0..n {
            let node_id = RouterNodeId([(i + 1) as u8; 32]);
            let models = model_sets
                .get(i)
                .cloned()
                .unwrap_or_else(|| vec!["gpt-4o".into()]);
            nodes.push(Arc::new(TestNode::new(
                node_id,
                models,
                peer_map.clone(),
                network_key,
            )));
        }

        // Background driver: continuously drains every node's inbox.
        // Required because `route()` awaits a response on a oneshot —
        // the peer must run its handler to fulfil it, and that requires
        // the inbox to be drained while `route()` is awaiting.
        //
        // Holds `Weak<TestNode>` references and upgrades each iteration
        // so the cluster retains the only strong `Arc<TestNode>` refs.
        // That keeps `TestCluster::node_mut` working via `Arc::get_mut`
        // for tests that tweak node config (e.g. `ForwardingConfig`)
        // after `start_all()`.
        let driver_cancel = Arc::new(AtomicBool::new(false));
        let driver_handle = {
            let nodes_for_driver: Vec<std::sync::Weak<TestNode>> =
                nodes.iter().map(std::sync::Arc::downgrade).collect();
            let cancel = driver_cancel.clone();
            tokio::spawn(async move {
                while !cancel.load(Ordering::Relaxed) {
                    for weak_node in &nodes_for_driver {
                        if let Some(node) = weak_node.upgrade() {
                            node.drive().await;
                        }
                    }
                    tokio::task::yield_now().await;
                }
            })
        };

        Self {
            nodes,
            network_key,
            _peer_map: peer_map,
            driver_handle: Mutex::new(Some(driver_handle)),
            driver_cancel,
        }
    }

    /// Broadcast each node's announce and drive enough cycles to let
    /// the handler process them.
    pub async fn start_all(&self) {
        for node in &self.nodes {
            node.broadcast_announce().await;
        }
        for _ in 0..5 {
            for node in &self.nodes {
                node.drive().await;
            }
            tokio::task::yield_now().await;
        }
    }

    pub async fn drive_all(&self) {
        for node in &self.nodes {
            node.drive().await;
        }
    }

    pub async fn broadcast_all_gossip(&self) {
        for node in &self.nodes {
            node.broadcast_gossip().await;
        }
    }

    pub async fn wait_converged(&self, timeout: std::time::Duration) {
        let start = tokio::time::Instant::now();
        while start.elapsed() < timeout {
            self.drive_all().await;
            self.broadcast_all_gossip().await;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    /// Remove a peer's inbox from the in-process mesh. Useful for
    /// simulating wire-level unreachability: the gossip cache still
    /// mentions the peer, so `select_destinations` will pick it, but
    /// the `InProcessSender` has nowhere to deliver the forward.
    pub fn sever_peer(&self, node_id: RouterNodeId) {
        self._peer_map.lock().unwrap().remove(&node_id);
    }

    /// Push a raw envelope directly into `target`'s inbox. Bypasses
    /// the transport so the test can craft envelopes that the
    /// production sender path doesn't normally produce (e.g., a
    /// RouterWithdraw from a phantom node).
    /// `sender` is the identity of the simulated sender (for handler trust checks).
    pub fn inject(&self, target: RouterNodeId, sender: RouterNodeId, envelope: Vec<u8>) {
        let map = self._peer_map.lock().unwrap();
        let tx = map
            .get(&target)
            .expect("target node not present in peer_map")
            .clone();
        tx.try_send((sender, envelope))
            .expect("inbox channel full or closed");
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        self.driver_cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.driver_handle.lock().unwrap().take() {
            handle.abort();
        }
    }
}

// ── helpers ────────────────────────────────────────────────────────

pub fn make_request(model: &str) -> RequestContext {
    RequestContext {
        model: model.to_string(),
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
    }
}
