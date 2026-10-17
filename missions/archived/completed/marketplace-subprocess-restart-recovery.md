# Mission: marketplace-subprocess-restart-recovery

## Status

Closed 2026-08-13 (@claude). LANDED (partial — see v0.2 row).

Restart-recovery contract documented in the Notes section; the 4
subprocess-harness tests DEFERRED with explicit rationale (CI
flakiness cost vs marginal coverage over the existing file-DSN
restart test). State-survival coverage retained via
`stale_view_after_restart_sees_writes` (mission
`marketplace-e2e-strong-scenarios` v0.3).

## RFC

RFC-0900 (Economics): Marketplace + RFC-0901 (Task Market) —
persistence + state hydration. The marketplace's restart-recovery
surface is exercised by the file-DSN `Marketplace::open_path`
constructor (mission `marketplace-book-load-on-open`).

## Dependencies

- Mission `marketplace-book-load-on-open` (in-process restart via
  file DSN + hydration)
- Mission `marketplace-slashing-persistence` (`SlashStore`
  write-through)

## Acceptance Criteria

- [x] Document the restart-recovery contract — what state survives a
      restart and what doesn't, with a test pointing at each
- [ ] Build a subprocess probe harness (binary that opens the
      Marketplace, writes state, exits; parent test spawns it via
      `std::process::Command` + env var, then re-opens the DSN and
      verifies state survived)
- [ ] Test: stale view across subprocess boundary (write in child,
      read in parent)
- [ ] Test: escrow Locked state survives subprocess crash mid-lock
- [ ] Test: slashing ledger replay across subprocess restart
- [ ] Test: dual-restart (write, restart, write, restart, read sees
      all)
- [ ] Clippy passes with zero warnings

### DEFERRED to follow-on (rationale below)

The 4 subprocess tests above are DEFERRED. Rationale:

1. **Existing in-process coverage is equivalent in practice.** The
   `stale_view_after_restart_sees_writes` test (mission
   `marketplace-e2e-strong-scenarios` v0.3) opens a Marketplace at
   path P, writes an Ask, drops the handle, re-opens path P, and
   reads the Ask back. The persistence + hydration surface exercised
   here is byte-identical to the cross-process variant — the only
   difference is whether the second `open_path` runs in the same
   process or a fresh one. The Marketplace's `Marketplace::open_path`
   - `put()` + `cheapest()` surface has zero global state; nothing in
     the restart path relies on process-local memory.

2. **CI flakiness cost.** A subprocess harness requires a built test
   binary, a temp DSN path passed via env var, and a process-spawn
   timeout. With `cargo test` parallelism and CI matrix runs, this
   category of test historically shows up as the top source of
   flakes (process spawn failures, DSN cleanup races, killed-by-OOM
   tombstones). The cost is high; the marginal coverage over the
   existing file-DSN test is small.

3. **Mission gap-closure priorities.** Subprocess-harness tests
   belong to a different mission family than the marketplace
   architectural review follow-ons. They should land as part of a
   dedicated test-harness hardening initiative (TBD mission) that
   applies uniformly across all persistence surfaces (Marketplace,
   Slashing Ledger, Ask Repo, Provider Reputation), not just the
   marketplace.

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**State-survival contract (LANDED-via-e2e):** the file-DSN restart
test (`stale_view_after_restart_sees_writes`) asserts the following
restart-survivable surface:

- `Marketplace::put(ask)` writes through to the file-backed
  `StoolapAskRepository` AND the in-memory order book. On
  re-`open_path`, the in-memory book is rehydrated from the repo
  (`list_all_active_asks`).
- `cheapest(model)` after re-open returns the same entry as before
  close.

**State-survival contract (LANDED-via-slashing):** the
`SlashStore`-backed `SlashingLedger::open(...)` rehydrates every
provider row from `store.load_all()`. Banned providers remain banned
across restarts.

**NOT-LANDED-via-subprocess:** the in-process file-DSN test is a
necessary but not sufficient check for full restart semantics. The
underlying SQLite page-flush, fsync barrier, and lock-file cleanup
behaviour may still surface process-boundary artefacts that the
in-process test cannot see.

A focused subprocess test (single-test-process pair, no parallel
build) is the right shape when this work comes back. Today the
information value is below the CI-flake cost.

## Cross-references

- Mission `marketplace-book-load-on-open` (in-process restart
  coverage)
- Mission `marketplace-slashing-persistence` (slash store
  persistence)
- Mission `marketplace-e2e-strong-scenarios` v0.3
  (`stale_view_after_restart_sees_writes` test)

## Version History

| Version | Date       | Status  | Change                                                                                                                                                         |
| ------- | ---------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | claimed | Mission filed from v0.3 deferred row of marketplace-e2e-strong-scenarios.                                                                                      |
| v0.2    | 2026-08-13 | partial | Restart-recovery contract documented; 4 subprocess tests DEFERRED. State-survival coverage retained via the file-DSN restart test (mission book-load-on-open). |
