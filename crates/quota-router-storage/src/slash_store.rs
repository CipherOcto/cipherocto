//! `SlashStore` — persistence layer for the marketplace `SlashingLedger`.
//!
//! Mission `marketplace-slashing-persistence`: SlashingLedger state must
//! survive process restarts so banned providers remain banned
//! (RFC-0900 §Slashing Model). Trait is defined here (Layer-B stable
//! per `crates/quota-router-storage`) with a stoolap-backed impl.
//!
//! Mission `0900-d` (RFC-0900 amendment v2.0): substrate is
//! chain-aware per §20.3 Model B. PK = `(chain_id, provider_id)` per
//! the v015 migration. `SlashLedgerRow` carries a `chain_id: [u8; 32]`
//! field matching the typed `ChainId` per RFC-0010 v1.4. The amount
//! columns remain BIGINT (i64) at scale=0 via the documented
//! `dqa_to_i64` / `i64_to_dqa` bridge (stoolap fork does not expose a
//! native Dqa driver; the bridge text form matches the canonical
//! `DqaEncoding` 16-byte BE at scale=0 by i64 zero-extension).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::migrations;

/// Default chain namespace (32 bytes of zero) per RFC-0010 v1.4
/// `ChainId::default()`. Matches the v015 migration's `DEFAULT X'00...00'`
/// for backfilled pre-v015 rows.
pub const DEFAULT_CHAIN_ID: [u8; 32] = [0_u8; 32];

/// Persisted mirror of a `ProviderStake` (marketplace::slashing).
///
/// Field shape matches the `slash_ledger` table (v015). Kept separate
/// from the marketplace type so storage schema evolution does not
/// bleed into the public marketplace API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashLedgerRow {
    /// Mission 0900-d: typed `ChainId` per RFC-0010 v1.4. The same
    /// `provider_id` may carry one row per chain (cross-chain stake
    /// partitioning). Mirrors vault v013 PK pattern.
    pub chain_id: [u8; 32],
    pub provider_id: String,
    /// S4 codemod: `u128 → DqaSerde` (DQA-encoded deterministic
    /// floating-point). `DqaSerde` is a `#[repr(transparent)]` newtype
    /// over `Dqa` that exposes serde `Serialize`/`Deserialize` via the
    /// canonical 16-byte BE `DqaEncoding` wire format
    /// (see `crate::dqa_serde`). Always stored at `scale = 0`
    /// (integer micro-OCTO_W counts). Saturates to `i64::MAX` on
    /// overflow (well above realistic stake sizes per RFC-0871
    /// §Adversary A7).
    #[serde(with = "crate::dqa_serde::field")]
    pub stake_micro_octo_w: octo_determin::Dqa,
    #[serde(with = "crate::dqa_serde::field")]
    pub initial_stake_micro_octo_w: octo_determin::Dqa,
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
///
/// Mission 0900-d: `append_outcome` signature widens to include
/// `chain_id: [u8; 32]` so audit-table extensions can carry the
/// chain partition alongside the outcome record. The default impl
/// stays a no-op (extensions opt in).
pub trait SlashStore: Send + Sync {
    /// Load every persisted `SlashLedgerRow`.
    /// # Errors
    /// Returns `SlashStoreError::Db` on query failure.
    fn load_all(&self) -> Result<Vec<SlashLedgerRow>, SlashStoreError>;

    /// Upsert a provider's current stake state (per
    /// `(chain_id, provider_id)` partition).
    /// # Errors
    /// Returns `SlashStoreError::Db` on insert / update failure.
    fn upsert_stake(&self, row: &SlashLedgerRow) -> Result<(), SlashStoreError>;

    /// Append an outcome record (slash / dispute resolution).
    /// Default impl is a no-op (extensions may override for audit).
    /// # Errors
    /// Returns `SlashStoreError::Db` on insert failure.
    fn append_outcome(
        &self,
        _chain_id: [u8; 32],
        _provider_id: &str,
        _reason: &str,
        _amount_micro_octo_w: octo_determin::Dqa,
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

/// Read a 32-byte BLOB column into a `[u8; 32]` array. Stoolap exposes
/// `r.get::<Vec<u8>>(idx)` for BLOB columns on the iterator's
/// `ResultRow`; pad/truncate to 32 bytes.
fn read_chain_id(r: &stoolap::ResultRow, idx: usize) -> Result<[u8; 32], SlashStoreError> {
    let v: Vec<u8> = r
        .get::<Vec<u8>>(idx)
        .map_err(|e| SlashStoreError::Db(format!("chain_id: {e}")))?;
    if v.len() != 32 {
        return Err(SlashStoreError::Db(format!(
            "chain_id length {} != 32",
            v.len()
        )));
    }
    let mut out = [0_u8; 32];
    out.copy_from_slice(&v);
    Ok(out)
}

impl SlashStore for StoolapSlashStore {
    fn load_all(&self) -> Result<Vec<SlashLedgerRow>, SlashStoreError> {
        let mut rows = self
            .db
            .query(
                "SELECT chain_id, provider_id, stake_micro_octo_w, initial_stake_micro_octo_w, \
                 offense_count, cumulative_loss_pct_micro, last_updated_unix \
                 FROM slash_ledger",
                (),
            )
            .map_err(|e| SlashStoreError::Db(format!("load_all query: {e}")))?;
        let mut out = Vec::new();
        for r in &mut rows {
            let r = r.map_err(|e| SlashStoreError::Db(format!("load_all row: {e}")))?;
            out.push(SlashLedgerRow {
                chain_id: read_chain_id(&r, 0)?,
                provider_id: r
                    .get::<String>(1)
                    .map_err(|e| SlashStoreError::Db(format!("provider_id: {e}")))?,
                stake_micro_octo_w: i64_to_dqa(
                    r.get::<i64>(2)
                        .map_err(|e| SlashStoreError::Db(format!("stake: {e}")))?,
                )
                .map_err(|e| SlashStoreError::Db(format!("stake decode: {e:?}")))?,
                initial_stake_micro_octo_w: i64_to_dqa(
                    r.get::<i64>(3)
                        .map_err(|e| SlashStoreError::Db(format!("initial_stake: {e}")))?,
                )
                .map_err(|e| SlashStoreError::Db(format!("initial_stake decode: {e:?}")))?,
                offense_count: r
                    .get::<i64>(4)
                    .map_err(|e| SlashStoreError::Db(format!("offense_count: {e}")))?
                    as u32,
                cumulative_loss_pct_micro: r
                    .get::<i64>(5)
                    .map_err(|e| SlashStoreError::Db(format!("cumulative_loss_pct_micro: {e}")))?
                    as u64,
                last_updated_unix: r
                    .get::<i64>(6)
                    .map_err(|e| SlashStoreError::Db(format!("last_updated_unix: {e}")))?
                    as u64,
            });
        }
        Ok(out)
    }

    fn upsert_stake(&self, row: &SlashLedgerRow) -> Result<(), SlashStoreError> {
        // Mission 0900-d: chain-aware SELECT + INSERT-or-UPDATE. The
        // composite UNIQUE INDEX on (chain_id, provider_id) enforces
        // single-row-per-chain-per-provider at the DB layer; the
        // SELECT-then-INSERT-or-UPDATE pattern (held inside a
        // transaction) is the idempotent upsert pathway. Stoolap fork
        // does not enforce PRIMARY KEY uniqueness on INSERT (per Round
        // 1 audit), so we cannot rely on `INSERT ... ON CONFLICT`
        // semantics — must check first.
        let mut tx = self
            .db
            .begin()
            .map_err(|e| SlashStoreError::Db(format!("begin tx: {e}")))?;
        let mut existing = tx
            .query(
                "SELECT row_id FROM slash_ledger \
                 WHERE chain_id = $1 AND provider_id = $2",
                (row.chain_id.to_vec(), row.provider_id.clone()),
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
                    dqa_to_i64(row.stake_micro_octo_w),
                    dqa_to_i64(row.initial_stake_micro_octo_w),
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
            // Stoolap fork quirk (mission 0900-d, verified 2026-08-18):
            // the fork binds parameters by SCHEMA column order, not by
            // the column list in the INSERT statement. The v015 ALTER
            // TABLE placed `chain_id` at the END of the schema (after
            // the existing columns from v012). The INSERT column list
            // MUST therefore match schema order:
            //   row_id, provider_id, stake_micro_octo_w,
            //   initial_stake_micro_octo_w, offense_count,
            //   cumulative_loss_pct_micro, last_updated_unix, chain_id.
            // Same quirk found for the column-level UNIQUE autoindex
            // naming: the fork names it `unique_<table>_<col>` instead
            // of the SQLite convention `sqlite_autoindex_<table>_1`
            // — see `v015__chain_aware_slash_ledger.sql`.
            tx.execute(
                "INSERT INTO slash_ledger \
                 (row_id, provider_id, stake_micro_octo_w, initial_stake_micro_octo_w, \
                  offense_count, cumulative_loss_pct_micro, last_updated_unix, chain_id) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                (
                    next_id,
                    row.provider_id.clone(),
                    dqa_to_i64(row.stake_micro_octo_w),
                    dqa_to_i64(row.initial_stake_micro_octo_w),
                    row.offense_count as i64,
                    row.cumulative_loss_pct_micro as i64,
                    row.last_updated_unix as i64,
                    row.chain_id.to_vec(),
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

/// Saturating `Dqa -> i64` projection for stoolap INTEGER columns.
/// See `dqa_to_i64` in `stoolap_spend_ledger` for the same rationale.
///
/// `Dqa::value` is `i64`, so a direct pass-through is safe under
/// current types; the function name + Saturating comment pair stays
/// as the future-proofing doc anchor should `Dqa::value` widen to
/// `i128`.
fn dqa_to_i64(v: octo_determin::Dqa) -> i64 {
    assert_eq!(
        v.scale, 0,
        "stake stored at scale=0; schema invariant violated"
    );
    v.value
}

/// Lossy `i64 -> Dqa` projection (stoolap INTEGER column read).
fn i64_to_dqa(v: i64) -> Result<octo_determin::Dqa, octo_determin::DqaError> {
    octo_determin::Dqa::new(v, 0)
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
            chain_id: DEFAULT_CHAIN_ID,
            provider_id: "alice".to_string(),
            stake_micro_octo_w: octo_determin::Dqa::new(900_000, 0).expect("non-overflow"),
            initial_stake_micro_octo_w: octo_determin::Dqa::new(1_000_000, 0)
                .expect("non-overflow"),
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
            chain_id: DEFAULT_CHAIN_ID,
            provider_id: "bob".to_string(),
            stake_micro_octo_w: octo_determin::Dqa::new(1_000, 0).expect("non-overflow"),
            initial_stake_micro_octo_w: octo_determin::Dqa::new(1_000, 0).expect("non-overflow"),
            offense_count: 0,
            cumulative_loss_pct_micro: 0,
            last_updated_unix: 1,
        };
        store.upsert_stake(&r1).expect("upsert 1");
        let r2 = SlashLedgerRow {
            stake_micro_octo_w: octo_determin::Dqa::new(900, 0).expect("non-overflow"),
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

    #[test]
    fn cross_chain_same_provider_two_distinct_rows() {
        // Mission 0900-d AC-9: chain_id discriminator — rows in
        // different chains do NOT collapse on `load_all`.
        let store = StoolapSlashStore::open_in_memory().expect("open");
        let chain_a: [u8; 32] = [0xaa_u8; 32];
        let chain_b: [u8; 32] = [0xbb_u8; 32];
        let r1 = SlashLedgerRow {
            chain_id: chain_a,
            provider_id: "alice".to_string(),
            stake_micro_octo_w: octo_determin::Dqa::new(900_000, 0).expect("non-overflow"),
            initial_stake_micro_octo_w: octo_determin::Dqa::new(1_000_000, 0)
                .expect("non-overflow"),
            offense_count: 1,
            cumulative_loss_pct_micro: 100_000,
            last_updated_unix: 1_700_000_000,
        };
        let r2 = SlashLedgerRow {
            chain_id: chain_b,
            stake_micro_octo_w: octo_determin::Dqa::new(500_000, 0).expect("non-overflow"),
            initial_stake_micro_octo_w: octo_determin::Dqa::new(500_000, 0).expect("non-overflow"),
            offense_count: 0,
            cumulative_loss_pct_micro: 0,
            last_updated_unix: 1_700_000_001,
            ..r1.clone()
        };
        store.upsert_stake(&r1).expect("upsert a");
        store.upsert_stake(&r2).expect("upsert b");
        let loaded = store.load_all().expect("load_all");
        assert_eq!(loaded.len(), 2, "two chains = two rows");
        let mut by_chain: std::collections::HashMap<[u8; 32], SlashLedgerRow> =
            loaded.into_iter().map(|r| (r.chain_id, r)).collect();
        assert_eq!(by_chain.remove(&chain_a).expect("chain a"), r1);
        assert_eq!(by_chain.remove(&chain_b).expect("chain b"), r2);
    }
}
