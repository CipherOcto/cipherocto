# 0011-h-drift-0851p-a-seed-health-check — Drift closure for seed-health check divergence between RFC-0011-h + RFC-0851p-a

## Status

Completed (2026-09-20) — Drift-closure mission CLOSED. G26 substrate slice LANDED at `next cb5e0d3a` with `BootstrapState::refuse_start: bool` field derived from `SeedHealth::refuses_start()` per RFC-0851p-a §Seed Health Check. CLI dispatch slice LANDED at `next 2ba4273b` with `NetworkStatusOutput::seed_health_refuses_start: bool` field. Drift closure cross-reference added to RFC-0011-h §Subcommand Taxonomy row for `status` (next 2ba4273b). 402/402 octo-cli tests pass (was 396). Phase 6 IMPLEMENTATION CLOSED for drift-closure row per RFC-0011-n §Implementation Phases Phase 6 closure card.

## RFC

RFC-0011-n §Substrate-Additions Companion Missions drift-closure row (Phase 6 closure artifacts slice) + RFC-0851p-a §Seed Health Check + RFC-0011-h §Output Envelope

## Summary

Drift identified during RFC-0011-h 6-phase rollout between `SeedHealth::check` at `crates/octo-network/src/mon/bootstrap.rs:94` (RFC-0851p-a substrate) and the `octo network status` aggregate envelope (RFC-0011-h §Output Envelope). Drift manifests as: seed-health check reports `refuses_start()` correctly per RFC-0851p-a, but the `octo network status` envelope does NOT surface the `refuses_start` boolean. Operators relying on the CLI status output for seed-list health monitoring cannot see the refusal signal.

### Drift scope

- `crates/octo-network/src/mon/bootstrap.rs` L89-L150 — `SeedHealth` enum + `check` method + `refuses_start` method (RFC-0851p-a substrate, PRESENT)
- `crates/octo-cli/src/commands/network.rs` (CLI dispatch, NOT YET LANDED for `octo network status` per RFC-0011-n Phase 6 closure)
- `rfcs/accepted/networking/0851p-a-network-bootstrap.md` — RFC anchor for seed-health substrate

### Drift closure target

```rust
// crates/octo-network/src/mon/bootstrap.rs (existing module, in G26 substrate slice)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BootstrapState {
    pub mode: BootstrapMode,
    pub started_at_epoch: Option<u64>,
    pub refuse_start: bool,    // <-- DRIFT CLOSURE: surfaced from SeedHealth::refuses_start()
    pub peer_count: usize,
}
```

The `refuse_start: bool` field in `BootstrapState` is computed by delegating to `SeedHealth::refuses_start()` per RFC-0851p-a §Seed Health Check. The CLI's `NetworkStatusOutput` envelope surfaces this as `seed_health_refuses_start: bool` for operator-side observability.

Layer C CLI dispatch translation (lands in `octo network status` CLI dispatch slice):

```rust
// crates/octo-cli/src/commands/network.rs (CLI dispatch slice)
#[derive(Clone, Debug, Serialize)]
pub struct NetworkStatusOutput {
    // ... existing fields ...
    pub seed_health_refuses_start: bool,  // <-- DRIFT CLOSURE
}
```

## Acceptance Criteria

- [x] `BootstrapState::refuse_start: bool` field lands in G26 substrate slice per RFC-0851p-a §Seed Health Check (next cb5e0d3a)
- [x] `octo network status` envelope surfaces `seed_health_refuses_start: bool` field derived from `SeedHealth::refuses_start()` per RFC-0851p-a §Seed Health Check (next 2ba4273b)
- [x] Drift closure documented in RFC-0011-n §Substrate-Additions drift-closure row (next 2ba4273b)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (next 2ba4273b)
- [x] `cargo test -p octo-cli --lib` green (402/402, was 396 before Phase 6 vectors) (next 2ba4273b)
- [x] Layer discipline preserved (Layer C CLI dispatch only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle) (next 2ba4273b)
- [x] Substrate-faithful boundary: typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle (next 2ba4273b)
- [x] Drift closure cross-reference added to RFC-0011-h §Subcommand Taxonomy row for `status` (next 2ba4273b)

## Dependencies

- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.2, DRY CLOSED)
- Soft sequencing: G26 (`0011-h-s-a-bootstrap-orchestrator-v2`) lands before drift-closure can wire the envelope field
- Soft sequencing: `SeedHealth::refuses_start()` substrate at `octo-network/src/mon/bootstrap.rs:139` (LANDED per RFC-0851p-a)

## Out of Scope

- SeedHealth substrate additions (covered by RFC-0851p-a, substrate PRESENT)
- New OctoCliError variants (covered by RFC-0011-h §Error Handling row 89 + slot arithmetic; 0 NEW in Phase 6)
- Writer election drift (covered by `0011-h-s-a-writer-election-struct` G18)
- NetworkSender drift (covered by `0011-h-s-a-network-sender` G20)

## Notes

Drift-closure mission stub filed 2026-09-20 during RFC-0011-n Phase 6 closure artifacts review. Drift identified via cross-RFC substrate-faithfulness verification between RFC-0011-h §Output Envelope and RFC-0851p-a §Seed Health Check substrate. The drift is small in code surface (one bool field + one envelope field) but operationally important: operators who relied on `octo network status` for seed-list health monitoring would have missed the refusal signal pre-closure. Closure lands during the G26 substrate slice (the `BootstrapState` struct gain) + the CLI dispatch slice (the `NetworkStatusOutput` envelope gain).
