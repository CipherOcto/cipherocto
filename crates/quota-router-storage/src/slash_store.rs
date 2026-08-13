//! `SlashStore` — persistence layer for the marketplace `SlashingLedger`.
//!
//! Mission `marketplace-slashing-persistence`: SlashingLedger state must
//! survive process restarts so banned providers remain banned
//! (RFC-0900 §Slashing Model). Trait is defined here (Layer-B stable
//! per `crates/quota-router-storage`) with a stoolap-backed impl.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::migrations;

/// Persisted mirror of a `ProviderStake` (marketplace::slashing).
///
/// Field shape matches the `slash_ledger` table (v012). Kept separate
/// from the marketplace type so storage schema evolution does not
/// bleed into the public marketplace API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashLedgerRow {
    pub provider_id: String,
    pub stake_micro_octo_w: u128,
    pub initial_stake_micro_octo_w: u128,
    pub offense_count: u32,
    /// `cumulative_loss_pct * 1_000_000`, rounded.
    pub cumulative_loss_pct_micro: u64,
    pub last_updated_unix: u64,
}

/// Persistence trait for the slashing ledger.
///
/// The trait lives in the storage crate (Layer-B stable) so that
/// extension crates can implement it against alternate backends (e.g.,
/// a remote-signer-controlled slashing store). The marketplace crate
/// consumes `Arc<dyn SlashStore>` via the `open()` constructor.
///
/// `append_outcome` is called after every `slash` /
/// `slash_with_pct` (write-through). For replay / audit purposes,
/// extensions may also append outcomes via the dedicated
/// `append_outcome` method without driving a full slash (e.g., a
/// dispute-resolution subsystem that records an outcome directly).
pub trait SlashStore: Send + Sync {
    /// Load every persisted `SlashLedgerRow`.
    /// # Errors
    /// Returns `SlashStoreError::Db` on query failure.
    fn load_all(&self) -> Result<Vec<SlashLedgerRow>, SlashStoreError>;

    /// Upsert a provider's current stake state.
    /// # Errors
    /// Returns `SlashStoreError::Db` on insert / update failure.
    fn upsert_stake(&self, row: &SlashLedgerRow) -> Result<(), SlashStoreError>;

    /// Append an outcome record (slash / dispute resolution).
    /// Default impl is a no-op (extensions may override for audit).
    /// # Errors
    /// Returns `SlashStoreError::Db` on insert failure.
    fn append_outcome(
        &self,
        _provider_id: &str,
        _reason: &str,
        _amount_micro_octo_w: u128,
        _new_cumulative_loss_pct_micro: u64,
    ) -> Result<(), SlashStoreError> {
        Ok(())
    }
}

/// Slash store errors.
#[derive(Debug, Error)]
pub enum SlashStoreError {
    #[error("database error: {0}")]
    Db(String),
    #[error("migration error: {0}")]
    Migration(#[from] migrations::MigrationError),
}

/// Stoolap-backed `SlashStore` impl (production).
///
/// Wraps a `stoolap::Database` handle. The database must have had
/// `migrations::apply_pending` invoked at startup; `open_in_memory`
/// and `open_path` helpers apply pending migrations for convenience.
pub struct StoolapSlashStore {
    db: stoolap::Database,
}

impl StoolapSlashStore {
    /// Open against an in-memory database. Applies pending migrations.
    /// # Errors
    /// Returns `SlashStoreError` on open / migration failure.
    pub fn open_in_memory() -> Result<Self, SlashStoreError> {
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| SlashStoreError::Db(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)?;
        Ok(Self { db })
    }

    /// Open against a file-backed database (DSN like `file:///path`).
    /// Applies pending migrations.
    /// # Errors
    /// Returns `SlashStoreError` on open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, SlashStoreError> {
        let db = stoolap::Database::open(path)
            .map_err(|e| SlashStoreError::Db(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)?;
        Ok(Self { db })
    }

    /// Wrap an existing database handle. Caller is responsible for
    /// applying pending migrations at startup.
    #[must_use]
    pub fn from_db(db: stoolap::Database) -> Self {
        Self { db }
    }
}

impl SlashStore for StoolapSlashStore {
    fn load_all(&self) -> Result<Vec<SlashLedgerRow>, SlashStoreError> {
        let mut rows = self
            .db
            .query(
                "SELECT provider_id, stake_micro_octo_w, initial_stake_micro_octo_w, \
                 offense_count, cumulative_loss_pct_micro, last_updated_unix \
                 FROM slash_ledger",
                (),
            )
            .map_err(|e| SlashStoreError::Db(format!("load_all query: {e}")))?;
        let mut out = Vec::new();
        for r in &mut rows {
            let r = r.map_err(|e| SlashStoreError::Db(format!("load_all row: {e}")))?;
            out.push(SlashLedgerRow {
                provider_id: r
                    .get::<String>(0)
                    .map_err(|e| SlashStoreError::Db(format!("provider_id: {e}")))?,
                stake_micro_octo_w: r
                    .get::<i64>(1)
                    .map_err(|e| SlashStoreError::Db(format!("stake: {e}")))?
                    as u128,
                initial_stake_micro_octo_w: r
                    .get::<i64>(2)
                    .map_err(|e| SlashStoreError::Db(format!("initial_stake: {e}")))?
                    as u128,
                offense_count: r
                    .get::<i64>(3)
                    .map_err(|e| SlashStoreError::Db(format!("offense_count: {e}")))?
                    as u32,
                cumulative_loss_pct_micro: r
                    .get::<i64>(4)
                    .map_err(|e| SlashStoreError::Db(format!("cumulative_loss_pct_micro: {e}")))?
                    as u64,
                last_updated_unix: r
                    .get::<i64>(5)
                    .map_err(|e| SlashStoreError::Db(format!("last_updated_unix: {e}")))?
                    as u64,
            });
        }
        Ok(out)
    }

    fn upsert_stake(&self, row: &SlashLedgerRow) -> Result<(), SlashStoreError> {
        // Stoolap fork does not enforce PRIMARY KEY uniqueness on
        // INSERT (per Round 1 audit); use SELECT-then-INSERT-or-UPDATE
        // inside a single transaction. The transaction holds the
        // executor lock for the duration so concurrent writers cannot
        // race the check-then-update-or-insert sequence.
        let mut tx = self
            .db
            .begin()
            .map_err(|e| SlashStoreError::Db(format!("begin tx: {e}")))?;
        let mut existing = tx
            .query(
                "SELECT row_id FROM slash_ledger WHERE provider_id = $1",
                (row.provider_id.clone(),),
            )
            .map_err(|e| SlashStoreError::Db(format!("check exists: {e}")))?;
        let result = if let Some(r) = existing.next() {
            let r = r.map_err(|e| SlashStoreError::Db(format!("row: {e}")))?;
            let row_id: i64 = r
                .get::<i64>(0)
                .map_err(|e| SlashStoreError::Db(format!("row_id: {e}")))?;
            tx.execute(
                "UPDATE slash_ledger SET stake_micro_octo_w = $1, initial_stake_micro_octo_w = $2, \
                 offense_count = $3, cumulative_loss_pct_micro = $4, last_updated_unix = $5 \
                 WHERE row_id = $6",
                (
                    row.stake_micro_octo_w as i64,
                    row.initial_stake_micro_octo_w as i64,
                    row.offense_count as i64,
                    row.cumulative_loss_pct_micro as i64,
                    row.last_updated_unix as i64,
                    row_id,
                ),
            )
            .map_err(|e| SlashStoreError::Db(format!("update: {e}")))?;
            Ok(())
        } else {
            // Pick a row_id via MAX+1 (stoolap INTEGER PK without
            // AUTO_INCREMENT per existing ask_repo convention).
            let max_q = tx
                .query("SELECT COALESCE(MAX(row_id), 0) FROM slash_ledger", ())
                .map_err(|e| SlashStoreError::Db(format!("max row_id: {e}")))?;
            let next_id: i64 = if let Some(r) = max_q.into_iter().next() {
                let row = r.map_err(|e| SlashStoreError::Db(format!("row: {e}")))?;
                row.get::<i64>(0)
                    .map_err(|e| SlashStoreError::Db(format!("get max: {e}")))?
            } else {
                1
            };
            let next_id = next_id + 1;
            tx.execute(
                "INSERT INTO slash_ledger \
                 (row_id, provider_id, stake_micro_octo_w, initial_stake_micro_octo_w, \
                  offense_count, cumulative_loss_pct_micro, last_updated_unix) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                (
                    next_id,
                    row.provider_id.clone(),
                    row.stake_micro_octo_w as i64,
                    row.initial_stake_micro_octo_w as i64,
                    row.offense_count as i64,
                    row.cumulative_loss_pct_micro as i64,
                    row.last_updated_unix as i64,
                ),
            )
            .map_err(|e| SlashStoreError::Db(format!("insert: {e}")))?;
            Ok(())
        };
        match result {
            Ok(()) => tx
                .commit()
                .map_err(|e| SlashStoreError::Db(format!("commit: {e}"))),
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_all_empty_on_fresh_db() {
        let store = StoolapSlashStore::open_in_memory().expect("open");
        let rows = store.load_all().expect("load_all");
        assert!(rows.is_empty(), "fresh db must have no rows");
    }

    #[test]
    fn upsert_then_load_round_trips_row() {
        let store = StoolapSlashStore::open_in_memory().expect("open");
        let row = SlashLedgerRow {
            provider_id: "alice".to_string(),
            stake_micro_octo_w: 900_000,
            initial_stake_micro_octo_w: 1_000_000,
            offense_count: 1,
            cumulative_loss_pct_micro: 100_000,
            last_updated_unix: 1_700_000_000,
        };
        store.upsert_stake(&row).expect("upsert");
        let loaded = store.load_all().expect("load_all");
        assert_eq!(loaded, vec![row]);
    }

    #[test]
    fn upsert_overwrites_existing_provider() {
        let store = StoolapSlashStore::open_in_memory().expect("open");
        let r1 = SlashLedgerRow {
            provider_id: "bob".to_string(),
            stake_micro_octo_w: 1_000,
            initial_stake_micro_octo_w: 1_000,
            offense_count: 0,
            cumulative_loss_pct_micro: 0,
            last_updated_unix: 1,
        };
        store.upsert_stake(&r1).expect("upsert 1");
        let r2 = SlashLedgerRow {
            stake_micro_octo_w: 900,
            offense_count: 1,
            cumulative_loss_pct_micro: 100_000,
            last_updated_unix: 2,
            ..r1.clone()
        };
        store.upsert_stake(&r2).expect("upsert 2");
        let loaded = store.load_all().expect("load_all");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], r2);
    }
}
