---
name: 0009-e10-capability-algorithm-binding
description: Make the capability holder signature commit to the algorithm it claims, closing the gap between the algorithm being carried in band and being signed in band.
metadata:
  node_type: mission
  type: research
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e5-capability-signer-algorithm-binding
    - 0009-e8-capability-variable-width-keys
---

# Mission `0009-e10-capability-algorithm-binding`

## Status

**Implemented.** `0009-e5` made the capability token carry the algorithm it was
signed under. It did not make the signature _commit_ to that claim. `holder_msg`
now leads with the `AlgorithmId`, supplied by the signer at mint and read back
off `HolderSignature` at verify, so the two stay symmetric and no new wire field
or trait method was needed.

## §The finding

`HolderSignature` is a pair of `(algorithm_id, signature)`. The signature is
produced over `holder_msg`, and `holder_msg` takes exactly two arguments —
`root_id` and `caveats`:

```rust
fn holder_msg(root_id: &MacaroonId, caveats: &[Caveat]) -> Vec<u8>
```

So the `algorithm_id` in the pair is **unauthenticated metadata**. It is
written into the token, carried in wire segment 2, and read back at verify time
to pick a verifier — but nothing in the signed bytes commits to it. A signature
is therefore valid over an algorithm-independent message, and its algorithm
label is a label attached after the fact.

That is the same defect `0009-e5` fixed one level up, just relocated. `e5` found
the parser _inventing_ Ed25519 and fixed it by carrying the real claim in band.
The claim now travels, but it is still not signed, so "carried" and "committed"
are not the same property.

### Why it is not an active forgery today

Honesty about severity, because the fix is cheap and the alternative reading
inflates it. Verification dispatches on the _claimed_ algorithm. Rewriting a
token's `algorithm_id` to a different algorithm therefore routes the signature to
a verifier it was never produced for, and it fails. So today an attacker gets a
denial, not a forged token.

The exposure is latent rather than absent. It becomes real the moment any two
registered algorithms can both accept a signature the other produced — a second
Schnorr parameterisation, or a future algorithm reusing a curve. At that point
the rewrite silently succeeds, and the failure lands on whoever chose the wrong
pairing rather than on the signature.

## §Scope

1. **`holder_msg` takes the `AlgorithmId` and prefixes it.** The signer
   supplies it from `CapabilitySigner::algorithm_id()`, the verifier reads it
   back from `HolderSignature::algorithm_id()`. Symmetric, so no new wire field
   and no new trait method — both already exist and are required.
2. **A vector pinning that the algorithm is in the signed bytes.**

## §What must not be done

- **Do not add a `key_id` field here.** The key is already bound: verification
  selects it through `holder_pub`. The missing binding is the _algorithm_, and
  the `SignablePrefix` shape (algorithm ‖ key ‖ version) should not be
  half-adopted — either the whole prefix or none of it. Record it; land it with
  the key-id question when a real second key per DID exists.
- **Do not reuse `SignablePrefix` for this.** It requires a `key_id`, which this
  path has no source for. Inventing a `key_id`-less prefix variant would fork
  the prefix type, and the prefix is Layer A and frozen.
- **Do not relax this to "the verifier already dispatches on it".** Dispatch is
  how the claim is _used_; binding is how it is _authenticated_. They are
  different properties and conflating them is what left the gap.
- **Do not mint the message from `governance_algorithm_id()`** or any other
  constant. The value must come from the signer, per the rule mission `0009-e3`
  settled: the signing party names its own domain.

## §Verification for whoever implements this

The trap here is that a weak vector passes both before and after the fix. Today,
rewriting `algorithm_id` on a valid token already fails verification — for the
_wrong reason_ (the signature is dispatched to the wrong verifier). So an
end-to-end "rewrite it and assert it fails" vector is **vacuous**: it is green
before the change and green after, and proves nothing about binding.

The discriminating vector has to be about the _message_, not the outcome:

- `holder_msg` under two different `AlgorithmId`s over identical
  `root_id` and `caveats` must produce different bytes. This is green after the
  fix and red before it, and it is the actual property.

That is a white-box assertion and it should say so. It is still the right one,
because "the algorithm is in the signed bytes" is not otherwise observable from
the outside without registering a second lenient algorithm in the shared global
registry, which a unit test must not do.

Also keep the existing `0009-e5` vectors green. Two of them
(`a_non_ed25519_survivor_is_not_downgraded_by_the_wire`,
`a_signer_naming_another_algorithm_is_recorded_faithfully`) construct
signatures under an algorithm this deployment has not registered, so the
signature length is not enforced for them. Adding a fixed prefix to the message
must not accidentally start requiring registration.
