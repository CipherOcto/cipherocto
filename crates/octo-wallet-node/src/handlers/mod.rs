//! Wallet payload handlers (RFC-0871 §Wallet Node Lifecycle).
//!
//! Each handler maps one `WALLET_*` payload kind to its business logic.
//! All handlers route through `EnvelopeDispatcher` for envelope_id dedup
//! + expiry + signature verification (no handler shortcuts, no per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-wallet` + `octo-ident` types
//! and never reach into `ed25519_dalek` directly. Signing always flows
//! through `Arc<dyn HsmAdapter>`.

use octo_protocol::ProtocolError;

pub mod attenuate;
pub mod mint;
pub mod paid_query;
pub mod resolve;
pub mod sign;

pub use attenuate::{AttenuateHandler, AttenuateRequest};
pub use mint::{MintHandler, MintRequest};
pub use paid_query::{PaidQueryVerifyHandler, PaidQueryVerifyRequest};
pub use resolve::{ResolveDIDHandler, ResolveDIDRequest};
pub use sign::{SignHandler, SignRequest};

/// Output of a wallet handler invocation.
///
/// Either returns a response envelope that the caller (`WalletNode`)
/// transmits back to the originating peer, or a local effect (e.g. a
/// created `CapabilityToken`), or both, or neither (e.g. an
/// attenuation that mutates existing state).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandlerOutput {
    /// Optional response envelope to send back to the requester.
    pub response_payload: Option<Vec<u8>>,
    /// Optional response payload kind (RFC-0871 §Response convention).
    pub response_payload_kind: Option<octo_protocol::PayloadKindId>,
    /// Optional human-readable note (for logs; never on wire).
    pub note: Option<String>,
    /// Optional V2 bundle envelope bytes (mission
    /// `0957-f-v2-bundle-consumer-migration`). Surfaced alongside the
    /// primary `response_payload` so downstream consumers can adopt V2
    /// at their own pace; the primary payload retains the V1 wire
    /// form for backward compatibility until the V2 cutover mission
    /// lands. Never on wire (log/audit only).
    pub v2_envelope_bytes: Option<Vec<u8>>,
}

impl HandlerOutput {
    /// Empty output (no response, no local effect).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a response envelope payload + payload kind.
    #[must_use]
    pub fn response(payload: Vec<u8>, payload_kind: octo_protocol::PayloadKindId) -> Self {
        Self {
            response_payload: Some(payload),
            response_payload_kind: Some(payload_kind),
            note: None,
            v2_envelope_bytes: None,
        }
    }

    /// Attach a note (for logs).
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Attach V2 bundle envelope bytes (mission
    /// `0957-f-v2-bundle-consumer-migration`). Callers that adopt the
    /// V2 wire form surface the envelope here alongside the primary
    /// `response_payload`; the envelope is verified by `octo-cap-zk`
    /// and downstream V2 consumers.
    #[must_use]
    pub fn with_v2_envelope(mut self, envelope_bytes: Vec<u8>) -> Self {
        self.v2_envelope_bytes = Some(envelope_bytes);
        self
    }
}

/// Convert a `Wallet` error into a `ProtocolError` for handler failures.
///
/// Mapping is shallow: `wallet_error_to_protocol(error)` carries the
/// display string through. Handlers SHOULD preserve enough context
/// (e.g. error in `note`) for callers to diagnose.
pub fn wallet_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::AuthorizationFailed(e.to_string())
}

/// Map a `octo_ident::DidError` to a `ProtocolError` for invalid DID inputs.
pub fn did_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::InvalidDid(e.to_string())
}

/// Map a `octo_cap_macaroon::wire::WireError` to a `ProtocolError`
/// for the macaroon wire-form substrate (RFC-0957 §3.7, mission
/// 0957-phase2a). The wire form is byte-string; serialization
/// failures surface as `AuthorizationFailed` so callers don't
/// mistake them for runtime state (the wire form is pure crypto,
/// not envelope state).
pub fn wire_error_to_protocol(e: octo_cap_macaroon::wire::WireError) -> ProtocolError {
    ProtocolError::AuthorizationFailed(format!("capability wire form: {e}"))
}
