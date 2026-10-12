---
name: 0009-d-stark-schnorr-algorithm
description: RFC-0009. Implement Schnorr over the Cairo STARK curve as a standalone crate registering into AlgorithmRegistry with ZkSupport::None. Promotion to ZkSupport::Circuit needs a circuit change and therefore an RFC-0958 amendment, which is out of scope here.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
---

# Mission `0009-d-stark-schnorr-algorithm` — IMPLEMENTED 2026-10-09

## Status

**Claimed and implemented (2026-10-09).** `octo-algo-stark-schnorr` 46,
`octo-algo-ed25519` 11, `octo-ident` 84, `octo-cli` security guards 45, and
`scarb test` in `cairo/` 111 — all passing. Clippy clean with
`--all-targets --all-features` across all four crates.

All seven §Test Vectors are in place under the specified names. The mission's
framing holds: the algorithm registers with `ZkSupport::None`, and nothing
touched `stage2_main` or the committed CASM hash.

### Three findings worth the read

**1. `bootstrap` could not hold two algorithms, and the second crate proved
it.** `bootstrap` is idempotent only for an _identical_ set — a second call
with different ids is a conflict. So with one algorithm everything worked, and
the moment a second one existed, whichever crate installed second got
`ConflictingBootstrap`. A registry of one algorithm with a deadlock at the
install site.

The fix is `octo_ident::bootstrap_additional`, which _adds_ to a populated
registry and only ever treats an already-registered algorithm as idempotent.
Both per-crate `install()`s now use it. Per RFC-0009 §Dual-format signature
algorithms and the ZK-capable key this was expected — two algorithms on one
identity — but it could not be seen until the second algorithm existed.

The honest limitation is recorded rather than hidden: additive install means a
deployment cannot _restrict_ the set, only extend it. See §What this does not
do: restrict a set in `bootstrap_additional`.

**2. `starknet_crypto::poseidon_hash` is not Cairo's `poseidon_hash_span`.**
This is the trap the crate is most exposed to. Cairo's
`schnorr_challenge(r_x, m)` is `poseidon_hash_span([r_x, m])`, whose Rust
counterpart is `poseidon_hash_many`. The two-_argument_ `poseidon_hash` has
the right arity, the right return type and a plausible name, and it is a
**different hash** — it permutes `[x, y, 2]` where the span form absorbs into
`[0, 0, 0]` and applies two permutations.

Using it would have produced an implementation entirely self-consistent,
passing every positive vector written against itself, and disagreeing with
the circuit on every message. `the_two_argument_poseidon_is_a_different_function`
pins the distinction, and `challenge_matches_the_circuit_vectors` pins the
value against the identical constant asserted in
`cairo/src/stark_schnorr.cairo` §`challenge_parity_vector`.

**3. `ProjectivePoint::x()` is homogeneous; `AffinePoint::x()` is affine.**
The verification equation compares `[s]G - [e]Q` against `R` on the
x-coordinate. In `starknet-types-core`, `AffinePoint::x()` normalises through
`to_affine()` and `ProjectivePoint::x()` does not — it returns `X`, where the
affine x is `X/Z`. Comparing `lhs.x()` against `r` compares against a value
scaled by an unknown `Z`.

It shipped in the first draft of this crate and rejected every honest
signature while still rejecting every forgery, so a positive vector is the
only thing that catches it. `verify_with_challenge` now goes through
`to_affine()`, which also supplies the point-at-infinity refusal for free.

### And one that the tests caught, not the code

`pk_lo`/`pk_hi` split the **low 31 bytes** of the x-coordinate little-endian.
The first draft read `x` as little-endian and so packed `x[0]` at bit
position 0 — but `x` is stored big-endian, where `x[0]` is the _most_
significant byte. Split and join were mutual inverses, so every round-trip
test passed; the values were simply the wrong 31 bytes with the wrong byte in
`pk_hi`.

The vector that catches it is
`stark_schnorr_pk_lo_hi_round_trip`'s sibling
`pk_lo_holds_the_low_thirty_one_bytes_little_endian`, which asserts each
byte's exact position rather than only that the pair round-trips. A symmetric
x-coordinate could not tell the two apart either — the ascending pattern in
that test is what makes an endianness slip visible at all.

### Mutation-verified

Nine mutations were applied and reverted; every one turned the suite red.
A vector nobody has seen fail is a vector nobody knows works.

| Mutation                                          | Caught by                                      |
| ------------------------------------------------- | ---------------------------------------------- |
| `pk_lo` byte order reverted                       | `pk_lo_holds_the_low_thirty_one_bytes…`        |
| Challenge built with two-argument `poseidon_hash` | `challenge_matches_the_circuit_vectors`        |
| Verify compares the homogeneous x                 | `stark_schnorr_verifies_against_reference_…`   |
| Curve order off by one                            | `order_is_the_curve_group_order`               |
| `zk_support` claims `Circuit`                     | `zk_support_is_none_while_no_in_circuit_…`     |
| Ed25519 `install()` declares the whole set        | coexistence test + `the_two_algorithm_crates…` |
| STARK `install()` declares the whole set          | `the_two_algorithm_crates…`                    |
| STARK takes a runtime dep on Ed25519              | `the_two_algorithm_crates…`                    |
| `octo-ident` names an algorithm crate             | `the_two_algorithm_crates…`                    |

### Decisions taken here

- **Wire form is 33 bytes** — 32-byte big-endian x-coordinate plus one parity
  byte. RFC-0009 says "a 32-byte x-coordinate plus a parity bit" without
  fixing a layout, and `VerificationMethod.public_key` is a `Vec<u8>` bounded
  by `public_key_len`, so the crate states it. Bit-packing the parity into the
  spare high nibble would also have been legal and is deliberately not done:
  it is the kind of cleverness that reads as a bug three months later.
- **The challenge absorbs `[r] ++ message_felts`**, with byte messages packed
  31 bytes per felt and length-prefixed. That mirrors the existing convention
  in `octo-cap-macaroon` §`substrate_poseidon` for "bytes into a Poseidon
  absorption" and it is _not_ that crate's caveat-chain encoding — reusing a
  chain encoding here would put a capability-chain value into a signature
  domain. The crate owns this encoding and says so.
- **`pk_hi = (leading_byte << 1) | parity`.** RFC-0009's "with the parity bit
  as its only significant bit" reads as _in addition to_ the byte it carries;
  discarding the byte would make two keys sharing 31 bytes indistinguishable,
  which is the ambiguity the parity bit exists to resolve. The round trip pins
  the reading.
- **`sign` takes a caller-supplied nonce** and refuses to invent one. A
  deterministic nonce needs a keyed derivation this crate would have to make
  up, and an invented one is worse than an explicit freshness obligation.
- **The constants are checked, not remembered.** `curve_order()` is verified
  by `[ORDER]G == O` (and `[ORDER−1]G != O`, which rules out an off-by-one),
  and the generator is asserted against the hex `core::ec::stark_curve` prints.
  Both were measured from the toolchain rather than transcribed.

## Dependencies

`depends_on: 0009-b-algorithm-registry`. Hard dependency — this crate has
nothing to register into otherwise.

## Scope

Implement Schnorr over the Cairo STARK curve as a real crate implementing
`SignatureAlgorithm`, and register it with **`ZkSupport::None`**.

### The registration MUST say `None`, and that is the whole point

`ZkSupport::None` is not a placeholder for work-in-progress. RFC-0009 defines
it as: no constrained in-circuit verifier exists, so a holder whose key is
registered here cannot produce a ZK capability proof, and **a mint attempt
MUST fail closed rather than fall back to another key.** `ZkSupport::Circuit
{ circuit }` is the affirmative claim that a constrained verifier exists.

The circuit does not verify signatures today. `verify_holder_sig` in
`cairo/src/lib.cairo` is a four-felt sum with a non-zero assertion, and its
own doc comment says so: _"This is a structural binding, not RFC 8032
verification."_ The real verifier in `cairo/src/ed25519.cairo` is
`#[cfg(test)]`-gated and unwired; `stark_schnorr.cairo` is likewise
`#[cfg(test)]`-gated and never reached from the entrypoint. `stage2_main`
feeds the same commitment into the holder slot four times.

So registering `ZkSupport::Circuit` here would assert a constrained
in-circuit verifier that does not exist, and RFC-0009 AC#14's fail-closed
mint gate would become fail-open — broken from the other side, by admitting
exactly the key whose in-circuit verification is missing. It would also be
the one algorithm whose signature is length-identical to Ed25519's, so the
mistake would be invisible in any length-based sanity check.

**Promotion to `ZkSupport::Circuit` is a separate, later change** and needs
its own approval: it moves the committed CASM digest, so it invalidates
existing proofs under the migration grace and cannot be smuggled in as part
of registering an algorithm. Record the promotion as pending when this
mission lands, and do not open the test vector below as a pass/fail gate on
`Circuit` — invert it, as stated.

### What already exists, and what it does not

`cairo/src/stark_schnorr.cairo` holds 7 test functions containing **2
positive vectors** (`V1_*`, `V2_*`) against independently computed Python
values, plus **6** negative cases. It is `#[cfg(test)]`-gated and is not
called from the entrypoint.

**It measures the thing this mission exists to make real, and it does not yet
do it.** Two numbers, both from the same measurement session:

| Algorithm                      | CASM words added       | Proving time            |
| ------------------------------ | ---------------------- | ----------------------- |
| Circuit baseline               | 3,004                  | 440.58s                 |
| + conformant Ed25519 verifier  | +41,439 (44,443 total) | not measured end to end |
| + STARK-curve Schnorr verifier | +295 (3,299 total)     | 465.19s (+5.6%)         |

The 5.6% figure is a full prove-and-verify run including tampered-proof
rejection. The ratio is the justification for the whole dual-format design and
it is sound.

What the numbers do **not** say is that either verifier is wired into the
holder-signature slot. The capability circuit's holder slot is a structural
commitment: it Poseidon-hashes four commitments and asserts non-zero, and
`stage2_main` feeds all four the same commitment. It verifies nothing today.

### Out of scope, and deliberately so

Wiring the holder-signature slot is **not** this mission. It changes the CASM
hash, which changes `COMPILED_CASM_BLAKE3_HASH`, which changes
`accepted_casm_blake3_hashes`, which invalidates existing proofs under the
migration grace in RFC-0958 §Data Structures. That is an RFC-0958 amendment,
not a mission task. RFC-0009's §Dual-format section records it as
aspirational; do not quietly close that gap here.

If you find yourself changing `stage2_main`, stop — that is a different
mission and a different approval.

### Key packing

A STARK-curve public key is a 32-byte x-coordinate plus a parity bit. It does
**not** fit the 31-bytes-per-felt convention RFC-0958 applies to _digests_ in
`PublicInputs`. Do not copy that convention — it carries no parity bit and
says nothing about half ordering. The normative packing is in RFC-0009
§Dual-format signature algorithms and the ZK-capable key:

```
pk_lo = felt252 from the low 31 bytes of the x-coordinate, little-endian
pk_hi = felt252 carrying the remaining byte, with the parity bit as its
        only significant bit
```

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors

- `stark_schnorr_verifies_against_reference_vectors` — the existing 7 vectors
  in `stark_schnorr.cairo`, re-run from the new crate
- `stark_schnorr_rejects_wrong_public_key`
- `stark_schnorr_rejects_wrong_message`
- `stark_schnorr_rejects_tampered_signature`
- `stark_schnorr_pk_lo_hi_round_trip` — the packing above, including a key
  whose high byte is non-zero, which is the case a naive split gets wrong
- `stark_schnorr_pk_lo_hi_rejects_inconsistent_halves`
- `zk_support_is_none_while_no_in_circuit_verifier_exists` — registration
  reports `None`, **not** `Circuit { .. }`. This is the vector that keeps
  AC#14's fail-closed gate shut, and it is the direct inverse of what the
  earlier draft of this mission asserted. If it ever has to flip, the flip is
  a circuit change with its own approval, not a registry edit.

Gate: `scarb test` in `cairo/` plus the new crate's suite.

## Environment note

Proving anything that uses `core::ec` requires a large stack — the prover
recurses deeply enough to overrun the default test-thread stack, and it aborts
rather than panicking, so it takes the whole binary down. The `stwo-sys`
proving test now pins this at the call site by spawning a thread with an
explicit stack size, so `cargo test -p stwo-sys` is safe. A new proving entry
point added here MUST do the same rather than relying on `RUST_MIN_STACK` in
the environment.

## Layer direction (per `docs/architecture/design-principles.md`)

The new crate is a user extension: it registers into the Layer B registry and
depends on nothing in it. If this crate needs to reach into `octo-ident`
internals to register, the registry's interface is wrong — fix that rather than
working around it.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-algo-stark-schnorr -p octo-algo-ed25519 -p octo-ident -p octo-cli \
  --all-targets --all-features -- -D warnings
scarb test          # in cairo/
cargo test -p octo-algo-stark-schnorr
cargo test -p octo-cli --test security_regressions
```

`octo-algo-ed25519` and `octo-ident` are in the list because this mission
changed both: the Ed25519 `install()` path, and the registry's install
interface it moved to.

## Backward compat

Additive. A holder with no STARK-curve key is unaffected, and a mint attempt
for a ZK capability with only an Ed25519 key MUST fail closed rather than fall
back — AC#14.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — §Dual-format
  signature algorithms and the ZK-capable key, G8, AC#14
- `rfcs/accepted/proof-systems/0958-zk-capability-subclass.md` — §Algorithms,
  the circuit wiring that this mission deliberately does not touch
- `rfcs/draft/economics/0974-zk-capability-platform-extension-surface.md` —
  §Step 1, where the full measurement record lives
- `cairo/src/stark_schnorr.cairo` — the existing verifier, and now the other
  end of the host/circuit challenge parity vector
- `crates/octo-algo-stark-schnorr` — the crate this mission produced
- `missions/claimed/0009-b-algorithm-registry`

## Claimant

@mmacedoeu

## Pull Request

#
