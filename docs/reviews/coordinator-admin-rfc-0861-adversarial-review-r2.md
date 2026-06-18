# RFC-0861 + Mission 0861 — Adversarial Review, Round 2

**Branch:** `next`
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.1) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24a)
**Scope:** Re-audit after the 9 R24a fixes. Specifically: did the R24a
fixes introduce new inconsistencies? Did R24a miss anything I could
spot on a fresh read?

## Method

Read both files end-to-end. Cross-checked every line number, code
identifier, and architectural claim against the actual code on
`next`. Looked for:

- Numerical/identifier drift from R24a fixes
- New content that contradicts unchanged content
- Vague or misleading implementation guidance

## Findings (8)

### RFC-0861 (6)

#### N31 [HIGH] — Appendix A missing H6; M3 in stale Phase 4

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:380-398`.

**Bug 1 — missing row:** The Appendix A table has 16 rows, not 17.
H6 (WhatsApp `add_member` partial-success) is in the RFC body (§3
H6, `AddMemberOutput` struct) but not in the table. The table even
self-contradicts: it says "17 entries" but contains 16 rows. The
canonical RFC for 17 findings needs all 17 in the canonical mapping.

**Bug 2 — stale phase:** The M3 row says `Phase 4 (blocked on C1)`,
but Phase 4 was deleted in R24a. M3 is now in Phase 3, fully
unblocked. This row contradicts both the §7 body (which says "M3 is
unblocked") and the Implementation Phases section (which lists M3
under Phase 3).

**Fix:** add the H6 row with `RFC § | §3 | HIGH | WhatsApp | 2`; fix
M3's phase column to `3`.

#### N33 [MEDIUM] — M7's "the same channel can carry reply codes" is false

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:222-225`.

**Bug:** The M7 spec says "see R1 H5 fix for the `shutdown_tx`
infrastructure; the same channel can carry reply codes". But
`shutdown_tx` (per `crates/octo-adapter-irc/src/lib.rs:232`) is
`Mutex<Option<tokio::sync::watch::Sender<bool>>>` — a watch channel
for `bool` (the shutdown signal). It cannot carry reply codes.

**Fix:** clarify that a NEW channel/buffer is needed. Recommended
shape: `pending_replies: Mutex<HashMap<CommandId,
oneshot::Sender<NumericResult>>>` on the listener side, keyed by a
per-command nonce that the `add_member` impl inserts on send and
removes on receive.

#### N36 [LOW] — Rationale "all 11 fixes" → 17

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:349`.

**Bug:** "implement all 11 fixes in one mission" — but the RFC
specifies 17 findings (3 HIGH + 14 MEDIUM). The 11 was a stale
number from the R5 review's bare deferral count; the RFC and
mission both corrected this to 17, but the Rationale prose didn't
get the same correction.

**Fix:** change "11 fixes" to "17 fixes".

#### N37 [LOW] — Phase 1 "(no behavior change)" parenthetical is misleading

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:33`
(RFC doesn't have this parenthetical, only mission does).

**Bug:** Phase 1 is titled "Trait surface (no behavior change)" but
the Phase 1 acceptance criteria include:

- New `try_new` ctors (new API surface)
- `AddMemberOutput` returned from `add_member` (signature change)
- `initial_admins_promoted: bool` added to `GroupHandle` (new field
  on the public struct, defaults via `#[serde(default)]` but callers
  reading existing serializations get `false`)
- `list_own_groups_with_invites` (new trait method)

These are public API additions. The spirit is correct — no existing
caller breaks — but the parenthetical is too strong.

**Fix:** rename to "Trait surface (additive; no breakage for
existing callers)" or "Trait surface (additive API)".

#### N38 [LOW] — Future Work F1 "matrix-sdk" is a client lib, not an adapter

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:327-329`.

**Bug:** "Add `CoordinatorAdmin` impl for Telegram TDLib, Matrix,
matrix-sdk" — three items, but `matrix-sdk` is the Rust client
library for Matrix, not a separate adapter. The list reads as if
Matrix and matrix-sdk are different adapters.

**Fix:** "Add `CoordinatorAdmin` impl for Telegram TDLib and
Matrix (via `matrix-sdk`)." Drop one of the three.

### Mission (3)

#### N32 [MEDIUM] — M7 says "Use the `out_tx` channel already in place"

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:95-99`.

**Bug:** The M7 implementation note says "Use the `out_tx` channel
already in place (R23d H4 fix) to correlate the command with the
response numeric." But `out_tx` (per
`crates/octo-adapter-irc/src/lib.rs:222`) is
`Mutex<Option<mpsc::Sender<String>>>` — the mpsc::Sender for
OUTBOUND IRC lines to the server. It carries lines out, not
response codes back. The mission is pointing the implementer at the
wrong channel.

The R23d H4 fix it references actually created/used a *different*
mechanism — likely the `out_tx`/`out_rx` pair (line 346). But neither
half of that pair can carry reply codes back to the caller.

**Fix:** replace with the same N33 guidance: "Add a new
`pending_replies: Mutex<HashMap<CommandId,
oneshot::Sender<NumericResult>>>` on the listener (or in
`IrcAdapter`); have `add_member` register a oneshot before sending
the INVITE; have the listener resolve it when the matching
`ERR_CHANOPRIVSNEEDED` arrives. The existing `out_tx` is for
outbound lines only and cannot carry reply codes."

#### N34 [LOW] — Stale line numbers in two more spots

**Where:**
- `missions/open/0861-coordinator-admin-trait-refinements.md:47`
- `missions/open/0861-coordinator-admin-trait-refinements.md:127`

**Bug:** R24a's N24 fix corrected line numbers in the RFC body and
the Phase 3 bullets, but missed two `leave_group_str` precedent
references in the mission:

- Line 47: "leave_group precedent at `adapter.rs:1767-1796`"
- Line 127: "leave_group_str rename at
  `crates/octo-adapter-whatsapp/src/adapter.rs:1767-1796`"

Both should say line 1769 (just the inherent method; the comment
block is 1763-1764 and the trait impl is 1467-1479, but the
`leave_group_str` method body itself is at line 1769).

**Fix:** update both to "leave_group_str` inherent at
`adapter.rs:1769` (comment block 1763-1764, trait impl 1467-1479)".

#### N35 [LOW] — "Phases 1, 6" typo

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:68`.

**Bug:** "trait surface (Phases 1, 6)" — there is no Phase 6. The
"6" is a leftover from before R24a (when there were 7 spec
sections, with §6 being the trait doc section that was folded into
Phase 1 work). Should be just "Phase 1".

**Fix:** "trait surface (Phase 1)".

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N31 | HIGH | RFC Appendix A | missing H6 row; M3 in stale Phase 4 |
| N33 | MEDIUM | RFC §4 M7 | false claim that `shutdown_tx` can carry reply codes |
| N32 | MEDIUM | Mission Implementation Notes | wrong channel name (`out_tx`) for M7 correlation |
| N36 | LOW | RFC Rationale | "11 fixes" → 17 |
| N37 | LOW | Mission Phase 1 title | misleading "(no behavior change)" |
| N38 | LOW | RFC Future Work F1 | "matrix-sdk" listed as separate adapter |
| N34 | LOW | Mission 2 spots | stale line numbers (1767-1796) |
| N35 | LOW | Mission Location | "Phases 1, 6" typo |

8 findings: 1 HIGH, 2 MEDIUM, 5 LOW.

## Cross-references

- R24a review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`