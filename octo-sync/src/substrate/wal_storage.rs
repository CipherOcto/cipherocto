//! In-memory WAL backing (per RFC-0862 v1.4 §Concrete Impl Extension).
//!
//! `InMemoryWal` implements the canonical `WalWriter` + `WalReader` +
//! `WalNonceScanner` traits on top of a shared `Arc<Cluster>`, plus
//! the local `governance::WalAppender` supertrait that `NonceTracker`
//! consumes. Production deployments replace this with a disk-backed WAL
//! (per RFC-0862 v1.3 §WAL Replay Algorithm + §Test Vectors) — the
//! `InMemoryWal` impl exists to exercise the substrate trait surface in
//! the cross-instance test harness.
//!
//! # Sealed trait pattern
//!
//! All four trait impls on the same struct are allowed because Rust
//! distinguishes methods by trait (the local `WalAppender` has a SYNC
//! `append_nonce_record`; the canonical `WalWriter` has an ASYNC
//! `append_nonce_record`). The cluster routes both to the same
//! `Cluster::append_nonce_record` method, so storage is single-source-of-truth.

use std::sync::Arc;

use async_trait::async_trait;

use super::cluster::Cluster;
use super::governance;
use super::records::{NonceRecord, WriterElectionError};
use super::wal::WalEntry;
use super::wal_traits::{WalNonceScanner, WalReader, WalWriter};

/// In-memory WAL impl backed by `Arc<Cluster>`.
pub struct InMemoryWal {
    cluster: Arc<Cluster>,
}

impl InMemoryWal {
    /// Construct a new `InMemoryWal` sharing state with the given cluster.
    pub fn new(cluster: Arc<Cluster>) -> Self {
        Self { cluster }
    }
}

#[async_trait]
impl WalWriter for InMemoryWal {
    async fn append_entry(&self, entry: &WalEntry) -> Result<u64, WriterElectionError> {
        self.cluster.append_wal_entry(entry.clone())
    }

    async fn append_nonce_record(&self, record: &NonceRecord) -> Result<(), WriterElectionError> {
        self.cluster.append_nonce_record(record.clone())
    }
}

#[async_trait]
impl WalReader for InMemoryWal {
    async fn read_range(
        &self,
        from_lsn: u64,
        to_lsn: Option<u64>,
    ) -> Result<Vec<WalEntry>, WriterElectionError> {
        Ok(self.cluster.read_wal_range(from_lsn, to_lsn))
    }
}

impl WalNonceScanner for InMemoryWal {
    fn scan_nonce_records(&self) -> Box<dyn Iterator<Item = NonceRecord> + '_> {
        Box::new(self.cluster.scan_nonce_records().into_iter())
    }
}

/// Local `governance::WalAppender` supertrait impl.
///
/// `NonceTracker` (in `governance.rs`) takes `Arc<dyn WalAppender>` and
/// needs sync `append_nonce_record` + `scan_nonce_records`. This impl
/// satisfies that without forcing `NonceTracker` onto the canonical
/// (async) `WalWriter` supertrait (per RFC-0862 v1.3 R13 M2: lift in
/// v1.4 amendment).
impl governance::WalAppender for InMemoryWal {
    fn append_nonce_record(&self, record: &NonceRecord) -> Result<(), WriterElectionError> {
        self.cluster.append_nonce_record(record.clone())
    }

    fn scan_nonce_records(&self) -> Box<dyn Iterator<Item = NonceRecord> + '_> {
        Box::new(self.cluster.scan_nonce_records().into_iter())
    }
}
