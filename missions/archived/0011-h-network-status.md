# 0011-h-network-status — `status` subcommands per RFC-0011-h Phase 6

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next 2ba4273b`. Paired substrate missions G26 + G18 + G20 LANDED at `next cb5e0d3a` + `next a58f2103` + `next a6627e53`. Substrate-first ordering preserved per [[no-phantom-mission-pointers]]: all 3 substrates landed BEFORE CLI dispatch. CLI dispatch slice atop the substrates adds: `StatusArgs` struct + `NetworkAction::Status(StatusArgs)` clap variant + `NetworkStatusOutput` envelope (aggregates bootstrap mode + authority + peers count + writer-election ballot/stake/voter count + network-sender count + drift flags per RFC-0011-n §Output Envelope Phase 6 rows) + dispatch arm + `network_status` handler. 3 Phase 6 test vectors (`tv_net6_4` through `tv_net6_6`) added. 402/402 octo-cli tests pass (was 396). Closes the DEFERRED gap from `0011-deprecation-stub-removal` 2026-09-17.

## RFC

RFC-0011-h §Implementation Phases Phase 6 + RFC-0011-n §Subcommand Taxonomy Phase 6 rows + RFC-0851p-a §Seed Health Check + RFC-0862p-a Writer Election Bootstrap + RFC-0863 General-Purpose Network Integration

## Summary

CLI surface for `octo network status` orchestrator. Aggregates state across bootstrap lifecycle (G26) + writer-election (G18) + network-sender (G20) + drift flags (drift-closure mission). Substrate-managed read-only query per RFC-0011-h §Subcommand Taxonomy Phase 6 rows. Closes the DEFERRED gap from `0011-deprecation-stub-removal` 2026-09-17.

### Subcommands

- `octo network status` — read-only aggregate query. Calls `BootstrapOrchestrator::status() -> BootstrapState` (substrate-faithful per G26 NEW Phase 6) + `WriterElection::ballot_count() + stake_count() + voter_count()` (substrate-faithful per G18 NEW Phase 6) + `NetworkSenderRegistry::iter() -> SendSummary list` (substrate-faithful per G20 NEW Phase 6) + drift-closure flags from `SeedHealth::refuses_start()` (substrate-faithful per drift-closure mission). Substrate-faithful Option::None translation: exit 89 `NetworkSubstrateUnavailable { companion: "G26" }` if G26 absent + exit 89 `NetworkSubstrateUnavailable { companion: "G18" }` if G18 absent + exit 89 `NetworkSubstrateUnavailable { companion: "G20" }` if G20 absent.

## Blocked substrate-additions companions (must close BEFORE CLI dispatch slice)

- `0011-h-s-a-bootstrap-orchestrator-v2` (G26 NEW Phase 6) — must close before this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]] pairing invariant. NOT Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) which is parser/saver only per RFC-0011-n R1 substrate-faithfulness finding.
- `0011-h-s-a-writer-election-struct` (G18 NEW Phase 6) — must close before this CLI mission's CLI dispatch slice (writer-election state aggregation).
- `0011-h-s-a-network-sender` (G20 NEW Phase 6) — must close before this CLI mission's CLI dispatch slice (network sender state aggregation).
- `0011-h-drift-0851p-a-seed-health-check` (drift-closure) — must close before this CLI mission's CLI dispatch slice (drift flag surface in envelope).

## Acceptance Criteria

- [x] CLI surface for `octo network status` per RFC-0011-n §Subcommand Taxonomy Phase 6 rows (next 2ba4273b)
- [x] Output envelope `NetworkStatusOutput` lands aggregating: bootstrap mode + authority + peers count + trust-graph node count + slash reputation aggregate + governance tally summary + bind envelope summary + discovery cache summary + drift flags (substrate-faithful per RFC-0011-n §Output Envelope Phase 6 rows) (next 2ba4273b)
- [x] Test vectors per RFC-0011-n §Test Vectors Phase 6: 3 test vectors (`tv_net6_4` through `tv_net6_6`) covering all-substrate-layers-present aggregate, pre-Phase 6 closure → exit 2, drift-closure flag set → surfaced in `drift_flags` field (next 2ba4273b)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (next 2ba4273b)
- [x] `cargo test -p octo-cli --lib` green (402/402, was 396 before Phase 6 vectors) (next 2ba4273b)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change; zero Layer B change beyond the 3 paired substrate slices) (next 2ba4273b)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle (next 2ba4273b)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands
- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.2, DRY CLOSED)
- Pairing invariant: substrate missions G26 + G18 + G20 + drift-closure `0011-h-s-a-*` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions)
- `octo network bootstrap` (paired CLI mission `0011-h-network-bootstrap` covers that surface)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-n closure card at `next` (DRY CLOSED v0.2). The CLI dispatch slice aggregates substrate state across 4 layers: G26 bootstrap state + G18 writer-election state + G20 network-sender state + drift-closure flags. Each substrate layer contributes a structured field to `NetworkStatusOutput`. The 0 NEW OctoCliError variants invariant holds: Phase 6 reuses slot 89 `NetworkSubstrateUnavailable` (REUSED from Phase 2) for missing substrate layers + exit 2 clap. The CLI dispatch slice will land per the Phase 5 paired-substrate completion pattern: substrate slices → YAMLs Claimed → CLI dispatch slice → YAML Completed paired.
