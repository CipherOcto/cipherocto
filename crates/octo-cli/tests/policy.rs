//! `octo policy` integration tests — RFC-0011 §Test Vectors (policy group).
//!
//! 6 TV per mission 0011-policy-commands AC:
//!   TV-POL1 — show with no version (latest_version → NotFound → exit 13)
//!   TV-POL2 — show with unknown name (substrate NotFound → exit 13)
//!   TV-POL3 — list with filter (empty result, schema_version present)
//!   TV-POL4 — show with explicit --version (substrate NotFound → exit 13)
//!   TV-POL5 — list empty (no filter, empty result)
//!   TV-POL6 — list with invalid filter (CLI parse → exit 16)
//!
//! NOTE: The `octo-policy` substrate stub returns `NotFound` for every
//! `show` / `latest_version` call, so the "success" vectors here observe
//! the stub-returned `PolicyNotFound` (exit 13). When the substrate is
//! wired up per RFC-0967, TV-POL1 / TV-POL4 should flip to exit 0 and
//! exit 14 respectively per mission AC. The TV-POL4 → exit-13 assertion
//! is the stub-correct form per the implementation guide.

use assert_cmd::Command;
use predicates::str as pred_str;

fn octo() -> Command {
    Command::cargo_bin("octo").unwrap()
}

#[test]
fn tv_pol1_show_default_version() {
    // No --version → CLI calls `latest_version`, which the substrate stub
    // returns as `NotFound(name)`. CLI maps to `PolicyNotFound` (exit 13).
    octo()
        .args(["--json", "policy", "show", "rate_limit"])
        .assert()
        .code(13)
        .stderr(pred_str::contains("policy not found"));
}

#[test]
fn tv_pol2_show_not_found() {
    // Substrate stub returns `NotFound` for any unknown name.
    octo()
        .args(["--json", "policy", "show", "no_such_policy"])
        .assert()
        .code(13)
        .stderr(pred_str::contains("policy not found"));
}

#[test]
fn tv_pol3_list_filter() {
    // `parse_filter("kind=rate_limit")` parses cleanly; substrate `list`
    // stub returns `Ok(Vec::new())`; CLI renders the envelope.
    octo()
        .args(["--json", "policy", "list", "--filter", "kind=rate_limit"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("schema_version"))
        .stdout(pred_str::contains(r#""policies":[]"#));
}

#[test]
fn tv_pol4_show_version_mismatch() {
    // Explicit `--version 999` skips `latest_version`. The substrate `show`
    // stub returns `NotFound(name)` for any lookup; CLI maps to
    // `PolicyNotFound` (exit 13). Mission AC expects exit 14 once the
    // substrate is wired (CLI will then surface `VersionMismatch` → exit 14).
    octo()
        .args(["--json", "policy", "show", "rate_limit", "--version", "999"])
        .assert()
        .code(13)
        .stderr(pred_str::contains("policy not found"));
}

#[test]
fn tv_pol5_list_empty() {
    // No filter → `PolicyFilter::default()`; substrate `list` stub returns
    // an empty list; CLI renders `"policies":[]`.
    octo()
        .args(["--json", "policy", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains(r#""policies":[]"#));
}

#[test]
fn tv_pol6_invalid_filter() {
    // `--filter bogus` lacks `=`, so `parse_filter` rejects with
    // `InvalidFilter` (CLI-side, exit 16).
    octo()
        .args(["--json", "policy", "list", "--filter", "bogus"])
        .assert()
        .code(16)
        .stderr(pred_str::contains("invalid filter"));
}

#[test]
fn tv_pol7_unknown_filter_key() {
    // `parse_filter("unknown=value")` rejects with `InvalidFilter`.
    octo()
        .args(["--json", "policy", "list", "--filter", "unknown=value"])
        .assert()
        .code(16)
        .stderr(pred_str::contains("invalid filter"));
}

#[test]
fn tv_pol8_kind_and_class_filter_parses() {
    // Both `kind=` and `class=` keys are accepted; substrate `list` stub
    // still returns empty, but the filter must not error out.
    octo()
        .args([
            "--json",
            "policy",
            "list",
            "--filter",
            "kind=rate_limit,class=high",
        ])
        .assert()
        .code(0)
        .stdout(pred_str::contains("schema_version"));
}
