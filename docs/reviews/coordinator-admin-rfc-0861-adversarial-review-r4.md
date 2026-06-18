# RFC-0861 + Mission 0861 — Adversarial Review, Round 4

**Branch:** `next` (at commit c891478)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.3) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24c)
**Scope:** Re-audit after the 8 R24c fixes. Look for any remaining
substantive or accuracy issues, including ones that survived all
three prior rounds.

## Method

Re-read both files end-to-end. Cross-checked against actual code.
Focused this round on:

- Substantive internal contradictions in the spec
- Doc-vs-type mismatches that would break the implementer
- Lost history in version-control tables

## Findings (5)

#### N48 [MEDIUM] — Version History 1.2 row was overwritten, not added

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:361-367`.

**Bug:** R24c's edit replaced the existing 1.2 row with the new 1.3
row instead of *adding* a 1.3 row below the existing 1.2 row. Result:
the Version History now reads

```
| 1.0 | 2026-06-18 | Initial... |
| 1.1 | 2026-06-18 | R24a fixes: ... |
| 1.3 | 2026-06-18 | R24c fixes: ... |
```

The 1.2 row (which documented R24b's 8 findings: N31-N38) is gone.
This is a loss of project history. Anyone reading the version
history won't see what R24b fixed or that it happened at all.

**Fix:** re-insert the 1.2 row between 1.1 and 1.3. The R24b row
text is recoverable from commit `b3ca322` or
`docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`.

#### N55 [MEDIUM] — AddMemberOutput.promoted doc contradicts its declared type

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:165-178`
(and inherited in mission Phase 1 line 37).

**Bug:** the spec declares:

```rust
pub struct AddMemberOutput {
    /// True if the platform confirmed the add.
    pub added: bool,
    /// The result of the optional promote (None if `is_admin`
    /// was false at the call site). `Err` means the add
    /// succeeded but the promote failed.
    pub promoted: Result<(), PlatformAdapterError>,
}
```

But `promoted` is `Result<(), PlatformAdapterError>`, which can be
`Ok(())` or `Err(...)` — it CANNOT be `None`. The doc says "None if
`is_admin` was false at the call site" which is impossible for the
declared type.

The implementer has two choices:
1. Follow the doc and use `Option<Result<(), PlatformAdapterError>>`
2. Follow the type and use `Result<(), PlatformAdapterError>`
   (reinterpreting "None" as "Ok if no promote was attempted")

The spec has to pick one. The natural read of "optional promote" +
"None if not attempted" is `Option<Result<(), PlatformAdapterError>>`.

**Fix:** change the type to `Option<Result<(), PlatformAdapterError>>`
and clarify the doc:

```rust
pub struct AddMemberOutput {
    /// True if the platform confirmed the add.
    pub added: bool,
    /// The result of the optional promote. `None` if `is_admin`
    /// was false at the call site (no promote was attempted).
    /// `Some(Ok(()))` if the promote succeeded.
    /// `Some(Err(e))` if the add succeeded but the promote failed.
    pub promoted: Option<Result<(), PlatformAdapterError>>,
}
```

Mission Phase 1 line 37 also needs to be updated to match.

#### N49 [LOW] — Phase 2 M1 line says "the renamed `create_group_str`'s sibling method"

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:308`.

**Bug:** "Add `set_ephemeral` overflow error per §3 (M1) — in the
renamed `create_group_str`'s sibling method, not in `create_group`
itself". But `set_ephemeral` is a TRAIT method
(`CoordinatorAdmin::set_ephemeral`), not a "sibling" of the
inherent `create_group_str`. They're on different sides of the
trait/inherent split. The "sibling" framing is misleading.

**Fix:** "Add `set_ephemeral` overflow error per §3 (M1) — in the
`set_ephemeral` TRAIT impl (not the inherent `create_group_str`;
they're separate methods)".

#### N50 [LOW] — Mission Phase 2 M5 still says "in `create_group`"

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:49`.

**Bug:** "get_group_metadata and get_invite_link errors in
`create_group` log at `tracing::debug!` and continue (M5)". After
H2 renames `create_group` → `create_group_str` (and Phase 2 plan
reorders H2 first per R24c N47), M5's edits land in `create_group_str`.

**Fix:** "in `create_group_str` (the post-H2 name)".

#### N56 [LOW] — Phase 2 plan reuses ambiguous "Phase 2 edits land on the renamed function" phrasing

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:307`.

**Bug:** "Rename inherent `create_group` to `create_group_str` per §3
(H2) — **do this FIRST**, all other Phase 2 edits land on the
renamed function". But not all Phase 2 edits land on
`create_group_str`: M1 (set_ephemeral), M11 (list_own_groups), M16
(WhatsAppConfig::validate), H1 (join_by_invite), and M5 (mostly
create_group_str but also reaches into get_group_metadata /
get_invite_link helpers). The claim "all other Phase 2 edits" is
too broad.

**Fix:** "Rename inherent `create_group` to `create_group_str` per
§3 (H2) — **do this FIRST**, since M5's edit (`.ok()` →
`tracing::debug!`) lands on the renamed `create_group_str`. M1
(`set_ephemeral`), M11 (`list_own_groups`), M16
(`WhatsAppConfig::validate`), and H1 (`join_by_invite`) are
separate methods and don't depend on the H2 rename."

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N48 | MEDIUM | RFC Version History | 1.2 row lost in R24c edit (was overwritten, not appended) |
| N55 | MEDIUM | RFC §3 H6 + Mission Phase 1 | `AddMemberOutput.promoted` doc says "None if X" but type is `Result`, not `Option<Result>` |
| N49 | LOW | RFC Phase 2 plan | "sibling method" framing wrong for trait/inherent split |
| N50 | LOW | Mission Phase 2 | M5 says "in create_group" but H2 renamed it |
| N56 | LOW | RFC Phase 2 plan | "all other Phase 2 edits" is too broad |

5 findings: 2 MEDIUM, 3 LOW.

## Cross-references

- R24a review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`
- R24b review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`
- R24c review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r3.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`