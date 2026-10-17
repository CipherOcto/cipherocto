---
name: the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist
description: "RFC-0959 specifies SettlementReceipt as a router-signed envelope carrying receipt_id = BLAKE3(canonical_ser(event || nonce || settled_at_unix)), with the router signature covering canonical_ser((receipt_id, event, nonce, settled_at_unix)) and replay keyed on it per router DID. The implementation's SettlementReceipt has three fields (event, router_signature, nonce) and no receipt_id at all; the signature preimage omits it; and the ConsumedReceiptIndex is a flat HashMap keyed on the envelope nonce with no per-router scoping. Separately, sign_settlement_receipt and verify_settlement_receipt have zero production callers, all eleven call sites sit inside the ask.rs test module, and SettlementEvent carries no asker_did or holder_did, so even the signed surface records no parties. The tree also holds a second, entirely disjoint settlement stack in octo-settlement-core with its own Receipt, receipt_id_for and verify_receipt_chain, with no dependency edge in either direction."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0014
    - RFC-0016
---

# Mission `the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`

## Status

Deferred — adding `receipt_id` to the receipt and to the signature preimage
is a Layer A wire-format change, and reconciling two settlement stacks is an
architecture decision with cross-RFC implications (the two trace to
different RFC lineages). Nothing was actioned; the measurement probes were
removed and the tree matches its prior state.

## Finding

RFC-0959 specifies the router-signed receipt in detail. The implementation
has three of its fields and none of the replay structure.

### What the RFC specifies

```text
`SettlementReceipt` — router-signed envelope binding the event + nonce +
settled_at_unix + receipt_id. `receipt_id =
BLAKE3(canonical_ser(event || nonce || settled_at_unix))`. Replay defense
via `ConsumedReceiptIndex: HashMap<DID, HashSet<ReceiptId>>`.
```

and the wire shape:

```rust
// signed_payload = canonical_ser((receipt_id, event, nonce, settled_at_unix))
pub receipt_id: ReceiptId,   // BLAKE3(canonical_ser((event, nonce, settled_at_unix)))
```

with acceptance criterion G6: "Same `(event_tuple, nonce)` from same router
MUST yield distinct `receipt_id`; consumed-receipt index rejects duplicates
deterministically."

### What the implementation has

```rust
pub struct SettlementReceipt {
    pub event: SettlementEvent,
    pub router_signature: Vec<u8>,
    pub nonce: [u8; 16],
}
```

Three fields. No `receipt_id`. The signature preimage is
`canonical_ser(event) || nonce || settled_at_unix` — the function's own
comment says it approximates `canonical_ser((event || nonce ||
settled_at_unix))`, dropping `receipt_id` relative to the RFC's
`canonical_ser((receipt_id, event, nonce, settled_at_unix))`.

`ConsumedReceiptIndex` is:

```rust
pub struct ConsumedReceiptIndex {
    seen: HashMap<[u8; 32], ()>,
}
```

A flat map keyed on the 32-byte envelope nonce. The RFC specifies
`HashMap<DID, HashSet<ReceiptId>>` — per-router scoping over receipt ids.
Neither the set, the DID keying, nor the id exists. Grepping `receipt_id`
across all Rust returns hits only in `octo-settlement-core`, a different
crate with a different `Receipt` type.

### The signed surface records no parties either

`SettlementEvent` is `cap_root_hash`, `ask_id`, `invocation_hash`,
`axes_consumed`, `cost`, `settled_at_unix`. There is no `asker_did` and no
`holder_did`.

So the router signature binds the amount, the ask, the consumption and the
time — and no identity. The only surface in the crate that carries the
parties, `SettlementEnvelope`, is the one with no signature at all.

**Neither settlement surface is both signed and identity-bearing.** That is
the finding underneath the individual gaps, and it is why they have not been
caught in combination: each is defensible read alone.

### Nothing verifies these receipts in production

All eleven call sites of `sign_settlement_receipt` and
`verify_settlement_receipt` are inside `ask.rs`'s `#[cfg(test)] mod tests`,
which begins well above them. The only non-test reference is the `lib.rs`
re-export. Same shape as `AskSigned::verify` in mission
`the-ask-signature-gates-nothing`.

This is worth stating carefully: the mechanism is a reasonable
implementation of a signed receipt. It is simply not called, and the thing it
would protect does not carry the parties.

## The second stack

`octo-settlement-core` is a complete, separate settlement implementation:

```text
crates/octo-settlement-core/src/{receipt,chain,store,sink,reservation,error}.rs
```

with its own `Receipt` (carrying `router_id`, `router_sig`, `subject_did`,
`cost_dqa`, `settlement_hash`), its own `receipt_id_for`, its own
`verify_receipt_chain`, its own store and sink.

The two crates share **no dependency edge in either direction**:

```text
quota-router-storage -> octo-settlement-core : no
octo-settlement-core -> quota-router-storage : no
```

`octo-settlement-core` is depended on by `octo-settlement`, `octo-audit`
and `quota-router-sm-engine`. The RFC-0959 settlement stack that the
marketplace CLI actually uses is not among them.

The two also disagree on what a receipt id _is_, which is not a naming
detail:

- RFC-0959: `receipt_id = BLAKE3(canonical_ser(event || nonce || settled_at_unix))`
  — a content hash.
- `octo-settlement-core`: `receipt_id: u64`, monotonic per stream, and
  `receipt_id_for` hashes `(receipt_id, ask_id, router_id, router_sig,
timestamp_unix)` — it chains a counter, and notably does **not** include
  `settlement_hash`, `cost_dqa` or `subject_did`.

So the crate that implements a receipt-id chain does not implement
RFC-0959's receipt id, and the crate that implements RFC-0959 does not
implement a receipt-id chain. Neither is wrong on its own terms; together
they mean the marketplace's settlement receipt has no replay identity of any
kind.

## How this composes with the other settlement findings

Three measured defects, each in its own mission, form one exploitable chain:

| Step                         | Mission                                                                   |
| ---------------------------- | ------------------------------------------------------------------------- |
| Envelope has no signature    | `settlement-hash-does-not-bind-the-cost` (the hash is the only mechanism) |
| Hash omits the parties       | `settlement-hash-does-not-bind-the-cost` (v1.1)                           |
| Hash omits `cost`            | `settlement-hash-does-not-bind-the-cost` (v1.0)                           |
| Replay keys on author nonce  | `replay-defense-is-a-no-op-in-the-default-cli-configuration` (v1.2)       |
| Nothing verifies any receipt | this mission                                                              |

The v1.2 replay measurement is the one that closes the loop: three
submissions of one economic event with three different nonces all returned
`verify: OK` against a **persistent** store, with `index_len` growing 1, 2, 3.
Making the index persistent — the fix the original replay mission proposed —
does not help, because the indexed value is chosen by the party being
checked.

## What the RFC owner needs to decide

- Should `SettlementReceipt` gain `receipt_id` as a field and include it in
  the signature preimage, as RFC-0959 specifies? This is Layer A wire format
  and changes every receipt on the wire.
- If yes, does the existing `ConsumedReceiptIndex` get replaced by the
  specified `HashMap<DID, HashSet<ReceiptId>>`, or extended? The current
  per-nonce flat map is neither shape.
- Should `SettlementEvent` gain `asker_did` / `holder_did`, or should
  settlement identity live only on the envelope? Right now identity exists
  only on the unsigned surface, which is the worst of the two placements.
- Should `sign_settlement_receipt` / `verify_settlement_receipt` have
  production callers at all? If the router-signed receipt is the intended
  settlement attestation, nothing currently produces or checks one.
- Which settlement stack is canonical — the RFC-0959 one the marketplace CLI
  uses, or `octo-settlement-core`? They are disjoint today, which means the
  RFC-0959 CLI path and the audited settlements are not the same system. If
  `octo-settlement-core` is intended to be the substrate, RFC-0959's receipt
  definition and `octo-settlement-core`'s counter-based id need reconciling
  rather than parallel maintenance.
- Is G6's scoping over `(event_tuple, nonce)` still the right requirement
  once it is clear that the nonce is author-controlled? See the v1.2 replay
  mission.

## Reproduction

```bash
# the RFC's receipt definition and wire shape
grep -n -B2 -A4 'router-signed envelope binding' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n -B3 -A2 'pub receipt_id: ReceiptId' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the implementation's three fields
grep -n -A12 'pub struct SettlementReceipt' crates/quota-router-storage/src/ask.rs

# the signature preimage omits receipt_id
grep -n -A16 'pub fn sign_settlement_receipt' crates/quota-router-storage/src/ask.rs

# the flat nonce-keyed index, not HashMap<DID, HashSet<ReceiptId>>
grep -n -A6 'pub struct ConsumedReceiptIndex' crates/quota-router-storage/src/ask.rs

# SettlementEvent has no identity fields
grep -n -A16 'pub struct SettlementEvent' crates/quota-router-storage/src/ask.rs

# zero production callers: the test module starts above every call site
grep -rn 'sign_settlement_receipt\|verify_settlement_receipt' \
  --include=*.rs crates/ | grep -v 'pub fn' | grep -v '/tests/'

# the two stacks share no edge
grep -n 'octo-settlement-core' crates/quota-router-storage/Cargo.toml || echo none
grep -n 'quota-router' crates/octo-settlement-core/Cargo.toml || echo none
grep -rn 'octo-settlement-core' --include=Cargo.toml crates/ \
  | grep -v '^crates/octo-settlement-core/'

# the two disagree on what a receipt id is
grep -n -A9 'pub fn receipt_id_for' crates/octo-settlement-core/src/chain.rs
grep -n -A4 'pub receipt_id' crates/octo-settlement-core/src/receipt.rs
```

## Cross-references

- RFC-0959 §Data Structures — `SettlementReceipt` and `ConsumedReceiptIndex`
  as specified, including `receipt_id`
- RFC-0959 §Algorithms — the signature input `canonical_ser((receipt_id,
event, nonce, settled_at_unix))`
- RFC-0959 §Acceptance criteria — G6, the replay-determinism guarantee
- RFC-0014 §Module Layout / RFC-0016-a §6.4 — the lineage of the
  `octo-settlement-core` receipt chain
- Mission `settlement-hash-does-not-bind-the-cost` — the unsigned envelope and
  the unbound `cost` / parties; this mission is the signed counterpart
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` — v1.2
  withdraws its "mechanism is sound" claim and measures nonce-varied replay
  against a persistent store
- Mission `the-settlement-path-records-but-never-settles` — why all of this
  is latent rather than exploited today
- Mission `the-settlement-audit-trail-is-never-written` — the event repository
  that would record any of it
- Mission `two-crates-define-spend-ledger-with-incompatible-schemas` — the
  same duplicated-subsystem shape, in storage rather than settlement
