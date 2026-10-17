//! Integration test — CORR-16 / SEC-04 cfg-gate regression guard.
//!
//! Pins the structural `#[cfg(not(test))]` gate that confines the
//! hardcoded successor seed (`octo_wallet::IdentityKey::from_seed([1u8; 32])`)
//! in `commands/identity.rs` AND the hardcoded root_secret
//! (`octo_cap_macaroon::mint(&[0u8; 32], ...)`) in
//! `commands/capability.rs` to dev/test builds; release builds must
//! refuse both via `is_dev_mode(cli)`.
//!
//! **Why an integration test (not a unit test inside `commands/identity.rs`):**
//!
//! R23 + R24 lessons: any substring assertion reading identity.rs (or
//! capability.rs) from within the same file is vacuous — the
//! substring literal appears in the test's own source (docstring +
//! assertion + Rust string literal), so `include_str!(...)` always
//! returns `true` regardless of whether the production guard exists.
//!
//! Moving the test to `tests/` breaks the loop: integration tests
//! are compiled as a separate binary that links against the lib.
//! The integration test source is not in the production files, so
//! `std::fs::read_to_string(...)` returns only the production source.
//!
//! **Why order-based assertion (R25 lesson):**
//!
//! Earlier rounds (R23, R24) tried pinning the exact 3-line
//! structural substring (`#[cfg(not(test))] {\n    if !cli.mode.dry_run ...`).
//! Brittle: a legitimate refactor that moves the guard into a
//! closure or helper function with different indentation would fail
//! the test even though the gate is structurally correct. The
//! order-based check (`cfg(not(test))` appears in source BEFORE the
//! seed call) pins the structural relationship, not the formatting.
//!
//! Reference: RFC-0011 §Adversary Analysis (InMemorySigner downgrade),
//! CORR-16, SEC-04.

use std::fs;
use std::path::PathBuf;

/// Find a file at `<crate root>/<relative>` via `CARGO_MANIFEST_DIR`.
fn read_crate_source(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Pin the structural relationship: a `#[cfg(not(test))]` attribute
/// must appear in source order BEFORE the well-known public seed
/// call. If the cfg-gate is removed, the `cfg` position is `None`
/// or appears after the seed call, and the assertion fires.
fn assert_cfg_gate_before_seed(source: &str, seed_call: &str, file_label: &str) {
    let cfg_pos = source.find("#[cfg(not(test))]");
    let seed_pos = source.find(seed_call);
    assert!(
        cfg_pos.is_some(),
        "{file_label}: missing #[cfg(not(test))] structural gate \
         (CORR-16/SEC-04) — release builds must refuse hardcoded seed/root paths"
    );
    assert!(
        seed_pos.is_some(),
        "{file_label}: missing well-known seed call `{seed_call}` \
         — cfg-gate test no longer guards the expected invariant"
    );
    let cfg_pos = cfg_pos.expect("checked above");
    let seed_pos = seed_pos.expect("checked above");
    assert!(
        cfg_pos < seed_pos,
        "{file_label}: #[cfg(not(test))] appears at offset {cfg_pos}, \
         but `{seed_call}` appears at offset {seed_pos} — the cfg-gate \
         must precede the seed call in source order so release builds \
         (which exclude the cfg-gated block) cannot reach the seed call"
    );
}

#[test]
fn identity_rotate_cfg_gate_precedes_hardcoded_successor_seed() {
    let src = read_crate_source("src/commands/identity.rs");
    assert_cfg_gate_before_seed(&src, "from_seed([1u8; 32])", "commands/identity.rs");
}

#[test]
fn capability_mint_cfg_gate_precedes_hardcoded_root_secret() {
    let src = read_crate_source("src/commands/capability.rs");
    // The capability mint dev path mints with `&[0u8; 32]` as the
    // root_secret placeholder. Same structural gate as identity
    // rotate: a `#[cfg(not(test))]` block must precede this call so
    // release builds refuse it via the `is_dev_mode` check.
    assert_cfg_gate_before_seed(
        &src,
        "octo_cap_macaroon::mint(&[0u8; 32]",
        "commands/capability.rs",
    );
}
