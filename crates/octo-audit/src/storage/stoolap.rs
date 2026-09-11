//! Stoolap-backed implementation of [`AppendOnlyAuditSink`] (RFC-0012
//! §Trait G3 mitigation).
//!
//! Persists audit events in a single `audit_events` table with
//! `(event_id PK, node_did, event_kind, cap_root_hash, at_millis_unix,
//! prev_chain_hash, chain_hash)`. Monotonicity enforced via
//! `MAX(event_id)` lookup on each append.
//!
//! Layer B facade (octo-audit) over the Layer A substrate
//! (octo-audit-core). The adapter is DOMAIN code per CLAUDE.md
//! §Architectural Principles: the substrate stays free of
//! Stoolap-specific types; the concrete impl lives here.

use std::sync::{Arc, Mutex};

use octo_audit_core::{
    compute_chain_hash, AppendOnlyAuditSink, AuditError, AuditEvent, AuditEventKind,
};
use octo_storage_core::Database;

/// DDL for the audit_events table. Run once on `open_in_memory` /
/// `open`.
const AUDIT_EVENTS_DDL: &str = "CREATE TABLE IF NOT EXISTS audit_events (
    event_id        INTEGER PRIMARY KEY,
    node_did        TEXT NOT NULL,
    event_kind      INTEGER NOT NULL,
    cap_root_hash   BLOB NOT NULL,
    at_millis_unix  INTEGER NOT NULL,
    prev_chain_hash BLOB NOT NULL,
    chain_hash      BLOB NOT NULL
)";

/// SQL tag for the event_kind column.
fn event_kind_sql(kind: AuditEventKind) -> i64 {
    match kind {
        AuditEventKind::Insert => 0,
        AuditEventKind::Revoke => 1,
        AuditEventKind::Sync => 2,
        // `#[non_exhaustive]` extension variants cannot exist yet
        // (the substrate owns the enum); defensive default until a
        // future amendment updates this match.
        _ => u8::MAX as i64,
    }
}

/// Errors from the storage layer (distinct from substrate's
/// `AuditError` for the cross-trait envelope).
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Stoolap engine failure.
    #[error("stoolap error: {0}")]
    Stoolap(String),
}

/// Stoolap-backed append-only audit sink.
///
/// Cheap-cloneable handle (`Arc<Mutex<Database>>`) — cloning the sink
/// shares the same underlying database (matches `StoolapStore` pattern
/// from `quota-router-sm-engine`).
#[derive(Clone)]
pub struct StoolapAuditSink {
    db: Arc<Mutex<Database>>,
}

impl std::fmt::Debug for StoolapAuditSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapAuditSink").finish_non_exhaustive()
    }
}

impl StoolapAuditSink {
    /// Open an in-memory database + create the schema.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let db = Database::open_in_memory()
            .map_err(|e| StorageError::Stoolap(format!("{e}")))?;
        db.execute(AUDIT_EVENTS_DDL, ())
            .map_err(|e| StorageError::Stoolap(format!("{e}")))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    /// Open a persistent database at the given path + create the schema.
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let db = Database::open(path).map_err(|e| StorageError::Stoolap(format!("{e}")))?;
        db.execute(AUDIT_EVENTS_DDL, ())
            .map_err(|e| StorageError::Stoolap(format!("{e}")))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }
}

impl AppendOnlyAuditSink for StoolapAuditSink {
    fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError> {
        let db = self.db.lock().expect("stoolap mutex poisoned");

        // Monotonicity check: last persisted event_id.
        let last = db
            .query("SELECT COALESCE(MAX(event_id), -1) FROM audit_events", ())
            .map_err(|e| AuditError::SinkSpecific(format!("{e}")))?;
        let last_id: i64 = last
            .into_iter()
            .next()
            .ok_or_else(|| AuditError::SinkSpecific("no MAX row".into()))?
            .map_err(|e| AuditError::SinkSpecific(format!("{e}")))?
            .get(0)
            .map_err(|e| AuditError::SinkSpecific(format!("{e}")))?;
        let expected = last_id + 1;
        if event.event_id != expected as u64 {
            return Err(AuditError::SequenceGap {
                event_id: event.event_id,
                prev: last_id.max(0) as u64,
            });
        }

        // Compute canonical chain_hash; reject mismatches.
        let expected_hash = compute_chain_hash(event);
        if event.chain_hash != expected_hash {
            return Err(AuditError::SinkSpecific(format!(
                "chain_hash mismatch at event_id {}",
                event.event_id
            )));
        }

        let insert_sql = "INSERT INTO audit_events
            (event_id, node_did, event_kind, cap_root_hash, at_millis_unix, prev_chain_hash, chain_hash)
            VALUES (?, ?, ?, ?, ?, ?, ?)";
        match db.execute(
            insert_sql,
            (
                event.event_id as i64,
                event.node_did.clone(),
                event_kind_sql(event.event_kind),
                event.cap_root_hash.to_vec(),
                event.at_millis_unix as i64,
                event.prev_chain_hash.to_vec(),
                event.chain_hash.to_vec(),
            ),
        ) {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = format!("{e}").to_lowercase();
                if msg.contains("unique") || msg.contains("duplicate") || msg.contains("primary") {
                    Err(AuditError::AlreadyExists(event.event_id))
                } else {
                    Err(AuditError::SinkSpecific(format!("{e}")))
                }
            }
        }
    }

    fn last_event_id(&self) -> Result<Option<u64>, AuditError> {
        let db = self.db.lock().expect("stoolap mutex poisoned");
        let rows = db
            .query("SELECT COALESCE(MAX(event_id), -1) FROM audit_events", ())
            .map_err(|e| AuditError::SinkSpecific(format!("{e}")))?;
        let Some(row_result) = rows.into_iter().next() else {
            return Ok(None);
        };
        let row = row_result.map_err(|e| AuditError::SinkSpecific(format!("{e}")))?;
        let last_id: i64 = row
            .get(0)
            .map_err(|e| AuditError::SinkSpecific(format!("{e}")))?;
        if last_id < 0 {
            Ok(None)
        } else {
            Ok(Some(last_id as u64))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(id: u64, at: u64, prev: [u8; 32]) -> AuditEvent {
        let mut e = AuditEvent {
            event_id: id,
            node_did: "did:oct:test".to_owned(),
            event_kind: AuditEventKind::Insert,
            cap_root_hash: [0xab; 32],
            at_millis_unix: at,
            prev_chain_hash: prev,
            chain_hash: [0; 32],
        };
        e.chain_hash = compute_chain_hash(&e);
        e
    }

    #[test]
    fn open_in_memory_succeeds() {
        let sink = StoolapAuditSink::open_in_memory();
        assert!(sink.is_ok(), "{:?}", sink.err());
    }

    #[test]
    fn append_increments_last_event_id() {
        let mut sink = StoolapAuditSink::open_in_memory().unwrap();
        assert_eq!(sink.last_event_id().unwrap(), None);
        let e0 = make_event(0, 1000, [0; 32]);
        sink.append(&e0).unwrap();
        assert_eq!(sink.last_event_id().unwrap(), Some(0));
        let e1 = make_event(1, 1100, e0.chain_hash);
        sink.append(&e1).unwrap();
        assert_eq!(sink.last_event_id().unwrap(), Some(1));
    }

    #[test]
    fn append_rejects_gap() {
        let mut sink = StoolapAuditSink::open_in_memory().unwrap();
        let e0 = make_event(0, 1000, [0; 32]);
        sink.append(&e0).unwrap();
        let e2 = make_event(2, 1200, e0.chain_hash); // skip 1
        let err = sink.append(&e2).unwrap_err();
        assert!(matches!(err, AuditError::SequenceGap { .. }));
    }

    #[test]
    fn append_rejects_duplicate() {
        // Duplicate event_id is rejected via the MAX-based monotonicity
        // check (SequenceGap variant, since the duplicate id != expected
        // successor). PK-based AlreadyExists is reserved for adapters
        // without the MAX pre-check.
        let mut sink = StoolapAuditSink::open_in_memory().unwrap();
        let e0 = make_event(0, 1000, [0; 32]);
        sink.append(&e0).unwrap();
        let err = sink.append(&e0).unwrap_err();
        assert!(matches!(err, AuditError::SequenceGap { .. }));
    }
}
