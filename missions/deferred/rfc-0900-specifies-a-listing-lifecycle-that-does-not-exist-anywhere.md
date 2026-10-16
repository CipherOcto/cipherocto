---
name: rfc-0900-specifies-a-listing-lifecycle-that-does-not-exist-anywhere
description: "RFC-0900 section State Machines draws three lifecycle diagrams. The purchase and dispute machines are implemented, with their gaps documented in the escrow enum's own doc comments. The listing lifecycle is not: ListingState appears in no crate, and the marketplace module holds escrow, orderbook, scoring, slashing and reputation compatibility with no listing entity at all. Two further internal contradictions sit in the same document. Section Security lists a stake requirement as a deployed security mechanism while Open Questions still asks what the minimum stake should be, and the code has no stake gate. The purchase diagram specifies a DISPUTED to REFUNDED transition while Open Questions asks how refunds work, and the escrow's Slashed variant carries no refund representation. The Observability section specifies a MarketTelemetry interface enumerating what must never be logged, and no telemetry struct exists in any crate. The slash ledger substrate in the same document is sound and is recorded here as such."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0959
---

# Mission `rfc-0900-specifies-a-listing-lifecycle-that-does-not-exist-anywhere`

## Status

Deferred. A listing entity is a type and schema decision, the refund
representation is an economics question, and the telemetry interface is a new
surface. Nothing was actioned.

First structural sweep of RFC-0900. Its twenty-three existing deferred missions
are all code-level; none sweeps a self-describing structure the way RFC-0959's
did.

## Finding

RFC-0900 §State Machines contains three Mermaid diagrams:

- **Listing Lifecycle** — `CREATED → ACTIVE → EXHAUSTED | CANCELLED`
- **Purchase Lifecycle** — `INITIATED → ESCROWED → COMPLETED | DISPUTED → REFUNDED | CONFIRMED`
- **Dispute Lifecycle** — `FILED → INVESTIGATING → VALID | INVALID | PARTIAL → SLASHED | KEEP | PARTIAL_REFUND`

Two are implemented with their gaps honestly documented. One is not implemented
at all.

## The listing lifecycle has no representation anywhere

`ListingState` does not appear in a single crate. The marketplace module
contains `escrow`, `mod`, `orderbook`, `reputation_compat`, `scoring` and
`slashing` — there is no listing entity of any kind, and the only occurrence of
"exhausted" in the module is a comment about orderbook bid residuals.

The RFC's terminal state `EXHAUSTED: All prompts sold` is the clearest tell. A
listing is sold out when its prompt inventory reaches zero; the `Ask` type that
plays the listing's role in RFC-0959 has a TTL and a rate table but no
quantity and no exhaustion.

This is the same gap
`the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`
already records, from the other document. Two accepted RFCs specify a listing
lifecycle and neither the `Ask` nor a `Listing` type carries one. What is new
here is that the parent RFC's diagram is unswept, so the same absence appears
twice with no mission connecting them.

## Two contradictions inside one document

**§Security lists a stake requirement among the deployed mechanisms:**

| Mechanism             | Purpose                      |
| --------------------- | ---------------------------- |
| Local proxy only      | API keys never leave machine |
| Balance check first   | Prevent overspending         |
| **Stake requirement** | **Prevent spam/abuse**       |
| Reputation system     | Build trust                  |

**§Open Questions, the document's final section, still asks:**

> 2. Minimum stake for sellers?

The document simultaneously presents the stake requirement as a security
mechanism in place and as an open question. The code resolves the question in
the negative — there is no stake gate on the marketplace path, which
`the-marketplace-has-no-stake-gate-and-the-role-it-would-gate-on-is-not-in-the-role-registry`
already measures. So the §Security table is the one place in the document that
reads as though the control exists, and it is the place with the least support.

**The purchase diagram specifies a refund the RFC also leaves open.** It draws
`DISPUTED → REFUNDED: Valid dispute`. §Open Questions asks:

> 3. How to handle failed requests (refund OCTO-W)?

And the escrow's `Slashed` variant says so in its own doc comment:

> RFC-0900 §Escrow Flow step 4 makes a valid dispute two effects — refund the
> buyer **and** slash the seller's stake. This variant names only the slash;
> the refund has no representation, consistent with this module moving no
> funds.

So the refund is specified as a transition, omitted from the type, and listed
as an open question. Three places in one document, three different treatments,
and the implementer was left to pick.

## MarketTelemetry does not exist

§Observability specifies an interface with an explicit privacy contract:

```typescript
interface MarketTelemetry {
  // What we log (no PII)
  event: "purchase" | "listing" | "swap" | "dispute";
  timestamp: number;
  provider: string;
  octo_w_amount: number;
  latency_ms: number;
  success: boolean;

  // What we DON'T log
  // - Prompt content
  // - API keys
  // - Wallet addresses (use hash instead)
}
```

`MarketTelemetry` appears in no crate, and the marketplace module emits no
telemetry of any kind. This is the `documentation-that-describes-a-mechanism-nobody-built`
class, and it is the one instance in this document that carries a **negative**
requirement: a list of things that must never be logged. A privacy contract
with no implementation is not a missing feature so much as an unenforced
promise, and the `// - Wallet addresses (use hash instead)` line in particular
asks for a hashing helper that a reader would reasonably expect to find.

## The slash ledger substrate, which is sound

Worth recording at the same length as the findings, because it is the section
most likely to be assumed broken.

§Slash Ledger Substrate is the best-specified part of RFC-0900 and it is
implemented as written:

- **v015 landed.** `v015__chain_aware_slash_ledger.sql` adds `chain_id BLOB`,
  backfills the 32-zero-byte default namespace, drops the `provider_id` UNIQUE
  under all three fork-specific index names, and creates
  `slash_ledger_chain_provider_idx` on `(chain_id, provider_id)` — every step the
  RFC claims.
- **The cross-chain partition invariant has a real test.**
  `cross_chain_same_provider_two_distinct_rows` writes one provider in two
  chains and asserts the rows do not collapse and each round-trips exactly. That
  is the invariant the RFC calls out, tested the way it should be.
- **The fork gap is disclosed twice,** in the RFC and in the migration's own
  header: no native Dqa driver, only `r.get::<i64>()` for amount columns, with
  DQA(12) adoption deferred. Both documents agree, and RFC-0959-v27 reaches the
  same conclusion independently.
- **The production caveat is stated rather than hidden:** production paths use
  `DEFAULT_CHAIN_ID` for all providers, and the tuple-keyed map is deferred until
  a multi-chain path activates.

One imprecision worth noting: the RFC calls `(chain_id, provider_id)` the
table's **primary key**, while v015 delivers a nullable `chain_id` inside a
UNIQUE index rather than a PK constraint. In SQLite-derived engines NULLs are
distinct in a unique index, so two rows with a NULL `chain_id` and the same
`provider_id` would both be admitted. No code path reaches that — every
`SlashLedgerRow` binds a `[u8; 32]` — so it is a schema weaker than the stated
invariant rather than a live defect.

## What the owner should do

- **Decide whether listings are a type.** Two RFCs draw the lifecycle and
  nothing carries it. If the `Ask` is the listing, say so and add exhaustion; if
  not, the type needs writing.
- **Resolve §Open Questions 2 and 3, or mark them deferred.** Both contradict
  the same document's §Security table and state diagram respectively, and a
  reader cannot tell which is current.
- **Either implement `MarketTelemetry` or move it to §Future Work.** The
  negative list is the part worth keeping either way; a privacy contract with no
  emitter is documentation of an intention.
- **Take the slash ledger section as the model** for the rest of the document. It
  states its invariants, names its migration, discloses its fork gap twice, and
  says what production does not yet do. That is what the listing lifecycle and
  the telemetry contract would need.

## Reproduction

```bash
# the listing lifecycle, absent from every crate
grep -rn 'ListingState' --include=*.rs crates/ determin/ octo-sync/ \
  || echo "ABSENT"
ls crates/quota-router-core/src/marketplace/

# the three diagrams
grep -n '### Listing Lifecycle\|### Purchase Lifecycle\|### Dispute Lifecycle' \
  rfcs/accepted/economics/0900-ai-quota-marketplace.md

# stake as a deployed mechanism and as an open question
grep -n 'Stake requirement' rfcs/accepted/economics/0900-ai-quota-marketplace.md
grep -n 'Minimum stake for sellers' rfcs/accepted/economics/0900-ai-quota-marketplace.md

# refund specified, omitted, and open
grep -n 'DISPUTED --> REFUNDED' rfcs/accepted/economics/0900-ai-quota-marketplace.md
grep -n 'How to handle failed requests' rfcs/accepted/economics/0900-ai-quota-marketplace.md
grep -n 'the refund has no representation' crates/quota-router-core/src/marketplace/escrow.rs

# telemetry, absent
grep -rn 'MarketTelemetry' --include=*.rs crates/ || echo "ABSENT"
grep -niE 'telemetry' --include=*.rs crates/quota-router-core/src/marketplace/ | head

# the sound part
ls crates/quota-router-storage/migrations/ | grep v015
grep -n 'cross_chain_same_provider_two_distinct_rows' \
  crates/quota-router-storage/src/slash_store.rs
grep -n 'does NOT expose a native Dqa driver' \
  rfcs/accepted/economics/0900-ai-quota-marketplace.md \
  crates/quota-router-storage/migrations/v015__chain_aware_slash_ledger.sql
```

## Cross-references

- Mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`
  — the same absence from RFC-0959's side; nothing currently links the two
- Mission `a-disputed-escrow-with-no-arbitrator-can-never-be-resolved` — the
  dispute machine's other gap; `PARTIAL` and the arbitrator
- Mission `the-marketplace-has-no-stake-gate-and-the-role-it-would-gate-on-is-not-in-the-role-registry`
  — the code side of §Open Questions 2
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  `MarketTelemetry` joins
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27 reaches the same Stoolap Dqa-driver conclusion independently, which is
  corroboration rather than coincidence
