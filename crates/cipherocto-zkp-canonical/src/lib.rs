//! CipherOcto ZK canonicalization primitives (R2 fix-up 2026-08-05;
//! R3 fix-up 2026-08-05 to remove orphan symbols).
//!
//! Mission 0958-a R1 register (commit `6b9baad6`) claimed that
//! `canonicalize_axes` was "extracted to single trait in
//! `crates/zk-circuit/src/canonicalize.rs`" and that there was a "single
//! shared `canonical_ser` crate" with "single `pub const ZKP_DOMAIN_PREFIX`
//! shared between crates". R2 (commit `c55304f0`) created this crate to
//! perform the consolidation; **R3 caught that R2 itself repeated the
//! pattern**: it shipped `canonical_ser` + `CanonicalPublicInputs` trait
//! symbols with **zero non-test callers** and no production struct that
//! could implement the trait (the trait's required fields don't match
//! `zk_verifier::PublicInputs` or `octo_wallet::PublicInputs`).
//!
//! **R3 fix-up (2026-08-05):** the orphan `canonical_ser` and
//! `CanonicalPublicInputs` trait were **removed**. This crate now
//! exposes only what is actually consumed:
//!
//! - [`canonicalize_axes`] — single canonical implementation (sort by
//!   `(name, value)`); the prior duplicate definitions in
//!   `octo-wallet::capability::zk_mint` and
//!   `quota-router-core::zk_verify::capability` both delegate here.
//! - [`ZKP_DOMAIN_PREFIX`] — single `pub const [u8; 4]` shared between
//!   `zk-verifier` (single-cap prefix `b"zkp:"`) and `zk-circuit`
//!   (per-cap prefix `b"zkp_per_cap:"`). Two distinct prefixes
//!   preserved for backward compat with on-disk commitments.
//!
//! **Deferral honest disclosure (was disposition drift in R2):**
//! `zk-circuit::canonical_ser(BatchSigPublicInputs) -> Vec<u8>` (line
//! 809) and `zk-verifier::canonicalize_public(PublicInputs) -> Vec<u8>`
//! (line 308) remain as two distinct canonical encoders. They serve
//! different shapes (batch-sig inputs vs verifier-side public inputs)
//! and unifying them into a single `canonical_ser` is the work of
//! follow-up mission `missions/open/0958-b-real-cairo-crypto.md` —
//! the trait needs adapter types per PublicInputs struct (the trait
//! fields don't currently match either struct). Do not claim the
//! consolidation is done.
//!
//! ## Determinism contract (RFC-0958 §Determinism Class A)
//!
//! Same inputs → same canonical bytes → same BLAKE3 hash. Across
//! processes, across architectures, across platforms.

// `blake3` removed from deps in R3 — the only function that used it
// (canonical_ser) was an orphan symbol with zero non-test callers. The
// remaining crate surface uses only the constants + canonicalize_axes
// (sort helper). R3 disposition: drop the symbol, drop the dep.

/// Domain prefix for single-capability ZK commitments.
///
/// Used by `zk-verifier::canonicalize_public` and the
/// `stub_commitment` / `StubShapedProofRejected` rejection path
/// (mission 0958-a R2 fix-up, 2026-08-05).
pub const ZKP_DOMAIN_PREFIX: &[u8; 4] = b"zkp:";

/// Domain prefix for per-capability ZK commitments (the batch envelope
/// path). Distinct from [`ZKP_DOMAIN_PREFIX`] to avoid commitment
/// collisions across the single-cap and per-cap surfaces.
///
/// Used by `zk-circuit::batch_proof_commitment`.
pub const ZKP_PER_CAP_DOMAIN_PREFIX: &[u8; 12] = b"zkp_per_cap:";

/// Canonicalize `axes_consumed` order (mission 0958-a R3 fix-up, 2026-07-31;
/// R4 fix 2026-08-04: sort by `(name, value)`; R2 fix 2026-08-05:
/// consolidated here).
///
/// **Contract:** `axes_consumed` MUST be sorted by `(axis_name, axis_value)`
/// before any structural equality check OR proof generation. Class A
/// determinism (RFC-0958 §Determinism contract).
///
/// Called at every boundary:
/// 1. Mint site (`octo-wallet::capability::zk_mint::mint_with_zk_and_signers`)
///    — so the proofer sees the canonical order.
/// 2. Verify site (`verify_capability_zk`, just before `public_inputs_equal`)
///    — so the structural comparison is order-independent.
pub fn canonicalize_axes(axes: &mut [(String, u64)]) {
    axes.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_axes_sorts_by_name_then_value() {
        let mut axes = vec![
            ("z".to_string(), 1),
            ("a".to_string(), 2),
            ("a".to_string(), 1),
            ("b".to_string(), 0),
        ];
        canonicalize_axes(&mut axes);
        let names: Vec<&str> = axes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "a", "b", "z"]);
        let values: Vec<u64> = axes.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, vec![1, 2, 0, 1]);
    }

    #[test]
    fn zkp_domain_prefix_constants_are_distinct() {
        assert_ne!(
            ZKP_DOMAIN_PREFIX.as_slice(),
            ZKP_PER_CAP_DOMAIN_PREFIX.as_slice()
        );
        assert_eq!(ZKP_DOMAIN_PREFIX.len(), 4);
        assert_eq!(ZKP_PER_CAP_DOMAIN_PREFIX.len(), 12);
    }
}
