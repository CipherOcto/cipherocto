# RFC-0861 + Mission 0861 — Adversarial Review, Round 10

**Branch:** `next` (at commit 96933bb)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.9) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24i)
**Scope:** Final comprehensive sweep. Look for residual drift after
the R24i line-number fixes. Cross-check Phase plans, Appendix A
mapping table, §X H/M item coverage, and section cross-references.

## Method

Read each Phase plan against the corresponding Phase acceptance
in the mission. Re-grep all `~line`, `line [0-9]+`, `lib.rs:`,
`adapter.rs:`, `§X` references. Verify Appendix A entries point
to sections that actually contain the finding's spec.

## Findings (2 — 1 MEDIUM, 1 LOW)

#### N73 [MEDIUM] — RFC Phase 2 plan lists H1 BEFORE H2; contradicts H2's "do this FIRST" instruction AND the Mission (post-R24f fix)

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:316-317`.

**Bug:** The RFC Phase 2 plan:

```
- Implement `join_by_invite` via `client.groups().join_with_invite_code(...)` per §1 (H1)
- Rename inherent `create_group` to `create_group_str` per §3 (H2) — **do this FIRST**, since M5's edit ...
```

The second bullet (H2) explicitly says "do this FIRST", but the
H1 bullet is listed before it. This is self-contradictory.

It also contradicts the Mission Phase 2 acceptance (post-R24f N62
fix), which correctly puts H2 first (mission line 45), then H1
(line 46). The Mission explains WHY H2 must be first: M5's edit
lands on the renamed function.

An implementer following the RFC plan will (correctly) read H2's
"do this FIRST" and reorder the work, but the plan order as
written is wrong and creates confusion.

**Fix:** move the H2 bullet to line 316 (first), and H1 to line 317
(second). The H2 bullet keeps its "do this FIRST" annotation;
the H1 bullet keeps its §1 reference. This matches the Mission.

#### N74 [LOW] — Phase 2 plan line 316 cites §1 for H1; primary impl spec is in §3

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:316`.

**Bug:** The Phase 2 plan says:

> Implement `join_by_invite` via `client.groups().join_with_invite_code(...)` per **§1** (H1)

The §1 table (line 99-102) has H1's capability-report bit honesty
rule, and the per-adapter fix row for `can_join_by_invite`. But
the actual `join_with_invite_code(...)` call instruction and the
`JoinGroupResult` variant mapping (`Joined(Jid)` vs
`PendingApproval(Jid)`) is in **§3 H1** (line 132-156). The §1
table even cross-references this: "(see §3 H1 for the variant
mapping)".

So `per §1 (H1)` is misleading — the implementer looking for the
`JoinGroupResult` enum spec would have to also read §3 anyway.
The §3 cite is more accurate as the primary spec location.

**Fix:** change `per §1 (H1)` to `per §1+§3 (H1; primary impl spec
in §3 H1)`. (Or simply `per §3 (H1)` — both are defensible.)

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N73 | MEDIUM | RFC Phase 2 plan | H1 listed before H2, contradicts H2 "do this FIRST" |
| N74 | LOW | RFC Phase 2 plan | H1 cite says §1, primary spec is in §3 |

N73 is MEDIUM because it can mislead an implementer into doing
work in the wrong order (and would force a Phase 2 commit
re-order). N74 is LOW because it's a cite-precision issue, not
behavior-changing.

## Cross-references

- R24a–R24i reviews: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r{1..9}.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`