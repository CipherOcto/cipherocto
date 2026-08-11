//! WAL trait surface (per RFC-0862 v1.3 §Supporting types + error enums).
//!
//! Per RFC-0862 v1.3 R12 M20: split `WalAppender` into three narrower
//! traits (`WalWriter`, `WalReader`, `WalNonceScanner`) to respect
//! Interface Segregation. A legacy `WalAppender` alias combines
//! `WalWriter + WalNonceScanner` for backward-compat with the
//! `NonceTracker` interface (per R13 M2: `#[async_trait]` is required
//! for dyn-compat).
//!
//! Also exposes `replay_wal` (per §WAL Replay Algorithm) and
//! `apply_entry` (per §Supporting types).

use async_trait::async_trait;

use super::ids::ShardKey;
use super::records::NonceRecord;
use super::records::WriterElectionError;
use super::state::ReplayState;
use super::state::WriterContext;
use super::wal::WalEntry;

/// Per-RFC-0862 v1.3 R12 M20 append-only WAL sink.
#[async_trait]
pub trait WalWriter: Send + Sync {
    /// Append a generic WAL entry. Returns the assigned LSN.
    async fn append_entry(&self, entry: &WalEntry) -> Result<u64, WriterElectionError>;
    /// Append a nonce record (entry type `ENTRY_TYPE_NONCE_RECORD`).
    async fn append_nonce_record(&self, record: &NonceRecord) -> Result<(), WriterElectionError>;
}

/// Per-RFC-0862 v1.3 R12 M20 WAL reader.
#[async_trait]
pub trait WalReader: Send + Sync {
    /// Read a range of WAL entries. `from_lsn` is inclusive; `to_lsn`
    /// is exclusive. `to_lsn = None` means "to the current tip".
    async fn read_range(
        &self,
        from_lsn: u64,
        to_lsn: Option<u64>,
    ) -> Result<Vec<WalEntry>, WriterElectionError>;
}

/// Per-RFC-0862 v1.3 R12 M20 nonce scanner (for `NonceTracker` replay).
///
/// Synchronous because the WAL reader is typically in-memory-backed
/// after a single cold-start scan. Follow-on implementations may
/// promote to async if the WAL is on slow storage.
pub trait WalNonceScanner: Send + Sync {
    /// Iterate over all nonce records (entry type `ENTRY_TYPE_NONCE_RECORD`).
    fn scan_nonce_records(&self) -> Box<dyn Iterator<Item = NonceRecord> + '_>;
}

/// Legacy alias combining `WalWriter + WalNonceScanner` (per
/// RFC-0862 v1.3 R13 M2: `#[async_trait]` keeps the trait object-safe).
///
/// `#[deprecated]` — prefer `WalWriter` + `WalReader` + `WalNonceScanner`
/// individually. `NonceTracker` (per governance.rs) still requires
/// this supertrait until the v1.4 amendment lifts the constraint.
#[async_trait]
#[deprecated(since = "1.3.0", note = "use WalWriter + WalReader + WalNonceScanner")]
pub trait WalAppender: WalWriter + WalNonceScanner {}

/// Per-RFC-0862 v1.3 §WAL Replay Algorithm.
///
/// Fail-closed on corruption (per R10 H3). Tracks `tip_lsn` and
/// rejects gaps + non-monotonic LSNs (per R10 H4). Takes `&mut WriterContext`
/// (per R10 H5). Apply failure → `ReplayState::Failed` (per R10 H6).
/// Verifies `entry.shard_key` (per R11 H14). Checksum covers full
/// 60-byte entry prefix + payload (per R12 H16).
pub async fn replay_wal(
    context: &mut WriterContext,
    start_lsn: u64,
    shard_key: &ShardKey,
    wal: &dyn WalReader,
) -> Result<u64, WriterElectionError> {
    let mut last_applied_lsn = start_lsn;
    let mut attempted_entries: u32 = 0;
    context.replay_state = ReplayState::InProgress {
        start_lsn,
        last_applied_lsn,
        attempted_entries,
    };
    let entries = wal.read_range(start_lsn, None).await?;
    let mut prev_lsn = start_lsn;
    for entry in entries {
        attempted_entries += 1;
        if entry.lsn != prev_lsn + 1 {
            context.replay_state = ReplayState::Failed {
                start_lsn,
                last_applied_lsn,
                attempted_entries,
                reason: "WAL LSN gap or non-monotonic",
            };
            return Err(WriterElectionError::WalCorruption);
        }
        let mut checksum_input = Vec::with_capacity(60 + entry.payload.len());
        checksum_input.extend_from_slice(&entry.prefix_bytes);
        checksum_input.extend_from_slice(&entry.payload);
        if entry.checksum != *blake3::hash(&checksum_input).as_bytes() {
            context.replay_state = ReplayState::Failed {
                start_lsn,
                last_applied_lsn,
                attempted_entries,
                reason: "WAL checksum mismatch",
            };
            return Err(WriterElectionError::WalCorruption);
        }
        if entry.shard_key != *shard_key {
            context.replay_state = ReplayState::Failed {
                start_lsn,
                last_applied_lsn,
                attempted_entries,
                reason: "WAL entry shard_key mismatch",
            };
            return Err(WriterElectionError::WalCorruption);
        }
        if let Err(e) = apply_entry(&entry, shard_key) {
            context.replay_state = ReplayState::Failed {
                start_lsn,
                last_applied_lsn,
                attempted_entries,
                reason: "apply failed",
            };
            return Err(e);
        }
        last_applied_lsn = entry.lsn;
        prev_lsn = entry.lsn;
        context.replay_state = ReplayState::InProgress {
            start_lsn,
            last_applied_lsn,
            attempted_entries,
        };
    }
    let tip_lsn = last_applied_lsn;
    context.replay_state = ReplayState::Complete {
        tip_lsn,
        total_entries: attempted_entries,
    };
    Ok(tip_lsn)
}

/// Per-RFC-0862 v1.3 §Supporting types: dispatch entry to its
/// per-entry-type apply logic. The default impl is a no-op stub;
/// concrete impls (per §V2 WAL header_size extension) dispatch on
/// `entry.entry_type` for `ENTRY_TYPE_DRAIN`,
/// `ENTRY_TYPE_DID_REGISTER`, `ENTRY_TYPE_DID_REVOKE`.
pub fn apply_entry(entry: &WalEntry, shard_key: &ShardKey) -> Result<(), WriterElectionError> {
    let _ = (entry, shard_key);
    Ok(())
}
