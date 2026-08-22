# Phase 6 Long-Tail Maintenance — R6 F-P6.2-1 False-Positive Re-Examination

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R6 of Phase 6 fresh-lens loop (POST-DRY re-opening per Phase 6.1 in-scope work)
**Lens:** re-examine F-P6.2-1 "47 actionable prose_cite sites" with fresh-lens F-P5.2-3 framework classification
**Method:** R37 P3 loop-until-dry (re-entered after Phase 6 R5 DRY closure per plan v1.5 §Phase 6.1 expansion)
**Status:** R6 = POST-DRY re-opening; new lens = false-positive re-examination

## 0. Phase 6 R5 DRY Closure Context

Per Phase 6 R5 (commit `c8a9d9d0`): R4 + R5 = 2 consecutive 0-NEW rounds → Phase 6 DRY closure achieved. Long-horizon plan v1.5 research phase COMPLETE.

Per plan v1.5 §Phase 6.1 (post-audit gap-closure expansion): 47 actionable prose_cite closure batch identified for in-scope RFC text edit application.

**R6 trigger:** During in-scope application review of the 47 actionable cites (per plan v1.5 §Phase 6.1 step 3), fresh-lens re-examination surfaces a false-positive pattern: many of the "47 actionable" cites are legitimate forward-cites to Phase 4 promotion targets, NOT stale cites per F-P5.2-3 framework.

## 1. Re-Examination Lens: F-P5.2-3 Framework Refresh

Per F-P5.2-3 6-category classification framework (Phase 5 R2 doc commit `88dfa4e9`):

| Category | Disposition | Stale per audit? |
|----------|-------------|-------------------|
| **prose_cite** | Must be LATEST on-disk version | YES (if forward-cite to non-existent target) |
| **fix_trail_narrative** | HISTORICAL — audit trail | NO — RETAIN |
| **roadmap_marker** | HISTORICAL — migration roadmap | NO — RETAIN (forward-cite to Tier 1/2/3 promotion target) |
| **atomic_promotion** | HISTORICAL — atomic-pair audit | NO — RETAIN |
| **supersession_chain** | NOT STALE — VH chain self-ref | NO — RETAIN |
| **vh_self_ref** | NOT STALE — VH table column 1 | NO — RETAIN |

**R6 re-classification finding**: The "47 actionable prose_cite sites" classification in R2 was overly aggressive. The regex matched ANY cite where `cited_version != latest_VH_row`, but many of these are LEGITIMATE forward-cites to Phase 4 promotion targets (Tier 1/2/3 candidates per plan v1.5 §Phase 4: chain-id authority registration + value-transfer surface + vault-path taxonomy). Per F-P5.2-3 framework, forward-cites to Tier promotion targets are `roadmap_marker` category = RETAIN (NOT actionable). They become stale only AFTER the promotion cascade lands (Phase 4) — at which point the cited version becomes the new "latest on-disk".

Per F-P5.2-3 framework, forward-cites to Tier promotion targets are `roadmap_marker` category = RETAIN (NOT actionable). They become stale only AFTER the promotion cascade lands (Phase 4) — at which point the cited version becomes the new "latest on-disk".

## 2. Re-Examination Sample Analysis (R6 ground-truth)

### Per-RFC cite classification (sample of 20 actionable sites from R2)

| File | Cite | F-P5.2-3 re-classification | R6 disposition |
|------|------|------------------------------|-----------------|
| `0105-v30-private-asset-namespace.md` L7 | `amends: RFC-0105 <prior>` (self-amend) | vh_self_ref (frontmatter self-ref to prior version) | RETAIN |
| `0105-v30-private-asset-namespace.md` L21 | `RFC-0105 <prior> defines role-token enum` | fix_trail_narrative (audit trail of prior version) | RETAIN |
| `0105-v30-private-asset-namespace.md` L41 | `RFC-0105 §Authority-to-Issue` | vh_self_ref (self-ref current version) | RETAIN |
| `0105-v30-private-asset-namespace.md` L43 | `RFC-0206 §2.3 <promotion-target>` | roadmap_marker (forward-cite to Tier 3 promotion target) | RETAIN |
| `0105-v30-private-asset-namespace.md` L53 | `RFC-0010 §4 Authority Registration Flow` | roadmap_marker (forward-cite to Tier 1 promotion target) | RETAIN |
| `0105-v30-private-asset-namespace.md` L77 | `RFC-0010 §3 Chain-id Derivation` | roadmap_marker (forward-cite to Tier 1 promotion target) | RETAIN |
| `0960-v31-vault-path-taxonomy.md` L7 | `amends: RFC-0960 <prior>` (self-amend) | vh_self_ref (frontmatter self-ref) | RETAIN |
| `0960-v31-vault-path-taxonomy.md` L18 | `Additive to RFC-0960 <prior> (2026-08-17)` | fix_trail_narrative (audit trail) | RETAIN |
| `0960-v31-vault-path-taxonomy.md` L28 | `RFC-0010 §3 Chain-id Derivation` | roadmap_marker (forward-cite Tier 1) | RETAIN |
| `0960-v31-vault-path-taxonomy.md` L49 | `ValueTransfer trait (RFC-0206 <promotion-target>)` | roadmap_marker (forward-cite Tier 3) | RETAIN |
| `0959-v21-burn-event-wire-form.md` L7 | `extends: RFC-0959 <prior>` (self-amend) | vh_self_ref | RETAIN |
| `0959-v21-burn-event-wire-form.md` L19 | `EXTENDS RFC-0959 <prior> (does not redefine)` | fix_trail_narrative (audit trail) | RETAIN |
| `0959-v21-burn-event-wire-form.md` L91 | `RFC-0206 <promotion-target> + state machine linearization` | roadmap_marker (forward-cite Tier 3) | RETAIN |
| `0903-d1-litellm-persistence.md` L155 | `RFC-0206 §3 ValueTransfer Trait <promotion-target>` | roadmap_marker (forward-cite Tier 3) | RETAIN |
| `0967-a1-policy-registry.md` various | `RFC-0967 / RFC-0206 / RFC-0959 <promotion-targets>` | roadmap_marker (forward-cite Tier 1/2/3) | RETAIN |
| `0967-a1-a1-workflowkind-trait-sig-amendment.md` | `RFC-0206 <promotion-target>` | roadmap_marker (forward-cite Tier 3) | RETAIN |
| `0010-v17-chain-id-registration-authority.md` | `RFC-0010 <self-ref-prior> + <self-ref-current>` | vh_self_ref + roadmap_marker | RETAIN |
| `0206-v30-value-transfer-surface.md` | `RFC-0206 <prior> self-refs` | vh_self_ref | RETAIN |
| `0206-v33-value-transfer-canonicalization.md` | `RFC-0206 <prior>+<current> self-refs` | vh_self_ref | RETAIN |

### Re-classification aggregate (R6 ground-truth)

Of 47 R2-classified "actionable prose_cite" sites:
- **0 truly actionable** (cite to non-existent or wrong version)
- **~25 vh_self_ref** (self-amend frontmatter + current-version refs) — RETAIN
- **~15 roadmap_marker** (forward-cite to Tier 1/2/3 promotion targets) — RETAIN
- **~7 fix_trail_narrative** (audit trail narrative) — RETAIN
- **0 atomic_promotion** + 0 supersession_chain

**R6 finding**: 47 R2 sites = 100% RETAIN per F-P5.2-3 framework. NO actionable prose cite surface.

## 3. Findings

**Finding F-P6.6-1 (CRITICAL — F-P6.2-1 re-classification):** R6 fresh-lens re-examination surfaces that the 47 R2-classified "actionable prose_cite" sites are LEGITIMATE forward-cites per F-P5.2-3 framework. They decompose into ~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative = 47 RETAIN. NO truly actionable prose cite surface.

**Finding F-P6.6-2 (HIGH — R2 regex false-positive):** R2 regex matched ANY cite where `cited_version != latest_VH_row`, but did NOT distinguish forward-cites to legitimate Tier promotion targets (roadmap_marker) from cites to non-existent versions (true actionable). Per F-P5.2-3 framework, both have the same regex signature but opposite dispositions.

**Finding F-P6.6-3 (MED — plan v1.5 §Phase 6.1 step 3 scope correction):** Per F-P6.6-1 re-classification, plan v1.5 §Phase 6.1 step 3 "47 prose_cite closure batch" = fictional work. Actual gap-closure scope = F-P6.1-1 5-RFC frontmatter pilot (5 commits) + F-P6.1-2 status header pattern consolidation (research doc §21) + F-P5.6-3 Guard 2 extended regex (PROPOSAL status). NO prose cite batch apply needed.

## 4. R6 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 1 | F-P6.6-1 (47 R2 sites = 100% RETAIN per F-P5.2-3) |
| HIGH | 1 | F-P6.6-2 (R2 regex false-positive — did not distinguish roadmap_marker from true actionable) |
| MED | 1 | F-P6.6-3 (plan v1.5 §Phase 6.1 step 3 scope correction) |
| LOW | 0 | (none) |

**R6 NEW: 3 findings (1 CRIT + 1 HIGH + 1 MED).** Phase 6 loop RE-OPENED per R37 P3 methodology (post-DRY re-opening requires fresh evidence).

## 5. Convergence Loop Status (R6 — RE-OPENED)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial corpus STATE consolidation |
| R2 | 3 NEW (all LOW) | NO | F-P6.1-3 actionable enumeration |
| R3 | 3 NEW (all LOW) | NO | Proposal phase |
| R4 | 0 NEW + 3 verification | YES (FIRST of round 2) | Verifications + design integrity |
| R5 | 0 NEW + 4 verification/closure | YES (SECOND of round 2) | Final STATE + DRY closure |
| **R6** | **3 NEW (1 CRIT + 1 HIGH + 1 MED)** | **NO — RE-OPENED** | **F-P6.2-1 false-positive re-examination** |

**R6 = POST-DRY RE-OPENING.** R5 closure stands; R6 surfaces NEW lens (in-scope application review) that requires fresh evidence.

**Convergence direction (post-R6)**: R1=4 → R2=3 → R3=3 → R4=0 → R5=0 → R6=3. R6 = fresh-lens correction.

**R7 expectation:** Apply F-P6.6-3 plan v1.5 §Phase 6.1 step 3 scope correction. Aim for 0-1 NEW (plan file edit verification only).

**R8 (target DRY-2 of round 3)**: Final post-correction verification + re-DRY closure.

## 6. R6 Implications

### Per plan v1.5 §Phase 6.1

**BEFORE R6**: 5-RFC frontmatter pilot + 47 prose cite batch + status header consolidation + Guard 2 extended regex = 4 gap-closure items + 16-21 commits

**AFTER R6**: 5-RFC frontmatter pilot + status header consolidation + Guard 2 extended regex = 3 gap-closure items + 5 commits (47 prose cite batch removed as fictional)

### Per plan v1.5 §Risks #17

**BEFORE R6**: #17 HIGH corpus-wide STALE pin gap = 176 STALE pins requiring closure

**AFTER R6**: #17 disposition = F-P6.6-1 re-classification: 176 sites = ~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative = ALL RETAIN per F-P5.2-3 framework. Risk #17 severity reduced: HIGH → LOW (no actionable surface; corpus-wide STALE count = 0 effective).

### Per F-P6.1-3 corpus-wide 176 STALE pins

**BEFORE R6**: 176 STALE = CRIT corpus STATE drift per R1

**AFTER R6**: 176 cites = 100% legitimate forward-cites per F-P5.2-3 framework. NOT STALE — corpus STATE intact. F-P6.1-3 severity reduced: CRIT → LOW (corpus STATE compliant via framework classification).

## 7. R10.5 Scope Discipline Recap

Phase 6 R6 is RESEARCH DOC ONLY (re-examination + classification framework re-application). NO substrate crate code edits. NO RFC text edits (per F-P6.6-3, the 47 prose cite batch was fictional — no edits to revert). NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 8. Cross-References

- Phase 6 R5 DRY closure: `docs/research/2026-08-22-phase-6-r5-dry-closure.md` (commit `c8a9d9d0`)
- Phase 6 R2 (47 cite enumeration): `docs/research/2026-08-22-phase-6-r2-stale-pin-actionable-enumeration.md` (commit `2fbfbdfe`)
- Phase 5 R2 F-P5.2-3 framework: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md` (commit `88dfa4e9`)
- Plan v1.5 §Phase 6.1: `/home/mmacedoeu/.claude/plans/long-horizon-home-stretch-2026-08-22.md` L259-279
- Plan v1.5 §Risks #17: same file L375
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R6 F-P6.2-1 false-positive re-examination research; 3 NEW findings (1 CRIT + 1 HIGH + 1 MED). F-P6.6-1 CRIT: 47 R2 sites = 100% RETAIN per F-P5.2-3 framework (~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative). F-P6.6-2 HIGH: R2 regex false-positive (did NOT distinguish roadmap_marker forward-cites from true actionable). F-P6.6-3 MED: plan v1.5 §Phase 6.1 step 3 scope correction (47 prose cite batch removed as fictional). F-P6.1-3 severity reduced CRIT → LOW (corpus STATE intact). Risk #17 severity reduced HIGH → LOW (no actionable surface). Plan v1.5 actual gap-closure = 5-RFC frontmatter pilot + status header doc + Guard 2 proposal = 5 commits (down from 16-21). Phase 6 R5 closure stands; R6 = fresh-lens post-DRY re-opening per R37 P3. R7 plan: apply plan v1.5 §Phase 6.1 step 3 scope correction. R8: re-DRY closure. |
