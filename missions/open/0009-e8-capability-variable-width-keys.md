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

**Scoping corrected.** This mission was originally scoped as carrying a
storage-migration risk. That risk was measured and does not exist: the widened
field serializes byte-identically to the current one, so there is nothing to
migrate. See §Why it is decomposed rather than done. The blocker itself — the
widths — is unchanged and still real; what changed is how expensive it is to
remove.

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
| `HolderRecord.holder_pub`            | `[u8; 32]`, via `serde_bytes_32`             | the registry record pins the width                       |
| ~~`KeySet.keys` / `grace_keys`~~     | ~~`[u8; 32]`~~                               | **out of scope — see below**                             |

### The `KeySet` row was wrong, and is struck

The original table listed `octo-runtime`'s `KeySet` because it also carries a
`[u8; 32]`. It is not part of the capability path, and widening it is not
required for a non-Ed25519 capability holder:

- `octo-cap-macaroon` has no dependency edge on `octo-runtime` at all, so
  nothing in the capability verification path can reach it.
- `KeySet` lives entirely under `octo-runtime/src/handle/` — the **attach
  handle** subsystem owned by `0009-e4` and `0009-e7`, with its own wire
  format and its own `ENCODING_VERSION`.

Including it would have pulled this mission into a second subsystem and
doubled the blast radius for no gain in the stated goal. It stays fixed-width
here, and widening attach handles is its own mission if it is ever wanted.

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

1. **Half-measures achieve nothing.** Widening the trait and `HolderSignature`
   while `holder_pub` stays 32 bytes leaves a non-Ed25519 signer that can sign
   but cannot be registered — a new state that is strictly worse than the
   current one, because it looks supported. The change is all-or-nothing.
2. **Layer direction.** `octo-cap-macaroon` must not gain an edge on
   `octo-algo-stark-schnorr` just to name a width. The width should come from
   the `SignatureAlgorithm` trait at the point of use, which is what
   `verify_dispatch` already does on the verify side.
3. **Blast radius.** 106 sites across 18 files in 8 crates, so a partial
   landing leaves the tree uncompilable rather than half-done.

### The third original reason was wrong, and was removed

An earlier revision of this section led with the registry schema:

> `HolderRecord` is persisted and gossiped. Widening its key column needs a
> migration and a decision about what reads pre-migration rows.

**That is false, and it was the reason this mission was treated as risky.** It
was measured rather than assumed: `serde_bytes::Bytes` and `Vec<u8>` both
serialize through `serde_json` as a JSON array of integers, so
`holder_pub: [u8; 32]` under `serde_bytes_32` and `holder_pub: Vec<u8>` produce
byte-identical gossip for every key that exists today. Existing rows
deserialize into the widened field unchanged, so there is no migration and
nothing to decide about pre-migration reads.

The probe that established this was run against the real serde stack, not
derived from documentation, and is why the claim is retracted rather than
merely softened.

What genuinely remains for `e8b`, having removed the false blocker:

- **Field declaration order must not change.** The determinism contract at
  §G5 cross-node mint verifiability is `serde_json` in declaration order, so
  widening a field's type is safe but moving one is a breaking change.
- **The `Debug` redaction string** hardcodes `<redacted 32 bytes>` and a test
  asserts no key bytes leak. It must stop claiming a width it no longer knows.
- **Constructor signatures** (`from_bearer`, `from_capability`,
  `from_hop_capability`) take `&[u8; 32]` and must take a slice.

None of these is a migration. `e8b` is mechanical, and this mission is no
longer the risky one it was scoped to be.

## §Proposed decomposition

- **`0009-e8a` — `HolderSignature` becomes variable width**, with the length
  checked against `algorithm.signature_len()` rather than a literal 64, and the
  wire/serde segments length-delimited rather than "16 bytes then the rest".
  Mechanical, no schema.
- **`0009-e8b` — `holder_pub` becomes variable width** across
  `CapabilityToken` and `HolderRecord`. There is **no storage migration** — see
  §Why it is decomposed rather than done for the measurement that retracted
  that claim. What remains is the gossip determinism contract (declaration
  order), the `Debug` redaction string, and the constructor signatures.
- **`0009-e8c` — `CapabilitySigner` returns `Vec<u8>`**. This is the source of
  the wide values, so logically it comes first: without it `e8a` and `e8b`
  have nothing to carry. In practice all three land together because each one
  alone leaves the tree uncompilable.

Order is forced by reason 1: the change is all-or-nothing.

## §Implementation order

Bottom-up, so each step compiles before the next begins. Widening a type
breaks every caller at once, so the order is "widen the leaf, then fix
callers outward", never "widen the leaf and hope".

1. **`HolderSignature`** — `bytes: Vec<u8>`, with `new` validating against
   `shared_registry()?.get(algorithm_id)?.signature_len()` rather than a
   literal 64. `shared_registry()` is public on `octo-algo-ed25519`, which
   this crate already depends on, so this needs no new edge.
2. **`CapabilityToken.holder_pub`** and the wire segments that carry it.
3. **`HolderRecord.holder_pub`** — drop `serde_bytes_32` on this field only,
   keep it on `cap_root_hash` and `ask_id`, which are genuinely fixed-width
   hashes. Update the three constructors to take a slice and the `Debug`
   redaction string, which currently claims a width it will no longer know.
4. **`CapabilitySigner`** — `sign` and `public_key_bytes` return `Vec<u8>`,
   plus the two blanket impls.
5. **Callers**, crate by crate: `octo-wallet` (5 files), then
   `octo-cap-macaroon`'s own discharge and CLI surfaces, then the four
   remaining crates.

### The vector this must add

The retraction in §Why it is decomposed rather than done removed the migration
concern, but it introduced a new obligation: **nothing now pins the gossip
bytes**. The old `[u8; 32]` type enforced the width structurally, and the
`serde_bytes_32` helper pinned the representation. Removing both removes the
only things standing between a refactor and a silent wire change.

So this mission must land a vector asserting that a 32-byte key serializes to
exactly the bytes it serialized to before the widening — the same JSON array of
integers — and that a non-32-byte key round-trips. Without it, the claim that
no migration is needed is a claim about the past that nothing holds in place.

The existing `serde_json_round_trip` test is not sufficient: it round-trips
whatever the current type produces, so it would pass identically before and
after, and it would pass just as happily if the representation changed. It
needs to compare against pinned bytes, not against itself.

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
