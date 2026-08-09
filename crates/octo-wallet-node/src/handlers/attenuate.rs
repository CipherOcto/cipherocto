//! `WALLET_ATTENUATE_CAPABILITY` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<existing_token: [u8], new_caveat_cbor: [u8]>`.
//! Returns: `<attenuated_token: [u8]>`.
//!
//! Phase 1 MVP: the existing token wire form is the MVP stub
//! `CIPHEROCTO_MINT_V1:<did>`. The new caveat is opaque bytes (the
//! full macaroon caveat wiring lives in mission 0957 Phase 2 follow-on).

use borsh::{BorshDeserialize, BorshSerialize};
use octo_protocol::ProtocolError;

use super::{wallet_error_to_protocol, HandlerOutput};

/// Request payload for `WALLET_ATTENUATE_CAPABILITY`.
///
/// Wire form: borsh (`existing_token_wire`, `new_caveat_payload`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct AttenuateRequest {
    /// Existing token wire form (Phase 1 MVP: `CIPHEROCTO_MINT_V1:<did>`).
    pub existing_token: Vec<u8>,
    /// New caveat payload (opaque bytes; full caveat type lands in
    /// mission 0957-ext-macaroon Phase 2 follow-on).
    pub new_caveat_payload: Vec<u8>,
}

impl AttenuateRequest {
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

/// `WALLET_ATTENUATE_CAPABILITY` handler.
pub struct AttenuateHandler;

impl AttenuateHandler {
    /// Construct a new `AttenuateHandler`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Attenuate the existing token by appending `new_caveat_payload`.
    ///
    /// Phase 1 MVP: returns the wire form of the attenuated token as a
    /// placeholder. The full macaroon substrate attenuation is
    /// mission 0957 Phase 2 follow-on.
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` on unrecognized
    /// wire form.
    pub fn handle(&self, req: &AttenuateRequest) -> Result<HandlerOutput, ProtocolError> {
        let s = std::str::from_utf8(&req.existing_token)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        if !s.starts_with("CIPHEROCTO_MINT_V1:") {
            return Err(wallet_error_to_protocol(
                "existing_token: unrecognized wire form",
            ));
        }
        let _ = &req.new_caveat_payload; // Phase 1 MVP: ignored
        let wire = format!(
            "CIPHEROCTO_MINT_V1_ATTENUATED:{}:+{}bytes",
            s.strip_prefix("CIPHEROCTO_MINT_V1:").unwrap_or("?"),
            req.new_caveat_payload.len()
        );
        Ok(HandlerOutput::response(
            wire.into_bytes(),
            octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY,
        )
        .with_note("attenuation applied (MVP stub)"))
    }
}

impl Default for AttenuateHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuate_request_borsh_round_trip() {
        let req = AttenuateRequest {
            existing_token: b"CIPHEROCTO_MINT_V1:did:octo:zabc".to_vec(),
            new_caveat_payload: vec![0x01, 0x02, 0x03],
        };
        let bytes = req.to_borsh().unwrap();
        let back = AttenuateRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_unrecognized_wire_form() {
        let handler = AttenuateHandler::new();
        let req = AttenuateRequest {
            existing_token: b"unknown format".to_vec(),
            new_caveat_payload: vec![0x01],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    #[test]
    fn handle_attenuates_mvp_token() {
        let handler = AttenuateHandler::new();
        let req = AttenuateRequest {
            existing_token: b"CIPHEROCTO_MINT_V1:did:octo:zabc".to_vec(),
            new_caveat_payload: vec![0x01, 0x02, 0x03, 0x04, 0x05],
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let s = std::str::from_utf8(&payload).unwrap();
        assert!(s.starts_with("CIPHEROCTO_MINT_V1_ATTENUATED:did:octo:zabc"));
        assert!(s.contains("+5bytes"));
    }
}
