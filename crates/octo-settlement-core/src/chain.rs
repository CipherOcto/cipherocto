//! Receipt chain-integrity helpers per RFC-0014 §Design Goals G5.
//!
//! `verify_receipt_chain` validates a sequence of `Receipt`s for:
//! - Strict `receipt_id` monotonicity (gap detection)
//! - `settlement_hash` chain matches `BLAKE3-256` keyed with the
//!   canonical domain separator.
//!
//! `receipt_id_for` produces the canonical receipt ID for a given
//! (ask_id, prev_receipt_hash) pair.
//!
//! `receipt_id_for_digest` (RFC-0014-v2 + RFC-0016-a §6.4) provides
//! the canonical reverse-mapping from a settlement-hash digest to
//! its `ReceiptId`. The DOMAIN adapter (Layer B sink) registers the
//! concrete reverse-mapping callback at startup; the substrate
//! signature is frozen and the default returns `None` (forward-defer
//! per RFC-0014-v2 §FW2 paired-deferral contract).

use crate::error::SettlementError;
use crate::receipt::{Receipt, ReceiptId};

/// Canonical domain separator for the settlement receipt chain.
///
/// Per RFC-0959 + RFC-0014 §Test Vectors, the byte-pinned domain
/// separator is:
/// ```
/// const SEP: &[u8] = b"cipherocto/reservation/v1/";
/// ```
/// Any drift breaks cross-replica consensus; the constant is
/// exported for tests + cross-replica checksum verification.
pub const CHAIN_DOMAIN_SEPARATOR: &[u8] = b"cipherocto/reservation/v1/";

/// Compute the canonical receipt ID for `receipt` as
/// `BLAKE3-256(domain_separator || canonical_receipt_bytes)`.
///
/// `settlement_hash` is the OUTPUT of this function; it is NOT
/// included in the input (including it would make the hash depend on
/// itself, breaking determinism).
///
/// # Keyed-hash key
///
/// The BLAKE3 key is the zero-filled 32-byte constant `[0; 32]`. This
/// is INTENTIONAL — the substrate is Layer A frozen (RFC-0014
/// §Module Layout) and cross-replica consensus requires the receipt
/// hash to be deterministic for any replica that holds the same
/// `Receipt` inputs (no per-deployment key material). The
/// `domain_separator::mission_01_domain_separator_byte_pin`
/// test (RFC-0014 §Module Layout §Chain Helpers invariant; single
/// canonical substrate test surface) byte-pins this behavior.
///
/// Production deployments that need keyed-hash defense-in-depth
/// SHOULD wrap this function (e.g., via a `KeyedHasher` trait on
/// Layer C) and inject real key material from an HSM / Vault /
/// secrets manager at startup. The frozen zero key is a load-bearing
/// consensus invariant, not a permanent security posture.
pub fn receipt_id_for(receipt: &Receipt) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&[0; 32]);
    hasher.update(CHAIN_DOMAIN_SEPARATOR);
    hasher.update(&receipt.receipt_id.to_be_bytes());
    hasher.update(&receipt.ask_id);
    hasher.update(receipt.router_id.as_bytes());
    hasher.update(&receipt.router_sig);
    hasher.update(&receipt.timestamp_unix.to_be_bytes());
    *hasher.finalize().as_bytes()
}

/// Reverse-mapping from a canonical `settlement_hash` digest to its
/// `ReceiptId` (RFC-0016-a §6.4 + RFC-0014-v2 §Extension).
///
/// Substrate-level signature: returns `Option<ReceiptId>` (None when
/// the digest is unmapped). The DOMAIN adapter (Layer B sink) is
/// responsible for registering a real reverse-mapping callback that
/// walks the persisted receipt index; the substrate stub returns
/// `None` until a DOMAIN adapter registers. This pattern preserves
/// the Layer A frozen signature contract (no DOMAIN deps in substrate)
/// while giving the CLI a typed `From<[u8; 32]> for ReceiptId`
/// conversion that surfaces `<REDACTED>` + `<ReceiptNotFound>` per
/// the canonical `[ADD]` error envelope.
pub fn receipt_id_for_digest(digest: &[u8; 32]) -> Option<ReceiptId> {
    // Substrate-side stub: the DOMAIN adapter (e.g. Stoolap-backed
    // `quota-router-sm-engine::StoolapReceiptSink`) registers a real
    // reverse-mapping callback at startup; this stub returns `None`
    // until the registration lands. The signature is frozen so the
    // CLI conversion compiles today; the runtime behavior is documented
    // as `None → ReceiptNotFound` per RFC-0016-a §6.4 contract.
    let _ = digest;
    None
}

/// Verify a chain of receipts. Returns `Ok(())` on a valid chain;
/// returns `Err(SettlementError)` on the first detected violation.
///
/// **This is an integrity check, not an authenticity check.** It proves the
/// chain is internally consistent — monotonic ids and self-consistent hashes.
/// It does **not** prove a router signed anything: the keyed hash uses the
/// public zero key, so anyone can compute a valid `settlement_hash`, and
/// `router_sig` is hashed into the receipt but never verified here.
///
/// That division is deliberate and specified: RFC-0014 §Threats assigns
/// router-signature verification to the DOMAIN layer ("the substrate does NOT
/// verify signature — that's the domain's job"), and RFC-0014-v2 pins this
/// function's signature and behaviour. So the frozen signature stays as it is.
/// The missing capability is available additively as
/// [`verify_receipt_chain_authenticated`]; a caller that needs authenticity
/// must use that.
///
/// # Errors
///
/// - `SequenceGap` — `receipt_id` is not the successor of the previous
///   receipt's `receipt_id`.
/// - `ChainIntegrity` — a receipt's `settlement_hash` field does not
///   match `receipt_id_for(receipt)`.
pub fn verify_receipt_chain(receipts: &[Receipt]) -> Result<(), SettlementError> {
    let mut last_receipt_id: Option<u64> = None;
    for receipt in receipts {
        if let Some(prev) = last_receipt_id {
            if receipt.receipt_id != prev + 1 {
                return Err(SettlementError::SequenceGap {
                    receipt_id: receipt.receipt_id,
                    prev,
                });
            }
        }
        let expected_settlement_hash = receipt_id_for(receipt);
        if receipt.settlement_hash != expected_settlement_hash {
            return Err(SettlementError::ChainIntegrity {
                receipt_id: receipt.receipt_id,
            });
        }
        last_receipt_id = Some(receipt.receipt_id);
    }
    Ok(())
}

/// Canonical domain separator for the router settlement signature.
pub const ROUTER_SIG_DOMAIN_SEPARATOR: &[u8] = b"cipherocto/settlement/router-sig/v1";

/// Canonical byte preimage a router signs when issuing a settlement receipt.
///
/// ```text
/// ROUTER_SIG_DOMAIN_SEPARATOR
///     || receipt_id.to_be_bytes()   // 8 bytes, u64 BE
///     || ask_id                     // 32 bytes
///     || router_id                  // UTF-8, length-unambiguous (see below)
///     || timestamp_unix.to_be_bytes() // 8 bytes
/// ```
///
/// `settlement_hash` and `router_sig` are deliberately **absent**: including
/// the hash would make the signature depend on a value derived from the
/// signature, and including the signature itself is circular.
///
/// `router_id` is a variable-length UTF-8 string, so it is length-prefixed with
/// a 4-byte BE length before the bytes. Without that prefix, `("ab", "c")` and
/// `("a", "bc")` would produce the same preimage — two different routers could
/// reuse one another's signature. The length prefix is part of the byte-pinned
/// contract and changing it changes every signature.
pub fn router_signature_preimage(receipt: &Receipt) -> Vec<u8> {
    let router_id = receipt.router_id.as_bytes();
    let mut buf: Vec<u8> =
        Vec::with_capacity(ROUTER_SIG_DOMAIN_SEPARATOR.len() + 8 + 32 + 4 + router_id.len() + 8);
    buf.extend_from_slice(ROUTER_SIG_DOMAIN_SEPARATOR);
    buf.extend_from_slice(&receipt.receipt_id.to_be_bytes());
    buf.extend_from_slice(&receipt.ask_id);
    buf.extend_from_slice(&(router_id.len() as u32).to_be_bytes());
    buf.extend_from_slice(router_id);
    buf.extend_from_slice(&receipt.timestamp_unix.to_be_bytes());
    buf
}

/// BLAKE3 of [`router_signature_preimage`] — the 32 bytes the router's ed25519
/// signature covers.
pub fn router_signature_digest(receipt: &Receipt) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(blake3::hash(&router_signature_preimage(receipt)).as_bytes());
    out
}

/// Resolves the public key authorised to sign receipts for a router id.
///
/// The registry itself is domain-owned (Layer C); the substrate only needs the
/// lookup, so the dependency points at this trait and the domain implements it
/// (dependency inversion — the side that owns the data owns the impl).
pub trait RouterKeyResolver {
    /// The authorised signing key for `router_id`, or `None` if that router is
    /// unknown or has no active key.
    ///
    /// **A slice, not `[u8; 32]`.** This was a fixed 32-byte array while
    /// `router_algorithm_id` sat beside it and was documented as the way to
    /// name a router's algorithm. Those two could not both be true: a resolver
    /// could override `router_algorithm_id` to an algorithm whose key is not 32
    /// bytes — stark-schnorr's is 33 — and would then have no way to return the
    /// key that algorithm needs. The dispatch argument was load-bearing in name
    /// only.
    ///
    /// Borrowed rather than owned because this is called once per receipt in a
    /// chain walk, and a resolver should not allocate to answer a lookup.
    fn router_key(&self, router_id: &str) -> Option<&[u8]>;

    /// The algorithm `router_key` speaks.
    ///
    /// Beside the key rather than assumed by the verifier, for the same
    /// reason RFC-0009 BC#13 puts `algorithm_id` on the verification method:
    /// a verifier that supplies its own algorithm checks the signature
    /// under a key that may not be the one that signed it, and reports the
    /// result as authoritative (AC#11).
    ///
    /// Defaults to Ed25519 so an existing resolver keeps compiling and
    /// keeps its current meaning. Override it the moment a router speaks
    /// anything else — the default is the *documented historical*
    /// algorithm, not a fallback that silently rescues a mismatch.
    fn router_algorithm_id(&self, _router_id: &str) -> octo_ident::AlgorithmId {
        octo_algo_ed25519::ed25519_algorithm_id()
    }
}

/// Verify a settlement receipt chain **and** each router's signature.
///
/// This is the authenticity half that [`verify_receipt_chain`] deliberately
/// does not provide. It runs the frozen integrity check first, then requires,
/// per receipt, that:
///
/// 1. `router_id` resolves to an authorised key via the injected resolver, and
/// 2. `router_sig` is a valid ed25519 signature by that key over
///    [`router_signature_digest`].
///
/// An unknown router, a malformed key, a wrong-length signature, and a genuine
/// signature mismatch all return [`SettlementError::RouterSignatureInvalid`];
/// the error does not distinguish them, so a caller cannot probe which keys are
/// registered.
///
/// An empty chain returns `Ok(())` — there is nothing to authenticate, matching
/// [`verify_receipt_chain`].
///
/// # Errors
///
/// Everything [`verify_receipt_chain`] returns, plus
/// `RouterSignatureInvalid`.
pub fn verify_receipt_chain_authenticated(
    receipts: &[Receipt],
    keys: &dyn RouterKeyResolver,
) -> Result<(), SettlementError> {
    verify_receipt_chain(receipts)?;
    for receipt in receipts {
        let key =
            keys.router_key(&receipt.router_id)
                .ok_or(SettlementError::RouterSignatureInvalid {
                    receipt_id: receipt.receipt_id,
                })?;
        let digest = router_signature_digest(receipt);
        // Dispatch on the algorithm the resolver reports for this router
        // (RFC-0009 AC#11). NO 64-byte pre-filter: an earlier version
        // coerced the signature to a fixed array, which would refuse every
        // algorithm whose signatures are not 64 bytes and report it as an
        // invalid router signature — indistinguishable from a genuinely bad
        // one. The registered algorithm reports its own length error.
        octo_algo_ed25519::verify_dispatch(
            &keys.router_algorithm_id(&receipt.router_id),
            key,
            &digest,
            &receipt.router_sig,
        )
        .map_err(|_| SettlementError::RouterSignatureInvalid {
            receipt_id: receipt.receipt_id,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_receipt(id: u64, ask: [u8; 32], ts: u64) -> Receipt {
        let mut r = Receipt {
            receipt_id: id,
            ask_id: ask,
            settlement_hash: [0; 32],
            router_id: "did:oct:router".to_owned(),
            router_sig: vec![0xaa, 0xbb],
            timestamp_unix: ts,
            ..Default::default()
        };
        r.settlement_hash = receipt_id_for(&r);
        r
    }

    #[test]
    fn chain_empty_is_valid() {
        assert!(verify_receipt_chain(&[]).is_ok());
    }

    #[test]
    fn chain_single_accepts() {
        let r = make_receipt(0, [0x01; 32], 1000);
        assert!(verify_receipt_chain(std::slice::from_ref(&r)).is_ok());
    }

    #[test]
    fn chain_monotonic_5() {
        let mut receipts = Vec::new();
        for i in 0..5 {
            receipts.push(make_receipt(i, [0x01; 32], 1000 + i * 100));
        }
        assert!(verify_receipt_chain(&receipts).is_ok());
    }

    #[test]
    fn chain_gap_rejects() {
        let r0 = make_receipt(0, [0x01; 32], 1000);
        let r2 = make_receipt(2, [0x01; 32], 1200);
        assert!(matches!(
            verify_receipt_chain(&[r0, r2]),
            Err(SettlementError::SequenceGap { .. })
        ));
    }

    #[test]
    fn chain_integrity_rejects() {
        let r0 = make_receipt(0, [0x01; 32], 1000);
        let mut r1 = make_receipt(1, [0x01; 32], 1100);
        r1.settlement_hash[0] ^= 1;
        assert!(matches!(
            verify_receipt_chain(&[r0, r1]),
            Err(SettlementError::ChainIntegrity { .. })
        ));
    }

    // Byte-pin test lives at `domain_separator::mission_01_…` (single
    // canonical substrate surface per substrate-faithful no-duplication
    // policy). Do NOT add an inline duplicate here.

    // -- C-5: router signature verification (the authenticity half) --

    use ed25519_dalek::{Signer as _, SigningKey};

    /// Single-router key registry, standing in for the domain-owned
    /// `RouterKeyResolver` implementation.
    struct OneRouter {
        router_id: String,
        // Slice-shaped owner rather than a fixed array: the vector below
        // stores a 33-byte stark-schnorr key in here, and a 32-byte field
        // would make that test impossible to write rather than merely failing.
        key: Vec<u8>,
    }

    impl RouterKeyResolver for OneRouter {
        fn router_key(&self, router_id: &str) -> Option<&[u8]> {
            (router_id == self.router_id).then_some(self.key.as_slice())
        }
    }

    fn signed_receipt(
        sk: &SigningKey,
        id: u64,
        ask: [u8; 32],
        ts: u64,
        router_id: &str,
    ) -> Receipt {
        let mut r = Receipt {
            receipt_id: id,
            ask_id: ask,
            settlement_hash: [0; 32],
            router_id: router_id.to_owned(),
            router_sig: vec![0u8; 64],
            timestamp_unix: ts,
            ..Default::default()
        };
        r.router_sig = sk.sign(&router_signature_digest(&r)).to_bytes().to_vec();
        r.settlement_hash = receipt_id_for(&r);
        r
    }

    /// A resolver that reports a different algorithm for the same key.
    struct LyingRouter {
        inner: OneRouter,
        algorithm: octo_ident::AlgorithmId,
    }

    impl RouterKeyResolver for LyingRouter {
        fn router_key(&self, router_id: &str) -> Option<&[u8]> {
            self.inner.router_key(router_id)
        }

        fn router_algorithm_id(&self, _router_id: &str) -> octo_ident::AlgorithmId {
            self.algorithm
        }
    }

    #[test]
    fn a_router_reporting_an_unregistered_algorithm_is_refused() {
        // The point of moving the algorithm onto the resolver. Before this,
        // `verify_receipt_chain_authenticated` built an Ed25519 key
        // unconditionally, so a resolver reporting anything else was checked
        // as though it were Ed25519 and the verdict was reported as
        // authoritative.
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let keys = LyingRouter {
            inner: OneRouter {
                router_id: "did:oct:router".into(),
                key: sk.verifying_key().to_bytes().to_vec(),
            },
            algorithm: octo_ident::AlgorithmId::derive("not-a-registered-algorithm"),
        };
        let chain = vec![signed_receipt(&sk, 0, [0x01; 32], 1000, "did:oct:router")];
        assert!(
            verify_receipt_chain_authenticated(&chain, &keys).is_err(),
            "a receipt must not verify under an algorithm the deployment \
             does not implement"
        );
    }

    #[test]
    fn a_genuine_signature_under_the_reported_algorithm_still_verifies() {
        // The control on the refusal above: refusing everything would also
        // pass it. The default resolver reports Ed25519 and a real Ed25519
        // signature must still verify.
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let keys = OneRouter {
            router_id: "did:oct:router".into(),
            key: sk.verifying_key().to_bytes().to_vec(),
        };
        assert_eq!(
            keys.router_algorithm_id("did:oct:router"),
            octo_algo_ed25519::ed25519_algorithm_id()
        );
        let chain = vec![signed_receipt(&sk, 0, [0x01; 32], 1000, "did:oct:router")];
        assert!(
            verify_receipt_chain_authenticated(&chain, &keys).is_ok(),
            "a genuine signature must not be refused by the dispatch path"
        );
    }

    /// A resolver whose router speaks stark-schnorr, end to end.
    ///
    /// Neither existing vector can show that a second algorithm actually
    /// verifies. The refusal one is satisfied by refusing everything, and the
    /// control one only ever registers Ed25519 — so an implementation that
    /// supported Ed25519 and rejected every other algorithm would pass both.
    ///
    /// They do catch the opposite failure, and that is worth crediting rather
    /// than overstating: hardcoding Ed25519 and ignoring `router_algorithm_id`
    /// turns the refusal test red, because a genuine Ed25519 receipt under a
    /// resolver claiming an unregistered algorithm then verifies when it
    /// should not. Between them the pair catches "assumes Ed25519" and
    /// "refuses too much". Neither catches "supports only Ed25519".
    ///
    /// A stark-schnorr key is 33 bytes and its signature covers a digest built
    /// from those same 33 bytes, so neither the key nor the signed message can
    /// be quietly reinterpreted as Ed25519.
    ///
    /// **This vector could not have existed before the key was widened.** While
    /// `router_key` returned `[u8; 32]`, a resolver could override
    /// `router_algorithm_id` — it was documented as the way to name a router's
    /// algorithm — and still had no way to hand back the key that algorithm
    /// needs. The override was unimplementable for its stated purpose.
    #[test]
    fn a_router_speaking_a_second_algorithm_verifies_end_to_end() {
        use octo_algo_stark_schnorr::{public_key, sign, Felt};

        octo_algo_ed25519::install().expect("install ed25519");
        octo_algo_stark_schnorr::install().expect("install stark-schnorr");

        let secret = Felt::from_hex_unchecked("0x77aa11bb22cc33dd44ee55ff66009911");
        let wire = public_key(&secret).expect("derives a public key").to_wire();
        assert_eq!(
            wire.len(),
            33,
            "a stark-schnorr key is 33 bytes — that is what the widened \
             `router_key` now makes it possible to return"
        );

        let router_id = "did:oct:router";
        let keys = LyingRouter {
            inner: OneRouter {
                router_id: router_id.into(),
                key: wire.to_vec(),
            },
            algorithm: octo_algo_stark_schnorr::stark_schnorr_algorithm_id(),
        };

        // Build the receipt the way the router would. Two ordering facts are
        // load-bearing and both are easy to get backwards:
        //
        //  - `settlement_hash` is `receipt_id_for`, which hashes `router_sig`.
        //    It is therefore computed AFTER the signature is in place.
        //  - `router_signature_digest` does not cover `settlement_hash`, so the
        //    digest is the same either side of that step.
        //
        // `signed_receipt` encodes the first fact already; the stark-schnorr
        // equivalent is written out here rather than reused, because that
        // helper signs with Ed25519.
        let mut receipt = Receipt {
            receipt_id: 0,
            ask_id: [0x01; 32],
            settlement_hash: [0; 32],
            router_id: router_id.to_owned(),
            router_sig: vec![0u8; 64],
            timestamp_unix: 1000,
            ..Default::default()
        };
        let signature = sign(
            &secret,
            &router_signature_digest(&receipt),
            &Felt::from_hex_unchecked("0x13579bdf02468ace"),
        )
        .expect("sign")
        .to_wire();
        receipt.router_sig = signature.to_vec();
        receipt.settlement_hash = receipt_id_for(&receipt);

        assert!(
            verify_receipt_chain_authenticated(&[receipt.clone()], &keys).is_ok(),
            "a genuine stark-schnorr receipt must not be refused by the \
             dispatched path"
        );

        // And the reported algorithm is load-bearing: the same receipt under
        // the default Ed25519 resolver is a different claim, and must fail.
        let ed = OneRouter {
            router_id: router_id.into(),
            key: wire.to_vec(),
        };
        assert!(
            verify_receipt_chain_authenticated(&[receipt], &ed).is_err(),
            "the same key and receipt must not verify when the resolver \
             reports ed25519"
        );
    }

    #[test]
    fn authenticated_chain_accepts_a_genuine_signature() {
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let keys = OneRouter {
            router_id: "did:oct:router".into(),
            key: sk.verifying_key().to_bytes().to_vec(),
        };
        let chain: Vec<Receipt> = (0..3)
            .map(|i| signed_receipt(&sk, i, [0x01; 32], 1000 + i * 100, "did:oct:router"))
            .collect();
        verify_receipt_chain_authenticated(&chain, &keys).unwrap();
    }

    #[test]
    fn authenticated_chain_rejects_forged_receipts() {
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let keys = OneRouter {
            router_id: "did:oct:router".into(),
            key: sk.verifying_key().to_bytes().to_vec(),
        };
        let good = signed_receipt(&sk, 0, [0x01; 32], 1000, "did:oct:router");

        // 1. The zero signature that the integrity-only path accepts.
        let mut zeroed = good.clone();
        zeroed.router_sig = vec![0u8; 64];
        zeroed.settlement_hash = receipt_id_for(&zeroed);
        // This is the exact forgery C-5 describes: the chain is internally
        // consistent, and `verify_receipt_chain` accepts it.
        assert!(verify_receipt_chain(std::slice::from_ref(&zeroed)).is_ok());
        assert!(matches!(
            verify_receipt_chain_authenticated(&[zeroed], &keys),
            Err(SettlementError::RouterSignatureInvalid { .. })
        ));

        // 2. Empty and wrong-length signature blobs error, not panic.
        for bad in [vec![], vec![0u8; 63], vec![0u8; 65]] {
            let mut r = good.clone();
            r.router_sig = bad.clone();
            r.settlement_hash = receipt_id_for(&r);
            assert!(matches!(
                verify_receipt_chain_authenticated(&[r], &keys),
                Err(SettlementError::RouterSignatureInvalid { .. })
            ));
        }

        // 3. A receipt claiming a router that is not registered.
        let stranger = signed_receipt(&sk, 0, [0x01; 32], 1000, "did:oct:someone-else");
        assert!(matches!(
            verify_receipt_chain_authenticated(&[stranger], &keys),
            Err(SettlementError::RouterSignatureInvalid { .. })
        ));

        // 4. A valid signature rebound to a different ask_id — the signature
        //    covers ask_id, so this must not verify.
        let mut rebound = good.clone();
        rebound.ask_id = [0x02; 32];
        rebound.settlement_hash = receipt_id_for(&rebound);
        assert!(matches!(
            verify_receipt_chain_authenticated(&[rebound], &keys),
            Err(SettlementError::RouterSignatureInvalid { .. })
        ));

        // 5. Same fields, different router key: the signature does not verify
        //    under the other router's key.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let other_keys = OneRouter {
            router_id: "did:oct:router".into(),
            key: other.verifying_key().to_bytes().to_vec(),
        };
        assert!(matches!(
            verify_receipt_chain_authenticated(&[good], &other_keys),
            Err(SettlementError::RouterSignatureInvalid { .. })
        ));
    }

    #[test]
    fn authenticated_chain_still_reports_integrity_failures() {
        // The authenticated path must not lose the frozen integrity checks.
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let keys = OneRouter {
            router_id: "did:oct:router".into(),
            key: sk.verifying_key().to_bytes().to_vec(),
        };
        let r0 = signed_receipt(&sk, 0, [0x01; 32], 1000, "did:oct:router");
        let r2 = signed_receipt(&sk, 2, [0x01; 32], 1200, "did:oct:router");
        assert!(matches!(
            verify_receipt_chain_authenticated(&[r0, r2], &keys),
            Err(SettlementError::SequenceGap { .. })
        ));
    }

    #[test]
    fn authenticated_empty_chain_is_ok() {
        let keys = OneRouter {
            router_id: "did:oct:router".into(),
            key: SigningKey::from_bytes(&[3u8; 32])
                .verifying_key()
                .to_bytes()
                .to_vec(),
        };
        assert!(verify_receipt_chain_authenticated(&[], &keys).is_ok());
    }

    #[test]
    fn router_id_length_prefix_prevents_boundary_collisions() {
        // Without the 4-byte length prefix, ("ab","c") and ("a","bc") hash the
        // same bytes, so one router could replay another's signature.
        let mut a = Receipt {
            receipt_id: 1,
            ask_id: [0x01; 32],
            settlement_hash: [0; 32],
            router_id: "ab".into(),
            router_sig: vec![],
            timestamp_unix: 1000,
            ..Default::default()
        };
        let mut b = a.clone();
        a.router_id = "ab".into();
        a.ask_id = [0x01; 32];
        b.router_id = "a".into();
        b.ask_id = [0x01; 32];
        // Same ask_id/timestamp/receipt_id, different router split — the
        // preimages must still differ.
        let pa = router_signature_preimage(&a);
        let pb = router_signature_preimage(&b);
        assert_ne!(pa, pb, "router_id boundary collision");

        // And the documented layout: domain || id(8) || ask(32) || len(4) || id || ts(8)
        let d = ROUTER_SIG_DOMAIN_SEPARATOR;
        assert_eq!(&pa[..d.len()], d);
        let off = d.len();
        assert_eq!(&pa[off..off + 8], &1u64.to_be_bytes());
        assert_eq!(&pa[off + 8..off + 40], &[0x01; 32]);
        assert_eq!(&pa[off + 40..off + 44], &2u32.to_be_bytes());
        assert_eq!(&pa[off + 44..off + 46], b"ab");
        assert_eq!(&pa[off + 46..off + 54], &1000u64.to_be_bytes());
        assert_eq!(pa.len(), off + 54);
    }
}
