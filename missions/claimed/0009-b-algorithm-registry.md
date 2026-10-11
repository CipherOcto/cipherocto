---
name: 0009-b-algorithm-registry
description: RFC-0009 BC#13. Replace the VerificationMethodKind central enum with a SignatureAlgorithm trait + AlgorithmRegistry keyed by AlgorithmId(Uuid). Add VerificationMethodId, revoked_at and custody fields, and the load-bearing storage migration in octo-ident-storage.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
---

# Mission `0009-b-algorithm-registry` — CLAIMED 2026-10-09

## Status

Claimed (2026-10-09). Claimant: @main. **Root mission for the RFC-0009
decomposition**; `0009-c`, `0009-d` and `0009-e` all depend on this one and MUST
NOT start before it lands.

Implementation proceeds in the order this file already fixes, because the
storage migration is load-bearing: a half-migrated table is the one state where
DIDs resolve successfully while owning no usable key.

1. Newtypes, trait, registry (this commit)
2. `VerificationMethod` field changes and enum removal
3. Storage migration across all three read paths

## Dependencies

None. This is the root of the RFC-0009 mission set. Its gating condition is
RFC-0009 acceptance, which is recorded.

## Scope

Implement RFC-0009 §Dual-format signature algorithms and the ZK-capable key
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
     the mechanism meant to prevent them. **This holds even when the two
     implementations are byte-identical:** two crates linking two versions of
     the same algorithm is a link-time fact the registry must surface, not
     absorb. An implementer who adds an `if impl == existing { Ok(()) }`
     fast path satisfies the obvious reading and reintroduces the silent
     swap.
   - `populate` marks the registry ready. A lookup before that is
     `NotPopulated`, **not** `UnknownAlgorithm` — the first is a deployment
     that has not booted, the second is an algorithm nobody registered.
   - `get` MISSES on an unregistered id. There is no `Default` arm and no
     fallback. Returning `Ed25519` here is the exact failure A12 exists to
     prevent.

### Two things the registry shape does not settle on its own

**Borsh representation of the newtypes — settled.** `AlgorithmId`,
`VerificationMethodId` and `CircuitId` each store a `[u8; 16]` field and
expose `to_uuid` / `from_uuid`. Not a preference: **`Uuid` has no borsh
implementation available in this workspace.** No crate enables a `uuid`
borsh feature — every `uuid` dependency in `crates/` requests `v4`, `v5` and/or
`serde` only — and no crate derives borsh on a struct containing a `Uuid`
field, so `#[derive(BorshSerialize)] struct AlgorithmId(Uuid)` does not
compile here. `crates/octo-ident` still needs the `uuid` dependency this
mission adds, but for `to_uuid`/`from_uuid` and `Uuid::new_v5`, not for derive.

Storing the bytes also makes the signable prefix's fixed-16-byte
self-delimiting property true **by construction** rather than by a serialiser's
behaviour, and it is the only form in which the storage decode and the prefix
agree on what a `key_id` byte is. Deriving borsh over a `String`-backed or
`Uuid`-backed newtype would let those two disagree silently.

**The singleton invariant is unenforceable.** §Algorithms says the bootstrap
registers Ed25519 **once**, globally. No code can check that: the registry
exposes `get`, which accepts any `AlgorithmId`, and a rogue `populate()`
admitting a second Ed25519-family implementation is indistinguishable from a
correct one to every caller. So either the registry enforces it — by rejecting
any registration whose family collides with a singleton, which means the
trait has to expose a family — or the bootstrap is the only enforcement and
that is worth saying out loud. What must not happen is the mission implying
the shape guarantees something it does not.

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

### The legacy blob carries no `id`, and that is a separate problem

The migration rule above covers the **algorithm** byte. It says nothing about
the **key** id, and a legacy blob cannot supply one: the current
`VerificationMethod` is exactly `kind: VerificationMethodKind` plus
`public_key: [u8; 32]`, borsh-encoded. There is no `id` field in it.

RFC-0009 forbids both obvious repairs. §Domain separation is mandatory, not
hygiene: `key_id` "MUST NOT be derived from the public key, and it MUST NOT
be locally allocated at signing time." So:

- `Uuid::new_v4()` per decode gives an **unstable** id — a different one on
  every `resolve`. Any signature made over the signable prefix against a
  migrated method becomes unverifiable, which breaks `0009-e`.
- A deterministic derivation from the public key violates the MUST directly,
  and is the mistake RFC-0009 singles out when it rejected the existing
  string-fragment key id.

**The migration rule for `id` is therefore: derive it once, deterministically,
from the document identity — `Uuid::new_v5(MIGRATION_NAMESPACE, canonical_hash
‖ legacy_index)` — never from key material, and never regenerate it.** A
migrated method's id is stable across `resolve` calls, across processes, and
across resolvers, which is the property the signable prefix depends on. Use a
reserved namespace constant so this can never be confused with a
locally-allocated id.

### NULL, empty blob, and undecodable blob are three cases, not one

The decode site has **two** `unwrap_or_default()` calls, not one: the outer
one turns a NULL column into an empty `Vec` without ever reaching borsh, the
inner one swallows a malformed blob. "MUST NOT route through
`unwrap_or_default()`" does not say which survives, and the three cases pull
in different directions:

| Column state                                                     | Required behaviour           | Why                                                                                                                       |
| ---------------------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| NULL                                                             | empty list, **not** an `Err` | legacy pre-v009 rows are legitimately empty; RFC-0010 §Forward compatibility fail-soft survives for genuinely absent data |
| empty blob                                                       | empty list                   | a present-but-empty column is absence, not corruption                                                                     |
| present and undecodable under **both** the new and legacy shapes | `Err`                        | this is the silent-downgrade case BC#13 exists to catch                                                                   |

There is one existing green test touching the empty-blob case:
`resolve_legacy_row_returns_empty_vevs` in the storage test suite (its name
carries a typo — `vevs` — worth fixing while the file is open, so the next
person grepping for it finds it). **Read it for what it is: it never touches
`StoolapDidRegistry`.** Its own comment says the registry has no `db`
accessor, so the test exercises `borsh::from_slice::<Vec<T>>` on an empty BLOB
directly. It pins borsh's behaviour, and nothing else.

So it guards neither row of the table that matters: it does not exercise
`resolve`, so it cannot detect removal of the outer `unwrap_or_default()`, and
it does not cover the NULL column at all. **Add vectors for both** — a
NULL-column row resolving to an empty list through `resolve`, and a
present-but-undecodable blob returning `Err` through `resolve`. Neither exists
today.

### `list` is a third path, and it is worse

The same file's `list` does not decode the rich columns at all. Its query
selects `public_key` only, and every document is built with
`..Default::default()`, so every row comes back with zero verification
methods — before and after the migration, and with no error to observe.

The migration instruction above covers `resolve` and `resolve_in_chain` and
would leave `list` untouched. That is the same defect as the one above,
reached by a shorter route: a DID that resolves, carries a public key, and
owns no algorithm-bearing key. Once `revoked_at` exists per key, anything
that enumerates identities through `list` sees no keys and therefore nothing
to revoke, which is precisely the vacuous cascade A14 was added to stop.

Nothing in `crates/` calls `DidRegistry::list` outside tests today, which is
the only reason this has stayed invisible. That is not a reason to skip it:
the fix is the same column selection and the same decode path as `resolve`,
and leaving one reader of the table on the old shape means the storage
contract has two incompatible readers. Fix `list` in this mission and state
the requirement in the RFC, or record explicitly in BC#13 that `list` is out
of scope and that its callers MUST NOT be used for key-bearing or
revocation-bearing work until it is fixed.

### Also in scope

`VerificationMethodKind::from_byte` collapses every unknown byte onto
`Reserved` with `Reserved.as_byte() == 0x00`, so two unknown algorithms become
mutually indistinguishable. Its doc comment claims the opposite — that the
mapping makes old code "fail-closed on unknown discriminators". Removing the
enum removes the contradiction; do not port the comment.

`MAX_VERIFICATION_METHODS` in the same module is `2`, documented as covering
"the canonical Ed25519 + PQC future combination". Under v1.3 that composition
is Ed25519 + STARK-curve Schnorr, so the stated rationale is stale and the
bound is now exactly saturated with zero headroom. The value itself does not
need to change — two methods is the whole design — but the doc comment is a
normative-looking claim about a design that is no longer the one specified, and
it is the same class of defect as the dangling `§VerificationMethodKind`
citation: a doc comment asserting something no specification supports. Correct
the wording, and record that the next algorithm lands by raising the bound
rather than by reusing a slot.

## Known gaps, recorded not hidden

Three things this mission deliberately does **not** close. All three were
raised by the R43 review of the implementation; none is safe to wave away.

**`list` fails the whole enumeration on one undecodable row.** Fail-closed, and
that is the right trade here: a revocation cascade that skips an unreadable row
would treat its revoked key as absent, which is precisely the vacuous cascade
A14 exists to prevent. The cost is availability — one bad row takes `list` down
for every identity — and it is theoretical today, because nothing in `crates/`
calls `DidRegistry::list` outside tests. If a caller needs partial results,
the fix is a typed per-row outcome on the trait, not a silent skip. **Do not
"fix" this by skipping rows.**

**The registry is not yet the process-wide singleton.** RFC-0009 requires it,
owned by the defining crate and populated by an init function before any
verification runs. `new()` is public and `Default` is derived, so two verifiers
in one process could disagree about which algorithms exist. The bootstrap lands
with `0009-c`, which is where the first real caller appears; until then the type
is inert. Stated in the module doc too, so a reader does not have to discover
it by grepping for call sites and finding nothing.

**Key-id uniqueness within a document is not enforced.** `VerificationMethod::new`
documents the obligation and the constructor forces the caller to choose an id,
so the accidental collision a constant-id constructor caused is gone. But
nothing rejects two methods sharing an id, and the migration derives distinct
ids per entry. A writer that reuses an id gets a document where per-key
revocation is ambiguous. Worth a decode-time check when a writer-side entry
point exists to hang it on.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met. Do not add a
separate checkbox list here — it would duplicate the vectors and let the two
drift apart.

## Test Vectors

AC#16 names three of these in RFC-0009; the rest are this mission's own
additions. The migration itself is the acceptance condition, not the list.

- `legacy_document_migrates_to_algorithm_id` — a pre-migration document
  decodes through the migration path with a non-empty method list and the
  correct Ed25519 `algorithm_id`
- `migrated_id_is_stable_across_decodes` — the `new_v5` derivation, not `new_v4`
- `current_shape_is_not_migrated` — a row already in the new shape passes
  through untouched, so the writer's own id is not re-derived
- `migrated_document_still_resolves_with_a_verification_method` — resolver
  level, not just codec level
- `listed_documents_carry_their_verification_methods` — the `list` path, which
  is a separate column selection and not covered by the resolver vector
- `listed_documents_report_per_key_revocation` — an enumeration that returns no
  keys also returns nothing to revoke
- `post_migration_decode_failure_is_an_error` — **the one that matters most.**
  Asserts a malformed blob is `Err`, not an empty list. Covers a truncated
  mid-entry write too
- `resolve_reports_a_corrupt_row_as_an_error_not_an_empty_list` — the same
  property at the boundary a caller actually sees. Needs `from_database`
- `list_reports_a_corrupt_row_as_an_error_too` — `list` has its own column
  selection and its own decode call, so the resolver vector cannot observe it
- `resolve_migrates_a_legacy_row_end_to_end` — the migration through `resolve`,
  not the codec
- `resolve_returns_an_empty_list_for_a_null_column` — the NULL half of the
  three-case rule, which the codec-level vector cannot reach
- `null_and_empty_blob_yield_an_empty_list_not_an_error` — the codec half
- `migrated_id_does_not_depend_on_platform_word_size`
- `legacy_non_ed25519_byte_fails_closed_and_stays_distinct`
- `two_keys_in_one_document_have_distinct_ids`
- `zk_support_none_is_reported_by_the_algorithm`
- `unregistered_algorithm_id_is_rejected_after_population`
- `unregistered_algorithm_id_is_not_silently_ed25519` — Ed25519 **is**
  registered and the lookup still misses, which is the A12 property in its
  sharpest form
- `lookup_before_population_is_not_populated`
- `unknown_algorithm_ids_stay_distinguishable` — the property the deleted
  enum violated; a former test asserted the opposite
- `duplicate_registration_is_rejected_even_when_byte_identical`
- `identifiers_are_sixteen_bytes_and_round_trip`
- `derivation_is_stable_across_calls`
- `distinct_namespaces_produce_distinct_identifiers`
- `raw_prefix_is_self_delimiting`
- `borsh_wire_form_is_exactly_sixteen_bytes` — gated on the `borsh` feature
- `wrong_length_is_rejected_not_accepted`
- `zk_support_is_reported_by_the_algorithm`
- `verification_method_ed25519_default`
- `verification_method_revocation_is_per_key`
- `custody_posture_is_distinguishable`
- `tv5_verification_method_type_discriminator` — rewritten; the original
  could not detect a collapsed discriminator because it only compared one
  method against `Reserved`

**Names drifted deliberately.** Three vectors below are named for a stronger
property than the mission originally specified
(`duplicate_registration_is_rejected_even_when_byte_identical`,
`unknown_algorithm_ids_stay_distinguishable`, and the byte-exact borsh form).
The mission list is updated to the real test names rather than the tests being
weakened to match the list.

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

None, by design. RFC-0009 removed the `derive_capability_key_v11` shim
and adds none here: nothing is deployed, and a `#[deprecated]` path that keeps
a lossy decoder reachable has no audience.

`VerificationMethodKind` appears in no Accepted RFC. The only trace is a
doc comment in `crates/octo-ident/src/rich_document.rs` citing
"RFC-0010 §VerificationMethodKind" as normative, and neither RFC-0010 file
contains that section or the symbol. There is therefore no cross-RFC conflict
and nothing to amend in RFC-0010 — an earlier draft of this mission claimed
one and was wrong. The enum is currently defined in code with a citation that
resolves nowhere, and removing the enum removes the dangling citation with
it. Tracked as A15 in RFC-0009.

Do not open an RFC-0010 amendment on the strength of this mission.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Dual-format
  signature algorithms and the ZK-capable key, §Domain separation, BC#13,
  AC#11/AC#12/AC#15/AC#16, A12, A14, A15
- `rfcs/accepted/process/0010-canonical-did-codec.md` — the real RFC-0010
  Rich DID Document codec. Cited for the forward-compatibility fail-soft
  contract that survives the `list` and NULL-column cases above. There is **no**
  cross-RFC discriminator conflict; RFC-0009 retracted the claim that
  there was, and an earlier draft of this mission repeated it
- `missions/claimed/0009-c-verification-dispatch` — every call site
- `missions/claimed/0009-d-stark-schnorr-algorithm`
- `missions/claimed/0009-e-signable-domain-separation`
- `missions/open/0009-v12-identity-evolution` — the v1.2 mission this does not
  supersede

## Claimant

@main (claimed 2026-10-09)

## Pull Request

#
