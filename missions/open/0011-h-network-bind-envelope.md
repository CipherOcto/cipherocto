# 0011-h-network-bind-envelope — `bind-envelope` subcommands per RFC-0011-h Phase 4

## Status

Open (2026-09-18) — CLI mission per RFC-0011-h §Implementation Phases Phase 4

## RFC

RFC-0011-h §Implementation Phases Phase 4

## Summary

CLI surface for bind-envelope read + payload builders. BLOCKED pending G22 (lookup) + G21 (key rotation for rebind-prepare/commit). Per RFC-0011-h §Implementation Phases Phase 4.

### Subcommands

- `bind-envelope show`
- `bind-envelope rebind-prepare`
- `bind-envelope rebind-commit`
- `bind-envelope rebind-abort`

## Blocked substrate-additions companions

- `0011-h-s-a-bind-envelope-lookup` — must close before this CLI mission opens
- `0011-h-s-a-attached-handle-key-rotation` — must close before this CLI mission opens

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
