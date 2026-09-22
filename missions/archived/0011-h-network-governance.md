# 0011-h-network-governance — `governance` subcommands per RFC-0011-h Phase 3

## Status

Completed (2026-09-20) — CLI dispatch slice CLOSED. `octo network governance tally --proposal-id <N>` wired at `next 624998ba` per RFC-0011-h §Implementation Phases Phase 3. `governance rotation status` was already wired in Phase 1 (next c2fee8f3).

## RFC

RFC-0011-h §Implementation Phases Phase 3

## Summary

CLI surface for `governance tally` (read-only; canonical-bytes hash projection) + `governance rotation status` (Phase 1 read; substrate-faithful boundary).

### Subcommands

- `governance tally --proposal-id <N>` — calls `governance_proposal_canonical_bytes` (substrate-faithful per G3b); emits 64-char canonical_hash_hex envelope field
- `governance rotation status --did-codec <DID>` (Phase 1) — read-only; substrate-faithful boundary

## Blocked substrate-additions companions (now CLOSED)

- `0011-h-s-a-voting-tally-canonical-bytes` — CLOSED at `next 10ae8e18` (substrate) + `next 624998ba` (CLI dispatch); YAML Completed landed

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy Phase 3
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (2 Phase 3 test vectors added: tv_net3_5 + tv_net3_8; Phase 1 rotation status test vectors from prior session)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (368/368, +2 above Phase 2 baseline)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.
- Pairing invariant: substrate mission G3b `0011-h-s-a-voting-tally-canonical-bytes` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions)
- Governance tally persistence adapter (Phase 6 follow-on)
- Vote ledger persistence (deferred to `0011-h-s-a-attest-vote` companion family)

## Notes

CLI dispatch slice landed 2026-09-20 at `next 624998ba`. The CLI today projects the canonical-bytes hash of the zero-default proposal at the requested `proposal_id` (substrate-faithful boundary: the canonical-bytes helper is the only persistence surface today). The full tally ledger (approval_bps + rejection_bps + state) lands as the Phase 6 persistence adapter follow-on.
