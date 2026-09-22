# 0011-h-network-coordinator — `coordinator` subcommands per RFC-0011-h Phase 3

## Status

Completed (2026-09-20) — CLI dispatch slice CLOSED. `octo network coordinator show <coordinator_id_hex>` + `octo network coordinator admin --action <LABEL>` (3-flag confirmation) wired at `next 624998ba` per RFC-0011-h §Implementation Phases Phase 3.

## RFC

RFC-0011-h §Implementation Phases Phase 3

## Summary

CLI surface for `coordinator show` (read-only) + `coordinator admin` (write; 3-flag confirmation). Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle.

### Subcommands

- `coordinator show <coordinator_id_hex>` — calls `CoordinatorRecord::load` (substrate-faithful per G12b); translates `Option::None` to typed exit 84 `NetworkCoordinatorNotFound`
- `coordinator admin --action <LABEL> --group-id <ID> --target <peer_hex>` — 3 action labels (`transfer_ownership` | `ban_member` | `promote_to_admin`); 3-flag confirmation; substrate `dispatch_coordinator_admin_action` returns `AdapterUnwired` today (Phase 6 follow-on)

## Blocked substrate-additions companions (now CLOSED)

- `0011-h-s-a-coordinator-record-loader` — CLOSED at `next 10ae8e18` (substrate) + `next 624998ba` (CLI dispatch); YAML Completed landed
- `0011-h-s-a-coordinator-admin-trait` — CLOSED at `next 10ae8e18` (substrate) + `next 624998ba` (CLI dispatch); YAML Completed landed

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy Phase 3
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (3 + 3 = 6 test vectors added: tv_net3_1 + tv_net3_2 + tv_net3_6 for `coordinator show`; tv_net3_3 + tv_net3_4 + tv_net3_7 for `coordinator admin`)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (368/368, +6 above Phase 2 baseline)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.
- Pairing invariant: substrate missions G12 + G12b `0011-h-s-a-coordinator-record-loader` + `0011-h-s-a-coordinator-admin-trait` must land BEFORE this CLI mission's CLI dispatch slice per [[no-phantom-mission-pointers]]

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions)
- CoordinatorRecord persistence adapter (Phase 6 follow-on per `0011-h-s-a-coordinator-record-persistence`)
- CoordinatorAdmin adapter wiring (Phase 6 follow-on per `0011-h-s-a-coordinator-admin-adapter`)

## Notes

CLI dispatch slice landed 2026-09-20 at `next 624998ba`. The substrate-faithful Option::None translation (coordinator show) + AdapterUnwired translation (coordinator admin) surfaces as typed OctoCliError variants that operator switch tables can grep on exit codes 84 (NetworkCoordinatorNotFound) and 1 (Internal with the typed Phase 6 follow-on note). Phase 6 adapter follow-on will populate the success-envelope branches.
