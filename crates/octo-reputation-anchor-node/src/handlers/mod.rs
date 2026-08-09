//! Reputation-anchor payload handlers (RFC-0871 §Roles and Authorities,
//! mission 0871c-reputation-anchor-node).
//!
//! Phase 3 MVP exposes only `REPUTATION_ANCHOR_QUERY` — a typed hand-off
//! point that validates a canonical DID and returns a stub
//! `(anchor_score, attestation_count)` response. All handlers route through
//! `EnvelopeDispatcher` for envelope_id dedup + expiry + signature
//! verification (no handler shortcuts, no per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-protocol` + `octo-ident` types
//! and never reach into storage or registry substrate directly. The real
//! reputation registry lookup (RFC-0968 `ReputationRegistry` +
//! RFC-0955-R1 anchoring substrate) lands in mission
//! 0968a-reputation-anchoring follow-on.

use octo_protocol::ProtocolError;

pub mod query;

pub use query::{QueryAnchorHandler, QueryAnchorRequest, QueryAnchorResponse};

/// Output of a reputation-anchor handler invocation.
///
/// Either returns a response envelope that the caller
/// (`ReputationAnchorNode`) transmits back to the originating peer, or a
/// local effect (e.g. an updated reputation record), or both, or neither.
///
/// Phase 3 MVP: only `response_payload` + `response_payload_kind` are
/// populated. The reputation event emission (RFC-0968 §11. Audit Trail)
/// arrives with the full `REPUTATION_UPDATE` handler in the follow-on
/// mission 0968a-reputation-anchoring.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandlerOutput {
    /// Optional response envelope to send back to the requester.
    pub response_payload: Option<Vec<u8>>,
    /// Optional response payload kind (RFC-0871 §Response convention).
    pub response_payload_kind: Option<octo_protocol::PayloadKindId>,
    /// Optional human-readable note (for logs; never on wire).
    pub note: Option<String>,
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
        }
    }

    /// Attach a note (for logs).
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Map a `octo_ident::DidError` to a `ProtocolError` for invalid DID inputs.
///
/// Mirrors the wallet-node mapping pattern (RFC-0010 v1.2 F4).
pub fn did_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::InvalidDid(e.to_string())
}
