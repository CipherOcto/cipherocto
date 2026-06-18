# RFC-0861 + Mission 0861 — Adversarial Review, Round 7

**Branch:** `next` (at commit ccf6aab)
**Reviewed:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md` (v1.6) + `missions/open/0861-coordinator-admin-trait-refinements.md`
**Date:** 2026-06-18
**Reviewer:** Jcode (adversarial, post-R24f)
**Scope:** Re-audit after the 3 R24f fixes. Look for internal
spec inconsistencies that survive the prior rounds — particularly
spec-vs-code drift after the R24d `GroupHandle` change.

## Method

Re-read both files. Specifically cross-checked:

- The §3 H1 struct literal against the current `GroupHandle` shape
  (and what M4 says it must become)
- The §4 M8 "clear on disconnect" instruction against the actual
  IRC code paths (`mark_disconnected` vs `shutdown`)

## Findings (3)

#### N64 [MEDIUM] — §3 H1 struct literal is missing `initial_admins_promoted`

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:145-147`.

**Bug:** The spec says:

> Map both variants to
> `Ok(GroupHandle { id: GroupId::new(jid.to_string()), is_admin: false,
> subject: None, invite_url: None, member_count: None, mode_flags: None })`.

But R24d's M4 fix (RFC §3 M4) requires `GroupHandle` to gain a new
field:

```rust
pub initial_admins_promoted: bool,
```

(verified in §3 M4 spec lines 188-194). And `GroupHandle` does
NOT derive `Default` (per
`crates/octo-network/src/dot/adapters/coordinator_admin.rs:216`:
`#[derive(Clone, Debug, Serialize, Deserialize)]`). So after M4
is implemented, the H1 struct literal at lines 145-147 will fail
to compile — it doesn't initialize the new field and there's no
Default to fall back on.

An implementer reading only the H1 spec would write the literal
as-is and get a compile error after Phase 1 (M4) lands, even
though both H1 and M4 are correctly specified.

**Fix:** add `initial_admins_promoted: false,` to the literal
in §3 H1. The H1 path doesn't go through `create_group`, so
`false` is correct (no promote was attempted by this code path —
the caller joined via invite code, not as initial admin).

The corrected literal:

```rust
Ok(GroupHandle {
    id: GroupId::new(jid.to_string()),
    is_admin: false,
    subject: None,
    invite_url: None,
    member_count: None,
    mode_flags: None,
    initial_admins_promoted: false,
})
```

#### N65 [MEDIUM] — §4 M8 "clear on disconnect" is ambiguous between two code paths

**Where:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:235-237`
(body) and line 327 (Key Files row).

**Bug:** The §4 M8 body says:

> The IRC impl MUST add an `is_authenticated: AtomicBool` field
> on `IrcAdapter`, set it on the first RPL_ENDOFMOTD (376) or
> ERR_NOMOTD (422) received, and **clear it on disconnect or
> session restart**.

But the IRC adapter has TWO disconnect-shaped methods:

- `mark_disconnected` at `crates/octo-adapter-irc/src/lib.rs:377-380`
  — clears `connected` and `out_tx`. Called when the listener
  detects a closed socket.
- `shutdown` at `crates/octo-adapter-irc/src/lib.rs:1086` — clears
  `shutdown_tx`, `out_tx`, `listener_handle`. Called on full
  adapter teardown.

The Key Files row (line 327, after R24c N46) says:

> CLEAR it in `disconnect` next to the existing `shutdown_tx`
> clear (M8)

But `shutdown_tx` is only cleared in `shutdown` (line 1113), NOT
in `mark_disconnected`. The Key Files row implies `is_authenticated`
should be cleared alongside `shutdown_tx` (i.e., in `shutdown`),
but the spec body says "on disconnect" (which most readers would
read as `mark_disconnected`).

The right answer is BOTH:

- `mark_disconnected` clears `is_authenticated` (transient drop)
- `shutdown` clears `is_authenticated` (full teardown)

Otherwise, after a transient drop + reconnect, the old
`is_authenticated = true` survives until the new 376/422
arrives, giving a brief window where `health_check` lies.

**Fix:** replace the body instruction with:

> clear it in **both** `mark_disconnected` (transient drop, at
> `lib.rs:377`) and `shutdown` (full teardown, at `lib.rs:1086`).
> The Key Files row's "next to the existing `shutdown_tx` clear"
> refers to the `shutdown` site, but the implementer MUST also add
> the clear in `mark_disconnected` because transient drops
> otherwise leave `is_authenticated = true` until the next
> 376/422 arrives.

#### N66 [LOW] — Mission Phase 3 has no test for the new `pending_replies` field

**Where:** `missions/open/0861-coordinator-admin-trait-refinements.md:54-63`.

**Bug:** Phase 3 acceptance criteria cover M7/M8/M10/M3 but
don't require a unit test for the new `pending_replies:
Mutex<HashMap<CommandId, oneshot::Sender<NumericResult>>>`
field that M7 introduces (per RFC §4 M7). Without a test,
the implementer could ship M7 with the field but no INVITE
correlation logic, or with the logic but the field mistakenly
shared across adapter instances (the HashMap is per-adapter, not
global).

**Fix:** add to Phase 3: "Unit test in
`crates/octo-adapter-irc/src/lib.rs` test module: register a
fake INVITE nonce, simulate `ERR_CHANOPRIVSNEEDED` arriving
in the listener, assert that the oneshot resolves with the
correct error and that the HashMap entry is removed. Also
verify that two `IrcAdapter` instances don't share the
HashMap (independence test)."

## Severity Summary

| ID | Sev | Where | What |
|---|---|---|---|
| N64 | MEDIUM | RFC §3 H1 | struct literal missing `initial_admins_promoted: false` (would fail to compile after M4 lands) |
| N65 | MEDIUM | RFC §4 M8 | "clear on disconnect" ambiguous between `mark_disconnected` and `shutdown`; should be both |
| N66 | LOW | Mission Phase 3 | No test required for the new `pending_replies` HashMap |

3 findings: 2 MEDIUM, 1 LOW.

## Cross-references

- R24a review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`
- R24b review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`
- R24c review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r3.md`
- R24d review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r4.md`
- R24e review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r5.md`
- R24f review: `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r6.md`
- RFC: `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`
- Mission: `missions/open/0861-coordinator-admin-trait-refinements.md`