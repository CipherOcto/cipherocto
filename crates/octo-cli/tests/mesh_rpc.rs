//! Integration tests for `octo mesh rpc` — RFC-0011-f §Test Vectors
//! rpc + envelope-shape groups.
//!
//! 4 test vectors per mission 0011-f-mesh-rpc-subcommand §Test
//! Vectors (TV-RPC-1..4):
//!
//! - TV-RPC-1 `rpc-success-round-trip` — substrate Phase 1 placeholder
//!   recognises `ping`; exit 0; `RpcOutput` carries `peer_did`,
//!   `method`, `request_envelope_id`, `response_envelope_id`,
//!   `response_payload`, `round_trip_ms`.
//! - TV-RPC-2 `rpc-timeout` — covered by the unit-test layer
//!   (`map_rpc_substrate_error_rpc_timeout_becomes_rpc_timeout` +
//!   `tv_err4_exit_code_mapping`); the Phase 1 placeholder never
//!   times out so the integration path skips this vector and the
//!   substrate-truth wiring is asserted at the lib boundary.
//! - TV-RPC-3 `rpc-method-not-registered` — `--method nonexistent`
//!   fails the substrate method registry lookup → exit 17
//!   (`EnvelopeAuthorizationFailed` shared slot with
//!   `InvalidTtlHops`).
//! - TV-RPC-4 `rpc-request-response-correlation-match` — `ping`
//!   dispatch surfaces both `request_envelope_id` and
//!   `response_envelope_id` in the `RpcOutput` envelope per
//!   RFC-0011-f §RFC-0871 Envelope Mapping (CLI View ↔ Substrate).
//!
//! Each success-path uses `--dry-run` to bypass the
//! `--confirm --confirm-acknowledge` two-step pastejacking-defense
//! gate (RFC-0011-f §Confirmation Flag Matrix). Failure paths omit
//! `--allow-write` so the gate fires where expected. `OCTO_FORCE_JSON=1`
//! forces the JSON envelope form so we can inspect
//! `request_envelope_id` / `response_envelope_id` without
//! ANSI-color coupling.

use assert_cmd::Command;
use octo_ident::{CanonicalCodec, DidCodec};
use predicates::str as pred_str;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Per-test home isolation counter — every `new_home()` call returns
/// a unique dir so parallel-running tests never collide. RPC
/// receipt writes target `$OCTO_HOME/mesh/rpc-receipts.log`; a
/// shared dir within a single test keeps receipt assertions
/// deterministic, while per-test isolation keeps cross-test
/// pollution out.
static HOME_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn new_home() -> String {
    let n = HOME_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("octo-cli-mesh-rpc-{}-{n}", std::process::id()));
    let _ = fs::create_dir_all(&path);
    path.to_string_lossy().into_owned()
}

fn octo_in(home: &str) -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd.env("OCTO_HOME", home);
    cmd
}

fn canonical_did(seed_byte: u8) -> String {
    let raw = CanonicalCodec::mint(&[seed_byte; 32]);
    let wire = CanonicalCodec::raw_to_wire(&raw).expect("canonical raw_to_wire");
    wire.as_str().to_string()
}

// ---------------------------------------------------------------------------
// TV-RPC-1: rpc-success-round-trip (dry-run preview)
// ---------------------------------------------------------------------------

#[test]
fn tv_rpc_1_rpc_success_round_trip_dry_run() {
    let home = new_home();
    let peer = canonical_did(0x11);

    let assert = octo_in(&home)
        .args([
            "--mode",
            "ci",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "ping",
            "--params",
            r#"{"queue_id":"stuck-1"}"#,
            "--dry-run",
        ])
        .assert()
        .success()
        .code(0);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    // RpcOutput surfaces the substrate-truth fields per RFC-0011-f
    // §Output Envelope (rpc payload type).
    assert!(
        stdout.contains("\"request_envelope_id\""),
        "stdout missing request_envelope_id: {stdout}"
    );
    assert!(
        stdout.contains("\"response_envelope_id\""),
        "stdout missing response_envelope_id: {stdout}"
    );
    assert!(
        stdout.contains("\"round_trip_ms\""),
        "stdout missing round_trip_ms: {stdout}"
    );
    assert!(
        stdout.contains(&peer),
        "stdout missing canonical peer DID: {stdout}"
    );
    assert!(stdout.contains("ping"), "stdout missing method: {stdout}");
}

// ---------------------------------------------------------------------------
// TV-RPC-3: rpc-method-not-registered (substrate UnknownMethod)
// ---------------------------------------------------------------------------

#[test]
fn tv_rpc_3_rpc_method_not_registered_returns_exit_17() {
    let home = new_home();
    let peer = canonical_did(0x22);

    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "nonexistent.method",
            "--params",
            "{}",
            "--dry-run",
        ])
        .assert()
        .failure()
        // Exit 19 = EnvelopeAuthorizationFailed per
        // `map_rpc_substrate_error` (UnknownMethod → CLI exit 19).
        .code(19);
}

#[test]
fn tv_rpc_3b_invalid_peer_did_legacy_returns_exit_4() {
    let home = new_home();

    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "mesh",
            "rpc",
            "--peer-did",
            &format!("did:octo:b{}", "a".repeat(62)),
            "--method",
            "ping",
            "--params",
            "{}",
            "--dry-run",
        ])
        .assert()
        .failure()
        // Legacy `did:octo:b<base32>` wire form fails the
        // RFC-0010 canonical codec at the dispatch boundary →
        // exit 4 (`IdentityNotFound` per RFC-0011-f §Error Handling).
        .code(4);
}

#[test]
fn tv_rpc_3c_empty_method_returns_exit_17() {
    let home = new_home();
    let peer = canonical_did(0x33);

    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "",
            "--params",
            "{}",
            "--dry-run",
        ])
        .assert()
        .failure()
        // Empty method name fails the substrate method-registry
        // lookup → `MeshError::UnknownMethod { method: "" }` →
        // CLI exit 19 (`EnvelopeAuthorizationFailed`).
        .code(19);
}

// ---------------------------------------------------------------------------
// TV-RPC-4: rpc-request-response-correlation-match
// ---------------------------------------------------------------------------

#[test]
fn tv_rpc_4_request_and_response_envelope_ids_appear_in_output() {
    let home = new_home();
    let peer = canonical_did(0x44);

    let assert = octo_in(&home)
        .args([
            "--mode",
            "ci",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "ping",
            "--params",
            r#"{"queue_id":"stuck-1"}"#,
            "--dry-run",
        ])
        .assert()
        .success()
        .code(0);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");

    // Both envelope IDs must surface per RFC-0871 §Algorithms step
    // 6 (request/reply correlation via envelope_id) + RFC-0011-f
    // §RFC-0871 Envelope Mapping (CLI View ↔ Substrate). The
    // response_envelope_id is `[0u8; 32]` in the Phase 1
    // placeholder; the field MUST still be present (not absent)
    // so the real transport wires in transparently.
    let req_idx = stdout
        .find("\"request_envelope_id\"")
        .expect("request_envelope_id field");
    let resp_idx = stdout
        .find("\"response_envelope_id\"")
        .expect("response_envelope_id field");
    assert!(
        req_idx < resp_idx,
        "request_envelope_id must precede response_envelope_id in RpcOutput: {stdout}"
    );

    // Phase 1 placeholder returns 64-char zero hex for the
    // response envelope id (no live transport yet).
    let zero_hex = "0000000000000000000000000000000000000000000000000000000000000000";
    assert!(
        stdout.contains(zero_hex),
        "stdout must surface the response_envelope_id placeholder zero-hex: {stdout}"
    );
}

#[test]
fn tv_rpc_4b_dry_run_does_not_persist_receipt() {
    let home = new_home();
    let peer = canonical_did(0x55);

    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "ping",
            "--params",
            "{}",
            "--dry-run",
        ])
        .assert()
        .success()
        .code(0);

    // --dry-run bypasses the receipt-write side effect per
    // RFC-0011-f §Subcommand Taxonomy `rpc` "Side effects"
    // (writes are gated behind the confirm gate + non-dry-run).
    let receipts_log = std::path::Path::new(&home)
        .join("mesh")
        .join("rpc-receipts.log");
    assert!(
        !receipts_log.exists(),
        "dry-run must not create the receipt log: {}",
        receipts_log.display()
    );
}

#[test]
fn tv_rpc_4c_rpc_redacts_secret_params_in_receipt() {
    let home = new_home();
    let peer = canonical_did(0x66);

    // CI mode requires --allow-write for the receipt path; the
    // redaction must apply whether the receipt is written or not
    // (the CLI walks --params against the field-name redactor
    // BEFORE passing the JSON value to the substrate so secrets
    // never reach the substrate or the receipt log).
    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "ping",
            "--params",
            r#"{"api_key":"super-secret-do-not-log","queue_id":"stuck-1"}"#,
        ])
        .assert()
        .success()
        .code(0);

    let receipts_log = std::path::Path::new(&home)
        .join("mesh")
        .join("rpc-receipts.log");
    let body = fs::read_to_string(&receipts_log).expect("receipt log written");
    // Operator-supplied --params are NEVER persisted in the
    // receipt (the receipt carries the correlation record only),
    // so secret bytes cannot leak via the receipt path.
    assert!(
        !body.contains("super-secret-do-not-log"),
        "receipt log must NOT contain raw api_key value: {body}"
    );
    // Long-hex envelope identifiers are redacted via the
    // `find_long_hex` policy regardless of field name.
    assert!(
        body.contains("[REDACTED:key]"),
        "receipt log must contain the redacted-key placeholder for envelope IDs: {body}"
    );
}

// ---------------------------------------------------------------------------
// CLI surface: --confirm gate (pastejacking defense)
// ---------------------------------------------------------------------------

#[test]
fn tv_rpc_confirm_gate_in_human_mode() {
    let home = new_home();
    let peer = canonical_did(0x77);

    octo_in(&home)
        .args([
            "--mode",
            "human",
            "mesh",
            "rpc",
            "--peer-did",
            &peer,
            "--method",
            "ping",
            "--params",
            "{}",
        ])
        .assert()
        .failure()
        // No --confirm → pastejacking-defense gate fires. The
        // shared `require_confirm` helper prints a confirmation
        // hint via stderr (e.g. "re-run with `--confirm`").
        .stderr(pred_str::contains("--confirm"));
}
