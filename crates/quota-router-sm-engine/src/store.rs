//! Stoolap-backed implementation of the settlement store.
//!
//! RFC-0206 v2.1 §Migration Order: `Migration`/`apply_pending` legacy
//! aliases; deprecation noise silenced at module level.
#![allow(deprecated)]
//! Cipherocto wraps stoolap as an embedded SQL engine. Schema lives in
//! cipherocto (`schema.rs` + migrations); stoolap is the storage layer
//! per [[stoolap-general-purpose-db]] Path B.

use std::sync::{Arc, Mutex};

use crate::schema::apply_migrations;
use crate::{Ask, AskState, Receipt, SettlementError};

/// Errors from the storage layer (distinct from settlement logic errors).
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("stoolap error: {0}")]
    Stoolap(String),
    #[error("migration error: {0}")]
    Migration(#[from] octo_storage_core::_legacy_StorageError),
    #[error("row decode error: {0}")]
    Decode(String),
}

/// Settlement store trait.
///
/// Cipherocto's quota-router-core and Python SDK consume this trait.
/// `StoolapStore` is the production impl; tests can use an in-memory mock.
pub trait SettlementStore {
    fn mint(&self, ask: &Ask) -> Result<(), SettlementError>;
    fn settle(&self, ask_id: &[u8; 32], receipt: &Receipt) -> Result<[u8; 32], SettlementError>;
    fn consume(&self, receipt_id: &[u8; 32]) -> Result<(), SettlementError>;
    fn get(&self, ask_id: &[u8; 32]) -> Result<(Ask, AskState), SettlementError>;
}

/// Re-export the CipherOcto fork of stoolap's Database so the rest of
/// cipherocto depends on this crate, not stoolap directly. Path B.
pub use octo_storage_core::Database as StoolapDatabase;

/// CipherOcto-side wrapper around an embedded stoolap database.
#[derive(Clone)]
pub struct StoolapStore {
    db: Arc<Mutex<StoolapDatabase>>,
}

impl std::fmt::Debug for StoolapStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapStore").finish_non_exhaustive()
    }
}

impl StoolapStore {
    /// Open an in-memory stoolap database + apply migrations.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let db =
            StoolapDatabase::open_in_memory().map_err(|e| StorageError::Stoolap(e.to_string()))?;
        apply_migrations(&db)?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    /// Open a persistent stoolap database at the given path + apply migrations.
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let db = StoolapDatabase::open(path).map_err(|e| StorageError::Stoolap(e.to_string()))?;
        apply_migrations(&db)?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }
}

impl SettlementStore for StoolapStore {
    fn mint(&self, ask: &Ask) -> Result<(), SettlementError> {
        let db = self.db.lock().expect("stoolap mutex poisoned");
        let axes_bytes = serde_json::to_vec(&ask.axes_consumed)
            .map_err(|e| StorageError::Decode(e.to_string()))?;
        let output_hash_param: Option<Vec<u8>> = ask.output_hash.map(|h| h.to_vec());
        let sql = "INSERT INTO asks (
                ask_id, holder_did, axes_consumed, cap_root_hash, invocation_hash,
                current_unix_time, output_hash, settlement_hash, state, created_at
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, 'Minted', ?
            )";
        // `id` (PRIMARY KEY INTEGER rowid) auto-assigned by stoolap; do NOT include.
        let _ = sql;
        db.execute(
            sql,
            (
                ask.ask_id.to_vec(),
                ask.holder_did.clone(),
                axes_bytes,
                ask.cap_root_hash.to_vec(),
                ask.invocation_hash.to_vec(),
                ask.current_unix_time as i64,
                output_hash_param,
                vec![0u8; 32], // settlement_hash placeholder; locked at settle
                ask.current_unix_time as i64,
            ),
        )
        .map(|_| ())
        .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;
        Ok(())
    }

    fn settle(&self, ask_id: &[u8; 32], receipt: &Receipt) -> Result<[u8; 32], SettlementError> {
        // Compute settlement_hash = blake3(canonical_ser(ask || receipt)).
        let stored = self.get(ask_id)?;
        let mut canonical = Vec::new();
        canonical.extend_from_slice(
            &serde_json::to_vec(&stored.0)
                .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?,
        );
        canonical.extend_from_slice(
            &serde_json::to_vec(receipt)
                .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?,
        );
        let settlement_hash: [u8; 32] = *blake3::hash(&canonical).as_bytes();

        if receipt.ask_id != *ask_id {
            return Err(SettlementError::SettlementHashMismatch {
                expected: hex::encode(ask_id),
                got: hex::encode(receipt.ask_id),
            });
        }
        if stored.1 != AskState::Minted {
            return Err(SettlementError::InvalidTransition {
                from: stored.1,
                to: AskState::Settled,
            });
        }

        let db = self.db.lock().expect("stoolap mutex poisoned");
        let sql = "UPDATE asks
             SET state = 'Settled', settlement_hash = ?, settled_at = ?
             WHERE ask_id = ?";
        db.execute(
            sql,
            (
                settlement_hash.to_vec(),
                receipt.timestamp_unix as i64,
                ask_id.to_vec(),
            ),
        )
        .map(|_| ())
        .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;

        Ok(settlement_hash)
    }

    fn consume(&self, receipt_id: &[u8; 32]) -> Result<(), SettlementError> {
        let db = self.db.lock().expect("stoolap mutex poisoned");

        // First: is this receipt already consumed? (PK lookup)
        let dup_rows = db
            .query(
                "SELECT ask_id FROM consumed_receipt_index WHERE receipt_id = ?",
                (receipt_id.to_vec(),),
            )
            .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;
        if dup_rows.count() > 0 {
            return Err(SettlementError::AlreadyConsumed(hex::encode(receipt_id)));
        }

        // Find ask_id by settlement_hash (= receipt_id for our schema; in a
        // richer impl the receipt carries explicit ask_id).
        let ask_rows = db
            .query(
                "SELECT ask_id FROM asks WHERE settlement_hash = ?",
                (receipt_id.to_vec(),),
            )
            .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;
        let Some(row_result) = ask_rows.into_iter().next() else {
            return Err(SettlementError::AskNotFound(hex::encode(receipt_id)));
        };
        let row = row_result
            .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;
        let ask_id_bytes: Vec<u8> = row
            .get(0)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        if ask_id_bytes.len() != 32 {
            return Err(SettlementError::Storage(StorageError::Decode(format!(
                "ask_id wrong length: {}",
                ask_id_bytes.len()
            ))));
        }
        let mut ask_id = [0u8; 32];
        ask_id.copy_from_slice(&ask_id_bytes);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Insert into receipt index; on PK collision → AlreadyConsumed.
        let insert_sql = "INSERT INTO consumed_receipt_index (receipt_id, ask_id, consumed_at)
             VALUES (?, ?, ?)";
        match db.execute(
            insert_sql,
            (receipt_id.to_vec(), ask_id.to_vec(), now as i64),
        ) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("unique")
                    || msg.contains("constraint")
                    || msg.contains("duplicate")
                    || msg.contains("primary")
                {
                    return Err(SettlementError::AlreadyConsumed(hex::encode(receipt_id)));
                }
                return Err(SettlementError::Storage(StorageError::Stoolap(
                    e.to_string(),
                )));
            }
        }

        // Move ask state to Consumed (only if currently Settled).
        let update_sql = "UPDATE asks
             SET state = 'Consumed', consumed_at = ?
             WHERE ask_id = ? AND state = 'Settled'";
        db.execute(update_sql, (now as i64, ask_id.to_vec()))
            .map(|_| ())
            .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;

        Ok(())
    }

    fn get(&self, ask_id: &[u8; 32]) -> Result<(Ask, AskState), SettlementError> {
        let db = self.db.lock().expect("stoolap mutex poisoned");
        let sql = "SELECT holder_did, axes_consumed, cap_root_hash, invocation_hash,
                    current_unix_time, output_hash, state
             FROM asks WHERE ask_id = ?";
        let rows = db
            .query(sql, (ask_id.to_vec(),))
            .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;

        let Some(row_result) = rows.into_iter().next() else {
            return Err(SettlementError::AskNotFound(hex::encode(ask_id)));
        };
        let row = row_result
            .map_err(|e| SettlementError::Storage(StorageError::Stoolap(e.to_string())))?;

        let holder_did: String = row
            .get(0)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let axes_bytes: Vec<u8> = row
            .get(1)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let axes_consumed: Vec<(String, u64)> = serde_json::from_slice(&axes_bytes)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let cap_root_hash_vec: Vec<u8> = row
            .get(2)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let invocation_hash_vec: Vec<u8> = row
            .get(3)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let current_unix_time: i64 = row
            .get(4)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let output_hash: Option<Vec<u8>> = row
            .get(5)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;
        let state_sql: String = row
            .get(6)
            .map_err(|e| SettlementError::Storage(StorageError::Decode(e.to_string())))?;

        if cap_root_hash_vec.len() != 32 || invocation_hash_vec.len() != 32 {
            return Err(SettlementError::Storage(StorageError::Decode(format!(
                "hash field wrong length: cap={}, inv={}",
                cap_root_hash_vec.len(),
                invocation_hash_vec.len()
            ))));
        }
        let mut cap_root_hash = [0u8; 32];
        cap_root_hash.copy_from_slice(&cap_root_hash_vec);
        let mut invocation_hash = [0u8; 32];
        invocation_hash.copy_from_slice(&invocation_hash_vec);
        let output_hash_arr = match output_hash {
            Some(v) if v.len() == 32 => {
                let mut o = [0u8; 32];
                o.copy_from_slice(&v);
                Some(o)
            }
            Some(_) => {
                return Err(SettlementError::Storage(StorageError::Decode(
                    "output_hash wrong length".to_owned(),
                )))
            }
            None => None,
        };
        let state = AskState::from_sql(&state_sql).ok_or_else(|| {
            SettlementError::Storage(StorageError::Decode(format!(
                "unknown ask state: {state_sql}"
            )))
        })?;

        Ok((
            Ask {
                ask_id: *ask_id,
                holder_did,
                axes_consumed,
                cap_root_hash,
                invocation_hash,
                current_unix_time: current_unix_time as u64,
                output_hash: output_hash_arr,
            },
            state,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ask, Receipt, SettlementStore};

    fn sample_ask(ask_id: [u8; 32]) -> Ask {
        Ask {
            ask_id,
            holder_did: "did:octo:holder-001".to_owned(),
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            cap_root_hash: [0x11; 32],
            invocation_hash: [0x22; 32],
            current_unix_time: 1_700_000_000,
            output_hash: None,
        }
    }

    fn sample_receipt(ask_id: [u8; 32]) -> Receipt {
        Receipt {
            receipt_id: [0u8; 32], // overridden per test
            ask_id,
            settlement_hash: [0u8; 32],
            router_id: "router-alpha".to_owned(),
            router_sig: vec![0xab; 64],
            timestamp_unix: 1_700_000_010,
        }
    }

    #[test]
    fn open_in_memory_succeeds() {
        let store = StoolapStore::open_in_memory();
        assert!(store.is_ok(), "open_in_memory: {:?}", store.err());
    }

    #[test]
    fn mint_then_get_returns_ask() {
        let store = StoolapStore::open_in_memory().unwrap();
        let ask = sample_ask([0x01; 32]);
        store.mint(&ask).unwrap();
        let (got, state) = store.get(&ask.ask_id).unwrap();
        assert_eq!(got.holder_did, ask.holder_did);
        assert_eq!(state, AskState::Minted);
    }

    #[test]
    fn full_flow_mint_settle_consume() {
        let store = StoolapStore::open_in_memory().unwrap();
        let ask = sample_ask([0x02; 32]);
        store.mint(&ask).unwrap();

        // Settle.
        let mut receipt = sample_receipt(ask.ask_id);
        // receipt_id will be the settlement_hash returned by settle(); we
        // use a fixed id so consume() can find the ask via settlement_hash.
        receipt.receipt_id = [0u8; 32]; // placeholder; settle returns real hash
        let settlement_hash = store.settle(&ask.ask_id, &receipt).unwrap();
        assert_eq!(settlement_hash.len(), 32);

        // State must be Settled.
        let (_, state) = store.get(&ask.ask_id).unwrap();
        assert_eq!(state, AskState::Settled);

        // Consume using the settlement_hash as receipt_id (per current
        // schema mapping; richer impl uses explicit receipt_id).
        store.consume(&settlement_hash).unwrap();

        // State must be Consumed.
        let (_, state) = store.get(&ask.ask_id).unwrap();
        assert_eq!(state, AskState::Consumed);
    }

    #[test]
    fn consume_replay_returns_already_consumed() {
        let store = StoolapStore::open_in_memory().unwrap();
        let ask = sample_ask([0x03; 32]);
        store.mint(&ask).unwrap();
        let receipt = sample_receipt(ask.ask_id);
        let settlement_hash = store.settle(&ask.ask_id, &receipt).unwrap();

        store.consume(&settlement_hash).unwrap();
        let err = store.consume(&settlement_hash).unwrap_err();
        assert!(matches!(err, SettlementError::AlreadyConsumed(_)));
    }

    #[test]
    fn settle_rejects_already_settled() {
        let store = StoolapStore::open_in_memory().unwrap();
        let ask = sample_ask([0x04; 32]);
        store.mint(&ask).unwrap();
        let receipt = sample_receipt(ask.ask_id);
        store.settle(&ask.ask_id, &receipt).unwrap();
        let err = store.settle(&ask.ask_id, &receipt).unwrap_err();
        assert!(matches!(err, SettlementError::InvalidTransition { .. }));
    }
}
