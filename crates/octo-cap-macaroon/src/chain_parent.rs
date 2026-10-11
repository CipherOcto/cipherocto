//! Hierarchical attenuation chain parent binding (RFC-0009 §Threat model A10).
//!
//! This module owns one thing: the BLAKE3 binding that a `CapabilityTokenV2`
//! carries in its `chain_parent` field. It computes that binding and checks it.
//! It does not mint, attenuate, transport or verify anything else — the wire
//! type it serves lives in [`crate::bundle_v2`], and nothing in production
//! calls [`verify_chain_parent`] (see mission `0009-cf`).
//!
//! ## The formula
//!
//! ```text
//! chain_parent = BLAKE3(parent_cap_root || child_cap_root || child_depth.to_be_bytes())
//! ```
//!
//! RFC-0009 §`compute_chain_parent` states the depth is **one byte**, and warns
//! explicitly against using the four-byte big-endian encoding that appears in
//! the neighbouring information string. [`CHAIN_PARENT_INPUT_LEN`] pins that
//! choice: the binding input is exactly 65 bytes, and a test locks the value
//! against a four-byte implementation.
//!
//! ## What the key parameters are
//!
//! Both key parameters are **capability root hashes**, carried by a holder
//! record as `cap_root_hash: [u8; 32]` — a BLAKE3 digest of the credential.
//!
//! This is deliberately a fixed-width array, and it is deliberately *not* the
//! defect RFC-0009 AC#11 is about. AC#11 concerns public keys whose width is
//! the signature algorithm's; a digest is 32 bytes for every algorithm that
//! exists and every algorithm that will. Widening these parameters would not
//! buy a second algorithm — it would only remove the property that makes the
//! binding input self-delimiting, which is what lets the three fields be
//! concatenated without length framing.
//!
//! ## Domain separation
//!
//! The derivation above is **not domain separated**, and that is a deliberate
//! reading of the RFC rather than an oversight. RFC-0009's normative text for
//! these two functions specifies plain `blake3::hash` over the concatenation,
//! and deviating from an accepted RFC's normative formula would produce a value
//! no other conformant implementation can reproduce. The crate's other
//! derivations *are* separated (`bundle_id` prefixes
//! `BUNDLE_ID_DOMAIN_V2`), so the inconsistency is real and is recorded for
//! RFC amendment rather than fixed unilaterally in an implementation.
//!
//! What the missing separator does and does not threaten: the input is a fixed
//! 65 bytes of high-entropy digest material, so there is no ambiguity for two
//! honest callers to collide over. The exposure is cross-protocol — a value
//! computed here is not distinguishable from any other BLAKE3 digest over 65
//! bytes elsewhere. That is a real weakening, and it is a property of the
//! specification, not of this code.

use crate::bundle_v2::MAX_CHAIN_DEPTH;

/// Length in bytes of the `chain_parent` binding input: two 32-byte capability
/// root hashes plus a one-byte depth.
///
/// A compile-time constant because the self-delimiting property of the
/// concatenation is the reason the RFC gets away without length framing, and a
/// test that pins the length is what keeps that property honest.
pub const CHAIN_PARENT_INPUT_LEN: usize = 2 * 32 + 1;

/// Build the binding input that [`compute_chain_parent`] hashes.
///
/// Exposed so a verifier in another crate — or a circuit — can reproduce the
/// exact bytes rather than trusting that both sides happened to call the same
/// helper. RFC-0009 §`compute_chain_parent` defines the input as the
/// concatenation of the two capability roots and the one-byte depth.
#[must_use]
pub fn chain_parent_binding_input(
    parent_cap_root: &[u8; 32],
    child_cap_root: &[u8; 32],
    child_depth: u8,
) -> [u8; CHAIN_PARENT_INPUT_LEN] {
    let mut input = [0u8; CHAIN_PARENT_INPUT_LEN];
    input[..32].copy_from_slice(parent_cap_root);
    input[32..64].copy_from_slice(child_cap_root);
    input[64] = child_depth;
    input
}

/// Compute the `chain_parent` binding for a child capability (RFC-0009
/// §`compute_chain_parent`).
///
/// The symmetric mint-side counterpart to [`verify_chain_parent`]. Without it an
/// implementer cannot derive a `chain_parent` value that the verifier accepts,
/// which is why the RFC specifies both rather than only the check.
#[must_use]
pub fn compute_chain_parent(
    parent_cap_root: &[u8; 32],
    child_cap_root: &[u8; 32],
    child_depth: u8,
) -> [u8; 32] {
    let input = chain_parent_binding_input(parent_cap_root, child_cap_root, child_depth);
    *blake3::hash(&input).as_bytes()
}

/// Recompute the `chain_parent` binding and report whether `chain_parent`
/// matches (RFC-0009 §`verify_chain_parent`).
///
/// # What the binding actually buys
///
/// Because the input commits to `parent`, `child` **and** `depth`, a holder
/// cannot re-parent a child onto a different ancestor without recomputing the
/// value, and cannot claim a shallower position in the chain. That is the whole
/// of RFC-0009 §Threat model A10: forgery requires inverting BLAKE3 over
/// 65 bytes of digest material.
///
/// It does **not** by itself establish that the named parent exists, that it is
/// unrevoked, or that the whole chain is well formed. Those are
/// `check_wrapped_chain`'s job (RFC-0009 §`check_wrapped_chain`), which walks
/// the chain and consults a revocation set. Neither that walk nor any caller of
/// this function exists in the workspace yet, so **this predicate is currently
/// unreachable from production code**. It is substrate for mission `0009-cf`.
///
/// A production capability verifier now exists — `ProductionCapabilityVerifier`
/// in `quota-router-core` — and it does **not** call this. Not an oversight on
/// its side: it verifies the v1 macaroon wire, and that wire carries no
/// `chain_parent`. The binding lives on `CapabilityTokenV2`, inside the V2
/// bundle, and the two representations meet nowhere. See §`0009-cf` in the
/// mission for why that is a structural gap rather than a missing call.
#[must_use]
pub fn verify_chain_parent(
    parent_cap_root: &[u8; 32],
    child_cap_root: &[u8; 32],
    chain_parent: &[u8; 32],
    child_depth: u8,
) -> bool {
    // Strictness beyond the RFC's formula: a depth past the cap cannot name a
    // real chain position, so it is refused rather than bound. The RFC's own
    // decoder rejects the same values at `canonical_de`, so this changes no
    // valid token's outcome — it only stops the predicate from being usable to
    // bless a depth the chain format forbids. Fail-closed on purpose.
    if child_depth > MAX_CHAIN_DEPTH {
        return false;
    }
    let expected = compute_chain_parent(parent_cap_root, child_cap_root, child_depth);
    constant_time_eq(&expected, chain_parent)
}

/// Compare two 32-byte values without an early exit on the first difference.
///
/// The compared values are digests over public capability roots rather than
/// secrets, so this is hygiene rather than a defence against a specific attack.
/// It is cheap and it removes the question.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: [u8; 32] = [0x11; 32];
    const CHILD_ROOT: [u8; 32] = [0x22; 32];

    /// The binding input is 65 bytes and ends in the one-byte depth.
    #[test]
    fn binding_input_is_sixty_five_bytes() {
        let input = chain_parent_binding_input(&ROOT, &CHILD_ROOT, 3);
        assert_eq!(input.len(), 65);
        assert_eq!(input.len(), CHAIN_PARENT_INPUT_LEN);
        assert_eq!(&input[..32], &ROOT[..]);
        assert_eq!(&input[32..64], &CHILD_ROOT[..]);
        assert_eq!(input[64], 3, "depth occupies exactly one byte");
    }

    /// Locks the derivation against the four-byte encoding the RFC warns about.
    ///
    /// A four-byte big-endian depth produces a different digest, so asserting the
    /// two differ is what makes this vector an actual pin on the encoding rather
    /// than a value that happens to be correct today.
    #[test]
    fn compute_chain_parent_is_one_byte_depth() {
        let one_byte = compute_chain_parent(&ROOT, &CHILD_ROOT, 3);

        let mut four_byte_input = [0u8; 68];
        four_byte_input[..32].copy_from_slice(&ROOT);
        four_byte_input[32..64].copy_from_slice(&CHILD_ROOT);
        four_byte_input[64..68].copy_from_slice(&3u32.to_be_bytes());
        let four_byte = *blake3::hash(&four_byte_input).as_bytes();

        assert_ne!(
            one_byte, four_byte,
            "RFC-0009 requires the 1-byte depth; the 4-byte form must not be silently accepted"
        );
    }

    /// Byte-exact vector for the derivation.
    ///
    /// The digest is `9d8d35bb…80dbe` = BLAKE3 over the 65-byte input
    /// `11×32 ‖ 22×32 ‖ 01`. It was computed by a BLAKE3 implementation
    /// outside this crate and cross-checked against the Rust result, so this
    /// vector pins the *formula* and not merely whatever this code currently
    /// emits — copying the implementation's own output would have made the
    /// test a regression lock with no correctness claim behind it.
    #[test]
    fn compute_chain_parent_matches_pinned_digest() {
        assert_eq!(
            compute_chain_parent(&ROOT, &CHILD_ROOT, 1),
            [
                0x9d, 0x8d, 0x35, 0xbb, 0xf3, 0x77, 0x74, 0xaf, 0xf0, 0xa0, 0x9e, 0x1f, 0xe3, 0x66,
                0x65, 0x83, 0xe6, 0x2a, 0x3c, 0xef, 0x66, 0x61, 0xe2, 0x9a, 0x71, 0x6e, 0xcd, 0x84,
                0xf6, 0xd8, 0x0d, 0xbe
            ],
            "chain_parent binding digest changed"
        );
    }

    /// The gate RFC-0009 §A10 names, in the shape the threat model states:
    /// a correctly computed binding verifies.
    #[test]
    fn a_correctly_computed_binding_verifies() {
        let binding = compute_chain_parent(&ROOT, &CHILD_ROOT, 1);
        assert!(verify_chain_parent(&ROOT, &CHILD_ROOT, &binding, 1));
    }

    /// `chain_parent_forgery_breaks_verification` — the named A10 gate.
    ///
    /// Forgery here is an attacker choosing their own parent value without
    /// knowing the preimage. Flipping the digest models that exactly: the
    /// attacker does not get to recompute a valid one.
    #[test]
    fn chain_parent_forgery_breaks_verification() {
        let binding = compute_chain_parent(&ROOT, &CHILD_ROOT, 1);

        let mut forged = binding;
        forged[17] ^= 0x01;
        assert!(
            !verify_chain_parent(&ROOT, &CHILD_ROOT, &forged, 1),
            "a forged parent binding must not verify"
        );
    }

    /// Each of the three committed values is load-bearing: changing any one of
    /// them changes the digest, so a child cannot be re-parented, re-keyed or
    /// re-depthed without recomputing the binding.
    #[test]
    fn parent_child_and_depth_are_all_committed() {
        let base = compute_chain_parent(&ROOT, &CHILD_ROOT, 1);

        let other_parent = compute_chain_parent(&[0x33; 32], &CHILD_ROOT, 1);
        let other_child = compute_chain_parent(&ROOT, &[0x44; 32], 1);
        let other_depth = compute_chain_parent(&ROOT, &CHILD_ROOT, 2);

        assert_ne!(base, other_parent, "parent must be committed");
        assert_ne!(base, other_child, "child must be committed");
        assert_ne!(base, other_depth, "depth must be committed");

        assert!(!verify_chain_parent(&[0x33; 32], &CHILD_ROOT, &base, 1));
        assert!(!verify_chain_parent(&ROOT, &[0x44; 32], &base, 1));
        assert!(!verify_chain_parent(&ROOT, &CHILD_ROOT, &base, 2));
    }

    /// Depth above the cap is refused rather than bound.
    #[test]
    fn depth_past_the_cap_is_refused() {
        assert_eq!(MAX_CHAIN_DEPTH, 8);
        let at_cap = compute_chain_parent(&ROOT, &CHILD_ROOT, MAX_CHAIN_DEPTH);
        assert!(verify_chain_parent(
            &ROOT,
            &CHILD_ROOT,
            &at_cap,
            MAX_CHAIN_DEPTH
        ));

        let over_cap = compute_chain_parent(&ROOT, &CHILD_ROOT, MAX_CHAIN_DEPTH + 1);
        assert!(
            !verify_chain_parent(&ROOT, &CHILD_ROOT, &over_cap, MAX_CHAIN_DEPTH + 1),
            "a depth past MAX_CHAIN_DEPTH must not verify even with a matching binding"
        );
    }

    /// The zero array every production mint writes today is not a valid binding.
    ///
    /// This is the property that makes the current gap latent rather than live:
    /// a root token's `chain_parent` of `[0u8; 32]` does not satisfy the
    /// predicate for any real parent/child pair, so nothing built today can
    /// accidentally pass a chain check.
    #[test]
    fn the_zero_value_mint_writes_is_not_a_valid_binding() {
        assert!(
            !verify_chain_parent(&ROOT, &CHILD_ROOT, &[0u8; 32], 1),
            "[0u8; 32] must not satisfy the predicate"
        );
        assert!(
            !verify_chain_parent(&ROOT, &CHILD_ROOT, &[0u8; 32], 0),
            "[0u8; 32] must not satisfy the predicate at depth 0 either"
        );
    }
}
