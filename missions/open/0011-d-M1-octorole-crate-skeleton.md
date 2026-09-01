---
name: 0011-d-M1-octorole-crate-skeleton
description: Scaffold new `crates/octo-role/` Layer B crate per RFC-0011-d §7.4; Cargo.toml deps on `octo-slash-ledger` + `octo-policy`; empty `lib.rs` + `types.rs` with module-level doc comments referencing §7.4 + §7.5.
metadata:
  node_type: substrate-cli
  type: substrate-crate-skeleton
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-d
    - RFC-0900
    - RFC-0855
status: Claimed
---

# 0011-d-M1-octorole-crate-skeleton — Scaffold `crates/octo-role/` crate per RFC-0011-d §7.4

**Status:** Claimed 2026-09-01 by @mmacedoeu — unblocked.
**Substrate:** RFC-0011-d §7.4 Substrate `[ADD]` signatures
**Parent:** RFC-0011-d (mission `0011-d-role-subcommands-phase1` aggregate; this M1 is the first atomic of 9)
**Depends on:** none (foundation mission)

## Status

Claimed (2026-09-01) by @mmacedoeu. Foundation mission for the 9 Phase 1 atomic missions decomposing `0011-d-role-subcommands-phase1`. Lands the empty crate skeleton; M2-M9 add types, entrypoints, CLI binding, errors, tests, doc follow-on.

## Substrate (RFC-0011-d)

RFC-0011-d §7.4 Substrate `[ADD]` signatures (`octo_role::list`, `octo_role::show`, `octo_role::select`); §7.5 Role Summary (canonical role → role-token mapping).

## Parent

RFC-0011-d (`octo role` provisioning subcommands; Phase 1 of the RFC-0011 amendment chain). Inherits every contract from parent RFC-0011 (`octo` CLI substrate).

## Depends on

Hard sequence: M1 (this mission) → M2 → M3 → M4 → M5 → M6 → M7 → M8 + M9 (parallel doc). Mission 9 is independent pure-doc and may land in parallel with M4-M8.

## Acceptance Criteria

- [ ] `crates/octo-role/` directory created (NEW per RFC-0011-d §7.4)
- [ ] `crates/octo-role/Cargo.toml` with deps `octo-slash-ledger = { path = "../octo-slash-ledger" }` + `octo-policy = { path = "../octo-policy" }`
- [ ] `crates/octo-role/src/lib.rs` with module-level doc comment referencing RFC-0011-d §7.4 + §7.5
- [ ] `crates/octo-role/src/types.rs` (empty module placeholder) with doc comment explaining the upcoming `RoleSummary` / `RoleRecord` / `RoleFilter` / `SlashingRule` / `RoleBinding` / `RoleError` types per RFC-0011-d §7.4
- [ ] `crates/octo-role/README.md` (NEW) summarizing crate purpose + RFC-0011-d anchor
- [ ] `crates/octo-role/src/lib.rs` `pub use types::*` re-export placeholder (will be populated by M2)
- [ ] `cargo build -p octo-role` succeeds with zero warnings
- [ ] `cargo clippy -p octo-role --all-targets -- -D warnings` clean
- [ ] `cargo fmt -p octo-role -- --check` clean
- [ ] `octo-role` registered in workspace `Cargo.toml` `[members]` array
- [ ] No `octo-cli` changes (CLI binding lands in M6)

## Scope

Scaffold only. NO type definitions (M2), NO substrate entrypoints (M3, M4), NO CLI binding (M6). This mission lands the crate skeleton + documentation scaffolding that subsequent missions fill in.

## Sub-steps

1. Create directory `crates/octo-role/`
2. Create `Cargo.toml` with deps
3. Create `src/lib.rs` with module doc
4. Create `src/types.rs` with placeholder doc
5. Create `README.md` with crate summary
6. Add to workspace `Cargo.toml`
7. Verify `cargo build -p octo-role` succeeds

## Test Vectors

N/A — skeleton mission. Type/entrypoint tests land in M2/M3/M4/M8.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-role` (Layer B; NEW per RFC-0011-d §7.4) — foundation crate for role substrate. No consumers yet (M3-M8 add consumers in substrate + CLI).
- `octo-slash-ledger` (Layer B; per RFC-0900) — pre-existing crate; `octo-role` depends on it (slash ledger substrate for `select` envelope per M4)
- `octo-policy` (Layer B; per RFC-0967) — pre-existing crate; `octo-role` depends on it (policy substrate for role filters)

## Backward compat

Additive: new crate, no existing crates modified. No RFC migration etiquette triggers (no removed APIs, no `schema_version` bumps, no exit-code changes).

## Cross-references

- RFC-0011-d §7.4 Substrate `[ADD]` signatures
- RFC-0011-d §7.5 Role Summary
- RFC-0900 §Slash Ledger Substrate
- RFC-0855 §Mission Overlay Networks (role namespace)
- RFC-0011 — `octo` CLI substrate (parent of RFC-0011-d)
- [[cipherocto-design-principles]] — Layer B stability contract; no central enum

## Notes

- Foundation mission — empty crate skeleton only; types land in M2, entrypoints in M3+M4, CLI binding in M6
- NO business logic in this mission; pure scaffolding per RFC §Mission Decomp M1 row
- Per [[implementation-workflow-hook]]: skeleton missions are normal Phase 1 work (NOT deferred); claim once M2-M8 prereqs are tracked
- Lands as first PR in the 9-mission Phase 1 chain

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
