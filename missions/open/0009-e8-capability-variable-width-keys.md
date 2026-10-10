---
name: 0009-e8-capability-variable-width-keys
description: Widen the capability substrate off fixed-width Ed25519 key and signature arrays so a non-Ed25519 holder key is representable. Decomposition and blast radius, not a single-commit change.
metadata:
  node_type: mission
  type: research
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-e5-capability-signer-algorithm-binding
---

# Mission `0009-e8-capability-variable-width-keys` — SCOPED, NOT IMPLEMENTED

## Status

**Scoped, deliberately not implemented in one commit.** This is the last
RFC-0009 gap on the capability path: `CapabilitySigner` cannot represent a
non-Ed25519 holder key, so multi-algorithm capability holders are impossible
regardless of how much domain separation is layered on top.

The blocker is width, not policy. Measured, not estimated.

## §What is actually blocked

`octo_algo_ed25519::verify_dispatch` already takes `&[u8]` for both key and
signature and dispatches through the shared `AlgorithmRegistry`. **The
verification substrate is already algorithm-agnostic.** The widths are all in
the capability layer above it:

| Site                                 | Width                                        | Why it blocks                                            |
| ------------------------------------ | -------------------------------------------- | -------------------------------------------------------- |
| `CapabilitySigner::sign`             | `-> [u8; 64]`                                | a 33-byte-key algorithm's signature may differ in length |
| `CapabilitySigner::public_key_bytes` | `-> [u8; 32]`                                | a 33-byte key cannot be returned at all                  |
| `HolderSignature.bytes`              | `[u8; 64]` + a 64-byte length check in `new` | same                                                     |
| `CapabilityToken.holder_pub`         | `[u8; 32]`                                   | the token cannot carry a 33-byte key                     |
| `HolderRecord.holder_pub`            | `[u8; 32]`, via `serde_bytes_32`             | the **registry schema** pins the width                   |
| `KeySet.keys` / `grace_keys`         | `[u8; 32]`                                   | same, in `octo-runtime`                                  |

## §Measured blast radius

`holder_pub` appears at **106 sites across 18 files in 8 crates**; 98 of those
are in `src` rather than tests:

`octo-wallet` (5 files), `octo-cap-macaroon` (5), `octo-runtime` (3),
`quota-router-sm-engine`, `octo-wallet-node`, `octo-protocol`,
`octo-ident-storage`, `octo-cli`.

`HolderSignature` appears at 23 sites. The `HolderRecord` width also carries a
`serde_bytes_32` helper and is used by `octo-ident-storage`'s holder registry,
so this is a **storage schema change**, not a type alias swap.

## §Why it is decomposed rather than done

Three reasons, in order of weight:

1. **The registry schema.** `HolderRecord` is persisted and gossiped. Widening
   its key column needs a migration and a decision about what reads pre-migration
   rows. That is a different piece of work from a Rust type change and should not
   be entangled with one.
2. **Half-measures achieve nothing.** Widening the trait and `HolderSignature`
   while `holder_pub` stays 32 bytes leaves a non-Ed25519 signer that can sign
   but cannot be registered — a new state that is strictly worse than the
   current one, because it looks supported. The change is all-or-nothing.
3. **Layer direction.** `octo-cap-macaroon` must not gain an edge on
   `octo-algo-stark-schnorr` just to name a width. The width should come from
   the `SignatureAlgorithm` trait at the point of use, which is what
   `verify_dispatch` already does on the verify side.

## §Proposed decomposition

- **`0009-e8a` — `HolderSignature` becomes variable width**, with the length
  checked against `algorithm.signature_len()` rather than a literal 64, and the
  wire/serde segments length-delimited rather than "16 bytes then the rest".
  Mechanical, no schema.
- **`0009-e8b` — `holder_pub` becomes variable width** across
  `CapabilityToken` and `HolderRecord`, including the storage migration and the
  gossip determinism contract. The risky one; carries its own review.
- **`0009-e8c` — `CapabilitySigner` returns `Vec<u8>`**, once `e8b` has made a
  non-Ed25519 holder representable. Without this the first two are inert.

Order is forced by reason 2: `e8a` and `e8b` land together or neither does.

## §What must not be done

- **Do not derive the key width from the algorithm at the trait boundary.**
  `public_key_bytes` returning "whatever this algorithm wants" pushes the
  registry question into every caller. The honest home is the `KeySet` /
  `HolderRecord` layer, which is what `e8b` changes.
- **Do not add an `octo-algo-stark-schnorr` dev-dependency to prove it works
  here.** The cross-algorithm replay vectors live in the algorithm crate
  (`octo-algo-stark-schnorr`'s own test) per the Layer E rule. This mission's
  vectors should use a local fixture reporting an arbitrary `AlgorithmId`, the
  same narrowing `0009-e5` used and recorded.
- **Do not relax `HolderSignature::new`'s length check to "any length".** An
  unchecked length is how a truncated signature becomes a "bad" signature rather
  than a malformed one. Validate against the algorithm.

## §Verification for whoever implements this

The discriminating property is the one that has bitten three times in this chain
and is worth naming: **a vector set that only ever exercises Ed25519 cannot
distinguish "reads the algorithm" from "assumes Ed25519."** Reverting every
algorithm-dependent line back to a hardcoded Ed25519 must leave the new tests
_green_, and that is the signal the set is not testing anything. `0009-e5` hit
this and needed a second vector to close the gap.
