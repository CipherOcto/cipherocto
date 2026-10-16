---
name: the-same-ask-has-two-different-content-addresses
description: "RFC-0959 defines ask_id = BLAKE3(canonical_ser(AskUnsignedPayload)) as the content address in the wire/storage form, stated three times and marked an invariant. The implementation computes it two incompatible ways. AskSigned.ask_id and AskUnsignedPayload.ask_id hash the canonical payload, matching the RFC. Ask::id, used as the database primary-key-equivalent and as the in-memory index key, hashes asker_did || model_wire || axes_hash || nonce instead. Measured on one Ask: signed e244e07d6f6f..., storage 5a257384ac50..., unequal. No conversion exists between AskSigned or AskUnsignedPayload and Ask anywhere in the tree, so the two addresses are never reconciled and nothing could reconcile them. The quota-router-core copy of the storage derivation is a third implementation of the same formula, currently in agreement."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0853
---

# Mission `the-same-ask-has-two-different-content-addresses`

## Status

Deferred — this is a Layer A content-addressing decision. Changing either
derivation invalidates every stored `ask_id`, and the RFC states the invariant
in terms of the payload, so the storage side is the one that would have to
change. Nothing was actioned; the measurement probe was removed and the tree
matches its prior state.

## Finding

RFC-0959 specifies **one** `ask_id`. The implementation computes **two**,
and they are not equal.

### What the RFC specifies

Stated three times, and once as an explicit invariant:

```text
`Ask` primitive — content-addressable, signed, multi-axis price quote.
`ask_id = BLAKE3(canonical_ser(AskUnsignedPayload))`.
```

```rust
// Wire/storage form. `ask_id = BLAKE3(canonical_ser(AskUnsignedPayload))`.
pub ask_id: AskId,   // BLAKE3(canonical_ser(AskUnsignedPayload))
```

```text
**Invariant:** `ask_id = BLAKE3(canonical_ser(AskUnsignedPayload))`. Hash
preimage = `AskUnsignedPayload` only — never `Ask` (which contains the
signature). Non-circular; signature-stable.
```

Note "**Wire/storage** form". One address, both sides. The invariant section
is careful about which type may appear in the preimage, and the answer it
gives is `AskUnsignedPayload` — including for storage.

### What the implementation computes

| Surface                                             | Preimage                                             | RFC-compliant |
| --------------------------------------------------- | ---------------------------------------------------- | ------------- |
| `AskUnsignedPayload::ask_id()` / `AskSigned.ask_id` | `BLAKE3(canonical_ser(self))`                        | **yes**       |
| `Ask::id()`                                         | `BLAKE3(asker_did ‖ model_wire ‖ axes_hash ‖ nonce)` | **no**        |

`Ask::id()` is what `StoolapAskRepository` writes as the `asks.ask_id` column
and what `MarketplaceIndex` keys its `BTreeMap` on. So the RFC-compliant
address is the one that goes on the wire, and the non-compliant one is the one
that is stored and indexed.

### Measured

One `Ask`, built and signed the way `ask publish` builds it:

```text
signed_id   (AskSigned.ask_id, on the wire) = e244e07d6f6fd47161be101fed5b7785f23069d7355e5fc054f821a171fa7f41
storage_id (Ask::id, DB + index key)        = 5a257384ac50dd551407b31a61738289b6bb1c3cd6350c5bdadedfd5d43ad122
equal = false
```

## Nothing reconciles them

There is no conversion from `AskSigned` or `AskUnsignedPayload` into `Ask`
anywhere in the tree — no `From`, no `TryFrom`, no `into_ask`. The only way
to construct a storage-form `Ask` is to call `Ask::new` with five of the
eight payload fields, which is exactly the manual, lossy projection described
in mission `rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields`.

So the two addresses are never compared, and no code exists that could compare
them. A peer that receives a signed Ask carrying `e244e0…` and asks the local
index for it will look for `5a2573…` and find nothing.

### The consequence for settlement

`SettlementEnvelope.ask_id` is a 32-byte field, and `SettlementEvent.ask_id`
likewise. Neither carries a discriminator saying which derivation produced it.
Once a settlement names an Ask, the id in the receipt and the id in the
`asks` table are the same bytes meaning different things, and nothing in the
crate can tell whether a settlement's `ask_id` refers to a real stored Ask.

This sits directly underneath the settlement chain measured in
`settlement-hash-does-not-bind-the-cost`: the hash binds `ask_id`, but which
Ask that id names is itself ambiguous.

## A third copy of the storage derivation

`quota-router-core::marketplace::compute_ask_id_static` re-implements the
storage-side formula:

```rust
msg = asker_did ‖ model_wire ‖ axes_hash ‖ nonce
```

where `axes_hash()` is `BLAKE3(serde_json::to_vec(&ask.rates))` — byte-identical
to what `Ask::id()` computes inline. The two currently agree, so this is a
parallel-abstraction risk rather than a live defect: a future change to one
copy silently desynchronises the hydration path from the storage path, and
nothing would fail because no test compares them.

That is worth flagging separately from the divergence above. Two copies
agreeing today is an accident of implementation, not an enforced property.

## Why no test catches it

The AskId tests check internal consistency of the _signed_ representation:

```rust
assert_eq!(signed.ask_id, payload.ask_id());
assert_eq!(s1.ask_id, payload.ask_id());   // stable across re-sign
```

Both are correct and both are about `canonical_ser(payload)`. Neither compares
against `Ask::id()`, because the two types are never brought together — there
is no conversion to write the test with.

That is the same pattern as several other findings in this audit: a check that
holds within one representation, named as though it held across the system.
The property that actually matters is the cross-boundary one, and it is not
expressible until the two types meet.

There is also no RFC test vector to fail against. RFC-0959 §Test Vectors
gives the inputs and writes `= [32-byte digest — implementer computes]`, so
there is no byte-exact expected value for either derivation. A wrong
derivation on either side is therefore untestable against the specification as
written, independent of this audit finding it.

## What the RFC owner needs to decide

- Should `Ask::id()` be replaced by `BLAKE3(canonical_ser(AskUnsignedPayload))`,
  matching the RFC invariant and the wire form? This is the direct question.
  It invalidates every stored `ask_id` and every fixture.
- If not, should the RFC be amended to specify two addresses with distinct
  names and a stated relationship? "Wire/storage form" currently promises
  one. An amendment is legitimate; the present text is simply not what the
  code does.
- Should the storage form hash the _canonical payload_ or a structured
  concatenation? See §A collision attempt that did not reproduce — the
  concatenation is delimiter-free in principle, and the evidence gathered
  here is that `to_wire()` canonicalisation probably closes it, but that was
  not established either way.
- Should `SettlementEnvelope.ask_id` and `SettlementEvent.ask_id` record
  which derivation they use, or should the ambiguity be removed before
  settlement depends on it?
- Should `compute_ask_id_static` in `quota-router-core` be replaced by a call
  to `Ask::id()`? It is a duplicate of a Layer A formula in a Layer B crate.

## A collision attempt that did not reproduce

`Ask::id()` concatenates `asker_did ‖ model_wire ‖ axes_hash ‖ nonce` with no
delimiters and no length prefixes. Both `asker_did` and `model_wire` are
variable-length, and `asker_did` is an arbitrary non-empty string, so in
principle a boundary could be shifted between them to produce the same
preimage for two different Asks.

That reasoning was tested and **failed**, so the concern is recorded as
unresolved rather than asserted:

```text
honest  did:octo:x        model=openai/gpt-4
forged  did:octo:xopenai/g model=pt-4
concatenation equal = false
honest  id = 6039a2862af1c35681ee23bb53699dc61e8ee72f24d1d266e5ce60e822abfbfc
forged  id = 9b85853fd0f4c858785bc12d92843874d28643221951e2c159fd3e0ce712c75c
COLLIDE = false
```

The reason is `to_wire()`. It does not echo the input string; it reconstructs
`"{namespace}/{family}"` or `"{namespace}/{family}/{version}"`. And
`ModelRef::from(&str)` routes through `parse_lenient`, so the second model's
single segment becomes `namespace="" , family="pt-4"` and its wire form is
`"/pt-4"` — which inserts a separator that breaks the intended ambiguity.

So the canonicalisation plausibly closes the gap. That is **not** a proof:
the attempt tried one shift, and a delimiter-free concatenation is not
obviously safe in general. Establishing it either way is a small piece of work
— try the shift on each variable-length boundary, including the version
segment — and it belongs with the RFC decision rather than inside this audit.

What can be said without that: the wire-side derivation hashes a canonical
serialisation and carries no such ambiguity, and it is the derivation the RFC
specifies.

## Reproduction

```bash
# the RFC's single address, stated as wire/storage form and as an invariant
grep -n 'ask_id = BLAKE3(canonical_ser(AskUnsignedPayload))' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the two implementations
grep -n -A14 'pub fn id(&self)' crates/quota-router-storage/src/ask.rs
grep -n -A6 'pub fn ask_id(&self)' crates/quota-router-storage/src/ask.rs

# the storage id is what is persisted and indexed
grep -n 'ask_id: ask.id()' crates/quota-router-storage/src/ask_repo.rs
grep -n 'let id = ask.id();' crates/quota-router-storage/src/marketplace.rs

# no conversion exists between the representations
grep -rn 'impl From<AskSigned>\|impl From<AskUnsignedPayload>\|impl TryFrom<AskUnsignedPayload>' \
  --include=*.rs crates/ || echo none

# the third copy in the facade
grep -n -A10 'fn compute_ask_id_static' crates/quota-router-core/src/marketplace/mod.rs

# the tests that stay inside one representation
grep -n 'signed.ask_id, payload.ask_id()' crates/quota-router-storage/src/ask.rs

# no expected digest to fail against
grep -n '32-byte digest' rfcs/accepted/economics/0959-ask-settlement-chain.md | head
```

## Cross-references

- RFC-0959 §Data Structures — the `ask_id` invariant and the "wire/storage
  form" comment that promises a single address
- RFC-0959 §Test Vectors — supplies inputs but no expected digest, so neither
  derivation can be validated against the specification
- RFC-0853 §Networking — BLAKE3 as the `ask_id` primitive
- Mission `rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields`
  — the missing conversion is the same three-field projection; this mission is
  what that projection does to the content address
- Mission `the-ask-signature-gates-nothing` — the signed envelope never
  reaches storage, which is why no bridge exists for its id either
- Mission `settlement-hash-does-not-bind-the-cost` — the settlement hash binds
  `ask_id`, whose referent is ambiguous for the same reason
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the
  duplicate-implementation risk in a different subsystem
