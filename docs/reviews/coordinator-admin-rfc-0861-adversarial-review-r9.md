# RFC-0861 + Mission 0861 — Adversarial Review, Round 9

**Branch:** `next` (at commit 2a5a674)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.8) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24h)
**Scope:** Re-verify every concrete line number cited in the
RFC and mission against the actual code at this commit.

## Method

Opened each cited file at HEAD (`2a5a674`) and `grep`'d for the
specific symbols / line numbers cited in the spec. Three drift
findings.

## Findings (3 LOW)

#### N70 [LOW] — Key Files row claims `IrcAdapter` struct at "~line 225", actual is line 208

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:335`.

**Bug:** The Key Files row says:

> `is_authenticated: AtomicBool` field on `IrcAdapter` (struct **~line 225**, next to `out_tx`/`shutdown_tx`)

Actual `pub struct IrcAdapter` declaration is at
`crates/octo-adapter-irc/src/lib.rs:208`. The field
declarations for `out_tx` and `shutdown_tx` are at lines 222
and 232 respectively — so the "~line 225" cite puts the
implementer in the wrong ballpark of where the new field
declaration will go (it should go INSIDE the struct, at ~line
225 — but the struct opens at 208). The "~" prefix makes
this cosmetic, not load-bearing, but it's misleading.

**Fix:** change to "struct at line 208; new field goes between
the existing declarations at ~line 226, near `out_tx`/`shutdown_tx`".

#### N71 [LOW] — Mission Phase 2 H2 says `leave_group_str` "comment block 1763-1764", actual is 1763-1767

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:45`.

**Bug:** The bullet says:

> `leave_group_str` precedent at `adapter.rs:1769` (inherent; **comment block 1763-1764**, trait impl 1467-1479)

The doc comment is 5 lines (1763-1767):
```
1763: /// Internal alias: `impl WhatsAppWebAdapter::leave_group` and the
1764: /// `CoordinatorAdmin::leave_group` trait method have the same name
1765: /// (and the trait method wins resolution). We re-bind the public
1766: /// `String`-returning method to a distinct local name so the trait
1767: /// impl above can call it.
```

An implementer reading "comment block 1763-1764" will miss the
key sentence ("We re-bind the public `String`-returning method
to a distinct local name") that's at lines 1765-1767 — which
is exactly the rationale for the H2 rename they're about to do.

**Fix:** change "comment block 1763-1764" to "comment block
1763-1767 (rationale: re-bind the public `String`-returning
method to a distinct local name so the trait impl can call
it)".

#### N72 [LOW] — RFC §2 H2 says `leave_group_str` "comment block at lines 1763-1764", actual is 1763-1767

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:161-162`.

**Bug:** Same as N71. The §2 H2 paragraph says:

> ... method at `crates/octo-adapter-whatsapp/src/adapter.rs:1769`, **comment block at lines 1763-1764**, trait impl at lines 1467-1479 ...

**Fix:** change "comment block at lines 1763-1764" to "comment
block at lines 1763-1767 (rationale: re-bind the public
`String`-returning method to a distinct local name so the trait
impl can call it)".

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N70 | LOW | RFC Key Files row | IrcAdapter struct cited as ~225, actual is 208 |
| N71 | LOW | Mission Phase 2 H2 | leave_group_str comment cited as 1763-1764, actual is 1763-1767 |
| N72 | LOW | RFC §2 H2 | leave_group_str comment cited as 1763-1764, actual is 1763-1767 |

3 findings, all LOW. All three are line-number drift — the
specs are otherwise correct, just pointing readers at the wrong
spot in the file.

## Cross-references

- R24a–R24h reviews: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r{1..8}.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`