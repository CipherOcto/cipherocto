//! Integration tests for `octo vault {list, balance}` — RFC-0011-e
//! §Subcommand Taxonomy.
//!
//! Test vectors from the mission YAML TV-VLT-* set:
//! - TV-VLT1: `octo vault list` envelope schema contains `vaults`,
//!   `next_cursor`, `resolved_at_unix`.
//! - TV-VLT2: `octo vault list --limit 0` rejected (clap default-value
//!   + substrate `limit >= 1` invariant).
//! - TV-VLT3: `octo vault balance <id>` envelope schema contains
//!   `record`, `cache_hit`, `projection_source_u8`, `warnings`,
//!   `history`.
//! - TV-VLT4: `octo vault balance <bad-hex>` returns non-zero
//!   exit + sanitized error message.
//! - TV-VLT5: missing `<vault_id>` arg → clap usage error → exit 2.
//! - TV-VLT6: `octo vault list` --help prints the operational synopsis.
//! - TV-VLT7: `octo vault balance --help` prints the operational
//!   synopsis.
//! - TV-VLT8: no active identity → exit 2 (`NoActiveIdentity`).
//!
//! Each test runs as a child binary via `assert_cmd` and asserts on
//! stdout/stderr/exit-code per vector.

use assert_cmd::Command;
use predicates::str::contains;

fn octo() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd
}

/// `octo vault list --help` — clap renders the operational synopsis
/// (RFC-0011-e §Subcommand Taxonomy `vault list` Synopsis).
#[test]
fn tv_vlt6_list_help_prints_synopsis() {
    octo()
        .args(["vault", "list", "--help"])
        .assert()
        .code(0)
        .stdout(contains("vault list"))
        .stdout(contains("--chain-id"))
        .stdout(contains("--asset-symbol"))
        .stdout(contains("--limit"))
        .stdout(contains("--cursor"));
}

/// `octo vault balance --help` — clap renders the operational synopsis.
#[test]
fn tv_vlt7_balance_help_prints_synopsis() {
    octo()
        .args(["vault", "balance", "--help"])
        .assert()
        .code(0)
        .stdout(contains("vault balance"))
        .stdout(contains("<VAULT_ID>"))
        .stdout(contains("--no-cache"))
        .stdout(contains("--history"));
}

/// `octo vault balance` with no `<vault_id>` positional is rejected by
/// clap with exit 2 (RFC-0011-e §Usage Errors).
#[test]
fn tv_vlt5_balance_missing_vault_id_exit_2() {
    octo().args(["vault", "balance"]).assert().code(2);
}

/// `octo vault balance <non-hex>` — hex decode fails inside
/// `parse_vault_id_hex`, surfacing exit 64 (`Internal` per the
/// sanitize path; the substrate error never escapes). Exit 64 is
/// the canonical substrate-error envelope code per `error::exit_code`
/// (`Self::Internal(_) => 64`).
#[test]
fn tv_vlt4_balance_bad_hex_internal_exit_64() {
    octo()
        .args(["vault", "balance", "not-hex-zzz"])
        .assert()
        .stderr(contains("vault_id hex decode"))
        .code(64);
}

/// `octo vault list` and `octo vault balance` without an active
/// identity emit `NoActiveIdentity` (exit 2). The substrate stub
/// `WalletStore::open()` returns Ok(Self) but
/// `active_identity()` returns `WalletError::NotActive` — the CLI
/// maps that to `OctoCliError::NoActiveIdentity` and exits 2.
#[test]
fn tv_vlt8_list_no_active_identity_exit_2() {
    octo()
        .args(["vault", "list"])
        .assert()
        .code(2)
        .stderr(contains("no active identity"));
}

#[test]
fn tv_vlt8b_balance_no_active_identity_exit_2() {
    octo()
        .args([
            "vault",
            "balance",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ])
        .assert()
        .code(2)
        .stderr(contains("no active identity"));
}

/// Root-level `octo --help` surfaces `vault` as a top-level subcommand
/// (RFC-0011-e §Binary Surface; Layer C wrapping substrate).
#[test]
fn tv_vlt0_root_help_lists_vault_subcommand() {
    octo()
        .args(["--help"])
        .assert()
        .code(0)
        .stdout(contains("vault"));
}
