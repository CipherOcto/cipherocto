//! `StoolapSpendLedger` (mission 0871e-phase5b-stoolap-ledger).
//!
//! Persistent spend-ledger backed by a `octo_storage_core::Database` with the
//! `spend_ledger` table (migration v007). Replaces the in-memory
//! `InMemorySpendLedger` for production deployments where the spend
//! ledger MUST survive process restarts and be shareable across
//! multiple wallet-node instances.
//!
//! ## Atomicity (RFC-0862)
//!
//! `try_deduct` runs inside a stoolap transaction wrapped by the
//! per-instance `drain_lock` (S6c Round 2 documented here; pre-c3
//! doc claimed `SELECT ... FOR UPDATE` row-locking — the stoolap
//! fork does not implement `FOR UPDATE` and the SQL never carried
//! the clause; corrected in mission 0862-c10):
//!
//! 1. Acquire `drain_lock` (per-instance `Mutex<()>`, mission 0862-c8)
//! 2. Begin stoolap `Transaction` (`db.begin() -> Transaction`)
//!    — mission 0862-c3 AC-2 layer.
//! 3. `Transaction::query("SELECT balance FROM spend_ledger WHERE
//!    holder_did = ? AND macaroon_id = ? LIMIT 1")`.
//!    No `FOR UPDATE` (stoolap storage layer returns
//!    `NotSupported` for `FOR UPDATE` locking; the substrate relies
//!    on drain_lock + tx wrapper + cross-process flock, not row locks).
//! 4. Decode balance via `dqa_to_i64` (returns `Result<i64,
//!    SpendLedgerError>` post-mission 0862-c4).
//! 5. Check `balance >= cost` (else abort with `InsufficientBalance`;
//!    tx drops on early-return, no commit).
//! 6. `Transaction::execute("UPDATE spend_ledger SET balance = ?,
//!    updated_at_unix_ms = ? WHERE holder_did = ? AND macaroon_id =
//!    ?")` with `self.clock.unix_millis() as i64` for the timestamp
//!    (mission 0862-c2).
//! 7. `Transaction::commit()`. Lock released on Drop.
//!
//! Serialization in this process comes from `drain_lock` (a single
//! global `Mutex` per `StoolapSpendLedger` instance, NOT per-key —
//! see TV-0862-15). Cross-process serialization comes from the
//! `fs2` advisory flock on `<dsn-dir>/.spend_ledger.lock` (see
//! ## Cross-process atomicity paragraph below). No double-spend
//! under concurrent drains on the same `(holder_did, macaroon_id)`
//! key.
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
//! ## Cross-process atomicity (mission 0862-c3)
//!
//! The per-instance `drain_lock` covers same-process concurrency
//! (two threads racing SELECT-then-UPDATE on the same
//! `StoolapSpendLedger` instance). Cross-process coordination uses
//! two layers:
//!
//! 1. **Advisory file lock** (AC-1): `open_path_with_clock` acquires
//!    an exclusive `flock(2)` (Linux/Unix) / `LockFileEx` (Windows)
//!    on the DB file via `fs2::FileLock::lock_exclusive`. The lock
//!    is held for the substrate's lifetime; `open_path` from a
//!    different process on the same file surfaces
//!    `SpendLedgerError::LockUnavailable` (fail-closed).
//!
//! 2. **Stoolap transaction** (AC-2): `try_deduct` runs its
//!    SELECT-then-UPDATE window inside a stoolap `Transaction`
//!    (`db.begin()` → `Transaction::query` → `Transaction::execute`
//!    → `Transaction::commit()`). This provides atomicity (the
//!    deduction either commits in full or not at all) and read-your-own-writes
//!    isolation across the SELECT + UPDATE pair.
//!
//! Both layers are needed: the advisory lock provides
//! serialization (mutual exclusion across processes), the
//! transaction provides atomicity (the UPDATE either lands or
//! doesn't). Either alone leaves a gap. Pin via
//! `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
//! TV-0862-11 (file-backed two-instance concurrent-deduct: 10
//! threads × 100 cost on 1000 budget → exactly 10 succeed, 10 fail
//! with `InsufficientBalance`; no over-drain).
//!
//! ## No DID validation (mission 0862-c6)
//!
//! `StoolapSpendLedger` accepts any byte slice as `holder_did` and any
//! 16-byte raw slice as `macaroon_id` — the substrate performs NO
//! `CanonicalCodec` / DID-format / `did:octo:` prefix check. The
//! canonical validation site is the wallet-node boundary in
//! `crates/octo-paid-query/src/handlers/` (per RFC-0862 §Layer
//! discipline + the cross-crate "validation lives at the boundary,
//! not the substrate" convention). A direct DB write (e.g. migration
//! tooling, a future CLI repair command) can therefore insert
//! non-canonical DIDs by design. Pin via
//! `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs`
//! TV-0862-14 (substrate accepts arbitrary byte slice as
//! `holder_did`; no format check; no rejection).
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

use std::os::unix::fs::PermissionsExt;
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
    /// Cross-process advisory file lock not acquired (mission
    /// 0862-c3 AC-1). Returned from `open_path` /
    /// `open_path_with_clock` when another process holds an
    /// exclusive `flock(2)` on the same DB file. Substrate is
    /// fail-closed — concurrent opens from a different process
    /// surface this error rather than silently racing.
    #[error("advisory file lock unavailable for {path}: {reason}")]
    LockUnavailable {
        /// Path of the DB file that could not be locked.
        path: String,
        /// Underlying fs2 / OS error.
        reason: String,
    },
    /// Lock file path is a symlink (mission 0862-c11 AC-1, S6c Round 3
    /// `toctou-symlink-race` HIGH finding). `open_path_with_clock`
    /// rejects any pre-existing symlink at
    /// `<dsn-dir>/.spend_ledger.lock` to prevent the lock being
    /// acquired on an attacker-controlled inode. The substrate is
    /// fail-closed — symlinks surface this error rather than
    /// silently flocking the symlink target.
    #[error("lock path is a symlink: {path}")]
    LockPathSymlink {
        /// Path of the lock file that is a symlink.
        path: String,
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
    db: Arc<octo_storage_core::Database>,
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
    /// Cross-process advisory file lock. Held on the underlying
    /// stoolap DB file (only populated when `open_path` /
    /// `open_path_with_clock` succeeded; `None` for
    /// `open_in_memory*`). The File itself carries the lock state
    /// via `fs2::FileExt::lock_exclusive` — the lock is released
    /// when the File is dropped. Wrapped in `Arc` so the substrate
    /// can still derive `Clone` (File isn't Clone directly). Two
    /// `StoolapSpendLedger` instances on the SAME file from
    /// DIFFERENT processes serialize their SELECT-then-UPDATE
    /// critical section via this lock (per mission 0862-c3,
    /// RFC-0862 §Cross-process atomicity). On Linux/Unix the lock
    /// is `flock(2)` (advisory); on Windows it is `LockFileEx`
    /// (mandatory).
    #[allow(dead_code)] // held for OS-level lock lifetime; released on Drop
    lock_file: Option<Arc<std::fs::File>>,
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
        let db = octo_storage_core::Database::open_in_memory()
            .map_err(|e| SpendLedgerError::Storage(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| SpendLedgerError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self {
            db: Arc::new(db),
            drain_lock: Arc::new(std::sync::Mutex::new(())),
            clock,
            lock_file: None,
        })
    }

    /// Open a file-backed database at `path` with the spend_ledger
    /// schema applied. Production deployments persist balances
    /// across restarts. Defaults to `SystemClock`.
    ///
    /// `path` MUST be a stoolap DSN (e.g. `file:///var/data/ledger.db`
    /// per RFC-0862 §StoolapSpendLedger + `crate::slash_store` precedent).
    /// The bare file path is rejected by stoolap's DSN parser.
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, SpendLedgerError> {
        Self::open_path_with_clock(path, Arc::new(SystemClock))
    }

    /// Open a file-backed database with an injected wall-clock source.
    /// Production deployments that need to pin time (e.g. replay
    /// tools) use this constructor.
    /// # Cross-process atomicity (mission 0862-c3)
    ///
    /// Acquires an advisory file lock (`fs2::FileLock::lock_exclusive`
    /// via `flock(2)` on Linux/Unix, `LockFileEx` on Windows) on the
    /// underlying DB file (the path with `file://` DSN prefix stripped).
    /// The lock is held for the substrate's lifetime and released on
    /// drop. Two `StoolapSpendLedger` instances on the SAME file from
    /// DIFFERENT processes serialize via this lock — the per-instance
    /// `drain_lock` only covers same-process concurrency.
    ///
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on DB open / migration
    /// failure. Returns `SpendLedgerError::LockUnavailable` if the
    /// file lock cannot be acquired (e.g. another process holds an
    /// exclusive lock — fail-closed per mission 0862-c3 AC-1).
    pub fn open_path_with_clock(
        path: &str,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, SpendLedgerError> {
        let db = octo_storage_core::Database::open(path)
            .map_err(|e| SpendLedgerError::Storage(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)
            .map_err(|e| SpendLedgerError::Storage(format!("apply_pending: {e}")))?;
        // Mission 0862-c3 AC-1: acquire advisory file lock on a
        // sibling lock file (the DSN path is a directory for WAL +
        // snapshots per stoolap fork persistence — not a regular
        // file). The lock file lives at `<dsn-dir>/.spend_ledger.lock`
        // and is created on demand; the OS-level `flock(2)` is held
        // for the substrate's lifetime (released on File drop).
        // Wrapped in `Arc` so the struct stays Clone-able (File
        // isn't Clone).
        use fs2::FileExt;
        let fs_path = path.strip_prefix("file://").unwrap_or(path);
        let lock_path = std::path::Path::new(fs_path).join(".spend_ledger.lock");
        // Mission 0862-c11 AC-1: pre-open symlink check. Reject any
        // pre-existing symlink at <dsn-dir>/.spend_ledger.lock to
        // prevent the lock being acquired on an attacker-controlled
        // inode (S6c Round 3 `toctou-symlink-race` HIGH finding).
        // Pre-check narrows the race window to the few microseconds
        // between symlink_metadata() and open(); a strict O_NOFOLLOW
        // fix would require a libc dep (1 line) which we accept the
        // alternative path for now.
        match std::fs::symlink_metadata(&lock_path) {
            Ok(md) if md.file_type().is_symlink() => {
                return Err(SpendLedgerError::LockPathSymlink {
                    path: lock_path.display().to_string(),
                });
            }
            // ENOENT (file doesn't exist) is fine — create(true) handles it.
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(SpendLedgerError::Storage(format!(
                    "lock stat({}): {e}",
                    lock_path.display()
                )));
            }
            _ => {}
        }
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| {
                SpendLedgerError::Storage(format!("lock open({}): {e}", lock_path.display()))
            })?;
        // Mission 0862-c11 AC-2: lock the file to 0600 (owner-only) so a
        // different uid cannot unlink + recreate to defeat serialization
        // (S6c Round 3 `lock-bypass` HIGH finding). Best-effort: chmod
        // failure surfaces as Storage error (e.g. read-only fs).
        std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600)).map_err(
            |e| SpendLedgerError::Storage(format!("lock chmod({}): {e}", lock_path.display())),
        )?;
        lock_file
            .try_lock_exclusive()
            .map_err(|e| SpendLedgerError::LockUnavailable {
                path: path.to_owned(),
                reason: format!("flock: {e}"),
            })?;
        Ok(Self {
            db: Arc::new(db),
            drain_lock: Arc::new(std::sync::Mutex::new(())),
            clock,
            lock_file: Some(Arc::new(lock_file)),
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
        // Mission 0862-c3 AC-2: wrap the SELECT + UPDATE in a stoolap
        // transaction so the read-modify-write pair is atomic (the
        // UPDATE either lands or the SELECT is rolled back). Combined
        // with the cross-process advisory file lock acquired in
        // `open_path_with_clock`, this closes the S6c Round 1 finding
        // #4 cross-process double-spend surface.
        let mut tx = self
            .db
            .begin()
            .map_err(|e| SpendLedgerError::Storage(format!("tx begin: {e}")))?;
        let rows = tx.query(
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
            // No UPDATE issued — just abandon the transaction.
            // `tx` drops without `commit`; stoolap rolls back.
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
        let result = tx.execute(
            "UPDATE spend_ledger SET balance = ?, updated_at_unix_ms = ? \
             WHERE holder_did = ? AND macaroon_id = ?",
            (
                dqa_to_i64(new_balance)?,
                now_ms,
                holder_did.as_bytes().to_vec(),
                macaroon_id.to_vec(),
            ),
        );
        if let Err(e) = result {
            return Err(SpendLedgerError::Storage(format!("update: {e}")));
        }
        // Commit the transaction before releasing `drain_lock`. If
        // commit fails (rare — typically disk full), surface as
        // `Storage` and the UPDATE is rolled back.
        tx.commit()
            .map_err(|e| SpendLedgerError::Storage(format!("tx commit: {e}")))?;
        Ok(new_balance)
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
