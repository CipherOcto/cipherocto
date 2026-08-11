//! 4 cross-instance test vectors (RFC-0862 v1.4 §Test Vectors) +
//! optional 5th CRDT LWW TV (gated on `--features crdt`).
#![allow(clippy::doc_lazy_continuation)]
//!
//! Multi-instance test harness exercising the concrete
//! `RaftLikeWriterElection` + `RaftLikeDidWriteCoordinator` impls
//! (mission `0871e-f7-coordinator-impl` task #122). 3 instances share
//! one `Arc<Cluster>` — in production the cluster is across instances;
//! here it is in-process.
//!
//! ## Test vectors
//!
//! - TV-1 atomic_register — 3 instances concurrent acquire + register;
//!   exactly the leader commits.
//! - TV-2 leader_failover — leader A's lease expires; B becomes leader.
//! - TV-3 wal_replay — 3 entries appended; replay via
//!   `replay_wal` returns the same order with valid checksums.
//! - TV-4 fail_closed — no elected writer → register fails-closed.
//! - TV-5 crdt_lww (optional, `crdt` feature) — local fallback succeeds
//!   without leader election when `crdt` is enabled.
//!
//! ## Why `replay_wal` instead of cluster read
//!
//! TV-3 exercises the canonical `replay_wal` function (per RFC-0862 v1.3
//! §WAL Replay Algorithm) over the `InMemoryWal::read_range` reader. This
//! validates the checksum + LSN chain + shard_key checks end-to-end.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use std::time::Duration;

use octo_ident::{
    canonical_hash, ChainId, DidDocument, DidWriteCoordinator, DidWriteCoordinatorError,
};
use octo_sync::substrate::{
    replay_wal, Cluster, HlcTimestamp, RaftLikeDidWriteCoordinator, RaftLikeWriterElection,
    ReplayState, ShardKey, WalEntry, WalWriter, WriterContext, WriterElection, WriterNodeId,
    ENTRY_TYPE_NONCE_RECORD, WAL_MAGIC_V13,
};

/// 3-instance fixture: instances A, B, C + 1 shared cluster.
fn fixture() -> (Arc<Cluster>, [WriterNodeId; 3]) {
    let cluster = Cluster::new();
    let ids = [
        WriterNodeId([1u8; 32]),
        WriterNodeId([2u8; 32]),
        WriterNodeId([3u8; 32]),
    ];
    (cluster, ids)
}

fn sample_doc(seed: u8) -> DidDocument {
    DidDocument {
        public_key: [seed; 32],
        revoked: false,
        ..Default::default()
    }
}

/// TV-1 atomic_register — 3 instances concurrent register of the
/// same DID; exactly the elected leader commits.
#[tokio::test]
async fn tv1_atomic_register() {
    let (cluster, ids) = fixture();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    let d = sample_doc(11);
    let did_hash = canonical_hash(&d);
    let shard_key = ShardKey::derive_canonical(&did_hash);

    let elections: Vec<Arc<RaftLikeWriterElection>> = ids
        .iter()
        .map(|id| {
            Arc::new(RaftLikeWriterElection::new(
                *id,
                cluster.clone(),
                chain_id.clone(),
            ))
        })
        .collect();
    let coordinators: Vec<Arc<RaftLikeDidWriteCoordinator>> = ids
        .iter()
        .zip(elections.iter())
        .map(|(id, e)| {
            Arc::new(RaftLikeDidWriteCoordinator::new(
                cluster.clone(),
                chain_id.clone(),
                *id,
                e.clone() as Arc<dyn WriterElection>,
            ))
        })
        .collect();

    // All 3 instances try to acquire the lease. Exactly one wins.
    let mut acquire_results = Vec::new();
    for e in &elections {
        acquire_results.push(e.acquire_writer(&shard_key, 1_000).await);
    }
    let winners: Vec<_> = acquire_results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();
    assert_eq!(winners.len(), 1, "exactly one instance must win the lease");
    let leader_id = winners[0].writer_node_id;
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        leader_id
    );

    // All 3 instances attempt register. Only the leader commits.
    let mut register_results = Vec::new();
    for c in &coordinators {
        register_results.push(c.submit_register(&did_hash, &chain_id, &d).await);
    }
    let oks = register_results.iter().filter(|r| r.is_ok()).count();
    let fail_closed = register_results
        .iter()
        .filter(|r| matches!(r, Err(DidWriteCoordinatorError::WriterUnavailable)))
        .count();
    assert_eq!(oks, 1, "exactly one coordinator commits");
    assert_eq!(
        fail_closed, 2,
        "non-leaders fail-closed with WriterUnavailable"
    );

    // Verify the WAL has exactly 1 register entry.
    let entries = cluster.read_wal_range(1, None);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].lsn, 1);
}

/// TV-2 leader_failover — A acquires; lease expires; B wins.
#[tokio::test]
async fn tv2_leader_failover() {
    let (cluster, ids) = fixture();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    cluster.set_lease_duration_ms(1);
    let shard_key = ShardKey([7u8; 32]);
    let writer_a = Arc::new(RaftLikeWriterElection::new(
        ids[0],
        cluster.clone(),
        chain_id.clone(),
    ));
    let writer_b = Arc::new(RaftLikeWriterElection::new(
        ids[1],
        cluster.clone(),
        chain_id.clone(),
    ));

    let id_a = writer_a.acquire_writer(&shard_key, 1_000).await.unwrap();
    assert_eq!(id_a.writer_node_id, ids[0]);
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        ids[0]
    );

    // Lease expires (1ms < 50ms sleep).
    tokio::time::sleep(Duration::from_millis(50)).await;

    let id_b = writer_b.acquire_writer(&shard_key, 1_000).await.unwrap();
    assert_eq!(id_b.writer_node_id, ids[1]);
    assert!(id_b.term > id_a.term);
    assert_eq!(
        cluster.current_leader(shard_key).unwrap().writer_node_id,
        ids[1]
    );
}

/// TV-3 wal_replay — append 3 entries, replay returns them in order.
#[tokio::test]
async fn tv3_wal_replay() {
    let (cluster, ids) = fixture();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    let shard_key = ShardKey([7u8; 32]);
    let writer = RaftLikeWriterElection::new(ids[0], cluster.clone(), chain_id.clone());
    let _ = writer.acquire_writer(&shard_key, 1_000).await.unwrap();
    let wal = octo_sync::substrate::InMemoryWal::new(cluster.clone());

    for i in 1u8..=3u8 {
        let entry = WalEntry::build_v13(ENTRY_TYPE_NONCE_RECORD, shard_key, vec![i]);
        let lsn = wal.append_entry(&entry).await.unwrap();
        assert_eq!(lsn, i as u64);
    }

    let mut context = WriterContext {
        relinquish_pending: AtomicBool::new(false),
        flush_attempts: AtomicU32::new(0),
        max_attempts: 100,
        replay_state: ReplayState::Idle,
    };
    let reader = octo_sync::substrate::InMemoryWal::new(cluster.clone());
    let tip = replay_wal(&mut context, 1, &shard_key, &reader)
        .await
        .unwrap();
    assert_eq!(tip, 3);
    assert!(matches!(
        context.replay_state,
        ReplayState::Complete {
            tip_lsn: 3,
            total_entries: 3
        }
    ));

    // Verify LSN chain.
    let entries = cluster.read_wal_range(1, None);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].previous_lsn, 0);
    assert_eq!(entries[1].previous_lsn, 1);
    assert_eq!(entries[2].previous_lsn, 2);
    // Magic preserved.
    for e in &entries {
        assert_eq!(e.magic, WAL_MAGIC_V13);
    }
    // HLC-ish: each entry has a fresh physical_ms (from HlcClock::now).
    // Just verify the prefix bytes for the LSN field are encoded.
    let e1 = &entries[0];
    let lsn_bytes: [u8; 8] = e1.prefix_bytes[40..48].try_into().unwrap();
    assert_eq!(u64::from_be_bytes(lsn_bytes), 1);
    // HlcTimestamp unused but kept for future fault-injection tests.
    let _ = HlcTimestamp {
        physical_ms: 0,
        logical: 0,
        writer_node_id: ids[0],
    };
}

/// TV-4 fail_closed — no elected writer → register fails-closed.
#[tokio::test]
async fn tv4_fail_closed() {
    let (cluster, _ids) = fixture();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    let node_id = WriterNodeId([99u8; 32]);
    let election = Arc::new(RaftLikeWriterElection::new(
        node_id,
        cluster.clone(),
        chain_id.clone(),
    ));
    let coordinator = RaftLikeDidWriteCoordinator::new(
        cluster.clone(),
        chain_id.clone(),
        node_id,
        election.clone(),
    );
    let d = sample_doc(13);
    let did_hash = canonical_hash(&d);
    let r = coordinator.submit_register(&did_hash, &chain_id, &d).await;
    assert!(matches!(
        r,
        Err(DidWriteCoordinatorError::WriterUnavailable)
    ));
}

/// TV-5 crdt_lww — gated on `crdt` feature.
#[cfg(feature = "crdt")]
#[tokio::test]
#[allow(deprecated)]
async fn tv5_crdt_lww_succeeds_without_leader() {
    let (cluster, _ids) = fixture();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    let node_id = WriterNodeId([55u8; 32]);
    let election = Arc::new(RaftLikeWriterElection::new(
        node_id,
        cluster.clone(),
        chain_id.clone(),
    ));
    let coordinator = RaftLikeDidWriteCoordinator::new(
        cluster.clone(),
        chain_id.clone(),
        node_id,
        election.clone(),
    );
    // No acquire — LWW fallback should still succeed with `crdt` enabled.
    let d = sample_doc(17);
    let did_hash = canonical_hash(&d);
    let r = coordinator
        .submit_register_local_fallback(&did_hash, &chain_id, &d)
        .await;
    assert!(r.is_ok(), "crdt local fallback must succeed: {r:?}");
    let entries = cluster.read_wal_range(1, None);
    assert_eq!(entries.len(), 1);
}
