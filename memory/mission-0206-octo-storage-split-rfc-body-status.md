---
name: mission-0206-octo-storage-split-rfc-body-status
description: LANDED 2026-08-19; RFC-0206 octo-storage Substrate Split Draft v1.1 (S7 NEW RFC 2/2); three-tier (Layer A core + Layer B facade + per-owner adapter crates); 320+ lines; R1 review fixes landed
metadata:
  node_type: memory
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  modified: 2026-08-20T00:44:14.711Z
---

# Mission `0206-octo-storage-split-rfc-body` — LANDED 2026-08-19

## What

Filed S7 NEW RFC body for RFC-0206 (Storage): octo-storage Substrate
Split. Closes review §4.6 MED blocker (not §4.6.1 — phantom ref
removed in v1.1); resolves §4.4 / §4.6 owner-crate cycle risk.

## Substrate filed

- `rfcs/draft/storage/0206-octo-storage-split.md` (320+ lines)
- v1.0 → v1.1 Round 1 review fixes (commit `70aba493` 2026-08-19)
- Three-tier architecture: Layer A frozen core (`octo-storage-core`)
  + Layer B re-export facade (`octo-storage`) + 5 per-owner adapter
  crates (`octo-cap-macaroon-storage` + `octo-ident-storage` +
  `octo-policy-storage` + `octo-vault-storage` + `octo-market-storage`)
- §Cargo.toml Templates: aligned with current on-disk state
  (octo-storage-core uses `stoolap` branch + `thiserror`; facade is
  thin re-export of core; adapter wiring is Phase 3 future work)
- §Wiring Pattern: `register(Arc<Database>) -> Arc<dyn OwnerTrait>`
- §Determinism Requirements (DQA wire form + Stoolap fork pin)
- §Operation Class Mapping (renamed from "RFC-0008 Execution Class
  Mapping" — phantom RFC-0008 removed; uses `apply_pending` free fn
  per actual `crates/octo-storage-core/src/lib.rs`)
- §Implicit Assumptions Audit: 5 entries
- §Adversary Analysis: 4-row decision table
- §Test Vectors: 7 governance TV (TV-0206-04 + TV-0206-07 flagged as
  forward requirements gated on adapter crate landings)
- §Alternatives Considered: 4 options; Option D three-tier adopted
- §Implementation Phases: 3 phases (core / adapters / facade) with
  Phase 1 Tasks 1/2 + Phase 3 Task 8 marked [x] LANDED 2026-08-19
- §Promotion Path: NEW section with S7 termination conditions
- §Key Files to Modify: 11 files (LANDED/NEW status per file)
- §Future Work: `octo-storage-facade-versioning.md` + `octo-storage-core-deprecation.md`
  flagged **to be filed**; `cipherocto-policy/` vs `octo-policy-storage/`
  naming gap flagged for follow-on RFC

## Commits

- `79df76b6` — feat(0206): RFC-0206 Draft v1.0 (315 lines)
- `70aba493` — fix(0206): round 1 review fixes — phantom refs + Cargo.toml + API match

## Round 1 review fixes (9 defects)

| Severity | Defect | Fix |
| -------- | ------ | --- |
| CRIT | Phantom RFC-0914-a | → RFC-0914 (Economics) — real RFC at rfcs/accepted/economics/0914-stoolap-only-quota-router-persistence.md |
| CRIT | Phantom RFC-0001 / RFC-0008 | → BLUEPRINT.md ref + inline Operation Class Mapping |
| CRIT | octo-storage-core Cargo.toml template mismatch (claimed `octo-stoolap-frozen` rev; actual `stoolap` branch) | Template aligned with on-disk + noted post-RFC-0205 freeze migration path |
| HIGH | Phantom §4.6.1 | → §4.6 (review doc has §4.6 Layer A substrate separation; no §4.6.1 sub-section exists) |
| HIGH | Dependency Validation Rules "All upstream Accepted" listed RFC-0205 Draft inconsistently | → "Required RFCs at minimum Draft"; per-RFC status enumerated |
| MED | MigrationsHandle::apply_pending phantom type | → `octo_storage_core::apply_pending` (free fn per crates/octo-storage-core/src/lib.rs) |
| MED | Implementation Phases checkboxes stale | → Phase 1 Tasks 1/2 + Phase 3 Task 8 marked [x] LANDED 2026-08-19 |
| MED | Adapter Crate List section ref mismatch | → §Adapter Crate List (Initial) everywhere |
| MED | Layer self-declaration missing | → added **Layer:** B to Status block |
| MED | Roles table Source/Ref vague | → precise §Three-Tier Architecture / §Adapter Crate List (Initial) / §Promotion Path |
| MED | Future Work phantom mission pointers | → marked **to be filed** |
| MED | Backward compat claim misleading (no owner crates bypass octo-storage today) | → clarified per workspace audit finding |
| MED | TV-0206-04 / TV-0206-07 untestable today | → flagged as forward requirements gated on adapter crate landings |
| MED | `octo-policy-storage` vs `cipherocto-policy/` naming gap | → flagged in §Future Work for follow-on RFC |

## S7 NEW RFC closure

Both S7 NEW RFC bodies filed (2/2):

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` — LANDED
  2026-08-19 (`75868942` + `8d86835b` + `c782dec8`)
- `rfcs/draft/storage/0206-octo-storage-split.md` — LANDED
  2026-08-19 (`79df76b6` + `70aba493`)

S7 NEW RFC gap = **CLOSED**.

## Verification

- Prettier formatting applied (post-edit + post-review-fix)
- §4.6 cited 4× (Motivation + Three-Tier + Alternatives + Adversary)
- 7 governance TV (TV-0206-01..07)
- 4 options in Alternatives (Option D adopted)
- `cargo clippy --all-targets --features full -- -D warnings` clean
  (workspace-wide; no RFC-introduced warnings)
- Round 1 reviewers: 4 (correctness / cross-RFC / process-compliance) +
  1 (RFC-0206-specific); loop DRY pending Round 2

## Out of scope

- `octo-storage-core` Cargo.toml freeze-pin migration to `octo-stoolap-frozen`
  (Phase 1 Task 3 — gated on RFC-0205 Phase 1 Task 1 freeze)
- Per-owner adapter crate creation (Phase 2 Tasks 5-7)
- Facade re-export wiring (Phase 3 Tasks 9-10)
- 90-day migration window enforcement for owner crates to remove
  direct `stoolap` deps

## Related

- [[mission-0205-stoolap-fork-stability-rfc-body-status]] — sibling
  S7 NEW RFC landing (1/2)
- [[storage-restructure-plan-audit-2026-08-19]] — full plan audit
