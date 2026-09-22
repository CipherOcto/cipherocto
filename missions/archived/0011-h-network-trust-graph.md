# 0011-h-network-trust-graph — `trust-graph` subcommands per RFC-0011-h Phase 1

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next c2fee8f3` (RFC-0011-i Phase 1 IMPLEMENTATION 3-commit chain). 1 subcommand wired: `trust-graph render --depth 1-100 --format ascii|dot`. Substrate-faithful per RFC-0011-i §Substrate Mapping Table: `TrustGraph::render(format: GraphFormat)` returns `String`. Layer B substrate already present at `crates/octo-network/src/mon/trust_graph.rs:89` per RFC-0011-h closure verification — no companion substrate needed (per [[substrate-faithfulness-verification]] R7.5 lesson). 339/339 octo-cli tests (was 323, +13 Phase 1 vectors across peers/identity/trust-graph/governance). Layer discipline preserved (zero Layer A change).

## RFC

RFC-0011-h §Implementation Phases Phase 1 + RFC-0011-i §Subcommand Taxonomy Phase 1 rows + RFC-0011-i §Substrate Mapping Table

## Summary

Substrate-faithful CLI surface for `octo network trust-graph render --depth 1-100 --format ascii|dot`. Per RFC-0011-h §Implementation Phases Phase 1.

### Subcommands

- `trust-graph render` (CLOSE-OUT per `next c2fee8f3`)

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy (next c2fee8f3)
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (next c2fee8f3)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (next c2fee8f3)
- [x] `cargo test -p octo-cli --lib` green (next c2fee8f3)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change) (next c2fee8f3)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle (next c2fee8f3)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands — LANDED (RFC-0011-h Accepted at `next 8696ec4b`)
- Hard sequencing: RFC-0011-i must be Accepted before this mission lands — paired companion mission YAML closed paired with Phase 1 IMPLEMENTATION `next c2fee8f3`

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions; `TrustGraph::render` already present at `crates/octo-network/src/mon/trust_graph.rs:89` — no companion substrate needed per [[substrate-faithfulness-verification]] R7.5 lesson)

## Notes

Stub filed 2026-09-18 per [[no-phantom-mission-pointers]]. Paired closure transition paired with RFC-0011-i Phase 1 IMPLEMENTATION COMPLETE closure card. Substrate-faithfulness verified: `TrustGraph::render(GraphFormat)` returns `String` per RFC-0011-h closure audit at commit `next 638c8ac7` (PRE-PHASE-1 IMPL).
