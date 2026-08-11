//! `IdentityResolverNode` — the identity-resolver specialized-node adapter
//! (RFC-0871 §Roles and Authorities).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-ident`) and registers as a `NetworkReceiver` via the Layer D
//! transport (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871b-storage-backend
//!
//! `IdentityResolverNodeConfig.registry: Arc<dyn DidRegistry>` slot
//! wires the production storage backend (`StoolapDidRegistry`) without
//! coupling this Layer C crate to `quota-router-storage`. Default
//! construction falls back to `InMemoryDidRegistry` for tests + Phase 1
//! deployments.

use std::sync::Arc;

use async_trait::async_trait;
use octo_ident::{DidRegistry, InMemoryDidRegistry};
use octo_protocol::dispatch::ReferenceDispatcher;
use octo_protocol::payload_kind::PayloadKindId;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::NodeEnvelope;
use octo_protocol::ProtocolError;
use octo_protocol::SystemClock;
use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::TransportError;
use octo_transport::NodeTransport;

use crate::handlers::{resolver_error_to_protocol, ResolveHandler, ResolveRequest};
use crate::is_identity_resolver_payload_kind;

/// Identity-resolver node configuration.
#[derive(Clone)]
pub struct IdentityResolverNodeConfig {
    /// Network transport (RFC-0863 NodeTransport).
    pub transport: Arc<NodeTransport>,
    /// HSM-routed identity (mission 0959-placeholder-identity-binding).
    /// Resolver nodes are read-only by default; identity is optional
    /// and used only to bind the announce's `RouterNodeId` to a
    /// trustworthy key. Production deployments inject a real
    /// resolver identity via this slot.
    pub identity: Option<Arc<octo_wallet::identity::IdentityKey>>,
    /// Network key for `RouterAnnouncePayload` HMAC anti-spoof.
    pub network_key: [u8; 32],
    /// Mission 0871b-storage-backend: production DID-document registry.
    /// When `None` at construction time, `IdentityResolverNode::new`
    /// defaults to `InMemoryDidRegistry` (test + Phase 1 deployments).
    /// Production deployments pass `Arc::new(StoolapDidRegistry::open_path(...))`.
    /// Injected via `Arc<dyn DidRegistry>` — no `quota-router-storage`
    /// dep at this Layer C crate (registry is a trait-object boundary).
    pub registry: Option<Arc<dyn DidRegistry>>,
}

/// Opaque handle returned by `IdentityResolverNode::start()`.
#[derive(Clone, Debug)]
pub struct IdentityResolverNodeHandle {
    pub(crate) _private: (),
}

/// Identity-resolver node errors.
#[derive(Debug, thiserror::Error)]
pub enum IdentityResolverNodeError {
    /// `start()` called when transport is already registered.
    #[error("already started")]
    AlreadyStarted,
    /// `start()` called when payload-kind service is misconfigured.
    #[error("payload kind {0:?} not an identity-resolver payload kind")]
    UnknownPayloadKind(PayloadKindId),
}

/// Wall-clock + cache-backed dispatcher construction helper.
pub fn default_dispatcher() -> ReferenceDispatcher {
    ReferenceDispatcher::new(
        octo_protocol::dispatch::ValidationCache::new(),
        Box::new(SystemClock),
        octo_protocol::dispatch::DispatcherConfig::permissive(),
    )
}

/// Identity-resolver specialized-node adapter.
pub struct IdentityResolverNode {
    config: IdentityResolverNodeConfig,
    /// Cached resolved registry (either the injected one or the default
    /// `InMemoryDidRegistry`). Cloned into handler instances per request.
    registry: Arc<dyn DidRegistry>,
    dispatcher: ReferenceDispatcher,
    started: std::sync::atomic::AtomicBool,
}

impl IdentityResolverNode {
    /// Construct a new `IdentityResolverNode`.
    ///
    /// Mission 0871b-storage-backend: if `config.registry` is `None`,
    /// defaults to `Arc::new(InMemoryDidRegistry::default())`. This
    /// keeps backward-compat with Phase 1 MVP callers that pre-date the
    /// storage substrate while allowing production deployments to inject
    /// `Arc::new(StoolapDidRegistry::open_path(...))`.
    #[must_use]
    pub fn new(config: IdentityResolverNodeConfig) -> Self {
        let registry = config
            .registry
            .clone()
            .unwrap_or_else(|| Arc::new(InMemoryDidRegistry::default()));
        Self {
            config,
            registry,
            dispatcher: default_dispatcher(),
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Construct a new `IdentityResolverNode` with a custom dispatcher.
    #[must_use]
    pub fn with_dispatcher(
        config: IdentityResolverNodeConfig,
        dispatcher: ReferenceDispatcher,
    ) -> Self {
        let registry = config
            .registry
            .clone()
            .unwrap_or_else(|| Arc::new(InMemoryDidRegistry::default()));
        Self {
            config,
            registry,
            dispatcher,
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Register the node as a `NetworkReceiver` on the underlying transport.
    ///
    /// After `start()`, the transport routes any incoming borsh-encoded
    /// `NodeEnvelope` whose `payload_kind` is `IDENTITY_RESOLVE` to this
    /// node's `on_receive` method.
    ///
    /// # Errors
    /// Returns `IdentityResolverNodeError::AlreadyStarted` if already registered.
    /// Returns `IdentityResolverNodeError::UnknownPayloadKind` for any payload
    /// kind in the identity-resolver namespace that the dispatcher doesn't serve.
    pub fn start(&self) -> Result<IdentityResolverNodeHandle, IdentityResolverNodeError> {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(IdentityResolverNodeError::AlreadyStarted);
        }
        for kind in crate::IDENTITY_RESOLVER_PAYLOAD_KINDS {
            if !is_identity_resolver_payload_kind(kind) {
                return Err(IdentityResolverNodeError::UnknownPayloadKind(*kind));
            }
        }
        self.config
            .transport
            .register_receiver(self_clone_to_receiver(self));
        Ok(IdentityResolverNodeHandle { _private: () })
    }

    /// Dispatch an inbound envelope to the appropriate handler.
    ///
    /// Verification order:
    /// 1. `ReferenceDispatcher::verify_all` — `Vec<Authorization>` logical-AND
    ///    (RFC-0871 §Adversary Analysis A6). envelope_id dedup + expiry + TTL
    ///    ceiling enforced by the dispatcher's full flow (`dispatch`); here we
    ///    only verify authz because the transport layer already routed by
    ///    payload kind.
    /// 2. `payload_kind` UUID lookup → handler map.
    /// 3. Handler returns `HandlerOutput` (response envelope payload).
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` for unsupported
    /// payload kinds, borsh decode failures, or authorization failures.
    pub fn handle_envelope(
        &self,
        envelope: &NodeEnvelope,
    ) -> Result<crate::handlers::HandlerOutput, ProtocolError> {
        // 1. Verify authorizations (RFC-0871 §Adversary Analysis A6).
        self.dispatcher.verify_all(envelope)?;

        // 2. Dispatch by payload kind.
        match envelope.payload_kind {
            k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE => {
                let req = ResolveRequest::from_borsh(&envelope.payload)
                    .map_err(resolver_error_to_protocol)?;
                ResolveHandler::new(self.registry.clone())
                    .handle(&req)
                    .map_err(resolver_error_to_protocol)
            }
            _ => Err(ProtocolError::AuthorizationFailed(format!(
                "unsupported payload kind: {:?}",
                envelope.payload_kind
            ))),
        }
    }

    /// Broadcast an `IDENTITY_RESOLVE` announce to the network via the
    /// transport's broadcast channel.
    ///
    /// Phase 1 MVP: announce is a stub `RouterAnnouncePayload`-shaped
    /// envelope body that lists the `IDENTITY_RESOLVE` payload kind as
    /// supported. The full RFC-0870 `RouterAnnouncePayload` extension
    /// shape (per RFC-0871 §Roles and Authorities) lands in mission
    /// 0870-b follow-on.
    pub async fn broadcast_announce(&self) -> Result<usize, TransportError> {
        // Mission 0871e-phase5c: emit canonical RouterAnnouncePayload
        // (replaces the Phase 1 MVP
        // `CIPHEROCTO_IDENTITY_RESOLVER_ANNOUNCE_V1:1_payload_kind`
        // stub bytes).
        // Mission 0959-placeholder-identity-binding: when `identity`
        // is present, derive real `RouterNodeId` + sign HMAC.
        use quota_router_core::node::announce::{
            PricingPolicy, RouterAnnouncePayload, SignedPayload,
        };
        let pk = self
            .config
            .identity
            .as_ref()
            .map(|i| i.public_key_bytes())
            .unwrap_or([0u8; 32]);
        let mut announce = RouterAnnouncePayload {
            node_id: quota_router_core::node::provider::RouterNodeId(pk),
            network_id: quota_router_core::node::provider::NetworkId([0u8; 32]),
            supported_models: vec![],
            capacities: vec![],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            hmac: [0u8; 32],
            pricing_policy: Some(PricingPolicy {
                drain_per_query: 0,
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }),
        };
        if self.config.network_key != [0u8; 32] {
            announce.hmac = announce.compute_hmac(&self.config.network_key);
        }
        let announce_body = serde_json::to_vec(&announce)
            .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        let from_did =
            match octo_protocol::wire_did(&format!("did:octo:z{}", bs58::encode(pk).into_string()))
            {
                Ok(d) => d,
                Err(_) => {
                    return Err(TransportError::EnvelopeConstruction(
                        "resolver announce: failed to construct placeholder from_did".into(),
                    ))
                }
            };
        let envelope = NodeEnvelope::build(
            from_did,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::IDENTITY_RESOLVE,
            announce_body,
            vec![],
            [0u8; 32],
            0,
        )
        .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        let bytes = borsh::to_vec(&envelope)
            .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        Ok(self
            .config
            .transport
            .broadcast(&bytes, &Default::default())
            .await)
    }

    /// Borrow the transport.
    #[must_use]
    pub fn transport(&self) -> &Arc<NodeTransport> {
        &self.config.transport
    }

    /// Borrow the registry (mission 0871b-storage-backend test surface).
    #[must_use]
    pub fn registry(&self) -> &Arc<dyn DidRegistry> {
        &self.registry
    }
}

/// Helper: `IdentityResolverNode` -> `Arc<dyn NetworkReceiver>`.
///
/// `IdentityResolverNode` itself is not `NetworkReceiver`-shaped (it's a plain
/// struct). This wrapper implements the trait and delegates to
/// `IdentityResolverNode::handle_envelope`.
struct IdentityResolverNodeReceiver {
    node: Arc<IdentityResolverNode>,
}

#[async_trait]
impl NetworkReceiver for IdentityResolverNodeReceiver {
    async fn on_receive(
        &self,
        payload: &[u8],
        _ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        let envelope: NodeEnvelope = borsh::from_slice(payload)
            .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        self.node
            .handle_envelope(&envelope)
            .map(|_| ())
            .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))
    }

    fn name(&self) -> &str {
        "identity-resolver-node"
    }
}

fn self_clone_to_receiver(node: &IdentityResolverNode) -> Arc<dyn NetworkReceiver> {
    // Wrap in Arc — `IdentityResolverNode` is not Clone, so we move the
    // configuration into a new Arc<IdentityResolverNode> for the receiver.
    let arc = Arc::new(IdentityResolverNode {
        config: node.config.clone(),
        registry: node.registry.clone(),
        dispatcher: default_dispatcher(),
        started: std::sync::atomic::AtomicBool::new(true),
    });
    Arc::new(IdentityResolverNodeReceiver { node: arc })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::{DidCodec, DidDocument};

    fn fresh_transport() -> Arc<NodeTransport> {
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        Arc::new(NodeTransport::new(senders))
    }

    #[test]
    fn resolver_node_constructs_and_starts() {
        let transport = fresh_transport();
        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: None,
        };
        let node = IdentityResolverNode::new(cfg);
        let handle = node.start().expect("start should succeed");
        let _ = handle;
    }

    #[test]
    fn resolver_node_rejects_double_start() {
        let transport = fresh_transport();
        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: None,
        };
        let node = IdentityResolverNode::new(cfg);
        let _ = node.start();
        let err = node.start().unwrap_err();
        assert!(matches!(err, IdentityResolverNodeError::AlreadyStarted));
    }

    #[test]
    fn handle_envelope_rejects_unsupported_payload_kind() {
        let transport = fresh_transport();
        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: None,
        };
        let node = IdentityResolverNode::new(cfg);
        // Build a payload with an unknown payload kind (random 16-byte UUID).
        let unknown_kind = PayloadKindId([0xAB; 16]);
        // Use a valid canonical DID for the from_did field.
        let canonical = "did:octo:zCt5bENb7tA2b9xeamSEnHF7cZ6Kk8h9p2Z6nT8pVk9R";
        let envelope = NodeEnvelope::build(
            octo_ident::WireDid::new(canonical.to_owned()),
            RecipientRef::Broadcast,
            unknown_kind,
            vec![0x01],
            vec![],
            [0u8; 32],
            u64::MAX,
        )
        .unwrap();
        let err = node.handle_envelope(&envelope).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    #[test]
    fn handle_envelope_uses_injected_registry_when_provided() {
        // Mission 0871b-storage-backend TV: `resolve_handler_uses_registry`.
        // Inject a custom registry returning a distinct `public_key`;
        // handler returns THAT key, NOT the placeholder hash.
        let transport = fresh_transport();
        let mut pk_bytes = [0u8; 32];
        for (i, b) in pk_bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        let raw = octo_ident::CanonicalCodec::mint(&pk_bytes);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        let wire_str = wire.as_str().to_owned();

        let registry = Arc::new(InMemoryDidRegistry::default());
        let custom_pubkey = [0xCCu8; 32];
        registry
            .register(
                &raw.hash,
                DidDocument {
                    public_key: custom_pubkey,
                    revoked: false,
                },
            )
            .unwrap();

        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: Some(registry.clone()),
        };
        let node = IdentityResolverNode::new(cfg);
        let req = ResolveRequest(wire_str.clone());
        let payload = req.to_borsh().unwrap();
        let envelope = NodeEnvelope::build(
            wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::IDENTITY_RESOLVE,
            payload,
            vec![],
            [0u8; 32],
            u64::MAX,
        )
        .unwrap();
        let out = node
            .handle_envelope(&envelope)
            .expect("handle should succeed");
        let resp_bytes = out.response_payload.expect("response payload present");
        let resp: crate::handlers::ResolveResponse = borsh::from_slice(&resp_bytes).unwrap();
        assert_eq!(resp.canonical_did, wire_str);
        assert_eq!(resp.public_key, custom_pubkey);
    }

    #[test]
    fn handle_envelope_defaults_to_in_memory_registry_when_omitted() {
        // Mission 0871b-storage-backend backward-compat: Phase 1 MVP callers
        // that pre-date the storage substrate still work — but resolve
        // returns `Storage("unknown DID")` because the default registry is
        // empty.
        let transport = fresh_transport();
        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: None,
        };
        let node = IdentityResolverNode::new(cfg);
        // Pre-register the DID so the empty-registry case doesn't trip.
        let mut pk_bytes = [0u8; 32];
        for (i, b) in pk_bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        let raw = octo_ident::CanonicalCodec::mint(&pk_bytes);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        let wire_str = wire.as_str().to_owned();
        node.registry()
            .register(
                &raw.hash,
                DidDocument {
                    public_key: raw.hash,
                    revoked: false,
                },
            )
            .unwrap();

        let req = ResolveRequest(wire_str.clone());
        let payload = req.to_borsh().unwrap();
        let envelope = NodeEnvelope::build(
            wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::IDENTITY_RESOLVE,
            payload,
            vec![],
            [0u8; 32],
            u64::MAX,
        )
        .unwrap();
        let out = node
            .handle_envelope(&envelope)
            .expect("handle should succeed");
        let resp_bytes = out.response_payload.expect("response payload present");
        let resp: crate::handlers::ResolveResponse = borsh::from_slice(&resp_bytes).unwrap();
        assert_eq!(resp.canonical_did, wire_str);
        // Pre-cutover placeholder public_key (RawDid::hash) survives the
        // cutover when the registry returns it explicitly — regression guard
        // for `wire_shape_byte_exact_across_cutover` TV.
        assert_eq!(resp.public_key, raw.hash);
    }
}
