use std::collections::BTreeMap;
use std::sync::Arc;

use octo_transport::sender::{NetworkSender, SendContext, TransportError};
use quota_router::announce::SignedPayload;
use quota_router::gossip::CapacityGossipPayload;
use quota_router::provider::{
    LocalProvider, ProviderCapacity, ProviderConfig, ProviderError, ProviderHealth, RouterNodeId,
};
use quota_router::request::RequestContext;
use quota_router::QuotaRouterNode;

pub type PeerMap =
    Arc<std::sync::Mutex<BTreeMap<RouterNodeId, tokio::sync::mpsc::Sender<Vec<u8>>>>>;

// ── MockLocalProvider ──────────────────────────────────────────────

pub struct MockLocalProvider {
    models: Vec<String>,
    health: ProviderHealth,
}

impl MockLocalProvider {
    pub fn new(models: Vec<String>) -> Self {
        Self {
            models,
            health: ProviderHealth::Healthy,
        }
    }

    pub fn with_health(mut self, health: ProviderHealth) -> Self {
        self.health = health;
        self
    }
}

#[async_trait::async_trait]
impl LocalProvider for MockLocalProvider {
    async fn completion(
        &self,
        model: &str,
        _messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        Ok(format!("response-{}", model).into_bytes())
    }

    async fn health_check(&self) -> ProviderHealth {
        self.health.clone()
    }

    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

// ── InProcessTransport ─────────────────────────────────────────────

pub struct InProcessTransport {
    peers: PeerMap,
    self_id: RouterNodeId,
}

impl InProcessTransport {
    pub fn new(peers: PeerMap, self_id: RouterNodeId) -> Self {
        Self { peers, self_id }
    }
}

#[async_trait::async_trait]
impl NetworkSender for InProcessTransport {
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
            let _ = sender.send(payload.to_vec());
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

// ── TestNode ───────────────────────────────────────────────────────

pub struct TestNode {
    pub node_id: RouterNodeId,
    pub node: Arc<tokio::sync::Mutex<QuotaRouterNode>>,
    pub network_key: [u8; 32],
    pub inbox_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    pub inbox_rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    peer_map: PeerMap,
}

impl TestNode {
    pub fn new(
        node_id: RouterNodeId,
        models: Vec<String>,
        peer_map: PeerMap,
        network_key: [u8; 32],
    ) -> Self {
        let (inbox_tx, inbox_rx) = tokio::sync::mpsc::channel(256);

        // Register this node's inbox in the shared peer map (synchronous)
        peer_map.lock().unwrap().insert(node_id, inbox_tx.clone());

        let node = QuotaRouterNode::builder()
            .node_id(node_id)
            .network_id(quota_router::provider::NetworkId([1u8; 32]))
            .policy(quota_router::request::RoutingPolicy::Balanced)
            .gossip_interval(std::time::Duration::from_secs(10))
            .provider(ProviderConfig {
                name: models[0].clone(),
                endpoint: "http://localhost".into(),
                auth: quota_router::provider::ProviderAuth::Local,
                models: models.clone(),
            })
            .build()
            .unwrap();

        let node = Arc::new(tokio::sync::Mutex::new(node));

        Self {
            node_id,
            node,
            network_key,
            inbox_tx,
            inbox_rx: tokio::sync::Mutex::new(inbox_rx),
            peer_map,
        }
    }

    pub async fn drive(&self) {
        let mut rx = self.inbox_rx.lock().await;
        while let Ok(payload) = rx.try_recv() {
            drop(rx);
            self.handle_payload(&payload).await;
            rx = self.inbox_rx.lock().await;
        }
    }

    async fn handle_payload(&self, payload: &[u8]) {
        if payload.is_empty() {
            return;
        }
        let disc = payload[0];
        match disc {
            0xC6 => {
                // CapacityGossip — skip discriminator byte
                if let Ok(gossip) = bincode::deserialize::<CapacityGossipPayload>(&payload[1..]) {
                    if gossip.verify_hmac(&self.network_key) {
                        let mut node = self.node.lock().await;
                        node.gossip_cache.merge(gossip.sender_id, gossip.capacities);
                        for peer_id in gossip.known_peers {
                            node.peer_cache.try_add(peer_id);
                        }
                    }
                }
            }
            0xCA => {
                // RouterAnnounce — skip discriminator byte
                if let Ok(announce) = bincode::deserialize::<
                    quota_router::announce::RouterAnnouncePayload,
                >(&payload[1..])
                {
                    if announce.verify_hmac(&self.network_key) {
                        let mut node = self.node.lock().await;
                        node.gossip_cache
                            .merge(announce.node_id, announce.capacities.clone());
                        node.peer_cache
                            .add_direct(announce.node_id, announce.capacities);
                    }
                }
            }
            0xCB => {
                // RouterWithdraw — skip discriminator byte
                if let Ok(withdraw) = bincode::deserialize::<
                    quota_router::announce::RouterWithdrawPayload,
                >(&payload[1..])
                {
                    if withdraw.verify_hmac(&self.network_key) {
                        let mut node = self.node.lock().await;
                        node.peer_cache.remove(withdraw.node_id);
                    }
                }
            }
            _ => {}
        }
    }

    pub async fn broadcast_announce(&self) {
        let node = self.node.lock().await;
        let models: Vec<String> = node.local_provider_models();
        let capacities: Vec<ProviderCapacity> = node
            .config
            .providers
            .iter()
            .map(|p| ProviderCapacity::from_config(p, node.config.node_id))
            .collect();
        let mut announce = quota_router::announce::RouterAnnouncePayload {
            node_id: node.config.node_id,
            network_id: node.config.network_id,
            supported_models: models,
            capacities,
            timestamp: quota_router::gossip::monotonic_now(),
            hmac: [0u8; 32],
        };
        announce.hmac = announce.compute_hmac(&self.network_key);
        let body = bincode::serialize(&announce).unwrap();
        // Prepend discriminator byte (0xCA = RouterAnnounce)
        let mut payload = vec![0xCAu8];
        payload.extend_from_slice(&body);
        drop(node);
        // Send to all peers except self
        let peers = self.peer_map.lock().unwrap();
        for (id, tx) in peers.iter() {
            if *id != self.node_id {
                let _ = tx.try_send(payload.clone());
            }
        }
    }

    pub async fn broadcast_gossip(&self) {
        let gossip = {
            let node = self.node.lock().await;
            node.build_capacity_gossip()
        };
        let body = bincode::serialize(&gossip).unwrap();
        // Prepend discriminator byte (0xC6 = CapacityGossip)
        let mut payload = vec![0xC6u8];
        payload.extend_from_slice(&body);
        // Send to all peers except self
        let peers = self.peer_map.lock().unwrap();
        eprintln!(
            "broadcast_gossip from {:?}: peer_map has {} entries",
            self.node_id,
            peers.len()
        );
        for (id, tx) in peers.iter() {
            if *id != self.node_id {
                let result = tx.try_send(payload.clone());
                eprintln!("  -> sent to {:?}: {:?}", id, result);
            }
        }
    }

    pub async fn route(
        &self,
        ctx: &RequestContext,
        payload: &[u8],
    ) -> Result<Vec<u8>, quota_router::RouterNodeError> {
        let node = self.node.lock().await;
        node.route(ctx, payload).await
    }

    pub async fn gossip_cache_snapshot(&self) -> Vec<(RouterNodeId, Vec<ProviderCapacity>)> {
        let node = self.node.lock().await;
        node.gossip_cache.snapshot()
    }

    pub async fn peer_count(&self) -> usize {
        let node = self.node.lock().await;
        node.peer_count()
    }
}

// ── Topology ───────────────────────────────────────────────────────

pub enum Topology {
    Star,
    Line,
    FullMesh,
}

// ── TestCluster ────────────────────────────────────────────────────

pub struct TestCluster {
    pub nodes: Vec<TestNode>,
    pub network_key: [u8; 32],
    peer_map: PeerMap,
}

impl TestCluster {
    pub fn new(n: usize, topology: Topology, model_sets: Vec<Vec<String>>) -> Self {
        // Derive network_key from network_id to match QuotaRouterNode::network_key()
        let network_id = [1u8; 32];
        let network_key = *blake3::hash(&network_id).as_bytes();
        let peer_map: PeerMap = Arc::new(std::sync::Mutex::new(BTreeMap::new()));

        let mut nodes = Vec::new();
        for i in 0..n {
            let node_id = RouterNodeId([(i + 1) as u8; 32]);
            let models = model_sets
                .get(i)
                .cloned()
                .unwrap_or_else(|| vec!["gpt-4o".into()]);
            nodes.push(TestNode::new(
                node_id,
                models,
                peer_map.clone(),
                network_key,
            ));
        }

        // Wire peers according to topology
        match topology {
            Topology::Star => {
                // All nodes connect to node 0 (no-op for in-process, peer_map handles routing)
            }
            Topology::Line => {}
            Topology::FullMesh => {}
        }

        Self {
            nodes,
            network_key,
            peer_map,
        }
    }

    pub async fn start_all(&self) {
        for node in &self.nodes {
            node.broadcast_announce().await;
        }
        // Drive all inboxes to process announces
        for _ in 0..3 {
            for node in &self.nodes {
                node.drive().await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
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
        loop {
            self.drive_all().await;
            self.broadcast_all_gossip().await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            if start.elapsed() > timeout {
                break;
            }
        }
    }
}

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
