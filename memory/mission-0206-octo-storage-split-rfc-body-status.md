---
name: mission-0206-octo-storage-split-rfc-body-status
description: LANDED 2026-08-19; RFC-0206 octo-storage Substrate Split Draft v1.5 (S7 NEW RFC 2/2); three-tier (Layer A core + Layer B facade + per-owner adapter crates); R1 + R2 + R3 + R4 + R5 review fixes landed (R5 = process + correctness + security + TV completeness + future-work)
metadata:
  node_type: memory
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  modified: 2026-08-19T23:55:00.000Z
---

# Mission `0206-octo-storage-split-rfc-body` — LANDED 2026-08-19

## What

Filed S7 NEW RFC body for RFC-0206 (Storage): octo-storage Substrate
Split. Closes review §4.6.1 MED blocker; resolves §4.4 / §4.6 owner-crate
cycle risk.

## Substrate filed

- `rfcs/draft/storage/0206-octo-storage-split.md` (320+ lines)
- v1.0 → v1.1 (Round 1) → v1.2 (Round 2) → v1.3 (Round 3) → v1.4 (Round 4) review fixes
- **Round 4 coordination with RFC-0205 v1.4 mechanism rewrite:** RFC-0205's `[patch.crates-io]` story was inert for git-sourced deps; switched to direct `rev` pin in sole consumer. RFC-0206 v1.4 reflects: Layer A template rewritten to direct `rev` (with INERT-[patch.crates-io] correction comment), Layer B facade template clarifies 8 re-export vs 12 substrate distinction, Wiring Pattern `register(Arc<Database>)` → `register(Arc<octo_storage_core::Database>)` (re-exported handle), TV-0206-01 retargeted to direct `rev` pin + handle re-export verification, §Implicit Assumptions row 5 adds `crates/quota-router-storage/src/` exemption, §Implementation Phases Phase 1 Task 4 added quota-router-storage exemption note, Key Files to Modify added 3 missing adapter crate rows (reputation + cap-macaroon-vault + matrix-session), §Operation Class Mapping added Layer B fork-dep addition Class C row.
- Three-tier architecture: Layer A frozen core (`octo-storage-core`)
  - Layer B re-export facade (`octo-storage`) + per-owner adapter
    crates (`octo-cap-macaroon-storage` → HolderRegistry +
    `octo-ident-storage` → DidRegistry + `octo-policy-storage` →
    PolicyStore [conditional] + `octo-vault-storage` → VaultStore +
    `octo-reputation-storage` → ReputationStore +
    `octo-cap-macaroon-vault-storage` → VaultLookup +
    `octo-matrix-session-storage` → SessionStore; R3 extended from 4 to 7
    adapter crates; `octo-market-storage/` deferred per plan §4.2 B.4)
- §Cargo.toml Templates: aligned with current on-disk state
- §Wiring Pattern: `register(Arc<Database>) -> Arc<dyn OwnerTrait>`
- §Determinism Requirements (DQA wire form + Stoolap fork pin)
- §Operation Class Mapping (renamed from phantom "RFC-0008 Execution Class Mapping")
- §Implicit Assumptions Audit: 5 entries
- §Adversary Analysis: 4-row decision table
- §Test Vectors: 7 governance TV (TV-0206-06 grep pattern fixed R2:
  on-disk crate names `cipherocto-policy/`, no `octo-market/`; TV-0206-04
  - TV-0206-07 flagged as forward requirements)
- §Alternatives Considered: 4 options; Option D three-tier adopted
- §Implementation Phases: 3 phases (Phase 1 Tasks 1/2 + Phase 3 Task 8
  marked `[x]` LANDED; Phase 2 Task 6 fixed R2 to remove `octo-market-storage/`)
- §Promotion Path: 4 enumerated conditions + cross-ref note (R2 made
  the section self-contained vs external plan §S7 reference)
- §Key Files to Modify: 11 files (R2 removed `crates/octo-market-storage/`)
- §Future Work: `octo-storage-facade-versioning.md` +
  `octo-storage-core-deprecation.md` flagged `to be filed` (backticked R2)

## Commits

- `79df76b6` — feat(0206): RFC-0206 Draft v1.0
- `70aba493` — fix(0206): R1 fixes (phantom refs + Cargo.toml + API match)
- `afc96e5a` — fix(0206): R2 fixes (octo-market removal + TV-0206-06 grep +
  Version History v1.2 + Promotion Path self-contained + backticks)
- `eff55a2a` — fix(0206): R3 deep-dive fixes (7 adapter crates + grep
  word-boundary + curated re-export + clippy lint)

## Round 1 review fixes (9 defects)

| Severity | Defect                                                    | Fix                                                            |
| -------- | --------------------------------------------------------- | -------------------------------------------------------------- |
| CRIT     | Phantom `RFC-0914-a`                                      | → `RFC-0914 (Economics)`                                       |
| CRIT     | Phantom `RFC-0001`/`RFC-0008`                             | → `BLUEPRINT.md` ref + inline Operation Class Mapping          |
| CRIT     | octo-storage-core Cargo.toml template mismatch            | Template aligned + post-RFC-0205 migration path noted          |
| HIGH     | `MigrationsHandle::apply_pending` phantom type            | → `octo_storage_core::apply_pending` free fn                   |
| MED      | Dependency Validation Rules inconsistent                  | → "Required RFCs at minimum Draft" + per-RFC status enumerated |
| MED      | Implementation Phases checkboxes stale                    | → Phase 1 Tasks 1/2 + Phase 3 Task 8 `[x]`                     |
| MED      | Adapter Crate List §ref mismatch                          | → §Adapter Crate List (Initial) everywhere                     |
| MED      | Layer self-declaration missing                            | → added to Status                                              |
| MED      | Roles Source/Ref vague                                    | → precise §names                                               |
| MED      | Future Work phantom pointers                              | → marked `to be filed`                                         |
| MED      | Backward compat claim misleading                          | → clarified per workspace audit                                |
| MED      | TV-0206-04/07 untestable today                            | → flagged as forward requirements                              |
| MED      | `cipherocto-policy/` vs `octo-policy-storage/` naming gap | → flagged in §Future Work                                      |

## Round 2 review fixes (CRIT regressions + MED)

| Severity | Defect                                                                                                                                           | Fix                                                                                                                                                                                        |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| CRIT     | R1 reviewer claimed `§4.6.1` was phantom; reviewer was WRONG — review doc has `§4.6.1 octo-storage layer assignment (MED blocker)` at line 1732  | Reverted §4.6 → §4.6.1 in Summary + Motivation (3 sites); documented in Version History v1.2                                                                                               |
| CRIT     | R2 surfaced octo-market-storage phantom exposure (workspace has no `crates/octo-market/`; plan §4.2 B.4 defers octo-market primitive extraction) | Removed from Maintainers co-maintainer list + §Adapter Crate List + §Cargo.toml Templates Layer B facade "NOT a dep" list + Phase 2 Task 6 + §Key Files Modified; deferred rationale added |
| CRIT     | TV-0206-06 grep pattern wrong (`market` nonexistent + `policy` → `cipherocto-policy/`)                                                           | Pattern: `crates/octo-{ident,cap-macaroon}/src/ crates/cipherocto-policy/src/ crates/octo-vault/src/`                                                                                      |
| MED      | Version History v1.1 self-contradictory ("phantom §4.6.1 removed")                                                                               | v1.2 row added with correct narrative                                                                                                                                                      |
| MED      | "as a published crate" wording                                                                                                                   | → "as a workspace pin / `[patch.crates-io]` entry"                                                                                                                                         |
| MED      | `RFC-0206 v1.0` version pin in §Compatibility                                                                                                    | → "at RFC-0206 drafting time" (Version History is sole version-pin site)                                                                                                                   |
| MED      | `**to be filed**` bolded                                                                                                                         | → backticked                                                                                                                                                                               |
| MED      | §Promotion Path cited plan §S7 as source of truth                                                                                                | → 4 conditions enumerated inline; plan ref demoted to cross-ref note                                                                                                                       |

## Round 3 review fixes (deep-dive CRITICAL + 6 HIGH/MED)

| Severity | Defect                                                                                       | Fix                                                                                                                                                                                                                                                           |
| -------- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CRIT     | TV-0206-06 grep pattern: `use stoolap::` missed bare references + omitted 3 substrate crates | Pattern rewritten to `rg '\bstoolap::' crates/octo-{ident,cap-macaroon,reputation,cap-macaroon-vault,matrix-session-store}/src/ crates/cipherocto-policy/src/ crates/octo-vault/src/` (word-boundary + 7-crate list; verified via `rg '\bstoolap::' crates/`) |
| CRIT     | §Adapter Crate List (Initial) only had 4 owner crates                                        | Extended to 7: `octo-reputation-storage` → impl `ReputationStore`, `octo-cap-macaroon-vault-storage` → impl `VaultLookup`, `octo-matrix-session-storage` → impl `SessionStore`                                                                                |
| HIGH     | §Three-Tier Owner crates edge label had 4 crates                                             | Updated to 7 (matches adapter coverage)                                                                                                                                                                                                                       |
| HIGH     | §Roles Authorities row 3 trait list had 6 (incl. OrderBookStore, EscrowStore)                | Updated to 7: HolderRegistry, DidRegistry, PolicyStore, VaultStore, ReputationStore, VaultLookup, SessionStore                                                                                                                                                |
| HIGH     | §Cargo.toml Templates Layer B facade had no curated re-export policy                         | Added "12 substrate types at current audit; not wildcard `*`; new substrate types require RFC"                                                                                                                                                                |
| HIGH     | §Wiring Pattern didn't address migration-file location split                                 | Added: SQL files in `crates/<owner>/migrations/*.sql`; Rust runner in adapter crate — precondition for TV-0206-06                                                                                                                                             |
| HIGH     | §Implicit Assumptions row 5 missing                                                          | Added: `octo_storage_no_direct_stoolap` Clippy lint + CI grep step (forward requirement; Phase 1 Task 4)                                                                                                                                                      |
| MED      | §Implementation Phases Phase 1 Task 4 description vague                                      | Expanded with the 7-crate lint scope                                                                                                                                                                                                                          |
| MED      | §Adversary Row 4 (Adapter registry) Q5 Residual Risk capped at "facade surface narrow"       | Expanded with curated re-export cap (12 substrate types per §Cargo.toml Templates)                                                                                                                                                                            |

## S7 NEW RFC closure

Both S7 NEW RFC bodies filed (2/2):

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` — LANDED
- `rfcs/draft/storage/0206-octo-storage-split.md` — LANDED

S7 NEW RFC gap = **CLOSED**.

## Verification

- Prettier + cargo fmt --all applied (post-edit + post-review-fix)
- §4.6.1 cited 4× (Summary + Motivation + Three-Tier + Alternatives)
- 7 governance TV (TV-0206-01..07)
- 4 options in Alternatives (Option D adopted)
- `cargo clippy --all-targets --features full -- -D warnings` clean
- 4 R1 + 4 R2 reviewers; loop DRY pending Round 3

## Round 4 review fixes (coord with RFC-0205 v1.4 — 1 CRIT + 6 HIGH + 7 MED + 2 LOW)

| Severity | Defect                                                                                                                        | Fix                                                                                                                                                                                                                                                                                         |
| -------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CRIT     | §Cargo.toml Templates Layer A still showed `[patch.crates-io]` redirect (INERT for git-sourced)                               | Template rewritten to direct `rev = "<sha-0>"` pin in sole consumer `crates/octo-storage-core/Cargo.toml`; comment block updated with INERT-[patch.crates-io] correction note + required `pub use stoolap::Database;` in `crates/octo-storage-core/src/lib.rs` (handle re-export invariant) |
| HIGH     | §Wiring Pattern `register(Arc<Database>)` used bare fork handle (would force every adapter crate to add direct `stoolap` dep) | → `register(Arc<octo_storage_core::Database>)` (re-exported handle); migration-file location split updated to reference re-exported handle                                                                                                                                                  |
| HIGH     | §Dependencies RFC-0205 description still named `[patch.crates-io]` mechanism (stale post-RFC-0205 v1.4 rewrite)               | Rewritten to "Layer A freeze via direct `rev` pin in sole consumer + handle re-export (RFC-0205 v1.4 §Two-Tier Architecture)"                                                                                                                                                               |
| HIGH     | §Three-Tier Architecture Mermaid Core node said `[patch.crates-io]` redirect                                                  | Updated to `Cargo.toml: stoolap rev equals sha-0` + re-exports handle as `octo_storage_core::Database`                                                                                                                                                                                      |
| HIGH     | §Determinism Requirements row 1 said `octo-stoolap-frozen`                                                                    | → "frozen rev pin per RFC-0205 v1.4"                                                                                                                                                                                                                                                        |
| HIGH     | §Test Vectors TV-0206-01 asserted `[patch.crates-io]` redirect                                                                | Retargeted to direct `rev` pin verification (`grep -r "stoolap" crates/*/Cargo.toml` shows exactly 1 site at `crates/octo-storage-core/Cargo.toml` with `rev =` not `branch =`) + handle re-export verification (`grep "pub use stoolap::Database" crates/octo-storage-core/src/lib.rs`)    |
| HIGH     | §Operation Class Mapping omitted "Layer B crate adds direct fork dep"                                                         | Added Class C row (caught by TV-0206-06 grep + RFC-0205 TV-0205-04 resolve-graph query)                                                                                                                                                                                                     |
| HIGH     | §Key Files to Modify missed 3 adapter crates (reputation + cap-macaroon-vault + matrix-session)                               | 3 rows added; total now 14 adapter rows + 4 substrate rows + 2 facade rows                                                                                                                                                                                                                  |
| MED      | §Cargo.toml Templates Layer B facade "12 substrate types" framing conflated 8 re-exported vs 12 total                         | Clarified: 12 substrate types in `octo-storage-core`; 8 re-exported by `octo-storage` (facade); 4 tracker functions are internal helpers (not re-exported); not wildcard `*`; new substrate types require RFC                                                                               |
| MED      | §Cargo.toml Templates Layer B facade `quota-router-storage` listed alongside adapter crates (wrong scope)                     | Moved to "NOT a dep" exemption block: `quota-router-storage` is sibling Layer B substrate for `quota-router-core` domain; carries direct `stoolap` dep; same Layer B rules but separate facade                                                                                              |
| MED      | §Cargo.toml Templates Layer B facade `stoolap` not listed in "NOT a dep"                                                      | Added: `stoolap` (re-exported handle via `octo_storage_core::Database`)                                                                                                                                                                                                                     |
| MED      | §Implicit Assumptions row 5 said "no direct `stoolap` dep" without listing the exemption                                      | Added `crates/quota-router-storage/src/` exemption note                                                                                                                                                                                                                                     |
| MED      | §Adversary Row 4 (Adapter registry) Q5 cited "12 substrate types" without distinguishing 8 re-exported vs 12 total            | Rephrased: "12 substrate types in `octo-storage-core`, 8 re-exported by `octo-storage` facade; 4 tracker functions internal to core"                                                                                                                                                        |
| MED      | §Implementation Phases Phase 1 Task 4 "remove direct stoolap deps from owner crates" missed `quota-router-storage`            | Added exemption note: exempt `quota-router-storage` (sibling Layer B substrate; same dep-allowed status)                                                                                                                                                                                    |
| MED      | §Implementation Phases Phase 1 Task 3 + Task 4 said `[patch.crates-io]` mechanism (stale)                                     | Rephrased for direct `rev` pin + handle re-export                                                                                                                                                                                                                                           |
| MED      | §Promotion Path Condition 1 said `[patch.crates-io]` mechanism (stale)                                                        | Rephrased: "RFC-0205 v1.4 mechanism in place (sole `octo-storage-core` consumer pins `rev = "<sha-0>"`; `pub use stoolap::Database` re-export; Layer B has no direct `stoolap` dep)"                                                                                                        |
| LOW      | §Compatibility Backward/Forward referenced "RFC-0205 v1.0 mechanism"                                                          | → "RFC-0205 v1.4 mechanism"                                                                                                                                                                                                                                                                 |
| LOW      | §Future Work `stoolap-drop-fork-migration.md` cross-ref still cited RFC-0205 v1.0                                             | → "RFC-0205 v1.4 §Future Work"                                                                                                                                                                                                                                                              |

## Out of scope

- `octo-storage-core` Cargo.toml freeze-pin migration to `octo-stoolap-frozen`
  (Phase 1 Task 3 — gated on RFC-0205 Phase 1 Task 1 freeze)
- Per-owner adapter crate creation (Phase 2 Tasks 5-7)
- Facade re-export wiring (Phase 3 Tasks 9-10)
- 90-day migration window for owner crates to remove direct `stoolap` deps
- `octo-market-storage/` adapter (deferred per plan §4.2 B.4)

## Round 5 review fixes (process + correctness + security + TV completeness + future-work — 5 reviewers)

| Severity | Defect                                                                                                          | Fix                                                                                                                                                                                                                                                                                                                          |
| -------- | --------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CRIT     | §Cargo.toml Templates Layer A missing atomicity clause (Cargo.toml `branch → rev` + `pub use` MUST land together) | Added atomicity clause in Layer A template comment: half-state = E0308 mismatch waiting to happen; CI gate TV-0206-01 + Phase 1 Task 4b workspace `cargo build` verification enforces atomicity                                                                                                                                  |
| CRIT     | §Implicit Assumptions row 5 referenced `octo_storage_no_direct_stoolap` Clippy lint but never defined where     | Added lint definition: registered in workspace `clippy.toml` via `declare_clippy_lint!` + `register_clippy_lint!`; the lint scope (the rg pattern) is the same as TV-0206-06 — lint = in-process enforcement of grep CI gate                                                                                                                                  |
| HIGH     | Typo: `octo-matrix-session-storage` (10 sites) but on-disk = `octo-matrix-session-store`                          | All 10 sites renamed: Maintainers, Mermaid Block 1, Adapter Crate List, Cargo.toml Templates Layer A + Layer B facade, Implicit Assumptions row 5, TV-0206-06, Phase 1 Task 4, Promotion Path Condition 2, Key Files to Modify, v1.3 row text                                                                                                                                  |
| HIGH     | §Cargo.toml Templates Layer A + Layer B lacked "current vs target" tables                                         | Added before/after tables showing all field changes for both manifests (Layer A: source/pin/thiserror/lib.rs; Layer B: octo-storage-core dep/adapter deps/pub use count/stoolap dep)                                                                                                                                          |
| HIGH     | §Cargo.toml Templates Layer B facade missing 8-pub-use-cap CI step                                                | Added CI cap: `rg '^\s*pub use\b' crates/octo-storage/src/lib.rs | wc -l` MUST equal 8; gates TV-0206-02 + TV-0206-03; new substrate type requires RFC                                                                                                                                                                                                                  |
| HIGH     | §Wiring Pattern missing trait orphan-rule gap explanation                                                         | Added: Rust orphan rule prevents adapter crate from impl'ing foreign trait in foreign crate; adapter crate MUST `use`/re-export trait from owner crate; `register(Arc<Database>) -> Arc<dyn OwnerTrait>` returns trait object whose trait is owner-trait; §Cargo.toml Templates Per-owner adapter template is canonical        |
| HIGH     | 15 `RFC-0205 v1.4` prose pins outside Version History (process violation per CLAUDE.md rule)                       | Dropped all 15 via awk filter (kept Version History row intact); Version History is sole version-pin site                                                                                                                                                                                                                      |
| MED      | §Dependencies had `(Economics)`/`(Storage)`/`(Numeric)`/`(Process)` category suffixes                              | All category suffixes dropped                                                                                                                                                                                                                                                                                                  |
| MED      | §Roles row 3 HolderRegistry line ref `crates/quota-router-storage/src/holder_registry.rs:33` (line-ref violation) | Dropped line ref; replaced with §section-name convention                                                                                                                                                                                                                                                                        |
| MED      | §Roles row 3 trait list said "5 NEW traits"; 3 actually exist on disk                                            | Corrected: `HolderRegistry` + `DidRegistry` + `VaultLookup` + `ReputationStore` + `SessionStore` ALREADY exist (per `docs/audits/octo-storage-trait-surface-2026-08-19.md`); only `PolicyStore` + `VaultStore` are NEW                                                                                                                                  |
| MED      | TV-0206-06 didn't capture migration status (3 of 7 already migrated)                                              | Added migration status note: 3 of 7 owner crates already migrated (`octo-ident`, `octo-cap-macaroon`, `cipherocto-policy`); 4 still carry direct `stoolap` deps (`octo-reputation`, `octo-cap-macaroon-vault`, `octo-matrix-session-store`, `octo-vault`)                                                                                                                                  |
| MED      | §Security Considerations missing DROP TABLE defense                                                                | Added: CI greps `rg '\bDROP\s+TABLE\b' crates/*/migrations/*.sql`; any match MUST be reviewed against the adapter's ownership table; cross-adapter destroy rejected                                                                                                                                                            |
| MED      | §Cargo.toml Templates Layer A missing `<sha-0>` resolution ordering                                               | Added note: `<sha-0>` = HEAD of upstream `feat/blockchain-sql` AT FREEZE TIME; freeze tag `octo-stoolap-frozen-vN` byte-equal to `git rev-parse <tag>`; subsequent rev bumps advance `<sha-N>` to new freeze-tag SHA; Layer B re-compiles via re-export without direct dep edits                                                                                                                                  |
| MED      | §Cargo.toml Templates Layer A missing mutable-branch-pin HIGH-severity risk pre-Phase-1-Task-2                     | Added HIGH-severity risk note: until Phase 1 Task 2 lands (transition to direct `rev` pin), the manifest carries `branch = "feat/blockchain-sql"` — mutable pointer; every new commit to `feat/blockchain-sql` silently picked up                                                                                                                                  |
| LOW      | Mermaid Block 2 singular `OwnerTrait` node (each owner crate defines ONE trait)                                  | → `OwnerTraits` (plural)                                                                                                                                                                                                                                                                                                       |
| LOW      | §Cargo.toml Templates Layer A missing re-exported set enumeration (Database, Value, Error)                         | Added explicit list: Database, Value, Error + curated facade set per Layer B; adding NEW substrate type (anything from `stoolap::*` beyond current set) requires RFC                                                                                                                                                                                                                  |
| LOW      | §Promotion Path Condition 1 missing blocked-on note                                                                | Added: "Blocked-on: RFC-0206 promotion to Accepted is BLOCKED until RFC-0205 reaches Accepted; coupled pair in reviewer queue; while RFC-0205 is Draft, RFC-0206 stays Draft"                                                                                                                                                |
| LOW      | §Out of Scope section missing                                                                                       | New §Out of Scope: enumerated Phase 1 Tasks 3/4 + Phase 2 Tasks 5/6/7 + Phase 3 Tasks 9/10 + `octo-market-storage/` deferral + naming-resolution RFC + all 3 Future-Work sub-missions                                                                                                                                        |
| TV       | TV-0206-01..07 coverage gaps                                                                                       | Added 8 new TVs (TV-0206-A1..A8): handle re-export exact-once / 8-pub-use cap / no-direct-stoolap-dep in facade Cargo.toml / `rev =` not `branch =` in core Cargo.toml / DROP TABLE grep / migration status audit (3 of 7 already migrated) / substrate-zero-domain-knowledge / adapter dep count = 3                                                                                                                                  |

Commit: `13409b44` — fix(0206): R5 review fixes (process + correctness + security + TV + future-work)

## Related

- [[mission-0205-stoolap-fork-stability-rfc-body-status]] — sibling S7 NEW RFC 1/2
- [[storage-restructure-plan-audit-2026-08-19]] — plan §10 reconciliation
