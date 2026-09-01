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
//! ## Transfer subcommand (TV-XFER*, TV-12a-d)
//!
//! Integration coverage for `octo vault transfer` is bounded by the
//! `NoActiveIdentity` substrate gate — every child-process test runs
//! without a populated `$OCTO_HOME/identity.json`, so any code path
//! past `active_owner_did()` is unreachable from here. The
//! integration suite therefore covers the gates that run BEFORE the
//! active-identity check (the confirm-flag matrix, input parse
//! failures) and the `--help` synopsis. The four pre-flight checks
//! (role provisioned → HSM reachable → vault owned → balance
//! sufficient) plus chain-id validation are exercised by lib tests in
//! `crates/octo-cli/src/commands/vault.rs` via `set_ports_for_test`.
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

fn any_valid_vault_hex() -> String {
    "aa".repeat(32)
}

fn any_other_vault_hex() -> String {
    "bb".repeat(32)
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
/// clap with exit 2 (RFC-0011-e §Error Handling).
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
/// (RFC-0011-e §Subcommand Taxonomy; Layer C wrapping substrate).
#[test]
fn tv_vlt0_root_help_lists_vault_subcommand() {
    octo()
        .args(["--help"])
        .assert()
        .code(0)
        .stdout(contains("vault"));
}

// ---------------------------------------------------------------------------
// `octo vault transfer` — RFC-0011-e §Subcommand Taxonomy
// ---------------------------------------------------------------------------

/// `octo vault transfer --help` — clap renders the operational
/// synopsis including every flag from RFC-0011-e §Transfer Flags.
#[test]
fn tv_xfer_help_prints_synopsis() {
    octo()
        .args(["vault", "transfer", "--help"])
        .assert()
        .code(0)
        .stdout(contains("vault transfer"))
        .stdout(contains("--from"))
        .stdout(contains("--to"))
        .stdout(contains("--amount"))
        .stdout(contains("--asset"))
        .stdout(contains("--memo"))
        .stdout(contains("--include-memo"))
        .stdout(contains("--redact-ids"))
        .stdout(contains("--dest-chain-id"))
        .stdout(contains("--dry-run"));
}

/// TV-12b: Human mode without `--confirm` AND `--confirm-acknowledge`
/// requires the two-step pastejacking defense. The confirm gate runs
/// BEFORE the active-identity check, so the failure surfaces as
/// `ConfirmationRequired` (exit 2) with a "confirmation" diagnostic.
#[test]
fn tv_xfer_human_without_confirm_exit_2() {
    octo()
        .args([
            "vault",
            "transfer",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(2)
        .stderr(contains("ConfirmationRequired"));
}

/// TV-12b-Auditor: Auditor mode is denied BEFORE the dry-run bypass
/// (RFC-0011 §Security 1a). Exit 2 with an "auditor" diagnostic; the
/// active-identity gate is never reached.
#[test]
fn tv_xfer_auditor_mode_denied_exit_2() {
    octo()
        .args([
            "vault",
            "transfer",
            "--mode",
            "auditor",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(2)
        .stderr(contains("auditor"));
}

/// TV-12c: a non-integer `--amount` fails the CLI-side parse step
/// (`parse_amount_micros`) BEFORE the substrate call. Surfaces as
/// `Internal` (exit 64) per Wave B's substrate-error envelope code.
///
/// Note: the confirm gate runs first, so this test passes both
/// `--confirm` and `--confirm-acknowledge` to admit the request into
/// the parse stage.
#[test]
fn tv_xfer_amount_not_integer_exit_64() {
    octo()
        .args([
            "vault",
            "transfer",
            "--confirm",
            "--confirm-acknowledge",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "not-an-int",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(64)
        .stderr(contains("amount"));
}

/// TV-12c-zero: `--amount 0` is rejected at parse time (`amount_micros`
/// must be positive per RFC-0960-v36 §Wire Form).
#[test]
fn tv_xfer_amount_zero_exit_64() {
    octo()
        .args([
            "vault",
            "transfer",
            "--confirm",
            "--confirm-acknowledge",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "0",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(64)
        .stderr(contains("amount"));
}

/// Non-hex `--from` vault ID fails `parse_vault_id_hex` with exit 64.
/// Sits between the confirm gate and `active_owner_did()`.
#[test]
fn tv_xfer_vault_id_not_hex_exit_64() {
    octo()
        .args([
            "vault",
            "transfer",
            "--confirm",
            "--confirm-acknowledge",
            "--from",
            "not-hex-zzz",
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(64)
        .stderr(contains("vault_id"));
}

/// Non-hex `--dest-chain-id` fails `parse_chain_id_hex` with
/// `InvalidChainId` (exit 26) per RFC-0011-e §Error Handling + TV-12d.
/// Note: `--dest-chain-id` parsing happens BEFORE `active_owner_did`,
/// so we do not need to pass confirm flags here — but we still need
/// them to get past the confirm gate (which runs first).
#[test]
fn tv_xfer_dest_chain_id_not_canonical_exit_26() {
    octo()
        .args([
            "vault",
            "transfer",
            "--confirm",
            "--confirm-acknowledge",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
            "--dest-chain-id",
            "not-hex-zzz",
        ])
        .assert()
        .code(26)
        .stderr(contains("chain"));
}

/// TV-12d: `octo vault list --chain-id <bad-hex>` surfaces
/// `InvalidChainId` (exit 26). The `parse_chain_id_hex` migration
/// from `Internal` (64) to `InvalidChainId` (26) covers the list
/// subcommand too — both `--chain-id` (list) and `--dest-chain-id`
/// (transfer) feed the same parser.
#[test]
fn tv_vlt12d_list_chain_id_not_canonical_exit_26() {
    octo()
        .args(["vault", "list", "--chain-id", "not-hex-zzz"])
        .assert()
        .code(26)
        .stderr(contains("chain"));
}

/// Once the confirm gate admits the request, `active_owner_did()`
/// runs and surfaces `NoActiveIdentity` (exit 2). This is the
/// integration-test analogue of the "deeper gates" suite: in a
/// real run with `$OCTO_HOME/identity.json` populated, the
/// confirm-passing command reaches the role gate (exit 25) or
/// HSM probe (exit 5). The child-process harness has no
/// populated identity, so it stops here.
#[test]
fn tv_xfer_confirm_then_no_active_identity_exit_2() {
    octo()
        .args([
            "vault",
            "transfer",
            "--confirm",
            "--confirm-acknowledge",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(2)
        .stderr(contains("no active identity"));
}

/// `octo vault transfer` with `--dry-run` and Human mode WITHOUT
/// `--confirm-acknowledge`: the `--dry-run` flag bypasses the
/// confirm gate (per `require_confirm` semantics) but the
/// `active_owner_did()` check still runs. Surfaces
/// `NoActiveIdentity` (exit 2) — confirming that the dry-run
/// bypass does NOT leak past the identity check.
#[test]
fn tv_xfer_dry_run_bypasses_confirm_then_no_active_identity() {
    octo()
        .args([
            "vault",
            "transfer",
            "--dry-run",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(2)
        .stderr(contains("no active identity"));
}

/// Auditor mode denied EVEN with `--dry-run` — the Auditor short
/// circuit runs BEFORE the dry-run bypass (RFC-0011 §Security 1a).
#[test]
fn tv_xfer_auditor_with_dry_run_still_denied() {
    octo()
        .args([
            "vault",
            "transfer",
            "--mode",
            "auditor",
            "--dry-run",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1",
            "--asset",
            "OCTO",
        ])
        .assert()
        .code(2)
        .stderr(contains("auditor"));
}
