---
name: an-ask-declared-for-one-jurisdiction-is-served-to-any-other
description: "RFC-0959 defines jurisdiction as an Ask field, requires select_ask to filter on it, and specifies JurisdictionMismatch returning HTTP 451 (Unavailable For Legal Reasons). The implementation has no jurisdiction field on Ask at all, no jurisdiction parameter on Ask::new, and no jurisdiction column in the asks table or AskRow. select_ask hardcodes jurisdiction_matches with an empty declared list, so the filter always returns true. Measured: an Ask signed with jurisdiction [EU] is returned by select_ask for a [US] caller. JurisdictionMismatch is declared and never constructed, no 451 appears anywhere, and jurisdiction appears zero times in the facade marketplace module. Meanwhile the CLI makes --jurisdiction mandatory, so every operator must declare a restriction that is then discarded. The RFC's own implementation checklist lists a jurisdiction gate among the required unit tests."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
---

# Mission `an-ask-declared-for-one-jurisdiction-is-served-to-any-other`

## Status

Deferred — carrying jurisdiction requires a schema change (the `asks` table
and `AskRow`), an `Ask` field, and a construction-path change, and it changes
`ask_id` because the id is a hash over the canonical payload. It also needs a
decision the RFC does not settle: whether an Ask with an empty jurisdiction
means unrestricted or invalid, since `AskUnsignedPayload::new` currently
rejects empty and selection treats empty as wildcard. Nothing was actioned;
the measurement probe was removed and the tree matches its prior state.

## Finding

The jurisdiction restriction an asker signs is not enforced anywhere, and
cannot be: the field does not exist on the type that selection reads.

### What the RFC requires

Four separate places, and it is not ambiguous in any of them:

1. §Data Structures defines it on the Ask:
   `pub jurisdiction: BTreeSet<String>, // ISO-3166 alpha-2 codes`
2. §Roles specifies the index's authority as
   `select_ask(did, model, jurisdiction, budget_ceiling)` — jurisdiction is
   one of four filter inputs.
3. A dedicated error and a specific status code:
   `| JurisdictionMismatch | Yes | Router returns HTTP 451 to client;
client relocates request through appropriate jurisdiction gateway |`
4. Its own implementation checklist: "Add unit tests: compute_cost overflow
   guard, **jurisdiction gate**, cache classification, ...".

HTTP 451 is _Unavailable For Legal Reasons_. This is a regulatory control,
not a routing preference.

### What the implementation has

Nothing, at every layer:

| Layer                     | Jurisdiction?                                                     |
| ------------------------- | ----------------------------------------------------------------- |
| `AskUnsignedPayload`      | yes — validated non-empty, signed over, present in the envelope   |
| `Ask` struct              | **no field**                                                      |
| `Ask::new`                | **no parameter** — there is no constructor path that could set it |
| `AskRow`                  | **no field**                                                      |
| `asks` table (v001)       | **no column**                                                     |
| `select_ask`              | hardcodes `jurisdiction_matches(&[], …)` → always true            |
| facade marketplace module | **zero occurrences** of the string                                |
| `JurisdictionMismatch`    | declared, **never constructed**                                   |
| HTTP 451                  | **absent from the tree**                                          |

The filter function `jurisdiction_matches` exists and is correct. It is called
with a hardcoded empty `declared` list, which its own body treats as the
wildcard, so the branch can never be taken. It is dead weight that reads like
an implemented control.

## Measurement

The signed path is real. `octo-wallet ask publish --jurisdiction EU` — the
flag is **mandatory**, the CLI rejects an empty list — produces:

```text
signed envelope jurisdiction : ['EU']
fields on the signed payload : ['asker_did', 'jurisdiction', 'model',
                                'node_type', 'nonce', 'published_at_unix',
                                'rates', 'ttl_unix']
```

That payload, converted to the `Ask` the index holds:

```text
signed jurisdiction = ["EU"]
Ask { asker_did: "did:octo:probe_seller",
      model: ModelRef { namespace: "openai", family: "gpt-4", version: None },
      rates: ModelRateTable { … },
      nonce: [66, 66, …],
      expires_at_unix: 1900000000 }

select_ask(jurisdiction=[US]) -> Some (SELECTED)
```

An Ask explicitly restricted to the EU is served to a US caller. The
restriction is not merely unenforced — it is absent from the type, so there is
no code that could enforce it.

## The shape of the gap: a mandatory input that is discarded

The most misleading part is not that the gate is missing. It is that the
interface **compels the operator to supply it**.

`AskUnsignedPayload::new` rejects an empty jurisdiction, and `ask publish`
refuses to run without `--jurisdiction`. Every published Ask therefore carries
a declared, signed, operator-chosen restriction — and every one of them is
discarded at the mint boundary.

An operator who believes they have excluded a jurisdiction has been told so
by the tool's own validation. That is worse than a missing feature: it is an
interface that collects a compliance-relevant decision and reports success.

## Why the existing test does not catch it

`select_ask_jurisdiction_filter_structural` is the only jurisdiction test in
the tree, and it passes. It documents the structural gap honestly in its
comment — "future versions that store jurisdiction on the post-mint form will
tighten the filter" — so this is not a hidden defect.

But it does not test the case that matters. The Ask it inserts has **no
jurisdiction at all**, and it then queries with `jurisdiction = ["EU"]` and
asserts the Ask is selected. That is the wildcard contract working correctly:
an unrestricted Ask matches any jurisdiction.

So the test covers the one input where "always true" is the right answer, and
names itself after the gate that is missing. A restricted Ask is never
inserted, so the filter is never exercised against anything that could fail
it. The RFC's required "jurisdiction gate" test does not exist in any form.

## Consequence

An asker who restricts an Ask to one jurisdiction gets it selected for callers
in every other jurisdiction. The RFC's prescribed response — 451 and
relocation through the appropriate gateway — cannot occur, because the error
variant is never constructed and no 451 path exists.

This is a compliance control that is specified three ways, collected from
every operator, and enforced in none. It is worth separating from the other
structural gaps in this audit: the Ask lifecycle and the settlement-hash
findings are integrity and economics problems that need an RFC decision before
they bite. This one collects a declaration from users today, so the exposure
begins as soon as anything else is wired.

## What the RFC owner needs to decide

- Does `Ask` gain a `jurisdiction` field, and if so does `ask_id` change? The
  id is a hash over the canonical payload, so a jurisdiction-restricted Ask
  would have a different id from an unrestricted one with the same rates —
  probably correct, but it invalidates anything already published.
- What does an empty `jurisdiction` mean at selection time? Today the
  constructor rejects empty and selection treats empty as wildcard, so the
  two disagree about the same value. That needs one answer, not two.
- Should the `asks` table gain a `jurisdiction_json` column, or should the
  filter move to the payload form the repository does not currently hold?
- Should `jurisdiction_matches` stay if it cannot be reached? Either the call
  site gains a real `declared`, or the function is removed — a filter that is
  always true is a filter someone will read as implemented.
- Should `JurisdictionMismatch` be constructed, and should a 451 path exist
  at the router? If jurisdiction is a legal control rather than a routing
  hint, the error variant should be reachable or removed.
- Was jurisdiction ever intended to be enforced? Nothing in the code says no;
  the RFC says yes four times. If it was descoped, that decision should be
  recorded where an operator reading `--jurisdiction` would find it.

## Reproduction

```bash
# the CLI makes jurisdiction mandatory
./target/debug/octo-wallet ask publish --node-type wholesale \
  --model openai/gpt-4 --ttl-unix 1900000000 --seed ./seed.bin \
  --axes input_tokens_per_1k:30000 --nonce-hex 42424242424242424242424242424242
# error: jurisdiction list is empty (at least one required)

# the signed envelope carries it
./target/debug/octo-wallet ask publish ... --jurisdiction EU | jq .payload.jurisdiction

# Ask has no jurisdiction field and Ask::new no jurisdiction parameter
grep -n -A10 'pub struct Ask {' crates/quota-router-storage/src/ask.rs
grep -n -A8 'pub fn new(' crates/quota-router-storage/src/ask.rs | grep -A8 asker_did

# the filter is called with a hardcoded empty declared list
grep -n -B2 -A3 'jurisdiction_matches(&\[\]' crates/quota-router-storage/src/marketplace.rs

# absent from storage
grep -n -A10 'pub struct AskRow' crates/quota-router-storage/src/ask_repo.rs
grep -c jurisdiction crates/quota-router-storage/src/ask_repo.rs

# the error variant is never constructed; 451 is absent
grep -rn 'JurisdictionMismatch' --include=*.rs crates/
grep -rn '451' --include=*.rs crates/quota-router-core/src/ crates/quota-router-storage/src/

# the RFC's four requirements
grep -n 'jurisdiction' rfcs/accepted/economics/0959-ask-settlement-chain.md

# the test that names the missing gate and does not reach it
grep -n -A22 'fn select_ask_jurisdiction_filter_structural' \
  crates/quota-router-storage/src/marketplace.rs
```

## Cross-references

- RFC-0959 §Data Structures — `jurisdiction: BTreeSet<String>` on the Ask
- RFC-0959 §Roles — `select_ask(did, model, jurisdiction, budget_ceiling)`
- RFC-0959 §Implementation checklist — the required jurisdiction gate test
- Mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`
  — the same structural pattern: a post-mint field dropped from the type
  selection reads, which also disabled the `soft_delay` gate
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — records
  that the facade's ranking implements neither jurisdiction nor
  `budget_ceiling`, so there is no second filter to fall back to
- Mission `ask-publish-fails-with-its-own-default-nonce` — the other defect
  the CLI's validation imposes on operators
- Mission `marketplace-accepts-a-listing-whose-did-cannot-be-parsed` — the
  adjacent identity shape on the same type: `Ask::new` accepts any non-empty
  `asker_did`, so the publisher side is unvalidated there too
