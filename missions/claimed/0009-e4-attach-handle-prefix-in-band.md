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

# Mission `0009-e4-attach-handle-prefix-in-band` — CLAIMED 2026-10-10

## Status

Claimed (2026-10-10). The design is settled and recorded here because the
first reading of it was wrong, and the correction is the whole content of this
mission.

## Dependencies

`0009-e` for `SignablePrefix` / `SignedMessage` and the `SignableForm::Raw`
shape. No dependency on `0009-e2` or `0009-e3`.

## §Why this one is not like the others

Every other site in the `0009-e` outstanding table is signed by a party that
also holds the key context, so the verifier reconstructs the prefix from what
it already has. The wallet-node oracle works exactly that way, and its
`SignRequest` stays a bare `[u8; 32]`.

**An attach-handle token travels.** The verifier arrives holding the token
bytes and the holder DID, and nothing else. It can recover the public key from
the DID, but it has no `key_id` — and RFC-0009 forbids deriving a `key_id`
from the public key precisely so a key can be rotated independently. So a
reconstructed prefix is not weaker here, it is _unavailable_.

This is why v2's hand-rolled `key_id.to_be_bytes()` suffix is **not** the
redundant parallel abstraction it first appears to be. Putting the `key_id` in
band is correct for a transported token, and it is doing a job the prefix is
not doing in that design. An earlier note in `0009-e` claimed otherwise; that
claim is corrected in `0009-e` §The two `octo-runtime` attach-handle paths.

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

- [ ] The token's canonical bytes contain a serialised `SignablePrefix`.
- [ ] `verify_attach_handle_payload` no longer hardcodes an algorithm.
- [ ] `attach_algorithm_id` has no remaining caller on the verify path.
- [ ] v1 and v2 share one binding mechanism; the rotation gate is retired or
      reduced to the `KeySet` concern it actually governs.
- [ ] Every vector above exists under its specified name and is
      mutation-checked.
- [ ] Clippy clean with `--all-targets --all-features` on `octo-runtime` and
      `octo-cli`.

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
  does not change, but every CLI test identity is built with `from_seed`, which
  carries no `verification_method_id` and so cannot mint a bound token. Those
  fixtures must attach a `key_id` — the same requirement that surfaced in
  `0009-e3`, and for the same reason: visible at the call site rather than
  assumed.
