---
name: 0871e1-paid-query-payload-carries-no-holder
description: "RFC-0871's PAID_QUERY_VERIFY payload carries no holder DID, while RFC-0862's SpendLedger is keyed by (holder_did, macaroon_id) and deliberately treats an empty holder as a valid distinct key — RFC-0862 TV-0862-14 pins exactly that. The handler substituted an empty DID and drained against whatever row sat under it. Measured, not inferred: with one row seeded under the empty holder, the production envelope path returned Proceed and drained query_cost. Anything able to seed that row owned a budget every unauthenticated paid query could spend. The handler now refuses, which closes the hazard and leaves paid queries non-functional; making them work is a wire decision this is not."
metadata:
  node_type: mission
  type: specification
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0862
    - RFC-0871
---

# Mission `0871e1-paid-query-payload-carries-no-holder`

> **DEFERRED — the hazard is closed; the capability is still off.**
>
> The unsafe behaviour was removed on 2026-10-12 because it was measured, not
> suspected. What remains is a wire decision that belongs to RFC-0871 and
> RFC-0862 together.

## Status

Deferred (2026-10-12) — hazard closed by refusing; paid queries over this payload
are non-functional until an RFC decides how the holder reaches the verifier.

## What two RFCs between them allow

| RFC                             | Says                                                                                                                                          |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| RFC-0862 §SpendLedger           | `seed(holder_did, macaroon_id, budget)` and `try_deduct(holder_did, macaroon_id, cost)`. The holder is part of the key.                       |
| RFC-0862 v2.0.7                 | The substrate performs **no** canonical holder validation — _"canonical validation lives at the wallet-node boundary"_.                       |
| RFC-0862 TV-0862-14             | Pins that an **empty** `holder_did` is accepted and persists as a **valid, distinct** row, alongside non-`did:octo:` and binary-garbage keys. |
| RFC-0871 §Roles and Authorities | `PAID_QUERY_VERIFY` payload is `{macaroon_id, caveat, query_cost, query_model, now_unix_ms}`. **No holder.**                                  |

Each is defensible alone. Together they leave the wallet-node boundary — the one
place RFC-0862 says validation belongs — with no holder to validate and a
fallback that is a real ledger key rather than a miss.

## Measured, not inferred

The handler substituted `WireDid::new(String::new())` when no holder hint was
supplied, which is what every production caller does: `handle()` passes `None`,
and the only `Some(..)` callers in the workspace are tests.

A throwaway probe seeded one row under the empty holder and called the production
path:

```
DECISION  = Proceed { remaining_budget: Dqa { value: 999000, scale: 0 } }
DRAINED   = Dqa { value: 1000, scale: 0 }
REMAINING = Dqa { value: 4999000, scale: 0 }
```

Not `UnknownHolder`. Not a rejection. **A draw against a holder-less pool**, with
no per-holder isolation, reachable by anyone who can seed that row — and
`SpendLedger::seed` is public and RFC-0862 permits the key.

The hazard was latent rather than live, because the mint path validates the DID
before seeding and nothing else seeds. Safe by accident of who has written code,
not by a property anything enforces.

## What was done

`PaidQueryVerifyHandler::handle` now refuses, naming the missing holder, and
`handle_with_holder` refuses rather than substituting when no holder is supplied.
A test seeds the empty-holder row **first** and then asserts the refusal and that
the balance is untouched — the planted violation, so the test is a check rather
than a description. Restoring the old code fails exactly the three tests that
claim this.

Two pre-existing tests were rerouted rather than deleted. They exercised caveat
expiry and payload kind through `handle()`, which no longer reaches a decision;
they now use `handle_with_holder` with a holder, which preserves the properties
they were written to pin.

## What the RFC has to decide

Not "should this be refused" — it must be, since the handler cannot identify the
holder. The question is how the holder legitimately reaches the verifier:

1. **The payload carries it.** Add `holder_did` to `PAID_QUERY_VERIFY`. Simple,
   but it is a wire change to a served payload kind, and the holder then asserts
   its own identity in the same envelope it is being charged for — which needs an
   argument about why that is not the same as self-asserted authority elsewhere.
2. **The caveat carries it.** `PaymentCaveat` has no audience field today; adding
   one means the caveat, not the envelope, names the payer. It is already signed
   by the issuer and travels inside the capability, so it is harder to swap.
3. **Derive it.** The capability's `HolderRecord` names the holder and the
   registry is authoritative. This removes the holder from the wire entirely, but
   requires the verifier to resolve the registry on every paid query and to have
   a `macaroon_id` to `cap_root_hash` mapping, which no code performs today.

Option 3 is the one that matches how the rest of the system decides identity,
and it is the most expensive. That is a design argument, not an implementation
detail, and it is not settled here.

## Acceptance Criteria

- [ ] An RFC states how the holder reaches the paid-query verifier.
- [ ] `PAID_QUERY_VERIFY` over the envelope charges the **right** holder's ledger
      row, proven by a test that seeds two holders, charges once, and shows the
      second is untouched. **Per-holder isolation has no test anywhere today**,
      because there was never a way to express which holder is being charged.
- [ ] The refusal on the no-holder path stays, so the payload cannot silently
      regress into charging the empty-holder row.

## What must not happen

**Do not "fix" this by making the empty holder legitimate.** It already is,
under RFC-0862, and that is the hazard rather than the fix.

**Do not relax the refusal to make paid queries appear to work.** A handler that
charges an arbitrary row and returns `Proceed` is worse than one that refuses:
it converts a missing capability into a wrong charge.
