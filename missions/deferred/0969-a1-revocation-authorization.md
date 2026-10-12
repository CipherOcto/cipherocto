---
name: 0969-a1-revocation-authorization
description: The capability issuer node revokes capabilities for real, but any caller that reaches CAPABILITY_REVOKE can revoke any capability the node knows. This is not a missing feature — no RFC specifies who may invoke a revoke. The rule this gap was documented against, an RFC-0965 RevocationCaveat, does not exist: RFC-0965 never mentions revocation, discriminator 0x1A sits in its reserved range, and Caveat has no Revocation variant. The phantom citation appeared in the handler docs and two claimed missions, which made an undesigned security boundary look like an implementation backlog item.
metadata:
  node_type: mission
  type: specification
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0965
    - RFC-0957-A1
---

# Mission `0969-a1-revocation-authorization`

> **DEFERRED — blocked on an RFC that does not exist yet.**
>
> There is nothing to implement until someone specifies the rule. Filing this as
> `deferred` rather than `open` because `docs/BLUEPRINT.md` §Mission Lifecycle
> names "governance decision" and "RFC pending" as triggers for that state, and
> this is both.

## Status

Deferred (2026-10-12) — pending an RFC that allocates the revocation caveat and
states who may invoke a revoke.

## The gap

`RevokeHandler` now calls `HolderRegistry::revoke(cap_root_hash, clock)` and
reports honestly what happened. That part is done and mutation-verified.

What is missing is authorization. The envelope's signature, TTL and dedup are
checked upstream in the dispatcher, but **nothing ties the signer to the
capability being revoked**. Any caller that can reach `CAPABILITY_REVOKE` can
revoke any capability this node knows about.

## Why this is not a backlog item

The gap was documented — in the handler's own docs and in two claimed missions —
as requiring an "RFC-0965 `RevocationCaveat` issued by the original issuer or a
higher-authority governance capability". **That rule does not exist.**

| Claimed                               | Reality                                                                         |
| ------------------------------------- | ------------------------------------------------------------------------------- |
| RFC-0965 defines a `RevocationCaveat` | RFC-0965 contains **zero** occurrences of "revocation"                          |
| Discriminator `0x1A` is that caveat   | `0x1A` is in RFC-0965 §Discriminators' **reserved** range for future extensions |
| `Caveat` has a revocation variant     | `Caveat` is `#[non_exhaustive]` and has no `Revocation` variant                 |

The phantom citation was repeated in `missions/claimed/0871d` (four places),
`missions/claimed/0871e` (one), and the handler source. All are corrected.

This matters beyond the wrong reference number. A named RFC rule reads as "the
design decided this; the code has not caught up". What is actually true is
"nobody decided this". Those look identical in a mission file and call for
completely different responses.

## What the RFCs do say

- **RFC-0957-A1** specifies the **mechanism** in full: `revoke(cap_root_hash,
clock)`, idempotent, writing `revoked_at_millis_unix`, with `lookup_active`
  refusing a revoked record. Implemented and correct.
- **RFC-0957-A1 §Roles and Authorities** gives the Token Issuer the authority
  _"mint + revoke + register"_. That is a **role** statement, not a **request**
  rule. It says the issuer may revoke; it does not say how a request proves the
  sender is the issuer.
- **No RFC states who may invoke a revoke.** Nothing at request level.

## What the RFC has to decide

This is the part that cannot be guessed at, and the reason it is a spec mission
rather than a code change.

1. **Who may revoke.** The issuing node only? The holder, for its own
   capability? A governance key? More than one, with a precedence order?
2. **What proves it.** A caveat on the capability being revoked is the obvious
   mechanism and the pattern the codebase already has — but a caveat travelling
   _with_ the target capability is not obviously the right carrier for authority
   over revoking it. A holder able to attach a caveat could attach its own
   revocation authority. This needs an actual argument, not a plausible shape.
3. **Which discriminator.** RFC-0965 reserves `0x1A-0xCF` for future
   extensions. Allocating one is a maintainers' decision.
4. **Whether revocation is monotonic across kinds.** `HolderRegistry::revoke` is
   monotonic per record; whether revoking a `V1` record also revokes its
   `ZKBearing` and `HopCapability` relatives is unstated.

## Scope

### In Scope

- An RFC amendment that states who may invoke a revoke and by what proof.
- Discriminator allocation through the RFC-0965 pattern, if a caveat is the
  chosen mechanism.
- A `Revocation` caveat variant plus its evaluator, once the rule exists.
- `RevokeHandler` enforcing that rule, and a test that a caller without it is
  refused.

### Out of Scope

- Revocation **sync across nodes**. RFC-0957-A1 revocation gossip is separately
  marked "not yet spec'd" and is its own problem.
- The revocation **mechanism** itself, which is specified, implemented and
  mutation-verified.
- Event emission on revoke, which remains open and is not a security boundary.

## Acceptance Criteria

- [ ] An RFC states who may invoke a revoke and what proves it.
- [ ] The handler refuses a caller that does not meet that rule, with a test
      that is mutation-checked — a handler that ignores the rule must turn it
      red.
- [ ] No doc, source comment or mission cites an RFC rule for revocation
      authorization that does not exist. **Already satisfied for every site found
      (four files, seven occurrences), and deliberately NOT guarded.**

  A guard was written for this and deleted. It blocked a fixed list of terms
  already known to be phantom, so it could only ever catch a repeat of a finding
  already made — and it flagged every file that mentions the term in order to
  _say it does not exist_, which is the correction. It needed an exclusion for
  each corrected site, which is how a guard stops being a check and becomes a
  census that confirms what was already believed.

  The honest position: this criterion is enforced by reading, not mechanically.
  Citing a symbol no RFC defines is not mechanically detectable in general — the
  check that would generalize is "does this section say what the doc claims",
  and every version of that attempted produces false confidence rather than
  coverage. Recorded here rather than papered over with a guard that fires on
  its own corrections.

## What must not happen

**Do not pick a rule because it is implementable.** The obvious mechanism — a
caveat travelling with the target — has the property that whoever can attenuate
can grant themselves revocation authority over their own capability. That may be
acceptable; it may not. Deciding it is the work.

**Do not close this by documenting the current behaviour as intended.** The
current behaviour is that revocation is ungated. Calling that a design choice
would make the gap permanent and invisible, which is exactly the state this
mission exists to end.

**Do not cite an RFC that does not say it.** That is how this gap was hidden: a
mission file asserted a rule from an RFC that contains no such rule, and the
assertion was plausible enough to survive review by reading.
