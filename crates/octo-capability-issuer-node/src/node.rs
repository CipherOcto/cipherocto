//! `CapabilityIssuerNode` — the capability-issuer specialized-node
//! adapter (RFC-0871 §Roles and Authorities, mission
//! 0871d-capability-issuer-node).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-wallet`, `octo-ident`) + Layer 4 (`octo-cap-macaroon`) and
//! registers as a `NetworkReceiver` via the Layer D transport
//! (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871d-capability-issuer-node (RFC-0871 Phase 3)
//!
//! `CapabilityIssuerNode` advertises two Phase 3 MVP payload kinds
//! from the RFC-0871 `CAPABILITY_*` namespace (sub-namespace `0x0005`):
//! `CAPABILITY_ISSUE` + `CAPABILITY_REVOKE`. Each request envelope is
//! dispatched via `EnvelopeDispatcher` (replay defense + signature
//! verification) then routed to the appropriate handler
//! (`issue`, `revoke`).
//!
//! ## Phase 3 MVP scope
//!
//! The node accepts the Phase 3 MVP handler set; the macaroon substrate
//! (`CapabilityToken::mint` + `HolderRegistry::register` + RFC-0957-A1
//! §HolderRecord State Machine transitions) lands in mission 0957
//! Phase 2 follow-on. `CapabilityIssuerNodeConfig` carries only the
//! transport; `CapabilityIssuerNode::new` does NOT yet take an
//! `Arc<IdentityKey>` or `Arc<dyn HolderRegistry>` — those slot in
//! with the macaroon substrate.

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

use crate::handlers::{HandlerOutput, IssueHandler, IssueRequest, RevokeHandler, RevokeRequest};
use crate::is_capability_payload_kind;

/// Capability-issuer node configuration.
#[derive(Clone)]
pub struct CapabilityIssuerNodeConfig {
    /// Network transport (RFC-0863 NodeTransport).
    pub transport: Arc<NodeTransport>,
}

/// Opaque handle returned by `CapabilityIssuerNode::start()`.
#[derive(Clone, Debug)]
pub struct CapabilityIssuerNodeHandle {
    pub(crate) _private: (),
}

/// Capability-issuer node errors.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityIssuerNodeError {
    /// `start()` called when transport is already registered.
    #[error("already started")]
    AlreadyStarted,
    /// `start()` called when payload-kind service is misconfigured.
    #[error("payload kind {0:?} not a capability-issuer payload kind")]
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

/// Capability-issuer specialized-node adapter.
pub struct CapabilityIssuerNode {
    config: CapabilityIssuerNodeConfig,
    dispatcher: ReferenceDispatcher,
    started: std::sync::atomic::AtomicBool,
}

impl CapabilityIssuerNode {
    /// Construct a new `CapabilityIssuerNode`.
    #[must_use]
    pub fn new(config: CapabilityIssuerNodeConfig) -> Self {
        Self {
            config,
            dispatcher: default_dispatcher(),
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Construct a new `CapabilityIssuerNode` with a custom dispatcher.
    #[must_use]
    pub fn with_dispatcher(
        config: CapabilityIssuerNodeConfig,
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
    /// `NodeEnvelope` whose `payload_kind` is a `CAPABILITY_*` kind to
    /// this node's `on_receive` method.
    ///
    /// # Errors
    /// Returns `CapabilityIssuerNodeError::AlreadyStarted` if already
    /// registered. Returns `CapabilityIssuerNodeError::UnknownPayloadKind`
    /// for any payload kind in the capability-issuer namespace that the
    /// dispatcher doesn't serve.
    pub fn start(&self) -> Result<CapabilityIssuerNodeHandle, CapabilityIssuerNodeError> {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(CapabilityIssuerNodeError::AlreadyStarted);
        }
        for kind in crate::CAPABILITY_PAYLOAD_KINDS {
            if !is_capability_payload_kind(kind) {
                return Err(CapabilityIssuerNodeError::UnknownPayloadKind(*kind));
            }
        }
        self.config
            .transport
            .register_receiver(self_clone_to_receiver(self));
        Ok(CapabilityIssuerNodeHandle { _private: () })
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
            k if k == octo_protocol::payload_kind::CAPABILITY_ISSUE => {
                let req = IssueRequest::from_borsh(&envelope.payload)?;
                IssueHandler::new().handle(&req)
            }
            k if k == octo_protocol::payload_kind::CAPABILITY_REVOKE => {
                let req = RevokeRequest::from_borsh(&envelope.payload)?;
                RevokeHandler::new().handle(&req)
            }
            _ => Err(ProtocolError::AuthorizationFailed(format!(
                "unsupported payload kind: {:?}",
                envelope.payload_kind
            ))),
        }
    }

    /// Broadcast a `CAPABILITY_ISSUE` + `CAPABILITY_REVOKE` announce to
    /// the network via the transport's broadcast channel.
    ///
    /// Phase 3 MVP: announce is a stub borsh-encoded envelope that lists
    /// the served `CAPABILITY_*` payload kinds. The full
    /// `RouterAnnouncePayload` extension shape (per RFC-0871 §Roles and
    /// Authorities) lands in mission 0870-b follow-on or a dedicated
    /// capability-issuer-announce mission.
    ///
    /// Phase 3 MVP also uses a placeholder `from_did` derived from a
    /// zeroed 32-byte payload (no signing identity bound yet — full
    /// HSM-bound identity lands in mission 0957 Phase 2 follow-on
    /// alongside the macaroon substrate).
    pub async fn broadcast_announce(&self) -> Result<usize, TransportError> {
        let announce_body = b"CIPHEROCTO_CAPABILITY_ISSUER_ANNOUNCE_V1:2_payload_kinds";
        // Phase 3 MVP: placeholder from_did (canonical DID shape).
        // The real bound identity (Arc<IdentityKey> via HSM) lands in
        // mission 0957 Phase 2 follow-on alongside the macaroon
        // substrate.
        let placeholder_pk = [0u8; 32];
        let placeholder_did = CanonicalCodec::mint(&placeholder_pk);
        let placeholder_wire =
            <CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&placeholder_did)
                .map_err(|e| TransportError::EnvelopeConstruction(e.to_string()))?;
        let envelope = NodeEnvelope::build(
            placeholder_wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::CAPABILITY_ISSUE,
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

/// Helper: `CapabilityIssuerNode` -> `Arc<dyn NetworkReceiver>`.
///
/// `CapabilityIssuerNode` itself is not `NetworkReceiver`-shaped (it's
/// a plain struct). This wrapper implements the trait and delegates to
/// `CapabilityIssuerNode::handle_envelope`.
struct CapabilityIssuerNodeReceiver {
    node: Arc<CapabilityIssuerNode>,
}

#[async_trait]
impl NetworkReceiver for CapabilityIssuerNodeReceiver {
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
        "capability-issuer-node"
    }
}

fn self_clone_to_receiver(node: &CapabilityIssuerNode) -> Arc<dyn NetworkReceiver> {
    // Wrap in Arc — `CapabilityIssuerNode` is not Clone, so we move
    // the configuration into a new Arc<CapabilityIssuerNode> for the
    // receiver.
    let arc = Arc::new(CapabilityIssuerNode {
        config: node.config.clone(),
        dispatcher: default_dispatcher(),
        started: std::sync::atomic::AtomicBool::new(true),
    });
    Arc::new(CapabilityIssuerNodeReceiver { node: arc })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::{IssueRequest, RevokeRequest};

    fn test_transport() -> Arc<NodeTransport> {
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        Arc::new(NodeTransport::new(senders))
    }

    fn sample_did() -> String {
        let pk = [0x42u8; 32];
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    #[test]
    fn capability_issuer_node_constructs_and_starts() {
        let transport = test_transport();
        let cfg = CapabilityIssuerNodeConfig { transport };
        let node = CapabilityIssuerNode::new(cfg);
        let handle = node.start().expect("start should succeed");
        let _ = handle;
    }

    #[test]
    fn capability_issuer_node_rejects_double_start() {
        let transport = test_transport();
        let cfg = CapabilityIssuerNodeConfig { transport };
        let node = CapabilityIssuerNode::new(cfg);
        let _ = node.start();
        let err = node.start().unwrap_err();
        assert!(matches!(err, CapabilityIssuerNodeError::AlreadyStarted));
    }

    #[test]
    fn handle_envelope_rejects_unsupported_payload_kind() {
        let transport = test_transport();
        let cfg = CapabilityIssuerNodeConfig { transport };
        let node = CapabilityIssuerNode::new(cfg);
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

    #[test]
    fn handle_envelope_routes_capability_issue() {
        let transport = test_transport();
        let cfg = CapabilityIssuerNodeConfig { transport };
        let node = CapabilityIssuerNode::new(cfg);
        let placeholder_pk = [0u8; 32];
        let placeholder_did = CanonicalCodec::mint(&placeholder_pk);
        let placeholder_wire =
            <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&placeholder_did)
                .unwrap();
        let req = IssueRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
        };
        let payload = req.to_borsh().unwrap();
        let envelope = NodeEnvelope::build(
            placeholder_wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::CAPABILITY_ISSUE,
            payload,
            vec![],
            [0u8; 32],
            0,
        )
        .unwrap();
        let out = node.handle_envelope(&envelope).unwrap();
        assert!(out.response_payload.is_some());
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::CAPABILITY_ISSUE)
        );
    }

    #[test]
    fn handle_envelope_routes_capability_revoke() {
        let transport = test_transport();
        let cfg = CapabilityIssuerNodeConfig { transport };
        let node = CapabilityIssuerNode::new(cfg);
        let placeholder_pk = [0u8; 32];
        let placeholder_did = CanonicalCodec::mint(&placeholder_pk);
        let placeholder_wire =
            <octo_ident::CanonicalCodec as octo_ident::DidCodec>::raw_to_wire(&placeholder_did)
                .unwrap();
        let req = RevokeRequest {
            token_id: [0x42; 16],
        };
        let payload = req.to_borsh().unwrap();
        let envelope = NodeEnvelope::build(
            placeholder_wire,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::CAPABILITY_REVOKE,
            payload,
            vec![],
            [0u8; 32],
            0,
        )
        .unwrap();
        let out = node.handle_envelope(&envelope).unwrap();
        assert!(out.response_payload.is_some());
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::CAPABILITY_REVOKE)
        );
    }
}
