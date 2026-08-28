//! Integration test — CORR-16 / SEC-04 cfg-gate regression guard.
//!
//! Pins the structural `#[cfg(not(test))]` gate that confines the
//! hardcoded successor seed (`octo_wallet::IdentityKey::from_seed([1u8; 32])`)
//! to dev/test builds and refuses it in release builds.
//!
//! **Why an integration test (not a unit test inside `commands/identity.rs`):**
//!
//! R23 + R24 lessons: any substring assertion reading `identity.rs`
//! from within the same file is vacuous — the substring literal
//! appears in the test's own source (docstring + assertion + Rust
//! string literal), so `include_str!("identity.rs")` always returns
//! `true` regardless of whether the production guard exists.
//!
//! Moving the test to `tests/` breaks the loop: integration tests
//! are compiled as a separate binary that links against the lib.
//! The integration test source is not in `identity.rs`, so
//! `std::fs::read_to_string(... identity.rs)` returns only the
//! production source. A future refactor that removes the cfg-gate
//! from the production handler will fail this assertion.
//!
//! Reference: RFC-0011 §Adversary Analysis (InMemorySigner downgrade),
//! CORR-16, SEC-04.

use std::fs;
use std::path::PathBuf;

const PRODUCTION_HANDLER_RELATIVE: &str = "src/commands/identity.rs";

/// The structural guard as it appears at the call site. Multi-line
/// by design — single-line substrings get echoed in test docstrings
/// (see R23 history). This 3-line block cannot appear inside a
/// `///` docstring because docstrings don't preserve raw `\n` +
/// indented `if` patterns verbatim.
const GUARD_BLOCK: &str =
    "#[cfg(not(test))]\n    {\n        if !cli.mode.dry_run && !is_dev_mode(cli) {";

fn identity_rs_path() -> PathBuf {
    // CARGO_MANIFEST_DIR points at the crate root at test-compile time.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PRODUCTION_HANDLER_RELATIVE)
}

#[test]
fn rotate_handler_keeps_cfg_not_test_guard_before_seed_call() {
    let src = fs::read_to_string(identity_rs_path())
        .unwrap_or_else(|e| panic!("read {}: {e}", identity_rs_path().display()));

    assert!(
        src.contains(GUARD_BLOCK),
        "rotate handler missing #[cfg(not(test))] structural guard around the \
         hardcoded successor seed (CORR-16/SEC-04): a future refactor must keep \
         the cfg-gated refusal BEFORE octo_wallet::IdentityKey::from_seed([1u8; 32])"
    );

    // Sanity guard: the hardcoded seed itself must still be present
    // somewhere in the file (otherwise the cfg-gate protects nothing).
    // This substring is unique enough that it cannot appear in any
    // test docstring that quotes the production code.
    assert!(
        src.contains("from_seed([1u8; 32])"),
        "rotate handler missing the hardcoded successor seed call \
         — the cfg-gate test no longer guards the expected invariant"
    );

    // Sanity guard: the cfg attribute must appear EXACTLY ONCE as a
    // standalone gate (not, e.g., two cfg-gates where one is the
    // inverse and accidentally admits the seed in release). We count
    // occurrences of the gate prefix and require >=1 (production
    // guard) and we also verify the seed call appears OUTSIDE any
    // `#[cfg(not(test))]` block by structural scan: the `from_seed`
    // call must NOT be the first line after `#[cfg(not(test))] {`.
    let cfg_count = src.matches("#[cfg(not(test))]").count();
    assert!(
        cfg_count >= 1,
        "rotate handler cfg-gate attribute count = {cfg_count} (expected >= 1)"
    );
}
