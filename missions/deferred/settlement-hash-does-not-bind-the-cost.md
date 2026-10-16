---
name: settlement-hash-does-not-bind-the-cost
description: "The live settlement-hash surface omits THREE fields from its preimage: cost, asker_did and holder_did. SettlementEnvelope::compute_settlement_hash has no signature field either, so the hash is its only integrity mechanism. Measured on the built binary: rewriting the payee from the legitimate seller to did:octo:attacker leaves settlement_hash byte-identical (8138e9a8...), and the envelope passes settle-replay with verify OK and exit 0 — the hash a third party would check against the seller's signature does not change when the payee does. Swapping the payer is equally invisible. Separately, inflating cost 1000x while keeping the original hash passes verify. RFC-0959 requires cost in the preimage explicitly as an R3 fix whose stated purpose was that tampering breaks the hash. The RFC names Ask forgery (claiming another asker_did) as a consensus attack, so the identity omission is a specified threat, not a hypothetical."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.2"
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

## v1.2 — a surface that binds cost already exists, and is not on the live path

The recommendation below reads "bind `cost` into the settlement hash, and
this needs an RFC decision". That understates how settled the question is.
**RFC-0959-v28 specifies a ten-field body hash that binds `cost` — with its
scale as a separate byte — and it is implemented correctly** in
`quota-router-sm-engine`, with a domain separator and a green determinism
test.

It is unreachable from the live path for one reason: `quota-router-cli` does
not depend on `quota-router-sm-engine`. So the open question is not "what
should the preimage be" — it is written down and built — but whether the CLI
should gain that dependency or the body hash should move to a crate both can
reach.

A fourth construction also exists: a private helper in `anti_fraud` that
claims in its doc comment to mirror the envelope hash and does not, backing
an invariance test that therefore cannot fail on a production divergence.

Both surfaces, the dependency evidence, and three of the four constructions
are worked through in
`the-settlement-hash-that-binds-cost-is-built-and-not-on-the-live-path`.
This mission's measurements are unaffected.

## v1.1 — the omission is larger than cost, and worse

The original finding was that `cost` is absent from the live preimage. That
is correct but incomplete: **`asker_did` and `holder_did` are absent too.**
They are the payer and the payee, and their omission outranks the cost
problem — a settlement can be redirected to a different recipient while
carrying the _original_ hash, which is the value any third party would check.

The rest of this document stands; the finding, measurement and RFC sections
below are updated to cover the complete unbound set.

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

Three fields on the struct are in that list nowhere: `cost`, `asker_did`,
`holder_did`.

The first is the amount. The other two are _who is paying and who is being
paid_ — and a settlement's entire purpose is a transfer between them. Both
are named in the struct's own field docs ("Asker DID", "Holder DID") as
carrying identity, and neither participates in the commitment to that
identity.

And `SettlementEnvelope` has **no signature field at all** — no
`router_signature`, no signer identity. `verify_and_insert` recomputes the
hash and compares it to the stored one; that is the entire integrity
check.

So neither the amount charged nor the parties to the transfer is committed
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

### The identity omission, end to end on the built binary

Three envelopes differing only in party identity, hashed:

```text
honest          (holder=legit_seller, asker=legit_buyer) = 8138e9a8d4c36b111ab8023987af3181e67e3e182de2c86e5bbc1293e34528ba
attacker_holder (holder=attacker,     asker=legit_buyer) = 8138e9a8d4c36b111ab8023987af3181e67e3e182de2c86e5bbc1293e34528ba
swapped_asker   (holder=legit_seller, asker=attacker)     = 8138e9a8d4c36b111ab8023987af3181e67e3e182de2c86e5bbc1293e34528ba

holder_changed = true
asker_changed  = true
```

Byte-identical digests. Neither party is committed to.

The full attack chain, on `quota-router` built from this tree. An envelope
whose payee was rewritten to `did:octo:attacker`, carrying the honest
envelope's hash:

```text
$ quota-router settle --from tampered.json
settlement_hash = 8138e9a8d4c36b111ab8023987af3181e67e3e182de2c86e5bbc1293e34528ba
exit=0

$ quota-router settle-replay --from tampered.json
settlement_hash = 8138e9a8d4c36b111ab8023987af3181e67e3e182de2c86e5bbc1293e34528ba
verify: OK (hash matches + nonce inserted)
exit=0
```

Two things make this worse than a stale-hash problem. `settle`
**overwrites** `settlement_hash` with a freshly computed value for whatever
JSON it is handed, so an attacker does not have to preserve anything — the
tool mints the "valid" hash for the tampered envelope. And the resulting hash
equals the honest envelope's hash, so the redirected settlement is
indistinguishable from the genuine one to any third party holding the
original value.

The default (no `--db-path`) replay store is also empty per invocation, so
the nonce check contributes nothing here; that is a separate finding
(`replay-defense-is-a-no-op-in-the-default-cli-configuration`) and is not
load-bearing for this one — the hash check passes on its own.

### The cost omission

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

The RFC also names the identity omission as a threat rather than leaving it
to be discovered. §Security Considerations §Consensus attacks lists:

```text
**Ask forgery:** attacker publishes Ask claiming someone else's
`asker_did`. Mitigation: Ed25519 signature + `asker_did` in
canonical_ser — recipients verify signature before marketplace insert.
```

The same class of attack applies to the settlement envelope, where the
mitigation is the hash. `asker_did` and `holder_did` are the two identity
fields a settlement commits to, and neither is in the preimage.

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

For `cost`, both directions are available, because `axes_consumed` **is**
bound while `cost` is not:

- **Inflate** the cost, keeping `axes_consumed` honest. The recipient
  pays 1000x what the recorded consumption supports.
- **Deflate** it to zero. The settlement verifies and nothing is charged.

For the identity fields the direction is singular and unconditional: replace
`holder_did` with any DID and the hash is unchanged. There is no partial
mitigation and no "only inflation" reading — the payee is simply not part of
what the hash commits to.

Whether this is exploitable in practice depends on who can write an
envelope row and whether any party re-derives `cost` from `axes_consumed`
and the Ask's rates. Nothing in the crate does — the settlement-time path
recomputes the hash and stops there. The marketplace escrow is unwired
(separate deferred mission), so the dispute path that would catch an
inflated amount does not run. The same caveat applies to a redirected
payee: today the settlement is recorded but no tokens move (see
`the-settlement-path-records-but-never-settles`), so the practical loss is a
mis-attributed audit record rather than a theft. That is the only thing
keeping this latent, and it is a property of the _next_ subsystem rather
than a safeguard in this one.

## What the RFC owner needs to decide

- Should `asker_did` and `holder_did` be added to the `SettlementEnvelope`
  preimage? This is the direct question for the identity omission, it is the
  more urgent of the two, and it is Layer A. Adding them invalidates every
  stored `settlement_hash` — but so does adding `cost`, and the two should
  almost certainly land together rather than in sequence.
- Should `cost` be added to the `SettlementEnvelope` preimage? This is the
  direct question and it is Layer A. Adding it invalidates every stored
  `settlement_hash`.
- Or should `SettlementEnvelope` stop being an integrity surface at all
  and gain the router signature that `SettlementReceipt` already has? That
  would make the signature, not the hash, the thing that binds the amount —
  and would also bind the parties, since `canonical_ser(envelope)` covers
  them. Given three unbound fields, this is the option worth weighing first.
- The two surfaces should probably be reconciled. `SettlementEnvelope`'s
  preimage shares neither the domain separator, nor `cap_root_hash`, nor
  `invocation_hash` with RFC-0959 §Settlement hash, yet both are called a
  settlement hash and one is documented as implementing the RFC. Two
  settlement hashes with different preimages is the duplicated-path shape
  that has produced several other findings in this audit.
- Should the R3 "cost bound into hash" requirement be restated as an
  acceptance criterion with a concrete expected digest, so a future
  preimage error can actually fail a test? The same applies to identity.

## Reproduction

```bash
# Surface B preimage enumerates cost's absence
grep -n -A40 'The v2.0 canonical preimage that the single function' \
  crates/quota-router-storage/src/ask.rs

# Surface B has no signature field
grep -n -A34 'pub struct SettlementEnvelope' crates/quota-router-storage/src/ask.rs

# the three unbound fields are on the struct and absent from the preimage
grep -n -A30 'pub fn compute_settlement_hash(&self)' \
  crates/quota-router-storage/src/ask.rs

# the RFC names Ask forgery as a consensus attack
grep -n -A2 'Ask forgery' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

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
- RFC-0959 §Security Considerations §Consensus attacks — names "Ask forgery"
  (claiming another `asker_did`) as a consensus attack, which is the same
  omission on the Ask side
- RFC-0959 §Compute cost — the function that produces the unbound value
- RFC-0900 §Dispute Resolution — the escrow path that would catch an
  inflated amount; unwired today
- Mission `a-signed-ask-does-not-prove-who-published-it` — the Ask-side
  identity gap: `verify` does not bind the DID to the key, and the RFC's
  own pseudocode takes the DID as a verify argument
- Mission `the-ask-signature-gates-nothing` — why no signature is available
  to bind these fields on the Ask side either
- Mission `the-settlement-path-records-but-never-settles` — the only thing
  keeping a redirected payee latent today
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` — the
  nonce half of the same live command; separate defect, not load-bearing here
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` —
  the other RFC-0959 conformance gap in the same file, and the second
  where the code is fail-open where the RFC is fail-closed
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the same `cost` value, wrong for a different reason
- Mission `the-marketplace-facade-has-no-production-caller` — the standing
  caveat, which does **not** apply to Surface B
