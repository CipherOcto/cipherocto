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
//! **Measured, and proved end to end.**
//!
//! With this verifier wired into `main` the circuit goes 3,004 → 3,299 CASM
//! words, and a full STARK prove-and-verify passes with the syscall-free EcOp
//! path: **465.19 s**, against 440.58 s for the committed circuit. That is about
//! +5.6%, not the 13.8× the Ed25519 word count would imply.
//!
//! The reason the word count understates it is the same reason it is cheap:
//! `ec_state_add_mul` lowers to a handful of instructions that read a result
//! out of the EcOp builtin segment. The multiplication is not in the Cairo
//! trace; it is in the builtin's algebraic constraint. So +295 words is not the
//! price, it is the part of the price that lands in CASM.
//!
//! **Two operational requirements came out of this.**
//!
//! 1. Proving a circuit that uses `core::ec` needs a **larger stack than a
//!    default test thread has**. At the default the prover aborts with
//!    `has overflowed its stack`; at `RUST_MIN_STACK=134217728` it completes.
//!    Any CI job or integration test that exercises this path must set it, or
//!    run on the main thread.
//! 2. The EcOp lowering emits **two hints at one CASM pc**. The lowering gate in
//!    `zk-circuit` refused that, on the stated ground that cairo-vm resolves one
//!    hint per pc. That was true of cairo-vm 3.1.0 and is not of the pinned
//!    3.2.0, whose `HintsCollection` keeps a `HintRange` per pc and resolves all
//!    of them. The gate was protecting against a dropped constraint and was doing
//!    its job against a version of cairo-vm the project no longer uses; it has
//!    been corrected. Note the *executable* path never hit this gate, because it
//!    does not flatten the hint map — so the blocker only ever affected
//!    measurement, and the circuit compiled, ran and proved regardless.

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