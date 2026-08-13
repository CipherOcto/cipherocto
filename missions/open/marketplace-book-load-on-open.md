# Mission: marketplace-book-load-on-open

## Status

Open. Follow-on to Round 1 marketplace review (commit `264e2665`).
CRITICAL correctness bug: `Marketplace::open_path()` opens a
stoolap-backed `repo` with N existing Asks, but constructs an empty
in-memory `book`. After restart, every existing Ask is in `repo` but
invisible to `cheapest()` until re-`put()` into the new process.
Silent data loss for routing.

## RFC

RFC-0900 (Economics): Marketplace

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable
- Mission `marketplace-repo-trait-decouple` — repo trait lands first

## Acceptance Criteria

- [ ] `Marketplace::open_path(path)` hydrates `book` from `repo.cheapest(model)` for every known model
- [ ] OR: drop in-memory `book` entirely and scan `repo` directly (≤1k providers per Gap 5 perf note makes O(N) acceptable)
- [ ] Add integration test: populate `repo` with N asks via `put`, drop the process, reopen with `open_path`, `cheapest(model)` returns the saved entry
- [ ] Add restart-equivalence test: `cheapest()` before restart matches `cheapest()` after restart across 3+ scenarios
- [ ] Document the load-on-open contract in `Marketplace::open_path` doc comment
- [ ] Fix production-routing-silent-data-loss vector (the original bug)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new restart tests (≥3)

## Claimant

(unclaimed)

## Pull Request

#

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
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 8 ACs. |

Last Updated: 2026-08-13
Version: 0.1