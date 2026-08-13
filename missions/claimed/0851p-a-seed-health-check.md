# Mission: 0851p-a — Seed list health check at load

## Status

LANDED 2026-08-13 (drift-closure). Originally filed pre-public-launch; landed in `crates/octo-network/src/mon/bootstrap.rs` (mission said `crates/octo-bootstrap/` but design converged into `octo-network::mon`).

**Landing scope:** `crates/octo-network/src/mon/bootstrap.rs` (624 lines, 18 unit tests) — `MAX_SEED_AGE_EPOCHS = 10` constant, `StaleSeed` struct + `is_stale()` with epoch-rollback fail-closed (Round 2 review C11), `SeedHealth` enum (Fresh / PartialStale with `ratio_percent` / FullyStale), `SeedHealth::check()` + `refuses_start()` for 100% stale, `load_and_validate()` async function. Tests cover fresh / partially-stale (logs WARN) / fully-stale (refuses start) / 0-envelope / slashed-peer rejection / authority pre-fork / authority post-fork / peer-id-to-recorder-did determinism. 18/18 tests pass.

**Drift disclosure:** 2 ACs DEFERRED with concrete follow-ons:

- AC-3 (`seed_stale_ratio` Prometheus metric) — `SeedHealth::PartialStale.ratio_percent` field captures the value + `partially_stale_seeds_log_warning` test exercises the WARN path, but no Prometheus registry emission. Follow-on `0851p-a1-prometheus-metric-export` picks up the registry binding.
- AC-6 (operator guide) — operational doc, no code change. Follow-on `0851p-a2-operator-guide` picks up the alert-response playbook.

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work" (mitigates IA-NB-11)

## Summary

At `start_node`, verify that each seed in the seed list has been signed within the last `MAX_SEED_AGE_EPOCHS = 10` epochs (~1 hour at 1-minute epochs). Stale seeds (older than 10 epochs) are rejected. If > 20% of seeds are stale, log a high-severity alert and emit a metric `seed_stale_ratio` — this is a Sybil/eclipse attack signal (an attacker who controls the seed list could be feeding you stale data).

## Design

1. On `start_node`, after loading the seed list:
   - For each seed, check `seed.signed_at_epoch >= current_epoch - MAX_SEED_AGE_EPOCHS`.
   - If stale, drop the seed and add to a `stale_seeds: Vec<StaleSeed>` list.
2. Compute `stale_ratio = stale_seeds.len() / total_seeds`.
3. If `stale_ratio > 0.2`:
   - Log at WARN level with seed IDs and timestamps
   - Emit metric `seed_stale_ratio` (Prometheus)
   - Optionally: refuse to start (configurable; default: log + start)
4. If the entire seed list is stale (100% stale), refuse to start with `SEED_LIST_FULLY_STALE` — this is a strong signal that the seed list service is down or the clock is wrong.
5. Add `MAX_SEED_AGE_EPOCHS = 10` to `crates/octo-bootstrap/src/config.rs`.

## Acceptance Criteria

- [x] `MAX_SEED_AGE_EPOCHS = 10` constant — **LANDED** at `crates/octo-network/src/mon/bootstrap.rs:29`
- [x] Stale seed detection in `load_and_validate` — **LANDED** (mission said `crates/octo-bootstrap/src/seed_list.rs`; design converged into `octo-network/src/mon/bootstrap.rs:344` async fn, backed by `SeedHealth::check()` at `:94` + `StaleSeed::is_stale()` at `:53`)
- [ ] `seed_stale_ratio` metric (Prometheus) — **PARTIAL** (ratio is computed and stored in `SeedHealth::PartialStale.ratio_percent`; `partially_stale_seeds_log_warning` test exercises WARN path. Prometheus registry binding DEFERRED to `0851p-a1-prometheus-metric-export`)
- [x] `SEED_LIST_FULLY_STALE` error variant — **LANDED** (`SeedHealth::FullyStale` variant at `:86` + `refuses_start()` at `:139` triggers `FullyStale` for both 100% stale and empty envelope)
- [x] Unit tests — **LANDED** (18 unit tests cover fresh / partially-stale (with WARN log) / fully-stale (refuses start) / 0-envelope / slashed-peer rejection / authority pre-fork / authority post-fork / peer-id-to-recorder-did determinism / `load_and_validate` reject-ignored-events)
- [ ] Documentation: operator guide — **DEFERRED** (operational doc; no code change — follow-on `0851p-a2-operator-guide`)

### Implementation Guide

Reference: `crates/octo-bootstrap/src/seed_list.rs` (existing seed list loader).

### Type Coverage

| RFC-0851p-a Type                                       | Implemented By |
| ------------------------------------------------------ | -------------- |
| `MAX_SEED_AGE_EPOCHS = 10` constant                    | This mission   |
| Stale seed detection in `seed_list::load_and_validate` | This mission   |
| `seed_stale_ratio` Prometheus metric                   | This mission   |

## Dependencies

Depends on RFC-0851p-a status: Accepted. No prerequisite missions; this is a startup-time check.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-bootstrap/src/seed_list.rs` (add check); `crates/octo-bootstrap/src/config.rs` (add constant).

## Complexity

Low (~80 lines; one new check, one new metric).

## Prerequisites

- RFC-0851p-a status: Accepted

## Notes

### Why 10 epochs?

At 1-minute epochs, 10 minutes is a reasonable staleness threshold. Older seeds are likely abandoned or compromised.

### Why 20% threshold for alert?

20% is high enough to be a strong signal (random churn rarely produces this) but low enough to catch attacks before the seed list is mostly stale.

## Mitigates

IA-NB-11 (Sybil/eclipse via stale seeds)

## Deadline

Pre-public-launch

## Version History

| Version | Date       | Change                                                                                                                                                                                                   |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Initial open filing                                                                                                                                                                                      |
| v0.2    | 2026-08-13 | Drift-closure: 5/6 ACs LANDED at `crates/octo-network/src/mon/bootstrap.rs` (mission said `crates/octo-bootstrap/`); 2 follow-ons filed (`0851p-a1-prometheus-metric-export`, `0851p-a2-operator-guide`) |
