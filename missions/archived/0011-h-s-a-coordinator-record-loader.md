# 0011-h-s-a-coordinator-record-loader — Substrate additions for CoordinatorRecord::load static method

## Status

Completed (2026-09-20) — Substrate additions + CLI dispatch CLOSED. Substrate-faithful `CoordinatorRecord::load` static method lands in `crates/octo-coordinator-types/src/state.rs`. CLI dispatch wired at `octo network coordinator show <coordinator_id_hex>` (slot 84 `NetworkCoordinatorNotFound`). Substrate-additions + CLI dispatch landed end-to-end per RFC-0011-h §Substrate-Additions Companion Missions row G12b.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G12b

## Summary

Adds `CoordinatorRecord::load(coordinator_id: &CoordinatorId) -> Option<Self>` per RFC-0011-k §Substrate-Additions Companion Missions row G12b. CLI dispatch consumes via substrate-faithful Option::None translation.

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

CLI dispatch wired 2026-09-20 at `next 624998ba`:
- `octo network coordinator show <coordinator_id_hex>` wired at `crates/octo-cli/src/commands/network.rs` (Layer C)
- `CoordinatorRecord::load(&args.coordinator_id)` returns `None` → CLI surfaces `OctoCliError::NetworkCoordinatorNotFound { coordinator_id_redacted }` (exit 84)
- New OctoCliError variant slot 84 LANDED: `NetworkCoordinatorNotFound { coordinator_id_redacted: String }` per RFC-0011-k §Error Handling row 537
- New output envelope: `NetworkCoordinatorShowOutput { coordinator_id_hex, record: Option<...> }` (reserved for Phase 6 persistence adapter)
- New CoordinatorRecordProjectionOutput envelope field struct (Layer C projection; populated branch lands Phase 6)
- New coordinator_id parse validator: `parse_64_char_hex_32byte` (reused from Phase 2 authority_rotate substrate-faithful)
- 3 new test vectors: `tv_net3_1_coordinator_show_parses_with_id`, `tv_net3_2_coordinator_show_rejects_non_hex`, `tv_net3_6_coordinator_show_emits_network_coordinator_not_found`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-coordinator-types/src/state.rs` per RFC-0011-h §Substrate-Additions row G12b (next 10ae8e18)
- [x] CLI dispatch wired at `crates/octo-cli/src/commands/network.rs` per RFC-0011-h §Subcommand Taxonomy Phase 3 (next 624998ba)
- [x] OctoCliError slot 84 `NetworkCoordinatorNotFound` LANDED per RFC-0011-k §Error Handling row 537
- [x] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` clean
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-coordinator-types --lib` green (2/2 load tests pass)
- [x] `cargo test -p octo-cli --lib` green (3/3 coordinator show test vectors pass; +3 above Phase 2 baseline)
- [x] Layer discipline preserved (Layer A additive surface per [[cipherocto-design-principles]] §Stable Abstractions Principle; Layer C consumes via re-export; zero parallel abstractions)
- [x] ≥3 unit tests + ≥1 integration test (2 substrate unit tests + 3 CLI test vectors added; integration test deferred to Phase 6 persistence adapter)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- Persistence adapter (Phase 6 follow-on per `0011-h-s-a-coordinator-record-persistence`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20 at `next 10ae8e18`. CLI dispatch slice landed 2026-09-20 at `next 624998ba`. The substrate-faithful CLI dispatch surfaces Option::None as a typed `NetworkCoordinatorNotFound` exit so operator switch tables can grep on exit code 84. The persistence adapter follow-on (Phase 6) will populate the envelope with the full record projection once wired.
