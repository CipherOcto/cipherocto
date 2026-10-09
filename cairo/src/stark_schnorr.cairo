//! Schnorr signature verification over the Cairo **STARK curve**, for the
//! RFC-0958 step 1 holder-signature slot.
//!
//! This exists to answer one question with a measurement rather than an
//! estimate: Ed25519 costs 44,443 CASM words in this circuit, and the suspected
//! reason is that GF(2²⁵⁵−19) needs eight `u32` limbs and a hand-rolled
//! reduction. The STARK curve's field is felt252 itself, so a curve element is a
//! single felt and a scalar is a single felt.
//!
//! **This is sound, unlike a syscall.** `ec_state_add_mul` lowers to a handful
//! of CASM instructions that read a result out of the EcOp builtin segment —
//! structurally the same shape as `StarknetHint::SystemCall`, and the reason
//! that one is unsound. The difference is entirely downstream: cairo-air
//! implements `EcOpBuiltin` as a full AIR component, with `partial_ec_mul`
//! decompositions, regression tests, and a segment-range check in the verifier.
//! The EcOp segment is constrained. A syscall segment is not constrained by
//! anything, because hints are not part of the proof statement at all.
//! See `sha512_probe.cairo` for the syscall contrast.
//!
//! **The scheme.** Standard Schnorr over the STARK curve. The signer holds `x`
//! with `Q = [x]G`. To sign `m` it picks `k`, sets `R = [k]G`, and returns
//! `e = H(R_x, R_y, m)` and `s = k + e·x (mod ORDER)`. Verification is
//! `[s]G == R + [e]Q`, which this module checks as `[s]G + R - [e]Q == O` in a
//! single `EcState`, so it costs two scalar multiplications rather than
//! restarting a state.
//!
//! `H` is Poseidon, not SHA-512, and that is not a compromise — it is required.
//! Poseidon lowers to pure CASM with no hints, so the challenge is constrained;
//! SHA-512 would be a syscall and the challenge would be prover-chosen. Poseidon
//! also gives a 4-element digest natively, which suits `(R_x, R_y, m)`.
//!
//! **What this is not.** Not RFC-0009's key format. Holders would need to sign
//! with a scheme only this protocol can verify, which costs hardware-wallet and
//! MPC compatibility. That is a product trade, not a cryptographic one.
//!
//! **Measured: +295 CASM words.** With this verifier wired into `main` the
//! circuit goes 3,004 → 3,299 words, against 44,443 for the conformant Ed25519
//! verifier. The reason is not that the scalar multiplication got cheap —
//! `ec_state_add_mul` still lowers to a handful of instructions that read a
//! result out of the EcOp builtin segment. The multiplication simply is not in
//! the Cairo trace; it is in the builtin's algebraic constraint. So the word
//! count understates the cost, and the real price is proving time for the EcOp
//! component, which is not measured here.
//!
//! **Two things are not yet settled, and both matter more than the number.**
//!
//! 1. The circuit does not yet build in the harness. The EcOp lowering emits two
//!    hints at one CASM pc, and `bundled_casm_bytes` refuses that because
//!    `HintsCollection` resolves exactly one hint per pc — it would otherwise
//!    silently drop a constraint. That refusal is correct and should stay; the
//!    fix belongs in the harness, not in the gate.
//! 2. Nothing has been proved end to end. The +295 figure is a compile-time
//!    measurement of a program that cannot yet be executed by the proving
//!    pipeline, so it is not yet evidence that a STARK over this circuit
//!    verifies. Treat it as a strong lead, not a result.

use core::ec::{
    EcPoint, EcPointTrait, EcState, EcStateTrait, NonZeroEcPoint,
    stark_curve::{GEN_X, GEN_Y, ORDER},
};

/// Poseidon challenge over the Schnorr transcript.
///
/// Two-element digest is enough: `H(R_x, m)`. The y-coordinate is not needed —
/// `[s]G + R - [e]Q == O` already pins `R` exactly, so folding in `R_y` would
/// add nothing to soundness.
pub fn schnorr_challenge(r_x: felt252, message: felt252) -> felt252 {
    core::poseidon::poseidon_hash_span(array![r_x, message].span())
}

/// Verify a Schnorr signature over the STARK curve.
///
/// `pubkey_x` and `pubkey_sign` together determine `Q` uniquely. Recovering a
/// point from an x-coordinate is inherently ±-ambiguous — `R` and `-R` share an
/// x — and `ec_point_from_x_nz` resolves that by returning whichever root its
/// square-root hint produced, which measured here is the *negative* of the
/// point a conventional implementation would name `Q`. So the parity bit of the
/// true `y` travels with the key, and the verifier negates when recovery
/// disagrees. Without it, verification would be a coin flip that rejects honest
/// signatures roughly half the time.
///
/// **The check itself is on an x-coordinate, for the same reason.** The signing
/// equation is `[s]G == R + [e]Q`, but only `r = R.x` is transmitted. Comparing
/// points would require reconstructing `R`, which is ambiguous for the reason
/// above. Instead the verifier rearranges to `[s]G - [e]Q == R` and compares
/// only the left-hand side's x-coordinate. That is unambiguous and drops a whole
/// point reconstruction.
///
/// A first version of this function reconstructed `R` from `r`, subtracted it,
/// and rejected every honest signature while still rejecting all the forged
/// ones — a bug only the positive test could catch.
///
/// Returns false rather than panicking on malformed input: a non-curve pubkey,
/// a difference that is the point at infinity, or an x-coordinate mismatch.
pub fn schnorr_verify_stark_curve(
    pubkey_x: felt252, pubkey_sign: u8, r: felt252, s: felt252, e: felt252,
) -> bool {
    let q0: NonZeroEcPoint = match EcPointTrait::new_nz_from_x(pubkey_x) {
        Option::Some(p) => p,
        Option::None => { return false; },
    };
    let g: NonZeroEcPoint = match EcPointTrait::new_nz(GEN_X, GEN_Y) {
        Option::Some(p) => p,
        Option::None => { return false; },
    };

    // The verifier wants `-[e]·Q`. Recovery may hand back `Q` or `-Q`, and
    // `Neg` is not reachable for `NonZeroEcPoint` from outside corelib — but the
    // sign can be moved onto the scalar instead, which is the same effect and
    // costs nothing: `-[e]·Q = [ORDER-e]·Q`, and if recovery returned `-Q` then
    // `-[e]·(-Q) = [e]·(-Q)`.
    let y_low: u128 = { let yv: u256 = q0.y().into(); yv.low };
    let parity_ok: bool = (y_low & 1) == pubkey_sign.into();
    let neg_e: felt252 = if parity_ok { ORDER - e } else { e };

    let mut state: EcState = EcStateTrait::init();
    state.add_mul(s, g);
    state.add_mul(neg_e, q0);

    // `finalize_nz` yields None exactly when `[s]G - [e]Q` is the point at
    // infinity, which cannot equal any `R`, so that is a refusal.
    match state.finalize_nz() {
        Option::Some(p) => p.x() == r,
        Option::None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::schnorr_verify_stark_curve;

    // Vectors from an independent pure-Python implementation of the STARK curve
    // (y^2 = x^3 + x + beta over p = 2^251 + 17*2^192 + 1). That implementation
    // also confirmed G is on the curve, that G has full order with cofactor 1,
    // and that `[s]G == R + [e]Q` holds for each vector. So these are not the
    // Cairo code's own opinion of what a Schnorr signature looks like.
    //
    // `PUB_SIGN` is the parity of the true public key's y-coordinate. It is
    // load-bearing: without it recovery returns -Q and honest signatures fail.
    // `e` is passed in rather than derived, so the vectors need no Python
    // Poseidon; `schnorr_challenge` is exercised separately.
    const V1_PUB_X: felt252 = 0x314635a026b93e959147149cffb3e37cfec1b8c554e8b00b3ac8b30a7baa3c7;
    const V1_PUB_SIGN: u8 = 1;
    const V1_R: felt252 = 0x4baadfa62cb588e0d2d33f7c6f546fe31834eba183f0826bc91c96845b8c6f8;
    const V1_E: felt252 = 0xdeadbeef;
    const V1_S: felt252 = 0x16c341075d5b3b3e504f4d3b291704133cbf4068;

    const V2_PUB_X: felt252 = 0x64f83f2f683af14c41cba23ca0ebadcbcd1f0718a9cde058765cb061932238f;
    const V2_PUB_SIGN: u8 = 1;
    const V2_R: felt252 = 0x44063f06b37c09baf2b68de7a2f44d8e1af29ed24e7ab7302c8bb8441ab8fac;
    const V2_E: felt252 = 0xfeedface;
    const V2_S: felt252 = 0x2b0e05f3123579cd53a6da0d3fc7fc414ed10c52;

    #[test]
    fn a_correct_signature_verifies() {
        assert!(
            schnorr_verify_stark_curve(V1_PUB_X, V1_PUB_SIGN, V1_R, V1_S, V1_E),
            "vector 1 must verify",
        );
        assert!(
            schnorr_verify_stark_curve(V2_PUB_X, V2_PUB_SIGN, V2_R, V2_S, V2_E),
            "vector 2 must verify",
        );
    }

    /// The negative cases matter more than the positive one. A verifier that
    /// returned true unconditionally would pass `a_correct_signature_verifies`.
    #[test]
    fn a_wrong_challenge_is_refused() {
        assert!(
            !schnorr_verify_stark_curve(V1_PUB_X, V1_PUB_SIGN, V1_R, V1_S, V1_E + 1),
            "a wrong challenge must not verify",
        );
    }

    #[test]
    fn a_wrong_signature_scalar_is_refused() {
        assert!(
            !schnorr_verify_stark_curve(V1_PUB_X, V1_PUB_SIGN, V1_R, V1_S + 1, V1_E),
            "a wrong s must not verify",
        );
        assert!(
            !schnorr_verify_stark_curve(V1_PUB_X, V1_PUB_SIGN, V1_R, V1_S - 1, V1_E),
            "s - 1 must not verify",
        );
    }

    /// A signature by a different key must not verify against this pubkey. This
    /// is the case that matters for step 1: it is what stops one holder from
    /// riding another's authorization.
    #[test]
    fn a_signature_by_another_key_is_refused() {
        assert!(
            !schnorr_verify_stark_curve(V1_PUB_X, V1_PUB_SIGN, V2_R, V2_S, V2_E),
            "another key's signature must not verify here",
        );
    }

    #[test]
    fn a_wrong_commitment_r_is_refused() {
        assert!(
            !schnorr_verify_stark_curve(V1_PUB_X, V1_PUB_SIGN, V2_R, V1_S, V1_E),
            "a mismatched r must not verify",
        );
    }

    /// Flipping the public key's parity bit must break verification. This is the
    /// regression that pins the -Q recovery trap: without the parity bit this
    /// assertion has nowhere to live, and the honest vector fails instead.
    #[test]
    fn a_flipped_public_key_parity_is_refused() {
        assert!(
            !schnorr_verify_stark_curve(V1_PUB_X, 1 - V1_PUB_SIGN, V1_R, V1_S, V1_E),
            "a flipped pubkey parity must not verify",
        );
    }

    /// A non-curve pubkey must be refused, not crash. Roughly half of all `x`
    /// values have no point on the curve, so a caller cannot assume success.
    #[test]
    fn a_non_curve_pubkey_is_refused() {
        let mut x: felt252 = 0x2a2b3c4d5e6f708192a3b4c5d6e7f809;
        let mut rejected = false;
        loop {
            if x == 0x2b2b3c4d5e6f708192a3b4c5d6e7f809 {
                break;
            }
            if core::ec::EcPointTrait::new_from_x(x).is_none() {
                rejected = true;
                assert!(
                    !schnorr_verify_stark_curve(x, 0, V1_R, V1_S, V1_E),
                    "a non-curve pubkey must be refused, not crash",
                );
                break;
            }
            x += 1;
        };
        assert!(rejected, "expected some x with no curve point");
    }
}