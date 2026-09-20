# 0011-h-network-status — `status` subcommands per RFC-0011-h Phase 6

## Status

Open (2026-09-18) — CLI mission per RFC-0011-h §Implementation Phases Phase 6

## RFC

RFC-0011-h §Implementation Phases Phase 6

## Summary

CLI surface for `octo network status` orchestrator. DEFERRED + BLOCKED pending G26 + G18 + G20. Per RFC-0011-h §Implementation Phases Phase 6 + RFC-0011-n §Substrate-Additions row G26 + G18 + G20 (NEW Phase 6 substrate additions; G26 distinct from Phase 2 G1 which is parser/saver only — R1 substrate-faithfulness finding).

### Subcommands

- `status`

## Blocked substrate-additions companions

- `0011-h-s-a-bootstrap-orchestrator-v2` (G26 NEW Phase 6) — must close before this CLI mission opens. NOT Phase 2 G1 (`0011-h-s-a-bootstrap-orchestrator`) which is parser/saver only per RFC-0011-n R1 substrate-faithfulness finding.
- `0011-h-s-a-writer-election-struct` (G18 NEW Phase 6) — must close before this CLI mission opens (writer-election state aggregation).
- `0011-h-s-a-network-sender` (G20 NEW Phase 6) — must close before this CLI mission opens (network sender state aggregation).

## Acceptance Criteria

- [ ] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy
- [ ] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands
- [ ] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-cli --lib` green
- [ ] Layer discipline preserved (CLI Layer C only; zero Layer A change)
- [ ] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions)

## Notes

Stub filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land when work enters Phase X.
