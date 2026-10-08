# Mission: marketplace-slashing-persistence

## Status

Closed. LANDED 2026-08-13.

## RFC

RFC-0900 (Economics): Marketplace §Slashing Model

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [x] Define `trait SlashStore` in `quota-router-storage`: `load_all()`, `upsert_stake(row: &SlashLedgerRow)`, `append_outcome(provider_id, reason, amount, new_cumulative_loss_pct_micro)` (default no-op)
- [x] Implement `StoolapSlashStore` in `quota-router-storage` backed by `slash_ledger` table (migration v012)
- [x] `SlashingLedger::open(store: Arc<dyn SlashStore>, rules)` constructor hydrates from `store.load_all()`
- [x] `register`, `slash`, `slash_with_pct` write through to `store.upsert_stake`
- [x] Restart-equivalence test: register + slash alice, drop ledger, open new ledger against same store, alice's cumulative_loss_pct + offense_count + ban status preserved (`ban_persists_across_restart`)
- [x] Fix the misleading module doc (`in-memory; production backed by stoolap`)
- [x] Clippy passes with zero warnings
- [x] All existing tests pass + new persistence tests (≥3) — 3 storage + 5 core persistence tests land

## Claimant

mmacedoeu (2026-08-13)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-storage/src/slash_store.rs` (NEW) — trait + StoolapSplit
- `crates/quota-router-storage/migrations/v012__create_slash_ledger.sql` (NEW) — schema
- `crates/quota-router-core/src/marketplace/slashing.rs` — `open()` constructor + write-through + Debug impl
- `crates/quota-router-storage/src/migrations.rs` — v012 entry
- `crates/quota-router-storage/src/lib.rs` — `pub mod slash_store`

Design notes:
- `sum_micro` encoding for `cumulative_loss_pct` (micro-percent, 1e6) keeps column Eq-comparable without f64 round-trip ambiguity.
- Stoolap fork UPSERT limitations: SELECT-then-INSERT-or-UPDATE inside transaction with explicit `row_id INTEGER PRIMARY KEY` (no `ON CONFLICT` / `ON DUPLICATE KEY UPDATE` / `VALUES()`).
- `Arc<dyn SlashStore>` doesn't implement Debug — manual Debug impl on `SlashingLedger` formats store as `<dyn SlashStore>`.
- `append_outcome` defaulted to no-op so extensions may override for audit (semantics preserved; marketplace drives audit via `upsert_stake` snapshot).

Tests added:
- `slash_store::tests::load_all_empty_on_fresh_db`
- `slash_store::tests::upsert_then_load_round_trips_row`
- `slash_store::tests::upsert_overwrites_existing_provider`
- `slashing::tests::open_hydrates_from_store`
- `slashing::tests::register_writes_through_to_store`
- `slashing::tests::slash_writes_through_to_store`
- `slashing::tests::slash_with_pct_writes_through_to_store`
- `slashing::tests::ban_persists_across_restart`

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 9 ACs. |
| v0.2    | 2026-08-13 | Mission CLOSED. AC-1..9 closed. 3 storage + 5 core persistence tests land. SlashStore trait + StoolapSlashStore + write-through + restart test. |

Last Updated: 2026-08-13
Version: 0.2
