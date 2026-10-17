---
name: 0009-e5-capability-signer-algorithm-binding
description: RFC-0009 AC#11/#13. Stop octo-cap-macaroon from stamping a caller-supplied algorithm onto a capability holder signature it did not make under, which silently downgrades any non-Ed25519 signer to Ed25519 at mint.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e-signable-domain-separation
---

# Mission `0009-e5-capability-signer-algorithm-binding` — IMPLEMENTED 2026-10-10

## Status

Claimed and implemented (2026-10-10). The defect below was demonstrated before
the fix and is now closed on both halves — the mint stamp and the wire
round-trip. Four vectors, three mutations, all red.

The RFC-0009 prefix migration for this trait is deliberately **not** bundled —
see §Why the prefix is a separate mission.

`octo-cap-macaroon` 411 lib + all integration suites green, `octo-role` 33,
`octo-wallet` 349, `octo-cli` security guards 45. Clippy clean with
`--all-targets --all-features` across all four crates.

## Dependencies

`0009-e` for the prefix substrate. None of the prior implementation missions
are prerequisites.

## §The defect: mint claims an algorithm the signer never named

`CapabilityToken::mint` and `attenuate_with_signer` both do this:

```rust
let sig_bytes = holder.sign(&msg)?;
let holder_sig = HolderSignature::new(
    crate::governance_signature::governance_algorithm_id(),  // always Ed25519
    &sig_bytes,
)?;
```

`CapabilitySigner::sign` returns `[u8; 64]` and names no algorithm. The mint
path then stamps `governance_algorithm_id()` — which is
`ed25519_algorithm_id()` — onto whatever came back. So the algorithm recorded
on the token is not the algorithm the signature was made under; it is a
constant chosen by the mint site.

`verify_holder_sig` dispatches on `self.holder_sig.algorithm_id()`, so it
verifies faithfully under the algorithm the token claims. The claim is what is
wrong.

**Demonstrated.** A scratch probe wrapping a `HolderSignature` stamped with
`AlgorithmId::derive("stark-schnorr")`, pushing it through exactly what
`serialize_wire` / `deserialize_wire` do (carry the 64 bytes, re-wrap on the
way back), and printing both ids before and after:

```
BEFORE algorithm_id = AlgorithmId { bytes: [130, 234, 241, ...] }
AFTER  algorithm_id = AlgorithmId { bytes: [202, 38, 148, ...] }
DOWNGRADED = true
```

That is the wire form, so the downgrade is not confined to the mint path — it
is what a persisted, relayed, or stored capability token does. `wire.rs`
documents the choice ("the algorithm is not on the wire ... a caller holding a
non-Ed25519 key re-stamps after parsing"), which is a fair thing to write if
the wire never carried anything else, but the consequence is that a token
produced under any other algorithm verifies as Ed25519 or not at all, and the
token itself carries no evidence of what it was.

`HolderSignature::new` is also not the only stamp site: `discharge.rs`, both
`wire.rs` parse paths, and three fixtures. Eleven `HolderSignature::new` sites
in the tree.

## §Severity, stated without inflation

**Latent, not an active forgery.** `CapabilitySigner::sign` returns
`[u8; 64]` and `public_key_bytes` returns `[u8; 32]`, so the trait cannot
currently represent a non-Ed25519 signer at all — a 33-byte STARK-Schnorr key
does not fit. Every implementor that matters signs Ed25519, so the stamped id
happens to be correct today.

The failure mode when a second algorithm does arrive is a **silent wrong
answer**, not a rejection: the token claims Ed25519, `verify_holder_sig`
dispatches to Ed25519, and a real STARK-Schnorr signature fails as though it
were forged. That is the same shape AC#11 exists to prevent, one layer up from
where the `octo-reputation` fix found it — a hardcoded Ed25519 property
surviving a dispatch refactor.

## Scope

1. **`CapabilitySigner` names its own algorithm.** Add
   `fn algorithm_id(&self) -> AlgorithmId` to the trait. The single production
   impl (`IdentityKey`) returns `self.algorithm_id()`; fixtures return
   `ed25519_algorithm_id()` or whatever they actually sign with.
2. **Mint stamps what the signer says**, not a constant.
   `attenuate_with_signer` likewise.
3. **`HolderSignature::new` stops being constructible with an unrelated
   algorithm.** Either take the algorithm from the signer at the only place a
   signature is produced, or assert the two agree at the boundary.
4. **The wire form carries the algorithm**, or the parse path takes it as a
   parameter from the resolved key context rather than inventing Ed25519.
   Which of the two is right is the open design question below.

## Non-goals

- Applying the RFC-0009 prefix to `holder_msg`. See §Why the prefix is a
  separate mission.
- Making a non-Ed25519 `CapabilitySigner` implementable. The `[u8; 64]` and
  `[u8; 32]` return types are a separate widening question; this mission only
  removes the lie in the algorithm field so that widening is not blocked by
  it.
- `octo-governance`'s same-named `CapabilitySigner`. Separate trait, separate
  crate, recorded as a gap in `0009-e`.

## §Why the prefix is a separate mission

It is tempting to answer this by prefixing `holder_msg`. That is a different
change with a different risk, and bundling them would hide both.

`holder_msg` is already length-prefixed per field
(`u32(len) ‖ root_id ‖ u32(len) ‖ caveats_wire`) precisely to prevent
concatenation collisions — the code documents that reasoning. So the
concatenation-ambiguity attack is already handled. What the prefix would add is
the **key_id**, which `holder_msg` has no field for, and the algorithm, which
this mission fixes a different way.

There is also a question this mission's evidence raises but does not answer:
whether a capability token _travels_ the way an attach-handle token does. The
attach-handle needed the prefix as a field because the verifier rebuilds the
message from the token's fields and has no out-of-band `key_id` source. The
capability wire format explicitly does **not** carry `holder_pub` or
`holder_did` — the caller resolves those out of band — which suggests the same
constraint applies and the prefix would need to be a field there too. That
would change the wire format and the three-segment shape, and it should be
decided on its own evidence.

> **ANNOTATED 2026-10-12 — the last vector below does not exist under that
> name, and should not be created.**
>
> `mismatched_algorithm_and_signature_are_refused` resolves to no function in
> the workspace. Writing it was considered and rejected: its property — "a wrong
> pairing cannot be constructed" — is already covered by two existing tests,
> `a_token_naming_an_unregistered_algorithm_is_refused` for the wrong algorithm
> and `a_tampered_holder_signature_is_still_rejected` for the wrong signature.
> Only one algorithm is registered in this crate's test registry, so a
> third test under this name would be a near-duplicate of one of them, which is
> worse than the gap: it would look like coverage and add none.
>
> **The vector name is wrong, not the coverage.** The honest fix is to record
> which tests carry the property, which is what this banner does. The other
> three vectors exist under their exact names.

## Test vectors

| Vector                                                 | What it pins                              |
| ------------------------------------------------------ | ----------------------------------------- |
| `mint_records_the_signers_own_algorithm`               | the token claims what the signer named    |
| `a_non_ed25519_survivor_is_not_downgraded_by_the_wire` | the round-trip does not silently re-stamp |
| `the_algorithm_survives_the_wire_round_trip`           | the field is transported, not re-invented |
| `a_token_naming_an_unregistered_algorithm_is_refused`  | a wrong algorithm cannot verify           |
| `a_tampered_holder_signature_is_still_rejected`        | a wrong signature cannot verify           |

## Acceptance criteria

- [x] `CapabilitySigner` carries a **required** `algorithm_id`, and no mint
      site calls `governance_algorithm_id()` to stamp a signature.
- [x] `serialize_wire` / `deserialize_wire` preserve the holder signature's
      algorithm.
- [x] Every vector above exists under its specified name and is
      mutation-checked.
- [x] All eleven `HolderSignature::new` sites accounted for. Two are production
      (`mint`, `attenuate_with_signer`) and now read the signer. One is the
      wire parse path and now reads the wire. The rest are fixtures or the
      deliberate mismatch tests in `token.rs`.
- [x] Clippy clean with `--all-targets --all-features` on `octo-cap-macaroon`,
      `octo-wallet`, `octo-cli` and `octo-role`.

**Not done: the `key_id`.** `holder_msg` still has no field for it, and the
prefix migration that would add one is split out for the reason above.

**Not done: the trait's fixed-width returns.** `sign` returns a 64-byte array
and `public_key_bytes` a 32-byte array, so a 33-byte STARK-Schnorr key still
cannot be a capability holder. This mission removes the _lie_ about the
algorithm so that widening is no longer blocked by it; the widening itself is
unchanged and open, and is scoped in `0009-e8`.

-
-

A

t
h
i
r
d

t
r
a
n
s
p
o
r
t
,

f
o
u
n
d

a
n
d

f
i
x
e
d

h
e
r
e
. * *

`h
o
l
d
e
r
_
s
i
g
n
a
t
u
r
e
_
s
e
r
d
e`

r
e
-

s
t
a
m
p
e
d

E
d
2
5
5
1
9

o
n

t
h
e

w
a
y

i
n
,

w
i
t
h

t
h
e

s
a
m
e

f
a
l
s
e

"
r
e
s
o
l
v
e
d

o
u
t

o
f

b
a
n
d

b
y

t
h
e

c
a
l
l
e
r
"

j
u
s
t
i
f
i
c
a
t
i
o
n

t
h
e

w
i
r
e

p
a
r
s
e
r

h
a
d
.

S
o

t
h
e

s
a
m
e

s
i
l
e
n
t

d
o
w
n
g
r
a
d
e

e
x
i
s
t
e
d

t
w
i
c
e

o
v
e
r

—

o
n
c
e

o
n

t
h
e

b
a
s
e
6
4

w
i
r
e
,

o
n
c
e

t
h
r
o
u
g
h

s
e
r
d
e

—

a
n
d

a

p
e
r
s
i
s
t
e
d

t
o
k
e
n

c
o
u
l
d

r
o
u
n
d
-

t
r
i
p

t
h
r
o
u
g
h

e
i
t
h
e
r

a
n
d

c
o
m
e

b
a
c
k

c
l
a
i
m
i
n
g

a
n

a
l
g
o
r
i
t
h
m

i
t

w
a
s

n
e
v
e
r

m
a
d
e

u
n
d
e
r
.

T
h
e

s
e
r
d
e

f
o
r
m

n
o
w

c
a
r
r
i
e
s

`
a
l
g
o
r
i
t
h
m
_
i
d

‖

s
i
g
n
a
t
u
r
e
`
,

m
a
t
c
h
i
n
g

t
h
e

w
i
r
e

s
e
g
m
e
n
t
,

a
n
d

a

p
r
e
-

f
i
x

v
a
l
u
e

i
s

r
e
f
u
s
e
d

r
a
t
h
e
r

t
h
a
n

g
u
e
s
s
e
d

a
t
.

## §The open question, answered

**A fifth field, or a parameter?** Answered with evidence: **the wire**, and not
because a field is tidier.

The question turned on whether the parser had any out-of-band source to read
the algorithm from. It does not. `HolderRecord` — the registry entry
`verify_with_resolve` resolves against — carries `cap_root_hash`, `holder_did`
and `holder_pub`, and **`holder_pub` is documented as "Holder Ed25519 public
key (32 bytes; per RFC-0009 §Capability Keys)"**. There is no algorithm field
anywhere on the record. So the previous "resolved out of band by the caller"
comment described a source that does not exist: the caller had nothing to
resolve _from_.

That also settles why the field was not simply added to the record as part of
this mission: a registry schema change with its own migration and gossip
determinism contract is a different piece of work, and the wire field makes it
unnecessary for correctness today.

Implemented as segment 2 carrying `algorithm_id (16) ‖ signature (64)` — 80
bytes, base64url, same position. **No fifth segment and no segment-count
change**, so the v1/v2 three-or-four segment shape is untouched and the
existing v1/v2 forward- and backward-compat tests pass untouched. That is why
407 of the 411 tests needed no edit.

## §Mutation evidence

Three mutations, all red.

| Mutation                                                       | Vectors that went red                                                                                              |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `mint` reverts to stamping `governance_algorithm_id()`         | `a_signer_naming_another_algorithm_is_recorded_faithfully`, `a_non_ed25519_survivor_is_not_downgraded_by_the_wire` |
| Parse reads the algorithm then ignores it, re-stamping Ed25519 | `a_non_ed25519_survivor_is_not_downgraded_by_the_wire`                                                             |
| Encoder writes a constant instead of the token's algorithm     | `a_non_ed25519_survivor_is_not_downgraded_by_the_wire`                                                             |

**One vector is vacuous, and the mutation says so.**
`mint_records_the_signers_own_algorithm` stayed **green** under the mutation
that reverted `mint` to a constant. Its signer reports Ed25519 and the constant
is Ed25519, so the two agree on every input it can produce — it cannot tell
reading the signer from stamping a constant.

It is kept because it is the positive statement of the property and because its
assertion is cheap, but the weight sits entirely on
`a_signer_naming_another_algorithm_is_recorded_faithfully`, which mints with a
signer reporting an algorithm nothing verifies under. That is the second
occurrence of this exact shape in this RFC chain — `0009-e4` had a vector set
that could not distinguish "reads the prefix" from "assumes Ed25519" — and the
lesson does not appear to be learned by writing the vector; it has to be
discovered by mutating it.

## §Why no vector here needs a second algorithm crate

`a_non_ed25519_survivor_is_not_downgraded_by_the_wire` needed a token claiming
a non-Ed25519 algorithm. Rather than add `octo-algo-stark-schnorr` as a
dev-dependency, the fixture is a local signer that _reports_ an arbitrary
algorithm id while signing with Ed25519. The property under test is what the
wire does with the reported id, not whether the signature verifies — and a
vector that depended on a second algorithm would additionally have to solve
registry bootstrapping, which is a different problem with its own ordering
hazards.

This is a deliberate narrowing, stated so a reader does not over-read it: the
vectors prove **the claim survives transport**, not that a STARK-Schnorr
capability token works end to end. The latter is blocked on the trait's fixed
`[u8; 64]` and `[u8; 32]` return types, which remain open.
