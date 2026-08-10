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

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::wire::serialize_wire;
use octo_ident::DidCodec;
use octo_protocol::ProtocolError;
use octo_wallet::capability::CapabilityToken;
use octo_wallet::identity::IdentityKey;

use super::{
    did_error_to_protocol, wallet_error_to_protocol, wire_error_to_protocol, HandlerOutput,
};

/// Request payload for `WALLET_MINT_CAPABILITY`.
///
/// Wire form: borsh (`holder_did`, `capability_root`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct MintRequest {
    /// Canonical DID of the holder (`did:octo:z<base58btc>`).
    pub holder_did: String,
    /// 32-byte capability root secret (random nonce per mint).
    pub capability: [u8; 32],
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
}

impl<'a> MintHandler<'a> {
    /// Construct a new `MintHandler` bound to the given identity key.
    #[must_use]
    pub const fn new(identity: &'a IdentityKey) -> Self {
        Self { identity }
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

        let token = CapabilityToken::mint(&req.capability, self.identity, &req.holder_did, &[])
            .map_err(wallet_error_to_protocol)?;

        // Real macaroon wire form (RFC-0957 §3.7): 3 base64url-no-pad
        // segments separated by `.`. Replaces the Phase 1 MVP
        // `CIPHEROCTO_MINT_V1:<holder_did>` placeholder.
        let minted_wire = serialize_wire(&token).map_err(wire_error_to_protocol)?;

        Ok(HandlerOutput::response(
            minted_wire.into_bytes(),
            octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
        )
        .with_note(format!("minted for {}", req.holder_did)))
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
        };
        let bytes = req.to_borsh().unwrap();
        let back = MintRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: "did:octo:bad".into(),
            capability: [0xab; 32],
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
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(
            matches!(err, ProtocolError::InvalidDid(_)),
            "non-canonical DID must yield InvalidDid, got {err:?}"
        );
    }
}
