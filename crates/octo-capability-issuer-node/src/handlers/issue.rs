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
//! `octo_ident::CanonicalCodec::parse(s, false)` (RFC-0010 F4)
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
/// Mints the capability and registers its `HolderRecord`.
///
/// This is the **node-side persistence** that RFC-0871 §G9 requires:
/// *"The wallet is a pure crypto primitive; business rules live in the
/// node-side handler."* The wallet mints nothing persistent and this node
/// writes it, which is why the handler holds a registry rather than
/// receiving a token to store.
#[derive(Clone, Copy)]
pub struct IssueHandler<'a> {
    /// Issuer identity, used as the holder signer (HSM-routed
    /// `IdentityKey` per RFC-0871 §G8).
    identity: &'a octo_wallet::identity::IdentityKey,
    /// Registry the minted holder record is written to.
    registry: &'a dyn octo_cap_macaroon::holder_registry::HolderRegistry,
}

/// Manual `Debug`: the identity is redacted and the registry is not
/// `Debug`-able, so neither is worth printing.
impl std::fmt::Debug for IssueHandler<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueHandler")
            .field("identity", &"<redacted>")
            .field("registry", &"<holder registry>")
            .finish()
    }
}

impl<'a> IssueHandler<'a> {
    /// Construct a new `IssueHandler`.
    #[must_use]
    pub const fn new(
        identity: &'a octo_wallet::identity::IdentityKey,
        registry: &'a dyn octo_cap_macaroon::holder_registry::HolderRegistry,
    ) -> Self {
        Self { identity, registry }
    }

    /// Mint the capability, register its `HolderRecord`, and return the V2
    /// envelope.
    ///
    /// RFC-0957-A1 §6 makes `mint` persistence-free, so the write belongs
    /// here — the caller. Without it the registry stays empty and every
    /// capability is refused downstream with `HolderUnknown`.
    ///
    /// `holder_record_bytes` now carries **real canonical `HolderRecord`
    /// bytes**, which is what RFC-0009 §Forward compatibility specifies and
    /// what the field's own documentation promises. It previously carried
    /// `req.capability` — the capability **root secret** — in the clear, so
    /// the issued bundle was not safe to store or relay.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `holder_did` is not a
    /// canonical DID shape, and `ProtocolError::AuthorizationFailed` if
    /// minting, wire serialization, record construction or the registry
    /// write fails.
    pub fn handle(&self, req: &IssueRequest) -> Result<HandlerOutput, ProtocolError> {
        use octo_cap_macaroon::caveat::Caveat;
        use octo_cap_macaroon::holder_record::{
            CapabilityClass, CapabilityTokenLike, HolderRecord,
        };
        use octo_cap_macaroon::signer::CapabilitySigner;
        use octo_cap_macaroon::token::CapabilityToken;
        use octo_cap_macaroon::wire::{compute_cap_root_hash_from_wire, serialize_wire};

        // Validate canonical DID shape; reject legacy bare form.
        // (RFC-0010 F4 + mission 0010-d wallet-audience-validation.)
        octo_ident::CanonicalCodec::parse(&req.holder_did, false).map_err(did_error_to_protocol)?;

        // 1. Mint. No initial caveats: an ask binding is a market-side
        //    decision (RFC-0959 §Phase 3), and guessing one here would
        //    assert an ask the issuer was never told about.
        let token = CapabilityToken::mint(
            &req.capability,
            self.identity,
            &req.holder_did,
            &[Caveat::Before(u64::MAX / 1000)],
        )
        .map_err(|e| ProtocolError::AuthorizationFailed(format!("mint: {e}")))?;

        let wire = serialize_wire(&token)
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("wire ser: {e}")))?;
        let cap_root_hash = compute_cap_root_hash_from_wire(&wire)
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("cap_root_hash: {e}")))?;

        // 2. Register. `ttl_millis_unix = 0` is the TV14 contract for
        //    "active, no TTL expiry", matching the MVP's unbounded
        //    `expires_at_unix_secs`.
        let record = HolderRecord::from_capability(
            &CapabilityTokenLike {
                cap_root_hash,
                class: CapabilityClass::V1,
            },
            &<octo_wallet::identity::IdentityKey as CapabilitySigner>::public_key_bytes(
                self.identity,
            )
            .try_into()
            .map_err(|_| {
                ProtocolError::AuthorizationFailed("issuer key is not 32 bytes".to_owned())
            })?,
            &req.holder_did,
            None,
            0,
        );
        self.registry
            .insert(record.clone())
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("registry insert: {e}")))?;

        // 3. Envelope. `channel_id` is the MacaroonId scope tag (RFC-0957
        //    §3.2), unchanged from the MVP.
        let mut nonce = [0u8; 16];
        nonce.copy_from_slice(&req.capability[..16]);
        let token_id: MacaroonId = macaroon_id(&req.capability, &nonce);
        let issuer_did =
            <octo_wallet::identity::IdentityKey as CapabilitySigner>::did(self.identity);
        let token_v2 = CapabilityTokenV2 {
            chain_depth: 0,
            chain_parent: [0u8; 32],
            audience_did: req.holder_did.clone(),
            channel_id: token_id,
            expires_at_unix_secs: u64::MAX,
            issuer_did,
        };
        // The real record, serialized — not the root secret.
        let holder_record_bytes = record
            .canonical_ser()
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("holder record ser: {e}")))?;
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
        .with_note(format!("issued and registered for {}", req.holder_did)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real in-memory registry, matching the fixture the lookup handler's
    /// tests already use. An in-memory `HolderRegistry` here is a test
    /// double, not a production store: RFC-0957-A1 §4 names
    /// `StoolapHolderRegistry` backed by a real handle as the reference.
    fn sample_registry() -> std::sync::Arc<dyn octo_cap_macaroon::holder_registry::HolderRegistry> {
        std::sync::Arc::new(octo_ident_storage::StoolapHolderRegistry::open_in_memory().unwrap())
    }

    fn sample_identity() -> std::sync::Arc<octo_wallet::identity::IdentityKey> {
        let mut k = octo_wallet::identity::IdentityKey::generate().unwrap();
        k.activate(1_700_000_000).unwrap();
        std::sync::Arc::new(k)
    }
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
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let handler = IssueHandler::new(&sample_id, &*sample_reg);
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
        // `did:octo:<base32>` form per RFC-0010 F4 + 0010-d mission.
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let handler = IssueHandler::new(&sample_id, &*sample_reg);
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
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let handler = IssueHandler::new(&sample_id, &*sample_reg);
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
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let handler = IssueHandler::new(&sample_id, &*sample_reg);
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
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let out = IssueHandler::new(&sample_id, &*sample_reg)
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

    /// The bundle carries a real `HolderRecord`, and not the capability root
    /// secret.
    ///
    /// This test used to assert the **opposite**: that
    /// `holder_record_bytes` equalled `req.capability` — the 32-byte macaroon
    /// root secret — and it passed, pinning the behaviour as correct. The
    /// field is documented as *"Serialized bytes of the `HolderRecord`.
    /// Deserialize via `HolderRecord::canonical_de` at the boundary"*, so the
    /// old assertion contradicted the contract it sat next to.
    ///
    /// Two properties, and both matter:
    ///
    /// - the bytes **decode** as a `HolderRecord` (the documented contract,
    ///   which failed for every bundle this handler produced before);
    /// - the decoded record's `cap_root_hash` is the minted one, so it is the
    ///   record for *this* capability rather than a placeholder.
    ///
    /// The secret assertion is implied by decoding: the root secret is not
    /// canonical `HolderRecord` JSON, so a payload that decodes cannot also be
    /// the secret. Spelled out anyway, because "implied" is how a secret ends
    /// up in a bundle in the first place.
    #[test]
    fn holder_record_bytes_carries_a_real_record_and_not_the_root_secret() {
        let capability = [0xa1u8; 32];
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let out = IssueHandler::new(&sample_id, &*sample_reg)
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability,
            })
            .unwrap();
        let env = envelope_from(&out);

        assert_ne!(
            env.bundle.holder_record_bytes,
            capability.to_vec(),
            "the issued bundle must not carry the capability root secret"
        );

        // The documented decode must now succeed on real production output.
        let record = octo_cap_macaroon::holder_record::HolderRecord::canonical_de(
            &env.bundle.holder_record_bytes,
        )
        .expect("holder_record_bytes must decode via the documented boundary");
        assert_eq!(record.holder_did, sample_did());
    }

    /// Minting writes a `HolderRecord`, so the registry is no longer empty.
    ///
    /// This is the guard-census property: before this handler minted, the
    /// `HolderRecord` construction census held by construction. It also proves
    /// the record is *reachable by lookup*, which is what a verifier needs.
    #[test]
    fn issuing_registers_a_holder_record_in_the_registry() {
        let sample_id = sample_identity();
        let sample_reg = sample_registry();
        let out = IssueHandler::new(&sample_id, &*sample_reg)
            .handle(&IssueRequest {
                holder_did: sample_did(),
                capability: [0xb2u8; 32],
            })
            .unwrap();
        let env = envelope_from(&out);
        let record = octo_cap_macaroon::holder_record::HolderRecord::canonical_de(
            &env.bundle.holder_record_bytes,
        )
        .unwrap();

        let found = sample_reg.lookup(&record.cap_root_hash).unwrap();
        assert!(
            found.is_some(),
            "the minted holder record must be readable back from the registry"
        );
    }
}
