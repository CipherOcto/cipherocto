# RFC-0861 + Mission 0861 — Adversarial Review, Round 5

**Branch:** `next` (at commit 67e7ad7)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.3) + `missions/open/0861-coordinator-admin-trait-refinements.md` + cross-checked downstream `docs/reviews/coordinator-admin-impl-adversarial-review-r5.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24d)
**Scope:** Re-audit after the 5 R24d fixes. Look for any
remaining drift, including downstream docs (R5 closure summary
that the RFC cross-references).

## Method

Re-read both files end-to-end. Cross-checked downstream docs that
the RFC and mission cross-reference. Looked for:

- Stale references in docs the RFC/mission link to
- Doc-vs-code drift that survived 4 prior passes
- Self-contradictions in version history

## Findings (4)

#### N57 [MEDIUM] — R5 closure summary table is stale on M3 phase

**Where:** `docs/reviews/coordinator-admin-impl-adversarial-review-r5.md:67`
(cross-referenced from RFC line 385: "R5 closure summary; references
this RFC").

**Bug:** The R5 closure summary's "Still unaddressed from R1"
table at line 67 says M3 is in "**4 (blocked on C1)**". After
R24a (which the RFC documents), M3 was moved to **Phase 3,
unblocked since R23d C1 is fixed** (the RFC was even patched to
fix this in the RFC's own Appendix A and Implementation Phases,
but the R5 doc wasn't updated).

The R5 doc is the source the RFC links under "Related Review
Docs". A reader who clicks through will see the stale "Phase 4
(blocked on C1)" claim and may believe M3 is still blocked,
contradicting the RFC.

**Fix:** change the R5 table's M3 row from "4 (blocked on C1)" to
"3 (unblocked since R23d C1)" — or, more accurately, replace the
entire "Mission phase" column with a footnote pointing readers
to RFC-0861 Appendix A as the canonical, current mapping
(RFC-0861 has been updated post-R24a/R24b/R24c/R24d).

#### N58 [LOW] — Version History ends at 1.3; commit 67e7ad7 claimed to add 1.4

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:361-369`.

**Bug:** The R24d commit message (commit `67e7ad7`) said:

> **Version History:** kept 1.0 → 1.1 → 1.2 → 1.3; added 1.4 row for R24d.

But the actual file's Version History table ends at 1.3; no 1.4
row exists. The commit message described a change that didn't
happen. This is now an internal inconsistency: the commit log
claims a 1.4 row, the file doesn't have one.

**Fix:** add a 1.4 row documenting the R24d fixes (Version History
should be the canonical change log, so the 1.4 entry should
describe what R24d actually fixed: AddMemberOutput.promoted →
`Option<Result>`; Version History 1.2 row recovered; mission M5 →
create_group_str; Phase 2 plan H2 wording tightened; M1 sibling
phrasing corrected).

#### N59 [LOW] — R5 review says "16 of them" but lists 16 items — actually correct, verify the rest

**Where:** `docs/reviews/coordinator-admin-impl-adversarial-review-r5.md:80-82`.

**Bug:** "(R5 originally listed 16 of these; M2 was missed in the
R5 enumeration and is the same kind of trait-level input-validation
fix as M15/M16, so it was rolled into the same RFC.)" — this is
factually correct (R5's table has 16 rows; M2 was the omission).
But after R24a's fixes, the R5 table still lists M2 as the only
new addition — meaning readers might think the RFC and mission
spec'd only the 16 R5-listed items + M2, when in fact RFC-0861
covers all 17 with Appendix A as the canonical mapping.

The footnote is fine, but the column "RFC §" / "Mission phase"
should ideally be replaced with a pointer to RFC-0861 Appendix A
(which is now authoritative and updated). N57 captures the
substantive bug; this is a wording clarification.

**Fix:** add to the footnote: "Note: the RFC § / Mission phase
columns above are R5-time snapshots. The canonical, current
mapping is in RFC-0861 Appendix A, which has been updated through
R24d to reflect that M3 is unblocked and in Phase 3."

#### N60 [LOW] — Phase 1 H6 acceptance criterion should require a discriminator test

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:37`.

**Bug:** Phase 1 line 37 says "AddMemberOutput { added: bool,
promoted: Option<Result<(), PlatformAdapterError>> } defined and
the trait add_member returns it (H6)". This only requires
struct-existence + signature change. After the R24d N55 fix,
the `Option<Result<...>>` type is non-trivial — the implementer
should add a unit test that exercises each variant:
- `promoted: None` (is_admin was false at call site)
- `promoted: Some(Ok(()))` (promote succeeded)
- `promoted: Some(Err(e))` (promote failed despite add succeeding)

Without this test, the `Option` vs `Result` distinction is easy to
get wrong (e.g. always returning `Some(Ok(()))` would pass struct
existence).

**Fix:** add to Phase 1: "Unit test in
`crates/octo-network/src/dot/adapters/coordinator_admin.rs` test
module covering all three `promoted` variants (None / Some(Ok(())) /
Some(Err(e))); see R1 review H6 for context."

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N57 | MEDIUM | R5 closure review (line 67) | M3 still shown as "Phase 4 (blocked on C1)" — stale, post-R24a |
| N58 | LOW | RFC Version History | 1.4 row claimed in commit 67e7ad7 message but never added |
| N59 | LOW | R5 closure review footnote | "RFC § / Mission phase" columns are R5-time snapshots; should point to RFC-0861 Appendix A |
| N60 | LOW | Mission Phase 1 H6 line | Should require a discriminator test for the new `Option<Result<>>` type |

4 findings: 1 MEDIUM, 3 LOW.

## Cross-references

- R24a review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`
- R24b review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`
- R24c review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r3.md`
- R24d review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r4.md`
- R5 closure summary (downstream): `docs/reviews/coordinator-admin-impl-adversarial-review-r5.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`