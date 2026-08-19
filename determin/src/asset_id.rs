//! Asset ID derivation per RFC-0105 §Asset ID Derivation.
//!
//! [`asset_id_for`] computes the canonical 32-byte `asset_id` from a
//! role-token string per the review doc §20.3.1 derivation rule.
//!
//! ## Derivation
//!
//! ```text
//! asset_id = BLAKE3-256("cipherocto/asset/v1/" || role_token_bytes)[:32]
//! ```
//!
//! Domain-separated: namespace `cipherocto/asset/v1/` guarantees
//! future-version (`v2`, `v3`, ...) asset ID collision impossibility
//! (per RFC-0105 v2.0 §Asset ID Derivation).
//!
//! ## 9 role-token enumeration
//!
//! Mission 0105-v review §1336 cross-section reconciliation pins the 9
//! specialized role-tokens (Sovereign `OCTO` excluded — separately
//! handled per cross-layer capability-attestation path):
//!
//! - `OCTO-A` → AI Compute
//! - `OCTO-B` → Bandwidth
//! - `OCTO-D` → Developers
//! - `OCTO-M` → Marketing
//! - `OCTO-N` → Node Operators
//! - `OCTO-O` → Orchestrator
//! - `OCTO-S` → Storage
//! - `OCTO-H` → Historical
//! - `OCTO-W` → AI Wholesale
//!
//! ## Layer placement
//!
//! Per `cipherocto-design-principles.md` Layer A row: this function
//! lives in the frozen substrate (RFC-frozen, semver-major only). The
//! RFC-0105 v2.0 amendment mandates the canonical derivation rule +
//! namespace + 9-token enumeration; any future change requires a
//! semver-major bump (e.g. `v3` namespace bump for asset ID collision
//! avoidance across ecosystem forks).

/// Canonical asset-ID derivation domain prefix for RFC-0105 v2.0.
///
/// Future version bumps (`v2`, `v3`, ...) MUST change this string to
/// guarantee collision-free asset IDs across ecosystem forks. Any
/// change to this constant is a semver-major break for `octo-determin`.
pub const ASSET_ID_DOMAIN_V1: &[u8] = b"cipherocto/asset/v1/";

/// Canonical role-token string for the 9 enumeration slots. Re-exported
/// at crate root via `pub use asset_id::*`.
///
/// Sovereign `OCTO` excluded — see [`asset_id_for`] doc + review §1336
/// cross-section reconciliation. Future token additions (e.g., `OCTO-X`)
/// expand the TV-D9 fixture count (separate bump owed).
pub const ROLE_TOKENS: &[&str] = &[
    "OCTO-A", // AI Compute
    "OCTO-B", // Bandwidth
    "OCTO-D", // Developers
    "OCTO-M", // Marketing
    "OCTO-N", // Node Operators
    "OCTO-O", // Orchestrator
    "OCTO-S", // Storage
    "OCTO-H", // Historical
    "OCTO-W", // AI Wholesale
];

/// Compute the canonical `asset_id` for a role-token string.
///
/// Derivation: `BLAKE3-256("cipherocto/asset/v1/" || role_token)`.
/// Output is a 32-byte array — BLAKE3-256 output truncated to 32 bytes
/// (the output size IS 32 bytes; the slice notation is defensive).
///
/// # Determinism
///
/// Pure function: same `role_token` → same 32-byte sequence, on every
/// platform, forever. The BLAKE3 implementation is the workspace-shared
/// `blake3 = "1.5"` dep (matches `octo-vault::AssetId::derive`).
///
/// # Layer
///
/// RFC-0105 v2.0 §Asset ID Derivation — lives in Layer A frozen
/// substrate. The 9-role-token enumeration (see [`ROLE_TOKENS`]) pins
/// the canonical names; future additions land in a v1.1 enumeration
/// extension or a v2 namespace bump.
#[must_use]
pub fn asset_id_for(role_token: &str) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(ASSET_ID_DOMAIN_V1);
    h.update(role_token.as_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TV-D9 smoke: 9 role-tokens × `asset_id_for` yields deterministic
    /// 32-byte sequences. Byte-exact anchors are pinned by the central
    /// registry in `crates/octo-vault/tests/test_vectors.rs::tv_d9_asset_id`;
    /// these smoke tests assert the function shape (32 bytes, non-zero
    /// for at least one role-token, distinct bytes across roles).
    #[test]
    fn asset_id_for_returns_32_bytes() {
        for rt in ROLE_TOKENS {
            let id = asset_id_for(rt);
            assert_eq!(id.len(), 32, "asset_id_for MUST be 32 bytes");
            // Sanity: BLAKE3 of non-empty input yields non-zero output
            // (probabilistically guaranteed; if it ever fails, BLAKE3
            // itself is broken).
            assert!(
                id.iter().any(|&b| b != 0),
                "asset_id_for({rt:?}) MUST yield non-zero bytes"
            );
        }
    }

    #[test]
    fn asset_id_for_is_deterministic_per_role_token() {
        let a1 = asset_id_for("OCTO-A");
        let a2 = asset_id_for("OCTO-A");
        assert_eq!(a1, a2, "asset_id_for MUST be deterministic");
    }

    #[test]
    fn asset_id_for_distinct_role_tokens_yield_distinct_bytes() {
        // Pairwise distinct — different role-tokens MUST NOT collide
        // (BLAKE3 collision probability is 2^-128 per pair; this test
        // is exact-byte, not probabilistic).
        let mut seen = std::collections::HashSet::new();
        for rt in ROLE_TOKENS {
            let id = asset_id_for(rt);
            assert!(
                seen.insert(id),
                "asset_id_for({rt:?}) MUST be distinct from all prior role-tokens"
            );
        }
    }

    #[test]
    fn asset_id_for_anonymous_empty_string_still_yields_32_bytes() {
        // Future-proofing: empty `role_token` yields a valid
        // `asset_id_for("")` (BLAKE3 hashes empty input fine). Per
        // review §1362 cross-section, the canonical 9-role-token
        // enumeration excludes non-role-token strings (e.g.,
        // `"team-budget"`); consumers MUST gate on `ROLE_TOKENS`
        // membership before calling `asset_id_for` for
        // mission-critical identity paths.
        let id = asset_id_for("");
        assert_eq!(id.len(), 32);
    }

    #[test]
    fn role_tokens_enumeration_is_nine() {
        assert_eq!(
            ROLE_TOKENS.len(),
            9,
            "ROLE_TOKENS MUST be the canonical 9 per review §1336 (Sovereign OCTO excluded)"
        );
    }
}
