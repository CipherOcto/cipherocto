---
name: 0011-d-M2-octorole-types-and-errors
description: Add `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleFilter`, `RoleBinding`, `RoleError` substrate types per RFC-0011-d §7.4; `#[non_exhaustive]` on `RoleAction` per F-14.
metadata:
  node_type: substrate-cli
  type: substrate-types
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M1-octorole-crate-skeleton
status: Claimed
---

# 0011-d-M2-octorole-types-and-errors — Substrate types + errors per RFC-0011-d §7.4

**Status:** Claimed 2026-09-01 by @mmacedoeu — unblocked.
**Substrate:** RFC-0011-d §7.4 substrate types
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M1-octorole-crate-skeleton`

## Status

Claimed (2026-09-01) by @mmacedoeu. Second of 9 Phase 1 atomic missions. Lands the substrate type scaffolding that M3 (`list/show`) and M4 (`select`) consume.

## Substrate (RFC-0011-d)

RFC-0011-d §7.4 substrate types: `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleFilter`, `RoleBinding`, `RoleError`. Plus `RoleAction` enum (CLI-side dispatch) with `#[non_exhaustive]` per F-14.

## Parent

RFC-0011-d §7.4 (substrate types section); §7.5 (Role Summary canonical mapping); F-14 finding (W2 R2).

## Depends on

`0011-d-M1-octorole-crate-skeleton` (skeleton must exist before types can be added). Hard sequence: M1 → M2.

## Acceptance Criteria

- [ ] `RoleSummary` struct added to `crates/octo-role/src/types.rs` per §7.5 fields
- `RoleRecord` struct added (full record; includes `slashing_rules`, `allowed_actions`, `registry_ref`)
- `SlashingRule` struct added (per-rule entry per RFC-0900 §Slashing Model: `reason_code`, `description`, `penalty_pct_micro`, `escalation_multiplier_micro`)
- `RoleFilter` struct added (CLI-side parsing shape; fields: `kind`, `class`, `requires_octo_min`)
- `RoleBinding` struct added (persisted to wallet store on `select`; includes `signature_proof`, `role_binding_hash`)
- `RoleError` enum added (typed substrate error; variants: `RoleNotFound { role_id }`, `StakeInsufficient { required, available }`, `RoleNotSelectable { role_id, reason }`, `SignerMismatch { signer_did, operator_did }`) — NO `RoleBindingConflict` variant per RFC-0011-d §Security 2 (last-writer-wins semantics; substrate enforces atomically inside envelope-build step)
- `RoleAction` enum added (CLI-side dispatch; variants: `List`, `Show`, `Select`); `#[non_exhaustive]` attribute applied per F-14
- All types derive `Serialize` + `Deserialize` + `schemars::JsonSchema` (CLI surfaces require JSON Schema export per RFC-0011-d §Key Files row "JSON Schema export")
- `RoleSummary.role_kind_uuid: [u8; 16]` field (RFC-0855-namespaced UUIDv5 typed discriminator; NOT central enum per [[cipherocto-design-principles]])
- `RoleBinding.signature_proof: RedactedHex` (CLI redaction boundary; substrate owns the bytes)
- `RoleBinding.role_binding_hash: Hex32` (public material; BLAKE3-256 digest)
- `RoleAction::Select` round-trip test passes (`cargo test -p octo-role role_action_select_roundtrip`)
- `cargo check -p octo-role` zero warnings
- `cargo clippy -p octo-role --all-targets -- -D warnings` clean

## Scope

Type scaffolding only. NO entrypoint implementations (M3, M4). NO wallet persistence (M5). NO CLI binding (M6).

## Sub-steps

1. Add `RoleSummary` struct to `types.rs` with role_kind_uuid, name, ticker, stake fields per §7.5
2. Add `RoleRecord` struct extending RoleSummary with slashing_rules + allowed_actions + registry_ref
3. Add `SlashingRule` struct per RFC-0900 §Slashing Model
4. Add `RoleFilter` struct (CLI-side)
5. Add `RoleBinding` struct with signature_proof + role_binding_hash
6. Add `RoleError` thiserror enum
7. Add `RoleAction` enum with #[non_exhaustive]
8. Wire derives (Serialize, Deserialize, schemars::JsonSchema)
9. Add round-trip unit test for RoleAction::Select

## Test Vectors

N/A — type scaffolding mission. Type-level tests (TV-RX-1, TV-RX-2, TV-RX-3, TV-RX-4) land in M8 with full CLI integration. Unit tests for this mission: role_action_select_roundtrip + serde round-trip per type.

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
- RFC-0008 §Why a three-class taxonomy (execution class taxonomy; typed discriminator pattern parallels RFC-0008 §Classification assertion)
- [[cipherocto-design-principles]] — no central enum for extension-bearing types

## Notes

- Type scaffolding mission — NO entrypoint impls (those land in M3 + M4)
- `RoleAction` enum is a substrate ACTION VOCABULARY (CLI-side dispatch), NOT a role taxonomy; role taxonomy uses typed UUID discriminator (`role_kind_uuid: [u8; 16]`) per [[cipherocto-design-principles]] no-central-enum
- `#[non_exhaustive]` on `RoleAction` per F-14 preserves upgrade path (new CLI actions land without central enum edit)
- `RoleError` substrate errors map to `OctoCliError` in M7 (unidirectional mapping per [[cipherocto-design-principles]] no-premature-coupling)

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
