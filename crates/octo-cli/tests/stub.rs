//! Integration tests for the deprecated command stub banner.
//!
//! RFC-0011 §Compatibility (stub-banner contract): in v1.0 the `init`,
//! `join`, and `status` subcommands are deprecated stubs that emit a
//! banner on stderr and exit 0. The banner MUST:
//! 1. Begin with the canonical `DEPRECATED:` prefix so log scrapers can
//!    grep for it.
//! 2. Be emitted on stderr (not stdout) so JSON consumers can pipe
//!    `octo ... --json` output through `jq` without the banner
//!    contaminating the parseable stream.
//!
//! The `role` and `agent` subcommands are no longer stubs —
//! `role {list,show,select}` was wired by RFC-0011-d Phase 1 and
//! `agent {create,run,list,destroy,attach}` was wired by
//! RFC-0011-c (`0011-c-agent-create-subcommand`).
//!
//! These integration tests exercise the binary surface via `assert_cmd`
//! and assert against the captured stderr stream. The unit-level
//! `print_deprecated_with` test seam in `commands::stub` covers the
//! stale-window hard-error path; this file covers the v1.0 banner
//! surface.

use assert_cmd::Command;

fn octo() -> Command {
    Command::cargo_bin("octo").expect("cargo_bin octo")
}

/// Canonical banner prefix per SPEC-08. Production code in
/// `commands::stub::print_deprecated_with` MUST render a banner that
/// begins with this prefix.
const BANNER_PREFIX: &str = "DEPRECATED:";

/// SPEC-08 role-family stub superseded by RFC-0011-d Phase 1 structured
/// subcommands (`octo role {list,show,select}`). `octo role builder` now
/// hits clap as unrecognized subcommand → exit 2.
///
/// Phase 1 contract: structured role subcommands replace the SPEC-08
/// banner contract. This test pins the new contract so a future
/// refactor cannot accidentally route `role builder` back through the
/// deprecated stub path without a deliberate migration plan.
#[test]
fn tv_dep_banner_emitted_on_stderr_for_role_family() {
    let output = octo().args(["role", "builder"]).output().expect("run");
    assert_eq!(
        output.status.code(),
        Some(2),
        "Phase 1 structured subcommands MUST exit 2 for unknown subcommand, \
         got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized") && stderr.contains("builder"),
        "stderr MUST contain 'unrecognized' AND 'builder', got: {stderr}",
    );
}

#[test]
fn tv_dep_banner_emitted_on_stderr_for_init() {
    assert_banner_on_stderr(&["init"]);
}

#[test]
fn tv_dep_banner_emitted_on_stderr_for_join() {
    assert_banner_on_stderr(&["join"]);
}

#[test]
fn tv_dep_banner_emitted_on_stderr_for_status() {
    assert_banner_on_stderr(&["status"]);
}

/// REMOVED 2026-09-08 (mission `0011-c-agent-create-subcommand`):
/// `agent list` is now a real subcommand that exits 64 with a
/// follow-on-mission message — covered by `tests/agent.rs`
/// (notably `tv_agt13_list_subcommand_is_pending`). The banner
/// contract that this test was guarding no longer applies because
/// `octo agent` is no longer a deprecated stub. The unit-level
/// `print_deprecated_with` test seam in `commands::stub` keeps the
/// v1.0 banner contract guarded for the still-stubbed commands
/// (`init`, `join`, `status`).
/// Helper: assert a deprecated subcommand emits the canonical banner on
/// stderr and keeps stdout clean of the banner prefix. Used by the
/// `init` / `join` / `status` regression guards so a future refactor
/// cannot route one of these through `println!` (or a non-`print_deprecated`
/// path) without breaking JSON-consumer stdout streams.
fn assert_banner_on_stderr(args: &[&str]) {
    let output = octo().args(args).output().expect("run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "v1.0 deprecated stub MUST exit 0 for {args:?}, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(BANNER_PREFIX),
        "stderr MUST begin with {BANNER_PREFIX:?} for {args:?}, got: {stderr}",
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(BANNER_PREFIX),
        "banner MUST NOT leak to stdout for {args:?} (JSON contamination), got: {stdout}",
    );
}
