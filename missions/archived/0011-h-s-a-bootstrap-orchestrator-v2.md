# 0011-h-s-a-bootstrap-orchestrator-v2 — Substrate additions for BootstrapOrchestrator struct (RFC-0011-n Phase 6 G26)

## Status

Completed (2026-09-20) — Substrate slice LANDED at `next cb5e0d3a`. CLI dispatch slice LANDED at `next 2ba4273b`. `BootstrapOrchestrator` struct + `BootstrapState` struct + `BootstrapError` enum + `start_bootstrap` + `status` + `reset` methods + `BootstrapMode::refuses_start` helper landed in `crates/octo-network/src/mon/bootstrap.rs` per RFC-0011-n Phase 6 G26 NEW companion mission. 10 substrate unit tests added (1461 total octo-network tests, was 1451). Phase 6 CLI dispatch slice atop this substrate LANDED at `next 2ba4273b` (402/402 octo-cli tests, was 396 before Phase 6 vectors). Phase 6 IMPLEMENTATION CLOSED for G26 per RFC-0011-n §Implementation Phases Phase 6 closure card.

## RFC

RFC-0011-n §Substrate-Additions Companion Missions row G26 + RFC-0851p-a §1 BootstrapNode Registry + RFC-0011-h §Substrate-Additions row G26

## Summary

Adds `BootstrapOrchestrator` struct + `start_bootstrap(BootstrapConfig)` + `status() -> BootstrapState` methods at `crates/octo-network/src/mon/bootstrap.rs`. Pre-requisite for `octo network bootstrap` + `octo network status` (DEFERRED CLI subcommands from `0011-deprecation-stub-removal` 2026-09-17). Distinct from Phase 2 G1 which adds only the `BootstrapConfig::from_toml` parser + `BootstrapConfig::save_toml` writer (does NOT carry the lifecycle orchestrator struct).

### Substrate additions target

```rust
// crates/octo-network/src/mon/bootstrap.rs (existing module extended)
#[derive(Clone, Debug, Default)]
pub struct BootstrapOrchestrator {
    inner: Option<BootstrapState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BootstrapState {
    pub mode: BootstrapMode,
    pub started_at_epoch: Option<u64>,
    pub refuse_start: bool,
    pub peer_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BootstrapError {
    AlreadyStarted,
    InvalidConfig(String),
    SeedListUnavailable(String),
}

impl BootstrapOrchestrator {
    /// Start the bootstrap lifecycle from a `BootstrapConfig`
    /// (RFC-0011-n Phase 6 G26). Idempotent — returns
    /// `Err(BootstrapError::AlreadyStarted)` if already started.
    /// Refuses start if `BootstrapConfig::mode` indicates the
    /// node refuses to bootstrap (e.g. seed-health refuses_start).
    pub fn start_bootstrap(
        &mut self,
        config: BootstrapConfig,
    ) -> Result<(), BootstrapError>;

    /// Current bootstrap state for `octo network status` (RFC-0011-n
    /// Phase 6 G26). Default state before `start_bootstrap` is called.
    pub fn status(&self) -> BootstrapState;

    /// Reset the orchestrator (substrate-faithful registry surface;
    /// Phase 6 closure path remains `AdapterUnwired` for write paths).
    pub fn reset(&mut self);
}
```

Layer B substrate additions land in `crates/octo-network/src/mon/bootstrap.rs` (existing module extended alongside the existing `BootstrapConfig::from_toml` + `save_toml` methods from Phase 2 G1). No new module: the orchestrator struct shares the same substrate anchors (`BootstrapConfig`, `BootstrapMode`, `SeedListEnvelope`) per RFC-0851p-a.

The `BootstrapState` struct exposes the substrate-faithful observability surface for `octo network status`. The `refuse_start` field is computed by delegating to `SeedHealth::refuses_start()` per the drift-closure mission `0011-h-drift-0851p-a-seed-health-check`.

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/bootstrap.rs` per RFC-0011-n §Substrate-Additions row G26 (next cb5e0d3a)
- [x] `BootstrapOrchestrator` struct lands at the same path
- [x] `BootstrapState` struct lands at the same path
- [x] `BootstrapError` enum lands at the same path
- [x] `start_bootstrap(&mut self, BootstrapConfig) -> Result<(), BootstrapError>` method lands
- [x] `status(&self) -> BootstrapState` method lands
- [x] `reset(&mut self)` registry helper lands
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (1461/1461, +10 above Phase 5 baseline of 1451)
- [x] Layer discipline preserved (Layer B only, zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (10 substrate unit tests pin: start_bootstrap fresh + start_bootstrap idempotent + start_bootstrap error paths + status default-state + status post-start + reset + BootstrapState Serialize/Deserialize round-trip + BootstrapMode coverage for all 3 variants)
- [x] Distinct from Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) which adds only `BootstrapConfig::from_toml` parser + `save_toml` writer; G26 adds the lifecycle orchestrator struct + methods per RFC-0011-n R1 substrate-faithfulness finding

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted
- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.2, DRY CLOSED)
- Soft sequencing: Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) consumes `BootstrapConfig::from_toml` types that G26 uses for `start_bootstrap`
- Soft sequencing: `SeedHealth::refuses_start()` substrate for the drift-closure field

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-bootstrap` + `0011-h-network-status` cover that surface)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Writer election (covered by `0011-h-s-a-writer-election-struct` G18)
- NetworkSender trait (covered by `0011-h-s-a-network-sender` G20)
- Drift-closure field on `BootstrapState` (covered by `0011-h-drift-0851p-a-seed-health-check`)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-n R1 substrate-faithfulness finding — G1 does NOT carry `BootstrapOrchestrator` (Phase 2 G1 is parser/saver only); G26 NEW Phase 6 mission carries the lifecycle orchestrator struct + methods. The orchestrator struct is intentionally minimal (idempotent start + status + reset) to keep the canonical lifecycle logic in one place per [[cipherocto-design-principles]] §Stable Abstractions Principle. Future extensions (e.g. quorum tracking, governance tally aggregation) live in follow-on companion missions per the per-extension crate pattern. The `BootstrapState` struct's `peer_count` field will be populated by the follow-on companion mission that wires the peer-cache substrate into the status envelope.
