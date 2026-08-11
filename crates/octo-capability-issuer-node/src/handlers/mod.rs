//! Capability-issuer payload handlers (RFC-0871 §Roles and Authorities,
//! mission 0871d-capability-issuer-node).
//!
//! Each handler maps one `CAPABILITY_*` payload kind to its business logic.
//! All handlers route through `EnvelopeDispatcher` for envelope_id dedup
//! and expiry and signature verification (no handler shortcuts, no
//! per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-wallet` + `octo-ident` +
//! `octo-cap-macaroon` types and never reach into storage substrate
//! directly. The real macaroon mint + `HolderRegistry` registration
//! substrate (RFC-0957 §Algorithms and RFC-0957-A1 §Data Structures)
//! lands in mission 0957 Phase 2 follow-on.

use octo_protocol::ProtocolError;

pub mod issue;
pub mod lookup;
pub mod revoke;

pub use issue::{IssueHandler, IssueRequest, IssueResponse};
pub use lookup::{CapabilityLookupHandler, CapabilityLookupRequest, CapabilityLookupResponse};
pub use revoke::{RevokeHandler, RevokeRequest, RevokeResponse};

/// Output of a capability-issuer handler invocation.
///
/// Either returns a response envelope that the caller
/// (`CapabilityIssuerNode`) transmits back to the originating peer, or a
/// local effect (e.g. a `HolderRegistry` registration), or both, or
/// neither (e.g. a revocation that mutates existing state).
///
/// Phase 3 MVP: only `response_payload` + `response_payload_kind` are
/// populated. The macaroon mint + `HolderRegistry` registration + event
/// emission (RFC-0957-A1 §HolderRecord State Machine transitions) lands
/// with the full substrate in mission 0957 Phase 2 follow-on.
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

/// Map a `octo_ident::DidError` to a `ProtocolError` for invalid DID inputs.
///
/// Mirrors the wallet-node mapping pattern (RFC-0010 v1.2 F4).
pub fn did_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::InvalidDid(e.to_string())
}

/// Map a wallet-side error (e.g. `CapabilityToken::mint` failure in
/// mission 0957 Phase 2) to a `ProtocolError`.
///
/// Phase 3 MVP: reserved for the macaroon substrate landing in
/// mission 0957 Phase 2; the stub handlers don't reach into wallet
/// substrate (they only validate DID shape).
#[allow(dead_code)]
pub fn wallet_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::AuthorizationFailed(e.to_string())
}
