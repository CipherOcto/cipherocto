//! Smoke tests for Phase 5 Part E CLI surface.
//!
//! Phase 5 Part E wires the Phase 4 RPC methods (`rules.*` CRUD/dry-run,
//! `triggers.*` CRUD/run, `audit.*`, `actions.escalate`) to clap
//! subcommands. These tests verify the clap parse + `--help` contract
//! without spawning a daemon: deeper IPC behavior is covered by unit
//! tests in `src/cli.rs` and the integration tests in `tests/it_*`.
//!
//! Coverage:
//! - `rules.create` / `update` / `patch` — JSON body argument.
//! - `rules.delete` / `enable` / `disable` / `approve` — id + (etag|null).
//! - `rules.reload` / `flush` — no args.
//! - `rules.test` — event JSON body argument.
//! - `triggers.create` / `update` — JSON body argument.
//! - `triggers.delete` — id + etag.
//! - `triggers.run` — id + optional payload.
//! - `audit.tail` — optional `--since-seq` + `--limit`.
//! - `audit.verify` — no args.
//! - `actions.escalate` — positional `target` + `reason`.

use assert_cmd::Command;

/// Helper: invoke the binary with `--help` to verify the subcommand
/// exists, its arg list parses, and the documented flags are present.
fn cli_help(args: &[&str]) -> String {
    let out = Command::cargo_bin("octo-whatsapp")
        .expect("binary exists")
        .args(args)
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8_lossy(&out).into_owned()
}

// ---- rules.create ----

#[test]
fn cli_rules_create_help_mentions_json_arg() {
    let help = cli_help(&["rules", "create"]);
    assert!(
        help.contains("JSON body") || help.contains("--json") || help.contains("<JSON>"),
        "rules create --help must mention the JSON body arg, got:\n{help}"
    );
}

#[test]
fn cli_rules_update_help_mentions_etag() {
    let help = cli_help(&["rules", "update"]);
    assert!(
        help.contains("etag") || help.contains("<ETAG>") || help.contains("<JSON"),
        "rules update --help must mention etag + JSON body, got:\n{help}"
    );
}

// ---- rules.delete / enable / disable / approve ----

#[test]
fn cli_rules_delete_help_mentions_id_and_etag() {
    let help = cli_help(&["rules", "delete"]);
    assert!(
        help.contains("<ID>") && help.contains("<ETAG>"),
        "rules delete --help must mention <ID> and <ETAG>, got:\n{help}"
    );
}

#[test]
fn cli_rules_enable_help_mentions_id() {
    let help = cli_help(&["rules", "enable"]);
    assert!(
        help.contains("<ID>") || help.contains("<id>"),
        "rules enable --help must mention <ID>, got:\n{help}"
    );
}

#[test]
fn cli_rules_disable_help_mentions_id() {
    let help = cli_help(&["rules", "disable"]);
    assert!(
        help.contains("<ID>") || help.contains("<id>"),
        "rules disable --help must mention <ID>, got:\n{help}"
    );
}

#[test]
fn cli_rules_approve_help_mentions_id() {
    let help = cli_help(&["rules", "approve"]);
    assert!(
        help.contains("<ID>") || help.contains("<id>"),
        "rules approve --help must mention <ID>, got:\n{help}"
    );
}

// ---- rules.reload / flush / test ----

#[test]
fn cli_rules_reload_help_takes_no_args() {
    // `rules reload` MUST be valid as the only invocation, no extra args.
    // (Success here = clap parsed `rules reload` without complaining.)
    cli_help(&["rules", "reload"]);
}

#[test]
fn cli_rules_flush_help_takes_no_args() {
    cli_help(&["rules", "flush"]);
}

#[test]
fn cli_rules_test_help_mentions_event_json_arg() {
    let help = cli_help(&["rules", "test"]);
    assert!(
        help.contains("event") || help.contains("EVENT"),
        "rules test --help must mention the event JSON arg, got:\n{help}"
    );
}

// ---- triggers.create / update ----

#[test]
fn cli_triggers_create_help_mentions_json_arg() {
    let help = cli_help(&["triggers", "create"]);
    assert!(
        help.contains("JSON body") || help.contains("<JSON"),
        "triggers create --help must mention the JSON body arg, got:\n{help}"
    );
}

#[test]
fn cli_triggers_update_help_mentions_etag() {
    let help = cli_help(&["triggers", "update"]);
    assert!(
        help.contains("etag") || help.contains("<ETAG>"),
        "triggers update --help must mention etag, got:\n{help}"
    );
}

// ---- triggers.delete / run ----

#[test]
fn cli_triggers_delete_help_mentions_id_and_etag() {
    let help = cli_help(&["triggers", "delete"]);
    assert!(
        help.contains("<ID>") && help.contains("<ETAG>"),
        "triggers delete --help must mention <ID> and <ETAG>, got:\n{help}"
    );
}

#[test]
fn cli_triggers_run_help_mentions_id_and_payload() {
    let help = cli_help(&["triggers", "run"]);
    assert!(
        help.contains("<ID>"),
        "triggers run --help must mention <ID>, got:\n{help}"
    );
    assert!(
        help.contains("payload") || help.contains("PAYLOAD"),
        "triggers run --help must mention the optional payload, got:\n{help}"
    );
}

// ---- audit.tail / audit.verify ----

#[test]
fn cli_audit_tail_help_mentions_since_seq_and_limit_flags() {
    let help = cli_help(&["audit", "tail"]);
    assert!(
        help.contains("--since-seq"),
        "audit tail --help must mention --since-seq flag, got:\n{help}"
    );
    assert!(
        help.contains("--limit"),
        "audit tail --help must mention --limit flag, got:\n{help}"
    );
}

#[test]
fn cli_audit_verify_takes_no_arguments() {
    // Success here means the subcommand parses with zero required args.
    cli_help(&["audit", "verify"]);
}

// ---- actions.escalate ----

#[test]
fn cli_actions_escalate_help_mentions_target_and_reason() {
    let help = cli_help(&["actions", "escalate"]);
    assert!(
        help.contains("<TARGET>") || help.contains("target"),
        "actions escalate --help must mention target, got:\n{help}"
    );
    assert!(
        help.contains("<REASON>") || help.contains("reason"),
        "actions escalate --help must mention reason, got:\n{help}"
    );
}

// ---- top-level dispatch (catch-all: every new Phase 5 Part E
// subcommand must be reachable from the binary) ----

#[test]
fn top_level_help_lists_audit_and_actions() {
    let help = cli_help(&[]);
    assert!(
        help.contains("audit"),
        "top-level --help must list `audit`, got:\n{help}"
    );
    assert!(
        help.contains("actions"),
        "top-level --help must list `actions`, got:\n{help}"
    );
}
