//! `AskRepository` — DAO over the cipherocto `asks` table (Phase C).
//!
//! Per [[stoolap-general-purpose-db]] principle: cipherocto owns consumer schema;
//! the fork is a general-purpose DB. Migrations live in `crates/octo-core/migrations/`
//! and are run at startup via `migrations::apply_pending`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ask::{settlement_cost, Ask, AxisConsumption, ModelRateTable, ModelRef, PricingAxis};
use crate::migrations;

/// Maximum candidates to fetch for `cheapest()` query before sorting in Rust.
/// Keeps memory bounded + index-friendly (no full table scan).
const CHEAPEST_CANDIDATE_LIMIT: i64 = 1000;

/// DAO errors.
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("database error: {0}")]
    Db(String),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("migration error: {0}")]
    Migration(#[from] migrations::MigrationError),
}

/// Persisted row mirror of the `asks` table.
///
/// Kept separate from `Ask` so future schema additions (e.g., audit columns)
/// don't bleed into the public type. Conversion is via `From` impls below.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskRow {
    pub ask_id: Vec<u8>,
    pub asker_did: String,
    pub model: String,
    pub rates_json: Vec<u8>,
    pub nonce: Vec<u8>,
    pub expires_at_unix: i64,
    pub created_at_unix: i64,
}

impl TryFrom<&Ask> for AskRow {
    type Error = RepoError;
    fn try_from(ask: &Ask) -> Result<Self, Self::Error> {
        let rates_json =
            serde_json::to_vec(&ask.rates).map_err(|e| RepoError::Serde(format!("rates: {e}")))?;
        Ok(Self {
            ask_id: ask.id().to_vec(),
            asker_did: ask.asker_did.clone(),
            model: ask.model.to_wire(),
            rates_json,
            nonce: ask.nonce.to_vec(),
            expires_at_unix: ask.expires_at_unix as i64,
            created_at_unix: now_unix() as i64,
        })
    }
}

impl TryFrom<AskRow> for Ask {
    type Error = RepoError;
    #[allow(dead_code)]
    fn try_from(row: AskRow) -> Result<Self, Self::Error> {
        if row.ask_id.len() != 32 {
            return Err(RepoError::Serde(format!(
                "ask_id must be 32 bytes, got {}",
                row.ask_id.len()
            )));
        }
        if row.nonce.len() != 16 {
            return Err(RepoError::Serde(format!(
                "nonce must be 16 bytes, got {}",
                row.nonce.len()
            )));
        }
        let rates: ModelRateTable = serde_json::from_slice(&row.rates_json)
            .map_err(|e| RepoError::Serde(format!("rates: {e}")))?;
        let mut ask_id = [0u8; 32];
        ask_id.copy_from_slice(&row.ask_id);
        let mut nonce = [0u8; 16];
        nonce.copy_from_slice(&row.nonce);
        Ok(Self {
            asker_did: row.asker_did,
            model: ModelRef::from(row.model),
            rates,
            nonce,
            expires_at_unix: row.expires_at_unix as u64,
        })
    }
}

/// Current Unix timestamp in seconds (clock-best-effort).
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Cipherocto-side DAO for the `asks` table.
///
/// Owns its embedded stoolap connection. All methods run queries via
/// the embedded DB; no caching layer at MVP.
#[derive(Clone)]
pub struct AskRepository {
    db: stoolap::Database,
}

impl AskRepository {
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

    /// Insert or replace an Ask.
    ///
    /// Uses a stoolap transaction (ReadCommitted isolation) so the check-then-update-or-insert
    /// sequence is serialized correctly under concurrent writers — without
    /// transactions, two writers computing `MAX(row_id)+1` in parallel could
    /// both pick the same `row_id` and the second INSERT would fail with a
    /// PK violation. The transaction holds the executor lock for the duration
    /// of the put, eliminating the race.
    /// # Errors
    /// Returns `RepoError::Db` on stoolap failure (begin, query, execute, commit).
    pub fn put(&self, ask: &Ask) -> Result<(), RepoError> {
        let row = AskRow::try_from(ask)?;
        let mut tx = self
            .db
            .begin()
            .map_err(|e| RepoError::Db(format!("begin tx: {e}")))?;
        let result = self.put_in_tx(&mut tx, &row);
        match result {
            Ok(()) => tx
                .commit()
                .map_err(|e| RepoError::Db(format!("commit: {e}"))),
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }
    }

    /// put() body executed inside a transaction (private helper).
    fn put_in_tx(&self, tx: &mut stoolap::ApiTransaction, row: &AskRow) -> Result<(), RepoError> {
        // Check if row already exists by ask_id.
        let mut existing = tx
            .query(
                "SELECT row_id FROM asks WHERE ask_id = $1",
                (row.ask_id.clone(),),
            )
            .map_err(|e| RepoError::Db(format!("check exists: {e}")))?;
        if let Some(row_result) = existing.next() {
            let existing_row = row_result.map_err(|e| RepoError::Db(format!("row: {e}")))?;
            let row_id: i64 = existing_row
                .get::<i64>(0)
                .map_err(|e| RepoError::Db(format!("row_id: {e}")))?;
            tx.execute(
                "UPDATE asks SET asker_did = $1, model = $2, rates_json = $3, nonce = $4, \
                 expires_at_unix = $5, created_at_unix = $6 WHERE row_id = $7",
                (
                    row.asker_did.clone(),
                    row.model.clone(),
                    row.rates_json.clone(),
                    row.nonce.clone(),
                    row.expires_at_unix,
                    row.created_at_unix,
                    row_id,
                ),
            )
            .map_err(|e| RepoError::Db(format!("update: {e}")))?;
        } else {
            // Need a row_id; use a max-or-zero placeholder. stoolap PRIMARY KEY = INTEGER
            // without AUTO_INCREMENT, so we must pick a row_id ourselves. Use MAX+1.
            let max_q = tx
                .query("SELECT COALESCE(MAX(row_id), 0) FROM asks", ())
                .map_err(|e| RepoError::Db(format!("max row_id: {e}")))?;
            let next_id: i64 = if let Some(r) = max_q.into_iter().next() {
                let row = r.map_err(|e| RepoError::Db(format!("row: {e}")))?;
                row.get::<i64>(0)
                    .map_err(|e| RepoError::Db(format!("get max: {e}")))?
            } else {
                1
            };
            let next_id = next_id + 1;
            tx.execute(
                "INSERT INTO asks \
                 (row_id, ask_id, asker_did, model, rates_json, nonce, expires_at_unix, created_at_unix) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                (next_id, row.ask_id.clone(), row.asker_did.clone(), row.model.clone(),
                 row.rates_json.clone(), row.nonce.clone(), row.expires_at_unix, row.created_at_unix),
            )
            .map_err(|e| RepoError::Db(format!("insert: {e}")))?;
        }
        Ok(())
    }

    /// Fetch an Ask by its content-addressable id.
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn get(&self, ask_id: &[u8; 32]) -> Result<Option<Ask>, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT ask_id, asker_did, model, rates_json, nonce, expires_at_unix, created_at_unix \
                 FROM asks WHERE ask_id = $1",
                (ask_id.to_vec(),),
            )
            .map_err(|e| RepoError::Db(format!("select by id: {e}")))?;
        match rows.into_iter().next() {
            Some(Ok(row)) => Ok(Some(row_to_ask(row)?)),
            Some(Err(e)) => Err(RepoError::Db(format!("row: {e}"))),
            None => Ok(None),
        }
    }

    /// List all non-expired Asks for `model`, sorted by computed cost ascending.
    ///
    /// Cost computation happens in Rust (the rates_json contains per-axis
    /// rates; the SQL query can't easily compute them). We fetch a bounded
    /// number of candidates and sort locally.
    ///
    /// `now_unix` is the cutoff for `expires_at_unix > now_unix`.
    /// `axes` supplies the standard pricing axes for cost computation.
    /// # Errors
    /// Returns `RepoError::Db` on query failure, `RepoError::Serde` on row decode failure.
    pub fn cheapest(
        &self,
        model: &str,
        now_unix: u64,
        axes: &[PricingAxis],
    ) -> Result<Option<Ask>, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT ask_id, asker_did, model, rates_json, nonce, expires_at_unix, created_at_unix \
                 FROM asks WHERE model = $1 AND expires_at_unix > $2 \
                 ORDER BY expires_at_unix DESC LIMIT $3",
                (model.to_string(), now_unix as i64, CHEAPEST_CANDIDATE_LIMIT),
            )
            .map_err(|e| RepoError::Db(format!("cheapest query: {e}")))?;
        let mut best: Option<(crate::ask::MicroOCTO_W, Ask)> = None;
        for row_result in rows {
            let row = row_result.map_err(|e| RepoError::Db(format!("row: {e}")))?;
            let ask = row_to_ask(row)?;
            // Use a 1-unit-per-axis cost proxy (cheapest = lowest total rate).
            let consumed: Vec<AxisConsumption> = ask
                .rates
                .rates
                .iter()
                .map(|r| (r.axis.clone(), 1000))
                .collect();
            let cost = settlement_cost(&ask, &consumed, axes);
            let replace = match &best {
                None => true,
                Some((best_cost, _)) => cost < *best_cost,
            };
            if replace {
                best = Some((cost, ask));
            }
        }
        Ok(best.map(|(_, a)| a))
    }

    /// List all non-expired Asks published by `asker_did`.
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn list_by_asker(&self, asker_did: &str, now_unix: u64) -> Result<Vec<Ask>, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT ask_id, asker_did, model, rates_json, nonce, expires_at_unix, created_at_unix \
                 FROM asks WHERE asker_did = $1 AND expires_at_unix > $2 \
                 ORDER BY created_at_unix DESC",
                (asker_did.to_string(), now_unix as i64),
            )
            .map_err(|e| RepoError::Db(format!("list_by_asker: {e}")))?;
        let mut out = Vec::new();
        for row_result in rows {
            let row = row_result.map_err(|e| RepoError::Db(format!("row: {e}")))?;
            out.push(row_to_ask(row)?);
        }
        Ok(out)
    }

    /// List all non-expired Asks across all askers. Used by
    /// `Marketplace::open_path` to hydrate the in-memory order book on
    /// restart (mission `marketplace-book-load-on-open`).
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn list_all_active_asks(&self, now_unix: u64) -> Result<Vec<Ask>, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT ask_id, asker_did, model, rates_json, nonce, expires_at_unix, created_at_unix \
                 FROM asks WHERE expires_at_unix > $1",
                (now_unix as i64,),
            )
            .map_err(|e| RepoError::Db(format!("list_all_active_asks: {e}")))?;
        let mut out = Vec::new();
        for row_result in rows {
            let row = row_result.map_err(|e| RepoError::Db(format!("row: {e}")))?;
            out.push(row_to_ask(row)?);
        }
        Ok(out)
    }

    /// Delete an Ask by id (used by settlement engine for one-shot consumption).
    /// # Errors
    /// Returns `RepoError::Db` on delete failure.
    pub fn delete(&self, ask_id: &[u8; 32]) -> Result<(), RepoError> {
        self.db
            .execute("DELETE FROM asks WHERE ask_id = $1", (ask_id.to_vec(),))
            .map_err(|e| RepoError::Db(format!("delete: {e}")))?;
        Ok(())
    }
}

/// Convert a stoolap `ResultRow` into an `Ask` (read typed columns via `FromValue`).
fn row_to_ask(row: stoolap::ResultRow) -> Result<Ask, RepoError> {
    let ask_id: Vec<u8> = row
        .get::<Vec<u8>>(0)
        .map_err(|e| RepoError::Db(format!("ask_id: {e}")))?;
    let asker_did: String = row
        .get::<String>(1)
        .map_err(|e| RepoError::Db(format!("asker_did: {e}")))?;
    let model: String = row
        .get::<String>(2)
        .map_err(|e| RepoError::Db(format!("model: {e}")))?;
    let rates_json: Vec<u8> = row
        .get::<Vec<u8>>(3)
        .map_err(|e| RepoError::Db(format!("rates_json: {e}")))?;
    let nonce: Vec<u8> = row
        .get::<Vec<u8>>(4)
        .map_err(|e| RepoError::Db(format!("nonce: {e}")))?;
    let expires_at_unix: i64 = row
        .get::<i64>(5)
        .map_err(|e| RepoError::Db(format!("expires_at_unix: {e}")))?;
    let _created_at_unix: i64 = row
        .get::<i64>(6)
        .map_err(|e| RepoError::Db(format!("created_at_unix: {e}")))?;

    if ask_id.len() != 32 {
        return Err(RepoError::Serde(format!(
            "ask_id must be 32 bytes, got {}",
            ask_id.len()
        )));
    }
    if nonce.len() != 16 {
        return Err(RepoError::Serde(format!(
            "nonce must be 16 bytes, got {}",
            nonce.len()
        )));
    }
    let rates: ModelRateTable = serde_json::from_slice(&rates_json)
        .map_err(|e| RepoError::Serde(format!("rates_json: {e}")))?;
    let mut id_arr = [0u8; 32];
    id_arr.copy_from_slice(&ask_id);
    let mut nonce_arr = [0u8; 16];
    nonce_arr.copy_from_slice(&nonce);
    Ok(Ask {
        asker_did,
        model: ModelRef::from(model),
        rates,
        nonce: nonce_arr,
        expires_at_unix: expires_at_unix as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask::{Ask, AxisRate, ModelRateTable, ModelRef};

    fn sample_ask(asker: &str, model: &str, rate: u128, expires: u64) -> Ask {
        Ask {
            asker_did: asker.to_owned(),
            model: ModelRef::from(model),
            rates: ModelRateTable {
                model: ModelRef::from(model),
                rates: vec![AxisRate {
                    axis: "input_tokens_per_1k".to_owned(),
                    rate_per_1k: rate,
                }],
            },
            nonce: [0x42; 16],
            expires_at_unix: expires,
        }
    }

    #[test]
    fn open_in_memory_applies_migrations() {
        let _repo = AskRepository::open_in_memory().expect("open");
    }

    #[test]
    fn put_and_get_roundtrip() {
        let repo = AskRepository::open_in_memory().unwrap();
        let ask = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "openai/gpt-4",
            30_000,
            1_900_000_000,
        );
        let id = ask.id();
        repo.put(&ask).unwrap();
        let got = repo.get(&id).unwrap().expect("get");
        assert_eq!(got.asker_did, ask.asker_did);
        assert_eq!(got.model, ask.model);
        assert_eq!(got.expires_at_unix, ask.expires_at_unix);
        assert_eq!(got.id(), id);
    }

    #[test]
    fn put_overwrites_existing() {
        let repo = AskRepository::open_in_memory().unwrap();
        let mut ask = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "openai/gpt-4",
            30_000,
            1_900_000_000,
        );
        repo.put(&ask).unwrap();
        // Update rate, keep same id (since nonce + asker + model + rates are identical).
        ask.rates.rates[0].rate_per_1k = 25_000;
        repo.put(&ask).unwrap();
        let got = repo.get(&ask.id()).unwrap().unwrap();
        assert_eq!(got.rates.rates[0].rate_per_1k, 25_000);
    }

    #[test]
    fn get_missing_returns_none() {
        let repo = AskRepository::open_in_memory().unwrap();
        let missing = [0x99; 32];
        assert!(repo.get(&missing).unwrap().is_none());
    }

    #[test]
    fn cheapest_returns_lowest_rate() {
        let repo = AskRepository::open_in_memory().unwrap();
        let axes = PricingAxis::standard_axes();
        let now = 1_700_000_000;
        let cheap = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "openai/gpt-4",
            20_000,
            now + 1000,
        );
        let mid = sample_ask("did:octo:b", "openai/gpt-4", 30_000, now + 1000);
        let exp = sample_ask(
            &octo_ident::test_helpers::sample_did(50),
            "openai/gpt-4",
            10_000,
            now + 1000,
        );
        repo.put(&mid).unwrap();
        repo.put(&cheap).unwrap();
        repo.put(&exp).unwrap();
        let winner = repo
            .cheapest("openai/gpt-4", now, &axes)
            .unwrap()
            .expect("cheapest");
        assert_eq!(winner.asker_did, exp.asker_did);
        assert_eq!(winner.rates.rates[0].rate_per_1k, 10_000);
    }

    #[test]
    fn cheapest_excludes_expired() {
        let repo = AskRepository::open_in_memory().unwrap();
        let axes = PricingAxis::standard_axes();
        let now = 1_700_000_000;
        let active = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "openai/gpt-4",
            50_000,
            now + 1000,
        );
        let expired = sample_ask("did:octo:b", "openai/gpt-4", 1_000, now - 100);
        repo.put(&active).unwrap();
        repo.put(&expired).unwrap();
        let winner = repo
            .cheapest("openai/gpt-4", now, &axes)
            .unwrap()
            .expect("cheapest");
        // The expired (cheaper) Ask MUST NOT be returned.
        assert_eq!(winner.asker_did, active.asker_did);
        assert_eq!(winner.rates.rates[0].rate_per_1k, 50_000);
    }

    #[test]
    fn cheapest_unknown_model_returns_none() {
        let repo = AskRepository::open_in_memory().unwrap();
        let axes = PricingAxis::standard_axes();
        assert!(repo
            .cheapest("nonexistent", 1_700_000_000, &axes)
            .unwrap()
            .is_none());
    }

    #[test]
    fn cheapest_returns_zero_rate_ask_as_winner() {
        // A zero-rate Ask wins cheapest() over a non-zero-rate Ask (free vs paid
        // for the same model). Documents that rate=0 doesn't crash division
        // and that the cost calculation handles edge values gracefully.
        let repo = AskRepository::open_in_memory().unwrap();
        let axes = PricingAxis::standard_axes();
        let now = 1_700_000_000;
        let free = sample_ask(
            &octo_ident::test_helpers::sample_did(22),
            "openai/gpt-4",
            0,
            now + 1000,
        );
        let paid = sample_ask(
            &octo_ident::test_helpers::sample_did(142),
            "openai/gpt-4",
            30_000,
            now + 1000,
        );
        repo.put(&paid).unwrap();
        repo.put(&free).unwrap();
        let winner = repo
            .cheapest("openai/gpt-4", now, &axes)
            .unwrap()
            .expect("cheapest");
        // Free Ask wins cheapest (lowest cost among candidates).
        assert_eq!(winner.asker_did, octo_ident::test_helpers::sample_did(22));
        // Cost for free Ask is non-negative (using default rates for axes not in the
        // Ask's rates table; rate=0 only for the axis explicitly listed in rates).
        let consumed: Vec<_> = axes.iter().map(|a| (a.id.clone(), 1000u64)).collect();
        let cost = crate::ask::settlement_cost(&winner, &consumed, &axes);
        assert!(cost < crate::ask::settlement_cost(&paid, &consumed, &axes));
    }

    #[test]
    fn put_under_transaction_serializes_writers() {
        // Smoke test: put() opens + commits a transaction. Run two puts serially
        // (the executor lock serializes; full concurrent test needs a thread pool
        // which is out of scope for the unit test).
        let repo = AskRepository::open_in_memory().unwrap();
        let now = 1_700_000_000;
        for i in 0..5 {
            let ask = sample_ask(
                &format!("did:octo:a{i}"),
                "openai/gpt-4",
                10_000 + i as u128 * 1000,
                now + 1000,
            );
            repo.put(&ask).unwrap();
        }
        // All 5 Asks present.
        for i in 0..5 {
            let ask = sample_ask(
                &format!("did:octo:a{i}"),
                "openai/gpt-4",
                10_000 + i as u128 * 1000,
                now + 1000,
            );
            let got = repo.get(&ask.id()).unwrap().expect("get");
            assert_eq!(got.asker_did, format!("did:octo:a{i}"));
        }
    }

    #[test]
    fn list_by_asker_filters() {
        let repo = AskRepository::open_in_memory().unwrap();
        let now = 1_700_000_000;
        let a1 = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "openai/gpt-4",
            10_000,
            now + 1000,
        );
        let a2 = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "anthropic/claude",
            20_000,
            now + 1000,
        );
        let b1 = sample_ask("did:octo:b", "openai/gpt-4", 30_000, now + 1000);
        for a in [&a1, &a2, &b1] {
            repo.put(a).unwrap();
        }
        let alice_asks = repo
            .list_by_asker(&octo_ident::test_helpers::sample_did(94), now)
            .unwrap();
        assert_eq!(alice_asks.len(), 2);
        for ask in &alice_asks {
            assert_eq!(ask.asker_did, octo_ident::test_helpers::sample_did(94));
        }
        let bob_asks = repo.list_by_asker("did:octo:b", now).unwrap();
        assert_eq!(bob_asks.len(), 1);
        let empty = repo
            .list_by_asker(&octo_ident::test_helpers::sample_did(100), now)
            .unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn delete_removes_ask() {
        let repo = AskRepository::open_in_memory().unwrap();
        let ask = sample_ask(
            &octo_ident::test_helpers::sample_did(94),
            "openai/gpt-4",
            30_000,
            1_900_000_000,
        );
        let id = ask.id();
        repo.put(&ask).unwrap();
        assert!(repo.get(&id).unwrap().is_some());
        repo.delete(&id).unwrap();
        assert!(repo.get(&id).unwrap().is_none());
    }

    #[test]
    fn apply_pending_is_idempotent() {
        // Run apply_pending twice on the same DB; second call MUST be no-op.
        let db = stoolap::Database::open_in_memory().unwrap();
        migrations::apply_pending(&db).unwrap();
        migrations::apply_pending(&db).unwrap(); // must not error
    }
}

// Ensure file ends with the mod test block close brace.
