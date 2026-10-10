//! The mandatory signable prefix — RFC-0009 §Domain separation is mandatory,
//! not hygiene.
//!
//! A signed message is `signable = algorithm_id ‖ key_id ‖ canonical_ser(payload)`.
//! Both identifiers are the **16 raw bytes** of their `Uuid`, never the
//! 36-character hyphenated form, and the payload is canonicalized with
//! RFC-0126 §Part 3: Deterministic Canonical Serialization.
//!
//! # Layer, and why the builder is not Layer A
//!
//! An earlier draft of this module's mission called the construction "Layer A
//! — pure canonical encoding, frozen". That is wrong, and the reason is the
//! parameter types. A builder taking `[u8; 16]` genuinely would be Layer A,
//! but it could not be type-checked against the discriminators: a caller
//! passing sixteen arbitrary bytes compiles, and the whole point of naming
//! `algorithm_id` is that it is a specific algorithm's identifier.
//!
//! So the builder takes [`AlgorithmId`] and [`VerificationMethodId`], which
//! puts it in **Layer B alongside them**, in this crate. It depends on
//! `octo-determin` (Layer A) for the payload canonicalizer, which is the
//! correct direction: B depends on A.
//!
//! # Which form this implementation uses: **raw**
//!
//! RFC-0009 offers two encodings and they are **not interoperable**:
//!
//! ```text
//! raw    = algorithm_id ‖ key_id ‖ canonical_ser(payload)
//! hashed = BLAKE3-256("cipherocto/sigdomain/v1" ‖ algorithm_id ‖ key_id)
//!          ‖ canonical_ser(payload)
//! ```
//!
//! The hashed form is preferred for new implementations because it removes
//! every framing question at once. This implementation ships the **raw** form
//! and says so, which RFC-0009 requires of any implementation that chooses:
//! an implementation states which it uses and a verifier MUST reject the
//! other.
//!
//! Shipping raw is a deliberate departure from that preference, taken because
//! the raw form is the only one that carries the algorithm identifier
//! **in band** — which is what lets a verifier "verify under the algorithm the
//! prefix names rather than the one it prefers", and what makes
//! `cross_algorithm_signature_replay_rejected` expressible at all. See
//! [`SignedMessage::parse`] for why the hashed form cannot offer that.
//!
//! The raw form's safety rests entirely on both components being fixed at 16
//! bytes, so the prefix is self-delimiting and the payload needs no external
//! framing. [`SignablePrefix`] makes that true by construction — it stores the
//! newtypes, which store `[u8; 16]` — rather than by convention. If
//! `VerificationMethodId` ever becomes variable-width, the raw form MUST move
//! to the hashed form in the same change that makes it variable.
//!
//! # What `canonical_ser` resolves to
//!
//! Not to any function of that name. There are five `canonical_ser` functions
//! in this repository and **none of them is the DCS canonicalizer**:
//!
//! - `zk_circuit::canonical_ser` — a hand-rolled encoding with its own
//!   domain byte, not RFC-0126 Part 3.
//! - `CapabilityBundleV2::canonical_ser` and
//!   `CapabilityBundleV2Envelope::canonical_ser` — borsh.
//! - `Caveat::canonical_ser` and `HolderRecord::canonical_ser` — JSON, which
//!   RFC-0126 §JSON Allowed Contexts **forbids** for cryptographic proofs.
//!
//! An implementer who reached for the bare symbol would be signing a domain
//! the serialization RFC prohibits. This module calls
//! `octo_determin::dcs::dcs_serialize_bytes` by path instead, and
//! `canonical_ser_is_the_rfc0126_substrate_canonicalizer` pins which.

use blake3;
use octo_determin::dcs::dcs_serialize_bytes;

use crate::{AlgorithmId, VerificationMethodId};

/// Domain string for the hashed form, per RFC-0009 §Domain separation.
pub const SIGNABLE_DOMAIN: &[u8] = b"cipherocto/sigdomain/v1";

/// Which of the two non-interoperable encodings a peer is using.
///
/// Named rather than implied, because a verifier that guesses is exactly the
/// failure this exists to prevent: both forms produce a 32-byte prefix, so a
/// verifier that assumed one and received the other would not see a malformed
/// message — it would see a well-formed message carrying the wrong domain.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SignableForm {
    /// `algorithm_id ‖ key_id ‖ canonical_ser(payload)`. This implementation's
    /// form. Carries the algorithm in band.
    Raw,
    /// `BLAKE3-256(domain ‖ algorithm_id ‖ key_id) ‖ canonical_ser(payload)`.
    /// Carries **no** recoverable algorithm — see [`SignedMessage::parse`].
    Hashed,
}

impl SignableForm {
    /// Whether this is [`SignableForm::Raw`].
    #[must_use]
    pub const fn is_raw(self) -> bool {
        matches!(self, Self::Raw)
    }
}

/// The 32-byte mandatory prefix: an algorithm identifier and a key identifier.
///
/// Fixed width by construction — both fields are newtypes over `[u8; 16]` —
/// which is what makes the raw form self-delimiting.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SignablePrefix {
    algorithm_id: AlgorithmId,
    key_id: VerificationMethodId,
}

impl SignablePrefix {
    /// Wire length of the prefix: 16 + 16.
    pub const LEN: usize = 32;

    /// Build a prefix from the two typed identifiers.
    #[must_use]
    pub const fn new(algorithm_id: AlgorithmId, key_id: VerificationMethodId) -> Self {
        Self {
            algorithm_id,
            key_id,
        }
    }

    /// The algorithm this message is signed under.
    #[must_use]
    pub const fn algorithm_id(&self) -> AlgorithmId {
        self.algorithm_id
    }

    /// The key within the DID document that signed it.
    #[must_use]
    pub const fn key_id(&self) -> VerificationMethodId {
        self.key_id
    }

    /// The 32 prefix bytes: `algorithm_id ‖ key_id`, raw.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[..16].copy_from_slice(self.algorithm_id.as_bytes());
        out[16..].copy_from_slice(self.key_id.as_bytes());
        out
    }

    /// Read the 32 prefix bytes back.
    ///
    /// # Errors
    /// [`SignableError::TooShort`] if fewer than [`SignablePrefix::LEN`]
    /// bytes are supplied.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SignableError> {
        if bytes.len() < Self::LEN {
            return Err(SignableError::TooShort {
                got: bytes.len(),
                need: Self::LEN,
            });
        }
        let mut algorithm = [0u8; 16];
        let mut key = [0u8; 16];
        algorithm.copy_from_slice(&bytes[..16]);
        key.copy_from_slice(&bytes[16..Self::LEN]);
        Ok(Self::new(
            AlgorithmId::from_bytes(algorithm),
            VerificationMethodId::from_bytes(key),
        ))
    }
}

/// A payload canonicalized for the signable form.
///
/// Wrapped rather than aliased so the "this is the DCS form" property is
/// visible at the type level and cannot be quietly bypassed by passing raw
/// bytes straight to the builder.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CanonicalPayload(Vec<u8>);

impl CanonicalPayload {
    /// Canonicalize `payload` with RFC-0126 §Part 3.
    ///
    /// # Errors
    /// [`SignableError::PayloadTooLarge`] past the DCS 1 MiB limit. The
    /// payload is length-prefixed with `u32_be`, so this is a real boundary
    /// rather than a formality.
    pub fn new(payload: &[u8]) -> Result<Self, SignableError> {
        dcs_serialize_bytes(payload)
            .map(Self)
            .map_err(|_| SignableError::PayloadTooLarge)
    }

    /// The canonical bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Build the signable bytes a signature must cover.
///
/// # Errors
/// [`SignableError::PayloadTooLarge`] if the payload is past the DCS limit.
pub fn signable_bytes(
    prefix: &SignablePrefix,
    payload: &CanonicalPayload,
    form: SignableForm,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(SignablePrefix::LEN + 32 + payload.as_bytes().len());
    match form {
        SignableForm::Raw => out.extend_from_slice(&prefix.to_bytes()),
        SignableForm::Hashed => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(SIGNABLE_DOMAIN);
            hasher.update(&prefix.to_bytes());
            out.extend_from_slice(hasher.finalize().as_bytes());
        }
    }
    out.extend_from_slice(payload.as_bytes());
    out
}

/// A signed message on the wire: the prefix followed by the payload.
///
/// The payload is stored **unprefixed-by-DCS** here and canonicalized on
/// demand, so `to_bytes` and `signable` cannot disagree about which bytes the
/// signature covers.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SignedMessage {
    prefix: SignablePrefix,
    payload: Vec<u8>,
}

impl SignedMessage {
    /// Build a message from a prefix and its raw payload.
    #[must_use]
    pub fn new(prefix: SignablePrefix, payload: Vec<u8>) -> Self {
        Self { prefix, payload }
    }

    /// The prefix this message carries.
    #[must_use]
    pub fn prefix(&self) -> &SignablePrefix {
        &self.prefix
    }

    /// The raw payload.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// The bytes actually signed: `signable_bytes` over this message.
    ///
    /// # Errors
    /// [`SignableError::PayloadTooLarge`] if the payload is past the DCS limit.
    pub fn signable(&self, form: SignableForm) -> Result<Vec<u8>, SignableError> {
        Ok(signable_bytes(
            &self.prefix,
            &CanonicalPayload::new(&self.payload)?,
            form,
        ))
    }

    /// The wire form: prefix ‖ payload.
    ///
    /// The prefix is the raw 32 identifier bytes. For [`SignableForm::Hashed`]
    /// that is still what goes on the wire — the hashed form hashes
    /// `domain ‖ ids`, so the digest and the ids are two different renderings
    /// and only one of them can be the wire header a parser can read.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SignablePrefix::LEN + self.payload.len());
        out.extend_from_slice(&self.prefix.to_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Parse a wire message, declaring which form the peer is using.
    ///
    /// # Errors
    /// - [`SignableError::TooShort`] if there is no complete prefix.
    /// - [`SignableError::FormMismatch`] if a [`SignableForm::Hashed`] message
    ///   is offered, because the digest is not invertible — see below.
    ///
    /// ## Why the hashed form cannot be parsed back
    ///
    /// A verifier must "verify under the algorithm the prefix names rather
    /// than the one it prefers". In the raw form the prefix *names* the
    /// algorithm, so this is a lookup. In the hashed form the prefix is a
    /// digest: the algorithm is not recoverable from it by construction, so
    /// the algorithm would have to travel out of band — back to a verifier
    /// that trusts its own configuration, which is the property the prefix
    /// exists to remove.
    ///
    /// Refusing is therefore the honest answer, not a missing feature: this
    /// implementation ships the raw form, and a verifier declaring it rejects
    /// the other rather than guessing.
    pub fn parse(bytes: &[u8], form: SignableForm) -> Result<Self, SignableError> {
        if bytes.len() < SignablePrefix::LEN {
            return Err(SignableError::TooShort {
                got: bytes.len(),
                need: SignablePrefix::LEN,
            });
        }
        if !form.is_raw() {
            return Err(SignableError::FormMismatch { declared: form });
        }
        Ok(Self::new(
            SignablePrefix::from_bytes(bytes)?,
            bytes[SignablePrefix::LEN..].to_vec(),
        ))
    }
}

/// Failure of prefix construction or parsing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum SignableError {
    /// Fewer bytes than a prefix requires.
    #[error("message is {got} bytes; a signable prefix needs {need}")]
    TooShort {
        /// Bytes supplied.
        got: usize,
        /// Bytes required.
        need: usize,
    },
    /// The payload is past the RFC-0126 DCS length limit.
    #[error("payload exceeds the DCS length limit")]
    PayloadTooLarge,
    /// The declared form does not match what the message can carry.
    #[error("cannot parse a message declaring the {declared:?} form")]
    FormMismatch {
        /// The form the verifier declared.
        declared: SignableForm,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(name: &str, key: &str) -> SignablePrefix {
        SignablePrefix::new(AlgorithmId::derive(name), VerificationMethodId::derive(key))
    }

    // ── The vectors that compare byte strings ───────────────────────────

    #[test]
    fn signable_prefix_is_prefix_free() {
        // Three distinct triples must never collide. The two halves of the
        // first 32 bytes are fixed width, and the payload carries a `u32_be`
        // length prefix, so the whole thing is injective.
        let a = signable_bytes(
            &prefix("ed25519", "key-0"),
            &CanonicalPayload::new(b"abc").expect("payload"),
            SignableForm::Raw,
        );
        let b = signable_bytes(
            &prefix("ed25519", "key-1"),
            &CanonicalPayload::new(b"abc").expect("payload"),
            SignableForm::Raw,
        );
        let c = signable_bytes(
            &prefix("stark-schnorr", "key-0"),
            &CanonicalPayload::new(b"abc").expect("payload"),
            SignableForm::Raw,
        );
        // Length alone must not make them equal, and neither must a shift.
        let d = signable_bytes(
            &prefix("ed25519", "key-0"),
            &CanonicalPayload::new(b"bc").expect("payload"),
            SignableForm::Raw,
        );
        let e = signable_bytes(
            &prefix("ed25519", "key-0"),
            &CanonicalPayload::new(b"zabc").expect("payload"),
            SignableForm::Raw,
        );
        for (name, other) in [
            ("key-0/key-1", &b),
            ("algorithm swap", &c),
            ("shorter payload", &d),
            ("longer payload", &e),
        ] {
            assert_ne!(a, *other, "{name} must produce different signable bytes");
        }
    }

    #[test]
    fn signable_prefix_survives_algorithm_swap() {
        // Same key identifier, same payload, different algorithm: different
        // signed domain.
        let payload = CanonicalPayload::new(b"transfer 1").expect("payload");
        let ed = signable_bytes(&prefix("ed25519", "key-0"), &payload, SignableForm::Raw);
        let stark = signable_bytes(
            &prefix("stark-schnorr", "key-0"),
            &payload,
            SignableForm::Raw,
        );
        assert_ne!(ed, stark);
        // And the difference is confined to the first 32 bytes, which is
        // where the algorithm identifier lives.
        assert_eq!(ed[SignablePrefix::LEN..], stark[SignablePrefix::LEN..]);
    }

    #[test]
    fn signable_prefix_survives_key_swap() {
        let payload = CanonicalPayload::new(b"transfer 1").expect("payload");
        let a = signable_bytes(&prefix("ed25519", "key-0"), &payload, SignableForm::Raw);
        let b = signable_bytes(&prefix("ed25519", "key-1"), &payload, SignableForm::Raw);
        assert_ne!(a, b);
        assert_eq!(a[SignablePrefix::LEN..], b[SignablePrefix::LEN..]);
    }

    #[test]
    fn canonical_ser_is_the_rfc0126_substrate_canonicalizer() {
        // Pin WHICH canonicalizer this is. `dcs_serialize_bytes` is
        // `u32_be(len) ‖ bytes` — that length prefix is what makes the
        // encoding injective, and without it `[0x01, 0x02]` and
        // `[0x00, 0x01, 0x02]` would pack identically.
        let a = CanonicalPayload::new(&[0x01, 0x02])
            .expect("payload")
            .as_bytes()
            .to_vec();
        let b = CanonicalPayload::new(&[0x00, 0x01, 0x02])
            .expect("payload")
            .as_bytes()
            .to_vec();
        assert_ne!(a, b, "the length prefix is load-bearing, not framing noise");

        // Empty is length 0, not an absent field.
        let empty = CanonicalPayload::new(&[])
            .expect("payload")
            .as_bytes()
            .to_vec();
        assert_eq!(empty, vec![0, 0, 0, 0]);

        // And it is the substrate's function, not JSON and not borsh.
        let raw = CanonicalPayload::new(b"abc")
            .expect("payload")
            .as_bytes()
            .to_vec();
        assert_ne!(raw, serde_json_bytes(b"abc"));
        assert_ne!(raw, borsh_like_bytes(b"abc"));
    }

    /// A JSON encoding of the same value, for the disambiguation above.
    /// Local rather than a dependency: the point is that DCS is *not* JSON.
    fn serde_json_bytes(v: &[u8]) -> Vec<u8> {
        let mut out = vec![b'"'];
        out.extend_from_slice(v);
        out.push(b'"');
        out
    }

    /// A borsh-shaped encoding of the same value: the raw string with no
    /// length prefix at all.
    fn borsh_like_bytes(v: &[u8]) -> Vec<u8> {
        v.to_vec()
    }

    #[test]
    fn verifier_rejects_missing_prefix() {
        // No prefix at all: short, or prefix-shaped but unparseable.
        for len in [0usize, 1, 16, 31] {
            assert_eq!(
                SignedMessage::parse(&vec![0u8; len], SignableForm::Raw),
                Err(SignableError::TooShort {
                    got: len,
                    need: SignablePrefix::LEN
                }),
                "a {len}-byte message must not parse as prefixed"
            );
        }
        // Exactly the prefix with an empty payload IS well formed — an empty
        // payload is a payload, and refusing it would be a downgrade path.
        let empty = SignedMessage::new(prefix("ed25519", "key-0"), Vec::new());
        assert!(SignedMessage::parse(&empty.to_bytes(), SignableForm::Raw).is_ok());
    }

    #[test]
    fn verifier_rejects_hashed_form_when_declaring_raw() {
        // A peer sent the hashed form. Declaring raw, the verifier must not
        // accept it — and the failure is *not* "no prefix": it parses, and
        // then fails at dispatch on an algorithm id that is really a digest.
        let msg = SignedMessage::new(prefix("ed25519", "key-0"), b"payload".to_vec());
        let hashed = msg.signable(SignableForm::Hashed).expect("hashed");
        let mut as_raw = hashed.clone();
        // The digest stays; only the payload half is spliced back to the
        // raw message shape, and the payload half of a signable is the
        // *canonicalized* bytes, not the raw payload.
        let payload = CanonicalPayload::new(b"payload").expect("payload");
        as_raw[SignablePrefix::LEN..].copy_from_slice(payload.as_bytes());

        let parsed = SignedMessage::parse(&as_raw, SignableForm::Raw).expect("structurally valid");
        let named = parsed.prefix().algorithm_id();
        assert_ne!(
            named,
            AlgorithmId::derive("ed25519"),
            "a digest must not read back as the algorithm it was built from"
        );
    }

    #[test]
    fn verifier_rejects_raw_form_when_declaring_hashed() {
        let msg = SignedMessage::new(prefix("ed25519", "key-0"), b"payload".to_vec());
        let raw = msg.to_bytes();
        // Declaring hashed, the raw form is refused outright — the two are
        // not interoperable and a verifier states which it speaks.
        assert_eq!(
            SignedMessage::parse(&raw, SignableForm::Hashed),
            Err(SignableError::FormMismatch {
                declared: SignableForm::Hashed
            })
        );
    }

    #[test]
    fn the_two_forms_are_not_interoperable() {
        let msg = SignedMessage::new(prefix("ed25519", "key-0"), b"payload".to_vec());
        let raw = msg.signable(SignableForm::Raw).expect("raw");
        let hashed = msg.signable(SignableForm::Hashed).expect("hashed");
        assert_ne!(raw, hashed);
        // Same length, different bytes — a length check cannot tell them.
        assert_eq!(raw.len(), hashed.len());
        assert_eq!(raw[SignablePrefix::LEN..], hashed[SignablePrefix::LEN..]);
    }

    #[test]
    fn the_hashed_form_is_self_checking_because_it_is_a_digest() {
        // Rebuilding the digest from the identifiers it was built from
        // reproduces it exactly; feeding it any other prefix does not.
        let msg = SignedMessage::new(prefix("ed25519", "key-0"), b"payload".to_vec());
        let rebuilt = msg.signable(SignableForm::Hashed).expect("hashed");
        let digest = &rebuilt[..32];

        let mut hasher = blake3::Hasher::new();
        hasher.update(SIGNABLE_DOMAIN);
        hasher.update(&prefix("ed25519", "key-0").to_bytes());
        assert_eq!(digest, hasher.finalize().as_bytes());

        let mut other = blake3::Hasher::new();
        other.update(SIGNABLE_DOMAIN);
        other.update(&prefix("ed25519", "key-1").to_bytes());
        assert_ne!(digest, other.finalize().as_bytes());
    }

    #[test]
    fn the_prefix_is_thirty_two_bytes_of_raw_identifier_bytes() {
        let p = prefix("ed25519", "key-0");
        let bytes = p.to_bytes();
        assert_eq!(bytes.len(), SignablePrefix::LEN);
        // Raw, not the textual UUID. This is the distinction RFC-0009 makes
        // normative: borsh emits raw here and serde emits textual, and an
        // implementation that picks differently signs a different domain.
        assert_eq!(&bytes[..16], AlgorithmId::derive("ed25519").as_bytes());
        assert_eq!(
            &bytes[16..],
            VerificationMethodId::derive("key-0").as_bytes()
        );
        assert_eq!(SignablePrefix::from_bytes(&bytes).expect("round trip"), p);
    }

    #[test]
    fn wire_round_trips_and_signable_agrees_with_to_bytes() {
        let msg = SignedMessage::new(prefix("ed25519", "key-0"), b"the payload".to_vec());
        let wire = msg.to_bytes();
        let parsed = SignedMessage::parse(&wire, SignableForm::Raw).expect("parse");
        assert_eq!(parsed, msg);
        // The signed bytes must be reconstructible from the wire form alone,
        // or a verifier and a signer would be covering different things.
        assert_eq!(
            parsed.signable(SignableForm::Raw).expect("signable"),
            msg.signable(SignableForm::Raw).expect("signable")
        );
    }
}
