---
name: 0969-c-gateway-verifier-diverges-from-rfc-and-has-no-production-implementation
description: RFC-0969 specifies a capability verifier that reads ask_id via cap_token.ask_binding(). No such method exists — ask binding is a caveat on the macaroon. The RFC's v1.1 amendment removed cross-scheme linkage because dispatch rejects dual-scheme requests, but the implementation still evaluates it and still emits RoutingDecision::Dual. And the CapabilityVerifier trait the gateway authenticator requires has no production implementation anywhere.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0969
    - RFC-0957
    - 0009-cj-v2-wire-form-carries-no-capability-proof
---

# Mission `0969-c-gateway-verifier-diverges-from-rfc-and-has-no-production-implementation`

## Status

Open. Filed while attempting to land a production verification hook — the root
cause behind `0009-cf`, `0009-cg`, `0009-ci` and `0009-cj`. This is the missing
piece those four are waiting on.

## The finding

The gateway authenticator is the enforcement point the capability system is
built around. It cannot be used, for three separate and independently
sufficient reasons.

### 1. The trait has no production implementation

`GatewayAuthenticator::new` requires `Arc<dyn CapabilityVerifier>`. Measured:

| Item                          | Production callers / impls                                         |
| ----------------------------- | ------------------------------------------------------------------ |
| `impl CapabilityVerifier for` | **none** — 6 impls, all in `#[cfg(test)]` blocks or `tests/` files |
| `GatewayAuthenticator::new`   | **none** — only `authenticator.rs`'s own test module               |
| `impl BearerVerifier for`     | **none** — same shape                                              |

One implementation is named `ProductionCapabilityVerifier`. It is in
`octo-wallet/tests/dispatch_tv.rs`.

Because the trait has no production implementation, `GatewayAuthenticator`
cannot be constructed in production at all — this is not "wired up but unused",
it is "unbuildable".

### 2. The RFC's verifier algorithm references a method that does not exist

RFC-0969 §Algorithms specifies the capability branch as:

```rust
capability = Some(CapabilityVerification {
    holder_did: active.holder_did.clone(),
    ask_id: cap_token.ask_binding(),
});
```

**`CapabilityToken::ask_binding()` does not exist.** `CapabilityToken` has no
`ask_id` field and no such method. Ask binding is expressed in this workspace as
a **caveat** — `Caveat::AskBinding`, wire type `ask_binding`, domain
`cipherocto/cap/v1/caveat/ask_binding` — carried on the macaroon.

So an implementer following the RFC reaches for an accessor that is not there and
has to decide what to substitute. There is no obvious answer: the ask id is
inside a caveat list, and nothing today extracts it.

### 3. The dispatch semantics predate the RFC's own amendment

RFC-0969's v1.1 amendment removed cross-scheme linkage, on the stated grounds
that dispatch rejects dual-scheme requests:

> `AskBindingMissing` — REMOVED (single-scheme verification always has ask_id
> populated)
> `AskBindingMismatch` — REMOVED (cross-scheme linkage rejected at dispatch)

and the RFC's TV5 was retired as unreachable because "identity linkage requires
both schemes, which dispatch rejects".

**The implementation still does the removed thing.** `parse_auth_headers`
contains no dual rejection, `evaluate_linkage` compares bearer and capability
`ask_id` fields, and `authenticate` still returns `RoutingDecision::Dual`.

This matters for a specific reason: `CapabilityVerification.ask_id` is a
`[u8; 32]` with **no representation for "this capability is not ask-bound"**.
`HolderRecord::ask_id` is genuinely `Option<[u8; 32]>` — `None` for non-market
tokens and for HopCapability. Under dual semantics a verifier that fills an
unbound capability with a sentinel would be compared against the bearer's ask id
by `evaluate_linkage`, which does raw byte equality and has no zero case. That is
a false-linkage path. Under the RFC's v1.1 single-scheme semantics it cannot
occur, because the capability path is never compared to anything.

The divergence is therefore not cosmetic — it is the difference between an
interface defect being reachable and being inert.

## Why this is filed rather than attempted

A production `CapabilityVerifier` looks small: build a `VerifyContext` from the
clock and registry the authenticator already holds, call `verify_with_resolve`,
map the result. Every dependency is present (`quota-router-core` already depends
on both `octo-cap-macaroon` and `octo-wallet`) and the layering is legal.

What it needs first is a decision this mission should not make alone:

- **Where does `ask_id` come from?** From a `Caveat::AskBinding` on the macaroon,
  from the registry's `HolderRecord`, or from both with a consistency check. The
  RFC says `cap_token.ask_binding()` and the code has neither.
- **What does a verifier report for an ask-unbound capability?** The interface
  cannot say "unbound". Refusing is fail-closed and breaks non-market tokens;
  a sentinel is silent and depends on the dual semantics being dead.
- **Does the implementation adopt the v1.1 single-scheme dispatch, or does the
  RFC revert?** This decides whether the zero-`ask_id` linkage path is reachable.

## Acceptance Criteria

- [ ] `CapabilityVerification` can represent an ask-unbound capability, or the RFC states what a verifier reports for one and `evaluate_linkage` cannot compare it into a false link.
- [ ] `CapabilityToken` exposes the RFC's ask-binding accessor, or RFC-0969 §Algorithms is amended to name the caveat-based accessor that replaces it.
- [ ] Dispatch semantics are settled: either the implementation rejects dual-scheme requests per RFC-0969 v1.1, or the RFC revises the amendment. One of the two, not neither.
- [ ] A production `impl CapabilityVerifier` exists that calls real verification and is reachable from `GatewayAuthenticator::new` outside tests.
- [ ] Its failure modes are fail-closed per cause, not collapsed into one variant — `VerifyError` has seven variants and `CapError` has three.
- [ ] `a_forged_capability_is_refused_by_the_gateway` — mutation-verified, and named for what it proves rather than for the pipeline that hosts it.

## What must not happen

Do not implement the verifier by reusing `unverified_decode_capability` and
tightening the surrounding plumbing. That function derives `ask_id` from the first
32 bytes of the token string and `holder_did` from its first 8 characters — it
fabricates identity from untrusted input, which is what its name says. It belongs
in tests.

Do not let the implementation and the RFC stay diverged on dispatch semantics.
Every remaining question here is downstream of whether both schemes can be present
at once.

Do not resolve the unbound-`ask_id` case with a magic constant that no legitimate
ask can equal, without also recording why it cannot collide. A sentinel chosen
because it looks unreachable is an assumption, and `[0u8; 32]` has already been
assumed unreachable in this codebase once.

## Dependencies

- Mission `0009-cj-v2-wire-form-carries-no-capability-proof` — recorded the empty
  V2 wire form and the absent consumer
- Mission `0009-cf`, `0009-cg`, `0009-ci` — all waiting on an enforcement path
- Mission `0969-a-gateway-relocation` (relocated the authenticator to this crate)
- RFC-0969 §Algorithms, §Assumption IA-6 (capability `ask_id` is canonical)
- RFC-0957 §HolderRegistry
