# 0011-h-network-bootstrap — `bootstrap` subcommands per RFC-0011-h Phase 6

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next 2ba4273b`. Paired substrate mission G26 LANDED at `next cb5e0d3a`. Substrate-first ordering preserved per [[no-phantom-mission-pointers]]: G26 substrate (BootstrapOrchestrator struct + BootstrapState + BootstrapError + start_bootstrap/status/reset methods) landed BEFORE CLI dispatch. CLI dispatch slice atop the substrate adds: `BootstrapArgs` struct + `NetworkAction::Bootstrap(BootstrapArgs)` clap variant + `NetworkBootstrapOutput` envelope + dispatch arm + `network_bootstrap` handler + `bootstrap_config_companion` reader. 3 Phase 6 test vectors (`tv_net6_1` through `tv_net6_3`) added to `crates/octo-cli/src/commands/network.rs`. 402/402 octo-cli tests pass (was 396). Closes the DEFERRED gap from `0011-deprecation-stub-removal` 2026-09-17.

## RFC

RFC-0011-h §Implementation Phases Phase 6 + RFC-0011-n §Subcommand Taxonomy Phase 6 rows + RFC-0851p-a §1 BootstrapNode Registry

## Summary

CLI surface for `octo network bootstrap` orchestrator. Closes the DEFERRED gap from `0011-deprecation-stub-removal` 2026-09-17 (which removed `octo init` / `octo join` / `octo status` with user-accepted deferral of replacements). Substrate-managed lifecycle per RFC-0011-h §Subcommand Taxonomy Phase 6 rows.

### Subcommands

- `octo network bootstrap` — bootstrap lifecycle start. Calls `BootstrapOrchestrator::start_bootstrap(BootstrapConfig)` (substrate-faithful per G26 NEW Phase 6). No CLI-side confirmation flags (substrate-managed per RFC-0011-h row 97). Substrate-faithful error translation: `BootstrapError::AlreadyStarted` → exit 89 `NetworkSubstrateUnavailable { companion: "G26" }` (slot 89 REUSED from Phase 2 per RFC-0011-h §Error Handling row 89 + slot arithmetic). `BootstrapError::InvalidConfig(String)` → exit 2 (clap parse error pattern). `BootstrapError::SeedListUnavailable(String)` → exit 89 `NetworkSubstrateUnavailable { companion: "G26" }`.

The `BootstrapConfig` is loaded from `<octo_home>/network/bootstrap.toml` via `BootstrapConfig::from_toml(path)` (substrate-faithful per Phase 2 G1 LANDED at `next c9121aa5`). Substrate-faithful Option::None translation: `BootstrapConfig::from_toml` returns `Err(BootstrapConfigError::Io)` when `<octo_home>/network/bootstrap.toml` missing → exit 89 `NetworkSubstrateUnavailable { companion: "G1" }` (operator-side hint: run `octo network mode set` first).

## Blocked substrate-additions companions (must close BEFORE CLI dispatch slice)

- `0011-h-s-a-bootstrap-orchestrator-v2` (G26 NEW Phase 6) — must close before this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]] pairing invariant. NOT Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) which is parser/saver only per RFC-0011-n R1 substrate-faithfulness finding.

## Acceptance Criteria

- [x] CLI surface for `octo network bootstrap` per RFC-0011-n §Subcommand Taxonomy Phase 6 rows (next 2ba4273b)
- [x] Output envelope `NetworkBootstrapOutput` lands with `mode: BootstrapMode`, `started_at_epoch: Option<u64>`, `refuse_start: bool`, `peer_count: usize` (substrate-faithful translation of `BootstrapOrchestrator::start_bootstrap` return value + `BootstrapState` projection) (next 2ba4273b)
- [x] Test vectors per RFC-0011-n §Test Vectors Phase 6: 3 test vectors (`tv_net6_1` through `tv_net6_3`) covering success, pre-G26 closure → exit 2, substrate error (next 2ba4273b)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (next 2ba4273b)
- [x] `cargo test -p octo-cli --lib` green (402/402, was 396 before Phase 6 vectors) (next 2ba4273b)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change; zero Layer B change beyond the G26 substrate slice) (next 2ba4273b)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle (next 2ba4273b)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands
- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.2, DRY CLOSED)
- Pairing invariant: substrate mission G26 `0011-h-s-a-bootstrap-orchestrator-v2` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-bootstrap-orchestrator-v2` G26 companion mission)
- Per-extension transport impl (covered by `0011-h-s-a-network-sender` G20)
- Writer election (covered by `0011-h-s-a-writer-election-struct` G18)
- Drift-closure field on status envelope (covered by `0011-h-drift-0851p-a-seed-health-check`)
- `octo network status` (paired CLI mission `0011-h-network-status` covers that surface)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-n closure card at `next` (DRY CLOSED v0.2). Substrate-faithful to the canonical lifecycle management pattern: CLI does NOT carry confirmation flags; substrate `BootstrapOrchestrator` handles the state machine per RFC-0011-h row 97 (no CLI-side confirmation; substrate-managed). The 0 NEW OctoCliError variants invariant holds: Phase 6 reuses slot 89 `NetworkSubstrateUnavailable` (REUSED from Phase 2) + exit 2 clap. The CLI dispatch slice will land per the Phase 5 paired-substrate completion pattern: substrate slice → YAML Claimed → CLI dispatch slice → YAML Completed paired.
