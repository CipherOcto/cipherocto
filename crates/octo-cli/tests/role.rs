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

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn octo() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
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

/// TV-RX-2: `octo role select <unknown> --confirm` returns exit 31 (RoleNotFound).
#[test]
fn tv_rx_2_select_unknown_role_with_confirm_exit_31() {
    octo()
        .args(["role", "select", "nonexistent-role", "--confirm"])
        .assert()
        .code(31)
        .stderr(contains("role not found"));
}

/// TV-RX-3: `octo role select <existing> --confirm` succeeds with
/// envelope (last-writer-wins on re-select).
#[test]
fn tv_rx_3_select_existing_role_emits_envelope() {
    octo()
        .args(["role", "select", "builder", "--confirm"])
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
    octo()
        .args(["role", "select", "builder", "--confirm"])
        .assert()
        .code(0);
    octo()
        .args(["role", "select", "builder", "--confirm"])
        .assert()
        .code(0);
}

// ---------------------------------------------------------------------------
// TV-RP — proof (envelope schema verification)
// ---------------------------------------------------------------------------

/// TV-RP-1: select envelope contains `nonce` (per RFC-0011-d §7.4
/// envelope-build nonce field; M5 substrate contract).
#[test]
fn tv_rp_1_select_envelope_has_nonce_field() {
    octo()
        .args(["role", "select", "provider", "--confirm"])
        .assert()
        .code(0)
        .stdout(contains("\"nonce\""))
        .stdout(contains("\"role_binding_hash\""))
        .stdout(contains("\"body_hash\""))
        .stdout(contains("\"chain_id\""));
}
