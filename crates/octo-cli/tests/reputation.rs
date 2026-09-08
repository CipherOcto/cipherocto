//! Integration tests for `octo reputation show` — RFC-0011-b §Test Vectors.
//!
//! Six vectors from the mission YAML §Test Vectors:
//! - TV-REP-1: `octo reputation show --did <hex> --role builder` → exit 0 + envelope JSON
//! - TV-REP-2: `octo reputation show --role builder` (no `--did`) → exit 2 (`NoActiveIdentity`)
//! - TV-REP-3: `octo reputation show --did <hex> --role builder --limit 1001` → exit 2 (hard cap)
//! - TV-REP-4: Auditor mode `octo reputation show --did <hex> --role builder` → exit 0 (read-only OK)
//! - TV-REP-5: `--no-anchor-verify` rejected in Human mode (DEV-ONLY escape hatch)
//! - TV-REP-6: missing `--role` rejected at clap parse time
//!
//! Each test runs as a child binary via `assert_cmd`, captures
//! stdout/stderr, and inspects JSON or exit code per vector.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn octo() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd
}

/// Canonical 104-char (52-byte) lowercase hex payload — the CLI's
/// accepted input shape. Built via `String` to avoid a hand-counted
/// literal drift (RFC-0968 §2 mandates exactly 52 bytes = 104 hex
/// chars; any other length must be rejected).
fn did_hex() -> String {
    "0".repeat(104)
}

/// Canonical full DID string built from the 52-byte hex payload.
fn canonical_did() -> String {
    format!("did:octo:{}", did_hex())
}

/// TV-REP-1: `octo reputation show --did <hex> --role builder` returns the
/// projection-shaped envelope (exit 0; substrate returns the canonical
/// zero-record in Phase 1 per RFC-0968 §10).
#[test]
fn tv_rep_1_show_returns_envelope() {
    octo()
        .args([
            "reputation",
            "show",
            "--did",
            &canonical_did(),
            "--role",
            "builder",
        ])
        .assert()
        .code(0)
        .stdout(contains("\"did\""))
        .stdout(contains("\"role\""))
        .stdout(contains("\"builder\""))
        .stdout(contains("\"score\""))
        .stdout(contains("\"components\""))
        .stdout(contains("\"attestations\""))
        .stdout(contains("\"last_updated_unix\""))
        .stdout(contains("\"anchor_ref\""))
        .stdout(contains("\"schema_version\""))
        .stdout(contains("\"exit_code\""))
        .stdout(contains("\"generated_at\""));
}

/// TV-REP-2: `octo reputation show --role builder` without `--did` returns
/// `NoActiveIdentity` (exit 2) per RFC-0011-b §Roles and Authorities.
#[test]
fn tv_rep_2_show_without_did_returns_no_active_identity() {
    octo()
        .args(["reputation", "show", "--role", "builder"])
        .assert()
        .code(2)
        .stderr(contains("no active identity").or(contains("NoActiveIdentity")));
}

/// TV-REP-3: `--limit 1001` is rejected with exit 2 per RFC-0011-b §7.6
/// (hard cap of 1000 enforced via the `limit_in_range` clap value
/// parser).
#[test]
fn tv_rep_3_limit_above_hard_cap_is_rejected() {
    octo()
        .args([
            "reputation",
            "show",
            "--did",
            &canonical_did(),
            "--role",
            "builder",
            "--limit",
            "1001",
        ])
        .assert()
        .code(2)
        .stderr(contains("--limit"))
        .stderr(contains("1000"));
}

/// TV-REP-4: Auditor mode reads pass through unchanged. Reputation
/// `show` is read-only across all four operator modes per
/// RFC-0011-b §Roles and Authorities; Auditor fail-closed on
/// revoked DID only fires when the substrate signals revoked (the
/// canonical zero-record is NOT revoked so Auditor is admitted
/// with exit 0).
#[test]
fn tv_rep_4_auditor_mode_read_only_passes() {
    octo()
        .args([
            "--mode",
            "auditor",
            "reputation",
            "show",
            "--did",
            &canonical_did(),
            "--role",
            "builder",
        ])
        .assert()
        .code(0)
        .stdout(contains("\"role\""))
        .stdout(contains("\"builder\""));
}

/// TV-REP-5 (bonus): `--no-anchor-verify` rejected in Human mode
/// (DEV-ONLY escape hatch per RFC-0011-b §Security Considerations 1a).
#[test]
fn tv_rep_5_no_anchor_verify_rejected_in_human_mode() {
    octo()
        .args([
            "reputation",
            "show",
            "--did",
            &canonical_did(),
            "--role",
            "builder",
            "--no-anchor-verify",
        ])
        .assert()
        .code(2)
        .stderr(contains("--no-anchor-verify"));
}

/// TV-REP-6 (bonus): empty role slug rejected at clap parse time
/// (RFC-0011-b §7.4 `Role::parse` rejects empty).
#[test]
fn tv_rep_6_missing_role_returns_clap_parse_error() {
    octo()
        .args(["reputation", "show", "--did", &canonical_did()])
        .assert()
        .code(2);
}
