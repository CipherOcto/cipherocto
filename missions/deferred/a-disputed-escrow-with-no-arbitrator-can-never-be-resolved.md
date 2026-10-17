---
name: a-disputed-escrow-with-no-arbitrator-can-never-be-resolved
description: "Escrow::new is the primary constructor and sets arbitrator to the empty string. A buyer can then lock and dispute, reaching the Disputed state, from which no transition exists: settle is refused by the state guard because it requires Locked, and resolve_valid and resolve_invalid are both refused by check_caller because the stored arbitrator is empty. Measured: Pending to Locked to Disputed, then SettleFromInvalid(Disputed), UnauthorizedCaller for both resolutions, final state Disputed. is_terminal returns false, so the stuck state does not report as terminal either. The buyer can reach it unilaterally at will, and nothing times out or unwinds it. RFC-0900's Dispute interface carries no resolver field and states resolution as a governance vote or automated arbitration, so under the specification this situation cannot arise; the code makes resolution conditional on a field the primary constructor leaves empty."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `a-disputed-escrow-with-no-arbitrator-can-never-be-resolved`

## Status

Deferred — every fix is a dispute-policy decision the RFC does not settle:
whether the constructor should require an arbitrator, whether a dispute
should be withdrawable, or whether a resolution deadline should exist. All
three change behaviour a counterparty can rely on. Nothing was actioned; the
measurement probe was removed and the tree matches its prior state.

## Finding

The escrow has a one-way trap. A buyer can enter it unilaterally, no party
can leave it, and the type does not report it as stuck.

### The state machine

```text
Pending ──lock(buyer)──> Locked ──settle(seller)──> Settled   [terminal]
                         │
                         └──dispute(buyer)──> Disputed
                                                ├──resolve_valid(arbitrator)──> Slashed  [terminal]
                                                └──resolve_invalid(arbitrator)──> Settled  [terminal]
```

Every transition checks the source state, and every transition out of
`Disputed` requires `RequiredRole::Arbitrator`, which `check_caller` satisfies
only when the caller's identity equals the stored `self.arbitrator`.

### The gap

`Escrow::new` is the primary constructor, and it sets:

```rust
arbitrator: String::new(),
```

Its own doc says "Use `with_arbitrator` to set one" — correctly, because
`check_caller` refuses when the stored value is empty
(`stored.is_empty() || caller.identity() != stored`).

Nothing prevents constructing without one and then reaching a dispute.

### Measured

An escrow from the primary constructor, driven through every available
transition:

```text
initial         = Pending
lock            = Ok(Locked)
dispute         = Ok(Disputed)
settle (seller) = Err(SettleFromInvalid(Disputed))
resolve_valid   = Err(UnauthorizedCaller { required: "arbitrator", caller_role: "arbitrator", caller_identity: "arb" })
resolve_invalid = Err(UnauthorizedCaller { required: "arbitrator", caller_role: "arbitrator", caller_identity: "arb" })
FINAL state     = Disputed
is_terminal     = false
```

The seller is refused by the **state** guard — `settle` requires `Locked`.
The arbitrator is refused by the **identity** guard — there is none to match.
There is no timeout, no withdrawal, no force-resolve, and no transition the
probe did not try.

## Why `is_terminal() == false` matters

`is_terminal` is `matches!(state, Settled | Slashed)`. For this escrow it
returns false, which is technically correct — `Disputed` normally _is_
resolvable — and misleading in practice.

A caller checking whether an escrow is stuck has no expression for "stuck".
`is_terminal()` says no, so a sweep over escrows looking for non-terminal
entries reports a healthy marketplace. The one state that cannot progress is
the one state the type reports as still in progress.

That is the same failure shape as the rest of this audit, in a different
place: the type cannot represent the condition, so no query over it is
trustworthy. See
`documentation-that-describes-a-mechanism-nobody-built` for the class.

## What the specification says

RFC-0900 §Dispute Resolution defines the dispute record without any resolver:

```typescript
interface Dispute {
  id: string;
  buyer: string;
  seller: string;
  listing_id: string;
  reason: "failed_response" | "garbage_data" | "timeout";
  evidence: string;
  timestamp: number;
}
// Resolution: Governance vote or automated arbitration
```

Resolution is described as a **governance vote or automated arbitration** —
a process, not a registered identity. Nothing in the interface says a dispute
can be filed against an escrow that has no named resolver, because under that
model resolution is always available.

So the specification has no version of this state, and the implementation
introduced one by making resolution conditional on a constructor argument
that the primary constructor leaves empty.

The Dispute Lifecycle machine compounds it: `FILED → INVESTIGATING → VALID |
INVALID | PARTIAL` with every branch reachable and no expiry edge. The
implementation matches that topology and additionally has no way out of the
node the buyer can reach.

## Calibration: what this surface gets right

The escrow is the best-implemented authorization path in the marketplace, and
the contrast with `AskSigned::verify` is worth recording rather than
implying otherwise.

`check_caller` verifies **two** things: that the caller's `Party` variant
matches the required role, _and_ that the caller's identity equals the stored
counterparty. `AskSigned::verify`, by comparison, checks that a supplied key
signed a payload and never confirms the key belongs to the named DID. So the
pattern the Ask path needs already exists here, correctly, in the same crate
tree.

Every transition also guards the source state, so the trap above is not a
missing state check — it is a reachable state with no outgoing edge.

## What is not a finding here

- **`TaskEscrow` is not a second implementation.** `task_market::escrow` is a
  thin newtype that delegates every transition to `Escrow` via `self.base`,
  so the two cannot diverge. Worth stating because the duplicated-escrow
  reading is the obvious one and it is wrong.
- **Arbitrator independence is unspecified, not violated.** Neither
  `Escrow::new` nor `with_arbitrator` rejects an arbitrator equal to the buyer
  or seller. RFC-0900 says nothing about independence, and `grep -i
arbitrator` across `rfcs/` returns only a changelog line referring to the
  code. That is a gap in the specification, not a defect in the
  implementation — recorded here so the trap finding is not read as covering
  it.
- **The missing `PARTIAL` outcome is a separate defect, not part of this one.**
  RFC-0900's Dispute Lifecycle has `INVESTIGATING --> PARTIAL` and
  `PARTIAL --> PARTIAL_REFUND`; the implementation offers only `resolve_valid`
  and `resolve_invalid`. That is an absent branch rather than an unreachable
  one, and it has its own resolution requirements, so it is noted here rather
  than folded into this finding.

## What the RFC owner needs to decide

- Should `Escrow::new` take an arbitrator, or be renamed/removed so the
  unresolvable constructor is not the obvious one? This is the smallest fix
  and the most disruptive to callers.
- Should a dispute be withdrawable by the buyer, or expire by deadline, or be
  resolvable by the seller after a timeout? Any of the three removes the trap.
  All three are policy, and the RFC does not state any of them.
- What should `is_terminal()` report for a disputed escrow with no arbitrator?
  If the answer is "true", `Disputed` needs to become terminal in general,
  which changes resolution semantics; if the answer needs a distinct value,
  `is_terminal() -> bool` is the wrong signature.
- Should RFC-0900 §Dispute Resolution say who resolves and how they are
  selected? "Governance vote or automated arbitration" names two processes and
  neither is modelled, while the code has implemented a third — a registered
  arbitrator identity — without a specification to satisfy.
- If a registered arbitrator is the intended model, should the constructor
  reject one that equals the buyer or seller?

## Reproduction

```bash
# the primary constructor leaves the arbitrator empty
grep -n -A18 'pub fn new(' crates/quota-router-core/src/marketplace/escrow.rs

# every exit from Disputed requires the arbitrator
grep -n -A12 'pub fn resolve_valid\|pub fn resolve_invalid' \
  crates/quota-router-core/src/marketplace/escrow.rs
grep -n -A12 'pub fn settle' crates/quota-router-core/src/marketplace/escrow.rs

# check_caller refuses an empty stored arbitrator
grep -n -A28 'fn check_caller' crates/quota-router-core/src/marketplace/escrow.rs

# is_terminal cannot express "stuck"
grep -n -A4 'pub fn is_terminal' crates/quota-router-core/src/marketplace/escrow.rs

# the RFC's dispute record has no resolver field
grep -n -A14 'interface Dispute' rfcs/accepted/economics/0900-ai-quota-marketplace.md

# no timeout edge in either machine
grep -n -A10 'Purchase Lifecycle\|Dispute Lifecycle' \
  rfcs/accepted/economics/0900-ai-quota-marketplace.md

# TaskEscrow delegates rather than duplicating
grep -n -A4 'pub fn resolve_valid' crates/quota-router-core/src/task_market/escrow.rs

# arbitrator is named in no RFC
grep -ri 'arbitrator' rfcs/ | grep -v 0862
```

## Cross-references

- RFC-0900 §Dispute Resolution — the `Dispute` interface and its
  "governance vote or automated arbitration" note
- RFC-0900 §Purchase Lifecycle / §Dispute Lifecycle — the two state machines,
  neither of which has an expiry edge
- RFC-0918 — the specification behind `task_market`, the escrow's other
  consumer
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` — why
  this is latent rather than exploited today
- Mission `a-signed-ask-does-not-prove-who-published-it` — the contrast:
  `check_caller` here does bind identity to role, and `AskSigned::verify`
  does not
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  this belongs to, and why an unrepresentable state is the same shape
