# Mission: marketplace-book-load-on-open

## Status

Closed 2026-08-13 (@claude). LANDED.

**H3 from Round 1 marketplace review closed.** `Marketplace::open_path`
now hydrates the in-memory order book from `repo.list_all_active_asks()`,
fixing the silent-data-loss vector where a process restart would
leave the book empty even though `repo` still held every published Ask.

## RFC

RFC-0900 (Economics): Marketplace

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [x] `Marketplace::open_path(path)` hydrates `book` from
      `repo.list_all_active_asks(now)` and `place_ask` per row
- [x] `list_all_active_asks(now_unix: u64)` added to `AskRepository`
      (`crates/quota-router-storage/src/ask_repo.rs`)
- [x] Hydration skips expired Asks (`expires_at_unix <= now`)
- [x] Cost computation reuses the same `build_unit_consumed` +
      `settlement_cost` path as `put()` (no duplicated logic)
- [x] Integration test: populate `repo` with 3 Asks, drop, reopen,
      `cheapest(model)` returns the saved entries
      (`open_path_hydrates_book_from_repo`)
- [x] Restart-equivalence test: same set of `put`s produce the
      same `cheapest()` before/after a fresh `open_path` across
      4 Asks / 2 models (`open_path_cheapest_matches_in_memory`)
- [x] Expired-skip test: an Ask with `expires_at < now` does not
      appear in the hydrated book
      (`open_path_hydration_skips_expired_asks`)
- [x] Load-on-open contract documented in `Marketplace::open_path`
      doc comment
- [x] All 76 marketplace lib tests pass (`cargo test --features full --lib marketplace`)
- [x] Clippy passes with zero warnings

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

Key files:
- `crates/quota-router-core/src/marketplace/mod.rs:120, 144, 156` — open_path + book field
- `crates/quota-router-storage/src/ask_repo.rs` — must expose list_all() or per-model iteration if hydration path chosen

Round 1 review context (Pass 2 HIGH #H3): two parallel state stores,
one source of truth promised. The `book` cache drifts from `repo` on
restart because `put` is the only writer to `book`. Severity HIGH
because it's a hot-path correctness bug masquerading as architecture.

Choose one:
- (a) Hydrate `book` from `repo` on open_path — preserves O(1) cheapest
- (b) Drop `book` and scan `repo` — simpler but O(N) per query

Recommend (a) unless Gap 5 perf data shows O(N) is fine.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. H3 silent-data-loss vector scoped. |
| v0.2    | 2026-08-13 | Closed. H3 fixed: open_path hydrates book via `list_all_active_asks`. 3 new tests; 76/76 marketplace lib tests pass. |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 8 ACs. |

Last Updated: 2026-08-13
Version: 0.1