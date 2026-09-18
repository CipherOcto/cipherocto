# 0011-h-network-governance — `governance` subcommands per RFC-0011-h Phase 3

## Status

Open (2026-09-18) — CLI mission per RFC-0011-h §Implementation Phases Phase 3

## RFC

RFC-0011-h §Implementation Phases Phase 3

## Summary

CLI surface for governance tally + rotation status. canonical_bytes_hash REDACTED pending G3b. Per RFC-0011-h §Implementation Phases Phase 3.

### Subcommands

- `governance tally`
- `governance rotation status`

## Blocked substrate-additions companions

- `0011-h-s-a-voting-tally-canonical-bytes` — must close before this CLI mission opens

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
