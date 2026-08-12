//! Per-hop signature for cross-domain resolver chains
//! (mission `0871b-cross-node-forwarding`, RFC-0871 §Future Work +
//! RFC-0970 forwarding-hop pattern reference).
//!
//! Each hop in a cross-domain resolver chain signs a preimage that binds
//! TWO orthogonal integrity dimensions:
//!
//! - **Hop binding** (RFC-0970 §Data Structures `chain_hash`) — per-hop
//!   accumulator binding forwarded request + cumulative hop state.
//! - **Request correlation** (RFC-0871 §Algorithms step 2
//!   `envelope_id`) — BLAKE3-256 of the unsigned envelope. The
//!   `ChainResolveResponse.envelope_id` field binds the response to
//!   the originating request envelope for replay defense.
//!
//! The `HopSignature.signature` preimage binds both into one Ed25519
//! signature per hop:
//!
//! ```text
//! preimage = BLAKE3-256(
//!     canonical_ser((chain_hash, hop_index, BLAKE3(inner_payload), envelope_id))
//! )
//! ```
//!
//! The verification side recomputes the same preimage + verifies
//! `signature` against `signer_pub` (no registry lookup needed; the
//! public key travels in-band).

use borsh::{BorshDeserialize, BorshSerialize};

/// Per-hop Ed25519 signature binding a single hop in a cross-domain
/// resolver chain.
///
/// Wire form: borsh `(hop_index, hop_did, signature, signer_pub)`.
///
/// One `HopSignature` is produced by each intermediate hop in
/// `IDENTITY_RESOLVE_CHAIN`. The requester accumulates them in
/// `ChainResolveResponse.signature_chain` (outermost-first) and
/// verifies them locally against the embedded `signer_pub`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct HopSignature {
    /// 0-indexed position in the chain. `0` = the requester's wrapping
    /// node (the originator); `len(hops)` = the terminal destination.
    pub hop_index: u8,
    /// Canonical DID wire form (`did:octo:z<base58btc>`) of the wrapping
    /// node that produced this signature.
    pub hop_did: String,
    /// Ed25519 signature over
    /// `BLAKE3-256(canonical_ser((chain_hash, hop_index, BLAKE3(inner_payload), envelope_id)))`.
    pub signature: [u8; 64],
    /// 32-byte Ed25519 public key for verification (in-band; no
    /// registry lookup needed by the receiver).
    pub signer_pub: [u8; 32],
}

impl HopSignature {
    /// Build a new `HopSignature` with the given fields.
    #[must_use]
    pub const fn new(
        hop_index: u8,
        hop_did: String,
        signature: [u8; 64],
        signer_pub: [u8; 32],
    ) -> Self {
        Self {
            hop_index,
            hop_did,
            signature,
            signer_pub,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_signature_borsh_round_trip() {
        let sig = HopSignature::new(
            1,
            "did:octo:zCt5bENb7tA2b9xeamSEnHF7cZ6Kk8h9p2Z6nT8pVk9R".to_owned(),
            [0xAA; 64],
            [0xBB; 32],
        );
        let bytes = borsh::to_vec(&sig).expect("serialize");
        let back: HopSignature = borsh::from_slice(&bytes).expect("deserialize");
        assert_eq!(back, sig);
    }

    #[test]
    fn hop_signature_zero_values_borsh_round_trip() {
        // Boundary: empty/zero signature + zero pubkey survives borsh
        // round-trip (defensive against accidental Option/skip attributes).
        //
        // STRUCTURAL-ONLY: `hop_did = String::new()` is NOT a valid
        // canonical DID; if `HopSignature::new` ever gains a
        // `CanonicalCodec::parse` validation pass this test will need
        // to switch to a canonical form (e.g. `canonical_did(0)`).
        let sig = HopSignature::new(0, String::new(), [0u8; 64], [0u8; 32]);
        let bytes = borsh::to_vec(&sig).expect("serialize");
        let back: HopSignature = borsh::from_slice(&bytes).expect("deserialize");
        assert_eq!(back, sig);
    }

    /// AC-11 (mission `0870k-transport-request-response`): the `HopSignature`
    /// wire form binds an Ed25519 signature over the preimage
    /// `BLAKE3-256(canonical_ser((chain_hash, hop_index, BLAKE3(inner_payload), envelope_id)))`.
    ///
    /// Sign with `ed25519_dalek::SigningKey`; verify with
    /// `verify_ed25519_signature` (which decodes the canonical DID into
    /// the verifying key). The round-trip confirms the 5-tuple preimage
    /// + `signature` + `signer_pub` binding holds end-to-end.
    #[test]
    fn hop_signature_signs_and_verifies() {
        use crate::authorization::{verify_ed25519_signature, Ed25519SignatureBytes};
        use ed25519_dalek::{Signer, SigningKey};

        // 1. Generate a deterministic Ed25519 keypair from a 32-byte seed.
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        let sk = SigningKey::from_bytes(&seed);
        let pk_bytes = sk.verifying_key().to_bytes();
        let hop_did = format!("did:octo:z{}", bs58::encode(&pk_bytes).into_string());
        let hop_did = crate::WireDid::new(hop_did);

        // 2. Build the preimage per RFC-0871 §Algorithms step 4 +
        //    RFC-0970 forwarding-hop pattern:
        //    `BLAKE3-256(canonical_ser((chain_hash, hop_index, BLAKE3(inner_payload), envelope_id)))`
        let chain_hash = [0xABu8; 32];
        let hop_index: u8 = 1;
        let inner_payload = b"IDENTITY_RESOLVE_CHAIN payload bytes";
        let inner_hash = blake3::hash(inner_payload);
        let envelope_id = [0x42u8; 32];
        let preimage_struct = (chain_hash, hop_index, *inner_hash.as_bytes(), envelope_id);
        let preimage_bytes = borsh::to_vec(&preimage_struct).expect("canonical_ser preimage");
        let preimage_hash = blake3::hash(&preimage_bytes);

        // 3. Sign the BLAKE3-hashed preimage with Ed25519.
        let sig = Ed25519SignatureBytes::from_signature(&sk.sign(preimage_hash.as_bytes()));
        let hop_sig = HopSignature::new(hop_index, hop_did.as_str().to_owned(), sig.0, pk_bytes);

        // 4. Verify the signature via the public `verify_ed25519_signature`
        //    helper — same code path the `EnvelopeDispatcher` uses for
        //    `Authorization::Signature` verification. This pins the
        //    end-to-end sign/verify cycle over the 5-tuple preimage.
        verify_ed25519_signature(&hop_did, preimage_hash.as_bytes(), &sig)
            .expect("hop signature MUST verify against the 5-tuple preimage");
        // Re-decode the `HopSignature` wire form to confirm the
        // borsh layout is self-consistent end-to-end.
        let hop_sig_bytes = borsh::to_vec(&hop_sig).expect("HopSignature borsh");
        let _back: HopSignature = borsh::from_slice(&hop_sig_bytes).expect("HopSignature decode");
    }
}
