//! Shared helpers for the multi-node end-to-end suites.
//!
//! Used by the cross-process suite (cross-process isolation) and the
//! cross-container suite (cross-container isolation). Both drive the
//! real `octo` binary from the operator's point of view, so they
//! share the three things that must be identical across isolation
//! levels:
//!
//! 1. canonical DID minting (Layer A codec, RFC-0010) — hand-crafted
//!    DID strings are unsafe because the codec enforces the BLAKE3
//!    binding-domain walk and the base58btc alphabet;
//! 2. envelope projection — every read path goes through
//!    `OutputEnvelope`, whose payload lives under `.payload`, NOT at
//!    the top level;
//! 3. node-home resolution — `$OCTO_HOME` is the only thing that
//!    separates one node from another.

#![allow(dead_code)] // Shared by two test crates; each uses a subset.

use octo_ident::{CanonicalCodec, DidCodec};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Per-process home counter so parallel tests never collide on a
/// temp directory (same pattern as the RFC-0011-f peer test suite).
static HOME_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Allocate a fresh, empty node home directory.
///
/// Each call returns a unique path so tests running in parallel share
/// no state. A "node" in this suite is exactly one such directory:
/// `$OCTO_HOME` holds `mesh/peers.toml` and (when the wallet store
/// ships) `wallet/keystore.json`.
pub fn new_node_home(label: &str) -> PathBuf {
    let n = HOME_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("octo-e2e-{}-{}-{n}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create node home");
    path
}

/// Path of the peer table inside a node home (Layer C canonical
/// location, per the operator guide home-directory layout).
pub fn peers_toml(home: &Path) -> PathBuf {
    home.join("mesh").join("peers.toml")
}

/// Mint a fresh canonical wire-form DID via the Layer A codec.
///
/// `seed` selects a distinct 32-byte canonical payload per call, so
/// the same seed always yields the same DID (deterministic) while
/// different seeds never collide.
pub fn canonical_did(seed: u8) -> String {
    let raw = CanonicalCodec::mint(&[seed; 32]);
    let wire = CanonicalCodec::raw_to_wire(&raw).expect("canonical raw_to_wire");
    wire.as_str().to_string()
}

/// The `Untrusted` trust-level typed discriminator written by
/// `octo mesh peer add`. Peers always start Untrusted; promotion is
/// not exposed on the CLI surface.
pub const UNTRUSTED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000003";

/// The `Trusted` trust-level typed discriminator.
pub const TRUSTED: &str = "urn:octo:trust-level:00000000-0000-0000-0000-000000000001";

/// Endpoint URI scheme allowlist (Layer C, per the operator guide
/// peer-record shape). Anything else is rejected at the dispatch
/// boundary with the invalid-endpoint-scheme exit.
pub const ALLOWED_SCHEMES: &[&str] = &["tcp://", "quic://", "bluetooth://"];

/// A parsed operator output envelope.
///
/// Every `octo` read command returns this shape: the command
/// projection lives under `payload`, alongside a `command`
/// discriminator and a `schema_version`. Operator pipelines that
/// address the payload at the top level (rather than under
/// `payload`) silently read nothing — a failure mode the multi-node
/// suite asserts against explicitly.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub command: String,
    pub payload: Value,
    pub schema_version: u64,
    pub raw: Value,
}

impl Envelope {
    /// Parse an `octo --json` stdout stream into an envelope.
    ///
    /// Panics with the raw text when the payload is absent, because a
    /// missing `payload` key is exactly the drift the suite exists to
    /// catch — a silent projection miss must fail loudly here.
    pub fn parse(stdout: &str) -> Self {
        let raw: Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|e| panic!("stdout is not a JSON envelope ({e}): {stdout}"));
        let payload = raw
            .get("payload")
            .cloned()
            .unwrap_or_else(|| panic!("envelope has no `payload` key: {stdout}"));
        Self {
            command: raw
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            payload,
            schema_version: raw
                .get("schema_version")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            raw,
        }
    }

    /// The `peers` array from a `octo.mesh.peer.list.v1` payload.
    pub fn peers(&self) -> &[Value] {
        self.payload
            .get("peers")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("peer-list payload has no `peers` array: {}", self.payload))
    }

    /// The DIDs present in a peer-list payload, in listed order.
    pub fn peer_dids(&self) -> Vec<String> {
        self.peers()
            .iter()
            .map(|p| {
                p.get("peer_did")
                    .and_then(Value::as_str)
                    .expect("peer entry missing peer_did")
                    .to_string()
            })
            .collect()
    }

    /// The `total_count` field (peers in the table, before filtering).
    pub fn total_count(&self) -> u64 {
        self.payload
            .get("total_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("peer-list payload has no total_count: {}", self.payload))
    }

    /// The `filtered_count` field (peers surviving the filter).
    pub fn filtered_count(&self) -> u64 {
        self.payload
            .get("filtered_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("peer-list payload has no filtered_count: {}", self.payload))
    }
}

/// Resolve the `octo` binary built by cargo for this test run.
pub fn octo_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_octo"))
}

/// Environment for an `octo` invocation bound to one node home.
///
/// `OCTO_FORCE_JSON` forces the JSON envelope regardless of TTY so
/// assertions can parse it; `NO_COLOR` keeps pretty output clean.
pub fn node_env(home: &Path) -> Vec<(String, String)> {
    vec![
        ("OCTO_HOME".to_string(), home.to_string_lossy().to_string()),
        ("NO_COLOR".to_string(), "1".to_string()),
        ("OCTO_FORCE_JSON".to_string(), "1".to_string()),
        // Never let the ambient CI variable leak a mode into a
        // scenario that is explicitly exercising a specific mode.
        ("CI".to_string(), String::new()),
    ]
}

/// Build an `octo` invocation bound to one node home.
pub fn octo_in(home: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(octo_bin());
    for (k, v) in node_env(home) {
        cmd.env(k, v);
    }
    cmd
}

/// Build an `octo` invocation with the trust-level filter on a peer
/// list (the `--filter-trust` value space is the typed discriminator
/// URN, not a human word).
pub fn octo_list_filtered_by_trust(home: &Path, trust_urn: &str) -> std::process::Command {
    let mut cmd = octo_in(home);
    cmd.args([
        "mesh",
        "peer",
        "list",
        "--filter-trust",
        trust_urn,
        "--json",
    ]);
    cmd
}

/// The mutation prefix that satisfies the dispatch-side write gate
/// for a scripted (non-interactive) run.
pub const CI_WRITE: [&str; 2] = ["--mode", "ci"];

/// Add one peer to a node's peer table, bypassing the interactive
/// two-step confirmation gate. Returns the child's exit code.
pub fn peer_add(home: &Path, peer_did: &str, endpoint: &str) -> std::process::ExitStatus {
    octo_in(home)
        .args(CI_WRITE)
        .args([
            "--allow-write",
            "mesh",
            "peer",
            "add",
            peer_did,
            "--endpoint",
            endpoint,
            "--json",
        ])
        .output()
        .expect("spawn octo mesh peer add")
        .status
}

/// Remove one peer from a node's peer table (idempotent). Returns the
/// child's exit code.
pub fn peer_remove(home: &Path, peer_did: &str) -> std::process::ExitStatus {
    octo_in(home)
        .args(CI_WRITE)
        .args([
            "--allow-write",
            "mesh",
            "peer",
            "remove",
            peer_did,
            "--json",
        ])
        .output()
        .expect("spawn octo mesh peer remove")
        .status
}

/// Read a node's peer table through the CLI (a distinct process from
/// any writer).
pub fn peer_list(home: &Path) -> Envelope {
    let out = octo_in(home)
        .args(["mesh", "peer", "list", "--json"])
        .output()
        .expect("spawn octo mesh peer list");
    assert!(
        out.status.success(),
        "peer list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Envelope::parse(&String::from_utf8_lossy(&out.stdout))
}

/// Read a node's peer table through the CLI with a trust-level filter.
pub fn peer_list_filtered(home: &Path, trust_urn: &str) -> Envelope {
    let out = octo_list_filtered_by_trust(home, trust_urn)
        .output()
        .expect("spawn octo mesh peer list --filter-trust");
    assert!(
        out.status.success(),
        "filtered peer list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Envelope::parse(&String::from_utf8_lossy(&out.stdout))
}
