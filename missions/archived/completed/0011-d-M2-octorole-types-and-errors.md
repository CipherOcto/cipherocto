---
name: 0011-d-M2-octorole-types-and-errors
description: Add `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleFilter`, `RoleBinding`, `RoleError` substrate types per RFC-0011-d §7.4; `#[non_exhaustive]` on `RoleAction` per F-14.
metadata:
  node_type: substrate-cli
  type: substrate-types
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.0"
  landing_commit: "043f8543"
  verified_by: "@mmacedoeu"
  review_commit: "f08d0ca9"
  closed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M1-octorole-crate-skeleton
status: Completed
---

# 0011-d-M2-octorole-types-and-errors — Substrate types + errors per RFC-0011-d §7.4

**Status:** Completed (2026-09-01). LANDED commit `043f8543` (feat: M2 substrate types + RoleError + RoleAction) + review-loop substrate fixes `f08d0ca9`.

> **Retro-supersession (2026-09-01):** Mission landed in same substrate cycle as M1 (`003a7b0f`) per RFC-0011-d Phase 1 atomic chain. Substrate-truth deviations from original AC text documented inline below per M1 close-out pattern.

**Substrate:** RFC-0011-d §7.4 substrate types
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M1-octorole-crate-skeleton`

## Status

Closed (2026-09-01). Substrate delivered: `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleFilter`, `RoleBinding`, `RoleError`, `RoleAction` (`#[non_exhaustive]`) in `crates/octo-role/src/types.rs` + `crates/octo-role/src/error.rs`. 406/406 tests pass serial per RFC-0011-d Phase 1 DRY closure.

## Substrate (RFC-0011-d)

RFC-0011-d §7.4 substrate types: `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleFilter`, `RoleBinding`, `RoleError`. Plus `RoleAction` enum (CLI-side dispatch) with `#[non_exhaustive]` per F-14.

## Parent

RFC-0011-d §7.4 (substrate types section); §7.5 (Role Summary canonical mapping); F-14 finding (W2 R2).

## Depends on

`0011-d-M1-octorole-crate-skeleton` (skeleton must exist before types can be added). Hard sequence: M1 → M2.

## Acceptance Criteria

- [x] `RoleSummary` struct added to `crates/octo-role/src/types.rs` per §7.5 fields (role_kind_uuid, name, ticker, kind, class, requires_octo_min, requires_role_token_min)
- [x] `RoleRecord` struct added extending RoleSummary with slashing_rules + allowed_actions + registry_ref
- [x] `SlashingRule` struct added per RFC-0900 §Slashing Model fields
- [x] `RoleFilter` struct added (CLI-side parsing shape; fields: kind, class, requires_octo_min)
- [x] `RoleBinding` struct added (persisted to wallet store on `select`; includes signature_proof as RedactedHex + role_binding_hash as Hex32)
- [x] `RoleError` enum added (typed substrate error; 4 variants per RFC §Mission Decomp M7 row + RFC-0011-d §Security 2 last-writer-wins — NO RoleBindingConflict variant per substrate-truth)
- [x] `RoleAction` enum added (CLI-side dispatch; variants: List, Show, Select); `#[non_exhaustive]` attribute applied per F-14
- [x] All types derive Serialize + Deserialize + Debug + Clone (CLI surfaces JSON-serializable; schemars export deferred to RFC-0011-e vault CLI work per Phase 1+ roadmap)
- [x] `RoleSummary.role_kind_uuid: [u8; 16]` field present (RFC-0855-namespaced UUIDv5 typed discriminator; NOT central enum per [[cipherocto-design-principles]])
- [x] `RoleBinding.signature_proof: RedactedHex` (CLI redaction boundary; substrate owns the bytes)
- [x] `RoleBinding.role_binding_hash: Hex32` (public material; BLAKE3-256 digest)
- [x] `RoleAction::Select` round-trip test passes
- [x] `cargo check -p octo-role` zero warnings
- [x] `cargo clippy -p octo-role --all-targets -- -D warnings` clean

## Scope

Type scaffolding only. NO entrypoint implementations (M3, M4). NO wallet persistence (M5). NO CLI binding (M6).

## Sub-steps

1. Add `RoleSummary` struct to `types.rs` with role_kind_uuid, name, ticker, stake fields per §7.5
2. Add `RoleRecord` struct extending RoleSummary with slashing_rules + allowed_actions + registry_ref
3. Add `SlashingRule` struct per RFC-0900 §Slashing Model
4. Add `RoleFilter` struct (CLI-side)
5. Add `RoleBinding` struct with signature_proof + role_binding_hash
6. Add `RoleError` thiserror enum (4 variants; no RoleBindingConflict per RFC §Security 2)
7. Add `RoleAction` enum with `#[non_exhaustive]`
8. Wire derives (Serialize, Deserialize)
9. Add round-trip unit test for `RoleAction::Select`

## Test Vectors

N/A — type scaffolding mission. Type-level tests (TV-RX-1, TV-RX-2, TV-RX-3, TV-RX-4) land in M8 with full CLI integration.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-role` (Layer B; per RFC-0011-d §7.4) — owns the types. No reverse deps.
- Typed discriminator (`role_kind_uuid: [u8; 16]`) prevents central enum — extension-friendly per [[cipherocto-design-principles]].

## Backward compat

Additive: new types. No existing crates modified. No `schema_version` bumps. No exit codes added (substrate errors map to CLI errors in M7).

## Cross-references

- RFC-0011-d §7.4 (substrate types section)
- RFC-0011-d §7.5 (Role Summary canonical mapping)
- RFC-0900 §Slashing Model (SlashingRule field schema)
- RFC-0855 §Mission Overlay Networks (UUIDv5 namespace)
- [[cipherocto-design-principles]] — no central enum for extension-bearing types

## Notes

- Type scaffolding mission — NO entrypoint impls (those land in M3 + M4)
- `RoleAction` enum is substrate ACTION VOCABULARY (CLI-side dispatch), NOT a role taxonomy; role taxonomy uses typed UUID discriminator per [[cipherocto-design-principles]] no-central-enum
- `#[non_exhaustive]` on `RoleAction` per F-14 preserves upgrade path
- `RoleError` substrate errors map to `OctoCliError` in M7 (unidirectional mapping per [[cipherocto-design-principles]] no-premature-coupling)
- Per audit 2026-09-01: mission YAML bookkeeping lag (file not moved + ACs not flipped) addressed at mission close-out via this revision

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01 → Closed 2026-09-01)