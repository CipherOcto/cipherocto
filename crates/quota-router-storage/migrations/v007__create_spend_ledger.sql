-- Mission 0871e-phase5b-stoolap-ledger: persistent SpendLedger
-- substrate (RFC-0862 atomic transaction).
--
-- Schema: per-(holder_did, macaroon_id) prepaid balance keyed
-- for atomic drain via stoolap transaction. The wallet mints with
-- a PaymentCaveat that includes the budget; the mint handler seeds
-- the (holder_did, macaroon_id) -> budget entry on mint; subsequent
-- WALLET_PAID_QUERY_VERIFY calls deduct query_cost atomically.
--
-- Concurrency: per-instance `drain_lock` (Mutex<()>) wrapping a
-- stoolap `Transaction` (`db.begin()` -> `Transaction::query` ->
-- `Transaction::execute` -> `Transaction::commit()`). The drain path
-- does NOT use `SELECT ... FOR UPDATE` — the stoolap fork's storage
-- layer returns `NotSupported` for `FOR UPDATE` locking (see
-- `storage/traits/table.rs`); pre-c3 doc claimed `FOR UPDATE` row
-- locking which never shipped. Actual serialization: drain_lock
-- (intra-process) + cross-process `fs2` advisory flock on
-- `<dsn-dir>/.spend_ledger.lock` (mission 0862-c3, 0862-c8).
-- Concurrent drains on the same (holder_did, macaroon_id) serialize
-- so a double-spend is impossible (atomic per-key guarantee).
-- Corrected in mission 0862-c10 (S6c Round 3 doc-drift consolidation).
--
-- Cipherocto-side migration per [[stoolap-general-purpose-db]]:
-- schema lives in cipherocto crate, NOT in the stoolap fork.

CREATE TABLE IF NOT EXISTS spend_ledger (
    holder_did BLOB NOT NULL,
    macaroon_id BLOB NOT NULL,
    balance INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (holder_did, macaroon_id)
);

CREATE INDEX IF NOT EXISTS spend_ledger_updated_at_idx
    ON spend_ledger (updated_at_unix_ms);
