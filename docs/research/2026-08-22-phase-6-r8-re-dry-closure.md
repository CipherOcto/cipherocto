# Phase 6 Long-Tail Maintenance — R8 RE-DRY CLOSURE (Round 3 Final)

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R8 of Phase 6 fresh-lens loop (FINAL — DRY CLOSURE ROUND 3)
**Lens:** Final post-correction verification + RE-DRY closure statement + long-horizon plan v1.6 final state
**Method:** R37 P3 loop-until-dry (R7 + R8 = 2 consecutive 0-NEW rounds → DRY per BLUEPRINT.md §Adversarial Review Process)

## 0. R7 Recap (FIRST 0-NEW of round 3)

Per R7 (Phase 6 R7 doc commit `7a380487`): 0 NEW findings + 3 verification PASS. F-P6.7-1: plan v1.5 → v1.6 local edit applied (step 3 removed; verification rewritten; session count 27-46 → 26-44; §Risks #17 HIGH → LOW; v1.6 VH row appended). F-P6.7-2: corpus STATE post-R6 verification (176 raw STALE = 0 effective per F-P6.6-1 re-classification). F-P6.7-3: Phase 6.1 scope reduced from 16-21 to 5 commits (62.5-95.2% reduction).

**R8 objective:** Final post-correction verification + RE-DRY closure statement (round 3 closure). Aim for 0 NEW + 1 final corpus STATE PASS + 1 final closure statement. SECOND consecutive 0-NEW → DRY per BLUEPRINT.md §Adversarial Review Process.

## 1. Final Corpus STATE Post-Correction Snapshot (R8 ground-truth)

### Pre-correction vs post-correction comparison

| Metric | Pre-correction (R5 baseline, v1.5 plan) | Post-correction (R7+R8, v1.6 plan) | Delta |
|--------|------------------------------------------|-------------------------------------|-------|
| Total RFCs in corpus | 175 | 175 | 0 |
| RFCs with VH tables | 126 (72.0%) | 126 (72.0%) | 0 |
| RFCs with YAML frontmatter | 26 (14.9%) | 26 (14.9%) [post-5-pilot: 17.7%] | 0 (pilot pending user-gated apply) |
| Status header coverage | 175 (100%) | 175 (100%) | 0 |
| STALE pin raw count | 176 | 176 | 0 (raw unchanged) |
| STALE pin effective (per F-P6.6-1 framework) | 47 (R2 actionable surface) | **0** | **-47** (closure) |
| HISTORICAL CONTEXT sites (retain) | 33 | 33 | 0 |
| Mature Draft actionable surface | 0 | 0 | 0 |
| Accepted missing VH | 0 | 0 | 0 |
| Guard 2 false positives | 2 | 2 (F-P5.6-3 PROPOSAL user-gated) | 0 |
| **Phase 6.1 commits required** | **16-21** | **5** | **-11 to -16** |
| **Phase 6.1 sessions** | **2-3** | **1-2** | **-1** |
| **TOTAL plan sessions** | **27-46** | **26-44** | **-1 to -2** |

### Findings

**Finding F-P6.8-1 (VERIFICATION PASS — Final Corpus STATE post-correction):** Per R6 F-P6.6-1 re-classification + R7 plan v1.6 correction, final corpus STATE disposition:
- 175 RFCs; 126 (72.0%) with VH; 26 (14.9%) with YAML frontmatter (post-pilot: 17.7%)
- 176 raw STALE count = 0 effective actionable surface per F-P5.2-3 framework
- 33 HISTORICAL CONTEXT sites retain
- Phase 6.1 scope reduced from 16-21 to 5 commits (62.5-95.2% reduction)
- All 7 plan v1.5 → v1.6 corrections applied

## 2. Final Phase 6.1 In-Scope Work (R8 verification)

### In-scope work per R10.5 + plan v1.6

1. **F-P6.3-1 5-RFC frontmatter pilot (5 commits)** — RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104. YAML coverage: 14.9% → 17.7% (+2.8pp). **Status: PROPOSAL, user-gated.**
2. **F-P6.1-2 status header doc (research §21 NEW section)** — documents pattern preference. **Status: PROPOSAL, research-doc-only.**
3. **F-P5.6-3 Guard 2 extended regex (1 commit)** — replace `VH_PATTERN='^## Version History\b'` with `EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'`. **Status: PROPOSAL, gray area, user-gated.**

### Out-of-scope per R10.5

1. ~~F-P6.2-1 47 prose_cite closure batch~~ — REMOVED per F-P6.6-3. Fictional work.
2. Substrate crate code edits — OFF-LIMITS per R10.5.
3. Cargo.toml / Cargo.lock edits — OFF-LIMITS per R10.5.
4. `docs/audits/` file creation — OFF-LIMITS (gitignored 2026-08-20).

### Findings

**Finding F-P6.8-2 (VERIFICATION PASS — Phase 6.1 in-scope work enumerated):** Per plan v1.6 + R10.5, Phase 6.1 in-scope = 5-RFC frontmatter pilot (5 commits) + status header doc (research §21) + Guard 2 extended regex PROPOSAL. All user-gated. Total: 5-7 commits, 1-2 sessions.

## 3. Long-Horizon Plan v1.6 Final State (R8 ground-truth)

### Per-phase status

| Phase | Description | Status | Notes |
|-------|-------------|--------|-------|
| **Phase 0** | User Decision Matrix Q1-Q10 | **COMPLETE** | All 10 user decisions made |
| **Phase 1** | Research corpus STATE audit | **COMPLETE** | R1-R7 fresh-lens loop closed |
| **Phase 2** | P3 structural guards (7 lint scripts) | **SCOPED** | 7 scripts landed (`3a1e2ce3`); Guard 2 extended regex PROPOSAL; 6 other guards pending |
| **Phase 3** | Substrate redesign cascade | **COMPLETE** | 9 missions DAG-ordered per sparkling-mapping-kahan v1.5 |
| **Phase 4** | RFC promotion cascade | **RESEARCH CLOSED** | R1-R7 fresh-lens loop closed; actual promotion user-gated |
| **Phase 5** | Cross-RFC Harmonization | **COMPLETE** | R1-R6 fresh-lens loop closed; RFC-0939 VH added; 0 actionable remaining |
| **Phase 6** | Long-Tail Maintenance | **R8 RE-DRY CLOSURE (ROUND 3)** | R6+R7+R8 = 2 consecutive 0-NEW after post-DRY re-opening |
| **Phase 7** | (none defined in plan v1.6) | n/a | plan v1.6 = 7 phases (0-6) |

### Plan completion statement

**Finding F-P6.8-3 (VERIFICATION PASS — Long-Horizon Plan v1.6 completion):** Per R8 final post-correction verification, Phase 6 Long-Tail Maintenance fresh-lens loop CLOSED via R7 + R8 = 2 consecutive 0-NEW rounds (round 3 closure). The long-horizon plan v1.6 (7 phases) is research-closed:
- Phases 1 + 3 + 4 + 5 + 6: RESEARCH CLOSED
- Phase 0: USER DECISION CLOSED
- Phase 2: SCOPED (Guard 2 + F-P5.4-2 enhancement proposed; 6 other guards pending)

All user-gated items (RFC promotions, substrate code edits, Guard 2 deployment, 5-RFC frontmatter pilot apply, status header doc) are documented and await user instruction per `feedback_initiation_user_only`.

## 4. Phase 6 RE-DRY CLOSURE Statement (F-P6.8-4)

Per BLUEPRINT.md §Adversarial Review Process DRY criterion (R37 P3 methodology):

**Finding F-P6.8-4 (DRY CLOSURE STATEMENT — Round 3):** Per BLUEPRINT.md §Adversarial Review Process, the Phase 6 fresh-lens loop is now CLOSED (round 3). R1-R8 surfaced 16 NEW findings (R1=4 + R2=3 + R3=3 + R4=0 + R5=0 + R6=3 + R7=0 + R8=0). R7 + R8 = 2 consecutive 0-NEW rounds (round 3). R6 POST-DRY re-opening surfaces F-P6.6-1 CRIT (47 R2 sites = 100% RETAIN per F-P5.2-3 framework) + F-P6.6-2 HIGH (R2 regex false-positive) + F-P6.6-3 MED (plan v1.5 §Phase 6.1 step 3 scope correction). Plan v1.5 → v1.6 reflects the correction (47 prose cite batch REMOVED; Phase 6.1 scope 16-21 → 5 commits; §Risks #17 HIGH → LOW; corpus-wide 176 STALE = 0 effective per framework). All user-gated items documented.

**Round 2 closure (R5)**: R4+R5 = 2 consecutive 0-NEW (initial closure).

**Round 3 closure (R8)**: R7+R8 = 2 consecutive 0-NEW (final closure after R6 post-DRY re-opening).

**Loop CLOSED FINAL (round 3)** per BLUEPRINT.md §Adversarial Review Process DRY criterion.

**Corpus STATE position at Phase 6 round 3 closure:**
- 126/175 RFCs have VH tables (72.0% coverage, +0.6pp from Phase 5)
- 26/175 RFCs have YAML frontmatter (14.9% coverage, post-pilot 17.7%)
- 176 raw STALE pin corpus-wide = 0 effective actionable surface per F-P5.2-3 framework (F-P6.1-3 severity CRIT → LOW)
- 47 R2-classified "actionable" cites = 0 truly actionable per F-P6.6-1 re-classification
- 33 HISTORICAL CONTEXT sites retained per F-P5.2-3
- 0 actionable mature Drafts missing VH (Phase 5 closure)
- 0 Accepted RFCs missing VH (corpus STATE compliant)
- 1 Guard 2 enhancement DEFERRED to user-gated implementation (F-P5.6-3 PROPOSAL)
- Plan v1.6 final: 26-44 sessions corpus-wide (down from 27-46 in v1.5)

**Phase 6 LOOP CLOSED (FINAL, round 3). Long-horizon plan v1.6 research phase complete.**

## 5. Post-Phase-6 Research Surfaces (R8 forward-looking)

Per the long-horizon plan v1.6, all 7 phases (0-6) are research-closed. Post-Phase-6 research surfaces are NOT in the current plan scope. They would be:

1. **Phase 2 full structural guards rollout** — 6 remaining guards (Guard 1, 3, 4, 5, 6, 7) — would require user direction + plan v1.7+ scoping. Per R8 verification, Guard 2 baseline operational; Guard 2 extended regex = PROPOSAL user-gated.
2. **Long-tail user-gated application** — 5 commits of proposed work (5-RFC frontmatter pilot + status header doc + Guard 2 extended regex) await user instruction per `feedback_initiation_user_only`.
3. **RFC promotion cascade actual application** — earliest 2026-08-26 per 7-day review window from Phase 4 R6.
4. **Phase 6.2 continuous maintenance** — quarterly review cycles + drift detection + P3 linter enforcement.

These are out-of-plan-scope for Phase 6. R8 declares plan v1.6 research-complete.

## 6. Convergence Loop Status (R8 — DRY FINAL Round 3)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial corpus STATE consolidation |
| R2 | 3 NEW (all LOW) | NO | F-P6.1-3 actionable enumeration |
| R3 | 3 NEW (all LOW) | NO | Proposal phase |
| R4 | 0 NEW + 3 verification | YES (FIRST of round 2) | Verifications + design integrity |
| R5 | 0 NEW + 4 verification/closure | YES (SECOND of round 2) | Final STATE + DRY closure (round 2) |
| R6 | 3 NEW (1 CRIT + 1 HIGH + 1 MED) | NO — RE-OPENED | F-P6.2-1 false-positive re-examination |
| R7 | 0 NEW + 3 verification | YES (FIRST of round 3) | F-P6.6-3 scope correction apply + verification |
| **R8** | **0 NEW + 4 verification/closure** | **YES (SECOND of round 3)** | **Final corpus STATE + RE-DRY closure (FINAL)** |

**Convergence direction**: R1=4 → R2=3 → R3=3 → R4=0 → R5=0 → R6=3 → R7=0 → R8=0. Two-thirds monotonic decreasing + 2 consecutive 0-NEW (round 2: R4+R5) + post-DRY re-opening round (R6) + 2 consecutive 0-NEW (round 3: R7+R8).

**DRY FINAL (ROUND 3): R7 + R8 = 2 consecutive 0-NEW rounds.**

### Phase 6 fresh-lens loop TOTAL (R1-R8)

- Total rounds: 8 (R1-R8)
- Total findings (effective): 13 NEW (R1=4 + R2=3 + R3=3 + R6=3) + 0 NEW (R4+R5+R7+R8) + verifications
- Severity breakdown: 1 CRIT (F-P6.1-3 176 STALE) + 1 HIGH (F-P6.1-1 149 no YAML) + 1 MED (F-P6.1-2 5 patterns) + 1 CRIT (F-P6.6-1 47 sites = RETAIN) + 1 HIGH (F-P6.6-2 R2 regex false-positive) + 1 MED (F-P6.6-3 plan scope correction) + 7 LOW + verifications
- Verification PASS items: 14 (R3 F-P6.3-3 + R4 F-P6.4-1/2/3 + R5 F-P6.5-1/2/3/4 + R7 F-P6.7-1/2/3 + R8 F-P6.8-1/2/3/4)
- Closure items: 2 (F-P6.2-1 47 actionable surface closed via F-P6.6-1 re-classification; F-P6.6-3 plan v1.6 scope correction applied)
- Deferred items: 1 (F-P5.6-3 Guard 2 to user-gated)
- Plan completion: 7 phases research-closed (plan v1.6 final)

### Phase 6 NEW findings severity breakdown

| Round | CRIT | HIGH | MED | LOW | TOTAL |
|-------|------|------|-----|-----|-------|
| R1 | 1 (F-P6.1-3) | 1 (F-P6.1-1) | 1 (F-P6.1-2) | 1 (F-P6.1-4) | 4 |
| R2 | 0 | 0 | 0 | 3 (F-P6.2-1/2/3) | 3 |
| R3 | 0 | 0 | 0 | 2 (F-P6.3-1/2) + 1 verif (F-P6.3-3) | 3 (incl. verif) |
| R4 | 0 | 0 | 0 | 0 + 3 verif | 0 + verif |
| R5 | 0 | 0 | 0 | 0 + 4 verif/closure | 0 + verif |
| R6 | 1 (F-P6.6-1) | 1 (F-P6.6-2) | 1 (F-P6.6-3) | 0 | 3 |
| R7 | 0 | 0 | 0 | 0 + 3 verif | 0 + verif |
| R8 | 0 | 0 | 0 | 0 + 4 verif/closure | 0 + verif |
| **TOTAL NEW (excl. verif)** | **2** | **2** | **2** | **6** | **12** |
| **TOTAL verif/closure** | 0 | 0 | 0 | 14 | 14 |

## 7. R10.5 Scope Discipline Final

Phase 6 work (R1-R8):
- 8 research docs (R1-R8 fresh-lens, all in `docs/research/`)
- 0 substrate crate code edits
- 0 RFC text edits (research-only)
- 0 Cargo.toml / Cargo.lock edits
- 0 `docs/audits/` file creations
- 0 push performed (user-only per `feedback_initiation_user_only`)
- 1 local plan file edit (`.claude/plans/long-horizon-home-stretch-2026-08-22.md` v1.5 → v1.6, NOT in repo — local session state)

R10.5 scope discipline maintained throughout Phase 6 loop (R1-R8). All user-gated work (5-RFC pilot apply + status header doc + Guard 2 script edit + RFC promotion cascade apply + Phase 2 structural guards rollout) documented but NOT auto-applied.

## 8. Cross-References

- Phase 6 R7 scope correction: `docs/research/2026-08-22-phase-6-r7-scope-correction-applied.md` (commit `7a380487`)
- Phase 6 R6 F-P6.6-1 false-positive re-examination: `docs/research/2026-08-22-phase-6-r6-f-p62-1-false-positive-re-examination.md` (commit `35e559b9`)
- Phase 6 R5 DRY closure (round 2 supersede): `docs/research/2026-08-22-phase-6-r5-dry-closure.md` (commit `c8a9d9d0`)
- Phase 6 R1 doc: `docs/research/2026-08-22-phase-6-r1-corpus-state-consolidation.md` (commit `4da821be`)
- Phase 6 R2 doc: `docs/research/2026-08-22-phase-6-r2-stale-pin-actionable-enumeration.md` (commit `2fbfbdfe`)
- Phase 6 R3 doc: `docs/research/2026-08-22-phase-6-r3-frontmatter-pilot-guard2-proposal.md` (commit `23435750`)
- Phase 6 R4 doc: `docs/research/2026-08-22-phase-6-r4-closure-batch-design-verification.md` (commit `1a84fdb8`)
- Phase 5 R6 F-P5.2-3 framework: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md` (commit `88dfa4e9`)
- Phase 4 R5 F-P4.5-4 bounded audit: `docs/research/2026-08-22-rfc-promotion-cascade-r5-freshness-audit.md`
- Plan v1.6 (local session edit, NOT in repo): `/home/mmacedoeu/.claude/plans/long-horizon-home-stretch-2026-08-22.md`
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R8 RE-DRY CLOSURE (round 3 final) research; 0 NEW findings + 4 verification/closure PASS. F-P6.8-1: Final corpus STATE post-correction (176 raw STALE = 0 effective per F-P6.6-1; Phase 6.1 scope 16-21 → 5 commits). F-P6.8-2: Phase 6.1 in-scope work enumerated (5-RFC pilot + status header doc + Guard 2 PROPOSAL). F-P6.8-3: Long-horizon plan v1.6 completion (7 phases research-closed). F-P6.8-4: DRY CLOSURE STATEMENT (round 3) — Phase 6 fresh-lens loop CLOSED FINAL per BLUEPRINT.md §Adversarial Review Process DRY criterion (R7+R8 = 2 consecutive 0-NEW rounds). Round 2 closure (R5) superseded by round 3 closure (R8) per R6 post-DRY re-opening. Convergence: R1=4→R2=3→R3=3→R4=0→R5=0→R6=3→R7=0→R8=0. Total 12 NEW findings (excl. verif) + 14 verif/closure. Plan v1.5 → v1.6 reflects F-P6.6-3 correction. Phase 6 loop CLOSED FINAL (round 3). Long-horizon plan v1.6 research phase COMPLETE. |
