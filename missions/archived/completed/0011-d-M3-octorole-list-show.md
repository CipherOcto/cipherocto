---
name: 0011-d-M3-octorole-list-show
description: Implement `octo_role::list(filter)` + `octo_role::show(role_id)` per RFC-0011-d §Mission Decomposition M3 row; registry read-side (no HSM); 7 Phase 1 role slugs registered (builder, provider, storage, bandwidth, orchestrator, recorder, wallet).
metadata:
  node_type: substrate-cli
  type: substrate-entrypoint-read
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  landing_commit: "91c42ab2"
  verified_by: "@mmacedoeu"
  review_commit: "f08d0ca9"
  closed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - mission 0011-d-M2-octorole-types-and-errors
status: Completed
---

# 0011-d-M3-octorole-list-show — Read-side substrate entrypoints per RFC-0011-d §Mission Decomposition M3

**Status:** Completed (2026-09-01). LANDED commit `91c42ab2` (feat: M3 list + show read paths) + review-loop substrate fixes `f08d0ca9`.

> **Retro-supersession (2026-09-01):** Mission landed in same substrate cycle as M1+M4. **SIGNIFICANT substrate-truth deviation**: mission text specified "Reads via Stoolap" (`stoolap = { path = "../stoolap" }` workspace dep) but actual landed substrate reads from an in-memory `registry::all_roles()` function — Stoolap dep NOT in `crates/octo-role/Cargo.toml` for this read path. Rationale: Phase 1 surfaces a static canonical 7-role registry for CLI UX; substrate persistence layer stays in slash-ledger (M4). This deviation documented inline below per M1 close-out pattern.

**Substrate:** RFC-0011-d §Mission Decomposition M3 row; §7.4 Substrate `[ADD]` signatures — `octo_role::list` + `octo_role::show`
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M2-octorole-types-and-errors`

## Status

Closed (2026-09-01). Substrate delivered: `pub fn list(filter: RoleFilter) -> Vec<RoleSummary>` + `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` in `crates/octo-role/src/list_show.rs`. 7 Phase 1 role slugs registered in `crates/octo-role/src/registry.rs`: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`. 6 unit tests pass.

## Substrate (RFC-0011-d)

§Mission Decomposition M3 row (canonical):

- "Implement `octo_role::list(filter)` + `octo_role::show(role_id)` per §7.4; registry read-side (no HSM); Phase 1 role slugs (7) registered"

§7.4 Substrate `[ADD]` signatures:

- `pub fn list(filter: RoleFilter) -> Result<Vec<RoleSummary>, RoleError>` (reads) — **deviation**: actual signature `Vec<RoleSummary>` (no Result wrapper; cannot fail for filter query; `show` carries the Result)
- `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` (reads)

§7.5 Role Summary canonical mapping (registry-resolved name + ticker + stake).

## Parent

RFC-0011-d §Mission Decomposition M3 row; §7.4 Substrate `[ADD]` signatures; §7.5 Role Summary.

## Depends on

`0011-d-M2-octorole-types-and-errors` (types must exist before entrypoints can use them). Per RFC §Mission Decomposition M3 row: prereq = M2 only.

## Acceptance Criteria

- [x] `pub fn list(filter: &RoleFilter) -> Vec<RoleSummary>` implemented in `crates/octo-role/src/list_show.rs` — **substrate-truth deviation**: signature is `Vec<RoleSummary>` (NOT `Result<Vec<RoleSummary>, RoleError>`); reads from in-memory `registry::all_roles()` (NOT Stoolap). Rationale: list query cannot fail in current shape; substrate persistence layer reserved for M4/M5.
- [x] **substrate-truth deviation**: Reads via in-memory `registry::all_roles()` (NOT Stoolap). Stoolap dep NOT in `crates/octo-role/Cargo.toml`. Rationale: Phase 1 registry is canonical static 7-role table; persistence flows through M4 substrate (slash-ledger authoritative) per F-NEW-3 split.
- [x] 7 Phase 1 role slugs registered in `crates/octo-role/src/registry.rs`: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`
- [x] `list()` (no filter) returns exactly 7 entries
- [x] `list` returns empty Vec on no matches (NOT error)
- [x] **substrate-truth deviation**: deterministic sort by `name` ascending (NOT `role_kind_uuid` per RFC §Acceptance Criteria); tests assert `name`-based ordering. Rationale: registry uses `name` as natural primary key; UUID-based sort is a follow-on.
- [x] `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` implemented
- [x] `show` returns `RoleError::RoleNotFound { role_id }` on miss
- [x] **substrate-truth deviation**: `show(role_id)` ambiguous match path NOT implemented (no separate `RoleNotSelectable { reason: "ambiguous_role_id" }` surface); registry enforces name uniqueness so ambiguity impossible in practice. Rationale: defensive check deferred; current registry has no ambiguity surface.
- [x] `show` returns full `RoleRecord` including `slashing_rules`, `allowed_actions`, `registry_ref`
- [x] `list(filter.kind = None, filter.class = None, filter.requires_octo_min = None)` returns full 7-entry registry (no filter)
- [x] `list(filter.kind = Some("builder"))` returns only builder roles (string-based filter per RoleKind enum)
- [x] Read-only path — no transaction, no write lock
- [x] `cargo test -p octo-role list_filter_by_min_stake` passes
- [x] `cargo test -p octo-role list_filter_no_match` passes
- [x] `cargo test -p octo-role list_filter_kind` passes
- [x] `cargo test -p octo-role list_all` passes (7 entries)
- [x] `cargo test -p octo-role show_existing` passes
- [x] `cargo test -p octo-role show_unknown_returns_role_not_found` passes
- [x] `cargo check -p octo-role` zero warnings
- [x] `cargo clippy -p octo-role --all-targets -- -D warnings` clean

## Scope

Read-side only. NO `select` entrypoint (M4). NO wallet persistence (M5). NO CLI binding (M6).

## Sub-steps

1. Add 7 Phase 1 role slugs to `crates/octo-role/src/registry.rs`: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`
2. Add `pub fn list(filter: &RoleFilter) -> Vec<RoleSummary>` to `crates/octo-role/src/list_show.rs`
3. Add `pub fn show(role_id: &str) -> Result<RoleRecord, RoleError>` to `crates/octo-role/src/list_show.rs`
4. Wire `RoleError::RoleNotFound` variant (defined in M2)
5. Wire deterministic in-memory registry reads
6. Wire filter (kind, class, requires_octo_min)
7. Wire full RoleRecord population in show path
8. Add 6 unit tests
9. Verify cargo check + clippy + fmt

## Test Vectors

Per RFC-0011-d §11:

- TV-RL-1: `list()` (no filter) returns 7 Phase 1 entries — **deviation**: sorted by `name` ascending (NOT UUID per RFC §Acceptance Criteria)
- TV-RL-2: `list(filter.kind = Builder)` returns only builder roles
- TV-RS-1: `show("builder")` returns full `RoleRecord` including `slashing_rules` + `allowed_actions`
- TV-RS-2: `show("builder") --with-slashing-rules` returns `RoleRecord` with `slashing_rules` field populated
- TV-RS-3: `show("nonexistent-role")` returns `RoleError::RoleNotFound` (surfaces as exit 31 per M7)
- TV-RL-3: `list(filter.kind = nonexistent)` returns empty Vec (NOT error)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-role` (Layer B; per RFC-0011-d §7.4) — reads from in-memory registry.
- Stoolap (Layer D adapter; per RFC-0010) — used by M4 substrate (slash-ledger persistence); NOT used by M3 read path per substrate-truth deviation.

## Backward compat

Additive: new public functions. No existing substrate crates modified. No `schema_version` bump.

## Cross-references

- RFC-0011-d §Mission Decomposition M3 row (canonical scope)
- RFC-0011-d §7.4 Substrate `[ADD]` signatures (list + show)
- RFC-0011-d §7.5 Role Summary canonical mapping (7 Phase 1 role slugs)
- RFC-0010 (stoolap substrate; consumed by M4)
- [[cipherocto-design-principles]] — Layer B stability contract

## Notes

- Read-only — no nonce counter, no signature, no transaction
- 7 Phase 1 role slugs: `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet`
- Per RFC §Mission Decomp M3 row: prereq = M2 only (NOT RFC-0900 or RFC-0855p-c — those are M4 prereqs)
- Substrate-truth deviations: in-memory registry (not Stoolap); sort by name (not UUID); no ambiguous-match defensive path. All documented per M1 close-out pattern.
- Per audit 2026-09-01: mission YAML bookkeeping lag addressed at mission close-out via this revision

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01 → Closed 2026-09-01)