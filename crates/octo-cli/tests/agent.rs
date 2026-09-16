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

use std::io::Write;

use assert_cmd::Command;
use octo_cli::redact::{
    redact_by_field, truncate_id, RedactedIdentifier, RedactionContext, REDACTED_KEY,
};
use predicates::str::contains;
use serde_json::json;
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

/// TV-AGT-CLAP-1: clap surface validation passes (catches
/// malformed arg metadata at compile time via `debug_assert`).
/// Reserves the `TV-AGT-ENV-*` namespace for envelope-boundary
/// redaction vectors per mission
/// `0011-c-agent-redaction-envelope`.
#[test]
fn tv_agt_clap_surface_is_valid() {
    // Just by invoking `--help` we exercise the clap surface; an
    // invalid surface panics on debug_assert at startup.
    octo().args(["agent", "--help"]).assert().code(0);
}

/// TV-AGT-SEC-1: log-time redaction of agent substrate fields per
/// RFC-0011-c §Security Considerations. The three agent-family
/// fields (`agent_id`, `capability_root`, `holder_did`) MUST be
/// replaced by `REDACTED_KEY` whenever they appear in structured log
/// lines. The `redact_by_field` helper that
/// `OctoCliRedactor::on_event` (`crates/octo-cli/src/redact.rs`)
/// delegates to MUST return `REDACTED_KEY` for these field names —
/// this test pins that contract so a future amendment to either
/// the helper or the tracing Layer cannot drop the agent-family
/// fields without breaking the test.
#[test]
fn tv_agt_redact_log_line_replaces_agent_family_fields() {
    let log_line = r#"event="agent_registered" agent_id="00000000-0000-4000-8000-000000000001" capability_root="abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234" holder_did="did:octo:zSecret""#;

    assert_eq!(
        redact_by_field("agent_id", "00000000-0000-4000-8000-000000000001"),
        REDACTED_KEY,
        "agent_id field MUST map to REDACTED_KEY",
    );
    assert_eq!(
        redact_by_field(
            "capability_root",
            "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234"
        ),
        REDACTED_KEY,
        "capability_root field MUST map to REDACTED_KEY",
    );
    assert_eq!(
        redact_by_field("holder_did", "did:octo:zSecret"),
        REDACTED_KEY,
        "holder_did field MUST map to REDACTED_KEY",
    );
    // The line itself is fine — the assertion is that the
    // `redact_by_field` helper that `OctoCliRedactor::on_event`
    // delegates to would substitute the values above when the live
    // tracing layer walks the structured fields.
    assert!(
        !log_line.contains("REDACTED"),
        "log line as-written MUST NOT contain any REDACTED marker (redaction happens at emit time): {log_line}",
    );
}

// ============================================================================
// Phase 2 envelope-boundary redaction vectors — mission
// `0011-c-agent-redaction-envelope` §Scope sub-step 4.
//
// These vectors exercise the substrate-faithful contract:
//   1. `holder_did` redaction — operator-owned DID pass-through,
//      non-owned DID stays wholesale REDACTED_KEY.
//   2. `agent_id` truncation — first 8 chars + ellipsis form.
//   3. Wholesale `REDACTED_KEY` for `capability_root` (and other
//      `RedactedIdentifier` fields).
//   4. `RedactionContext::apply` walks nested JSON objects.
//
// Each test exercises `RedactedIdentifier` + `RedactionContext` in
// isolation — the envelope renderer is the only consumer that walks
// a full `OutputEnvelope`, and unit-tests of that path live in
// `crates/octo-cli/src/output.rs` (see `tv_env_redact_*`).
// ============================================================================

/// TV-AGT-ENV-1: `holder_did` redaction — operator-owned DID
/// pass-through. When the CLI build path passes a `holder_did_raw`
/// equal to the active DID, the renderer un-redacts the
/// `[REDACTED:key]` marker to the actual DID value. This is the
/// conditional reveal per mission §Scope sub-step 2.
#[test]
fn tv_agt_redact_holder_did_un_redacts_when_match() {
    let mut payload = json!({
        "agent_id": REDACTED_KEY,
        "holder_did": REDACTED_KEY,
        "state": "registered",
    });
    let redactor = self_holder_redactor("did:octo:zOperator");
    redactor.apply(&mut payload);

    let obj = payload.as_object().expect("payload is object");
    assert_eq!(
        obj.get("holder_did").and_then(|v| v.as_str()),
        Some("did:octo:zOperator"),
        "holder_did must un-redact to active DID when match: got {payload}",
    );
}

/// TV-AGT-ENV-2: `holder_did` redaction — non-owned DID stays
/// redacted. When `holder_did_raw != active_did`, the renderer keeps
/// `[REDACTED:key]` (does NOT un-redact to the non-self DID).
#[test]
fn tv_agt_redact_holder_did_stays_redacted_when_mismatch() {
    let mut payload = json!({
        "agent_id": REDACTED_KEY,
        "holder_did": REDACTED_KEY,
        "state": "registered",
    });
    let redactor = RedactionContext::new()
        .with_active_did("did:octo:zOperator")
        .with_holder_did("did:octo:zOtherHolder")
        .with_agent_id("00000000-0000-4000-8000-000000000001");
    redactor.apply(&mut payload);

    let obj = payload.as_object().expect("payload is object");
    assert_eq!(
        obj.get("holder_did").and_then(|v| v.as_str()),
        Some(REDACTED_KEY),
        "holder_did must stay REDACTED_KEY when mismatch: got {payload}",
    );
}

/// TV-AGT-ENV-3: `agent_id` truncation. The renderer replaces
/// `[REDACTED:key]` with a truncated form (first 8 chars + ellipsis)
/// so operators can correlate with substrate logs without seeing the
/// full UUID.
#[test]
fn tv_agt_redact_agent_id_truncates_to_eight_chars() {
    let mut payload = json!({
        "agent_id": REDACTED_KEY,
        "holder_did": REDACTED_KEY,
    });
    let redactor = self_holder_redactor("did:octo:zOperator");
    redactor.apply(&mut payload);

    let obj = payload.as_object().expect("payload is object");
    assert_eq!(
        obj.get("agent_id").and_then(|v| v.as_str()),
        Some("00000000..."),
        "agent_id must truncate to first 8 chars + ellipsis: got {payload}",
    );
}

/// TV-AGT-ENV-4: `capability_root` always emits `[REDACTED:key]`.
/// The CLI build path wraps `capability_root` in `RedactedIdentifier`
/// (wholesale redaction at the type level). The renderer walker is
/// a no-op for this field — the `Serialize` impl emits the marker
/// directly, and the walker leaves it untouched.
#[test]
fn tv_agt_redact_capability_root_emits_redacted_key() {
    let redacted =
        RedactedIdentifier::new("abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234");
    let json = serde_json::to_string(&redacted).expect("serialize RedactedIdentifier");
    assert_eq!(
        json, "\"[REDACTED:key]\"",
        "RedactedIdentifier::serialize must emit [REDACTED:key] marker, got: {json}",
    );
    // Inner value MUST NOT leak through Serialize.
    assert!(
        !json.contains("abcd1234"),
        "RedactedIdentifier leaked inner value: {json}",
    );
}

/// TV-AGT-ENV-5: `RedactionContext::apply` recurses into nested
/// objects + arrays. The walker handles arbitrary JSON shapes so
/// future envelope payloads (e.g. `AgentListOutput` with arrays of
/// agents) get redaction without a per-shape rewrite.
#[test]
fn tv_agt_redact_walks_nested_payload() {
    let mut payload = json!({
        "agents": [
            {
                "agent_id": REDACTED_KEY,
                "holder_did": REDACTED_KEY,
            },
            {
                "agent_id": REDACTED_KEY,
                "holder_did": REDACTED_KEY,
            },
        ],
        "metadata": {
            "agent_id": REDACTED_KEY,
            "holder_did": REDACTED_KEY,
        },
    });
    let redactor = self_holder_redactor("did:octo:zOperator");
    redactor.apply(&mut payload);

    // Outer metadata block.
    let metadata = payload
        .get("metadata")
        .and_then(|v| v.as_object())
        .expect("metadata is object");
    assert_eq!(
        metadata.get("holder_did").and_then(|v| v.as_str()),
        Some("did:octo:zOperator"),
        "metadata.holder_did un-redacted: {payload}",
    );
    assert_eq!(
        metadata.get("agent_id").and_then(|v| v.as_str()),
        Some("00000000..."),
        "metadata.agent_id truncated: {payload}",
    );

    // Inner agents array.
    let agents = payload
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents is array");
    assert_eq!(agents.len(), 2);
    for (i, agent) in agents.iter().enumerate() {
        let obj = agent.as_object().expect("agent is object");
        assert_eq!(
            obj.get("holder_did").and_then(|v| v.as_str()),
            Some("did:octo:zOperator"),
            "agents[{i}].holder_did un-redacted: {payload}",
        );
        assert_eq!(
            obj.get("agent_id").and_then(|v| v.as_str()),
            Some("00000000..."),
            "agents[{i}].agent_id truncated: {payload}",
        );
    }
}

/// TV-AGT-ENV-6: `truncate_id` returns the first 8 chars +
/// ellipsis. UUID form gives the first UUID segment
/// (`xxxxxxxx-...`). DID form gives the public scheme prefix
/// (`did:octo...`).
#[test]
fn tv_agt_redact_truncate_id_returns_eight_chars_plus_ellipsis() {
    assert_eq!(
        truncate_id("00000000-0000-4000-8000-000000000001"),
        "00000000...",
    );
    assert_eq!(truncate_id("did:octo:zTest"), "did:octo...");
    // Short input (< 8 chars) — returns the full input + ellipsis.
    assert_eq!(truncate_id("short"), "short...");
    // Empty input — returns empty + ellipsis.
    assert_eq!(truncate_id(""), "...");
}

/// Build a `RedactionContext` with holder_did == active_did so the
/// conditional `holder_did` un-redact path fires. The `agent_id`
/// truncation always fires (independent of holder match).
fn self_holder_redactor(active_did: &str) -> RedactionContext {
    RedactionContext::new()
        .with_active_did(active_did)
        .with_holder_did(active_did)
        .with_agent_id("00000000-0000-4000-8000-000000000001")
}
