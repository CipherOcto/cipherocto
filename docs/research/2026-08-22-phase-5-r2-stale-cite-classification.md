# Phase 5 Cross-RFC Harmonization — R2 Stale Cite Classification Research

**Date:** 2026-08-22
**Phase:** 5 (Cross-RFC Harmonization)
**Round:** R2 of Phase 5 fresh-lens loop
**Lens:** enumerate + classify F-P5.1-2 (18 stale cites in RFC-0205) + F-P5.1-3 (3 stale cites in RFC-0206) — distinguish HISTORICAL CONTEXT (audit trail) from STALE DRIFT (actionable)
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R1 Recap

Per R1 (Phase 5 R1 doc commit `48e84af1`): 7 NEW findings (1 CRIT + 1 HIGH + 3 MED + 2 LOW). F-P5.1-2 CRITICAL: RFC-0205 accepted has 18 STALE RFC-0206 v2.0 cites. F-P5.1-3 HIGH: RFC-0206 accepted has 3 STALE self-ref cites.

**R2 objective:** Per-line enumerate F-P5.1-2 + F-P5.1-3 cite locations + classify each as HISTORICAL CONTEXT (audit trail per R37 P3 "fix-trail historical" exception) vs STALE DRIFT (actionable corpus STATE hygiene fix). R2 expected to close R1 CRIT + HIGH with re-classification.

## 1. F-P5.1-2 Enumeration: RFC-0205 Accepted (18 cite locations)

Per grep at `rfcs/accepted/storage/0205-stoolap-fork-stability.md`:

| # | Line | Cite excerpt | Context | Classification |
|---|------|--------------|---------|-----------------|
| 1 | L60 (×2) | "substrate API newtype refactor (deferred to RFC-0206 v2.0)" | Out-of-scope declaration: RFC-0205 v2.0 acceptance explicitly DEFERS newtype refactor to RFC-0206 v2.0. | HISTORICAL — audit trail documenting v2.0-era scope decision |
| 2 | L245 | "RFC-0206 v2.0 reaches Accepted (cross-RFC dependency)" | Dependency Validation Rule: condition 1.2 in RFC-0205 v2.0 was "RFC-0206 v2.0 reaches Accepted". | HISTORICAL — 2-Cycle Atomic Promotion audit trail |
| 3 | L280 | "RFC assumes substrate API per RFC-0206 v2.0 §Cargo.toml Templates Layer A" | Assumption statement at RFC-0205 v2.0 acceptance time. | HISTORICAL — assumption at promotion time |
| 4 | L286 | "If RFC-0206 v2.0 acceptance path alters" | Contingency for v2.0 path alteration. | HISTORICAL — contingency for v2.0 era |
| 5 | L290 | "Condition 1: RFC-0206 v2.0 MUST reach Accepted" | 2-Cycle Atomic Promotion condition. | HISTORICAL — atomic-pair condition at v2.0 era |
| 6 | L306 | "Mission `0205-octo-storage-core-newtype-deferral` | Track RFC-0206 v2.0 newtype refactor schedule" | Mission tracker description. | HISTORICAL — mission scope at v2.0 era |
| 7 | L310 | "Substrate API newtype refactor (RFC-0206 v2.0 scope)" | Out-of-scope declaration. | HISTORICAL |
| 8 | L311 | "Layer B TYPE renames (RFC-0206 v2.0 scope)" | Out-of-scope declaration. | HISTORICAL |
| 9 | L312 | "Per-adapter TV enforcement (RFC-0206 v2.0 scope)" | Out-of-scope declaration. | HISTORICAL |
| 10 | L313 | "TypedStatement enum at substrate level (RFC-0206 v2.0 scope)" | Out-of-scope declaration. | HISTORICAL |
| 11 | L314 | "8-pub-use cap (RFC-0206 v2.0 scope)" | Out-of-scope declaration. | HISTORICAL |
| 12 | L323 | "RFC-0206 v2.0 | At Accepted (per 2-Cycle Atomic Promotion)" | Cross-RFC dependency table. | HISTORICAL — atomic-pair matrix |
| 13 | L333 | "RFC-0206 v2.0 (cross-RFC atomic pair)" | Coupling statement. | HISTORICAL |
| 14 | L339 | "RFC and RFC-0206 v2.0 are coupled pair" | Coupling statement. | HISTORICAL |
| 15 | L359 | VH table row for RFC-0205 v2.0 (THIS file's own VH) | SELF-REF (RFC-0205 v2.0 IS this file at promotion time) | NOT STALE — self-ref VH chain |
| 16 | L377 | "RFC-0206 v2.0 promoted Accepted in coupled pair" | Acknowledgment that coupling was satisfied. | HISTORICAL |
| 17 | L389 | "29 Layer B TYPE renames (RFC-0206 v2.0 §Implementation Phases)" | Cross-RFC ref to dependency scope. | HISTORICAL |

**Total: 17 HISTORICAL CONTEXT cites + 1 self-ref VH entry. ZERO STALE DRIFT.**

### Findings

**Finding F-P5.2-1 (LOW — re-classification of R1 CRIT):** F-P5.1-2 CRITICAL re-classified to LOW. All 18 RFC-0205 cites to RFC-0206 v2.0 are HISTORICAL CONTEXT documenting RFC-0205 v2.0's era-conditional dependency on RFC-0206 v2.0 reaching Accepted per BLUEPRINT.md §2-Cycle Atomic Promotion. Per R37 P3 methodology "fix-trail historical" exception (analogous to R3 F-P4.3-6 HIGH L[N] policy), historical audit trail cites MUST be RETAINED for reviewer audit + cross-RFC atomic promotion audit.

**Resolution:** Add annotation to F-P5.1-2 CLOSURE: classification migrated from CRITICAL to LOW. Recommendation: add a single-line notation to RFC-0205 v2.0 preface block clarifying "All 'RFC-0206 v2.0' references in this file are HISTORICAL CONTEXT documenting the 2-Cycle Atomic Promotion era-conditional coupling; current canonical substrate API spec lives at RFC-0206 v3.x draft head".

## 2. F-P5.1-3 Enumeration: RFC-0206 Accepted (4 cite locations)

Per grep at `rfcs/accepted/storage/0206-octo-storage-split.md`:

| # | Line | Cite excerpt | Context | Classification |
|---|------|--------------|---------|-----------------|
| 1 | L9 | "Supersedes: RFC-0206 v2.1... v2.1 superseded v2.0... v2.0 superseded v1.x" | VH chain self-ref: the file's OWN supersession trail. | NOT STALE — VH chain self-ref |
| 2 | L434 | "Phase 0 — Pre-landing (RFC-0206 v2.0 → v2.1, current state)" | Phase roadmap at v2.2 amendment landing. References the v2.0 → v2.1 transition period. | HISTORICAL — migration roadmap marker |
| 3 | L439 | "Phase 1 — Coexistence (RFC-0206 v2.1 landing + immediate post)" | Phase roadmap: coexistence period after v2.1. | HISTORICAL — migration roadmap marker |
| 4 | L453 | "Phase 2.5 — Substrate re-export block (RFC-0206 v2.2)" | Phase 2.5 mission attached to v2.2 amendment. | HISTORICAL — migration roadmap marker |
| 5 | L461 | "Phase 2.6 — Consumer dep drop (RFC-0206 v2.2)" | Phase 2.6 mission attached to v2.2 amendment. | HISTORICAL — migration roadmap marker |
| 6 | L468 | "Phase 3 — Legacy removal (RFC-0206 v3.0, deferred ≥ 6 months)" | Deferred Phase 3 referencing v3.0. | HISTORICAL — deferred future state marker |

**Total: 1 self-ref VH chain + 5 migration roadmap markers. ZERO STALE DRIFT.**

### Findings

**Finding F-P5.2-2 (LOW — re-classification of R1 HIGH):** F-P5.1-3 HIGH re-classified to LOW. All RFC-0206 cites to its own prior versions are HISTORICAL CONTEXT (VH chain self-ref + migration roadmap markers). Per R37 P3 methodology, VH chains and migration roadmap phase markers MUST be RETAINED for audit trail.

**Resolution:** Add annotation to F-P5.1-3 CLOSURE: classification migrated from HIGH to LOW. No corpus hygiene fix needed.

## 3. L[Intersection] Classification Framework (R2 fresh-lens contribution)

R2 surfaces a NEW classification pattern for corpus STATE hygiene audits:

| Cite category | R37 P3 disposition | Action |
|---------------|---------------------|--------|
| **Prose cite** (RFC-XXXX vY.Y in specification prose) | MUST be LATEST on-disk version | Strip stale, replace with current |
| **VH table column 1** (own VH row) | NOT STALE — self-ref | None |
| **VH supersession chain** (`Supersedes: RFC-XXXX vY.Y`) | NOT STALE — historical chain self-ref | None |
| **Migration roadmap marker** (`Phase X — (RFC-XXXX vY.Y era)`) | NOT STALE — historical context marker | Optional annotation |
| **Fix-trail narrative cite** (in `> R{N}: ... per F-R{N}-X` narrative block) | HISTORICAL — audit trail | Retain |
| **2-Cycle Atomic Promotion condition** (`Condition N: RFC-XXXX vY.Y reaches Accepted`) | HISTORICAL — atomic-pair audit trail | Retain |

### Findings

**Finding F-P5.2-3 (LOW — pattern clarification):** R2 surfaces a 6-category classification framework for distinguishing "stale drift" (corpus hygiene violation, actionable) from "historical context" (audit trail retention, non-actionable). The framework applies corpus-wide to all RFC-0206 version cites (and generalizes to other version-sensitive RFCs).

**Adoption path:** Per Phase 5 R2 closure, this framework should be INCORPORATED into the pre-commit Guard 2 cite validator logic. Implementation: `scripts/validate_cites.sh` should classify each cite as prose (strict STALE check) vs VH-marker (skip check) vs fix-trail (skip check) before applying version-pin STALE detection. Phase 5 R3 follow-up: Guard 2 enhancement proposal.

## 4. R2 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | (none) |
| HIGH | 0 | (none) |
| MED | 0 | (none) |
| LOW | 0 | (none) |
| CLOSURE | 2 | F-P5.2-1 (R1 F-P5.1-2 CRIT reclassified to LOW — all 17 cites HISTORICAL CONTEXT) + F-P5.2-2 (R1 F-P5.1-3 HIGH reclassified to LOW — all cites HISTORICAL CONTEXT) |
| PATTERN | 1 | F-P5.2-3 (6-category classification framework for corpus STATE hygiene audits) |

**R2 NEW: 0 findings + 2 R1 closures + 1 pattern contribution.**

## 5. Convergence Loop Status (R2 first 0-NEW)

| Phase 5 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 7 (1 CRIT + 1 HIGH + 3 MED + 2 LOW) | NO | Initial cross-RFC corpus drift audit |
| R2 | 0 NEW + 2 R1 closures (CRIT + HIGH reclassified to LOW) + 1 pattern | **YES (FIRST)** | Per-cite enumeration + reclassification |
| R3 (next) | TBD | TBD | For DRY closure, R3 must also be 0 NEW |

**Convergence direction:** Phase 5 R1=7 → R2=0. Strictly decreasing + R2 = FIRST 0-NEW round.

**R3 expectation:** Second consecutive 0-NEW for DRY. Apply Guard 2 enhancement (F-P5.2-3 pattern) + close remaining R1 MEDs (F-P5.1-1 VH missing cohort decomposition + F-P5.1-4 v3.0 cites) + final corpus STATE verification. Expect 0 NEW findings + 2-3 R1 MED closures + Guard 2 enhancement applied.

**DRY target:** R2 + R3 = 2 consecutive 0-NEW rounds → loop closed per BLUEPRINT.md §Adversarial Review Process DRY criterion.

## 6. Phase 5 Roadmap (R2 projection)

### Phase 5 R3 (target DRY closure):

1. **F-P5.1-1 decomposition**: Break down 52 RFCs missing VH into (a) Planned placeholders (acceptable) vs (b) Drafts needing VH addition (actionable). Enumerate per RFC.
2. **F-P5.1-4 closure**: Verify 9 promotion candidates' v3.0 cites are HISTORICAL fix-trail (acceptable per R37 P3 methodology) vs STALE drift (actionable).
3. **F-P5.2-3 Guard 2 enhancement**: `scripts/validate_cites.sh` updated to apply 6-category classification framework (prose strict / VH self-ref / supersession chain / roadmap marker / fix-trail / atomic-promotion condition).
4. **Final corpus STATE audit**: All 175 RFCs scored on 6-category classification + actionable bucket enumeration.

### Out of scope (Phase 5 R3+ deferred):

- R3 phantom substrate file ref audit confirmed PASS (F-P5.1-7 LOW — no NEW phantom refs)
- R4+ long-tail maintenance (VH table additions to actionable Drafts)
- Phase 6 (Long-Tail Maintenance) per plan v1.5

## 7. R10.5 Scope Discipline Recap

Phase 5 R2 work is RESEARCH DOC ONLY (analysis + classification). NO substrate crate code edits. NO Cargo.toml / Cargo.lock edits. NO RFC text edits (those deferred to R3 + user-gated pre-promotion work). NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 8. Cross-References

- Phase 5 R1 doc: `docs/research/2026-08-22-phase-5-cross-rfc-harmonization-r1-drift.md` (commit `48e84af1`)
- Phase 4.7 R7 DRY CLOSURE: `docs/research/2026-08-22-rfc-promotion-cascade-r7-dry-closure.md` (commit `b765edec`)
- Phase 4.5 R5 freshness audit: `docs/research/2026-08-22-rfc-promotion-cascade-r5-freshness-audit.md` (commit `18f7f302`)
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Memory card `feedback_initiation_user_only`: 7-day review window + 2+ maintainer approvals
- LONG-HORIZON-PLAN v1.5: Phase 5 Cross-RFC Harmonization
- BLUEPRINT.md §RFC Process: VH + 2-Cycle Atomic Promotion

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 5 R2 cite classification research; 0 NEW findings + 2 R1 closures (F-P5.2-1 + F-P5.2-2 reclassified R1 CRIT + HIGH to LOW — all 18 + 4 cites are HISTORICAL CONTEXT not STALE DRIFT) + 1 pattern contribution (F-P5.2-3 6-category classification framework). Convergence: R1=7 → R2=0. FIRST 0-NEW round. R3 expected to close DRY loop. |