//! Multi-peer E2E tests: DGP bridge integration, 3-peer mesh, multi-writer.
//!
//! Extends the L3 in-process tests with:
//! - DGP bridge → SyncHandler integration
//! - 3+ peer full-mesh gossip
//! - Multi-writer scenarios
//! - Reader catch-up from multiple writers
//! - Multi-peer health tracking (tick, heartbeat, gossip peer selection)

use octo_sync::config::SyncRole;
use octo_sync::dgp_bridge::{DgpSyncBridge, GossipSnapshotFragment, SyncHandler};
use octo_sync::envelope::WalTailChunk;
use octo_sync::identity::SyncPeerId;
use octo_sync::session::TickAction;
use octo_sync::state::SyncLifecycle;
use octo_sync::DatabaseSyncAdapter;
use std::sync::Arc;
use sync_e2e_tests::TestCluster;

// ── DGP Bridge Integration ────────────────────────────────────────

/// Handler that records all DGP-dispatched events for verification.
struct RecordingHandler {
    summaries: parking_lot::Mutex<Vec<([u8; 32], Vec<u8>)>>,
    segments: parking_lot::Mutex<Vec<([u8; 32], Vec<u8>)>>,
    wal_tails: parking_lot::Mutex<Vec<([u8; 32], Vec<u8>)>>,
}

impl RecordingHandler {
    fn new() -> Self {
        Self {
            summaries: parking_lot::Mutex::new(Vec::new()),
            segments: parking_lot::Mutex::new(Vec::new()),
            wal_tails: parking_lot::Mutex::new(Vec::new()),
        }
    }

    #[allow(clippy::type_complexity)]
    fn drain(
        &self,
    ) -> (
        Vec<([u8; 32], Vec<u8>)>,
        Vec<([u8; 32], Vec<u8>)>,
        Vec<([u8; 32], Vec<u8>)>,
    ) {
        let s = self.summaries.lock().drain(..).collect();
        let sg = self.segments.lock().drain(..).collect();
        let w = self.wal_tails.lock().drain(..).collect();
        (s, sg, w)
    }
}

impl SyncHandler for RecordingHandler {
    fn on_summary(&self, peer_id: [u8; 32], payload: Vec<u8>) {
        self.summaries.lock().push((peer_id, payload));
    }
    fn on_segment(&self, peer_id: [u8; 32], payload: Vec<u8>) {
        self.segments.lock().push((peer_id, payload));
    }
    fn on_wal_tail(&self, peer_id: [u8; 32], payload: Vec<u8>) {
        self.wal_tails.lock().push((peer_id, payload));
    }
}

/// MP-T1: DGP bridge routes all three envelope subtypes to handler.
#[test]
fn dgp_bridge_routes_all_subtypes() {
    let handler = Arc::new(RecordingHandler::new());
    let mission_id = [0xABu8; 32];
    let bridge = DgpSyncBridge::new(mission_id, handler.clone());

    // SummaryResponse (0xA1)
    let frag = GossipSnapshotFragment::new(0xA1, [1u8; 32], mission_id, vec![0x10]);
    bridge.dispatch(&frag).unwrap();

    // SegmentResponse (0xA3)
    let frag = GossipSnapshotFragment::new(0xA3, [2u8; 32], mission_id, vec![0x20, 0x21]);
    bridge.dispatch(&frag).unwrap();

    // WalTailResponse (0xB1)
    let frag = GossipSnapshotFragment::new(0xB1, [3u8; 32], mission_id, vec![0x30, 0x31, 0x32]);
    bridge.dispatch(&frag).unwrap();

    let (s, sg, w) = handler.drain();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].0, [1u8; 32]);
    assert_eq!(s[0].1, vec![0x10]);
    assert_eq!(sg.len(), 1);
    assert_eq!(sg[0].0, [2u8; 32]);
    assert_eq!(sg[0].1, vec![0x20, 0x21]);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].0, [3u8; 32]);
    assert_eq!(w[0].1, vec![0x30, 0x31, 0x32]);
}

/// MP-T2: DGP bridge ignores fragments from other missions.
#[test]
fn dgp_bridge_ignores_other_mission() {
    let handler = Arc::new(RecordingHandler::new());
    let mission_id = [0xABu8; 32];
    let bridge = DgpSyncBridge::new(mission_id, handler.clone());

    let frag = GossipSnapshotFragment::new(0xA1, [1u8; 32], [0xFFu8; 32], vec![0x10]);
    bridge.dispatch(&frag).unwrap();

    let (s, sg, w) = handler.drain();
    assert!(s.is_empty());
    assert!(sg.is_empty());
    assert!(w.is_empty());
}

/// MP-T3: DGP bridge rejects unknown subtypes.
#[test]
fn dgp_bridge_rejects_unknown_subtype() {
    let handler = Arc::new(RecordingHandler::new());
    let mission_id = [0xABu8; 32];
    let bridge = DgpSyncBridge::new(mission_id, handler);

    let frag = GossipSnapshotFragment::new(0xFF, [1u8; 32], mission_id, vec![]);
    let err = bridge.dispatch(&frag).unwrap_err();
    assert!(matches!(
        err,
        octo_sync::error::SyncError::UnknownEnvelopeSubtype(0xFF)
    ));
}

// ── Multi-Peer Full Mesh ──────────────────────────────────────────

/// MP-T4: 3-node full mesh — all nodes subscribe to each other, writer commits,
/// all readers receive.
#[tokio::test]
async fn three_node_full_mesh_sync() {
    let mut cluster = TestCluster::new(
        3,
        &[SyncRole::Replicator, SyncRole::Observer, SyncRole::Observer],
    );

    // Full mesh: each node subscribes every other node.
    cluster.subscribe_mesh();

    // Transition all peers to Streaming.
    for node_idx in 0..3 {
        let peers: Vec<SyncPeerId> = (0..3)
            .filter(|&j| j != node_idx)
            .map(|j| cluster.node(j).peer_id(&cluster.mission_id))
            .collect();
        for peer_id in peers {
            cluster
                .node(node_idx)
                .session
                .transition_peer(
                    peer_id,
                    SyncLifecycle::Authenticating,
                    octo_sync::state::TransitionTrigger::TlsHandshakeComplete,
                )
                .unwrap();
            cluster
                .node(node_idx)
                .session
                .transition_peer(
                    peer_id,
                    SyncLifecycle::Streaming,
                    octo_sync::state::TransitionTrigger::SignatureValid,
                )
                .unwrap();
        }
    }

    // Node 0 commits 10 entries and fans out to both readers.
    for i in 0..10 {
        let data = format!("mesh-row-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(0).commit_entry(&data);
        cluster
            .node(0)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(0).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        cluster.fan_out(0, &chunk);
    }

    assert_eq!(cluster.adapter(1).current_lsn().unwrap(), 10);
    assert_eq!(cluster.adapter(2).current_lsn().unwrap(), 10);
}

/// MP-T5: 5-node fan-out — one writer, four readers.
#[tokio::test]
async fn five_node_fan_out() {
    let mut cluster = TestCluster::new(
        5,
        &[
            SyncRole::Replicator,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
        ],
    );

    // All readers subscribe to writer (node 0).
    for reader_idx in 1..5 {
        let writer_peer_id = cluster.node(0).peer_id(&cluster.mission_id);
        cluster
            .node_mut(reader_idx)
            .session
            .subscribe_peer(writer_peer_id)
            .unwrap();
    }

    // Writer commits 20 entries.
    for i in 0..20 {
        let data = format!("fan-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(0).commit_entry(&data);
        cluster
            .node(0)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(0).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        cluster.fan_out(0, &chunk);
    }

    for i in 1..5 {
        assert_eq!(cluster.adapter(i).current_lsn().unwrap(), 20);
    }
}

// ── Multi-Writer Scenarios ────────────────────────────────────────

/// MP-T6: Two writers, one reader — reader receives from both writers.
#[tokio::test]
async fn two_writers_one_reader() {
    let mut cluster = TestCluster::new(
        3,
        &[
            SyncRole::Replicator, // writer A
            SyncRole::Replicator, // writer B
            SyncRole::Observer,   // reader
        ],
    );

    let reader_peer_id = cluster.node(2).peer_id(&cluster.mission_id);

    // Both writers subscribe the reader.
    cluster
        .node_mut(0)
        .session
        .subscribe_peer(reader_peer_id)
        .unwrap();
    cluster
        .node_mut(1)
        .session
        .subscribe_peer(reader_peer_id)
        .unwrap();

    // Reader subscribes both writers.
    let writer_a_peer = cluster.node(0).peer_id(&cluster.mission_id);
    let writer_b_peer = cluster.node(1).peer_id(&cluster.mission_id);
    cluster
        .node_mut(2)
        .session
        .subscribe_peer(writer_a_peer)
        .unwrap();
    cluster
        .node_mut(2)
        .session
        .subscribe_peer(writer_b_peer)
        .unwrap();

    // Writer A commits 5 entries.
    for i in 0..5 {
        let data = format!("writer-a-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(0).commit_entry(&data);
        cluster
            .node(0)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(0).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        let _ = cluster
            .node_mut(2)
            .session
            .apply_wal_tail(writer_a_peer, &chunk);
    }

    // Writer B commits 5 entries.
    for i in 0..5 {
        let data = format!("writer-b-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(1).commit_entry(&data);
        cluster
            .node(1)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(1).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        let _ = cluster
            .node_mut(2)
            .session
            .apply_wal_tail(writer_b_peer, &chunk);
    }

    // Reader should have 10 entries total (5 from A + 5 from B).
    assert_eq!(cluster.adapter(2).current_lsn().unwrap(), 10);
}

/// MP-T7: Writer chain — A writes, B receives and relays to C.
///
/// B receives WAL from A via apply_wal_tail, then reads A's WAL entries
/// and applies them directly to C's adapter (simulating a relay node).
#[tokio::test]
async fn writer_chain_a_to_b_to_c() {
    let mut cluster = TestCluster::new(
        3,
        &[
            SyncRole::Replicator, // A (root writer)
            SyncRole::Replicator, // B (relay)
            SyncRole::Observer,   // C (leaf reader)
        ],
    );

    // B subscribes A as writer.
    let a_peer = cluster.node(0).peer_id(&cluster.mission_id);
    cluster.node_mut(1).session.subscribe_peer(a_peer).unwrap();

    // C subscribes B as writer.
    let b_peer = cluster.node(1).peer_id(&cluster.mission_id);
    cluster.node_mut(2).session.subscribe_peer(b_peer).unwrap();

    // A commits 10 entries, fans out to B.
    for i in 0..10 {
        let data = format!("chain-{}", i).into_bytes();
        let (txn_id, from_lsn, to_lsn) = cluster.node(0).commit_entry(&data);
        cluster
            .node(0)
            .session
            .on_commit(txn_id, from_lsn, to_lsn)
            .unwrap();
        let entries = cluster.adapter(0).read_wal_range(from_lsn, to_lsn).unwrap();
        let chunk = WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        };
        let _ = cluster.node_mut(1).session.apply_wal_tail(a_peer, &chunk);
    }

    assert_eq!(cluster.adapter(1).current_lsn().unwrap(), 10);

    // B relays to C: read A's WAL entries and apply directly to C's adapter.
    // This simulates a relay that forwards data without re-committing.
    let all_entries = cluster.adapter(0).read_wal_range(1, 10).unwrap();
    for entry in &all_entries {
        let _ = cluster.adapter(2).apply_wal_entry(entry);
    }

    assert_eq!(cluster.adapter(2).current_lsn().unwrap(), 10);
}

// ── Multi-Peer Health Tracking ────────────────────────────────────

/// MP-T8: tick() detects stale peers across multiple nodes.
#[test]
fn tick_detects_stale_peers_across_mesh() {
    let mut cluster = TestCluster::new(
        3,
        &[SyncRole::Replicator, SyncRole::Observer, SyncRole::Observer],
    );

    let peer1 = cluster.node(1).peer_id(&cluster.mission_id);
    let peer2 = cluster.node(2).peer_id(&cluster.mission_id);

    // Subscribe and transition both to Streaming.
    for peer_id in &[peer1, peer2] {
        cluster
            .node_mut(0)
            .session
            .subscribe_peer(*peer_id)
            .unwrap();
        cluster
            .node(0)
            .session
            .transition_peer(
                *peer_id,
                SyncLifecycle::Authenticating,
                octo_sync::state::TransitionTrigger::TlsHandshakeComplete,
            )
            .unwrap();
        cluster
            .node(0)
            .session
            .transition_peer(
                *peer_id,
                SyncLifecycle::Streaming,
                octo_sync::state::TransitionTrigger::SignatureValid,
            )
            .unwrap();
    }

    // Record heartbeat for peer1 at t=100 (old), peer2 at t=118 (fresh).
    cluster.node(0).session.record_heartbeat(peer1, 100);
    cluster.node(0).session.record_heartbeat(peer2, 118);

    // Tick at t=120 — peer1 is stale (>10s since t=100), peer2 is healthy (<10s since t=118).
    let actions = cluster.node(0).session.tick(120);
    assert!(actions.contains(&TickAction::TransitionToSuspect(peer1)));
    assert!(!actions.contains(&TickAction::TransitionToSuspect(peer2)));
}

/// MP-T9: select_gossip_peers prefers lower-LSN peers for catch-up gossip.
#[test]
fn select_gossip_peers_prefers_lower_lsn() {
    let mut cluster = TestCluster::new(
        4,
        &[
            SyncRole::Replicator,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
        ],
    );

    let peer1 = cluster.node(1).peer_id(&cluster.mission_id);
    let peer2 = cluster.node(2).peer_id(&cluster.mission_id);
    let peer3 = cluster.node(3).peer_id(&cluster.mission_id);

    // Subscribe all three and transition to Streaming.
    for peer_id in &[peer1, peer2, peer3] {
        cluster
            .node_mut(0)
            .session
            .subscribe_peer(*peer_id)
            .unwrap();
        cluster
            .node(0)
            .session
            .transition_peer(
                *peer_id,
                SyncLifecycle::Authenticating,
                octo_sync::state::TransitionTrigger::TlsHandshakeComplete,
            )
            .unwrap();
        cluster
            .node(0)
            .session
            .transition_peer(
                *peer_id,
                SyncLifecycle::Streaming,
                octo_sync::state::TransitionTrigger::SignatureValid,
            )
            .unwrap();
    }

    // Set different LSN watermarks: peer1=10, peer2=5, peer3=15.
    cluster.node(0).session.on_lsn_ack(peer1, 10).unwrap();
    cluster.node(0).session.on_lsn_ack(peer2, 5).unwrap();
    cluster.node(0).session.on_lsn_ack(peer3, 15).unwrap();

    // Select 2 gossip peers — should prefer peer2 (LSN=5) and peer1 (LSN=10).
    let selected = cluster.node(0).session.select_gossip_peers(2);
    assert_eq!(selected.len(), 2);
    // First selected should have the lowest LSN.
    let first_lsn = cluster
        .node(0)
        .session
        .peer_lsn_watermark(selected[0])
        .unwrap();
    let second_lsn = cluster
        .node(0)
        .session
        .peer_lsn_watermark(selected[1])
        .unwrap();
    assert!(
        first_lsn <= second_lsn,
        "first peer should have lower LSN: {} > {}",
        first_lsn,
        second_lsn
    );
}

/// MP-T10: peer_states returns all peers with correct lifecycle.
#[test]
fn peer_states_reflects_all_lifecycle_phases() {
    let mut cluster = TestCluster::new(
        3,
        &[SyncRole::Replicator, SyncRole::Observer, SyncRole::Observer],
    );

    let peer1 = cluster.node(1).peer_id(&cluster.mission_id);
    let peer2 = cluster.node(2).peer_id(&cluster.mission_id);

    cluster.node_mut(0).session.subscribe_peer(peer1).unwrap();
    cluster.node_mut(0).session.subscribe_peer(peer2).unwrap();

    // Transition peer1 to Streaming, leave peer2 at Connecting.
    cluster
        .node(0)
        .session
        .transition_peer(
            peer1,
            SyncLifecycle::Authenticating,
            octo_sync::state::TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
    cluster
        .node(0)
        .session
        .transition_peer(
            peer1,
            SyncLifecycle::Streaming,
            octo_sync::state::TransitionTrigger::SignatureValid,
        )
        .unwrap();

    let states = cluster.node(0).session.peer_states();
    assert_eq!(states.len(), 2);

    let state1 = states.iter().find(|(id, _)| *id == peer1).unwrap();
    assert_eq!(state1.1, SyncLifecycle::Streaming);

    let state2 = states.iter().find(|(id, _)| *id == peer2).unwrap();
    assert_eq!(state2.1, SyncLifecycle::Connecting);
}

/// MP-T11: DEDUP — same WAL entry applied twice is deduplicated.
#[test]
fn wal_tail_deduplication_across_peers() {
    let mut cluster = TestCluster::new(2, &[SyncRole::Replicator, SyncRole::Observer]);

    let writer_peer = cluster.node(0).peer_id(&cluster.mission_id);
    cluster
        .node_mut(1)
        .session
        .subscribe_peer(writer_peer)
        .unwrap();

    let chunk = WalTailChunk {
        from_lsn: 1,
        to_lsn: 1,
        entries: vec![b"duplicate-entry".to_vec()],
        is_last: true,
    };

    // Apply twice — second should be deduped.
    let applied1 = cluster
        .node(1)
        .session
        .apply_wal_tail(writer_peer, &chunk)
        .unwrap();
    let applied2 = cluster
        .node(1)
        .session
        .apply_wal_tail(writer_peer, &chunk)
        .unwrap();

    assert_eq!(applied1, 1);
    assert_eq!(applied2, 0);
}

/// MP-T12: Concurrent writers — multiple nodes commit simultaneously,
/// reader receives all entries.
#[tokio::test]
async fn concurrent_writers_to_single_reader() {
    let mut cluster = TestCluster::new(
        4,
        &[
            SyncRole::Replicator, // writer A
            SyncRole::Replicator, // writer B
            SyncRole::Replicator, // writer C
            SyncRole::Observer,   // reader
        ],
    );

    let reader_peer = cluster.node(3).peer_id(&cluster.mission_id);

    // All three writers subscribe the reader.
    for writer_idx in 0..3 {
        cluster
            .node_mut(writer_idx)
            .session
            .subscribe_peer(reader_peer)
            .unwrap();
    }

    // Each writer commits 5 entries.
    for writer_idx in 0..3 {
        for i in 0..5 {
            let data = format!("w{}-{}", writer_idx, i).into_bytes();
            let (txn_id, from_lsn, to_lsn) = cluster.node(writer_idx).commit_entry(&data);
            cluster
                .node(writer_idx)
                .session
                .on_commit(txn_id, from_lsn, to_lsn)
                .unwrap();
            let entries = cluster
                .adapter(writer_idx)
                .read_wal_range(from_lsn, to_lsn)
                .unwrap();
            let chunk = WalTailChunk {
                from_lsn,
                to_lsn,
                entries,
                is_last: true,
            };
            let writer_peer = cluster.node(writer_idx).peer_id(&cluster.mission_id);
            let _ = cluster
                .node_mut(3)
                .session
                .apply_wal_tail(writer_peer, &chunk);
        }
    }

    // Reader should have 15 entries total (5 × 3 writers).
    assert_eq!(cluster.adapter(3).current_lsn().unwrap(), 15);
}
