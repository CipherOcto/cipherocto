---
name: rfc-0900-specifies-a-listing-lifecycle-that-does-not-exist-anywhere
description: "RFC-0900 section State Machines draws three lifecycle diagrams. The purchase and dispute machines are implemented, with their gaps documented in the escrow enum's own doc comments. The listing lifecycle is not: ListingState appears in no crate, and the marketplace module holds escrow, orderbook, scoring, slashing and reputation compatibility with no listing entity at all. Two further internal contradictions sit in the same document. Section Security lists a stake requirement as a deployed security mechanism while Open Questions still asks what the minimum stake should be, and the code has no stake gate. The purchase diagram specifies a DISPUTED to REFUNDED transition while Open Questions asks how refunds work, and the escrow's Slashed variant carries no refund representation. The Observability section specifies a MarketTelemetry interface enumerating what must never be logged, and no telemetry struct exists in any crate. The slash ledger substrate in the same document is sound at the storage layer and recorded here as such; its in-memory mirror block disagrees with the struct it mirrors on exactly the field the RFC had argued must be integer."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.2"
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

## v1.2 — the in-memory mirror is wrong on the field the RFC argued hardest about

v1.1 called this section sound. That verdict holds for the storage layer and
every migration claim, all of which I re-verified independently. It does not
hold for the **in-memory mirror**, the code block the RFC presents as the shape
backing §Slashing Model. That block is wrong twice, and wrong on exactly the
field the RFC had made an argument about.

The RFC's storage invariant is explicit about why the loss column is an integer:

> **`cumulative_loss_pct_micro` BIGINT** — not amount-bearing; encoded as
> integer micro-percent (1e6) to keep the column Eq-comparable **without f64
> round-trip ambiguity**. 500_000 = 50.0000%.

That invariant **holds at the storage layer**. The migration declares
`cumulative_loss_pct_micro BIGINT NOT NULL`, and the storage row type is
`cumulative_loss_pct_micro: u64`. The DB comparison the RFC wanted is intact.

The mirror block then specifies the in-memory shape as

```rust
cumulative_loss_pct_micro: u64,
last_updated_unix: u64,
```

and the actual `ProviderStake` in `quota-router-core/src/marketplace/slashing.rs`
has neither. It has `cumulative_loss_pct: f64`, and no `last_updated_unix` at
all — six fields where the block specifies seven.

So the f64 the RFC set out to avoid is present, just one layer up from where the
RFC thought it was excluded. The row is read as
`cumulative_loss_pct_micro as f64 / 1_000_000.0` and the permanent-ban test is
`self.cumulative_loss_pct >= rules.permanent_ban_at`, both `f64`.

Whether this is a live defect is a narrower question than it first looks, and
worth answering exhaustively rather than by inspection. I tested the whole
representable domain rather than the one boundary the RFC names. Over every
`cumulative_loss_pct_micro` from 0 to 1,000,000:

- `500_000 / 1e6 == 0.5` **exactly**, as the RFC's stated boundary requires.
- **Zero** values below the threshold compare `>= 0.5` — no false ban.
- **Zero** values at or above the threshold fail to compare `>= 0.5` — no missed
  ban.
- **Zero** values fail to round-trip through `f64` and back.

So the `f64` is **behaviourally harmless across its entire domain**. The defect is
narrower than it looks and should be recorded at its actual size: the RFC's
in-memory block describes a struct that does not exist, and the type no longer
carries the property that made it Eq-comparable and free of round-trip
ambiguity. The comparison the f64 was meant to endanger produces correct answers
for every input it can receive. This is a documentation and typing defect, not a
correctness defect, and calling it a correctness defect would overstate it.

**What does not change:** v1.1's finding about `chain_id` being a nullable column
inside a UNIQUE index rather than a PK constraint still stands, and every v015
step the RFC claims is present in the migration. The slash ledger remains the
best-specified part of RFC-0900. This is a qualification of one code block, not
a reversal of the section's verdict.

`SlashingRules` was also re-checked while measuring this: `first_offense_penalty:
0.10`, `offense_multiplier: 1.5` and `permanent_ban_at: 0.50` are all exactly as
the RFC specifies. The implementation adds one field the RFC does not mention,
`miss_rate_tolerance`, defaulted to `0.0`. That is an addition, not a
contradiction.

## v1.1 — the slash ledger section is sound, but its anchor is unreachable

The section below is the best-specified part of RFC-0900 and it is implemented
as written. One qualification belongs with that praise: it anchors itself twice
to **§20.3 Model B**, and RFC-0900 has no numbered sections at all. §20.3 is a
section of `docs/reviews/`, which `.gitignore` excludes in its entirety and
which BLUEPRINT mandates be ephemeral and uncommitted. The normative anchor for
this substrate specification is therefore invisible to any reader who does not
happen to have that scratchpad on their machine.

Worked through in `rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad`.
The implementation findings below stand; the citation does not resolve.

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

**A second qualification, added in v1.2:** the section's _in-memory mirror_ block
disagrees with the struct it mirrors on `cumulative_loss_pct_micro` / `f64`, and
omits `last_updated_unix`. See §v1.2 above. The storage-layer verdict below is
unaffected.

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

# v1.2 — storage layer is integer, as the RFC's invariant requires
grep -n 'cumulative_loss_pct_micro' \
  crates/quota-router-storage/migrations/v012__create_slash_ledger.sql
grep -n 'pub cumulative_loss_pct_micro' \
  crates/quota-router-storage/src/slash_store.rs

# ...and the in-memory struct it mirrors is f64, with one field fewer
awk '/^pub struct ProviderStake \{/,/^\}/' \
  crates/quota-router-core/src/marketplace/slashing.rs \
  | grep -oE '^    pub [a-z_]+' | tr -d ' '
# 6 fields, cumulative_loss_pct (f64), no last_updated_unix

# the conversion that reintroduces the f64 the RFC excluded
grep -n 'cumulative_loss_pct_micro as f64' \
  crates/quota-router-core/src/marketplace/slashing.rs
grep -n 'self.cumulative_loss_pct >= rules.permanent_ban_at' \
  crates/quota-router-core/src/marketplace/slashing.rs

# the rule constants are exact as specified
grep -nE 'first_offense_penalty: 0\.|offense_multiplier: 1\.5|permanent_ban_at: 0\.5' \
  crates/quota-router-core/src/marketplace/slashing.rs

# ...and the f64 is harmless across its WHOLE domain, not just the boundary.
# Answer all four: exact boundary, no false ban, no missed ban, no round-trip loss.
python3 - <<'PY'
def d(n):
    return n / 1_000_000.0

print("boundary exact:", d(500000) == 0.5)
print("false bans below threshold:", sum(1 for n in range(500000) if d(n) >= 0.5))
print("missed bans at/above threshold:",
      sum(1 for n in range(500000, 1000001) if not d(n) >= 0.5))
print("round-trip losses:",
      sum(1 for n in range(1000001) if round(d(n) * 1_000_000) != n))
PY
# expected: True / 0 / 0 / 0
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
