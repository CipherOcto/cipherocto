//! `CAPABILITY_REVOKE` handler (RFC-0871 §Roles and Authorities, mission
//! 0871d-capability-issuer-node).
//!
//! Receives: `<token_id: [u8; 16]>` — the 16-byte `MacaroonId` of the
//! capability to revoke.
//! Returns: `<token_id: [u8; 16], acknowledged: bool>` — an
//! acknowledgement stub (Phase 3 MVP: always `true`; the real
//! `HolderRegistry` revocation + event emission lands in mission
//! 0957 Phase 2).
//!
//! ## Phase 3 MVP
//!
//! The handler accepts the 16-byte `token_id` (which is the
//! `MacaroonId` per RFC-0957 §Wire Format — `MacaroonId` is a type
//! alias for `[u8; 16]`) and returns an acknowledgement. No
//! `HolderRegistry` mutation, no RFC-0965 `RevocationCaveat`
//! validation, no event emission — these land in mission 0957 Phase 2
//! follow-on.
//!
//! ## Production semantics (deferred to 0957 Phase 2)
//!
//! `CAPABILITY_REVOKE` requires `Authorization::Capability(token)` with
//! a `RevocationCaveat` (RFC-0965 caveat type) issued by either the
//! original issuer OR a higher-authority governance capability. The
//! `HolderRegistry` revocation is monotonic (revoked stays revoked; no
//! un-revoke). Eventual-consistency gossip for revocation propagation
//! across nodes is a separate concern (RFC-0957-A1 revocation sync —
//! not yet spec'd; deferred to a future mission if needed).

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::MacaroonId;
use octo_protocol::ProtocolError;

use super::HandlerOutput;

/// Request payload for `CAPABILITY_REVOKE`.
///
/// Wire form: borsh (`token_id`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RevokeRequest {
    /// 16-byte token id (`MacaroonId` per RFC-0957 §Wire Format).
    pub token_id: MacaroonId,
}

impl RevokeRequest {
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
        borsh::to_vec(self).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }
}

/// Response payload for `CAPABILITY_REVOKE`.
///
/// Wire form: borsh (`token_id`, `acknowledged`).
///
/// Phase 3 MVP stub: `acknowledged` is always `true` (no `HolderRegistry`
/// mutation). The real revocation flow (`HolderRegistry::revoke` +
/// RFC-0957-A1 §HolderRecord State Machine transition + event emission)
/// lands in mission 0957 Phase 2 follow-on.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RevokeResponse {
    /// 16-byte token id (echoed back).
    pub token_id: MacaroonId,
    /// Acknowledgement flag (Phase 3 MVP: always `true`).
    pub acknowledged: bool,
}

/// `CAPABILITY_REVOKE` handler.
///
/// Phase 3 MVP: returns an acknowledgement stub. The handler is
/// unit-typed (no constructor state); the full substrate handler will
/// take a `HolderRegistry` reference (RFC-0957-A1) + a
/// `RevocationCaveat` validator (RFC-0965 reserved range).
#[derive(Debug, Default, Clone, Copy)]
pub struct RevokeHandler;

impl RevokeHandler {
    /// Construct a new `RevokeHandler`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Acknowledge the revocation request.
    ///
    /// Phase 3 MVP: always acknowledges. No `HolderRegistry` mutation,
    /// no event emission — substrate lands in mission 0957 Phase 2.
    pub fn handle(&self, req: &RevokeRequest) -> Result<HandlerOutput, ProtocolError> {
        let response = RevokeResponse {
            token_id: req.token_id,
            acknowledged: true,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::CAPABILITY_REVOKE)
                .with_note(format!(
                    "revoke acknowledged (MVP stub) for token_id {:02x?}",
                    req.token_id
                )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoke_request_borsh_round_trip() {
        let req = RevokeRequest {
            token_id: [0xab; 16],
        };
        let bytes = req.to_borsh().unwrap();
        let back = RevokeRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_returns_acknowledgement_stub() {
        let handler = RevokeHandler::new();
        let req = RevokeRequest {
            token_id: [0x42; 16],
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.expect("response payload present");
        let resp: RevokeResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.token_id, req.token_id);
        assert!(resp.acknowledged);
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::CAPABILITY_REVOKE)
        );
    }

    #[test]
    fn revoke_response_borsh_round_trip() {
        let resp = RevokeResponse {
            token_id: [0xef; 16],
            acknowledged: true,
        };
        let bytes = borsh::to_vec(&resp).unwrap();
        let back: RevokeResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }
}
