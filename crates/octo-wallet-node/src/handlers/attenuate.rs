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
//! HMAC chain is re-derived; and the holder signature is **re-derived
//! with the wallet's holder key** so the result is a verifiable token.
//!
//! Earlier revisions of this handler called `attenuate` (no signer),
//! which sets `holder_sig_stale`. `verify_holder_sig` refuses a stale
//! token outright, so every capability this handler produced was one no
//! verifier would accept — the wallet -> attenuate -> verify path did not
//! exist. Mission `0957-phase2d` listed exactly that under "Blocks:
//! production attenuation end-to-end"; this handler now satisfies it.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::caveat::Caveat;
use octo_cap_macaroon::macaroon::InMemoryCatalog;
use octo_cap_macaroon::wire::{deserialize_wire, serialize_wire};
use octo_protocol::ProtocolError;
use octo_wallet::identity::IdentityKey;

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
pub struct AttenuateHandler<'a> {
    identity: &'a IdentityKey,
    catalog: InMemoryCatalog,
}

impl<'a> AttenuateHandler<'a> {
    /// Construct a new `AttenuateHandler` bound to the wallet's holder key,
    /// with a default (empty) `InMemoryCatalog`. The catalog enforces the
    /// `WrappedOnly` chain guard (RFC-0965 §3.7) on attenuation;
    /// for non-WrappedOnly caveats, an empty catalog is sufficient
    /// (the guard is a no-op walk over an empty parent set).
    #[must_use]
    pub fn new(identity: &'a IdentityKey) -> Self {
        Self {
            identity,
            catalog: InMemoryCatalog::default(),
        }
    }

    /// Construct a handler with a pre-populated catalog (for tests
    /// that register `Caveat::Raw` names per RFC-0957-a AC #13).
    #[must_use]
    pub const fn with_catalog(identity: &'a IdentityKey, catalog: InMemoryCatalog) -> Self {
        Self { identity, catalog }
    }

    /// Attenuate the existing token by appending `new_caveat`, re-signing with
    /// the wallet's holder key.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if the wire-form
    /// DID lookup fails. Returns `ProtocolError::AuthorizationFailed`
    /// on borsh decode / macaroon wire deserialize failure / macaroon
    /// attenuation failure (catalog rejection, depth exceeded,
    /// WrappedParentNotFound, etc.), or if the holder key refuses to sign.
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
        //
        // The empty DID is safe downstream and that is not an accident:
        // `verify_with_resolve` re-deserializes with the *registry's*
        // holder_did, which is authoritative over anything the wire
        // claims (R7-N7). What the wire must get right is the holder
        // signature, and that is re-derived below.
        let token = deserialize_wire(&req.existing_token_wire, String::new(), &req.holder_pub)
            .map_err(wire_error_to_protocol)?;

        // Parse the new caveat from canonical-JSON bytes.
        let new_caveat = parse_new_caveat(&req.new_caveat_json)?;

        // Append the new caveat AND re-sign. The catalog guards
        // WrappedOnly chains; for other caveat variants, the guard is a
        // no-op (no parents to walk).
        //
        // `attenuate_with_signer`, not `attenuate`: the unsigned variant
        // sets `holder_sig_stale`, and `verify_holder_sig` refuses any
        // stale token outright. An unsigned attenuation therefore produced
        // a capability that no verifier in the workspace would accept —
        // which is the wallet -> attenuate -> verify path this handler
        // exists to provide. Mission 0957-phase2d recorded this under
        // "Blocks: production attenuation end-to-end"; this is that fix.
        let attenuated = token
            .attenuate_with_signer(new_caveat, self.identity, &self.catalog)
            .map_err(wallet_error_to_protocol)?;

        // Real wire form (RFC-0957 §3.7). Replaces the Phase 1 MVP
        // `CIPHEROCTO_MINT_V1_ATTENUATED:*` placeholder.
        let new_wire = serialize_wire(&attenuated).map_err(wire_error_to_protocol)?;

        Ok(HandlerOutput::response(
            new_wire.into_bytes(),
            octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY,
        )
        .with_note(format!(
            "attenuated and re-signed ({} caveats)",
            attenuated.macaroon.caveats.len()
        )))
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
            holder_pub: sample_identity()
                .public_key_bytes_array()
                .expect("the test identity is a 32-byte ed25519 key"),
        };
        let bytes = req.to_borsh().unwrap();
        let back = AttenuateRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_unparseable_wire_form() {
        let identity = sample_identity();
        let handler = AttenuateHandler::new(&identity);
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
        let identity = sample_identity();
        let handler = AttenuateHandler::new(&identity);
        let parent_wire = minted_token_wire();
        let parent_hash = compute_cap_root_hash_from_wire(&parent_wire).expect("parent hash");
        let caveat_json = serde_json::to_vec(&Caveat::Before(2_000_000_000)).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: parent_wire.clone(),
            new_caveat_json: caveat_json,
            holder_pub: sample_identity()
                .public_key_bytes_array()
                .expect("the test identity is a 32-byte ed25519 key"),
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
        let identity = sample_identity();
        let handler = AttenuateHandler::new(&identity);
        let caveat_json =
            serde_json::to_vec(&Caveat::Model("gpt-4".to_owned())).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: minted_token_wire(),
            new_caveat_json: caveat_json,
            holder_pub: sample_identity()
                .public_key_bytes_array()
                .expect("the test identity is a 32-byte ed25519 key"),
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let new_wire = std::str::from_utf8(&payload).unwrap();
        let attenuated = deserialize_wire(
            new_wire,
            sample_did(),
            &sample_identity()
                .public_key_bytes_array()
                .expect("the test identity is a 32-byte ed25519 key"),
        )
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
        let identity = sample_identity();
        let handler = AttenuateHandler::new(&identity);
        let caveat_json = serde_json::to_vec(&Caveat::Before(2_000_000_000)).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: String::new(),
            new_caveat_json: caveat_json,
            holder_pub: [0u8; 32],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    /// TV4 — **the wallet -> attenuate -> verify path exists.**
    ///
    /// This is the property the previous implementation could not satisfy at
    /// any setting. Attenuation used `CapabilityToken::attenuate`, which does
    /// not re-sign: the emitted `holder_sig` still covers the *pre*-attenuation
    /// caveat list. The signature therefore stops matching the moment the
    /// caveat is appended, and every capability this handler produced was
    /// unverifiable.
    ///
    /// Note *how* it failed, because it is not the obvious way. `attenuate`
    /// sets `holder_sig_stale = true`, and `verify_holder_sig` short-circuits
    /// on that flag — but `deserialize_wire` hardcodes `holder_sig_stale` to
    /// `false`, so the flag is erased by the wire round-trip and the stale
    /// branch never fires on a token that arrived over the wire. The signature
    /// check runs anyway and fails cryptographically. Fail-closed either way,
    /// but the staleness diagnosis is lost, and the "surface stale state at
    /// the verify boundary" safety net only protects in-process tokens.
    #[test]
    fn handle_output_passes_the_holder_signature_check() {
        let identity = sample_identity();
        let handler = AttenuateHandler::new(&identity);
        let parent_wire = minted_token_wire();
        let caveat_json =
            serde_json::to_vec(&Caveat::Model("gpt-4".to_owned())).expect("caveat ser");
        let req = AttenuateRequest {
            existing_token_wire: parent_wire,
            new_caveat_json: caveat_json,
            holder_pub: identity
                .public_key_bytes_array()
                .expect("32-byte ed25519 key"),
        };

        let out = handler.handle(&req).expect("attenuate");
        let payload = out.response_payload.unwrap();
        let new_wire = std::str::from_utf8(&payload).expect("utf8");
        let attenuated = deserialize_wire(
            new_wire,
            sample_did(),
            &identity
                .public_key_bytes_array()
                .expect("32-byte ed25519 key"),
        )
        .expect("deserialize");

        attenuated
            .verify_holder_sig()
            .expect("an attenuated capability must verify, or the wallet path is a dead end");
    }

    /// TV5 — attenuation must be verifiable **and** carry the binding, so the
    /// ask a holder attenuates onto a capability survives to the point where a
    /// verifier reads it.
    ///
    /// This is the shape the RFC-0969 gateway needs: an ask bound at mint time
    /// or at attenuation time, readable via `ask_binding()` off a token that
    /// actually passes verification.
    #[test]
    fn an_attenuated_ask_binding_is_readable_and_verifiable() {
        let identity = sample_identity();
        let handler = AttenuateHandler::new(&identity);
        let ask = [0xA5u8; 32];
        let caveat_json =
            serde_json::to_vec(&Caveat::AskBinding(ask)).expect("caveat ser: ask binding");
        let req = AttenuateRequest {
            existing_token_wire: minted_token_wire(),
            new_caveat_json: caveat_json,
            holder_pub: identity
                .public_key_bytes_array()
                .expect("32-byte ed25519 key"),
        };

        let out = handler.handle(&req).expect("attenuate with an ask binding");
        let payload = out.response_payload.unwrap();
        let new_wire = std::str::from_utf8(&payload).expect("utf8");
        let attenuated = deserialize_wire(
            new_wire,
            sample_did(),
            &identity
                .public_key_bytes_array()
                .expect("32-byte ed25519 key"),
        )
        .expect("deserialize");

        attenuated.verify_holder_sig().expect("must verify");
        assert_eq!(
            attenuated.ask_binding().expect("single binding"),
            Some(ask),
            "an attenuated ask binding must be readable by the gateway verifier"
        );
    }
}
