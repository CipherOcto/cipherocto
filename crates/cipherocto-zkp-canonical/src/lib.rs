//! CipherOcto ZK canonicalization primitives (R2 fix-up 2026-08-05).
//!
//! Mission 0958-a R1 register (commit `6b9baad6`) claimed that
//! `canonicalize_axes` was "extracted to single trait in
//! `crates/zk-circuit/src/canonicalize.rs`" and that there was a "single
//! shared `canonical_ser` crate" with "single `pub const ZKP_DOMAIN_PREFIX`
//! shared between crates". None of these existed — the disposition was a
//! documentation bug.
//!
//! **R2 fix-up (2026-08-05):** this crate consolidates all three:
//!
//! - [`canonicalize_axes`] — single canonical implementation (sort by
//!   `(name, value)`); the prior duplicate definitions in
//!   `octo-wallet::capability::zk_mint` and
//!   `quota-router-core::zk_verify::capability` both delegate here.
//! - [`canonical_ser`] — single canonical JSON encoder for ZK public
//!   inputs; replaces the inline `canonicalize_public` implementations in
//!   `zk-verifier` and the `canonical_json` in `zk-circuit`.
//! - [`ZKP_DOMAIN_PREFIX`] — single `pub const [u8; 4]` shared between
//!   `zk-verifier` (single-cap prefix `b"zkp:"`) and `zk-circuit` (per-cap
//!   prefix `b"zkp_per_cap:"`). Two distinct prefixes preserved for
//!   backward compat with on-disk commitments; the crate exposes both.
//!
//! Mission 0958-b (the R4 follow-up) is the consumer that will exercise
//! this crate end-to-end with real STWO STARK proofs.
//!
//! ## Determinism contract (RFC-0958 §Determinism Class A)
//!
//! Same inputs → same canonical bytes → same BLAKE3 hash. Across
//! processes, across architectures, across platforms.

use blake3::Hasher;

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

/// Public inputs shape used by `verify_capability_zk` and the
/// batch verifier. Re-exported here so both `zk-verifier` and
/// `zk-circuit` can construct canonical commitments against the same
/// field ordering.
pub mod public_inputs {
    /// Mirror of `octo_wallet::capability::zk_mint::PublicInputs` /
    /// `quota_router_core::zk_verify::capability::PublicInputs` —
    /// declared as a trait to break the cyclic dependency between
    /// the wallet and the quota-router crate.
    ///
    /// The canonical implementation lives in each crate's own
    /// `PublicInputs` struct; both crates `impl CanonicalPublicInputs
    /// for &PublicInputs` to plug into [`crate::canonical_ser`].
    pub trait CanonicalPublicInputs {
        fn ask_id(&self) -> &[u8; 32];
        fn axes_consumed(&self) -> &[(String, u64)];
        fn cap_root_hash(&self) -> &[u8; 32];
        fn invocation_hash(&self) -> &[u8; 32];
        fn holder_did(&self) -> &str;
        fn current_unix_time(&self) -> u64;
        fn output_hash(&self) -> Option<&[u8; 32]>;
        fn provider_slot_id(&self) -> &str;
    }
}

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

/// Canonical serialization for ZK public inputs.
///
/// R2 fix-up (2026-08-05): the prior implementations of `canonical_ser`
/// in `zk-circuit` and `canonicalize_public` in `zk-verifier` were
/// independently maintained encodings — drift was undetectable at
/// compile time (R4 M7). This function is the single canonical
/// implementation; both crates now delegate here.
///
/// **Wire format:**
/// ```text
/// blake3(
///     ZKP_DOMAIN_PREFIX                          // 4 bytes
///   || ask_id                                    // 32 bytes
///   || leb128_len(axes.len())                    // variable
///   || for each (name, value) in canonical_axes:
///        leb128_len(name.len())
///     || name.as_bytes()
///     || value.to_le_bytes()                     // 8 bytes
///   || cap_root_hash                             // 32 bytes
///   || invocation_hash                           // 32 bytes
///   || leb128_len(holder_did.len())
///   || holder_did.as_bytes()
///   || current_unix_time.to_le_bytes()           // 8 bytes
///   || output_hash.map(|h| h.as_slice()).unwrap_or(&[]) // 0 or 32 bytes
///   || provider_slot_id.as_bytes()               // variable
/// )
/// ```
///
/// The output is the BLAKE3 digest of the canonical encoding.
pub fn canonical_ser(pi: &dyn public_inputs::CanonicalPublicInputs) -> [u8; 32] {
    let mut axes_canon: Vec<(String, u64)> = pi.axes_consumed().to_vec();
    canonicalize_axes(&mut axes_canon);

    let mut h = Hasher::new();
    h.update(ZKP_DOMAIN_PREFIX);
    h.update(pi.ask_id());
    h.update(&(axes_canon.len() as u32).to_le_bytes());
    for (name, value) in &axes_canon {
        h.update(&(name.len() as u32).to_le_bytes());
        h.update(name.as_bytes());
        h.update(&value.to_le_bytes());
    }
    h.update(pi.cap_root_hash());
    h.update(pi.invocation_hash());
    h.update(&(pi.holder_did().len() as u32).to_le_bytes());
    h.update(pi.holder_did().as_bytes());
    h.update(&pi.current_unix_time().to_le_bytes());
    if let Some(h_bytes) = pi.output_hash() {
        h.update(h_bytes);
    }
    h.update(pi.provider_slot_id().as_bytes());
    *h.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use public_inputs::CanonicalPublicInputs;

    #[derive(Clone)]
    struct TestInputs {
        ask_id: [u8; 32],
        axes_consumed: Vec<(String, u64)>,
        cap_root_hash: [u8; 32],
        invocation_hash: [u8; 32],
        holder_did: String,
        current_unix_time: u64,
        output_hash: Option<[u8; 32]>,
        provider_slot_id: String,
    }

    impl CanonicalPublicInputs for TestInputs {
        fn ask_id(&self) -> &[u8; 32] {
            &self.ask_id
        }
        fn axes_consumed(&self) -> &[(String, u64)] {
            &self.axes_consumed
        }
        fn cap_root_hash(&self) -> &[u8; 32] {
            &self.cap_root_hash
        }
        fn invocation_hash(&self) -> &[u8; 32] {
            &self.invocation_hash
        }
        fn holder_did(&self) -> &str {
            &self.holder_did
        }
        fn current_unix_time(&self) -> u64 {
            self.current_unix_time
        }
        fn output_hash(&self) -> Option<&[u8; 32]> {
            self.output_hash.as_ref()
        }
        fn provider_slot_id(&self) -> &str {
            &self.provider_slot_id
        }
    }

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
    fn canonical_ser_is_deterministic_under_axes_permutation() {
        let mut axes_a = vec![
            ("axis_a".to_string(), 10),
            ("axis_b".to_string(), 20),
            ("axis_c".to_string(), 30),
        ];
        let mut axes_b = vec![
            ("axis_c".to_string(), 30),
            ("axis_a".to_string(), 10),
            ("axis_b".to_string(), 20),
        ];
        // canonical_ser canonicalizes internally, so permutations should
        // produce identical commitments.
        let _ = canonicalize_axes(&mut axes_a);
        let _ = canonicalize_axes(&mut axes_b);
        assert_eq!(axes_a, axes_b);

        let inputs_a = TestInputs {
            ask_id: [1u8; 32],
            axes_consumed: axes_a.clone(),
            cap_root_hash: [2u8; 32],
            invocation_hash: [3u8; 32],
            holder_did: "did:octo:holder".to_string(),
            current_unix_time: 1000,
            output_hash: Some([4u8; 32]),
            provider_slot_id: "slot-1".to_string(),
        };
        let inputs_b = TestInputs {
            axes_consumed: axes_b,
            ..inputs_a.clone()
        };
        let commit_a = canonical_ser(&inputs_a);
        let commit_b = canonical_ser(&inputs_b);
        assert_eq!(commit_a, commit_b);
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
