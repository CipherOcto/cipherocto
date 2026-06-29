use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::{SendContext, TransportError};

use super::announce::{RouterAnnouncePayload, RouterWithdrawPayload, SignedPayload};
use super::forward::{
    ForwardRejectPayload, ForwardRejectReason, ForwardRequestPayload, ForwardResponsePayload,
};
use super::gossip::CapacityGossipPayload;
use super::provider::{LocalProvider, PeerTrust, ProviderCapacity, RouterNodeId};
use super::scorer::Destination;
use super::QuotaRouterNode;

fn serialize<T: serde::Serialize>(v: &T) -> Result<Vec<u8>, TransportError> {
    bincode::serialize(v).map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))
}

fn deserialize<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, TransportError> {
    bincode::deserialize(bytes).map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))
}

pub struct QuotaRouterHandler {
    pub(crate) node: Arc<Mutex<QuotaRouterNode>>,
    pub(crate) provider: Arc<dyn LocalProvider>,
    pub(crate) network_key: [u8; 32],
    pub(crate) transport: Arc<octo_transport::node_transport::NodeTransport>,
}

impl QuotaRouterHandler {
    /// Create a new handler wrapping the given node and transport.
    ///
    /// The handler implements `NetworkReceiver` and processes inbound
    /// DOT envelopes (forward requests, gossip, announce, withdraw).
    pub fn new(
        node: Arc<Mutex<QuotaRouterNode>>,
        provider: Arc<dyn LocalProvider>,
        network_key: [u8; 32],
        transport: Arc<octo_transport::node_transport::NodeTransport>,
    ) -> Self {
        Self {
            node,
            provider,
            network_key,
            transport,
        }
    }
}

enum DropAction {
    Reject,
    LocalDispatch(ProviderCapacity),
    Forward,
}

#[async_trait]
impl NetworkReceiver for QuotaRouterHandler {
    async fn on_receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError> {
        let discriminator = payload
            .first()
            .copied()
            .ok_or_else(|| TransportError::EnvelopeConstruction("empty payload".into()))?;

        match discriminator {
            0xC3 => self.handle_forward_request(payload, ctx).await,
            0xC4 => self.handle_forward_response(payload).await,
            0xC5 => self.handle_forward_reject(payload).await,
            0xC6 => self.handle_capacity_gossip(payload).await,
            0xC7 => self.handle_capacity_request(payload, ctx).await,
            0xCA => self.handle_router_announce(payload).await,
            0xCB => self.handle_router_withdraw(payload).await,
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
        let req: ForwardRequestPayload = deserialize(payload)?;

        // HMAC verification (RFC v1.10 / 0870d): the spec defaults to
        // `Trusted` (no verify) and only enforces on `Verified` peers.
        // We look the sender up by node ID — when `ReceiveContext.sender_id`
        // is present we can resolve it to a configured peer and check its
        // trust level. When absent (transports that don't authenticate),
        // we fall back to `Trusted` (skip verification).
        let sender_node_id: Option<RouterNodeId> = if let Some(sender_bytes) = ctx.sender_id {
            let sender = RouterNodeId(sender_bytes);
            let trust = {
                let node = self.node.lock().unwrap();
                node.config
                    .peers
                    .iter()
                    .find(|p| p.node_id == sender)
                    .map(|p| p.trust_level.clone())
                    .unwrap_or(PeerTrust::Trusted)
            };
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
        {
            let node = self.node.lock().unwrap();
            if let Some(sender) = sender_node_id {
                if !node.rate_limiter.check_peer(&sender) {
                    return Err(TransportError::AdapterFailure(
                        "peer rate limit exceeded".into(),
                    ));
                }
            } else if !node.rate_limiter.check_consumer(&req.context.consumer_id) {
                return Err(TransportError::AdapterFailure(
                    "consumer rate limit exceeded".into(),
                ));
            }
        }

        if req.ttl == 0 {
            self.send_forward_reject(req.request_id, ForwardRejectReason::TtlExpired)
                .await?;
            return Ok(());
        }

        let action = {
            let node = self.node.lock().unwrap();
            let local: Vec<ProviderCapacity> = node
                .config
                .providers
                .iter()
                .map(|p| ProviderCapacity::from_config(p, node.config.node_id))
                .collect();
            let peer_caps = node.gossip_cache.snapshot();
            let destinations =
                node.select_destinations(&req.context, &local, &peer_caps, &node.config.policy);

            if destinations.is_empty() {
                DropAction::Reject
            } else {
                match destinations.first() {
                    Some(Destination::Local { provider, .. }) => {
                        DropAction::LocalDispatch(provider.clone())
                    }
                    Some(Destination::Remote { .. }) => DropAction::Forward,
                    None => unreachable!(),
                }
            }
        };

        match action {
            DropAction::Reject => {
                self.send_forward_reject(req.request_id, ForwardRejectReason::NoProvider)
                    .await?;
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
                    serialize(&fwd)?
                };
                self.transport
                    .send_best(&fwd_bytes, &SendContext::default())
                    .await?;
            }
        }

        Ok(())
    }

    async fn handle_forward_response(&self, payload: &[u8]) -> Result<(), TransportError> {
        let resp: ForwardResponsePayload = deserialize(payload)?;
        let node = self.node.lock().unwrap();
        node.pending.complete(resp.request_id, resp.response);
        Ok(())
    }

    async fn handle_forward_reject(&self, payload: &[u8]) -> Result<(), TransportError> {
        let reject: ForwardRejectPayload = deserialize(payload)?;
        let node = self.node.lock().unwrap();
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
        let mut node = self.node.lock().unwrap();
        node.gossip_cache.merge(gossip.sender_id, gossip.capacities);
        for peer_id in gossip.known_peers {
            node.peer_cache.try_add(peer_id);
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
        let mut node = self.node.lock().unwrap();
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
                .merge(announce.node_id, announce.capacities.clone());
            node.peer_cache
                .add_direct(announce.node_id, announce.capacities);
        }
        Ok(())
    }

    async fn handle_capacity_request(
        &self,
        _payload: &[u8],
        _ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        let payload_bytes = {
            let node = self.node.lock().unwrap();
            let gossip = node.build_capacity_gossip();
            serialize(&gossip)?
        };
        self.transport
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
        let mut node = self.node.lock().unwrap();
        node.peer_cache.remove(withdraw.node_id);
        Ok(())
    }

    async fn send_forward_response(
        &self,
        request_id: [u8; 32],
        response: Vec<u8>,
    ) -> Result<(), TransportError> {
        // v1: uses send_best which broadcasts. F8 (per-peer routing)
        // will replace with targeted send to origin_node.
        let payload_bytes = {
            let node = self.node.lock().unwrap();
            let payload = ForwardResponsePayload {
                request_id,
                response,
                executed_by: node.primary_provider_id(),
                latency_ms: 0,
            };
            serialize(&payload)?
        };
        self.transport
            .send_best(&payload_bytes, &SendContext::default())
            .await
    }

    async fn send_forward_reject(
        &self,
        request_id: [u8; 32],
        reason: ForwardRejectReason,
    ) -> Result<(), TransportError> {
        let payload_bytes = {
            let node = self.node.lock().unwrap();
            let payload = ForwardRejectPayload {
                request_id,
                peer_id: node.config.node_id,
                reason,
            };
            serialize(&payload)?
        };
        self.transport
            .send_best(&payload_bytes, &SendContext::default())
            .await
    }
}
