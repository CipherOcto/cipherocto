---
name: 0969-c-gateway-verifier-diverges-from-rfc-and-has-no-production-implementation
description: RFC-0969 specifies a capability verifier that reads ask_id via cap_token.ask_binding(). No such method exists — ask binding is a caveat on the macaroon. RFC-0969 v1.2 also replaced the dual-scheme dispatch table with a 4-way table rejecting both-schemes-present, but the implementation still carries the removed linkage machinery forward — relocated three days after the amendment, with BothSchemesUnsupported and NoAuth absent from the workspace entirely. Partly delivered: ask_binding(), the v1.2 dispatch, and a production CapabilityVerifier have all landed; what remains is a production mint that attaches AskBinding, without which the verifier correctly refuses every mintable capability.
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

Open, with one of the three blockers delivered. The accessor the RFC names now
exists; the semantics question it exposed is answered, and what remains is the
trait implementation and the dispatch catch-up.

Filed while attempting to land a production verification hook — the root cause
behind `0009-cf`, `0009-cg`, `0009-ci` and `0009-cj`.

## What has landed

`CapabilityToken::ask_binding()` — the accessor RFC-0969 §Algorithms names — now
exists on the token. Ask binding is `Caveat::AskBinding` on the macaroon rather
than a struct field, so the accessor reads the caveat list.

Writing it surfaced a design question that the RFC's one-line call does not
mention: **what happens when a capability carries two different ask bindings?**
That is reachable. Attenuation is monotonic and `attenuate` rejects neither a
repeated caveat nor a contradictory one, so an attenuator can append a second
`AskBinding` to a capability that already has one. A capability asserting two
asks has no single answer, and an accessor that returned the first would let
caveat order — not anything the verifier chose — decide what the verifier
believes.

So `ask_binding()` returns `Result<Option<AskId>, MacaroonError>`, and the new
`MacaroonError::AmbiguousAskBinding` fires on two or more **distinct** bindings.
Repeated _identical_ bindings agree, so they return the shared value rather than
an error. `Ok(None)` is a value, not a failure — it is the normal state for
non-market tokens and HopCapability.

Five tests, three mutations verified red:

| Mutation                                         | Tests that caught it        |
| ------------------------------------------------ | --------------------------- |
| ambiguity silently resolves to the first binding | 2 (both ambiguity tests)    |
| always reports unbound                           | 4 (every bound-path test)   |
| identical repeats wrongly treated as ambiguous   | 1 — exactly the test for it |

The third matters: it caught by exactly one test, which is what shows that test
is not redundant with the others.

**This does not resolve where the verifier's `ask_id` should come from.** It
supplies the token-side answer the RFC specifies. The open question in
§Why this is filed rather than attempted is whether the registry's
`HolderRecord.ask_id` must agree with it, and it remains open.

## One stub is the origin of three of this arc's findings

The gate sweep's TV9 result turned out to be load-bearing rather than incidental.
`mint_dual` is where a capability would receive its `Caveat::AskBinding` — the
RFC's `mint_dual(buyer_did, buyer_holder_pub, ask_id, …)` takes the ask id for
exactly that reason. So a single unimplemented function explains:

| Gap                                                    | Why it follows                                                 |
| ------------------------------------------------------ | -------------------------------------------------------------- |
| RFC-0969 TV9 has no test                               | TV9 is dual-issuance atomicity, which is what `mint_dual` does |
| No production mint attaches an ask binding             | `mint_dual` is the only path that would                        |
| The gateway `CapabilityVerifier` has nothing to verify | it would verify an ask-bound capability                        |

Three findings filed separately across this arc are one missing function.

### And two of its three stated blockers were already resolved

`mint_dual`'s own header listed three deferrals. Measured:

| Deferral                                                                                  | State                                                                                                                                   |
| ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| "RFC-0957-A1 4-arg mint deferred — current `CapabilityToken::mint` is 5-arg with catalog" | **stale.** `mint` now takes exactly `(root_secret, holder, holder_did, initial_caveats)` — no catalog                                   |
| "BearerCapsule mint with X25519 + ChaCha20-Poly1305 deferred"                             | **still true**                                                                                                                          |
| "Live `Transaction::insert_dual` deferred stub in 0957-c"                                 | **stale.** `insert_dual` is implemented over a real Stoolap transaction with drop-triggered rollback and a defence-in-depth commit path |

So the stub is blocked on **one** precondition, not three. Nobody re-evaluated
it because the note reads as current — the same failure as every other stale
doc this arc found, and the one that costs the most, because a stale blocker
list is an argument for leaving the work alone.

### The remaining blocker, stated precisely — and narrowed since

`BearerCapsule` is a structural stub with the right 3-field wire shape and a
`new()` constructor, but **no cryptography**: `encrypted_capsule` is arbitrary
bytes and `seller_signature` is an unverified 64-byte array. Nothing in the
workspace verifies that signature, and nothing constructs a `BearerCapsule`
outside tests — so it is inert rather than forgeable today. The real type is
owned by mission `0959-a1`.

That is the single thread to pull. Unblocking it releases TV9 and the
ask-binding mint.

**It did not release the verifier path, because it was never on it.** This
section previously claimed it did. See §The blocker named by this mission did
not block this work: the capability verifier shipped without `BearerCapsule`, and
what is actually blocked now is narrower than this section used to say.

## RFC-0969 gate sweep: three findings, one now closed

The doc sweep was closed, so this round swept the RFC's **own gates** — the
move that produced the RFC-0009 gate table earlier in the arc. RFC-0969 names
TV1–TV12; the implementation ships twelve tests, which sounds complete and is
not.

| Gate                              | State                                                                                                                                                                                                                                                                                                                                |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **TV9** (Dual-Issuance Atomicity) | **no test exists anywhere.** The RFC's own criteria list it separately as an "Integration test", so its absence from the unit file is expected — but it is absent entirely. It is also **unsatisfiable today**: `mint_dual`, the function it would test, is a stub that returns `Err(MintError::RootSecretMissing)` unconditionally. |
| **TV11**                          | **was a stale gate, now has a subject.** The v1.2 amendment retargeted TV11 to "Both-Schemes-Present — Rejected by Dispatch". The only test carrying the name still asserted the _pre-amendment_ expectation, that an ask mismatch surfaces as `AuthError::AskBindingMismatch` — a variant the amendment removed.                    |
| **TV12**                          | **two tests claim the number** — `tv12_cross_impl_routing_decision_is_identical` and `tv12_cross_impl_determinism_holds_for_indeterminate_path`.                                                                                                                                                                                     |

### TV11, closed

`tv11_both_schemes_present_rejected_by_dispatch` now exists alongside the stale
one. Both pass: the new test drives `authenticate_v12` with a bearer and a
capability whose bindings differ, and asserts `AuthError::BothSchemesUnsupported`.

The stale test is **left in place** — it exercises the pre-v1.2 `authenticate()`
path, which still exists, and deleting it is not on the table. Having both is
what makes the transition visible: the same gate name, two assertions, one of
which encodes superseded behaviour. Mutation-verified — removing the v1.2
refusal turns the new test red and leaves the old one green, so the new test is
measuring the v1.2 property specifically rather than duplicating coverage.

### What TV9 and the duplicate TV12 need

Both are recorded rather than fixed. TV9 needs `mint_dual` to exist before a
test for it means anything, and writing a test for a stub would be the exact
"passes while measuring nothing" trap this arc has refused twice. The TV12
collision needs a decision about which vector the RFC means, and the RFC is a
draft-facing artefact whose numbering should be settled there.

## What has landed, part 3: a guard, because the shape recurs

The verifier trait having no production implementation is not a one-off. It is
the shape this arc found repeatedly — an interface that reads as the enforcement
point, with nothing behind it. So it is now a gate rather than a note.

`every_verifier_trait_has_a_production_implementation_or_is_accounted_for`, in
`crates/octo-cli/tests/security_regressions.rs`.

Measured before building it, because a guard the measurement cannot support is a
comment with an assertion. Every trait whose name marks it as a verification
boundary, and where its implementations live:

| Trait                  | `impl`s | Production?                    |
| ---------------------- | ------- | ------------------------------ |
| `AuthorityPolicy`      | 1       | yes — `octo-policy`            |
| `PasskeyAuthenticator` | 3       | yes — `octo-adapter-whatsapp`  |
| `BearerVerifier`       | 5       | **no** — all in `#[cfg(test)]` |
| `CapabilityVerifier`   | 6       | **no** — all in `#[cfg(test)]` |

Two of four, and the two are exactly the gateway pair. Small enough to pin
exactly, with no sweeping allowlist.

**Why it pins a set rather than classifying.** A text scan cannot reliably tell a
production `impl` from one inside a `#[cfg(test)] mod` in a `src/` file — on the
page they are identical. That is the `CHAIN_DEPTH` lesson again: a scan that
finds something is not a scan that finds the _right_ thing. So the guard does not
guess. It pins the complete set of `impl <Trait> for <Type>` names and asserts
the measured set matches, which makes "these are all stubs" a claim that fails
the moment it stops being true.

Two mutations verified red, in both directions:

| Change                                 | Result                                                                                       |
| -------------------------------------- | -------------------------------------------------------------------------------------------- |
| a **real production verifier appears** | red, naming the impl and telling the implementer to _delete_ the entry rather than extend it |
| a recorded stub is renamed             | red, reporting both the new unrecorded name and the vanished recorded one                    |

The first is the scenario that matters, and the message it produces is the
useful part: the guard says the entry means "no production impl", so landing one
means retiring the entry, not adding a name to it.

The other two candidates (`AuthorityPolicy`, `PasskeyAuthenticator`) are
deliberately **not** in the table. A trait gaining its first production
implementation is the mission landing, not a regression, and putting them there
would mean the guard fires on success.

## What has landed, part 2: the v1.2 dispatch, additively

Full conformance needs four public items removed and the seventeen tests that
pin them deleted. **Deletion was declined**, so this is additive: the RFC's
dispatch now exists as new API, and the pre-v1.2 path is untouched.

| Added                                        | Where                    |
| -------------------------------------------- | ------------------------ |
| `RoutingDecision::BothSchemesUnsupported`    | `ingress::authenticator` |
| `RoutingDecision::NoAuth`                    | `ingress::authenticator` |
| `AuthError::BothSchemesUnsupported`          | `capability::dispatch`   |
| `GatewayAuthenticator::routing_decision_v12` | `ingress::authenticator` |
| `GatewayAuthenticator::authenticate_v12`     | `ingress::authenticator` |

`authenticate_v12` is the amendment's substance: the routing decision is
determined by the headers alone, and a both-schemes request is refused
**before either verifier runs**, which is what makes cross-scheme linkage
unreachable rather than merely unused. Verification itself is delegated to the
injected verifiers exactly as before — only the dispatch was rewritten.

Four tests, three mutations verified red:

| Mutation                                  | Caught by                                                 |
| ----------------------------------------- | --------------------------------------------------------- |
| both-schemes refusal removed              | `authenticate_v12_refuses_dual_schemes_without_verifying` |
| dispatch table collapsed to one arm       | 3 of 4                                                    |
| **verification moved before the refusal** | `authenticate_v12_refuses_dual_schemes_without_verifying` |

The third is the one that matters. A refusal that runs _after_ verification
still returns the right error, still satisfies any assertion on the returned
value, and is exactly the "passes while measuring nothing" shape this arc exists
to catch. It is caught only because the test counts verifier invocations rather
than checking the outcome.

**All seventeen pre-v1.2 tests still pass, unchanged.** `authenticate` still
returns `RoutingDecision::Dual`, `evaluate_linkage` still exists, and both
semantics are live in the crate at once. That is the accepted cost of the
additive route: the divergence is now _recorded and implementable_, not closed.

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

### 3. The implementation carries pre-v1.2 semantics forward, written _after_ the amendment that removed them

**This corrects an earlier reading of my own finding, which had the direction
backwards.** I first recorded the implementation as predating the amendment. The
dates say otherwise:

| Event                                                                               | Date           |
| ----------------------------------------------------------------------------------- | -------------- |
| RFC-0969 v1.2 dispatch amendment                                                    | **2026-08-08** |
| `0969-a-gateway-relocation`, which relocated `GatewayAuthenticator` into this crate | **2026-08-11** |

The relocation landed **three days after** the amendment, and it carried the
superseded semantics forward. That is a harder finding than an implementation
lagging its spec: the two were in the same file at the same time, and the
superseded half was moved rather than removed.

RFC-0969 v1.2 replaced the linkage machinery outright. Its dispatch table:

| Headers present | `RoutingDecision`            |
| --------------- | ---------------------------- |
| bearer only     | `Bearer`                     |
| capability only | `Capability`                 |
| **both**        | **`BothSchemesUnsupported`** |
| neither         | **`NoAuth`**                 |

and the amendment explicitly **removed** `LinkageResult::Indeterminate` +
`RoutingDecision::Dual` machinery, plus four `AuthError` variants
(`IdentityMismatch`, `AskBindingMissing`, `HolderPubMissing`,
`AskBindingMismatch`) replaced by a single `BothSchemesUnsupported`.

Measured in the implementation:

| RFC-0969 v1.2 requires          | Implementation                                                                    |
| ------------------------------- | --------------------------------------------------------------------------------- |
| `BothSchemesUnsupported`        | **absent — zero matches anywhere in `crates/`**                                   |
| `NoAuth`                        | **absent — zero matches anywhere in `crates/`**                                   |
| `RoutingDecision::Dual` removed | **present** — `Dual` is still a variant                                           |
| `AuthError` loses 4 variants    | **all four still present**, including `IdentityMismatch` and `AskBindingMismatch` |
| dual-scheme requests rejected   | `parse_auth_headers` accepts them; `authenticate` returns `RoutingDecision::Dual` |
| identity linkage removed        | `evaluate_linkage` still compares `subject_did` **and** `ask_id`                  |

The RFC's own §v1.2 row records that the relocation was prompted by an audit
finding "`0 production callers per audit 2026-08-08`". So the orphan status was
known at amendment time, and the semantics were still carried into the new home.

### Why the divergence is not cosmetic

`CapabilityVerification.ask_id` is a `[u8; 32]` with **no representation for
"this capability is not ask-bound"**. `HolderRecord::ask_id` is genuinely
`Option<[u8; 32]>` — `None` for non-market tokens and for HopCapability.

Under the RFC's v1.2 single-scheme semantics a sentinel can never be compared
against anything, because a capability is never evaluated beside a bearer. Under
the semantics the implementation actually has, `evaluate_linkage` compares
`ask_id` with raw byte equality and no zero case — so it can.

**The divergence is the difference between that interface defect being reachable
and being inert.**

## Why this is filed rather than attempted

A production `CapabilityVerifier` looks small: build a `VerifyContext` from the
clock and registry the authenticator already holds, call `verify_with_resolve`,
map the result. Every dependency is present (`quota-router-core` already depends
on both `octo-cap-macaroon` and `octo-wallet`) and the layering is legal.

What it needs first is a decision this mission should not make alone:

- **Where does `ask_id` come from?** **Answered: nowhere.** The token-side
  accessor exists, but no production mint path attaches a `Caveat::AskBinding`,
  so every producible capability is ask-unbound. That turns this from a design
  choice into a sequencing constraint — see §Where `ask_id` comes from.
- **What does a verifier report for an ask-unbound capability?** The interface
  cannot say "unbound". Refusing is fail-closed and breaks non-market tokens;
  a sentinel is silent and depends on the dual semantics being dead.
- **Does the implementation adopt the v1.1 single-scheme dispatch, or does the
  RFC revert?** This decides whether the zero-`ask_id` linkage path is reachable.

## Acceptance Criteria

- [x] `CapabilityVerification` can represent an ask-unbound capability, or the RFC states what a verifier reports for one and `evaluate_linkage` cannot compare it into a false link. **Done by refusal** — see §What landed part 4. The verifier never constructs a `CapabilityVerification` for an ask-unbound token, so no unbound `ask_id` exists to compare. The field itself is unchanged and still cannot represent absence; the route taken removes the need for it to.
- [x] `CapabilityToken` exposes the RFC's ask-binding accessor, or RFC-0969 §Algorithms is amended to name the caveat-based accessor that replaces it. **Done** — `ask_binding()` reads `Caveat::AskBinding` from the macaroon, since `CapabilityToken` carries no `ask_id` field.
- [x] Dispatch semantics are settled. **The RFC already answered this one** — v1.2's 4-way table is unambiguous and `BothSchemesUnsupported` / `NoAuth` are its names. The remaining question is only whether the implementation adopts it or the RFC is reopened, and adopting it is the default unless there is a recorded reason not to. **Adopted** — `authenticate_v12` + `routing_decision_v12`, additively; `authenticate` keeps pre-v1.2 semantics.
- [ ] A production mint path attaches a `Caveat::AskBinding`, so a capability that is ask-bound can exist. **Still open, but no longer a substrate gap — the shape now exists and only needs to be used.** A capability can carry an ask binding today: attenuating `Caveat::AskBinding` through the wallet's attenuation handler attaches it, and the result verifies and reads back through `ask_binding()`. What is missing is a caller that does so on the market path. Owned by RFC-0959 §Phase 3, where the item is already an unchecked box — this is not new work, it is work that RFC already tracks and nobody has ticked.
- [ ] `CapabilityVerification.ask_id` can represent "unbound", matching the `Option` RFC-0970 already assumes `ask_binding()` returns. A bare `[u8; 32]` cannot, and that value flows into `AuthenticatedRequest.ask_id` and on into settlement. **Deliberately not done.** Refusing unbound is strictly safer than a representable absent, and is reversible; widening the field is neither. Revisit only if the market path needs unbound capabilities at the gateway, which RFC-0959 §Phase 3 would have to argue for.
- [x] A production `impl CapabilityVerifier` exists that calls real verification and is reachable from `GatewayAuthenticator::new` outside tests. **Done** — `quota_router_core::ingress::capability_verifier::ProductionCapabilityVerifier`, injected as `Arc<dyn CapabilityVerifier>`, so `GatewayAuthenticator::new` needed no change.
- [x] Its failure modes are fail-closed per cause, not collapsed into one variant — `VerifyError` has seven variants and `CapError` has three. **Done** — `CapError` gains `Malformed`, `HolderUnknown`, `HolderNotActive`, `RegistryUnavailable`, `InvalidSignature`, `AttestationRefused`, `AskUnbound`, `AskBindingAmbiguous`. One variant per cause; none carries a message across.
- [x] `a_forged_capability_is_refused_by_the_gateway` — mutation-verified, and named for what it proves rather than for the pipeline that hosts it. **Done**, and it is the only test that catches a disabled holder-signature check.

## The blocker named by this mission did not block this work

The mission recorded `BearerCapsule` as the thing holding the capability verifier
back. **That is wrong, and it is wrong in a way worth naming.** `BearerCapsule` is
the RFC-0903 _bearer_ path plus the `mint_dual` stub. Neither is on the capability
verification path. Everything that path needs already existed:

| What the verifier needs             | Where it was                                               |
| ----------------------------------- | ---------------------------------------------------------- |
| wire parse + registry resolve + sig | `verify_with_resolve` (shipped by `0957-d`)                |
| the ask, off the macaroon caveats   | `CapabilityToken::ask_binding()` (shipped by this mission) |
| holder authoritative on identity    | `VerifyContext::with_registry` (`0957-d`)                  |

So the verifier was not blocked; it was unwritten. This is the third time this
arc an inherited blocker turned out to be narrower than recorded, and the first
where the blocked work was the work in progress.

## What landed part 4: the verifier, and the refusal it is built on

`ProductionCapabilityVerifier` composes `verify_with_resolve` and
`ask_binding()`. Ten tests, four mutations, each caught by exactly the test that
claims the property:

| Mutation                                          | Caught by                                                                                                        |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| fill unbound with `[0u8; 32]` instead of refusing | `an_ask_unbound_capability_is_refused_not_zero_filled`, `two_unbound_capabilities_never_produce_the_same_ask_id` |
| collapse all seven causes into `MacaroonInvalid`  | the five per-cause tests                                                                                         |
| disable the holder-signature check                | `a_forged_capability_is_refused_by_the_gateway` **only**                                                         |
| disable the revoked/active check                  | `a_revoked_holder_is_refused` **only**                                                                           |

**The refusal.** An ask-unbound capability is refused (`CapError::AskUnbound`)
rather than given a sentinel, because `evaluate_linkage` compares `ask_id` bytes
directly: two _different_ unbound capabilities held by the same subject would
compare equal and read as linked to a shared ask neither asserted. Refusing is
the fail-closed direction and is reversible.

**The cost, stated rather than buried.** No production mint attaches an
`AskBinding`, so **every capability the system can mint today is refused by this
verifier.** That is correct verifier behaviour and a broken deployment state.
The two are tracked apart: the verifier is right, and RFC-0959 §Phase 3 is the
open item above.

## What this unblocked: ask binding is now attachable

The refusal above reads as a dead end until you notice what it rules out. It
rules out attaching an ask at _mint_ time, because `MintRequest` has no ask
field and adding one is a wire change.

It does **not** rule out attaching one by _attenuation_. That route needed no
protocol change and, as of the attenuation fix, it now works: attenuating
`Caveat::AskBinding` through `WALLET_ATTENUATE_CAPABILITY` produces a token that
passes `verify_holder_sig` and whose `ask_binding()` returns the ask. That was
not possible before the fix, because the attenuate handler did not re-sign and
every token it produced failed verification.

So the remaining item is no longer "build a substrate". It is "call the thing
that exists" — a market-side decision about _when_ a capability is bound to an
ask, owned by RFC-0959 §Phase 3. That is a smaller and more honest mission than
the one this file was carrying when the verifier shipped.

## The guard retired itself, which is the point of a guard

`every_verifier_trait_has_a_production_implementation_or_is_accounted_for` went
red on the new type and named its own remedy — _"If this is a real production
verifier, DELETE the entry for CapabilityVerifier"_. That is exactly the moment
the guard was built for, and it is the first evidence that it measures something.

`BearerVerifier` stays on the list; its substrate is still a stub.

**Coverage narrowed, honestly.** With `CapabilityVerifier` removed, the guard no
longer watches that trait at all. A stub added _alongside_ the real verifier
would not be caught. That is a different risk from the one it was written for and
is not covered by it — recorded here rather than left for someone to assume.

## Questions the RFCs answer on their own

Recorded so they are not re-opened as open questions:

- **Does the implementation lag the RFC, or does the RFC need to move?** The RFC
  moved on 2026-08-08 with an explicit dispatch table and an explicit list of
  removed variants. The implementation relocated on 2026-08-11 carrying the
  removed machinery. The default is that the implementation catches up.
- **Is the zero-`ask_id` false-linkage path reachable today?** **No, and it is now
  closed by construction rather than by accident.** It used to be unreachable only
  because no verifier existed to report an unbound ask. A verifier now exists and
  refuses rather than sentinel-filling, so the sentinel path has no producer. A
  test pins that: two unbound capabilities for the same holder never both yield
  an `ask_id`, because neither yields one at all.

## The semantics question is answered by the RFCs, not by a decision

The last open item was what a verifier reports for an ask-unbound capability.
It does not need to be chosen — three accepted RFCs already say.

**RFC-0970 §Forwarding Hop Auth Envelope states the accessor's return type:**

> `ask_binding()` returns `Option<[u8; 32]>` so the types don't match

That is the method just implemented, with the `Option` the RFC names. Two things
follow. The accessor was _expected to exist_ — RFC-0970 was written against it.
And unbound is a **first-class state**, not a failure, because the return type
carries `Option`.

The same RFC passage calls out a sibling `audience_did_for_check()` as a
"phantom method", which is the identical defect class as RFC-0969's missing
`ask_binding()`: a spec calling a method the substrate never had. Two accepted
RFCs, the same mistake.

**Ask binding is a market-path concern, not a universal one:**

| Evidence                    | Says                                                                                               |
| --------------------------- | -------------------------------------------------------------------------------------------------- |
| RFC-0957 caveat schema      | `AskBinding(AskId), // bind capability to a specific Ask (RFC-0959)`                               |
| RFC-0959 §Economic exploits | `ask_id` binding is the mitigation for _rate-table switch after settlement_ — a marketplace attack |
| RFC-0959 §Phase 3           | unchecked: "Bind Ask identity into RFC-0957 `AskBinding` caveat payload"                           |
| `HolderRecord::ask_id`      | `None` for non-market tokens and for `HopCapability`                                               |

So: attach the binding on the settlement path, not on every mint, and let
`Ok(None)` stand everywhere else.

### The consequence, which is a type change rather than a choice

A verifier that reports `ask_id` faithfully must be able to say "unbound".
`CapabilityVerification.ask_id` is a bare `[u8; 32]` and cannot.

Under RFC-0969's v1.2 single-scheme dispatch this is only a reporting defect —
the value is never compared — but it is still a defect, because a capability-only
request flows that `ask_id` straight into `AuthenticatedRequest.ask_id` and on
into settlement. Today the only thing preventing a fabricated binding is that no
production verifier exists. That is the same "unreachable today, reachable the
moment someone fills in the trait" property as before, one type over.

The honest fix is `Option<[u8; 32]>` on `CapabilityVerification`, matching what
RFC-0970 already assumes. Whether to land that before or after the v1.2 dispatch
catch-up is a sequencing judgement: removing the dead linkage first retires the
comparison hazard, and doing the type change first leaves work to be undone.

## Where `ask_id` comes from: answered, and the answer is "nowhere"

The third question was left open pending this. It has an answer, and it is a
negative one.

`CapabilityToken::ask_binding()` now exists and works. **No production code path
constructs a `Caveat::AskBinding`.** Measured across every construction site:

| Mint path                                    | Caveats attached                                                                                                                                      |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `octo-wallet-node` mint handler — payment    | `&[Caveat::Payment(p)]`                                                                                                                               |
| `octo-wallet-node` mint handler — no payment | `&[]`                                                                                                                                                 |
| `octo-cli` capability mint                   | unreachable: inside a `#[cfg(test)]` block, behind a `#[cfg(not(test))]` guard that refuses in release builds with "root secret derivation not wired" |
| `mint_dual` (RFC-0969 §Algorithms)           | a stub that always returns `Err(RootSecretMissing)`                                                                                                   |

So **every capability the system can actually produce reports `Ok(None)` from
`ask_binding()`.** The bound case exists only in test fixtures.

This falsifies the premise RFC-0969 used to justify its own amendment. v1.2
removed `AskBindingMissing` because, in its words, "single-scheme verification
always has ask_id populated". Against this codebase nothing populates it. The
amendment's justification does not hold here, which is a separate reason to
revisit it beyond the implementation lag.

### What this means for the verifier

It converts a design question into a sequencing one. The blocker is **not**
"where does the verifier read `ask_id` from" — it is that the value it must
report **is never minted**.

A verifier cannot be written correctly against this substrate:

- Reporting `Ok(None)` faithfully means refusing every capability that exists,
  which is not a verifier, it is an outage.
- Reporting a sentinel means the zero-`ask_id` linkage risk becomes live the
  moment dual-scheme dispatch is reconciled — which is the very next step.

So the honest order is: **mint the ask binding, then implement the verifier.**
Doing it the other way round forces a choice between refusing everything and
inventing a sentinel, and both are worse than waiting.

This is the same shape as the rest of the arc's findings — the enforcement path
is not merely missing, the thing it would enforce has never been produced.

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
