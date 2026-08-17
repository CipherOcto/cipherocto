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
//!
//! ## Mission 0871e-f7-impl-resolver-mediation
//!
//! `IdentityResolverNodeConfig.write_coordinator: Option<Arc<dyn
//! DidWriteCoordinator>>` slot wires the cross-instance write
//! coordination substrate (RFC-0862 v1.3 §DidWriteCoordinator) without
//! coupling this Layer C crate to the future `octo-sync` crate. When
//! `None`, the resolver-node refuses `IDENTITY_REGISTER` +
//! `IDENTITY_REVOKE` with `IdentityResolveError::CoordinatorUnavailable`
//! (fail-closed per RFC-0862 v1.3 R12). Production HA / sharded
//! deployments inject a concrete coordinator; single-instance
//! deployments may legitimately leave this slot `None` (writes are
//! refused, which is the safe default for an unconfigured cluster).
//!
//! ## Mission 0871b-cross-domain-resolution-impl
//!
//! `IDENTITY_RESOLVE_CHAIN` adds multi-hop DID resolution. The chain
//! handler walks `Vec<ResolverHop>` with cycle detection + TTL budget
//! against the local `DidRegistry`. Cross-node forwarding (network
//! call hop N → hop N+1) requires a request/response substrate that
//! does not yet exist in `octo-transport` — the chain-traversal LOGIC
//! lands here; network forwarding lands in a follow-on mission when
//! the substrate is available.

use std::sync::Arc;

use async_trait::async_trait;
use octo_ident::{ChainId, DidRegistry, DidWriteCoordinator, InMemoryDidRegistry};
use octo_protocol::dispatch::ReferenceDispatcher;
use octo_protocol::envelope::VERSION_TAG_V2;
use octo_protocol::payload_kind::PayloadKindId;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::NodeEnvelope;
use octo_protocol::ProtocolError;
use octo_protocol::SystemClock;
use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::TransportError;
use octo_transport::NodeTransport;

use crate::handlers::{
    resolver_error_to_protocol, ChainResolveRequest, LocalResolverBackend, RegisterHandler,
    RegisterRequest, ResolveChainHandler, ResolveHandler, ResolveRequest, ResolveWithChainHandler,
    ResolveWithChainRequest, RevokeHandler, RevokeRequest,
};
use crate::is_identity_resolver_payload_kind;
// Import the `ResolverBackend` trait from its canonical Layer-B site
// (`octo_ident::resolver_backend`) rather than the Layer-C re-export,
// so the layer boundary is auditable in source (no indirection through
// `crate::handlers`). `LocalResolverBackend` (the impl) is the
// `crate::handlers` re-export because it lives at Layer C.
use octo_ident::resolver_backend::ResolverBackend;

/// Default `ChainId` used when no explicit chain is configured.
///
/// Per RFC-0862 v1.3 R12 + RFC-0010 v1.3 §Future Work F2 the chain
/// identifier gains typed-namespace + federation semantics in a future
/// RFC-0010 v1.4 amendment. Until then, `"cipherocto-mainnet"` is the
/// sole canonical chain and serves as the default for the
/// `IDENTITY_REGISTER` + `IDENTITY_REVOKE` mediation path.
pub const DEFAULT_CHAIN_ID: &str = "cipherocto-mainnet";

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
    /// Mission 0871e-f7-impl-resolver-mediation: cross-instance DID
    /// write coordination substrate. When `None`, the resolver-node
    /// refuses all writes with `CoordinatorUnavailable` (fail-closed).
    /// Injected via `Arc<dyn DidWriteCoordinator>` — no `octo-sync` dep
    /// at this Layer C crate (coordinator is a trait-object boundary;
    /// sealed trait per RFC-0862 v1.3 prevents downstream extension).
    pub write_coordinator: Option<Arc<dyn DidWriteCoordinator>>,
    /// Mission 0871e-f7-impl-resolver-mediation: chain ID used for
    /// the `IDENTITY_REGISTER` + `IDENTITY_REVOKE` mediation path.
    /// Defaults to [`DEFAULT_CHAIN_ID`] when `None`. Operators with a
    /// custom chain namespace pass `Some(ChainId::new("..."))`.
    pub chain_id: Option<ChainId>,
    /// Mission `0870k-transport-request-response` + mission
    /// `0871b-cross-node-forwarding` T4: opt-in `ResolverBackend`
    /// for cross-node `IDENTITY_RESOLVE_CHAIN` walks. When `None`
    /// at construction time, `IdentityResolverNode::new` defaults
    /// to `LocalResolverBackend` over `self.registry` (in-process
    /// walk; no network). Production deployments with cross-node
    /// resolver chains inject `Some(RemoteResolverBackend::arc(self.transport.clone()))`
    /// to route `IDENTITY_RESOLVE_CHAIN` hops through the
    /// `NodeTransport::request_response` substrate.
    pub resolver_backend: Option<Arc<dyn ResolverBackend>>,
}

impl Default for IdentityResolverNodeConfig {
    /// Default config for tests + Phase 1 MVP callers: no transport,
    /// no identity, no network key, no registry (defaults to
    /// `InMemoryDidRegistry`), no coordinator (fail-closed writes),
    /// no explicit chain (`DEFAULT_CHAIN_ID`), no resolver backend
    /// (defaults to `LocalResolverBackend`).
    ///
    /// Production deployments construct via struct-literal with all
    /// fields filled in.
    fn default() -> Self {
        Self {
            transport: Arc::new(NodeTransport::new(Vec::new())),
            identity: None,
            network_key: [0u8; 32],
            registry: None,
            write_coordinator: None,
            chain_id: None,
            resolver_backend: None,
        }
    }
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
    /// Mission `0871b-cross-node-forwarding`: cached resolver backend
    /// (Layer B trait) used by `ResolveChainHandler` for chain walks.
    /// Default-wrapped to `LocalResolverBackend` over `self.registry`
    /// so the in-process behavior of mission
    /// `0871b-cross-domain-resolution-impl` is preserved. A future
    /// mission swaps this for `RemoteResolverBackend` once mission
    /// `0870k-transport-request-response` lands the substrate.
    resolver_backend: Arc<dyn ResolverBackend>,
    /// Mission 0871e-f7-impl-resolver-mediation: cached coordinator
    /// (cloned from `config.write_coordinator`) for the
    /// `IDENTITY_REGISTER` + `IDENTITY_REVOKE` mediation path. `None`
    /// means fail-closed (writes refused with
    /// `IdentityResolveError::CoordinatorUnavailable`).
    write_coordinator: Option<Arc<dyn DidWriteCoordinator>>,
    /// Mission 0871e-f7-impl-resolver-mediation: resolved chain ID
    /// (defaults to [`DEFAULT_CHAIN_ID`] when `config.chain_id` is `None`).
    chain_id: ChainId,
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
    ///
    /// Mission 0871e-f7-impl-resolver-mediation: `config.chain_id`
    /// defaults to [`DEFAULT_CHAIN_ID`] when `None`; the coordinator
    /// slot defaults to `None` (fail-closed writes).
    ///
    /// Mission `0870k-transport-request-response` T4 + mission
    /// `0871b-cross-node-forwarding` T4: `config.resolver_backend`
    /// defaults to `LocalResolverBackend` over `self.registry` when
    /// `None`. Production deployments with cross-node resolver chains
    /// inject `Some(RemoteResolverBackend::arc(self.transport.clone()))`.
    #[must_use]
    pub fn new(config: IdentityResolverNodeConfig) -> Self {
        let registry = Self::materialize_registry(&config);
        let resolver_backend = Self::materialize_resolver_backend(&config, &registry);
        let write_coordinator = config.write_coordinator.clone();
        let chain_id = Self::materialize_chain_id(&config);
        Self {
            config,
            registry,
            resolver_backend,
            write_coordinator,
            chain_id,
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
        let registry = Self::materialize_registry(&config);
        let resolver_backend = Self::materialize_resolver_backend(&config, &registry);
        let write_coordinator = config.write_coordinator.clone();
        let chain_id = Self::materialize_chain_id(&config);
        Self {
            config,
            registry,
            resolver_backend,
            write_coordinator,
            chain_id,
            dispatcher,
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Default `registry` from `IdentityResolverNodeConfig.registry`,
    /// falling back to `Arc::new(InMemoryDidRegistry::default())`.
    /// Single-purpose helper so `new()` + `with_dispatcher()` share the
    /// defaulting logic without triggering `clippy::type_complexity` on
    /// a multi-return helper.
    fn materialize_registry(config: &IdentityResolverNodeConfig) -> Arc<dyn DidRegistry> {
        config
            .registry
            .clone()
            .unwrap_or_else(|| Arc::new(InMemoryDidRegistry::default()))
    }

    /// Default `chain_id` from `IdentityResolverNodeConfig.chain_id`,
    /// falling back to [`DEFAULT_CHAIN_ID`].
    fn materialize_chain_id(config: &IdentityResolverNodeConfig) -> ChainId {
        config.chain_id.clone().unwrap_or_else(|| {
            // `DEFAULT_CHAIN_ID` is a 17-char static literal that
            // passes RFC-0010 v1.4 validation (non-empty, ≤ 64 chars,
            // no control chars). `.expect` documents the invariant
            // at this call site.
            ChainId::new(DEFAULT_CHAIN_ID)
                .expect("DEFAULT_CHAIN_ID is a valid RFC-0010 v1.4 chain namespace")
        })
    }

    /// Default `resolver_backend` from
    /// `IdentityResolverNodeConfig.resolver_backend`, falling back to a
    /// `LocalResolverBackend` over the already-materialized `registry`.
    fn materialize_resolver_backend(
        config: &IdentityResolverNodeConfig,
        registry: &Arc<dyn DidRegistry>,
    ) -> Arc<dyn ResolverBackend> {
        config
            .resolver_backend
            .clone()
            .unwrap_or_else(|| LocalResolverBackend::new(registry.clone()))
    }

    /// Register the node as a `NetworkReceiver` on the underlying transport.
    ///
    /// After `start()`, the transport routes any incoming borsh-encoded
    /// `NodeEnvelope` whose `payload_kind` matches one of the
    /// `IDENTITY_RESOLVER_PAYLOAD_KINDS` to this node's `on_receive`
    /// method (`IDENTITY_RESOLVE`, `IDENTITY_REGISTER`,
    /// `IDENTITY_REVOKE`, `IDENTITY_RESOLVE_CHAIN`).
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
    ///
    /// Mission 0871e-f7-impl-resolver-mediation: this method is `async`
    /// because the `IDENTITY_REGISTER` + `IDENTITY_REVOKE` paths consult
    /// an `Arc<dyn DidWriteCoordinator>` whose trait surface is
    /// async (RFC-0862 v1.3). The `IDENTITY_RESOLVE` path remains
    /// synchronous (registry lookup is sync).
    pub async fn handle_envelope(
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
            k if k == octo_protocol::payload_kind::IDENTITY_REGISTER => {
                let req = RegisterRequest::from_borsh(&envelope.payload)
                    .map_err(resolver_error_to_protocol)?;
                RegisterHandler::new(
                    self.registry.clone(),
                    self.write_coordinator.clone(),
                    self.chain_id.clone(),
                )
                .handle(&req)
                .await
                .map_err(resolver_error_to_protocol)
            }
            k if k == octo_protocol::payload_kind::IDENTITY_REVOKE => {
                let req = RevokeRequest::from_borsh(&envelope.payload)
                    .map_err(resolver_error_to_protocol)?;
                RevokeHandler::new(
                    self.registry.clone(),
                    self.write_coordinator.clone(),
                    self.chain_id.clone(),
                )
                .handle(&req)
                .await
                .map_err(resolver_error_to_protocol)
            }
            k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN => {
                // Mission 0871b-cross-domain-resolution-impl: chain
                // resolution. Handler is async since round-2
                // (`#[async_trait] ResolverBackend`); no cross-network
                // I/O in this mission but the trait signature stays
                // stable for when mission
                // `0870k-transport-request-response` lands. The `.await`
                // here is the visible bit; `LocalResolverBackend` is
                // sync internally but the trait dispatch is async.
                //
                // Mission 0871b-cross-node-forwarding: thread
                // `envelope.envelope_id` as the replay-defense
                // correlation key into the chain response (5-tuple
                // field added in mission T5). Use the cached
                // `self.resolver_backend` (default `LocalResolverBackend`
                // over `self.registry`) — production HA swaps this for
                // `RemoteResolverBackend` once the request/response
                // substrate lands.
                let req = ChainResolveRequest::from_borsh(&envelope.payload)
                    .map_err(resolver_error_to_protocol)?;
                ResolveChainHandler::new(self.resolver_backend.clone())
                    .handle(&req, envelope.envelope_id)
                    .await
                    .map_err(resolver_error_to_protocol)
            }
            k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE_WITH_CHAIN => {
                // Mission 0010-f2-multi-chain-routing: chain-aware
                // resolve. Routes a single resolve request to a
                // specific chain namespace on a multi-chain
                // deployment. Distinct from `IDENTITY_RESOLVE_CHAIN`
                // (chain-of-resolvers, walks `Vec<ResolverHop>`).
                let req = ResolveWithChainRequest::from_borsh(&envelope.payload)
                    .map_err(resolver_error_to_protocol)?;
                ResolveWithChainHandler::new(self.registry.clone())
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
        // Mission 0871-phase5-router-dispatch-wiring: use the shared
        // `RouterAnnounceBuilder` (single source of truth — matches
        // `QuotaRouterNode::broadcast_announce` + 3 other specialized
        // nodes).
        use quota_router_core::node::announce::{PricingPolicy, RouterAnnounceBuilder};
        let pk = self
            .config
            .identity
            .as_ref()
            .map(|i| i.public_key_bytes())
            .unwrap_or([0u8; 32]);
        let announce = RouterAnnounceBuilder::new(
            quota_router_core::node::provider::RouterNodeId(pk),
            quota_router_core::node::provider::NetworkId([0u8; 32]),
        )
        .pricing_policy(Some(PricingPolicy {
            drain_per_query: 0,
            accepted_payment_capabilities: vec![],
            settlement_recipient: None,
        }))
        .build(&self.config.network_key);
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
            VERSION_TAG_V2,
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
            .await
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
        resolver_backend: node.resolver_backend.clone(),
        write_coordinator: node.write_coordinator.clone(),
        chain_id: node.chain_id.clone(),
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
            write_coordinator: None,
            chain_id: None,
            resolver_backend: None,
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
            write_coordinator: None,
            chain_id: None,
            resolver_backend: None,
        };
        let node = IdentityResolverNode::new(cfg);
        let _ = node.start();
        let err = node.start().unwrap_err();
        assert!(matches!(err, IdentityResolverNodeError::AlreadyStarted));
    }

    #[tokio::test]
    async fn handle_envelope_rejects_unsupported_payload_kind() {
        let transport = fresh_transport();
        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: None,
            write_coordinator: None,
            chain_id: None,
            resolver_backend: None,
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
            VERSION_TAG_V2,
        )
        .unwrap();
        let err = node.handle_envelope(&envelope).await.unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    #[tokio::test]
    async fn handle_envelope_uses_injected_registry_when_provided() {
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
                    ..Default::default()
                },
            )
            .unwrap();

        let cfg = IdentityResolverNodeConfig {
            transport: transport.clone(),
            identity: None,
            network_key: [0u8; 32],
            registry: Some(registry.clone()),
            write_coordinator: None,
            chain_id: None,
            resolver_backend: None,
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
            VERSION_TAG_V2,
        )
        .unwrap();
        let out = node
            .handle_envelope(&envelope)
            .await
            .expect("handle should succeed");
        let resp_bytes = out.response_payload.expect("response payload present");
        let resp: crate::handlers::ResolveResponse = borsh::from_slice(&resp_bytes).unwrap();
        assert_eq!(resp.canonical_did, wire_str);
        assert_eq!(resp.public_key, custom_pubkey);
    }

    #[tokio::test]
    async fn handle_envelope_defaults_to_in_memory_registry_when_omitted() {
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
            write_coordinator: None,
            chain_id: None,
            resolver_backend: None,
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
                    ..Default::default()
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
            VERSION_TAG_V2,
        )
        .unwrap();
        let out = node
            .handle_envelope(&envelope)
            .await
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
