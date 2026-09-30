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
