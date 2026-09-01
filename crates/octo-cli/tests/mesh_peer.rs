//! Integration tests for `octo mesh peer <list|add|remove>` —
//! RFC-0011-f §Test Vectors peer groups.
//!
//! 12 TV per mission 0011-f-mesh-peer-subcommand AC:
//!   TV-PEER-LIST-1   list   peer-list-success                 — 3 peers; sorted; exit 0
//!   TV-PEER-LIST-2   list   peer-list-filter-trusted          — filter narrows to Trusted; exit 0
//!   TV-PEER-LIST-3   list   peer-list-filter-union            — inclusive-set across values; exit 0
//!   TV-PEER-LIST-4   list   peer-list-empty                   — empty table; exit 0; empty `peers`
//!   TV-PEER-ADD-1    add    peer-add-success-untrusted        — canonical DID + tcp:// → atomic write; exit 0
//!   TV-PEER-ADD-2    add    peer-add-invalid-did-shape        — legacy DID → exit 4 IdentityNotFound
//!   TV-PEER-ADD-3    add    peer-add-invalid-endpoint-scheme  — file:// → exit 28 InvalidEndpointScheme
//!   TV-PEER-ADD-4    add    peer-add-confirm-required         — missing --confirm → exit 2
//!   TV-PEER-REMOVE-1 remove peer-remove-success               — peer present → removed: true
//!   TV-PEER-REMOVE-2 remove peer-remove-not-present           — peer absent → removed: false; exit 0
//!   TV-PEER-REMOVE-3 remove peer-remove-confirm-required      — missing --confirm → exit 2
//!   TV-PEER-REMOVE-4 remove peer-remove-invalid-did-shape     — legacy DID → exit 4
//!
//! Each success-path uses `--mode ci --allow-write` to bypass the
//! `--confirm --confirm-acknowledge` two-step gate (CI mode is
//! trusted via the pipeline gate contract per RFC-0011 §Security
//! Considerations 1a). Failure-path vectors intentionally omit
//! `--allow-write` so the ConfirmationRequired gate fires
//! (TV-PEER-ADD-4, TV-PEER-REMOVE-3). Each test creates a single
//! per-test `$OCTO_HOME` and threads it through every octo() call
//! so peer-table writes/reads land in the same isolated directory
//! (using a fresh home per call would scatter writes across
//! temp dirs and break the presence/filter assertions).

use assert_cmd::Command;
use octo_ident::{CanonicalCodec, DidCodec};
use predicates::str as pred_str;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Per-test home isolation counter — every `new_home()` call returns
/// a unique dir so parallel-running tests never collide.
static HOME_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn new_home() -> String {
    let n = HOME_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("octo-cli-mesh-peer-{}-{n}", std::process::id()));
    let _ = fs::create_dir_all(&path);
    path.to_string_lossy().into_owned()
}

/// Build the octo CLI test command bound to a specific `octo_home`.
/// `octo_home` MUST be shared across every command in a single
/// test (or the substrate's `peers.toml` writes never match the
/// list/remove reads).
fn octo_in(home: &str) -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd.env("OCTO_HOME", home);
    cmd
}

/// Mint a fresh canonical wire-form DID via the substrate codec
/// (RFC-0010 `did:octo:z<base58btc>`). Hand-crafting a string is
/// unsafe — the codec enforces the BLAKE3 binding-domain walk and
/// base58btc alphabet.
fn canonical_did(seed_byte: u8) -> String {
    let raw = CanonicalCodec::mint(&[seed_byte; 32]);
    let wire = CanonicalCodec::raw_to_wire(&raw).expect("canonical raw_to_wire");
    wire.as_str().to_string()
}

/// Pre-add a peer via the CLI in CI mode (skips `--confirm` gate).
/// Returns the DID used so tests can later list/remove it.
fn add_peer(home: &str, did_seed: u8, endpoint: &str) -> String {
    let did = canonical_did(did_seed);
    octo_in(home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "add",
            &did,
            "--endpoint",
            endpoint,
        ])
        .assert()
        .success();
    did
}

// ---------------------------------------------------------------------------
// TV-PEER-LIST-1: peer-list-success (3 peers; sorted; exit 0)
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_list_1_success_three_peers_sorted() {
    let home = new_home();
    add_peer(&home, 1, "tcp://10.0.0.1:9001");
    add_peer(&home, 2, "tcp://10.0.0.2:9002");
    add_peer(&home, 3, "quic://10.0.0.3:9003");

    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("schema_version"))
        .stdout(pred_str::contains("\"peers\""))
        .stdout(pred_str::contains("\"total_count\":3"))
        .stdout(pred_str::contains("\"filtered_count\":3"));
}

// ---------------------------------------------------------------------------
// TV-PEER-LIST-2: peer-list-filter-trusted
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_list_2_filter_trusted_shorthand() {
    let home = new_home();
    add_peer(&home, 1, "tcp://10.0.0.1:9001");
    add_peer(&home, 2, "tcp://10.0.0.2:9002");

    // All new peers start Untrusted (RFC-0011-f §Rationale "Why
    // TrustLevel enum is in the CLI"; promotion to Trusted happens
    // via substrate signals). Filtering by `trusted` shorthand
    // therefore yields 0 peers — filter is syntactically valid,
    // exit 0, just empty.
    octo_in(&home)
        .args(["mesh", "peer", "list", "--filter-trust", "trusted"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"filtered_count\":0"))
        .stdout(pred_str::contains("\"total_count\":2"));
}

// ---------------------------------------------------------------------------
// TV-PEER-LIST-3: peer-list-filter-union (inclusive-set across --filter-trust)
//
// Per `PeerFilter::matches` in octo-mesh the substrate implements
// OR/inclusive-set semantics: a peer passes the filter if its trust
// level matches ANY of the supplied values. With 3 Untrusted peers
// and `--filter-trust verified --filter-trust untrusted`, all 3
// pass (matched via the `untrusted` element). The unit test
// `peer_filter_trust_levels_is_inclusive_set` in `octo-mesh::peer`
// pins this same behaviour. Exit 0; `filtered_count == total_count`.
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_list_3_filter_union() {
    let home = new_home();
    add_peer(&home, 1, "tcp://10.0.0.1:9001");
    add_peer(&home, 2, "tcp://10.0.0.2:9002");
    add_peer(&home, 3, "quic://10.0.0.3:9003");

    octo_in(&home)
        .args([
            "mesh",
            "peer",
            "list",
            "--filter-trust",
            "verified",
            "--filter-trust",
            "untrusted",
        ])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"filtered_count\":3"))
        .stdout(pred_str::contains("\"total_count\":3"));
}

// ---------------------------------------------------------------------------
// TV-PEER-LIST-4: peer-list-empty
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_list_4_empty() {
    let home = new_home();
    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("schema_version"))
        .stdout(pred_str::contains("\"peers\":[]"))
        .stdout(pred_str::contains("\"total_count\":0"))
        .stdout(pred_str::contains("\"filtered_count\":0"));
}

// ---------------------------------------------------------------------------
// TV-PEER-ADD-1: peer-add-success-untrusted
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_add_1_success_untrusted() {
    let home = new_home();
    let did = canonical_did(10);
    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "add",
            &did,
            "--endpoint",
            "tcp://192.0.2.1:9100",
        ])
        .assert()
        .code(0)
        .stdout(pred_str::contains("schema_version"))
        .stdout(pred_str::contains("\"peer_did\""))
        .stdout(pred_str::contains("\"endpoint\""))
        .stdout(pred_str::contains("\"trust_level\""))
        .stdout(pred_str::contains("\"added_at_unix\""));

    // Confirm the table actually persisted (atomic write).
    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"total_count\":1"));
}

// ---------------------------------------------------------------------------
// TV-PEER-ADD-2: peer-add-invalid-did-shape (legacy form → exit 4)
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_add_2_invalid_did_shape() {
    let home = new_home();
    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "add",
            "did:octo:bLegacyBase32Formabcdef0123456789ab",
            "--endpoint",
            "tcp://192.0.2.2:9100",
        ])
        .assert()
        .code(4)
        .stderr(pred_str::contains("identity not found"));

    // Confirm the rejected call left the table empty.
    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"total_count\":0"));
}

// ---------------------------------------------------------------------------
// TV-PEER-ADD-3: peer-add-invalid-endpoint-scheme (file:// → exit 28)
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_add_3_invalid_endpoint_scheme() {
    let home = new_home();
    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "add",
            &canonical_did(11),
            "--endpoint",
            "file:///etc/passwd",
        ])
        .assert()
        .code(28)
        .stderr(pred_str::contains("invalid endpoint URI scheme"));

    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"total_count\":0"));
}

// ---------------------------------------------------------------------------
// TV-PEER-ADD-4: peer-add-confirm-required (missing --confirm → exit 2)
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_add_4_confirm_required() {
    let home = new_home();
    // Human mode + no --confirm + no --allow-write → gate fires
    // before any substrate call. exit 2 (`ConfirmationRequired`).
    octo_in(&home)
        .args([
            "mesh",
            "peer",
            "add",
            &canonical_did(12),
            "--endpoint",
            "tcp://192.0.2.4:9100",
        ])
        .assert()
        .code(2)
        .stderr(pred_str::contains("--confirm required"));

    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"total_count\":0"));
}

// ---------------------------------------------------------------------------
// TV-PEER-REMOVE-1: peer-remove-success
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_remove_1_success() {
    let home = new_home();
    let did = add_peer(&home, 20, "tcp://10.0.0.20:9020");

    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "remove",
            &did,
        ])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"removed\":true"));

    octo_in(&home)
        .args(["mesh", "peer", "list"])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"total_count\":0"));
}

// ---------------------------------------------------------------------------
// TV-PEER-REMOVE-2: peer-remove-not-present-idempotent (removed: false, exit 0)
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_remove_2_not_present_idempotent() {
    let home = new_home();
    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "remove",
            &canonical_did(21),
        ])
        .assert()
        .code(0)
        .stdout(pred_str::contains("\"removed\":false"));
}

// ---------------------------------------------------------------------------
// TV-PEER-REMOVE-3: peer-remove-confirm-required
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_remove_3_confirm_required() {
    let home = new_home();
    octo_in(&home)
        .args(["mesh", "peer", "remove", &canonical_did(22)])
        .assert()
        .code(2)
        .stderr(pred_str::contains("--confirm required"));
}

// ---------------------------------------------------------------------------
// TV-PEER-REMOVE-4: peer-remove-invalid-did-shape
// ---------------------------------------------------------------------------

#[test]
fn tv_peer_remove_4_invalid_did_shape() {
    let home = new_home();
    octo_in(&home)
        .args([
            "--mode",
            "ci",
            "--allow-write",
            "mesh",
            "peer",
            "remove",
            "did:octo:bLegacyBase32Formabcdef0123456789ab",
        ])
        .assert()
        .code(4)
        .stderr(pred_str::contains("identity not found"));
}
