---
name: settlement-hash-does-not-bind-the-cost
description: "Both settlement-hash surfaces in quota-router-storage omit the cost from their preimage. The live one, SettlementEnvelope::compute_settlement_hash, has no signature field either, so the hash is its only integrity mechanism. Measured: inflating cost 1000x while keeping the original settlement_hash passes SettlementEnvelope::verify() and ConsumedReceiptRepository::verify_and_insert. RFC-0959 requires cost in the preimage explicitly as an R3 fix whose stated purpose was that tampering breaks the hash."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0909
---

# Mission `settlement-hash-does-not-bind-the-cost`

## Status

Deferred — **this is a Layer A hash-preimage question and no RFC-level
decision is the auditor's to make.** Changing a settlement hash preimage
invalidates every stored `settlement_hash` and is a consensus-visible
change, so it needs owner sign-off. Nothing was actioned; the temporary
measurement probes were removed and the tree matches its prior state.

This is the highest-severity finding of the marketplace audit, and the
first one where a **live production path** carries the defect.

## Finding

There are two settlement hashes in `quota-router-storage`. Neither binds
the amount charged. They differ a lot in how much that matters.

### Surface B — `SettlementEnvelope::compute_settlement_hash` (LIVE)

This is the one with a production caller — `quota-router-cli` sets
`envelope.settlement_hash` at command construction — and the one
persisted and verified by `ConsumedReceiptRepository::verify_and_insert`.

Its preimage, enumerated at length in its own doc comment, is:

```text
model_wire || serde_json::to_vec(axes_consumed) || ask_id || nonce
|| timestamp_unix.to_le_bytes()
|| cost_vault_id_opt_prefix || [if Some] cost_vault_id
|| chain_id_opt_prefix || [if Some] chain_id
```

`cost` is a field on the struct. It is not in the preimage.

And `SettlementEnvelope` has **no signature field at all** — no
`router_signature`, no signer identity. `verify_and_insert` recomputes the
hash and compares it to the stored one; that is the entire integrity
check.

So the amount charged in a verified settlement envelope is not committed
to by anything.

### Surface A — `ask::compute_settlement_hash(event)` (latent)

This one is closer to the RFC: it uses the domain separator and four of
the RFC's five fields (`cap_root_hash`, `ask_id`, `invocation_hash`,
`canonical_ser(axes_consumed)`). It is missing exactly one field, `cost`.

It has no production callers. Where it is used, in
`verify_settlement_receipt`, the router Ed25519 signature is verified over
`canonical_ser(event) || nonce || settled_at_unix` — and `event` carries
`cost`, so **the signature does bind the amount**. Surface A is a
conformance deviation, not an exploitable hole.

## Measurement

Surface B, on the live verify path:

```text
honest cost=30000                   verify_ok=true
forged cost=30000000                hash_same=true
cost_swapped_with_honest_hash       verify_ok=true
```

An attacker takes a valid envelope, multiplies `cost` by 1000, keeps the
original `settlement_hash`, and `verify()` returns `Ok`. The 1000x
inflation is undetectable by anything in the crate.

Surface A:

```text
base_cost=1000000  hash=[247, 72, 171, ...]
tampd_cost=       1 hash=[247, 72, 171, ...]
hash_differs=false
```

Byte-identical digests for a millionfold cost change.

## What the RFC requires

RFC-0959 §Settlement hash lists the preimage inputs as five fields plus a
version tag, and the last one is:

```text
6. `cost` (16 bytes, u128 LE) — R3 fix: bound into hash
```

and the corresponding pseudocode is:

```rust
h.update(&event.cost.0.to_le_bytes());   // R3 fix: cost bound into hash; tampering breaks hash
```

The stated purpose of that R3 fix is exactly the property that Surface B
lacks: **tampering breaks the hash**. Measured, it does not.

RFC-0959 Appendix C's fail-closed principle applies to axes; the
cost-binding requirement is stated twice independently in §Settlement
hash, which is unusually strong for this document.

## Why no test catches it

Three reasons, each verified rather than assumed:

1. **The RFC supplies no expected digest.** Both test vectors say
   `[32-byte digest — implementer computes]`. There is no byte-exact
   value to fail against, so a wrong preimage cannot fail a vector.
2. **The cross-impl test covers a different type.**
   `cross_impl_tv1_settlement_hash_matches` compares
   `SettlementEnvelope::compute_settlement_hash` against a hand-rolled
   reference that mirrors the _same_ preimage. Agreement between an
   implementation and a transcription of itself is not evidence.
3. **The tamper test picks a field that happens to be hashed.**
   `verify_and_insert_rejects_tampered_envelope` alters
   `timestamp_unix`, which is in the preimage, so it passes and reads as
   proof that tamper detection works. It says nothing about `cost`.

That third one is the pattern worth naming: a test named for a property,
passing, while the property it implies is false for the field that matters
economically.

## Direction of the failure

Both directions are available, because `axes_consumed` **is** bound while
`cost` is not:

- **Inflate** the cost, keeping `axes_consumed` honest. The recipient
  pays 1000x what the recorded consumption supports.
- **Deflate** it to zero. The settlement verifies and nothing is charged.

Whether this is exploitable in practice depends on who can write an
envelope row and whether any party re-derives `cost` from `axes_consumed`
and the Ask's rates. Nothing in the crate does — the settlement-time path
recomputes the hash and stops there. The marketplace escrow is unwired
(separate deferred mission), so the dispute path that would catch an
inflated amount does not run.

## What the RFC owner needs to decide

- Should `cost` be added to the `SettlementEnvelope` preimage? This is the
  direct question and it is Layer A. Adding it invalidates every stored
  `settlement_hash`.
- Or should `SettlementEnvelope` stop being an integrity surface at all
  and gain the router signature that `SettlementReceipt` already has? That
  would make the signature, not the hash, the thing that binds the amount.
- The two surfaces should probably be reconciled. `SettlementEnvelope`'s
  preimage shares neither the domain separator, nor `cap_root_hash`, nor
  `invocation_hash` with RFC-0959 §Settlement hash, yet both are called a
  settlement hash and one is documented as implementing the RFC. Two
  settlement hashes with different preimages is the duplicated-path shape
  that has produced several other findings in this audit.
- Should the R3 "cost bound into hash" requirement be restated as an
  acceptance criterion with a concrete expected digest, so a future
  preimage error can actually fail a test?

## Reproduction

```bash
# Surface B preimage enumerates cost's absence
grep -n -A40 'The v2.0 canonical preimage that the single function' \
  crates/quota-router-storage/src/ask.rs

# Surface B has no signature field
grep -n -A34 'pub struct SettlementEnvelope' crates/quota-router-storage/src/ask.rs

# the live verification path recomputes only the hash
grep -n -A10 'pub fn verify_and_insert' \
  crates/quota-router-storage/src/consumed_receipt_repo.rs

# the live production caller
grep -rn 'envelope.settlement_hash = envelope.compute_settlement_hash' \
  --include=*.rs crates/

# Surface A preimage, missing cost
grep -n -A12 'pub fn compute_settlement_hash(event' \
  crates/quota-router-storage/src/ask.rs

# the RFC requirement
grep -n 'R3 fix: cost bound into hash' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the RFC supplies no expected digest to fail against
grep -n '32-byte digest — implementer computes' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md | head

# the tamper test that picks a hashed field
grep -n -A12 'fn verify_and_insert_rejects_tampered_envelope' \
  crates/quota-router-storage/src/consumed_receipt_repo.rs
```

## Cross-references

- RFC-0959 §Settlement hash — the required preimage, including `cost`
- RFC-0959 §Compute cost — the function that produces the unbound value
- RFC-0900 §Dispute Resolution — the escrow path that would catch an
  inflated amount; unwired today
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` —
  the other RFC-0959 conformance gap in the same file, and the second
  where the code is fail-open where the RFC is fail-closed
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the same `cost` value, wrong for a different reason
- Mission `the-marketplace-facade-has-no-production-caller` — the standing
  caveat, which does **not** apply to Surface B
