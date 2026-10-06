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

/// `octo vault balance <non-hex>` — the auth-first exit-code
/// precedence (RFC-0011 §Exit Code precedence, fix Wave 2.5 F1)
/// surfaces `NoActiveIdentity` (exit 2) BEFORE the hex decode path,
/// so a sessionless invocation with a bad hex still exits 2 (NOT
/// 64). The parse-after-auth path is exercised by the lib test
/// `vault_balance_cmd_bad_hex_after_active_owner` in
/// `crates/octo-cli/src/commands/vault.rs` via `set_ports_for_test`.
/// This integration test pins the sessionless precedence.
#[test]
fn tv_vlt4_balance_bad_hex_no_identity_exit_2() {
    octo()
        .args(["vault", "balance", "not-hex-zzz"])
        .assert()
        .code(2)
        .stderr(contains("no active identity"));
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

/// TV-12d (precedence update, Wave 1.5 fix 6): per RFC-0011 §Exit
/// Code precedence, the auth prerequisite (`NoActiveIdentity`, exit 2)
/// fires BEFORE argument validation. The child-process harness has
/// no populated identity, so `octo vault list --chain-id <bad-hex>`
/// now exits 2 (auth gate) rather than 26 (chain-id parse). When the
/// operator runs the same command with an active identity, the parse
/// failure DOES surface as exit 26 — covered by the lib-test
/// `parse_chain_id_hex` round-trip at `commands::vault::tests`.
#[test]
fn tv_vlt12d_list_chain_id_session_precedence_exit_2() {
    octo()
        .args(["vault", "list", "--chain-id", "not-hex-zzz"])
        .assert()
        .code(2)
        .stderr(contains("no active identity"));
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

// ---------------------------------------------------------------------------
// C1 — the transfer pipeline's actual reachable end state
// ---------------------------------------------------------------------------
//
// Finding #1 asked for the 9-stage pipeline to be driven to a successful
// envelope. It cannot be, and the reason is the finding, not an obstacle to
// it: `vault_transfer_cmd` evaluates the transfer role gate at stage 4
// (`crates/octo-cli/src/commands/vault.rs:1325`), the production port bundle
// always resolves that gate to `UnprovisionedRoleGate` (`:735` → `:745`), and
// `UnprovisionedRoleGate::can_transfer` returns `false` for every DID
// unconditionally (`:627`). Nothing in the codebase ever installs a different
// gate: `with_role_gate` is called from tests only.
//
// So stages 4 through 11 are unreachable in production, and no operator
// action changes that. This matters to the guide, which presents `§7` as a
// runnable transfer scenario and whose own error hint tells the operator to
// run `octo role select` — a command that cannot clear this gate either.
//
// The tests below pin the two facts that were previously unpinned and that
// the guide gets wrong: where the pipeline actually stops, and what `--amount`
// actually accepts.

// A wallet with a registered, activated identity — the state every existing
// test in this file is denied. Registering through the binary rather than
// through the wallet substrate keeps the test honest: it proves the CLI can
// reach the identity gate, so anything after it is a property of the transfer
// pipeline and not of an empty `$OCTO_HOME`.
fn wallet_with_active_identity() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("tempdir");
    let path = home.path();
    // Clears `MIN_PASSPHRASE_CHARS` at both `register` and `unlock`.
    let passphrase = path.join("passphrase.txt");
    std::fs::write(&passphrase, "correct-horse-battery-staple-passphrase-01\n")
        .expect("write passphrase");
    octo()
        .env("OCTO_HOME", path)
        .args([
            "identity",
            "register",
            "--label",
            "c1-probe",
            "--passphrase-file",
            passphrase.to_str().expect("utf-8 path"),
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .success();
    home
}

/// TV-XFER12 — with an active identity, a syntactically valid amount, valid
/// 32-byte vault hex on both sides, and a confirmed mutation, the transfer
/// stops at the ROLE gate, not at the identity gate.
///
/// This is the vector the audit's Layer 2 analysis said was missing: the two
/// existing tests that get past validation (`tv_xfer_confirm_then_no_active_
/// identity_exit_2` and its dry-run twin) terminate at exit 2 with no
/// identity, so nothing in the repo had ever observed where a *fully
/// provisioned* invocation actually lands.
///
/// It is also a fail-closed assertion, not a success assertion. If the role
/// gate is ever wired — the event this test exists to make visible — this
/// test fails, and the failure is the signal to rewrite it into the
/// success-path test the audit asked for.
#[test]
fn tv_xfer12_walled_at_role_gate_even_with_an_active_identity() {
    let home = wallet_with_active_identity();
    octo()
        .env("OCTO_HOME", home.path())
        .args([
            "vault",
            "transfer",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1000000",
            "--asset",
            "DQA",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(25)
        .stderr(contains("role not provisioned"));
}

/// TV-XFER13 — `--amount` is an INTEGER count of DQA micros.
///
/// The guide's `§7` step 6 and `§24` step 2 both pass `--amount "1.000000"`
/// and a comment there states the flag "takes DQA canonical-form decimal
/// string per RFC-0960-v36 §Wire Form (NOT micro-integer)". That is the
/// inverse of the implemented contract: the decimal form is rejected at exit
/// 64 before any wallet or substrate call, and only an integer passes.
///
/// The three cases are asserted together because the ORDER is the contract:
/// a non-integer is rejected as malformed, `0` is rejected as non-positive,
/// and a positive integer clears stage 2 and advances to stage 4. Asserting
/// only the first would not distinguish "amount parsing is strict" from
/// "amount parsing is strict AND the pipeline then proceeds".
#[test]
fn tv_xfer13_amount_is_integer_micros_not_a_decimal_string() {
    let home = wallet_with_active_identity();

    // The guide's own documented form, verbatim.
    octo()
        .env("OCTO_HOME", home.path())
        .args([
            "vault",
            "transfer",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1.000000",
            "--asset",
            "DQA",
            "--dry-run",
        ])
        .assert()
        .code(64)
        .stderr(contains("integer count of DQA micros"));

    // Well-formed but non-positive.
    octo()
        .env("OCTO_HOME", home.path())
        .args([
            "vault",
            "transfer",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "0",
            "--asset",
            "DQA",
            "--dry-run",
        ])
        .assert()
        .code(64)
        .stderr(contains("must be > 0"));

    // A positive integer clears stage 2 — proven by where it stops next.
    octo()
        .env("OCTO_HOME", home.path())
        .args([
            "vault",
            "transfer",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1000000",
            "--asset",
            "DQA",
            "--dry-run",
        ])
        .assert()
        .code(25)
        .stderr(contains("role not provisioned"));
}

/// TV-XFER14 — `--dry-run` does not bypass the role gate.
///
/// Recorded because `§7` step 6 tells the operator to run `--dry-run` FIRST
/// precisely because it is "safe and canonical" and "prints the canonical
/// envelope (handle_id + nonce + status)". It prints an error envelope. The
/// dry-run rewrite at stage 10 happens *after* the role gate at stage 4, so
/// there is no pre-flight dry-run for a transfer — which removes the one
/// step the guide offers as a way to try the command safely.
#[test]
fn tv_xfer14_dry_run_does_not_bypass_the_role_gate() {
    let home = wallet_with_active_identity();
    octo()
        .env("OCTO_HOME", home.path())
        .args([
            "vault",
            "transfer",
            "--from",
            &any_valid_vault_hex(),
            "--to",
            &any_other_vault_hex(),
            "--amount",
            "1000000",
            "--asset",
            "DQA",
            "--dry-run",
        ])
        .assert()
        .code(25)
        .stderr(contains("role not provisioned"));
}
