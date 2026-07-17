//! Type aliases used in the [`DatabaseSyncAdapter`](crate::DatabaseSyncAdapter) trait signatures.
//!
//! Per RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait §Type aliases. All aliases are
//! newtypes around primitive types; they preserve strong typing at the trait
//! boundary without imposing runtime cost.

/// WAL Logical Sequence Number (monotonic per writer; per RFC-0862 §4.3.2).
///
/// The LSN counter is append-only per the WAL V2 binary format at
/// `stoolap/src/storage/mvcc/wal_manager.rs:69`. Implementations MUST return
/// LSNs in monotonically increasing order from `current_lsn()`.
pub type Lsn = u64;

/// Mission identifier (per RFC-0853 MissionKeyHierarchy).
///
/// Returned by [`DatabaseSyncAdapter::mission_id`](crate::DatabaseSyncAdapter::mission_id).
/// The cipherocto sync engine uses this to derive the per-mission `transport_key`
/// and `execution_key` via `HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)`.
pub type MissionId = [u8; 32];

/// `SyncNodeId = BLAKE3(public_key || mission_id)` (per RFC-0862 §4.3.1).
///
/// Returned by [`DatabaseSyncAdapter::node_id`](crate::DatabaseSyncAdapter::node_id).
/// MUST be stable for the lifetime of a sync session.
pub type NodeId = [u8; 32];

/// Database table identifier.
///
/// Assigned by the underlying engine (e.g., BLAKE3-256 of the table name in the
/// Stoolap fork, or the engine's own numeric table_id). The cipherocto sync engine
/// is agnostic to the assignment scheme; it just round-trips the value.
pub type TableId = u32;

/// Ordinal position of a snapshot segment within a table's snapshot directory.
///
/// `segment_index = 0` is the first (oldest) snapshot file; subsequent files
/// increment by 1. The mapping `segment_index → snapshot-<ts>.bin` is computed
/// on demand by the adapter impl.
pub type SegmentIndex = u32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_alias_sizes() {
        assert_eq!(std::mem::size_of::<Lsn>(), 8);
        assert_eq!(std::mem::size_of::<MissionId>(), 32);
        assert_eq!(std::mem::size_of::<NodeId>(), 32);
        assert_eq!(std::mem::size_of::<TableId>(), 4);
        assert_eq!(std::mem::size_of::<SegmentIndex>(), 4);
    }
}
