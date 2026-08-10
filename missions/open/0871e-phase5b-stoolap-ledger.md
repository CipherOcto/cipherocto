# 0871e-phase5b-stoolap-ledger — Stoolap-backed SpendLedger impl

**Status:** unassigned (wave 3a step 2; gap surfaced 2026-08-10)
**Substrate:** RFC-0862 atomic transaction + stoolap-fork substrate
**Parent:** 0871e-phase5b (landed `ebdbf4cd`) per [[mission-0871e-phase5b-status]]

## Scope

`SpendLedger` trait landed in `ebdbf4cd`; only `InMemorySpendLedger` impl exists. Production needs persistent ledger backed by stoolap-fork per [[stoolap-general-purpose-db]]. Migration target: cipherocto-side schema (NOT stoolap fork hosting cipherocto business schema per [[stoolap-general-purpose-db]] red line).

1. `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (NEW) — `StoolapSpendLedger` impl. Schema:
   - Table `spend_ledger` with columns `(holder_did BLOB, macaroon_id BLOB, balance INTEGER, updated_at_unix_ms INTEGER, PK(holder_did, macaroon_id))`.
   - `open_in_memory() -> Result<Self, RegistryError>` for tests (parallel to `StoolapHolderRegistry::open_in_memory`).
   - `seed` → `INSERT OR REPLACE INTO spend_ledger (holder_did, macaroon_id, balance, updated_at_unix_ms) VALUES (?, ?, ?, ?)`
   - `try_deduct` → stoolap transaction: `SELECT balance ... FOR UPDATE` → check ≥ cost → `UPDATE balance = balance - cost` → commit. Returns new balance.
   - `balance` → `SELECT balance FROM spend_ledger WHERE holder_did = ? AND macaroon_id = ?`
2. `crates/quota-router-storage/src/lib.rs` — re-export `StoolapSpendLedger`.
3. `crates/octo-paid-query/src/lib.rs` — `SpendLedger` trait re-export unchanged; impl lives in storage crate (Layer B-adjacent).

## Test vector discipline

- 5 new TV in `stoolap_spend_ledger.rs`:
  - `seed_and_deduct_round_trip` — basic flow
  - `deduct_unknown_holder_errors` — UNIQUE constraint miss → `UnknownHolder`
  - `deduct_insufficient_balance_carries_amounts` — check fails → error carries both numbers
  - `deduct_is_atomic_under_concurrent_load` — spawn N concurrent drains summing to > budget, assert at most floor(budget/cost) succeed
  - `seed_replaces_existing_balance` — `seed` is upsert (INSERT OR REPLACE)
- 1 new integration TV in `crates/octo-paid-query/tests/stoolap_ledger_e2e.rs` — open in-memory, full lifecycle (seed → multi-deduct → exhaust → fail-closed).

## Depends on

- 0871e-phase5b landed (`ebdbf4cd`) — `SpendLedger` trait + `InMemorySpendLedger`
- stoolap-fork substrate ([[stoolap-general-purpose-db]])

## Blocks

- Production paid-query end-to-end (current impl is in-memory only; restart loses all balances)
- Cross-node drain visibility (each wallet-node today has its own in-memory ledger; no shared state)

## Layer direction

- `octo-paid-query` (Layer E) owns `SpendLedger` trait
- `quota-router-storage` (Layer B-adjacent) owns the stoolap-backed impl
- Wallet-node (Layer C) holds `Arc<dyn SpendLedger>` slot — already wired in `ebdbf4cd`

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy -p quota-router-storage -p octo-paid-query --all-targets -- -D warnings`
- `cargo test --lib -p quota-router-storage`
- `cargo test --lib -p octo-paid-query`
- `cargo test -p octo-paid-query --test stoolap_ledger_e2e`

## Cross-references

- [[mission-0871e-phase5b-status]] — trait substrate (this mission adds the persistent impl)
- [[wave-3-gaps-2026-08-10]] — gap surface context
- [[stoolap-general-purpose-db]] — schema placement rule (cipherocto-side, NOT stoolap fork)
- [[cipherocto-design-principles]] — Layer B-adjacent storage layout
