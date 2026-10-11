---
name: 0959-b2-deliver-algorithm
description: Owns deliver_at_settlement, the chain_tip_lock CAS, BearerCapsule cryptography, and RFC-0959-A1 TV1/TV2/TV3/TV5/TV6/TV10/TV12. Deferred to by mission 0959-b in 2026-08-06 and never opened, which is why the BearerCapsule stub still has no cryptography and why 0959-a1 can claim seven vectors as GREEN with nothing behind them.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959-A1
    - 0959-a1-market-delivery
    - 0959-d-market-delivery-roll-up-is-green-on-work-measurement-contradicts
---

# Mission `0959-b2-deliver-algorithm`

## Status

Open. Created 2026-10-12 to give this work an owner; it was deferred to by
`0959-b` on 2026-08-06 and the deferral target was never opened.

## Why this mission exists

`0959-b` closed Band A on 2026-08-06 and deferred its algorithm phase to three
follow-ups: `b1` (types drift), `b2` (this one) and `b3` (settlement chain).

`b1` was opened and completed — `RoleTag` and `DeliveryError` now match the RFC.
**`b2` and `b3` were never opened.** Meanwhile the parent roll-up in `0959-a1`
records all twelve RFC-0959-A1 vectors as GREEN.

So this mission's absence is the direct cause of two findings elsewhere:

- **`BearerCapsule` has no cryptography.** The stub is a 3-field shape with
  arbitrary `encrypted_capsule` bytes and an unverified `seller_signature`. The
  real type needs X25519 agreement, ChaCha20-Poly1305 and an Ed25519 signature,
  and it is this mission that would own it — `0959-b` explicitly disclosed the
  stub re-export rather than claiming authorship.
- **`deliver_at_settlement` does not exist.** One occurrence workspace-wide,
  inside a doc comment in `macaroon.rs` referring to its own step 3. The types
  (`MarketDeliveryEnvelope`, 38 references; `DealSettled`, 52) are real and
  heavily used; the algorithm that produces them is not.

It also sits directly behind the capability track in mission `0969-c`: the
gateway `CapabilityVerifier` has nothing ask-bound to verify, and `mint_dual` is
blocked on exactly the `BearerCapsule` type this mission would finish.

## Vectors owned here

Deferred from `0959-b`, quoted in its own terms:

- **TV1 — Minimal Delivery.** Full happy path; verify envelope structure + chain hash.
- **TV2 — Atomicity Rollback, bearer insert fails.** Forced failure on bearer insert; capability MUST NOT persist.
- **TV3 — Atomicity Rollback, capability insert fails.** Forced failure on capability insert; bearer MUST NOT persist.
- **TV5 — Backward Compat, legacy verifier.** A pre-RFC-0959-A1 verifier processes `SettlementEvent` + `SettlementReceipt` without crashing; the envelope is opaque to it.
- **TV6 — Replay Defense.** Duplicate `EnvelopeId` insert returns `DeliveryError::ReplayDetected`.
- **TV10 — Chain-Tip TOCTOU Race.** Two concurrent `deliver_at_settlement` calls on the same ask; one wins via `chain_tip_lock` CAS, the other returns `DeliveryError::ChainTipMismatch`.
- **TV12 — Buyer Identity Binding.** The envelope's `buyer_did` matches the buyer's `IdentityKey::did()`; tampering with `buyer_did` invalidates the seller signature.

## Acceptance Criteria

- [ ] `deliver_at_settlement` exists as a function and is the single entry point `0959-a1`'s Type Coverage table names.
- [ ] `chain_tip_lock` is a real compare-and-swap over the chain tip, not a read-then-write.
- [ ] `BearerCapsule` carries real cryptography — X25519 agreement, ChaCha20-Poly1305 `encrypted_capsule`, Ed25519 `seller_signature` — and **the receiving side verifies that signature.** A signature nothing checks is a field, not a defence.
- [ ] TV1–TV3, TV5, TV6, TV10, TV12 each resolve to a test findable under the RFC's own name.
- [ ] `0959-a1`'s roll-up is amended to stop claiming these seven as GREEN before they are.

## What must not happen

Do not write the seven vectors against the current stub. A `BearerCapsule` built
from arbitrary bytes passes every assertion about its own shape, and a suite of
those reads as coverage of an encrypted delivery transport that does not exist.
This is the third time this arc has refused that trade.

Do not satisfy TV10 with a sequential test. The vector is a **race**; a test that
runs one delivery after another proves ordering, not atomicity. It has to be two
concurrent callers contending for the same ask.

Do not mark `BearerCapsule` done on the strength of `encrypted_capsule` being
non-empty. Encryption without a verified signature is a confidentiality control
an attacker can forge by choosing their own plaintext.

## Dependencies

- Mission `0959-b-market-delivery-impl` — deferred this work explicitly
- Mission `0959-b3-settlement-chain` — owns `append_deal_settled`, which TV1 and TV10 need
- Mission `0969-c` — the capability verifier waits on this mission's `BearerCapsule`
- Mission `0959-d` — the contested roll-up this mission makes honest
- RFC-0959-A1 §Algorithms, §Security, §Test Vectors
