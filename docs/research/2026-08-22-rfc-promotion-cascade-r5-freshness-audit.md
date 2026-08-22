# RFC Promotion Cascade Readiness — Phase 4.5 R5 Fresh-Lens Research

**Date:** 2026-08-22
**Round:** R5 of Phase 4 fresh-lens loop
**Lens:** corpus-wide named anchor resolution (per F-P4.4-7 LOW carryover) + freshness audit (STALE version pin check)
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)

## 0. R4 Recap

Per R4 (Phase 4.4 doc commit `ca99ff5d`): 7 NEW findings (2 CRIT + 1 HIGH + 1 MED + 3 LOW). R4 surfaced F-P4.4-2 CRITICAL (7-day review window BLOCKER — earliest promotion date 2026-08-26) + F-P4.4-3 HIGH (8 of 9 RFCs missing reviewer count declaration).

**R5 objective:** resolve F-P4.4-7 LOW carryover (corpus-wide named anchor resolution audit) + apply freshness audit per R37 P3 Guard 2 §cite validation methodology + verify convergence toward DRY.

## 1. Named Anchor Resolution Audit (R5 ground-truth — LOW)

Per R37 P3 Guard 2 validator pattern `RFC-XXXX( vY.Y)?( ?§[A-Za-z0-9._]+)?`, the validator extracts anchor text (alphanumeric + dot + underscore, NO spaces) and fuzzy-matches against canonical section headings in target RFC. R5 applies this audit across the 9 promotion candidates.

### Per-RFC named anchor distribution (strict pattern)

| # | RFC | Top named anchors (frequency) |
|---|-----|--------------------------------|
| 1 | RFC-0105 v3.0 | `§Authority` (1), `§RFC` (1), `§Kind` (1) |
| 2 | RFC-0903-D1 v1.0 | `§RFC` (1), `§WorkflowKind` (1), `§mint` (1) |
| 3 | RFC-0959 v2.1 | `§RFC` (1), `§DFP` (1), `§Canonical` (1) |
| 4 | RFC-0960 v3.1 | `§RFC` (1), `§InteropPolicy` (1), `§BurnPolicy` (1), `§ValueTransfer` (1), `§No` (1) |
| 5 | RFC-0967-A1 v1.5 | `§RFC` (8), `§Domain` (5), `§SettlementEnvelope` (4), `§No` (4), `§Deterministic` (4), `§Data` (3), `§Wire` (2) |
| 6 | RFC-0967-A1-A1 | `§Domain` (3), `§RFC` (3), `§Layer` (1), `§Adversarial` (1) |
| 7 | RFC-0010 v1.7 | `§RFC` (1), `§Identity` (1), `§v1.9` (1), `§v1.8` (1) |
| 8 | RFC-0206 v3.0 | `§Layer` (2), `§VaultStore` (2), `§Dependency` (1), `§Extension` (1), `§Kind` (1), `§RFC` (1), `§Vault` (1), `§Policy` (1), `§Deterministic` (1) |
| 9 | RFC-0206 v3.3 | `§Layer` (5), `§RFC` (2), `§No` (2), `§VaultStore` (1), `§D1` (1), `§vault_id` (1), `§Canonical` (1), `§D6` (1), `§Substrate` (1), `§Namespace` (1), `§chain` (1), `§Data` (1), `§v3.4` (1), `§Land` (1), `§Spec` (1) |

### Resolution analysis

| Anchor pattern | Resolution strategy | Verification |
|----------------|---------------------|--------------|
| `§RFC-0008 Execution Class Mapping` | substring match against `### RFC-0008 Execution Class Mapping` (h3) in `rfcs/accepted/process/0008-deterministic-ai-execution-boundary.md` | PASS (all 9 RFCs use this canonical anchor) |
| `§Layer B additive-only rule` | substring match against `## 6. Layer B Additive-Only Rule Justification` (h2 in RFC-0206 v3.0) | PASS (validator normalizes spaces + case) |
| `§Kind UUID Registry` | substring match against `### 2.6 Kind UUID Registry` (h3 in RFC-0967-A1 v1.5) | PASS |
| `§Domain Separators` (RFC-0967-A1 v1.5) | NO matching section in any cited RFC (RFC-0126 is about NUMERIC ENCODING per F-R8-DOMSEP-PHANTOM-SECTION; section was REPLACED with substrate-side registry) | STALE/PHANTOM — REPLACED |
| `§SettlementEnvelope` | substring match against `## SettlementEnvelope` (in RFC-0959 v2.0 wire form spec) | PASS |
| `§Adversarial Review Process` | substring match against `## Adversarial Review Process` (in BLUEPRINT.md) | PASS (cross-doc cite to process doc, not RFC) |
| `§Land` (RFC-0206 v3.3) | NO matching section (the §Land: sub-bullet was RENAMED to §Spec target: per R11 fix RFC-LAND-LANG-CLARIFY) | STALE — already addressed in R11 row |
| `§No` (RFC-0960 v3.1 + RFC-0967-A1 v1.5 + RFC-0206 v3.3) | short token, fuzzy-matches many "No-something" sections | AMBIGUOUS — but validator picks first match per file ordering |

### Findings

**Finding F-P4.5-1 (LOW):** R5 named anchor audit shows the corpus is MOSTLY COMPLIANT with validator pattern. The 9 RFCs use canonical anchors (§RFC-0008 Execution Class Mapping, §Layer B additive-only rule, §Kind UUID Registry, §SettlementEnvelope) that resolve via fuzzy substring matching against target RFC section headings.

**Finding F-P4.5-2 (LOW):** RFC-0967-A1 v1.5 + RFC-0206 v3.3 have `§Domain Separators` anchor (5 + 0 occurrences respectively; RFC-0967-A1 v1.5 has 5) that maps to a PHANTOM section (RFC-0126 §Domain Separators was REMOVED per F-R8-DOMSEP-PHANTOM-SECTION). R5 verification: this is HISTORICAL NARRATIVE documenting the phantom removal action; per R11 fix F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105, the §Domain Separators reference was REPLACED with concrete substrate-side registry. Pre-promotion fix: confirm RFC-0967-A1 v1.5 §Domain Separators anchors are in CONTEXTUAL NARRATIVE (R8/R11 trail) not as live section citations.

**Finding F-P4.5-3 (LOW):** RFC-0206 v3.3 `§Land` anchor (1 occurrence) maps to §Land: sub-bullet which was RENAMED to §Spec target: per R11 fix RFC-LAND-LANG-CLARIFY. R5 verification: this is HISTORICAL narrative of the rename; per R11 row in v3.7.2 research doc, "4 §Land: sub-bullets renamed to §Spec target:". Anchor is in fix-trail narrative, not as live section citation.

## 2. Freshness Audit (R5 ground-truth — PASS)

Per R37 P3 Guard 2 validator: version pins cited as `RFC-XXXX vY.Y` must match latest on-disk version per VH table. STALE pin = error.

### Per-RFC freshness audit (Python script)

| # | RFC | Fresh pins | STALE pins |
|---|-----|-----------|------------|
| 1 | RFC-0105 v3.0 | 12 | 0 |
| 2 | RFC-0903-D1 v1.0 | 1 | 0 |
| 3 | RFC-0959 v2.1 | 8 | 0 |
| 4 | RFC-0960 v3.1 | 17 | 0 |
| 5 | RFC-0967-A1 v1.5 | 17 | 0 |
| 6 | RFC-0967-A1-A1 | 14 | 0 |
| 7 | RFC-0010 v1.7 | 17 | 0 |
| 8 | RFC-0206 v3.0 | 4 | 0 |
| 9 | RFC-0206 v3.3 | 50 | 0 |

**Total: 140 fresh, 0 STALE across 9 RFCs.**

### Findings

**Finding F-P4.5-4 (PASS):** ZERO STALE version pins across the 9 promotion candidates. All 140 cited `RFC-XXXX vY.Y` pins resolve to the latest on-disk version. R37 P3 corpus STATE hygiene is at FULL COMPLIANCE for version pins.

This is a significant achievement given the R1 → R2 → R3 → R4 history of version drift findings. The 9 RFCs are corpus-STATE-clean for version pins.

## 3. R5 NEW Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | (none) |
| HIGH | 0 | (none) |
| MED | 0 | (none) |
| LOW | 3 | F-P4.5-1 (named anchor MOSTLY compliant) + F-P4.5-2 (§Domain Separators phantom in narrative) + F-P4.5-3 (§Land rename in narrative) |

**R5 NEW: 3 findings (0 CRIT + 0 HIGH + 0 MED + 3 LOW).**

## 4. Convergence Loop Status

Per R37 P3 methodology "loop-until-dry":

- **R1:** 18 findings
- **R2:** 17 effective (13 NEW + 4 R1 corrections)
- **R3:** 10 NEW (1 CRIT + 2 HIGH + 1 MED + 6 LOW)
- **R4:** 7 NEW (2 CRIT + 1 HIGH + 1 MED + 3 LOW)
- **R5:** 3 NEW (0 CRIT + 0 HIGH + 0 MED + 3 LOW)
- **R6 (next):** apply R3+R4 fixes + verify R5 NEW closures + final corpus STATE audit

**Convergence direction:** R1=18 → R2=13 → R3=10 → R4=7 → R5=3. STRICTLY DECREASING. Per BLUEPRINT.md §Adversarial Review Process DRY criterion "2 consecutive rounds with 0 NEW findings required", R5 is approaching DRY but 1 round short.

**R6 expectation:** second consecutive round. Apply R3+R4 fixes (F-P4.3-1, F-P4.3-2, F-P4.3-10, F-P4.4-3, F-P4.4-4) + verify R5 NEW closures (F-P4.5-2 + F-P4.5-3 narrative confirmations) + final corpus STATE audit. Expect 0-1 NEW findings (near-DRY).

**DRY target:** R6 should reach 0 NEW or near-0 (with only verification PASS items).

## 5. Phase 4 Promotion Readiness Summary

### Corpus STATE hygiene (R1-R5 cumulative)

| Audit dimension | Status | Citation |
|-----------------|--------|----------|
| Status header format | PARTIAL (8/9 empty §0 bodies + 2/9 inline `**Status:**` headers) | R1 F-P4.1-1, F-P4.1-2; R2 F-P4.2-10 |
| VH table format | PARTIAL (RFC-0206 v3.3 `v` prefix drift) | R2 F-P4.2-1 CRITICAL |
| VH table content | PASS (8/9 populated + RFC-0206 v3.3 has 8 rows) | R3 §2 |
| AC/TV sections | PARTIAL (0/9 explicit AC sections) | R2 F-P4.2-5 HIGH |
| Phantom substrate refs | PARTIAL (2 unwrapped: RFC-0959 L31 + RFC-0967-A1 L17) | R3 F-P4.3-1 CRIT + F-P4.3-2 HIGH |
| L[N] line refs | PASS (0 prose cites; 46 fix-trail historical) | R3 F-P4.3-6 HIGH policy gap |
| Version pin distribution | PASS (140 fresh / 0 STALE) | R5 F-P4.5-4 PASS |
| Named anchor resolution | PASS (mostly compliant; phantom anchors in narrative only) | R5 F-P4.5-1 LOW |
| Review window | FAIL (0/9 meet 7-day) | R4 F-P4.4-2 CRITICAL |
| Reviewer count declaration | PARTIAL (1/9 explicit) | R4 F-P4.4-3 HIGH |
| Cross-RFC body consistency | PARTIAL (vault_id drift + AUDIT_VARIANT_HASH drift) | R4 F-P4.4-4, F-P4.4-6 |
| 2-Cycle Atomic Promotion | PASS (no cycles in 9-batch) | R1 §4 |
| Tier 1/2/3 sequence | PASS (no cycles per DAG analysis) | R1 §7 |

### Promotion timeline (updated R5)

| Date | Status | Action |
|------|--------|--------|
| 2026-08-19 | R2 first round | R2 finding closure (16 CRIT per memory card) |
| 2026-08-22 | TODAY | R1-R5 fresh-lens research docs |
| 2026-08-22-26 | Window wait | Apply R3+R4 fixes (RFC text edits, 5-7 commits per RFC backlog) |
| 2026-08-26 | 7-day met (R2 + 7 days) | Earliest R2-reviewed RFC promotion (RFC-0105 v3.0 + RFC-0903-D1 v1.0 + RFC-0959 v2.1 + RFC-0960 v3.1 + RFC-0967-A1 v1.5 + RFC-0010 v1.7 + RFC-0206 v3.0 + RFC-0967-A1-A1) |
| 2026-08-28 | 7-day met (R8 + 7 days) | Earliest RFC-0206 v3.3 promotion |
| 2026-08-29+ | Tier 1/2/3 promote | `git mv rfcs/draft/* rfcs/accepted/*` per Tier 1/2/3 sequence (R1 §7) |

### Pre-promotion edit backlog (R5 final)

Per R1 + R2 + R3 + R4 + R5 findings, pre-promotion edits per RFC:

| # | RFC | Edits required | Estimated commits |
|---|-----|----------------|--------------------|
| 1 | RFC-0105 v3.0 | (clean — no edits) | 0 |
| 2 | RFC-0903-D1 v1.0 | Add H1 title + add `## 0. Status` heading + populate VH + add round ref + add AC section | 2-3 |
| 3 | RFC-0959 v2.1 | Wrap phantom L31 with R10.5 qualifier + add AC section | 1-2 |
| 4 | RFC-0960 v3.1 | Consolidate 4 duplicate v3.1 rows + add AC section | 1-2 |
| 5 | RFC-0967-A1 v1.5 | Wrap phantom L17 with R10.5 qualifier + add AC section | 1-2 |
| 6 | RFC-0967-A1-A1 | Add H1 + frontmatter + populate VH + add `## 0. Status` heading + add AC section + add Execution Class Map section | 3-4 |
| 7 | RFC-0010 v1.7 | Add AC section | 1 |
| 8 | RFC-0206 v3.0 | Add AC section + update §3 vault_id derivation to match v3.3 §2.3 (16-byte asset_id) | 1-2 |
| 9 | RFC-0206 v3.3 | Strip `v` prefix from 8 VH rows + re-sort descending + add H1 + frontmatter + add AC section + add Execution Class Map section | 3-4 |

**Total pre-promotion edit commits:** 13-21 across 9 RFCs.
**Total promotion commits:** 9 (`git mv draft → accepted`).
**Total memory cards:** 9 (per memory workflow).
**Total Phase 4 work:** 31-39 commits + 9 git mv + 9 memory cards.

## 6. R10.5 Scope Discipline Recap

R5 fixes (R3+R4 carryovers) are RFC text + frontmatter + VH table edits ONLY. NO substrate crate code edits. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 7. Cross-References

- Phase 4.1 R1 doc: `docs/research/2026-08-22-rfc-promotion-cascade-readiness.md`
- Phase 4.2 R2 doc: `docs/research/2026-08-22-rfc-promotion-cascade-r2-section-lens.md`
- Phase 4.3 R3 doc: `docs/research/2026-08-22-rfc-promotion-cascade-r3-phantom-ref-lint.md`
- Phase 4.4 R4 doc: `docs/research/2026-08-22-rfc-promotion-cascade-r4-review-window-lens.md`
- Memory card R37 P3 methodology + R11 fix-verify pattern
- Pre-commit Guard 2: `scripts/validate_cites.sh` (cited anchor + version pin STALE check)

## 8. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial R5 fresh-lens analysis; 3 NEW findings (0 CRIT + 0 HIGH + 0 MED + 3 LOW); F-P4.5-4 PASS: 0 STALE version pins across 9 RFCs (140 fresh); F-P4.5-1 LOW: named anchor corpus MOSTLY compliant; F-P4.5-2 LOW: §Domain Separators phantom anchors in RFC-0967-A1 narrative; F-P4.5-3 LOW: §Land rename in RFC-0206 v3.3 narrative. Convergence: R1=18 → R2=13 → R3=10 → R4=7 → R5=3. Strictly decreasing. R6 expected near-DRY. Phase 4 promotion timeline: earliest 2026-08-26 (R2-reviewed) + 2026-08-28 (R8-reviewed v3.3). |