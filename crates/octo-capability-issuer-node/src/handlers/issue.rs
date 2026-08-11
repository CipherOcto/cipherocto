//! `CAPABILITY_ISSUE` handler (RFC-0871 §Roles and Authorities, mission
//! 0871d-capability-issuer-node).
//!
//! Receives: `<holder_did: String, capability: [u8; 32]>` — the holder
//! DID + a 32-byte capability root secret.
//! Returns: V2 `CapabilityBundleV2Envelope` canonical_ser bytes — the
//! canonical wire form post-cutover (mission `0957-f-v2-bundle-cutover`).
//!
//! ## Phase 3 MVP
//!
//! The handler validates `holder_did` shape via
//! `octo_ident::CanonicalCodec::parse(s, false)` (RFC-0010 v1.2 F4)
//! and derives a deterministic 16-byte `token_id` from the 32-byte
//! capability root via `octo_cap_macaroon::macaroon_id`. The token_id
//! is encoded as `CapabilityTokenV2.channel_id` in the V2 envelope;
//! downstream consumers (Wallet, `octo-cap-zk`) recover it via
//! `CapabilityBundleV2Envelope::canonical_de`.
//!
//! The full substrate flow:
//! 1. `CapabilityToken::mint(&capability, &issuer_key, &holder_did, &[])`
//!    (RFC-0957 §Algorithms) — requires holder's pre-signed commitment
//!    envelope (RFC-0871 §Authorization model) and calls `IdentityKey::sign`
//!    (HSM-routed via `Arc<dyn HsmAdapter>` per mission 0009-a).
//! 2. `HolderRegistry::register(token)` (RFC-0957-A1 §Data Structures).
//! 3. Returns the V2 envelope bytes (mission `0957-f-v2-bundle-cutover`).
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

    /// Validate `holder_did` shape; mint a V2 envelope root bundle and
    /// emit it as the primary `response_payload`.
    ///
    /// Post-cutover (mission `0957-f-v2-bundle-cutover`): the envelope
    /// IS the wire form. `channel_id` on the envelope is the
    /// deterministic 16-byte `token_id` (RFC-0957 §3.2).
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `holder_did` is not a
    /// canonical DID shape.
    pub fn handle(&self, req: &IssueRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate canonical DID shape; reject legacy bare form.
        // (RFC-0010 v1.2 F4 + mission 0010-d wallet-audience-validation.)
        octo_ident::CanonicalCodec::parse(&req.holder_did, false).map_err(did_error_to_protocol)?;

        // Phase 3 MVP: derive deterministic 16-byte token_id from the
        // 32-byte capability root via the macaroon-id primitive. Encoded
        // as `channel_id` on the V2 envelope (the full macaroon substrate
        // lands in mission 0957 Phase 2).
        let mut nonce = [0u8; 16];
        nonce.copy_from_slice(&req.capability[..16]);
        let token_id: MacaroonId = macaroon_id(&req.capability, &nonce);

        // V2 root bundle envelope (mission 0957-f-v2-bundle-cutover).
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

        Ok(HandlerOutput::response(
            v2_envelope_bytes,
            octo_protocol::payload_kind::CAPABILITY_ISSUE,
        )
        .with_note(format!(
            "issued (MVP stub + V2 envelope) for {}",
            req.holder_did
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::{
        CapabilityBundleV2Envelope, CIPHEROCTO_V2_BUNDLE_PREFIX, MAX_CHAIN_DEPTH,
    };

    fn sample_did() -> String {
        // Derive a canonical DID from a deterministic 32-byte payload so
        // tests don't depend on a crypto adapter crate (the handler is
        // pure for the MVP per mission 0871d scope).
        let pk = [0x42u8; 32];
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    /// Helper: extract the V2 envelope from `out.response_payload` and
    /// decode it. Mission `0957-f-v2-bundle-cutover`: `response_payload`
    /// IS the V2 envelope via `canonical_ser()` (16-byte prefix + borsh).
    fn envelope_from(out: &HandlerOutput) -> CapabilityBundleV2Envelope {
        let bytes = out
            .response_payload
            .as_ref()
            .expect("V2 envelope is the primary response payload post-cutover");
        assert_eq!(
            &bytes[..16],
            CIPHEROCTO_V2_BUNDLE_PREFIX.as_slice(),
            "response_payload must start with canonical V2 prefix"
        );
        CapabilityBundleV2Envelope::canonical_de(bytes).expect("canonical_de")
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

    /// TV — `response_payload` is the V2 envelope canonical_ser bytes
    /// (16-byte prefix + borsh-encoded bundle). The envelope's
    /// `channel_id` carries the deterministic 16-byte `token_id`
    /// derived from the capability root.
    #[test]
    fn handle_emits_v2_envelope_with_channel_id_as_token_id() {
        let handler = IssueHandler::new();
        let capability = [0xcdu8; 32];
        let req = IssueRequest {
            holder_did: sample_did(),
            capability,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        // channel_id is the deterministic 16-byte token_id.
        let mut nonce = [0u8; 16];
        nonce.copy_from_slice(&capability[..16]);
        let expected_token_id = macaroon_id(&capability, &nonce);
        assert_eq!(
            env.bundle.token_v2.channel_id, expected_token_id,
            "envelope channel_id = deterministic token_id"
        );
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::CAPABILITY_ISSUE)
        );
    }

    /// TV — distinct capability roots produce distinct `channel_id`
    /// (no accidental collision).
    #[test]
    fn handle_channel_id_varies_with_capability() {
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
        let env_a = envelope_from(&out_a);
        let env_b = envelope_from(&out_b);
        assert_ne!(
            env_a.bundle.token_v2.channel_id,
            env_b.bundle.token_v2.channel_id
        );
    }

    /// TV — issue envelope carries root invariants: `chain_depth = 0`,
    /// `chain_parent = [0u8; 32]`, `audience_did` = holder.
    #[test]
    fn handle_emits_v2_root_bundle_envelope() {
        let out = IssueHandler::new()
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability: [0xab; 32],
            })
            .unwrap();
        let env = envelope_from(&out);
        assert_eq!(env.prefix, *CIPHEROCTO_V2_BUNDLE_PREFIX);
        assert_eq!(env.bundle.token_v2.chain_depth, 0, "root mint = depth 0");
        assert_eq!(
            env.bundle.token_v2.chain_parent, [0u8; 32],
            "root mint = null parent binding"
        );
        assert!(env.bundle.token_v2.chain_depth <= MAX_CHAIN_DEPTH);
        assert_eq!(env.bundle.token_v2.audience_did, sample_did());
    }

    /// TV — `holder_record_bytes` carries the 32-byte capability root
    /// as MVP placeholder (the substrate HolderRecord bytes land in a
    /// follow-on mission).
    #[test]
    fn handle_holder_record_bytes_carries_capability_root() {
        let capability = [0xa1u8; 32];
        let out = IssueHandler::new()
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability,
            })
            .unwrap();
        let env = envelope_from(&out);
        assert_eq!(env.bundle.holder_record_bytes, capability.to_vec());
    }
}
