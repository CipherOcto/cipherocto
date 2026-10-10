---
name: 0009-e3-wallet-node-signing-oracle
description: RFC-0009 AC#13. Close the octo-wallet-node WALLET_SIGN_ED25519 signing oracle, which signs a bare 32-byte digest with no algorithm or key binding, by signing under the identity's own signable prefix.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e-signable-domain-separation
    - 0009-e2-lifecycle-proof-domain-separation
---

# Mission `0009-e3-wallet-node-signing-oracle` — IMPLEMENTED 2026-10-10

## Status

Claimed and implemented (2026-10-10). All five §Test Vectors are in place
under their specified names and every one is mutation-checked. `octo-wallet-node`
44 lib tests green; clippy clean with `--all-targets --all-features` on
`octo-wallet-node` and `octo-wallet`.

## Dependencies

`0009-e` supplies `sign_prefixed` / `verify_prefixed` / `prefixed_message` on
`IdentityKey`, and `0009-e2` persists `verification_method_id` so an unlocked
identity can carry a `key_id`. Both are prerequisites; neither is re-done
here.

## Scope

`octo-wallet-node` §`SignHandler::handle` signs `req.0` — a bare 32-byte
digest — with `IdentityKey::sign`, and returns a bare 64-byte signature. The
result is a **remote signing oracle**: any peer that can reach the handler can
obtain a signature over any 32-byte value of its choosing, and that signature
is byte-identical in form to one the same key would produce for any other
purpose. Nothing in the signed bytes names the algorithm, the key, or the
domain.

1. **The handler signs under the identity's own prefix.** `sign_prefixed`
   rather than `sign`, so every signature the oracle emits is bound to
   `algorithm_id ‖ key_id ‖ canonical_ser(payload)`.
2. **The identity supplies the prefix, never the caller.** See §Who builds the
   prefix — this is the load-bearing design decision, not an incidental one.
3. **The wire form of `SignRequest` does not change.** It stays
   `[u8; 32]`. The prefix is not transported; the signer applies its own and
   the verifier reconstructs it from the resolved key context.
4. **Refusal over downgrade.** An identity with no `key_id` must not serve
   this handler by falling back to a bare signature.

## Non-goals

- The other three outstanding `0009-e` sites (`IdentityKey::sign` itself, the
  `identity_store` wrapper, the two `octo-runtime` §`handle/signing` sites).
- The `CapabilitySigner::sign` trait change in `octo-cap-macaroon`. The mint and
  attenuate handlers reach the signer through that trait, not through
  `IdentityKey::sign`, so they are a separate migration.
- Adding a `key_id` to the `WALLET_SIGN_ED25519` response. Not required, and it
  would move an RFC-0871 wire format to no benefit — the verifier resolves the
  key context anyway.

## §Who builds the prefix

There are two designs, and the choice is the whole mission.

**The caller builds it** — `SignRequest` carries `prefix ‖ payload`, the wallet
signs those bytes verbatim. This is how `SignedMessage` is meant to travel, and
it looks more faithful to RFC-0009. It is also the worse design _here_, for a
reason specific to what this endpoint is: the caller would be choosing the
`algorithm_id` and `key_id` the wallet signs under. Unless the wallet
re-validates that the offered prefix is its own — at which point it has
re-derived the prefix anyway and may as well build it — the oracle can be
steered into emitting a signature that claims a different key or a different
algorithm. For a signing oracle, letting the requester name the domain inverts
the control the prefix exists to establish.

**The wallet builds it.** `sign_prefixed(&req.0)` applies the identity's own
prefix, and the verifier reconstructs the same wire form with
`prefixed_message(&req.0)` before calling `verify_prefixed`.

This is chosen. It also happens to require no wire change at all, which is a
consequence rather than the reason — the reason is that the signer, not the
requester, must name the domain.

Note this does not weaken the in-band parsing. `verify_prefixed` still parses
the prefix out of the wire bytes and refuses a `key_id` that is not the
verifying key's; the bytes are assembled from the resolved key context rather
than received. That is the same "algorithm travels out-of-band with the key
wherever a resolver already exists" decision `0009-e` already took.

## Test vectors

| Vector                                                        | What it pins                                      |
| ------------------------------------------------------------- | ------------------------------------------------- |
| `the_oracle_refuses_to_emit_a_bare_digest_signature`          | the signature does not verify as a bare signature |
| `the_oracle_signature_verifies_under_the_prefix`              | it does verify under the prefixed path            |
| `an_identity_with_no_key_id_cannot_serve_the_oracle`          | no silent downgrade to bare signing               |
| `a_signature_from_one_identitys_oracle_is_refused_by_another` | cross-key replay through the oracle is dead       |
| `the_sign_request_wire_form_is_unchanged`                     | RFC-0871's request contract did not move          |

## Acceptance criteria

- [x] `SignHandler::handle` calls `sign_prefixed`, and the grep for a bare
      `IdentityKey::sign` in the handler is empty.
- [x] `SignRequest` remains `[u8; 32]` and its borsh round-trip is unchanged.
- [x] Every vector above exists under its specified name and is mutation-checked.
- [x] Clippy clean with `--all-targets --all-features` on `octo-wallet-node`.
- [x] The `0009-e` outstanding table is updated to mark this site migrated.

## Design decisions

- **Refuse rather than downgrade when no `key_id` is attached.** `0009-e`
  already established that a downgrade path is how an unprefixed message keeps
  verifying after the prefix is deployed. This endpoint is the sharpest case
  for that rule: it exists to sign _anything_, so a fallback there is not a
  downgrade, it is the default.
- **`from_seed` identities cannot serve this handler.** `from_seed` returns
  `verification_method_id: None`, so `sign_prefixed` refuses with
  `WalletError::NoKeyContext`. That is correct and intended. Production
  identities come from `identity_store::unlock`, which restores the persisted
  `verification_method_id` per `0009-e2`. Test fixtures that build an identity
  with `from_seed` must attach a `key_id` explicitly — and that is the point:
  the requirement is visible at the call site rather than assumed.

## §Mutation evidence

Four mutations, all red, each aimed at the implementation rather than at a
test's own assertion.

| Mutation                                                       | Vector(s) that went red                                                                |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Handler reverted to `IdentityKey::sign`                        | `refuses_to_emit_a_bare_digest`, `verifies_under_the_prefix`, `no_key_id_cannot_serve` |
| `verify_prefixed`'s `key_id` comparison disabled               | `a_signature_from_one_identitys_oracle_is_refused_by_another`                          |
| Handler falls back to `sign` on `NoKeyContext` (the downgrade) | `an_identity_with_no_key_id_cannot_serve_the_oracle`                                   |
| `to_borsh` given a length prefix (the wire-form break)         | `the_sign_request_wire_form_is_unchanged`                                              |

Two of these are worth calling out because they are not the same test.

The `key_id` mutation was necessary because reverting the handler does **not**
make the cross-key vector fail: with a bare signature, `verify_prefixed` rejects
the mismatched `key_id` anyway, so that vector stayed green under the obvious
mutation and would have looked like it was covering something it was not. Its
real dependency is the `key_id` comparison in `verify_prefixed`, so that is
what had to be broken to show the vector is live.

The downgrade mutation is the one this mission exists to prevent, and it is
not the same as the plain revert: it keeps `sign_prefixed` as the happy path
and falls back only on `NoKeyContext`. That is the realistic way this control
gets quietly removed by a well-meaning "make the node work again" change, and
`an_identity_with_no_key_id_cannot_serve_the_oracle` catches it on its own.

## §A bad assertion, and what it should have been

The first cut of `an_identity_with_no_key_id_cannot_serve_the_oracle` asserted
the error text contained `key`. It failed — correctly — because
`WalletError::NoKeyContext` renders as "no **verification-method** id attached",
which does not contain that substring. The handler was right and the assertion
was wrong.

The fix was to match the `ProtocolError` variant and then the reason, rather
than reach for a looser substring. A loose substring would have passed on the
_lifecycle_ error too, and the lifecycle check is not what this vector is
testing — the identity under test is deliberately activated. This is the
narrower lesson of the vacuous-vector failures recorded in `0009-e`: a
substring proxy for an error is an assertion about wording, not about
behaviour, and it can go red for the wrong reason just as easily as it can go
green for one.
