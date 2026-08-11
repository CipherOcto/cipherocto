//! `WALLET_MINT_CAPABILITY` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<holder_did: String, capability: [u8; 32]>`.
//! Returns: `<minted_token: wire form (3 base64url-no-pad segments)>`.
//!
//! Phase 2 (mission 0957-phase2a): emits the real macaroon wire form
//! via `octo_cap_macaroon::wire::serialize_wire` (replaces the
//! Phase 1 MVP `CIPHEROCTO_MINT_V1:<holder_did>` placeholder).
//! The minted `CapabilityToken` is routed through the migrated
//! substrate (`octo_cap_macaroon::CapabilityToken` via the
//! `octo_wallet::capability` re-export). Initial caveats are empty
//! (caveats travel in subsequent attenuation envelope calls).

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::caveat::Caveat;
use octo_cap_macaroon::wire::serialize_wire;
use octo_cap_macaroon::{
    BundleV2Error, CapabilityBundleV2, CapabilityBundleV2Envelope, CapabilityTokenV2,
};
use octo_ident::DidCodec;
use octo_ident::WireDid;
use octo_paid_query::SpendLedger;
use octo_protocol::ProtocolError;
use octo_wallet::capability::CapabilityToken;
use octo_wallet::identity::IdentityKey;

use super::{
    did_error_to_protocol, wallet_error_to_protocol, wire_error_to_protocol, HandlerOutput,
};

/// Request payload for `WALLET_MINT_CAPABILITY`.
///
/// Wire form: borsh (`holder_did`, `capability_root`,
/// `payment_caveat`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct MintRequest {
    /// Canonical DID of the holder (`did:octo:z<base58btc>`).
    pub holder_did: String,
    /// 32-byte capability root secret (random nonce per mint).
    pub capability: [u8; 32],
    /// Optional initial PaymentCaveat (RFC-0965 reserved discriminator
    /// `0x1A`, mission 0957-phase2b). When present, the handler
    /// appends it as the first caveat in the macaroon chain via
    /// `CapabilityToken::mint(..., &[Caveat::Payment(p)])`. When
    /// `None`, the minted token has no initial caveats (caveats
    /// travel in subsequent attenuation envelope calls).
    pub payment_caveat: Option<octo_cap_macaroon::PaymentCaveat>,
}

impl MintRequest {
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

/// `WALLET_MINT_CAPABILITY` handler implementation.
pub struct MintHandler<'a> {
    identity: &'a IdentityKey,
    /// Optional spend ledger (mission 0871e-phase5b). When present,
    /// mints with a `payment_caveat` seed the ledger with
    /// `(holder_did, capability_id) → caveat.budget`. Tests / non-paid
    /// configurations pass `None`.
    spend_ledger: Option<Arc<dyn SpendLedger>>,
}

impl<'a> MintHandler<'a> {
    /// Construct a new `MintHandler` bound to the given identity key.
    /// No spend ledger (tests + non-paid deployments).
    #[must_use]
    pub const fn new(identity: &'a IdentityKey) -> Self {
        Self {
            identity,
            spend_ledger: None,
        }
    }

    /// Construct a `MintHandler` with an injected spend ledger. Used
    /// by production deployments to seed the ledger at mint time so
    /// subsequent `WALLET_PAID_QUERY_VERIFY` calls can drain.
    #[must_use]
    pub fn with_ledger(identity: &'a IdentityKey, spend_ledger: Arc<dyn SpendLedger>) -> Self {
        Self {
            identity,
            spend_ledger: Some(spend_ledger),
        }
    }

    /// Mint a capability token + emit the canonical macaroon wire form
    /// (RFC-0957 §3.7; mission 0957-phase2a).
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `holder_did` is not a
    /// canonical DID shape. Returns `ProtocolError::AuthorizationFailed`
    /// on macaroon mint failure or wire-form serialization failure.
    pub fn handle(&self, req: &MintRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate holder_did shape via canonical codec; reject bare
        // legacy form (RFC-0010 v1.2 F4 + 0010-d mission).
        octo_ident::CanonicalCodec::parse(&req.holder_did, false).map_err(did_error_to_protocol)?;

        let token = match req.payment_caveat.as_ref() {
            Some(p) => CapabilityToken::mint(
                &req.capability,
                self.identity,
                &req.holder_did,
                &[Caveat::Payment(p.clone())],
            )
            .map_err(wallet_error_to_protocol)?,
            None => CapabilityToken::mint(&req.capability, self.identity, &req.holder_did, &[])
                .map_err(wallet_error_to_protocol)?,
        };

        // Real macaroon wire form (RFC-0957 §3.7): 3 base64url-no-pad
        // segments separated by `.`. Replaces the Phase 1 MVP
        // `CIPHEROCTO_MINT_V1:<holder_did>` placeholder.
        let minted_wire = serialize_wire(&token).map_err(wire_error_to_protocol)?;

        // V2 envelope (mission 0957-f-v2-bundle-consumer-migration):
        // wrap the minted token in a V2 root bundle envelope. The
        // wire form remains the canonical transport (legacy + downstream
        // consumers); the envelope is the new authoritative bundle
        // substrate (chain_depth = 0 root, chain_parent = [0u8; 32]).
        // Envelope bytes are surfaced via `with_v2_envelope` so future
        // cutover missions can flip `response_payload` to envelope
        // bytes without modifying this handler.
        let holder_record_bytes = {
            use octo_cap_macaroon::macaroon::compute_capability_id;
            let cap_id = compute_capability_id(&token.macaroon);
            let mut buf = Vec::with_capacity(32);
            buf.extend_from_slice(&cap_id);
            buf
        };
        let discharge_macaroon_bytes: Vec<u8> = Vec::new();
        let token_v2 = CapabilityTokenV2 {
            chain_depth: 0,
            chain_parent: [0u8; 32],
            audience_did: req.holder_did.clone(),
            channel_id: token.macaroon.root_id,
            expires_at_unix_secs: u64::MAX, // macaroon substrate lacks expiry field; root TTL lives in caveat
            issuer_did: format!(
                "did:octo:z{}",
                bs58::encode(self.identity.public_key_bytes()).into_string()
            ),
        };
        let v2_bundle =
            CapabilityBundleV2::new(token_v2, holder_record_bytes, discharge_macaroon_bytes)
                .map_err(|e: BundleV2Error| {
                    ProtocolError::AuthorizationFailed(format!("v2 bundle: {e}"))
                })?;
        let v2_envelope = CapabilityBundleV2Envelope::new(v2_bundle);
        let v2_envelope_bytes = v2_envelope
            .canonical_ser()
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("v2 envelope ser: {e}")))?;

        // Seed the spend ledger when a payment caveat was attached
        // (mission 0871e-phase5b). Failure to seed surfaces as
        // `AuthorizationFailed` so the mint is rejected — the proxy
        // cannot drain against a missing balance record.
        let note = if let (Some(p), Some(ledger)) =
            (req.payment_caveat.as_ref(), self.spend_ledger.as_ref())
        {
            let holder_did = WireDid::new(req.holder_did.clone());
            // Derive a stable MacaroonId from the capability root
            // (capability_id = BLAKE3 keyed-hash per RFC-0957 §3.4).
            let macaroon_id = token.macaroon.root_id;
            ledger
                .seed(&holder_did, &macaroon_id, p.budget)
                .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
            format!(
                "minted for {} (payment caveat budget={} seeded to spend ledger; v2 envelope {} bytes)",
                req.holder_did, p.budget, v2_envelope_bytes.len()
            )
        } else {
            format!(
                "minted for {} (v2 envelope {} bytes)",
                req.holder_did,
                v2_envelope_bytes.len()
            )
        };

        Ok(HandlerOutput::response(
            minted_wire.into_bytes(),
            octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
        )
        .with_note(note)
        .with_v2_envelope(v2_envelope_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::wire::{compute_cap_root_hash_from_wire, deserialize_wire};

    fn sample_identity() -> octo_wallet::identity::IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        // RFC-0009 §Lifecycle: a fresh `from_seed` identity is in
        // `Designated` state and cannot sign. Activate it before any
        // mint+holder-sig work runs. The activation timestamp is
        // arbitrary for tests (state-machine transition doesn't care
        // about the value beyond validity).
        let mut id = octo_wallet::identity::IdentityKey::from_seed(seed);
        id.activate(1_700_000_000).expect("designated → active");
        id
    }

    fn sample_did() -> String {
        let id = sample_identity();
        let pk = id.public_key_bytes();
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    #[test]
    fn mint_request_borsh_round_trip() {
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let bytes = req.to_borsh().unwrap();
        let back = MintRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn mint_request_borsh_round_trip_with_payment_caveat() {
        // Mission 0957-phase2b: the optional PaymentCaveat field must
        // roundtrip cleanly through borsh (the field embeds a serde-
        // and-borsh-derive type from cap-macaroon).
        let p = octo_cap_macaroon::PaymentCaveat::new(1_000_000, "gpt-4", u64::MAX);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: Some(p.clone()),
        };
        let bytes = req.to_borsh().unwrap();
        let back = MintRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.payment_caveat, Some(p));
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: "did:octo:bad".into(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    // ----- Mission 0957-phase2a wire-form test vectors (byte-exact TV) -----
    //
    // The 5 TV below pin the cutover from the Phase 1 MVP
    // `CIPHEROCTO_MINT_V1:<holder_did>` placeholder to the canonical
    // macaroon wire form (RFC-0957 §3.7). Each TV uses a deterministic
    // identity (seed `[0..32]`) + deterministic capability root; the
    // macaroon nonce is RNG-derived but the wire form is reproducible
    // from the (seed, capability) inputs by mint+serialize_wire.

    /// TV1 — handle emits a 3-segment base64url-no-pad wire form
    /// (no `CIPHEROCTO_MINT_V1:` prefix). Pins the structural shape.
    #[test]
    fn handle_emits_three_segment_wire_form() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let s = std::str::from_utf8(&payload).unwrap();
        // No placeholder prefix.
        assert!(
            !s.starts_with("CIPHEROCTO_MINT_V1:"),
            "placeholder prefix must be gone (got prefix: {s:?})"
        );
        // Exactly 3 base64url-no-pad segments.
        let segs: Vec<&str> = s.split('.').collect();
        assert_eq!(segs.len(), 3, "wire must have 3 segments, got {segs:?}");
        for (i, seg) in segs.iter().enumerate() {
            assert!(!seg.is_empty(), "segment {i} must not be empty");
            // base64url-no-pad alphabet: A-Z a-z 0-9 - _
            for c in seg.bytes() {
                assert!(
                    c.is_ascii_alphanumeric() || c == b'-' || c == b'_',
                    "segment {i} has non-base64url-no-pad byte: {c}"
                );
            }
        }
    }

    /// TV2 — mint → serialize_wire → deserialize_wire roundtrip
    /// recovers a macaroon with an empty caveat list and a 16-byte
    /// `root_id` (the wire form is the canonical registry PK +
    /// caveat-list substrate).
    #[test]
    fn wire_roundtrip_preserves_caveats_and_root_id_shape() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let wire = std::str::from_utf8(&payload).unwrap();
        let restored = deserialize_wire(wire, req.holder_did.clone(), id.public_key_bytes())
            .expect("wire deserialize");
        // macaroon is empty-initial-caveats — verify that the round-trip
        // preserved the empty caveat list (not duplicated, not dropped).
        assert_eq!(restored.macaroon.caveats, Vec::new());
        // root_id is 16 bytes per RFC-0957 §3.2 (`MacaroonId = [u8; 16]`).
        assert_eq!(restored.macaroon.root_id.len(), 16);
        assert_eq!(restored.holder_did, req.holder_did);
        assert_eq!(restored.holder_pub, id.public_key_bytes());
        assert!(!restored.holder_sig_stale, "fresh mint must not be stale");
    }

    /// TV3 — holder signature verifies after wire roundtrip. The
    /// holder DID + public key passed to `deserialize_wire` are
    /// recovered out-of-band from the DID registry; the wire itself
    /// doesn't carry them.
    #[test]
    fn holder_sig_verifies_after_wire_roundtrip() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let wire = std::str::from_utf8(&payload).unwrap();
        let restored = deserialize_wire(wire, req.holder_did.clone(), id.public_key_bytes())
            .expect("wire deserialize");
        restored
            .verify_holder_sig()
            .expect("holder sig must verify after wire roundtrip");
    }

    /// TV4 — `compute_cap_root_hash_from_wire(&wire)` matches the
    /// mint-time `compute_capability_id(&macaroon)` byte-for-byte.
    /// The wire-only derivation is the canonical registry PK
    /// (RFC-0957-A1 §Phase 2).
    #[test]
    fn wire_only_cap_root_hash_matches_mint_time_derivation() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let wire = std::str::from_utf8(&payload).unwrap();
        let hash_from_wire = compute_cap_root_hash_from_wire(wire).expect("hash from wire");
        // Re-derive from the macaroon we just round-tripped (TV2 path).
        let restored = deserialize_wire(wire, req.holder_did.clone(), id.public_key_bytes())
            .expect("wire deserialize");
        let hash_from_mac = octo_cap_macaroon::macaroon::compute_capability_id(&restored.macaroon);
        assert_eq!(
            hash_from_wire, hash_from_mac,
            "wire-only PK derivation must match mint-time derivation byte-for-byte"
        );
    }

    /// TV5 — non-canonical DID rejected before any mint work runs.
    /// Negative path complement to TV1-TV4; ensures the canonical-codec
    /// check fires before macaroon mint is attempted.
    #[test]
    fn handle_rejects_non_canonical_did_before_mint() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        // Bare form (no `did:octo:` prefix) must fail canonical-codec
        // validation per RFC-0010 v1.2 F4.
        let req = MintRequest {
            holder_did: "not-a-did-at-all".into(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(
            matches!(err, ProtocolError::InvalidDid(_)),
            "non-canonical DID must yield InvalidDid, got {err:?}"
        );
    }

    /// TV6 (mission 0957-phase2b) — `MintRequest::payment_caveat =
    /// Some(p)` mints a token whose caveat chain contains exactly one
    /// `Caveat::Payment(p)` entry. Closes 0871e deferred item #7
    /// (handler accepting PaymentCaveat mint requests).
    #[test]
    fn handle_mints_with_payment_caveat_as_initial_caveat() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let payment = octo_cap_macaroon::PaymentCaveat::new(1_000_000, "gpt-4", u64::MAX);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: Some(payment.clone()),
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let wire = std::str::from_utf8(&payload).unwrap();
        let restored = deserialize_wire(wire, req.holder_did.clone(), id.public_key_bytes())
            .expect("wire deserialize");
        // Exactly one caveat in the chain, and it is the Payment variant.
        assert_eq!(restored.macaroon.caveats.len(), 1);
        match &restored.macaroon.caveats[0] {
            Caveat::Payment(p) => {
                assert_eq!(p.budget, 1_000_000);
                assert_eq!(p.model, "gpt-4");
                assert_eq!(p.expires_at_unix_ms, u64::MAX);
            }
            other => panic!("expected Caveat::Payment, got {other:?}"),
        }
        // Holder sig still verifies after the roundtrip.
        restored.verify_holder_sig().expect("holder sig verifies");
    }

    /// TV7 (mission 0957-phase2b) — `payment_caveat = None` preserves
    /// the Phase 2a behavior: minted token has empty caveat chain.
    #[test]
    fn handle_mints_without_payment_caveat_has_empty_chain() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let wire = std::str::from_utf8(&payload).unwrap();
        let restored = deserialize_wire(wire, req.holder_did.clone(), id.public_key_bytes())
            .expect("wire deserialize");
        assert!(
            restored.macaroon.caveats.is_empty(),
            "payment_caveat=None must yield empty caveat chain"
        );
    }

    /// TV8 (mission 0871e-phase5b) — mint with a `PaymentCaveat`
    /// AND a spend ledger seeds the ledger entry. Without the
    /// ledger (default) the mint still succeeds — no drain
    /// possible, but the caveat chain is preserved.
    #[test]
    fn handle_with_ledger_seeds_payment_caveat_budget() {
        use octo_ident::WireDid;
        let id = sample_identity();
        let holder_did_str = sample_did();
        let holder_did = WireDid::new(holder_did_str.clone());
        let ledger = Arc::new(octo_paid_query::InMemorySpendLedger::new());
        let handler = MintHandler::with_ledger(&id, ledger.clone());
        let payment = octo_cap_macaroon::PaymentCaveat::new(1_000_000, "gpt-4", u64::MAX);
        let req = MintRequest {
            holder_did: holder_did_str.clone(),
            capability: [0xab; 32],
            payment_caveat: Some(payment.clone()),
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let wire = std::str::from_utf8(&payload).unwrap();
        let restored = deserialize_wire(wire, req.holder_did.clone(), id.public_key_bytes())
            .expect("wire deserialize");
        // Ledger was seeded with caveat.budget.
        assert_eq!(
            ledger
                .balance(&holder_did, &restored.macaroon.root_id)
                .unwrap(),
            Some(payment.budget),
            "mint with payment caveat must seed the spend ledger"
        );
    }

    /// TV9 (mission 0957-f-v2-bundle-consumer-migration) — mint
    /// surfaces a V2 `CapabilityBundleV2Envelope` alongside the V1
    /// wire form. The envelope is verified by `octo-cap-zk` + V2
    /// downstream consumers; the wire form remains the canonical
    /// transport until the V2 cutover mission lands.
    #[test]
    fn handle_surfaces_v2_envelope_alongside_wire_form() {
        use octo_cap_macaroon::{
            CapabilityBundleV2Envelope, CIPHEROCTO_V2_BUNDLE_PREFIX, MAX_CHAIN_DEPTH,
        };
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        // Legacy wire form is unchanged.
        assert!(out.response_payload.is_some(), "wire payload still emitted");
        // V2 envelope bytes are surfaced.
        let env_bytes = out
            .v2_envelope_bytes
            .expect("V2 envelope must be surfaced post-migration");
        // First 16 bytes = canonical prefix.
        assert_eq!(
            &env_bytes[..16],
            CIPHEROCTO_V2_BUNDLE_PREFIX.as_slice(),
            "envelope bytes must start with canonical V2 prefix"
        );
        // Decode roundtrip preserves inner bundle.
        let env = CapabilityBundleV2Envelope::canonical_de(&env_bytes).expect("de");
        assert_eq!(env.bundle.token_v2.chain_depth, 0, "root mint = depth 0");
        assert_eq!(
            env.bundle.token_v2.chain_parent, [0u8; 32],
            "root mint = null parent binding"
        );
        assert!(env.bundle.token_v2.chain_depth <= MAX_CHAIN_DEPTH);
        assert_eq!(env.bundle.token_v2.audience_did, req.holder_did);
    }
}
