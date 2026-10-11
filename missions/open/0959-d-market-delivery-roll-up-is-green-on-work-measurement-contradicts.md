---
name: 0959-d-market-delivery-roll-up-is-green-on-work-measurement-contradicts
description: Mission 0959-a1 records all 12 RFC-0959-A1 test vectors as GREEN via a 2026-08-07 audit-closure, and its type-coverage table lists BearerCapsule and deliver_at_settlement as implemented. Measured: BearerCapsule is a structural stub with no cryptography and one definition in the workspace, deliver_at_settlement appears once in the whole tree and only inside a doc comment, and none of the RFC's vector names resolve to a test. The delivery types themselves are real and heavily referenced.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959-A1
    - 0959-a1-market-delivery
---

# Mission `0959-d-market-delivery-roll-up-is-green-on-work-measurement-contradicts`

## Status

Open. Filed from measurement, against a recorded claim rather than against code.

## The distinction this finding rests on

This arc has treated every code-level claim as a **hypothesis** — including six
that measurement disproved. That is the right posture for code, because code
changes and a claim about it is a claim about a moment.

A mission's completion record is **governance**, not code. A GREEN roll-up is a
statement that something was reviewed and found sound. When measurement
contradicts it, that is a different kind of event, and it is recorded as one.

## What `0959-a1` records

> - [x] All 12 RFC-0959-A1 §Test Vectors pass (TV1: Minimal Delivery … TV12:
>       Buyer Identity Binding) → **GREEN roll-up complete** (2026-08-07
>       audit-closure)

and a type-coverage table listing `BearerCapsule` and `deliver_at_settlement` as
implemented by sub-mission `0959-b`.

## What measurement found

| Claimed                             | Measured                                                                                                                                          |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `BearerCapsule` implemented         | **one** `struct BearerCapsule` in the workspace, in `bearer_capsule_stub.rs`, and it is self-described as a structural stub                       |
|                                     | no cryptography: `encrypted_capsule` is arbitrary bytes, `seller_signature` is an unverified 64-byte array                                        |
|                                     | nothing in the workspace verifies that signature; no `BearerCapsule` is constructed outside tests                                                 |
|                                     | `ChaCha20-Poly1305` appears in `octo-wallet` **only** in `keystore.rs`, never in the capability or market-delivery path; no X25519 there either   |
|                                     | `market_delivery.rs` _imports_ the stub rather than defining a real type                                                                          |
| `deliver_at_settlement` implemented | **one** occurrence workspace-wide, inside a doc comment in `macaroon.rs` referring to its own step 3                                              |
| 12 test vectors GREEN               | none of the vector names resolve to a test: `minimal_delivery` 0, `bearer_insert_fails` 0; `market_delivery.rs` contains **zero** `tv*` functions |

## What is _not_ wrong, and matters to say

**`0959-b` is honest, and was being set up to take blame it did not earn.**

Read on its own terms, `0959-b` discloses almost everything:

- it says `BearerCapsule` is **re-exported** from `bearer_capsule_stub.rs`,
  rather than claiming it authored a capsule
- it marks **10 of the 12 vectors unchecked and `DEFERRED`**, naming the owning
  follow-up mission for each
- it surfaces two RFC drift items against itself: `RoleTag` shipping with
  `{Buyer, Seller, Router}` where RFC-0971 expects `{Asker, TokenIssuer, Router}`,
  and `DeliveryError` shipping with 6 variants where the RFC defines 14

`0959-b1-mdelivery-types-drift` then did that work and is archived/completed.
Measured today: `RoleTag` is `{Asker, TokenIssuer, Router}` and `DeliveryError`
carries 13 variants. **The drift item landed.**

So the chain of custody is sound for three steps and then stops:

| Mission                     | State                                                                            | Accountable?     |
| --------------------------- | -------------------------------------------------------------------------------- | ---------------- |
| `0959-b` Band A             | closed; stub re-export disclosed; vectors deferred                               | **yes — honest** |
| `0959-b1` types drift       | archived/completed; `RoleTag` + `DeliveryError` fixed                            | **yes — done**   |
| `0959-b2-deliver-algorithm` | **never opened.** Owns `deliver_at_settlement` and TV1/TV2/TV3/TV5/TV6/TV10/TV12 | **no owner**     |
| `0959-b3-settlement-chain`  | **never opened.** Owns `append_deal_settled` and TV8/TV11                        | **no owner**     |
| `0959-a1` roll-up           | claims all 12 GREEN                                                              | **no**           |

**The distortion is entirely in the roll-up, one level above the work.** It cites
`0959-b` for ten vectors that `0959-b` marks unchecked and deferred, and lists
`BearerCapsule` and `deliver_at_settlement` as implemented by a mission that
deferred the algorithm to a file that was never created.

The types are also real and heavily used: `MarketDeliveryEnvelope` has 38
references, `DealSettled` has 52, and `HolderRegistry::insert_dual` is genuinely
implemented over a real Stoolap transaction. This is not an abandoned mission.

### Correction to this mission's own first draft

The first version framed the overstatement as `0959-a1` versus measurement,
without establishing where the blame sat between parent and sub-missions. Read
through, it could have been taken as impugning `0959-b`, which had done nothing
but disclose its own gaps. It did not. The remediation is consequently narrower
and more useful than it first appeared: **open the two missing missions, and fix
the roll-up.**

## Why the BearerCapsule stub matters beyond the roll-up

`mint_dual` is the function that would issue a capability with its
`Caveat::AskBinding`, and it is blocked on exactly this type. So one stub with no
cryptography holds up, behind it:

- RFC-0969 TV9 (Dual-Issuance Atomicity), which has no test and cannot have one
- every production-minted capability being ask-unbound, so
  `CapabilityToken::ask_binding()` returns `Ok(None)` universally
- the gateway `CapabilityVerifier`, which has no ask-bound capability to verify

See mission `0969-c`, which records that convergence.

## Acceptance Criteria

Ordered, because the first item is bookkeeping and the rest are work that
belonged to missions which were never opened.

- [ ] The `0959-a1` acceptance line states what is covered rather than a blanket `[x]`. **Cheapest, and it is the actual defect.**
- [x] `0959-b2-deliver-algorithm` exists as a mission, owning `deliver_at_settlement` and TV1/TV2/TV3/TV5/TV6/TV10/TV12. Filed 2026-10-12 — it had been deferred to by `0959-b` since 2026-08-06 with no file behind it.
- [x] `0959-b3-settlement-chain` exists as a mission, owning `append_deal_settled` and TV8/TV11. Filed 2026-10-12, same origin.
- [ ] `BearerCapsule` carries real cryptography — X25519 agreement, ChaCha20-Poly1305 `encrypted_capsule`, Ed25519 `seller_signature` — and something verifies that signature on the receiving side. **Owned by `0959-b2` when it exists; not by this mission.**
- [ ] Each of the 12 vectors resolves to a test findable by the RFC's own name, or the roll-up records which are covered and which are not.

**`0959-b` needs nothing.** It recorded its own gaps accurately and its one
deferred item that had an owner was delivered. Re-opening it would be penalising
the part of the chain that behaved well.

## What must not happen

Do not satisfy the vector criterion by writing twelve tests against the current
stub. A `BearerCapsule` built from arbitrary bytes passes every assertion about
its own shape, and a suite of those reads as full coverage of an encrypted
transport that does not exist. This arc has declined that trade twice already —
once for FROST nonce determinism and once for capability revocation.

Do not resolve it by re-marking the roll-up as partially GREEN and moving on.
The record is wrong in a way a reader would act on, and the correction is cheap
precisely because the rest of the work is real.

## Dependencies

- Mission `0959-a1-market-delivery` — holds the roll-up claim
- Mission `0969-c-gateway-verifier-diverges-from-rfc-and-has-no-production-implementation`
- RFC-0959-A1 §Test Vectors, §Security, §Algorithms
