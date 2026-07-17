//! Test harness for Stoolap Data Sync E2E tests.
//!
//! Provides [`TestNode`], [`TestCluster`], and [`assert_converged`] for
//! driving in-process (L3), cross-process (L4), and container (L5) tests.

use std::sync::Arc;
use std::time::Duration;

use octo_sync::adapter::DatabaseSyncAdapter;
use octo_sync::config::{SyncConfig, SyncRole};
use octo_sync::identity::SyncPeerId;
use octo_sync::session::SyncSessionManager;
use octo_sync::test_util::MockAdapter;
use octo_sync::types::Lsn;

/// A single test node: owns a `MockAdapter` and a `SyncSessionManager`.
///
/// For L3 tests, all nodes run in the same process. Each node has its own
/// adapter (in-memory) and session manager. The test harness wires the
/// nodes together by passing `WalTailChunk`s between them.
pub struct TestNode {
    /// The node's identity (public key bytes).
    pub public_key: Vec<u8>,
    /// The underlying adapter.
    pub adapter: Arc<MockAdapter>,
    /// The session manager.
    pub session: SyncSessionManager,
}

impl TestNode {
    /// Create a node with a specific public key.
    pub fn with_key(mission_id: [u8; 32], role: SyncRole, public_key: Vec<u8>) -> Self {
        let node_id = octo_sync::identity::SyncNodeId::derive(&public_key, &mission_id);
        let adapter = Arc::new(MockAdapter::new(mission_id, *node_id.as_bytes()));

        let config = SyncConfig::new(mission_id, role, public_key.clone());
        let mission_root_key = [0x42u8; 32];
        let session = SyncSessionManager::new(
            adapter.clone() as Arc<dyn DatabaseSyncAdapter>,
            config,
            &mission_root_key,
        )
        .unwrap();

        Self {
            public_key,
            adapter,
            session,
        }
    }

    /// Return the local `SyncPeerId` for this node.
    pub fn peer_id(&self, mission_id: &[u8; 32]) -> SyncPeerId {
        SyncPeerId::derive(&self.public_key, mission_id)
    }

    /// Commit a WAL entry to the adapter and notify the session.
    ///
    /// Returns `(txn_id, from_lsn, to_lsn)` for the caller to fan out
    /// to readers.
    pub fn commit_entry(&self, data: &[u8]) -> (u64, Lsn, Lsn) {
        let prev_lsn = self.adapter.current_lsn().unwrap();
        self.adapter.apply_wal_entry(data).unwrap();
        let new_lsn = self.adapter.current_lsn().unwrap();
        let txn_id = prev_lsn; // simple txn_id = from_lsn
        (txn_id, prev_lsn + 1, new_lsn)
    }

    /// Commit N entries and return the chunks to ship to a reader.
    pub fn commit_entries(&self, n: usize) -> Vec<octo_sync::envelope::WalTailChunk> {
        let mut chunks = Vec::new();
        for i in 0..n {
            let data = format!("entry-{}", i).into_bytes();
            let (txn_id, from_lsn, to_lsn) = self.commit_entry(&data);
            let _ = self.session.on_commit(txn_id, from_lsn, to_lsn);
            // Drain the streamer's outbox for each peer and collect chunks.
            let subs = self.session.streamer().subscriber_count();
            if subs > 0 {
                // Read WAL entries directly from the adapter for the chunk.
                let entries = self.adapter.read_wal_range(from_lsn, to_lsn).unwrap();
                chunks.push(octo_sync::envelope::WalTailChunk {
                    from_lsn,
                    to_lsn,
                    entries,
                    is_last: true,
                });
            }
        }
        chunks
    }
}

/// A cluster of test nodes with in-process wiring.
///
/// The cluster manages peer subscriptions and provides helpers for
/// driving the sync protocol in tests.
pub struct TestCluster {
    /// The mission ID shared by all nodes.
    pub mission_id: [u8; 32],
    /// The nodes, indexed by position.
    nodes: Vec<TestNode>,
}

impl TestCluster {
    /// Create a new cluster with N nodes.
    ///
    /// `roles` assigns a role to each node. If `roles` is shorter than N,
    /// the remaining nodes get `Observer`.
    pub fn new(n: usize, roles: &[SyncRole]) -> Self {
        let mut mission_id = [0u8; 32];
        mission_id[0] = 0xAB;
        mission_id[1] = 0xCD;

        let mut nodes = Vec::with_capacity(n);
        for i in 0..n {
            let role = roles.get(i).copied().unwrap_or(SyncRole::Observer);
            let mut key = vec![0u8; 32];
            key[0] = (i + 1) as u8;
            let node = TestNode::with_key(mission_id, role, key);
            nodes.push(node);
        }

        Self { mission_id, nodes }
    }

    /// Return a reference to a node by index.
    pub fn node(&self, index: usize) -> &TestNode {
        &self.nodes[index]
    }

    /// Return a mutable reference to a node by index.
    pub fn node_mut(&mut self, index: usize) -> &mut TestNode {
        &mut self.nodes[index]
    }

    /// Return the number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Return `true` if the cluster is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Subscribe all nodes to each other (full mesh).
    ///
    /// Each node subscribes every other node as a peer in the WAL-tail
    /// streamer. This is the simplest topology for L3 tests.
    pub fn subscribe_mesh(&mut self) {
        let peer_ids: Vec<(usize, SyncPeerId)> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (i, n.peer_id(&self.mission_id)))
            .collect();

        for (i, node) in self.nodes.iter_mut().enumerate() {
            for (j, peer_id) in &peer_ids {
                if i != *j {
                    node.session.subscribe_peer(*peer_id).unwrap();
                }
            }
        }
    }

    /// Subscribe node `writer_idx` to feed `reader_idx`.
    ///
    /// The reader subscribes the writer as a peer in its streamer.
    pub fn subscribe_reader_to_writer(&mut self, reader_idx: usize, writer_idx: usize) {
        let writer_peer_id = self.nodes[writer_idx].peer_id(&self.mission_id);
        self.nodes[reader_idx]
            .session
            .subscribe_peer(writer_peer_id)
            .unwrap();
    }

    /// Subscribe `reader_idx` as a reader that receives from `writer_idx`.
    ///
    /// The writer subscribes the reader as a peer in its streamer.
    pub fn subscribe_writer_to_reader(&mut self, writer_idx: usize, reader_idx: usize) {
        let reader_peer_id = self.nodes[reader_idx].peer_id(&self.mission_id);
        self.nodes[writer_idx]
            .session
            .subscribe_peer(reader_peer_id)
            .unwrap();
    }

    /// Fan-out a chunk from writer to all subscribed readers.
    ///
    /// Returns the list of reader indices that successfully received the chunk.
    pub fn fan_out(
        &mut self,
        writer_idx: usize,
        chunk: &octo_sync::envelope::WalTailChunk,
    ) -> Vec<usize> {
        let writer_peer_id = self.nodes[writer_idx].peer_id(&self.mission_id);
        let mut received = Vec::new();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if i == writer_idx {
                continue;
            }
            if let Ok(applied) = node.session.apply_wal_tail(writer_peer_id, chunk) {
                if applied > 0 {
                    received.push(i);
                }
            }
        }
        received
    }

    /// Fan-out all chunks from writer to all subscribed readers.
    pub fn fan_out_all(
        &mut self,
        writer_idx: usize,
        chunks: &[octo_sync::envelope::WalTailChunk],
    ) -> Vec<usize> {
        let mut all_received = Vec::new();
        for chunk in chunks {
            let received = self.fan_out(writer_idx, chunk);
            all_received.extend(received);
        }
        all_received
    }

    /// Return the adapter for a node (for direct state inspection).
    pub fn adapter(&self, index: usize) -> &Arc<MockAdapter> {
        &self.nodes[index].adapter
    }
}

/// Wait until all nodes in the cluster have converged to the same LSN.
///
/// Polls every `poll_interval` up to `timeout`. Returns `Ok(())` when all
/// nodes agree, or `Err` with the current divergence info.
pub async fn assert_converged(
    cluster: &TestCluster,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let lsns: Vec<Lsn> = (0..cluster.len())
            .map(|i| cluster.adapter(i).current_lsn().unwrap())
            .collect();
        let all_same = lsns.windows(2).all(|w| w[0] == w[1]);
        if all_same {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "convergence failed after {:?}: lsns = {:?}",
                timeout, lsns
            ));
        }
        tokio::time::sleep(poll_interval).await;
    }
}
