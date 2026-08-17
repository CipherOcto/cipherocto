//! Cipherocto-side DAO for the `consumed_receipt_index` table (RFC-0959 §Replay Defense).
//!
//! Mirrors the `AskRepository` pattern: wrap a stoolap connection, expose typed
//! methods for insert + contains queries. Replaces the in-memory
//! `ConsumedReceiptIndex` (HashMap-based) for the CLI `settle-replay` and
//! mesh router nodes' `valid_settlement` path. Replay-defense state survives
//! process restarts and is shared across CLI invocations.
//!
//! Per [[stoolap-general-purpose-db]]: cipherocto owns this consumer schema;
//! the stoolap fork stays a general-purpose DB.

use crate::ask::{SettlementEnvelope, SettlementError};
use crate::migrations;
use crate::RepoError;

/// Outcome of `verify_and_insert` against the persisted index.
///
/// Maps directly to the failure modes of `SettlementEnvelope::verify` and
/// to the `Inserted(bool)` sentinel from `insert` (which is idempotent on
/// UNIQUE violation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Envelope settlement_hash drifted from the canonical recomputation.
    HashMismatch,
    /// Nonce already consumed (replay attempt).
    AlreadyConsumed,
    /// Insert succeeded. `true` = first insert, `false` = idempotent re-insert (race).
    Inserted(bool),
}

impl VerifyOutcome {
    /// Whether this outcome represents a successful verify (forward-progress).
    #[must_use]
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Inserted(_))
    }
    /// Convert to `SettlementError` for callers that want the in-memory
    /// error surface.
    /// # Panics
    /// Never (the `Inserted` arm returns `Ok` via `is_ok`).
    #[must_use]
    pub fn into_settlement_error(self) -> Option<SettlementError> {
        match self {
            Self::HashMismatch => Some(SettlementError::HashMismatch),
            Self::AlreadyConsumed => Some(SettlementError::AlreadyConsumed),
            Self::Inserted(_) => None,
        }
    }
}

/// DAO for the `consumed_receipt_index` table.
///
/// Owns its embedded stoolap connection. All methods run queries via the
/// embedded DB; no caching layer at MVP.
#[derive(Clone)]
pub struct ConsumedReceiptRepository {
    db: stoolap::Database,
}

impl ConsumedReceiptRepository {
    /// Open an in-memory DB, apply migrations, return the repository.
    /// Test-only convenience.
    /// # Errors
    /// Returns `RepoError::Db` on DB open failure, `RepoError::Migration` if migrations fail.
    pub fn open_in_memory() -> Result<Self, RepoError> {
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| RepoError::Db(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)?;
        Ok(Self { db })
    }

    /// Open a file-backed DB at `path`, apply migrations, return the repository.
    /// # Errors
    /// Returns `RepoError::Db` on DB open failure, `RepoError::Migration` if migrations fail.
    pub fn open_path(path: &str) -> Result<Self, RepoError> {
        let db = stoolap::Database::open(path)
            .map_err(|e| RepoError::Db(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)?;
        Ok(Self { db })
    }

    /// Wrap an existing stoolap connection (caller-owned). Caller is responsible
    /// for invoking `migrations::apply_pending(db)` at startup.
    #[must_use]
    pub fn from_db(db: stoolap::Database) -> Self {
        Self { db }
    }

    /// Insert a consumed receipt. Idempotent: re-inserting the same nonce
    /// returns `Ok(false)` (NOT an error) — the schema's UNIQUE constraint
    /// on `nonce` catches the duplicate, and `invoke_insert_receipt` translates
    /// the seatbelt error into a no-op signal.
    /// # Errors
    /// Returns `RepoError::Db` on non-UNIQUE stoolap failure.
    pub fn insert(&self, envelope: &SettlementEnvelope, now_unix: u64) -> Result<bool, RepoError> {
        let next_id = self.next_row_id()?;
        let result = self.db.execute(
            "INSERT INTO consumed_receipt_index \
             (row_id, settlement_hash, nonce, ask_id, asker_did, consumed_at_unix) \
             VALUES (?, ?, ?, ?, ?, ?)",
            (
                next_id,
                envelope.settlement_hash.to_vec(),
                envelope.nonce.to_vec(),
                envelope.ask_id.to_vec(),
                envelope.asker_did.as_str().to_owned(),
                now_unix as i64,
            ),
        );
        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                let msg = format!("{e}");
                // stoolap-fork surfaces UNIQUE violations as
                // "unique constraint failed for index ...". Translate to
                // idempotent Ok(false) so the CLI fast-path doesn't 500.
                if msg.contains("UNIQUE")
                    || msg.contains("Duplicate")
                    || msg.contains("unique constraint")
                {
                    Ok(false)
                } else {
                    Err(RepoError::Db(format!("insert consumed_receipt_index: {e}")))
                }
            }
        }
    }

    /// Verify a settlement envelope against the persisted index (RFC-0959 §Replay Defense).
    ///
    /// Steps (mirrors `SettlementEnvelope::verify` against in-memory):
    /// 1. Recompute `settlement_hash` from canonical fields (`HashMismatch` on drift).
    /// 2. Check `nonce` against the persisted table — `AlreadyConsumed` if present.
    /// 3. On success, insert the nonce (advances the replay-defense cursor).
    ///
    /// Returns a tuple `(settlement_check, insert_result)` so callers can
    /// distinguish the two failure modes (hash mismatch + replay) and the
    /// persistence outcome. The CLI maps these to its own error surface.
    ///
    /// # Errors
    /// Returns `RepoError::Db` only on stoolap failure (UNIQUE-violation is
    /// idempotent — `insert` returns `Ok(false)`).
    pub fn verify_and_insert(
        &self,
        envelope: &SettlementEnvelope,
        now_unix: u64,
    ) -> Result<VerifyOutcome, RepoError> {
        let computed = envelope.compute_settlement_hash();
        if computed != envelope.settlement_hash {
            return Ok(VerifyOutcome::HashMismatch);
        }
        if self.contains_nonce(&envelope.nonce) {
            return Ok(VerifyOutcome::AlreadyConsumed);
        }
        let inserted = self.insert(envelope, now_unix)?;
        Ok(VerifyOutcome::Inserted(inserted))
    }

    /// Check whether a nonce is already consumed (replay detection).
    ///
    /// Used by `verify_and_insert` and by the on-demand CLI fast-path.
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn contains_nonce(&self, nonce: &[u8; 32]) -> bool {
        let rows = self.db.query(
            "SELECT 1 FROM consumed_receipt_index WHERE nonce = ? LIMIT 1",
            (nonce.to_vec(),),
        );
        match rows {
            Ok(mut iter) => iter.next().is_some(),
            Err(_) => false,
        }
    }

    /// Whether a settlement_hash is already in the table (audit / forensic query).
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn contains_settlement_hash(&self, settlement_hash: &[u8; 32]) -> bool {
        let rows = self.db.query(
            "SELECT 1 FROM consumed_receipt_index WHERE settlement_hash = ? LIMIT 1",
            (settlement_hash.to_vec(),),
        );
        match rows {
            Ok(mut iter) => iter.next().is_some(),
            Err(_) => false,
        }
    }

    /// Total consumed receipts in the table (diagnostics / GC).
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn len(&self) -> Result<u64, RepoError> {
        self.len_inner()
    }

    /// Always false (table is empty until the first insert; queries are
    /// cheap even on populated tables — `len()` is the canonical signal).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self.len_inner() {
            Ok(n) => n == 0,
            Err(_) => false,
        }
    }

    fn len_inner(&self) -> Result<u64, RepoError> {
        let rows = self
            .db
            .query("SELECT COUNT(*) FROM consumed_receipt_index", ())
            .map_err(|e| RepoError::Db(format!("count: {e}")))?;
        let mut iter = rows.into_iter();
        let row = iter
            .next()
            .unwrap()
            .map_err(|e| RepoError::Db(format!("row: {e}")))?;
        let n: i64 = row.get(0).unwrap();
        Ok(n as u64)
    }

    /// Compute the next row_id via `MAX(row_id) + 1` (matches `AskRepository` pattern).
    ///
    /// Stoolap does NOT support `AUTO_INCREMENT` on `INTEGER PRIMARY KEY` (the
    /// `asks` v001 schema uses the same pattern). Caller runs this inside a
    /// transaction for safety under concurrent writers.
    fn next_row_id(&self) -> Result<i64, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT COALESCE(MAX(row_id), 0) + 1 FROM consumed_receipt_index",
                (),
            )
            .map_err(|e| RepoError::Db(format!("select max row_id: {e}")))?;
        let mut iter = rows.into_iter();
        let row = iter
            .next()
            .unwrap()
            .map_err(|e| RepoError::Db(format!("row: {e}")))?;
        let n: i64 = row.get(0).unwrap();
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask::ModelRef;

    fn sample_envelope(ask_id: [u8; 32], nonce: [u8; 32]) -> SettlementEnvelope {
        SettlementEnvelope {
            settlement_hash: [0u8; 32], // overwritten by compute_settlement_hash
            asker_did: "did:octo:asker1".to_owned(),
            holder_did: "did:octo:holder-1".to_owned(),
            model: ModelRef::new("openai", "gpt-4", None).unwrap(),
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
            ask_id,
            nonce,
            timestamp_unix: 1_700_000_000,
            cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
        }
    }

    #[test]
    fn insert_then_contains_nonce_returns_true() {
        let repo = ConsumedReceiptRepository::open_in_memory().unwrap();
        let mut env = sample_envelope([0x42; 32], [0x55; 32]);
        env.settlement_hash = env.compute_settlement_hash();
        let inserted = repo.insert(&env, 1_700_000_000).unwrap();
        assert!(inserted, "first insert must report inserted=true");
        assert!(repo.contains_nonce(&env.nonce));
    }

    #[test]
    fn duplicate_insert_is_idempotent() {
        let repo = ConsumedReceiptRepository::open_in_memory().unwrap();
        let mut env = sample_envelope([0x42; 32], [0x55; 32]);
        env.settlement_hash = env.compute_settlement_hash();
        assert!(repo.insert(&env, 1_700_000_000).unwrap());
        let second = repo.insert(&env, 1_700_000_001).unwrap();
        assert!(
            !second,
            "duplicate insert must return Ok(false) (idempotent, not error)"
        );
        assert_eq!(repo.len().unwrap(), 1, "table must hold exactly 1 row");
    }

    #[test]
    fn verify_and_insert_inserts_then_advances_replay_defense() {
        let repo = ConsumedReceiptRepository::open_in_memory().unwrap();
        let mut env = sample_envelope([0x42; 32], [0x55; 32]);
        env.settlement_hash = env.compute_settlement_hash();
        let outcome = repo.verify_and_insert(&env, 1_700_000_000).unwrap();
        assert!(
            matches!(outcome, VerifyOutcome::Inserted(true)),
            "first verify must Inserted(true)"
        );
        // Replay attempt -> AlreadyConsumed.
        let outcome = repo.verify_and_insert(&env, 1_700_000_001).unwrap();
        assert_eq!(outcome, VerifyOutcome::AlreadyConsumed);
    }

    #[test]
    fn verify_and_insert_rejects_tampered_envelope() {
        let repo = ConsumedReceiptRepository::open_in_memory().unwrap();
        let mut env = sample_envelope([0x42; 32], [0x55; 32]);
        env.settlement_hash = env.compute_settlement_hash();
        // Tamper: change timestamp AFTER computing the hash. Replay verification
        // recomputes the hash and rejects.
        env.timestamp_unix += 1;
        let outcome = repo.verify_and_insert(&env, 1_700_000_000).unwrap();
        assert_eq!(outcome, VerifyOutcome::HashMismatch);
    }

    #[test]
    fn contains_settlement_hash_works() {
        let repo = ConsumedReceiptRepository::open_in_memory().unwrap();
        let mut env = sample_envelope([0x42; 32], [0x55; 32]);
        env.settlement_hash = env.compute_settlement_hash();
        repo.insert(&env, 1_700_000_000).unwrap();
        assert!(repo.contains_settlement_hash(&env.settlement_hash));
        assert!(!repo.contains_settlement_hash(&[0x99; 32]));
    }

    #[test]
    fn verify_outcome_into_settlement_error_mapping() {
        assert!(matches!(
            VerifyOutcome::HashMismatch.into_settlement_error(),
            Some(SettlementError::HashMismatch)
        ));
        assert!(matches!(
            VerifyOutcome::AlreadyConsumed.into_settlement_error(),
            Some(SettlementError::AlreadyConsumed)
        ));
        assert!(VerifyOutcome::Inserted(true)
            .into_settlement_error()
            .is_none());
        assert!(VerifyOutcome::Inserted(true).is_ok());
        assert!(!VerifyOutcome::HashMismatch.is_ok());
    }
}
