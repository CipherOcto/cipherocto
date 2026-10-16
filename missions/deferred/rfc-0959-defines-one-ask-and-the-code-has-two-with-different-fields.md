---
name: rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields
description: "RFC-0959 defines a single Ask type with eight fields. The implementation has two: AskUnsignedPayload with all eight, and Ask with five. Three declared fields are dropped at the mint boundary - node_type, jurisdiction and published_at_unix - and the type every reader uses is the one that lost them. Each drop independently contradicts something the RFC specifies: jurisdiction loses the 451 gate, published_at_unix makes the soft_delay warm-up unimplementable, and node_type is stated to remain unchanged as persisted storage layout while appearing in no migration at all. The AskUnsignedPayload doc further claims node_type gates capability-class minting per RFC-0958 and informs the index's jurisdiction decisions. Both types carry a doc header claiming RFC-0959 conformance, which is what makes a three-field divergence readable as compliant."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
    - RFC-0958
---

# Mission `rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields`

## Status

Deferred — the fix is a type decision, not a patch. Either `Ask` absorbs the
three fields (which moves `ask_id`, changes the `asks` schema, and re-opens
the RFC-0959 vs RFC-0009 DID question) or `AskUnsignedPayload` is retired and
the selection path is re-typed on the payload form. Both are RFC-owner calls
with wire and storage consequences. Nothing was actioned; no measurement probe
was needed, since the field accounting is structural.

This is a synthesis of three separate findings. It exists because the three
share one cause, and an owner who fixes the cause fixes all three.

## Finding

RFC-0959 §Data Structures defines **one** Ask. The implementation has **two**,
with different fields, and the one every reader uses is the smaller.

| Field               | `AskUnsignedPayload` | `Ask`  | Note                                  |
| ------------------- | -------------------- | ------ | ------------------------------------- |
| `asker_did`         | yes                  | yes    |                                       |
| `model`             | yes                  | yes    |                                       |
| `rates`             | yes                  | yes    |                                       |
| `nonce`             | yes                  | yes    |                                       |
| `ttl_unix`          | yes                  | —      | renamed to `expires_at_unix` in `Ask` |
| `jurisdiction`      | **yes**              | **no** | dropped                               |
| `published_at_unix` | **yes**              | **no** | dropped                               |
| `node_type`         | **yes**              | **no** | dropped                               |

Eight declared, five kept, three dropped. (`ttl_unix` → `expires_at_unix` is a
rename, not a loss — both docs read "inclusive", and both the struct and the
table describe an expiry instant.)

So an Ask is minted and signed with eight fields, and everything downstream —
storage, selection, pricing, the in-memory index — operates on five of them.

### Why the split happened

`AskUnsignedPayload` is the pre-mint form: what gets hashed, signed, and put
on the wire. `Ask` is the post-mint form: what the index and the repository
hold.

`AskUnsignedPayload` is where the validation lives. Its constructor is what
rejects an empty jurisdiction — the reason `ask publish` refuses to run without
`--jurisdiction`. `Ask::new` validates `asker_did`, `model` and `nonce`, and
cannot validate the other two because it has no such fields.

That is the crux: **the validated type is the discarded one.** Everything an
asker is told is checked, happens on the eight-field struct. Everything that
enforces anything operates on the five-field struct. Neither is wrong in
isolation; the gap is entirely in the transition, and nothing in the
transition says it is lossy.

## What each drop costs

### `jurisdiction` → the 451 gate

Measured: an Ask signed `["EU"]` is returned by `select_ask` for a `["US"]`
caller. `select_ask` calls `jurisdiction_matches(&[], …)` with a hardcoded
empty declared list, so the correct filter function runs and cannot take its
rejecting branch. `JurisdictionMismatch` is declared and never constructed; no
451 path exists. Full evidence in mission
`an-ask-declared-for-one-jurisdiction-is-served-to-any-other`.

This is the most exposed of the three, because the CLI _compels_ the input:
an operator must declare a jurisdiction, and is told so by validation, and the
declaration is then discarded.

### `published_at_unix` → the soft_delay warm-up

`Published --> Active: now >= published_at + soft_delay` is a gate in the
§Ask state machine, and `soft_delay` appears nowhere in any `.rs` file. It
cannot be implemented while `Ask` lacks `published_at_unix` — there is no
clock to measure the delay against. An Ask is selectable the instant it
enters the index. Evidence in mission
`the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`.

### `node_type` → contradicted persistence

This one is the weakest of the three, and the honest reading matters.
RFC-0959's IA-7 amendment (v1.2, 2026-10-07) states `Ask.node_type` is "a
derived projection, **not an authorization input**", and retargets it to
RFC-0871. So dropping it does not disable an authorization control — there
was not meant to be one.

But the same amendment says `Ask.node_type` and its `#[repr(u8)]` encoding
"remain unchanged as **persisted storage layout**", and characterises the
amendment as "not a wire or storage break". `node_type` appears in **no
migration in the tree**, and in neither `Ask` nor `AskRow`. A field the RFC
says is persisted is not persisted anywhere.

The `AskUnsignedPayload` doc makes two further claims that do not hold:

- that `node_type` "gates downstream capability-class minting per RFC-0958
  §NodeType Gating Matrix" — the capability registry does gate Wholesale
  against ZKBearing, but it is keyed on a caller-supplied `node_did` in local
  state and is never sourced from an Ask;
- that it "informs the marketplace index's jurisdiction/routing decisions" —
  the index has neither field.

So the doc asserts an authorization role the RFC explicitly withdrew, and a
routing role the index cannot have.

## The pattern worth naming

This is the **first structural cause** the audit has produced, where three
independent findings reduce to one decision.

The recurring shape across this marketplace is not "a control is missing". It
is: **a value is validated or signed on one type, and consumed on a narrower
one, with nothing asserting the two agree.**

The same shape has now produced:

| Value               | Validated/signed on             | Consumed on         | Control lost          |
| ------------------- | ------------------------------- | ------------------- | --------------------- |
| `jurisdiction`      | `AskUnsignedPayload` (8 fields) | `Ask` (5)           | 451 jurisdiction gate |
| `published_at_unix` | `AskUnsignedPayload`            | `Ask`               | soft_delay warm-up    |
| `node_type`         | `AskUnsignedPayload`            | `Ask`               | persistence claim     |
| `signature`         | `AskSigned`                     | dropped entirely    | all attestation       |
| `asker_did` binding | `AskSigned` (signed over)       | unbound in `verify` | identity binding      |

The last two are in
`the-ask-signature-gates-nothing` and
`a-signed-ask-does-not-prove-who-published-it`. They are the same failure
generalised past the type boundary: the signature exists on `AskSigned`, and
the `Ask` that reaches storage has no field to carry it.

An audit that reads one function at a time finds these individually. They are
only visible as a class when someone enumerates the fields.

## Why this was not fixed

Three reasons, all dispositive:

1. **It moves `ask_id`.** `ask_id = BLAKE3(canonical_ser(payload))`. Adding
   fields to `Ask` changes every content address and invalidates every
   published Ask, every stored row, and every test fixture.
2. **It needs a schema change either way.** The `asks` table and `AskRow` both
   lack all three fields.
3. **The direction is an architecture decision.** Either the post-mint `Ask`
   grows, or the selection path is re-typed onto `AskUnsignedPayload` and the
   narrower struct is retired. The second is cleaner — the RFC has one Ask —
   but it touches every consumer and re-opens the jurisdiction question from
   `an-ask-declared-for-one-jurisdiction-is-served-to-any-other`, where empty
   is currently invalid in one place and wildcard in another.

## What the RFC owner needs to decide

- Should the two types be merged? RFC-0959 defines one Ask, and a single type
  makes the class of bug above unrepresentable. What are the concrete reasons
  for the split, and do any survive?
- If the split stays, what mechanism asserts that a post-mint `Ask` is a
  faithful projection of its payload? A conversion function that returns
  `Result` and rejects an unmigratable payload, or a `From` impl plus a
  round-trip test, would have caught all three drops.
- Should `Ask::new` gain the jurisdiction validation that currently lives only
  on the payload constructor? If the field is not carried, the check should
  not be silently discarded with it.
- Is `node_type` still meant to be persisted storage layout, or was that
  sentence left over from before IA-7? One of the two claims is now false.
- Should `AskUnsignedPayload`'s doc stop asserting that `node_type` gates
  capability minting? RFC-0009 v1.2 says mint authority keys on the in-boundary
  inference attestation payload kind, "not on a role name", and the capability
  registry does gate on a role name.

## Reproduction

```bash
# the two types, both claiming RFC-0959
grep -n -B2 -A10 'pub struct Ask {' crates/quota-router-storage/src/ask.rs
grep -n -B8 -A12 'pub struct AskUnsignedPayload {' crates/quota-router-storage/src/ask.rs

# Ask::new has no jurisdiction, node_type or published_at_unix parameter
grep -n -A8 'pub fn new(' crates/quota-router-storage/src/ask.rs | grep -A8 asker_did

# AskRow and the table lack all three
grep -n -A10 'pub struct AskRow' crates/quota-router-storage/src/ask_repo.rs
cat crates/quota-router-storage/migrations/v001__create_asks_table.sql

# jurisdiction: filter cannot reject, error never constructed
grep -n -B2 -A3 'jurisdiction_matches(&\[\]' crates/quota-router-storage/src/marketplace.rs
grep -rn 'JurisdictionMismatch' --include=*.rs crates/

# published_at_unix: soft_delay unimplemented anywhere
grep -rn 'soft_delay' --include=*.rs crates/

# node_type: in no migration at all
grep -rn 'node_type' crates/*/migrations/*.sql || echo none

# the RFC's own persistence claim and the IA-7 amendment
grep -n 'persisted storage layout' rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n 'IA-7' rfcs/accepted/economics/0959-ask-settlement-chain.md | head -3
```

## Cross-references

- RFC-0959 §Data Structures — the single Ask definition, eight fields
- RFC-0959 §Ask state machine — the soft_delay gate the `published_at_unix`
  drop makes unimplementable
- RFC-0959 IA-7 — the amendment withdrawing `node_type` as an authorization
  input while retaining the persistence claim
- RFC-0009 §Node — retargeted to RFC-0871, the citation on the dropped field
- Mission `an-ask-declared-for-one-jurisdiction-is-served-to-any-other` — the
  measured consequence of the `jurisdiction` drop
- Mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`
  — the `published_at_unix` drop and the state machine it disables
- Mission `the-ask-signature-gates-nothing` — the same class past the type
  boundary: a value present on one type and absent from the next
- Mission `a-signed-ask-does-not-prove-who-published-it` — the `asker_did`
  row of the same table
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the
  duplication pattern in a different shape; here the duplication is between a
  spec and a type
