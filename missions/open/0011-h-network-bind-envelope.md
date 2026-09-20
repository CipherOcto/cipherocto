# 0011-h-network-bind-envelope — `bind-envelope` subcommands per RFC-0011-h Phase 4

## Status

Completed (2026-09-20) — CLI dispatch slice CLOSED. `octo network bind-envelope show` + `octo network bind-envelope rebind-{prepare,commit,abort}` wired at `next 0df7e579` per RFC-0011-l Phase 4 §Subcommand Taxonomy. All 4 subcommand arms + 4 output envelopes + 12 test vectors + OctoCliError slot 88 `NetworkDryRunDenied` landed.

## RFC

RFC-0011-l Phase 4 §Implementation Phases + §Subcommand Taxonomy rows 322-326

## Summary

CLI surface for bind-envelope read (substrate-faithful `BindEnvelope::load` lookup per G22) + rebind-* payload builder trio (`dispatch_rebind_arm_action` per G21). Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle.

### Subcommands

- `bind-envelope show --domain-id <ID>` — calls `BindEnvelope::load(domain_id)` (substrate-faithful per G22); translates `Option::None` to typed exit 89 `NetworkSubstrateUnavailable { companion: "G22" }` (Phase 6 persistence adapter follow-on per `0011-h-s-a-bind-envelope-persistence`)
- `bind-envelope rebind-prepare --domain-id <ID> --no-dry-run --confirm-acknowledge` — calls `dispatch_rebind_arm_action(&coord, RebindArmAction::Prepare, RebindArmKey::V1)` (substrate-faithful per G21); `--allow-ci-deny-default` DEBUG-ONLY escape hatch wired per RFC-0011-h §Security Considerations experimental-flag contract
- `bind-envelope rebind-commit --domain-id <ID> --no-dry-run --confirm-acknowledge --confirm` — pastejacking-defense SECOND `--confirm` flag required per RFC-0011-h §Security Considerations rebind-commit row; missing `--confirm` fires typed exit 88 `NetworkDryRunDenied`
- `bind-envelope rebind-abort --domain-id <ID> --no-dry-run --confirm-acknowledge --reason <TEXT>` — substrate is idempotent on double-abort per RFC-0871 §Algorithms (no `--confirm` SECOND flag); operator-supplied `reason` redaction via `redact_reason` helper

## Blocked substrate-additions companions (now CLOSED)

- `0011-h-s-a-bind-envelope-lookup` (G22) — CLOSED at `next edcdc47a` (substrate `BindEnvelope::load` lookup method) + `next 475aa5af` (YAML Claimed) + `next 0df7e579` (CLI dispatch); YAML Completed landing
- `0011-h-s-a-attached-handle-key-rotation` (G21) — CLOSED at `next 3794e4a8` (substrate `RebindArmAction` + `dispatch_rebind_arm_action`) + `next 475aa5af` (YAML Claimed) + `next 0df7e579` (CLI dispatch); YAML Completed landing
- `0011-h-s-a-ci-detection` (G25) — CLOSED at `next 1bb1ecbc` (`CiDetection::detect` + `CiMode` enum helper) + `next fd3127c2` (YAML Claimed); YAML Completed landing. The CI gate itself (slot 90 `NetworkCIDenyDefault`) is forward-looking per RFC-0011-l Phase 4 row 50 (deferred to Phase 5/6); the `--allow-ci-deny-default` DEBUG-ONLY escape hatch is wired as a clap arm today.

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-l Phase 4 §Subcommand Taxonomy rows 322-326 (next 0df7e579)
- [x] Test vectors per RFC-0011-l Phase 4 §Test Vectors for the listed subcommands (12 test vectors tv_net4_1 through tv_net4_12 added; next 0df7e579)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (390/390, +12 above Phase 3 baseline)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change; zero Layer B change beyond the 3 paired substrate slices)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.
- Pairing invariant: substrate missions G22 + G21 + G25 `0011-h-s-a-*` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions; substrate slice already landed at next edcdc47a + next 3794e4a8)
- BindEnvelope persistence adapter (Phase 6 follow-on per `0011-h-s-a-bind-envelope-persistence`)
- RebindArm persistence adapter (Phase 6 follow-on per `0011-h-s-a-rebind-arm-persistence`)
- NetworkCIDenyDefault (slot 90) CI gate formalization (deferred to Phase 5/6 per RFC-0011-l Phase 4 row 50)

## Notes

CLI dispatch slice landed 2026-09-20 at `next 0df7e579`. The substrate-faithful Option::None translation (bind-envelope show → exit 89 NetworkSubstrateUnavailable) + AdapterUnwired translation (rebind-* trio → exit 1 Internal) + NetworkDryRunDenied translation (rebind-commit without --confirm → exit 88 per pastejacking defense) all surface as typed OctoCliError variants that operator switch tables can grep on exit codes 89 / 1 / 88. Phase 6 adapter follow-on will populate the success-envelope branches for rebind-*; Phase 5/6 follow-on will formalize the CI gate (slot 90 `NetworkCIDenyDefault`) wired today as `--allow-ci-deny-default` DEBUG-ONLY escape hatch.
