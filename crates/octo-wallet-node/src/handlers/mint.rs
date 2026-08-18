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
/// Wire form: JSON via `mint_wire::request_to_bytes` / `request_from_bytes`.
/// Borsh derives intentionally OMITTED (mission 0862-c9 RETIRED):
/// `PaymentCaveat::budget` is `Dqa`, which does not impl
/// `BorshSerialize` / `BorshDeserialize` in the upstream git dep. The
/// eventual re-introduction of borsh awaits the follow-on mission
/// that ships `Borsh` impls for `Dqa` upstream.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    // Borsh methods intentionally OMITTED (mission 0862-c9 RETIRED,
    // follows from the dropped `BorshSerialize`/`BorshDeserialize`
    // derives — `PaymentCaveat::budget` is `Dqa` which doesn't impl
    // borsh). Callers needing the wire form use
    // `mint_wire::request_to_bytes` / `request_from_bytes`.
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
        // NOTE: NOT the response payload after mission
        // `0957-f-v2-bundle-cutover`. V2 envelope is the canonical wire
        // form; V1 wire form is internal substrate and can be re-derived
        // from `token.macaroon` via `serialize_wire` if needed.
        let minted_wire = serialize_wire(&token).map_err(wire_error_to_protocol)?;
        // Retain `minted_wire` for substrate reference (spend ledger
        // off-band `wire` key uses V1 for audit history; `octo-cap-zk`
        // envelope has the V2 form). The V2 envelope is the on-the-wire
        // payload; V1 stays local.
        let _ = minted_wire;

        // V2 envelope is the PRIMARY response payload (mission
        // `0957-f-v2-bundle-cutover`). The V1 macaroon wire form
        // (`minted_wire`) is still produced internally for substrate
        // use (capability_id derivation + spend ledger seeding) but
        // is NO LONGER the response payload. Downstream V2 consumers
        // decode via `CapabilityBundleV2Envelope::canonical_de` after
        // checking the 16-byte `CIPHEROCTO_V2_BUNDLE_PREFIX` prefix.
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
                "minted for {} (payment caveat budget={:?} seeded to spend ledger; v2 envelope {} bytes)",
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
            v2_envelope_bytes,
            octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
        )
        .with_note(note))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::{
        CapabilityBundleV2Envelope, CIPHEROCTO_V2_BUNDLE_PREFIX, MAX_CHAIN_DEPTH,
    };

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

    /// Helper: extract the V2 envelope from `out.response_payload` and
    /// decode it. Panics if the payload is missing or malformed.
    /// Mission `0957-f-v2-bundle-cutover`: `response_payload` IS the
    /// V2 envelope via `canonical_ser()` (16-byte prefix + borsh).
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
    fn mint_request_wire_round_trip() {
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let bytes = crate::handlers::mint_wire::request_to_bytes(&req).unwrap();
        let back = crate::handlers::mint_wire::request_from_bytes(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn mint_request_wire_round_trip_with_payment_caveat() {
        // Mission 0957-phase2b: the optional PaymentCaveat field must
        // roundtrip cleanly through the dispatch wire form (former
        // borsh path retired in 0862-c9 — `PaymentCaveat::budget` is
        // `Dqa` which does not impl borsh).
        let p = octo_cap_macaroon::PaymentCaveat::new(
            octo_determin::Dqa::new(1_000_000, 0).expect("scale=0 valid"),
            "gpt-4",
            u64::MAX,
        );
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: Some(p.clone()),
        };
        let bytes = crate::handlers::mint_wire::request_to_bytes(&req).unwrap();
        let back = crate::handlers::mint_wire::request_from_bytes(&bytes).unwrap();
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

    // ----- Mission 0957-f-v2-bundle-cutover — V2 envelope TV -----
    //
    // Post-cutover (mission `0957-f-v2-bundle-cutover`), the response
    // payload is the V2 `CapabilityBundleV2Envelope` canonical_ser
    // bytes. The macaroon is internal substrate (not on the wire).
    // Tests verify the envelope invariants (chain_depth, chain_parent,
    // audience_did, channel_id, holder_record_bytes, issuer_did) and
    // the side effects (spend ledger seeding on payment caveat).

    /// TV1 — `response_payload` is the V2 envelope canonical_ser
    /// bytes (16-byte prefix + borsh-encoded bundle). Pins the
    /// structural shape of the post-cutover wire form.
    #[test]
    fn handle_emits_v2_envelope_as_primary_payload() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        // Payload kind is unchanged.
        assert_eq!(
            out.response_payload_kind,
            Some(octo_protocol::payload_kind::WALLET_MINT_CAPABILITY)
        );
        // Envelope prefix matches canonical.
        assert_eq!(&env.prefix, CIPHEROCTO_V2_BUNDLE_PREFIX.as_slice());
    }

    /// TV2 — mint envelope carries root invariants: `chain_depth = 0`,
    /// `chain_parent = [0u8; 32]` (no parent binding for root mint).
    #[test]
    fn handle_v2_envelope_root_invariants() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        assert_eq!(env.bundle.token_v2.chain_depth, 0, "root mint = depth 0");
        assert_eq!(
            env.bundle.token_v2.chain_parent, [0u8; 32],
            "root mint = null parent binding"
        );
        assert!(env.bundle.token_v2.chain_depth <= MAX_CHAIN_DEPTH);
    }

    /// TV3 — envelope `audience_did` matches the request's `holder_did`;
    /// `issuer_did` is the canonical `did:octo:z<bs58>` form of the
    /// minting identity's public key.
    #[test]
    fn handle_v2_envelope_audience_and_issuer_dids() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        assert_eq!(env.bundle.token_v2.audience_did, req.holder_did);
        let expected_issuer_did = format!(
            "did:octo:z{}",
            bs58::encode(id.public_key_bytes()).into_string()
        );
        assert_eq!(env.bundle.token_v2.issuer_did, expected_issuer_did);
    }

    /// TV4 — envelope `channel_id` is 16 bytes (the macaroon root_id,
    /// 16-byte MacaroonId per RFC-0957 §3.2). It is non-zero for a
    /// fresh mint and varies across distinct mints (the macaroon
    /// substrate uses a random nonce per mint, so root_id is unique
    /// per mint per RFC-0957 §3.2).
    #[test]
    fn handle_v2_envelope_channel_id_is_16_bytes_and_unique_per_mint() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out_a = handler.handle(&req).unwrap();
        let out_b = handler.handle(&req).unwrap();
        let env_a = envelope_from(&out_a);
        let env_b = envelope_from(&out_b);
        // channel_id is 16-byte MacaroonId (non-zero for fresh mint).
        assert_ne!(env_a.bundle.token_v2.channel_id, [0u8; 16]);
        // Distinct mints carry distinct channel_ids (macaroon nonce
        // is random per RFC-0957 §3.2).
        assert_ne!(
            env_a.bundle.token_v2.channel_id, env_b.bundle.token_v2.channel_id,
            "distinct mints carry distinct macaroon root_ids"
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

    /// TV6 — `holder_record_bytes` carries the 32-byte capability_id
    /// (BLAKE3 keyed-hash of the macaroon) — the canonical registry PK
    /// per RFC-0957-A1 §Phase 2. V2 envelope substrate for the
    /// `CapRegistry::lookup_by_capability_id` flow.
    #[test]
    fn handle_v2_envelope_holder_record_bytes_is_capability_id() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        // 32-byte capability_id (BLAKE3 output).
        assert_eq!(
            env.bundle.holder_record_bytes.len(),
            32,
            "holder_record_bytes carries 32-byte capability_id"
        );
        // Not all zero for a fresh mint (BLAKE3 of non-empty input).
        assert_ne!(
            env.bundle.holder_record_bytes,
            vec![0u8; 32],
            "capability_id is not the zero hash for fresh mint"
        );
    }

    /// TV7 — `discharge_macaroon_bytes` is empty for a root mint (no
    /// upstream attenuator). V2 envelope carries a single discharge
    /// (singular vs V1's Vec) per RFC-0009 v1.2 §Compatibility.
    #[test]
    fn handle_v2_envelope_discharge_macaroon_bytes_empty_for_root() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        assert!(
            env.bundle.discharge_macaroon_bytes.is_empty(),
            "root mint carries no upstream discharge"
        );
    }

    /// TV8 (mission 0871e-phase5b update) — mint with a `PaymentCaveat`
    /// AND a spend ledger seeds the ledger entry keyed by the
    /// envelope's `channel_id` (macaroon root_id). The envelope
    /// itself is unchanged in shape; the seeding is a side effect
    /// that downstream `octo-paid-query` drains against.
    #[test]
    fn handle_with_ledger_seeds_payment_caveat_budget() {
        use octo_ident::WireDid;
        let id = sample_identity();
        let holder_did_str = sample_did();
        let holder_did = WireDid::new(holder_did_str.clone());
        let ledger = Arc::new(octo_paid_query::InMemorySpendLedger::new());
        let handler = MintHandler::with_ledger(&id, ledger.clone());
        let payment = octo_cap_macaroon::PaymentCaveat::new(
            octo_determin::Dqa::new(1_000_000, 0).expect("scale=0 valid"),
            "gpt-4",
            u64::MAX,
        );
        let req = MintRequest {
            holder_did: holder_did_str.clone(),
            capability: [0xab; 32],
            payment_caveat: Some(payment.clone()),
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        // Ledger was seeded with caveat.budget keyed by channel_id.
        assert_eq!(
            ledger
                .balance(&holder_did, &env.bundle.token_v2.channel_id)
                .unwrap(),
            Some(payment.budget),
            "mint with payment caveat must seed the spend ledger"
        );
    }

    /// TV9 — without a payment caveat, the spend ledger is NOT
    /// seeded. Mint succeeds (no drain possible, but the envelope is
    /// the canonical wire form).
    #[test]
    fn handle_without_payment_caveat_does_not_seed_ledger() {
        use octo_ident::WireDid;
        let id = sample_identity();
        let holder_did_str = sample_did();
        let holder_did = WireDid::new(holder_did_str.clone());
        let ledger = Arc::new(octo_paid_query::InMemorySpendLedger::new());
        let handler = MintHandler::with_ledger(&id, ledger.clone());
        let req = MintRequest {
            holder_did: holder_did_str.clone(),
            capability: [0xab; 32],
            payment_caveat: None,
        };
        let out = handler.handle(&req).unwrap();
        let env = envelope_from(&out);
        // No payment caveat → no ledger seed → balance is `None`.
        assert_eq!(
            ledger
                .balance(&holder_did, &env.bundle.token_v2.channel_id)
                .unwrap(),
            None,
            "mint without payment caveat must NOT seed the spend ledger"
        );
    }
}
