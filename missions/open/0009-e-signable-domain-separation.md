---
name: 0009-e-signable-domain-separation
description: RFC-0009 v1.3 AC#13. Apply the mandatory algorithm_id + key_id signable prefix across every signer and verifier, so a cross-algorithm signature replay is a malformed signature rather than a wrong equation.
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
- **`key_id` is the `VerificationMethodId` from the DID document.** It MUST NOT
  be derived from the public key, and MUST NOT be locally allocated at signing
  time. A locally-allocated id makes the signed domain depend on who signed,
  which defeats the point.
- **`canonical_ser` means RFC-0126's substrate canonicalizer by name.** This
  repository has three distinct functions carrying that symbol, including a
  `Caveat::canonical_ser` method that is per-caveat deterministic JSON.
  Naming the bare symbol lets two implementations sign different domains.
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

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors (per RFC-0009 v1.3 §Acceptance Criteria)

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
  disambiguation; this exists because there are three functions by that name
- `cross_algorithm_signature_replay_rejected` — co-owned with `0009-c`, the
  end-to-end property

Gate: `cargo test -p octo-ident --lib`

## Layer direction (per `cipherocto-design-principles`)

The prefix construction is Layer A — pure canonical encoding, frozen. Every
signer and verifier consumes it. If two crates build the signable bytes
separately, that is a parallel encoding and the domain separation is not real.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-ident --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
```

## Backward compat

Additive on the message format, but every existing signer and verifier MUST
change together. A verifier reading an unprefixed message MUST reject it rather
than fall back to the legacy shape — prefix-stripping must not be a downgrade
path. Nothing is deployed, so there is no compatibility window and no shim.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Domain separation
  is mandatory, not hygiene; AC#13; A12
- `rfcs/accepted/process/rfc-0126-*` — the canonicalizer being disambiguated
- `rfcs/draft/economics/0974-zk-capability-platform-extension-surface.md` —
  §Step 1, the sign-bit defect that is the same class of bug
- `missions/open/0009-b-algorithm-registry`, `0009-c-verification-dispatch`

## Claimant

@unassigned

## Pull Request

#
