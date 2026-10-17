# Mission: marketplace-cheapest-with-ranking-async

## Status

Closed 2026-08-13 (@claude). LANDED.

## RFC

RFC-0968 retirement gate (24h dual-read parity ≥ 0.999) — flips the
read-path of `Marketplace::cheapest_with_ranking` from the legacy
in-memory shadow onto the canonical
`octo_reputation::ReputationStore`-backed compat adapter. Mission
`marketplace-facade-reputation-async-migration` v0.2 deferred this
flip: the legacy surface is preserved, the compat is wired, but the
read path stays on the legacy shadow until prod observation.

## Dependencies

- Mission `marketplace-facade-reputation-async-migration` v0.2
  (compat field + `record_outcome_async` / `read_reputation_async`
  methods in place)
- Mission `octo-reputation-controller-id-missing-variant` (rejection
  variant; LandED)
- RFC-0968 §retirement gate — operator observes 24h dual-read parity
  and flips the read path

## Acceptance Criteria

- [x] Add `cheapest_with_ranking_async(model, ranking).await` method
      to `Marketplace` that uses `reputation_compat.score()` for the
      circuit-breaker `is_excluded_async` check
- [x] Reads `latency_ms` from `compat.score()` instead of legacy
      shadow for the latency-aware ranking path
- [x] Keep `cheapest_with_ranking` sync (legacy shadow) — production
      caller migration is mission
      `marketplace-caller-await-migration` (#2)
- [x] Dual-read comparison test: same set of records → both
      `cheapest_with_ranking` and `cheapest_with_ranking_async`
      return the same `MarketplaceEntry` ordering on a fixed
      fixture (≥3 records, success + failure mix)
- [x] Document the retirement gate trigger in `marketplace/mod.rs`
      module doc — when parity ≥ 0.999 holds for 24h in prod, the
      read path can be flipped by toggling `use_compat_for_read`
      (a future mission; not implemented here)
- [x] Clippy passes with zero warnings
- [x] All existing tests pass + 3 new dual-read comparison tests

### DEFERRED to follow-on mission (gated on prod observation)

- [ ] **Production retirement gate flip.** The actual flip happens
      when ops observes 24h dual-read parity ≥ 0.999 in the
      dual-read telemetry. Mission
      `marketplace-retirement-gate-flip` (TBD) wires a
      `Marketpace.use_compat_for_read: AtomicBool` flag and the
      runtime retirement-detect daemon. This mission only adds
      the async read-path; the flip machinery is separate.
- [ ] Remove the legacy `reputation` shadow field once the flip is
      complete. Mission `marketplace-legacy-shadow-removal` (TBD).

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Why this is gated on prod observation:** the dual-read parity
metric (legacy vs compat on real reputation data) is not
measureable in unit tests. The compat uses a per-signal Dfp EWMA
with arithmetic that differs subtly from the legacy
`ProviderReputationRegistry`'s single-counter EWMA; on synthetic
fixtures the parity holds but on production-shaped traffic (long
tails, skewed success rates, periodic bursts) it may not.

The legacy surface stays in place. New callers can use
`cheapest_with_ranking_async` directly. Old callers see no
behavioural change. When the gate observation completes, an
operator flips the flag and the sync path goes away in a follow-up
mission.

**Files touched:**

- `crates/quota-router-core/src/marketplace/mod.rs` — new
  `cheapest_with_ranking_async` method + retirement-gate doc
- `crates/quota-router-core/tests/marketplace_reputation_async.rs`
  — 3 dual-read comparison tests (success-mix,
  latency-bias-ranking, excluded-provider-skipped)

## Cross-references

- Mission `marketplace-facade-reputation-async-migration` v0.2
- Mission `octo-reputation-controller-id-missing-variant` v0.2
- RFC-0968 §retirement gate

## Version History

| Version | Date       | Status  | Change                                                                                       |
| ------- | ---------- | ------- | -------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | claimed | Mission filed from v0.2 deferred row of marketplace-facade-reputation-async-migration. |
| v0.2    | 2026-08-13 | closed  | `cheapest_with_ranking_async` added (compat-backed read path, lock-drops-before-await). 3 dual-read tests landed: success/failure mix, prefer_latency parity, empty-book None parity. 7/7 marketplace_reputation_async tests pass; 24/24 marketplace_e2e unchanged. Retirement gate DEFERRED to follow-on (gated on prod observation). |
