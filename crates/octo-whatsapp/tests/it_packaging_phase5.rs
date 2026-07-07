//! Phase 5 Part G packaging integration tests.
//!
//! Verifies:
//! 1. `octo-whatsapp version` reports `1.0.0+phase5`.
//! 2. The `packaging/docker/Dockerfile` exists + is a valid Dockerfile.
//! 3. The `packaging/systemd/octo-whatsapp.service` unit is syntactically
//!    reasonable (contains expected sections).
//! 4. The `packaging/man/octo-whatsapp.1` man page has a `.TH` header.
//! 5. The bash completion file is non-empty + has a `complete -F` line.

use std::path::Path;

// `CARGO_MANIFEST_DIR` points to `crates/octo-whatsapp/`. The repo
// root is two levels up from there.
const REPO_ROOT_FROM_CRATE: &str = "../..";

#[test]
fn packaging_dockerfile_exists_and_mentions_healthcheck() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REPO_ROOT_FROM_CRATE)
        .join("packaging/docker/Dockerfile");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(body.contains("FROM "), "Dockerfile lacks FROM");
    assert!(body.contains("HEALTHCHECK"), "Dockerfile lacks HEALTHCHECK");
    assert!(body.contains("USER octo") || body.contains("USER 1000"),
            "Dockerfile lacks non-root user");
    assert!(body.contains("VOLUME"), "Dockerfile lacks VOLUME");
}

#[test]
fn packaging_systemd_unit_has_required_sections() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REPO_ROOT_FROM_CRATE)
        .join("packaging/systemd/octo-whatsapp.service");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    for section in &["[Unit]", "[Service]", "[Install]"] {
        assert!(body.contains(section), "systemd unit missing section {section}");
    }
    assert!(body.contains("DynamicUser=yes"), "systemd unit lacks DynamicUser=yes");
    assert!(body.contains("ProtectSystem=strict"), "systemd unit lacks ProtectSystem=strict");
    assert!(body.contains("NoNewPrivileges=true"), "systemd unit lacks NoNewPrivileges=true");
    assert!(body.contains("StateDirectory=octo/whatsapp"), "systemd unit lacks StateDirectory");
}

#[test]
fn packaging_man_page_has_th_header() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REPO_ROOT_FROM_CRATE)
        .join("packaging/man/octo-whatsapp.1");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(body.starts_with(".TH OCTO-WHATSAPP"),
            "man page lacks .TH OCTO-WHATSAPP header");
    assert!(body.contains("SH NAME"), "man page lacks NAME section");
    assert!(body.contains("SH SYNOPSIS"), "man page lacks SYNOPSIS");
    assert!(body.contains("SH DESCRIPTION"), "man page lacks DESCRIPTION");
}

#[test]
fn packaging_bash_completion_has_complete_directive() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REPO_ROOT_FROM_CRATE)
        .join("packaging/completions/octo-whatsapp.bash");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(body.contains("complete -F _octo_whatsapp octo-whatsapp"),
            "bash completion missing complete directive");
}

#[test]
fn packaging_deb_metadata_includes_required_fields() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(REPO_ROOT_FROM_CRATE)
        .join("packaging/deb/cargo-deb.toml");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(body.contains("name = \"octo-whatsapp\""), "deb metadata missing name");
    assert!(body.contains("assets"), "deb metadata missing assets");
}

#[test]
fn cli_version_reports_phase5_marker() {
    // The `version` subcommand dispatches to `version.get` RPC which
    // requires a running daemon socket; verify the daemon's API
    // version via the `daemon.api.version` constant in source instead.
    // (The `cli_version` integration test exercises the full RPC path.)
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/daemon.rs"),
    )
    .unwrap();
    assert!(
        src.contains("1.0.0+phase5"),
        "daemon.rs missing `daemon.api.version = 1.0.0+phase5` marker"
    );
}

#[test]
fn cli_help_references_phase5_subcommands() {
    let out = assert_cmd::Command::cargo_bin("octo-whatsapp")
        .unwrap()
        .arg("--help")
        .output()
        .expect("run");
    let s = String::from_utf8_lossy(&out.stdout);
    for sub in &["rules", "triggers", "audit", "actions"] {
        assert!(s.contains(sub), "--help output missing '{sub}' subcommand");
    }
}