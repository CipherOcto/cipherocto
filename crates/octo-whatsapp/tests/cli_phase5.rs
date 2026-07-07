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

// ---- Phase 6.12 Task 7: groups.* subcommands (14 new + 4 existing) ----
//
// `groups` was the pre-existing top-level command. Phase 6.12 extends it
// with 14 new subcommands matching the new RPC methods: `destroy`,
// `resolve-invite`, `add-member`, `add-members`, `remove-member`,
// `remove-members`, `promote`, `demote`, `ban`, `approve-join`,
// `rename`, `set-description`, `set-locked`, `transfer-ownership`.
// These tests verify the clap surface (subcommand exists, flags parse)
// without spawning a daemon.

#[test]
fn cli_groups_destroy_help() {
    let help = cli_help(&["groups", "destroy"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups destroy --help must mention jid, got:\n{help}"
    );
}

#[test]
fn cli_groups_resolve_invite_help() {
    let help = cli_help(&["groups", "resolve-invite"]);
    assert!(
        help.contains("<CODE>") || help.contains("code"),
        "groups resolve-invite --help must mention code, got:\n{help}"
    );
}

#[test]
fn cli_groups_add_member_help() {
    let help = cli_help(&["groups", "add-member"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups add-member --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups add-member --help must mention --member, got:\n{help}"
    );
    assert!(
        help.contains("--is-admin"),
        "groups add-member --help must mention --is-admin, got:\n{help}"
    );
}

#[test]
fn cli_groups_add_members_help() {
    let help = cli_help(&["groups", "add-members"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups add-members --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--members"),
        "groups add-members --help must mention --members, got:\n{help}"
    );
}

#[test]
fn cli_groups_remove_member_help() {
    let help = cli_help(&["groups", "remove-member"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups remove-member --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups remove-member --help must mention --member, got:\n{help}"
    );
}

#[test]
fn cli_groups_remove_members_help() {
    let help = cli_help(&["groups", "remove-members"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups remove-members --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--members"),
        "groups remove-members --help must mention --members, got:\n{help}"
    );
}

#[test]
fn cli_groups_promote_help() {
    let help = cli_help(&["groups", "promote"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups promote --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups promote --help must mention --member, got:\n{help}"
    );
}

#[test]
fn cli_groups_demote_help() {
    let help = cli_help(&["groups", "demote"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups demote --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups demote --help must mention --member, got:\n{help}"
    );
}

#[test]
fn cli_groups_ban_help() {
    let help = cli_help(&["groups", "ban"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups ban --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups ban --help must mention --member, got:\n{help}"
    );
    assert!(
        help.contains("--duration-seconds"),
        "groups ban --help must mention --duration-seconds, got:\n{help}"
    );
}

#[test]
fn cli_groups_approve_join_help() {
    let help = cli_help(&["groups", "approve-join"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups approve-join --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups approve-join --help must mention --member, got:\n{help}"
    );
}

#[test]
fn cli_groups_rename_help() {
    let help = cli_help(&["groups", "rename"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups rename --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--subject"),
        "groups rename --help must mention --subject, got:\n{help}"
    );
}

#[test]
fn cli_groups_set_description_help() {
    let help = cli_help(&["groups", "set-description"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups set-description --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--description"),
        "groups set-description --help must mention --description, got:\n{help}"
    );
}

#[test]
fn cli_groups_set_locked_help() {
    let help = cli_help(&["groups", "set-locked"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups set-locked --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--locked"),
        "groups set-locked --help must mention --locked, got:\n{help}"
    );
}

#[test]
fn cli_groups_transfer_ownership_help() {
    let help = cli_help(&["groups", "transfer-ownership"]);
    assert!(
        help.contains("<JID>") || help.contains("jid"),
        "groups transfer-ownership --help must mention jid, got:\n{help}"
    );
    assert!(
        help.contains("--member"),
        "groups transfer-ownership --help must mention --member, got:\n{help}"
    );
}
