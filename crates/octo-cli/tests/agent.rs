//! Integration tests for `octo agent create` — RFC-0011-c §Subcommand Taxonomy.
//!
//! Test vectors from mission `0011-c-agent-create-subcommand` §Test Vectors:
//! - TV-AGT1: missing `--manifest-path` rejected at clap parse time (exit 2)
//! - TV-AGT2: nonexistent manifest file → exit 39 (`ManifestParseError`)
//! - TV-AGT3: malformed JSON in manifest → exit 39 (`ManifestParseError`)
//! - TV-AGT4: Auditor mode → exit 2 (`AuditorDenied` — read-only role)
//! - TV-AGT13: pending subcommand (`agent list`) → exit 64 (Internal,
//!   surfaces follow-on mission slug)
//!
//! Each test runs as a child binary via `assert_cmd`, captures
//! stdout/stderr, and inspects exit code + JSON shape per vector.

use assert_cmd::Command;
use predicates::str::contains;
use std::io::Write;
use tempfile::NamedTempFile;

fn octo() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd
}

/// Build a minimal valid manifest JSON body.
fn valid_manifest_json() -> String {
    serde_json::json!({
        "manifest_id": "00000000-0000-4000-8000-000000000001",
        "holder_did": "did:octo:zTest",
        "label": "test-agent",
        "created_at_unix": 1_700_000_000_u64,
        "signature_hex": "ab".repeat(64),
    })
    .to_string()
}

/// TV-AGT1: missing `--manifest-path` is rejected at clap parse time
/// (exit 2 per RFC-0011 exit-code table).
#[test]
fn tv_agt1_missing_manifest_path_rejected_at_clap_parse() {
    octo()
        .args(["agent", "create"])
        .assert()
        .code(2)
        .stderr(contains("--manifest-path"));
}

/// TV-AGT2: a nonexistent manifest path returns `ManifestParseError`
/// (exit 39 per RFC-0011-c §9.8) — the operator sees the path they
/// supplied in the error message so they can correct the file
/// reference.
#[test]
fn tv_agt2_nonexistent_manifest_returns_exit_39() {
    octo()
        .args([
            "agent",
            "create",
            "--manifest-path",
            "/tmp/cipherocto-definitely-does-not-exist.json",
        ])
        .assert()
        .code(39)
        .stderr(contains("manifest parse error"));
}

/// TV-AGT3: malformed JSON in the manifest file returns
/// `ManifestParseError` (exit 39). The operator-supplied path is
/// surfaced verbatim so the error is actionable.
#[test]
fn tv_agt3_malformed_json_returns_exit_39() {
    let mut f = NamedTempFile::new().expect("create temp file");
    write!(f, "this is not json").expect("write malformed JSON");
    let path = f.path().to_str().expect("utf-8 path");

    octo()
        .args(["agent", "create", "--manifest-path", path])
        .assert()
        .code(39)
        .stderr(contains("manifest parse error"));
}

/// TV-AGT4: Auditor mode is read-only and rejects the write. The
/// CLI surfaces `OctoCliError::AuditorDenied` (exit 2) BEFORE
/// touching the substrate — defense in depth so a misconfigured
/// auditor never reaches the agent registry.
#[test]
fn tv_agt4_auditor_mode_denies_write() {
    let mut f = NamedTempFile::new().expect("create temp file");
    write!(f, "{}", valid_manifest_json()).expect("write manifest JSON");
    let path = f.path().to_str().expect("utf-8 path");

    octo()
        .args([
            "agent",
            "create",
            "--manifest-path",
            path,
            "--mode",
            "auditor",
        ])
        .assert()
        .code(2)
        .stderr(contains("auditor mode is read-only"));
}

/// TV-AGT13: sibling subcommands (`run` / `list` / `destroy` /
/// `attach`) that haven't been wired yet emit a clear "pending
/// follow-on mission" error instead of a clap-level rejection. The
/// operator sees the follow-on mission slug so they can track the
/// amendment chain.
#[test]
fn tv_agt13_list_subcommand_is_pending() {
    octo()
        .args(["agent", "list"])
        .assert()
        .code(64)
        .stderr(contains("follow-on mission"));
}

/// Pending subcommand — `agent run`.
#[test]
fn tv_agt13b_run_subcommand_is_pending() {
    octo()
        .args([
            "agent",
            "run",
            "--agent-id",
            "00000000-0000-4000-8000-000000000001",
        ])
        .assert()
        .code(64)
        .stderr(contains("follow-on mission"));
}

/// Pending subcommand — `agent destroy`.
#[test]
fn tv_agt13c_destroy_subcommand_is_pending() {
    octo()
        .args([
            "agent",
            "destroy",
            "--agent-id",
            "00000000-0000-4000-8000-000000000001",
        ])
        .assert()
        .code(64)
        .stderr(contains("follow-on mission"));
}

/// Pending subcommand — `agent attach`.
#[test]
fn tv_agt13d_attach_subcommand_is_pending() {
    octo()
        .args([
            "agent",
            "attach",
            "--agent-id",
            "00000000-0000-4000-8000-000000000001",
        ])
        .assert()
        .code(64)
        .stderr(contains("follow-on mission"));
}

/// Manifest parse error: a JSON document that deserializes to a
/// different shape (missing required field `signature_hex`) returns
/// exit 39 with a path-bearing message.
#[test]
fn tv_agt3b_manifest_missing_required_field_returns_exit_39() {
    let mut f = NamedTempFile::new().expect("create temp file");
    let body = serde_json::json!({
        "manifest_id": "00000000-0000-4000-8000-000000000001",
        "holder_did": "did:octo:zTest",
        "created_at_unix": 1_700_000_000_u64
        // signature_hex intentionally missing
    })
    .to_string();
    write!(f, "{}", body).expect("write manifest JSON");
    let path = f.path().to_str().expect("utf-8 path");

    octo()
        .args(["agent", "create", "--manifest-path", path])
        .assert()
        .code(39)
        .stderr(contains("manifest parse error"));
}

/// TV-AGT4 variant: `octo agent --help` lists the four sibling
/// subcommands so the operator can discover the surface.
#[test]
fn tv_agt_help_lists_all_subcommands() {
    octo()
        .args(["agent", "--help"])
        .assert()
        .code(0)
        .stdout(contains("create"))
        .stdout(contains("run"))
        .stdout(contains("list"))
        .stdout(contains("destroy"))
        .stdout(contains("attach"));
}

/// TV-AGT-ENV-1: clap surface validation passes (catches malformed
/// arg metadata at compile time via `debug_assert`).
#[test]
fn tv_agt_clap_surface_is_valid() {
    // Just by invoking `--help` we exercise the clap surface; an
    // invalid surface panics on debug_assert at startup.
    octo().args(["agent", "--help"]).assert().code(0);
}
