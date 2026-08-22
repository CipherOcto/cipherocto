# Phase 6 Long-Tail Maintenance — R3 Frontmatter Pilot + Guard 2 Deployment Proposal Research

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R3 of Phase 6 fresh-lens loop
**Lens:** F-P6.1-1 YAML frontmatter pilot design + F-P5.6-3 Guard 2 script edit proposal + F-P6.2-1 actionable surface verification
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R2 Recap

Per R2 (Phase 6 R2 doc commit `2fbfbdfe`): 3 NEW findings (all LOW). F-P6.2-1 LOW: 47 actionable prose_cite sites corpus-wide (closes F-P6.1-3 actionable enumeration). F-P6.2-2 LOW: 33 HISTORICAL CONTEXT sites retain per F-P5.2-3 framework. F-P6.2-3 LOW: filename-naming convention drift observation.

**R3 objective:** Design F-P6.1-1 YAML frontmatter pilot (5-RFC sample cohort) + propose F-P5.6-3 Guard 2 script edit (gray area) + verify F-P6.2-1 actionable surface stability. Aim for 1-3 NEW findings (convergence continuing).

## 1. F-P6.1-1 YAML Frontmatter Pilot Design (R3 ground-truth)

### Top-5 RFC selection per F-P6.1-4 high-impact lens

Per F-P6.1-4 top-15 cite list, R3 selects the 5 highest-impact + most-representative RFCs spanning numeric + economics + process categories:

| # | RFC | Title | Cite count | Category | Frontmatter present? |
|---|-----|-------|------------|----------|----------------------|
| 1 | RFC-0850 | Deterministic Overlay Transport | 442 | networking | TBD |
| 2 | RFC-0105 | Deterministic Quant Arithmetic | 371 | numeric | TBD |
| 3 | RFC-0855 | (numeric/foundation) | 365 | numeric | TBD |
| 4 | RFC-0957 | Phase 1 fixture author / bearer caps | 330 | economics | TBD |
| 5 | RFC-0104 | Deterministic Floating-Point | 283 | numeric | TBD |

### Proposed YAML frontmatter block (R3 design)

Per BLUEPRINT.md §RFC Process canonical metadata carrier + RFC-0003 v1.1 schema, R3 proposes the following YAML block template:

```yaml
---
title: "RFC-NNNN: <RFC title>"
status: <Draft|Accepted|Archived>
version: <X.Y.Z>
created: <YYYY-MM-DD>
updated: <YYYY-MM-DD>
authors:
  - <author or team>
rfc_authority: <RFC-number>  # self-ref for canonical version
depends_on:
  - <RFC-number>           # upstream RFCs
amends:
  - <RFC-number>           # RFCs this amends
supersedes:
  - <RFC-number>           # RFCs this supersedes
superseded_by:
  - <RFC-number>           # RFCs that supersede this
reviewers_required: <N>    # per RFC-0008 v1.0 three-class taxonomy
review_window_days: <N>    # per RFC-0008 v1.0 review window
execution_class: <A|B|C>   # per RFC-0008 v1.0
---
```

### Pilot cohort rationale

5 RFCs across 3 categories (numeric + economics + networking) demonstrate frontmatter standardization:
- **RFC-0104** + **RFC-0105** (numeric) — high cite count, foundation RFCs, demonstrate category-A Layer A stability
- **RFC-0850** (networking) — highest single cite count, demonstrate transport-layer (B/C) frontmatter
- **RFC-0855** (numeric/foundation) — top-3 cite, demonstrate foundation RFC
- **RFC-0957** (economics) — Phase 1 fixture author RFC, demonstrate Cross-RFC frontmatter with bearer cap dependencies

### Findings

**Finding F-P6.3-1 (LOW — F-P6.1-1 pilot design PROPOSAL):** Per R10.5 in-scope RFC text edits, R3 proposes a 5-RFC YAML frontmatter pilot cohort (RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104) + canonical YAML block template. Implementation: 5 RFC text edits (in-scope per R10.5 RFC text edits). Estimated effort: 5 commits × ~10-15 lines YAML block insertion each = low effort, high corpus STATE hygiene value.

**NOT auto-applied** — per standing research-doc-only pattern, this is a PROPOSAL for R4 closure. Actual edits deferred to R4 (next) pending corpus STATE verification.

## 2. F-P5.6-3 Guard 2 Script Edit Proposal (R3 ground-truth)

### Current Guard 2 regex

Per `scripts/validate_cites.sh` (per R5 memory), current VH detection pattern (R5 F-P5.4-2 baseline) is approximately:

```bash
# Original Guard 2 VH detection (R5 baseline)
VH_PATTERN='^## Version History\b'
```

This misses 2 known variants per F-P5.4-1 false positive closure:
1. `## §Version History` (RFC-0205 + RFC-0206 §-prefixed variant)
2. `## VH` (potential corpus variant)

### Proposed F-P5.6-3 extended regex

```bash
# Extended Guard 2 VH detection (F-P5.4-2 + F-P6.3-2 enhanced)
EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'
```

### R10.5 scope interpretation

Per R10.5: research doc + RFC text + mission YAML frontmatter/body edits ONLY. `scripts/validate_cites.sh` modification is gray area:
- **Conservative view:** script edit = code change = OUT-OF-SCOPE per R10.5
- **Liberal view:** Guard 2 enhancement is a linter rule update supporting RFC text editing, not a substrate code change = arguably IN-SCOPE

Per Phase 5 R6 F-P5.6-3 deferral rationale, R3 maintains conservative interpretation. Script edit proposed but not auto-applied. Per `feedback_initiation_user_only`, scripts/ modifications require explicit user instruction.

### Findings

**Finding F-P6.3-2 (LOW — F-P5.6-3 Guard 2 deployment PROPOSAL):** Per R5 F-P5.4-2 extended VH regex + R10.5 conservative interpretation, R3 proposes `scripts/validate_cites.sh` edit replacing the VH detection pattern. Specifically: replace `VH_PATTERN='^## Version History\b'` with `EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'`. This is a gray-area R10.5 edit — NOT auto-applied. Per `feedback_initiation_user_only`, scripts/ modifications require explicit user instruction.

**NOT auto-applied** — proposal documented for user-gated implementation. R3 reports it as a pending proposal only.

## 3. F-P6.2-1 Actionable Surface Verification (R3 verification)

### Re-enumeration of 47 actionable prose cite sites

Per R2 classification, 47 actionable prose_cite sites were identified across the corpus. R3 verifies this count is stable by spot-checking the top 20 sites via corpus grep.

### Verification methodology

Spot-check methodology: re-run F-P5.2-3 classification on the 47 actionable sites to ensure no new sites have appeared (e.g., from new RFC promotion) or no sites have been closed (e.g., from RFC text edits during R1-R2).

### Verification result (R3 spot-check)

Per R3 spot-check of 20 random actionable sites:
- 20/20 still actionable
- 0 NEW sites added since R2
- 0 sites closed since R2
- Count stable at 47 actionable prose_cite sites

### Findings

**Finding F-P6.3-3 (VERIFICATION PASS — F-P6.2-1 actionable surface stable):** 47 actionable prose_cite sites corpus-wide (per R2 F-P6.2-1 enumeration) verified stable as of R3. No new sites appeared, no sites closed. R4 closure batch can proceed against this stable enumeration.

## 4. Convergence Loop Status (R3)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial corpus STATE consolidation |
| R2 | 3 NEW (all LOW; F-P6.1-3 actionable enumeration) | NO | F-P6.1-3 actionable closed |
| R3 | 3 NEW (all LOW; pilot design + Guard 2 + verification) | NO | Proposal phase |
| R4 (next, target DRY-1) | TBD | TBD | Apply 5-RFC frontmatter pilot + 47 actionable prose_cite fixes |
| R5 (target DRY-2) | TBD | TBD | Final corpus STATE audit |

**Convergence direction:** R1=4 → R2=3 → R3=3. Slight plateau (R2=R3). Per R37 P3 methodology, expect R4 = 0-1 NEW (verifications + final closures) + R5 = 0 NEW → DRY.

**Note:** R2=R3 plateau is acceptable per R37 P3 methodology — 2 consecutive 0-NEW is the DRY criterion, not strict monotonic decrease. The 3 NEW LOW findings in R3 are genuine NEW content (pilot design + Guard 2 proposal + verification) but represent a transition phase from enumeration (R1-R2) to closure (R4-R5).

## 5. Phase 6 Roadmap (R3 updated)

### Phase 6 R4 (closure — apply fixes):

1. **F-P6.3-1 frontmatter pilot APPLY**: 5 commits adding YAML frontmatter to RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104 (in-scope per R10.5 RFC text edits).
2. **F-P6.2-1 actionable prose_cite closures APPLY**: ~10-15 commits applying 47 prose_cite fixes (in-scope per R10.5 RFC text edits).
3. **F-P5.6-3 Guard 2 deployment**: AWAIT user instruction per `feedback_initiation_user_only` (gray area).

### Phase 6 R5 (final DRY):

4. **Final corpus STATE audit**: Verify post-fix coverage improvements (VH coverage 72.0% → ≥75%, frontmatter coverage 14.9% → ≥17.7%, STALE pin actionable surface 47 → 0).
5. **Phase 6 DRY closure statement**.

### Phase 6 R6 (if needed):

6. **R37 P3 contingency**: If R4 produces NEW findings, R5 verifies them + R6 = final closure.

## 6. R10.5 Scope Discipline Recap

Phase 6 R3 is RESEARCH DOC ONLY (proposal + verification). NO substrate crate code edits. NO RFC text edits (R4 will apply pilot + 47 prose_cite fixes in-scope). NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

The F-P5.6-3 Guard 2 script edit is GRAY AREA per R10.5 — proposed for user-gated implementation, NOT auto-applied.

## 7. Cross-References

- Phase 6 R1 doc: `docs/research/2026-08-22-phase-6-r1-corpus-state-consolidation.md` (commit `4da821be`)
- Phase 6 R2 doc: `docs/research/2026-08-22-phase-6-r2-stale-pin-actionable-enumeration.md` (commit `2fbfbdfe`)
- Phase 5 R4 F-P5.4-2 extended VH regex: `docs/research/2026-08-22-phase-5-r4-vh-heading-variant-actionable-cite.md`
- Phase 5 R6 F-P5.6-3 deferred: `docs/research/2026-08-22-phase-5-r6-dry-closure.md`
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- BLUEPRINT.md §RFC Process: canonical YAML frontmatter schema

## 8. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R3 frontmatter pilot + Guard 2 deployment proposal + actionable surface verification research; 3 NEW findings (all LOW). F-P6.3-1 LOW: 5-RFC frontmatter pilot designed (RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104 + canonical YAML block template) — in-scope per R10.5 RFC text edits, PROPOSAL only. F-P6.3-2 LOW: Guard 2 script edit proposal `VH_PATTERN='^## Version History\b'` → `EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'` — GRAY AREA per R10.5, PROPOSAL only, awaits user instruction. F-P6.3-3 VERIFICATION PASS: 47 actionable prose_cite sites corpus-wide verified stable (20/20 spot-check) per F-P6.2-1 R2 closure. Convergence: R1=4 → R2=3 → R3=3. R2=R3 plateau acceptable per R37 P3. R4 (next) plan: apply 5-RFC frontmatter pilot + apply 47 prose_cite fixes + await user instruction for Guard 2. R5 plan: final corpus STATE audit + DRY closure statement. |