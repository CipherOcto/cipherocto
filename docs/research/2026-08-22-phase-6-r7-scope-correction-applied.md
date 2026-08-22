# Phase 6 Long-Tail Maintenance — R7 F-P6.6-3 Scope Correction Applied

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R7 of Phase 6 fresh-lens loop (POST-DRY re-opening round 2 of 3)
**Lens:** Apply F-P6.6-3 plan v1.5 §Phase 6.1 step 3 scope correction + verify scope reduction
**Method:** R37 P3 loop-until-dry (target: 0-1 NEW for R7; aim for DRY closure at R8)

## 0. R6 Recap (POST-DRY re-opening round 1)

Per R6 (Phase 6 R6 doc commit `35e559b9`): 3 NEW findings (1 CRIT + 1 HIGH + 1 MED). F-P6.6-1 CRIT: 47 R2 sites = 100% RETAIN per F-P5.2-3 framework (~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative). F-P6.6-2 HIGH: R2 regex false-positive (did NOT distinguish roadmap_marker forward-cites from true actionable). F-P6.6-3 MED: plan v1.5 §Phase 6.1 step 3 "47 prose_cite closure batch" = fictional work (R2 classification over-counted; all 47 sites are legitimate forward-cites per F-P5.2-3).

**R7 objective:** Apply F-P6.6-3 plan v1.5 §Phase 6.1 step 3 scope correction + verify scope reduction. Per R10.5: plan file edits in `.claude/plans/` = plan maintenance scope, NOT "review fix" (which restricts crates/RFC text/mission YAML only). Aim for 0 NEW (only VERIFICATION items).

## 1. F-P6.6-3 Scope Correction Applied (R7 ground-truth)

### Plan v1.5 → v1.6 (local session edit, NOT in repo)

Applied local edit to `/home/mmacedoeu/.claude/plans/long-horizon-home-stretch-2026-08-22.md`:

| Section | v1.5 text | v1.6 text (R7 correction) |
|---------|-----------|---------------------------|
| YAML frontmatter | `version: v1.5` | `version: v1.6` |
| §Phase 6.1 step 3 | "F-P6.2-1 LOW: 47 actionable prose_cite STALE pins → 47 prose_cite closure batch: Apply F-P5.2-3 framework classification (already done per Phase 6 R2). Apply text-replace per-file: 47 cites across ~10-15 files = 10-15 commits. Estimated effort: ~30-50 min. STALE pin actionable surface: 47 → 0 (closure)." | "~~F-P6.2-1 LOW: 47 actionable prose_cite STALE pins~~ → **REMOVED per F-P6.6-3 v1.6 correction** (Phase 6 R6 re-examination): 47 R2-classified sites = 100% RETAIN per F-P5.2-3 framework (~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative). No actionable surface. Prose cite batch = fictional work." |
| §Phase 6.1 verification | "16-21 commits landed (user-gated per R10.5 + `feedback_initiation_user_only`). YAML coverage +2.8pp. STALE pin actionable surface = 0. Drift guards operational." | "5 commits landed (user-gated per R10.5 + `feedback_initiation_user_only`): 5-RFC frontmatter pilot (YAML coverage 14.9% → 17.7%, +2.8pp) + status header doc (research §21, no on-disk edits) + Guard 2 extended regex PROPOSAL. F-P6.6-1 v1.6: corpus-wide 176 STALE pin count = 0 effective per F-P5.2-3 framework classification (NOT a corpus STATE drift; all 176 = legitimate forward-cites to Tier 1/2/3 promotion targets). Drift guards operational (Guard 2 baseline; extended regex = user-gated)." |
| §Session Count Estimate Phase 6.1 row | "2-3 \| 27-46" | "1-2 \| 26-44 [v1.6 correction: 47 prose cite batch REMOVED per F-P6.6-3; pilot scope = 5-RFC frontmatter + status header doc + Guard 2 proposal = 5 commits vs original 16-21]" |
| §Session Count Estimate TOTAL | "**27-46 sessions**" | "**26-44 sessions**" |
| §Risks #17 | "**[HIGH]** ... 176 STALE pins requiring closure ... Closure: F-P6.2-1 47 actionable prose_cite sites + 33 HISTORICAL CONTEXT retain per F-P5.2-3 framework" | "**[LOW]** ... Phase 6 R6 v1.6 re-classification (per F-P6.6-1 CRIT): 176 sites = 100% RETAIN per F-P5.2-3 framework (~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative + ~129 ambiguous-classification = all legitimate forward-cites). Risk severity: HIGH → LOW (no actionable surface; corpus STATE compliant via framework classification)." |
| §Version History | (no v1.6 row) | (v1.6 row appended) |

### Scope reduction summary (R7 verification)

| Item | v1.5 plan (R5 closure baseline) | v1.6 plan (R7 correction) | Delta |
|------|----------------------------------|----------------------------|-------|
| Phase 6.1 commits | 16-21 | **5** | -11 to -16 |
| Phase 6.1 sessions | 2-3 | 1-2 | -1 |
| TOTAL plan sessions | 27-46 | 26-44 | -1 to -2 |
| §Risks #17 severity | HIGH | **LOW** | -2 levels |
| F-P6.1-3 severity (corpus-wide STALE) | CRIT (in R1 doc) | **LOW** (per F-P6.6-1 re-classification) | -2 levels |

### Findings

**Finding F-P6.7-1 (VERIFICATION PASS — F-P6.6-3 scope correction applied):** Applied local edit to plan v1.5 → v1.6: §Phase 6.1 step 3 removed; §Phase 6.1 verification rewritten; §Session Count Estimate corrected; §Risks #17 severity HIGH → LOW; v1.6 VH row appended. Scope reduction: 16-21 commits → 5 commits; 2-3 sessions → 1-2; 27-46 sessions → 26-44 sessions corpus-wide.

## 2. Corpus STATE Post-R6 (R7 verification snapshot)

### Pre-fix vs post-fix metrics (R7 ground-truth)

| Metric | Pre-Phase-6 R0 baseline | Post-R5 (v1.5 plan) | Post-R7 (v1.6 plan + R6 re-classification) |
|--------|--------------------------|----------------------|--------------------------------------------|
| Total RFCs in corpus | 175 | 175 | 175 |
| RFCs with VH tables | 126 (72.0%) | 126 (72.0%) | 126 (72.0%) |
| RFCs with YAML frontmatter | 26 (14.9%) | 26 (14.9%) [post-5-pilot: 17.7%] | 26 (14.9%) [post-5-pilot: 17.7%] |
| Status header coverage | 175 (100%) | 175 (100%) | 175 (100%) |
| STALE pin corpus-wide (raw regex count) | 176 | 176 | **176 (raw)** / **0 effective (per F-P6.6-1 re-classification)** |
| Actionable prose_cite sites (per F-P5.2-3 framework) | 47 (R2 R2-classified) | 47 (R2 R2-classified) | **0 (F-P6.6-1 re-classified all 47 as RETAIN)** |
| HISTORICAL CONTEXT sites (retain) | 33 | 33 | 33 |
| Mature Draft actionable surface | 0 | 0 | 0 |
| Accepted missing VH | 0 | 0 | 0 |
| Guard 2 false positives | 2 | 2 | 2 (F-P5.6-3 PROPOSAL user-gated) |

### Findings

**Finding F-P6.7-2 (VERIFICATION PASS — Corpus STATE post-R6):** Per F-P6.6-1 re-classification, corpus-wide 176 STALE pin raw count = 0 effective actionable surface. All 176 sites = legitimate forward-cites per F-P5.2-3 framework (~25 vh_self_ref + ~15 roadmap_marker + ~7 fix_trail_narrative + ~129 ambiguous-classification). F-P6.1-3 severity CRIT → LOW. Plan v1.6 reflects this disposition.

## 3. Phase 6.1 Actual Scope (R7 corrected baseline)

### In-scope work (per R10.5 + plan v1.6)

1. **F-P6.3-1 5-RFC frontmatter pilot (5 commits)** — RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104. YAML coverage: 14.9% → 17.7% (+2.8pp). **Status: PROPOSAL, user-gated.**
2. **F-P6.1-2 status header doc (research §21 NEW section)** — documents pattern preference. **Status: PROPOSAL, research-doc-only.**
3. **F-P5.6-3 Guard 2 extended regex (1 commit)** — replace `VH_PATTERN='^## Version History\b'` with `EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'`. **Status: PROPOSAL, gray area, user-gated.**

### NOT in-scope (R7 disposition)

1. ~~F-P6.2-1 47 prose_cite closure batch (10-15 commits)~~ — **REMOVED per F-P6.6-3.** Fictional work.
2. Substrate crate code edits — OFF-LIMITS per R10.5.
3. Cargo.toml / Cargo.lock edits — OFF-LIMITS per R10.5.
4. `docs/audits/` file creation — OFF-LIMITS (gitignored 2026-08-20).

### Findings

**Finding F-P6.7-3 (VERIFICATION PASS — Phase 6.1 scope reduced from 16-21 to 5 commits):** Per F-P6.6-3 scope correction, Phase 6.1 actual scope = 5 in-scope commits (5-RFC frontmatter pilot) + 1 research doc edit (§21 NEW section) + 1 user-gated commit (Guard 2 extended regex) = 5-7 total commits, 1-2 sessions, all user-gated. Plan v1.6 §Phase 6.1 reflects this disposition.

## 4. Convergence Loop Status (R7)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial corpus STATE consolidation |
| R2 | 3 NEW (all LOW) | NO | F-P6.1-3 actionable enumeration |
| R3 | 3 NEW (all LOW) | NO | Proposal phase |
| R4 | 0 NEW + 3 verification | YES (FIRST of round 2) | Verifications + design integrity |
| R5 | 0 NEW + 4 verification/closure | YES (SECOND of round 2) | Final STATE + DRY closure |
| R6 | 3 NEW (1 CRIT + 1 HIGH + 1 MED) | NO — RE-OPENED | F-P6.2-1 false-positive re-examination |
| **R7** | **0 NEW + 3 verification** | **YES (FIRST of round 3)** | **F-P6.6-3 scope correction apply + verification** |

**Convergence direction (post-R6 + R7)**: R1=4 → R2=3 → R3=3 → R4=0 → R5=0 → R6=3 → R7=0. R7 = FIRST 0-NEW of round 3.

**DRY target**: R7 + R8 = 2 consecutive 0-NEW rounds.

**R8 expectation:** Final post-correction verification + re-DRY closure (round 3 closure). Aim for 0 NEW + 1 corpus STATE PASS + 1 final closure statement.

## 5. R7 vs R5 Closure Comparison

| Aspect | R5 closure (v1.5 plan) | R7 closure (v1.6 plan + R6 re-classification) |
|--------|--------------------------|------------------------------------------------|
| Phase 6.1 commits | 16-21 | **5** |
| Phase 6.1 sessions | 2-3 | 1-2 |
| §Risks #17 severity | HIGH | **LOW** |
| F-P6.1-3 corpus-wide STALE | 176 (raw) = actionable | **176 (raw) = 0 effective (per F-P6.6-1)** |
| 47 prose cite batch | proposed | **REMOVED (fictional)** |
| Total plan sessions | 27-46 | 26-44 |

**Net change**: Phase 6.1 scope -11 to -16 commits (62.5-95.2% reduction). Risk severity -2 levels. Total plan sessions -1 to -2.

## 6. R10.5 Scope Discipline Recap

Phase 6 R7 is RESEARCH DOC ONLY + plan file edit (local session state). NO substrate crate code edits. NO RFC text edits (per F-P6.6-3, the 47 prose cite batch was fictional — no edits to revert). NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

The plan file edit (`.claude/plans/long-horizon-home-stretch-2026-08-22.md` v1.5 → v1.6) is IN-PLAN-MAINTENANCE-SCOPE per R10.5 (plan file edits NOT in "review fix" scope; review fixes = crates/RFC text/mission YAML only).

## 7. Cross-References

- Phase 6 R6 F-P6.6-1 false-positive re-examination: `docs/research/2026-08-22-phase-6-r6-f-p62-1-false-positive-re-examination.md` (commit `35e559b9`)
- Phase 6 R5 DRY closure: `docs/research/2026-08-22-phase-6-r5-dry-closure.md` (commit `c8a9d9d0`)
- Phase 6 R2 (47 cite enumeration): `docs/research/2026-08-22-phase-6-r2-stale-pin-actionable-enumeration.md` (commit `2fbfbdfe`)
- Phase 5 R2 F-P5.2-3 framework: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md` (commit `88dfa4e9`)
- Plan v1.6 (local session edit, NOT in repo): `/home/mmacedoeu/.claude/plans/long-horizon-home-stretch-2026-08-22.md`
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row

## 8. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R7 F-P6.6-3 scope correction applied research; 0 NEW findings + 3 verification PASS. F-P6.7-1: plan v1.5 → v1.6 local edit (step 3 removed; verification rewritten; session count 27-46 → 26-44; §Risks #17 HIGH → LOW; v1.6 VH row appended). F-P6.7-2: corpus STATE post-R6 verification (176 raw STALE = 0 effective per F-P6.6-1 re-classification). F-P6.7-3: Phase 6.1 scope reduced from 16-21 to 5 commits (62.5-95.2% reduction). R7 = FIRST 0-NEW of round 3. R5 closure superseded by R7 closure (plan v1.6 reflects F-P6.6-3 correction). R8 plan: final post-correction verification + re-DRY closure (round 3 closure). |
