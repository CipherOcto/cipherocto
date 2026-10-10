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

# Mission `0009-e-signable-domain-separation` — OPEN 2026-10-09

## Status

Open (2026-10-09). Claimable once `0009-b-algorithm-registry` lands.

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
cargo clippy -p octo-ident -p octo-wallet -p octo-cap-macaroon --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
cargo test -p octo-wallet --lib
cargo test -p octo-cap-macaroon --lib
```

The last two are the ones that matter. `octo-ident` alone cannot observe
whether the prefix is applied, because it does not sign or verify anything.
`cross_algorithm_signature_replay_rejected` additionally needs `0009-d`'s
registration before it can run over a real pair.

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
- `missions/open/0009-b-algorithm-registry`, `0009-c-verification-dispatch`,
  `0009-d-stark-schnorr-algorithm`

## Claimant

@unassigned

## Pull Request

#
