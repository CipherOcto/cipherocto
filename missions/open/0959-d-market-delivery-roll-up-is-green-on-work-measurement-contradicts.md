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

The **types are real and heavily used.** `MarketDeliveryEnvelope` has 38
references and `DealSettled` has 52. A supporting macaroon helper exists,
referenced by `deliver_at_settlement` step 3. The atomicity substrate
(`HolderRegistry::insert_dual`) is genuinely implemented over a real Stoolap
transaction.

This is not an abandoned mission. It is a substantially-built one whose
**completion record overstates it on three specific axes**: one type is a stub,
one algorithm has no implementation, and the vector set cannot be located.

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

- [ ] `BearerCapsule` carries real cryptography — X25519 agreement, ChaCha20-Poly1305 `encrypted_capsule`, Ed25519 `seller_signature` — and something verifies that signature on the receiving side.
- [ ] `deliver_at_settlement` exists as a function, or the RFC's §Algorithms entry is amended to name what was built instead.
- [ ] Each of the 12 vectors resolves to a test findable by the RFC's own name, or the roll-up records which are covered and which are not.
- [ ] The `0959-a1` acceptance line states what is covered, rather than a blanket `[x]`.

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
