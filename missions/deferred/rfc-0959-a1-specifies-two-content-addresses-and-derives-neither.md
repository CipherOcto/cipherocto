---
name: rfc-0959-a1-specifies-two-content-addresses-and-derives-neither
description: "RFC-0959-A1 section Determinism Requirements specifies two Class-A content addresses with precise formulas: event_hash as BLAKE3 over canonical_ser of the DealSettledPayload with seller_signature and event_hash zeroed, and envelope_id as BLAKE3 over canonical_ser of MarketDeliveryEnvelopePreimage with envelope_id zeroed. Neither is implemented. The market_delivery module contains no BLAKE3 call and no canonical_ser at all, and no From conversion from an envelope to its preimage, so the formula has no entry point. What exists is the preimage struct itself, holding a public envelope_id field that its own doc comment says is always zero, with nothing enforcing it and a derived Serialize that would serialize whatever value it holds. A test named envelope_id_hash_eq asserts newtype equality and HashSet membership on two literal byte arrays and does not hash anything. Section Determinism Violations states none added."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959-A1
    - RFC-0126
---

# Mission `rfc-0959-a1-specifies-two-content-addresses-and-derives-neither`

## Status

Deferred. The derivations belong to the still-open `0959-b2-deliver-algorithm`,
so this is a record of what that mission has to build rather than a new
complaint. Nothing was actioned.

Ninth systematic sweep, and the second on the delivery amendment.

## Finding

A1 §Determinism Requirements names two content addresses and gives each an
exact formula — the kind of specification that cannot be satisfied
accidentally:

> **`event_hash` computation:** `BLAKE3(canonical_ser(DealSettledPayload w/o
seller_signature))` — `seller_signature` is zeroed, `event_hash` is zeroed.
>
> **`envelope_id` computation:**
> `BLAKE3(canonical_ser(MarketDeliveryEnvelopePreimage::from(&envelope)))`
> where the preimage struct zeroes the `envelope_id` field.

`crates/octo-wallet/src/capability/market_delivery.rs` — the module A1 names
as the home for exactly these types — contains **no BLAKE3 call and no
`canonical_ser` anywhere**. It is 673 lines of struct and enum definitions,
four hand-written redacting `Debug` impls, and 18 tests.

Neither address is derived. The `From<&MarketDeliveryEnvelope>` conversion the
second formula names does not exist, so the formula has no entry point at all.

## The preimage exists; the thing that makes it a preimage does not

`MarketDeliveryEnvelopePreimage` is declared, and its doc comment is careful:

> Preimage for envelope_id derivation. `envelope_id` field is always zero in the
> preimage (R10-N8 fix: avoid self-referential hash).

**"Always zero" is enforced by nothing.** The field is `pub`, so any caller can
construct a preimage with a non-zero `envelope_id`, and the struct derives
`Serialize`, so whatever value it holds would be fed straight into a hash the
moment the missing conversion is written.

This is the same lesson IA-9 teaches inside the base RFC, arriving again in a
different crate: an invariant that could be a type is written as a comment. The
RFC's own wording — "the preimage struct **zeroes** the `envelope_id` field" —
describes an action someone must perform, not a property the type has. A type
that omitted the field, or a constructor that fixed it, would make R10-N8 a
guarantee instead of a duty.

Whoever implements `0959-b2` will be handed a struct that looks ready and a
sentence explaining that one field must be zeroed first. The first person to
miss it gets a self-referential hash, and nothing in the type system objects.

## A test named for the hash that does not hash

```rust
#[test]
fn envelope_id_hash_eq() {
    let a = EnvelopeId([0x01; 32]);
    let b = EnvelopeId([0x01; 32]);
    let c = EnvelopeId([0x02; 32]);
    assert_eq!(a, b);
    assert_ne!(a, c);
    let mut set = std::collections::HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
}
```

Two literal byte arrays, compared for equality, and inserted into a `HashSet`.
No input is serialised and no digest is computed — this exercises
`#[derive(PartialEq)]` and `#[derive(Hash)]` on a newtype. The name reads as
"the envelope id hash is equal for equal inputs", which is the property A1
specifies and the module does not implement.

It is not a bad test. It is a correct test of the newtype's derives, wearing
the name of a property test. Which means the module's test list shows an
`envelope_id` hash test to anyone scanning names, and the property is absent.

This is the third instance this audit has found of the same shape — a helper or
fixture whose name or comment claims it covers a production canonicalisation and
does not. The other two are `anti_fraud`'s `compute_settlement_hash` and
`tv_0959_settlement_wire.rs`.

## Against §Determinism Violations

A1 §Determinism Violations is one sentence:

> None added. `settled_at_unix` is pinned from the prior `SettlementEvent`; chain
> hashes are deterministic.

Both clauses are about _reusing_ determinism that already exists elsewhere —
`settled_at_unix` from the prior event, chain hashes from the chain. Neither
makes a claim about `event_hash` or `envelope_id`, so strictly "none added" is
not contradicted; the section is only addressing violations added _by this
amendment_.

But a reader who arrives at "chain hashes are deterministic" while
`event_hash` and `envelope_id` are underived will take it as covering the
amendment's hashing story. It does not. The honest sentence would name the two
addresses that are specified and unbuilt.

## What is real here, and worth saying

The delivery types are genuinely well made, and `0959-d` already records that
they are heavily referenced. Specifically:

- the redaction work is thorough — hand-written `Debug` impls on all four
  delivery types, six redaction tests, and error variants that redact
  `envelope_id` and both chain hashes while preserving them for the caller;
- the error taxonomy is complete, including `ChainTipMismatch`,
  `ChainHashBroken` and `ChainAppendError` as distinct variants;
- `MarketDeliveryEnvelopePreimage` being declared at all, before any derivation
  exists, is the right order of work.

What is missing is the arithmetic. The module is a well-built set of shapes
awaiting the function that fills them.

## What the owner should do

- **Record `event_hash` and `envelope_id` as pending in `0959-b2`.** They are
  Class-A content addresses in the RFC-0008 sense — A1's own execution-class
  table rates both Class A — and nothing currently derives them.
- **Make the zeroing a type, not a comment,** when the conversion is written.
  Dropping `envelope_id` from the preimage struct entirely is the strongest
  form; a constructor that fixes it to `[0u8; 32]` is nearly as good and less
  invasive.
- **Rename `envelope_id_hash_eq`** to what it tests, so the eventual derivation
  can take the name it currently occupies.
- **Broaden §Determinism Violations** to name the two addresses rather than
  relying on the reader to connect "chain hashes" to the whole hashing story.

## Reproduction

```bash
# the two formulas
grep -n 'event_hash. computation\|envelope_id. computation' \
  rfcs/accepted/economics/0959-a1-market-delivery.md

# and their absence from the module that should hold them
grep -c 'blake3\|canonical_ser' \
  crates/octo-wallet/src/capability/market_delivery.rs \
  || echo "  (0 — no hashing in the module at all)"

# the conversion the second formula names
grep -n 'impl From<&MarketDeliveryEnvelope>\|MarketDeliveryEnvelopePreimage::from' \
  --include=*.rs -r crates/ || echo "no preimage conversion exists"

# what the module actually is: shapes and redacted Debug impls
grep -n 'pub struct \|pub enum \|impl std::fmt::Debug' \
  crates/octo-wallet/src/capability/market_delivery.rs

# the preimage's unenforced zero
grep -n 'pub struct MarketDeliveryEnvelopePreimage' -A 8 \
  crates/octo-wallet/src/capability/market_delivery.rs

# the test that does not hash
grep -n 'fn envelope_id_hash_eq' -A 11 \
  crates/octo-wallet/src/capability/market_delivery.rs

# and the section that says nothing is outstanding
grep -n -A 2 '### Determinism Violations' \
  rfcs/accepted/economics/0959-a1-market-delivery.md
```

## Cross-references

- Mission `0959-d-market-delivery-roll-up-is-green-on-work-measurement-contradicts`
  — open, and already recording that the delivery types are real while
  `deliver_at_settlement` is a doc comment; this mission covers the arithmetic
  those types are missing
- Mission `rfc-0959-a1-cites-a-transaction-guarantee-section-that-does-not-exist`
  — the same amendment, on IA-2 and IA-6 citing a section of RFC-0862 that
  does not exist
- Mission `rfc-0959-test-vectors-contain-no-expected-values` — A1 has twelve
  test vectors of its own; TV6 and TV12 depend on these two addresses
- Mission `four-of-rfc-0959s-ten-implementation-assumptions-fail` — IA-9 in the
  base RFC is the counterexample: `CACHE_KEY_DOMAIN` enforced by its type
  signature, which is what the preimage's `envelope_id` field is not
- Mission `the-settlement-hash-that-binds-cost-is-built-and-not-on-the-live-path`
  — the third named-hash-that-does-not-cover-what-it-claims, alongside
  `anti_fraud`'s replica and `tv_0959_settlement_wire.rs`
