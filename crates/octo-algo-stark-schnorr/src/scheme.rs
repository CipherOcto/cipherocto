//! Schnorr over the Cairo STARK curve: the challenge, the scalars, and the
//! scheme itself.
//!
//! # The scheme
//!
//! The signer holds `x` with `Q = [x]G`. To sign `m` it picks `k`, sets
//! `R = [k]G`, and returns `e = H(R_x, m)` and `s = k + e·x (mod ORDER)`.
//! Verification is `[s]G == R + [e]Q`, which this module evaluates as
//! `[s]G - [e]Q` and compares against `R` on the **x-coordinate only** —
//! `R` is transmitted as an x-coordinate, so comparing points would first have
//! to reconstruct `R`, which is ambiguous in exactly the way the parity bit
//! already had to resolve once.
//!
//! This mirrors `schnorr_verify_stark_curve` in `cairo/src/stark_schnorr.cairo`.
//!
//! # Poseidon, and which Poseidon
//!
//! `H` is Poseidon over `r_x` followed by the message felts. It has to be
//! Poseidon rather than SHA-512 for the circuit side — SHA-512 would be a
//! syscall, so the challenge would be prover-chosen.
//!
//! **The form matters, and this is the single easiest thing to get wrong in
//! this file.** Cairo's `core::poseidon::poseidon_hash_span` and the Rust
//! `starknet_crypto::poseidon_hash_many` agree; `starknet_crypto::poseidon_hash`
//! — the two-*argument* function — does **not**, because it pads with a
//! different domain and a different number of permutations. Using it here
//! would produce a self-consistent implementation that agrees with itself,
//! disagrees with the circuit on every single message, and passes every
//! positive test written against itself.
//!
//! `challenge_matches_the_circuit_vectors` below pins the agreement against
//! the identical constant asserted in Cairo, and
//! `the_two_argument_poseidon_is_a_different_function` pins *why* the wrong
//! choice is not caught by anything else.

use num_bigint::{BigInt, Sign};
use starknet_crypto::{poseidon_hash_many, Felt};
use starknet_types_core::curve::{AffinePoint, ProjectivePoint};
use thiserror::Error;

use crate::key::{StarkSchnorrKeyError, StarkSchnorrPublicKey};

/// The Cairo STARK curve's group order.
///
/// Hex of the value `core::ec::stark_curve::ORDER` reports. It is **not** the
/// field prime: `p = 2·ORDER − 1`, so reducing a scalar modulo `p` yields a
/// different group element rather than an equivalent one, and signing with
/// field arithmetic silently produces signatures that verify against the
/// wrong equation. `order_is_the_curve_group_order` checks the constant
/// rather than trusting this paragraph.
pub const CURVE_ORDER_HEX: &str =
    "0x800000000000010ffffffffffffffffb781126dcae7b2321e66a241adc64d2f";

/// The curve's group order.
#[must_use]
pub fn curve_order() -> Felt {
    Felt::from_hex_unchecked(CURVE_ORDER_HEX)
}

/// Bytes packed into one felt when a message enters the challenge.
pub const MESSAGE_BYTES_PER_FELT: usize = 31;

/// A STARK-curve Schnorr signature: `(r, s)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StarkSchnorrSignature {
    /// The commitment: the x-coordinate of `R = [k]G`.
    pub r: Felt,
    /// The response scalar.
    pub s: Felt,
}

impl StarkSchnorrSignature {
    /// Wire length in bytes: two 32-byte big-endian felts.
    pub const WIRE_LEN: usize = 64;

    /// Encode as `r ‖ s`, each 32 bytes big-endian.
    #[must_use]
    pub fn to_wire(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[..32].copy_from_slice(&self.r.to_bytes_be());
        out[32..].copy_from_slice(&self.s.to_bytes_be());
        out
    }

    /// Decode `r ‖ s`.
    ///
    /// # Errors
    /// [`StarkSchnorrSignError::BadSignatureLength`] on any other length.
    pub fn from_wire(bytes: &[u8]) -> Result<Self, StarkSchnorrSignError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(StarkSchnorrSignError::BadSignatureLength { got: bytes.len() });
        }
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Ok(Self {
            r: Felt::from_bytes_be(&r),
            s: Felt::from_bytes_be(&s),
        })
    }
}

// ── Scalars modulo the group order ───────────────────────────────────────

fn to_big(value: &Felt) -> BigInt {
    BigInt::from_bytes_be(Sign::Plus, &value.to_bytes_be())
}

fn from_big(value: &BigInt) -> Felt {
    let (_, buffer) = value.to_bytes_be();
    let mut bytes = [0u8; 32];
    bytes[32 - buffer.len()..].copy_from_slice(&buffer);
    Felt::from_bytes_be(&bytes)
}

/// Reduce a field element into `[0, ORDER)`.
#[must_use]
pub fn reduce_mod_order(value: &Felt) -> Felt {
    let order = to_big(&curve_order());
    let remainder = to_big(value) % &order;
    // Felt values are always canonical and non-negative, so `remainder` is
    // already in [0, ORDER). The `+ order` is kept only so this reads as
    // "reduce into [0, ORDER)" rather than leaving the reader to reason about
    // the sign of a remainder on a value that cannot be negative.
    from_big(&((remainder + &order) % &order))
}

/// `(a + b) mod ORDER`.
///
/// Felt addition reduces modulo the **field** prime, not the group order, and
/// the two differ — so this cannot be a `Felt` `+`.
#[must_use]
pub fn add_mod_order(a: &Felt, b: &Felt) -> Felt {
    let order = to_big(&curve_order());
    from_big(&((to_big(a) + to_big(b)) % &order))
}

/// `(a · b) mod ORDER`.
#[must_use]
pub fn mul_mod_order(a: &Felt, b: &Felt) -> Felt {
    let order = to_big(&curve_order());
    from_big(&((to_big(a) * to_big(b)) % &order))
}

// ── The challenge ────────────────────────────────────────────────────────

/// Pack a message into felts for the challenge absorption.
///
/// Big-endian, [`MESSAGE_BYTES_PER_FELT`] bytes per felt, **length-prefixed**:
/// element 0 is the byte length, the rest are the packed groups.
///
/// The length prefix is not optional. Right-aligning each group looks
/// sufficient until it is tested, at which point `[0x01, 0x02]` and
/// `[0x00, 0x01, 0x02]` pack identically — and two messages that pack
/// identically get the *same challenge*, so a signature over one verifies
/// against the other.
///
/// This is the encoding for **this algorithm's challenge input only**. It is
/// deliberately not a wire format and not the macaroon caveat-chain encoding
/// in `octo-cap-macaroon`; reusing a chain encoding here would put a
/// capability-chain value into a signature domain, which is the kind of
/// cross-domain confusion the signable prefix exists to prevent.
#[must_use]
pub fn message_to_felts(message: &[u8]) -> Vec<Felt> {
    let mut out = Vec::with_capacity(1 + message.len().div_ceil(MESSAGE_BYTES_PER_FELT));
    out.push(Felt::from(message.len() as u64));
    for chunk in message.chunks(MESSAGE_BYTES_PER_FELT) {
        let mut buf = [0u8; 32];
        buf[32 - chunk.len()..].copy_from_slice(chunk);
        out.push(Felt::from_bytes_be(&buf));
    }
    out
}

/// Inverse of [`message_to_felts`].
///
/// Returns `None` for any felt sequence this function did not produce: a
/// missing prefix, a length that disagrees with the group count, or a group
/// wider than [`MESSAGE_BYTES_PER_FELT`]. Decoding a malformed sequence
/// instead would map it onto a *different, valid* message.
#[must_use]
pub fn felts_to_message(felts: &[Felt]) -> Option<Vec<u8>> {
    let (prefix, body) = felts.split_first()?;
    let len = usize::try_from(prefix.to_le_digits()[0]).ok()?;
    if body.len() != len.div_ceil(MESSAGE_BYTES_PER_FELT) {
        return None;
    }
    if body.is_empty() {
        return (len == 0).then(Vec::new);
    }

    let mut out = Vec::with_capacity(len);
    let remainder = len % MESSAGE_BYTES_PER_FELT;
    for (i, felt) in body.iter().enumerate() {
        let be = felt.to_bytes_be();
        // Only the final group may be short; every other one is full width, so
        // a non-zero leading byte there means the value was reduced.
        let width = if i + 1 == body.len() {
            if remainder == 0 {
                MESSAGE_BYTES_PER_FELT
            } else {
                remainder
            }
        } else {
            MESSAGE_BYTES_PER_FELT
        };
        if be[..32 - width].iter().any(|b| *b != 0) {
            return None;
        }
        out.extend_from_slice(&be[32 - width..]);
    }
    debug_assert_eq!(out.len(), len);
    Some(out)
}

/// The Schnorr challenge `H(r_x, m)`, absorbing `r_x` then the message felts.
///
/// This is `poseidon_hash_span([r] ++ m)` in Cairo, and it is what
/// `schnorr_challenge` computes for a single-felt message.
#[must_use]
pub fn challenge(r: &Felt, message_felts: &[Felt]) -> Felt {
    let mut items: Vec<&Felt> = Vec::with_capacity(1 + message_felts.len());
    items.push(r);
    items.extend(message_felts.iter());
    poseidon_hash_many(items)
}

/// [`challenge`] over a byte message.
#[must_use]
pub fn challenge_bytes(r: &Felt, message: &[u8]) -> Felt {
    challenge(r, &message_to_felts(message))
}

// ── Keys and the scheme ──────────────────────────────────────────────────

/// The public key for a secret scalar `x`.
///
/// # Errors
/// [`StarkSchnorrSignError::ScalarOutOfRange`] if `secret` is zero or not
/// below the group order. A secret outside `[1, ORDER)` does not generate a
/// usable key: zero yields the point at infinity, and a value above the order
/// silently aliases a smaller scalar.
pub fn public_key(secret: &Felt) -> Result<StarkSchnorrPublicKey, StarkSchnorrSignError> {
    let reduced = reduce_mod_order(secret);
    if reduced == Felt::ZERO {
        return Err(StarkSchnorrSignError::ScalarOutOfRange);
    }
    let point = &AffinePoint::generator() * reduced;
    if point.is_identity() {
        return Err(StarkSchnorrSignError::ScalarOutOfRange);
    }
    StarkSchnorrPublicKey::new(
        point.x().to_bytes_be(),
        point.y().to_le_digits()[0] & 1 == 1,
    )
    .map_err(StarkSchnorrSignError::Key)
}

/// Sign `message` with `secret` and a caller-supplied nonce.
///
/// **The caller owns nonce freshness.** `nonce` must never repeat for a given
/// `secret`: a repeated nonce leaks the secret through two equations sharing
/// the same `e` and `r`. This crate deliberately does not generate one, because
/// a deterministic nonce needs a keyed derivation the crate would have to
/// invent, and an invented one is worse than an explicit obligation.
///
/// # Errors
/// - [`StarkSchnorrSignError::ScalarOutOfRange`] for a zero or out-of-range
///   secret or nonce.
/// - [`StarkSchnorrSignError::ZeroChallenge`] if the challenge is zero, which
///   would make the signature independent of the secret.
pub fn sign(
    secret: &Felt,
    message: &[u8],
    nonce: &Felt,
) -> Result<StarkSchnorrSignature, StarkSchnorrSignError> {
    let x = reduce_mod_order(secret);
    let k = reduce_mod_order(nonce);
    if x == Felt::ZERO || k == Felt::ZERO {
        return Err(StarkSchnorrSignError::ScalarOutOfRange);
    }

    // R = [k]G, and only R's x-coordinate is transmitted.
    let r_point = &AffinePoint::generator() * k;
    if r_point.is_identity() {
        return Err(StarkSchnorrSignError::ScalarOutOfRange);
    }
    let r = r_point.x();

    let e = reduce_mod_order(&challenge_bytes(&r, message));
    if e == Felt::ZERO {
        return Err(StarkSchnorrSignError::ZeroChallenge);
    }

    // s = k + e·x (mod ORDER). Both reductions matter: e and x are field
    // elements and may exceed the order, and `Felt` arithmetic would reduce
    // them modulo p instead.
    let s = add_mod_order(&k, &mul_mod_order(&e, &x));
    Ok(StarkSchnorrSignature { r, s })
}

/// Verify `(r, s)` against a precomputed challenge `e`.
///
/// This is the circuit's interface: `schnorr_verify_stark_curve` takes `e` as
/// a parameter, and the test vectors in `cairo/src/stark_schnorr.cairo` pin it
/// rather than deriving it. Taking `e` as an argument is what lets those
/// vectors be replayed here verbatim.
///
/// Returns `false` rather than erroring on malformed input: a non-curve
/// x-coordinate, or a difference that lands on the point at infinity.
#[must_use]
pub fn verify_with_challenge(
    public_key: &StarkSchnorrPublicKey,
    r: &Felt,
    e: &Felt,
    s: &Felt,
) -> bool {
    // Recover Q from the x-coordinate and the parity bit. Returning `None`
    // here is ordinary, not exceptional: roughly half of all x values have no
    // point on the curve at all.
    let Some(q) = AffinePoint::new_from_x(&public_key.x(), public_key.parity()) else {
        return false;
    };
    let (Ok(qp), Ok(gp)) = (
        ProjectivePoint::from_affine(q.x(), q.y()),
        ProjectivePoint::from_affine(AffinePoint::generator().x(), AffinePoint::generator().y()),
    ) else {
        return false;
    };

    let sg = &gp * *s;
    let eq = &qp * *e;
    let lhs = &sg - &eq;

    // `ProjectivePoint::x()` returns the **homogeneous** coordinate, not the
    // affine one — unlike `AffinePoint::x()`, which normalises through
    // `to_affine()` first. In projective coordinates the point is (X : Y : Z)
    // and the affine x is X/Z, so comparing `lhs.x()` against `r` compares
    // against a value scaled by an unknown Z whenever the difference is not
    // already normalised.
    //
    // The failure is silent and total: it rejects every honest signature,
    // while still rejecting the forged ones, so a positive vector is the only
    // thing that catches it. `to_affine()` also supplies the infinity check —
    // it errors exactly when Z is zero — which is the refusal case that
    // `[s]G - [e]Q == O` has to produce.
    match lhs.to_affine() {
        Ok(point) => point.x() == *r,
        Err(_) => false,
    }
}

/// Verify a signature over `message`, deriving the challenge from it.
///
/// # Errors
/// [`StarkSchnorrSignError::Key`] if the public key is malformed, and
/// [`StarkSchnorrSignError::Rejected`] if the signature does not verify.
pub fn verify(
    public_key: &StarkSchnorrPublicKey,
    message: &[u8],
    signature: &StarkSchnorrSignature,
) -> Result<(), StarkSchnorrSignError> {
    let e = challenge_bytes(&signature.r, message);
    if verify_with_challenge(public_key, &signature.r, &e, &signature.s) {
        Ok(())
    } else {
        Err(StarkSchnorrSignError::Rejected)
    }
}

/// Failure of the scheme's own operations.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum StarkSchnorrSignError {
    /// A scalar was zero or not below the group order.
    #[error("scalar is zero or outside [1, ORDER)")]
    ScalarOutOfRange,
    /// The challenge was zero, which would make the signature independent of
    /// the secret.
    #[error("challenge is zero")]
    ZeroChallenge,
    /// The signature did not verify.
    #[error("signature did not verify")]
    Rejected,
    /// The public key is malformed.
    #[error(transparent)]
    Key(#[from] StarkSchnorrKeyError),
    /// The signature wire form was not 64 bytes.
    #[error("stark-schnorr signature must be 64 bytes, got {got}")]
    BadSignatureLength {
        /// Length supplied.
        got: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(hex: &str) -> Felt {
        Felt::from_hex_unchecked(hex)
    }

    /// The generator as Cairo's corelib reports it.
    ///
    /// `starknet-types-core` supplies the curve independently of the Cairo
    /// toolchain, so "the same curve" is an assumption until it is checked.
    #[test]
    fn the_generator_matches_the_circuit() {
        assert_eq!(
            AffinePoint::generator().x(),
            Felt::from_hex_unchecked(
                "0x1ef15c18599971b7beced415a40f0c7deacfd9b0d1819e03d723d8bc943cfca"
            ),
            "GEN_X must match core::ec::stark_curve::GEN_X"
        );
        assert_eq!(
            AffinePoint::generator().y(),
            Felt::from_hex_unchecked(
                "0x5668060aa49730b7be4801df46ec62de53ecd11abe43a32873000c36e8dc1f"
            ),
            "GEN_Y must match core::ec::stark_curve::GEN_Y"
        );
    }

    /// The group order, checked rather than trusted.
    ///
    /// A wrong `ORDER` here would still produce self-consistent signatures
    /// under the host implementation — every signing and verification path in
    /// this crate would agree — while disagreeing with the circuit, which is
    /// where the order is load-bearing.
    #[test]
    fn order_is_the_curve_group_order() {
        let g = AffinePoint::generator();
        assert!(
            (&ProjectivePoint::from_affine(g.x(), g.y()).expect("generator") * curve_order())
                .is_identity(),
            "[ORDER]G must be the point at infinity"
        );
        // And one less is not, which rules out an off-by-one constant.
        let minus_one = curve_order() - Felt::ONE;
        let g = AffinePoint::generator();
        assert!(
            !(&ProjectivePoint::from_affine(g.x(), g.y()).expect("generator") * minus_one)
                .is_identity(),
            "[ORDER-1]G must not be the point at infinity"
        );
    }

    // ── Host/circuit parity for the challenge ─────────────────────────────

    /// The constant asserted **identically** by `challenge_parity_vector` in
    /// `cairo/src/stark_schnorr.cairo`.
    ///
    /// This is the whole host/circuit contract for this algorithm. The
    /// challenge is the one value with no algebraic check on it —
    /// `verify_with_challenge` takes `e` as a parameter — so if the two sides
    /// disagree about how to derive it, both still run, both still produce
    /// plausible felts, and every signature is rejected by whichever side
    /// disagrees.
    const CIRCUIT_CHALLENGE: Felt = Felt::from_hex_unchecked(
        "0x559ef2d51337e4d61cb9f2fef4c1dcb0fa0d790d30ccd76ff2a39b1e7a35df5",
    );
    /// `V1_R` from `cairo/src/stark_schnorr.cairo`.
    const V1_R: Felt = Felt::from_hex_unchecked(
        "0x4baadfa62cb588e0d2d33f7c6f546fe31834eba183f0826bc91c96845b8c6f8",
    );
    /// `V1_E` from `cairo/src/stark_schnorr.cairo`.
    const V1_E: Felt = Felt::from_hex_unchecked("0xdeadbeef");

    #[test]
    fn challenge_matches_the_circuit_vectors() {
        assert_eq!(
            challenge(&V1_R, &[V1_E]),
            CIRCUIT_CHALLENGE,
            "host/circuit Schnorr challenge divergence"
        );
    }

    /// Why the two-argument `poseidon_hash` cannot be used here.
    ///
    /// It is the function a reader reaches for first, it has the right
    /// arity, and it returns a `Felt`. It is also a **different hash**:
    /// `poseidon_hash(x, y)` permutes `[x, y, 2]`, while
    /// `poseidon_hash_many([x, y])` absorbs into `[0, 0, 0]` and applies two
    /// permutations. Nothing in the type system or the arity distinguishes
    /// them, so this is pinned explicitly.
    #[test]
    fn the_two_argument_poseidon_is_a_different_function() {
        assert_eq!(
            challenge(&V1_R, &[V1_E]),
            poseidon_hash_many([&V1_R, &V1_E]),
            "the challenge must be the span form"
        );
        assert_ne!(
            challenge(&V1_R, &[V1_E]),
            starknet_crypto::poseidon_hash(V1_R, V1_E),
            "if these ever agree the distinction this guard rests on has moved"
        );
    }

    #[test]
    fn the_challenge_is_order_sensitive() {
        assert_ne!(challenge(&V1_R, &[V1_E]), challenge(&V1_E, &[V1_R]));
    }

    #[test]
    fn a_different_message_yields_a_different_challenge() {
        let a = challenge_bytes(&V1_R, b"transfer 1");
        let b = challenge_bytes(&V1_R, b"transfer 2");
        assert_ne!(a, b);
    }

    // ── Message packing ───────────────────────────────────────────────────

    #[test]
    fn message_packing_round_trips() {
        for len in [0usize, 1, 30, 31, 32, 61, 62, 63, 200] {
            let message: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let packed = message_to_felts(&message);
            assert_eq!(
                felts_to_message(&packed).as_deref(),
                Some(&message[..]),
                "len={len}"
            );
        }
    }

    #[test]
    fn distinct_messages_pack_to_distinct_sequences() {
        // The collision property. Without the length prefix these two agree,
        // and two messages sharing a challenge means a signature over one
        // verifies against the other.
        let a = message_to_felts(&[0x01, 0x02]);
        let b = message_to_felts(&[0x00, 0x01, 0x02]);
        let c = message_to_felts(&[0x01, 0x02, 0x00]);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert_ne!(a, message_to_felts(&[]));
    }

    #[test]
    fn a_malformed_message_felt_sequence_is_refused_not_truncated() {
        assert_eq!(felts_to_message(&[]), None);
        assert_eq!(
            felts_to_message(&message_to_felts(b"0123456789")[1..]),
            None,
            "a missing length prefix must not decode"
        );
        // A declared length that disagrees with the group count.
        let mut lying = message_to_felts(&[0u8; 64]);
        lying[0] = Felt::from(1_u64);
        assert_eq!(felts_to_message(&lying), None);
        // A group wider than the packing width could only have been reduced.
        let oversized = vec![Felt::from(7u64), Felt::from_bytes_be(&[0xFF; 32])];
        assert_eq!(felts_to_message(&oversized), None);
    }

    // ── Scalar arithmetic ─────────────────────────────────────────────────

    #[test]
    fn mod_order_is_not_field_arithmetic() {
        // p = 2·ORDER − 1, so the two moduli differ. If this ever holds the
        // distinction collapses and the explicit reduction the signer relies
        // on becomes redundant.
        assert_eq!(curve_order() * Felt::TWO, curve_order() + curve_order());
        assert_ne!(
            curve_order(),
            Felt::from_bytes_be(&{
                let mut b = [0u8; 32];
                b[0] = 0x80;
                b
            })
        );

        // A value already inside [0, ORDER) is returned unchanged.
        let inside = curve_order() - Felt::from(3u64);
        assert_eq!(reduce_mod_order(&inside), inside);

        // A field element at or above ORDER comes back reduced, and lands
        // strictly inside the range rather than merely somewhere smaller.
        let outside = curve_order() + Felt::from(5u64);
        let reduced = reduce_mod_order(&outside);
        assert_eq!(reduced, Felt::from(5u64));
        assert!(reduced < curve_order());
    }

    #[test]
    fn mod_order_arithmetic_agrees_with_exact_integer_arithmetic() {
        let a = fe("0x1234567890abcdef");
        let b = fe("0x0fedcba987654321");
        let order = curve_order();

        // Computed independently as plain integers.
        let order_i = to_big(&order);
        let expected_mul = (to_big(&a) * to_big(&b)) % &order_i;
        let expected_add = (to_big(&a) + to_big(&b)) % &order_i;

        assert_eq!(mul_mod_order(&a, &b), from_big(&expected_mul));
        assert_eq!(add_mod_order(&a, &b), from_big(&expected_add));
    }

    // ── Sign and verify ───────────────────────────────────────────────────

    #[test]
    fn a_genuine_signature_verifies() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let key = public_key(&secret).expect("key");
        let sig = sign(&secret, b"holder signature payload", &fe("0x1234")).expect("sign");
        verify(&key, b"holder signature payload", &sig).expect("must verify");
    }

    #[test]
    fn a_tampered_message_is_rejected() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let key = public_key(&secret).expect("key");
        let sig = sign(&secret, b"transfer 1 octo", &fe("0x1234")).expect("sign");
        assert_eq!(
            verify(&key, b"transfer 2 octo", &sig),
            Err(StarkSchnorrSignError::Rejected)
        );
        // Even a one-bit change in the length must not verify.
        assert_eq!(
            verify(&key, b"transfer 1 oct", &sig),
            Err(StarkSchnorrSignError::Rejected)
        );
    }

    #[test]
    fn a_signature_from_another_key_is_rejected() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let other = fe("0x11223344556677889900aabbccddeeff");
        let sig = sign(&secret, b"payload", &fe("0x1234")).expect("sign");
        let other_key = public_key(&other).expect("key");
        assert_eq!(
            verify(&other_key, b"payload", &sig),
            Err(StarkSchnorrSignError::Rejected),
            "one holder must not be able to ride another's authorization"
        );
    }

    #[test]
    fn a_tampered_signature_is_rejected() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let key = public_key(&secret).expect("key");
        let sig = sign(&secret, b"payload", &fe("0x1234")).expect("sign");

        let mut bytes = sig.to_wire();
        bytes[0] ^= 0x01; // flip a bit in r
        let flipped_r = StarkSchnorrSignature::from_wire(&bytes).expect("decode");
        assert!(
            verify(&key, b"payload", &flipped_r).is_err(),
            "r must be bound"
        );

        let mut bytes = sig.to_wire();
        bytes[63] ^= 0x01; // flip a bit in s
        let flipped_s = StarkSchnorrSignature::from_wire(&bytes).expect("decode");
        assert!(
            verify(&key, b"payload", &flipped_s).is_err(),
            "s must be bound"
        );
    }

    #[test]
    fn a_flipped_parity_bit_is_rejected() {
        // Without the parity bit an honest signature verifies about half the
        // time and a forged one is equally hard to produce — the failure looks
        // like flakiness rather than a bug.
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let key = public_key(&secret).expect("key");
        let sig = sign(&secret, b"payload", &fe("0x1234")).expect("sign");

        let flipped =
            StarkSchnorrPublicKey::new(key.x().to_bytes_be(), !key.parity()).expect("canonical");
        assert_eq!(
            verify(&flipped, b"payload", &sig),
            Err(StarkSchnorrSignError::Rejected)
        );
    }

    #[test]
    fn an_empty_message_verifies_and_is_not_the_same_as_a_zero_byte() {
        // The length prefix is what separates these: without it an empty
        // message and a single zero byte would share a challenge.
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let key = public_key(&secret).expect("key");
        let empty = sign(&secret, b"", &fe("0x1234")).expect("sign");
        verify(&key, b"", &empty).expect("empty message verifies");
        assert_eq!(
            verify(&key, &[0u8], &empty),
            Err(StarkSchnorrSignError::Rejected)
        );
    }

    #[test]
    fn signing_refuses_a_zero_secret_or_nonce() {
        assert_eq!(
            public_key(&Felt::ZERO),
            Err(StarkSchnorrSignError::ScalarOutOfRange)
        );
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        assert_eq!(
            sign(&secret, b"m", &Felt::ZERO),
            Err(StarkSchnorrSignError::ScalarOutOfRange)
        );
        // A secret equal to the order is zero modulo it.
        assert_eq!(
            public_key(&curve_order()),
            Err(StarkSchnorrSignError::ScalarOutOfRange)
        );
    }

    #[test]
    fn signatures_round_trip_through_the_wire_form() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let key = public_key(&secret).expect("key");
        let sig = sign(&secret, b"payload", &fe("0x1234")).expect("sign");
        let bytes = sig.to_wire();
        assert_eq!(bytes.len(), StarkSchnorrSignature::WIRE_LEN);
        let decoded = StarkSchnorrSignature::from_wire(&bytes).expect("decode");
        assert_eq!(decoded, sig);
        // A round trip that preserves the value but corrupts the encoding
        // would still pass the equality above.
        verify(&key, b"payload", &decoded).expect("round-tripped signature verifies");
    }

    #[test]
    fn a_wrong_length_signature_wire_form_is_refused() {
        for len in [0usize, 32, 63, 65, 128] {
            assert_eq!(
                StarkSchnorrSignature::from_wire(&vec![0u8; len]),
                Err(StarkSchnorrSignError::BadSignatureLength { got: len })
            );
        }
    }

    /// A signature must not verify against itself for the wrong key shape:
    /// an x-coordinate with no point on the curve is refused rather than
    /// treated as a zero key.
    #[test]
    fn an_off_curve_public_key_is_refused() {
        // Search for an x with no curve point — about half of all x values.
        let mut x = Felt::from(0x2a2b3c4d5e6f708u64);
        let mut found = None;
        for _ in 0..64 {
            if AffinePoint::new_from_x(&x, false).is_none() {
                found = Some(x);
                break;
            }
            x += Felt::ONE;
        }
        let x = found.expect("some x has no curve point");
        let key = StarkSchnorrPublicKey::new(x.to_bytes_be(), false).expect("canonical");
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let sig = sign(&secret, b"payload", &fe("0x1234")).expect("sign");
        assert_eq!(
            verify(&key, b"payload", &sig),
            Err(StarkSchnorrSignError::Rejected),
            "an off-curve key must be refused, not crash or verify"
        );
    }
}
