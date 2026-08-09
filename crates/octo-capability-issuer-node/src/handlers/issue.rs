//! `CAPABILITY_ISSUE` handler (RFC-0871 §Roles and Authorities, mission
//! 0871d-capability-issuer-node).
//!
//! Receives: `<holder_did: String, capability: [u8; 32]>` — the holder
//! DID + a 32-byte capability root secret.
//! Returns: `<minted_token_id: [u8; 16]>` — a placeholder 16-byte
//! `token_id` (MVP stub: derived from the capability root via
//! `octo_cap_macaroon::macaroon_id`; the full macaroon substrate
//! lands in mission 0957 Phase 2).
//!
//! ## Phase 3 MVP
//!
//! The handler validates `holder_did` shape via
//! `octo_ident::CanonicalCodec::parse(s, false)` (RFC-0010 v1.2 F4)
//! and derives a deterministic 16-byte `token_id` from the 32-byte
//! capability root via the macaroon-id derivation
//! (`octo_cap_macaroon::macaroon_id`). The returned wire form is a
//! placeholder string `CIPHEROCTO_ISSUE_V1:<holder_did>:<hex_token_id>`.
//!
//! The full substrate flow:
//! 1. `CapabilityToken::mint(&capability, &issuer_key, &holder_did, &[])`
//!    (RFC-0957 §Algorithms) — requires holder's pre-signed commitment
//!    envelope (RFC-0871 §Authorization model) and calls `IdentityKey::sign`
//!    (HSM-routed via `Arc<dyn HsmAdapter>` per mission 0009-a).
//! 2. `HolderRegistry::register(token)` (RFC-0957-A1 §Data Structures).
//! 3. Returns the minted `CapabilityToken` wire form (mission 0957
//!    Phase 2 introduces the `Macaroon` struct and `wire` module).
//!
//! The full substrate lands in mission 0957 Phase 2 follow-on.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::{macaroon_id, MacaroonId};
use octo_ident::DidCodec;
use octo_protocol::ProtocolError;

use super::{did_error_to_protocol, HandlerOutput};

/// Request payload for `CAPABILITY_ISSUE`.
///
/// Wire form: borsh (`holder_did`, `capability_root`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct IssueRequest {
    /// Canonical DID of the holder (`did:octo:z<base58btc>`).
    pub holder_did: String,
    /// 32-byte capability root secret (random nonce per mint).
    pub capability: [u8; 32],
}

impl IssueRequest {
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

/// Response payload for `CAPABILITY_ISSUE`.
///
/// Wire form: borsh (`holder_did`, `token_id`).
///
/// Phase 3 MVP stub: `token_id` is a deterministic 16-byte derivation
/// from the 32-byte `capability_root` (via `octo_cap_macaroon::macaroon_id`,
/// truncated to 16 bytes — the same algorithm the full substrate uses
/// for `MacaroonId`). The full macaroon wire form (caveat chain + HMAC
/// tail + holder signature) lands in mission 0957 Phase 2 follow-on.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct IssueResponse {
    /// Canonical DID of the holder (echoed back).
    pub holder_did: String,
    /// 16-byte token id (`MacaroonId` per RFC-0957 §Wire Format).
    pub token_id: [u8; 16],
}

/// `CAPABILITY_ISSUE` handler implementation.
///
/// Phase 3 MVP: validates the canonical DID shape and derives a
/// deterministic `token_id` from the capability root. The handler is
/// unit-typed (no constructor state); the full substrate handler will
/// take an `Arc<dyn HsmAdapter>` for holder signing + a
/// `HolderRegistry` reference for registration.
#[derive(Debug, Default, Clone, Copy)]
pub struct IssueHandler;

impl IssueHandler {
    /// Construct a new `IssueHandler`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Validate `holder_did` shape and derive a stub `token_id`.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `holder_did` is not a
    /// canonical DID shape.
    pub fn handle(&self, req: &IssueRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate canonical DID shape; reject legacy bare form.
        // (RFC-0010 v1.2 F4 + mission 0010-d wallet-audience-validation.)
        let _parsed = octo_ident::CanonicalCodec::parse(&req.holder_did, false)
            .map_err(did_error_to_protocol)?;

        // Phase 3 MVP: derive deterministic 16-byte token_id from the
        // 32-byte capability root via the macaroon-id primitive. The
        // first 16 bytes of the nonce form a unique-per-mint identifier;
        // the full macaroon substrate (RFC-0957 §Algorithms) builds on
        // this primitive in mission 0957 Phase 2.
        let mut nonce = [0u8; 16];
        nonce.copy_from_slice(&req.capability[..16]);
        let token_id: MacaroonId = macaroon_id(&req.capability, &nonce);

        let response = IssueResponse {
            holder_did: req.holder_did.clone(),
            token_id,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::CAPABILITY_ISSUE)
                .with_note(format!("issued (MVP stub) for {}", req.holder_did)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_did() -> String {
        // Derive a canonical DID from a deterministic 32-byte payload so
        // tests don't depend on a crypto adapter crate (the handler is
        // pure for the MVP per mission 0871d scope).
        let pk = [0x42u8; 32];
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    #[test]
    fn issue_request_borsh_round_trip() {
        let req = IssueRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
        };
        let bytes = req.to_borsh().unwrap();
        let back = IssueRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let handler = IssueHandler::new();
        let req = IssueRequest {
            holder_did: "did:octo:bad".into(),
            capability: [0xab; 32],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn handle_rejects_legacy_bare_did() {
        // Mission 0871d AC: DID validation MUST reject legacy bare
        // `did:octo:<base32>` form per RFC-0010 v1.2 F4 + 0010-d mission.
        let handler = IssueHandler::new();
        let bare = format!("did:octo:{}", "a".repeat(52));
        let req = IssueRequest {
            holder_did: bare,
            capability: [0xab; 32],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn handle_returns_derived_token_id() {
        let handler = IssueHandler::new();
        let capability = [0xcdu8; 32];
        let req = IssueRequest {
            holder_did: sample_did(),
            capability,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.expect("response payload present");
        let resp: IssueResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.holder_did, req.holder_did);
        // Token id must be deterministic for the same capability root.
        let mut nonce = [0u8; 16];
        nonce.copy_from_slice(&capability[..16]);
        let expected = macaroon_id(&capability, &nonce);
        assert_eq!(resp.token_id, expected);
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::CAPABILITY_ISSUE)
        );
    }

    #[test]
    fn handle_token_id_varies_with_capability() {
        // Mission 0871d AC: distinct capability roots MUST produce
        // distinct token_ids (no accidental collision).
        let handler = IssueHandler::new();
        let out_a = handler
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability: [0x01u8; 32],
            })
            .unwrap();
        let out_b = handler
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability: [0x02u8; 32],
            })
            .unwrap();
        let resp_a: IssueResponse = borsh::from_slice(&out_a.response_payload.unwrap()).unwrap();
        let resp_b: IssueResponse = borsh::from_slice(&out_b.response_payload.unwrap()).unwrap();
        assert_ne!(resp_a.token_id, resp_b.token_id);
    }

    #[test]
    fn issue_response_borsh_round_trip() {
        let resp = IssueResponse {
            holder_did: sample_did(),
            token_id: [0xef; 16],
        };
        let bytes = resp.to_borsh_value();
        let back: IssueResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }

    // Helper: derive borsh bytes for IssueResponse (no impl on the type
    // to avoid leaking the wire format into the production API).
    impl IssueResponse {
        pub(crate) fn to_borsh_value(&self) -> Vec<u8> {
            borsh::to_vec(self).expect("IssueResponse borsh encode")
        }
    }
}
