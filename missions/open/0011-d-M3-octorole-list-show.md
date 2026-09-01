---
name: 0011-d-M3-octorole-list-show
description: Implement `octo_role::list(filter)` + `octo_role::show(role_id)` per RFC-0011-d §Mission Decomposition M3 row; registry read-side (no HSM); 7 Phase 1 role slugs registered (builder, provider, storage, bandwidth, orchestrator, recorder, wallet).
metadata:
  node_type: substrate-cli
  type: substrate-entrypoint-read
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M2-octorole-types-and-errors
status: Open
---

# 0011-d-M3-octorole-list-show — Read-side substrate entrypoints per RFC-0011-d §Mission Decomposition M3

**Status:** Open (2026-08-31) — unblocked.
**Substrate:** RFC-0011-d §Mission Decomposition M3 row; §7.4 Substrate `[ADD]` signatures — `octo_role::list` + `octo_role::show`
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M2-octorole-types-and-errors`

## Status

Open (2026-08-31). Third of 9 Phase 1 atomic missions. Lands the read-side substrate entrypoints that CLI `list` + `show` subcommands will call in M6. 7 Phase 1 role slugs registered: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`.

## Substrate (RFC-0011-d)

§Mission Decomposition M3 row (canonical):

- "Implement `octo_role::list(filter)` + `octo_role::show(role_id)` per §7.4; registry read-side (no HSM); Phase 1 role slugs (7) registered"

§7.4 Substrate `[ADD]` signatures:

- `pub fn list(filter: RoleFilter) -> Result<Vec<RoleSummary>, RoleError>` (reads)
- `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` (reads)

§7.5 Role Summary canonical mapping (registry-resolved name + ticker + stake).

## Parent

RFC-0011-d §Mission Decomposition M3 row; §7.4 Substrate `[ADD]` signatures; §7.5 Role Summary.

## Depends on

`0011-d-M2-octorole-types-and-errors` (types must exist before entrypoints can use them). Per RFC §Mission Decomposition M3 row: prereq = M2 only.

## Acceptance Criteria

- [ ] `pub fn list(filter: RoleFilter) -> Result<Vec<RoleSummary>, RoleError>` implemented in `crates/octo-role/src/lib.rs`
- Reads via Stoolap (`stoolap = { path = "../stoolap" }` workspace dep) per RFC-0960 vault asset registry pattern (general-purpose DB; cipherocto business schema forbidden per [[stoolap-general-purpose-db]])
- 7 Phase 1 role slugs registered in registry: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`
- `list()` (no filter) returns exactly 7 entries
- `list` returns empty Vec on no matches (NOT error)
- `list` returns Vec sorted by `role_summary.role_kind_uuid` ascending (deterministic per RFC-0104 DFP)
- `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` implemented
- `show` returns `RoleError::RoleNotFound { role_id }` on miss
- `show(role_id)` where `role_id` matches more than one registry entry returns `RoleError::RoleNotSelectable { role_id, reason: "ambiguous_role_id" }` (defensive error path; surfaces as M7 `RoleNotSelectable` variant exit 33)
- `show` returns full `RoleRecord` including `slashing_rules`, `allowed_actions`, `registry_ref`
- `list(filter.kind = None, filter.class = None, filter.requires_octo_min = None)` returns full 7-entry registry (no filter)
- `list(filter.kind = Some(RoleKind::Builder))` returns only builder roles (deterministic filter)
- Read-only path — no transaction, no write lock
- `cargo test -p octo-role list_returns_7_phase1_entries`
- `cargo test -p octo-role list_returns_sorted_by_uuid`
- `cargo test -p octo-role show_returns_full_role_record`
- `cargo test -p octo-role show_returns_role_not_found_on_miss`
- `cargo test -p octo-role show_returns_role_not_selectable_on_ambiguous`
- `cargo check -p octo-role` zero warnings
- `cargo clippy -p octo-role --all-targets -- -D warnings` clean

## Scope

Read-side only. NO `select` entrypoint (M4). NO wallet persistence (M5). NO CLI binding (M6).

## Sub-steps

1. Add `stoolap` dep to `crates/octo-role/Cargo.toml`
2. Add 7 Phase 1 role slugs to registry: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`
3. Add `pub fn list(filter: RoleFilter) -> Result<Vec<RoleSummary>, RoleError>` to lib.rs
4. Add `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` to lib.rs
5. Implement `RoleError::RoleNotFound` + `RoleError::RoleNotSelectable { reason: "ambiguous_role_id" }` variants (if not already in M2)
6. Wire Stoolap read query (registry-backed)
7. Wire deterministic sort by UUID
8. Wire filter (kind, class, requires_octo_min)
9. Wire full RoleRecord population in show path
10. Add 5 unit tests listed in Acceptance Criteria
11. Verify cargo check + clippy + fmt

## Test Vectors

Per RFC-0011-d §11:

- TV-RL-1: `list()` (no filter) returns 7 Phase 1 entries sorted by UUID
- TV-RL-2: `list(filter.kind = Builder)` returns only builder roles
- TV-RS-1: `show("builder")` returns full `RoleRecord` including `slashing_rules` + `allowed_actions`
- TV-RS-2: `show("builder") --with-slashing-rules` returns `RoleRecord` with `slashing_rules` field populated (assert field present)
- TV-RS-3: `show("nonexistent-role")` returns `RoleError::RoleNotFound` (surfaces as exit 31 per M7)
- TV-RL-3: `list(filter.kind = nonexistent)` returns empty Vec (NOT error)
- (defensive) `show(role_id)` with ambiguous match returns `RoleError::RoleNotSelectable { reason: "ambiguous_role_id" }` (surfaces as exit 33 per M7)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-role` (Layer B; per RFC-0011-d §7.4) — reads from Stoolap (general-purpose DB layer).
- Stoolap (Layer D adapter; per RFC-0010 + [[stoolap-fork-persistence]] + [[stoolap-general-purpose-db]]) — reads only, no cipherocto business schema stored there.

## Backward compat

Additive: new public functions. No existing substrate crates modified. No `schema_version` bump (registry schema already supports these tables).

## Risk

- **Ambiguous role_id pattern**: RFC-0855 namespacing uses UUIDv5 derived from role name; ambiguity should be impossible IF registry enforces uniqueness. Mitigation: surface `RoleNotSelectable { reason: "ambiguous_role_id" }` defensively; surfaces as M7 `RoleNotSelectable` variant exit 33.
- **Registry schema drift**: Stoolap schema must include `role_summary`, `role_record`, `slashing_rule` tables. Mitigation: schema migration owned by RFC-0011-d substrate accept; this mission lands queries against existing tables.
- **Determinism on concurrent writes**: Stoolap MVCC isolation ensures read snapshot is deterministic. Mitigation: use Stoolap `read_uncommitted` explicitly + assert in test.

## Notes

- Read-only — no nonce counter, no signature, no transaction
- 7 Phase 1 role slugs: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`
- Filter shape (kind, class, requires_octo_min) is canonical per RFC-0011-d §7.4; do not extend without RFC amendment
- Per RFC §Mission Decomp M3 row: prereq = M2 only (NOT RFC-0900 or RFC-0855p-c — those are M4 prereqs)

## Cross-references

- RFC-0011-d §Mission Decomposition M3 row (canonical scope)
- RFC-0011-d §7.4 Substrate `[ADD]` signatures (list + show)
- RFC-0011-d §7.5 Role Summary canonical mapping (7 Phase 1 role slugs)
- RFC-0960 §Vault Substrate (stoolap read pattern)
- RFC-0104 §Deterministic Floating-Point (deterministic sort)
- [[stoolap-general-purpose-db]] — fork constraints; cipherocto business schema forbidden
- [[stoolap-fork-persistence]] — stoolap fork pin
- [[cipherocto-design-principles]] — Layer B stability contract

## Claimant

@unassigned (mission lifecycle: Open)
