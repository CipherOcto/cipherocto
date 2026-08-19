# Mission: 0862-c13 — Phase 3 Performance Test Vector Fixture, TV-8 (RFC-0862)

## Status

**open** (2026-08-19)

## Summary

Author the performance-budget test vector fixture for **TV-8
only** (`wal_fanout_lag_under_100ms`) per RFC-0862 v1.3.0
§Performance Targets. Sibling to `0862-c12` (TV-5 LANDED 2026-08-19).
Separate fixture file per R17 M3 scope discipline — each TV owns
its own fixture file. TV-6/7 deferred to follow-on missions.

## Why TV-8 next (smallest viable landing unit)

- TV-8 substrate is sync (`Cluster::append_wal_entry` + `Cluster::read_wal_range`).
  No async runtime / tokio harness needed — same pattern as TV-5.
- Budget 100ms p99 over N iterations — measurable under CI slack factor 10x.
- WalEntry v1.3 (`build_v13`) available — no schema work.
- No WAL fan-out type in substrate — fan-out IS shared `Arc<Cluster>`
  state propagation. Reader via `read_wal_range` sees appended entry
  immediately (deterministic — no replication layer in test harness).

## Acceptance Criteria

- [ ] `tests/fixtures/phase3_tv_0862_tv8.json` exists at repo root.
- [ ] JSON contains entry for `TV-8` ONLY — TV-5 stays in
      `phase3_tv_0862.json` (LANDED 2026-08-19). TV-6/7 deferred.
- [ ] TV-8 entry has structure:
      - `name`: "TV-8"
      - `description`: short prose of the perf invariant
      - `test_function`: `wal_fanout_lag_under_100ms`
      - `budget_ms`: 100 (per RFC-0862 §Performance Targets)
      - `ci_slack_factor`: 10x (CI jitter; threshold = 1000ms)
      - `iterations`: 100 (number of trials to measure p99 over)
      - `verification_command`: exact `cargo test` invocation
- [ ] TV-8 reproducible (100-iter p99 well under 1000ms assertion
      threshold in current substrate).
- [ ] TV-5 (LANDED) still passes — no regression.
- [ ] All octo-sync integration tests green (256+ incl. TV-5).

## Substrate to land

- `tests/fixtures/phase3_tv_0862_tv8.json` (NEW) — repo-root
  perf fixture. 1 entry: TV-8 WAL fan-out lag.
- `octo-sync/tests/phase3_tv_0862_tv8.rs` (NEW) — gate test
  + dump test, mirrors `phase3_tv_0862.rs` pattern but separate
  file (per R17 M3).
- `octo-sync/tests/phase3_tv_0862.rs` — UNTOUCHED.

## Test shape (preview)

`fn tv8_wal_fanout_lag_under_100ms(iterations: u32) -> Vec<u8>`:
- `let cluster = Cluster::new();`
- For i in 0..iterations:
  - Build `WalEntry::build_v13(ENTRY_TYPE_DRAIN, ShardKey([i;32]), vec![i;64])`
  - t0 = Instant::now()
  - lsn = cluster.append_wal_entry(entry).unwrap()
  - entries = cluster.read_wal_range(lsn, Some(lsn+1))
  - elapsed_us = t0.elapsed().as_micros() as u64
  - assert entries[0].lsn == lsn (fan-out delivered)
  - per_iter_us.push(elapsed_us)
- Return: total_ms (u64 LE) || per_iter_us (u64 LE each)

## Verification (LANDED gate)

- `cargo test -p octo-sync --test phase3_tv_0862_tv8` — 2/2 green.
- `cargo test -p octo-sync --tests` — 258+/258+ green
  (incl. TV-5 regression coverage).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-sync --all-targets -- -D warnings` clean.

## Key design decisions

- **Per-TV file (R17 M3 discipline)** — separate fixture + gate
  test file per TV, not cumulative. Matches LANDED c12 pattern.
  Enables parallel landing without cross-file merge conflict.
- **Sync `append_wal_entry` + `read_wal_range`** — substrate
  fan-out via shared `Arc<Cluster>` state (no separate
  replication layer in test harness). TV-8 measures
  `append→read` lag within same Cluster.
- **`ci_slack_factor = 10`** (1000 ms threshold for 100 ms budget):
  100ms budget is tight; 10x slack absorbs CI jitter without
  false-failing. Actual fan-out is sub-microsecond in current
  substrate (mutex lock + HashMap insert).
- **`UPDATE_PHASE3_TV=1` regen pattern** mirrors c12 + Phase 1 +
  `goldens.rs` `UPDATE_GOLDENS=1`. Budget values are constants.

## Cross-references

- RFC-0862 v1.3.0 §Test Vectors (preview) — Phase 3 TV-8
- RFC-0862 v1.3.0 §Performance Targets — TV-8 budget 100 ms p99
- Mission `0862-c12` (LANDED 2026-08-19) — sibling TV-5 fixture
  + gate pattern. Reuse `Phase3Fixture` shape + `CARGO_MANIFEST_DIR`
  fixture path pattern.

## Out of scope (NOT this mission)

- TV-6 (`drain_throughput_1k_per_sec`) — follow-on mission.
  Requires async harness + `RaftLikeDrainCoordinator::submit_drain`.
- TV-7 (`failover_pause_under_3s`) — follow-on mission.
  Requires `set_lease_duration_ms(0)` + `kill` + re-acquire
  pattern across nodes.
- Mission 0111 (DECIMAL/DFP) — off-limits per user constraint.

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-19 | open   | Mission filed. Phase 3 TV-8 only (WAL fan-out ≤ 100ms p99) as smallest viable landing unit. TV-6/7 deferred. |