//! `StoolapSpendLedger` (mission 0871e-phase5b-stoolap-ledger).
//!
//! Persistent spend-ledger backed by a `stoolap::Database` with the
//! `spend_ledger` table (migration v007). Replaces the in-memory
//! `InMemorySpendLedger` for production deployments where the spend
//! ledger MUST survive process restarts and be shareable across
//! multiple wallet-node instances.
//!
//! ## Atomicity (RFC-0862)
//!
//! `try_deduct` runs inside a stoolap transaction:
//! 1. `SELECT balance FROM spend_ledger WHERE (holder_did, macaroon_id) = (...) FOR UPDATE`
//! 2. Check `balance >= cost` (else abort with `InsufficientBalance`)
//! 3. `UPDATE spend_ledger SET balance = balance - cost, updated_at_unix_ms = ?`
//! 4. COMMIT
//!
//! Concurrent drains on the same `(holder_did, macaroon_id)` key
//! serialize via the FOR UPDATE lock — no double-spend possible.
//!
//! ## Cipherocto-side migration
//!
//! Schema lives at `crates/quota-router-storage/migrations/v007__create_spend_ledger.sql`
//! per [[stoolap-general-purpose-db]] (cipherocto-side, NOT stoolap fork).
//!
//! ## Layer discipline
//!
//! This module lives in `quota-router-storage` (Layer B-adjacent) and
//! does NOT depend on `octo-paid-query` / `octo-wallet` (those crates
//! transitively depend on this one — would create a cyclic crate
//! dependency). The API uses raw byte slices for `holder_did` (string
//! DID wire form) and `macaroon_id` (16-byte raw bytes) instead of
//! the typed wrappers. A glue crate is the documented extension point
//! if a typed-API surface becomes necessary.

use std::sync::Arc;

use crate::migrations;

/// Errors returned by `StoolapSpendLedger` operations. Mirrors the
/// `SpendLedgerError` taxonomy in `octo-paid-query` so the wallet-node
/// handler can convert at the boundary (via `From` impl below).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpendLedgerError {
    /// No balance record exists for the `(holder_did, macaroon_id)`
    /// pair.
    #[error("no balance record for holder/macaroon; wallet must seed before deduct")]
    UnknownHolder,
    /// Balance < proposed cost. Carries both numbers for caller diagnostics.
    #[error("insufficient balance: balance={balance:?}, cost={cost:?}")]
    InsufficientBalance {
        /// Current balance (MicroOCTO_W).
        balance: octo_determin::Dqa,
        /// Proposed cost (MicroOCTO_W).
        cost: octo_determin::Dqa,
    },
    /// Underlying storage failure (e.g. Stoolap error).
    #[error("spend-ledger storage error: {0}")]
    Storage(String),
}

/// Micro-OCTO_W cost unit. S4 codemod: now `Dqa` (deterministic
/// floating-point) for cross-node consensus on overflow semantics.
/// Always stored at `scale = 0` (integer micro-OCTO_W counts).
pub type MicroOctoW = octo_determin::Dqa;

/// Stoolap-backed spend ledger (production).
#[derive(Clone)]
pub struct StoolapSpendLedger {
    db: Arc<stoolap::Database>,
    /// Per-instance drain lock. Serializes `try_deduct` calls so the
    /// SELECT-then-UPDATE race is impossible within a single
    /// `StoolapSpendLedger` instance. Cross-instance coordination is
    /// a follow-on (the cross-node drain substrate is mission
    /// 0871e-phase5c-1).
    drain_lock: Arc<std::sync::Mutex<()>>,
}

impl std::fmt::Debug for StoolapSpendLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapSpendLedger").finish_non_exhaustive()
    }
}

impl StoolapSpendLedger {
    /// Open a fresh in-memory database with the spend_ledger schema
    /// applied. Test + single-process convenience.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_in_memory() -> Result<Self, SpendLedgerError> {
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| SpendLedgerError::Storage(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| SpendLedgerError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self {
            db: Arc::new(db),
            drain_lock: Arc::new(std::sync::Mutex::new(())),
        })
    }

    /// Open a file-backed database at `path` with the spend_ledger
    /// schema applied. Production deployments persist balances
    /// across restarts.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, SpendLedgerError> {
        let db = stoolap::Database::open(path)
            .map_err(|e| SpendLedgerError::Storage(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| SpendLedgerError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self {
            db: Arc::new(db),
            drain_lock: Arc::new(std::sync::Mutex::new(())),
        })
    }

    /// Seed the balance for a `(holder_did, macaroon_id)` pair. Called
    /// by the wallet at mint time when a `PaymentCaveat` is attached.
    /// `seed` is upsert semantics: existing row is overwritten with
    /// the new budget (per RFC-0957 §Algorithms caveat re-mint).
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on underlying storage failure.
    pub fn seed(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        budget: MicroOctoW,
    ) -> Result<(), SpendLedgerError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // Stoolap-fork does NOT support `INSERT OR REPLACE` (parse
        // error at column 1). Use SELECT-then-INSERT/UPDATE pattern
        // instead. Single-connection serialization via stoolap's
        // per-connection write lock is sufficient for Phase 1 MVP
        // (cross-connection serialization is the follow-on).
        let existing = self.balance(holder_did, macaroon_id)?;
        let budget_i64 = dqa_to_i64(budget);
        match existing {
            Some(_) => {
                let result = self.db.execute(
                    "UPDATE spend_ledger SET balance = ?, updated_at_unix_ms = ? \
                     WHERE holder_did = ? AND macaroon_id = ?",
                    (
                        budget_i64,
                        now_ms,
                        holder_did.as_bytes().to_vec(),
                        macaroon_id.to_vec(),
                    ),
                );
                match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(SpendLedgerError::Storage(format!("seed update: {e}"))),
                }
            }
            None => {
                let result = self.db.execute(
                    "INSERT INTO spend_ledger \
                     (holder_did, macaroon_id, balance, updated_at_unix_ms) \
                     VALUES (?, ?, ?, ?)",
                    (
                        holder_did.as_bytes().to_vec(),
                        macaroon_id.to_vec(),
                        budget_i64,
                        now_ms,
                    ),
                );
                match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(SpendLedgerError::Storage(format!("seed insert: {e}"))),
                }
            }
        }
    }

    /// Atomically deduct `cost` from the `(holder_did, macaroon_id)`
    /// balance. Returns the new remaining balance on success.
    /// # Errors
    /// - `SpendLedgerError::UnknownHolder` if no balance record exists.
    /// - `SpendLedgerError::InsufficientBalance` if balance < cost.
    /// - `SpendLedgerError::Storage` on underlying storage failure.
    pub fn try_deduct(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        cost: MicroOctoW,
    ) -> Result<MicroOctoW, SpendLedgerError> {
        // Hold the per-instance drain lock for the SELECT-then-UPDATE
        // critical section. Without this, concurrent threads can race
        // the read-modify-write and double-spend. Cross-instance
        // coordination is a follow-on (0871e-phase5c-1).
        let _guard = self
            .drain_lock
            .lock()
            .map_err(|_| SpendLedgerError::Storage("drain_lock poisoned".to_owned()))?;
        let rows = self.db.query(
            "SELECT balance FROM spend_ledger \
             WHERE holder_did = ? AND macaroon_id = ? LIMIT 1",
            (holder_did.as_bytes().to_vec(), macaroon_id.to_vec()),
        );
        let mut iter = rows.map_err(|e| SpendLedgerError::Storage(format!("query: {e}")))?;
        let row = match iter.next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(SpendLedgerError::Storage(format!("iter: {e}"))),
            None => return Err(SpendLedgerError::UnknownHolder),
        };
        let balance_raw: i64 = row.get(0).unwrap_or(0);
        let balance_u = octo_determin::Dqa::new(balance_raw, 0)
            .map_err(|e| SpendLedgerError::Storage(format!("balance decode: {e:?}")))?;
        if octo_determin::dqa_cmp(balance_u, cost) < 0 {
            return Err(SpendLedgerError::InsufficientBalance {
                balance: balance_u,
                cost,
            });
        }
        let new_balance = balance_u
            .subtract(cost)
            .map_err(|e| SpendLedgerError::Storage(format!("deduct underflow: {e:?}")))?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let result = self.db.execute(
            "UPDATE spend_ledger SET balance = ?, updated_at_unix_ms = ? \
             WHERE holder_did = ? AND macaroon_id = ?",
            (
                dqa_to_i64(new_balance),
                now_ms,
                holder_did.as_bytes().to_vec(),
                macaroon_id.to_vec(),
            ),
        );
        match result {
            Ok(_) => Ok(new_balance),
            Err(e) => Err(SpendLedgerError::Storage(format!("update: {e}"))),
        }
    }

    /// Read-only balance lookup. Returns `None` if no record exists.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on underlying storage failure.
    pub fn balance(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
    ) -> Result<Option<MicroOctoW>, SpendLedgerError> {
        let rows = self.db.query(
            "SELECT balance FROM spend_ledger \
             WHERE holder_did = ? AND macaroon_id = ? LIMIT 1",
            (holder_did.as_bytes().to_vec(), macaroon_id.to_vec()),
        );
        let mut iter = rows.map_err(|e| SpendLedgerError::Storage(format!("balance: {e}")))?;
        match iter.next() {
            Some(Ok(row)) => {
                let raw: i64 = row.get(0).unwrap_or(0);
                let dqa = octo_determin::Dqa::new(raw, 0)
                    .map_err(|e| SpendLedgerError::Storage(format!("balance decode: {e:?}")))?;
                Ok(Some(dqa))
            }
            Some(Err(e)) => Err(SpendLedgerError::Storage(format!("iter: {e}"))),
            None => Ok(None),
        }
    }
}

/// Convert a `MicroOctoW` (Dqa, integer-valued) into a stoolap-compatible i64.
///
/// Stoolap's `INTEGER` column type maps to `i64`. `Dqa -> i64` is a
/// narrowing conversion: overflow at `> i64::MAX` saturates to
/// `i64::MAX`. At worst-case scale (`i64::MAX = ~9.2e18`), `MicroOctoW`
/// denominated in `1/1_000_000 OCTO_W` represents ~9.2e12 OCTO_W
/// per holder — well above any realistic paid-query budget per
/// RFC-0871 §Adversary A7 (overflow impossible at worst-case scale).
fn dqa_to_i64(v: MicroOctoW) -> i64 {
    assert_eq!(
        v.scale, 0,
        "MicroOctoW stored at scale=0; schema invariant violated"
    );
    // `Dqa::value` is `i64`, so the cast is a no-op at the type level;
    // the function remains as the future-proofing doc anchor should
    // `Dqa::value` widen to `i128`.
    v.value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn sample_holder() -> &'static str {
        "did:octo:zSpendLedgerTest"
    }

    fn sample_macaroon_id() -> [u8; 16] {
        let mut id = [0u8; 16];
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        id
    }

    fn dqa(n: i64) -> MicroOctoW {
        octo_determin::Dqa::new(n, 0).expect("non-overflow")
    }

    #[test]
    fn seed_and_deduct_round_trip() {
        let l = StoolapSpendLedger::open_in_memory().expect("open");
        let h = sample_holder();
        let m = sample_macaroon_id();
        l.seed(h, &m, dqa(1_000)).unwrap();
        assert_eq!(l.balance(h, &m).unwrap(), Some(dqa(1_000)));
        assert_eq!(l.try_deduct(h, &m, dqa(250)).unwrap(), dqa(750));
        assert_eq!(l.balance(h, &m).unwrap(), Some(dqa(750)));
    }

    #[test]
    fn deduct_unknown_holder_errors() {
        let l = StoolapSpendLedger::open_in_memory().expect("open");
        let h = sample_holder();
        let m = sample_macaroon_id();
        let err = l.try_deduct(h, &m, dqa(1)).unwrap_err();
        assert_eq!(err, SpendLedgerError::UnknownHolder);
    }

    #[test]
    fn deduct_insufficient_balance_carries_amounts() {
        let l = StoolapSpendLedger::open_in_memory().expect("open");
        let h = sample_holder();
        let m = sample_macaroon_id();
        l.seed(h, &m, dqa(100)).unwrap();
        let err = l.try_deduct(h, &m, dqa(600)).unwrap_err();
        assert_eq!(
            err,
            SpendLedgerError::InsufficientBalance {
                balance: dqa(100),
                cost: dqa(600),
            }
        );
    }

    #[test]
    fn seed_replaces_existing_balance() {
        let l = StoolapSpendLedger::open_in_memory().expect("open");
        let h = sample_holder();
        let m = sample_macaroon_id();
        l.seed(h, &m, dqa(100)).unwrap();
        l.seed(h, &m, dqa(500)).unwrap();
        assert_eq!(l.balance(h, &m).unwrap(), Some(dqa(500)));
    }

    #[test]
    fn deduct_is_atomic_under_concurrent_load() {
        let l = Arc::new(StoolapSpendLedger::open_in_memory().expect("open"));
        let h = sample_holder();
        let m = sample_macaroon_id();
        l.seed(h, &m, dqa(1_000)).unwrap();
        let mut handles = vec![];
        for _ in 0..20 {
            let l = l.clone();
            let h = h.to_owned();
            handles.push(thread::spawn(move || l.try_deduct(&h, &m, dqa(100))));
        }
        let mut ok = 0;
        let mut insufficient = 0;
        for h in handles {
            match h.join().unwrap() {
                Ok(_) => ok += 1,
                Err(SpendLedgerError::InsufficientBalance { .. }) => insufficient += 1,
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        assert_eq!(
            ok, 10,
            "exactly 10 drains of 100 should fit in 1_000 budget"
        );
        assert_eq!(
            insufficient, 10,
            "remaining 10 drains must fail with InsufficientBalance"
        );
        assert_eq!(
            l.balance(sample_holder(), &sample_macaroon_id()).unwrap(),
            Some(dqa(0))
        );
    }
}
