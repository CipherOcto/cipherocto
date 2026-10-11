---
name: 0009-cc-key-length-still-binds-the-algorithm
description: RFC-0009 AC#11 follow-on. Every dispatched verification site takes &[u8; 32], and stark-schnorr public keys are 33 bytes, so no site can accept the second algorithm. The one cross-algorithm test that appears to cover this is vacuous.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "2.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-cb-dot-verify-takes-key-bytes-not-verifying-key
---

# Mission `0009-cc-key-length-still-binds-the-algorithm` — COMPLETE 2026-10-10

## Status

Complete 2026-10-10. Found by testing the premise `0009-cb` rested on rather
than the premise itself.

### The scope figure in this file was wrong when it was written

It first claimed "79 parameters across 5 crates", measured by grepping for
parameter _names_ like `pubkey: &[u8; 32]`. That conflates every 32-byte array
in the workspace — macaroon root secrets, capability ids, Blake3 keys, field
elements, nonces — with public keys. Almost none of them are public keys, and
all of them genuinely are 32 bytes.

Measured properly, by finding parameters whose value actually reaches a
`verify_dispatch` call, the figure is **38 functions across 7 crates**. The
first number was a name match; this one is a role match, and the same lesson
as counting constructor calls instead of verification sites.

## `0009-cb` fixed the type and left the length

`0009-cb` changed 25 `dot/` verifiers from `key: &VerifyingKey` to
`key: &[u8; 32]`, on the reasoning that a caller holding a non-Ed25519 key could
not construct an `ed25519_dalek::VerifyingKey`, so the type had pre-decided the
algorithm before the dispatcher got a vote.

That reasoning was sound and the fix was incomplete. The array is still sized
for Ed25519:

- `octo-algo-stark-schnorr`'s `StarkSchnorrAlgorithm::PUBLIC_KEY_LEN` is
  **33**.
- Ed25519's is 32.

So a real stark-schnorr key cannot be passed to any of the 25 sites. The
dispatch parameter is accepted and used; the key type still decides in advance
which algorithm could possibly apply.

`&[u8; 32]` is a **workspace-wide convention, not something `0009-cb`
introduced**. `dot/envelope.rs` already used it before any of this work, and
`0009-c7`, `0009-c9` and `0009-ca` followed it. Measured by role: **38** verification
functions across `octo-network` (34), `octo-reputation`, `octo-runtime`,
`octo-wallet` and `quota-router-sm-engine`.

## The test that appears to cover this is vacuous

`crates/octo-wallet/tests/lifecycle_replay.rs` contains
`a_revocation_proof_replayed_as_the_other_algorithm_is_rejected`, whose doc
comment says the prefix exists so a caller cannot replay one algorithm's proof
as the other's. It is the only place in the workspace that names
`stark_schnorr_algorithm_id()` outside the algorithm crate itself, and it
therefore looks like the cross-algorithm coverage the whole arc was missing.

**It is not.** Proved by mutation: hardcoding the algorithm inside
`IdentityKey::verify_lifecycle_proof` — replacing the `algorithm_id` argument
to `verify_dispatch` with a literal `ed25519_algorithm_id()` — leaves all three
tests in that file **green**, including the cross-algorithm one.

The reason is the same shape as every vacuous guard this arc has found. The
test passes an Ed25519 key (`IdentityKey::public_key_bytes() -> [u8; 32]`) and
an Ed25519 proof, named as stark-schnorr. Both a dispatching verifier and a
hardcoded Ed25519 one refuse that, because the signature genuinely is not a
stark-schnorr signature. Rejection is guaranteed by the input, so it carries no
information about whether the algorithm argument was honoured.

The one vector in the workspace that appears to prove a second algorithm is
reachable proves nothing about it.

## What a real vector needs

Both halves, and neither exists yet:

1. **A stark-schnorr keypair and signature.** `octo-algo-stark-schnorr` has
   51 passing tests including reference vectors, so the material can be made.
2. **A verification site that accepts a 33-byte key.** Which is the work above.

With those, the vector is a round trip: sign under stark-schnorr, verify
through the dispatched site, expect `Ok`. Mutation then flips it, because a
hardcoded Ed25519 verifier cannot produce `Ok` from a 33-byte key and a
stark-schnorr signature.

That is the same shape as `dot_envelope_a_second_algorithm_reaches_the_verifier`
from `0009-cb`, except that one could only reach an _unregistered_ algorithm.
This one has to reach a **working** one.

## The change

`&[u8; 32]` becomes `&[u8]` at verification sites, matching
`verify_dispatch`'s own signature — which has taken `public_key: &[u8]` since
the registry landed.

The length check is not lost, it moves to the layer that owns it. Each
algorithm registration already does this explicitly and distinguishes it from a
bad signature:

```rust
if public_key.len() != Self::PUBLIC_KEY_LEN {
    return Err(SignatureError::BadKeyLength { alg, got, expected });
}
```

That is strictly better than a compile-time length, because it reports _which_
algorithm wanted _what_ length, and it keeps working when a third algorithm
lands with a different size.

## Scope — as landed

38 functions. Call sites needed almost no change, because `&[u8; 32]`
**coerces** to `&[u8]`, so widening a parameter does not churn its callers.
That is what made this tractable as one piece.

Three things came out of the work that were not in the scope above:

1. **`octo-reputation`'s governance digest chain.** `verify_governance_signature`
   already took `&[u8]` with the reasoning documented in full — including why
   there is no length pre-filter. Its _key_ was still `&[u8; 32]`, so the
   digest and preimage it feeds had to widen with it. The zero-key sentinel
   was `*pubkey == [0u8; 32]`, a length-dependent check that would silently
   never fire for a 33-byte key: a check that looks present and is
   unreachable.

2. **`octo-network`'s gossip verifier had the defect `octo-reputation` had
   already removed.** `verify_envelope_signature` opened with
   `if env.recorder_signature.len() != 64`. That is a hardcoded Ed25519
   signature length one layer above the dispatch it appeared to support,
   reporting `recorder_signature_wrong_length` — indistinguishable from a bad
   signature. It was invisible for stark-schnorr only because those
   signatures are also 64 bytes. Removed, with the reasoning recorded at the
   site.

3. **`octo-algo-stark-schnorr` could not be used to sign from outside.**
   `sign` and `public_key` both take a `&Felt`, and `Felt` was imported
   privately in `scheme`. The crate was verify-only to any external caller.

   That last one is why the vector below was unwritable, and it is the reason
   the whole arc produced no cross-algorithm coverage: not a missing test, but
   a missing API. `Felt` is now re-exported.

## Why this was not folded into `0009-cb`

`0009-cb` verified its own claim, and its claim was true as stated: it was
about the _type_. Discovering that a second layer existed required measuring
stark-schnorr's key length, which is a different question and a different
mission. Recording it rather than silently widening `0009-cb`'s scope is the
honest split — `0009-cb`'s mission file should not claim to have fixed
something it did not.

## What the mutation showed

`a_stark_schnorr_revocation_proof_verifies_through_the_dispatched_path` signs
with stark-schnorr over the production preimage and expects `Ok`, then checks
the same bytes are refused under Ed25519.

Hardcoding the algorithm inside `verify_lifecycle_proof` turns **exactly that
one test red**, and leaves the three pre-existing tests in the file green —
including `a_revocation_proof_replayed_as_the_other_algorithm_is_rejected`,
which is the vector that looked like it covered this and did not.

That contrast is the result. Before, the file's cross-algorithm coverage was
vacuous; now one test in it is not.

## Acceptance Criteria

- No verification site takes a fixed-size key array. Done.
- A round-trip vector that goes red under the hardcoded-algorithm mutation.
  Done.
- `octo-algo-stark-schnorr` — unchanged apart from the `Felt` re-export, which
  is an API addition, not a behaviour change.

## Five forwarders this scope missed, found another way

The 38 were found by locating parameters that reach a `verify_dispatch` call.
That misses a **forwarder** — a function that takes the key and passes it to a
_verify_ function rather than to the dispatcher. It compiles by coercion and
pins the length just the same:

| Site                    | Shape                                         |
| ----------------------- | --------------------------------------------- |
| `porelay/heartbeat.rs`  | `verify_full` forwards to `verify_signature`  |
| `porelay/forwarding.rs` | same                                          |
| `dot/witness.rs`        | `validate_bind` forwards to `envelope.verify` |
| `dot/handover.rs`       | `append` forwards to `event.verify`           |
| `dot/gateway.rs`        | forwards to `Envelope::verify`                |

All five are now `&[u8]`. They were found by an attempted automated guard,
recorded for what it got right and what it got wrong.

## The guard that was written and then withdrawn

The idea: assert no verification site pins its key to a fixed-size array, by
reading each `verify_dispatch(` call, taking its **second** argument, and
checking whether that binding is declared `[u8; N]`. Two things happened.

**It had the bug this arc keeps finding.** The first version broke at the
_first_ comma, so it read argument one — the algorithm, declared as an
`AlgorithmId` and never an array. It passed with a fixed-size key reinstated.
Mutation caught it; the fix reads argument two. Shipped as-is it would have
been vacuous in exactly the way this arc keeps discovering.

**Corrected, it still reports what nobody can act on.** It matches a binding
name to a declaration _anywhere in the file_, so it flags names shared with
things that are not verification parameters:

- `octo-reputation`'s `governance_pubkey: [u8; 32]` — a **wire record field**.
- `octo-wallet`'s `IdentityKey::public_key: [u8; 32]` — the identity's own key,
  Ed25519 by construction.
- `seed_slot_slug_by_pubkey` — a slug helper, not a verifier.
- two test fakes whose parameter names collide.

Six residual hits, none actionable, and the real defect is invisible to it
without function-scoped matching it does not do.

So it is withdrawn, with the five forwarders it _did_ find kept. Making it
correct needs per-function slicing: for each `fn`, take its signature and body,
and ask whether a `verify_dispatch(` in that body names a binding declared
`[u8; N]` in that same function. That is real parsing work, and a guard that
fires on six false positives will be muted long before it earns that.

Same call as the mission-link guard earlier in the session, same reason: the
first exception a guard reports is one nobody can act on, and that is how a
guard teaches people to ignore it.

## The guard, rebuilt with function scoping

Withdrawn for being file-scoped, then rebuilt per function: slice each `fn`'s
signature and body, find `verify_dispatch(` in the body, read its second
argument, and check whether that binding is declared with a fixed size **in
that same function**. All four false positives disappear, because those
declarations are at file scope or inside a different `fn` than the call.

It reads **53** dispatch sites across 319 files and reports nothing.

Two things about building it are worth keeping, because both are mistakes this
arc has already made:

- **The first version read argument one, the algorithm.** It passed with a
  fixed-size key reinstated. Mutation caught it. A guard shipped in that state
  would have been vacuous in exactly the way this arc keeps discovering.
- **The floor was invented before it was measured.** It asserted 60 and went
  red on arrival against an actual 53. A floor guessed from a hunch is the
  same error as counting constructors instead of sites, in the other
  direction — it demands coverage that does not exist. It is 45 now, with the
  measurement recorded next to it.

### A one-time sweep for the forwarder class, since the guard cannot

Naming is acceptable for a sweep and wrong for a permanent guard — that
distinction is the whole reason the guard is scoped rather than
name-matched. Swept every `verif*`/`valid*`/`check*`/`auth*` function in the
workspace taking a fixed-size key: **seven hits, one real**.

| Site                                          | Verdict                                                        |
| --------------------------------------------- | -------------------------------------------------------------- |
| `octo-runtime` `verify_attach_handle_payload` | **real forwarder** — hands the key to `verify_bytes`. Widened. |
| `octo-network` `verify_platform_admin_proof`  | not a verification site                                        |
| `quota-router-core` `verify_hmac` x5          | HMAC keys                                                      |

The two exclusions are the useful part, because both _look_ exactly like the
target class:

**`verify_platform_admin_proof` verifies nothing.** It checks freshness,
compares the proof's `dc_pubkey` against an expected role-binding field, and
refuses an empty adapter signature — the actual verification is delegated to
the adapter. Its `expected_dc_pubkey` is a **role binding**, not a
verification key, and it is compared against a wire field that is genuinely
32 bytes. This is also why `dc/admin_attest.rs` showed up in `0009-c3` as a
`pubkey_from_did` caller: it resolves identity, it does not verify.

**`verify_hmac`'s `network_key` is an HMAC key.** BLAKE3 keyed hashing takes
exactly 32 bytes and there is no algorithm to select — a symmetric MAC is not
a signature and AC#11 does not reach it. Widening it would have been the
mirror-image error of widening a macaroon root secret.

So the forwarder class is now six sites, all fixed, and the sweep says there
are no more rather than assuming it.

### The second-algorithm vector, in the layer that has 25 of them

`octo-wallet`'s lifecycle path got the round trip in `0009-cc`. The 25 `dot/`
envelopes did not — they were widened and compiling, but nothing had ever
signed one with anything but Ed25519.

`dot_envelope_verifies_under_stark_schnorr` closes that. It builds a
`CreateGroupEnvelope`, signs its `cgroup_hash` with a 33-byte stark-schnorr key,
verifies through `CreateGroupEnvelope::verify`, and checks the same envelope is
refused under Ed25519 so the acceptance came from the algorithm.

The signature field needed no change to make this possible: it is a 64-byte
array and a stark-schnorr signature is 64 bytes.

Hardcoding the algorithm in `CreateGroupEnvelope::verify` turns it red. So now
both ends of the arc carry a vector a hardcoded verifier cannot pass — the
wallet lifecycle path and the transport-envelope layer that started it.

This is why the `dot/` layer needed the widening at all. Every vector it had
for the whole of the dispatch work would pass against a verifier that ignored
its algorithm argument, because every one of them signed with Ed25519.

### What it does not catch

**Forwarders.** A function that takes the key and hands it to another _verify_
function never mentions `verify_dispatch`, so this vector skips it. Verified by
planting one: green. The five that exist were fixed by hand the same day.

Catching them means following the call graph. That was prototyped and measured
before any Rust was written, and it does not work — recorded so nobody attempts
it a third time.

**The prototype.** Take the set of function names whose body contains
`verify_dispatch`, then iterate to a fixpoint adding any name called by
something already in the set. Flag a fixed-size key parameter on any function
whose name lands in that closure.

**The result.** The closure explodes: **3,265 of 5,457** function names enter
it. Method names are not unique across types, so `new`, `verify` and `default`
pull in almost everything, including functions with nothing to do with
verification.

**The verdict: 15 hits, not one of them a forwarder.** Onion-layer
`session_key`s and a `relay_private_key` (symmetric, genuinely 32 bytes), a
`cache_key_hash` in a circuit breaker, `signing_key`s for proof aggregation,
two `*_key_id` fields that are identifiers rather than keys, and
`verify_platform_admin_proof` again — the role-binding comparison that looks
like a verifier and is not.

Zero true positives against fifteen false ones. A correct version would need
type or path resolution to disambiguate method names, which is an analysis of
the crate rather than a scan of its text, and it would have to stay correct as
the code moves underneath it.

So the gap stays open and stated. The forwarder class was swept by hand — six
sites, all fixed — and that sweep is what says there are no more. What remains
unguarded is a _new_ forwarder appearing, which is a real cost and is written
down rather than hidden.

The gap is also written into the guard's own doc comment, so a reader who
assumes it is total goes looking for that class instead of trusting a green
result.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network -p octo-wallet -p octo-runtime -p octo-reputation --all-targets --all-features -- -D warnings
cargo test -p octo-wallet --test lifecycle_replay
cargo test -p octo-cli --test security_regressions
```

## Backward compat

None required. Nothing is deployed.

## Results

1536 `octo-network` lib vectors, 251 `octo-reputation` lib vectors, 4
`lifecycle_replay` vectors, 47 gate vectors. Clippy clean at `-D warnings`
across all six touched crates.

## Cross-references

- `missions/claimed/0009-cb-dot-verify-takes-key-bytes-not-verifying-key` — the
  change this extends, and whose claim was narrower than it looked
- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11

## Claimant

@unassigned

## Pull Request

#
