//! Integration tests for the deprecated command stub banner.
//!
//! RFC-0011 §Compatibility (stub-banner contract): in v1.0 the `init`,
//! `join`, `role`, `agent`, and `status` subcommands are deprecated
//! stubs that emit a banner on stderr and exit 0. The banner MUST:
//! 1. Begin with the canonical `DEPRECATED:` prefix so log scrapers can
//!    grep for it.
//! 2. Be emitted on stderr (not stdout) so JSON consumers can pipe
//!    `octo ... --json` output through `jq` without the banner
//!    contaminating the parseable stream.
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
fn tv_dep_banner_emitted_on_stderr_for_agent_family() {
    let output = octo().args(["agent", "list"]).output().expect("run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "v1.0 deprecated stub MUST exit 0, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(BANNER_PREFIX),
        "stderr MUST begin with {BANNER_PREFIX:?}, got: {stderr}",
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(BANNER_PREFIX),
        "banner MUST NOT leak to stdout (JSON contamination), got: {stdout}",
    );
}

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
