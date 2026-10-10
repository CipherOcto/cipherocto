---
name: 0009-e7-attach-keyset-unification
description: Unify the attach-handle KeyId with RFC-0009 VerificationMethodId, retire the key_id byte suffix and the hardcoded algorithm, and fix a grace window that could never accept anything.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e4-attach-handle-prefix-in-band
---

# Mission `0009-e7-attach-keyset-unification` — IMPLEMENTED 2026-10-10

## Status

Claimed and implemented (2026-10-10). Closes the work `0009-e4` deferred. Two
vectors renamed, two added, two mutations, all red. `octo-runtime` 90 lib green
by default and 101 with `--features octo-attach-key-rotation`; clippy clean with
`--all-targets --all-features`.

## Dependencies

`0009-e4`. Everything below depends on the prefix being a token field.

## §The suffix was a parallel key identifier

`KeyId` was `pub type KeyId = u32` — a key-generation counter. RFC-0009's
`VerificationMethodId` is a stable identity for one key within one DID
document. They are the same concept twice, which is exactly what let the v2
`key_id.to_be_bytes()` suffix exist: the token bound a _generation counter the
verifier supplied_, while the prefix binds a _verification method id the token
carries_. With the prefix on the token, the counter has no source — a verifier
holding only a token cannot guess which generation number to use.

A counter also cannot state the property the grace window needs. "Rotate
key_id X from pubkey P1 to P2" requires the id to survive the key change; a
monotonic counter says _when_ a key was issued, not _which_ key.

`KeyId` is now `octo_wallet::VerificationMethodId`. The name is kept so the
`KeySet` API reads unchanged.

## §The suffix is gone, and so is the hardcoded algorithm

- `canonical_payload_bytes_v2` deleted — it existed only to append the suffix.
- `sign_attach_handle_payload_v2` / `verify_attach_handle_payload_v2` no longer
  take a `key_id`. The verifier reads it from the token's own prefix.
- `attach_algorithm_id()` deleted. It had exactly one caller, the v2 verify
  path, and it hardcoded Ed25519 as a verifier preference — the RFC-0009
  violation `0009-e4` fixed on the v1 path. v2 now dispatches on the prefix.

## §A pre-existing bug: the grace window could never accept anything

Found while porting the v2 tests, not by reasoning about them.

The old verifier did:

```rust
if let Some(pk) = key_set.lookup(key_id) {
    return verify_bytes(&attach_algorithm_id(), pk, &msg, sig);  // returns on MISMATCH
}
// grace fallback — unreachable whenever the id was in the active set
```

It returned `BadSignature` on an active-key **mismatch**, so the grace window
was only ever consulted when the key id was _absent_ from the active set. But
`move_to_grace(K)` removes the active entry, so a rotated key is never both
absent-and-in-grace under the old `insert` (see below). The grace window was
therefore dead: a rotation silently invalidated every in-flight token, which is
the exact failure the window exists to prevent. Its own doc comment described
the correct fall-through, so the code contradicted its documentation.

Fixed to try the active key, fall through to grace on mismatch, and only then
report the failure.

## §The KeySet could not represent a rotation

Compounding the above: `grace_keys` was `BTreeMap<KeyId, [u8; 32]>` (one
retired pubkey per id) and `insert` cleared grace on every write. So
`move_to_grace(K)` followed by `insert(K, new)` — the rotation sequence — threw
the retired key away on the second call. Once `KeyId` is a stable id (a rotation
keeps the id and changes the key), that is the _only_ rotation sequence, and it
was impossible.

Now `grace_keys` is `BTreeMap<KeyId, Vec<[u8; 32]>>`, `insert` preserves
grace, and `retire_grace_keys` ends the window explicitly. A key rotated twice
retains both retired pubkeys.

## Scope

1. `KeyId` unified with `VerificationMethodId`.
2. v2 suffix and `attach_algorithm_id` removed; the key comes from the token.
3. Grace fall-through fixed.
4. `KeySet` models a stable-id rotation.
5. `known_key_ids` deduplicated (it could previously report the same id twice,
   once from each map).

## Test vectors

| Vector                                                      | What it pins                                                   |
| ----------------------------------------------------------- | -------------------------------------------------------------- |
| `sign_v2_binds_the_prefix_not_a_key_id_suffix`              | v2 signs the same bytes v1 does                                |
| `the_key_is_read_from_the_token_not_supplied_by_the_caller` | a token naming an unknown key is rejected on the token's claim |
| `verify_v2_grace_period_accepts_rotated_key`                | the grace window actually accepts a pre-rotation token         |
| `verify_v2_grace_key_does_not_vouch_for_another_keys_token` | a retired key cannot vouch for a different id                  |
| `publishing_a_new_key_keeps_the_retired_one`                | `insert` no longer clears grace                                |
| `repeated_rotations_accumulate_grace_entries`               | a key rotated twice keeps both retired pubkeys                 |

## §Mutation evidence

| Mutation                                                          | Vectors that went red                                                                                                                     |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Grace fall-through reverted to an early return on active mismatch | `verify_v2_grace_period_accepts_rotated_key`                                                                                              |
| `insert` clears grace again                                       | `publishing_a_new_key_keeps_the_retired_one`, `repeated_rotations_accumulate_grace_entries`, `verify_v2_grace_period_accepts_rotated_key` |

The second row is the one that shows the two fixes are independent. Reinstating
`insert`'s grace-clearing makes the grace-period test fail even with the
fall-through fixed, because the retired key is gone before it can be tried.

## Non-goals

The `KeySet` pubkey storage is still fixed at 32 bytes, so a 33-byte
STARK-Schnorr holder key still cannot be registered. That widening is
unchanged and open, per RFC-0015-a §6.5.

`octo-attach-key-rotation` remains a feature gate — but now it gates
_`KeySet`-based multi-key verification_, which is genuinely rotation-specific,
rather than _whether a token is bound at all_. That distinction is the point.
