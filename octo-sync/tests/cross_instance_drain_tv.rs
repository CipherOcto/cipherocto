//! 4 cross-instance drain test vectors (mission 0871e-phase5c-1 +
//! RFC-0862 v1.4 §Test Vectors).
#![allow(clippy::doc_lazy_continuation)]
//!
//! Multi-instance test harness exercising the concrete
//! `RaftLikeDrainCoordinator` impl (per RFC-0862 v1.4 §Concrete Impl
//! Extension + mission `0871e-phase5c-1-cross-instance-drain`).
//! Mirrors the `cross_instance_tv.rs` pattern for
//! `RaftLikeDidWriteCoordinator`.
//!
//! ## Test vectors
//!
//! - TV-1 atomic_drain — 3 instances concurrent `submit_drain` for the
//!   same holder; exactly the elected leader commits. One WAL entry.
//! - TV-2 leader_failover — lease expires; B becomes leader; B's
//!   subsequent drain commits.
//! - TV-3 wal_replay — 3 drain entries appended; replay via
//!   `replay_wal` returns the same order with valid checksums.
//! - TV-4 fail_closed — no elected writer → drain fails-closed.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

use octo_ident::ChainId;
use octo_sync::substrate::{
    replay_wal, ActualDrained, Cluster, DrainCoordinator, DrainCoordinatorError,
    RaftLikeDrainCoordinator, RaftLikeWriterElection, ReplayState, ShardKey, WriterContext,
    WriterElection, WriterNodeId, ENTRY_TYPE_DRAIN, WAL_MAGIC_V13,
};

/// 3-instance fixture: instances A, B, C + 1 shared cluster.
fn fixture() -> (Arc<Cluster>, [WriterNodeId; 3], ChainId) {
    let cluster = Cluster::new();
    let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
    let ids = [
        WriterNodeId([1u8; 32]),
        WriterNodeId([2u8; 32]),
        WriterNodeId([3u8; 32]),
    ];
    (cluster, ids, chain_id)
}

/// Build 3 coordinators sharing the cluster + election.
fn build_coordinators(
    cluster: Arc<Cluster>,
    ids: [WriterNodeId; 3],
    chain_id: ChainId,
) -> [Arc<RaftLikeDrainCoordinator>; 3] {
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
    let mut coordinators: Vec<Arc<RaftLikeDrainCoordinator>> = Vec::new();
    for (id, e) in ids.iter().zip(elections.iter()) {
        coordinators.push(Arc::new(RaftLikeDrainCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            *id,
            e.clone() as Arc<dyn WriterElection>,
        )));
    }
    // SAFETY: `Vec::into_iter` yields exactly 3 elements; we know
    // `coordinators` has 3 entries.
    let coord_a = coordinators.remove(0);
    let coord_b = coordinators.remove(0);
    let coord_c = coordinators.remove(0);
    [coord_a, coord_b, coord_c]
}

/// TV-1 atomic_drain — 3 instances concurrent drain of the same
/// holder; exactly the elected leader commits.
#[tokio::test]
async fn tv1_atomic_drain() {
    let (cluster, ids, chain_id) = fixture();
    let coordinators = build_coordinators(cluster.clone(), ids, chain_id.clone());

    let holder = "did:octo:zHolderA";
    let macaroon_id: [u8; 16] = [0xA1; 16];
    let shard_key = ShardKey::derive_canonical(holder.as_bytes());

    // Construct elections explicitly so we can acquire the lease.
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

    // All 3 drain coordinators attempt drain. Only the leader commits.
    let mut drain_results = Vec::new();
    for c in &coordinators {
        drain_results.push(c.submit_drain(holder, &macaroon_id, 100).await);
    }
    let oks: Vec<_> = drain_results.iter().filter(|r| r.is_ok()).collect();
    let fail_closed: Vec<_> = drain_results
        .iter()
        .filter(|r| matches!(r, Err(DrainCoordinatorError::WriterUnavailable)))
        .collect();
    assert_eq!(oks.len(), 1, "exactly one coordinator commits");
    assert_eq!(fail_closed.len(), 2, "non-leaders fail-closed");

    let leader_wal = cluster.read_wal_range(1, None);
    assert_eq!(leader_wal.len(), 1, "exactly one WAL entry");
    assert_eq!(leader_wal[0].entry_type, ENTRY_TYPE_DRAIN);
    assert_eq!(leader_wal[0].lsn, 1);
}

/// TV-2 leader_failover — lease expires; B becomes leader.
#[tokio::test]
async fn tv2_leader_failover() {
    let (cluster, ids, chain_id) = fixture();
    let coordinators = build_coordinators(cluster.clone(), ids, chain_id.clone());

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

    let holder = "did:octo:zHolderB";
    let macaroon_id: [u8; 16] = [0xB2; 16];
    let shard_key = ShardKey::derive_canonical(holder.as_bytes());

    // A acquires the lease.
    let leader_a = elections[0]
        .acquire_writer(&shard_key, 1_000)
        .await
        .unwrap();
    assert_eq!(leader_a.writer_node_id, ids[0]);

    // B relinquishes (simulating lease expiry + cleanup).
    elections[0].relinquish_writer(&shard_key).await.unwrap();

    // B acquires the lease.
    let leader_b = elections[1]
        .acquire_writer(&shard_key, 1_000)
        .await
        .unwrap();
    assert_eq!(leader_b.writer_node_id, ids[1]);

    // B's coordinator commits the drain.
    let r = coordinators[1]
        .submit_drain(holder, &macaroon_id, 50)
        .await
        .expect("B should drain after failover");
    assert_eq!(r.receipt_lsn, 1);

    // A's coordinator fails-closed (no longer leader).
    let r_a = coordinators[0]
        .submit_drain(holder, &macaroon_id, 100)
        .await;
    assert!(
        matches!(r_a, Err(DrainCoordinatorError::WriterUnavailable)),
        "A must fail-closed post-failover, got {r_a:?}"
    );
}

/// TV-3 wal_replay — 3 drain entries appended; replay via
/// `replay_wal` returns the same order with valid checksums.
#[tokio::test]
async fn tv3_wal_replay() {
    let (cluster, ids, chain_id) = fixture();
    let coordinators = build_coordinators(cluster.clone(), ids, chain_id.clone());

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

    // Acquire the shard for the holder so all 3 drains write to the
    // same leader + same shard_key (replay_wal requires single shard).
    let holder_c = "did:octo:zHolderC1";
    let shard_key_c = ShardKey::derive_canonical(holder_c.as_bytes());
    let _ = elections[0]
        .acquire_writer(&shard_key_c, 1_000)
        .await
        .unwrap();

    // Drain 3 entries on the same holder (same shard_key) so
    // replay_wal can validate the LSN chain end-to-end.
    for i in 1u8..=3u8 {
        let mac_id = [i; 16];
        let r: ActualDrained = coordinators[0]
            .submit_drain(holder_c, &mac_id, u128::from(i) * 10)
            .await
            .expect("A should drain");
        assert_eq!(r.receipt_lsn, u64::from(i));
    }

    // Now replay the WAL.
    let wal_entries = cluster.read_wal_range(1, None);
    assert_eq!(wal_entries.len(), 3);
    for (i, e) in wal_entries.iter().enumerate() {
        assert_eq!(e.entry_type, ENTRY_TYPE_DRAIN);
        assert_eq!(e.lsn, (i + 1) as u64);
        assert_eq!(e.magic, WAL_MAGIC_V13);
    }

    // Use the canonical replay function over the WAL reader.
    let reader = octo_sync::substrate::InMemoryWal::new(cluster.clone());
    let mut context = WriterContext {
        relinquish_pending: AtomicBool::new(false),
        flush_attempts: AtomicU32::new(0),
        max_attempts: 100,
        replay_state: ReplayState::Idle,
    };
    let tip = replay_wal(&mut context, 1, &shard_key_c, &reader)
        .await
        .unwrap();
    assert_eq!(tip, 3);
}

/// TV-4 fail_closed — no elected writer → drain fails-closed.
#[tokio::test]
async fn tv4_fail_closed() {
    let (cluster, ids, chain_id) = fixture();
    let coordinators = build_coordinators(cluster.clone(), ids, chain_id.clone());

    // No leader acquired for any shard.
    let r = coordinators[0]
        .submit_drain("did:octo:zHolderD", &[0xD4; 16], 500)
        .await;
    assert!(
        matches!(r, Err(DrainCoordinatorError::WriterUnavailable)),
        "no leader must fail-closed, got {r:?}"
    );

    // No WAL entry.
    let entries = cluster.read_wal_range(1, None);
    assert_eq!(entries.len(), 0);
}
