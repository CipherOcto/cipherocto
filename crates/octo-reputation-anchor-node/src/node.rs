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
    /// Phase 3 MVP also uses a placeholder `from_did` derived from the
    /// node's announced kinds (no signing identity bound yet — full
    /// registry-backed identity lands in mission 0968a-reputation-anchoring).
    pub async fn broadcast_announce(&self) -> Result<usize, TransportError> {
        let announce_body = b"CIPHEROCTO_REPUTATION_ANCHOR_ANNOUNCE_V1:1_payload_kind";
        // Phase 3 MVP: placeholder from_did (canonical DID shape).
        // The real bound identity (Arc<IdentityKey> via HSM) lands in
        // mission 0968a-reputation-anchoring follow-on.
        let placeholder_pk = [0u8; 32];
        let placeholder_did = CanonicalCodec::mint(&placeholder_pk);
        let placeholder_wire =
            <CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&placeholder_did)
                .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        let envelope = NodeEnvelope::build(
            placeholder_wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::REPUTATION_ANCHOR_QUERY,
            announce_body.to_vec(),
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
        let cfg = ReputationAnchorNodeConfig { transport };
        let node = ReputationAnchorNode::new(cfg);
        let handle = node.start().expect("start should succeed");
        let _ = handle;
    }

    #[test]
    fn reputation_anchor_node_rejects_double_start() {
        let transport = test_transport();
        let cfg = ReputationAnchorNodeConfig { transport };
        let node = ReputationAnchorNode::new(cfg);
        let _ = node.start();
        let err = node.start().unwrap_err();
        assert!(matches!(err, ReputationAnchorNodeError::AlreadyStarted));
    }

    #[test]
    fn handle_envelope_rejects_unsupported_payload_kind() {
        let transport = test_transport();
        let cfg = ReputationAnchorNodeConfig { transport };
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
        )
        .unwrap();
        let err = node.handle_envelope(&envelope).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }
}
