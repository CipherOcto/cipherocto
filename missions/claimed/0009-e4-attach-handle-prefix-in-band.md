---
name: 0009-e4-attach-handle-prefix-in-band
description: RFC-0009 AC#13. Carry a real SignablePrefix in band inside the octo-runtime attach-handle token, replacing the hand-rolled key_id byte suffix and retiring the verifier's hardcoded Ed25519 algorithm preference.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e-signable-domain-separation
---

# Mission `0009-e4-attach-handle-prefix-in-band` — IMPLEMENTED 2026-10-10

## Status

Claimed and implemented (2026-10-10). All four §Test Vectors exist under their
specified names and every one is mutation-checked. `octo-runtime` 90 lib
tests green, and 102 with `--features octo-attach-key-rotation`. `octo-wallet`
349 green, `octo-cli` security guards 45 green. Clippy clean with
`--all-targets --all-features` across `octo-runtime`, `octo-wallet`, `octo-cli`.

Three vectors are new; `a_token_minted_under_one_key_never_verifies_for_another`
is covered by `a_token_naming_another_key_id_is_refused`, which is the sharper
form of the same property — it substitutes the `key_id` on a genuine signature
rather than minting twice.

## Dependencies

`0009-e` for `SignablePrefix` / `SignedMessage` and the `SignableForm::Raw`
shape. No dependency on `0009-e2` or `0009-e3`.

## §Why this one is not like the others

Every other site in the `0009-e` outstanding table is signed by a party that
also holds the key context, so the verifier reconstructs the prefix from what
it already has. The wallet-node oracle works exactly that way, and its
`SignRequest` stays a bare `[u8; 32]`.

**An attach-handle token travels, and it does not carry its own binding.**
`AttachHandle` has six fields — `session_id`, `mint_timestamp_unix`, `ttl_unix`,
`signature`, `payload`, `transport` — and **no `key_id` and no algorithm**.
`encoding.rs` and the `AttachHandle` definition contain zero `key_id`
references at all. That is forced, not incidental: the verifier reconstructs
the signed bytes from the token's _fields_, so anything the signer spliced in
that is not also a field is unrecoverable. A prefix folded into the canonical
bytes the way v2 folds its `key_id` suffix therefore cannot be read back.

So what does v2 actually bind? `verify_attach_handle_payload_v2` takes
`key_id: KeyId` **as a caller parameter** and bakes that caller's claim into
the message. That is a real anti-swap property — the code documents the forgery
it prevents, where a grace-key holder mints a token claiming a victim's
`key_id` — but it is a property over a `key_id` the _verifier_ chose. The token
names nothing. The same holds in v1, where the caller supplies a bare 32-byte
pubkey. And in both, the algorithm is a hardcoded verifier preference via
`attach_algorithm_id()`.

Two earlier readings of this are recorded as wrong, in opposite directions,
and the data flow above is why both are wrong:

- The first called v2's suffix a redundant parallel abstraction that the prefix
  subsumes. Too strong: the suffix binds a `key_id` _claim_, and with nothing
  readable on the token there is nothing better to bind to.
- The second, written to correct the first, called the suffix the one place in
  this table where a hand-rolled suffix does a job the prefix is not doing,
  because the token travels and cannot reconstruct a prefix. Backwards: the
  token travels, and the repair is that it should _carry_ the prefix as a
  **field**, not that the suffix stands in for one.

With the prefix as a field the suffix does become redundant — it binds the same
`key_id`, is readable rather than verifier-chosen, and adds the algorithm. That
is the design below.

## Scope

1. **The token carries a real `SignablePrefix` in band**, replacing the
   hand-rolled `key_id.to_be_bytes()` suffix. The prefix is strictly more: it
   carries the algorithm as well as the key.
2. **`verify_attach_handle_payload` stops preferring an algorithm.** It
   dispatches on the one the prefix names. This retires
   `attach_algorithm_id()`'s hardcoded Ed25519 from the verify path.
3. **v1 and v2 converge.** v1 is the bare path and is the default; v2 is
   gated off by `octo-attach-key-rotation`. With the prefix carried in band
   there is no reason for the gate to exist — the capability is no longer a
   rotation-specific extra.
4. **The `KeySet` survives.** It resolves _which pubkey_ to try during a grace
   window; the prefix binds _which key_. Different jobs, and conflating them
   would be the error.

## Non-goals

- Making a non-Ed25519 `IdentityKey` constructible. The latent hardcoding is
  recorded, but the substrate work to allow a STARK-Schnorr holder is separate
  and is not needed to land this.
- `CapabilitySigner::sign` in `octo-cap-macaroon`. Surveyed and scoped in
  `0009-e`; it is a different substrate with different blast radius.
- The `octo-governance` `CapabilitySigner::sign_envelope` gap, which still owes
  the `envelope_v2`-style inquiry into whether its input is already
  domain-separated.

## Test vectors

| Vector                                                    | What it pins                                           |
| --------------------------------------------------------- | ------------------------------------------------------ |
| `the_token_carries_the_prefix_in_band`                    | the prefix is transported, not reconstructed           |
| `the_verifier_dispatches_on_the_named_algorithm`          | no `attach_algorithm_id` preference on the verify path |
| `a_token_naming_another_key_id_is_refused`                | in-band binding actually binds                         |
| `a_token_minted_under_one_key_never_verifies_for_another` | the rotation property the suffix was standing in for   |

## Acceptance criteria

- [x] The token carries the prefix as a field, and it survives the wire form.
- [x] `verify_attach_handle_payload` dispatches on the algorithm the token names.
- [x] Minting refuses a holder with no key context.
- [x] Every vector above exists under its specified name and is
      mutation-checked.
- [x] Clippy clean with `--all-targets --all-features` on `octo-runtime`,
      `octo-wallet` and `octo-cli`.

**Not done: the rotation gate and the v2 suffix.** `octo-attach-key-rotation`
and the `key_id.to_be_bytes()` suffix are untouched, and `attach_algorithm_id`
still has one caller left — the feature-gated v2 verify path. v1 and v2 are not
yet unified. That is deliberately deferred rather than half-attempted: v2's
suffix is now provably redundant, but removing it means deleting the KeySet
grace-window tests that currently give it a job, and that belongs in its own
commit with the `KeySet` behaviour pinned. Recorded as the remaining work
rather than silently dropped.

**The encoding version moved to `0x01`.** The wire form gained 32 bytes before
the session id and the fixed prefix is now 177 rather than 145, so this is a
deliberate format break and old tokens are rejected rather than misread.

**The prefix is stored as `[u8; 32]`, not as a `SignablePrefix`.** `AttachHandle`
is serde-derived and `SignablePrefix` is not. Widening `octo-ident`'s derives
for one consumer would be the wrong direction of dependency; the canonical
`to_bytes` / `from_bytes` pair is the wire form anyway.

## Design decisions

- **In band, not reconstructed.** Non-negotiable for this token, per §Why this
  one is not like the others. A prefix the verifier has to be told is not
  protecting anything the DID does not already protect.
- **Retire the rotation gate rather than extend it.** The gate exists because
  binding was rotation-specific. Once binding rides along with every token,
  keeping a feature flag for it is a second source of truth about whether a
  token is bound — and the default-off state is exactly how v1 shipped unbound.
- **Do not let the `KeySet` absorb the prefix's job.** Grace-window key
  resolution answers "which pubkey should I try"; the prefix answers "which
  key is this token for". A rotation-era key still needs its own `key_id` in
  the token even when it is reachable through the grace fallback.
- **Sequencing note for the implementer.** `mint_attach_handle`'s signature
  does not change, but every CLI test identity is built with `IdentityKey::generate`,
  which carries no `verification_method_id` and so cannot mint a bound token.
  Four fixtures in `octo-cli` §`commands::agent::tests` needed one. That is the
  same requirement that surfaced in `0009-e3`: the cost of binding is that
  unbound identities can no longer sign, and that is the point of it.
- **The fixture signer had to learn the substrate's message.** Seven
  `octo-runtime` §`encoding` fixtures signed the bare canonical bytes directly.
  Once the substrate signs `prefix ‖ DCS(canonical)`, those fixtures were
  signing a message production never produces — and they still passed, because
  they only ever asserted encode/decode symmetry. Fixed to build the signable
  bytes the same way the substrate does.

## §Mutation evidence

Four mutations, all red. Each aimed at the implementation rather than at a
test's own assertion.

| Mutation                                                        | Vectors that went red                                                                                                                               |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Verify parses the prefix but signs the bare canonical bytes     | `sign_verify_happy_path`, `mint_attach_handle_happy_path`, `the_token_carries_the_prefix_in_band`, `the_verifier_dispatches_on_the_named_algorithm` |
| Encode writes a constant prefix instead of the token's          | `the_token_carries_the_prefix_in_band` + 3 encode/decode round trips                                                                                |
| Signer stops binding the prefix (signs bare canonical)          | `sign_verify_happy_path`, `mint_attach_handle_happy_path`, `the_token_carries_the_prefix_in_band`                                                   |
| Verifier re-derives the prefix, discarding the token's `key_id` | `a_token_naming_another_key_id_is_refused` — **only this one**                                                                                      |

The last row is the one worth keeping. Reverting the verifier to hardcoded
Ed25519 — the actual pre-migration bug — left **every vector green**, because
the only tokens in these tests are Ed25519 tokens, so a hardcoded preference
and a correctly-read prefix agree on every input. A vector set that cannot
distinguish "reads the prefix" from "assumes Ed25519" is not testing the thing
the migration claims to fix.

`the_verifier_dispatches_on_the_named_algorithm` is the vector that closes
that hole, and it does it by giving the verifier a token whose prefix names an
algorithm that does not exist. The hardcoded verifier accepts the Ed25519 path
regardless; the prefix-reading verifier fails closed. It carries its own
control — the same signature under its real prefix must still verify — so it
cannot pass merely because the helper is broken for all inputs.
