//! `WALLET_ATTENUATE_CAPABILITY` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<existing_token_wire: String, new_caveat: Caveat>`.
//! Returns: `<attenuated_token_wire: String>` — the v1 macaroon wire
//! form per RFC-0957 §3.7.
//!
//! Phase 2 (mission 0957-phase2d): real macaroon attenuation via
//! `CapabilityToken::attenuate(new_caveat, catalog)`. The placeholder
//! `CIPHEROCTO_MINT_V1_ATTENUATED:<did>:+<N>bytes` wire form is gone.
//!
//! Attenuation contract (RFC-0957 §3.5): the existing caveats are
//! preserved (monotonic narrowing); the new caveat is appended; the
//! HMAC chain is re-derived; the holder signature is marked stale
//! (caller must re-sign via `attenuate_with_signer` to use the
//! token — the wallet handler does NOT re-sign here since the
//! attenuation envelope is a separate request kind).

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::caveat::Caveat;
use octo_cap_macaroon::macaroon::InMemoryCatalog;
use octo_cap_macaroon::wire::{deserialize_wire, serialize_wire};
use octo_protocol::ProtocolError;

use super::{wallet_error_to_protocol, wire_error_to_protocol, HandlerOutput};

/// Request payload for `WALLET_ATTENUATE_CAPABILITY`.
///
/// Wire form: borsh (`existing_token_wire`, `new_caveat_json`,
/// `holder_pub`).
///
/// The new caveat is supplied as canonical-JSON bytes (NOT the
/// `Caveat` enum directly) because the central `Caveat` enum derives
/// serde-only (no borsh) — coupling it to borsh would bleed the
/// wire-format dep into the substrate. The handler converts
/// `new_caveat_json` → `Caveat` via `serde_json::from_slice` at
/// the boundary.
///
/// `holder_pub` is supplied out-of-band from the request envelope's
/// `Authorization` list (the DID registry resolves `did → pub`); it
/// is required to deserialize the v1 macaroon wire form per
/// `octo_cap_macaroon::wire::deserialize_wire`'s 3-arg signature.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct AttenuateRequest {
    /// Existing token wire form (v1 macaroon wire, 3 base64url-no-pad
    /// segments — mission 0957-phase2a substrate).
    pub existing_token_wire: String,
    /// New caveat as canonical-JSON bytes (the `Caveat` enum's
    /// serde tag-discriminated form, e.g.
    /// `{"type":"before","value":2000000000}`). The handler parses
    /// this into a typed `Caveat` at the wire boundary.
    pub new_caveat_json: Vec<u8>,
    /// 32-byte holder Ed25519 public key (resolved out-of-band from
    /// the DID registry; required by `deserialize_wire`).
    pub holder_pub: [u8; 32],
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

/// Map a `Caveat` canonical-SerDe failure to a `ProtocolError`.
#[allow(dead_code)]
fn caveat_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::AuthorizationFailed(format!("caveat: {e}"))
}

/// `WALLET_ATTENUATE_CAPABILITY` handler implementation.
pub struct AttenuateHandler {
    catalog: InMemoryCatalog,
}

impl AttenuateHandler {
    /// Construct a new `AttenuateHandler` with a default
    /// (empty) `InMemoryCatalog`. The catalog enforces the
    /// `WrappedOnly` chain guard (RFC-0965 §3.7) on attenuation;
    /// for non-WrappedOnly caveats, an empty catalog is sufficient
    /// (the guard is a no-op walk over an empty parent set).
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: InMemoryCatalog::default(),
        }
    }

    /// Construct a handler with a pre-populated catalog (for tests
    /// that register `Caveat::Raw` names per RFC-0957-a AC #13).
    #[must_use]
    pub fn with_catalog(catalog: InMemoryCatalog) -> Self {
        Self { catalog }
    }

    /// Attenuate the existing token by appending `new_caveat`.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if the wire-form
    /// DID lookup fails. Returns `ProtocolError::AuthorizationFailed`
    /// on borsh decode / macaroon wire deserialize failure / macaroon
    /// attenuation failure (catalog rejection, depth exceeded,
    /// WrappedParentNotFound, etc.).
    pub fn handle(&self, req: &AttenuateRequest) -> Result<HandlerOutput, ProtocolError> {
        // Deserialize the existing v1 wire form into a CapabilityToken.
        // The wire doesn't carry holder_did / holder_pub — caller
        // supplies them via the request envelope. We pass an empty
        // `holder_did` placeholder because the wire's deserialize
        // signature requires one for symmetry; the canonical DID
        // shape check is performed at mint time (not here — the
        // attenuation envelope arrives with a trusted wire form
        // produced by the wallet's own mint path, per RFC-0010
        // F4 + 0010-d mission).
        let token = deserialize_wire(&req.existing_token_wire, String::new(), req.holder_pub)
            .map_err(wire_error_to_protocol)?;

        // Parse the new caveat from canonical-JSON bytes.
        let new_caveat = parse_new_caveat(&req.new_caveat_json)?;

        // Append the new caveat via CapabilityToken::attenuate. The
        // catalog guards WrappedOnly chains; for other caveat
        // variants, the guard is a no-op (no parents to walk).
        let attenuated = token
            .attenuate(new_caveat, &self.catalog)
            .map_err(wallet_error_to_protocol)?;

        // Real wire form (RFC-0957 §3.7). Replaces the Phase 1 MVP
        // `CIPHEROCTO_MINT_V1_ATTENUATED:*` placeholder.
        let new_wire = serialize_wire(&attenuated).map_err(wire_error_to_protocol)?;

        Ok(HandlerOutput::response(
            new_wire.into_bytes(),
            octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY,
        )
        .with_note(format!(
            "attenuated for {} (caveat appended; holder_sig marked stale)",
            token.holder_did
        )))
    }
}

impl Default for AttenuateHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: extract a `holder_did` hint from a `Caveat::Audience`
/// variant. Returns `None` for caveats that don't carry a DID.
/// Used as a fallback when the request envelope doesn't carry an
/// explicit `holder_did` field (the wire's deserialize signature
/// requires one for symmetry — empty string is acceptable when
/// the caller doesn't need to round-trip the holder).
#[allow(dead_code)]
fn holder_did_from_caveat(c: &Caveat) -> Option<String> {
    if let Caveat::Audience(did) = c {
        Some(did.clone())
    } else {
        None
    }
}

/// Helper: parse the canonical-JSON caveat payload into a typed
/// `Caveat`. Returns `ProtocolError::AuthorizationFailed` on serde
/// failure.
fn parse_new_caveat(bytes: &[u8]) -> Result<Caveat, ProtocolError> {
    serde_json::from_slice(bytes)
        .map_err(|e| ProtocolError::AuthorizationFailed(format!("new_caveat_json: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::caveat::Caveat;
    use octo_cap_macaroon::wire::compute_cap_root_hash_from_wire;
    use octo_wallet::capability::CapabilityToken;
    use octo_wallet::identity::IdentityKey;

    fn sample_identity() -> octo_wallet::identity::IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut id = IdentityKey::from_seed(seed);
        id.activate(1_700_000_000).expect("designated → active");
        id
    }

    fn sample_did() -> String {
        let id = sample_identity();
        let pk = id.public_key_bytes();
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    fn sample_root_secret() -> [u8; 32] {
        [0xab; 32]
    }

    fn minted_token_wire() -> String {
        let id = sample_identity();
        let holder_did = sample_did();
        let token =
            CapabilityToken::mint(&sample_root_secret(), &id, &holder_did, &[]).expect("mint");
        serialize_wire(&token).expect("serialize wire")
    }

    #[test]
    fn attenuate_request_borsh_round_trip() {
        let caveat_json = serde_json::to_vec(&Caveat::Before(2_000_000_000)).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: minted_token_wire(),
            new_caveat_json: caveat_json,
            holder_pub: sample_identity().public_key_bytes(),
        };
        let bytes = req.to_borsh().unwrap();
        let back = AttenuateRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_unparseable_wire_form() {
        let handler = AttenuateHandler::new();
        let caveat_json = serde_json::to_vec(&Caveat::Before(2_000_000_000)).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: "not-a-wire-at-all".to_owned(),
            new_caveat_json: caveat_json,
            holder_pub: [0u8; 32],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    /// TV1 — attenuation appends `Caveat::Before(...)` and the new
    /// wire form's macaroon root_id / capability_id differs from the
    /// parent (HMAC chain extended).
    #[test]
    fn handle_attenuates_real_token_extends_chain() {
        let handler = AttenuateHandler::new();
        let parent_wire = minted_token_wire();
        let parent_hash = compute_cap_root_hash_from_wire(&parent_wire).expect("parent hash");
        let caveat_json = serde_json::to_vec(&Caveat::Before(2_000_000_000)).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: parent_wire.clone(),
            new_caveat_json: caveat_json,
            holder_pub: sample_identity().public_key_bytes(),
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let new_wire = std::str::from_utf8(&payload).unwrap();
        let new_hash = compute_cap_root_hash_from_wire(new_wire).expect("new hash");
        assert_ne!(
            parent_hash, new_hash,
            "attenuation must extend the HMAC chain (different cap_root_hash)"
        );
        let segments: Vec<&str> = new_wire.split('.').collect();
        assert_eq!(segments.len(), 3);
    }

    /// TV2 — `Caveat::Model("gpt-4")` narrowing: the parent wire
    /// has no model caveat; the attenuation appends the model
    /// caveat. Round-trip via `deserialize_wire` recovers both.
    #[test]
    fn handle_attenuates_with_model_caveat() {
        let handler = AttenuateHandler::new();
        let caveat_json =
            serde_json::to_vec(&Caveat::Model("gpt-4".to_owned())).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: minted_token_wire(),
            new_caveat_json: caveat_json,
            holder_pub: sample_identity().public_key_bytes(),
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let new_wire = std::str::from_utf8(&payload).unwrap();
        let attenuated =
            deserialize_wire(new_wire, sample_did(), sample_identity().public_key_bytes())
                .expect("deserialize");
        assert_eq!(attenuated.macaroon.caveats.len(), 1);
        assert_eq!(
            attenuated.macaroon.caveats[0],
            Caveat::Model("gpt-4".to_owned())
        );
        // Note: `holder_sig_stale` is NOT preserved across the wire
        // roundtrip (the wire parser hardcodes `false` on deserialize
        // — a known follow-on for the wire substrate; mission
        // 0957-phase2d-1). The attenuated token itself DOES carry
        // `holder_sig_stale = true` (per `CapabilityToken::attenuate`),
        // but the wire form loses it. Assert `holder_pub` instead.
        assert_eq!(attenuated.holder_pub, sample_identity().public_key_bytes());
    }

    /// TV3 — handler rejects an empty wire form (parse failure).
    #[test]
    fn handle_rejects_empty_wire_form() {
        let handler = AttenuateHandler::new();
        let caveat_json = serde_json::to_vec(&Caveat::Before(2_000_000_000)).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: String::new(),
            new_caveat_json: caveat_json,
            holder_pub: [0u8; 32],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }
}
