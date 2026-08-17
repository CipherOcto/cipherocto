---
name: mission-0870-c1-version-tag-amendment-status
description: S6a RFC-0870 version_tag amendment + TV-0870-01 byte-exact fixture LANDED 2026-08-17 (commit c7f99a47). 8/8 TV-0870-01 tests pass (5 original + 2 Round 1 review fix ab2b57b4 + 1 Round 2 review fix).
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
  `crates/octo-protocol/tests/tv_0870_version_tag.rs` (8/8 tests —
  5 original + 2 added in Round 1 review fix commit `ab2b57b4` +
  1 added in Round 2 review fix):
  - `tv_0870_01_v2_build_accepts_and_round_trips`
  - `tv_0870_01_v1_build_accepts_legacy_path`
  - `tv_0870_01_unknown_tag_rejected_at_build`
  - `tv_0870_01_verify_version_gate` (V2 ok, V1 rejected, unknown
    rejected)
  - `tv_0870_01_v1_and_v2_envelope_ids_differ`
    (version_tag-participates-in-hash invariant; NOT a literal
    V1-replay-defense assertion per Round 1 HIGH-2 fix)
  - `tv_0870_01_byte_position_pin` (Round 1 MED-4 — `bytes[32] ==
    0xA1` for V2, `0xA0` for V1)
  - `tv_0870_01_runtime_gate_rejects_bypassed_unknown_tag` (Round 1
    HIGH-3 — runtime gate rejects even when struct-literal-bypassed)
  - `tv_0870_01_absent_version_tag_field_rejected` (Round 2 M-1 —
    truncated borsh bytes → `from_slice` returns `Err`, not silent
    `version_tag = 0`)

## Verify gate (this session)

- `cargo test -p octo-protocol --test tv_0870_version_tag` → 8/8
  pass (Round 1 fix commit `ab2b57b4` added 2 regression tests;
  Round 2 fix added 1 regression test)
- `cargo test --workspace --lib` (excluding pre-existing S4 DFP
  Round 2 quota-router-cli failures) → all green
- `cargo clippy --workspace --all-targets --features full -- -D
  warnings` → clean
- `cargo fmt --all -- --check` → clean

## Why this matters

`NodeEnvelope.version_tag` is the wire-format discriminator that
operates independently of `payload_kind`. Without it, V1 receipts
would be byte-identical to V2 receipts at every offset except 32,
producing identical `envelope_id`s and defeating replay defense.
With it, `verify_version` hard-rejects V1 at the structural gate
per RFC-0870 §14.1, and V1/V2 `envelope_id`s differ as a
defense-in-depth check.

## Round 1 adversarial review (closed 2026-08-17)

13 findings (3 CRIT, 3 HIGH, 4 MED, 3 LOW) all closed in commit
`ab2b57b4`. See mission YAML `## Cross-reference` and the commit
message for the per-finding fix map.

## Round 2 adversarial review (closed 2026-08-17)

8 new findings (0 CRIT, 3 HIGH, 3 MED, 2 LOW) — drift introduced
by Round 1 fixes (test count 5/5 → 7/7 not propagated to memory
cards, mission Status/AC #4 gate wording mismatch, RFC v2.1 row
"Additive amendment" tail contradicts wire-format break claim,
"Why this matters" paragraph mischaracterizes the threat model,
missing-version_tag borsh decode error untested, blast radius
understated). All closed in Round 2 fix commit (this session).

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
