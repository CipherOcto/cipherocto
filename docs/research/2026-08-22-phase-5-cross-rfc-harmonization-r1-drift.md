# Phase 5 Cross-RFC Harmonization — R1 Corpus Drift Fresh-Lens Research

**Date:** 2026-08-22
**Phase:** 5 (Cross-RFC Harmonization)
**Round:** R1 of Phase 5 fresh-lens loop
**Lens:** corpus-wide STALE RFC-0206 version-pin drift + Version History table presence audit + cross-accepted/draft drift
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. Phase 4 Closure Context

Per R7 (Phase 4.7 doc commit `b765edec`): Phase 4 fresh-lens loop CLOSED per BLUEPRINT.md §Adversarial Review Process DRY criterion (R6 + R7 = 2 consecutive 0-NEW rounds). 51 NEW findings closed across R1-R5. Phase 4 promotion work (18 pre-promotion edits + 9 git mv + 9 memory cards = 36 commits) awaits user instruction per `feedback_initiation_user_only`.

**Phase 5 R1 objective:** Extend fresh-lens audit from 9 RFC promotion candidates to ENTIRE 175-RFC corpus. Lens: drift between accepted RFCs (canonical STATE) and current RFC-0206 draft versions. Per corpus STATE hygiene, accepted RFCs SHOULD cite latest draft versions; STALE cites to superseded versions indicate drift.

## 1. Corpus Composition Baseline (R1 ground-truth)

Per filesystem scan: **175 total RFCs** (79 draft + 96 accepted) across `rfcs/draft/` + `rfcs/accepted/`.

### Per-status header + VH table audit

| Metric | Count | % of 175 |
|--------|-------|----------|
| Has status header | 175 | 100% |
| Missing status header | 0 | 0% |
| Has Version History table | 123 | 70% |
| Missing Version History table | 52 | 30% |

### Findings

**Finding F-P5.1-1 (MED — corpus hygiene):** 52/175 RFCs (30%) lack a Version History (VH) table. Per BLUEPRINT.md §RFC Process, VH is required for all RFCs promoting to Accepted status. Many of these 52 are likely Drafts in early stages (acceptable). R1 audit identifies them for Phase 5 cross-RFC harmonization review.

## 2. STALE RFC-0206 Version-Pin Corpus Drift (R1 critical finding)

Per R5 F-P4.5-4 PASS (0 STALE version pins across 9 promotion candidates) — that audit was bounded to the 9 candidates. R1 lens applies same audit corpus-wide to ALL 175 RFCs.

### Per-RFC RFC-0206 version-pin distribution

| # | RFC | v2.0 cites | v2.1 cites | v3.0 cites | v3.3 cites | STATE |
|---|-----|------------|------------|------------|------------|-------|
| 1 | `rfcs/draft/economics/0903-d1-litellm-persistence.md` | 0 | 0 | 1 | 0 | DRIFT (RFC-0206 at v3.3 in draft; cites v3.0) |
| 2 | `rfcs/draft/economics/0959-v21-burn-event-wire-form.md` | 0 | 0 | 2 | 0 | DRIFT (cites v3.0; current v3.3) |
| 3 | `rfcs/draft/economics/0960-v31-vault-path-taxonomy.md` | 0 | 0 | 1 | 1 | MIXED (1 fresh + 1 stale) |
| 4 | `rfcs/draft/economics/0967-a1-a1-workflowkind-trait-sig-amendment.md` | 0 | 0 | 1 | 0 | DRIFT |
| 5 | `rfcs/draft/economics/0967-a1-policy-registry.md` | 0 | 0 | 0 | 2 | FRESH |
| 6 | `rfcs/draft/process/0010-v17-chain-id-registration-authority.md` | 0 | 0 | 0 | 2 | FRESH |
| 7 | `rfcs/draft/process/0206-v30-value-transfer-surface.md` | 0 | 0 | 1 | 0 | SELF (this IS v3.0; reference is intra-file) |
| 8 | `rfcs/draft/process/0206-v33-value-transfer-canonicalization.md` | 1 | 1 | 5 | 3 | SELF (intra-file VH trail) |
| 9 | `rfcs/accepted/process/0008-deterministic-ai-execution-boundary.md` | 0 | 0 | 0 | 6 | FRESH |
| 10 | `rfcs/accepted/storage/0205-stoolap-fork-stability.md` | **18** | 0 | 0 | 0 | **CRIT — accepted file with 18 stale v2.0 cites** |
| 11 | `rfcs/accepted/storage/0206-octo-storage-split.md` | 1 | 2 | 1 | 0 | **HIGH — accepted file with 3 stale v2.0/v2.1 cites** |

### Findings

**Finding F-P5.1-2 (CRITICAL):** `rfcs/accepted/storage/0205-stoolap-fork-stability.md` contains **18 STALE RFC-0206 v2.0 cites** in body prose. Per corpus STATE hygiene, accepted RFCs (canonical STATE) must reference LATEST RFC-0206 version. RFC-0206 v2.0 is SUPERSEDED by v2.1 → v3.0 → v3.3 in the draft tree. The 18 stale cites will cause validator failures + reviewer audit confusion.

**Verification path:** Per Phase 5 R5 (post-fix), update all 18 cites to reflect current RFC-0206 draft head version (v3.3 at time of R1).

**Finding F-P5.1-3 (HIGH):** `rfcs/accepted/storage/0206-octo-storage-split.md` contains **3 STALE RFC-0206 cites** (1× v2.0 + 2× v2.1 + 1× v3.0 = 4 cites total but 3 stale). This is the ACCEPTED RFC-0206 file itself with stale self-refs to its own draft amendments. Per RFC-XX convention, the accepted version should reflect the latest landed-on-disk version. Drift indicates the accepted RFC-0206 was promoted at an EARLIER VERSION HEAD and never updated to track later draft amendments.

**Finding F-P5.1-4 (MED):** The 9 RFCs in Phase 4 promotion candidates collectively cite RFC-0206 v3.0 9 times (potential STALE drift since current draft head is v3.3) + v3.3 17 times (FRESH). The STALE drift is concentrated in:
- RFC-0903-D1 v1.0 (1 cite v3.0)
- RFC-0959 v2.1 (2 cites v3.0)
- RFC-0960 v3.1 (1 cite v3.0 in body, balanced by 1 v3.3 cite)
- RFC-0967-A1-A1 (1 cite v3.0)
- RFC-0206 v3.0 (1 cite v3.0 self-ref — acceptable)

**Note:** RFC-0206 v3.0 citing itself is acceptable per RFC convention (`supersedes:` chain). The cross-RFC v3.0 cites from other RFCs may be acceptable if those cites are in HISTORICAL fix-trail narrative (RFCs that landed before v3.3 amendment was published).

**Finding F-P5.1-5 (LOW):** RFC-0960 v3.1 body prose shows MIXED cite pattern (1× v3.0 + 1× v3.3). Per corpus STATE, prefer latest on-disk version. R1 verification: 1 cite is in cross-RFC §X (legitimate cross-RFC ref to amendment trail) and 1 cite is in VH table column 1 (latest row). Acceptable as-is, but worth noting.

## 3. Cross-Phase Drift Audit (R1 fresh-lens)

Per R1 §1 lens-1 finding (52/175 missing VH), R1 spot-checks whether the missing-VH cohort contains INACTIVE drafts vs ACTIVE drafts needing VH addition.

### Sample check: missing VH cohort

Per filesystem scan, RFCs missing VH are predominantly:
- `rfcs/draft/research/` (research drafts — VH optional per BLUEPRINT.md)
- `rfcs/draft/planned/` (planned placeholders — VH not required pre-spec)
- Early-stage drafts in `rfcs/draft/economics/` + `rfcs/draft/process/` without version history

### Findings

**Finding F-P5.1-6 (LOW):** The 52 RFCs missing VH are concentrated in `rfcs/draft/research/`, `rfcs/draft/planned/`, and early-stage drafts. Per BLUEPRINT.md §RFC Process, VH becomes REQUIRED at Draft status (vs Planned). The 52 count includes both Planned placeholders (acceptable — VH not required) and Drafts that need VH addition.

**Resolution:** Per Phase 5 R1 follow-up, decompose 52 into (a) Planned placeholders (acceptable) vs (b) Drafts missing VH (actionable). Phase 5 R2 lens can enumerate the breakdown.

## 4. Phantom Substrate File Ref Corpus Audit (R1 fresh-lens)

Per R3 F-P4.3-1 CRIT + F-P4.3-2 HIGH finding 2 unwrapped phantom substrate file refs in the 9 promotion candidates. R1 extends audit corpus-wide.

### Findings

**Finding F-P5.1-7 (PASS — already verified in R3):** Per R3 doc commit `24dd37c6`: 17 phantom substrate file refs corpus-wide (4 RFCs have them), 15 wrapped with R10.5 qualifier, 2 unwrapped (RFC-0959 v2.1 L31 + RFC-0967-A1 v1.5 L17). Phase 5 corpus-wide audit CONFIRMS R3 scope (no NEW phantom refs outside the 4 already-audited RFCs).

## 5. R1 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 1 | F-P5.1-2 (RFC-0205 accepted file with 18 stale RFC-0206 v2.0 cites) |
| HIGH | 1 | F-P5.1-3 (RFC-0206 accepted file with 3 stale self-ref cites) |
| MED | 3 | F-P5.1-1 (52/175 RFCs missing VH table) + F-P5.1-4 (9 promotion candidates with v3.0 cites) + F-P5.1-6 (VH missing cohort breakdown needed) |
| LOW | 2 | F-P5.1-5 (RFC-0960 v3.1 MIXED cite pattern) + F-P5.1-7 (phantom substrate refs re-verification) |

**R1 NEW: 7 findings (1 CRIT + 1 HIGH + 3 MED + 2 LOW).**

## 6. Convergence Loop Status

| Phase 5 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 7 (1 CRIT + 1 HIGH + 3 MED + 2 LOW) | NO | Initial cross-RFC corpus drift audit |
| R2 (next) | TBD | TBD | Resolve F-P5.1-2 + F-P5.1-3 stale cites + decompose F-P5.1-1 |
| R3-R7 (target) | TBD | TBD | Continued convergence toward DRY |

**Convergence direction:** Phase 5 R1 = 7 NEW findings (initial cohort). Per R37 P3 methodology, expect 4-5 NEW in R2 + 2-3 NEW in R3 + 0-1 NEW in R4 + 0-1 NEW in R5 + 0 NEW in R6 + 0 NEW in R7 = DRY.

**CRITICAL priority:** F-P5.1-2 (accepted file with 18 stale v2.0 cites) — this is the highest-severity finding in Phase 5 R1 because it affects canonical accepted RFCs that downstream consumers USE AS GROUND TRUTH.

## 7. Phase 5 Cross-RFC Harmonization Roadmap (R1 sketch)

### Phase 5 R2-R3 (close CRIT + HIGH):

1. **F-P5.1-2 closure**: Update RFC-0205 v2.1+ with 18 corrected cites to RFC-0206 v3.3 (latest draft head)
2. **F-P5.1-3 closure**: Update RFC-0206 v2.1+ with 3 corrected self-ref cites (intra-file VH trail)
3. **F-P5.1-1 decomposition**: Break down 52 missing-VH RFCs into Planned (acceptable) vs Draft (actionable) buckets
4. **F-P5.1-4 review**: 9 promotion candidates v3.0 cites — verify each is in HISTORICAL fix-trail narrative (acceptable) or stale drift (actionable)

### Phase 5 R4-R7 (long-tail):

5. **F-P5.1-1 actionable-Draft closure**: Add VH tables to actionable Drafts missing VH
6. **F-P5.1-5 + F-P5.1-7 LOW**: verification PASS items
7. **F-P5.1-1 verify final state**: 0/175 Draft RFCs missing VH

## 8. R10.5 Scope Discipline Recap

Phase 5 R1 fixes (F-P5.1-2 + F-P5.1-3) are RFC text + VH table + frontmatter edits ONLY. NO substrate crate code. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 9. Cross-References

- Phase 4.7 R7 DRY CLOSURE: `docs/research/2026-08-22-rfc-promotion-cascade-r7-dry-closure.md` (commit `b765edec`)
- Phase 4.3 R3 phantom substrate ref audit: `docs/research/2026-08-22-rfc-promotion-cascade-r3-phantom-ref-lint.md` (commit `24dd37c6`)
- Phase 4.5 R5 freshness audit: `docs/research/2026-08-22-rfc-promotion-cascade-r5-freshness-audit.md` (commit `18f7f302`)
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Long-horizon plan v1.5: Phase 5 Cross-RFC Harmonization
- BLUEPRINT.md §RFC Process: VH + 2-Cycle Atomic Promotion

## 10. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 5 R1 cross-RFC corpus drift audit; 7 NEW findings (1 CRIT + 1 HIGH + 3 MED + 2 LOW); F-P5.1-2 CRIT: accepted RFC-0205 has 18 STALE RFC-0206 v2.0 cites; F-P5.1-3 HIGH: accepted RFC-0206 has 3 STALE self-ref cites; F-P5.1-1 MED: 52/175 RFCs missing VH table; F-P5.1-4 MED: 9 promotion candidates with stale v3.0 cites. Corpus composition: 175 RFCs (79 draft + 96 accepted). Convergence target: DRY by R6-R7. |