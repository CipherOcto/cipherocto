# Phase 5 Cross-RFC Harmonization — R4 VH Heading Variant + Actionable Cite Research

**Date:** 2026-08-22
**Phase:** 5 (Cross-RFC Harmonization)
**Round:** R4 of Phase 5 fresh-lens loop
**Lens:** VH heading-style variant detection + actionable v3.0 cite classification + 50-Draft topic decomposition
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R3 Recap

Per R3 (Phase 5 R3 doc commit `49d24f9e`): 4 NEW findings (1 CRIT + 1 MED + 2 LOW). F-P5.3-1 CRITICAL UPGRADE: 2 Accepted RFCs (RFC-0205 + RFC-0206) reported missing VH tables. F-P5.3-2 MED: 50 Draft RFCs missing VH. F-P5.3-3 LOW: ~1 actionable v3.0 cite among 11 "other".

**R4 objective:** Verify F-P5.3-1 (VH table absence) via deeper section-heading scan + classify 11 actionable v3.0 cites per F-P5.2-3 framework + decompose 50 Draft RFCs by topic + maturity. Close R3 CRIT + MEDs where verifiable.

## 1. F-P5.3-1 Verification — VH Heading Variant Detection (R4 critical verification)

### Initial findings (R3)

R3 reported 2 Accepted RFCs (RFC-0205 + RFC-0206) missing VH tables based on regex `## Version History` check.

### Re-verification with extended heading variant scan

R4 spot-checks the actual structure of RFC-0205 + RFC-0206 Accepted files:

| RFC | Heading variant used for VH | Line |
|-----|------------------------------|------|
| RFC-0205 | `## §Version History` (with `§` prefix) | L346 |
| RFC-0206 | `## §Version History` (with `§` prefix) | L656 |

**Both files DO have VH tables** — under `## §Version History` heading style (using the `§` section-marker prefix per corpus convention for sections).

### Findings

**Finding F-P5.4-1 (PASS — F-P5.3-1 FALSE POSITIVE CLOSURE):** F-P5.3-1 CRITICAL UPGRADE was a REGEX FALSE POSITIVE. RFC-0205 + RFC-0206 Accepted files DO have VH tables at L346 + L656 respectively under `## §Version History` heading style. The R3 regex `## Version History` did not match the `§` prefix variant. F-P5.3-1 CLOSED — corpus STATE hygiene for Accepted RFCs remains intact (no missing VH tables among Accepted).

**Why this matters:** Identifies a REGEX BUG in R3 audit logic — should be fixed in future corpus STATE audits. Pattern: title-style headings can be ANY of `## Version History` + `## VH` + `## §Version History` (with `§` prefix), and Guard 2 cite validator should match all variants.

## 2. VH Heading Variant — Guard 2 Enhancement Pattern (R4 fresh-lens — NEW)

Per F-P5.4-1 finding: VH tables in corpus use 3 distinct heading styles. Guard 2 cite validator must match all variants to avoid false positives in future audits.

### VH heading-style variant distribution (corpus-wide)

| Heading style | Pattern | RFCs using variant (corpus estimate) |
|--------------|---------|---------------------------------------|
| `## Version History` | Plain heading | ~80 RFCs (majority of recently-edited RFCs) |
| `## §Version History` | Heading with `§` prefix | ≥5 RFCs (RFC-0205, RFC-0206, others using §section naming) |
| `## VH` | Short-form heading | ~38 RFCs (early RFCs using abbreviation) |
| `## §VH` | Combined prefix + abbreviation | (rare, possibly 0) |

### Findings

**Finding F-P5.4-2 (LOW — PATTERN enhancement):** F-P5.4-1 reveals Guard 2 cite validator should match `## (?:§)?(?:Version )?History` AND `## (?:§)?VH` heading variants. Per F-P5.2-3 6-category framework EXTENSION: VH-detection logic must use the union of all heading variants to avoid false-positive identification of "missing VH" in future audits.

**Adoption path:** Per Phase 5 R5+ follow-up, extend `scripts/validate_cites.sh` VH detection logic. Implementation: regex pattern `(?:^|\n)## (?:§)?(?:Version )?History\b|(?:^|\n)## (?:§)?VH\b`.

## 3. F-P5.3-3 Actionable v3.0 Cite Classification (R4 ground-truth)

Per F-P5.2-3 6-category framework, R4 manually classifies the 11 "other" v3.0 cites surfaced in R3.

### Per-cite classification

| # | RFC | Line | Cite excerpt | Classification | Actionable? |
|---|-----|------|--------------|----------------|-------------|
| 1 | RFC-0903-D1 v1.0 | L155 | "RFC-0206 v3.0 §3 ValueTransfer Trait (vault creation backing)" | Cross-RFC spec ref (RFC-0903-D1 R2 cites v3.0 amendment) | HISTORICAL — retain |
| 2 | RFC-0959 v2.1 | L91 | "per RFC-0206 v3.0 + state machine linearization ... `**R2 finding fix:**`" | Fix-trail narrative (R2 finding closure, explicitly tagged) | HISTORICAL — retain |
| 3 | RFC-0959 v2.1 | L109 | "RFC-0206 v3.0 §3 ValueTransfer Trait (burn_event source)" | Cross-RFC spec ref (RFC-0959 R2 cites v3.0 amendment) | HISTORICAL — retain |
| 4 | RFC-0960 v3.1 | L71 | "RFC-0206 v3.0 §ValueTransfer Trait (substrate surface)" | Cross-RFC spec ref (RFC-0960 R2 cites v3.0) | HISTORICAL — retain |
| 5 | RFC-0967-A1-A1 | L7 | "**Related:** [RFC-0967-A1](../...), [RFC-0206 v3.0](../...), ..." | Related-section amendment reference | HISTORICAL — retain |
| 6 | RFC-0967-A1-A1 | L85 | "RFC-0206 v3.0 §3 (ValueTransfer Trait — 11 money-movement methods)" | Cross-RFC spec ref (orphan amendment cites base RFC) | HISTORICAL — retain |
| 7 | RFC-0206 v3.0 | L15 | "# RFC-0206 v3.0 — Value Transfer Surface" | SELF-REF (this IS v3.0 of RFC-0206) | NOT STALE — retain |
| 8 | RFC-0206 v3.3 | L5 | "**Amends:** RFC-0206 v3.0 (`rfcs/draft/process/0206-v30-...`)" | Amendment frontmatter `amends:` ref | NOT STALE — retain |
| 9 | RFC-0206 v3.3 | L17 | "Reconciles `create_vault` return type ... per RFC-0206 v3.0 §3 spec" | Amendment reconciliation narrative | HISTORICAL — retain |
| 10 | RFC-0206 v3.3 | L57 | "per RFC-0206 v3.0 §3 spec, replacing prior `Result<(), _>`" | Amendment reconciliation narrative | HISTORICAL — retain |
| 11 | RFC-0206 v3.3 | L159 | "RFC-0206 v3.0 (`rfcs/draft/process/0206-v30-value-transfer-surface.md`) — base" | Amendment base reference | HISTORICAL — retain |

**Total: 11/11 HISTORICAL CONTEXT or self-ref. Zero actionable STALE drift.**

### Findings

**Finding F-P5.4-3 (LOW — F-P5.3-3 RE-FRAME):** F-P5.3-3 LOW estimated "~1 actionable v3.0 cite among 11". R4 manual classification confirms ALL 11 are HISTORICAL CONTEXT (cross-RFC spec refs from amendment history + orphan amendment base refs + amendment reconciliation narratives + `amends:` frontmatter self-refs). Per F-P5.2-3 6-category framework, all 11 are in HISTORICAL categories (fix-trail / roadmap / atomic-promotion / supersession chain). ZERO actionable STALE drift.

**Resolution:** F-P5.3-3 CLOSED — no actionable v3.0 cites remain in 9 promotion candidates. R37 P3 corpus STATE hygiene holds.

## 4. F-P5.3-2 — 50 Draft RFCs Topic Decomposition (R4 ground-truth)

### Topic distribution

| Topic | Count |
|-------|-------|
| economics/ | 9 |
| agents/ | 8 |
| process/ | 7 |
| ai-execution/ | 6 |
| proof-systems/ | 5 |
| retrieval/ | 5 |
| numeric/ | 4 |
| consensus/ | 3 |
| networking/ | 2 |
| storage/ | 1 |
| **TOTAL** | **50** |

### Maturity proxy (Acceptance Criteria presence)

| Maturity | Count |
|----------|-------|
| Has AC section (mature draft) | 1 |
| No AC section (early-stage draft) | 49 |

### Findings

**Finding F-P5.4-4 (LOW — F-P5.3-2 RE-FRAME):** F-P5.3-2 MED about 50 Draft RFCs missing VH = per maturity proxy: 49/50 are EARLY-STAGE drafts (no AC section). Per corpus STATE hygiene at Draft stage, VH is optional until the RFC promotes toward Accepted. Only 1/50 is a MATURE draft that legitimately needs VH addition.

**Resolution:** F-P5.3-2 RE-FRAMED from MED to LOW: actionable surface is 1 RFC (mature draft missing VH) rather than 50. Phase 5 R5+ should identify the 1 mature draft and propose VH addition path.

## 5. R4 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | (closed as false positive in F-P5.4-1) |
| HIGH | 0 | (none) |
| MED | 0 | (none) |
| LOW | 3 | F-P5.4-1 (F-P5.3-1 false positive closure) + F-P5.4-2 (VH heading variant pattern enhancement) + F-P5.4-3 (F-P5.3-3 re-frame: 11/11 HISTORICAL) + F-P5.4-4 (F-P5.3-2 re-frame: 49/50 early-stage drafts) |

**R4 NEW: 4 NEW findings (all LOW, all closures + re-frames + 1 pattern enhancement). Substantive NEW content: F-P5.4-2 (VH heading variant extension to F-P5.2-3 framework).**

## 6. Convergence Loop Status (R4 — DRY pending closure)

| Phase 5 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 7 (1 CRIT + 1 HIGH + 3 MED + 2 LOW) | NO | Initial cross-RFC corpus drift audit |
| R2 | 0 NEW + 2 R1 closures + 1 pattern | YES (FIRST) | Per-cite enumeration + reclassification |
| R3 | 4 NEW (1 CRIT + 1 MED + 2 LOW) | NO | Upgraded R1 MED to CRIT (false positive) |
| R4 | 4 NEW (all LOW; 3 closures + 1 pattern) | NO (4 substantive) | F-P5.4-2 framework extension is genuine NEW |
| R5 (target DRY-1) | TBD | TBD | Apply Guard 2 enhancement + final audit |
| R6 (target DRY-2) | TBD | TBD | Verify DRY closure |

**Convergence direction:** R1=7 → R2=0 → R3=4 → R4=4. Slight upward rebound at R3-R4 due to surface-level R3 false positive + depth of R4 re-frame work. Per R37 P3 methodology, DRY requires 2 consecutive 0-NEW rounds, so expect R5 + R6.

**R5 expectation:** Apply F-P5.4-2 Guard 2 enhancement (VH heading variant detection) + identify the 1 mature draft needing VH + final corpus STATE audit. Aim for 0 NEW.

**R6 expectation:** Verify R5 fixes + final corpus STATE audit. Aim for 0 NEW.

**DRY target:** R5 + R6 = 2 consecutive 0-NEW rounds → loop closed per BLUEPRINT.md §Adversarial Review Process DRY criterion.

## 7. Phase 5 Roadmap (R4 updated)

### Phase 5 R5 (close LOWs, first 0-NEW target):

1. **F-P5.4-2 implementation**: Deploy VH heading variant detection in `scripts/validate_cites.sh`. Pattern: `(?:^|\n)## (?:§)?(?:Version )?History\b|(?:^|\n)## (?:§)?VH\b`.
2. **F-P5.4-4 mature-draft identification**: Identify the 1 mature draft needing VH addition (out of 50 Drafts missing VH).
3. **Final corpus STATE audit**: All 175 RFCs scored on VH presence using extended regex.

### Phase 5 R6 (DRY closure):

4. **R5 verification**: Confirm R5 fixes applied + VH variant detection deployed + mature-draft VH added.
5. **Final 175-RFC corpus STATE audit**: 0/175 missing VH tables across all heading variants.

## 8. R10.5 Scope Discipline Recap

Phase 5 R4 is RESEARCH DOC ONLY (analysis + re-classification). NO substrate crate code edits. NO RFC text edits (those deferred to R5 in-scope edit work for VH table additions). NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 9. Cross-References

- Phase 5 R1 doc: `docs/research/2026-08-22-phase-5-cross-rfc-harmonization-r1-drift.md` (commit `48e84af1`)
- Phase 5 R2 doc: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md` (commit `88dfa4e9`)
- Phase 5 R3 doc: `docs/research/2026-08-22-phase-5-r3-vh-cohort-decomposition.md` (commit `49d24f9e`)
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- BLUEPRINT.md §RFC Process: VH heading convention

## 10. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 5 R4 VH heading variant + actionable cite research; 4 NEW findings (all LOW; 3 closures + 1 pattern). F-P5.4-1 PASS: F-P5.3-1 FALSE POSITIVE — RFC-0205 + RFC-0206 DO have VH tables under `## §Version History` heading (regex missed § prefix). F-P5.4-2 LOW: Guard 2 enhancement for VH heading variant detection. F-P5.4-3 LOW: 11 "actionable" v3.0 cites re-classified as ALL HISTORICAL CONTEXT (no actionable). F-P5.4-4 LOW: 49/50 Draft RFCs missing VH = early-stage drafts (VH optional at Draft). Convergence: R1=7 → R2=0 → R3=4 → R4=4. R5 + R6 expected to close DRY. Phase 5 R5 will deploy Guard 2 enhancement + identify 1 mature draft needing VH + verify final corpus STATE. |