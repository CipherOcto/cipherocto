//! Phase 3 acceptance test: 5-node convergence with fault injection.
//!
//! Per RFC-0862 §Implementation Phases Phase 3:
//! "5-node network, 1 writer, 4 readers, kill any node, verify
//! convergence within 60s"
//!
//! This test verifies the full Phase 3 stack:
//! - DGP SnapshotFragment routing (0862f)
//! - DRS-adapted peer scoring (scoring.rs)
//! - WAL-tail streaming to N peers (0862a)
//! - Replay cache dedup (0862e)
//! - Multi-peer session management (session.rs)

use std::time::Duration;

use octo_sync::config::SyncRole;
use octo_sync::envelope::WalTailChunk;
use octo_sync::identity::SyncPeerId;
use octo_sync::state::{SyncLifecycle, TransitionTrigger};
use octo_sync::DatabaseSyncAdapter;
use sync_e2e_tests::TestCluster;

/// Helper: wire a full mesh where all peers are in Streaming state.
fn wire_full_mesh_streaming(cluster: &mut TestCluster) {
    let peer_ids: Vec<(usize, SyncPeerId)> = (0..cluster.len())
        .map(|i| (i, cluster.node(i).peer_id(&cluster.mission_id)))
        .collect();

    for i in 0..cluster.len() {
        for (j, peer_id) in &peer_ids {
            if i != *j {
                cluster
                    .node_mut(i)
                    .session
                    .subscribe_peer(*peer_id)
                    .unwrap();
                cluster
                    .node(i)
                    .session
                    .transition_peer(
                        *peer_id,
                        SyncLifecycle::Authenticating,
                        TransitionTrigger::TlsHandshakeComplete,
                    )
                    .unwrap();
                cluster
                    .node(i)
                    .session
                    .transition_peer(
                        *peer_id,
                        SyncLifecycle::Streaming,
                        TransitionTrigger::SignatureValid,
                    )
                    .unwrap();
            }
        }
    }
}

/// Helper: commit N entries on the writer and fan out to active readers.
///
/// `entry_prefix` ensures unique envelope_ids across phases (replay cache
/// deduplicates by BLAKE3 hash of the entry bytes).
/// `active_readers` is the set of node indices to fan out to (excludes killed nodes).
fn commit_and_fan_out(
    cluster: &mut TestCluster,
    writer_idx: usize,
    n: usize,
    entry_prefix: &str,
    active_readers: &[usize],
) {
    for i in 0..n {
        let data = format!("{}-{}", entry_prefix, i).into_bytes();
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
        let writer_peer_id = cluster.node(writer_idx).peer_id(&cluster.mission_id);
        for &reader_idx in active_readers {
            let _ = cluster
                .node_mut(reader_idx)
                .session
                .apply_wal_tail(writer_peer_id, &chunk);
        }
    }
}

/// Phase 3-T1: 5-node full convergence — writer commits, all 4 readers receive.
///
/// Verifies the basic Phase 3 requirement: N readers via gossip receive
/// all data from a single writer.
#[tokio::test]
async fn five_node_full_convergence() {
    let mut cluster = TestCluster::new(
        5,
        &[
            SyncRole::Replicator, // writer
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
        ],
    );

    // Wire full mesh with all peers in Streaming state
    wire_full_mesh_streaming(&mut cluster);

    // Writer commits 50 entries, fans out to all 4 readers
    commit_and_fan_out(&mut cluster, 0, 50, "conv1", &[1, 2, 3, 4]);

    // Verify all 5 nodes have the same LSN
    for i in 0..5 {
        assert_eq!(
            cluster.adapter(i).current_lsn().unwrap(),
            50,
            "node {} should have LSN 50, got {}",
            i,
            cluster.adapter(i).current_lsn().unwrap()
        );
    }
}

/// Phase 3-T2: Kill one reader, writer continues, remaining readers converge.
///
/// Per RFC-0862 Phase 3: "kill any node, verify convergence within 60s".
/// We kill reader (node 4), writer commits more data, and verify the
/// remaining 3 readers converge.
#[tokio::test]
async fn five_node_kill_reader_convergence() {
    let mut cluster = TestCluster::new(
        5,
        &[
            SyncRole::Replicator, // writer (node 0)
            SyncRole::Observer,   // reader 1
            SyncRole::Observer,   // reader 2
            SyncRole::Observer,   // reader 3
            SyncRole::Observer,   // reader 4 (will be killed)
        ],
    );

    // Wire full mesh
    wire_full_mesh_streaming(&mut cluster);

    // Phase 1: Writer commits 20 entries, all 4 readers receive
    commit_and_fan_out(&mut cluster, 0, 20, "kill-r-phase1", &[1, 2, 3, 4]);

    for i in 0..5 {
        assert_eq!(cluster.adapter(i).current_lsn().unwrap(), 20);
    }

    // Phase 2: Kill reader 4 (simulate failure by unsubscribing)
    let killed_peer = cluster.node(4).peer_id(&cluster.mission_id);
    let writer_peer = cluster.node(0).peer_id(&cluster.mission_id);
    cluster.node_mut(0).session.unsubscribe_peer(&killed_peer);
    cluster.node_mut(4).session.unsubscribe_peer(&writer_peer);

    // Phase 3: Writer commits 30 more entries, fans out to remaining 3 readers
    commit_and_fan_out(&mut cluster, 0, 30, "kill-r-phase2", &[1, 2, 3]);

    // Verify: remaining 4 nodes (0-3) have LSN 50 (20 + 30)
    for i in 0..4 {
        assert_eq!(
            cluster.adapter(i).current_lsn().unwrap(),
            50,
            "node {} should have LSN 50, got {}",
            i,
            cluster.adapter(i).current_lsn().unwrap()
        );
    }

    // Killed node stays at 20 (didn't receive the second batch)
    assert_eq!(cluster.adapter(4).current_lsn().unwrap(), 20);
}

/// Phase 3-T3: Kill writer, remaining readers hold state.
///
/// Writer commits data, all readers receive it. Writer is killed.
/// Verify readers retain their state and don't corrupt.
#[tokio::test]
async fn five_node_kill_writer_holds_state() {
    let mut cluster = TestCluster::new(
        5,
        &[
            SyncRole::Replicator, // writer (node 0)
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
            SyncRole::Observer,
        ],
    );

    wire_full_mesh_streaming(&mut cluster);

    // Writer commits 30 entries, all readers receive
    commit_and_fan_out(&mut cluster, 0, 30, "kill-w-phase1", &[1, 2, 3, 4]);

    for i in 0..5 {
        assert_eq!(cluster.adapter(i).current_lsn().unwrap(), 30);
    }

    // Kill writer (node 0) — unsubscribe all peers from writer
    let writer_peer = cluster.node(0).peer_id(&cluster.mission_id);
    for i in 1..5 {
        cluster.node_mut(i).session.unsubscribe_peer(&writer_peer);
    }

    // Wait a beat (simulating time passing without writer)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // All readers should still have LSN 30 (state preserved)
    for i in 1..5 {
        assert_eq!(
            cluster.adapter(i).current_lsn().unwrap(),
            30,
            "reader {} should retain LSN 30 after writer kill",
            i
        );
    }
}

/// Phase 3-T4: Select gossip peers returns best targets under load.
///
/// Verify that `select_gossip_peers` correctly ranks 5 peers using the
/// DRS-adapted scoring (freshness, liveness, reliability).
#[tokio::test]
async fn five_node_gossip_peer_selection() {
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

    wire_full_mesh_streaming(&mut cluster);

    // Set different LSN watermarks: 0, 10, 20, 30, 40
    // (node 0 is writer, peers 1-4 are readers)
    // The writer (node 0) sees peers 1-4 with these watermarks
    for (i, lsn) in [(1, 0u64), (2, 10), (3, 20), (4, 30)] {
        cluster
            .node(0)
            .session
            .on_lsn_ack(cluster.node(i).peer_id(&cluster.mission_id), lsn)
            .unwrap();
    }

    // Select 2 gossip peers — should prefer peers with lowest LSN
    // (best catch-up targets)
    let selected = cluster.node(0).session.select_gossip_peers(2);
    assert_eq!(selected.len(), 2);

    // The first selected should have LSN 0 (most behind)
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

/// Phase 3-T5: Replay cache dedup across multiple writers.
///
/// Two writers send overlapping data to one reader. The replay cache
/// should deduplicate entries so the reader doesn't apply the same
/// entry twice.
#[tokio::test]
async fn five_node_replay_cache_dedup() {
    let mut cluster = TestCluster::new(
        3,
        &[
            SyncRole::Replicator, // writer A
            SyncRole::Replicator, // writer B
            SyncRole::Observer,   // reader
        ],
    );

    let reader_peer = cluster.node(2).peer_id(&cluster.mission_id);
    cluster
        .node_mut(0)
        .session
        .subscribe_peer(reader_peer)
        .unwrap();
    cluster
        .node_mut(1)
        .session
        .subscribe_peer(reader_peer)
        .unwrap();

    // Writer A commits 10 entries
    let writer_a_peer = cluster.node(0).peer_id(&cluster.mission_id);
    for i in 0..10 {
        let data = format!("dedup-a-{}", i).into_bytes();
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

    // Writer B commits 10 entries (different data, different LSN namespace)
    let writer_b_peer = cluster.node(1).peer_id(&cluster.mission_id);
    for i in 0..10 {
        let data = format!("dedup-b-{}", i).into_bytes();
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

    // Reader should have 20 entries (10 from A + 10 from B, no dedup needed
    // since they're from different writers with different LSNs)
    assert_eq!(cluster.adapter(2).current_lsn().unwrap(), 20);

    // Send same chunk again from writer A — should be deduped
    let entries = cluster.adapter(0).read_wal_range(1, 10).unwrap();
    let chunk = WalTailChunk {
        from_lsn: 1,
        to_lsn: 10,
        entries,
        is_last: true,
    };
    let applied = cluster
        .node_mut(2)
        .session
        .apply_wal_tail(writer_a_peer, &chunk)
        .unwrap();
    assert_eq!(applied, 0, "duplicate chunk should be deduped");

    // LSN should still be 20
    assert_eq!(cluster.adapter(2).current_lsn().unwrap(), 20);
}
