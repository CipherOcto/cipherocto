---
name: mission-0205-stoolap-fork-stability-rfc-body-status
description: LANDED 2026-08-19; RFC-0205 Stoolap fork stability certification Draft v1.4 (S7 NEW RFC 1/2); Layer A freeze via direct rev pin in sole consumer + handle re-export; R1 + R2 + R3 + R4 review fixes landed (R4 = wholesale mechanism rewrite)
metadata:
  node_type: memory
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  modified: 2026-08-19T23:30:00.000Z
---

# Mission `0205-stoolap-fork-stability-rfc-body` — LANDED 2026-08-19

## What

Filed S7 NEW RFC body for RFC-0205 (Storage): Stoolap Fork Stability
Certification. Closes review §8.1.7 HIGH blocker.

## Substrate filed

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` (290+ lines, 32 sections)
- v1.0 → v1.1 (Round 1) → v1.2 (Round 2) → v1.3 (Round 3) → v1.4 (Round 4) review fixes
- **Round 4 mechanism wholesale rewrite:** the v1.3 `[patch.crates-io]` story was inert (cargo only rewrites deps resolved from the named source; the fork is consumed via git, not crates-io). R4 rewrites to direct `rev` pin in the SOLE consumer `crates/octo-storage-core/Cargo.toml` + handle re-export (`octo_storage_core::Database`) preventing two-package E0308 mismatch. Layer B crates carry NO direct `stoolap` dep (go through re-export; TV-0206-06 grep enforces)
- v1.4 single-tier Layer A freeze: `octo-storage-core` is the SOLE workspace crate
  that names `stoolap` types directly (pinned `rev = "<sha-0>"` in
  `crates/octo-storage-core/Cargo.toml`); re-exports `Database` handle so Layer B
  consumes via `octo_storage_core::Database` (no direct `stoolap` dep, no E0308).
- §Release-Tag Pin Policy: 6-row trigger/action/owner/SLA table; monthly
  trigger = 30 days from last freeze tag (deterministic sliding window);
  pre-v0 bootstrap = daily-watch on `feat/blockchain-sql` with 24 h RFC
  reviewer escalation (R2 added)
- §Operation Class Mapping (renamed from "RFC-0008 Execution Class Mapping")
- §Implicit Assumptions Audit: 4 entries (Q1 mitigation: HW key)
- §Adversary Analysis: 3-row decision table (Q4: out-of-band HW key)
- §Test Vectors: 6 governance TV (TV-0205-05: ancestor-of + tag-match
  `git rev-parse <tag> == <rev>` byte-equal — R2 added tag-match check
  to defend against force-push retargeting)
- §Alternatives Considered: 4 options; Option D (two-tier) adopted
- §Future Work: 2 phantom missions flagged `to be filed` (backticked R2)
- §Two-Tier Architecture: Mermaid graph TD with subgraph Layer A/B (renamed `Frozen` → `FrozenTier` R4 to avoid subgraph/node id collision; inner node renamed; square brackets escaped; `Determin -. MUST NOT .-> Frozen` edge added)
- §Key Files to Modify: runbook content checklist enumerated R2
  (pre-v0 daily-watch / freeze signing / CVE bypass audit-log format /
  merge-base + tag-match CI gate / quarterly review template)

## Commits

- `75868942` — feat(0205): RFC-0205 Draft v1.0
- `8d86835b` — chore(missions): drift-close mission YAML
- `c782dec8` — fix(0205): R1 fixes (phantom refs + line refs + Mermaid)
- `ba78fefe` — fix(0205): R2 fixes (bootstrap clause + tag-match check +
  runbook checklist + backticks)

## Round 1 review fixes (10 defects)

| Severity | Defect                                                       | Fix                                                   |
| -------- | ------------------------------------------------------------ | ----------------------------------------------------- |
| CRIT     | Memory claimed "8 ACs PASS" — governance RFC has no AC table | Corrected memory                                      |
| HIGH     | `Cargo.toml:156` line ref in prose                           | → `Cargo.toml` `[patch.crates-io]` block              |
| HIGH     | Phantom `RFC-0001`/`RFC-0008`                                | → `BLUEPRINT.md` ref + inline Operation Class Mapping |
| MED      | Roles Source/Ref vague                                       | → precise §names                                      |
| MED      | Two-Tier ASCII                                               | → Mermaid graph TD                                    |
| MED      | Monthly re-cert trigger ambiguous                            | → "30 days from last freeze tag"                      |
| MED      | TV-0205-05 "reachable from" undefined                        | → "ancestor of" + `git merge-base --is-ancestor`      |
| MED      | TV-0205-06 script phantom                                    | → marked NEW per Phase 1 Task 4                       |
| MED      | Adversary Q1 mitigation weak                                 | → out-of-band HW key held by separate person          |
| MED      | Layer self-declaration missing                               | → added to Status                                     |
| MED      | Future Work phantom pointers                                 | → marked `to be filed`                                |

## Round 2 review fixes (4 defects)

| Severity | Defect                                               | Fix                                                                                                                                              |
| -------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| MED      | Pre-v0 re-cert schedule undefined                    | → bootstrap clause: daily-watch on `feat/blockchain-sql` with 24 h RFC-reviewer escalation; SLA column split                                     |
| MED      | TV-0205-05 force-push retargeting bypass             | → tag-match check `git rev-parse <tag> == <rev>` byte-equal added alongside `merge-base --is-ancestor`                                           |
| MED      | `docs/runbooks/stoolap-steward.md` content undefined | → enumerated checklist (pre-v0 daily-watch / freeze signing / CVE bypass audit-log format / merge-base + tag-match CI gate / quarterly template) |
| MED      | Future Work `**to be filed**` bolded                 | → backticked `\`to be filed\``                                                                                                                   |

## Round 3 review fixes (deep-dive CRITICAL+HIGH+MED)

| Severity | Defect                                                                                                              | Fix                                                                                                                                                                                                                                                                                                                                                                                         |
| -------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CRIT     | Consumer misidentification: RFC-0205 v1.2 named `octo-determin` as sole Layer A consumer                            | Verified via `crates/octo-storage-core/Cargo.toml` (sole `stoolap` dep in tree) + `determin/Cargo.toml` (zero stoolap deps). Wholesale rewrite: §Summary, §Motivation, §Roles, §Two-Tier Architecture Mermaid, §Implicit Assumptions row 3, §Security Considerations Layer-A-drift row, §Operation Class Mapping row 1, §Compatibility Backward — all `octo-determin` → `octo-storage-core` |
| CRIT     | §Cargo.toml Pinning Layer A template used nonexistent `[dependencies.octo-stoolap-frozen]`                          | Verified fork repo `Cargo.toml` declares `name = "stoolap"` (upstream name); no `octo-stoolap-frozen` crate exists. Template rewritten to actual mechanism: workspace `[patch.crates-io] stoolap = { git = "...", rev = "<sha>" }` redirect                                                                                                                                                 |
| HIGH     | §Release-Tag Pin Policy missing CRITICAL CVE mid-cycle trigger                                                      | New row: ice-until-patch semantics; v{N+1} bump RFC + TV-0205-05 gate; `octo-storage-core` auto-picks up via `[patch.crates-io]` redirect; SLA 7 days                                                                                                                                                                                                                                       |
| HIGH     | §Adversary Row 1 Q4 didn't cite TV-0205-05 explicitly                                                               | Q4 Defense now cites TV-0205-05 (`git merge-base --is-ancestor` + `git rev-parse <tag> == <rev>` byte-equal)                                                                                                                                                                                                                                                                                |
| MED      | §Operation Class Mapping didn't restate the sole-consumer invariant                                                 | Row 1 updated: "Layer A substrate; years-stable; sole consumer is `octo-storage-core`"; new row 5 added for CRITICAL CVE mid-cycle bump (Class A)                                                                                                                                                                                                                                           |
| MED      | §Test Vectors §General rule + all 6 entries still referenced `octo-stoolap-frozen` crate + `octo-determin` consumer | Rewrote §General rule + TV-0205-01..05 to reference `[patch.crates-io]` redirect + `octo-storage-core` + backward slug path; TV-0205-02 references `crates/octo-storage-core/Cargo.toml`                                                                                                                                                                                                    |
| MED      | §Implementation Phases Phase 1 Task 2 said `octo-determin/Cargo.toml`                                               | Updated to "workspace `Cargo.toml` `[patch.crates-io]` block edit"                                                                                                                                                                                                                                                                                                                          |
| MED      | §Key Files to Modify referenced `crates/octo-determin/Cargo.toml` (nonexistent path)                                | Replaced with `crates/octo-storage-core/Cargo.toml` row (no change — documents why)                                                                                                                                                                                                                                                                                                         |
| MED      | §Compatibility Backward had post-RFC-0205 `octo-stoolap-frozen` (stale ref)                                         | Rewrote to "post-RFC-0205 workspace `[patch.crates-io]` frozen redirect (the upstream `stoolap` crate is patched to the fork rev at build time)"                                                                                                                                                                                                                                            |
| MED      | §Compatibility Forward didn't address drop-fork scenario                                                            | Added clause: Layer B branch pin → crates-io semver migration per §Future Work                                                                                                                                                                                                                                                                                                              |

## S7 NEW RFC closure

Both S7 NEW RFC bodies filed (2/2):

- `rfcs/draft/storage/0205-stoolap-fork-stability.md` — LANDED
- `rfcs/draft/storage/0206-octo-storage-split.md` — LANDED

S7 NEW RFC gap = **CLOSED**.

## Verification

- Prettier + cargo fmt --all applied
- Cross-references to review §8.1.7 verified at 4 sites
- `cargo clippy --all-targets --features full -- -D warnings` clean
- 4 R1 reviewers + 4 R2 reviewers; loop DRY pending Round 3

## Round 4 review fixes (mechanism wholesale rewrite — 4 CRIT + 9 HIGH + 11 MED + 5 LOW)

| Severity | Defect                                                                                                                                                                                          | Fix                                                                                                                                                                                                                                                                                                                                                                    |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CRIT     | `[patch.crates-io]` mechanism inert for git-sourced deps (cargo only rewrites deps resolved from named source; fork is git)                                                                     | Wholesale rewrite to direct `rev` pin in SOLE consumer `crates/octo-storage-core/Cargo.toml` + handle re-export `octo_storage_core::Database`. Workspace root `[patch.crates-io]` block removed (was dead config). Documented in §Two-Tier Architecture Note + §Cargo.toml Pinning Layer A.                                                                            |
| CRIT     | Two-tier split makes workspace uncompilable (distinct git sources don't unify → two `stoolap` packages → E0308 mismatch)                                                                        | Handle re-export `pub use stoolap::Database` in `crates/octo-storage-core/src/lib.rs`; Layer B consumes `octo_storage_core::Database` (re-exported type, same package instance, no E0308). New TV-0205-03: `cargo metadata` resolve-graph returns exactly ONE `stoolap` package id (gates the E0308 class). New Phase 1 Task 4b: workspace `cargo build` verification. |
| CRIT     | §Release-Tag Pin Policy rows 3+4 still named `octo-determin`                                                                                                                                    | Row 3 (`For-Layer-A consumer request`): `octo-determin` → `octo-storage-core only`. Row 4 (`Emergency CVE bypass`): `octo-determin` → `octo-storage-core`. Owner column also updated (octo-determin owner → octo-storage-core owner; the role was undefined in §Roles table).                                                                                          |
| CRIT     | §Key Files row 2 said "No change" but `octo-storage-core/Cargo.toml` still uses branch pin                                                                                                      | Row rewritten to "Change `branch = ...` → `rev = ..."` (the actual freeze edit). New row for `crates/octo-storage-core/src/lib.rs` (handle re-export). New rows for Layer B crate dep removals (`octo-storage/Cargo.toml`, `octo-vault/Cargo.toml`, `quota-router-storage/Cargo.toml`, owner-trait crates, adapter crates).                                            |
| HIGH     | Mermaid subgraph/node id collision (`Frozen`)                                                                                                                                                   | Subgraph `Frozen` → `FrozenTier`; inner node `Frozen` retained (no longer collides).                                                                                                                                                                                                                                                                                   |
| HIGH     | Mermaid `Facade --> Fork` edge false (facade has no stoolap dep)                                                                                                                                | Replaced with `Facade --> Core` (real edge). Added `Consumers -. MUST NOT .-> Core` edge (Layer B uses facade, not substrate directly).                                                                                                                                                                                                                                |
| HIGH     | TV-0205-03 asserted false fact (`octo-storage` has no stoolap dep)                                                                                                                              | Retargeted to `cargo metadata` resolve-graph two-package check (the E0308 guard).                                                                                                                                                                                                                                                                                      |
| HIGH     | jq CI-gate query doubly wrong (cartesian product of two `[]?` + inspects declared manifest not actual resolve)                                                                                  | Rewritten against `.resolve.nodes[]` + `.packages[]                                                                                                                                                                                                                                                                                                                    | .id` cross-reference. Both copies (TV-0205-04 + §Implicit Assumptions row 3) fixed. |
| HIGH     | §Dependencies mis-cited RFC-0870 (Node envelope version_tag is an addendum, not RFC-0870's subject)                                                                                             | RFC-0870 dropped; added RFC-0206 under Requires with reciprocal-edge rationale. §Dependency Validation Rules rephrased (was "All upstream RFCs are Accepted", now "Required RFCs at minimum Draft").                                                                                                                                                                   |
| HIGH     | §Compatibility Backward false by construction (pre/post identity claim contradicts the lag guarantee)                                                                                           | Reframed to "identical at v0 freeze instant; thereafter Layer A intentionally lags the active branch — the lag is the certification guarantee. What is invariant across the boundary is the DQA wire form (pinned at RFC-0105), not the resolved commit."                                                                                                              |
| HIGH     | §Compatibility Forward's drop-fork clause cited §Future Work but §Future Work had no entry                                                                                                      | Added `stoolap-fork-retirement.md` (to be filed) bullet enumerating the migration.                                                                                                                                                                                                                                                                                     |
| HIGH     | Mermaid `Determin` floating node (zero edges)                                                                                                                                                   | Added `Determin -. MUST NOT .-> Frozen` edge (matches §Implicit Assumptions row 3 sole-consumer claim). Label corrected to `octo-determin (determin/)` (package name vs directory).                                                                                                                                                                                    |
| MED      | 17 residual `octo-stoolap-frozen` crate-flavored occurrences (§Determinism Requirements row 1, §Error Handling row 1, §Release-Tag Pin Policy row 2, §Categories to Audit Upgrade safety, etc.) | Replaced with "the frozen rev" / "frozen rev bumps" wording; the `octo-stoolap-frozen-vN` string only appears in git-tag position now.                                                                                                                                                                                                                                 |
| MED      | Row 2 SLA mismatch (CVE severity on major release trigger)                                                                                                                                      | SLA → "90 days (next quarterly re-cert window)" (CVE SLAs are rows 4 and 7 exclusively).                                                                                                                                                                                                                                                                               |
| MED      | 24h + 7-day CVE SLAs never composed                                                                                                                                                             | Row 4 gained "bypass expires at the v{N+1} freeze; bypass outstanding > 7 days escalates to §Severity Classification HIGH".                                                                                                                                                                                                                                            |
| MED      | Rows 4 and 7 both fire on CRITICAL CVE with no disambiguator                                                                                                                                    | Row 4 Action prefixed with precondition "no patched freeze exists yet"; Row 7 prefixed "patch is on the branch, freeze must advance".                                                                                                                                                                                                                                  |
| MED      | §Test Vectors general rule said "TV-0205-01..05" while 6 TVs existed                                                                                                                            | Reframed to `TV-0205-01..07` (added new TV-0205-07).                                                                                                                                                                                                                                                                                                                   |
| MED      | §Operation Class Mapping omitted 2 operations (Layer B fork-dep addition, new-crate-joins-frozen-consumer-set)                                                                                  | Added Layer B fork-dep addition (Class C; caught by TV-0206-06 grep + RFC-0205 TV-0205-04); added new-crate-joins-frozen-consumer-set (Class A; requires RFC + role-table update).                                                                                                                                                                                     |
| MED      | Layer B Cargo.toml template "for documentation" framing self-defeating                                                                                                                          | Template rewritten: Layer B has NO direct `stoolap` dep; goes through re-exported handle. Coordination note added pointing at RFC-0206 §Wiring Pattern.                                                                                                                                                                                                                |
| MED      | §Two-Tier Architecture bullet 4 forbids per-crate `[patch.crates-io]` but no TV enforces                                                                                                        | New TV-0205-07: `! rg '\[patch\.[^]]+\]\s*stoolap\s*=' Cargo.toml crates/*/Cargo.toml crates/**/Cargo.toml`. Catches member-level overrides.                                                                                                                                                                                                                           |
| MED      | Third fork-source class (path-dep test harnesses like `sync-e2e-tests/stoolap-node/Cargo.toml`)                                                                                                 | §Two-Tier Architecture Note: path-dep test harnesses EXEMPT (live outside root workspace); CI gate scopes to root workspace only.                                                                                                                                                                                                                                      |
| LOW      | §Maintainers cited "per plan §3 A.2" without naming the plan                                                                                                                                    | Plan file named: `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §3 A.2.                                                                                                                                                                                                                                                                         |
| LOW      | §Motivation never cited the on-disk audit                                                                                                                                                       | Cited `docs/audits/stoolap-fork-stability-2026-08-16.md` (the on-disk authority the substrate's Cargo.toml comment points at).                                                                                                                                                                                                                                         |
| LOW      | §Design Goals G3/G4 metrics restated triggers rather than measuring anything                                                                                                                    | G3: "≥ 11 of 12 monthly cycles pass per year (allows 1 skip with documented residual)". G4: "≥ 3 of 4 quarterly reviews complete per year (allows 1 deferral; deferral = documented HIGH)".                                                                                                                                                                            |
| LOW      | §Error Handling ">100 commits" divergence threshold unsourced                                                                                                                                   | Threshold: `git rev-list --count upstream/main..feat/blockchain-sql`; 100 = approx. 1 month of active fork churn at typical commit cadence; quarterly review catches before compounds.                                                                                                                                                                                 |
| LOW      | §Economic Analysis inconsistent with §Performance Targets (~0.5 FTE/month vs < 1 day + 1 quarterly)                                                                                             | FTE breakdown line-itemed: 0.12 FTE re-cert + 0.02 FTE CVE triage + 0.05 FTE CVE bumps + 0.02 FTE consumer-set reviews + 0.25 FTE merge-to-upstream ≈ 0.5 FTE/month.                                                                                                                                                                                                   |
| LOW      | §Adversary Analysis missed "freeze configured but inert" (largest residual risk)                                                                                                                | New row added: detection via TV-0205-04 resolve-graph query; defense = multi-layer enforcement (declared + resolved + grep). Same row added to §Security Considerations.                                                                                                                                                                                               |

## Out of scope

- Cargo.toml actual SHA pin (Phase 1 Task 1)
- CI graph audit gate script (Phase 1 Task 3)
- `scripts/stoolap_recert.sh` (Phase 1 Task 4)
- Merge-to-upstream sub-mission (Phase 2 Task 5; deferred to follow-on RFC)

## Related

- [[mission-0862-c10b-rfc-version-pin-sweep-v2-status]] — sibling R3 fix
- [[storage-restructure-plan-audit-2026-08-19]] — plan §10 reconciliation
- [[mission-0206-octo-storage-split-rfc-body-status]] — sibling S7 NEW RFC 2/2
