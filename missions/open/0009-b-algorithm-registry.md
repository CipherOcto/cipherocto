---
name: 0009-b-algorithm-registry
description: RFC-0009 v1.3 BC#13. Replace the VerificationMethodKind central enum with a SignatureAlgorithm trait + AlgorithmRegistry keyed by AlgorithmId(Uuid). Add VerificationMethodId, revoked_at and custody fields, and the load-bearing storage migration in octo-ident-storage.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
---

# Mission `0009-b-algorithm-registry` — OPEN 2026-10-09

## Status

Open (2026-10-09). Claimable — no claimant assigned. **Base mission for the
RFC-0009 v1.3 decomposition**; `0009-c`, `0009-d` and `0009-e` all depend on
this one and MUST NOT start before it lands.

## Dependencies

None. This is the root of the v1.3 mission set. Its gating condition is
RFC-0009 v1.3 acceptance, which is recorded.

## Scope

Implement RFC-0009 v1.3 §Dual-format signature algorithms and the ZK-capable key
— the substrate half only. Verification-site dispatch is `0009-c`, the
STARK-curve registration is `0009-d`, and the signable prefix is `0009-e`.

1. **`AlgorithmId` and `CircuitId`** — typed `Uuid` discriminators in
   `crates/octo-ident`. Not a `u8`: a new algorithm must not be able to
   collide with an existing one by picking a free byte.

2. **`SignatureAlgorithm` trait + `ZkSupport` + `AlgorithmRegistry`** —
   per §Dual-format signature algorithms and the ZK-capable key. The registry
   has three non-obvious requirements:
   - `register` MUST reject a duplicate `AlgorithmId` with `AlreadyRegistered`
     rather than overwrite. Last-writer-wins is a silent wrong answer inside
     the mechanism meant to prevent them.
   - `populate` marks the registry ready. A lookup before that is
     `NotPopulated`, **not** `UnknownAlgorithm` — the first is a deployment
     that has not booted, the second is an algorithm nobody registered.
   - `get` MISSES on an unregistered id. There is no `Default` arm and no
     fallback. Returning `Ed25519` here is the exact failure A12 exists to
     prevent.

3. **`VerificationMethod` gains three fields** — `id: VerificationMethodId`
   (the `key_id` the signable prefix names; a verifier holding only a public
   key cannot reconstruct one), `revoked_at: Option<u64>` (without it the
   revocation cascade has nothing to act on, since RFC-0958's snapshot reads
   only identities), and `custody: CustodyPosture`.

4. **Remove `VerificationMethodKind`** and make `public_key` length-generic
   (`Vec<u8>` bounded by `SignatureAlgorithm::public_key_len`).

5. **The storage migration** — see below. This is the sharp edge of the whole
   change and the part most likely to be done last, which is exactly wrong.

### The storage migration is load-bearing

`crates/octo-ident-storage/src/did_registry.rs` decodes the rich-document
columns with

```rust
borsh::from_slice::<Vec<VerificationMethod>>(&bytes).unwrap_or_default()
```

in both `resolve` and `resolve_in_chain`, under a comment describing it as
deliberate fail-soft behaviour for legacy NULL rows.

Replacing a `u8` kind with a wider `AlgorithmId` makes every existing
document fail to decode. That produces **zero** verification methods while the
independent `public_key` column still resolves to 32 bytes — so the DID
resolves successfully, carries a public key, and owns no algorithm-bearing key
at all. A silent downgrade to "valid identity, unusable key".

The migration MUST ship before, or atomically with, the field change:
explicitly decode the legacy byte into the pre-registered Ed25519
`AlgorithmId`, and MUST NOT route the result through `unwrap_or_default()`.
Post-migration a decode failure MUST be an `Err`.

This does not repeal fail-soft for genuinely optional columns. It removes it
for the column that is load-bearing under v1.3.

### Also in scope

`VerificationMethodKind::from_byte` collapses every unknown byte onto
`Reserved` with `Reserved.as_byte() == 0x00`, so two unknown algorithms become
mutually indistinguishable. Its doc comment claims the opposite — that the
mapping makes old code "fail-closed on unknown discriminators". Removing the
enum removes the contradiction; do not port the comment.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met. Do not add a
separate checkbox list here — it would duplicate the vectors and let the two
drift apart.

## Test Vectors (per RFC-0009 v1.3 §Acceptance Criteria)

- `legacy_document_migrates_to_algorithm_id` — a pre-migration document
  decodes through the migration path with a non-empty method list and the
  correct Ed25519 `algorithm_id`
- `migrated_document_still_resolves_with_a_verification_method` — resolver
  level, not just codec level
- `post_migration_decode_failure_is_an_error` — **the one that matters most.**
  Asserts a malformed blob is `Err`, not an empty list
- `unregistered_algorithm_id_is_rejected`
- `unregistered_algorithm_id_is_not_silently_ed25519`
- `lookup_before_population_is_not_populated`
- `unknown_algorithm_ids_are_distinguishable_in_errors`
- `duplicate_registration_is_rejected`
- `software_custody_is_distinguishable_from_hardware`

Gate: `cargo test -p octo-ident -p octo-ident-storage`

## Layer direction (per `cipherocto-design-principles`)

- `octo-ident` (Layer B) — trait, registry, discriminators, `VerificationMethod`
- `octo-ident-storage` (Layer B) — decode migration only

The registry is the thing that makes this Layer B years-stable: a new algorithm
lands as a new crate registering into it, with no central edit. That is the
whole point of §Extension over enumeration, and it is why removing the enum is
worth a break.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-ident -p octo-ident-storage --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
cargo test -p octo-ident-storage
```

## Backward compat

None, by design. RFC-0009 v1.3 removed the `derive_capability_key_v11` shim
and adds none here: nothing is deployed, and a `#[deprecated]` path that keeps
a lossy decoder reachable has no audience.

RFC-0010 also claims to define `VerificationMethodKind` and its
`octo-ident` doc comment cites it as normative. This RFC wins for the
`octo-ident` substrate; RFC-0010 needs a pointer amendment. Tracked as A15 in
RFC-0009 — flag it in the PR rather than silently conflicting.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Dual-format
  signature algorithms and the ZK-capable key, BC#13, AC#11/AC#12/AC#15/AC#16,
  A12, A14, A15
- `rfcs/accepted/process/rfc-0010-*` — conflicting discriminator definition
- `missions/open/0009-c-verification-dispatch` — every call site
- `missions/open/0009-d-stark-schnorr-algorithm`
- `missions/open/0009-e-signable-domain-separation`
- `missions/open/0009-v12-identity-evolution` — the v1.2 mission this does not
  supersede

## Claimant

@unassigned

## Pull Request

#
