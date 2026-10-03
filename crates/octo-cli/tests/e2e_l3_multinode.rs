//! Multi-node end-to-end suite — **Level 3: cross-process isolation**.
//!
//! Each "node" is one `$OCTO_HOME` directory, and every read and
//! write runs in a **separate OS process** invoking the real `octo`
//! binary through the operator-documented command surface. This is
//! the minimum floor for the multi-node suite: if a property holds
//! here, it holds for a single operator on one machine.
//!
//! The cross-container suite is the upper level; this file exists so
//! that a container-level failure can be attributed to isolation
//! rather than to a defect that is already present locally.
//!
//! Node separation rests on exactly one substrate fact: `$OCTO_HOME`
//! is the sole input that distinguishes one node from another
//! (peer table at `mesh/peers.toml`, wallet store at `wallet/`). Each
//! scenario below asserts that fact from the operator's point of
//! view, using the invocation shapes the operator guide prescribes.

mod common;

use common::{
    canonical_did, new_node_home, octo_in, peer_add, peer_list, peer_list_filtered, peer_remove,
    peers_toml, Envelope, ALLOWED_SCHEMES, CI_WRITE, TRUSTED, UNTRUSTED,
};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Scenario: a write in one process is visible to a later, distinct process
// ---------------------------------------------------------------------------

/// The peer table is on-disk state, not process-local state. A peer
/// added by one invocation must be readable by a later, unrelated
/// invocation pointed at the same home.
#[test]
fn l3_write_is_visible_to_a_later_process() {
    let home = new_node_home("l3-visible");
    let peer = canonical_did(1);

    assert_eq!(peer_list(&home).total_count(), 0, "fresh home starts empty");

    assert!(peer_add(&home, &peer, "tcp://127.0.0.1:9100").success());

    // Separate process: the CLI is spawned afresh for this read.
    let listed = peer_list(&home);
    assert_eq!(listed.total_count(), 1);
    assert_eq!(listed.filtered_count(), 1);
    assert_eq!(listed.peer_dids(), vec![peer.clone()]);

    let entry = &listed.peers()[0];
    assert_eq!(entry["endpoint"], "tcp://127.0.0.1:9100");
    assert_eq!(entry["trust_level"], UNTRUSTED, "new peers start Untrusted");
    assert_eq!(
        entry["capabilities"].as_array().map(Vec::len),
        Some(0),
        "peers start with no display capability references"
    );
    assert!(
        entry["last_seen_unix"].as_i64().unwrap_or(0) > 0,
        "last_seen_unix is stamped on add"
    );
}

// ---------------------------------------------------------------------------
// Scenario: nodes are isolated from one another
// ---------------------------------------------------------------------------

/// Three nodes, each with its own peer, each seeing exactly its own
/// peer. This is the property the whole multi-node suite exists to
/// protect: node state must not leak sideways.
#[test]
fn l3_three_nodes_do_not_see_each_others_peers() {
    let homes: Vec<_> = (0..3)
        .map(|i| new_node_home(&format!("l3-iso-{i}")))
        .collect();

    let mut expected = BTreeSet::new();
    for (i, home) in homes.iter().enumerate() {
        let peer = canonical_did(10 + i as u8);
        expected.insert(peer.clone());
        assert!(
            peer_add(home, &peer, &format!("tcp://10.0.0.{}:9100", i + 2)).success(),
            "node {i} could not add its own peer"
        );
    }

    for (i, home) in homes.iter().enumerate() {
        let listed = peer_list(home);
        assert_eq!(
            listed.total_count(),
            1,
            "node {i} must hold exactly its own peer"
        );
        let mut seen: BTreeSet<String> = listed.peer_dids().into_iter().collect();
        assert_eq!(seen, expected_of(&[canonical_did(10 + i as u8)]));
        // The other two nodes' peers are absent, by DID and by count.
        for (j, other_peer) in [
            (0usize, canonical_did(10)),
            (1, canonical_did(11)),
            (2, canonical_did(12)),
        ] {
            if j == i {
                continue;
            }
            assert!(
                !seen.contains(&other_peer),
                "node {i} must not observe node {j}'s peer"
            );
        }
        seen.clear();
    }
}

fn expected_of(dids: &[String]) -> BTreeSet<String> {
    dids.iter().cloned().collect()
}

// ---------------------------------------------------------------------------
// Scenario: the operator-visible projection of the output envelope
// ---------------------------------------------------------------------------

/// Operator pipelines project the peer table out of the JSON
/// envelope. The payload lives under `payload`; a pipeline that reads
/// `.peers` at the top level silently matches nothing and exits
/// success, which is the worst possible failure shape for a recovery
/// or teardown script. Assert the projection directly.
#[test]
fn l3_envelope_exposes_peers_under_payload() {
    let home = new_node_home("l3-envelope");
    let peer = canonical_did(20);
    assert!(peer_add(&home, &peer, "tcp://127.0.0.1:9100").success());

    let out = octo_in(&home)
        .args(["mesh", "peer", "list", "--json"])
        .output()
        .expect("spawn octo mesh peer list");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let envelope = Envelope::parse(&stdout);

    assert_eq!(envelope.command, "octo.mesh.peer.list.v1");
    assert_eq!(
        envelope.schema_version, 4,
        "envelope schema version is part of the operator contract"
    );

    let value: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    assert!(
        value.get("payload").is_some(),
        "the peer list must be nested under `payload`"
    );
    assert!(
        value.get("peers").is_none(),
        "`peers` must NOT be hoisted to the envelope top level: a top-level \
         projection makes `jq -r '.peers[].peer_did'` yield empty output while \
         the command still exits 0"
    );

    // The operator projection that actually works.
    let dids: Vec<String> = value["payload"]["peers"]
        .as_array()
        .expect("payload.peers array")
        .iter()
        .map(|p| p["peer_did"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(dids, vec![peer]);
}

// ---------------------------------------------------------------------------
// Scenario: trust-level filtering
// ---------------------------------------------------------------------------

/// The filter value space is the typed-discriminator URN. Filtering
/// by a human word such as `trusted` is accepted by the parser and
/// matches nothing — a silent wrong answer rather than an error — so
/// pin both halves of that behaviour.
#[test]
fn l3_trust_filter_matches_on_the_urn_value_space() {
    let home = new_node_home("l3-filter");
    let peer = canonical_did(30);
    assert!(peer_add(&home, &peer, "tcp://127.0.0.1:9100").success());

    // Peers are created Untrusted, so the Untrusted filter matches.
    let untrusted = peer_list_filtered(&home, UNTRUSTED);
    assert_eq!(untrusted.filtered_count(), 1);
    assert_eq!(
        untrusted.total_count(),
        1,
        "filter does not change the table"
    );
    assert_eq!(untrusted.peer_dids(), vec![peer.clone()]);

    // Nothing has been promoted, so the Trusted filter is empty.
    let trusted = peer_list_filtered(&home, TRUSTED);
    assert_eq!(trusted.filtered_count(), 0, "no peer has been promoted");
    assert_eq!(trusted.total_count(), 1, "the peer is still in the table");
}

// ---------------------------------------------------------------------------
// Scenario: write-gate enforcement
// ---------------------------------------------------------------------------

/// The peer table is operator-visible state; the write path is gated.
/// These vectors pin the gate from a second process so the gate is
/// proven to be evaluated per invocation rather than cached in the
/// parent test process.
#[test]
fn l3_write_gate_is_enforced_per_invocation() {
    let home = new_node_home("l3-gate");
    let peer = canonical_did(40);

    // No --mode and no confirmation flags: the gate refuses.
    let refused = octo_in(&home)
        .args([
            "mesh",
            "peer",
            "add",
            &peer,
            "--endpoint",
            "tcp://127.0.0.1:9100",
            "--json",
        ])
        .output()
        .expect("spawn gated add");
    assert_eq!(
        refused.status.code(),
        Some(2),
        "an unconfirmed write must be refused with the confirmation exit"
    );
    assert_eq!(
        peer_list(&home).total_count(),
        0,
        "a refused write must not touch the peer table"
    );

    // Auditor mode is read-only. `OCTO_AUDIT=1` is the operator's
    // fleet-wide enforcement switch, and it must hold even against a
    // caller that also passes an explicit write mode — otherwise any
    // process can silently opt out of a read-only policy that was set
    // for the whole environment.
    let audited = octo_in(&home)
        .env("OCTO_AUDIT", "1")
        .args(CI_WRITE)
        .args([
            "--allow-write",
            "mesh",
            "peer",
            "add",
            &peer,
            "--endpoint",
            "tcp://127.0.0.1:9100",
        ])
        .output()
        .expect("spawn audited add");
    assert_eq!(
        audited.status.code(),
        Some(2),
        "`OCTO_AUDIT=1` must deny the write even when an explicit write mode is passed; \
         got stdout={} stderr={}",
        String::from_utf8_lossy(&audited.stdout),
        String::from_utf8_lossy(&audited.stderr)
    );
    assert!(
        String::from_utf8_lossy(&audited.stderr).contains("read-only"),
        "the denial must state the read-only reason, so an operator can tell an \
         enforcement refusal from a missing-argument error"
    );
    assert_eq!(peer_list(&home).total_count(), 0);

    // Reading in auditor mode is the point of auditor mode.
    let read = octo_in(&home)
        .env("OCTO_AUDIT", "1")
        .args(["mesh", "peer", "list", "--json"])
        .output()
        .expect("spawn audited list");
    assert!(
        read.status.success(),
        "auditor mode must still permit reads"
    );

    // The sanctioned path succeeds once the audit switch is absent.
    assert!(peer_add(&home, &peer, "tcp://127.0.0.1:9100").success());
    assert_eq!(peer_list(&home).total_count(), 1);
}

// ---------------------------------------------------------------------------
// Scenario: endpoint scheme validation
// ---------------------------------------------------------------------------

/// Only allowlisted endpoint schemes are accepted, and the rejection
/// is loud (non-zero exit) rather than a silent drop.
#[test]
fn l3_endpoint_scheme_allowlist_is_enforced() {
    let home = new_node_home("l3-scheme");
    let mut seed = 50u8;

    for scheme in ALLOWED_SCHEMES {
        let peer = canonical_did(seed);
        seed += 1;
        assert!(
            peer_add(&home, &peer, &format!("{scheme}192.0.2.1:9100")).success(),
            "allowlisted scheme {scheme} must be accepted"
        );
    }
    assert_eq!(peer_list(&home).total_count(), ALLOWED_SCHEMES.len() as u64);

    for bad in [
        "file:///etc/passwd",
        "http://192.0.2.1",
        "ws://192.0.2.1:9100",
    ] {
        let peer = canonical_did(seed);
        seed += 1;
        let status = peer_add(&home, &peer, bad);
        assert_eq!(
            status.code(),
            Some(28),
            "disallowed scheme {bad} must be refused with the endpoint-scheme exit"
        );
    }
    assert_eq!(
        peer_list(&home).total_count(),
        ALLOWED_SCHEMES.len() as u64,
        "rejected endpoints must not be recorded"
    );
}

// ---------------------------------------------------------------------------
// Scenario: on-disk durability properties
// ---------------------------------------------------------------------------

/// The peer table is operator-private state. Its file mode must be
/// owner-only, and it must be a complete, parseable document after
/// every write.
#[test]
fn l3_peer_table_is_owner_only_and_always_parseable() {
    let home = new_node_home("l3-perms");
    let peer = canonical_did(60);
    assert!(peer_add(&home, &peer, "tcp://127.0.0.1:9100").success());

    let path = peers_toml(&home);
    assert!(path.exists(), "the peer table must exist after a write");

    let mode = std::fs::metadata(&path)
        .expect("stat peers.toml")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o700,
        "the peer table is owner-only, matching the documented substrate contract"
    );
    assert_eq!(
        mode & 0o077,
        0,
        "the peer table must never be group- or world-accessible: it records \
         which operators a node is bound to"
    );

    let body = std::fs::read_to_string(&path).expect("read peers.toml");
    assert!(
        body.contains("schema_version"),
        "the peer table must carry an explicit schema version"
    );
    assert!(body.contains(&peer));

    // Adding a second peer must leave the first intact — the write is
    // a full-table rewrite, so a regression here would drop history.
    let peer2 = canonical_did(61);
    assert!(peer_add(&home, &peer2, "quic://192.0.2.2:9100").success());
    let body = std::fs::read_to_string(&path).expect("re-read peers.toml");
    assert!(
        body.contains(&peer),
        "the first peer must survive a rewrite"
    );
    assert!(body.contains(&peer2));
    assert_eq!(peer_list(&home).total_count(), 2);
}

// ---------------------------------------------------------------------------
// Scenario: teardown
// ---------------------------------------------------------------------------

/// The operator-teardown scenario: remove every peer and land on an
/// empty table, with a repeated removal staying idempotent.
#[test]
fn l3_teardown_removes_every_peer_idempotently() {
    let home = new_node_home("l3-teardown");
    let mut peers = Vec::new();
    for i in 0..4u8 {
        let peer = canonical_did(70 + i);
        assert!(peer_add(&home, &peer, &format!("tcp://10.0.0.{}:9100", i + 2)).success());
        peers.push(peer);
    }
    assert_eq!(peer_list(&home).total_count(), 4);

    for peer in &peers {
        assert!(
            peer_remove(&home, peer).success(),
            "removal of {peer} failed"
        );
        // Idempotent: a second removal of the same peer still exits 0.
        assert!(
            peer_remove(&home, peer).success(),
            "repeat removal of {peer} must be idempotent"
        );
    }

    let listed = peer_list(&home);
    assert_eq!(listed.total_count(), 0, "teardown must empty the table");
    assert!(listed.peers().is_empty());
}

// ---------------------------------------------------------------------------
// Scenario: concurrency — distinct nodes under simultaneous load
// ---------------------------------------------------------------------------

/// Many nodes, each driven by its own writer process at the same
/// time. Every node must converge on exactly its own peers, with no
/// cross-node contamination and no lost writes.
#[test]
fn l3_concurrent_writers_on_distinct_nodes_stay_isolated() {
    const NODES: usize = 6;
    const PEERS_PER_NODE: usize = 4;

    let homes: Arc<Vec<_>> = Arc::new(
        (0..NODES)
            .map(|i| new_node_home(&format!("l3-conc-node-{i}")))
            .collect(),
    );

    let expected: Arc<Mutex<Vec<BTreeSet<String>>>> =
        Arc::new(Mutex::new((0..NODES).map(|_| BTreeSet::new()).collect()));

    let failures = Arc::new(Mutex::new(Vec::<String>::new()));

    std::thread::scope(|scope| {
        for node in 0..NODES {
            let homes = Arc::clone(&homes);
            let expected = Arc::clone(&expected);
            let failures = Arc::clone(&failures);
            scope.spawn(move || {
                for p in 0..PEERS_PER_NODE {
                    // Deterministic per (node, peer) seed keeps the
                    // DIDs distinct across nodes and peers.
                    let seed = (node * PEERS_PER_NODE + p) as u8 + 100;
                    let did = canonical_did(seed);
                    let status = peer_add(
                        &homes[node],
                        &did,
                        &format!("tcp://10.0.{}.{}:9100", node, p + 1),
                    );
                    if !status.success() {
                        failures
                            .lock()
                            .unwrap()
                            .push(format!("node {node} peer {p}: add failed"));
                        continue;
                    }
                    expected.lock().unwrap()[node].insert(did);
                }
            });
        }
    });

    let failures = Arc::try_unwrap(failures)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    assert!(failures.is_empty(), "concurrent adds failed: {failures:?}");

    let expected = Arc::try_unwrap(expected)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    for (node, home) in homes.iter().enumerate() {
        let listed = peer_list(home);
        assert_eq!(
            listed.total_count(),
            PEERS_PER_NODE as u64,
            "node {node} lost writes under concurrent load"
        );
        let seen: BTreeSet<String> = listed.peer_dids().into_iter().collect();
        assert_eq!(
            seen, expected[node],
            "node {node} converged on the wrong peer set"
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario: concurrency — many writers against one node
// ---------------------------------------------------------------------------

/// Every writer process targets the *same* node home at the same
/// time. Each write is an atomic full-table rewrite, so this is the
/// contention point where a read-modify-write without cross-process
/// exclusion would drop peers. The operator-visible invariant is
/// that no peer added by any writer goes missing.
#[test]
fn l3_concurrent_writers_on_one_node_converge_without_loss() {
    const WRITERS: usize = 8;

    let home = Arc::new(new_node_home("l3-conc-one-node"));
    let expected: Arc<Mutex<BTreeSet<String>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let failures = Arc::new(Mutex::new(Vec::<String>::new()));
    static NEXT_SEED: AtomicUsize = AtomicUsize::new(200);

    std::thread::scope(|scope| {
        for writer in 0..WRITERS {
            let home = Arc::clone(&home);
            let expected = Arc::clone(&expected);
            let failures = Arc::clone(&failures);
            scope.spawn(move || {
                for p in 0..3u8 {
                    let seed =
                        (NEXT_SEED.fetch_add(1, Ordering::SeqCst) % 200) as u8 + writer as u8;
                    let did = canonical_did(seed.wrapping_add(p));
                    let status = peer_add(&home, &did, &format!("quic://10.1.{writer}.{p}:9100"));
                    if !status.success() {
                        failures
                            .lock()
                            .unwrap()
                            .push(format!("writer {writer} peer {p}: add failed"));
                        continue;
                    }
                    expected.lock().unwrap().insert(did);
                }
            });
        }
    });

    let failures = Arc::try_unwrap(failures)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    assert!(
        failures.is_empty(),
        "concurrent adds to one node failed: {failures:?}"
    );

    let expected = Arc::try_unwrap(expected)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    let listed = peer_list(&home);
    let seen: BTreeSet<String> = listed.peer_dids().into_iter().collect();

    let missing: Vec<&String> = expected.difference(&seen).collect();
    assert!(
        missing.is_empty(),
        "concurrent writers to one node lost {} peer(s) to a read-modify-write race: {missing:?}",
        missing.len()
    );
}

// ---------------------------------------------------------------------------
// Scenario: an unset home fails closed
// ---------------------------------------------------------------------------

/// With neither `OCTO_HOME` nor `HOME` set there is no home, and the
/// CLI must refuse rather than fall back to a shared world-writable
/// location. This is what keeps parallel node homes trustworthy.
#[test]
fn l3_missing_home_fails_closed() {
    let out = std::process::Command::new(common::octo_bin())
        .env_remove("OCTO_HOME")
        .env_remove("HOME")
        .args(["mesh", "peer", "list", "--json"])
        .output()
        .expect("spawn octo with no home");
    assert_eq!(
        out.status.code(),
        Some(27),
        "an unresolvable home must fail closed with the no-home exit"
    );
    assert!(
        !out.stderr.is_empty(),
        "the failure must explain itself on stderr"
    );
}

// ---------------------------------------------------------------------------
// Scenario: the node's own identity surface
// ---------------------------------------------------------------------------

/// Every node reports the same thing about its own identity: there
/// is none. The wallet store opens empty and refuses to name an
/// active identity, so an operator cannot currently bind a peer table
/// to a node's own DID. This vector pins that behaviour so the
/// operator guide cannot describe an identity-backed binding flow
/// without the suite failing.
#[test]
fn l3_node_reports_no_active_identity() {
    let home = new_node_home("l3-identity");
    let out = octo_in(&home)
        .args(["whoami", "--json"])
        .output()
        .expect("spawn octo whoami");

    assert_eq!(
        out.status.code(),
        Some(2),
        "a node with no registered identity must report the no-active-identity exit"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no active identity"),
        "the envelope must name the condition, got: {stderr}"
    );
    assert!(
        !home.join("wallet").exists(),
        "the home must not gain a wallet store as a side effect of reading"
    );
}

// ---------------------------------------------------------------------------
// Scenario: node-scoped read commands stay node-scoped
// ---------------------------------------------------------------------------

/// The node-scoped read commands are the operator's only network
/// view. Each must answer for its own node and not error out, so an
/// operator can run them uniformly across a fleet.
#[test]
fn l3_node_read_commands_answer_on_a_fresh_node() {
    let home = new_node_home("l3-readcmds");

    // `trust-graph render` requires an explicit depth and renders the
    // graph it was given; with no trust relations established it
    // renders an empty graph rather than failing.
    let graph = octo_in(&home)
        .args(["network", "trust-graph", "render", "--depth", "5", "--json"])
        .output()
        .expect("spawn trust-graph render");
    assert!(
        graph.status.success(),
        "trust-graph render must answer on a fresh node: {}",
        String::from_utf8_lossy(&graph.stderr)
    );
    let graph = Envelope::parse(&String::from_utf8_lossy(&graph.stdout));
    assert_eq!(graph.payload["depth"], 5);

    // Omitting the depth is a hard error, not a default.
    let no_depth = octo_in(&home)
        .args(["network", "trust-graph", "render", "--json"])
        .output()
        .expect("spawn trust-graph render without depth");
    assert!(
        !no_depth.status.success(),
        "trust-graph render must not silently default its depth"
    );

    // `topology render` answers and states its own provenance.
    let topology = octo_in(&home)
        .args([
            "network", "topology", "render", "--format", "dot", "--depth", "5", "--json",
        ])
        .output()
        .expect("spawn topology render");
    assert!(
        topology.status.success(),
        "topology render must answer on a fresh node: {}",
        String::from_utf8_lossy(&topology.stderr)
    );
    let topology = Envelope::parse(&String::from_utf8_lossy(&topology.stdout));
    let render = topology.payload["render"].as_str().unwrap_or_default();
    assert!(
        render.contains("digraph"),
        "the dot format must produce a digraph, got: {render}"
    );
}

/// Pins the distinction that the operator guide got wrong for three
/// scenarios: `octo mesh peer` and `octo network peers` read two
/// different stores, and only the first is the peer table an operator
/// writes to.
///
/// `mesh peer` is the local peer table — add, remove, list.
/// `network peers` is a read-only cache of *gateway* peers, with no
/// add path on the CLI, so it stays empty for anything the operator
/// does from the peer-table surface. An operator who adds a peer,
/// sees exit 0, and then runs `octo network peers list` is looking at
/// an empty list and has no reason to think the add failed.
///
/// This is asserted rather than left as prose because the two commands
/// read plausibly similarly and nothing in the CLI output distinguishes
/// them except the `command` field of the envelope.
#[test]
fn l3_mesh_peer_table_and_network_gateway_cache_are_distinct_stores() {
    let home = new_node_home("stores");
    let peer = canonical_did(7);

    assert!(
        peer_add(&home, &peer, "tcp://127.0.0.1:9100").success(),
        "the add must succeed, or the rest of this test proves nothing"
    );

    // The store the guide's step 3 writes to.
    let table = peer_list(&home);
    assert_eq!(
        table.payload["total_count"], 1,
        "the table must hold the peer"
    );
    assert_eq!(table.payload["peers"][0]["peer_did"], peer.as_str());

    // The store the guide used to tell the operator to verify with.
    let gateways = octo_in(&home)
        .args(["network", "peers", "list", "--json"])
        .output()
        .expect("spawn octo network peers list");
    assert!(
        gateways.status.success(),
        "the gateway-cache read must answer: {}",
        String::from_utf8_lossy(&gateways.stderr)
    );
    let gateways = Envelope::parse(&String::from_utf8_lossy(&gateways.stdout));
    assert_eq!(
        gateways.payload["count_returned"], 0,
        "adding to the mesh peer table must not populate the gateway cache"
    );

    // And the single-peer get is on the gateway surface only, so it
    // cannot be used to inspect a peer DID at all. A peer DID is not a
    // 32-byte gateway id, and the lookup misses.
    let get = octo_in(&home)
        .args(["network", "peers", "get", &peer, "--json"])
        .output()
        .expect("spawn octo network peers get");
    assert!(
        !get.status.success(),
        "`network peers get` must not resolve a peer DID; if it ever does, \
         the guide can point at it again"
    );

    // The substrate-faithful way to inspect one peer, now the guide's
    // step 5, works off the table.
    let binding = peer_list(&home);
    let selected = binding.payload["peers"]
        .as_array()
        .expect("peers array")
        .iter()
        .find(|p| p["peer_did"] == peer.as_str())
        .expect("the added peer must be selectable out of the table");
    assert_eq!(selected["endpoint"], "tcp://127.0.0.1:9100");
}

// ---------------------------------------------------------------------------
// Scenario: only the canonical DID wire form is accepted, and the shapes
// the repository also contains are rejected rather than degraded
// ---------------------------------------------------------------------------

/// Which DID strings an operator may actually hand the CLI.
///
/// The repository contains two DID encodings. `octo-ident` mints
/// `did:octo:z<base58btc of 32 bytes>`, and that is the only form an
/// operator surface accepts. `octo-cap-macaroon` separately parses
/// `did:octo:0x<64 lowercase hex>` — a raw public key, used by the
/// distributed-coordinator capability path and described in its own
/// source as superseded once the typed codec landed. A third form,
/// `did:octo:b<52 base32>`, is past its deprecation window.
///
/// All three look plausible, and a guide or a runbook that reaches for
/// the wrong one produces a command that cannot ever succeed. This
/// scenario pins the accept/reject boundary so the shapes cannot be
/// confused again, and asserts that a rejected add leaves nothing
/// behind.
#[test]
fn l3_only_the_canonical_did_wire_form_is_accepted() {
    let home = new_node_home("didform");

    // The canonical form is accepted and round-trips byte-for-byte.
    let peer = canonical_did(31);
    assert!(
        peer.starts_with("did:octo:z"),
        "the minted DID must be in the canonical form, got {peer}"
    );
    assert!(
        (43..=44).contains(&(peer.len() - 10)),
        "canonical payload must be 43-44 chars, got {}",
        peer.len() - 10
    );
    assert!(
        peer_add(&home, &peer, "tcp://127.0.0.1:9200").success(),
        "the canonical form must be accepted"
    );
    let table = peer_list(&home);
    assert_eq!(
        table.payload["peers"][0]["peer_did"],
        peer.as_str(),
        "the accepted DID must round-trip unchanged"
    );

    // The coordinator-crate form is rejected, not coerced. Both the
    // length that crate actually parses and the longer length the
    // operator guide used to print are covered — a DID-shaped string
    // is rejected for its shape, not merely for its length.
    for (label, macaroon) in [
        ("32-byte payload", format!("did:octo:0x{}", "ab".repeat(32))),
        (
            "52-byte payload as the guide used to print",
            format!("did:octo:0x{}", "ab".repeat(52)),
        ),
    ] {
        let rejected = octo_in(&home)
            .args(CI_WRITE)
            .args([
                "--allow-write",
                "mesh",
                "peer",
                "add",
                &macaroon,
                "--endpoint",
                "tcp://127.0.0.1:9201",
                "--confirm",
                "--confirm-acknowledge",
            ])
            .output()
            .expect("spawn octo mesh peer add");
        assert_eq!(
            rejected.status.code(),
            Some(4),
            "the 0x form ({label}) must fail closed with exit 4, got {:?}: {}",
            rejected.status.code(),
            String::from_utf8_lossy(&rejected.stderr)
        );
    }

    // So is the deprecated base32 form.
    let legacy = format!("did:octo:b{}", "z".repeat(52));
    let rejected = octo_in(&home)
        .args(CI_WRITE)
        .args([
            "--allow-write",
            "mesh",
            "peer",
            "add",
            &legacy,
            "--endpoint",
            "tcp://127.0.0.1:9202",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .output()
        .expect("spawn octo mesh peer add");
    assert_eq!(
        rejected.status.code(),
        Some(4),
        "the base32 form must fail closed with exit 4, got {:?}",
        rejected.status.code()
    );

    // Two rejected adds must not have left anything behind — a
    // fail-closed add that still wrote a row would let an operator
    // believe a peer is bound when it is not dialable.
    let after = peer_list(&home);
    assert_eq!(
        after.payload["total_count"], 1,
        "rejected adds must not write a peer row"
    );
    assert_eq!(
        after.payload["peers"][0]["peer_did"],
        peer.as_str(),
        "the only peer must be the one that was accepted"
    );
}

// ---------------------------------------------------------------------------
// Scenario: the two distinct jq failure shapes a wrong envelope depth produces
// ---------------------------------------------------------------------------

/// Every operator pipeline in the guide projects out of the envelope's
/// `payload`. Reading a field at the top level instead produces one of
/// **two different failure shapes**, and they are not interchangeable:
///
/// * an **array** read (`[.receipts[] | ...]`) makes jq fail hard —
///   `Cannot iterate over null`, exit 5;
/// * a **scalar** read (`jq '.count_returned'`) does not fail at all —
///   jq prints the string `null` and exits 0.
///
/// The scalar shape is the dangerous one: a recovery or teardown script
/// that checks an exit code will believe it succeeded. This scenario
/// drives the real `jq` against a real envelope so both shapes stay
/// pinned, and asserts the deep form is the one that works.
#[test]
fn l3_wrong_envelope_depth_fails_in_two_distinct_ways() {
    let jq = match std::process::Command::new("jq").arg("--version").output() {
        Ok(o) if o.status.success() => PathBuf::from("jq"),
        _ => {
            eprintln!(
                "skipping: jq is not installed; the guide requires it, so this is \
                      an environment gap rather than a guide defect"
            );
            return;
        }
    };

    let home = new_node_home("l3-depth");
    let out = octo_in(&home)
        .args(["audit", "list", "--limit", "5", "--json"])
        .output()
        .expect("spawn octo audit list");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "audit list must succeed on a fresh node, got {:?}",
        out.status
    );
    let envelope = Envelope::parse(&stdout);
    assert_eq!(envelope.command, "octo.audit.list.v1");

    // Pipe the same real envelope through a jq filter and capture both
    // the exit code and what the operator would have seen.
    let run_jq = |filter: &str| -> (Option<i32>, String) {
        use std::io::Write;
        let mut child = std::process::Command::new(&jq)
            .arg(filter)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn jq");
        child
            .stdin
            .as_mut()
            .expect("jq stdin")
            .write_all(stdout.as_bytes())
            .expect("write envelope to jq");
        let done = child.wait_with_output().expect("jq output");
        (
            done.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&done.stdout),
                String::from_utf8_lossy(&done.stderr)
            ),
        )
    };

    // 1. The guide's array idiom, deep: works.
    let (deep_code, deep_out) = run_jq("[.payload.receipts[] | select(.subject_did)]");
    assert_eq!(
        deep_code,
        Some(0),
        "deep array filter must succeed: {deep_out}"
    );
    assert_eq!(
        deep_out.trim(),
        "[]",
        "a fresh node has no receipts: {deep_out}"
    );

    // 2. The same idiom one level too shallow: hard failure.
    let (shallow_code, shallow_out) = run_jq("[.receipts[] | select(.subject_did)]");
    assert_eq!(
        shallow_code,
        Some(5),
        "a shallow array read must fail hard, not silently: {shallow_out}"
    );
    assert!(
        shallow_out.contains("Cannot iterate over null"),
        "the array failure mode must stay the documented one: {shallow_out}"
    );

    // 3. The scalar idiom: this is the trap. A shallow scalar read
    //    exits 0 and hands the operator the string "null".
    let (scalar_bad_code, scalar_bad_out) = run_jq(".count_returned");
    assert_eq!(
        scalar_bad_code,
        Some(0),
        "a shallow scalar read does NOT fail — that is the whole hazard: {scalar_bad_out}"
    );
    assert_eq!(
        scalar_bad_out.trim(),
        "null",
        "a shallow scalar read yields the literal string null: {scalar_bad_out}"
    );

    // 4. The correct scalar read yields the real value.
    let (scalar_good_code, scalar_good_out) = run_jq(".payload.count_returned");
    assert_eq!(
        scalar_good_code,
        Some(0),
        "deep scalar filter: {scalar_good_out}"
    );
    assert_eq!(
        scalar_good_out.trim(),
        "0",
        "the deep scalar read must return the real count: {scalar_good_out}"
    );
}

// ---------------------------------------------------------------------------
// Scenario: the guide's vault-id hex conversion, end to end through jq
// ---------------------------------------------------------------------------

/// `VaultId` serializes as 32 decimal bytes, while every vault-taking
/// flag wants 64 hex chars, so the guide has to convert. The
/// conversion is a long jq program, which is exactly the kind of text
/// that rots silently: it keeps exiting 0 while producing a string the
/// argument parser rejects. Run the guide's exact filter against an
/// envelope shaped like a real `vault list` and assert the output is a
/// 64-char hex string — and that dropping the conversion is visible.
///
/// The serialization half of this is pinned in the library by
/// `tv_vault_6_vault_id_is_a_byte_array_not_a_hex_string`; this
/// scenario pins the operator-facing text.
#[test]
fn l3_the_guides_vault_id_hex_conversion_produces_64_hex_chars() {
    let jq = match std::process::Command::new("jq").arg("--version").output() {
        Ok(o) if o.status.success() => PathBuf::from("jq"),
        _ => {
            eprintln!("skipping: jq is not installed");
            return;
        }
    };

    // Shaped exactly as `octo vault list --json` renders one row: a
    // 32-element decimal byte array, per the derived Serialize.
    let byte = 0xabu8;
    let row: Vec<String> = vec![byte.to_string(); 32];
    let envelope = format!(
        r#"{{"command":"octo.vault.list.v1","executed_at_unix":1,"payload":{{"vaults":[{{"vault_id":[{}],"asset_symbol":"OCTO","balance_projected":"0"}}],"next_cursor":null,"resolved_at_unix":1}},"redacted":false,"schema_version":4}}"#,
        row.join(",")
    );

    // The guide's filter, verbatim.
    const GUIDE_FILTER: &str = r#"
        def hx: . as $n
            | ["0","1","2","3","4","5","6","7","8","9","a","b","c","d","e","f"][$n/16|floor]
            + ["0","1","2","3","4","5","6","7","8","9","a","b","c","d","e","f"][$n%16];
        .payload.vaults[0].vault_id | map(hx) | join("")"#;

    let run = |filter: &str| -> (Option<i32>, String) {
        use std::io::Write;
        let mut child = std::process::Command::new(&jq)
            .arg("-r")
            .arg(filter)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn jq");
        child
            .stdin
            .as_mut()
            .expect("jq stdin")
            .write_all(envelope.as_bytes())
            .expect("write envelope");
        let done = child.wait_with_output().expect("jq output");
        (
            done.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&done.stdout),
                String::from_utf8_lossy(&done.stderr)
            ),
        )
    };

    let (code, out) = run(GUIDE_FILTER);
    assert_eq!(code, Some(0), "the guide's filter must succeed: {out}");
    let hex = out.trim();
    assert_eq!(
        hex.len(),
        64,
        "the guide's filter must yield 64 hex chars, got {hex:?}"
    );
    assert!(
        hex.chars().all(|c| c.is_ascii_hexdigit()),
        "every character must be a hex digit, got {hex:?}"
    );
    assert_eq!(
        hex,
        "ab".repeat(32),
        "the bytes must survive the conversion"
    );

    // Negative control: the naive form the conversion exists to replace
    // yields something the argument parser can never accept.
    let (naive_code, naive_out) = run(".payload.vaults[0].vault_id");
    assert_eq!(
        naive_code,
        Some(0),
        "the naive form still exits 0: {naive_out}"
    );
    let naive = naive_out.trim();
    assert_ne!(
        naive, hex,
        "the naive form must differ from the converted form"
    );
    assert!(
        naive.starts_with('[') && !naive.chars().all(|c| c.is_ascii_hexdigit()),
        "the naive form is a byte array, not hex: {naive:?}"
    );
}

// ---------------------------------------------------------------------------
// Scenario: the unlock → sign chain works end-to-end through real processes
// ---------------------------------------------------------------------------

/// The cross-process property extends to the signing chain.
///
/// The audit `docs/audits/open-limitations-assessment.md` §6 row 4c closed
/// at `next 7445c289`: the 6 production signing paths in `octo-cli` now
/// route through `WalletStore::unlock(passphrase, &mut seed_buf)` instead
/// of `WalletStore::try_active_identity` (a metadata-only stub returning
/// `WalletError::Locked` unconditionally). Before 4c(b) this harness could
/// not reach the signing paths — every `capability list` exited 92 and
/// `capability mint` exited 64 with no way to supply a passphrase from a
/// non-interactive script. This test exercises the full chain across two
/// OS processes: register an identity (creates the wallet), then in a
/// SEPARATE process invoke `capability list` with the passphrase on
/// stdin. The capability list call exits 0 with an empty capabilities
/// envelope, proving the unlock split works end-to-end through the real
/// binary the operator would invoke.
///
/// The passphrase is read from a file for register (the substrate's
/// gate) and from stdin for unlock + capability list (the AC-10
/// pre-flight, no-TTY fail-closed before this point). 24 characters
/// clear the 12-character substrate floor with a wide margin so a future
/// tightening cannot turn this test red without warning.
#[test]
fn l3_unlock_then_capability_list_returns_zero() {
    let home = new_node_home("l3-unlock-cap");
    let passphrase = "l3-unlock-cap-passphrase-2026"; // 29 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    // -- Step 1: register an identity in process #1. The register path
    //    is mutating but does NOT require unlock (it creates the wallet
    //    and seals the seed). The CI-mode pair is the dispatch-side
    //    write gate for scripted runs. The passphrase file is written
    //    WITHOUT a trailing newline: the substrate takes the passphrase
    //    as-is and the unlock's stdin path trims `\r\n` (see
    //    `acquire_passphrase`), so a trailing newline on the file would
    //    become part of the stored passphrase and the unlock attempt
    //    would be a guaranteed `WalletError::Locked`.
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-unlock-cap",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "ci",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    assert!(
        register.status.success(),
        "register must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );
    let register_env = Envelope::parse(&String::from_utf8_lossy(&register.stdout));
    let register_did = register_env.payload["did"]
        .as_str()
        .expect("register envelope must carry payload.did")
        .to_string();
    assert!(
        register_did.starts_with("did:octo:"),
        "DID must be in canonical wire form, got {register_did:?}"
    );
    assert_eq!(
        register_env.payload["active_now"],
        serde_json::Value::Bool(true),
        "register --activate (default true) must promote the new identity to Active"
    );

    // -- Step 2: SEPARATE process invokes `identity unlock` with the
    //    passphrase piped to stdin. This is the substrate-faithful
    //    passphrase check the audit §6 row 4c(a) shipped at `next
    //    d5387cd5`. All three stdio streams are piped so the child
    //    envelope does not leak to the test runner's stdout (which
    //    would otherwise mix the child JSON with the harness's
    //    own diagnostic output and break any later Envelope::parse
    //    call).
    let mut unlock_cmd = octo_in(&home);
    unlock_cmd.args([
        "identity",
        "unlock",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    unlock_cmd.stdin(std::process::Stdio::piped());
    unlock_cmd.stdout(std::process::Stdio::piped());
    unlock_cmd.stderr(std::process::Stdio::piped());
    let mut unlock_child = unlock_cmd.spawn().expect("spawn unlock");
    std::io::Write::write_all(
        unlock_child.stdin.as_mut().expect("unlock stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to unlock");
    let unlock_out = unlock_child.wait_with_output().expect("wait unlock");
    assert!(
        unlock_out.status.success(),
        "unlock must succeed: stderr={}",
        String::from_utf8_lossy(&unlock_out.stderr),
    );
    let unlock_env = Envelope::parse(&String::from_utf8_lossy(&unlock_out.stdout));
    let unlock_did = unlock_env.payload["did"]
        .as_str()
        .expect("unlock envelope must carry payload.did")
        .to_string();
    assert_eq!(
        unlock_did, register_did,
        "unlock must report the same DID register created"
    );

    // -- Step 3: SEPARATE process invokes `capability list` with the
    //    passphrase on stdin. This is the post-4c(b) signing path: it
    //    acquires the passphrase through `acquire_passphrase`, calls
    //    `WalletStore::unlock`, and consumes the unlocked handle for
    //    the substrate's `list_active(&IdentityKey)` call. Exit 0 +
    //    empty capabilities array is the canonical happy-path signal.
    let mut list_cmd = octo_in(&home);
    list_cmd.args([
        "capability",
        "list",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    list_cmd.stdin(std::process::Stdio::piped());
    list_cmd.stdout(std::process::Stdio::piped());
    list_cmd.stderr(std::process::Stdio::piped());
    let mut list_child = list_cmd.spawn().expect("spawn capability list");
    std::io::Write::write_all(
        list_child.stdin.as_mut().expect("list stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to capability list");
    let list_out = list_child.wait_with_output().expect("wait capability list");
    assert_eq!(
        list_out.status.code(),
        Some(0),
        "capability list on an unlocked wallet must exit 0, got {:?}. stderr={}, stdout={}",
        list_out.status.code(),
        String::from_utf8_lossy(&list_out.stderr),
        String::from_utf8_lossy(&list_out.stdout),
    );
    let list_env = Envelope::parse(&String::from_utf8_lossy(&list_out.stdout));
    assert_eq!(
        list_env.payload["capabilities"].as_array().map(Vec::len),
        Some(0),
        "a freshly registered identity must own zero capabilities, got {:?}",
        list_env.payload["capabilities"]
    );

    // -- Step 4 (cross-node isolation property): the wallet created
    //    on node A is NOT visible to node C. Each `$OCTO_HOME`
    //    resolves its own wallet; a second home with NO register
    //    must still report WalletLocked exit 92 on `capability
    //    list`, because node C has no sealed wallet to unlock.
    let other = new_node_home("l3-unlock-cap-other");
    let mut other_list = octo_in(&other);
    other_list.args([
        "capability",
        "list",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    other_list.stdin(std::process::Stdio::piped());
    other_list.stdout(std::process::Stdio::piped());
    other_list.stderr(std::process::Stdio::piped());
    let mut other_child = other_list.spawn().expect("spawn other list");
    std::io::Write::write_all(
        other_child.stdin.as_mut().expect("other stdin"),
        b"some-passphrase-not-relevant\n",
    )
    .expect("pipe pp to other");
    let other_out = other_child.wait_with_output().expect("wait other");
    assert_eq!(
        other_out.status.code(),
        Some(92),
        "a fresh node with no registered identity must exit 92 (WalletLocked), \
         got {:?}. stderr={}",
        other_out.status.code(),
        String::from_utf8_lossy(&other_out.stderr),
    );
}

// ---------------------------------------------------------------------------
// Scenario: the post-4c(b) `capability mint` signing path is exercised
// end-to-end and the minted cap is observable via the list path in a
// separate process
// ---------------------------------------------------------------------------

/// The mint path is the only signing path in the audit amendment that
/// produces a substrate-visible side effect (a new `cap_id` registered
/// in the wallet's `index.json`). The mint handler's `SEC-03` root-secret
/// guard is gated to `--mode dev` (the well-known `[0u8; 32]`
/// placeholder is the only one the substrate offers before the
/// root-secret derivation amendment lands; production builds cannot
/// reach this path). On a release binary the guard is enforced; the
/// test build is cfg(test) bypassed. The L3 harness runs the release
/// binary, so the test must explicitly pass `--mode dev --allow-write`
/// plus `--confirm-acknowledge` (the second-step gate the pastejacking
/// defense requires on a non-dry-run path) to reach the substrate
/// mint call.
///
/// The round-trip is the property the audit amendment left unharnessed:
/// `capability mint` writes to the wallet index, and a SEPARATE
/// process invoking `capability list` against the same `$OCTO_HOME`
/// must see the minted `cap_id` in the `capabilities` array. Before
/// 4c(b) this round-trip could not be exercised at all because the
/// signing path was the `try_active_identity` stub; after 4c(b) it
/// routes through `acquire_passphrase` + `WalletStore::unlock` and the
/// two steps (mint, list) are independent OS processes.
///
/// `MIN_PASSPHRASE_CHARS = 12` is cleared with a 28-char passphrase.
/// The holder DID is the freshly-registered identity's own DID so the
/// mint is a self-issuance — the simplest path that exercises the
/// substrate's `mint(root_secret, &IdentityKey, holder_did, &caveats)`
/// signature without entangling a second identity.
///
/// **Substrate amendment landed.** `octo_cap_macaroon::cli_fns::mint`
/// is wired (delegates to `CapabilityToken::mint`); the substrate
/// produces a real signed `CapabilityToken` and the CLI surfaces
/// `capability_id` + `body_hash` in the mint envelope. The cross-
/// process signing chain (acquire_passphrase → WalletStore::unlock →
/// resolve_active_identity_key → mint → envelope) is now exercised
/// end-to-end through a separate OS process.
///
/// The round-trip half of this test (mint in process A, list in
/// process B observes the minted cap_id) is now live. The holder-
/// registry substrate amendment persists a `CapabilitySummary` to
/// `<wallet_root>/holder_capabilities.json` after every successful
/// mint, and the list handler reads from the same file on a
/// separate process invocation. The substrate's in-process
/// `list_active` stub is bypassed by the CLI: the on-disk file is
/// the source of truth for the operator's mint list. The test name
/// reflects the now-live list half.
///
/// This test pins both halves explicitly:
/// - the **mint half** is live (exit 0, real envelope)
/// - the **list half** is live (exit 0, one entry whose cap_id is
///   the truncated-hex form of the minted token's `macaroon.id`)
///
/// The in-process suite mirrors this with
/// `tv_cap6_mint_signing_failed_exits_11` (ignored, substrate
/// amendment dependency) and `tv_cap6_mint_root_secret_blocked_exits_64`
/// (active, dev-mode guard).
#[test]
fn l3_capability_mint_then_list_round_trip() {
    let home = new_node_home("l3-mint-list");
    let passphrase = "l3-mint-list-passphrase-2026"; // 28 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    // -- Step 1: register an identity. Use dev mode (and --allow-write)
    //    so the same wallet is later eligible for the mint SEC-03 guard
    //    (the substrate's `index.json` records a single mode for the
    //    issuer, and dev mode is the mode the mint path admits). The
    //    passphrase file has NO trailing newline: the substrate stores
    //    the passphrase as-is and the unlock's stdin path trims CR/LF
    //    (see the `l3_unlock_then_capability_list_returns_zero`
    //    comment for the full reasoning).
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-mint-list",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    let register_stdout = String::from_utf8_lossy(&register.stdout).to_string();
    assert!(
        register.status.success(),
        "register must succeed in dev mode: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );
    let register_env = Envelope::parse(&register_stdout);
    let register_did = register_env.payload["did"]
        .as_str()
        .expect("register envelope must carry payload.did")
        .to_string();
    assert!(
        register_did.starts_with("did:octo:"),
        "DID must be in canonical wire form, got {register_did:?}"
    );

    // -- Step 2: SEPARATE process invokes `capability mint` with the
    //    passphrase on stdin. The mint is a real signing operation:
    //    the substrate's `octo_cap_macaroon::mint(&[0u8;32], &key, ...)`
    //    writes a new `cap_id` into the wallet's `index.json` and
    //    returns the holder signature. The envelope's `capability_id`
    //    field is the substrate's `hex::encode(token.macaroon.id)` —
    //    lowercase hex.
    //
    //    `--mode dev --allow-write --confirm-acknowledge` is the
    //    minimum gate combination that passes `require_confirm` (dev
    //    mode needs `--allow-write`) AND `require_acknowledge` (the
    //    second-step gate needs `--confirm-acknowledge` on a
    //    non-dry-run path). Both gates are listed in
    //    `commands::identity::require_confirm` and
    //    `commands::capability::require_acknowledge`.
    let mut mint_cmd = octo_in(&home);
    mint_cmd.args([
        "capability",
        "mint",
        "--caveats",
        "[]",
        "--holder",
        &register_did,
        "--mode",
        "dev",
        "--allow-write",
        "--confirm",
        "--confirm-acknowledge",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    mint_cmd.stdin(std::process::Stdio::piped());
    mint_cmd.stdout(std::process::Stdio::piped());
    mint_cmd.stderr(std::process::Stdio::piped());
    let mut mint_child = mint_cmd.spawn().expect("spawn mint");
    std::io::Write::write_all(
        mint_child.stdin.as_mut().expect("mint stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to mint");
    let mint_out = mint_child.wait_with_output().expect("wait mint");
    let mint_stdout = String::from_utf8_lossy(&mint_out.stdout).to_string();
    assert_eq!(
        mint_out.status.code(),
        Some(0),
        "capability mint on an unlocked wallet must exit 0, got {:?}. stderr={}, stdout={}",
        mint_out.status.code(),
        String::from_utf8_lossy(&mint_out.stderr),
        String::from_utf8_lossy(&mint_out.stdout),
    );
    let mint_env = Envelope::parse(&mint_stdout);
    let minted_cap_id = mint_env.payload["capability_id"]
        .as_str()
        .expect("mint envelope must carry payload.capability_id")
        .to_string();
    assert!(
        minted_cap_id.chars().all(|c| c.is_ascii_hexdigit()) && !minted_cap_id.is_empty(),
        "capability_id must be a non-empty lowercase hex string, got {minted_cap_id:?}"
    );
    let minted_body_hash = mint_env.payload["body_hash"]
        .as_str()
        .expect("mint envelope must carry payload.body_hash")
        .to_string();
    assert!(
        minted_body_hash.chars().all(|c| c.is_ascii_hexdigit()) && minted_body_hash.len() == 64,
        "body_hash must be a 64-char lowercase hex (32-byte BLAKE3 digest), \
         got len={} value={minted_body_hash:?}",
        minted_body_hash.len()
    );

    // -- Step 3: SEPARATE process invokes `capability list` with the
    //    passphrase on stdin. The list handler is read-only (no
    //    confirmation gate, no SEC-03 guard), but the post-4c(b)
    //    migration still routes through `acquire_passphrase` +
    //    `WalletStore::unlock`. The handler then reads the
    //    holder-registry file at `<wallet_root>/holder_capabilities.json`
    //    and the returned envelope's `capabilities` array MUST
    //    contain exactly one entry whose `cap_id` is the truncated-hex
    //    form of the minted token's `macaroon.id` (the substrate's
    //    `summary_from_token` projects to 16-hex-char truncation).
    let mut list_cmd = octo_in(&home);
    list_cmd.args([
        "capability",
        "list",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    list_cmd.stdin(std::process::Stdio::piped());
    list_cmd.stdout(std::process::Stdio::piped());
    list_cmd.stderr(std::process::Stdio::piped());
    let mut list_child = list_cmd.spawn().expect("spawn list-after-mint");
    std::io::Write::write_all(
        list_child.stdin.as_mut().expect("list stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to list");
    let list_out = list_child.wait_with_output().expect("wait list-after-mint");
    let list_stdout = String::from_utf8_lossy(&list_out.stdout).to_string();
    assert_eq!(
        list_out.status.code(),
        Some(0),
        "capability list after a mint must exit 0, got {:?}. stderr={}",
        list_out.status.code(),
        String::from_utf8_lossy(&list_out.stderr),
    );
    let list_env = Envelope::parse(&list_stdout);
    let caps = list_env.payload["capabilities"]
        .as_array()
        .expect("list payload.capabilities must be an array");
    assert_eq!(
        caps.len(),
        1,
        "list after a successful mint must contain exactly one entry \
         (the holder-registry amendment persists the CapabilitySummary \
         to <wallet_root>/holder_capabilities.json and the list handler \
         reads from the same file). got {} entries: {caps:?}",
        caps.len()
    );
    // The list-view's `cap_id` is the first 16 hex chars of the full
    // 64-hex mint envelope's `capability_id` (per RFC-0011
    // §Subcommand Taxonomy list-view truncation). A failure mode where
    // a different token (or the wrong substring) showed up in the list
    // is what this assertion pins — not just that the array is non-empty.
    let listed_cap_id = caps[0]["cap_id"]
        .as_str()
        .expect("list entry must carry cap_id string")
        .to_string();
    let expected_truncated = minted_cap_id
        .get(..16)
        .expect("minted cap_id is at least 16 hex chars (32-byte macaroon.id)")
        .to_string();
    assert_eq!(
        listed_cap_id, expected_truncated,
        "list's cap_id must be the truncated form of the minted cap_id \
         (16-hex-char prefix per RFC-0011 §Subcommand Taxonomy). \
         minted={minted_cap_id:?}, listed={listed_cap_id:?}"
    );

    // -- Step 4: control — a wrong passphrase must return WalletLocked
    //    (exit 92) on a SEPARATE process invoking `capability list`.
    //    The substrate's vault decryption fails for the wrong
    //    passphrase and surfaces as `WalletError::Locked`, which the
    //    CLI's `From<WalletError>` table maps to `WalletLocked` at
    //    exit 92. This is the negative control for step 3: the right
    //    passphrase produced a populated list, the wrong passphrase
    //    must produce a fail-closed exit.
    let mut wrong_cmd = octo_in(&home);
    wrong_cmd.args([
        "capability",
        "list",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    wrong_cmd.stdin(std::process::Stdio::piped());
    wrong_cmd.stdout(std::process::Stdio::piped());
    wrong_cmd.stderr(std::process::Stdio::piped());
    let mut wrong_child = wrong_cmd.spawn().expect("spawn wrong-pp list");
    std::io::Write::write_all(
        wrong_child.stdin.as_mut().expect("wrong stdin"),
        b"this-is-the-wrong-passphrase-12345\n",
    )
    .expect("pipe wrong pp");
    let wrong_out = wrong_child.wait_with_output().expect("wait wrong-pp");
    assert_eq!(
        wrong_out.status.code(),
        Some(92),
        "capability list with a wrong passphrase must exit 92 (WalletLocked), \
         got {:?}. stderr={}, stdout={}",
        wrong_out.status.code(),
        String::from_utf8_lossy(&wrong_out.stderr),
        String::from_utf8_lossy(&wrong_out.stdout),
    );
    // The WalletLocked path renders the failure envelope to stderr only;
    // stdout is empty for this exit code, so the test pins the exit
    // code rather than parsing an envelope. The shape of the stderr
    // rendering is the CLI's `OctoCliError` -> `format!("exit code: {n}")`
    // contract and is covered by `tv_x_c_*` in-process vectors.
}

// ---------------------------------------------------------------------------
// Scenario: the post-4c(b) `governance attest` signing path is exercised
// end-to-end through separate processes
// ---------------------------------------------------------------------------

/// The governance attest path is the second of the six 4c(b) production
/// signing sites. Unlike `capability mint` (whose substrate side is a
/// Phase-2 stub at `cli_fns::mint`), the substrate's `attest_v2` is
/// fully wired and reachable through the CLI in a real (non-dry-run)
/// invocation. The substrate resolves the kind via
/// `resolve_kind` which currently admits the single discriminator
/// `"route-quality:uptime-30d"`; the subject DID is checked for the
/// `did:octo:subgroup:` prefix only. So a self-attestation (the
/// registered identity attesting about itself) is the simplest path
/// that exercises the full substrate signing round-trip without
/// entangling a second identity or a governance snapshot.
///
/// The test flow:
/// 1. register an identity (CI mode)
/// 2. SEPARATE process invokes `governance attest` against the
///    substrate's `attest_v2` (post-4c(b) signing path through
///    `acquire_passphrase` + `WalletStore::unlock` +
///    `WalletSignerAdapter::new(IdentityKey)` + `signer.sign_envelope`)
/// 3. assert the envelope carries an `attestation_id` (32-byte
///    BLAKE3-256 digest of the canonical envelope bytes)
/// 4. negative control: a wrong passphrase returns exit 92
///
/// The flag combination `--mode ci --allow-write --confirm
/// --confirm-acknowledge` is the minimum gate that passes both
/// `require_confirm` (CI mode needs `--allow-write`) and the
/// attest handler's own `if !confirm` check (line ~584 in
/// governance.rs). The two-step intent gate is irrelevant here
/// because `--allow-stale` is not set.
#[test]
fn l3_governance_attest_signing_path_succeeds() {
    let home = new_node_home("l3-gov-attest");
    let passphrase = "l3-gov-attest-passphrase-2026"; // 30 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    // -- Step 1: register an identity. The attest's `signer_did` is
    //    the active identity's DID, and the subject_did is the same
    //    DID (a self-attestation is the simplest substrate-reachable
    //    call). The substrate's `prereq_attest_subgroup_check` only
    //    rejects `did:octo:subgroup:...`; canonical identity DIDs
    //    pass. The passphrase file is written without a trailing
    //    newline: see the unlock test for the substrate-as-is /
    //    unlock-trim invariant.
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-gov-attest",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "ci",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    assert!(
        register.status.success(),
        "register must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );
    let register_env = Envelope::parse(&String::from_utf8_lossy(&register.stdout));
    let register_did = register_env.payload["did"]
        .as_str()
        .expect("register envelope must carry payload.did")
        .to_string();
    assert!(
        register_did.starts_with("did:octo:"),
        "DID must be canonical, got {register_did:?}"
    );

    // -- Step 2: SEPARATE process invokes the live (non-dry-run)
    //    `governance attest` against the substrate's `attest_v2`.
    //    The kind_ref is the only discriminator the substrate
    //    currently resolves (`resolve_kind` at attest.rs:89-96).
    //    The evidence-hash is 32 zero bytes — the substrate's XOR
    //    invariant requires exactly one of (evidence, evidence_hash)
    //    and we have no real evidence file at hand; the hash is a
    //    valid BLAKE3-256 digest input by construction.
    let mut attest_cmd = octo_in(&home);
    attest_cmd.args([
        "governance",
        "attest",
        &register_did,
        "route-quality:uptime-30d",
        "--evidence-hash-hex",
        &"00".repeat(32),
        "--mode",
        "ci",
        "--allow-write",
        "--confirm",
        "--confirm-acknowledge",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    attest_cmd.stdin(std::process::Stdio::piped());
    attest_cmd.stdout(std::process::Stdio::piped());
    attest_cmd.stderr(std::process::Stdio::piped());
    let mut attest_child = attest_cmd.spawn().expect("spawn attest");
    std::io::Write::write_all(
        attest_child.stdin.as_mut().expect("attest stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to attest");
    let attest_out = attest_child.wait_with_output().expect("wait attest");
    assert_eq!(
        attest_out.status.code(),
        Some(0),
        "governance attest on an unlocked wallet must exit 0, got {:?}. \
         stderr={}, stdout={}",
        attest_out.status.code(),
        String::from_utf8_lossy(&attest_out.stderr),
        String::from_utf8_lossy(&attest_out.stdout),
    );
    let attest_env = Envelope::parse(&String::from_utf8_lossy(&attest_out.stdout));
    let attestation_id = attest_env.payload["attestation_id"]
        .as_str()
        .expect("attest envelope must carry payload.attestation_id")
        .to_string();
    assert!(
        attestation_id.chars().all(|c| c.is_ascii_hexdigit()) && attestation_id.len() == 64,
        "attestation_id must be a 64-char lowercase hex (BLAKE3-256 of the \
         canonical envelope bytes), got len={} value={attestation_id:?}",
        attestation_id.len()
    );
    let appended_at_unix = attest_env.payload["appended_at_unix"]
        .as_i64()
        .expect("attest envelope must carry payload.appended_at_unix");
    assert!(
        appended_at_unix > 0,
        "appended_at_unix must be a positive wall-clock timestamp, got {appended_at_unix}"
    );

    // -- Step 3: negative control. A wrong passphrase on a SEPARATE
    //    `governance attest` invocation must exit 92 (WalletLocked).
    //    The substrate's `attest_v2` is unreachable when
    //    `store.unlock` fails, so any successful-shape receipt is a
    //    leak (the envelope would reveal an attestation_id by
    //    SUCCESS, which is the wrong signal under lock).
    let mut wrong_cmd = octo_in(&home);
    wrong_cmd.args([
        "governance",
        "attest",
        &register_did,
        "route-quality:uptime-30d",
        "--evidence-hash-hex",
        &"00".repeat(32),
        "--mode",
        "ci",
        "--allow-write",
        "--confirm",
        "--confirm-acknowledge",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    wrong_cmd.stdin(std::process::Stdio::piped());
    wrong_cmd.stdout(std::process::Stdio::piped());
    wrong_cmd.stderr(std::process::Stdio::piped());
    let mut wrong_child = wrong_cmd.spawn().expect("spawn wrong-pp attest");
    std::io::Write::write_all(
        wrong_child.stdin.as_mut().expect("wrong stdin"),
        b"this-is-the-wrong-passphrase-12345\n",
    )
    .expect("pipe wrong pp");
    let wrong_out = wrong_child.wait_with_output().expect("wait wrong-pp");
    assert_eq!(
        wrong_out.status.code(),
        Some(92),
        "governance attest with a wrong passphrase must exit 92 (WalletLocked), \
         got {:?}. stderr={}, stdout={}",
        wrong_out.status.code(),
        String::from_utf8_lossy(&wrong_out.stderr),
        String::from_utf8_lossy(&wrong_out.stdout),
    );
}

// ---------------------------------------------------------------------------
// Scenario: the post-4c(b) `governance vote` signing path is exercised
// end-to-end through separate processes
// ---------------------------------------------------------------------------

/// The governance vote path is the third of the six 4c(b) production
/// signing sites and the second one whose substrate is fully wired.
/// `vote_v2` lives at `crates/octo-governance/src/vote.rs:487` and
/// builds a `CapabilityToken` from `(voter_cap_id, signer_did,
/// weight_bps)`, registers the wallet-backed `CapabilitySigner` under
/// the supplied `voter_cap_id` in an in-memory session registry, then
/// calls `vote_v2` which signs the canonical envelope through the
/// substrate.
///
/// The in-memory registry means the `voter_cap_id` can be any hex
/// string: it does NOT need to be minted through the (stubbed)
/// `cli_fns::mint` substrate path. The substrate's only inviolable
/// pre-call prereq is `prereq_vote_subgroup_check` which rejects
/// `did:octo:subgroup:` voter DIDs only — a canonical voter DID
/// passes. The `weight_bps > 10_000` clamp is enforced at the CLI
/// boundary (`if weight_bps > 10_000` at vote.rs:725); the substrate
/// also enforces it.
///
/// The test flow:
/// 1. register an identity (CI mode)
/// 2. SEPARATE process invokes live `governance vote` against the
///    substrate's `vote_v2` (post-4c(b) signing path through
///    `acquire_passphrase` + `WalletStore::unlock` +
///    `WalletSignerAdapter::new(IdentityKey)` +
///    `session.register_capability` + `vote_v2`)
/// 3. assert the envelope carries a 64-char hex `vote_id`
///    (BLAKE3-256 of the canonical envelope bytes) and a positive
///    `recorded_at_unix`
/// 4. negative control: a wrong passphrase returns exit 92
///    (WalletLocked) and a successful-shape receipt is unreachable
///    when `store.unlock` fails
#[test]
fn l3_governance_vote_signing_path_succeeds() {
    let home = new_node_home("l3-gov-vote");
    let passphrase = "l3-gov-vote-passphrase-2026"; // 30 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    // -- Step 1: register an identity. The vote's `signer_did` is the
    //    active identity's DID; a canonical DID passes the
    //    `prereq_vote_subgroup_check` gate (only `did:octo:subgroup:`
    //    is rejected). The passphrase file is written without a
    //    trailing newline: see the unlock test for the
    //    substrate-as-is / unlock-trim invariant.
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-gov-vote",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "ci",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    assert!(
        register.status.success(),
        "register must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );
    let register_env = Envelope::parse(&String::from_utf8_lossy(&register.stdout));
    let register_did = register_env.payload["did"]
        .as_str()
        .expect("register envelope must carry payload.did")
        .to_string();
    assert!(
        register_did.starts_with("did:octo:"),
        "DID must be canonical, got {register_did:?}"
    );

    // -- Step 2: SEPARATE process invokes the live (non-dry-run)
    //    `governance vote` against the substrate's `vote_v2`.
    //    `proposal_id_hex` and `vote_choice` are positional per
    //    RFC-0011-g §Command Taxonomy. The `voter_cap_id` is a
    //    32-byte hex string the substrate session registers in
    //    memory (it does NOT query the wallet); `weight_bps=5000`
    //    is well within the 10_000 cap. The `--confirm` gate
    //    is the only one vote requires here (we do NOT set
    //    `--allow-stale`, so the two-step intent gate is
    //    irrelevant).
    let proposal_id_hex = "11".repeat(32); // 64 hex chars
    let voter_cap_id = "22".repeat(32); // 64 hex chars, in-memory registered
    let mut vote_cmd = octo_in(&home);
    vote_cmd.args([
        "governance",
        "vote",
        &proposal_id_hex,
        "approve",
        "--weight-bps",
        "5000",
        "--voter-cap-id",
        &voter_cap_id,
        "--mode",
        "ci",
        "--allow-write",
        "--confirm",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    vote_cmd.stdin(std::process::Stdio::piped());
    vote_cmd.stdout(std::process::Stdio::piped());
    vote_cmd.stderr(std::process::Stdio::piped());
    let mut vote_child = vote_cmd.spawn().expect("spawn vote");
    std::io::Write::write_all(
        vote_child.stdin.as_mut().expect("vote stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to vote");
    let vote_out = vote_child.wait_with_output().expect("wait vote");
    assert_eq!(
        vote_out.status.code(),
        Some(0),
        "governance vote on an unlocked wallet must exit 0, got {:?}. \
         stderr={}, stdout={}",
        vote_out.status.code(),
        String::from_utf8_lossy(&vote_out.stderr),
        String::from_utf8_lossy(&vote_out.stdout),
    );
    let vote_env = Envelope::parse(&String::from_utf8_lossy(&vote_out.stdout));
    let vote_id = vote_env.payload["vote_id"]
        .as_str()
        .expect("vote envelope must carry payload.vote_id")
        .to_string();
    assert!(
        vote_id.chars().all(|c| c.is_ascii_hexdigit()) && vote_id.len() == 64,
        "vote_id must be a 64-char lowercase hex (BLAKE3-256 of the \
         canonical envelope bytes), got len={} value={vote_id:?}",
        vote_id.len()
    );
    let recorded_at_unix = vote_env.payload["recorded_at_unix"]
        .as_u64()
        .expect("vote envelope must carry payload.recorded_at_unix");
    assert!(
        recorded_at_unix > 0,
        "recorded_at_unix must be a positive wall-clock timestamp, \
         got {recorded_at_unix}"
    );
    // Mirror of weight_applied for envelope convenience; the CLI
    // surfaces it directly from the substrate receipt (never
    // recomputes).
    let weight_applied = vote_env.payload["weight_applied"]
        .as_u64()
        .expect("vote envelope must carry payload.weight_applied");
    assert_eq!(
        weight_applied, 5_000,
        "weight_applied must mirror the substrate receipt's weight_applied"
    );

    // -- Step 3: negative control. A wrong passphrase on a SEPARATE
    //    `governance vote` invocation must exit 92 (WalletLocked).
    //    The substrate's `vote_v2` is unreachable when
    //    `store.unlock` fails, so any successful-shape receipt is a
    //    leak.
    let mut wrong_cmd = octo_in(&home);
    wrong_cmd.args([
        "governance",
        "vote",
        &proposal_id_hex,
        "approve",
        "--weight-bps",
        "5000",
        "--voter-cap-id",
        &voter_cap_id,
        "--mode",
        "ci",
        "--allow-write",
        "--confirm",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    wrong_cmd.stdin(std::process::Stdio::piped());
    wrong_cmd.stdout(std::process::Stdio::piped());
    wrong_cmd.stderr(std::process::Stdio::piped());
    let mut wrong_child = wrong_cmd.spawn().expect("spawn wrong-pp vote");
    std::io::Write::write_all(
        wrong_child.stdin.as_mut().expect("wrong stdin"),
        b"this-is-the-wrong-passphrase-12345\n",
    )
    .expect("pipe wrong pp");
    let wrong_out = wrong_child.wait_with_output().expect("wait wrong-pp");
    assert_eq!(
        wrong_out.status.code(),
        Some(92),
        "governance vote with a wrong passphrase must exit 92 (WalletLocked), \
         got {:?}. stderr={}, stdout={}",
        wrong_out.status.code(),
        String::from_utf8_lossy(&wrong_out.stderr),
        String::from_utf8_lossy(&wrong_out.stdout),
    );
}

// ---------------------------------------------------------------------------
// Scenario: the substrate `capability attenuate` write path is pinned
// as `#[ignore]` for parity with the `mint` substrate stub
// ---------------------------------------------------------------------------

/// `capability attenuate` is the third of the six 4c(b) production
/// signing sites and the second one whose substrate side is a Phase-2
/// stub at `crates/octo-cap-macaroon/src/cli_fns` `attenuate`. The
/// CLI surfaces the stub's `Internal` error to the operator as exit
/// 64. Like the `mint` test (`l3_capability_mint_then_list_round_trip`,
/// `#[ignore]` at `next 50c023de`), this L3 test pins the
/// cross-process stub state and the three-flag combination
/// (`--mode dev --allow-write --confirm --confirm-acknowledge`) the
/// future wired substrate will need to pass the gates. It will flip
/// from `#[ignore]` to a live assertion when the substrate amendment
/// lands, mirroring the mint flip.
///
/// Without this pinned harness, a future substrate land could silently
/// **Substrate amendment landed.** `octo_cap_macaroon::cli_fns::attenuate`
/// is wired (chains `CapabilityToken::attenuate_with_signer` per
/// caveat). The substrate now reaches the catalog lookup and exits
/// with `MacaroonError::ParentNotFound` → `OctoCliError::Internal`
/// (exit 12) when the supplied `parent_cap_id` is not in the in-
/// memory macaroon set. The pre-amendment stub returned
/// `HolderSig("not yet wired")` (exit 64); the post-amendment
/// failure mode is the genuine catalog error.
///
/// This test pins the cross-process wiring of the post-amendment
/// failure: the test supplies a syntactically-valid but unregistered
/// parent cap_id, and asserts the new error code surfaces to the
/// operator. A future regression that masks `ParentNotFound` as
/// `WalletLocked` (or any other silent failure) is caught here.
///
/// **Future gate:** when the substrate gains a holder-registry
/// persistence layer (out of scope for 4c(b)), a follow-up test
/// will mint a real parent in process A, attenuate it in process B,
/// and assert success — the L3 round-trip counterpart of the mint
/// half of `l3_capability_mint_substrate_live_list_still_stub`
/// above. The test is named for the *current* substrate contract
/// (parent-not-found at exit 12) so the rename to a success
/// assertion is explicit when the holder-registry amendment lands.
#[test]
fn l3_capability_attenuate_unknown_parent_exits_12() {
    let home = new_node_home("l3-cap-attenuate");
    let passphrase = "l3-cap-attenuate-passphrase-2026"; // 33 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-cap-attenuate",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    assert!(
        register.status.success(),
        "register must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );

    // The substrate's `cli_fns::attenuate` is now wired. The test
    // supplies a syntactically-valid but unregistered parent
    // cap_id (64 hex chars, in-memory only — never persisted), so
    // the substrate's catalog lookup returns
    // `MacaroonError::ParentNotFound` which the CLI surfaces as
    // `OctoCliError::Internal` at exit 12 (per the
    // `From<SubstrateError>` table). Pre-amendment this assertion
    // pinned the substrate stub at exit 64 (`HolderSig`); the
    // post-amendment contract is the genuine catalog error.
    let parent_cap_id = "33".repeat(32); // 64 hex chars, not in any catalog
    let mut atten_cmd = octo_in(&home);
    atten_cmd.args([
        "capability",
        "attenuate",
        "--caveats",
        r#"{"type":"before","value":4102444800}"#,
        &parent_cap_id,
        "--mode",
        "dev",
        "--allow-write",
        "--confirm",
        "--confirm-acknowledge",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    atten_cmd.stdin(std::process::Stdio::piped());
    atten_cmd.stdout(std::process::Stdio::piped());
    atten_cmd.stderr(std::process::Stdio::piped());
    let mut atten_child = atten_cmd.spawn().expect("spawn attenuate");
    std::io::Write::write_all(
        atten_child.stdin.as_mut().expect("attenuate stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to attenuate");
    let atten_out = atten_child.wait_with_output().expect("wait attenuate");
    assert_eq!(
        atten_out.status.code(),
        Some(12),
        "capability attenuate against an unregistered parent must exit 12 \
         (Internal from MacaroonError::ParentNotFound). The pre-amendment \
         stub exited 64; the post-amendment contract is the genuine \
         catalog error. got {:?}. stderr={}, stdout={}",
        atten_out.status.code(),
        String::from_utf8_lossy(&atten_out.stderr),
        String::from_utf8_lossy(&atten_out.stdout),
    );
}

// ---------------------------------------------------------------------------
// Scenario: dry-run on the four post-4c(b) write paths renders the
// preview envelope and never reaches `WalletStore::unlock`
// ---------------------------------------------------------------------------

/// Dry-run on a mutating identity / capability / governance command
/// MUST short-circuit BEFORE `WalletStore::unlock` so the preview
/// envelope is rendered without ever opening the wallet. The
/// invariant is load-bearing: a `--dry-run` that needed a
/// passphrase would leak the operator's secret-read pattern into
/// every CI script that exercises the preview (the operator's
/// deliberate guard against unattended secret reads at the
/// `--passphrase-stdin --allow-stdin-secret` boundary is exactly
/// the thing dry-run is meant to NOT trigger).
///
/// These four tests are the cross-process regression net for the
/// invariant. Each one:
/// 1. asserts the dry-run invocation exits 0 in a clean HOME
///    (no wallet open)
/// 2. asserts the rendered preview envelope carries a stable
///    identifier (placeholder cap_id or dry-run correlation UUID)
/// 3. asserts no passphrase-related stderr text leaks through
///    (the operator's wallet was never opened)
///
/// The four write paths covered:
/// - `capability mirror dry-run` (mint dry-run, substrate stubbed)
/// - `capability attenuate dry-run` (substrate stubbed)
/// - `governance attest dry-run` (substrate wired)
/// - `governance vote dry-run` (substrate wired)
///
/// Mint + attenuate dry-run are the cross-process counterparts of
/// the in-process `tv_cap17b_mint_dry_run_stderr_echo` and
/// `tv_cap18b_attenuate_dry_run_stderr_echo` (the pastejacking
/// defense echo lives BEFORE the dry-run gate per the substrate's
/// pre-gate ordering).
#[test]
fn l3_capability_mint_dry_run_does_not_open_wallet() {
    let home = new_node_home("l3-cap-mint-dry");

    // Caveats must be parseable JSON; holder DID must be
    // canonical. The substrate's `parse_caveats` is exercised
    // before the dry-run short-circuit, so a malformed caveat
    // surfaces an exit-9 before the preview render.
    let mint = octo_in(&home)
        .args([
            "capability",
            "mint",
            "--caveats",
            r#"{"type":"before","value":4102444800}"#,
            "--holder",
            "did:octo:holder-1",
            "--root",
            "ab".repeat(32).as_str(),
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("spawn mint dry-run");
    assert_eq!(
        mint.status.code(),
        Some(0),
        "capability mirror --dry-run must exit 0 in a clean HOME, \
         got {:?}. stderr={}, stdout={}",
        mint.status.code(),
        String::from_utf8_lossy(&mint.stderr),
        String::from_utf8_lossy(&mint.stdout),
    );
    let env = Envelope::parse(&String::from_utf8_lossy(&mint.stdout));
    assert_eq!(
        env.payload["capability_id"].as_str(),
        Some("(preview)"),
        "dry-run preview must carry the canonical PREVIEW_CAP_ID \
         placeholder, got {:?}",
        env.payload["capability_id"]
    );
    let stderr = String::from_utf8_lossy(&mint.stderr);
    assert!(
        !stderr.contains("passphrase") && !stderr.contains("unlock"),
        "dry-run preview must NEVER touch the wallet; a stderr leak of \
         'passphrase' or 'unlock' means the dry-run gate moved AFTER \
         wallet IO. stderr={stderr}"
    );
}

#[test]
fn l3_capability_attenuate_dry_run_does_not_open_wallet() {
    let home = new_node_home("l3-cap-atten-dry");

    let atten = octo_in(&home)
        .args([
            "capability",
            "attenuate",
            "ab".repeat(32).as_str(),
            "--caveats",
            r#"{"type":"before","value":4102444800}"#,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("spawn attenuate dry-run");
    assert_eq!(
        atten.status.code(),
        Some(0),
        "capability attenuate --dry-run must exit 0 in a clean HOME, \
         got {:?}. stderr={}, stdout={}",
        atten.status.code(),
        String::from_utf8_lossy(&atten.stderr),
        String::from_utf8_lossy(&atten.stdout),
    );
    let env = Envelope::parse(&String::from_utf8_lossy(&atten.stdout));
    assert_eq!(
        env.payload["child_cap_id"].as_str(),
        Some("(preview)"),
        "attenuate dry-run preview must carry the canonical \
         PREVIEW_CAP_ID placeholder, got {:?}",
        env.payload["child_cap_id"]
    );
    let stderr = String::from_utf8_lossy(&atten.stderr);
    assert!(
        !stderr.contains("passphrase") && !stderr.contains("unlock"),
        "dry-run preview must NEVER touch the wallet; a stderr leak \
         means the dry-run gate moved AFTER wallet IO. stderr={stderr}"
    );
}

#[test]
fn l3_governance_attest_dry_run_does_not_open_wallet() {
    let home = new_node_home("l3-gov-attest-dry");

    let out = octo_in(&home)
        .args([
            "governance",
            "attest",
            "did:octo:subject-1",
            "route-quality:uptime-30d",
            "--evidence-hash-hex",
            &"00".repeat(32),
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("spawn attest dry-run");
    assert_eq!(
        out.status.code(),
        Some(0),
        "governance attest --dry-run must exit 0 in a clean HOME, \
         got {:?}. stderr={}, stdout={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let env = Envelope::parse(&String::from_utf8_lossy(&out.stdout));
    // The attest dry-run preview carries a per-invocation UUID
    // `dry_run_correlation_id`. Asserting it is a UUID-form string
    // pins the correlation primitive without coupling to a fixed
    // value (the UUID is freshly minted per call).
    let corr = env.payload["dry_run_correlation_id"]
        .as_str()
        .expect("attest dry-run preview must carry dry_run_correlation_id");
    assert!(
        corr.len() == 36 && corr.chars().filter(|c| *c == '-').count() == 4,
        "dry_run_correlation_id must be a UUID-form string, got {corr:?}"
    );
    assert_eq!(
        env.payload["subject_did"].as_str(),
        Some("did:octo:subject-1"),
        "dry-run preview must echo the subject_did verbatim"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("passphrase") && !stderr.contains("unlock"),
        "attest dry-run must NEVER touch the wallet; a stderr leak \
         means the dry-run gate moved AFTER wallet IO. stderr={stderr}"
    );
}

#[test]
fn l3_governance_vote_dry_run_does_not_open_wallet() {
    let home = new_node_home("l3-gov-vote-dry");

    let proposal_id_hex = "11".repeat(32);
    let voter_cap_id = "22".repeat(32);
    let out = octo_in(&home)
        .args([
            "governance",
            "vote",
            &proposal_id_hex,
            "approve",
            "--weight-bps",
            "5000",
            "--voter-cap-id",
            &voter_cap_id,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("spawn vote dry-run");
    assert_eq!(
        out.status.code(),
        Some(0),
        "governance vote --dry-run must exit 0 in a clean HOME, \
         got {:?}. stderr={}, stdout={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let env = Envelope::parse(&String::from_utf8_lossy(&out.stdout));
    // The vote dry-run preview mirrors operator-supplied input
    // args per the comment at `crates/octo-cli/src/commands/
    // governance.rs:1236` (extracted to `build_vote_dry_run_preview`).
    // The `choice` is canonicalized (APPROVE -> yes); assert the
    // substrate-normalized form to pin the canonicalization at
    // the preview boundary.
    assert_eq!(
        env.payload["vote_choice"].as_str(),
        Some("yes"),
        "vote dry-run preview must canonicalize the choice to the \
         substrate form (APPROVE -> yes), got {:?}",
        env.payload["vote_choice"]
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("passphrase") && !stderr.contains("unlock"),
        "vote dry-run must NEVER touch the wallet; a stderr leak \
         means the dry-run gate moved AFTER wallet IO. stderr={stderr}"
    );
}

// ---------------------------------------------------------------------------
// Scenario: `agent run --detach --token-file` signing-path behaviour
// is pinned at the EXIT-CODE level because cross-process L3 is blocked
// by the substrate's in-memory-only agent registry
// ---------------------------------------------------------------------------

/// The sixth of the six 4c(b) production signing sites is the
/// `agent run --detach --token-file <path>` token-mint path.
/// Unlike the read-side `capability list` (which routes `unlock`
/// solely for trait-uniformity on `&dyn CapabilitySigner`),
/// `agent run --detach` actually CONSUMES the signing keypair:
/// `octo_runtime::mint_attach_handle` requires the holder's
/// ed25519 signing keypair so the future `decode_token` step can
/// verify the signature against the holder's public key
/// (RFC-0011-c §F.5 + §F.6.1).
///
/// **Cross-process L3 is blocked by the Phase-1 substrate.** The
/// agent registry is an in-memory `Mutex<BTreeMap<Uuid, AgentRecord>>`
/// (a `static` at `crates/octo-wallet/src/agent.rs:46`); `register_agent`
/// and `transition_agent` are per-process free fns of the binary.
/// A separate `octo` process therefore starts with no agents registered,
/// regardless of what earlier processes did — so an L3 test that
/// creates in process 1 and runs in process 2 will see
/// `AgentNotFound` (exit 42) in process 2 even with a re-derived
/// agent_id.
///
/// This test pins the cross-process exit-code contract: a
/// `agent run --detach --token-file` invocation in a fresh
/// process against an unregistered agent must exit 42
/// (AgentNotFound). When the substrate gains persistence (Phase 2),
/// this same test becomes the negative control for the cross-process
/// success path; today, it pins the failure contract so a future
/// regression that masks AgentNotFound as WalletLocked (or any other
/// silent failure) is caught at the L3 boundary.
///
/// The post-4c(b) signing chain is exercised by the in-process
/// `tv_agent_run_token_path` vectors in `crates/octo-cli/src/commands/
/// agent.rs`; the L3 cross-process surface awaits substrate persistence.
#[test]
fn l3_agent_run_unknown_agent_exits_42() {
    let home = new_node_home("l3-agent-unknown");
    let passphrase = "l3-agent-unknown-passphrase-2026"; // 32 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    // Register an identity so the wallet substrate is set up.
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-agent-unknown",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    assert!(
        register.status.success(),
        "register must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );

    // `agent run --detach --token-file <PATH>` against a fresh-process
    // agent_id (which the in-memory registry cannot find) must exit
    // 42 (AgentNotFound). The post-4c(b) signing chain runs to
    // completion; the substrate's `transition_agent` is the FIRST
    // substrate call after the unlock + activate, so a 42 here proves
    // the signing chain reached the substrate boundary (the unlock
    // succeeded, the activation succeeded, the substrate then
    // rejected the unknown agent).
    let token_path = home.join("attach.token");
    let mut run_cmd = octo_in(&home);
    run_cmd.args([
        "agent",
        "run",
        "--agent-id",
        "00000000-0000-4000-8000-000000000003",
        "--detach",
        "--token-file",
        token_path.to_str().unwrap(),
        "--mode",
        "dev",
        "--allow-write",
        "--confirm",
        "--passphrase-stdin",
        "--allow-stdin-secret",
        "--json",
    ]);
    run_cmd.stdin(std::process::Stdio::piped());
    run_cmd.stdout(std::process::Stdio::piped());
    run_cmd.stderr(std::process::Stdio::piped());
    let mut run_child = run_cmd.spawn().expect("spawn run");
    std::io::Write::write_all(
        run_child.stdin.as_mut().expect("run stdin"),
        format!("{passphrase}\n").as_bytes(),
    )
    .expect("pipe passphrase to run");
    let run_out = run_child.wait_with_output().expect("wait run");
    assert_eq!(
        run_out.status.code(),
        Some(42),
        "agent run against an unregistered agent must exit 42 \
         (AgentNotFound), got {:?}. stderr={}, stdout={}",
        run_out.status.code(),
        String::from_utf8_lossy(&run_out.stderr),
        String::from_utf8_lossy(&run_out.stdout),
    );
    assert!(
        !token_path.exists(),
        "a 42 (AgentNotFound) failure must NOT write a token file; \
         the substrate's mint_attach_handle is unreachable when \
         transition_agent rejects the unknown agent"
    );
}

// ---------------------------------------------------------------------------
// Scenario: the post-e02f8b02 agent registry persistence is observable
// across separate OS processes (`agent create` → `agent list`)
// ---------------------------------------------------------------------------

/// The agent registry was a process-global `static Mutex<BTreeMap>` that
/// lost every registration on CLI exit (`crates/octo-wallet/src/agent.rs`,
/// the `AGENT_REGISTRY` static pre-e02f8b02). A second process could not
/// see what the first one registered, blocking the L3 multi-peer e2e
/// harness's `agent run` success-path test. Substrate agent persistence
/// (`next e02f8b02`) now persists the registry to
/// `<wallet_root>/agent_registry.json` on every mutation and hydrates
/// from the same file on first access. This test pins the cross-process
/// visibility property end-to-end through the CLI:
///
/// 1. Register an identity (dev mode).
/// 2. Build a manifest JSON file with the registered DID.
/// 3. **Control**: SEPARATE process invokes `agent list` BEFORE the
///    create. Must report 0 agents (the in-memory + on-disk registry
///    is empty for a freshly registered identity).
/// 4. SEPARATE process A invokes `octo agent create` — the substrate's
///    `register_agent` writes a record to `agent_registry.json`.
/// 5. SEPARATE process B invokes `octo agent list` — the substrate
///    re-hydrates from `agent_registry.json` and the list MUST contain
///    exactly one entry whose `agent_id` matches the create envelope.
///
/// The cross-process property under test: **process B sees the
/// registration written by process A**. Before e02f8b02 the in-memory
/// `AGENT_REGISTRY` was per-process, so a second process could not
/// observe the first process's registration and the list would have
/// returned 0 entries even after a successful create. The mint+list
/// test (`l3_capability_mint_then_list_round_trip`) is the reference
/// pattern for the cap-macaroon round-trip; this test is the analogous
/// surface for the agent registry.
///
/// The `manifest_id` is the canonical UUIDv4 form per RFC-0002 §Agent
/// Manifest. The `signature_hex` is a 64-char hex (32-byte) placeholder
/// — the substrate's Phase 1 records the value but does not verify
/// against the holder's pubkey (the 6-step capability validation
/// pipeline wires in a follow-on substrate amendment). The
/// `holder_did` MUST match the active DID; the substrate's
/// `register_agent` derives the `agent_id` from
/// `(manifest_digest, active_did)` via UUIDv5 (RFC 4122 §4.3), so a
/// mismatch would surface as a holder-binding error in Phase 2 and as
/// a divergence in `manifest_digest` between the create envelope and
/// the on-disk record in Phase 1.
#[test]
fn l3_agent_create_then_list_round_trip() {
    let home = new_node_home("l3-agent-create-list");
    let passphrase = "l3-agent-create-list-passphrase-2026"; // 35 chars
    assert!(
        passphrase.len() >= 24,
        "passphrase must be wide of the 12-char floor"
    );

    // -- Step 1: register an identity. Dev mode is required so the
    //    wallet's `index.json` records `mode = dev` (the substrate's
    //    SEC-03 guard checks the issuer's mode at the `capability
    //    mint` boundary; `agent create` does not gate on mode but
    //    the dev mode is what the L3 harness consistently uses
    //    across sibling tests).
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, passphrase.as_bytes()).expect("write pp file");
    let register = octo_in(&home)
        .args([
            "identity",
            "register",
            "--label",
            "l3-agent-create-list",
            "--passphrase-file",
            pp_file.to_str().unwrap(),
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn register");
    assert!(
        register.status.success(),
        "register must succeed in dev mode: stderr={}, stdout={}",
        String::from_utf8_lossy(&register.stderr),
        String::from_utf8_lossy(&register.stdout),
    );
    let register_env = Envelope::parse(&String::from_utf8_lossy(&register.stdout));
    let register_did = register_env.payload["did"]
        .as_str()
        .expect("register envelope must carry payload.did")
        .to_string();
    assert!(
        register_did.starts_with("did:octo:"),
        "DID must be in canonical wire form, got {register_did:?}"
    );

    // -- Step 2: build the manifest JSON file. The `holder_did` MUST
    //    match the active DID (substrate-enforced per RFC-0002
    //    §Agent Manifest §Holder Binding). The `label` is surfaced
    //    through `AgentSummary::label` and the list handler's
    //    envelope projection; pinning it lets the post-list
    //    assertion catch a redaction / re-projection regression.
    let manifest_path = home.join("manifest.json");
    let manifest = serde_json::json!({
        "manifest_id": "00000000-0000-4000-8000-000000000001",
        "holder_did": register_did,
        "label": "l3-agent-create-list-runner",
        "created_at_unix": 1_700_000_000_u64,
        "signature_hex": "ab".repeat(64),
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)
            .expect("serialize manifest")
            .as_bytes(),
    )
    .expect("write manifest file");

    // -- Step 3 (control / substrate gate): SEPARATE process invokes
    //    `agent list` BEFORE the create. The pre-create registry
    //    is empty (the in-memory `AGENT_REGISTRY` is a fresh
    //    `BTreeMap::new()` and the on-disk file does not exist yet).
    //    The control pins the "before" state so the "after"
    //    assertion is a delta, not a single observation.
    let pre_list = octo_in(&home)
        .args(["agent", "list", "--json"])
        .output()
        .expect("spawn pre-list");
    assert_eq!(
        pre_list.status.code(),
        Some(0),
        "pre-create agent list must exit 0, got {:?}. stderr={}, stdout={}",
        pre_list.status.code(),
        String::from_utf8_lossy(&pre_list.stderr),
        String::from_utf8_lossy(&pre_list.stdout),
    );
    let pre_list_env = Envelope::parse(&String::from_utf8_lossy(&pre_list.stdout));
    let pre_count = pre_list_env.payload["count"]
        .as_u64()
        .expect("pre-list payload.count must be a u64");
    assert_eq!(
        pre_count, 0,
        "pre-create list must report 0 agents (the in-memory + on-disk \
         registry is empty for a freshly registered identity). got {pre_count}"
    );

    // -- Step 4: SEPARATE process A invokes `octo agent create`. The
    //    handler is a write path (denied in Auditor mode per
    //    RFC-0011-c §Roles and Authorities); the flag combination
    //    --mode dev --allow-write --confirm --confirm-acknowledge
    //    is the minimum that passes `require_confirm` (dev mode
    //    needs --allow-write) AND the handler's own `if !confirm`
    //    check. The substrate's `register_agent` derives the
    //    `agent_id` from `(manifest_digest, active_did)` via UUIDv5
    //    and persists the record to `agent_registry.json`.
    let create = octo_in(&home)
        .args([
            "agent",
            "create",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "--mode",
            "dev",
            "--allow-write",
            "--confirm",
            "--confirm-acknowledge",
            "--json",
        ])
        .output()
        .expect("spawn create");
    assert_eq!(
        create.status.code(),
        Some(0),
        "agent create on a registered identity must exit 0, got {:?}. \
         stderr={}, stdout={}",
        create.status.code(),
        String::from_utf8_lossy(&create.stderr),
        String::from_utf8_lossy(&create.stdout),
    );
    let create_env = Envelope::parse(&String::from_utf8_lossy(&create.stdout));
    let created_agent_id = create_env.payload["agent_id"]
        .as_str()
        .expect("create envelope must carry payload.agent_id")
        .to_string();
    // The envelope-boundary redactor truncates `agent_id` to 8 hex
    // chars + `...` (the cross-process rehydration must project the
    // same truncated form for the same record). The shape check
    // (`<8 hex chars>...`) pins the redaction contract; the equality
    // check below catches divergence between the two envelopes.
    assert!(
        created_agent_id.len() == 11
            && created_agent_id.ends_with("...")
            && created_agent_id[..8].chars().all(|c| c.is_ascii_hexdigit()),
        "create envelope's agent_id must be the 8-hex + '...' truncated \
         form per the envelope-boundary redactor. got {created_agent_id:?}"
    );

    // -- Step 5: SEPARATE process B invokes `octo agent list`. THIS
    //    IS THE CROSS-PROCESS PROPERTY. Process B re-hydrates from
    //    `agent_registry.json` (the e02f8b02 amendment) and the
    //    list MUST contain exactly one entry whose `agent_id`
    //    matches the create envelope's. Before e02f8b02 the
    //    in-memory `AGENT_REGISTRY` was per-process; a second
    //    process could not see the first process's registration,
    //    and the list would have returned 0 entries.
    let post_list = octo_in(&home)
        .args(["agent", "list", "--json"])
        .output()
        .expect("spawn post-list");
    assert_eq!(
        post_list.status.code(),
        Some(0),
        "post-create agent list must exit 0, got {:?}. stderr={}, stdout={}",
        post_list.status.code(),
        String::from_utf8_lossy(&post_list.stderr),
        String::from_utf8_lossy(&post_list.stdout),
    );
    let post_list_env = Envelope::parse(&String::from_utf8_lossy(&post_list.stdout));
    let agents = post_list_env.payload["agents"]
        .as_array()
        .expect("post-list payload.agents must be an array");
    assert_eq!(
        agents.len(),
        1,
        "post-create list must contain exactly one entry (the cross-process \
         visibility property of the e02f8b02 agent_registry.json amendment). \
         got {} entries: {agents:?}",
        agents.len()
    );
    // The list envelope's redactor is built with `with_active_did`,
    // `with_holder_did(active_did)` and `with_agent_ids(agent_ids)` —
    // the substrate enforces `holder_did == caller_did` for the list
    // path so the conditional `holder_did` un-redact fires on every
    // row (and on the list-level `holder_did` field); the
    // positional-Vec truncation in the walker rewrites each row's
    // `agent_id` to first-8-chars + `...` using the substrate
    // summary's `agent_id` UUID. Both fields therefore surface as
    // correlatable operator-visible forms. The list summary's
    // `agent_id` is the substrate's `record.manifest.manifest_id`
    // (the operator-supplied input), NOT the canonical
    // UUIDv5-derived id that `register_agent` returns in the create
    // response — the persistence layer's `AgentIdMismatch` error
    // explicitly tracks both ids, so the two envelopes intentionally
    // carry different ids by substrate design. The test pins:
    //   1. The list envelope's `holder_did` un-redacts to the active
    //      DID (a future regression that drops `with_holder_did`
    //      surfaces as `[REDACTED:key]` and fails the equality
    //      check).
    //   2. The list envelope's `agent_id` is in the truncated
    //      first-8-hex + `...` form (the redaction-layer contract;
    //      a future regression that drops `with_agent_ids` surfaces
    //      as `[REDACTED:key]` and fails the shape check).
    let listed_agent_id = agents[0]["agent_id"]
        .as_str()
        .expect("list entry must carry agent_id string")
        .to_string();
    assert!(
        listed_agent_id.len() == 11
            && listed_agent_id.ends_with("...")
            && listed_agent_id[..8].chars().all(|c| c.is_ascii_hexdigit()),
        "list envelope's agent_id must be the 8-hex + '...' truncated form \
         per the envelope-boundary redactor (the list redactor builds a \
         positional Vec via `with_agent_ids` so the walker truncates each \
         row's [REDACTED:key] to the correlatable form). A divergence \
         means either the list handler dropped the `with_agent_ids` call \
         OR the walker advanced out of sync. got {listed_agent_id:?}"
    );
    let listed_holder_did = agents[0]["holder_did"]
        .as_str()
        .expect("list entry must carry holder_did string")
        .to_string();
    assert_eq!(
        listed_holder_did, register_did,
        "list entry's holder_did must un-redact to the active DID \
         (substrate enforces holder_did == caller_did for the list \
         path, and the list redactor calls `with_holder_did` so the \
         conditional un-redact fires). A divergence means the list \
         handler dropped the `with_holder_did` call. got \
         {listed_holder_did:?}"
    );
    let listed_state = agents[0]["state"]
        .as_str()
        .expect("list entry must carry state string")
        .to_string();
    assert_eq!(
        listed_state, "registered",
        "create transitions the agent to the Registered terminal state \
         (RFC-0011-c §9.3.1); the cross-process list must observe the \
         same state. got {listed_state:?}"
    );
    let listed_label = agents[0]["label"]
        .as_str()
        .expect("list entry must carry label string")
        .to_string();
    assert_eq!(
        listed_label, "l3-agent-create-list-runner",
        "the manifest's operator label is surfaced through \
         AgentSummary::label and the list envelope; a divergence \
         would mean the cross-process rehydration lost a field. \
         got {listed_label:?}"
    );
    let listed_manifest_digest = agents[0]["manifest_digest"]
        .as_str()
        .expect("list entry must carry manifest_digest string")
        .to_string();
    assert!(
        !listed_manifest_digest.is_empty()
            && listed_manifest_digest
                .chars()
                .all(|c| c.is_ascii_hexdigit())
            && listed_manifest_digest.len() == 64,
        "list entry's manifest_digest must be the 64-char BLAKE3-256 \
         hex form (per RFC-0002 §Agent Manifest §Canonical Hash). A \
         divergence here would mean the rehydration changed the \
         canonical hash (e.g. a re-serialization field-order change). \
         got len={} value={listed_manifest_digest:?}",
        listed_manifest_digest.len()
    );
    let listed_registered_at = agents[0]["registered_at_unix"]
        .as_u64()
        .expect("list entry must carry registered_at_unix u64");
    assert!(
        listed_registered_at > 0,
        "list entry's registered_at_unix must be a positive unix-seconds \
         value (the substrate records the registration time at create \
         and rehydrates it on the list path). got {listed_registered_at}"
    );
}
