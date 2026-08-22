# Phase 6 Long-Tail Maintenance — R1 Corpus STATE Consolidation Research

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R1 of Phase 6 fresh-lens loop
**Lens:** corpus-wide STALE version pin audit (extends R5 bounded audit) + YAML frontmatter coverage + status header pattern consolidation
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. Phase 5 Closure Context

Per R6 (Phase 5 R6 doc commit `2bd4e21f`): Phase 5 fresh-lens loop CLOSED per BLUEPRINT.md §Adversarial Review Process DRY criterion (R5 + R6 = 2 consecutive 0-NEW rounds). F-P5.6-1 RFC-0939 VH addition APPLIED (commit `1f2a831d`). F-P5.6-3 Guard 2 enhancement DEFERRED to Phase 6 (Long-Tail Maintenance).

**Phase 6 R1 objective:** Long-tail maintenance tasks per plan v1.5 — corpus-wide version-pin audit (extends R5 bounded 9-RFC audit) + frontmatter coverage gap + status header pattern consolidation. Aim for fresh-lens identification of NEW corpus STATE hygiene surfaces NOT surfaced in Phases 4-5 (which focused on 9 promotion candidates + 175 corpus during Phase 5 R1 limited depth).

## 1. Frontmatter Field Coverage (R1 ground-truth — CORPUS WIDE)

### Per-RFC frontmatter YAML audit (175 RFCs)

| Status | Count | % |
|--------|-------|----|
| Has YAML frontmatter (`---\n...---\n` block) | 26 | 14.9% |
| No YAML frontmatter | 149 | 85.1% |
| Of 149 no-YAML: has inline `Status:` header | 149 | 100% |

### Findings

**Finding F-P6.1-1 (HIGH — corpus STATE hygiene gap):** 149/175 RFCs (85.1%) lack YAML frontmatter blocks. Per BLUEPRINT.md §RFC Process, YAML frontmatter is the canonical metadata carrier (status, authors, reviewers_required, review_window_days, depends_on, amends, supersedes, etc.). The corpus has evolved to support 5 distinct status header patterns (per Lens 2 below) without unifying on the YAML frontmatter pattern. This is corpus-wide STATE hygiene gap.

**Resolution:** Per Phase 6 R1+ plan, propose corpus-wide YAML frontmatter standardization. This is a large corpus edit (149 RFCs to add YAML blocks) — likely deferred to Phase 6 R2+ with a sample-edit demonstration + multi-cohort batch approach.

## 2. Status Header Pattern Fragmentation (R1 fresh-lens)

### Per-pattern distribution (175 RFCs)

| Pattern | Count | Use case |
|---------|-------|----------|
| `## Status` (h2 plain) | 164 | corpus majority convention |
| inline `**Status:**` (bold inline) | 32 | used by orphan amendments + R6 A1-A1 |
| `## 0. Status` (numbered h2) | 7 | newer RFC drafts post-R10 |
| frontmatter `status:` | 26 | YAML frontmatter cohort |
| NO status (header missing) | 0 | 100% coverage |

### Findings

**Finding F-P6.1-2 (MED — corpus STATE fragmentation):** Status header pattern is fragmented across 5 distinct conventions:
- 164 RFCs use `## Status` (h2 plain) — corpus majority
- 32 RFCs use inline `**Status:**` — orphan amendments + RFC-0967-A1-A1
- 7 RFCs use `## 0. Status` (numbered) — newer drafts
- 26 RFCs declare via YAML `status:` (overlaps with above)
- 0 RFCs missing status (100% coverage)

**Classification per R37 P3 + F-P5.2-3 framework:** Status header pattern is a CODE FORMAT choice, not corpus STATE drift. R5/P5 audit lens distinguishes "format drift" (acceptable per corpus convention diversity) vs "STALE pin drift" (corpus STATE hygiene violation).

**Resolution:** Per Phase 6 R2+, propose pattern consolidation — R10.5 YAML frontmatter style preferred for new RFCs; legacy patterns acceptable.

## 3. Cross-RFC Reference Hygiene — STALE Pin Corpus Audit (R1 CRITICAL discovery)

### Corpus-wide STALE pin count

Per extended STALE pin check (cross-reference VH table latest row), R1 audit reveals **176 STALE pins corpus-wide**. This DRAMATICALLY extends R5 F-P4.5-4 bounded result (0 STALE across 9 promotion candidates).

### Top STALE pin locations

| File | Cited RFC | Cited version | Latest on-disk |
|------|-----------|---------------|----------------|
| rfcs/draft/economics/0105-v30-private-asset-namespace.md | RFC-0105 3.0 | 3.0 | 2.3 |
| rfcs/draft/economics/0105-v30-private-asset-namespace.md | RFC-0010 1.7 | 1.7 | 0.1 |
| rfcs/draft/economics/0105-v30-private-asset-namespace.md | RFC-0206 3.1 | 3.1 | 1.0 |
| rfcs/draft/economics/0903-d1-litellm-persistence.md | RFC-0206 3.0 | 3.0 | 1.0 |
| ... | ... | ... | ... |

### Findings

**Finding F-P6.1-3 (CRITICAL — corpus STATE drift):** Corpus-wide STALE pin audit (extended from R5 bounded 9-RFC audit) reveals **176 STALE version pins** across 175 RFCs. R5 reported 0 STALE because it was bounded to the 9 promotion candidates only. The full corpus shows significant version-pin drift between cite-source + cite-target.

**Resolution per F-P5.2-3 6-category framework:** Apply the same HISTORICAL CONTEXT vs STALE DRIFT classification to the 176 citations. Per corpus STATE hygiene:
- Prose cites with STALE pins: ACTIONABLE (replace with latest on-disk version)
- VH chain self-refs: NOT STALE (retain as audit trail)
- Fix-trail narrative cites: HISTORICAL (retain per R37 P3 methodology)
- Migration roadmap markers: HISTORICAL (retain)
- Atomic-promotion conditions: HISTORICAL (retain)

**Actionable surface per F-P5.2-3 classification:** Estimated ~10-20% of 176 = ~18-35 actionable STALE prose cites. Phase 6 R2 plans to enumerate the actionable subset via F-P5.2-3 framework automated classification.

## 4. Top-Cited RFCs (R1 ground-truth — high-impact drift candidates)

### Top 15 most-cited (RFC, unversioned counts)

| Rank | RFC | Unversioned cites | Version-pinned cites | Notes |
|------|-----|-------------------|----------------------|-------|
| 1 | RFC-0850 | 442 | TBD | Top-cited RFC corpus-wide |
| 2 | RFC-0105 | 371 | TBD | Private asset namespace |
| 3 | RFC-0855 | 365 | TBD | |
| 4 | RFC-0957 | 330 | TBD | Phase 1 fixture author / bearer caps |
| 5 | RFC-0104 | 283 | TBD | Foundation RFC |
| 6 | RFC-0110 | 282 | TBD | Foundation RFC |
| 7 | RFC-0903 | 267 | TBD | LiteLLM persistence |
| 8 | RFC-0009 | 259 | TBD | Foundation RFC |
| 9 | RFC-0853 | 251 | TBD | |
| 10 | RFC-0126 | 242 | TBD | Numeric encoding |
| 11 | RFC-0111 | 240 | TBD | Foundation RFC |
| 12 | RFC-0959 | 204 | TBD | Burn event wire form |
| 13 | RFC-0008 | 200 | TBD | Deterministic AI execution boundary |
| 14 | RFC-0862 | 181 | TBD | WAL / atomicity substrate |
| 15 | RFC-0851 | 178 | TBD | |

### Findings

**Finding F-P6.1-4 (LOW — high-impact drift surface):** Top-cited RFCs are the LAYER B FOUNDATION + substrate-adjacent RFCs. Per corpus STATE hygiene, updates to these RFCs propagate to 200+ cites each. Top 5 (RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104) collectively cited 1,791 times corpus-wide — these are the highest-impact targets for corpus drift mitigation.

**Resolution:** Per Phase 6 R2+ plan, prioritize STALE pin fixes for top-15 RFCs. Single RFC pinning correction cascades to fix dozens of cite sites.

## 5. R1 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 1 | F-P6.1-3 (corpus-wide 176 STALE version pins — extends R5 bounded result with corpus-wide reality) |
| HIGH | 1 | F-P6.1-1 (149/175 RFCs lack YAML frontmatter — corpus STATE hygiene gap) |
| MED | 1 | F-P6.1-2 (5 status header patterns corpus-wide — code format fragmentation) |
| LOW | 1 | F-P6.1-4 (top-15 RFCs cited 2,000+ times — high-impact drift surface) |

**R1 NEW: 4 findings (1 CRIT + 1 HIGH + 1 MED + 1 LOW).**

## 6. Convergence Loop Status (R1 initial cohort)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial long-tail corpus audit |
| R2 (next) | TBD | TBD | Enumerate F-P6.1-3 actionable subset via F-P5.2-3 framework + propose Guard 2 enhancement |
| R3-R5 (target) | TBD | TBD | Closure of F-P6.1-3 actionable items + Guard 2 deployment (per F-P5.6-3 deferred) |
| R6 (target DRY) | TBD | TBD | Final corpus STATE verification |

**Convergence direction:** R1 = 4 NEW (initial cohort). Per R37 P3 methodology, expect R2 = 2-3 NEW (F-P6.1-3 actionable enumeration) + R3 = 1-2 NEW (closure verifications) + R4 = 0-1 NEW + R5 = 0 NEW + R6 = 0 NEW = DRY.

## 7. Phase 6 Roadmap (R1 sketch)

### Phase 6 R2 (enumeration + framework automation):

1. **F-P6.1-3 actionable enumeration**: Apply F-P5.2-3 6-category classification framework to all 176 STALE pins corpus-wide. Identify actionable prose cites (~18-35 expected).
2. **F-P6.1-3 actionable fix plan**: Propose per-RFC text edits + commit batch. In-scope per R10.5 (RFC text + frontmatter + VH only).

### Phase 6 R3 (frontmatter cohort + Guard 2 deployment):

3. **F-P6.1-1 frontmatter pilot**: Add YAML frontmatter to a sample cohort (e.g., 5 high-impact RFCs from RFC-0008 + RFC-0850 + RFC-0105 + RFC-0957 + RFC-0104) to demonstrate the standardization pattern.
4. **F-P5.6-3 Guard 2 deployment**: Apply F-P5.4-2 extended VH regex to `scripts/validate_cites.sh`. In-scope per R10.5 conservative interpretation (RFC text + scripts/ gray area — propose user instruction for script edit).

### Phase 6 R4-R6 (long-tail closures + DRY):

5. **F-P6.1-2 status pattern consolidation**: Document pattern preference (YAML > `## 0. Status` > `## Status` > inline `**Status:**`) in BLUEPRINT.md §RFC Process amendment.
6. **F-P6.1-3 actionable closure**: Apply per-RFC STALE pin fixes per actionable enumeration.
7. **Final corpus STATE audit**: Verify post-fix coverage improvements.

## 8. R10.5 Scope Discipline Recap

Phase 6 R1 is RESEARCH DOC ONLY (analysis + corpus audit). NO substrate crate code edits. NO RFC text edits (those deferred to R2-R5 in-scope edit work). NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

The R3 Guard 2 script edit (`scripts/validate_cites.sh`) is in gray area — proposed for user-instruction verification before application.

## 9. Cross-References

- Phase 5 R6 doc: `docs/research/2026-08-22-phase-5-r6-dry-closure.md` (commit `2bd4e21f`)
- Phase 5 R5 doc: `docs/research/2026-08-22-phase-5-r5-vh-cohort-final-audit.md` (commit `409658f9`)
- Phase 5 R4 F-P5.2-3 framework: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md` 6-category framework
- Phase 4 R5 F-P4.5-4 PASS: `docs/research/2026-08-22-rfc-promotion-cascade-r5-freshness-audit.md` (bounded 9-RFC audit)
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Long-horizon plan v1.5: Phase 6 Long-Tail Maintenance

## 10. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R1 corpus STATE consolidation research; 4 NEW findings (1 CRIT + 1 HIGH + 1 MED + 1 LOW). F-P6.1-3 CRIT: 176 STALE version pins corpus-wide (extends R5 bounded result with corpus-wide reality). F-P6.1-1 HIGH: 149/175 RFCs lack YAML frontmatter. F-P6.1-2 MED: 5 status header patterns corpus-wide. F-P6.1-4 LOW: top-15 RFCs cited 2,000+ times corpus-wide (high-impact drift surface). Convergence: R1=4 NEW (initial cohort); R2 expected to enumerate F-P6.1-3 actionable subset via F-P5.2-3 framework. Phase 6 R2-R6 plan: actionable enumeration + Guard 2 deployment + long-tail closures + DRY by R5+R6. |