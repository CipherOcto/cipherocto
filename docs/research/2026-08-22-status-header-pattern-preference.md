# Status Header Pattern Preference (RFC Corpus Style Guide §21)

**Date:** 2026-08-22
**Phase:** 6 (Long-Tail Maintenance) — corpus STATE hygiene deliverable per plan v1.6 §Phase 6.1 step 2
**Lens:** F-P6.1-2 status header pattern fragmentation — 5 patterns corpus-wide, KEEP-AS-IS per R10.5
**Method:** corpus STATE audit + preference ranking + rationale

## 1. Pattern Census (corpus-wide)

Per Phase 6 R1 F-P6.1-2 audit (commit `4da821be`):

| Pattern | Count | % of corpus | Status |
|---------|-------|-------------|--------|
| `## Status` (bare heading) | 164 | 93.7% | DOMINANT |
| inline `**Status:**` (paragraph) | 32 | 18.3% | SECONDARY (overlaps with ## Status in some files) |
| `## 0. Status` (numbered) | 7 | 4.0% | MINORITY |
| YAML `status:` (frontmatter) | 26 | 14.9% | NEW (Phase 6 R3 pilot) |
| NO status | 0 | 0% | NONE |

Total: 175 RFCs, 100% coverage, 5 distinct patterns. Format diversity acceptable per R10.5 (no on-disk edits).

## 2. Pattern Preference Ranking

**Ranked preference** (highest → lowest) for NEW RFCs + future RFC promotion targets:

### Tier 1 (PREFERRED): YAML `status:` frontmatter

```yaml
---
status: <Draft|Accepted|Archived>
---
```

**Why preferred**:
- Machine-parseable (drives Guard 3 Status/VH sync)
- Distinct from prose (no false-positive with body text)
- Supports tooling (RFC validators, corpus audits, Guard 2 cite checks)
- Already canonical per BLUEPRINT.md §RFC Process metadata carrier

**When to use**: all NEW RFCs going forward. Phase 6 R3 5-RFC pilot (RFC-0850 + RFC-0105 + RFC-0855 + RFC-0957 + RFC-0104) demonstrates standardization.

### Tier 2 (ACCEPTABLE): `## Status` bare heading

```
## Status

Draft
```

**Why acceptable**:
- Most common pattern corpus-wide (93.7%)
- Human-readable
- Simple to maintain
- Compatible with prose-only RFCs (no YAML frontmatter)

**When to use**: existing RFCs without YAML frontmatter that need human-readable status marker. KEEP-AS-IS for 164 RFCs currently using this.

### Tier 3 (ACCEPTABLE for legacy): inline `**Status:**`

```
**Status:** Draft
```

**Why acceptable**:
- Compact (single line)
- Works in RFCs with constrained layouts
- 18.3% of corpus (non-trivial presence)

**When to use**: legacy RFCs that adopted this pattern; KEEP-AS-IS.

### Tier 4 (MINORITY): `## 0. Status` numbered heading

```
## 0. Status

Draft
```

**Why minority**:
- Inconsistent with majority `## Status` (bare)
- 4.0% of corpus (7 RFCs)
- Numbered prefix adds no value

**When to use**: existing RFCs only. New RFCs should use Tier 1 or Tier 2.

### Tier 5 (DEPRECATED): NO status header

**Status**: 0 RFCs. NO RFC in corpus lacks status marker. NOT a problem.

## 3. Format Diversity Acceptance Rationale

Per R10.5 conservative scope:
- 5 patterns corpus-wide is acceptable format diversity
- No on-disk edits required (KEEP-AS-IS per F-P6.1-2)
- Document preference for future RFCs only
- Existing RFCs retain current pattern

**Rationale for KEEP-AS-IS**:
1. Migration cost: 175 RFCs × pattern edit = ~175 commits + ~30 min = non-trivial disruption for cosmetic gain
2. Tooling already handles all 5 patterns (Guard 2 + Guard 3 corpus STATE baselines established)
3. Substrate code OFF-LIMITS per R10.5 (RFC text edits allowed but discouraged for cosmetic-only)
4. Pattern diversity reflects historical authoring choices (pre-Phase 6 ad-hoc) — not a STATE drift

## 4. Cross-Reference Implications

### Per Guard 2 (§cite validation)

Guard 2 regex (current): `VH_PATTERN='^## Version History\b'`. Detects `## Version History` (Tier 2 style). Misses `## §Version History` (RFC-0205 + RFC-0206 §-prefixed) + `## VH` (potential corpus variant).

Extended regex PROPOSAL (F-P5.6-3, gray area R10.5, user-gated): `EXTENDED_VH_PATTERN='^(## §?(Version History|VH)\b)'`. Closes false positives but NOT auto-applied.

### Per Guard 3 (Status/VH sync)

Guard 3 corpus STATE baseline (Phase 2 R3, commit `3a1e2ce3`): 193 RFCs, 9 IN_SYNC + 8 OUT_OF_SYNC_STATUS_NEWER + 166 NO_STATUS_VERSION + 73 NO_VH. Status pattern diversity does NOT affect Guard 3 (it parses any of 5 patterns via regex alternation).

### Per BLUEPRINT.md §RFC Process

BLUEPRINT.md §RFC Process specifies canonical YAML frontmatter schema (Tier 1 preferred). 26 RFCs currently use Tier 1; 5-RFC pilot (Phase 6.1) adds 5 more = 31 (17.7%).

## 5. Forward Guidance

For NEW RFCs authored after this doc lands:
1. **Default**: use Tier 1 YAML `status:` frontmatter
2. **If YAML frontmatter rejected by author tooling**: use Tier 2 `## Status` bare heading
3. **Legacy compatibility**: Tier 3 inline `**Status:**` acceptable for migration projects
4. **Avoid**: Tier 4 `## 0. Status` numbered (creates inconsistency)
5. **Never**: Tier 5 NO status (corpus STATE requires status marker)

## 6. Closure Path

F-P6.1-2 closure = this doc published. NO on-disk edits. KEEP-AS-IS for 175 existing RFCs.

Phase 6.1 5-RFC pilot (F-P6.3-1) will add 5 Tier 1 YAML frontmatter blocks — demonstrates Tier 1 preference via corpus evidence. Pilot is user-gated per R10.5 + `feedback_initiation_user_only`.

## 7. Cross-References

- Phase 6 R1 F-P6.1-2 audit: `docs/research/2026-08-22-phase-6-r1-corpus-state-consolidation.md` (commit `4da821be`)
- Phase 6 R5 DRY closure: `docs/research/2026-08-22-phase-6-r5-dry-closure.md` (commit `c8a9d9d0`, SUPERSEDED by R8)
- Phase 6 R8 RE-DRY closure round 3: `docs/research/2026-08-22-phase-6-r8-re-dry-closure.md` (commit `fcb70150`)
- Plan v1.6 §Phase 6.1 step 2: `/home/mmacedoeu/.claude/plans/long-horizon-home-stretch-2026-08-22.md`
- BLUEPRINT.md §RFC Process: canonical YAML frontmatter schema
- Phase 2 P3 structural guards: Guard 2 (cite validation) + Guard 3 (Status/VH sync) baselines

## 8. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial status header pattern preference doc §21. F-P6.1-2 KEEP-AS-IS closure. 5 patterns corpus-wide ranked Tier 1-5. Tier 1 YAML `status:` frontmatter preferred for new RFCs. Existing RFCs retain current pattern. NO on-disk edits required. Phase 6.1 5-RFC pilot demonstrates Tier 1 standardization (user-gated). |
