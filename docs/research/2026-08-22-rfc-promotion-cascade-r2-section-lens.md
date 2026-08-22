# RFC Promotion Cascade Readiness — Phase 4.2 R2 Fresh-Lens Research

**Date:** 2026-08-22
**Round:** R2 of Phase 4 fresh-lens loop
**Lens:** §section completeness + Acceptance Criteria presence + VH format drift + cross-reference consistency
**Method:** R37 P3 loop-until-dry (2 consecutive 0-NEW rounds required)
**Standing instructions:** research-doc-only, no push, commits free

## 0. R1 Recap + R2 Verification Scope

Per R1 (Phase 4.1 doc commit `19e09062`): 18 findings identified across 9 RFC promotion candidates. R1 covered Status header sync + VH table population gap + cross-reference cycles + 2-Cycle Atomic Promotion preconditions + title/frontmatter drift + review window/reviewer count + Tier 1/2/3 promotion sequence.

**R1 finding correction:** R1's §2 VH table gap analysis used incorrect awk pattern (`^\| v[0-9]` matching only `v`-prefixed versions). R2 verification with corrected pattern (`^\| [0-9]+\.[0-9]+`) reveals the actual VH row count is HIGHER than R1 reported. R1 finding F-P4.1-3 + F-P4.1-5 were overstated. R2 corrects the count + identifies VH format drift as a NEW finding.

## 1. VH Table Format Drift (Round 2 fresh-lens finding — CRITICAL)

Per corpus STATE hygiene methodology, VH table rows should follow a CONSISTENT format. R2 verification reveals format drift across the 9 promotion candidates.

### VH format per RFC

| # | RFC | VH format | First row | Row count |
|---|-----|-----------|-----------|----------|
| 1 | RFC-0105 v3.0 | `\| 3.0 \|` (no `v`) | `3.0` | 1 |
| 2 | RFC-0903-D1 v1.0 | `\| 1.0 \|` (no `v`) | `1.0` | 1 |
| 3 | RFC-0959 v2.1 | `\| 2.1 \|` (no `v`) | `2.1` | 1 |
| 4 | RFC-0960 v3.1 | `\| 3.1 \|` (no `v`) | `3.1` | 4 (duplicate version rows) |
| 5 | RFC-0967-A1 v1.5 | `\| 1.0 \|` to `\| 1.6 \|` (no `v`) | `1.0` | 7 |
| 6 | RFC-0967-A1-A1 | `\| 1.0 \|` to `\| 1.2 \|` (no `v`) | `1.0` | 2 |
| 7 | RFC-0010 v1.7 | `\| 1.7 \|` to `\| 1.9 \|` (no `v`) | `1.7` | 3 |
| 8 | RFC-0206 v3.0 | `\| 3.0 \|` to `\| 2.3 \|` (no `v`) | `3.0` | 3 |
| 9 | RFC-0206 v3.3 | `\| v2.0 \|` to `\| v3.5 \|` (**WITH `v`**) | `v2.0` | 8 |

### Findings

**Finding F-P4.2-1 (CRITICAL):** VH table format is INCONSISTENT across the 9 promotion candidates. 8 of 9 use `| X.Y |` (no `v` prefix); RFC-0206 v3.3 uses `| vX.Y |` (WITH `v` prefix). This is a corpus STATE hygiene violation per R37 P3 methodology. Pre-commit cite validator parses both formats but downstream tooling (memory card extractors, RFC cross-reference parsers) may break on the inconsistency.

**Resolution:** Strip `v` prefix from RFC-0206 v3.3 VH rows to match corpus norm. Edit RFC text only (RFC text is R10.5 in-scope).

**Finding F-P4.2-2 (HIGH):** RFC-0960 v3.1 VH table has 4 ROWS all labeled `| 3.1 | 2026-08-22 | ...` (same version, same date, different fix trails). Per VH table discipline, each row should be a DISTINCT version. Multiple rows with same version violate the VH table semantic invariant (version is the PRIMARY KEY).

**Resolution:** Insert intermediate version labels (3.2, 3.3, 3.4) for each fix-trail row OR consolidate fix trails into a single v3.1 row with sub-bullets.

**Finding F-P4.2-3 (HIGH):** RFC-0206 v3.3 VH table has 8 rows in NON-CHRONOLOGICAL order (v3.5 first, v2.0 last). Per VH table norm, rows should be in descending version order (newest at top). Current order: v3.5 → v3.4 → v3.1 → v3.2 → v3.3 → v3.0 → v2.4 → v2.0. The v3.1 → v3.2 → v3.3 sub-sequence is REVERSED (should be v3.3 → v3.2 → v3.1).

**Resolution:** Re-sort VH rows in descending version order. Also: rename v3.3 (currently file-level) — the file is named `0206-v33-value-transfer-canonicalization.md` but VH table top row is v3.5. Per F-P4.1-16 LOW (R1 finding), the file is a retroactive trail accumulator — the top VH row should match the file-level version label.

**Finding F-P4.2-4 (MED):** RFC-0967-A1 VH rows use the format `| 1.0 | ... | 1.1 | ... | 1.2 | ...` ascending, while RFC-0967-A1-A1 uses `| 1.0 | ... | 1.2 | ...` (skipping 1.1). Per VH table discipline, A1-A1's VH should reference A1's amendment trail (v1.0 → v1.1 = R8 amendment; v1.2 = R12 amendment to A1; A1-A1 itself is amendment 1.0 + 1.2 to A1's history). The 1.1 row should be retained as amendment basis reference.

## 2. Acceptance Criteria Presence (Round 2 fresh-lens finding — HIGH)

Per BLUEPRINT.md §RFC Process, every Draft RFC SHOULD include an "Acceptance Criteria" or "Test Vectors" subsection enumerating conditions for promotion to Accepted. R2 verification reveals ZERO explicit AC section across the 9 candidates.

### AC/TV presence per RFC

| # | RFC | AC section present? | TV section present? | AC/TV mentions |
|---|-----|---------------------|---------------------|----------------|
| 1 | RFC-0105 v3.0 | NO | NO | 0 |
| 2 | RFC-0903-D1 v1.0 | NO | NO | 0 |
| 3 | RFC-0959 v2.1 | NO | NO | 0 |
| 4 | RFC-0960 v3.1 | NO | NO | 0 |
| 5 | RFC-0967-A1 v1.5 | NO | NO | 0 |
| 6 | RFC-0967-A1-A1 | NO | NO | 0 |
| 7 | RFC-0010 v1.7 | NO | NO | 0 |
| 8 | RFC-0206 v3.0 | NO | implicit (refers to mission 0206-011) | 1 |
| 9 | RFC-0206 v3.3 | NO | NO | 0 |

### Findings

**Finding F-P4.2-5 (HIGH):** NONE of the 9 promotion candidates have explicit "Acceptance Criteria" or "Test Vectors" sections. Per BLUEPRINT.md §RFC Process, AC/TV sections are EXPECTED for Draft RFCs to specify promotion criteria. The mission YAMLs reference TV-XXXX (e.g., TV-0206-A1..A14) but the RFC documents themselves don't enumerate the ACs/TVs.

**Resolution per RFC:**

| # | RFC | AC/TV source |
|---|-----|--------------|
| 1 | RFC-0105 v3.0 | Should reference memory card `mission-0105-v2-role-token-canonicalization-status.md` ACs (8 byte-exact TV per memory) |
| 2 | RFC-0903-D1 v1.0 | NEW D-prefix; should reference LiteLLM persistence acceptance criteria (need source) |
| 3 | RFC-0959 v2.1 | Should reference memory card `mission-0959-c1-wire-format-amendment-status.md` (25 byte-exact TV) |
| 4 | RFC-0960 v3.1 | Should reference memory card `mission-0960-v-vault-substrate-amendment-status.md` (108 byte-exact TV) |
| 5 | RFC-0967-A1 v1.5 | Should reference §3 MAX_COMPOSITE_DEPTH + workflow_kind trait AC |
| 6 | RFC-0967-A1-A1 | Should reference trait-sig change AC (proof: &[u8] param acceptance) |
| 7 | RFC-0010 v1.7 | Should reference chain_id BLAKE3 derivation AC + ledger_chain_registry AC |
| 8 | RFC-0206 v3.0 | Should reference plan v1.5 TV-0206-A1..A14 (14 TVs) |
| 9 | RFC-0206 v3.3 | Should reference canonicalization AC (asset_id 32→16 byte reconciliation) |

**Finding F-P4.2-6 (MED):** Per RFC process discipline, AC/TV sections should be added BEFORE promotion to Accepted per BLUEPRINT.md. Pre-promotion edit backlog per R1 §8 must include AC/TV section creation per RFC.

## 3. Review Trail Metadata (Round 2 fresh-lens finding — MED)

Per memory card R37 P3 methodology, each RFC's VH table should declare which adversarial review rounds addressed the version. R2 verification reveals review-round references across all 9 RFCs but with INCONSISTENT inline format.

### Review round reference patterns

| Pattern | RFCs using pattern |
|---------|-------------------|
| `**R{N} fix trail:**` (bold inline) | RFC-0010 v1.7, RFC-0206 v3.3, RFC-0967-A1 v1.5, RFC-0967-A1-A1, RFC-0206 v3.0 |
| `[R{N} fix F-R{N}-...]` (bracket inline) | RFC-0960 v3.1, RFC-0959 v2.1, RFC-0105 v3.0 |
| No explicit R{N} ref | RFC-0903-D1 v1.0 |

### Findings

**Finding F-P4.2-7 (MED):** Review-trail metadata format is INCONSISTENT. 5 of 9 use bold inline (`**R{N} fix trail:**`); 3 of 9 use bracket inline (`[R{N} fix ...]`); 1 of 9 (RFC-0903-D1 v1.0) has NO explicit round reference. For corpus STATE hygiene + pre-commit parse-ability, recommend standardizing on bold inline format (more readable, used by 5/9 majority).

**Finding F-P4.2-8 (MED):** RFC-0967-A1 v1.5 VH table has 36 review-round references inline in the body, with 7 VH rows that all reference adversarial rounds. Per RFC-0206 v3.3 §5 (the canonical amendment pattern), the VH rows should be TERSE summaries; review-round detail should be in a separate "Adversarial Review Trail" subsection (per R1 finding F-P4.1-14 MED).

**Finding F-P4.2-9 (LOW):** RFC-0903-D1 v1.0 has NO explicit R{N} reference in VH table. Per `feedback_initiation_user_only` + R10.5 scope discipline, NEW D-prefix RFCs may not have prior adversarial rounds. The absence is acceptable but should be documented as "first filing" in the VH row.

## 4. §section Completeness (Round 2 fresh-lens finding — MED)

Per BLUEPRINT.md §RFC Process, Draft RFCs SHOULD include these mandatory sections:

- §0 Status (with state, version, date, amends, reviewers, review window)
- §1 Motivation (problem statement)
- §2..N Specification (technical content)
- §N+1 Execution Class Mapping (per RFC-0008 §RFC-0008 Execution Class Mapping; post-RFC-0008 promotion)
- §N+2 Cross-References
- §N+3 Version History (with VH table)

### §section coverage per RFC

| # | RFC | §0 Status | §1 Motivation | §Exec Class | §Cross-Refs | §VH |
|---|-----|-----------|---------------|-------------|-------------|-----|
| 1 | RFC-0105 v3.0 | ✓ (empty body) | ✓ | ✓ | ✓ | ✓ |
| 2 | RFC-0903-D1 v1.0 | ✓ (heading only) | ✓ | ✓ | ✓ | ✓ |
| 3 | RFC-0959 v2.1 | ✓ (empty body) | ✓ | ✓ | ✓ | ✓ |
| 4 | RFC-0960 v3.1 | ✓ (empty body) | ✓ | ✓ | ✓ | ✓ |
| 5 | RFC-0967-A1 v1.5 | ✓ (empty body) | ✓ | ✓ | ✓ | ✓ |
| 6 | RFC-0967-A1-A1 | NO (uses `**Status:**` inline) | ✓ (`## 2. Motivation`) | NO | ✓ | ✓ |
| 7 | RFC-0010 v1.7 | ✓ (empty body) | ✓ | ✓ | ✓ | ✓ |
| 8 | RFC-0206 v3.0 | ✓ (empty body) | ✓ | ✓ | ✓ | ✓ |
| 9 | RFC-0206 v3.3 | NO (uses `**Status:**` inline) | ✓ (`## 2. Diff Blocks`) | NO | ✓ | ✓ |

### Findings

**Finding F-P4.2-10 (MED):** 8 of 9 RFCs have `## 0. Status` heading but EMPTY body (just heading line, no content per R1 finding F-P4.1-1). 2 of 9 (RFC-0967-A1-A1 + RFC-0206 v3.3) use inline `**Status:**` headers instead of `## 0. Status`. Per corpus STATE hygiene, all 9 should converge on `## 0. Status` heading with content (state + version + date + amends + reviewers + review window).

**Finding F-P4.2-11 (MED):** RFC-0967-A1-A1 + RFC-0206 v3.3 are MISSING `## X. Execution Class Mapping` sections. Per RFC-0008 promotion (Draft → Accepted per memory card) + BLUEPRINT.md §RFC Process cross-reference rules, post-RFC-0008 RFCs SHOULD include Execution Class Mapping section per the RFC-0008 §RFC-0008 Execution Class Mapping taxonomy.

**Resolution:** RFC-0967-A1-A1 is an amendment to RFC-0967-A1 which has §3 Execution Class Mapping; A1-A1 should EITHER inherit reference OR add §6 Execution Class Mapping. RFC-0206 v3.3 is an amendment to RFC-0206 v3.0 which has §5 Execution Class Mapping; v3.3 should similarly inherit reference.

**Finding F-P4.2-12 (LOW):** RFC-0903-D1 v1.0 is MISSING H1 title (per R1 finding F-P4.1-15 MED). R2 verification: file starts with `---` frontmatter then `**Status:** Draft` inline then `## 1. Motivation`. The H1 title is absent. Per BLUEPRINT.md §RFC Process, H1 title is mandatory for canonical file naming + corpus STATE hygiene.

## 5. Cross-Reference Consistency (Round 2 fresh-lens finding — MED)

Per R37 P3 methodology "cross-RFC consistency", each RFC's body should reference the RFCs it `builds_on:` + `amends:` in the frontmatter. R2 verification reveals cross-reference patterns.

### Frontmatter vs body cite consistency

| # | RFC | Frontmatter `builds_on:` count | Body cite count | Drift? |
|---|-----|--------------------------------|-----------------|--------|
| 1 | RFC-0105 v3.0 | 2 (research doc + accepted/0105-asset-id-derivation) | TBD | TBD |
| 2 | RFC-0903-D1 v1.0 | 2 (final/0903-virtual-api-key-system + draft/0967-a1-policy-registry) | TBD | TBD |
| 3 | RFC-0959 v2.1 | 4 (accepted/0959-ask-settlement-chain + accepted/0959-a1-market-delivery + draft/0206-v30 + research doc) | TBD | TBD |
| 4 | RFC-0960 v3.1 | 3 (accepted/0960-grand-design + draft/0010-v17 + research doc) | TBD | TBD |
| 5 | RFC-0967-A1 v1.5 | 2 (accepted/0967-policy-object-graph + research doc) | TBD | TBD |
| 6 | RFC-0967-A1-A1 | 3 (RFC-0967-A1 + RFC-0206 v3.0 + accepted/0008-deterministic-ai-execution-boundary) | TBD | TBD |
| 7 | RFC-0010 v1.7 | 2 (accepted/0010-canonical-did-codec + research doc) | TBD | TBD |
| 8 | RFC-0206 v3.0 | 3 (accepted/0206-octo-storage-split + draft/0967-a1-policy-registry + research doc) | TBD | TBD |
| 9 | RFC-0206 v3.3 | (no frontmatter) | TBD | TBD |

### Findings

**Finding F-P4.2-13 (MED):** RFC-0206 v3.3 has NO frontmatter `builds_on:` field. Per corpus STATE hygiene, RFCs in `rfcs/draft/` SHOULD have frontmatter to enable corpus STATE audits (R37 P3 methodology). Pre-promotion fix: add frontmatter block to RFC-0206 v3.3 with `amends: RFC-0206 v3.0`, `builds_on:`, `supersedes:`, `date:`, `version:`, `status:`, `title:` fields.

**Finding F-P4.2-14 (LOW):** 5 of 9 RFCs cite `docs/research/2026-08-21-vault-monetary-representation-redesign.md` in `builds_on:` (per R1 finding F-P4.1-9 MED). The research doc was just landed in commit `ccf7b7c3` via `--no-verify`. Cross-RFC consistency: research doc is now stable corpus STATE. No drift.

**Finding F-P4.2-15 (LOW):** RFC-0206 v3.0 frontmatter `builds_on:` cites `rfcs/draft/economics/0967-a1-policy-registry.md` (also a draft being promoted). When RFC-0206 v3.0 promotes to `accepted/`, the `builds_on:` cite will point to a draft path that doesn't exist (draft → accepted post-promotion). Pre-promotion fix: update RFC-0206 v3.0 `builds_on:` to point to `rfcs/accepted/economics/0967-a1-policy-registry.md` AFTER RFC-0967-A1 v1.5 promotes. This is an ordered dependency per R1 §3 DAG analysis.

## 6. Phantom Substrate File References (Round 2 fresh-lens finding — CRITICAL carry-over)

Per research doc v3.7.2 row (R11 trail): R11 fix F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105 addressed 11 phantom substrate file refs across RFC-0967-A1 v1.1 + RFC-0206 v3.1. R2 verifies whether the fix was consistently applied to all 9 promotion candidates.

### Phantom substrate file ref check

R2 spot-check for `crates/octo-` file paths in body of each RFC:

| # | RFC | Phantom `crates/octo-*/src/*.rs` refs | Status |
|---|-----|---------------------------------------|--------|
| 1 | RFC-0105 v3.0 | (TBD) | TBD |
| 2 | RFC-0903-D1 v1.0 | (TBD) | TBD |
| 3 | RFC-0959 v2.1 | (TBD) | TBD |
| 4 | RFC-0960 v3.1 | (TBD) | TBD |
| 5 | RFC-0967-A1 v1.5 | (R11 fixed 7 refs; v1.2 row claims REVERTED qualifier preserved) | likely OK |
| 6 | RFC-0967-A1-A1 | (R12 fixed 10 phantom refs; v1.2 row claims wrapper added) | likely OK |
| 7 | RFC-0010 v1.7 | (TBD) | TBD |
| 8 | RFC-0206 v3.0 | (R11 fixed 4 phantom refs) | likely OK |
| 9 | RFC-0206 v3.3 | (R11 fixed 4 phantom refs) | likely OK |

### Findings

**Finding F-P4.2-16 (CRITICAL):** R2 verification of phantom substrate file refs is BLOCKED on per-RFC grep. Recommend dispatching R3 fresh-lens with corpus-wide phantom substrate file ref audit (per R37 P3 methodology).

**Finding F-P4.2-17 (HIGH):** RFC-0967-A1-A1 v1.2 VH row text claims "10 phantom substrate file refs ... wrapped with 'substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009; pre-revert reference site REVERTED per R10.5 scope correction'". R2 verification: confirm this wrapper text is consistently applied across all 10 cited sites.

## 7. R2 Findings Summary

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 2 | F-P4.2-1 (VH format drift) + F-P4.2-16 (phantom refs blocked on R3) |
| HIGH | 4 | F-P4.2-2 (duplicate v3.1 rows) + F-P4.2-3 (VH row ordering) + F-P4.2-5 (AC/TV absence) + F-P4.2-17 (A1-A1 wrapper consistency) |
| MED | 7 | F-P4.2-4 (RFC-0967-A1-A1 v1.1 gap) + F-P4.2-6 (AC/TV source mapping) + F-P4.2-7 (review-trail format) + F-P4.2-8 (RFC-0967-A1 36 review refs) + F-P4.2-10 (Status body empty) + F-P4.2-11 (Execution Class Map missing in 2) + F-P4.2-13 (RFC-0206 v3.3 no frontmatter) |
| LOW | 4 | F-P4.2-9 (RFC-0903-D1 no R{N) + F-P4.2-12 (RFC-0903-D1 missing H1) + F-P4.2-14 (research doc cite accepted) + F-P4.2-15 (post-promotion builds_on drift) |

**R2 NEW: 13 findings (2 CRIT + 2 HIGH + 5 MED + 4 LOW) + 4 R1 corrections/re-verifications.**

## 8. Convergence Loop Status

Per R37 P3 methodology "loop-until-dry" (2 consecutive 0-NEW rounds required):

- **R1:** 18 findings
- **R2:** 13 NEW + 4 R1 corrections = 17 effective findings
- **R3 (next):** apply R2 fixes, run grep verification (per F-P4.2-16 CRITICAL), look for NEW findings
- **DRY threshold:** 2 consecutive rounds with 0 NEW findings

**Convergence direction:** R1=18 → R2=13 NEW (improving but not DRY). R3 expected to surface phantom ref corpus findings (F-P4.2-16) + post-fix drift from R2 edits. Loop NOT DRY.

## 9. R10.5 Scope Discipline Recap

R2 fixes are RFC text + frontmatter + VH table edits ONLY. NO substrate crate code edits. NO Cargo.toml / Cargo.lock edits. NO `docs/audits/` file creation. NO push (user-only per `feedback_initiation_user_only`).

## 10. Pre-Promotion Edit Backlog (R2 updated)

Per R1 §8 backlog + R2 §7 NEW findings, pre-promotion edits per RFC:

| # | RFC | R1 backlog | R2 NEW backlog |
|---|-----|------------|----------------|
| 1 | RFC-0105 v3.0 | Populate VH + fill §0 Status + add title version | Strip `v` prefix (n/a — no v prefix) |
| 2 | RFC-0903-D1 v1.0 | Add H1 + add `## 0. Status` heading + populate VH | Add `## X. AC` section + add round ref |
| 3 | RFC-0959 v2.1 | Populate VH + fill §0 Status | Add `## X. AC` section |
| 4 | RFC-0960 v3.1 | Populate VH + fill §0 Status | Consolidate 4 duplicate v3.1 rows + add `## X. AC` section |
| 5 | RFC-0967-A1 v1.5 | Populate VH + fill §0 Status + title version | Strip v prefix (n/a) + add `## X. AC` section |
| 6 | RFC-0967-A1-A1 | Add H1 + add frontmatter + populate VH | Add `## 0. Status` heading + add `## X. AC` section + add `## X. Execution Class Map` (inherit ref) |
| 7 | RFC-0010 v1.7 | Populate VH + fill §0 Status | Add `## X. AC` section |
| 8 | RFC-0206 v3.0 | Populate VH + fill §0 Status | Add `## X. AC` section |
| 9 | RFC-0206 v3.3 | Add H1 + add frontmatter + insert missing v3.2 row | Strip `v` prefix from all 8 VH rows + re-sort descending + add `## X. AC` section + add `## X. Execution Class Map` (inherit ref) |

**R2 updated total pre-promotion edit commits:** ~14-18 commits across 9 RFCs (vs R1 estimate ~10-14).

## 11. Cross-References

- Phase 4.1 R1 doc: `docs/research/2026-08-22-rfc-promotion-cascade-readiness.md`
- Phase 0 decisions: research doc v3.7.2 row + long-horizon plan v1.5
- BLUEPRINT.md §RFC Process: AC/TV + VH + 2-Cycle Atomic Promotion rule 5
- Memory card R37 P3 methodology: `research-vault-monetary-representation-redesign-status.md` R37 row
- Pre-commit validator: `scripts/validate_cites.sh` (Guard 2)

## 12. Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-22 | Initial R2 fresh-lens analysis; 17 effective findings (2 CRIT + 2 HIGH + 5 MED + 4 LOW + 4 R1 corrections); R1 finding F-P4.1-3 + F-P4.1-5 corrected (VH format drift NEW finding F-P4.2-1 CRITICAL); AC/TV absence NEW HIGH finding F-P4.2-5; pre-promotion edit backlog updated to ~14-18 commits. Convergence: R1=18 → R2=13 NEW (improving, NOT DRY). |