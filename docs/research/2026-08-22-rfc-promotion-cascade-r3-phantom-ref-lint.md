# RFC Promotion Cascade Readiness — Phase 4.3 R3 Fresh-Lens Research

**Date:** 2026-08-22
**Round:** R3 of Phase 4 fresh-lens loop
**Lens:** phantom substrate file ref corpus audit (per F-P4.2-16 CRITICAL) + L[N] line ref compliance + version pin distribution
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R2 Recap

Per R2 (Phase 4.2 doc commit `a419190c`): 17 effective findings (2 CRIT + 2 HIGH + 5 MED + 4 LOW + 4 R1 corrections). R2 surfaced F-P4.2-16 CRITICAL (phantom substrate file ref corpus audit BLOCKED on per-RFC grep) + F-P4.2-17 HIGH (RFC-0967-A1-A1 wrapper consistency).

**R3 objective:** resolve F-P4.2-16 CRITICAL via per-RFC phantom substrate file ref grep + assess L[N] line ref compliance + version pin distribution.

## 1. Phantom Substrate File Ref Corpus Audit (R3 ground-truth)

Per research doc v3.7.2 row R11 fix F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105: phantom substrate file refs (paths to `crates/octo-*/src/*.rs` files that don't exist or were REVERTED per R10.5 scope correction) must be wrapped with "REVERTED per R10.5" qualifier OR replaced with substrate-side registry placeholder text.

### Per-RFC phantom ref grep (R3 ground-truth)

| # | RFC | Total `crates/octo-*/src/` refs | REVERTED-wrapped | Bare (unwrapped) | Status |
|---|-----|----------------------------------|------------------|------------------|--------|
| 1 | RFC-0105 v3.0 | 0 | 0 | 0 | ✓ no phantom refs |
| 2 | RFC-0903-D1 v1.0 | 0 | 0 | 0 | ✓ no phantom refs |
| 3 | RFC-0959 v2.1 | **1** | 0 | **1 (L31: `// crates/octo-vault/src/burn_event_ref.rs`)** | ⚠ 1 unwrapped phantom |
| 4 | RFC-0960 v3.1 | 0 | 0 | 0 | ✓ no phantom refs |
| 5 | RFC-0967-A1 v1.5 | **5** | 4 | **1 (L17: `crates/octo-policy/src/domain_separators.rs` in R8 narrative)** | ⚠ 1 unwrapped phantom |
| 6 | RFC-0967-A1-A1 | **9** | 9 | 0 | ✓ all wrapped |
| 7 | RFC-0010 v1.7 | 0 | 0 | 0 | ✓ no phantom refs |
| 8 | RFC-0206 v3.0 | **2** | 2 | 0 | ✓ all wrapped |
| 9 | RFC-0206 v3.3 | 0 | 0 | 0 | ✓ no phantom refs |

### Findings

**Finding F-P4.3-1 (CRITICAL):** RFC-0959 v2.1 L31 contains a BARE phantom substrate file ref: `// crates/octo-vault/src/burn_event_ref.rs`. This is OUTSIDE the R11 PARTIAL-TIGHTEN + R12 fix trail narrative scope and represents a MISSED F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105 application. Pre-promotion fix: wrap with "substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009; pre-revert reference site REVERTED per R10.5 scope correction" OR replace with substrate-side registry placeholder.

**Finding F-P4.3-2 (HIGH):** RFC-0967-A1 v1.5 L17 contains a BARE phantom substrate file ref: `crates/octo-policy/src/domain_separators.rs` in the bold R8 amendments narrative block ("phantom 'RFC-0126 §Domain Separators' citation removed per F-R8-DOMSEP-PHANTOM-SECTION (replaced with concrete `crates/octo-policy/src/domain_separators.rs` registry)"). The R8 narrative DESCRIBES the phantom resolution action; the literal path `crates/octo-policy/src/domain_separators.rs` is itself a phantom (REVERTED per R10.5). Pre-promotion fix: apply R10.5 wrapper to the L17 narrative OR rewrite narrative to "substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009; pre-revert reference site was REVERTED per R10.5".

**Finding F-P4.3-3 (HIGH):** RFC-0967-A1-A1 has 9 phantom substrate file refs, ALL wrapped with R10.5 qualifier per F-P4.2-17. R3 verification confirms wrapper consistency (audit = PASS). Per memory card R37 P3 methodology, this is the corpus STATE benchmark pattern.

**Finding F-P4.3-4 (LOW):** RFC-0967-A1 v1.5 L152 retains `crates/octo-policy/src/domain_separators.rs` in §0 Status cross-RFC reconciliation note wrapped with "R11 PARTIAL" qualifier. R3 verification: wrapper present (acceptable per R11 PARTIAL fix convention).

**Finding F-P4.3-5 (LOW):** RFC-0206 v3.0 L37 + L48 phantom substrate file refs both wrapped with R12 fix trail attribution (per R12 fix F-R12-XR-PHANTOM-0206-V30-MISSED). R3 verification: wrapper present (acceptable).

### Resolution path

Per R10.5 scope discipline (text-only edits to RFCs), the 2 unwrapped phantom refs (F-P4.3-1 CRITICAL + F-P4.3-2 HIGH) can be fixed via direct RFC text edit. Pre-promotion edit backlog addition: 2 RFC text edits (1 in RFC-0959 v2.1 L31 + 1 in RFC-0967-A1 v1.5 L17).

## 2. L[N] Line Ref Compliance (R3 fresh-lens finding)

Per `no-line-refs-anywhere.md` memory card: "All section references in RFCs, prose, cross-references, and approval criteria use `§section_name` or symbol names. NEVER by file:line. Same rule applies to principle references." Per CLAUDE.md §RFC Reference Conventions Reaffirmed: bare RFC numbers only, no file:line.

### L[N] line ref count per RFC

| # | RFC | L[N] count | Context |
|---|-----|------------|---------|
| 1 | RFC-0105 v3.0 | 1 | L34-37 in §0 Status R13 fix trail narrative (HISTORICAL — acceptable) |
| 2 | RFC-0903-D1 v1.0 | 0 | n/a |
| 3 | RFC-0959 v2.1 | 0 | n/a |
| 4 | RFC-0960 v3.1 | 0 | n/a |
| 5 | RFC-0967-A1 v1.5 | 23 | VH table rows + §0 R15 narrative + §2 R11 narrative (HISTORICAL fix trails) |
| 6 | RFC-0967-A1-A1 | 9 | §3 narrative + §4 narrative + §0 R12 narrative (HISTORICAL amendment trails) |
| 7 | RFC-0010 v1.7 | 0 | n/a |
| 8 | RFC-0206 v3.0 | 0 | n/a |
| 9 | RFC-0206 v3.3 | 13 | §2.1 L39, §2.5 L124/L130, §5 VH table rows (HISTORICAL R10/R11/R12 fix trails) |

### Findings

**Finding F-P4.3-6 (HIGH):** Total L[N] line refs across the 9 promotion candidates = 46. Distribution:
- 0 RFCs have L[N] refs in PROSE cites (outside VH table / fix-trail narrative)
- 3 RFCs have L[N] refs in HISTORICAL fix-trail narrative (RFC-0105 v3.0: 1, RFC-0967-A1 v1.5: 23, RFC-0967-A1-A1: 9, RFC-0206 v3.3: 13)

Per `no-line-refs-anywhere.md`, L[N] refs violate CLAUDE.md §No line refs anywhere. BUT per R37 P3 methodology, L[N] refs in HISTORICAL fix-trail narrative (VH table rows + R{N} fix trail explanations) serve a DIFFERENT purpose than prose cites — they document WHICH LINES were changed in WHICH ROUND, which is necessary for adversarial review audit trail.

**Resolution:** Distinguish "prose cite" L[N] (must strip) from "fix-trail historical" L[N] (must retain for audit). Corpus STATE hygiene per R37 P3:
- Prose cites: replace with `§section` references
- Fix-trail historical: retain with annotation "L[N] reference = line in v[prior_version] as of round R[N]; per R10.5 revert, current line position may differ"

**Finding F-P4.3-7 (MED):** RFC-0967-A1 v1.5 has 23 L[N] refs in fix-trail narrative. Per RFC-0967-A1 v1.2 VH row text: "removed literal `crates/octo-policy/src/domain_separators.rs` + `kind_uuid_registry.rs` file paths per R11 PARTIAL reviewer recommendation". The R12 fix F-R12-DOC-LINE-REF-CLAUDE-MD-VIOLATION was applied but L[N] refs in fix-trail narrative retained for audit. Recommend: per RFC, add a NOTE clarifying L[N] in fix-trail rows are HISTORICAL positions.

**Finding F-P4.3-8 (LOW):** RFC-0206 v3.3 §5 v3.3 VH row text contains: "F-R12-XR-EXECUTIONCLASS-DISCRIMINANT-0X01- (MED, §2.5 L130) — enum discriminant clarification ... + line ref removed per CLAUDE.md §No line refs". This is a self-referential annotation: the row says "line ref removed" but the row itself contains a line ref "L130". Minor contradiction.

## 3. Version Pin Distribution (R3 fresh-lens finding)

Per BLUEPRINT.md §RFC Reference Conventions: bare RFC numbers only in prose; version pins OK in `amends:` / `supersedes:` frontmatter + VH table column 1.

### Per-RFC version pin distribution

| # | RFC | Total pins | VH table pins | Body pins (fix-trail narrative) | Prose pins (MUST STRIP) |
|---|-----|-----------|---------------|--------------------------------|------------------------|
| 1 | RFC-0105 v3.0 | 12 | 0 | 12 (R2/R13/R15 cross-RFC consistency) | 0 |
| 2 | RFC-0903-D1 v1.0 | 1 | 0 | 1 (header `extends: RFC-XXXX vY.Y`) | 0 |
| 3 | RFC-0959 v2.1 | 8 | 0 | 8 (R10/R11 fix-trail + cross-RFC) | 0 |
| 4 | RFC-0960 v3.1 | 17 | 0 | 17 (R13/R15 cross-RFC consistency) | 0 |
| 5 | RFC-0967-A1 v1.5 | 17 | 0 | 17 (R11/R12/R15 fix-trail + cross-RFC) | 0 |
| 6 | RFC-0967-A1-A1 | 14 | 0 | 14 (R8/R9/R12 amendment narrative) | 0 |
| 7 | RFC-0010 v1.7 | 17 | 0 | 17 (R13/R14/R15 cross-RFC) | 0 |
| 8 | RFC-0206 v3.0 | 4 | 0 | 4 (R6/R12 fix-trail) | 0 |
| 9 | RFC-0206 v3.3 | 50 | 9 (VH table column 1) | 41 (R10/R11/R12/R14/R15 fix-trail) | 0 |

### Findings

**Finding F-P4.3-9 (LOW):** Across all 9 RFCs, ZERO prose cite version pins (i.e., body pins are ALL inside fix-trail narrative blocks, not in normal specification prose). The 153 total pins are CORRECTLY DISTRIBUTED per BLUEPRINT.md allowance (VH table column 1 + frontmatter amends/supersedes/extends + historical fix-trail narrative). No new finding — distribution is corpus STATE compliant.

**Finding F-P4.3-10 (LOW):** RFC-0206 v3.3 VH table column 1 uses `| v3.5 |` format (WITH `v` prefix) per F-P4.2-1 CRITICAL. Other RFCs use `| 3.0 |` (no `v`). Per corpus STATE hygiene, normalize RFC-0206 v3.3 VH format to drop `v` prefix from column 1.

## 4. Pre-Promotion Edit Backlog (R3 updated)

Per R1 + R2 + R3 findings, pre-promotion edit backlog per RFC:

| # | RFC | R1 backlog | R2 NEW | R3 NEW |
|---|-----|------------|--------|--------|
| 1 | RFC-0105 v3.0 | Populate VH + fill §0 Status + add title version | (none) | (none — already clean) |
| 2 | RFC-0903-D1 v1.0 | Add H1 + add `## 0. Status` heading + populate VH | Add `## X. AC` section + add round ref | (none) |
| 3 | RFC-0959 v2.1 | Populate VH + fill §0 Status | Add `## X. AC` section | **Wrap phantom L31 with R10.5 qualifier** (F-P4.3-1 CRITICAL) |
| 4 | RFC-0960 v3.1 | Populate VH + fill §0 Status | Consolidate 4 duplicate v3.1 rows + add `## X. AC` section | (none) |
| 5 | RFC-0967-A1 v1.5 | Populate VH + fill §0 Status + title version | Strip v prefix (n/a) + add `## X. AC` section | **Wrap phantom L17 with R10.5 qualifier** (F-P4.3-2 HIGH) |
| 6 | RFC-0967-A1-A1 | Add H1 + add frontmatter + populate VH | Add `## 0. Status` heading + add `## X. AC` section + add `## X. Execution Class Map` (inherit ref) | (none — wrapper verified) |
| 7 | RFC-0010 v1.7 | Populate VH + fill §0 Status | Add `## X. AC` section | (none) |
| 8 | RFC-0206 v3.0 | Populate VH + fill §0 Status | Add `## X. AC` section | (none — wrapper verified) |
| 9 | RFC-0206 v3.3 | Add H1 + add frontmatter + insert missing v3.2 row | Strip `v` prefix from 8 VH rows + re-sort descending + add `## X. AC` + add `## X. Execution Class Map` | Strip `v` prefix from column 1 (F-P4.2-1 CRITICAL carryover) |

**R3 final pre-promotion edit backlog:** 16-20 commits across 9 RFCs.

## 5. R3 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 1 | F-P4.3-1 (RFC-0959 v2.1 L31 bare phantom) |
| HIGH | 2 | F-P4.3-2 (RFC-0967-A1 v1.5 L17 bare phantom) + F-P4.3-6 (L[N] corpus hygiene policy gap) |
| MED | 1 | F-P4.3-7 (RFC-0967-A1 L[N] 23 concentration) |
| LOW | 6 | F-P4.3-3 + F-P4.3-4 + F-P4.3-5 + F-P4.3-8 + F-P4.3-9 + F-P4.3-10 |

**R3 NEW: 10 findings (1 CRIT + 2 HIGH + 1 MED + 6 LOW).**

## 6. Convergence Loop Status

Per R37 P3 methodology "loop-until-dry":

- **R1:** 18 findings
- **R2:** 17 effective (13 NEW + 4 R1 corrections)
- **R3:** 10 NEW (1 CRIT + 2 HIGH + 1 MED + 6 LOW)
- **R4 (next):** apply R3 fixes (F-P4.3-1 + F-P4.3-2 phantom ref wraps + F-P4.2-1 v prefix strip + R2 backlog items) + verify zero NEW findings

**Convergence direction:** R1=18 → R2=13 NEW → R3=10 NEW. Improving but NOT DRY (need 2 consecutive 0-NEW rounds).

**R4 expectation:** apply fixes, run cite validator + phantom ref grep + L[N] count + version pin distribution. Expect 0-3 NEW findings (small drift closures from R3 edits).

## 7. R10.5 Scope Discipline Recap

R3 fixes (F-P4.3-1 + F-P4.3-2 phantom ref wraps + F-P4.2-1 v prefix strip in RFC-0206 v3.3 VH table) are RFC text + frontmatter edits ONLY. NO substrate crate code edits. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 8. Cross-References

- Phase 4.1 R1 doc: `docs/research/2026-08-22-rfc-promotion-cascade-readiness.md`
- Phase 4.2 R2 doc: `docs/research/2026-08-22-rfc-promotion-cascade-r2-section-lens.md`
- Research doc v3.7.2 row R11 fix F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Memory card `no-line-refs-anywhere.md`: L[N] ref discipline
- Pre-commit validator: `scripts/validate_cites.sh` (Guard 2)
- BLUEPRINT.md §RFC Reference Conventions: bare RFC numbers only

## 9. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial R3 fresh-lens analysis; 10 NEW findings (1 CRIT + 2 HIGH + 1 MED + 6 LOW); F-P4.2-16 CRITICAL resolved with per-RFC phantom ref grep table; F-P4.3-1 CRITICAL NEW: RFC-0959 v2.1 L31 bare phantom ref; F-P4.3-2 HIGH NEW: RFC-0967-A1 v1.5 L17 bare phantom ref in R8 narrative; L[N] ref analysis: 0 prose cites, 46 fix-trail historical cites (corpus STATE hygiene policy gap); version pin distribution: 0 prose cites, 153 fix-trail historical (corpus STATE compliant). Convergence: R1=18 → R2=13 NEW → R3=10 NEW. R4 expected to converge toward DRY. |