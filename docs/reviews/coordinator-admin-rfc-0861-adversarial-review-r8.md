# RFC-0861 + Mission 0861 — Adversarial Review, Round 8

**Branch:** `next` (at commit dbeb455)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.7) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24g)
**Scope:** Re-audit after the 3 R24g fixes. Specifically check
that the §4 M8 "clear in BOTH mark_disconnected AND shutdown"
fix (N65) was propagated to the Phase 3 plan, the Key Files
row, and the mission's Phase 3 acceptance criterion.

## Method

Grep'd for stale "clear on disconnect" / "next to the existing
shutdown_tx clear" references. Found 3 sites that didn't pick up
the N65 fix.

## Findings (3)

#### N67 [MEDIUM] — Phase 3 plan still says "clear on disconnect"

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:324`.

**Bug:** The Phase 3 plan line:

> Add `is_authenticated: AtomicBool` and `health_check` upgrade per §4 (M8) — set on 376/422, **clear on disconnect**

The §4 M8 body was fixed in R24g (N65) to say "clear in BOTH
`mark_disconnected` AND `shutdown`". The Phase 3 plan summary
wasn't updated, so an implementer following the Phase 3 plan
will add the clear in only ONE place — and probably the wrong one
(`mark_disconnected`, since that's the more obvious "disconnect"
method).

**Fix:** change to "set on 376/422, clear in BOTH
`mark_disconnected` (transient drop, `lib.rs:377`) AND `shutdown`
(full teardown, `lib.rs:1086`) per §4 M8".

#### N68 [MEDIUM] — Key Files row still says "CLEAR it in `disconnect` next to the existing `shutdown_tx` clear"

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:335`.

**Bug:** Same as N67. The Key Files row for the IRC lib (after
the R24c N46 expansion) says:

> ... CLEAR it in `disconnect` next to the existing `shutdown_tx` clear (M8)

But per N65, the clear needs to land in TWO places
(`mark_disconnected` AND `shutdown`), not just the one
"next to the existing `shutdown_tx` clear" site (which is in
`shutdown`, not `mark_disconnected`).

**Fix:** replace "CLEAR it in `disconnect` next to the existing
`shutdown_tx` clear" with "CLEAR it in BOTH `mark_disconnected`
(transient drop, at `lib.rs:377`, alongside the `connected =
false` / `out_tx = None` lines) AND `shutdown` (full teardown,
at `lib.rs:1116`, alongside the existing `out_tx = None` and
`shutdown_tx.take()` lines)".

#### N69 [MEDIUM] — Mission Phase 3 M8 acceptance still says "cleared on disconnect"

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:57`.

**Bug:** Same as N67/N68. The mission's Phase 3 acceptance:

> `IrcAdapter` has `is_authenticated: AtomicBool` field, set on the first RPL_ENDOFMOTD (376) or ERR_NOMOTD (422), **cleared on disconnect** (M8) — using 376/422 (not 001/RPL_WELCOME) because the listener has no 001 parsing; 376/422 is the canonical "post-handshake" signal

Needs the N65 expansion. A mission implementer reading this will
clear the field in only one place.

**Fix:** change "cleared on disconnect (M8)" to "cleared in BOTH
`mark_disconnected` (transient drop, at `lib.rs:377`) AND
`shutdown` (full teardown, at `lib.rs:1086`) per RFC §4 M8 (N65 fix)".

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N67 | MEDIUM | RFC Phase 3 plan | still says "clear on disconnect" |
| N68 | MEDIUM | RFC Key Files row | still says "CLEAR it in `disconnect` next to the existing `shutdown_tx` clear" |
| N69 | MEDIUM | Mission Phase 3 M8 acceptance | still says "cleared on disconnect" |

3 findings, all MEDIUM. All three are downstream propagations of
the R24g N65 fix that didn't reach the summary/table/acceptance
sites.

## Cross-references

- R24a–R24g reviews: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r{1..7}.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`