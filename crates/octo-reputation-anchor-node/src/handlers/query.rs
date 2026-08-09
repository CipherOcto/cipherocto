//! `REPUTATION_ANCHOR_QUERY` handler (RFC-0871 §Roles and Authorities,
//! mission 0871c-reputation-anchor-node).
//!
//! Receives: `<query_did: String>` — a canonical DID to look up.
//! Returns: `<anchor_score: u64, attestation_count: u32>` — a stub
//! response; real lookup lands in mission 0968a-reputation-anchoring
//! (RFC-0968 `ReputationRegistry` + RFC-0955-R1 anchoring substrate).
//!
//! Phase 3 MVP: only validates the canonical DID shape via
//! `octo_ident::CanonicalCodec::parse()`. The actual reputation registry
//! query + anchoring receipt lands in mission 0968a-reputation-anchoring
//! follow-on and is plugged in here via a registry backend trait in that
//! follow-on mission.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::DidCodec;
use octo_protocol::ProtocolError;

use super::{did_error_to_protocol, HandlerOutput};

/// Request payload for `REPUTATION_ANCHOR_QUERY`.
///
/// Wire form: borsh (`query_did`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct QueryAnchorRequest(pub String);

impl QueryAnchorRequest {
    /// Decode from borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, ProtocolError> {
        borsh::from_slice(bytes).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }

    /// Encode to borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, ProtocolError> {
        borsh::to_vec(&self.0).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }
}

/// Response payload for `REPUTATION_ANCHOR_QUERY`.
///
/// Wire form: borsh (`anchor_score`, `attestation_count`).
///
/// Phase 3 MVP stub: both fields are zero. The real values are sourced
/// from the RFC-0968 `ReputationRegistry` + RFC-0955-R1 anchoring
/// substrate in mission 0968a-reputation-anchoring follow-on.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct QueryAnchorResponse {
    /// Aggregated anchor score (Phase 3 MVP: zero).
    pub anchor_score: u64,
    /// Number of attestations on file (Phase 3 MVP: zero).
    pub attestation_count: u32,
}

/// `REPUTATION_ANCHOR_QUERY` handler.
///
/// Phase 3 MVP: only validates the canonical DID shape; returns a stub
/// `QueryAnchorResponse` with `anchor_score = 0` + `attestation_count = 0`.
/// The handler is a unit struct for the MVP; the follow-on mission
/// 0968a-reputation-anchoring introduces a registry backend trait that
/// `QueryAnchorHandler` will route through to surface real reputation
/// data.
#[derive(Debug, Default, Clone, Copy)]
pub struct QueryAnchorHandler;

impl QueryAnchorHandler {
    /// Construct a new `QueryAnchorHandler`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Validate the DID query and return a stub anchor response.
    ///
    /// Phase 3 MVP: validates the canonical DID shape; returns a
    /// `(0, 0)` placeholder response. The real registry lookup (RFC-0968
    /// `ReputationRegistry`) lands in mission 0968a-reputation-anchoring.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `query_did` is not a
    /// canonical DID shape.
    pub fn handle(&self, req: &QueryAnchorRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate canonical DID shape; reject legacy bare form.
        // (RFC-0010 v1.2 F4 + mission 0010-d wallet-audience-validation.)
        let _parsed =
            octo_ident::CanonicalCodec::parse(&req.0, false).map_err(did_error_to_protocol)?;

        let response = QueryAnchorResponse {
            anchor_score: 0,
            attestation_count: 0,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(HandlerOutput::response(
            payload,
            octo_protocol::payload_kind::REPUTATION_ANCHOR_QUERY,
        )
        .with_note(format!("anchor query (MVP stub) for {}", req.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_did() -> String {
        // Derive a canonical DID from a deterministic 32-byte payload so
        // tests don't depend on a crypto adapter crate (octo-wallet is
        // intentionally not in the MVP dep set per mission 0871c).
        let pk = [0x42u8; 32];
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    #[test]
    fn query_request_borsh_round_trip() {
        let req = QueryAnchorRequest(sample_did());
        let bytes = req.to_borsh().unwrap();
        let back = QueryAnchorRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let handler = QueryAnchorHandler::new();
        let req = QueryAnchorRequest("did:octo:bad".into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn handle_rejects_legacy_bare_did() {
        // Mission 0871c AC: DID validation MUST reject legacy bare
        // `did:octo:<base32>` form per RFC-0010 v1.2 F4 + 0010-d mission.
        let handler = QueryAnchorHandler::new();
        // 52-char base32 body, bare prefix — RFC-0010 legacy form.
        let bare = format!("did:octo:{}", "a".repeat(52));
        let req = QueryAnchorRequest(bare);
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn handle_returns_mvp_stub_response() {
        let handler = QueryAnchorHandler::new();
        let req = QueryAnchorRequest(sample_did());
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.expect("response payload present");
        let resp: QueryAnchorResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.anchor_score, 0);
        assert_eq!(resp.attestation_count, 0);
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::REPUTATION_ANCHOR_QUERY)
        );
    }

    #[test]
    fn query_response_borsh_round_trip() {
        let resp = QueryAnchorResponse {
            anchor_score: 0,
            attestation_count: 0,
        };
        let bytes = borsh::to_vec(&resp).unwrap();
        let back: QueryAnchorResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }
}
