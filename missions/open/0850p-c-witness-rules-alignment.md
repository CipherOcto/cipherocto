---
name: 0850p-c-witness-rules-alignment
description: dot/witness.rs implements a ten-rule BIND validator that is not RFC-0850p-c §8's rule set. Rule 5 (first-BIND-wins) can never fire because first_bind_seen is never written, and its logic is inverted against the RFC when it does; rule 10 is a rate limit that exists in neither the RFC nor the code, though its comment claims it is "satisfied by construction"; Six of RFC-0850p-c §8's ten rules — 2, 4, 5, 6, 8 and 9 — are absent, materially weaker, or materially different, three of them absent outright. Nothing is live — validate_bind has no production caller and BINDHook has no implementations — so this is a landmine for whoever wires Phase 5, not an exploit.
metadata:
  node_type: mission
  type: specification
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0850p-c
    - 0850p-c-base
---

# Mission `0850p-c-witness-rules-alignment`

> **Nothing here is live.** `validate_bind` has zero production callers and
> `BINDHook` has zero implementations, both measured. Nothing is exploitable
> today. The reason this matters is that the module doc says "Implements the 10
> witness validation rules for BIND envelopes" and a reader wiring Phase 5
> would reasonably believe the rules are done.
>
> **The validator itself is not changed by this filing.** It is Phase 3 of a
> claimed mission and it governs an Accepted RFC. Aligning it is maintainer
> work. What is filed here is the delta, measured.

## Status

Open (2026-10-12) — filed by audit. Requires maintainer sign-off before any
change to `validate_bind`, for the reason in §What must not happen.

## How this was found

A workspace census looked for verification-shaped functions
(`verify*`/`check*`/`is_*`/`validate*`/`meets_*`/`has_*`) with **zero**
production call sites — the shape that `governance_set_hash` had in
`octo-reputation` before it was fixed. That census returned 141 such
functions and was **not** treated as a defect list: most are Layer-A substrate
awaiting a Layer-C consumer, which is the intended layer direction, not a bug.

The census was then joined against prose. Seven comments across the workspace
claimed a dead verification function was called. Six were honest deferrals.
One pointed at a rule comment inside `validate_bind` itself that asserted an
enforcement the body does not contain, which is how this was reached.

**The census's first version was wrong and is worth recording.** It cut each
file at the first `#[cfg(test)]` marker. `dispatch.rs` carries a method-level
`#[cfg(test)]` accessor twenty lines above `verify_all`, so the scan reported
`verify_authorization_signature` as uncalled when `verify_all` calls it twenty
lines further down. A confident false positive from a plausible mechanism is
worse than no scan. The corrected version brace-matches whole test modules and
drops the false positive.

## The delta, rule by rule

RFC-0850p-c §8 "Witness Validation Rules" against `validate_bind`. Rule
numbers on the left are the RFC's.

| RFC §8 | Rule                                                          | In `validate_bind`                                | State                                                             |
| ------ | ------------------------------------------------------------- | ------------------------------------------------- | ----------------------------------------------------------------- |
| 1      | Signature                                                     | Rule 1                                            | matches                                                           |
| 2      | `mission_id` matches a known `MissionDescriptor`              | Rule 8 checks `mission_id != [0u8; 32]`           | **weaker** — a non-zero unknown mission passes                    |
| 3      | Platform match                                                | Rule 3                                            | matches                                                           |
| 4      | `group_jid` matches the group received in                     | Rule 9 checks JID well-formedness                 | **wrong rule** — well-formedness is not context binding           |
| 5      | No existing binding for `(mission_id, domain_id, platform)`   | —                                                 | **absent**                                                        |
| 6      | Coordinator eligibility per RFC-0855p-b                       | —                                                 | **absent**                                                        |
| 7      | Epoch sanity ±1                                               | Rule 7                                            | matches                                                           |
| 8      | `(bind_nonce, coordinator_id)` unseen in the last 1000 epochs | Rule 2, keyed `(platform, group_jid)`, window 100 | **wrong key and window**                                          |
| 9      | `coordinator_id == BLAKE3(coordinator_pubkey)`                | —                                                 | **absent**                                                        |
| 10     | Reconnect split-brain, then lowest `bind_hash` wins           | Rules 6 and 5                                     | split-brain matches; the tiebreaker does not fire and is inverted |

And two rules in the code that are not in the RFC at all: rule 4
(`domain_id` non-zero) and rule 10 (the rate limit).

## The two findings worth acting on first

### Rule 5 can never fire, and is inverted when it would

`ctx.first_bind_seen` is read at the rule 5 check and **never written
anywhere in the workspace** — the eleven writes are all `first_bind_seen: None`
in test fixtures. The rule is inert.

It is also backwards. The code rejects when the incoming `bind_hash` is
**lower** than the one already held:

```text
if prev.bind_hash > envelope.bind_hash && ... { Reject }
```

RFC §8 rule 10 says the opposite: "if the incoming `bind_hash` is lower AND
the BIND is valid, the witness **switches to** the incoming BIND". The code
rejects exactly the case the RFC requires accepting. This is latent only
because nothing populates the field — the first person to wire Phase 5 and set
`first_bind_seen` inherits a rule that fails the RFC's determinism requirement
in the direction that breaks convergence between witnesses.

### Rule 10 asserts an enforcement that does not exist

The rule 10 site is a comment and nothing else. It claims the rate limit is
"satisfied by construction" via the nonce table.

`NonceReplayTable` stores one `(nonce, first_seen)` per `(platform,
group_jid)`. It is a set of the most recent nonce per group, so it cannot count
BINDs at all. A second BIND for the same group with a fresh nonce is not a
replay of the stored nonce and passes rule 2.

Measured, not inferred. Two BIND envelopes for the same `group_jid`, epochs
100 and 101 — inside the 100-epoch window — with different nonces, both with
valid signatures:

```text
PROBE first  = Accept
PROBE second = Accept
```

Rule 10 as documented ("at most 1 BIND per `(platform, group_jid)` per
`BIND_WITNESS_TIMEOUT = 100` epochs") is not enforced.

Note also that the code's rule 10 misreads the RFC constant.
`BIND_WITNESS_TIMEOUT = 100` is the implicit-coordinator **`BIND_ACK` timeout**
in §3, with 3 retries at `50, 200, 800` epochs — not a per-group submission
rate limit. There is no such limit in §8.

## Scope

### In Scope

- Decide, per rule in the table, whether the code or RFC-0850p-c §8 is the
  intended behaviour, and record the decision.
- Make rule 5 able to fire, with the RFC's direction: lowest `bind_hash` wins.
- Delete the rule 10 comment or implement a rate limit the RFC actually
  specifies. Right now it claims enforcement that does not exist.
- Reconcile the nonce table with RFC rule 8's `(bind_nonce, coordinator_id)`
  key and 1000-epoch window.
- Supply RFC rules 2, 4, 5, 6 and 9, or record each as an accepted gap with the
  threat it leaves open — the way `verify_receipt_chain` documents its own
  integrity/authenticity split.

### Out of Scope

- Phase 5 adapter integration and the `BINDHook` implementations.
- The `GroupRegistry` binding state machine, owned by the base mission.
- RFC-0855p-b coordinator eligibility itself — only that §8 rule 6 is not
  called.

## Acceptance Criteria

- [ ] Every row of §The delta is either implemented or recorded as an accepted
      gap naming the threat it leaves open.
- [ ] A BIND with a lower `bind_hash` than the one held is accepted and
      replaces it, per RFC §8 rule 10. Mutation-verified: restoring the
      current inverted comparison turns that test red.
- [ ] A second BIND for the same group inside the window is either refused by
      a rule the RFC specifies, or the claim that it is refused is gone.
      Mutation-verified against the rule 10 comment, which is the thing
      currently making the claim.
- [ ] The module header no longer says the RFC's rules are implemented while
      they are not.
- [ ] `validate_bind` has a production caller, or §The gap says in the module
      that nothing calls it. **Both halves are load-bearing**: the second is
      true today and is stated nowhere.

## What must not happen

**Do not align the code to the code.** The tempting move is to rewrite the ten
rules into whatever the current implementation does and call it aligned. The
RFC is Accepted; where the two disagree, the RFC is the specification and the
code is the deviation. Fixing the code to match itself would resolve the
delta by deleting it.

**Do not rename rule numbers to make the table fit.** The code numbers its
rules 1-10 and the RFC numbers its rules 1-10, and they are not the same
lists. The numbering collision is what made this look finished. Any alignment
that renumbers to hide it repeats the original problem.

**Do not wire Phase 5 before the delta is decided.** Wiring an adapter to a
validator whose rule 5 is inverted and whose rule 10 is a comment would put a
claimed-enforced rule set on a live path. That is the specific outcome this
filing exists to prevent.

**Do not treat the 141-function census as a defect list.** Most are Layer-A
substrate awaiting a consumer, which is the layer model working. Only the
prose claims that assert a dead check is enforced are findings, and that is
why the join — not the census — is what produced this mission.
