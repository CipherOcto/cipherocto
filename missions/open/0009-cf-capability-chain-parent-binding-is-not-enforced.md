---
name: 0009-cf-capability-chain-parent-binding-is-not-enforced
description: RFC-0009 threat model A10. compute_chain_parent and verify_chain_parent now exist, byte-pinned and mutation-verified, but the documented defence is still absent rather than merely untested. chain_parent lives on CapabilityTokenV2 inside the V2 bundle; the one production capability verifier consumes the v1 macaroon wire, which carries no chain state, and nothing decides anything from a V2 bundle at all. Every production mint writes the zero value, and the attenuation path that would extend a hierarchy never touches a bundle. Two independent gaps — no reader and no writer — and the writer must land first or a chain check would pass while measuring nothing.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-ce-a-type-alias-hides-the-same-pin
---

# Mission `0009-cf-capability-chain-parent-binding-is-not-enforced`

## Status

Partially delivered. The binding itself now exists; the enforcement path it was
to be wired into does not, and did not.

Filed from measured evidence, not from a failing test. Amended 2026-10-12 after
implementation exposed a scoping error in the original criteria.

## Amendment (2026-10-12): the wiring criterion was vacuous

The third acceptance criterion below reads:

> Every path that verifies a `CapabilityTokenV2` consults `verify_chain_parent`
> before accepting the token.

Measured before writing any code, that criterion had **zero paths to satisfy**.

The capability verification production performs is `verify_capability` in
`octo-wallet`'s capability module. It works on the V1 macaroon substrate and
contains no reference to `CapabilityBundleV2` or `token_v2` — measured, both
terms return zero matches. `CapabilityBundleV2` appears in exactly three
non-test places: the wallet mint handler, the capability issuer handler, and
`octo-cap-zk`'s inert proof-bundle field. **All three emit it. None consumes
it for a trust decision.**

So an implementer could tick that box by doing nothing at all, and a reviewer
checking it would find nothing to check. That is the same trap the
must-not-happen section below was written to prevent — it just arrived one level
up, in an acceptance criterion rather than in a test.

The chain parent binding is one field inside a token type that no verifier
reads. Fixing the binding does not make it enforced; it makes it _enforceable_.

## What has landed

`octo-cap-macaroon` now has a `chain_parent` module implementing
`compute_chain_parent` and `verify_chain_parent` exactly as RFC-0009 specifies,
including the one-byte depth encoding the RFC explicitly warns against
confusing with the four-byte information-string form.

The derivation carries a byte-exact vector that was cross-checked against a
BLAKE3 implementation outside this crate, so it pins the formula rather than
merely recording whatever the code emitted. Four mutations were verified to turn
it red:

| Mutation                        | Tests that caught it                          |
| ------------------------------- | --------------------------------------------- |
| 4-byte depth instead of 1-byte  | 3 (binding length, pinned digest, 1-byte pin) |
| parent/child order inverted     | 2 (binding layout, pinned digest)             |
| depth cap check removed         | 1 (`depth_past_the_cap_is_refused`)           |
| predicate made to always accept | 3 (forgery gate, commitment, zero-value)      |

The last one is the important one: it is the mutation that turns this into a
suite that passes while measuring nothing, and three tests catch it.

### Why the AC#11 guard does not flag this new code, verified rather than assumed

`verify_chain_parent` takes two `[u8; 32]` parameters and its name starts with
`verify`, so it is exactly the shape the AC#11 guards were built to catch. It is
in a crate those guards scan (`crates/octo-cap-macaroon/src` is in
`AC11_DISPATCHED`), and it is not flagged.

That was confirmed by planting a genuine violation inside the new module: a
`verify_dispatch` call taking `public_key: &[u8; 32]` in the key's own
argument position. The guard went red and named the file and the binding. So the
scan covers this file, and the silence is a real result rather than a file the
guard never read.

The substantive reason it is out of scope: the guard keys on
`verify_dispatch(`, because AC#11 is about sites that _choose_ a verification
algorithm. `verify_chain_parent` chooses none. It hashes two capability root
digests, and a BLAKE3 digest is 32 bytes for every algorithm that exists and
every algorithm that will. Widening those parameters would not enable a second
algorithm; it would only destroy the self-delimiting property that lets the
three fields concatenate without length framing.

The parameters are named `*_cap_root` rather than the RFC's `*_cap_key` because
that is what they are — `HolderRecord::cap_root_hash`, a digest, not a public
key. The RFC's `CapabilityKey` type does not exist in this workspace.

## The finding

RFC-0009 §Threat model A10 specifies the defence plainly:

> `verify_chain_parent` checks `blake3(parent || child || depth) == chain_parent`;
> forgery requires inverting BLAKE3.

Measured across the workspace **at the time this was filed**:

| Claim                                | State                               |
| ------------------------------------ | ----------------------------------- |
| `compute_chain_parent`               | did not exist                       |
| `verify_chain_parent`                | did not exist                       |
| `CapabilityTokenV2::chain_parent`    | documented as carrying that binding |
| Any code reading the field to decide | **none**                            |

The first two rows are **stale by design and marked so**: both functions have since
landed — see §What has landed below. A reader landing on this table first should
take the last two rows as current and the first two as history. They were left
rather than silently updated because the whole point of the table is the
as-filed measurement, and a table that quietly rewrites itself stops being
evidence.

Every occurrence of the field is an assignment, a `Debug` print, or a test
asserting the value round-trips.

**Every production mint writes `chain_depth: 0` and `chain_parent: [0u8; 32]`.**
The single non-zero depth in the tree is a fixture in `octo-cap-zk`'s tests. So
hierarchical capability chaining is not implemented, and the parent binding is
inert.

## Why this is filed rather than closed

The field documentation asserted a cryptographic binding that no code performs.
That has been corrected — the field now says it is reserved, names the missing
functions, and states that every mint writes the zero value. Correcting the
documentation does not create the defence.

**The risk is latent, not live.** There is no chain to forge today, precisely
because the feature is unused: the attack surface is empty for the same reason
the defence is missing. What exists now is a trap for whoever adds chained
issuance — the field looks bound, and would not be.

## Amendment: the enforcement point exists, and it is aimed at the other artifact

The arc that produced this mission concluded that four sibling missions shared one
root cause — **there was no production capability verification path anywhere** —
and recommended landing a verification hook before building enforcement against
it. That hook has landed: `ProductionCapabilityVerifier` in `quota-router-core`
composes `verify_with_resolve` and `CapabilityToken::ask_binding`, and it is
mutation-verified and proven reachable through `GatewayAuthenticator`.

**It does not call `verify_chain_parent`, and it cannot.** Not an oversight on
its side. It is handed a string by `CapabilityVerifier::verify` and parses it as
the **v1 macaroon wire**. That wire carries `macaroon`, `holder_sig` and
discharges. It has no `chain_parent`, no `chain_depth`, and no notion of a
delegation hierarchy at all.

So the two representations never meet:

|                  | where it lives                                      | who reads it                | who writes it                        |
| ---------------- | --------------------------------------------------- | --------------------------- | ------------------------------------ |
| v1 macaroon wire | the `Authorization: CipherOcto-Cap` header          | the one production verifier | `CapabilityToken::mint`, `attenuate` |
| V2 bundle        | `CapabilityTokenV2` in `CapabilityBundleV2Envelope` | **nothing**                 | every production mint, zeros         |

`bundle_v2`'s own module documentation already says this: _"V2 bundles are
minted and transported; nothing decides anything from them."_ The chain metadata
lives entirely on the side that nothing decides from.

### Why "just wire it up" is not the work

The missing call is the smaller half. There are **two** independent gaps, and
closing the reader without the writer would leave something worse than nothing:

1. **No reader.** No code presents or decides on a V2 bundle.
2. **No writer, and the obvious writer does not even touch it.** Every production
   mint writes `chain_depth: 0` and `chain_parent: [0u8; 32]`. The one operation
   that _should_ extend a hierarchy is attenuation — and the wallet's
   `AttenuateHandler` operates on the v1 macaroon wire and returns a v1 wire. It
   never constructs, reads or updates a V2 bundle, so attenuating a capability
   grows the macaroon's HMAC caveat chain while the bundle's `chain_depth` and
   `chain_parent` stay exactly as minted.

The zero written by every mint is _load-bearing for the current safety story_:
there is no non-root token, so there is no parent to forge. A verifier wired to
read bundles without a mint that can produce a non-root one would gain no
capability to check anything — the same class of change as the AC#11 guard that
"passes while measuring nothing".

So the sequence matters: **a non-root mint must exist before a chain check can
mean anything**, and the AC below is ordered that way. This is a stronger claim
than "the criterion was vacuous", which is what the earlier amendment recorded:
the criterion was not vacuous, it was **premature**.

### Two constraints on the writer that shape it more than expected

Measured while scoping this, and both are the kind of thing that costs an
implementer a day to rediscover.

**1. The v1 wire carries no chain metadata at all.** `Macaroon` is
`root_id + root_secret_hash + caveats + chain`. There is no `chain_depth` and no
parent pointer anywhere in it. So the attenuate handler cannot recover the
_parent's_ depth from the token it was handed, and RFC-0009's formula needs the
child's depth to compute the binding. **The parent depth has to arrive in
`AttenuateRequest`.** Deriving it from the caveat count would be wrong, not
merely inelegant — caveat count tracks attenuation steps, while `chain_depth` is
hierarchical position, and the two diverge the moment a capability is delegated
sideways rather than down.

**2. The two wallet handlers disagree about their own output wire form.**
`MintHandler` returns a `CapabilityBundleV2Envelope`; `AttenuateHandler` returns
a v1 macaroon wire. So a capability is a V2 bundle on the way out of mint and a
v1 wire on the way out of attenuate. **Attenuation discards the very artifact
that carries the chain metadata.** That is the mechanical explanation for why the
hierarchy is inert: mint creates chain metadata, attenuate drops it, and the
verifier never saw either.

Any writer has to make those two agree. Deciding _which_ form attenuate should
return is a protocol decision rather than an implementation detail — the answer
determines whether the v1 wire is still a credential or becomes an internal
substrate form only, which in turn decides what the gateway verifies.

### The stale claim this turned up

`octo-cap-zk` carried a test comment asserting `compute_chain_parent` and
`verify_chain_parent` "do not exist". That was true when written and became false
when the functions landed. Corrected in the same change, keeping the part that
matters: the `[0xCC; 32]` parent in that test is still an arbitrary constant that
nothing computes or checks, and nothing in production still calls either
function.

## Acceptance Criteria

Reordered after implementation. The first two are done; the third was found to
be vacuous and has been replaced with the criterion that is actually blocking.

- [x] `compute_chain_parent(parent_cap_key, child_cap_key, child_depth)` exists and returns the BLAKE3-256 binding the field documents.
- [x] `verify_chain_parent` recomputes that binding and rejects a mismatch. Mutation-verified against a predicate that always accepts.
- [ ] **A non-root token can actually be minted.** Moved ahead of everything below, and promoted from optional to blocking. `AttenuateHandler` is the natural home — it is the operation that extends a chain and it already holds the wallet's identity key — but it would have to begin emitting a V2 bundle whose `chain_depth` and `chain_parent` reflect the attenuation, which today it does not do at all. **Until this is done there is nothing for a chain check to check**, and any verifier wired to bundles below would pass while measuring nothing.
- [ ] The wallet's two handlers agree on one output wire form, so attenuating a minted capability does not silently discard the bundle carrying its chain metadata. Deciding _which_ form wins is a protocol decision: it determines whether the v1 wire remains a credential the gateway verifies or becomes an internal substrate form only.
- [ ] **A V2 verification path exists.** This is the reader half. The capability verifier must gain a route by which a `CapabilityBundleV2` can be presented and decided on — or the V2 bundle must be retired as a wire form. A production verifier now exists and consumes the _other_ artifact, so this is no longer "no enforcement point anywhere"; it is "the enforcement point is pointed at the representation that has no chain state". Until one of those happens, no wiring criterion below is checkable.
- [ ] That path consults `verify_chain_parent` before accepting a token with `chain_depth > 0`, and treats depth 0 as a root rather than running the binding.
- [ ] A missing parent in the catalog is refused rather than treated as a root token. (`check_wrapped_chain`'s revocation walk is out of scope here; it needs `0009-cg`.)
- [x] `chain_parent_forgery_breaks_verification` — the gate A10 names — exists and is mutation-verified: forging a parent value turns it red.
- [ ] A test proves the _writer_ half rather than the formula: attenuate a real capability and assert the emitted bundle's `chain_depth` increments and its `chain_parent` equals `compute_chain_parent` of the real values. A test that constructs the bundle by hand, as `octo-cap-zk` currently does, measures serde and nothing else.

## What must not happen

A test that constructs a token with a wrong `chain_parent` and asserts only that
the value round-trips. That reads as coverage of A10 while measuring nothing,
and the property under test would not exist in the code being tested.

The same trap exists one level up, and it has already been sprung once: an
acceptance criterion phrased as "every path that verifies X does Y" reads as
coverage while being satisfiable by an empty set. **Do not restore the wiring
criterion above without first establishing that the path exists.** A criterion
over an empty population is not a gate; it is a comment with a checkbox.

## Dependencies

- Mission `0009-ce-a-type-alias-hides-the-same-pin` (recorded the finding)
- RFC-0009 §Threat model A10
- RFC-0871 capability chain material
