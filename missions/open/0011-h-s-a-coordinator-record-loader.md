# 0011-h-s-a-coordinator-record-loader — Substrate additions for CoordinatorRecord::load static method

## Status

Claimed (2026-09-20) — Substrate additions LANDED at `next 10ae8e18`. Substrate-faithful `CoordinatorRecord::load` static method lands in `crates/octo-coordinator-types/src/state.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G12b.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G12b

## Summary

Adds `CoordinatorRecord::load(coordinator_id: &CoordinatorId) -> Option<Self>` per RFC-0011-k §Substrate-Additions Companion Missions row G12b. Pre-requisite for `octo network coordinator show` CLI dispatch.

### Substrate additions target

```rust
// crates/octo-coordinator-types/src/state.rs
impl CoordinatorRecord {
    pub fn load(coordinator_id: &CoordinatorId) -> Option<Self>;
}
```

Substrate additions land 2026-09-20 at `next 10ae8e18`:
- `CoordinatorRecord::load(coordinator_id: &CoordinatorId) -> Option<Self>`
- Substrate-faithful: returns `None` until the persistence adapter lands (Phase 6 follow-on per `0011-h-s-a-coordinator-record-persistence`). The CLI receives `None` and translates to exit 84 `NetworkCoordinatorNotFound`.
- 2 unit tests: `t_load_returns_none_substrate_faithful`, `t_load_idempotent_for_same_id`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-coordinator-types/src/state.rs` per RFC-0011-h §Substrate-Additions row G12b
- [x] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-coordinator-types --lib` green (2/2 load tests pass)
- [x] Layer discipline preserved (Layer A additive surface per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (2 unit tests added; integration test deferred to Phase 6 persistence adapter)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-coordinator` covers that surface — pending Phase 3 IMPLEMENTATION)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Persistence adapter (Phase 6 follow-on)

## Notes

Substrate slice landed 2026-09-20 at `next 10ae8e18`. Companion substrate slice (`0011-k-phase-3-substrate-slice` commit `10ae8e18`) bundles G3b + G12 + G12b together per the substrate-first ordering principle. Phase 3 IMPLEMENTATION closes the CLI dispatch surface (`octo network coordinator show`) after this mission transitions to Completed.
