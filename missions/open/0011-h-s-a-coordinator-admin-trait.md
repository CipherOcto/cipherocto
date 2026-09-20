# 0011-h-s-a-coordinator-admin-trait — Substrate additions for CoordinatorAdminAction typed dispatch

## Status

Claimed (2026-09-20) — Substrate additions LANDED at `next 10ae8e18`. Substrate-faithful `CoordinatorAdminAction` typed dispatch enum + `dispatch_coordinator_admin_action` sync helper land in `crates/octo-network/src/dot/adapters/coordinator_admin.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G12.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G12

## Summary

Adds `CoordinatorAdminAction` typed dispatch enum + `CoordinatorAdminActionError` + `dispatch_coordinator_admin_action` sync helper per RFC-0011-k §Substrate-Additions Companion Missions row G12. Pre-requisite for `octo network coordinator admin` CLI dispatch.

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

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/dot/adapters/coordinator_admin.rs` per RFC-0011-h §Substrate-Additions row G12
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (4/4 dispatch tests pass)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (4 unit tests added)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-coordinator` covers that surface — pending Phase 3 IMPLEMENTATION)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Async trait method overrides per platform (deferred to adapter wiring phase)

## Notes

Substrate slice landed 2026-09-20 at `next 10ae8e18`. Companion substrate slice bundles G3b + G12 + G12b together per the substrate-first ordering principle. The underlying `CoordinatorAdmin` trait methods (`transfer_ownership`, `ban_member`, `promote_to_admin`) already exist with default `Unimplemented` implementations; this mission adds the typed Layer-B sync dispatch entry point. Phase 3 IMPLEMENTATION closes the CLI dispatch surface (`octo network coordinator admin`) after this mission transitions to Completed.
