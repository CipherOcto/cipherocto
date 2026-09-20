# 0011-h-s-a-bootstrap-orchestrator-v2 — Substrate additions for BootstrapOrchestrator struct (RFC-0011-n Phase 6)

## Status

Open (2026-09-20) — Substrate-additions prerequisite per RFC-0011-n §Substrate-Additions Companion Missions row G26 (NEW Phase 6, distinct from Phase 2 G1 which is parser/saver only)

## RFC

RFC-0011-n §Substrate-Additions Companion Missions row G26

## Summary

Adds `BootstrapOrchestrator` struct + `start_bootstrap(BootstrapConfig)` + `status() -> BootstrapState` methods at `crates/octo-network/src/mon/bootstrap.rs`. Pre-requisite for `octo network bootstrap` + `octo network status` (DEFERRED CLI subcommands from `0011-deprecation-stub-removal` 2026-09-17).

### Substrate additions target

```rust
// crates/octo-network/src/mon/bootstrap.rs
struct BootstrapOrchestrator;
impl BootstrapOrchestrator {
    pub fn start_bootstrap(&self, config: BootstrapConfig) -> Result<(), BootstrapError>;
    pub fn status(&self) -> BootstrapState;
}
```

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [ ] Substrate additions land in `crates/octo-network/src/mon/bootstrap.rs` per RFC-0011-n §Substrate-Additions row G26
- [ ] `BootstrapOrchestrator` struct lands with `start_bootstrap(BootstrapConfig)` + `status() -> BootstrapState` methods
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test
- [ ] Distinct from Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) which adds only `BootstrapConfig::from_toml` parser + `save_toml` writer; G26 adds the lifecycle orchestrator struct + methods per RFC-0011-n R1 substrate-faithfulness finding

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted
- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.1.5 at `next a3e714f4`; pending DRY CLOSURE gate)
- Soft sequencing: Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) consumes `BootstrapConfig` types that G26 uses

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-bootstrap` + `0011-h-network-status` cover that surface)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Writer election (covered by `0011-h-s-a-writer-election-struct` G18)
- NetworkSender trait (covered by `0011-h-s-a-network-sender` G20)

## Notes

Stub filed 2026-09-20 per RFC-0011-n R1 substrate-faithfulness finding — G1 does NOT carry `BootstrapOrchestrator` (Phase 2 G1 is parser/saver only); G26 NEW Phase 6 mission carries the lifecycle orchestrator struct + methods. Full AC + scope land when work enters Phase X.
