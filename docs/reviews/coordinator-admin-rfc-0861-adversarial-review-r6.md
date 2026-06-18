# RFC-0861 + Mission 0861 — Adversarial Review, Round 6

**Branch:** `next` (at commit 240770b)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.5) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24e)
**Scope:** Re-audit after the 4 R24e fixes. Look for drift between
RFC and mission that the previous rounds didn't catch.

## Method

Re-read both files. Diffed the Phase 2 plan and Acceptance Criteria
sections between the RFC and the mission (the most likely place for
drift after multiple round edits). Also verified the Version
History table is now well-formed.

## Findings (3)

#### N61 [MEDIUM] — Version History sequence skips 1.3

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:362-370`.

**Bug:** The Version History table now reads:

```
| 1.0 | 2026-06-18 | Initial. ... |
| 1.1 | 2026-06-18 | R24a fixes: ... |
| 1.2 | 2026-06-18 | R24b fixes: ... |
| 1.4 | 2026-06-18 | R24d fixes: ... |
| 1.5 | 2026-06-18 | R24e fixes: ... |
```

The 1.3 row is missing. The 1.5 row's text even says "Version
History 1.4 row added (was claimed in R24d commit message but never
written to file)" — but doesn't acknowledge that 1.3 was lost in
the R24c edit (R24c replaced the existing 1.2 row with a 1.3 row,
so the post-R24c table was actually 1.0/1.1/1.3; then R24d
recovered the 1.2 row but the 1.3 row was lost in the process).

The 1.4 row text does mention "Version History 1.2 row recovered
(was overwritten in R24c)" but doesn't reconstruct the 1.3 row
that R24c itself added. So R24c's substantive fixes (8 LOW accuracy
gaps) are partially documented in the 1.4 row (which says it
"recovered the 1.2 row") but the actual R24c changes (N39 line
fixes, N40 wacore line fix, N41 Phase 1 title fix, N42 Ordering
fix, N44 IrcConfig::validate line fix, N45 config.rs path fix,
N46 M8 row extension, N47 M5 cross-ref) have no dedicated row.

**Fix:** insert a 1.3 row between 1.2 and 1.4 with the R24c content
(this was already documented in commit `c891478`'s message and in
`docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r3.md`).
The sequence becomes 1.0 / 1.1 / 1.2 / 1.3 / 1.4 / 1.5 — complete.

#### N62 [MEDIUM] — Mission Phase 2 ordering doesn't match RFC Phase 2 plan

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:43-53`.

**Bug:** The RFC's Phase 2 plan (lines 304-311, after R24d N56 fix)
puts H2 (`create_group_str` rename) **first** because M5's edit
lands on the renamed function. The mission's Phase 2 acceptance
criteria (lines 45-52) put H1 first, then H1 second, then H2
third. This contradicts the RFC.

The RFC §Phase 2 says:

> - Implement join_by_invite via `client.groups().join_with_invite_code(...)` per §1 (H1)
> - **Rename inherent create_group to create_group_str per §3 (H2) — do this FIRST**, since M5's edit (`.ok()` → `tracing::debug!`) lands on the renamed `create_group_str`. M1, M11, M16, and H1 are separate methods and don't depend on the H2 rename.
> - Add set_ephemeral overflow error per §3 (M1) ...

But the mission lists (in order):

> - [ ] `join_by_invite` impl ... (H1)
> - [ ] `capabilities().can_join_by_invite` remains `true` ... (H1)
> - [ ] Inherent `create_group` renamed to `create_group_str` ... (H2)  ← H2 third, not first
> - [ ] `set_ephemeral` ... (M1)
> - [ ] `get_group_metadata` and `get_invite_link` errors in `create_group_str` ... (M5)
> - [ ] `list_own_groups` ... (M11)
> - [ ] `WhatsAppConfig::validate()` ... (M16)
> - [ ] `group_to_jid` ... (M16)

A mission implementer reading the mission first (which is normal —
missions are actionable) will do H1 → H2 → M1 → M5, doing M5 BEFORE
the H2 rename has happened, which means M5 will silently land on
the pre-rename `create_group` instead of `create_group_str`. The
mission's M5 line says "in `create_group_str` (post-H2 rename)"
which makes sense only AFTER H2 has been done.

**Fix:** reorder the mission Phase 2 to put H2 before M5 (the
R24d N56 fix only patched the RFC; the mission's order is the
operational instruction and should match).

Reorder to:

> - [ ] Inherent `create_group` renamed to `create_group_str` ... (H2) — **do this FIRST**; M5 lands on the renamed function
> - [ ] `join_by_invite` impl ... (H1)
> - [ ] `capabilities().can_join_by_invite` remains `true` ... (H1)
> - [ ] `set_ephemeral` returns `ApiError ...` (M1)
> - [ ] `get_group_metadata` and `get_invite_link` errors in `create_group_str` (post-H2 rename; was `create_group` pre-H2) log at `tracing::debug!` and continue (M5)
> - [ ] `list_own_groups` builds a `HashSet<String>` ... (M11)
> - [ ] `WhatsAppConfig::validate()` ... (M16)
> - [ ] `group_to_jid` ... (M16)

#### N63 [LOW] — Mission Phase 2 bullets for H1 are split into two separate checkboxes

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:45-46`.

**Bug:** Phase 2 has two separate H1 checkboxes (lines 45 and 46)
for "join_by_invite impl" and "capabilities().can_join_by_invite
remains true". They go together — the bit is set true BECAUSE the
impl is real. Splitting them into separate checkboxes implies
they're independent, which they aren't (the second is a
consequence of the first). If you do (1) without (2), you have
inconsistent state.

**Fix:** merge into one checkbox:

> - [ ] `join_by_invite` impl calls `client.groups().join_with_invite_code(...)` and returns a proper `GroupHandle`; `capabilities().can_join_by_invite` remains `true` (matches the new impl) (H1)

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N61 | MEDIUM | RFC Version History | 1.3 row still missing (sequence: 1.0/1.1/1.2/1.4/1.5) |
| N62 | MEDIUM | Mission Phase 2 order | H2 not first despite RFC saying it must be |
| N63 | LOW | Mission Phase 2 H1 bullets | Two separate checkboxes for what should be one coupled acceptance criterion |

3 findings: 2 MEDIUM, 1 LOW.

## Cross-references

- R24a review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`
- R24b review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`
- R24c review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r3.md`
- R24d review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r4.md`
- R24e review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r5.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`