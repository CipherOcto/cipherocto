# 0011-h-drift-0851p-a-seed-health-check — Drift closure for seed-health check divergence between RFC-0011-h + RFC-0851p-a

## Status

Open (2026-09-20) — Drift-closure mission identified during RFC-0011-h 6-phase rollout (Phase 6 closure artifacts slice per RFC-0011-n §Substrate-Additions Companion Missions drift-closure row)

## RFC

RFC-0011-n §Substrate-Additions Companion Missions drift-closure row (Phase 6 closure artifacts slice)

## Summary

Drift identified during RFC-0011-h 6-phase rollout between `SeedHealth::check` at `crates/octo-network/src/mon/bootstrap.rs:94` (RFC-0851p-a substrate) and the `octo network status` aggregate envelope (RFC-0011-h §Output Envelope). Drift manifests as: seed-health check reports `refuses_start()` correctly per RFC-0851p-a, but the `octo network status` envelope does NOT surface the `refuses_start` boolean. Operators relying on the CLI status output for seed-list health monitoring cannot see the refusal signal.

### Drift scope

- `crates/octo-network/src/mon/bootstrap.rs` L89-L150 — `SeedHealth` enum + `check` method + `refuses_start` method (RFC-0851p-a substrate, PRESENT)
- `crates/octo-cli/src/commands/network.rs` (CLI dispatch, NOT YET LANDED for `octo network status` per RFC-0011-n Phase 6 closure)
- `rfcs/accepted/networking/0851p-a-network-bootstrap.md` — RFC anchor for seed-health substrate

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [ ] `octo network status` envelope surfaces `seed_health_refuses_start: bool` field derived from `SeedHealth::refuses_start()` per RFC-0851p-a §Seed Health Check
- [ ] Drift closure documented in RFC-0011-n §Substrate-Additions drift-closure row
- [ ] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-cli --lib` green
- [ ] Layer discipline preserved (Layer C CLI dispatch only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] Substrate-faithful boundary: typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle
- [ ] Drift closure cross-reference added to RFC-0011-h §Subcommand Taxonomy row for `status`

## Dependencies

- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.1.5 at `next a3e714f4`; pending DRY CLOSURE gate)
- Soft sequencing: G26 (`0011-h-s-a-bootstrap-orchestrator-v2`) lands before drift-closure can wire the envelope field

## Out of Scope

- SeedHealth substrate additions (covered by RFC-0851p-a, substrate PRESENT)
- New OctoCliError variants (covered by RFC-0011-h §Error Handling row 89 + slot arithmetic)
- Writer election drift (covered by `0011-h-s-a-writer-election-struct` G18)
- NetworkSender drift (covered by `0011-h-s-a-network-sender` G20)

## Notes

Drift-closure mission stub filed 2026-09-20 during RFC-0011-n Phase 6 closure artifacts review. Drift identified via cross-RFC substrate-faithfulness verification between RFC-0011-h §Output Envelope and RFC-0851p-a §Seed Health Check substrate. Full AC + scope land when work enters Phase X.
