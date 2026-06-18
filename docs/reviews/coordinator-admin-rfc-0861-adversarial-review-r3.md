# RFC-0861 + Mission 0861 — Adversarial Review, Round 3

**Branch:** `next` (at commit b3ca322)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.2) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24b)
**Scope:** Re-audit after the 8 R24b fixes. Look for newly-introduced
inconsistencies, stale numbers, and minor accuracy gaps that survived
the first two passes.

## Method

Read both files end-to-end. Cross-checked every line number and
identifier against the actual code on `next`. Looked at:

- Off-by-one line refs (real boundary drift)
- Code references that the implementer would copy and discover are wrong
- Phrasing/parentheticals that contradict the body

## Findings (8)

All LOW. The previous two rounds were heavy on substantive bugs;
this round is mostly stale off-by-one line refs and minor wording
clarifications.

#### N39 [LOW] — Capability report line off by one

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:327`.

**Bug:** "capability report at line 1190" — but `can_join_by_id: false`
(which is the bit M10 flips) is at line **1189** of
`crates/octo-adapter-irc/src/lib.rs`. `can_join_by_invite: true` is at
1190, so the cited line is one row down.

**Fix:** change "line 1190" → "line 1189".

#### N40 [LOW] — `wacore` JoinGroupResult line off by one

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:136`.

**Bug:** "`wacore/src/iq/groups.rs:2318`" — verified at the actual
SDK checkout
(`/home/mmacedoeu/.cargo/git/checkouts/whatsapp-rust-6e26428647c827f3/9734fb2/wacore/src/iq/groups.rs`),
`pub enum JoinGroupResult` is at line **2319**.

**Fix:** change "2318" → "2319".

#### N41 [LOW] — Phase 1 title still says "(low risk, no behavior change)"

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:294`.

**Bug:** R24b N37 fixed the mission's Phase 1 title but missed the
identical wording in the RFC:

```
### Phase 1: Trait surface (low risk, no behavior change)
```

But Phase 1 adds `try_new` ctors, `AddMemberOutput` (a public API
return-type change on `add_member`), `initial_admins_promoted: bool`
on `GroupHandle`, and `list_own_groups_with_invites` — all additive
but not "no behavior change".

**Fix:** match the mission's revised title: "Trait surface (additive;
no breakage for existing callers)".

#### N42 [LOW] — M8 implementation note uses broken AtomicBool syntax

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:104-105`.

**Bug:** "*self.is_authenticated.store(true)" — `AtomicBool::store`
takes `(self, value: bool, order: Ordering)`. The note omits the
required `Ordering` argument.

**Fix:** "*self.is_authenticated.store(true, Ordering::SeqCst)" or
just "*self.is_authenticated.store(true, std::sync::atomic::Ordering::SeqCst)".

#### N44 [LOW] — IrcConfig::validate cited at line ~140 but actually at line 95

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:327`.

**Bug:** "IrcConfig::validate channel rules (M15, line ~140)" — the
actual `pub fn validate` is at line **95** of
`crates/octo-adapter-irc/src/lib.rs` (struct at line 58, impl at
line 82, validate at line 95). The "~140" is off by 45.

**Fix:** change "line ~140" → "line 95".

#### N45 [LOW] — Mission says WhatsAppConfig may be in a separate config.rs

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:70`.

**Bug:** "`octo-adapter-whatsapp/src/config.rs` (if separate) —
`WhatsAppConfig::validate` (M16)". Verified: `ls
crates/octo-adapter-whatsapp/src/` shows no `config.rs` — the files
are `adapter.rs`, `lib.rs`, `state.rs`, `store.rs`. `WhatsAppConfig`
(struct at line 30, impl at line 83, validate at line 97) lives in
`adapter.rs`. The "(if separate)" hedge is misleading because
verification shows it is NOT separate.

**Fix:** drop the "(if separate)" and the path, replace with
"`WhatsAppConfig::validate` lives in `crates/octo-adapter-whatsapp/src/adapter.rs`
(struct line 30, impl line 83, validate line 97) — M16 edits the
`validate` method (line 97)".

#### N46 [LOW] — Key Files row for M8 only names the SET site, not the field decl

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:327`.

**Bug:** "is_authenticated (M8, in irc_session at line ~838)" — 838
is the line of the 376/422 handler where `*is_authenticated.store(true,…)`
will go. But the new field `is_authenticated: AtomicBool` is on
`IrcAdapter` (per §4 M8 spec), not on `irc_session`. The
implementer has to find the struct declaration too.

**Fix:** "is_authenticated: AtomicBool field on IrcAdapter
(struct def at line ~225 — next to `out_tx: Mutex<…>` and
`shutdown_tx: Mutex<…>`); SET it true inside the existing 376/422
branch in `irc_session` at line 838, CLEAR it inside `disconnect`
(wherever the existing shutdown_tx.clear happens)".

#### N47 [LOW] — Phase 2 M5 line says "in `create_group`" but H2 renames it to `create_group_str`

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:309`.

**Bug:** "Add `M5` debug logging in `create_group` (already
documented)". But Phase 2's H2 (line 307) renames the inherent
`create_group` to `create_group_str`. The M5 edits (replacing
`.ok()` with `tracing::debug!`) happen in the renamed
`create_group_str`. The current text doesn't cross-reference the
rename, so a careless implementer might M5-edit the wrong function
or get confused about the order.

**Fix:** "Add `M5` debug logging (replace `.ok()` with
`tracing::debug!` + continue) inside `create_group_str` (the
post-H2 name; pre-H2 it's `create_group`). Order within Phase 2:
H2 rename FIRST, then M5 / M11 / M1 / M16 edits land on the renamed
function."

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N39 | LOW | RFC Key Files | capability report line 1190 → 1189 |
| N40 | LOW | RFC §3 H1 | wacore line 2318 → 2319 |
| N41 | LOW | RFC Phase 1 title | "(low risk, no behavior change)" misleading |
| N42 | LOW | Mission Implementation Notes | `store(true)` missing Ordering |
| N44 | LOW | RFC Key Files | `IrcConfig::validate` line ~140 → 95 |
| N45 | LOW | Mission Location | WhatsAppConfig NOT in a separate config.rs |
| N46 | LOW | RFC Key Files | M8 row only names SET site, not field decl |
| N47 | LOW | RFC Phase 2 | M5 should cross-ref H2's `create_group_str` rename |

8 findings, all LOW. No CRITICAL, HIGH, or MEDIUM.

## Cross-references

- R24a review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`
- R24b review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`