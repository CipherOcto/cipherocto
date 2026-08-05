use std::sync::{Arc, Weak};

use async_trait::async_trait;

use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::{SendContext, TransportError};

use super::announce::{RouterAnnouncePayload, RouterWithdrawPayload, SignedPayload};
use super::forward::{
    ForwardRejectPayload, ForwardRejectReason, ForwardRequestPayload, ForwardResponsePayload,
};
use super::gossip::CapacityGossipPayload;
use super::provider::{LocalProvider, PeerTrust, ProviderCapacity, RouterNodeId};
use super::scorer::{Destination, SelectionState};
use super::QuotaRouterNode;
use super::{
    envelope, DISC_CAPACITY_GOSSIP, DISC_FORWARD_REJECT, DISC_FORWARD_REQUEST,
    DISC_FORWARD_RESPONSE,
};

fn deserialize<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, TransportError> {
    bincode::deserialize(bytes).map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))
}

pub struct QuotaRouterHandler {
    /// Back-reference to the owning node. Wrapped in `Mutex<Weak<...>>`
    /// rather than a bare `Weak` so that `QuotaRouterNode::node_mut()`
    /// can temporarily clear the weak and let `Arc::get_mut` succeed on
    /// the inner `Arc<QuotaRouterNode>`. Without this indirection, the
    /// weak keeps `Arc::get_mut` returning `None`.
    pub(crate) node: std::sync::Mutex<Weak<QuotaRouterNode>>,
    pub(crate) provider: Arc<dyn LocalProvider>,
    pub(crate) network_key: [u8; 32],
}

impl QuotaRouterHandler {
    /// Create a new handler wrapping the given node.
    ///
    /// The handler implements `NetworkReceiver` and processes inbound
    /// DOT envelopes (forward requests, gossip, announce, withdraw).
    /// The `Weak` reference (wrapped in a Mutex for release-ability)
    /// breaks the Arc cycle: the node owns the handler via
    /// `Arc<QuotaRouterHandler>` while the handler holds only a `Weak`
    /// back to the node — when the builder drops the node, the
    /// handler's `upgrade()` returns `None` and inbound dispatch
    /// becomes a no-op.
    pub fn new(
        node: Weak<QuotaRouterNode>,
        provider: Arc<dyn LocalProvider>,
        network_key: [u8; 32],
    ) -> Self {
        Self {
            node: std::sync::Mutex::new(node),
            provider,
            network_key,
        }
    }

    /// Upgrade the handler's `Weak` reference back to a strong `Arc`.
    /// Returns `Err` if the node has been dropped or the weak has been
    /// temporarily released for `Arc::get_mut`.
    fn upgrade_node(&self) -> Result<Arc<QuotaRouterNode>, TransportError> {
        self.node
            .lock()
            .unwrap()
            .upgrade()
            .ok_or_else(|| TransportError::AdapterFailure("node dropped".into()))
    }

    /// Temporarily clear the back-reference so `Arc::get_mut` can succeed
    /// on the inner `Arc<QuotaRouterNode>`. Returns the previously-stored
    /// weak so callers can restore it after mutation.
    pub(crate) fn release_back_ref(&self) -> Weak<QuotaRouterNode> {
        let mut guard = self.node.lock().unwrap();
        std::mem::replace(&mut *guard, Weak::new())
    }

    /// Restore a previously-released back-reference. After this call,
    /// inbound dispatch resumes reaching the node.
    pub(crate) fn restore_back_ref(&self, weak: Weak<QuotaRouterNode>) {
        *self.node.lock().unwrap() = weak;
    }
}

enum DropAction {
    Reject(ForwardRejectReason),
    LocalDispatch(ProviderCapacity),
    Forward,
}

#[async_trait]
impl NetworkReceiver for QuotaRouterHandler {
    async fn on_receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError> {
        let (discriminator, body) = payload
            .split_first()
            .ok_or_else(|| TransportError::EnvelopeConstruction("empty payload".into()))?;

        match discriminator {
            0xC3 => self.handle_forward_request(body, ctx).await,
            0xC4 => self.handle_forward_response(body).await,
            0xC5 => self.handle_forward_reject(body).await,
            0xC6 => self.handle_capacity_gossip(body).await,
            0xC7 => self.handle_capacity_request(body, ctx).await,
            0xCA => self.handle_router_announce(body).await,
            0xCB => self.handle_router_withdraw(body).await,
            _ => Ok(()),
        }
    }

    fn name(&self) -> &str {
        "quota-router-handler"
    }
}

impl QuotaRouterHandler {
    async fn handle_forward_request(
        &self,
        payload: &[u8],
        ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        let node = self.upgrade_node()?;
        let req: ForwardRequestPayload = deserialize(payload)?;

        // HMAC verification (RFC v1.10 / 0870d): the spec defaults to
        // `Trusted` (no verify) and only enforces on `Verified` peers.
        // We look the sender up by node ID — when `ReceiveContext.sender_id`
        // is present we can resolve it to a configured peer and check its
        // trust level. When absent (transports that don't authenticate),
        // we fall back to `Trusted` (skip verification).
        let sender_node_id: Option<RouterNodeId> = if let Some(sender_bytes) = ctx.sender_id {
            let sender = RouterNodeId(sender_bytes);
            let trust = node
                .config
                .peers
                .iter()
                .find(|p| p.node_id == sender)
                .map(|p| p.trust_level.clone())
                .unwrap_or(PeerTrust::Trusted);
            if trust == PeerTrust::Verified && !req.verify_hmac(&self.network_key) {
                return Err(TransportError::AdapterFailure(
                    "forward request HMAC mismatch".into(),
                ));
            }
            Some(sender)
        } else {
            None
        };

        // Per-peer rate limit (0870d acceptance criterion #4).
        // When the sender is identifiable we charge its bucket; otherwise
        // we fall back to a synthetic per-consumer bucket derived from
        // the consumer_id inside the request context.
        if let Some(sender) = sender_node_id {
            if !node.rate_limiter.lock().unwrap().check_peer(&sender) {
                return Err(TransportError::AdapterFailure(
                    "peer rate limit exceeded".into(),
                ));
            }
        } else if !node
            .rate_limiter
            .lock()
            .unwrap()
            .check_consumer(&req.context.consumer_id)
        {
            return Err(TransportError::AdapterFailure(
                "consumer rate limit exceeded".into(),
            ));
        }

        if req.ttl == 0 {
            self.send_forward_reject(req.request_id, ForwardRejectReason::TtlExpired)
                .await?;
            return Ok(());
        }

        let action = {
            let local: Vec<ProviderCapacity> = node
                .config
                .providers
                .iter()
                .map(|p| ProviderCapacity::from_config(p, node.config.node_id))
                .collect();
            let peer_caps = node.gossip_cache.lock().unwrap().snapshot();
            let selection = node.select_destinations_with_state(
                &req.context,
                &local,
                &peer_caps,
                &node.config.policy,
            );

            match selection {
                SelectionState::Matched(destinations) => match destinations.first() {
                    Some(Destination::Local { provider, .. }) => {
                        DropAction::LocalDispatch(provider.clone())
                    }
                    Some(Destination::Remote { .. }) => DropAction::Forward,
                    None => unreachable!(),
                },
                SelectionState::CapacityExhausted => {
                    DropAction::Reject(ForwardRejectReason::CapacityExhausted)
                }
                SelectionState::NoMatch => DropAction::Reject(ForwardRejectReason::NoProvider),
            }
        };

        match action {
            DropAction::Reject(reason) => {
                self.send_forward_reject(req.request_id, reason).await?;
            }
            DropAction::LocalDispatch(provider) => {
                let response = self
                    .provider
                    .completion(&req.context.model, &req.payload, &provider)
                    .await
                    .map_err(|e| TransportError::AdapterFailure(e.to_string()))?;
                self.send_forward_response(req.request_id, response).await?;
            }
            DropAction::Forward => {
                let fwd_bytes = {
                    let mut fwd = req.clone();
                    fwd.ttl -= 1;
                    fwd.hop_count += 1;
                    envelope(DISC_FORWARD_REQUEST, &fwd)?
                };
                node.transport
                    .send_best(&fwd_bytes, &SendContext::default())
                    .await?;
            }
        }

        Ok(())
    }

    async fn handle_forward_response(&self, payload: &[u8]) -> Result<(), TransportError> {
        let resp: ForwardResponsePayload = deserialize(payload)?;
        let node = self.upgrade_node()?;
        node.pending.complete(resp.request_id, resp.response);
        Ok(())
    }

    async fn handle_forward_reject(&self, payload: &[u8]) -> Result<(), TransportError> {
        let reject: ForwardRejectPayload = deserialize(payload)?;
        let node = self.upgrade_node()?;
        node.pending
            .reject(reject.request_id, reject.reason.clone());
        // Trigger pull-gossip so we learn the rejecting peer's fresh
        // capacity. This avoids repeatedly hitting a peer whose state
        // has changed. (RFC v1.10 / 0870d.)
        if matches!(reject.reason, ForwardRejectReason::CapacityExhausted) {
            node.request_capacity_from(reject.peer_id);
        }
        Ok(())
    }

    async fn handle_capacity_gossip(&self, payload: &[u8]) -> Result<(), TransportError> {
        let gossip: CapacityGossipPayload = deserialize(payload)?;
        if !gossip.verify_hmac(&self.network_key) {
            return Err(TransportError::AdapterFailure(
                "capacity gossip HMAC mismatch".into(),
            ));
        }
        let node = self.upgrade_node()?;
        node.gossip_cache
            .lock()
            .unwrap()
            .merge(gossip.sender_id, gossip.capacities);
        for peer_id in gossip.known_peers {
            node.peer_cache.lock().unwrap().try_add(peer_id);
        }
        Ok(())
    }

    async fn handle_router_announce(&self, payload: &[u8]) -> Result<(), TransportError> {
        let announce: RouterAnnouncePayload = deserialize(payload)?;
        if !announce.verify_hmac(&self.network_key) {
            return Err(TransportError::AdapterFailure(
                "router announce HMAC mismatch".into(),
            ));
        }
        let node = self.upgrade_node()?;
        let local_models: Vec<String> = node.local_provider_models();
        let has_overlap = announce
            .supported_models
            .iter()
            .any(|m| local_models.contains(m));
        if has_overlap {
            // Merge the announce's capacities into the gossip cache so
            // they participate in destination scoring on the very next
            // route call. Previously `add_direct` accepted capacities
            // but discarded them (Round 1 finding #11) — that lost the
            // peer's model availability data entirely.
            node.gossip_cache
                .lock()
                .unwrap()
                .merge(announce.node_id, announce.capacities.clone());
            node.peer_cache
                .lock()
                .unwrap()
                .add_direct(announce.node_id, announce.capacities);
        }
        Ok(())
    }

    async fn handle_capacity_request(
        &self,
        _payload: &[u8],
        _ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        let node = self.upgrade_node()?;
        let gossip = node.build_capacity_gossip();
        let payload_bytes = envelope(DISC_CAPACITY_GOSSIP, &gossip)?;
        node.transport
            .send_best(&payload_bytes, &SendContext::default())
            .await
    }

    async fn handle_router_withdraw(&self, payload: &[u8]) -> Result<(), TransportError> {
        let withdraw: RouterWithdrawPayload = deserialize(payload)?;
        if !withdraw.verify_hmac(&self.network_key) {
            return Err(TransportError::AdapterFailure(
                "router withdraw HMAC mismatch".into(),
            ));
        }
        let node = self.upgrade_node()?;
        node.peer_cache.lock().unwrap().remove(withdraw.node_id);
        Ok(())
    }

    async fn send_forward_response(
        &self,
        request_id: [u8; 32],
        response: Vec<u8>,
    ) -> Result<(), TransportError> {
        // v1: uses send_best which broadcasts. F8 (per-peer routing)
        // will replace with targeted send to origin_node.
        let node = self.upgrade_node()?;
        let payload = ForwardResponsePayload {
            request_id,
            response,
            executed_by: node.primary_provider_id(),
            latency_ms: 0,
        };
        let payload_bytes = envelope(DISC_FORWARD_RESPONSE, &payload)?;
        node.transport
            .send_best(&payload_bytes, &SendContext::default())
            .await
    }

    async fn send_forward_reject(
        &self,
        request_id: [u8; 32],
        reason: ForwardRejectReason,
    ) -> Result<(), TransportError> {
        let node = self.upgrade_node()?;
        let payload = ForwardRejectPayload {
            request_id,
            peer_id: node.config.node_id,
            reason,
        };
        let payload_bytes = envelope(DISC_FORWARD_REJECT, &payload)?;
        node.transport
            .send_best(&payload_bytes, &SendContext::default())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::provider::{ProviderAuth, ProviderConfig, RouterNodeId};
    use crate::node::request::{ForwardingConfig, RequestContext, RoutingPolicy};
    use crate::node::QuotaRouterNode;
    use std::sync::Arc;

    fn make_node() -> Arc<QuotaRouterNode> {
        QuotaRouterNode::builder()
            .node_id(RouterNodeId([1u8; 32]))
            .network_id(crate::node::provider::NetworkId([2u8; 32]))
            .provider(ProviderConfig {
                name: "openai".into(),
                endpoint: "https://api.openai.com".into(),
                auth: ProviderAuth::ApiKey("test".into()),
                models: vec!["gpt-4o".into()],
            })
            .policy(RoutingPolicy::Balanced)
            .forwarding(ForwardingConfig::default())
            .build()
            .unwrap()
    }

    fn make_ctx() -> ReceiveContext {
        ReceiveContext {
            source_transport: "test".into(),
            mission_id: [0u8; 32],
            sender_id: None,
        }
    }

    fn make_request_ctx(model: &str) -> RequestContext {
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

    #[tokio::test]
    async fn handler_unknown_discriminator_is_ok() {
        let node = make_node();
        let ctx = make_ctx();
        let r = node.receive(&[0xFF], &ctx).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn handler_empty_payload_is_err() {
        let node = make_node();
        let ctx = make_ctx();
        let r = node.receive(&[], &ctx).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn handle_forward_request_ttl_zero_rejects() {
        let node = make_node();
        let ctx = make_ctx();
        let req = super::super::forward::ForwardRequestPayload {
            request_id: [1u8; 32],
            network_id: crate::node::provider::NetworkId([2u8; 32]),
            context: make_request_ctx("gpt-4o"),
            payload: b"test".to_vec(),
            ttl: 0,
            origin_node: RouterNodeId([3u8; 32]),
            hop_count: 0,
            created_at: 0,
            hmac: [0u8; 32],
        };
        let payload = envelope(DISC_FORWARD_REQUEST, &req).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn handle_forward_request_no_provider_rejects() {
        let node = make_node();
        let ctx = make_ctx();
        let req = super::super::forward::ForwardRequestPayload {
            request_id: [2u8; 32],
            network_id: crate::node::provider::NetworkId([2u8; 32]),
            context: make_request_ctx("unsupported-model"),
            payload: b"test".to_vec(),
            ttl: 3,
            origin_node: RouterNodeId([3u8; 32]),
            hop_count: 0,
            created_at: 0,
            hmac: [0u8; 32],
        };
        let payload = envelope(DISC_FORWARD_REQUEST, &req).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn handle_forward_request_local_dispatch() {
        let node = make_node();
        let ctx = make_ctx();
        let req = super::super::forward::ForwardRequestPayload {
            request_id: [3u8; 32],
            network_id: crate::node::provider::NetworkId([2u8; 32]),
            context: make_request_ctx("gpt-4o"),
            payload: b"test".to_vec(),
            ttl: 3,
            origin_node: RouterNodeId([3u8; 32]),
            hop_count: 0,
            created_at: 0,
            hmac: [0u8; 32],
        };
        let payload = envelope(DISC_FORWARD_REQUEST, &req).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn handle_capacity_gossip_invalid_hmac_rejects() {
        let node = make_node();
        let ctx = make_ctx();
        let gossip = super::super::gossip::CapacityGossipPayload {
            sender_id: RouterNodeId([5u8; 32]),
            timestamp: 100,
            capacities: vec![],
            known_peers: vec![],
            hmac: [0u8; 32],
        };
        let payload = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
        let r = node.receive(&payload, &ctx).await;
        // HMAC mismatch → Err
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn handle_capacity_gossip_valid_hmac_merges() {
        let node = make_node();
        let network_key = *blake3::hash(node.config.network_id.0.as_ref()).as_bytes();
        let ctx = make_ctx();
        let mut gossip = super::super::gossip::CapacityGossipPayload {
            sender_id: RouterNodeId([5u8; 32]),
            timestamp: 100,
            capacities: vec![],
            known_peers: vec![],
            hmac: [0u8; 32],
        };
        gossip.hmac = gossip.compute_hmac(&network_key);
        let payload = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn handle_capacity_request_replies() {
        let node = make_node();
        let ctx = make_ctx();
        let req = super::super::forward::CapacityRequestPayload {
            requester_id: RouterNodeId([5u8; 32]),
        };
        // 0xC7 is capacity request discriminator
        let payload = envelope(super::super::DISC_CAPACITY_REQUEST, &req).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn handle_router_announce_invalid_hmac_rejects() {
        let node = make_node();
        let ctx = make_ctx();
        let announce = super::super::announce::RouterAnnouncePayload {
            node_id: RouterNodeId([6u8; 32]),
            network_id: crate::node::provider::NetworkId([2u8; 32]),
            supported_models: vec!["gpt-4o".into()],
            capacities: vec![],
            timestamp: 100,
            hmac: [0u8; 32],
        };
        let payload = envelope(super::super::DISC_ROUTER_ANNOUNCE, &announce).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn handle_router_withdraw_invalid_hmac_rejects() {
        let node = make_node();
        let ctx = make_ctx();
        let withdraw = super::super::announce::RouterWithdrawPayload {
            node_id: RouterNodeId([7u8; 32]),
            reason: super::super::announce::WithdrawReason::Graceful,
            timestamp: 100,
            hmac: [0u8; 32],
        };
        let payload = envelope(super::super::DISC_ROUTER_WITHDRAW, &withdraw).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn handle_forward_response_unknown_id_is_ok() {
        let node = make_node();
        let ctx = make_ctx();
        let resp = super::super::forward::ForwardResponsePayload {
            request_id: [99u8; 32], // unknown request_id
            response: b"result".to_vec(),
            executed_by: super::super::provider::ProviderId([1u8; 32]),
            latency_ms: 100,
        };
        let payload = envelope(DISC_FORWARD_RESPONSE, &resp).unwrap();
        let r = node.receive(&payload, &ctx).await;
        assert!(r.is_ok());
    }
}
