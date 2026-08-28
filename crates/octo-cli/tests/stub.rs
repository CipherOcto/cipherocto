//! Integration tests for the deprecated command stub banner.
//!
//! RFC-0011 §Compatibility: in v1.0 the `init`, `join`, `role`, `agent`,
//! and `status` subcommands are deprecated stubs that emit a banner on
//! stderr and exit 0. The banner MUST:
//! 1. Begin with the canonical `DEPRECATED:` prefix so log scrapers can
//!    grep for it (SPEC-08).
//! 2. Be emitted on stderr (not stdout) so JSON consumers can pipe
//!    `octo ... --json` output through `jq` without the banner
//!    contaminating the parseable stream (R4 MEDIUM #12).
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

/// SPEC-08: every deprecated stub command emits the canonical banner on
/// stderr. We exercise each deprecated subcommand family and assert the
/// banner prefix surfaces on stderr (not stdout) so JSON consumers can
/// rely on stdout being parseable.
#[test]
fn tv_dep_banner_emitted_on_stderr_for_role_family() {
    let output = octo().args(["role", "builder"]).output().expect("run");
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
