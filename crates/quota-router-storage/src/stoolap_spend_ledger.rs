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
//! ## Clock injection (mission 0862-c2)
//!
//! `updated_at_unix_ms` is sourced from an injected [`crate::clock::Clock`]
//! trait (default impl: `SystemClock`). Production deployments use
//! `SystemClock`; tests pin a deterministic `FixedClock` to validate
//! the column without time-mocking. Trait re-uses the
//! RFC-0957-A1 §Algorithms `Clock` already exported from
//! `crates/quota-router-storage/src/clock.rs` (mission 0957-c); this
//! mission only rewires the spend-ledger call sites.
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

use crate::clock::{Clock, SystemClock};
use crate::migrations;
use octo_determin::Dqa;

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
        /// Current balance (Dqa).
        balance: octo_determin::Dqa,
        /// Proposed cost (Dqa).
        cost: octo_determin::Dqa,
    },
    /// Cost is negative. `try_deduct` rejects negative cost as a
    /// precondition violation (defense-in-depth against signed
    /// underflow in caller fee-computation paths and wire-decoded
    /// `i64` amounts that would otherwise inflate the balance).
    /// Per RFC-0862 §StoolapSpendLedger preconditions.
    #[error("negative cost rejected: cost={cost:?}")]
    NegativeCost {
        /// The rejected cost.
        cost: octo_determin::Dqa,
    },
    /// `Dqa` scale outside the schema's accepted range. The spend
    /// ledger schema stores balances as `INTEGER` (i64) at `scale=0`
    /// (per RFC-0862 §StoolapSpendLedger substrate). A `Dqa` carrying
    /// a non-zero scale would lose precision on round-trip through
    /// the storage layer, so it is rejected here as a typed error
    /// rather than panicking. Per mission 0862-c4 (S6c Round 1
    /// security review finding #8 — `assert!` is not an error path).
    #[error("invalid Dqa scale: expected={expected}, actual={actual}")]
    InvalidScale {
        /// Scale the schema expects (currently 0; future widening
        /// to a multi-scale schema would bump this).
        expected: u8,
        /// Scale the caller supplied.
        actual: u8,
    },
    /// Underlying storage failure (e.g. Stoolap error).
    #[error("spend-ledger storage error: {0}")]
    Storage(String),
}

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
    /// Wall-clock source for `updated_at_unix_ms` column writes.
    /// Injected per mission 0862-c2 (RFC-0862 §StoolapSpendLedger
    /// substrate §Clock subsection). Default: `SystemClock`.
    /// Tests substitute `FixedClock` (mission 0862-c2 TV-0862-10).
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for StoolapSpendLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapSpendLedger").finish_non_exhaustive()
    }
}

impl StoolapSpendLedger {
    /// Open a fresh in-memory database with the spend_ledger schema
    /// applied. Test + single-process convenience. Defaults to
    /// `SystemClock` for the `updated_at_unix_ms` column; tests
    /// needing deterministic time use
    /// [`Self::open_in_memory_with_clock`].
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_in_memory() -> Result<Self, SpendLedgerError> {
        Self::open_in_memory_with_clock(Arc::new(SystemClock))
    }

    /// Open a fresh in-memory database with the spend_ledger schema
    /// applied and an injected wall-clock source. Test fixture used
    /// by mission 0862-c2 TV-0862-10 to pin `updated_at_unix_ms`.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_in_memory_with_clock(clock: Arc<dyn Clock>) -> Result<Self, SpendLedgerError> {
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| SpendLedgerError::Storage(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| SpendLedgerError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self {
            db: Arc::new(db),
            drain_lock: Arc::new(std::sync::Mutex::new(())),
            clock,
        })
    }

    /// Open a file-backed database at `path` with the spend_ledger
    /// schema applied. Production deployments persist balances
    /// across restarts. Defaults to `SystemClock`.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, SpendLedgerError> {
        Self::open_path_with_clock(path, Arc::new(SystemClock))
    }

    /// Open a file-backed database with an injected wall-clock source.
    /// Production deployments that need to pin time (e.g. replay
    /// tools) use this constructor.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_path_with_clock(
        path: &str,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, SpendLedgerError> {
        let db = stoolap::Database::open(path)
            .map_err(|e| SpendLedgerError::Storage(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| SpendLedgerError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self {
            db: Arc::new(db),
            drain_lock: Arc::new(std::sync::Mutex::new(())),
            clock,
        })
    }

    /// Seed the balance for a `(holder_did, macaroon_id)` pair. Called
    /// by the wallet at mint time when a `PaymentCaveat` is attached.
    /// `seed` is upsert semantics: existing row is overwritten with
    /// the new budget (per RFC-0957 §Algorithms caveat re-mint).
    /// # Preconditions
    /// - `budget.scale == 0` (Dqa at scale=0; same invariant as
    ///   `try_deduct`). Symmetric mirror of `try_deduct` precondition
    ///   guarding `dqa_to_i64` (per mission 0862-c8).
    /// - `budget.value >= 0`. Negative budget is rejected with
    ///   `SpendLedgerError::NegativeCost` (same defense-in-depth as
    ///   `try_deduct` against signed underflow in caller fee / wire
    ///   decode paths). Per mission 0862-c8 (Round 1 fix was
    ///   asymmetric — only `try_deduct` had the guard).
    /// # Atomicity
    /// Acquires `drain_lock` around the balance-read + UPDATE-or-INSERT
    /// window so concurrent `seed()` calls on the same
    /// `(holder_did, macaroon_id)` serialize (no double-mint, no
    /// masked PRIMARY KEY violation). Per mission 0862-c8.
    /// # Errors
    /// - `SpendLedgerError::InvalidScale` if `budget.scale != 0`.
    /// - `SpendLedgerError::NegativeCost` if `budget.value < 0`.
    /// - `SpendLedgerError::Storage` on underlying storage failure or
    ///   lock poisoning.
    pub fn seed(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        budget: Dqa,
    ) -> Result<(), SpendLedgerError> {
        // Scale precondition: schema stores INTEGER at scale=0 only.
        // A non-zero scale would lose precision on round-trip; surface
        // as a typed error per mission 0862-c4 (S6c Round 1 security
        // review finding #8) before touching the DB.
        if budget.scale != 0 {
            return Err(SpendLedgerError::InvalidScale {
                expected: 0,
                actual: budget.scale,
            });
        }
        // Negative-cost defense (mirrors try_deduct, per mission
        // 0862-c8).
        if budget.value < 0 {
            return Err(SpendLedgerError::NegativeCost { cost: budget });
        }
        // Mission 0862-c2: `updated_at_unix_ms` now sourced from the
        // injected `Clock` (default `SystemClock`). Production path
        // unchanged for callers (`SystemClock` default); tests pin
        // `FixedClock` for deterministic TV.
        let now_ms = self.clock.unix_millis() as i64;
        // Hold the per-instance drain lock for the
        // SELECT-then-INSERT/UPDATE critical section. Without this,
        // concurrent threads can race the read-modify-write and one
        // thread's INSERT collides with another's PRIMARY KEY.
        // Cross-instance coordination is a follow-on (0871e-phase5c-1).
        let _guard = self
            .drain_lock
            .lock()
            .map_err(|_| SpendLedgerError::Storage("drain_lock poisoned".to_owned()))?;
        // Stoolap-fork does NOT support `INSERT OR REPLACE` (parse
        // error at column 1). Use SELECT-then-INSERT/UPDATE pattern
        // instead.
        let existing = self.balance(holder_did, macaroon_id)?;
        let budget_i64 = dqa_to_i64(budget)?;
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
    /// # Preconditions
    /// - `cost.value >= 0`. Negative cost is rejected with
    ///   `SpendLedgerError::NegativeCost` (defense-in-depth against
    ///   signed underflow in caller fee-computation paths).
    /// # Errors
    /// - `SpendLedgerError::InvalidScale` if `cost.scale != 0`.
    /// - `SpendLedgerError::NegativeCost` if `cost.value < 0`.
    /// - `SpendLedgerError::UnknownHolder` if no balance record exists.
    /// - `SpendLedgerError::InsufficientBalance` if balance < cost.
    /// - `SpendLedgerError::Storage` on underlying storage failure.
    pub fn try_deduct(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        cost: Dqa,
    ) -> Result<Dqa, SpendLedgerError> {
        // Scale precondition: schema stores INTEGER at scale=0 only.
        // A non-zero scale would be normalized by `dqa_cmp`/`subtract`
        // and silently drain a fraction of the perceived cost — the
        // caller likely intends integer units. Surface as a typed
        // error per mission 0862-c4 BEFORE the DB hit / arithmetic
        // so an upstream caller bug surfaces cleanly without a
        // hidden partial-deduct.
        if cost.scale != 0 {
            return Err(SpendLedgerError::InvalidScale {
                expected: 0,
                actual: cost.scale,
            });
        }
        // Precondition: reject negative cost. `dqa_subtract` on
        // negative cost returns a "larger" value (i64 underflow
        // would inflate balance otherwise) — S4 Round 2 surfaced
        // the same class of bug elsewhere; pin it closed here.
        if cost.value < 0 {
            return Err(SpendLedgerError::NegativeCost { cost });
        }
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
        // Mission 0862-c2: injected `Clock` (see `seed` analogue above).
        let now_ms = self.clock.unix_millis() as i64;
        let result = self.db.execute(
            "UPDATE spend_ledger SET balance = ?, updated_at_unix_ms = ? \
             WHERE holder_did = ? AND macaroon_id = ?",
            (
                dqa_to_i64(new_balance)?,
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
    ) -> Result<Option<Dqa>, SpendLedgerError> {
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

    /// Test-only raw SQL query accessor.
    ///
    /// **Should NOT be used outside tests.** Production paths use the
    /// typed substrate methods (`seed` / `try_deduct` / `balance`);
    /// this accessor exists for fixture-level assertions that need to
    /// read columns the typed API does not surface (e.g. mission
    /// 0862-c2 TV-0862-10 asserts that the injected `Clock` value is
    /// written to the `updated_at_unix_ms` column — the typed
    /// `balance()` accessor returns only the `balance` column).
    ///
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on underlying SQL error.
    pub fn raw_query(
        &self,
        sql: &str,
        params: (Vec<u8>, Vec<u8>),
    ) -> Result<stoolap::Rows, SpendLedgerError> {
        self.db
            .query(sql, params)
            .map_err(|e| SpendLedgerError::Storage(format!("raw_query: {e}")))
    }
}

/// Convert a `Dqa` (Dqa, integer-valued) into a stoolap-compatible i64.
///
/// Stoolap's `INTEGER` column type maps to `i64`. `Dqa -> i64` is an
/// **identity conversion** today (`Dqa::value` is `i64`, so the cast is
/// a no-op at the type level). The function is preserved as the
/// future-proofing anchor should `Dqa::value` widen to `i128`.
///
/// **Precondition (mission 0862-c4):** `Dqa::scale` MUST be `0` —
/// the spend-ledger schema stores balances as `INTEGER`, which is
/// scale-implicit; a non-zero scale would lose precision on
/// round-trip. We enforce this as a typed
/// `SpendLedgerError::InvalidScale` (carries `expected = 0`,
/// `actual = v.scale`) rather than `panic!`. The check runs in both
/// debug and release so TV-0862-12 can pin the typed-error path
/// under `cargo test` (dev profile).
///
/// **Note (S6c Round 2 code review LOW #5):** this fn does NOT
/// implement saturation. Future widening to `i128` will require an
/// explicit `try_into()` + `SpendLedgerError::Storage` return on
/// overflow (mirroring the `0862-c7` adjacent u64→i64 wrap
/// mitigation in `quota-router-core`).
fn dqa_to_i64(v: Dqa) -> Result<i64, SpendLedgerError> {
    if v.scale != 0 {
        return Err(SpendLedgerError::InvalidScale {
            expected: 0,
            actual: v.scale,
        });
    }
    Ok(v.value)
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

    fn dqa(n: i64) -> Dqa {
        Dqa::new(n, 0).expect("non-overflow")
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
