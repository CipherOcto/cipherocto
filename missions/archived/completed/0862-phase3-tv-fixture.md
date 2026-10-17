# Mission: 0862-phase3-tv-fixture — Phase 3 Performance Test Vector Fixture (RFC-0862)

## Status

**LANDED 2026-08-19**

## Summary

Author the performance-budget test vector fixture
`tests/fixtures/phase3_tv_0862.json` that gates Phase 3 acceptance
for RFC-0862 v1.3.0. Per R17 M3: separate file from Phase 1's
`phase1_tv_0862.json` (each phase owns its own fixture file). This
mission covers **TV-5 only** (election acquire returns within 3s)
as the smallest viable landing unit. TV-6 (drain throughput 1k/s),
TV-7 (failover pause under 3s), TV-8 (wal fan-out lag under 100ms)
deferred to follow-on missions per RFC-0862 §Test Vectors scope
discipline.

## Acceptance Criteria

- [x] `tests/fixtures/phase3_tv_0862.json` exists at repo root.
- [x] JSON contains entry for `TV-5` (RFC-0862 v1.3.0 Phase 3 TV)
      ONLY — Phase 1 TVs go in `phase1_tv_0862.json`; TV-6/7/8
      deferred to follow-on missions per R17 M3.
- [x] TV-5 entry has structure:
      - `name`: "TV-5"
      - `description`: short prose of the perf invariant under
        test
      - `test_function`: `election_acquire_returns_within_3s`
      - `budget_ms`: 3000 (per RFC-0862 §Performance Targets)
      - `ci_slack_factor`: 10x (account for CI jitter;
        assertion threshold = budget_ms × slack_factor = 30s)
      - `iterations`: 100 (number of trials to average over)
      - `verification_command`: exact `cargo test` invocation
- [x] TV-5 is reproducible (100-iter average well under 30s
      assertion threshold in current substrate).
- [x] Phase 1 substrate (`phase1_tv_0862`) still passes (no
      regression — 256/256 octo-sync integration tests green).

## Substrate landed

- `tests/fixtures/phase3_tv_0862.json` (NEW) — repo-root perf
  fixture. 1 entry: TV-5 election acquire (budget 3000 ms, CI
  slack 10x, iterations 100).
- `octo-sync/tests/phase3_tv_0862.rs` (NEW) — gate test
  (`phase3_tv_0862_election_acquire_within_3s`) loads JSON,
  re-measures via 100 sequential `Cluster::try_acquire_leader`
  calls on unique shard keys (no lease contention), asserts
  under CI slack threshold. Dump test
  (`phase3_tv_0862_dump`) regenerates the JSON when
  `UPDATE_PHASE3_TV=1` is set.
- `octo-sync/tests/phase1_tv_0862.rs` (MOD) — fixed pre-existing
  `FIXTURE_PATH` drift (`../../../tests/fixtures/...` →
  `../../tests/fixtures/...`). The 3-level path resolved past
  repo root into `/home/_w/tests/fixtures/...` and only worked
  because a stray fixture copy lived at that location from a
  prior bootstrap. When `phase3_tv_0862` was added to the same
  `cargo test --tests` binary sweep, the path mismatch surfaced
  as a `phase1_tv_0862_match` failure. Fixed to 2-level path
  (matches reality — `octo-sync/` is 1 level deep, repo root is
  2 levels up).

## Verification (LANDED gate)

- `cargo test -p octo-sync --test phase3_tv_0862` — 2/2 green
  (`phase3_tv_0862_dump` + `phase3_tv_0862_election_acquire_within_3s`).
- `cargo test -p octo-sync --tests` — 256/256 green (229 +
  4 + 4 + 8 + 2 + 2 + 7 across 7 integration test binaries,
  including `phase1_tv_0862` regression coverage).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-sync --all-targets -- -D warnings` clean.

## Key design decisions

- **Perf-budget fixture format (different from byte-exact)**:
  Phase 1 fixtures store `outputs_hex` (byte-exact reproducible).
  Phase 3 fixtures store `budget_ms`, `ci_slack_factor`,
  `iterations` (perf-budget, NOT byte-exact). Gate test
  re-measures fresh and asserts under CI slack — no observed
  value is pinned, so perf noise is expected and absorbed by
  the slack factor.
- **`ci_slack_factor = 10`** (30 s assertion threshold for
  budget 3000 ms): generous enough to absorb CI jitter without
  false-failing, tight enough to catch real perf regressions.
  100 acquires on `Cluster::try_acquire_leader` (which is
  HashMap insert under parking_lot::Mutex) measured sub-second
  in current substrate — slack factor of 10 leaves 29+ seconds
  headroom.
- **Unique shard_key per iter** (no lease contention): each
  acquire uses `ShardKey([i;32])` so all 100 acquires succeed
  without any wait-for-lease path. Measures pure acquire hot
  path, not retry logic.
- **`UPDATE_PHASE3_TV=1` regen pattern** (mirrors Phase 1
  `UPDATE_PHASE1_TV=1` + `goldens.rs` `UPDATE_GOLDENS=1`):
  budget values are constants — no observed data is pinned in
  the fixture, so re-bootstrap only updates the comments /
  verification_command paths if they ever change.
- **Pre-existing `FIXTURE_PATH` drift in phase1** fixed as part
  of this mission (single 1-line change + comment block). The
  drift was a known issue (documented in 0957-phase1 memory
  card as "pre-existing 0862 FIXTURE_PATH mismatch — defer").
  This mission DEFERRED no longer — fixing it here prevents
  ongoing drift across the combined `cargo test --tests`
  binary sweep.

## Cross-references

- RFC-0862 v1.3.0 §Test Vectors (preview) — Phase 3 TV-5..TV-8
- RFC-0862 v1.3.0 §Performance Targets — TV-5 budget 3000 ms
- Mission `0862-phase1-tv-fixture` — sibling Phase 1 fixture
  (LANDED 2026-08-19, same session; FIXTURE_PATH drift fixed
  here)
- Mission `0862-c1-dqa-vault-bump-amendment` — pre-req LANDED
  (RFC-0862 v1.4.0 → v2.0)

## Out of scope (NOT this mission)

- TV-6 (`drain_throughput_1k_per_sec`) — follow-on mission.
- TV-7 (`failover_pause_under_3s`) — follow-on mission.
- TV-8 (`wal_fanout_lag_under_100ms`) — follow-on mission.
- Mission 0111 (DECIMAL/DFP) — off-limits per user constraint.

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-19 | open   | Mission filed. Phase 3 TV-5 only (election acquire ≤ 3s) as smallest viable landing unit. TV-6/7/8 deferred to follow-on missions per RFC-0862 §Test Vectors scope discipline. |
| v1.0    | 2026-08-19 | LANDED | Fixture + gate test + dump test landed. 1 perf-budget TV: election acquire (3000 ms budget × 10x CI slack = 30 s threshold, 100 iterations). `UPDATE_PHASE3_TV=1` regen pattern + hand-rolled JSON serializer/parser. Pre-existing phase1 FIXTURE_PATH drift (`../../../` → `../../`) fixed here to prevent regression in combined test sweep. 256/256 octo-sync integration tests + clippy + fmt green. |
