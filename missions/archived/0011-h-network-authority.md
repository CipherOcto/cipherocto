# 0011-h-network-authority — `authority` subcommands per RFC-0011-h Phase 2

## Status

Completed (2026-09-20) — CLI dispatch LANDED at `next 8649ca4d`. RFC-0011-j Phase 2 `authority` subcommand surface wired to substrate.

## RFC

RFC-0011-h §Implementation Phases Phase 2 + RFC-0011-j §Subcommand Taxonomy

## Summary

CLI surface for `octo network authority show` + `octo network authority rotate` per RFC-0011-j Phase 2.

### Subcommands

- `authority show` — projects current `SeedListAuthority` + deprecation state via `verify_authority`
- `authority rotate --new-authority <foundation|dao> --quorum-proof-hex <64 lowercase hex>` — invokes `SeedListAuthority::rotate_post_fork` after 3-flag confirmation; Foundation rejected post-fork; zero-digest rejected

### Implementation

CLI dispatch landed 2026-09-20 at `next 8649ca4d`:

- `AuthorityShowArgs` + `AuthorityRotateArgs` clap structs (Layer C)
- `authority_show` + `authority_rotate` handlers in `crates/octo-cli/src/commands/network.rs`
- `NetworkAuthorityShowOutput` + `NetworkAuthorityRotateOutput` envelope payloads
- `NetworkSubstrateUnavailable` (slot 89) error mapping for `SeedAuthorityError` with companion tag "G8"
- 6 test vectors: `tv_net2_4`, `tv_net2_5`, `tv_net2_6`, `tv_net2_7`, `tv_net2_15`, `clap_parses_all_phase_2_subcommands`

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (6 vectors added)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (360/360)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted
- Hard sequencing: substrate companion `0011-h-s-a-seed-list-authority-rotate` (G8) must be LANDED — landed at `next 931dc7b1`

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions — G8 landed)
- Authority re-vote persistence (deferred to Phase 3 / RFC-0011-k)

## Notes

Phase 2 IMPLEMENTATION closed at `next 8649ca4d`. Authority rotation exercises the G8 substrate-faithful rejection paths for Foundation + zero-digest, mapping both to exit 89 `NetworkSubstrateUnavailable` companion "G8". Phase 3 will expand this surface with `authority re-vote` + `authority quorum` operations per RFC-0011-k.
