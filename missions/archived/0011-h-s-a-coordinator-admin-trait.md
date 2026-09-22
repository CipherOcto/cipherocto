# 0011-h-s-a-coordinator-admin-trait — Substrate additions for CoordinatorAdminAction typed dispatch

## Status

Completed (2026-09-20) — Substrate additions + CLI dispatch CLOSED. Substrate-faithful `CoordinatorAdminAction` typed dispatch enum + `dispatch_coordinator_admin_action` sync helper land in `crates/octo-network/src/dot/adapters/coordinator_admin.rs`. CLI dispatch wired at `octo network coordinator admin --action <LABEL>` (3-flag confirmation). Substrate-additions + CLI dispatch landed end-to-end per RFC-0011-h §Substrate-Additions Companion Missions row G12.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G12

## Summary

Adds `CoordinatorAdminAction` typed dispatch enum + `CoordinatorAdminActionError` + `dispatch_coordinator_admin_action` sync helper per RFC-0011-k §Substrate-Additions Companion Missions row G12. CLI dispatch consumes via substrate-faithful `AdapterUnwired` translation.

### Substrate additions target

```rust
// crates/octo-network/src/dot/adapters/coordinator_admin.rs
pub enum CoordinatorAdminAction {
    TransferOwnership { group_id: GroupId, new_owner_peer_id: [u8; 32] },
    BanMember { group_id: GroupId, member_peer_id: [u8; 32] },
    PromoteToAdmin { group_id: GroupId, member_peer_id: [u8; 32] },
}

pub enum CoordinatorAdminActionError {
    AdapterUnwired,
}

pub fn dispatch_coordinator_admin_action(action: &CoordinatorAdminAction) -> Result<(), CoordinatorAdminActionError>;
```

Substrate additions land 2026-09-20 at `next 10ae8e18`:
- `CoordinatorAdminAction` `#[non_exhaustive]` enum with 3 variants
- `CoordinatorAdminActionError` enum with `AdapterUnwired` variant
- `dispatch_coordinator_admin_action` sync helper returns `AdapterUnwired` for all actions (substrate-faithful: no `CoordinatorAdmin` adapter wired at the CLI dispatch boundary)
- 4 unit tests: `t_dispatch_transfer_ownership_returns_adapter_unwired`, `t_dispatch_ban_member_returns_adapter_unwired`, `t_dispatch_promote_to_admin_returns_adapter_unwired`, `t_dispatch_action_enum_is_non_exhaustive`
- Re-exports through `crates/octo-network/src/dot/mod.rs`

CLI dispatch wired 2026-09-20 at `next 624998ba`:
- `octo network coordinator admin --action <LABEL> --group-id <ID> --target <peer_hex>` wired at `crates/octo-cli/src/commands/network.rs` (Layer C)
- `CoordinatorAdminActionLabel` CLI-only enum translates string label to typed `CoordinatorAdminAction` at the dispatch boundary (parse via `parse_coordinator_admin_action_label`)
- 3 action labels: `transfer_ownership`, `ban_member`, `promote_to_admin`
- `dispatch_coordinator_admin_action(&substrate_action)` returns `AdapterUnwired` → CLI surfaces `OctoCliError::Internal("coordinator admin adapter not yet wired (Phase 6 follow-on)")`
- 3-flag confirmation per `require_confirm` (auditor denied; CI requires `--allow-write`)
- target peer_id 64-char lowercase hex validated at parse time via `parse_64_char_hex_32byte` (pastejacking defense)
- New output envelope: `NetworkCoordinatorAdminOutput { action, group_id, target_peer_redacted }` (reserved for Phase 6 adapter)
- 3 new test vectors: `tv_net3_3_coordinator_admin_parses_transfer_ownership`, `tv_net3_4_coordinator_admin_rejects_unknown_label`, `tv_net3_7_coordinator_admin_substrate_returns_adapter_unwired`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/dot/adapters/coordinator_admin.rs` per RFC-0011-h §Substrate-Additions row G12 (next 10ae8e18)
- [x] CLI dispatch wired at `crates/octo-cli/src/commands/network.rs` per RFC-0011-h §Subcommand Taxonomy Phase 3 (next 624998ba)
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (4/4 dispatch tests pass)
- [x] `cargo test -p octo-cli --lib` green (3/3 coordinator admin test vectors pass; +3 above Phase 2 baseline)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle; Layer C consumes via re-export)
- [x] ≥3 unit tests + ≥1 integration test (4 substrate unit tests + 3 CLI test vectors added)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CoordinatorAdmin adapter wiring (Phase 6 follow-on per `0011-h-s-a-coordinator-admin-adapter`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Async trait method overrides per platform (deferred to adapter wiring phase)

## Notes

Substrate slice landed 2026-09-20 at `next 10ae8e18`. CLI dispatch slice landed 2026-09-20 at `next 624998ba`. The underlying `CoordinatorAdmin` trait methods (`transfer_ownership`, `ban_member`, `promote_to_admin`) already exist with default `Unimplemented` implementations; this mission adds the typed Layer-B sync dispatch entry point. Phase 6 follow-on will wire the adapter at the dispatch boundary; the CLI today surfaces the typed `AdapterUnwired` mapping as `OctoCliError::Internal` per the substrate-faithful pattern.
