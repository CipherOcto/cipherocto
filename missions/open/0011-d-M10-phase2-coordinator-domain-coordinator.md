---
name: 0011-d-M10-phase2-coordinator-domain-coordinator
description: Phase 2 (gated) per RFC-0011-d §Mission Decomposition M10 row: add `octo role select coordinator` + `domain-coordinator` clap subcommands + substrate entrypoints; gate on RFC-0855p-d + RFC-0855p-e reaching Accepted.
metadata:
  node_type: substrate-cli
  type: cli-subcommand-phase2
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - RFC-0855p-d (must be Accepted)
    - RFC-0855p-e (must be Accepted)
    - mission 0011-d-M6-octocli-role-commands
    - mission 0011-d-M11-phase2-domain-coordinator-platform-binding
status: Claimed
---

# 0011-d-M10-phase2-coordinator-domain-coordinator — Phase 2 `octo role select coordinator` + `domain-coordinator` per RFC-0011-d §Mission Decomposition M10

**Status:** Claimed 2026-09-01 by @mmacedoeu — GATED.
**Substrate:** RFC-0011-d §Mission Decomposition M10 row; §Phase 2; §Compatibility partial-prereq caveat
**Parent:** RFC-0011-d
**Depends on:** RFC-0855p-d Accepted + RFC-0855p-e Accepted (gate); `0011-d-M6-octocli-role-commands` (Phase 1 surface); `0011-d-M11-phase2-domain-coordinator-platform-binding` (M11 must land FIRST per substrate-first pattern)

## Status

Claimed (2026-09-01) by @mmacedoeu — **GATED**. Does NOT claim until RFC-0855p-d + RFC-0855p-e reach Accepted status. Tenth of 11 atomic missions (9 Phase 1 + 2 Phase 2).

## Substrate (RFC-0011-d)

§Mission Decomposition M10 row (canonical):

- "Add `octo role select coordinator` + `domain-coordinator` clap subcommands + substrate entrypoints; gate on RFC-0855p-d + RFC-0855p-e reaching Accepted"

§Compatibility partial-prereq caveat: until both prereq RFCs reach Accepted, `octo role select coordinator` and `octo role select domain-coordinator` return exit 33 (`RoleNotSelectable` per M7 variant) + prereq RFC names in error message (TV-RP-1 carries forward from M8).

## Parent

RFC-0011-d §Mission Decomposition M10 row; §Phase 2; §Compatibility partial-prereq caveat; §Test Vectors Phase 2 (+3 vectors beyond Phase 1).

## Depends on (GATE)

**HARD GATE**: RFC-0855p-d + RFC-0855p-e MUST reach Accepted status before this mission claims. Per [[deferred-vs-unspecified]] + BLUEPRINT §Mission Lifecycle.

- `RFC-0855p-d` (Sub-Domain / Sub-Group Nesting substrate) — required for `domain-coordinator` (sub-group nesting is a prereq for `domain-coordinator` per §Compatibility)
- `RFC-0855p-e` (HandoverRequest Envelope & Coordinator Term Handover substrate) — required for BOTH `coordinator` AND `domain-coordinator` role bindings
- `0011-d-M11-phase2-domain-coordinator-platform-binding` (substrate-first pattern; M11 must land BEFORE M10 per substrate-first workflow)
- `0011-d-M6-octocli-role-commands` (Phase 1 surface; Phase 2 extends with 2 new role_id args under existing `octo role select`)

## Acceptance Criteria

- [ ] Extend existing `crates/octo-cli/src/commands/role.rs` (NOT new top-level `coordinator` subcommand group per RFC §Mission Decomp M10 row)
- Add `octo role select coordinator` clap subcommand (extends existing `Role::Select`)
- Add `octo role select domain-coordinator` clap subcommand (extends existing `Role::Select`)
- Add 2 substrate entrypoints: `octo_role::select_coordinator(operator_did, signer)` + `octo_role::select_domain_coordinator(operator_did, signer)` per RFC §Mission Decomp M10 row ("substrate entrypoints")
- `octo role select coordinator` calls `octo_role::select_coordinator(...)` (M10 substrate)
- `octo role select domain-coordinator` calls `octo_role::select_domain_coordinator(...)` (M10 substrate); binds via `octo_coordinator::bind_domain_coordinator` (M11 substrate)
- `--confirm` required for both (exit 33 `RoleNotSelectable` per M7 if missing confirm)
- Partial-prereq guard ENFORCED until release_gate unblocks: `coordinator` + `domain-coordinator` return exit 33 + `RoleNotSelectable` + prereq RFC names in error message (TV-RP-1 carries forward from M8)
- New error variant `RoleNotSelectable { reason: "RFC-0855p-d not Accepted" | "RFC-0855p-e not Accepted" | ... }` (already in M7 §variant list)
- 3 additional test vectors pass (Phase 2 §Test Vectors per RFC)
- `cargo test -p octo-cli role_select_coordinator_dry_run`
- `cargo test -p octo-cli role_select_domain_coordinator_dry_run`
- `cargo test -p octo-cli role_select_coordinator_partial_prereq_guard`
- `cargo test -p octo-cli role_select_domain_coordinator_partial_prereq_guard`
- `cargo check -p octo-cli -p octo-role -p octo-coordinator` zero warnings
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- **GATE CHECK**: `git log --oneline rfcs/accepted/governance/0855p-d-*.md rfcs/accepted/governance/0855p-e-*.md` shows both Accepted

## Scope

Phase 2 CLI extension + substrate entrypoints. NO Phase 2 platform binding (M11 owns `bind_domain_coordinator` with `platform_admin_id`).

## Sub-steps

1. **VERIFY GATE** (first step; abort if not met)
2. **VERIFY M11 LANDED** (second step; substrate-first pattern)
3. Extend `crates/octo-role/src/lib.rs` with 2 new substrate entrypoints: `select_coordinator` + `select_domain_coordinator`
4. Extend `crates/octo-cli/src/commands/role.rs` with 2 new role_id args: `coordinator` + `domain-coordinator`
5. Wire `select_coordinator` to RFC-0855p-e HandoverRequest envelope
6. Wire `select_domain_coordinator` to M11 `bind_domain_coordinator` (platform_admin_id update)
7. Add partial-prereq guard returning exit 33 + `RoleNotSelectable` + prereq RFC names
8. Add 4 unit tests listed in Acceptance Criteria
9. Verify cargo check + clippy + fmt

## Test Vectors (Phase 2 +3 per RFC)

Per RFC-0011-d §Test Vectors Phase 2:

- TV-RP-1 (carried forward from M8): partial-prereq guard returns exit 33 + `RoleNotSelectable` + prereq RFC names in error message
- TV-RC-1: `octo role select coordinator` success path (post-0855p-e Accept)
- TV-RDC-1: `octo role select domain-coordinator` success path (post-0855p-d + 0855p-e Accept); `DomainCoordinatorRecord.platform_admin_id` updated per RFC-0855p-c

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C) — thin CLI wrapper extending existing `Role::Select`
- `octo-role` (Layer B; per RFC-0011-d §7.4) — Phase 2 substrate entrypoints
- `octo-coordinator` (Layer B; per RFC-0855p-d) — domain-coordinator substrate (M11)
- `octo-slash-ledger` (Layer B; per RFC-0900) — HandoverRequest envelope (RFC-0855p-e)

## Backward compat

Additive: 2 new role_id args + 2 new substrate entrypoints. NO existing CLI commands modified (extends existing `octo role select`).

## Risk

- **Gate not met**: this mission cannot claim until RFC-0855p-d + RFC-0855p-e Accepted. Mitigation: explicit GATE CHECK in Sub-step 1; mission stays Open until met.
- **Substrate-first ordering**: M10 calls M11 substrate; M11 must land FIRST. Mitigation: explicit M11 LANDED check in Sub-step 2.
- **Error code drift**: `RoleNotSelectable { reason }` carries prereq RFC names; reason string format must match TV-RP-1. Mitigation: canonical reason string per RFC-0011-d §Compatibility partial-prereq caveat.

## Notes

- Phase 2 mission — gated
- Per [[deferred-vs-unspecified]]: deferred (not unspecified); gate enforced
- Sub-step 1 = gate check; Sub-step 2 = M11 substrate-first check
- Extends existing `octo role select` (NOT new top-level `coordinator` group per RFC §Mission Decomp M10 row)

## Cross-references

- RFC-0011-d §Mission Decomposition M10 row (canonical scope; extends `octo role select` + substrate entrypoints)
- RFC-0011-d §Phase 2
- RFC-0011-d §Compatibility partial-prereq caveat
- RFC-0011-d §Test Vectors Phase 2 (+3 vectors beyond Phase 1)
- RFC-0855p-d (Sub-Domain / Sub-Group Nesting; gate)
- RFC-0855p-e (HandoverRequest Envelope; gate)
- RFC-0855p-c (DomainCoordinatorRecord.platform_admin_id wiring target)
- M11 (substrate-first ordering)
- [[deferred-vs-unspecified]] — gated release

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
