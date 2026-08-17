---
name: mission-0870-c1-version-tag-amendment-status
description: S6a RFC-0870 version_tag amendment + TV-0870-01 byte-exact fixture LANDED 2026-08-17 (commit c7f99a47). 5/5 TV-0870-01 tests pass.
metadata:
  type: project
---

# S6a — RFC-0870 version_tag amendment LANDED 2026-08-17

Mission `0870-c1-version-tag-amendment` closed. First S6 sub-session
per user split-by-RFC decision (overrides plan §22 atomic-blocker
bundle for this session).

## What landed

- **RFC-0870 §Version History v2.1 row** added:
  `rfcs/accepted/networking/0870-distributed-quota-router-network.md`
- **RFC-0870 §NodeEnvelope Version Tag subsection** added under
  §Specification (immediately after §NodeEnvelope Adoption):
  - `version_tag: u8` field placement (after `envelope_id`, before
    `from_did`)
  - `VERSION_TAG_V1 = 0xA0` / `VERSION_TAG_V2 = 0xA1` constants
  - Build rejects unknown tags with
    `ProtocolError::UnsupportedVersion(u8)`
  - Verify-time policy: V1 receipts deterministically rejected per
    RFC-0870 §14.1
  - Wire-format break notice (V1/V2 distinct `envelope_id` —
    replay defense across cutover)
- **TV-0870-01 byte-exact fixture**:
  `crates/octo-protocol/tests/tv_0870_version_tag.rs` (5/5 tests):
  - `tv_0870_01_v2_build_accepts_and_round_trips`
  - `tv_0870_01_v1_build_accepts_legacy_path`
  - `tv_0870_01_unknown_tag_rejected_at_build`
  - `tv_0870_01_verify_version_gate` (V2 ok, V1 rejected, unknown
    rejected)
  - `tv_0870_01_v1_and_v2_envelope_ids_differ` (replay-defense
    invariant)

## Verify gate (this session)

- `cargo test -p octo-protocol --test tv_0870_version_tag` → 5/5 pass
- `cargo test --workspace --lib` (excluding pre-existing S4 DFP
  Round 2 quota-router-cli failures) → all green
- `cargo clippy --workspace --all-targets --features full -- -D
warnings` → clean
- `cargo fmt --all -- --check` → clean

## Why this matters

`NodeEnvelope.version_tag` is the wire-format discriminator that
operates independently of `payload_kind`. Without it, the V1→V2
cutover is silent — old receipts could replay at the same
`envelope_id` as new receipts. With it, V1 and V2 receipts of the
same logical payload produce distinct `envelope_id`s, so the
replay-defense invariant holds across the cutover boundary.

## Push authorization

Commit `c7f99a47` queued on `next`. Push user-only per
[[feedback_initiative_user_only]] + [[git-workflow]].

## Next sub-session

S6b RFC-0957 (20 TV) — verify-time + caveat DSL amendment + TV
fixtures. Mission YAML to be filed at next claim.

## Cross-reference

- Plan: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6
- Mission: `missions/open/0870-c1-version-tag-amendment.md`
  (status: LANDED)
- Pre-req: `memory/mission-0957-g-verify-time-invariant-status.md`
- Review source: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1
