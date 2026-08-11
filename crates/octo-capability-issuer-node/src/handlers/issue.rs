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
use octo_cap_macaroon::{
    macaroon_id, BundleV2Error, CapabilityBundleV2, CapabilityBundleV2Envelope, CapabilityTokenV2,
    MacaroonId,
};
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
/// Wire form: borsh (`holder_did`, `token_id`, `v2_envelope_bytes`).
///
/// Phase 3 MVP stub: `token_id` is a deterministic 16-byte derivation
/// from the 32-byte `capability_root` (via `octo_cap_macaroon::macaroon_id`,
/// truncated to 16 bytes — the same algorithm the full substrate uses
/// for `MacaroonId`). The full macaroon wire form (caveat chain + HMAC
/// tail + holder signature) lands in mission 0957 Phase 2 follow-on.
///
/// `v2_envelope_bytes` (mission `0957-f-v2-bundle-consumer-migration`):
/// canonical_ser bytes of a `CapabilityBundleV2Envelope` wrapping a
/// root bundle (`chain_depth = 0`, `chain_parent = [0u8; 32]`).
/// Downstream V2 consumers (Wallet, `octo-cap-zk`) verify the
/// envelope via `CapabilityBundleV2Envelope::canonical_de`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct IssueResponse {
    /// Canonical DID of the holder (echoed back).
    pub holder_did: String,
    /// 16-byte token id (`MacaroonId` per RFC-0957 §Wire Format).
    pub token_id: [u8; 16],
    /// V2 bundle envelope bytes (mission
    /// `0957-f-v2-bundle-consumer-migration`). Carries the root
    /// bundle substrate; consumers decode via
    /// `CapabilityBundleV2Envelope::canonical_de`.
    pub v2_envelope_bytes: Vec<u8>,
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

        // V2 root bundle envelope (mission 0957-f-v2-bundle-consumer-migration).
        // Issuer mints a root bundle: chain_depth = 0, chain_parent =
        // [0u8; 32]. `channel_id` is the MacaroonId (16-byte scope tag).
        // `holder_record_bytes` carries the 32-byte capability_root as
        // MVP placeholder (follow-on mission replaces with real
        // HolderRecord bytes). MVP issuer lacks identity key —
        // `issuer_did` is a canonical placeholder string.
        let issuer_did = "did:octo:zIssuerMVPPlaceholder".to_owned();
        let token_v2 = CapabilityTokenV2 {
            chain_depth: 0,
            chain_parent: [0u8; 32],
            audience_did: req.holder_did.clone(),
            channel_id: token_id,
            expires_at_unix_secs: u64::MAX,
            issuer_did,
        };
        let holder_record_bytes = req.capability.to_vec();
        let discharge_macaroon_bytes: Vec<u8> = Vec::new();
        let v2_bundle =
            CapabilityBundleV2::new(token_v2, holder_record_bytes, discharge_macaroon_bytes)
                .map_err(|e: BundleV2Error| {
                    ProtocolError::AuthorizationFailed(format!("v2 bundle: {e}"))
                })?;
        let v2_envelope = CapabilityBundleV2Envelope::new(v2_bundle);
        let v2_envelope_bytes = v2_envelope
            .canonical_ser()
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("v2 envelope ser: {e}")))?;

        let response = IssueResponse {
            holder_did: req.holder_did.clone(),
            token_id,
            v2_envelope_bytes: v2_envelope_bytes.clone(),
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::CAPABILITY_ISSUE)
                .with_note(format!(
                    "issued (MVP stub + V2 envelope {} bytes) for {}",
                    v2_envelope_bytes.len(),
                    req.holder_did
                ))
                .with_v2_envelope(v2_envelope_bytes),
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
            v2_envelope_bytes: vec![0u8; 0],
        };
        let bytes = resp.to_borsh_value();
        let back: IssueResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }

    /// TV (mission 0957-f-v2-bundle-consumer-migration) — issue emits
    /// a V2 `CapabilityBundleV2Envelope` as a root bundle
    /// (`chain_depth = 0`, `chain_parent = [0u8; 32]`). The
    /// `v2_envelope_bytes` field carries the canonical_ser bytes;
    /// downstream V2 consumers decode via
    /// `CapabilityBundleV2Envelope::canonical_de`.
    #[test]
    fn issue_emits_v2_root_bundle_envelope() {
        use octo_cap_macaroon::{
            CapabilityBundleV2Envelope, CIPHEROCTO_V2_BUNDLE_PREFIX, MAX_CHAIN_DEPTH,
        };
        let out = IssueHandler::new()
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability: [0xab; 32],
            })
            .unwrap();
        let resp: IssueResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();
        // Envelope bytes are non-empty + start with V2 prefix.
        assert!(!resp.v2_envelope_bytes.is_empty());
        assert_eq!(
            &resp.v2_envelope_bytes[..16],
            CIPHEROCTO_V2_BUNDLE_PREFIX.as_slice(),
            "issue envelope must carry canonical V2 prefix"
        );
        // Decode roundtrip preserves root invariants.
        let env = CapabilityBundleV2Envelope::canonical_de(&resp.v2_envelope_bytes).expect("de");
        assert_eq!(env.bundle.token_v2.chain_depth, 0, "root mint = depth 0");
        assert_eq!(
            env.bundle.token_v2.chain_parent, [0u8; 32],
            "root mint = null parent binding"
        );
        assert!(env.bundle.token_v2.chain_depth <= MAX_CHAIN_DEPTH);
        assert_eq!(env.bundle.token_v2.audience_did, sample_did());
        // HandlerOutput also surfaces the envelope bytes.
        assert_eq!(out.v2_envelope_bytes, Some(resp.v2_envelope_bytes));
    }

    // Helper: derive borsh bytes for IssueResponse (no impl on the type
    // to avoid leaking the wire format into the production API).
    impl IssueResponse {
        pub(crate) fn to_borsh_value(&self) -> Vec<u8> {
            borsh::to_vec(self).expect("IssueResponse borsh encode")
        }
    }
}
