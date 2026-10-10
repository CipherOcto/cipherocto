---
name: 0009-e6-registration-key-context
description: RFC-0009 AC#13. Close the wallet store's bare-sign door and mint a verification method at registration, so a normally-registered operator identity can actually sign under the RFC-0009 prefix.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e-signable-domain-separation
    - 0009-e2-lifecycle-proof-domain-separation
    - 0009-e3-wallet-node-signing-oracle
    - 0009-e4-attach-handle-prefix-in-band
---

# Mission `0009-e6-registration-key-context` — IMPLEMENTED 2026-10-10

## Status

Claimed and implemented (2026-10-10). Two defects, one found and one exposed
while fixing the first. Three vectors, two mutations, all red. `octo-wallet`
351 lib green; clippy clean with `--all-targets --all-features` on
`octo-wallet` and `octo-cli`.

## Dependencies

Everything above. This mission is the _last_ of the `0009-e` production sign
sites, and it is the one that made the others usable.

## §Defect one: every registered identity could not sign anything

This is the finding. `0009-e2` gave `IdentityRecord` a persisted
`verification_method_id` and carried it at `register` — but it only carried
whatever the incoming key already had. And **neither `IdentityKey::from_seed`
nor `IdentityKey::generate` attaches one.** Both return
`verification_method_id: None`.

Those two constructors are exactly what `octo identity register` uses
(`octo-cli/src/commands::identity.rs` §the register path: seed file →
`from_seed`, otherwise `generate`, then straight into `store.register`). So
**every normally-registered operator identity has no key context.**

The consequence is not subtle. `sign_prefixed` refuses with `NoKeyContext`,
and so does every migrated site that depends on it:

- `octo-runtime` §`sign_attach_handle_payload` — so `octo agent run --detach`
  fails for a real operator identity.
- `octo-wallet-node` §`SignHandler::handle` — the signing oracle refuses.
- `UnlockedWallet`'s own prefixed wrapper.

It is fail-closed, which is why nothing looked broken. But it means the entire
RFC-0009 prefix substrate — the thing `0009-e`, `0009-e2`, `0009-e3` and
`0009-e4` were all built to deliver — is **unreachable for every identity the
platform can actually create**. The migrations were correct and the platform
could not use them.

This was not visible while `0009-e3`/`0009-e4` were landing because their test
fixtures attach a `key_id` explicitly (as those missions record: the
requirement is meant to be visible at the call site). The gap only surfaced
when the wrapper's own two tests were moved onto the prefixed path and failed
with `NoKeyContext` — which is the second time this session a test I was
rewriting for an unrelated reason exposed the real defect.

## §Defect two: an unbound signing door on an unlocked handle

`UnlockedWallet::sign(&self, msg)` delegated straight to `IdentityKey::sign`
and signed whatever it was handed, binding nothing. An `UnlockedWallet` is the
natural place to reach for a key, which makes a bare-sign entry point there the
one most likely to be walked through by whoever writes the next feature.

It had **no production callers** — two uses in its own test module. Removed and
replaced by `sign_prefixed`.

Removed rather than deprecated, deliberately: a deprecated bare-sign entry
point is still an entry point, and the compiler is a stronger guard than a
lint. This follows the standing directive that nothing is live.

## Scope

1. **`register` attaches a verification method when the key has none**,
   before anything is written.
2. **`UnlockedWallet::sign` is removed**; `sign_prefixed` replaces it.
3. **An explicit caller choice is preserved.** A key that arrives with its own
   `verification_method_id` keeps it.

## §Why `key-0`, and why not derived from the key

The default is `VerificationMethodId::derive("key-0")`.

RFC-0009 defines `VerificationMethodId` as "stable identity of one key **within
one DID document**" and requires only that it be fixed-width, stable in that
document, and rotatable independently of the key. It does not prescribe who
mints one. `key-0` is the DID-document convention for a document's first key.

**It is deliberately not derived from key material.** Anything derived from the
public key would defeat the reason the `key_id` exists — a key must be
rotatable without its identifier changing. A name chosen at authoring time
satisfies the rule and nothing else.

Two identities sharing `key-0` is not a collision, and worth stating why: the
id is scoped to the document. A signature replayed into another document still
fails, because the verifying key is resolved from that document's DID and a
different document means a different key. The prefix binds _which key within
this document_, and the DID is what says which document.

## Test vectors

| Vector                                               | What it pins                                 |
| ---------------------------------------------------- | -------------------------------------------- |
| `a_registered_identity_can_sign_under_the_prefix`    | registration leaves a **usable** key context |
| `register_preserves_an_explicit_verification_method` | a caller's choice is not silently clobbered  |

## Acceptance criteria

- [x] `register` attaches a verification method when the incoming key has none.
- [x] A key arriving with its own keeps it.
- [x] `UnlockedWallet::sign` no longer exists; `sign_prefixed` replaces it.
- [x] Every vector above exists under its specified name and is
      mutation-checked.
- [x] Clippy clean with `--all-targets --all-features` on `octo-wallet` and
      `octo-cli`.

## §Mutation evidence

Two mutations, both red.

| Mutation                                              | Vectors that went red                                                                                                                                                                           |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `register` stops attaching a default key context      | `a_registered_identity_can_sign_under_the_prefix`, plus `tv_x_21_unlock_then_sign_verifies_with_recorded_pubkey` and `tv_x_71_register_then_unlock_round_trip_with_floor_passphrase`            |
| `register` always overwrites, clobbering the caller's | `register_preserves_an_explicit_verification_method`, `the_persisted_form_is_the_raw_uuid_bytes`, `a_key_context_survives_the_store_round_trip`, `a_proof_minted_after_a_reload_still_verifies` |

The second row is the one that shows the first vector is not enough. A fix that
unconditionally stamped `key-0` would satisfy "every registered identity can
sign" while silently destroying a rotation-aware choice — and four vectors,
including three from `0009-e2`, would catch it.
