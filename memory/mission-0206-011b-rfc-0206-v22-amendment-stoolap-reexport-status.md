---
name: 0206-011b-rfc-0206-v22-amendment-stoolap-reexport
description: "OPEN 2026-08-20 v1.0; Phase 1.9 sweep blocker. RFC-0206 v2.1→v2.2 amendment: substrate adds `pub mod stoolap` re-export block (ResultRow + ApiTransaction + Rows + Error). Resolves D1 deviation in 0206-002 v3.0 + 0206-008 audits. Unblocks 0206-008b consumer dep drop (13 sites → 5 sites = Layer A pin only). Layer A semver-major. Push awaits user instruction per feedback_initiation_user_only."
metadata:
  node_type: memory
  type: project
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  modified: 2026-08-20T16:58:39.085Z
---

# Mission `0206-011b-rfc-0206-v22-amendment-stoolap-reexport` v1.0 — OPEN 2026-08-20

## Phase 1.9 sweep result

TV-0206-A9(b) FAILS at 13 vs ≤ 5 stoolap deps workspace-wide. Plan §Verification gate
`rg -l '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l` ≤ 5 is the load-bearing constraint.

All other TV-0206-A1..A14 gates PASS with RFC v2.1-corrected interpretation.

## Root cause

0206-002 v3.0 (commit `2e25668b`) + 0206-008 (commit `927008d6`) both landed with explicit **D1 deviation** documented in their audits (`docs/audits/0206-002-layer-b-type-renames-audit.md` line 64-66 + `docs/audits/0206-008-layer-b-type-renames-expansion-audit.md` line 101-114):

> "stoolap direct dep RETAINED in consumer crates — substrate v3.0 does NOT yet re-export `stoolap::ResultRow` / `stoolap::ApiTransaction` / `stoolap::Rows` / `stoolap::Error`. Consumer crates need direct dep because they decode rows returned by `Database::execute_checked`."

The substrate IS the abstraction layer per RFC-0206 v2.1 §Substrate Newtype Refactor — consumers should NEVER reach for `stoolap::ResultRow` directly. Missed surface = missed RFC requirement.

## Resolution path (3 missions)

1. **0206-011b** (this mission) — RFC v2.1 → v2.2 amendment: add §Substrate Re-export Block section
2. **0206-001 v3.0b** — substrate adds `pub mod stoolap { ... }` re-export block + Layer A semver-major version bump (1.0.0 → 2.0.0)
3. **0206-008b** — 13 consumer crates drop direct `stoolap` dep (replace `use stoolap::ResultRow` with `use octo_storage_core::stoolap::ResultRow`)

## TV-0206-A9(b) final-state projection

After 0206-008b lands:
- `crates/octo-storage-core/Cargo.toml` (Layer A substrate — MUST keep) = 1
- 4 Layer A internal allowlisted pins (adapter crate optional-feature gates per 0206-002 v3.0 + 0206-008 audit structure) = 4
- **Total = 5** ≤ 5 → PASS

## Scope (RFC only this mission)

RFC-0206 v2.1 → v2.2 changes:
1. Add §Substrate Re-export Block section: `pub mod stoolap { ResultRow / ApiTransaction / Rows / Error }`
2. Update §Cargo.toml Templates Layer A: add `pub mod stoolap` to the table (8 top-level `pub use` cap UNCHANGED)
3. Update §RFC Process Audit Condition 2: "Layer B consumer crates drop direct `stoolap` dep" → projected PASS after 0206-008b
4. Add §Migration Order follow-on: v2.2 re-export block is prerequisite for 0206-008b dep drop
5. Version bump v2.1 → v2.2 (Layer A semver-major)

## AC gates

| Gate | Status | Evidence |
|------|--------|----------|
| Mission file filed | PASS | `missions/open/0206-011b-rfc-0206-v22-amendment-stoolap-reexport.md` (351 LOC) |
| RFC body updated | DEFER to 0206-011b impl | RFC body change TBD in 0206-011b implementation |
| Substrate re-export block | DEFER to 0206-001 v3.0b | `pub mod stoolap` declaration TBD |
| 13 consumer crates drop dep | DEFER to 0206-008b | TBD |
| TV-0206-A9(b) ≤ 5 | DEFER to 0206-008b | 13 sites → 5 sites projection |

## Out of scope (deferred)

- 0206-001 v3.0b substrate re-export block implementation (separate mission)
- 0206-008b 13 consumer crates dep drop (separate mission)
- New typed query DSL (Phase 2 work per RFC-0206 v2.1 §Implementation Phases 2.1)
- Push to remote — awaits user instruction per `feedback_initiation_user_only`

## Next in DAG

`0206-001 v3.0b` — substrate re-export block + version bump. Then `0206-008b` — consumer dep drop. Then Phase 1.9 terminal sweep redo.

## Why this works

The 0206-001 v3.0 substrate redesign chose the minimal-surface implementation (8 top-level `pub use` + `pub mod migrations`) and documented 6 `_legacy_*` aliases for the transition window. D1 deviation was foreseeable but deferred to `0206-011b` rather than including in the v2.1 RFC amendment. The substrate v3.0 is **architecturally correct** — it just has a missing re-export block that the v2.2 RFC amendment formally cites. This is the same Layer A pattern as RFC-0206 v2.1 itself resolving the v2.0 internal contradiction (≥ 8 vs ≤ 11 pub use cap).
