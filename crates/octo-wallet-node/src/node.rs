//! `WalletNode` — the wallet's specialized-node adapter (RFC-0871 §Wallet Node Lifecycle).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
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
    AttenuateHandler, AttenuateRequest, HandlerOutput, MintHandler, MintRequest, ResolveDIDHandler,
    ResolveDIDRequest, SignHandler, SignRequest,
};
use crate::is_wallet_payload_kind;

/// Wallet node configuration.
#[derive(Clone)]
pub struct WalletNodeConfig {
    /// Wallet's identity key (HSM-routed signing).
    pub identity: Arc<IdentityKey>,
    /// Network transport (RFC-0863 NodeTransport).
    pub transport: Arc<NodeTransport>,
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
                let req = MintRequest::from_borsh(&envelope.payload)?;
                MintHandler::new(identity).handle(&req)
            }
            k if k == octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY => {
                let req = AttenuateRequest::from_borsh(&envelope.payload)?;
                AttenuateHandler::new().handle(&req)
            }
            k if k == octo_protocol::payload_kind::WALLET_RESOLVE_DID => {
                let req = ResolveDIDRequest::from_borsh(&envelope.payload)?;
                ResolveDIDHandler::new(identity).handle(&req)
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
        // Phase 1 MVP: emit a simple borsh-encoded announce envelope
        // body. The full RouterAnnouncePayload shape lives in RFC-0870.
        let announce_body = b"CIPHEROCTO_WALLET_ANNOUNCE_V1:4_payload_kinds";
        let from_did = WireDid::new(format!(
            "did:octo:z{}",
            bs58::encode(self.config.identity.public_key_bytes()).into_string()
        ));
        let envelope = NodeEnvelope::build(
            from_did,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::WALLET_SIGN_ED25519,
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

    fn sample_identity() -> IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        IdentityKey::from_seed(seed)
    }

    #[test]
    fn wallet_node_constructs_and_starts() {
        let identity = Arc::new(sample_identity());
        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = vec![];
        let transport = Arc::new(NodeTransport::new(senders));
        let cfg = WalletNodeConfig {
            identity: identity.clone(),
            transport,
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
            transport,
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
            transport,
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
        )
        .unwrap();
        let err = node.handle_envelope(&envelope).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }
}
