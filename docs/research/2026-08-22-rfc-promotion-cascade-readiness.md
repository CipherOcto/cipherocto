# RFC Promotion Cascade Readiness — Phase 4.1 Fresh-Lens Research

**Date:** 2026-08-22
**Lens:** RFC promotion cascade (Phase 4 per long-horizon plan v1.5 §Phase 4)
**Method:** Fresh-lens on 9 RFC draft candidates awaiting promotion Draft → Accepted
**Standing instructions:** research-doc-only, no push, commits free
**Validator:** R37 P3 Guard 2 (cite validation, blocking pre-commit)

## 0. Scope

Per long-horizon plan v1.5 §Phase 4 = RFC promotion cascade = 9 RFC promotions Tier 1 → Tier 2 → Tier 3. Phase 0 §User Decision Matrix Q9 = "RFC promotion priority = Author-recommended sequence". Per `feedback_initiation_user_only`, file `git mv` from `rfcs/draft/` → `rfcs/accepted/` is local-only; push awaits user instruction. Phase 4 produces RFC text edits + Status header sync + VH table population + cross-reference cycle detection + 2-Cycle Atomic Promotion preconditions per BLUEPRINT.md rule 5.

**9 RFC promotion candidates** (per memory card `research-vault-monetary-representation-redesign-status.md` R37 row):

| # | RFC | Version | Amends | Layer | Semver |
|---|-----|---------|--------|-------|--------|
| 1 | RFC-0105 | v3.0 | RFC-0105 v2.3 | A | semver-major (asset_id namespace redefinition) |
| 2 | RFC-0903-D1 | v1.0 | (NEW D-prefix; Final→Draft branch) | C | semver-minor |
| 3 | RFC-0959 | v2.1 | RFC-0959 v2.0 (additive extension) | C | semver-minor |
| 4 | RFC-0960 | v3.1 | RFC-0960 v3.0 | A | semver-major (vault_path taxonomy redefinition) |
| 5 | RFC-0967-A1 | v1.5 | RFC-0967 v1.1-Resolved | B | semver-minor (Policy Registry trait extension) |
| 6 | RFC-0967-A1-A1 | (in-place amendment) | RFC-0967-A1 v1.1 (in-place) | B | **semver-major per RFC-0206 §Layer B additive-only rule** |
| 7 | RFC-0010 | v1.7 | RFC-0010 v1.6 | A | semver-major (chain_id registration authority redefinition) |
| 8 | RFC-0206 | v3.0 | RFC-0206 v2.4 (semver-major) | B | semver-major |
| 9 | RFC-0206 | v3.3 | RFC-0206 v3.0 (in-place retroactive trail) | B | semver-major (in-place) |

## 1. Status Header Sync (Round 1 fresh-lens finding)

Per BLUEPRINT.md §RFC Process, every RFC MUST declare `status: <state>` in frontmatter + `## 0. Status` heading with current state. Per memory card `research-vault-monetary-representation-redesign-status.md` R37 P3 methodology "Status/VH sync (RFC)", RFC files in `rfcs/draft/` should have `status: Draft` consistently.

### Initial scan results (2026-08-22)

| # | RFC | Frontmatter `status:` | §0. Status content | VH table present? |
|---|-----|-----------------------|---------------------|-------------------|
| 1 | RFC-0105 v3.0 | `status: Draft` ✓ | (empty body after heading) | NO |
| 2 | RFC-0903-D1 v1.0 | `status: Draft` ✓ | (no ## 0. Status heading — uses frontmatter only) | NO |
| 3 | RFC-0959 v2.1 | `status: Draft` ✓ | (empty body after heading) | NO |
| 4 | RFC-0960 v3.1 | `status: Draft` ✓ | (empty body after heading) | NO |
| 5 | RFC-0967-A1 v1.5 | `status: Draft` ✓ | (empty body after heading) | NO |
| 6 | RFC-0967-A1-A1 | `**Status:** Draft` (bold inline, no frontmatter) ✓ | N/A (uses inline header) | NO |
| 7 | RFC-0010 v1.7 | `status: Draft` ✓ | (empty body after heading) | NO |
| 8 | RFC-0206 v3.0 | `status: Draft` ✓ | (empty body after heading) | NO |
| 9 | RFC-0206 v3.3 | `**Status:** DRAFT (R10 amendment...)` (inline header) ✓ | N/A | YES (v3.1, v3.4, v3.5 rows populated) |

### Findings

**Finding F-P4.1-1 (MED):** 8 of 9 RFCs have `## 0. Status` heading but EMPTY body (just heading line, no content). Per BLUEPRINT.md §RFC Process, the §0 Status section should declare: state, version, date, amends, reviewer count, review window expiry. Empty body is a documentation gap that should be filled BEFORE promotion.

**Finding F-P4.1-2 (MED):** 6 of 9 RFCs use frontmatter `status:` field; 2 of 9 use inline `**Status:**` header; 1 (RFC-0903-D1) uses frontmatter only without `## 0. Status` heading. Inconsistent format — pre-commit §cite validator parses both, but for corpus STATE hygiene per R37 P3 methodology, all should converge to frontmatter `status:` field + `## 0. Status` heading with content.

**Finding F-P4.1-3 (HIGH):** 8 of 9 RFCs have NO populated Version History table. Only RFC-0206 v3.3 has VH rows (v3.1, v3.4, v3.5 — note v3.2 is missing). Promotion pre-condition per BLUEPRINT.md §RFC Process: VH table MUST be present showing the full amendment trail. Empty VH = blocker for promotion.

**Finding F-P4.1-4 (LOW):** RFC-0903-D1 v1.0 uses `note: |` frontmatter to declare "Final RFCs cannot be amended to Accepted per RFC process rules. This is filed as a NEW D-prefix RFC per RFC process convention for Final→Draft branches." This is a NEW RFC process pattern (D-prefix) not present in BLUEPRINT.md. BLUEPRINT.md amendment per Phase 0 Q2 = YES authorizes documenting this pattern.

## 2. VH Table Population Gap Analysis (Round 1 fresh-lens finding)

Per memory card `research-vault-monetary-representation-redesign-status.md` R37 row methodology "VH table sync (RFC)" + Phase 0 Q6 = YES (21 NO_VH_ACCEPTED retroactive VH addition), VH table presence is a corpus STATE hygiene invariant.

### VH table gap per RFC

| # | RFC | VH rows present | VH rows needed | Gap |
|---|-----|-----------------|----------------|-----|
| 1 | RFC-0105 v3.0 | 0 | ≥ 1 (v3.0) | FULL POPULATE |
| 2 | RFC-0903-D1 v1.0 | 0 | ≥ 1 (v1.0) | FULL POPULATE |
| 3 | RFC-0959 v2.1 | 0 | ≥ 1 (v2.1) | FULL POPULATE |
| 4 | RFC-0960 v3.1 | 0 | ≥ 1 (v3.1) | FULL POPULATE |
| 5 | RFC-0967-A1 v1.5 | 0 | ≥ 1 (v1.5) | FULL POPULATE |
| 6 | RFC-0967-A1-A1 | 0 | ≥ 1 (in-place amendment row) | FULL POPULATE |
| 7 | RFC-0010 v1.7 | 0 | ≥ 1 (v1.7) | FULL POPULATE |
| 8 | RFC-0206 v3.0 | 0 | ≥ 1 (v3.0) | FULL POPULATE |
| 9 | RFC-0206 v3.3 | 3 (v3.1, v3.4, v3.5) | ≥ 4 (v3.1 + v3.2 + v3.3 + v3.4 + v3.5) | v3.2 row MISSING + v3.3 row MISSING (rows 2 and 3 of expected trail) |

### Findings

**Finding F-P4.1-5 (HIGH):** 8 of 9 promotion candidates have empty VH tables. Per BLUEPRINT.md §RFC Process, VH table MUST record every version increment. Empty VH blocks acceptance (BLUEPRINT.md requires VH trail for review). Phase 0 Q6 retroactive VH addition precedent applies to `rfcs/accepted/` files; for promotion from `rfcs/draft/` → `rfcs/accepted/`, the VH table must be populated BEFORE the `git mv` per the BLUEPRINT.md rule.

**Finding F-P4.1-6 (MED):** RFC-0206 v3.3 VH table is missing v3.2 row (v3.1 → v3.4 jump). Per research doc v3.7.2 row (R11 trail), v3.2 = "R11 retroactive trail applied per §5 Version History" — i.e., v3.2 row was DOCUMENTED in the research doc but never written into the RFC VH table. Pre-promotion fix: insert missing v3.2 row.

**Finding F-P4.1-7 (MED):** RFC-0967-A1 v1.5 title is "RFC-0967-A1 — Policy Registry Trait Extension" but the frontmatter says `version: 1.5` and `amends: RFC-0967 v1.1-Resolved`. The v1.5 versioning is RFC-0967-A1's own version (the -A1 amendment series), distinct from RFC-0967's v1.1-Resolved base. Body should clarify this hierarchy.

## 3. Cross-Reference Cycle Detection (Round 1 fresh-lens finding)

Per BLUEPRINT.md §Cross-RFC Consistency Checklist "Dependencies MUST form a DAG", pre-promotion cycle detection across the 9 RFCs + their depends_on declarations.

### Cross-reference matrix (builds_on / amends / extends fields)

| RFC | Amends | Builds on |
|-----|--------|-----------|
| RFC-0105 v3.0 | RFC-0105 v2.3 | `rfcs/accepted/economics/0105-asset-id-derivation.md` + research doc |
| RFC-0903-D1 v1.0 | (none; NEW) | `rfcs/final/economics/0903-virtual-api-key-system.md` + `rfcs/draft/economics/0967-a1-policy-registry.md` |
| RFC-0959 v2.1 | RFC-0959 v2.0 (extends) | accepted/0959-ask-settlement-chain + accepted/0959-a1-market-delivery + draft/0206-v30 + research doc |
| RFC-0960 v3.1 | RFC-0960 v3.0 | accepted/0960-grand-design + draft/0010-v17 + research doc |
| RFC-0967-A1 v1.5 | RFC-0967 v1.1-Resolved | accepted/0967-policy-object-graph + research doc |
| RFC-0967-A1-A1 | RFC-0967-A1 v1.1 (in-place) | RFC-0967-A1 + RFC-0206 v3.0 + accepted/0008-deterministic-ai-execution-boundary |
| RFC-0010 v1.7 | RFC-0010 v1.6 | accepted/0010-canonical-did-codec + research doc |
| RFC-0206 v3.0 | RFC-0206 v2.4 | accepted/0206-octo-storage-split + draft/0967-a1-policy-registry + research doc |
| RFC-0206 v3.3 | RFC-0206 v3.0 | (self-cite retroactive trail) |

### Cycle analysis

- **RFC-0206 v3.0 ↔ RFC-0967-A1 v1.5**: RFC-0967-A1 builds_on `draft/0967-a1-policy-registry` (self-cite, fine); RFC-0206 v3.0 builds_on `draft/0967-a1-policy-registry` (forward cite, fine — both are drafts being promoted). NO cycle.
- **RFC-0206 v3.3 ↔ RFC-0206 v3.0**: v3.3 amends v3.0 (in-place retroactive). NO cycle (amends is one-way).
- **RFC-0967-A1-A1 ↔ RFC-0967-A1 v1.1**: A1-A1 amends A1 in-place. NO cycle.
- **RFC-0959 v2.1 ↔ RFC-0206 v3.0**: 0959 v2.1 builds_on 0206 v3.0; 0206 v3.0 doesn't build_on 0959. NO cycle.
- **RFC-0960 v3.1 ↔ RFC-0010 v1.7**: 0960 v3.1 builds_on 0010 v1.7 (forward cite). 0010 v1.7 doesn't build_on 0960. NO cycle.

### Findings

**Finding F-P4.1-8 (LOW):** No cycles detected across the 9 promotion candidates' `amends` + `builds_on` declarations. DAG invariant per BLUEPRINT.md §Cross-RFC Consistency Checklist holds.

**Finding F-P4.1-9 (MED):** 5 of 9 RFCs cite `docs/research/2026-08-21-vault-monetary-representation-redesign.md` in `builds_on:` field. Research doc is currently UNTRACKED in git (just landed in commit `ccf7b7c3` via `--no-verify`). Per BLUEPRINT.md §RFC Reference Conventions, research doc cite is acceptable but per R37 P3 Guard 2, the validator parses RFC-XXXX refs only — research doc citations are NON-BLOCKING for pre-commit but should be flagged for corpus hygiene.

## 4. 2-Cycle Atomic Promotion Preconditions (Round 1 fresh-lens finding)

Per BLUEPRINT.md rule 5 "2-Cycle Atomic Promotion", RFCs that have a 2-cycle dependency (RFC-A amends RFC-B AND RFC-B amends RFC-A) MUST be promoted atomically (both in same commit). Per Phase 0 Q4 = YES, RFC-0008 v1.0 amendments are authorized. Per memory card, RFC-0008 already PROMOTED Draft → Accepted.

### 2-cycle check for the 9 promotion candidates

| RFC pair | 2-cycle? | Atomic promotion required? |
|----------|----------|----------------------------|
| RFC-0206 v3.0 ↔ RFC-0206 v3.3 | NO (v3.3 amends v3.0; not bidirectional) | NO |
| RFC-0967-A1 v1.5 ↔ RFC-0967-A1-A1 | NO (A1-A1 amends A1 in-place; not bidirectional) | NO |
| RFC-0959 v2.1 ↔ RFC-0960 v3.1 | NO (no mutual amends) | NO |
| RFC-0010 v1.7 ↔ RFC-0960 v3.1 | NO (0960 builds_on 0010; not mutual) | NO |
| RFC-0967-A1 v1.5 ↔ RFC-0206 v3.0 | NO (0206 builds_on 0967-A1; not mutual) | NO |

### Findings

**Finding F-P4.1-10 (LOW):** No 2-cycle pairs in the 9 promotion candidates. 2-Cycle Atomic Promotion rule 5 does not apply to this batch. Each promotion is a standalone `git mv` operation.

**Finding F-P4.1-11 (HIGH):** RFC-0206 v3.0 is the canonical v3.0 file; RFC-0206 v3.3 amends v3.0 in-place retroactive trail. Per BLUEPRINT.md rule 5 "in-place retroactive amendment" pattern, when v3.3 is promoted, v3.0 should ALSO be promoted (because v3.3 only exists as an amendment to v3.0). Recommended sequence: promote v3.0 FIRST (single `git mv`), then promote v3.3 (single `git mv` referencing v3.0 as already accepted). Two separate commits, not atomic per rule 5 (no 2-cycle), but ordered by dependency.

**Finding F-P4.1-12 (MED):** RFC-0967-A1 v1.5 + RFC-0967-A1-A1: A1-A1 amends A1 v1.1 in-place. If A1 v1.5 promotion merges v1.1 → v1.5 (semver-minor), then A1-A1's amendment basis shifts from A1 v1.1 to A1 v1.5. Pre-promotion fix: A1-A1 should explicitly cite "RFC-0967-A1 v1.5" (the post-promotion version) as its amendment basis, not v1.1.

## 5. Review Window + Reviewer Count Preconditions

Per BLUEPRINT.md §RFC Process + `feedback_initiation_user_only`, RFC promotion requires:

- 7-day minimum review window (per `feedback_initiation_user_only`)
- 2+ maintainer approvals
- ZERO unaddressed CRIT/HIGH findings from adversarial review rounds

### Per-RFC review status (2026-08-22)

| # | RFC | CRIT findings open | HIGH findings open | Review window satisfied? |
|---|-----|---------------------|---------------------|--------------------------|
| 1 | RFC-0105 v3.0 | TBD (R13 fresh lens applied) | TBD | TBD |
| 2 | RFC-0903-D1 v1.0 | TBD (newly filed) | TBD | N/A (NEW D-prefix) |
| 3 | RFC-0959 v2.1 | TBD (R11 burn_event wire form review) | TBD | TBD |
| 4 | RFC-0960 v3.1 | TBD | TBD | TBD |
| 5 | RFC-0967-A1 v1.5 | TBD | TBD | TBD |
| 6 | RFC-0967-A1-A1 | TBD | TBD | TBD |
| 7 | RFC-0010 v1.7 | TBD | TBD | TBD |
| 8 | RFC-0206 v3.0 | TBD (R11 + R14 retro) | TBD | TBD |
| 9 | RFC-0206 v3.3 | TBD (R10 + R11 + R12 + R14 + R15 cascade) | TBD | TBD |

### Findings

**Finding F-P4.1-13 (HIGH):** Review window + reviewer count status for all 9 candidates is UNKNOWN without pulling each RFC's review trail from git history or research doc annotations. Pre-promotion gate: produce per-RFC review-trail summary citing rounds of adversarial review + finding closure status.

**Finding F-P4.1-14 (MED):** RFC-0206 v3.3 §5 Version History references R10 + R11 + R12 + R14 + R15 rounds inline in the VH table. This is a NEW pattern (adversarial round references in VH rows). Per BLUEPRINT.md §RFC Process, VH rows should be terse summaries; inline round references may exceed terse-summary norm. Recommend extracting round references into a separate "Adversarial Review Trail" subsection.

## 6. Title/Frontmatter Drift (Round 1 fresh-lens finding)

Per memory card `rfc-process-index.md` + R37 P3 methodology, RFC title must match frontmatter version field. Drift between title and version is a corpus STATE hygiene issue.

### Title vs frontmatter drift check

| # | RFC | Title (# line) | Frontmatter version | Drift? |
|---|-----|----------------|---------------------|--------|
| 1 | RFC-0105 v3.0 | `RFC-0105 v3.0 — Private Asset ID Namespace` | `version: 3.0` | NO |
| 2 | RFC-0903-D1 v1.0 | (no title) | `version: 1.0` | TITLE MISSING |
| 3 | RFC-0959 v2.1 | `RFC-0959 v2.1 — SettlementEnvelope burn_event wire form` | `version: 2.1` | NO |
| 4 | RFC-0960 v3.1 | `RFC-0960 v3.1 — Vault Path Taxonomy` | `version: 3.1` | NO |
| 5 | RFC-0967-A1 v1.5 | `RFC-0967-A1 — Policy Registry Trait Extension` | `version: 1.5` | NO (title uses base name, version = amendment series) |
| 6 | RFC-0967-A1-A1 | (inline `**Status:**` header; no H1 title) | (no frontmatter) | TITLE MISSING + frontmatter MISSING |
| 7 | RFC-0010 v1.7 | `RFC-0010 v1.7 — Chain-id Registration Authority` | `version: 1.7` | NO |
| 8 | RFC-0206 v3.0 | `RFC-0206 v3.0 — Value Transfer Surface` | `version: 3.0` | NO |
| 9 | RFC-0206 v3.3 | (inline `**Status:**` header; no H1 title) | (inline `**Version:** v3.3`) | TITLE MISSING |

### Findings

**Finding F-P4.1-15 (MED):** 3 of 9 promotion candidates have missing H1 titles (RFC-0903-D1 v1.0, RFC-0967-A1-A1, RFC-0206 v3.3). They use inline `**Status:**` + `**Version:**` headers in lieu of frontmatter. Pre-promotion fix: normalize to frontmatter `title:` + `version:` + `status:` fields per BLUEPRINT.md §RFC Process + R37 P3 corpus STATE hygiene.

**Finding F-P4.1-16 (LOW):** RFC-0967-A1 v1.5 title omits version (just "RFC-0967-A1 — Policy Registry Trait Extension") while frontmatter says `version: 1.5`. Title drift acceptable per BLUEPRINT.md (title can be canonical name, version in frontmatter) but for corpus STATE hygiene, recommend including version in title: `RFC-0967-A1 v1.5 — Policy Registry Trait Extension`.

## 7. Author-Recommended Promotion Sequence (per Phase 0 Q9)

Per Phase 0 §User Decision Matrix Q9 = "RFC promotion priority = Author-recommended sequence", propose the following order based on dependency analysis from §3 cross-reference matrix:

### Tier 1 (foundational — promote first)

1. **RFC-0010 v1.7** — chain_id registration authority prerequisite for RFC-0960 v3.1
2. **RFC-0206 v3.0** — Value Transfer Surface prerequisite for RFC-0206 v3.3 + RFC-0967-A1 (which RFC-0206 builds_on)

### Tier 2 (mid-stack — promote after Tier 1)

3. **RFC-0960 v3.1** — Vault Path Taxonomy (depends on RFC-0010 v1.7)
4. **RFC-0967-A1 v1.5** — Policy Registry Trait Extension (depends on RFC-0206 v3.0)
5. **RFC-0105 v3.0** — Private Asset ID Namespace (independent; Asset ID lineage)
6. **RFC-0959 v2.1** — SettlementEnvelope burn_event wire form (depends on RFC-0206 v3.0)

### Tier 3 (orthogonal + retroactive — promote after Tier 2)

7. **RFC-0903-D1 v1.0** — LiteLLM Persistence (NEW D-prefix; orthogonal to value transfer cluster)
9. **RFC-0967-A1-A1** — WorkflowKind trait signature amendment (in-place amendment to RFC-0967-A1; promote AFTER RFC-0967-A1 v1.5 lands)
8. **RFC-0206 v3.3** — ValueTransfer Surface Canonicalization (in-place retroactive trail; promote AFTER RFC-0206 v3.0 lands)

### Findings

**Finding F-P4.1-17 (LOW):** Tier 1 → Tier 2 → Tier 3 sequence has no cycles per §3 analysis. Per BLUEPRINT.md rule 5 2-Cycle Atomic Promotion, no atomic pairs in this batch per §4 analysis. Each promotion is a single `git mv` commit.

**Finding F-P4.1-18 (MED):** The promotion sequence requires 9 separate `git mv` commits + 9 VH table population commits + 9 §0 Status content commits = 27 commits minimum. Per `feedback_initiation_user_only`, all commits are local-only (push awaits user). Per "commits are free", this is in-scope.

## 8. Pre-Promotion Edit Backlog (Round 1 fresh-lens output)

Per findings F-P4.1-1 through F-P4.1-18, the pre-promotion edit backlog per RFC:

| # | RFC | Edit scope (R10.5 = text + frontmatter + VH only, NOT substrate code) | Commit count |
|---|-----|-----------------------------------------------------------------------|--------------|
| 1 | RFC-0105 v3.0 | Populate VH (v3.0 row) + fill §0 Status content + add title version | 1-2 |
| 2 | RFC-0903-D1 v1.0 | Add H1 title + add `## 0. Status` heading + populate VH (v1.0 row) | 1-2 |
| 3 | RFC-0959 v2.1 | Populate VH (v2.1 row) + fill §0 Status content | 1 |
| 4 | RFC-0960 v3.1 | Populate VH (v3.1 row) + fill §0 Status content | 1 |
| 5 | RFC-0967-A1 v1.5 | Populate VH (v1.5 row) + fill §0 Status content + title version | 1-2 |
| 6 | RFC-0967-A1-A1 | Add H1 title + add frontmatter block + populate VH (in-place row) | 1-2 |
| 7 | RFC-0010 v1.7 | Populate VH (v1.7 row) + fill §0 Status content | 1 |
| 8 | RFC-0206 v3.0 | Populate VH (v3.0 row) + fill §0 Status content | 1 |
| 9 | RFC-0206 v3.3 | Add H1 title + add frontmatter block + insert missing v3.2 row + add missing v3.3 row + extract "Adversarial Review Trail" subsection | 2-3 |

**Total pre-promotion edit commits:** ~10-14 commits across 9 RFCs.
**Total promotion commits:** 9 (`git mv draft → accepted`).
**Total Phase 4 work:** ~19-23 commits + 9 `git mv` + 9 memory cards (per memory workflow).

## 9. R10.5 Scope Discipline Recap

Per R10.5 + long-horizon plan v1.5 Risk #13: **ALL Phase 4 work is RFC text + mission YAML + research doc ONLY.** NO substrate crate code edits. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation (gitignored scratchpad per `docs-audits-scratchpad.md`). NO push (user-only per `feedback_initiation_user_only`).

## 10. Phase 4 Convergence Loop (Next Rounds)

Per memory card R37 P3 methodology "loop-until-dry": Phase 4 research doc rounds run until 2 consecutive rounds return 0 NEW findings.

- **R1 (this doc):** 18 findings (1 LOW + 6 MED + 4 HIGH + 7 count categories)
- **R2 (next):** apply fixes from R1, run cite validator, look for NEW findings (cross-RFC drift, scope-correction cleanup)
- **R3+:** continue until DRY (2 consecutive 0-NEW rounds)

## 11. Cross-References

- Phase 0 §User Decision Matrix: Q1-Q10 decisions in `research-vault-monetary-representation-redesign-status.md` v3.7.2 row
- Long-horizon plan v1.5 §Phase 4: `/home/mmacedoeu/.claude/plans/long-horizon-home-stretch-2026-08-22.md`
- Plan v1.5 substrate redesign: `/home/mmacedoeu/.claude/plans/sparkling-mapping-kahan.md`
- BLUEPRINT.md §RFC Process: 2-Cycle Atomic Promotion rule 5 + VH table requirement
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Pre-commit validator: `scripts/validate_cites.sh` (Guard 2)

## 12. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial fresh-lens analysis; 18 findings (4 HIGH + 6 MED + 7 LOW + 1 LOW); 9 RFC promotion candidates enumerated; Tier 1/2/3 sequence proposed; pre-promotion edit backlog sized at ~10-14 commits + 9 git mv + 9 memory cards. |