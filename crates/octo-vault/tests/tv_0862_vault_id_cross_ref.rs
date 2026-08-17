//! TV-0862 vault_id derivation cross-ref — per RFC-0862 §StoolapSpendLedger.
//!
//! Pin the canonical BLAKE3 derivation that the spend_ledger substrate
//! cross-references (per RFC-0862 §StoolapSpendLedger `Vault row
//! cross-ref`). Moved here from `quota-router-storage/tests/tv_0862_spend_ledger.rs`
//! after S6c Round 1 review (code review #4 — the substrate does not
//! own `vault_id`; it belongs in `octo-vault/tests/` next to the
//! existing `vault_id_uses_canonical_blake3_derivation` test).
//!
//! Pins:
//! - Domain-separated BLAKE3 prefix `"cipherocto/vault/v1/"`
//! - 32-byte output digest (regression: empty-input BLAKE3)
//! - Determinism across rehash (regression: hash-instance RNG)
//!
//! Uses `octo_vault::vault_id(...)` so a prefix drift in the
//! production derivation is caught (TV-0862 was a tautology when it
//! recomputed BLAKE3 locally without calling the production fn).

use octo_vault::{vault_id, AssetId, ChainId};

// =============================================================================
// Fixtures (byte-pinned constants)
// =============================================================================

/// 32-byte chain_id fixture (zero-distinct).
const TV_0862_CHAIN_ID: [u8; 32] = [
    0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0,
    0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0,
];

/// 32-byte asset_id fixture (production shape; pinned to fixed-32 so
/// `owner_did || asset_id` boundary is unambiguous per
/// S6c Round 1 security finding #2).
const TV_0862_ASSET_ID: [u8; 32] = [
    0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F, 0x80,
    0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F, 0x90,
];

/// TV-0862 vault_id derivation cross-ref: production `octo_vault::vault_id`
/// must produce a 32-byte non-zero digest for the canonical input
/// shape. Drift in the BLAKE3 domain prefix breaks the
/// vault_id ↔ spend_ledger binding.
#[test]
fn tv_0862_vault_id_cross_ref_production_derivation() {
    let owner_did: &str = "did:octo:zVaultId0862";

    let vid = vault_id(
        ChainId(TV_0862_CHAIN_ID),
        owner_did,
        AssetId(TV_0862_ASSET_ID),
    );

    assert_eq!(
        vid.0.len(),
        32,
        "TV-0862: vault_id MUST be exactly 32 bytes (regression: empty-input BLAKE3)"
    );
    assert_ne!(
        vid.0, [0u8; 32],
        "TV-0862: vault_id MUST be non-zero for canonical input (regression: all-zero digest)"
    );
}

/// Determinism: identical inputs MUST yield identical vault_id
/// across calls (regression: hash-instance RNG, mutable hasher state).
#[test]
fn tv_0862_vault_id_cross_ref_deterministic() {
    let owner_did: &str = "did:octo:zVaultId0862";

    let a = vault_id(
        ChainId(TV_0862_CHAIN_ID),
        owner_did,
        AssetId(TV_0862_ASSET_ID),
    );
    let b = vault_id(
        ChainId(TV_0862_CHAIN_ID),
        owner_did,
        AssetId(TV_0862_ASSET_ID),
    );
    assert_eq!(
        a.0, b.0,
        "TV-0862: vault_id derivation MUST be deterministic across calls"
    );
}

/// Domain separation: vault_id derivation MUST NOT collide with
/// adjacent derivation prefixes. This test guards against an accidental
/// `cipherocto/vault/v1/` prefix drift (e.g. typo to
/// `cipherocto/vautl/v1/`) by asserting the production derivation
/// differs from a one-character-prefix-typo derivation.
///
/// **Scope (S6c Round 2 code review LOW #6):** this is a single-point
/// drift guard, NOT a full sweep over adjacent derivation prefixes
/// (e.g. `cipherocto/macaroon/v1/`, `cipherocto/chain/v1/`,
/// `cipherocto/asset/v1/`, `cipherocto/cap/v1/*`). Those sweep
/// assertions are filed as follow-on `0862-c5-domain-sep` (untagged
/// hash prefix hygiene mission).
#[test]
fn tv_0862_vault_id_cross_ref_domain_separation() {
    let owner_did: &str = "did:octo:zVaultId0862";

    // Production derivation.
    let expected = vault_id(
        ChainId(TV_0862_CHAIN_ID),
        owner_did,
        AssetId(TV_0862_ASSET_ID),
    );

    // Recompute the canonical input locally (mirrors what the
    // production fn does internally, with the EXACT production prefix).
    let prefix = b"cipherocto/vault/v1/";
    let owner_did_bytes = owner_did.as_bytes();
    let mut input = Vec::with_capacity(
        prefix.len() + TV_0862_CHAIN_ID.len() + owner_did_bytes.len() + TV_0862_ASSET_ID.len(),
    );
    input.extend_from_slice(prefix);
    input.extend_from_slice(&TV_0862_CHAIN_ID);
    input.extend_from_slice(owner_did_bytes);
    input.extend_from_slice(&TV_0862_ASSET_ID);

    let raw: [u8; 32] = blake3::hash(&input).into();
    assert_eq!(
        raw, expected.0,
        "TV-0862: production vault_id MUST match the documented BLAKE3 derivation (regression: prefix drift in octo-vault)"
    );

    // Cross-prefix sanity: changing the prefix byte must change the
    // digest. Catches the typo class of bug.
    let mut typo_input = input.clone();
    typo_input[11] = b'w'; // cipherocto/wa ult/v1/ typo (v→w)
    let typo: [u8; 32] = blake3::hash(&typo_input).into();
    assert_ne!(
        typo, expected.0,
        "TV-0862: prefix typo MUST change digest (sanity: BLAKE3 prefix is not no-op)"
    );
}
