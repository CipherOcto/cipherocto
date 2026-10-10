---
name: 0009-e-signable-domain-separation
description: RFC-0009 AC#13. Apply the mandatory algorithm_id + key_id signable prefix across every signer and verifier, so a cross-algorithm signature replay is a malformed signature rather than a wrong equation.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
---

# Mission `0009-e-signable-domain-separation` — IMPLEMENTED 2026-10-09

## Status

**Claimed and implemented (2026-10-09), with the signer migration explicitly
outstanding** — see §What this does not cover.

All seven §Test Vectors are in place under their specified names.
`octo-ident` 95, `octo-wallet` 337, `octo-algo-stark-schnorr` 46 + 5
integration. Clippy clean with `--all-targets --all-features` across all three.

### Layer decision, written down as §Layer direction demands

The builder is in **`octo-ident`, Layer B**, and it depends on
**`octo-determin`, Layer A**, for the payload canonicalizer.

The mission's §Layer direction offers two readings of a builder taking
`[u8; 16]` and calls for a decision. A `[u8; 16]` builder really would be
Layer A, but it cannot be type-checked against the discriminators — a caller
passing sixteen arbitrary bytes compiles, and naming `algorithm_id` at all is
the point. So the builder takes `AlgorithmId` and `VerificationMethodId`,
which are Layer B, and it lives beside them. Calling it Layer A would be
wrong; the B → A dependency on `octo-determin` is the correct direction.

### The five findings

**1. `canonical_ser` resolves to none of the five functions by that name.**
The mission frames this as "which of the five is the DCS substrate
canonicalizer". The honest answer is that **none of them is**:

| Function                                    | Encoding                                |
| ------------------------------------------- | --------------------------------------- |
| `zk_circuit::canonical_ser`                 | hand-rolled, its own domain byte `0xA8` |
| `CapabilityBundleV2::canonical_ser`         | borsh                                   |
| `CapabilityBundleV2Envelope::canonical_ser` | borsh                                   |
| `Caveat::canonical_ser`                     | JSON                                    |
| `HolderRecord::canonical_ser`               | JSON                                    |

So there was nothing to point at and the mission's own assumption did not
hold. The builder calls `octo_determin::dcs::dcs_serialize_bytes` by path,
and `canonical_ser_is_the_rfc0126_substrate_canonicalizer` pins that it is
DCS and not JSON and not borsh. The JSON exclusion the mission predicted is
real; the mission just also excluded the other three.

**2. A bare payload longer than 32 bytes parses into an attacker-chosen
prefix.** `verifier_rejects_missing_prefix` was specified as "the verifier
refuses a message with no prefix", which suggests a structural check. There
is no structural check to be had: `SignedMessage::parse` splits the first 32
bytes off as a prefix, and any payload at least that long _parses_. Feed it
the ASCII text `transfer 25 OCTO to did:octo:zrecipient` and the prefix reads
back as whatever those bytes say.

Nothing rejects it structurally, and that is the sharp edge the vector now
asserts: a real signature still fails, on the **key binding**, and a verifier
that only asked "does this parse?" would accept the message. The two checks
that do the work are the `key_id` comparison and dispatching on the prefix's
algorithm — both of which `verification_follows_the_algorithm_the_prefix_names`
pins by asserting on _which_ error comes back.

**3. The cross-algorithm replay is refused by the prefix and by key shape,
and only the key-id swap is attributable to the prefix alone.** Said
precisely in the test file's header rather than claimed as a clean result:
Ed25519 keys are 32 bytes and STARK-curve keys are 33, so a STARK public key
dispatched to Ed25519 is refused on length before any equation runs. The
`key_id` swap is the clean demonstration — same algorithm, same key, same
payload, so the only reason it can fail is that the recomputed signable bytes
differ.

**4. `IdentityKey` cannot fail _open_.** RFC-0009 forbids deriving the
`key_id` from the public key and forbids allocating one at signing time, so
an identity with no `key_id` has nothing to invent. `sign_prefixed` and
`verify_prefixed` return `WalletError::NoKeyContext` rather than falling back
to an unprefixed signature — a downgrade path is precisely how an unprefixed
message keeps verifying after the prefix is deployed.

**5. A pre-existing break, found by running a target this mission had no
reason to.** `crates/octo-wallet/tests/ask_publish_cli.rs` has not compiled
since mission `0009-c2` changed `AskSigned::verify` to take an `algorithm_id`.
That mission's gate ran `cargo test -p octo-wallet --lib`, which never
compiles an integration test, so the break was invisible until
`cargo clippy --all-targets` ran on the crate this mission had to touch.
Fixed, and the lesson is recorded rather than the fix alone: **a `--lib` gate
does not check that the crate's integration tests build.**

### Mutation-verified

Six mutations applied and reverted; five fired red on the first pass.

| Mutation                                                 | Caught by                                                       |
| -------------------------------------------------------- | --------------------------------------------------------------- |
| Verifier ignores the prefix's algorithm                  | `verification_follows_the_algorithm_the_prefix_names`           |
| `key_id` binding check removed                           | `a_message_naming_another_key_id_is_rejected`                   |
| Signer allocates a `key_id` instead of refusing          | `an_identity_with_no_key_id_refuses_to_sign_a_prefixed_message` |
| Payload canonicalized without the `u32_be` length prefix | `canonical_ser_is_the_rfc0126_substrate_canonicalizer`          |
| Prefix emits hex identifiers instead of raw bytes        | `the_prefix_is_thirty_two_bytes_of_raw_identifier_bytes`        |
| Hashed form collapses onto the raw form                  | `the_hashed_form_is_self_checking_because_it_is_a_digest`       |

**A sixth mutation came back green, and the green was the finding.** The
first attempt to test the key-id swap deleted the assertion from the test
rather than breaking the implementation — of course a test that no longer
asserts anything still passes. A vacuous vector is indistinguishable from a
passing one if you never try to break it. Redone against
`SignablePrefix::to_bytes` (drop the `key_id`), it goes red immediately, and
that is the form the vector is worth keeping in.

A deliberate no-op mutation was also run to confirm the harness does not
report red for everything.

### Decisions taken here

- **Raw form, declared.** RFC-0009 prefers the hashed form for new
  implementations; this ships raw and says so, which is what the RFC
  requires of any implementation that chooses. The reason is that raw is the
  only form carrying the algorithm **in band**, and "verify under the
  algorithm the prefix names rather than the one it prefers" is not expressible
  otherwise — a digest is not invertible, so a hashed-form verifier would have
  to take the algorithm out of band, back to trusting its own configuration.
  `SignedMessage::parse` refuses a hashed message for exactly this reason,
  which is why the two rejection vectors are principled rather than heuristic.
- **`SignedMessage` stores the payload uncanonicalized** and canonicalizes on
  demand, so `to_bytes` and `signable` cannot disagree about which bytes the
  signature covers.
- **The replay test lives in `octo-algo-stark-schnorr`, not `octo-ident`.**
  `octo-ident` is Layer B and must not depend on either algorithm crate; the
  only crate that can see both is one that depends on both.
- **`from_record` rehydrates without a `key_id`** and refuses prefixed
  signing. The persisted record does not carry one, and inventing it locally
  is what the RFC forbids. Carrying it through `IdentityRecord` is the
  follow-up that lifts the restriction.

## What this does not cover

The mission's §Scope says "every signer and verifier". What is done is the
**substrate, one real signer end to end, and the two decisive vectors over a
real algorithm pair**. The mechanical migration is not done, and is recorded
here rather than left to be discovered:

- `IdentityKey::sign` and `IdentityKey::verify` still sign and verify **bare**
  messages. `sign_prefixed` / `verify_prefixed` are the prefixed path and are
  complete, but nothing forces a caller onto them.

  An earlier draft of this section said "30 in-crate call sites and roughly ten
  downstream crates". **That number was wrong** and is corrected here. It came
  from a raw `grep -c '\.sign('` over `crates/octo-wallet/src`, which counted
  `ed25519_dalek::SigningKey::sign`, `HsmAdapter` internals, and test bodies
  alongside genuine `IdentityKey::sign` calls. Counting only non-test
  production sites gives **eight**:

  | Site                                 | What it signs                                     |
  | ------------------------------------ | ------------------------------------------------- |
  | `octo-wallet` §`revoke`              | `b"revoke"` — a 6-byte constant, bound to nothing |
  | `octo-wallet` §`begin_rotation`      | `b"rotate" ‖ new_pub`                             |
  | `octo-wallet` §`IdentityKey::sign`   | the caller's bare message                         |
  | `octo-wallet` §`identity_store`      | a wrapper over the above                          |
  | `octo-wallet-node` §sign handler     | the request bytes                                 |
  | `octo-runtime` §handle/signing (two) | the holder message                                |
  | `quota-router-core` §`envelope_v2`   | a preimage that may already carry its own domain  |

  The last one must be checked before migrating: if that preimage already
  contains a domain separator, prefixing it would bind two domains at once.

- `CapabilitySigner::sign(&self, msg: &[u8])` in `octo-cap-macaroon` is
  unchanged. It carries no key context at all, so the RFC-named "real
  signers" list is only half covered. Making it carry one is a trait change
  across every implementor.
- `verify_successor_proof` / `verify_revocation_proof` take an `algorithm_id`
  **out of band** because they are pure static helpers with no signer to ask.
  That is exactly the "verifier prefers its own algorithm" shape the prefix
  removes, and it is the first migration slice.
- `IdentityRecord` does not persist a `key_id`.

The follow-up is a migration mission, not an extension of this one: replace
`sign`/`verify` at each site and delete the bare path, since RFC-0009's
§Backward compat forbids a downgrade route and nothing is deployed to
deprecate towards. **Until then, a prefixed message is rejected by a bare
verifier and a bare message is rejected by a prefixed verifier** — the two
shapes are not interoperable, which is the property that keeps the partial
migration safe rather than silently wrong.

## Dependencies

`depends_on: 0009-b-algorithm-registry` — the prefix names a
`VerificationMethodId` and an `AlgorithmId`, both of which that mission adds.

## Scope

Make the signable prefix real at every signer and verifier.

```
signable = algorithm_id ‖ key_id ‖ canonical_ser(payload)
```

**This mission ships the raw form**, and states so because RFC-0009 requires an
implementation to declare which of the two it uses — the vectors below include
`verifier_rejects_hashed_form_when_declaring_raw` and its reverse, and neither
means anything until the choice is on the record. RFC-0009 prefers the hashed
form for new implementations, so shipping raw here is a deliberate departure
from that preference: it is recorded in RFC-0009 rather than left implicit, and
every verifier in this mission MUST reject the hashed form. If the raw form's
fixed-width precondition ever breaks, that is the moment to switch, and the
switch is an RFC amendment, not an implementation detail.

## Why this is not hygiene

Ed25519 signatures are 64 bytes (`R ‖ S`). STARK-curve Schnorr signatures are
also 64 bytes (`r ‖ s`). Same length, different meaning, and neither is a type
error for the other. A verifier that dispatches on its own configured algorithm
rather than the one named in the message gets a silent wrong answer — no
exception, no rejection, just a check run against the wrong equation. Same
shape as the sign-bit defect recorded in RFC-0974 §Step 1, where every vector
in the original suite happened to have the sign bit clear.

## The prefix is specified byte-exactly. Follow the spec, not the sketch

RFC-0009 §Dual-format signature algorithms and the ZK-capable key gives the
normative rules, and each one exists because the naive version has a real
failure mode:

- **`algorithm_id` is the 16 raw bytes of its `Uuid`**, not the 36-character
  hyphenated form. Both are in use in this codebase — borsh encodes the raw
  form, serde the textual one — and an implementation that picks differently
  from its peers signs a different domain.
- **`key_id` is the 16 raw bytes of the `VerificationMethodId` `Uuid`** — not
  the 36-character hyphenated form, exactly as above. It MUST NOT be derived
  from the public key, and MUST NOT be locally allocated at signing time. A
  locally-allocated id makes the signed domain depend on who signed, which
  defeats the point.
- **Both components are fixed at 16 bytes, so the raw prefix is
  self-delimiting and needs no length framing.** An earlier draft of this
  mission dropped this bullet and was wrong: it is the precondition that makes
  the raw form safe. `VerificationMethodId` is `struct VerificationMethodId(Uuid)`
  — RFC-0009 now defines it, and until that amendment it had **no declared
  representation anywhere**, so nothing fixed the width. The repository's
  existing convention for a verification-method key id is a _string fragment_
  (`DidDocument::authenticates` matches values like `"#key-0"`), which is what
  an implementer reaches for — and at variable width the raw form collides:

  | triple A                        | triple B                        | identical signable bytes |
  | ------------------------------- | ------------------------------- | ------------------------ |
  | `key_id = ""`, payload `"abc…"` | `key_id = "a"`, payload `"bc…"` | identical                |
  | `key_id = "ab"`, payload `"c"`  | `key_id = "a"`, payload `"bc"`  | identical                |

  The empty-`key_id` row needs no crafted attack, only a payload whose first
  byte looks like an id character. If `key_id` is ever variable-width, the raw
  form MUST move to the hashed form in the same change that makes it
  variable.

- **`canonical_ser` means RFC-0126 §Part 3: Deterministic Canonical
  Serialization, as this repository implements it — `octo-determin`'s
  `dcs_serialize_*`.** RFC-0009 corrected this bullet; the earlier text
  resolved the symbol "by name" against RFC-0126, which defines no such symbol,
  and counted three functions where there are **five**
  (`zk_circuit::canonical_ser`, `CapabilityBundleV2::canonical_ser`,
  `CapabilityBundleV2Envelope::canonical_ser`, `Caveat::canonical_ser`,
  `HolderRecord::canonical_ser`). Two of those five are JSON, and RFC-0126
  §JSON Allowed Contexts lists cryptographic proofs among the contexts where
  JSON is **forbidden** — so `Caveat::canonical_ser` and
  `HolderRecord::canonical_ser` are excluded by rule, not preference. A signer
  that reached either by grepping for the bare symbol would be signing a domain
  the serialization RFC prohibits.
- **A verifier MUST reject a message with no prefix**, and MUST verify under
  the algorithm the prefix names rather than the one it prefers.

The hashed form —

```
signable = BLAKE3-256("cipherocto/sigdomain/v1" ‖ algorithm_id ‖ key_id)
           ‖ canonical_ser(payload)
```

— is preferred for new implementations: it removes every framing question in
one step. The two forms are **not** interoperable. An implementation states
which it uses; a verifier MUST reject the other.

## Where the signer gets `key_id` — the part the RFC forbids and the mission must solve

RFC-0009 forbids the two obvious answers: deriving `key_id` from the public
key, and locally allocating it at signing time. Those are the only two sources
a signer currently has, and neither is available.

`IdentityKey` in `octo-wallet` carries no DID document, no
`VerificationMethodId` and no algorithm identifier. `CapabilitySigner::sign`
in `octo-cap-macaroon` takes `(&self, msg: &[u8])` — no key context at all.
So neither signer can currently produce a correct prefix, and the highest
probability implementation is precisely the forbidden one.

This mission must therefore supply the key context, not assume it. The options
are narrow: the signer holds or is handed its `VerificationMethodId` alongside
its private key (a constructor parameter, a registry keyed by key, or a DID
resolution the signer performs). Whichever is chosen, it is a **public API
change on the signer type**, and it is the deliverable that makes the rest of
this mission possible. An implementer who skips it and reaches for
`Uuid::new_v4()` has implemented RFC-0009's explicitly forbidden repair.

If that turns out to require a signing API break that collides with
`0957-ext-macaroon-crate`, declare the conflict rather than routing around it
— a prefix whose key id is not stable is worse than no prefix, because it
looks implemented.

## The gate must compile the crates that actually sign and verify

The original gate was `cargo test -p octo-ident --lib`. **`octo-ident` has no
signing or verification code and no `ed25519-dalek` dependency** — its
manifest is `blake3`, `thiserror`, `hex`, `bs58`, `async-trait`, `parking_lot`,
and `borsh` behind the optional `borsh` feature. The only
Ed25519-adjacent symbols there are the `VerificationMethod` data carrier and a
64-byte signature field on `ResolverBackend`.

The real signers are `IdentityKey::sign` and `HsmAdapter::sign` in
`octo-wallet`, and `CapabilitySigner::sign` in `octo-cap-macaroon`. The real
verifiers are the `VerifyingKey::from_bytes` sites and the 27
`&VerifyingKey`-parameter `verify` functions `0009-c` enumerates.

So the original gate can go fully green with the whole mission implemented
inside `octo-ident`, while every signer and verifier in the repository still
signs the unprefixed shape. That is the "prefix computed but never covered"
defect, and the gate was structurally incapable of catching it.

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors

Six of these are this mission's own additions, not RFC acceptance criteria —
see the note below the list.

- `signable_prefix_is_prefix_free` — no two distinct
  `(algorithm_id, key_id, payload)` triples produce the same byte string
- `signable_prefix_survives_algorithm_swap` — same key material, different
  `algorithm_id` prefix, different signable bytes
- `signable_prefix_survives_key_swap` — same algorithm, different
  `verification_method_id`, different signable bytes
- `verifier_rejects_missing_prefix`
- `verifier_rejects_hashed_form_when_declaring_raw` and the reverse — the two
  encodings are not interoperable and that is a correctness property
- `canonical_ser_is_the_rfc0126_substrate_canonicalizer` — pins the
  disambiguation; this exists because there are **five** functions by that
  name, two of them JSON and therefore forbidden for proofs
- `cross_algorithm_signature_replay_rejected` — the decisive one. A signature
  made under algorithm A, presented as algorithm B, must be **rejected**.
  **Owned by this mission**, not shared with `0009-c`: it is a property of the
  prefix, and RFC-0009 AC#13 names it here. Note it is specified over the
  Ed25519 / STARK-Schnorr pair, so it cannot be satisfied until `0009-d` has
  registered a second algorithm — declare that ordering rather than mocking it
  away, because a mock proves dispatch rejects a mismatched identifier, which
  is `0009-c`'s concern and not this one.

### Most of this list is not in the RFC

The heading above attributes these vectors to RFC-0009 §Acceptance Criteria.
**Only one of the seven appears there** — `cross_algorithm_signature_replay_rejected`
at AC#13. The other six are this mission's own additions and are not RFC
acceptance criteria. That attribution is the same defect class A15 records: a
normative citation that resolves to nothing.

Also note that five of the seven only compare **byte strings**. They never
build a signature, call `verify`, or assert a rejection. A prefix helper that
returns the right bytes and is never called by any signer satisfies all five.
The vectors that can actually detect the vulnerability are
`cross_algorithm_signature_replay_rejected` and
`verifier_rejects_missing_prefix`; treat those as the acceptance condition and
the rest as supporting checks.

Gate: `cargo test -p octo-ident --lib -p octo-wallet --lib -p octo-cap-macaroon --lib`.
The last two are the ones that matter, and `octo-ident` alone is not an
acceptance condition for this mission — see §The gate must compile the crates
that actually sign and verify.

## Layer direction (per `cipherocto-design-principles`)

**Name the crate and the function, or this section does not do anything.** An
earlier draft called the prefix construction "Layer A — pure canonical encoding,
frozen" while the builder's parameters are `AlgorithmId` and
`VerificationMethodId`, which `0009-b` places in `octo-ident` — Layer B. Two
readings follow, with opposite dependency directions:

- a builder taking `[u8; 16]` is genuinely Layer A, but cannot be
  type-checked against the discriminators, so a caller passing a non-Uuid
  16 bytes compiles
- a builder taking `AlgorithmId` / `VerificationMethodId` is Layer B by
  construction, which is fine and consistent, but then calling it Layer A is
  wrong

Decide this in the mission and write the crate name down. What must not happen
is the current state: a warning about parallel encodings with no named owner,
which is a warning nobody can act on.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-ident -p octo-wallet -p octo-algo-stark-schnorr \
  --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
cargo test -p octo-wallet --lib
cargo test -p octo-wallet --test ask_publish_cli
cargo test -p octo-algo-stark-schnorr
```

`octo-ident --lib` alone cannot observe whether the prefix is applied, because
it does not sign or verify anything — the mission's own §The gate must compile
the crates that actually sign and verify. `cross_algorithm_signature_replay_rejected`
lives in an integration test of the crate that can see both algorithms.

The `--all-targets` flag is not decoration. It is what caught the
`ask_publish_cli` break described in §Status, which a `--lib` gate cannot see.

## Backward compat

Additive on the message format, but every existing signer and verifier MUST
change together. A verifier reading an unprefixed message MUST reject it rather
than fall back to the legacy shape — prefix-stripping must not be a downgrade
path. Nothing is deployed, so there is no compatibility window and no shim.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Domain separation
  is mandatory, not hygiene; AC#13; A12
- `rfcs/accepted/numeric/0126-deterministic-serialization.md` — §Part 3:
  Deterministic Canonical Serialization, and §JSON Allowed Contexts. The
  previous reference `rfcs/accepted/process/rfc-0126-*` resolved to nothing:
  wrong directory and wrong glob
- `rfcs/draft/economics/0974-zk-capability-platform-extension-surface.md` —
  §Step 1, the sign-bit defect that is the same class of bug
- `missions/claimed/0009-b-algorithm-registry`, `0009-c-verification-dispatch`,
  `0009-d-stark-schnorr-algorithm`
- `crates/octo-ident/src/signable_prefix.rs` — the prefix substrate
- `crates/octo-algo-stark-schnorr/tests/signable_replay.rs` — the two decisive
  vectors, over a real Ed25519 / STARK-Schnorr pair

## Claimant

@mmacedoeu

## Pull Request

#
