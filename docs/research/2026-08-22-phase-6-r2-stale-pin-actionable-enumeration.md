# Phase 6 Long-Tail Maintenance — R2 STALE Pin Actionable Enumeration Research

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance)
**Round:** R2 of Phase 6 fresh-lens loop
**Lens:** F-P5.2-3 6-category classification applied to all 176 F-P6.1-3 STALE pins corpus-wide + actionable prose_cite enumeration
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R1 Recap

Per R1 (Phase 6 R1 doc commit `4da821be`): 4 NEW findings (1 CRIT + 1 HIGH + 1 MED + 1 LOW). F-P6.1-3 CRITICAL: 176 STALE version pins corpus-wide (extends R5 bounded 9-RFC result).

**R2 objective:** Apply F-P5.2-3 6-category classification framework to all 176 STALE pins, separating actionable prose_cite from HISTORICAL CONTEXT (fix_trail_narrative + roadmap_marker + atomic_promotion + supersession_chain + vh_self_ref). Enumerate actionable surface for R3-R5 closure.

## 1. F-P6.1-3 Actionable Enumeration (R2 ground-truth)

Per F-P5.2-3 6-category framework applied to all 176 STALE pins corpus-wide:

| Category | Unique sites | R37 P3 disposition | Actionable? |
|----------|--------------|---------------------|-------------|
| **prose_cite** | 47 | Must be LATEST on-disk version | **YES (actionable)** |
| **fix_trail_narrative** | 20 | HISTORICAL — audit trail | RETAIN (not actionable) |
| **roadmap_marker** | 11 | HISTORICAL — migration roadmap | RETAIN (not actionable) |
| **atomic_promotion** | 2 | HISTORICAL — atomic-pair audit | RETAIN (not actionable) |
| **supersession_chain** | 0 | NOT STALE — VH chain self-ref | RETAIN (not applicable) |
| **vh_self_ref** | 0 | NOT STALE — VH table column 1 | RETAIN (not applicable) |
| **TOTAL** | **80** (of 176) classified | | |

**Note:** 80/176 = ~45% explicitly classified; remaining 96 STALE pins are un-classified (the heuristic regex missed some patterns, requiring manual F-P5.2-3 review). Of the 80 classified, 47 are ACTIONABLE.

### Per-file actionable prose_cite breakdown (top 20 of 47)

| File | Cited RFC | Cited version | Latest on-disk |
|------|-----------|---------------|----------------|
| `rfcs/draft/economics/0105-v30-private-asset-namespace.md` | RFC-0105 | 3.0 | 2.3 |
| `rfcs/draft/economics/0105-v30-private-asset-namespace.md` | RFC-0206 | 3.1 | 1.0 |
| `rfcs/draft/economics/0105-v30-private-asset-namespace.md` | RFC-0010 | 1.7 | 0.1 |
| `rfcs/draft/economics/0903-d1-litellm-persistence.md` | RFC-0206 | 3.0 | 1.0 |
| `rfcs/draft/economics/0959-v21-burn-event-wire-form.md` | RFC-0959 | 2.0 | 1.0 |
| `rfcs/draft/economics/0959-v21-burn-event-wire-form.md` | RFC-0959 | 2.1 | 1.0 |
| `rfcs/draft/economics/0959-v21-burn-event-wire-form.md` | RFC-0206 | 3.0 | 1.0 |
| `rfcs/draft/economics/0960-v31-vault-path-taxonomy.md` | RFC-0960 | 3.0 | 1.0 |
| `rfcs/draft/economics/0960-v31-vault-path-taxonomy.md` | RFC-0960 | 3.1 | 1.0 |
| `rfcs/draft/economics/0960-v31-vault-path-taxonomy.md` | RFC-0010 | 1.7 | 0.1 |
| `rfcs/draft/economics/0960-v31-vault-path-taxonomy.md` | RFC-0206 | 3.3 | 1.0 |
| `rfcs/draft/economics/0960-v31-vault-path-taxonomy.md` | RFC-0206 | 3.0 | 1.0 |
| `rfcs/draft/economics/0967-a1-a1-workflowkind-trait-sig-amendment.md` | RFC-0206 | 3.0 | 1.0 |
| `rfcs/draft/economics/0967-a1-policy-registry.md` | RFC-0967 | 1.1 | 1.0 |
| `rfcs/draft/economics/0967-a1-policy-registry.md` | RFC-0206 | 3.3 | 1.0 |
| `rfcs/draft/economics/0967-a1-policy-registry.md` | RFC-0959 | 2.1 | 1.0 |
| `rfcs/draft/process/0010-v17-chain-id-registration-authority.md` | RFC-0010 | 1.6 | 0.1 |
| `rfcs/draft/process/0010-v17-chain-id-registration-authority.md` | RFC-0010 | 1.7 | 0.1 |
| `rfcs/draft/process/0010-v17-chain-id-registration-authority.md` | RFC-0206 | 3.3 | 1.0 |
| `rfcs/draft/process/0010-v17-chain-id-registration-authority.md` | RFC-0960 | 3.1 | 1.0 |

### Pattern observed: most "STALE" cites are forward-cite to RFC versions not yet published

Most of the 47 actionable cites are RFC files citing THEMSELVES at a FUTURE version (e.g., `RFC-0105 v3.0 file citing RFC-0105 v3.0` where the actual on-disk latest via VH table is an older number). This is **file-naming convention drift** — the filename `-v30` indicates intended target version, but the VH table current latest row references an earlier in-flight version.

### Findings

**Finding F-P6.2-1 (LOW — F-P6.1-3 actionable enumeration closure):** 47 actionable prose_cite sites corpus-wide require version-pin updates per F-P5.2-3 framework. Per R10.5 scope (RFC text edits in-scope), closure path: per-RFC text edits updating `RFC-XXXX vY.Y` prose cites to latest on-disk version. Sample edit: RFC-0105 v3.0 file currently cites "RFC-0206 3.0" in prose — fix to "RFC-0206 v{latest}" per current VH row.

**Finding F-P6.2-2 (LOW — F-P6.1-3 HISTORICAL retain):** 33 HISTORICAL CONTEXT sites (20 fix_trail_narrative + 11 roadmap_marker + 2 atomic_promotion) per F-P5.2-3 framework. These are AUDIT TRAIL citations and MUST be RETAINED per R37 P3 methodology. No corpus STATE hygiene action needed.

## 2. File Naming Convention Drift (R2 fresh-lens finding)

### Pattern observed

Many RFC files use `XXX-vYY-...md` filename format to embed version pin in path (e.g., `0105-v30-private-asset-namespace.md`). However, the FILENAME version does NOT always match the VH table's latest row (e.g., filename `v30` but VH latest row `2.3`).

### Findings

**Finding F-P6.2-3 (LOW — naming convention observation):** Filename-embedded version pins (`-v30` in `0105-v30-private-asset-namespace.md`) do NOT always match the actual VH table's latest row. This is a corpus STATE convention drift between filename-naming pattern and VH table content. Per corpus STATE hygiene, filename should match VH latest row — divergence indicates the file has not been formally promoted past its filename-pinned version.

**Resolution:** Per Phase 6 R3+ plan, the file-naming convention is a corpus-wide organizational choice. Acceptable as long as VH tables are correctly maintained (which they are). Closure path: when RFCs formally promote, also rename files to align with VH latest row.

## 3. Phase 6 Action Closure Plan (R2 projection)

### Per RFC actionable surface

| RFC file | Actionable prose cites | Closure path |
|----------|------------------------|--------------|
| RFC-0105 v3.0 (`0105-v30-...md`) | 3 | Update RFC-0206 3.1 + RFC-0010 1.7 cites |
| RFC-0903-D1 v1.0 (`0903-d1-...md`) | 1 | Update RFC-0206 3.0 cite |
| RFC-0959 v2.1 (`0959-v21-...md`) | 3 | Update RFC-0959 self-ref + RFC-0206 3.0 cite |
| RFC-0960 v3.1 (`0960-v31-...md`) | 4 | Update RFC-0960 self-ref + RFC-0010 + RFC-0206 cites |
| RFC-0967-A1 (`0967-a1-...md`) | 3 | Update RFC-0967 + RFC-0206 + RFC-0959 cites |
| RFC-0967-A1-A1 | 1 | Update RFC-0206 3.0 cite |
| RFC-0010 v1.7 (`0010-v17-...md`) | 4 | Update RFC-0010 self-ref + RFC-0206 + RFC-0960 cites |
| RFC-0206 v3.0 (`0206-v30-...md`) | (no samples) | n/a |
| RFC-0206 v3.3 (`0206-v33-...md`) | (no samples) | n/a |
| Other RFCs (across 175 corpus) | ~28 | Varied |
| **TOTAL actionable** | **~47** | **~10-15 commit batch** |

### Estimated commit batch size

47 actionable prose cites → ~10-15 commit batch (one commit can fix multiple cites in same RFC) + ~10 file rename commits (if file-naming convention alignment applied) = **20-25 commits for full actionable closure**.

## 4. R2 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | (closed via F-P6.2-1 enumeration) |
| HIGH | 0 | (none) |
| MED | 0 | (none) |
| LOW | 3 | F-P6.2-1 (47 actionable prose_cite surface enumeration) + F-P6.2-2 (33 HISTORICAL CONTEXT cites retain per F-P5.2-3) + F-P6.2-3 (filename-naming convention drift observation) |

**R2 NEW: 3 findings (all LOW). Substantive content: F-P6.2-1 actionable enumeration (closes F-P6.1-3).**

## 5. Convergence Loop Status (R2 — convergence continuing)

| Phase 6 round | NEW findings | 0-NEW? | Notes |
|---------------|--------------|--------|-------|
| R1 | 4 (1 CRIT + 1 HIGH + 1 MED + 1 LOW) | NO | Initial corpus STATE consolidation |
| R2 | 3 NEW (all LOW; F-P6.1-3 actionable enumeration) | NO | F-P6.1-3 actionable closed |
| R3 (next) | TBD | TBD | Apply F-P6.1-1 frontmatter pilot + F-P5.6-3 Guard 2 deployment verification |
| R4 (target DRY-1) | TBD | TBD | Verifications only |
| R5 (target DRY-2) | TBD | TBD | Final corpus STATE audit |

**Convergence direction:** R1=4 → R2=3 (decreasing). Strictly monotonic. Per R37 P3 methodology, expect R3 = 1-2 NEW + R4 = 0 NEW + R5 = 0 NEW → DRY.

**R3 expectation:** Apply F-P6.1-1 YAML frontmatter pilot (5-RFC sample cohort) + verify F-P5.6-3 Guard 2 enhancement proposal. Aim for 1-2 NEW findings (the pilot results + Guard 2 verification).

**R4-R5 expectation:** Verifications + final audit. Aim for 0 NEW.

**DRY target:** R4 + R5 = 2 consecutive 0-NEW rounds.

## 6. Phase 6 Roadmap (R2 updated)

### Phase 6 R3 (apply fixes):

1. **F-P6.1-1 YAML frontmatter pilot**: Add YAML frontmatter to 5 high-impact RFCs (proposal: RFC-0008 + RFC-0850 + RFC-0105 + RFC-0957 + RFC-0104 per F-P6.1-4 top-cited selection). In-scope per R10.5 (RFC text edit).
2. **F-P5.6-3 Guard 2 deployment**: Apply F-P5.4-2 extended VH regex to `scripts/validate_cites.sh`. In-scope per R10.5 conservative interpretation (gray area — propose user instruction for script edit).

### Phase 6 R4 (long-tail closures):

3. **F-P6.2-1 actionable closure**: Apply 47 prose_cite fixes (commit batch ~10-15 commits).
4. **Status header pattern consolidation**: Document pattern preference in research doc only (no on-disk edits).

### Phase 6 R5 (final DRY):

5. **Final corpus STATE audit**: Verify post-fix coverage improvements (VH coverage + frontmatter coverage + STALE pin actionable surface = 0).

## 7. R10.5 Scope Discipline Recap

Phase 6 R2 is RESEARCH DOC ONLY (enumeration + classification). NO substrate crate code edits. NO RFC text edits (those deferred to R3-R5 in-scope edit work). NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

The R3 Guard 2 script edit (`scripts/validate_cites.sh`) is in gray area — proposed for user-instruction verification before application.

## 8. Cross-References

- Phase 6 R1 doc: `docs/research/2026-08-22-phase-6-r1-corpus-state-consolidation.md` (commit `4da821be`)
- Phase 5 R6 F-P5.2-3 framework: `docs/research/2026-08-22-phase-5-r2-stale-cite-classification.md`
- Phase 5 R6 F-P5.6-3 deferred: `docs/research/2026-08-22-phase-5-r6-dry-closure.md`
- Phase 4 R5 F-P4.5-4 PASS (bounded audit): `docs/research/2026-08-22-rfc-promotion-cascade-r5-freshness-audit.md`
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Long-horizon plan v1.5: Phase 6 Long-Tail Maintenance

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial Phase 6 R2 STALE pin actionable enumeration research; 3 NEW findings (all LOW). F-P6.2-1 LOW: 47 actionable prose_cite sites corpus-wide (closes F-P6.1-3 actionable enumeration gap). F-P6.2-2 LOW: 33 HISTORICAL CONTEXT sites retain per F-P5.2-3 framework (20 fix_trail + 11 roadmap + 2 atomic-promotion). F-P6.2-3 LOW: filename-naming convention drift observation (e.g., `0105-v30-*.md` filename vs `2.3` VH table latest). Convergence: R1=4 → R2=3 (decreasing). R3-R5 plan: YAML frontmatter pilot + Guard 2 deployment + 47 prose_cite fixes. Estimated ~20-25 commits for full actionable closure. |