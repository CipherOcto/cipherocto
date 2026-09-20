# 0011-h-network-discovery — `discovery` subcommands per RFC-0011-h Phase 5

## Status

Open (2026-09-20) — CLI mission per RFC-0011-h §Implementation Phases Phase 5 + RFC-0011-m §Subcommand Taxonomy Phase 5. CLI dispatch slice pending per substrate-first ordering (paired G23 + G24 substrate missions YAML filled in; substrate slices + YAML Claimed transitions + CLI dispatch slice + paired YAML Completed transition pending per directive sequencing — substrate coding is LAST).

## RFC

RFC-0011-h §Implementation Phases Phase 5 + RFC-0011-m Phase 5 §Subcommand Taxonomy + RFC-0855 §8.2 Mission Advertisement + Mission Invitation

## Summary

CLI surface for discovery cache reads. Substrate-faithful boundary per [[cipherocto-design-principles]] §Stable Abstractions Principle.

### Subcommands

- `discovery advertisement show [--advertisement-id <HEX>] [--hops <U16>]` — read-only. Calls `MissionAdvertisementCache::get(advertisement_id)` (substrate-faithful per G23) for targeted lookup; calls `MissionAdvertisementCache::iter()` (substrate-faithful per G23) for full enumeration. `--hops <U16>` optional arg surfaces `is_ttl_exceeded(hop_count)` per RFC-0855 §8.2. Substrate-faithful Option::None translation: exit 89 `NetworkSubstrateUnavailable { companion: "G23" }`
- `discovery invitation show [--invitation-id <HEX>]` — read-only. Calls `MissionInvitationCache::get(invitation_id)` (substrate-faithful per G24) for targeted lookup; calls `MissionInvitationCache::iter()` (substrate-faithful per G24) for full enumeration. Substrate-faithful Option::None translation: exit 89 `NetworkSubstrateUnavailable { companion: "G24" }`

Both subcommands carry no confirmation flags (read-only per RFC-0011-h row 86 + row 664 ALLOW-in-CI). The `--hops 65536` overflow rejected pre-dispatch via clap u16 parse error (exit 2) per `tv_net5_3`.

## Blocked substrate-additions companions

- `0011-h-s-a-discovery-advertisement-cache` (G23) — YAML filled in 2026-09-20; substrate slice + YAML Claimed + CLI dispatch slice + YAML Completed transition pending per directive sequencing
- `0011-h-s-a-discovery-invitation-cache` (G24) — YAML filled in 2026-09-20; substrate slice + YAML Claimed + CLI dispatch slice + YAML Completed transition pending per directive sequencing

## Acceptance Criteria

- [ ] CLI surface for the listed subcommands per RFC-0011-l Phase 5 §Subcommand Taxonomy rows 327-329 (next PENDING CLI dispatch slice)
- [ ] Test vectors per RFC-0011-m Phase 5 §Test Vectors for the listed subcommands (6 test vectors tv_net5_1 through tv_net5_6 added; next PENDING CLI dispatch slice)
- [ ] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-cli --lib` green (≥6 tests added above Phase 4 baseline of 390)
- [ ] Layer discipline preserved (CLI Layer C only; zero Layer A change; zero Layer B change beyond the 2 paired substrate slices)
- [ ] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.
- Pairing invariant: substrate missions G23 + G24 `0011-h-s-a-*` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-discovery-advertisement-cache` + `0011-h-s-a-discovery-invitation-cache` companion missions)
- MissionAdvertisementCache persistence adapter (Phase 6 follow-on per `0011-h-s-a-discovery-advertisement-persistence`)
- MissionInvitationCache persistence adapter (Phase 6 follow-on per `0011-h-s-a-discovery-invitation-persistence`)
- Signature verification (substrate-side; CLI side hex-encodes signing bytes per RFC-0011-m §Security Considerations)

## Notes

CLI dispatch slice pending 2026-09-20 per RFC-0011-m closure card at `next` (DRY CLOSED v0.2). The substrate-faithful Option::None translation (discovery advertisement show → exit 89 NetworkSubstrateUnavailable G23) + Option::None translation (discovery invitation show → exit 89 NetworkSubstrateUnavailable G24) surface as typed OctoCliError variants that operator switch tables can grep on exit code 89 (REUSED from Phase 2 per RFC-0011-h §Error Handling row 526). Phase 6 adapter follow-on will populate the success-envelope branches with persisted cache reads. The `--hops <U16>` clap arg overflow protection (`--hops 65536` → exit 2) surfaces in `tv_net5_3` test vector.
