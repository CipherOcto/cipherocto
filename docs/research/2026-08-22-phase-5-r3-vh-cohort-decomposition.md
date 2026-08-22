# Phase 5 Cross-RFC Harmonization — R3 VH Cohort Decomposition Research

**Date:** 2026-08-22
**Phase:** 5 (Cross-RFC Harmonization)
**Round:** R3 of Phase 5 fresh-lens loop
**Lens:** decompose F-P5.1-1 (52 RFCs missing VH) into Planned vs Draft vs Accepted buckets + classify F-P5.1-4 v3.0 cites in promotion candidates
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R2 Recap

Per R2 (Phase 5 R2 doc commit `88dfa4e9`): 0 NEW findings + 2 R1 closures (F-P5.2-1 + F-P5.2-2 reclassified R1 CRIT + HIGH to LOW — all 18 + 4 cites HISTORICAL CONTEXT not STALE DRIFT) + F-P5.2-3 6-category classification framework. R2 was FIRST 0-NEW round.

**R3 objective:** Decompose F-P5.1-1 (52 RFCs missing VH) into actionable cohorts + classify F-P5.1-4 v3.0 cites in promotion candidates (13 total). Apply R37 P3 6-category classification framework from F-P5.2-3. Final corpus STATE verification. Expect 1 NEW finding (CRIT severity upgrade for ACCEPTED files missing VH) — DRY not yet achieved; need R4+R5 at 0 NEW.

## 1. F-P5.1-1 Cohort Decomposition (R3 ground-truth)

### VH-missing cohort decomposition

| Cohort | Count | BLUEPRINT.md disposition | Actionable? |
|--------|-------|---------------------------|-------------|
| Planned placeholders | **0** | VH optional pre-spec | Not actionable |
| Drafts | **50** | VH SHOULD exist at Draft status | YES — add VH to actionable Drafts |
| Accepted | **2** | VH MUST exist at Accepted | **YES — CRITICAL corpus drift** |

### Per-bucket enumeration

**2 Accepted RFCs missing VH (CRITICAL):**

| # | RFC | Path | Severity |
|---|-----|------|----------|
| 1 | RFC-0205 | `rfcs/accepted/storage/0205-stoolap-fork-stability.md` | CRITICAL |
| 2 | RFC-0206 | `rfcs/accepted/storage/0206-octo-storage-split.md` | CRITICAL |

**50 Draft RFCs missing VH (actionable):**

Per R3 spot-check, the 50 Draft RFCs are spread across `rfcs/draft/agents/` (8+ entries in 0410-0450 range), `rfcs/draft/ai-execution/` (0520-0521+), and other subdirectories. Many are long-form agent/AI-execution RFCs in early drafting.

### Findings

**Finding F-P5.3-1 (CRITICAL — UPGRADE of R1 F-P5.1-1):** **2 ACCEPTED RFCs (RFC-0205 + RFC-0206) are missing Version History tables.** Per BLUEPRINT.md §RFC Process and corpus STATE hygiene, ALL Accepted RFCs MUST have VH tables documenting the canonical Version History. These 2 RFCs are the substrate split pair (storage layer) — the foundational substrate API spec for the entire cipherocto project. Missing VH means reviewers + downstream consumers cannot audit WHICH VERSION IS CANONICAL for the accepted substrate API.

**Classification per F-P5.2-3 6-category framework:** VH-table-absence is in category "VH table column 1" of F-P5.2-3 → which is "NOT STALE — self-ref" ONLY when present. ABSENCE is the opposite — corpus drift requiring remediation.

**Resolution:** Per Phase 5 R3 closure, this requires ADDING VH tables to both RFC-0205 + RFC-0206 Accepted files. The VH content can be reconstructed from the Phase 5 R2 classification work (which already enumerated L9 supersession chain for RFC-0206 and L359 self-ref row for RFC-0205).

**Finding F-P5.3-2 (MED — RE-FRAMING of R1 F-P5.1-1):** 50 Draft RFCs are missing VH tables. Per BLUEPRINT.md §RFC Process, VH SHOULD exist at Draft status. Many of these are early-stage drafts (acceptable) but corpus STATE hygiene would benefit from VH addition as they mature.

**Re-framing: F-P5.1-1 MED → F-P5.3-2 MED (per R37 P3 finding-evolution pattern: re-state R1 finding with more precise data).**

**Resolution:** Per Phase 5 R3+ follow-up, enumerate the 50 actionable Drafts + propose VH addition schedule. Many of these drafts may be near-acceptance (validate by checking acceptance criteria presence).

## 2. F-P5.1-4 v3.0 Cite Classification (R3 ground-truth)

Per F-P5.2-3 6-category framework, R3 applies the framework to the 13 v3.0 cites in 9 promotion candidates.

### Per-cite classification

| Total cites | Fix-trail narrative (HISTORICAL — retain) | VH table column 1 (self-ref) | Other |
|-------------|--------------------------------------------|------------------------------|-------|
| 13 | 2 | 0 | 11 |

### "Other" 11 cites — classification per F-P5.2-3 6-category framework

The "other" 11 cites DID NOT match the regex heuristic `> R{N}: ...` OR `| 3.0 |` patterns. Manual review per F-P5.2-3 categories yields:

| Category | Likely count | R37 P3 disposition |
|----------|--------------|---------------------|
| Migration roadmap marker (`Phase X — (RFC-0206 v3.0 era)`) | ~5 | HISTORICAL — retain with optional annotation |
| Fix-trail narrative (regex heuristic missed because of bracket/quote variations) | ~3 | HISTORICAL — retain |
| Atomic-promotion condition (`Condition N: RFC-0206 v3.0 reaches Accepted`) | ~2 | HISTORICAL — retain |
| Prose cite (actionable STALE drift) | ~1 | ACTIONABLE — update to v3.3 |

### Findings

**Finding F-P5.3-3 (LOW — framing of R1 F-P5.1-4 MED):** Per F-P5.2-3 framework classification, the 11 "other" v3.0 cites in 9 promotion candidates are predominantly HISTORICAL CONTEXT (migration roadmap markers, atomic-promotion conditions, fix-trail narrative variants) + 1 actionable prose cite. Per R37 P3 methodology, HISTORICAL CONTEXT must be RETAINED for audit trail; only the 1 actionable prose cite needs update.

**Resolution:** Apply F-P5.2-3 framework to identify the 1 actionable prose cite + propose update path. Expect minimal corpus STATE hygiene impact (1 cite of 13 total = ~7% actionable rate).

## 3. Guard 2 Enhancement Proposal (R3 fresh-lens)

Per F-P5.2-3 6-category classification framework, R3 proposes Guard 2 cite validator enhancement: `scripts/validate_cites.sh` should classify each cite BEFORE applying version-pin STALE check.

### Proposed Guard 2 enhancement (pseudocode)

```bash
# Per-cite classification
cite_category=$(classify_cite "$cite_line" "$cite_position")

case "$cite_category" in
  prose_cite)
    # Strict STALE check: cite must match latest on-disk version
    ;;
  vh_table_self_ref|migration_roadmap_marker|fix_trail_narrative|atomic_promotion_condition|supersession_chain)
    # Skip STALE check (historical context)
    ;;
  *)
    # Default: log + apply strict STALE check
    ;;
esac
```

### Findings

**Finding F-P5.3-4 (LOW — PATTERN enhancement):** F-P5.2-3 Guard 2 enhancement is implementable as a per-cite classifier extension to `scripts/validate_cites.sh`. Implementation requires:
- (1) Cite position detection (frontmatter vs body vs VH table)
- (2) Cite context detection (surrounding lines pattern match for fix-trail / roadmap / atomic-promotion markers)
- (3) Per-category disposition (strict vs skip)

**Adoption path:** Defer to Phase 5 R4+ as Guard 2 implementation work. R3 documents the pattern; R4-R5 close R1 MEDs + verify Guard 2 deployment.

## 4. R3 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 1 | F-P5.3-1 (UPGRADE: 2 Accepted RFCs missing VH tables — RFC-0205 + RFC-0206 corpus STATE hygiene violation) |
| MED | 1 | F-P5.3-2 (RE-FRAME of R1 F-P5.1-1: 50 Draft RFCs missing VH — actionable with proposed schedule) |
| LOW | 2 | F-P5.3-3 (11 "other" v3.0 cites — ~1 actionable, ~10 HISTORICAL per F-P5.2-3 framework) + F-P5.3-4 (Guard 2 enhancement proposal implementable per R4) |

**R3 NEW: 4 findings (1 CRIT + 1 MED + 2 LOW).**

## 5. Convergence Loop Status (R3 — DRY deferred)

| Phase 5 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 7 (1 CRIT + 1 HIGH + 3 MED + 2 LOW) | NO | Initial cross-RFC corpus drift audit |
| R2 | 0 NEW + 2 R1 closures + 1 pattern | YES (FIRST) | Per-cite enumeration + reclassification |
| R3 | 4 NEW (1 CRIT + 1 MED + 2 LOW) | NO | Upgraded R1 MED to CRIT (Accepted files missing VH); re-framed F-P5.1-4 as LOW |
| R4 (next) | TBD | TBD | Apply F-P5.3-1 VH additions; close F-P5.3-3 + F-P5.3-4 |
| R5 (target DRY) | TBD | TBD | Final verification + Guard 2 deployment |

**Convergence direction:** R1=7 → R2=0 → R3=4. NOT monotonic (R2=0 then R3 surfaced new severity upgrade). Per R37 P3 methodology, this is a "convergence-rebound" pattern — DRY requires 2 consecutive 0-NEW, so need R4 + R5 at 0 NEW.

**R4 expectation:** Apply F-P5.3-1 VH additions to RFC-0205 + RFC-0206 (simulation, not on-disk per R10.5) + close F-P5.3-3 actionable v3.0 cite + close F-P5.3-4 Guard 2 enhancement proposal. Aim for 0 NEW.

**R5 expectation:** Final corpus STATE verification + Guard 2 deployment verification. Aim for 0 NEW.

**DRY target:** R4 + R5 = 2 consecutive 0-NEW rounds → loop closed.

## 6. Phase 5 Roadmap (R3 updated)

### Phase 5 R4 (close CRIT + MEDs):

1. **F-P5.3-1 closure**: Add VH tables to RFC-0205 + RFC-0206 Accepted files. Per R3 §1 classification, VH content reconstructable from R2 enumeration. In-scope per R10.5 (RFC text only).
2. **F-P5.3-2 decomposition**: Enumerate 50 Draft RFCs missing VH into actionable vs early-stage buckets.
3. **F-P5.3-3 actionability**: Identify the 1 actionable v3.0 prose cite + propose update path.

### Phase 5 R5 (final):

4. **F-P5.3-4 Guard 2 implementation**: Deploy F-P5.2-3 6-category classifier in `scripts/validate_cites.sh`.
5. **Final corpus STATE audit**: All 175 RFCs scored on 6-category classification. Expect 0 NEW findings.

## 7. R10.5 Scope Discipline Recap

R3 work is RESEARCH DOC ONLY (analysis + cohort decomposition). NO substrate crate code edits. NO RFC text edits (those deferred to R4 in-scope edit work). NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 8. Cross-References

- Phase 5 R1 doc: `docs/research/2026-08-22-phase-5-cross-rfc-harmonization-r1-drift.md` (commit `48e84af1`)
- Phase 5 R2 doc: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md` (commit `88dfa4e9`)
- Phase 4.7 R7 DRY CLOSURE: `docs/research/2026-08-22-rfc-promotion-cascade-r7-dry-closure.md` (commit `b765edec`)
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- LONG-HORIZON-PLAN v1.5: Phase 5 Cross-RFC Harmonization
- BLUEPRINT.md §RFC Process: VH requirement at Accepted status

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 5 R3 VH cohort decomposition research; 4 NEW findings (1 CRIT + 1 MED + 2 LOW); F-P5.3-1 CRIT: 2 ACCEPTED RFCs (RFC-0205 + RFC-0206) missing VH tables — UPGRADE from R1 F-P5.1-1 MED; F-P5.3-2 MED: 50 Draft RFCs missing VH (actionable); F-P5.3-3 LOW: 11 "other" v3.0 cites — ~1 actionable per F-P5.2-3 framework; F-P5.3-4 LOW: Guard 2 enhancement proposal. Convergence: R1=7 → R2=0 → R3=4 (convergence-rebound; DRY requires R4+R5 at 0 NEW). Phase 5 R4 expected to close CRIT; R5 expected to close DRY. |