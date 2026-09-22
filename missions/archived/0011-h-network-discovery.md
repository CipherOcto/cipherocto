# 0011-h-network-discovery — `discovery` subcommands per RFC-0011-h Phase 5

## Status

Completed (2026-09-20) — CLI dispatch slice CLOSED. `octo network discovery advertisement show` + `octo network discovery invitation show` wired at `next 346f10cc` per RFC-0011-m Phase 5 §Subcommand Taxonomy. Both subcommand arms + 2 output envelopes + 6 test vectors landed.

## RFC

RFC-0011-h §Implementation Phases Phase 5 + RFC-0011-m Phase 5 §Subcommand Taxonomy + RFC-0855 §8.2 Mission Advertisement + Mission Invitation

## Summary

CLI surface for discovery cache reads. Substrate-faithful boundary per [[cipherocto-design-principles]] §Stable Abstractions Principle.

### Subcommands

- `discovery advertisement show [--advertisement-id <HEX>] [--hops <U16>]` — read-only. Calls `MissionAdvertisementCache::get(advertisement_id)` (substrate-faithful per G23) for targeted lookup; calls `MissionAdvertisementCache::iter()` (substrate-faithful per G23) for full enumeration. `--hops <U16>` optional arg surfaces `is_ttl_exceeded(hop_count)` per RFC-0855 §8.2. Substrate-faithful Option::None translation: exit 89 `NetworkSubstrateUnavailable { companion: "G23" }`
- `discovery invitation show [--invitation-id <HEX>]` — read-only. Calls `MissionInvitationCache::get(invitation_id)` (substrate-faithful per G24) for targeted lookup; calls `MissionInvitationCache::iter()` (substrate-faithful per G24) for full enumeration. Substrate-faithful Option::None translation: exit 89 `NetworkSubstrateUnavailable { companion: "G24" }`

Both subcommands carry no confirmation flags (read-only per RFC-0011-h row 86 + row 664 ALLOW-in-CI). The `--hops 65536` overflow rejected pre-dispatch via clap u16 parse error (exit 2) per `tv_net5_3`.

## Blocked substrate-additions companions (now CLOSED)

- `0011-h-s-a-discovery-advertisement-cache` (G23) — CLOSED at `next 24bfec96` (substrate `MissionAdvertisementCache` + 6 unit tests) + `next fcb58331` (YAML Claimed) + `next 346f10cc` (CLI dispatch); YAML Completed landing
- `0011-h-s-a-discovery-invitation-cache` (G24) — CLOSED at `next 24bfec96` (substrate `MissionInvitationCache` + 7 unit tests) + `next fcb58331` (YAML Claimed) + `next 346f10cc` (CLI dispatch); YAML Completed landing

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-m Phase 5 §Subcommand Taxonomy rows 327-329 (next 346f10cc)
- [x] Test vectors per RFC-0011-m Phase 5 §Test Vectors for the listed subcommands (6 test vectors tv_net5_1 through tv_net5_6 added; next 346f10cc)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (396/396, +6 above Phase 4 baseline of 390)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change; zero Layer B change beyond the 2 paired substrate slices)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.
- Pairing invariant: substrate missions G23 + G24 `0011-h-s-a-*` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-discovery-advertisement-cache` + `0011-h-s-a-discovery-invitation-cache` companion missions)
- MissionAdvertisementCache persistence adapter (Phase 6 follow-on per `0011-h-s-a-discovery-advertisement-persistence`)
- MissionInvitationCache persistence adapter (Phase 6 follow-on per `0011-h-s-a-discovery-invitation-persistence`)
- Signature verification (substrate-side; CLI side hex-encodes signing bytes per RFC-0011-m §Security Considerations)

## Notes

CLI dispatch slice landed 2026-09-20 at `next 346f10cc`. The substrate-faithful Option::None translation (discovery advertisement show → exit 89 NetworkSubstrateUnavailable G23) + Option::None translation (discovery invitation show → exit 89 NetworkSubstrateUnavailable G24) surface as typed OctoCliError variants that operator switch tables can grep on exit code 89 (REUSED from Phase 2 per RFC-0011-h §Error Handling row 526). Phase 6 adapter follow-on will populate the success-envelope branches with persisted cache reads. The `--hops <U16>` clap arg overflow protection (`--hops 65536` → exit 2) surfaces in `tv_net5_3` test vector. The pastejacking defense (mixed-case hex rejected) surfaces in `tv_net5_6` test vector.
