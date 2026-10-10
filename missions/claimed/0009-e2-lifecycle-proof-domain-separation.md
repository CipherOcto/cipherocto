---
name: 0009-e2-lifecycle-proof-domain-separation
description: RFC-0009 AC#13, migration slice 1 of 0009-e. Prefix the revocation and successor lifecycle proofs, which today sign a 6-byte constant and a bare concatenation and are verified against a caller-supplied algorithm id.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-e-signable-domain-separation
---

# Mission `0009-e2-lifecycle-proof-domain-separation` — IMPLEMENTED 2026-10-09

## Status

**Both lifecycle proofs are migrated.** `octo-wallet` 349 lib + 3
lifecycle-replay + 3 ask-publish, all passing. Clippy clean with
`--all-targets --all-features`.

This mission ran twice. The first attempt bound both proofs and broke **18
tests**, and the reason turned out to be an ordering constraint rather than an
estimate. Both halves are recorded below, because the first failure is what
identified the right sequence.

### The ordering constraint the first attempt found

The first attempt bound both lifecycle proofs and broke **18 tests**. Eleven of
them are in `identity_store` and they are the informative ones:

`begin_rotation` returning a bound proof needs a key context, and
`IdentityKey::from_record` does not have one — `IdentityRecord` does not carry
it. So rotation would have succeeded on a live key and **failed after the store
was reloaded**, including in `tv_x_49_rotate_complete_survives_a_store_reopen`,
whose entire purpose is to check that a rotation survives persistence.

That is the worst of the three available outcomes. Refusing everywhere is
uniform and safe; permissively signing unbound proofs is uniform and weak. A
behaviour that depends on whether the process has saved recently is neither,
and it is a genuinely hard bug to diagnose later.

So the prerequisite was `IdentityRecord` persisting `verification_method_id`,
which has since landed, and the successor proof followed. The sequence is
recorded so the reasoning survives even though both halves are now done.

### The persistence work

`IdentityRecord` gains `verification_method_id: Option<[u8; 16]>` with
`#[serde(default)]`, carried from the key at §`register` and restored at
§`unlock` and in the successor rehydration path. Raw bytes, not the textual
UUID — the distinction RFC-0009 §Domain separation makes normative, and the
signable prefix reads exactly these.

`None` is preserved as an honest value rather than papered over: a record
written before the field parses, its identity simply has no key context, and it
still signs and revokes, meeting a bound proof with an explicit refusal rather
than a silently wrong answer.

### The store stopped re-signing to check a proof

`rehydrate_successor_key` used to _recompute_ `b"rotate" || new_pub`, sign it
with the predecessor key, and compare bytes. With the prefix that would have
meant **reproducing the prefix in a second place** — and any drift between
that function and `begin_rotation` would silently accept a forged rotation
event. It now calls the same `verify_successor_proof` the rest of the system
uses. One implementation, two callers, no second encoding to drift.

This is the one change here that was not strictly required, and the one most
easily missed: the re-sign still _passed_ every vector while the two encodings
quietly diverged.

### Revocation is deliberately asymmetric with rotation

`revoke()` **never fails for want of a key context.** An identity without one
revokes normally and simply records no bound audit proof.

Gating the ability to revoke on an optional audit annotation would hand anyone
who can strip a `key_id` from a stored record a **permanent denial of service
on the safety action itself** — an identity that can never be revoked. That is
strictly worse than the weaker proof it would buy. `begin_rotation` refuses,
because rotation is an operator action with the caller present and an unbound
successor proof is not a weaker proof — it is no proof at all, since it is the
authorisation for the handoff.

The asymmetry is intentional and is pinned by
`revocation_is_never_blocked_by_a_missing_key_context`.

### What the revocation proof did and did not bind

Before: a signature over the six bytes `b"revoke"`. It attested to nothing
beyond "someone holding this key signed the word revoke" — the same six bytes
under any algorithm, any verification method, any time.

After: it is bound to this identity's `algorithm_id` and `key_id`. It still
does not bind a timestamp. RFC-0009 puts `revoked_at` on the
`VerificationMethod` and the proof attests to the lifecycle edge, not to when
it happened, so **this mission must not be read as "the revocation proof is
now strong"** — it is "the revocation proof now binds its algorithm and key,
which it previously did not bind at all."

### Mutation-verified

Nine mutations across both halves, all red.

| Mutation                                           | Caught by                                                                                                    |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Store drops the key context on reload              | `a_key_context_survives_the_store_round_trip`                                                                |
| `register` stops persisting the key id             | `the_persisted_form_is_the_raw_uuid_bytes`                                                                   |
| Persisted form is not the raw bytes                | `the_persisted_form_is_the_raw_uuid_bytes`                                                                   |
| Successor proof signed without a prefix            | `a_successor_proof_is_bound_to_its_key_id`                                                                   |
| Rehydration stops checking the successor proof     | `tv_x_49_rotate_complete_survives_a_store_reopen`, `tv_x_65_a_rotation_event_with_a_forged_proof_is_refused` |
| Lifecycle verifier ignores the `key_id`            | `a_revocation_proof_is_bound_to_its_key_id`                                                                  |
| Revocation blocked by a missing key context        | `revocation_is_never_blocked_by_a_missing_key_context`                                                       |
| Revocation proof signed without a prefix           | `a_revocation_proof_round_trips`                                                                             |
| Verifier accepts the bare literal (downgrade path) | `a_revocation_proof_is_refused_without_a_prefix`, `the_legacy_bare_constant_is_refused`                      |

### A trap the tests walked into

`a_revocation_proof_is_refused_without_a_prefix` first tried to mint the legacy
bare literal from an already-revoked identity. That fails at the adapter —
`revoke()` swaps the signer for a `NullSigner`, so the key cannot sign anything
afterwards. The vector now signs _before_ revoking. Worth recording because the
error is `Device("signer revoked; cannot produce new signatures")` and reads
like a wallet bug rather than the lifecycle gate working.

## Dependencies

`depends_on: 0009-e-signable-domain-separation`, which supplies the prefix
builder and `IdentityKey` §`sign_prefixed` / §`verify_prefixed`.

## Scope

Migrate the two RFC-0009 lifecycle proofs onto the mandatory signable prefix.

These are the two signatures in the repository that are dispatched on an
algorithm the **verifier's caller** chose, and they are the worst offenders:

- `IdentityKey::revoke` signs the 6-byte constant `b"revoke"`.
- `IdentityKey::begin_rotation` signs `b"rotate" ‖ new_pub`.
- `IdentityKey::verify_successor_proof` and §`verify_revocation_proof` are
  **pure static helpers** that take `algorithm_id` as a parameter, because
  they have no signer to ask.

RFC-0009 requires a verifier to "verify under the algorithm the prefix names
rather than the one it prefers". A helper whose caller hands it the algorithm
is the preference case by construction: there is nothing in the signed bytes
saying which equation was used.

## What the revocation proof does and does not bind

`b"revoke"` is a constant. As it stands the proof says only _someone holding
this key signed the six bytes "revoke"_ — it is not bound to the algorithm, to
the verification method, or to any identity beyond the key it was checked
against.

The prefix binds the algorithm and the `key_id`, which is a strict improvement
and is what this mission delivers. It does **not** bind a timestamp: RFC-0009
puts `revoked_at` on the `VerificationMethod` and the proof attests to the
lifecycle edge, not to when it happened. Anyone reading this mission should
not read it as "the revocation proof is now strong" — it is "the revocation
proof now binds its algorithm and key, which it previously did not bind at
all."

## Why the verifiers need a `key_id` parameter

They are static and have no key context. The prefix cannot be rebuilt without
one, so each verifier gains a `VerificationMethodId` parameter. That is a
breaking signature change on two public helpers, and it is unavoidable: a
verifier that cannot name the key cannot rebuild the domain the signer signed.

Callers are updated in the same change. Nothing is deployed, so there is no
shim and no compatibility window.

## Out of scope, deliberately

- **The other six production sign sites** listed in
  `missions/claimed/0009-e-signable-domain-separation` §What this does not
  cover. This is one slice, chosen because it is the one with an actual
  cross-algorithm replay to demonstrate.
- **`CapabilitySigner::sign`** in `octo-cap-macaroon`. A trait change across
  every implementor, and a separate decision about where a capability key's
  `key_id` comes from.
- **Binding a timestamp into the proof.** That is an RFC-0009 question, not an
  implementation detail, and the RFC currently places `revoked_at` on the
  document rather than in the proof.
- **`quota-router-core` §`envelope_v2`.** Its preimage may already carry a
  domain separator; prefixing it without checking would bind two domains at
  once.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `a_revocation_proof_round_trips`
- `a_revocation_proof_is_bound_to_its_key_id` — the same proof, replayed under
  another `key_id`, is refused
- `a_revocation_proof_is_refused_without_a_prefix` — the legacy bare
  `b"revoke"` shape is not a downgrade path
- `a_revocation_proof_binds_its_own_algorithm_and_key`
- `a_revocation_proof_replayed_as_the_other_algorithm_is_rejected` — the
  Ed25519 proof presented as `stark-schnorr`, over the **real** second
  algorithm from `0009-d`, not a mock
- `the_legacy_bare_constant_is_refused` — the downgrade path
- `revocation_is_never_blocked_by_a_missing_key_context` — the DoS shape

- `a_successor_proof_is_bound_to_its_key_id`
- `a_successor_proof_binds_the_successor_key` — a proof for one successor does
  not verify for another
- `a_successor_proof_is_refused_without_a_prefix`
- `a_key_context_survives_the_store_round_trip` — the persistence prerequisite
- `a_proof_minted_after_a_reload_still_verifies`
- `a_record_without_the_field_still_loads`
- `the_persisted_form_is_the_raw_uuid_bytes`

The mission's inherited caveat applies: five of these compare behaviour rather
than bytes, but only `a_revocation_proof_is_bound_to_its_key_id` and
`cross_algorithm_lifecycle_proof_replay_rejected` can detect the actual
vulnerability. Treat those two as the acceptance condition.

Gate: `cargo test -p octo-wallet --lib` and
`cargo test -p octo-algo-stark-schnorr --test lifecycle_replay`.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-wallet -p octo-algo-stark-schnorr --all-targets --all-features -- -D warnings
cargo test -p octo-wallet --lib
cargo test -p octo-wallet --test ask_publish_cli
cargo test -p octo-algo-stark-schnorr
```

## Backward compat

Breaking on two public static helpers. Nothing is deployed, so no shim. A
verifier MUST reject an unprefixed proof rather than strip the prefix and fall
back to the legacy shape.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Domain separation
  is mandatory, not hygiene; AC#13; §Dual-format signature algorithms and the
  ZK-capable key
- `rfcs/accepted/process/0853-mpc-identity-federation.md` — §12, the successor
  proof this mission prefixes
- `missions/claimed/0009-e-signable-domain-separation` — the substrate and the
  corrected migration inventory
- `missions/claimed/0009-d-stark-schnorr-algorithm` — the second algorithm the
  replay vector needs
- `crates/octo-ident/src/signable_prefix.rs` — the prefix

## Claimant

@mmacedoeu

## Pull Request

#
