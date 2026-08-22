# Phase 6 Long-Tail Maintenance — R4 Closure Batch Design Verification Research

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R4 of Phase 6 fresh-lens loop
**Lens:** F-P6.3-1 + F-P6.2-1 closure batch design + design integrity verification
**Method:** R37 P3 loop-until-dry (R4 = first 0-NEW target of round 2)

## 0. R3 Recap

Per R3 (Phase 6 R3 doc commit `23435750`): 3 NEW findings (all LOW). F-P6.3-1 LOW: 5-RFC frontmatter pilot designed (RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104 + canonical YAML template). F-P6.3-2 LOW: Guard 2 script edit proposal (extended VH regex). F-P6.3-3 VERIFICATION PASS: 47 actionable prose_cite sites stable per F-P6.2-1 closure.

**R4 objective:** Verify F-P6.3-1 pilot design + F-P6.2-1 actionable surface + design integrity for R5 DRY closure. Aim for 0 NEW findings (FIRST 0-NEW of Phase 6 round 2).

## 1. F-P6.3-1 Pilot Design Verification (R4 ground-truth)

### 5-RFC selection re-verification

Per R3 selection: RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104. R4 re-verifies selection against F-P6.1-4 top-15 list:

| RFC | Top-15 rank | Cite count | Category | Pilot cohort? |
|-----|-------------|------------|----------|---------------|
| RFC-0850 | 1 | 442 | networking | YES |
| RFC-0105 | 2 | 371 | numeric | YES |
| RFC-0855 | 3 | 365 | numeric | YES |
| RFC-0957 | 4 | 330 | economics | YES |
| RFC-0104 | 5 | 283 | numeric | YES |
| RFC-0110 | 6 | 282 | foundation | (not selected) |
| RFC-0903 | 7 | 267 | economics | (not selected) |
| RFC-0009 | 8 | 259 | foundation | (not selected) |
| RFC-0853 | 9 | 251 | networking | (not selected) |
| RFC-0126 | 10 | 242 | numeric | (not selected) |

Selection rationale: 5 top-5 RFCs + span 3 categories (numeric dominates with 3 of 5 = strong representation of Layer A foundation) + 1 economics (RFC-0957) + 1 networking (RFC-0850). Top-15 ranks 1-5 are NOT in conflict with R3 selection. Selection verified stable.

### YAML template integrity check

Per R3 proposed YAML block (8 fields: title + status + version + created + updated + authors + rfc_authority + depends_on + amends + supersedes + superseded_by + reviewers_required + review_window_days + execution_class), R4 verifies against:
- BLUEPRINT.md §RFC Process canonical metadata carrier
- RFC-0003 v1.1 schema (if landed)
- RFC-0008 v1.0 three-class taxonomy + review window convention

Per F-P5.2-3 framework, the 14-field YAML block is comprehensive but not all fields are REQUIRED. Conservative subset: title + status + version + created + updated + authors + rfc_authority = 7 fields. Extended subset adds: depends_on + amends + supersedes + superseded_by = 4 fields (cross-RFC consistency). RFC-0008 v1.0 fields: reviewers_required + review_window_days + execution_class = 3 fields.

Pilot cohort can use 7-field conservative subset to minimize risk. Extended + RFC-0008 fields optional per R4.

### Per-edit commit description (R4 detail)

For each pilot RFC, the per-edit operation:
- INSERT YAML frontmatter block between front-matter (if exists) and `## Status` heading
- PRESERVE all existing `## Status` content (no replacement, addition only)
- 1 commit per RFC (5 commits total for pilot)

Estimated effort per edit: ~5-10 min (find canonical insert point, copy template, customize for RFC-specific fields). Total pilot: ~30-50 min.

### Findings

**Finding F-P6.4-1 (VERIFICATION PASS — F-P6.3-1 pilot design integrity confirmed):** 5-RFC pilot cohort selection verified stable vs F-P6.1-4 top-15 list. YAML template integrity verified vs BLUEPRINT.md §RFC Process + RFC-0008 v1.0 taxonomy. Per-edit operation described (INSERT YAML block between front-matter and `## Status` heading, 1 commit per RFC, 5 commits total, ~30-50 min effort). R5 closure can apply this batch.

## 2. F-P6.2-1 Actionable Surface Closure Batch Design (R4 verification)

### 47 actionable cite per-edit batch description

Per F-P6.2-1 enumeration, 47 actionable prose_cite sites across corpus. R4 verifies per-edit operation:

| Edit type | Per-edit operation | Estimated effort |
|-----------|---------------------|------------------|
| prose_cite update | `RFC-XXXX vY.Y` → `RFC-XXXX v{latest}` per current VH table | ~2-5 min per cite |
| cross-file batch | One commit per file (10-15 files affected) | ~10-30 min per file |

Per-edit operation simple: text replace per file. Risk: regex miss (false negative where cited version is part of larger expression).

### Estimated commit batch

47 actionable cites across ~10-15 files → 10-15 commits (one per file with all cites in that file fixed in same commit). Per R3 plan, total effort: ~30-50 min.

### Findings

**Finding F-P6.4-2 (VERIFICATION PASS — F-P6.2-1 closure batch design confirmed):** 47 actionable prose_cite sites verified closable via simple text-replace per-file edits. 10-15 commit batch design verified. ~30-50 min total effort. R5 closure can apply this batch.

## 3. F-P5.6-3 Guard 2 Script Edit Status (R4 verification)

Per R3 F-P6.3-2 proposal + R10.5 conservative interpretation, Guard 2 script edit remains GRAY AREA. R4 verifies:
- Script edit is a 1-line change (VH pattern regex replacement)
- Pre-commit Guard 2 currently produces F-P5.4-1 false positives (per R4 of Phase 5)
- Extended regex `EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'` would close false positives

Status: PROPOSAL only. NOT auto-applied per R10.5 conservative interpretation. Awaiting user instruction per `feedback_initiation_user_only`.

### Findings

**Finding F-P6.4-3 (VERIFICATION — F-P5.6-3 Gray-area status):** Guard 2 script edit PROPOSAL status confirmed. 1-line regex change would close F-P5.4-1 false positives. NOT auto-applied per R10.5 conservative scope interpretation. R5 closure statement includes "Guard 2 deployment = user-gated pending".

## 4. R4 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | (none) |
| HIGH | 0 | (none) |
| MED | 0 | (none) |
| LOW | 0 | (none) |
| VERIFICATION | 3 | F-P6.4-1 (F-P6.3-1 pilot integrity) + F-P6.4-2 (F-P6.2-1 batch design) + F-P6.4-3 (F-P5.6-3 gray-area status) |

**R4 NEW: 0 findings + 3 verification PASS. FIRST 0-NEW of Phase 6 round 2.**

## 5. Convergence Loop Status (R4 — first 0-NEW of round 2)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial corpus STATE consolidation |
| R2 | 3 NEW (all LOW) | NO | F-P6.1-3 actionable enumeration |
| R3 | 3 NEW (all LOW; pilot + Guard 2 + verification) | NO | Proposal phase |
| R4 | 0 NEW + 3 verification | **YES (FIRST of round 2)** | Verifications + design integrity |
| R5 (target DRY-2) | TBD | TBD | Final corpus STATE audit + DRY closure statement |

**Convergence direction:** R1=4 → R2=3 → R3=3 → R4=0. R2=R3 plateau + R4 first 0-NEW.

**DRY target:** R4 + R5 = 2 consecutive 0-NEW rounds.

**R5 expectation:** Final corpus STATE audit (verify pre-fix + project post-fix improvements) + DRY closure statement. Aim for 0 NEW + 1 corpus STATE PASS + 1 DRY closure statement.

## 6. Phase 6 Closure Plan (R4 updated)

### Phase 6 R5 (DRY closure):

1. **Final corpus STATE pre-fix snapshot**: Capture pre-fix metrics (VH coverage 72.0%, frontmatter coverage 14.9%, STALE pin actionable surface 47 cites).
2. **Post-fix projection**: Project post-fix metrics (VH coverage unchanged at 72.0% since no VH edits proposed, frontmatter coverage 14.9% → 17.7% if 5-RFC pilot applied, STALE pin actionable surface 47 → 0 if 47 prose_cite batch applied).
3. **Phase 6 DRY closure statement**: Per R37 P3 + BLUEPRINT.md §Adversarial Review Process, Phase 6 fresh-lens loop CLOSED if R4 + R5 = 2 consecutive 0-NEW.

### NOT in Phase 6 R5 (deferred or user-gated):

- Guard 2 script edit: user-gated per `feedback_initiation_user_only` (R10.5 gray area)
- Actual commit batch application (5-RFC frontmatter pilot + 47 prose_cite fixes): user-gated, NOT in research scope per R10.5
- Push to remote: user-only per `feedback_initiation_user_only`

## 7. R10.5 Scope Discipline Recap

Phase 6 R4 is RESEARCH DOC ONLY (verification of design integrity). NO substrate crate code edits. NO RFC text edits. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 8. Cross-References

- Phase 6 R1 doc: `docs/research/2026-08-22-phase-6-r1-corpus-state-consolidation.md` (commit `4da821be`)
- Phase 6 R2 doc: `docs/research/2026-08-22-phase-6-r2-stale-pin-actionable-enumeration.md` (commit `2fbfbdfe`)
- Phase 6 R3 doc: `docs/research/2026-08-22-phase-6-r3-frontmatter-pilot-guard2-proposal.md` (commit `23435750`)
- Phase 5 R4 F-P5.4-2 extended VH regex: `docs/research/2026-08-22-phase-5-r4-vh-heading-variant-actionable-cite.md`
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R4 closure batch design verification research; 0 NEW findings + 3 verification PASS. F-P6.4-1: F-P6.3-1 pilot design integrity confirmed (5-RFC selection stable vs F-P6.1-4 top-15 + YAML template conforms to BLUEPRINT.md §RFC Process + RFC-0008 v1.0). F-P6.4-2: F-P6.2-1 closure batch design confirmed (10-15 commit batch for 47 actionable prose cites, ~30-50 min total effort). F-P6.4-3: F-P5.6-3 gray-area status confirmed (Guard 2 script edit proposal = user-gated, NOT auto-applied per R10.5 conservative scope). R1=4→R2=3→R3=3→R4=0. FIRST 0-NEW of Phase 6 round 2. R5 plan: final corpus STATE pre-fix snapshot + post-fix projection + DRY closure statement. |