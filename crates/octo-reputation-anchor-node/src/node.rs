//! `ReputationAnchorNode` — the reputation-anchor specialized-node adapter
//! (RFC-0871 §Roles and Authorities, mission 0871c-reputation-anchor-node).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-ident`) and registers as a `NetworkReceiver` via the Layer D
//! transport (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871c-reputation-anchor-node (RFC-0871 Phase 3)
//!
//! `ReputationAnchorNode` advertises the Phase 3 MVP payload kind
//! `REPUTATION_ANCHOR_QUERY` (UUID
//! `0x0009:0004:0000:0000:0000:0000:0000:0001`). The handler validates a
//! canonical DID via `octo_ident::CanonicalCodec::parse(s, false)` and
//! returns a stub `(anchor_score = 0, attestation_count = 0)` response.
//!
//! The full RFC-0968 / RFC-0955-R1 reputation surface
//! (`REPUTATION_QUERY`, `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`)
//! lands in mission 0968a-reputation-anchoring follow-on once the
//! reputation registry substrate + anchoring substrate are production-ready.
//!
//! ## Replay + authorization
//!
//! All inbound envelopes route through `octo_protocol::EnvelopeDispatcher`
//! for envelope_id dedup + expiry check + signature verification. The
//! dispatcher reference is injectable so production code uses a
//! `ReferenceDispatcher` (full verification + cache) and tests use the
//! default dispatcher (in-memory cache + system clock).

use std::sync::Arc;

use async_trait::async_trait;
use octo_ident::CanonicalCodec;
use octo_ident::DidCodec;
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

use crate::handlers::{HandlerOutput, QueryAnchorHandler, QueryAnchorRequest};
use crate::is_reputation_payload_kind;

/// Reputation-anchor node configuration.
#[derive(Clone)]
pub struct ReputationAnchorNodeConfig {
    /// Network transport (RFC-0863 NodeTransport).
    pub transport: Arc<NodeTransport>,
    /// HSM-routed identity (mission 0959-placeholder-identity-binding).
    pub identity: Option<Arc<octo_wallet::identity::IdentityKey>>,
    /// Network key for `RouterAnnouncePayload` HMAC anti-spoof.
    pub network_key: [u8; 32],
}

/// Opaque handle returned by `ReputationAnchorNode::start()`.
#[derive(Clone, Debug)]
pub struct ReputationAnchorNodeHandle {
    pub(crate) _private: (),
}

/// Reputation-anchor node errors.
#[derive(Debug, thiserror::Error)]
pub enum ReputationAnchorNodeError {
    /// `start()` called when transport is already registered.
    #[error("already started")]
    AlreadyStarted,
    /// `start()` called when payload-kind service is misconfigured.
    #[error("payload kind {0:?} not a reputation-anchor payload kind")]
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

/// Reputation-anchor specialized-node adapter.
pub struct ReputationAnchorNode {
    config: ReputationAnchorNodeConfig,
    dispatcher: ReferenceDispatcher,
    started: std::sync::atomic::AtomicBool,
}

impl ReputationAnchorNode {
    /// Construct a new `ReputationAnchorNode`.
    #[must_use]
    pub fn new(config: ReputationAnchorNodeConfig) -> Self {
        Self {
            config,
            dispatcher: default_dispatcher(),
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Construct a new `ReputationAnchorNode` with a custom dispatcher.
    #[must_use]
    pub fn with_dispatcher(
        config: ReputationAnchorNodeConfig,
        dispatcher: ReferenceDispatcher,
    ) -> Self {
        Self {
            config,
            dispatcher,
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Register the node as a `NetworkReceiver` on the underlying transport.
    ///
    /// After `start()`, the transport routes any incoming borsh-encoded
    /// `NodeEnvelope` whose `payload_kind` is a `REPUTATION_*` kind to
    /// this node's `on_receive` method.
    ///
    /// # Errors
    /// Returns `ReputationAnchorNodeError::AlreadyStarted` if already
    /// registered. Returns `ReputationAnchorNodeError::UnknownPayloadKind`
    /// for any payload kind in the reputation-anchor namespace that the
    /// dispatcher doesn't serve.
    pub fn start(&self) -> Result<ReputationAnchorNodeHandle, ReputationAnchorNodeError> {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(ReputationAnchorNodeError::AlreadyStarted);
        }
        for kind in crate::REPUTATION_PAYLOAD_KINDS {
            if !is_reputation_payload_kind(kind) {
                return Err(ReputationAnchorNodeError::UnknownPayloadKind(*kind));
            }
        }
        self.config
            .transport
            .register_receiver(self_clone_to_receiver(self));
        Ok(ReputationAnchorNodeHandle { _private: () })
    }

    /// Dispatch an inbound envelope to the appropriate handler.
    ///
    /// Verification order:
    /// 1. `EnvelopeDispatcher` — envelope_id dedup + expiry + signature
    ///    verification (RFC-0871 §Adversary Analysis A6).
    /// 2. `payload_kind` UUID lookup → handler map.
    /// 3. Handler returns `HandlerOutput` (response envelope payload).
    pub fn handle_envelope(&self, envelope: &NodeEnvelope) -> Result<HandlerOutput, ProtocolError> {
        // 1. Verify envelope (replay defense + signature)
        self.dispatcher.verify_all(envelope)?;

        // 2. Dispatch by payload kind
        match envelope.payload_kind {
            k if k == octo_protocol::payload_kind::REPUTATION_ANCHOR_QUERY => {
                let req = QueryAnchorRequest::from_borsh(&envelope.payload)?;
                QueryAnchorHandler::new().handle(&req)
            }
            _ => Err(ProtocolError::AuthorizationFailed(format!(
                "unsupported payload kind: {:?}",
                envelope.payload_kind
            ))),
        }
    }

    /// Broadcast a `REPUTATION_ANCHOR_QUERY` announce to the network via
    /// the transport's broadcast channel.
    ///
    /// Phase 3 MVP: announce is a stub borsh-encoded envelope that lists
    /// the served `REPUTATION_*` payload kinds. The full
    /// `RouterAnnouncePayload` extension shape (per RFC-0871
    /// §Roles and Authorities) lands in mission 0870-b follow-on or a
    /// dedicated reputation-announce mission.
    ///
    /// # Identity is mandatory (RFC-0009 §Identity)
    ///
    /// This node announces under a DID derived from its signing key, so
    /// [`ReputationAnchorNodeConfig::identity`] is **required**. The node
    /// fails closed without one.
    ///
    /// It previously substituted `pk = [0u8; 32]` and minted a DID from
    /// that. Because [`CanonicalCodec::mint`] is a pure function of its
    /// pubkey input, **every unconfigured anchor announced the identical
    /// DID** — a collision in the identifier space rather than an
    /// anonymous-but-distinct identity. Two anchors could not be told
    /// apart, and neither could be attributed.
    ///
    /// The zero-key fallback is therefore removed rather than defaulted.
    /// Binding a real identity (full registry-backed identity) lands in
    /// mission 0968a-reputation-anchoring; until then an operator must
    /// supply a key, and an unconfigured node refuses to announce.
    ///
    /// # Errors
    /// [`TransportError::GovernanceViolation`] when no identity is bound.
    pub async fn broadcast_announce(&self) -> Result<usize, TransportError> {
        // Mission 0871e-phase5c: emit canonical RouterAnnouncePayload
        // (replaces the Phase 3 MVP
        // `CIPHEROCTO_REPUTATION_ANCHOR_ANNOUNCE_V1:1_payload_kind`
        // stub bytes).
        // Mission 0959-placeholder-identity-binding: when `identity`
        // is present, derive real `RouterNodeId`.
        // Mission 0871-phase5-router-dispatch-wiring: use the shared
        // `RouterAnnounceBuilder` (single source of truth).
        use quota_router_core::node::announce::{PricingPolicy, RouterAnnounceBuilder};
        // Fail closed: no identity ⇒ no announce. See the doc comment for
        // why the zero-key DID was worse than an anonymous announce.
        let identity = self.config.identity.as_ref().ok_or_else(|| {
            TransportError::GovernanceViolation(
                "reputation-anchor announce requires a bound identity; refusing to mint a \
                 zero-key DID (see RFC-0009 §Identity)"
                    .to_string(),
            )
        })?;
        // The router node id encodes a fixed 32-byte slot on the wire. Refused
        // for any other length rather than truncated.
        let pk = identity.public_key_bytes_array().ok_or_else(|| {
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
        let node_did = CanonicalCodec::mint(&pk);
        let node_wire = <CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&node_did)
            .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        let envelope = NodeEnvelope::build(
            node_wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::REPUTATION_ANCHOR_QUERY,
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
}

/// Helper: `ReputationAnchorNode` -> `Arc<dyn NetworkReceiver>`.
///
/// `ReputationAnchorNode` itself is not `NetworkReceiver`-shaped (it's a
/// plain struct). This wrapper implements the trait and delegates to
/// `ReputationAnchorNode::handle_envelope`.
struct ReputationAnchorNodeReceiver {
    node: Arc<ReputationAnchorNode>,
}

#[async_trait]
impl NetworkReceiver for ReputationAnchorNodeReceiver {
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
        "reputation-anchor-node"
    }
}

fn self_clone_to_receiver(node: &ReputationAnchorNode) -> Arc<dyn NetworkReceiver> {
    // Wrap in Arc — `ReputationAnchorNode` is not Clone, so we move the
    // configuration into a new Arc<ReputationAnchorNode> for the receiver.
    let arc = Arc::new(ReputationAnchorNode {
        config: node.config.clone(),
        dispatcher: default_dispatcher(),
        started: std::sync::atomic::AtomicBool::new(true),
    });
    Arc::new(ReputationAnchorNodeReceiver { node: arc })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_transport() -> Arc<NodeTransport> {
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        Arc::new(NodeTransport::new(senders))
    }

    #[test]
    fn reputation_anchor_node_constructs_and_starts() {
        let transport = test_transport();
        let cfg = ReputationAnchorNodeConfig {
            transport,
            identity: None,
            network_key: [0u8; 32],
        };
        let node = ReputationAnchorNode::new(cfg);
        let handle = node.start().expect("start should succeed");
        let _ = handle;
    }

    #[test]
    fn reputation_anchor_node_rejects_double_start() {
        let transport = test_transport();
        let cfg = ReputationAnchorNodeConfig {
            transport,
            identity: None,
            network_key: [0u8; 32],
        };
        let node = ReputationAnchorNode::new(cfg);
        let _ = node.start();
        let err = node.start().unwrap_err();
        assert!(matches!(err, ReputationAnchorNodeError::AlreadyStarted));
    }

    #[test]
    fn handle_envelope_rejects_unsupported_payload_kind() {
        let transport = test_transport();
        let cfg = ReputationAnchorNodeConfig {
            transport,
            identity: None,
            network_key: [0u8; 32],
        };
        let node = ReputationAnchorNode::new(cfg);
        // Build a payload with an unknown payload kind (use a random 16-byte UUID).
        let unknown_kind = PayloadKindId([0xAB; 16]);
        let placeholder_pk = [0u8; 32];
        let placeholder_did = CanonicalCodec::mint(&placeholder_pk);
        let placeholder_wire =
            <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&placeholder_did)
                .unwrap();
        let envelope = NodeEnvelope::build(
            placeholder_wire,
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

    // ---- D1-A: identity binding (zero-key DID elimination) ----

    #[tokio::test]
    async fn announce_fails_closed_without_identity() {
        let transport = test_transport();
        let node = ReputationAnchorNode::new(ReputationAnchorNodeConfig {
            transport,
            identity: None,
            network_key: [0u8; 32],
        });

        let err = node
            .broadcast_announce()
            .await
            .expect_err("announce must fail closed without a bound identity");

        match err {
            TransportError::GovernanceViolation(msg) => {
                assert!(
                    msg.contains("bound identity"),
                    "error should name the missing identity, got: {msg}"
                );
            }
            other => panic!("expected GovernanceViolation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn announce_proceeds_with_bound_identity() {
        let transport = test_transport();
        // The test transport has zero senders, so `broadcast` returns
        // Ok(0) — it succeeds having delivered to nobody. Reaching `Ok`
        // proves the identity gate let the call through. Paired with
        // `announce_fails_closed_without_identity`, this pins the gate in
        // both directions: it is a real condition, not a blanket failure.
        let identity = Arc::new(octo_wallet::identity::IdentityKey::generate().expect("generate"));
        let node = ReputationAnchorNode::new(ReputationAnchorNodeConfig {
            transport,
            identity: Some(identity),
            network_key: [0u8; 32],
        });

        assert_eq!(
            node.broadcast_announce()
                .await
                .expect("bound identity must proceed"),
            0
        );
    }

    #[test]
    fn zero_key_mint_is_colliding_which_is_why_it_is_refused() {
        // Pins the property that made the zero-key DID dangerous:
        // `CanonicalCodec::mint` is a pure function of the pubkey, so the
        // former fallback produced the SAME DID for every unconfigured
        // anchor. If this ever stops holding, the rationale for failing
        // closed needs re-examination.
        let a = CanonicalCodec::mint(&[0u8; 32]);
        let b = CanonicalCodec::mint(&[0u8; 32]);
        assert_eq!(a, b, "mint must be deterministic — this is the collision");

        // Two distinct real keys must NOT collide.
        let k1 = octo_wallet::identity::IdentityKey::generate().expect("k1");
        let k2 = octo_wallet::identity::IdentityKey::generate().expect("k2");
        assert_ne!(
            CanonicalCodec::mint(
                &k1.public_key_bytes_array()
                    .expect("a generated ed25519 key is 32 bytes")
            ),
            CanonicalCodec::mint(
                &k2.public_key_bytes_array()
                    .expect("a generated ed25519 key is 32 bytes")
            ),
            "distinct identities must mint distinct DIDs"
        );
    }
}
