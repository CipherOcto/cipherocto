---
name: 0011-d-M11-phase2-domain-coordinator-platform-binding
description: Phase 2 (gated) per RFC-0011-d §Mission Decomposition M11 row: implement `[ADD] octo_coordinator::bind_domain_coordinator(role_binding, platform_admin_id)` per §7.4; wire to RFC-0855p-c `DomainCoordinatorRecord.platform_admin_id` atomic update.
metadata:
  node_type: substrate-cli
  type: substrate-entrypoint-phase2
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - RFC-0855p-c (must be Accepted)
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
status: Claimed
---

# 0011-d-M11-phase2-domain-coordinator-platform-binding — Phase 2 substrate `bind_domain_coordinator` per RFC-0011-d §Mission Decomposition M11

**Status:** Claimed 2026-09-01 by @mmacedoeu — GATED.
**Substrate:** RFC-0011-d §Mission Decomposition M11 row; §7.4 Substrate `[ADD]`; RFC-0855p-c wiring target
**Parent:** RFC-0011-d
**Depends on:** RFC-0855p-c Accepted (gate); `0011-d-M4-octorole-select-with-stoolap-tx` (substrate precedent: BEGIN IMMEDIATE + signed envelope + Stoolap)

## Status

Claimed (2026-09-01) by @mmacedoeu — **GATED**. Eleventh of 11 atomic missions (9 Phase 1 + 2 Phase 2). M11 substrate-first ordering: M11 must land BEFORE M10 per substrate-first workflow.

## Substrate (RFC-0011-d)

§Mission Decomposition M11 row (canonical):

- "Implement `[ADD] octo_coordinator::bind_domain_coordinator(role_binding, platform_admin_id)` per §7.4; wire to RFC-0855p-c `DomainCoordinatorRecord.platform_admin_id` atomic update"

§7.4 Substrate `[ADD]` signature:

- `pub fn bind_domain_coordinator(role_binding: &RoleBinding, platform_admin_id: &PlatformAdminId) -> Result<(), CoordinatorError>` (atomic; updates RFC-0855p-c `DomainCoordinatorRecord.platform_admin_id`)

## Parent

RFC-0011-d §Mission Decomposition M11 row; §7.4 Substrate `[ADD]` signatures; §Phase 2.

## Depends on (GATE)

**HARD GATE**: RFC-0855p-c MUST reach Accepted status. Per [[deferred-vs-unspecified]] + BLUEPRINT §Mission Lifecycle.

- `RFC-0855p-c` (Domain Coordinator Record platform_admin_id target; the actual wiring target per RFC §Mission Decomp M11 row)
- `0011-d-M4-octorole-select-with-stoolap-tx` (substrate precedent: BEGIN IMMEDIATE + signed envelope + Stoolap pattern)

## Acceptance Criteria

- [ ] Extend existing `crates/octo-coordinator/` crate (NEW per RFC-0855p-d substrate pattern; already created by RFC-0855p-d Accept)
- Add `pub fn bind_domain_coordinator(role_binding: &RoleBinding, platform_admin_id: &PlatformAdminId) -> Result<(), CoordinatorError>` per RFC §Mission Decomp M11 row
- Atomic: updates RFC-0855p-c `DomainCoordinatorRecord.platform_admin_id` in same Stoolap `BEGIN IMMEDIATE` tx
- Returns `CoordinatorError::AdminMismatch { expected, actual }` on `platform_admin_id` mismatch with existing record
- Returns `CoordinatorError::CoordinatorNotFound { role_binding_hash }` if no matching RFC-0855p-c `DomainCoordinatorRecord`
- Rolls back on any error (no partial state)
- NO new substrate entrypoints (unbind/list/show are NOT in RFC §Mission Decomp M11 row scope; do NOT add)
- `#[non_exhaustive]` on `CoordinatorError` per F-14
- `cargo test -p octo-coordinator bind_domain_coordinator_updates_platform_admin_id`
- `cargo test -p octo-coordinator bind_domain_coordinator_rolls_back_on_admin_mismatch`
- `cargo test -p octo-coordinator bind_domain_coordinator_rolls_back_on_not_found`
- `cargo check -p octo-coordinator` zero warnings
- `cargo clippy -p octo-coordinator --all-targets -- -D warnings` clean
- **GATE CHECK**: `git log --oneline rfcs/accepted/governance/0855p-c-*.md` shows Accepted

## Scope

Single substrate entrypoint per RFC §Mission Decomp M11 row. NO new substrate entrypoints (unbind/list/show OUT OF SCOPE). NO CLI binding (M10 owns CLI extension).

## Sub-steps

1. **VERIFY GATE** (RFC-0855p-c Accepted)
2. Extend `crates/octo-coordinator/src/lib.rs` with `bind_domain_coordinator`
3. Wire Stoolap `BEGIN IMMEDIATE` transaction
4. Wire RFC-0855p-c `DomainCoordinatorRecord.platform_admin_id` atomic update
5. Wire `AdminMismatch` error path
6. Wire `CoordinatorNotFound` error path
7. Wire rollback on error
8. Add 3 unit tests
9. Verify cargo check + clippy + fmt

## Test Vectors

- TV-DC-SUB-1: `bind_domain_coordinator(role_binding, platform_admin_id)` with matching existing RFC-0855p-c `DomainCoordinatorRecord` → atomic update of `platform_admin_id`
- TV-DC-SUB-2: `bind_domain_coordinator(role_binding, platform_admin_id)` with mismatched `platform_admin_id` → returns `CoordinatorError::AdminMismatch`; tx rolls back
- TV-DC-SUB-3: `bind_domain_coordinator(role_binding, platform_admin_id)` with non-existent RFC-0855p-c `DomainCoordinatorRecord` → returns `CoordinatorError::CoordinatorNotFound`; tx rolls back
- TV-DC-SUB-4: DomainCoordinator role binding round-trips with `platform_admin_id` per RFC §Mission Decomp M11 row exit criteria

## Layer direction (per [[cipherocto-design-principles]])

- `octo-coordinator` (Layer B; per RFC-0855p-d substrate) — Phase 2 platform-binding substrate
- `octo-wallet` (Layer B; per RFC-0009) — role_binding substrate (input)
- Stoolap (Layer D adapter; per RFC-0010) — `BEGIN IMMEDIATE` tx

## Backward compat

Additive: 1 new public fn on existing `octo-coordinator` crate. NO existing crates modified.

## Risk

- **Gate not met**: cannot claim until RFC-0855p-c Accepted. Mitigation: explicit GATE CHECK in Sub-step 1.
- **Atomicity boundary**: `bind_domain_coordinator` MUST update RFC-0855p-c `DomainCoordinatorRecord.platform_admin_id` atomically with the role_binding. Mitigation: single Stoolap `BEGIN IMMEDIATE` tx covers both writes; explicit rollback test (TV-DC-SUB-2 + TV-DC-SUB-3).
- **Scope creep**: unbind/list/show are tempting to add but NOT in RFC §Mission Decomp M11 row. Mitigation: explicit "NO new substrate entrypoints" in Acceptance Criteria.

## Notes

- Phase 2 mission — gated
- Per [[deferred-vs-unspecified]]: deferred (not unspecified)
- Substrate-first ordering: M11 must land BEFORE M10 (M10 calls M11 substrate)
- Sub-step 1 = gate check; if unmet, mission pauses

## Cross-references

- RFC-0011-d §Mission Decomposition M11 row (canonical single-entrypoint scope)
- RFC-0011-d §7.4 (Substrate `[ADD]` signatures)
- RFC-0855p-c (DomainCoordinatorRecord.platform_admin_id wiring target; gate)
- RFC-0855p-d (octo-coordinator crate owner)
- M10 (substrate-first ordering: M10 calls M11)
- [[deferred-vs-unspecified]] — gated release

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
