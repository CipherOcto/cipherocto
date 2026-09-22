# 0011-h-network-slash-stats — `slash` subcommands per RFC-0011-h Phase 2

## Status

Completed (2026-09-20) — CLI dispatch LANDED at `next 8649ca4d`. RFC-0011-j Phase 2 `slash` subcommand surface wired to substrate.

## RFC

RFC-0011-h §Implementation Phases Phase 2 + RFC-0011-j §Subcommand Taxonomy

## Summary

CLI surface for `octo network slash {excluded,stats,list,show}` per RFC-0011-j Phase 2.

### Subcommands

- `slash excluded <did>` — reads `SlashReputationStoreCompat::is_excluded`
- `slash stats` — reads `did_count` + `total_slashes` aggregate counters
- `slash list [--slash-reason <u16>] [--limit <n>]` — reads `list` with `SlashListFilter`
- `slash show <slash_id>` — reads `show` for envelope detail

### Implementation

CLI dispatch landed 2026-09-20 at `next 8649ca4d`:

- `SlashExcludedArgs` + `SlashStatsArgs` + `SlashListArgs` + `SlashShowArgs` clap structs (Layer C)
- 4 handlers in `crates/octo-cli/src/commands/network.rs` (`slash_excluded`, `slash_stats`, `slash_list`, `slash_show`)
- `NetworkSlashExcludedOutput` + `NetworkSlashStatsOutput` + `NetworkSlashListOutput` + `NetworkSlashShowOutput` envelope payloads
- 7 test vectors: `tv_net2_8`, `tv_net2_9`, `tv_net2_10`, `tv_net2_11`, `tv_net2_12`, `tv_net2_13`, `tv_net2_16`

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (7 vectors added)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (360/360)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted
- Hard sequencing: substrate companion `0011-h-s-a-slash-store` (G6) must be LANDED — landed at `next 931dc7b1`

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions — G6 + G6b landed)
- Slash persistence adapter (Phase 6 follow-on, deferred)

## Notes

Phase 2 IMPLEMENTATION closed at `next 8649ca4d`. Slash observability surface exercises the G6 substrate-faithful reads for the slash envelope log. The G6b `SlashStoreLoader` hydration facade is not exercised by Phase 2 CLI (CLI uses per-process `SlashReputationStoreCompat::new()` substrate-faithfully); G6b hydration lands with the persistence adapter in Phase 6.
