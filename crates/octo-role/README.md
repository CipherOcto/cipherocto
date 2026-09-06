# octo-role

CipherOcto role provisioning substrate — Layer C specialized node per RFC-0011-d §7.4 (v1.7 layer label fix).

This crate provides the canonical role substrate for the `octo role {list, show, select}`
CLI subcommands (RFC-0011-d Phase 1). It is a Layer C specialized node (specialized
identity-adjacent role substrate): substrate-typed 128-bit UUID discriminators per RFC-0855 namespace,
no central enums for extension-bearing types per
[[cipherocto-design-principles]].

## Public surface

- `octo_role::list(filter: RoleFilter) -> Result<Vec<RoleSummary>, RoleError>` —
  list roles with optional kind/class/`requires_octo_min` filter (M3).
- `octo_role::show(role_id: &str) -> Result<RoleRecord, RoleError>` — full role
  record including slashing rules + allowed actions + registry_ref (M3).
- `octo_role::select(role_id, operator_did, signer) -> Result<RoleBinding, RoleError>`
  — sync write-path with Stoolap `BEGIN IMMEDIATE` transaction + last-writer-wins
  on re-select (M4).
- `octo_role::default_store() -> BindingStore` — Phase 2 seam for substrate
  migration away from the in-process store.

## Layer model

- **Layer A** (RFC-frozen): `octo-cap-macaroon` (`CapabilitySigner`, `did_from_pubkey`,
  `blake3_hash`).
- **Layer A** (RFC-frozen): `octo-cap-macaroon` (`CapabilitySigner`, `did_from_pubkey`,
  `blake3_hash`).
- **Layer C** (specialized nodes): `octo-role` (this crate), `octo-wallet`
  (`next_nonce_counter`, `OctoRoleBinding` cached projection — M5).
- **Layer C** (CLI orchestrator): `octo-cli` `RoleAction` enum + `dispatch()` (M6).

## Cross-references

- RFC-0011-d §7.4 Substrate `[ADD]` signatures
- RFC-0011-d §7.5 Role Summary
- RFC-0900 §Slash Ledger Substrate
- RFC-0855 §Mission Overlay Networks (role namespace)
- RFC-0011 — `octo` CLI substrate (parent of RFC-0011-d)
