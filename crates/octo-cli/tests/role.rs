//! Integration tests for `octo role` — RFC-0011-d §11 Test Vectors.
//!
//! 11 vectors from §11:
//! - TV-RL-1..3 (list)
//! - TV-RS-1..3 (show)
//! - TV-RX-1..4 (select)
//! - TV-RP-1 (proof = envelope schema verification)
//!
//! Each test runs as a child binary via `assert_cmd`, captures
//! stdout/stderr, and inspects JSON or exit code per vector.
//!
//! `role select` tests pass `--dev` so the in-memory signer stub is
//! admitted (R12 HIGH-9: HSM downgrade vector is blocked; the dev
//! stub requires explicit dev-mode opt-in).

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn octo() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd
}

/// `octo ... --dev` — selects admit the in-memory signer stub.
fn octo_dev() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd.arg("--dev");
    cmd
}

/// `octo ... --dev --confirm --confirm-acknowledge` — selects pass the
/// pastejacking-defense two-step gate (Wave 2 HIGH-1: `role::select_role`
/// routes through `require_confirm`, which requires BOTH flags for
/// Human mode).
fn octo_dev_select() -> Command {
    let mut cmd = octo_dev();
    cmd.arg("--confirm");
    cmd.arg("--confirm-acknowledge");
    cmd
}

// ---------------------------------------------------------------------------
// TV-RL — list vectors
// ---------------------------------------------------------------------------

/// TV-RL-1: `octo role list` returns 7 canonical base roles.
#[test]
fn tv_rl_1_list_returns_seven_base_roles() {
    octo()
        .args(["role", "list"])
        .assert()
        .code(0)
        .stdout(contains("builder"))
        .stdout(contains("provider"))
        .stdout(contains("storage"))
        .stdout(contains("bandwidth"))
        .stdout(contains("orchestrator"))
        .stdout(contains("recorder"))
        .stdout(contains("wallet"));
}

/// TV-RL-2: `octo role list --kind provider` returns only `provider`.
#[test]
fn tv_rl_2_list_filter_kind() {
    octo()
        .args(["role", "list", "--kind", "provider"])
        .assert()
        .code(0)
        .stdout(contains("provider"))
        .stdout(contains("\"builder\"").not());
}

/// TV-RL-3: `octo role list --kind nonexistent` returns empty.
#[test]
fn tv_rl_3_list_filter_kind_no_match() {
    octo()
        .args(["role", "list", "--kind", "nonexistent"])
        .assert()
        .code(0)
        .stdout(contains("[]").or(contains("[],")).or(contains("\"\"")));
}

// ---------------------------------------------------------------------------
// TV-RS — show vectors
// ---------------------------------------------------------------------------

/// TV-RS-1: `octo role show builder` returns the builder record.
#[test]
fn tv_rs_1_show_existing_role() {
    octo()
        .args(["role", "show", "builder"])
        .assert()
        .code(0)
        .stdout(contains("builder"))
        .stdout(contains("OCTO-A"));
}

/// TV-RS-2: `octo role show <unknown>` returns exit 31 (RoleNotFound).
#[test]
fn tv_rs_2_show_unknown_role_exit_31() {
    octo()
        .args(["role", "show", "nonexistent-role"])
        .assert()
        .code(31)
        .stderr(contains("role not found"));
}

/// TV-RS-3: `octo role show` without role_id exits 2 (clap parse).
#[test]
fn tv_rs_3_show_missing_role_id_exit_2() {
    octo().args(["role", "show"]).assert().code(2);
}

// ---------------------------------------------------------------------------
// TV-RX — select vectors
// ---------------------------------------------------------------------------

/// TV-RX-1: `octo role select <unknown>` without --confirm returns
/// exit 2 (ConfirmationRequired).
#[test]
fn tv_rx_1_select_unknown_role_no_confirm_exit_2() {
    octo()
        .args(["role", "select", "nonexistent-role"])
        .assert()
        .code(2)
        .stderr(contains("--confirm"));
}

/// TV-RX-2: `octo role select <unknown> --confirm --confirm-acknowledge`
/// returns exit 31 (RoleNotFound). Wave 2 HIGH-1: Human mode now
/// requires both confirm flags (pastejacking defense).
#[test]
fn tv_rx_2_select_unknown_role_with_confirm_exit_31() {
    octo_dev_select()
        .args(["role", "select", "nonexistent-role"])
        .assert()
        .code(31)
        .stderr(contains("role not found"));
}

/// TV-RX-3: `octo role select <existing>` (with pastejacking-defense
/// gate) succeeds with envelope (last-writer-wins on re-select).
#[test]
fn tv_rx_3_select_existing_role_emits_envelope() {
    octo_dev_select()
        .args(["role", "select", "builder"])
        .assert()
        .code(0)
        .stdout(contains("\"role_id\":\"builder\""))
        .stdout(contains("\"operator_did\""))
        .stdout(contains("\"signature_proof\""))
        .stdout(contains("\"role_binding_hash\""));
}

/// TV-RX-4: re-selecting the same role overwrites silently
/// (last-writer-wins; no RoleBindingConflict variant).
#[test]
fn tv_rx_4_reselect_last_writer_wins_no_conflict() {
    // Two consecutive selects; second must succeed (last-writer-wins).
    octo_dev_select()
        .args(["role", "select", "builder"])
        .assert()
        .code(0);
    octo_dev_select()
        .args(["role", "select", "builder"])
        .assert()
        .code(0);
}

/// TV-RX-5: `octo role select --mode auditor` is denied (Wave 2 HIGH-2
/// regression vector). Auditor is a read-only role; the select
/// write-path must surface `AuditorDenied` (exit 2) — NOT the generic
/// `--confirm required` message (Wave 3 LOW functional: adding
/// `--confirm` does not unblock an Auditor session).
#[test]
fn tv_rx_5_select_auditor_mode_denied() {
    octo_dev()
        .args([
            "--mode",
            "auditor",
            "--confirm",
            "--confirm-acknowledge",
            "role",
            "select",
            "builder",
        ])
        .assert()
        .code(2)
        .stderr(contains("auditor mode is read-only"))
        .stderr(contains("role select"));
}

/// TV-RX-6: `octo role select --mode auditor --dry-run` is STILL
/// denied (Wave 3 LOW coverage). The Auditor short-circuit fires
/// before the `--dry-run` bypass; auditors must not preview mutations.
#[test]
fn tv_rx_6_select_auditor_with_dry_run_still_denied() {
    octo_dev()
        .args([
            "--mode",
            "auditor",
            "--dry-run",
            "role",
            "select",
            "builder",
        ])
        .assert()
        .code(2)
        .stderr(contains("auditor mode is read-only"));
}

/// TV-RX-7: `octo role select --mode ci --allow-write --dev ...`
/// succeeds (Wave 3 LOW coverage). CI mode admits via `--allow-write`
/// instead of `--confirm`/`--confirm-acknowledge`. The `--dev` flag
/// is also required for the Phase 1 in-memory signer stub.
#[test]
fn tv_rx_7_select_ci_mode_with_allow_write_succeeds() {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd.args([
        "--mode",
        "ci",
        "--allow-write",
        "--dev",
        "role",
        "select",
        "wallet",
    ]);
    cmd.assert()
        .code(0)
        .stdout(contains("\"role_id\":\"wallet\""));
}

// ---------------------------------------------------------------------------
// TV-RP — proof (envelope schema verification)
// ---------------------------------------------------------------------------

/// TV-RP-1: select envelope contains `nonce` (per RFC-0011-d §7.4
/// envelope-build nonce field; M5 substrate contract) AND the
/// `signature_proof` field renders as `[REDACTED:sig]` (per the CLI
/// redaction boundary — substrate bytes never escape to operator
/// output). Wave 2 LOW finding: the previous assertion only checked
/// field NAMES; the redaction boundary could be silently broken and
/// TV-RP-1 would still pass.
#[test]
fn tv_rp_1_select_envelope_has_nonce_field() {
    let output = octo_dev_select()
        .args(["role", "select", "provider"])
        .assert()
        .code(0)
        .stdout(contains("\"nonce\""))
        .stdout(contains("\"role_binding_hash\""))
        .stdout(contains("\"body_hash\""))
        .stdout(contains("\"chain_id\""))
        .stdout(contains("\"signature_proof\":\"[REDACTED:sig]\""))
        .stdout(contains("\"role_binding_hash\":\"[REDACTED:sig]\""))
        .stdout(contains("\"body_hash\":\"[REDACTED:sig]\""))
        .stdout(contains("\"chain_id\":\"[REDACTED:sig]\""))
        .stdout(contains("\"role_kind_uuid\":\"[REDACTED:sig]\""))
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.contains("\"nonce\":"),
        "nonce field must serialize as a number-prefixed key: {stdout}"
    );
    // Defense-in-depth — the raw 64-byte signature must NEVER escape
    // the redaction boundary. Assert no `0x`-prefixed hex fragments
    // and no plaintext 4-byte marker that would betray a leaked byte.
    assert!(
        !stdout.contains("0xdeadbeef") && !stdout.contains("\"signature_proof\":\"0x"),
        "no raw hex material must leak into select envelope: {stdout}"
    );
}
