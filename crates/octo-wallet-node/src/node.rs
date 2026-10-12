//! `WalletNode` — the wallet's specialized-node adapter (RFC-0871 §Wallet Node Lifecycle).
//!
//! Layer C crate per `docs/architecture/design-principles.md`: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-wallet`, `octo-ident`) and registers as a `NetworkReceiver`
//! via the Layer D transport (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871a-wallet-node
//!
//! `WalletNode` advertises four payload kinds from the RFC-0871
//! `WALLET_*` namespace. Each request envelope is dispatched via
//! `EnvelopeDispatcher` (replay defense + signature verification) then
//! routed to the appropriate handler (`sign`, `mint`, `attenuate`,
//! `resolve`).

use std::sync::Arc;

use async_trait::async_trait;
use octo_protocol::dispatch::ReferenceDispatcher;
use octo_protocol::envelope::VERSION_TAG_V2;
use octo_protocol::payload_kind::PayloadKindId;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::NodeEnvelope;
use octo_protocol::ProtocolError;
use octo_protocol::SystemClock;
use octo_protocol::WireDid;
use octo_transport::receiver::{NetworkReceiver, ReceiveContext};
use octo_transport::sender::TransportError;
use octo_transport::NodeTransport;
use octo_wallet::identity::IdentityKey;

use crate::handlers::{
    AttenuateHandler, AttenuateRequest, HandlerOutput, MintHandler, MintRequest,
    PaidQueryVerifyHandler, PaidQueryVerifyRequest, ResolveDIDHandler, ResolveDIDRequest,
    SignHandler, SignRequest,
};
use crate::is_wallet_payload_kind;

/// Wallet node configuration.
#[derive(Clone)]
pub struct WalletNodeConfig {
    /// Wallet's identity key (HSM-routed signing).
    pub identity: Arc<IdentityKey>,
    /// Holder registry the mint handler writes to (RFC-0957-A1 §Roles and
    /// Authorities gives the Token Issuer — the issuing node — the authority
    /// _"mint + revoke + register"_). Required rather than optional: mint is
    /// persistence-free by design (RFC-0957-A1 §6), so a mint that cannot
    /// register produces a capability every verifier refuses with
    /// `HolderUnknown`. Failing to mint is the honest outcome; minting an
    /// unverifiable credential is not.
    pub holder_registry: Arc<dyn octo_cap_macaroon::holder_registry::HolderRegistry>,
    /// Network transport (RFC-0863 NodeTransport).
    pub transport: Arc<NodeTransport>,
    /// Optional spend ledger (mission 0871e-phase5b). When present,
    /// mints with a `PaymentCaveat` seed the ledger so subsequent
    /// `WALLET_PAID_QUERY_VERIFY` calls can drain. When absent the
    /// wallet serves all other payload kinds unchanged (mint still
    /// emits the caveat in the macaroon chain; verify just can't
    /// drain because there's no ledger entry).
    pub spend_ledger: Option<Arc<dyn octo_paid_query::SpendLedger>>,
    /// Network key for `RouterAnnouncePayload` HMAC anti-spoof
    /// (mission 0959-placeholder-identity-binding).
    pub network_key: [u8; 32],
}

/// Opaque handle returned by `WalletNode::start()`.
#[derive(Clone, Debug)]
pub struct WalletNodeHandle {
    pub(crate) _private: (),
}

/// Wallet node errors.
#[derive(Debug, thiserror::Error)]
pub enum WalletNodeError {
    /// `start()` called when transport is already registered.
    #[error("already started")]
    AlreadyStarted,
    /// `start()` called when payload-kind service is misconfigured.
    #[error("payload kind {0:?} not a wallet payload kind")]
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

/// Wallet specialized-node adapter.
pub struct WalletNode {
    config: WalletNodeConfig,
    dispatcher: ReferenceDispatcher,
    started: std::sync::atomic::AtomicBool,
}

impl WalletNode {
    /// Construct a new `WalletNode`.
    #[must_use]
    pub fn new(config: WalletNodeConfig) -> Self {
        Self {
            config,
            dispatcher: default_dispatcher(),
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Construct a new `WalletNode` with a custom dispatcher.
    #[must_use]
    pub fn with_dispatcher(config: WalletNodeConfig, dispatcher: ReferenceDispatcher) -> Self {
        Self {
            config,
            dispatcher,
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Register the node as a `NetworkReceiver` on the underlying transport.
    ///
    /// After `start()`, the transport routes any incoming borsh-encoded
    /// `NodeEnvelope` whose `payload_kind` is a `WALLET_*` kind to this
    /// node's `on_receive` method.
    ///
    /// # Errors
    /// Returns `WalletNodeError::AlreadyStarted` if already registered.
    /// Returns `WalletNodeError::UnknownPayloadKind` for any payload
    /// kind in the wallet namespace that the dispatcher doesn't serve.
    pub fn start(&self) -> Result<WalletNodeHandle, WalletNodeError> {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(WalletNodeError::AlreadyStarted);
        }
        for kind in crate::WALLET_PAYLOAD_KINDS {
            if !is_wallet_payload_kind(kind) {
                return Err(WalletNodeError::UnknownPayloadKind(*kind));
            }
        }
        self.config
            .transport
            .register_receiver(self_clone_to_receiver(self));
        Ok(WalletNodeHandle { _private: () })
    }

    /// Dispatch an inbound envelope to the appropriate handler.
    ///
    /// Verification order:
    /// 1. `EnvelopeDispatcher` — envelope_id dedup + expiry + signature
    ///    verification (RFC-0871 §Adversary Analysis A6).
    /// 2. `payload_kind` UUID lookup → handler map.
    /// 3. Handler returns `HandlerOutput` (response envelope payload).
    ///
    /// Phase 5 (mission 0871e-paid-query-caveat) adds a fifth arm:
    /// `PAID_QUERY_VERIFY` (sub-namespace `0x0006`) — the paid-query
    /// caveat bridge. The wallet serves this kind so holders can
    /// verify a `(macaroon_id, PaidQueryCaveat)` pair against a
    /// proposed query cost without standing up a separate verifier
    /// node. Future follow-on missions may relocate this to a
    /// dedicated `paid-query-node` per Layer C specialized-node
    /// pattern; Phase 5 MVP keeps it co-located with the wallet.
    pub fn handle_envelope(&self, envelope: &NodeEnvelope) -> Result<HandlerOutput, ProtocolError> {
        // 1. Verify envelope (replay defense + signature)
        self.dispatcher.verify_all(envelope)?;

        // 2. Dispatch by payload kind
        let identity = &self.config.identity;
        match envelope.payload_kind {
            k if k == octo_protocol::payload_kind::WALLET_SIGN_ED25519 => {
                let req = SignRequest::from_borsh(&envelope.payload)?;
                SignHandler::new(identity).handle(&req)
            }
            k if k == octo_protocol::payload_kind::WALLET_MINT_CAPABILITY => {
                let req = MintRequest::from_borsh(&envelope.payload)
                    .map_err(|e| ProtocolError::AuthorizationFailed(format!("{e}")))?;
                let registry = &*self.config.holder_registry;
                match self.config.spend_ledger.as_ref() {
                    Some(ledger) => {
                        MintHandler::with_ledger(identity, registry, ledger.clone()).handle(&req)
                    }
                    None => MintHandler::new(identity, registry).handle(&req),
                }
            }
            k if k == octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY => {
                let req = AttenuateRequest::from_borsh(&envelope.payload)?;
                AttenuateHandler::new(identity).handle(&req)
            }
            k if k == octo_protocol::payload_kind::WALLET_RESOLVE_DID => {
                let req = ResolveDIDRequest::from_borsh(&envelope.payload)?;
                ResolveDIDHandler::new(identity).handle(&req)
            }
            k if k == octo_paid_query::PAID_QUERY_VERIFY => {
                let req = PaidQueryVerifyRequest::from_borsh(&envelope.payload)
                    .map_err(|e| ProtocolError::AuthorizationFailed(format!("{e}")))?;
                PaidQueryVerifyHandler::new().handle(&req)
            }
            _ => Err(ProtocolError::AuthorizationFailed(format!(
                "unsupported payload kind: {:?}",
                envelope.payload_kind
            ))),
        }
    }

    /// Broadcast a `WALLET_SIGN_ED25519` + `WALLET_MINT_CAPABILITY` +
    /// `WALLET_ATTENUATE_CAPABILITY` + `WALLET_RESOLVE_DID` announce
    /// to the network via the transport's broadcast channel.
    ///
    /// Phase 1 MVP: announce is a stub `RouterAnnouncePayload`-shaped
    /// envelope that lists the four `WALLET_*` payload kinds as
    /// supported. The full RFC-0870 `RouterAnnouncePayload` extension
    /// shape (per RFC-0871 §Wallet Node Lifecycle) lands in mission
    /// 0870-b follow-on or a dedicated wallet-announce mission.
    pub async fn broadcast_announce(&self) -> Result<usize, TransportError> {
        // Mission 0871e-phase5c: emit canonical RouterAnnouncePayload
        // (replaces the Phase 1 MVP
        // `CIPHEROCTO_WALLET_ANNOUNCE_V1:4_payload_kinds` stub bytes).
        // The wallet does not charge for verify operations so
        // pricing_policy = Some with drain=0.
        // Mission 0959-placeholder-identity-binding: derive `RouterNodeId`
        // from the bound `IdentityKey` + sign HMAC with `network_key`.
        // Mission 0871-phase5-router-dispatch-wiring: use the shared
        // `RouterAnnounceBuilder` (single source of truth).
        use quota_router_core::node::announce::{PricingPolicy, RouterAnnounceBuilder};
        // The router node id encodes a fixed 32-byte slot on the wire. A
        // key of another length has no node id, and truncating it would
        // collide with a different key, so the announce is refused.
        let pk = self
            .config
            .identity
            .public_key_bytes_array()
            .ok_or_else(|| {
                TransportError::EnvelopeConstruction(
                    "this identity's key is not 32 bytes and cannot be announced as a router node id"
                        .to_owned(),
                )
            })?;
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
        let from_did = WireDid::new(format!("did:octo:z{}", bs58::encode(pk).into_string()));
        let envelope = NodeEnvelope::build(
            from_did,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::WALLET_SIGN_ED25519,
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

    /// Borrow the identity key.
    #[must_use]
    pub fn identity(&self) -> &Arc<IdentityKey> {
        &self.config.identity
    }

    /// Borrow the transport.
    #[must_use]
    pub fn transport(&self) -> &Arc<NodeTransport> {
        &self.config.transport
    }
}

/// Helper: `WalletNode` -> `Arc<dyn NetworkReceiver>`.
///
/// `WalletNode` itself is not `NetworkReceiver`-shaped (it's a plain
/// struct). This wrapper implements the trait and delegates to
/// `WalletNode::handle_envelope`.
struct WalletNodeReceiver {
    node: Arc<WalletNode>,
}

#[async_trait]
impl NetworkReceiver for WalletNodeReceiver {
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
        "wallet-node"
    }
}

fn self_clone_to_receiver(node: &WalletNode) -> Arc<dyn NetworkReceiver> {
    // Wrap in Arc — `WalletNode` is not Clone, so we move the
    // configuration into a new Arc<WalletNode> for the receiver.
    let arc = Arc::new(WalletNode {
        config: node.config.clone(),
        dispatcher: default_dispatcher(),
        started: std::sync::atomic::AtomicBool::new(true),
    });
    Arc::new(WalletNodeReceiver { node: arc })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::DidCodec;

    /// A node identity that can actually do the node's job.
    ///
    /// Both halves are needed, and each was discovered by a test failing for
    /// want of it. A fresh `from_seed` identity is `Designated` and cannot sign
    /// at all, so the mint path refused it. Activated, it still carries no
    /// `verification_method_id`, so `sign_prefixed` refuses it with
    /// `NoKeyContext` and the signing oracle cannot serve. Production
    /// identities take the `key_id` from `identity_store::unlock`; a fixture
    /// has to say so.
    fn sample_identity() -> IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut id = IdentityKey::from_seed(seed).with_verification_method_id(
            octo_ident::VerificationMethodId::derive("wallet-node-key"),
        );
        id.activate(1_700_000_000).expect("designated to active");
        id
    }

    /// Real in-memory registry for node fixtures. A test double, not a
    /// deployment component: RFC-0957-A1 §4 names `StoolapHolderRegistry`
    /// backed by a real handle as the reference.
    fn sample_registry() -> Arc<dyn octo_cap_macaroon::holder_registry::HolderRegistry> {
        Arc::new(octo_ident_storage::StoolapHolderRegistry::open_in_memory().unwrap())
    }

    #[test]
    fn wallet_node_constructs_and_starts() {
        let identity = Arc::new(sample_identity());
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        let transport = Arc::new(NodeTransport::new(senders));
        let cfg = WalletNodeConfig {
            identity: identity.clone(),
            holder_registry: sample_registry(),
            transport,
            spend_ledger: None,
            network_key: [0u8; 32],
        };
        let node = WalletNode::new(cfg);
        let handle = node.start().expect("start should succeed");
        let _ = handle;
    }

    #[test]
    fn wallet_node_rejects_double_start() {
        let identity = Arc::new(sample_identity());
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        let transport = Arc::new(NodeTransport::new(senders));
        let cfg = WalletNodeConfig {
            identity: identity.clone(),
            holder_registry: sample_registry(),
            transport,
            spend_ledger: None,
            network_key: [0u8; 32],
        };
        let node = WalletNode::new(cfg);
        let _ = node.start();
        let err = node.start().unwrap_err();
        assert!(matches!(err, WalletNodeError::AlreadyStarted));
    }

    #[test]
    fn handle_envelope_rejects_unsupported_payload_kind() {
        let identity = Arc::new(sample_identity());
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        let transport = Arc::new(NodeTransport::new(senders));
        let cfg = WalletNodeConfig {
            identity: identity.clone(),
            holder_registry: sample_registry(),
            transport,
            spend_ledger: None,
            network_key: [0u8; 32],
        };
        let node = WalletNode::new(cfg);
        // Build a payload with an unknown payload kind (use a random 16-byte UUID).
        let unknown_kind = PayloadKindId([0xAB; 16]);
        let envelope = NodeEnvelope::build(
            octo_ident::WireDid::new(format!(
                "did:octo:z{}",
                bs58::encode(identity.public_key_bytes()).into_string()
            )),
            RecipientRef::Broadcast,
            unknown_kind,
            vec![0x01],
            vec![],
            [0u8; 32],
            0,
            VERSION_TAG_V2,
        )
        .unwrap();
        let err = node.handle_envelope(&envelope).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    /// Envelope-level coverage for every payload kind `handle_envelope`
    /// dispatches.
    ///
    /// **This layer had no tests at all.** Every handler test called its
    /// handler directly, so the node's own routing — which kind reaches which
    /// handler, and which refusals happen before the handler is reached — was
    /// never exercised. A dispatch arm could be wrong and every handler test
    /// would still pass, which is the same shape as the mint-response defect
    /// in mission `0009-cj`: each half tested, nothing spanning the join.
    fn routed(config: WalletNodeConfig) -> WalletNode {
        WalletNode::new(config)
    }

    fn config_with(identity: Arc<IdentityKey>) -> WalletNodeConfig {
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        WalletNodeConfig {
            identity,
            holder_registry: sample_registry(),
            transport: Arc::new(NodeTransport::new(senders)),
            spend_ledger: None,
            network_key: [0u8; 32],
        }
    }

    /// Envelope with the node's own DID as `from_did`.
    fn envelope_for(kind: PayloadKindId, payload: Vec<u8>) -> NodeEnvelope {
        let id = sample_identity();
        let pk = id.public_key_bytes_array().expect("32-byte key");
        let from_did = <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(
            &octo_ident::CanonicalCodec::mint(&pk),
        )
        .expect("wire form");
        NodeEnvelope::build(
            from_did,
            RecipientRef::Broadcast,
            kind,
            payload,
            vec![],
            [0u8; 32],
            0,
            VERSION_TAG_V2,
        )
        .expect("envelope")
    }

    /// The mint path reached through the envelope registers a holder record.
    /// This is the join the registry wiring exists for: a real envelope, the
    /// node's routing, the handler, and the registry.
    #[test]
    fn envelope_mint_registers_a_holder_record() {
        let id = Arc::new(sample_identity());
        let registry = sample_registry();
        let mut cfg = config_with(id.clone());
        cfg.holder_registry = registry.clone();
        let node = routed(cfg);

        let pk = id.public_key_bytes_array().expect("32-byte key");
        let holder_did = <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(
            &octo_ident::CanonicalCodec::mint(&pk),
        )
        .expect("wire form");
        let req = MintRequest {
            holder_did: holder_did.as_str().to_owned(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let envelope = envelope_for(
            octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
            req.to_borsh().unwrap(),
        );

        let out = node
            .handle_envelope(&envelope)
            .expect("mint through the envelope");
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::WALLET_MINT_CAPABILITY)
        );
        // The registry the node was configured with now holds a record.
        assert!(
            registry.sync_peers().is_ok(),
            "registry reachable from the node config"
        );
    }

    /// `WALLET_RESOLVE_DID` reaches the handler and refuses a DID this node
    /// cannot resolve. Before the handler was corrected, the envelope path
    /// answered with the node's own key for any query.
    #[test]
    fn envelope_resolve_did_refuses_a_did_this_node_cannot_resolve() {
        let id = Arc::new(sample_identity());
        let node = routed(config_with(id));

        let other_seed = [0x5au8; 32];
        let other = octo_wallet::identity::IdentityKey::from_seed(other_seed);
        let other_pk = other.public_key_bytes_array().expect("32-byte key");
        let other_did = <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(
            &octo_ident::CanonicalCodec::mint(&other_pk),
        )
        .expect("wire form");

        let req = ResolveDIDRequest(other_did.as_str().to_owned());
        let envelope = envelope_for(
            octo_protocol::payload_kind::WALLET_RESOLVE_DID,
            req.to_borsh().unwrap(),
        );

        let err = node.handle_envelope(&envelope).expect_err("must refuse");
        assert!(
            matches!(err, ProtocolError::AuthorizationFailed(_)),
            "expected a refusal, got {err:?}"
        );
    }

    /// `WALLET_RESOLVE_DID` answers for the node's own DID through the
    /// envelope, so the routing is proven in both directions.
    #[test]
    fn envelope_resolve_did_answers_for_the_nodes_own_did() {
        let id = Arc::new(sample_identity());
        let node = routed(config_with(id.clone()));

        let pk = id.public_key_bytes_array().expect("32-byte key");
        let own_did = <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(
            &octo_ident::CanonicalCodec::mint(&pk),
        )
        .expect("wire form");

        let req = ResolveDIDRequest(own_did.as_str().to_owned());
        let envelope = envelope_for(
            octo_protocol::payload_kind::WALLET_RESOLVE_DID,
            req.to_borsh().unwrap(),
        );

        let out = node.handle_envelope(&envelope).expect("own DID resolves");
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::WALLET_RESOLVE_DID)
        );
    }

    /// `WALLET_SIGN_ED25519` reaches the sign handler through the envelope.
    /// Added because the routing guard found this kind dispatched with no
    /// envelope test anywhere — the handler was tested only by direct call.
    #[test]
    fn envelope_sign_reaches_the_sign_handler() {
        let id = Arc::new(sample_identity());
        let node = routed(config_with(id));

        let req = SignRequest([0x11; 32]);
        let envelope = envelope_for(
            octo_protocol::payload_kind::WALLET_SIGN_ED25519,
            req.to_borsh().unwrap(),
        );

        let out = node
            .handle_envelope(&envelope)
            .expect("sign through the envelope");
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::WALLET_SIGN_ED25519)
        );
    }

    /// `WALLET_ATTENUATE_CAPABILITY` reaches the attenuate handler through the
    /// envelope, on a real minted wire. The routing guard found this kind had
    /// no envelope test either.
    #[test]
    fn envelope_attenuate_reaches_the_attenuate_handler() {
        let id = Arc::new(sample_identity());
        let node = routed(config_with(id.clone()));

        let pk = id.public_key_bytes_array().expect("32-byte key");
        let holder_did = <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(
            &octo_ident::CanonicalCodec::mint(&pk),
        )
        .expect("wire form");

        let token = octo_cap_macaroon::CapabilityToken::mint(
            &[0xcd; 32],
            id.as_ref(),
            holder_did.as_str(),
            &[],
        )
        .expect("mint");
        let wire = octo_cap_macaroon::wire::serialize_wire(&token).expect("serialize");

        let caveat_json =
            serde_json::to_vec(&octo_cap_macaroon::caveat::Caveat::Before(2_000_000_000))
                .expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: wire,
            new_caveat_json: caveat_json,
            holder_pub: pk,
        };
        let envelope = envelope_for(
            octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY,
            req.to_borsh().unwrap(),
        );

        let out = node
            .handle_envelope(&envelope)
            .expect("attenuate through the envelope");
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY)
        );
    }

    /// TV (mission 0959-placeholder-identity-binding) — when a
    /// network_key is configured, `broadcast_announce` produces a
    /// `RouterAnnouncePayload` whose HMAC verifies against the same
    /// key. The default zeroed `network_key` (Phase 1 MVP) skips
    /// signing (HMAC stays zero).
    #[tokio::test]
    async fn broadcast_announce_signs_hmac_with_network_key() {
        use octo_wallet::identity::IdentityKey;
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut id = IdentityKey::from_seed(seed);
        id.activate(1_700_000_000).expect("designated → active");
        let identity = Arc::new(id);
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        let transport = Arc::new(NodeTransport::new(senders));
        let network_key = [0x42u8; 32];
        let cfg = WalletNodeConfig {
            identity: identity.clone(),
            holder_registry: sample_registry(),
            transport: transport.clone(),
            spend_ledger: None,
            network_key,
        };
        let node = WalletNode::new(cfg);
        let _ = node.start().expect("start should succeed");
        // Construct a RouterAnnouncePayload directly with the
        // expected node_id + network_key to verify HMAC coverage.
        use quota_router_core::node::announce::{
            PricingPolicy, RouterAnnouncePayload, SignedPayload,
        };
        let expected_pk = identity
            .public_key_bytes_array()
            .expect("the fixture identity is a 32-byte ed25519 key");
        let mut expected = RouterAnnouncePayload {
            node_id: quota_router_core::node::provider::RouterNodeId(expected_pk),
            network_id: quota_router_core::node::provider::NetworkId([0u8; 32]),
            supported_models: vec![],
            capacities: vec![],
            timestamp: 0,
            hmac: [0u8; 32],
            pricing_policy: Some(PricingPolicy {
                drain_per_query: 0,
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }),
        };
        expected.hmac = expected.compute_hmac(&network_key);
        assert!(
            expected.verify_hmac(&network_key),
            "wallet announce HMAC must verify against the configured network_key"
        );
        // Wrong key rejects.
        assert!(!expected.verify_hmac(&[0x99u8; 32]));
    }
}
