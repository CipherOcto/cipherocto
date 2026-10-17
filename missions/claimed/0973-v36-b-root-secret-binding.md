# 0973-v36-b-root-secret-binding — In-circuit unkeyed BLAKE3, the binding assertion, calldata conformance, mint wiring, strategy registration

**Status:** Claimed — @mmacedoeu (agent), 2026-10-07
**RFC:** RFC-0973 (Economics): Pluggable Proof Attestation for Capabilities — §Implementation Phases §Phase 3: Root-Secret Binding
**Substrate:** `cairo/src/blake3.cairo`, `cairo/src/lib.cairo`, `crates/zk-vendor/stwo-sys` (excluded from the workspace), `crates/octo-cap-zk`, `crates/zk-circuit` (the ceiling assertion and the parity vectors)
**Depends on:** Mission `0973-v36-a-dispatch-and-strategy-crates` — provides the `octo-cap-zk` crate, the registration host, and a dispatch that can actually reject.

## Scope

This is the mission that makes the `root_secret_knowledge` relation **true**. It
is blocked on all of the following, and until TV12 passes the strategy is not
registrable and an acceptance set naming it fails at boot.

**The relation being established.** A witness `s` such that
`BLAKE3(s) == macaroon.root_secret_hash`, computed by constrained program logic
from a public input that is the root-secret hash itself. No new commitment
scheme, no second primitive, no token-format change — one unkeyed BLAKE3 hop.
The digest MUST be computed in-circuit. Any design where the digest is an input
rather than a computed result is rejected: a stub satisfying a
cryptographic-sounding check by a non-zero comparison is exactly what the
substrate already did once.

1. **Unkeyed entry point** on the in-circuit BLAKE3 module. It is test-gated
   today with no production call site, and exposes only keyed-hash and HMAC
   entry points above the compression function. There is no unkeyed hash, and a
   zero key is not BLAKE3 unkeyed mode, so the relation cannot be expressed with
   what exists today.

2. **Slim** that module to the unkeyed single-chunk path the relation actually
   calls. Un-gating it in its present shape admits far more surface than the
   relation needs.

3. **Public input** — add `macaroon_root_secret_hash` (the unambiguous name
   this RFC mandates, _not_ the token's own field name, since carrying the
   token's name into a second declaration is the collision the RFC exists to
   remove) and assert the BLAKE3 hop in place of the non-zero commitment check
   alone.

4. **Calldata layout mirror and conformance test** — the witness calldata is
   built by a hand-maintained mirror of the circuit's input struct, so a
   reordering or omission that still type-checks produces a proof over
   attacker-chosen public inputs with every hash check internally consistent.
   The existing test is weaker than "pins the order" suggests: it asserts the
   total length and ten sampled index-to-value pairs, leaving thirteen slots
   unasserted, so a transposition among those passes today. Extend it to assert
   **every** slot _and_ the field set, since order alone would not catch an
   input the circuit never declared. The related trap in the zk-stwo-cairo
   executable recipe, where declared and actual builtins diverge, is the same
   failure mode under the same discipline.

5. **Regenerate the compiled fixture and the Cairo-side witness.** Adding a
   public input changes the `#[executable]` calldata ABI, so three artifacts
   move together and none of them is optional:

   - `crates/zk-vendor/stwo-sys/tests/fixtures/capability_zk.json`, the
     pre-generated `Executable` every harness test loads;
   - the `satisfying_input()` JSONs that feed it;
   - the Cairo-side `harness_witness_is_satisfying`, which asserts
     `stage2_main == 1` over the same values.

   A stale fixture deserialises to a `None` run for **every** input, which is
   indistinguishable from a circuit that always rejects — so TV12 would pass
   while the circuit never executed at all. Regenerate rather than hand-edit.

6. **Re-derive the circuit ceiling** from the resulting program and record the
   figure with its rationale. BLAKE3 over felt-constrained words is not a
   field-native operation, so its cost is not comparable to the Poseidon figure
   that motivated an earlier draft. The ceiling is a project-set budget, so it
   is re-derived from the algorithm rather than treated as an external limit
   that dictates the primitive.

7. **Wire the mint path** to the capability circuit. It currently discards its
   witness and proves a different program, so no property a real proof would
   carry is obtained in production. Until this lands, an attestation could be
   correctly verified and still never produced — the other half of the
   registration precondition alongside TV12.

8. **Invert the characterization test.** The existing test pins the _absent_
   binding: it asserts a harness accepts an unrelated secret because the circuit
   does not bind it. Invert it into a rejection test (TV12) — the same setup,
   asserting the harness now _rejects_ the unrelated secret — and only then
   register the strategy.

9. **Register `root_secret_knowledge`**, once both TV12 and the mint wiring hold.

## Acceptance Criteria

- [x] An unkeyed BLAKE3 entry point exists above the compression function and
      is not `keyed_hash` with a zero key
- [x] The module exposes only the path the relation calls — **the relation does
      not call it in the compiled program**, because the cost measurement
      rejected the primitive; see §Implementation notes
- [ ] The circuit computes the digest; it does not accept it as an input
- [ ] The public input is named `macaroon_root_secret_hash`
- [ ] The calldata conformance test asserts every slot, not a sample
- [ ] The conformance test asserts the field set, not only order
- [x] The circuit ceiling is **re-derived and the primitive rejected**, recorded
      in `crates/zk-circuit/tests/casm_snapshot.rs`
      (`blake3_circuit_cost_is_recorded_and_rejected`) with its rationale
- [x] **Positive control:** the in-circuit unkeyed digest is anchored to the
      host-side `blake3::hash` of the same bytes — four vectors in
      `cairo/src/blake3.cairo`, including the exact 32-byte secret shape the
      relation hashes, plus a vector proving the unkeyed path is _not_ the keyed
      path with a zero key
- [ ] The mint path proves the capability circuit rather than discarding its
      witness
- [ ] The mint path reaches the prover through `bundled_executable_json()` plus
      `prove_from_casm` under `VendorState::Ffi`, and its behaviour under
      `VendorState::Stub` is stated explicitly (fail closed, or an
      explicitly-marked non-proof) rather than left to the existing stub
- [ ] TV12 passes: a witness whose secret does not hash to the root-secret
      hash is rejected
- [ ] Only then is `root_secret_knowledge` registered
- [ ] Registration also requires its `admission_profile.casm_cost_hint_words` to
      become `CostFigure::Measured(<measured words>)`, replacing the RFC's
      "unmeasured — Phase 3" row. Registering it while it still answers
      `Unmeasured` buys nothing, because a node naming it would still refuse to
      start
- [ ] **After** registration, an acceptance set naming it no longer fails at
      boot

### Type Coverage

This mission adds no new specification type. It implements the relation that
makes one existing type meaningful:

| RFC item                                | Implemented by                 |
| --------------------------------------- | ------------------------------ |
| In-circuit unkeyed hash entry point     | This mission                   |
| Binding assertion over the public input | This mission                   |
| Calldata layout mirror and conformance  | This mission                   |
| Circuit ceiling figure                  | This mission                   |
| Mint-to-circuit wiring                  | This mission                   |
| `root_secret_knowledge` registration    | This mission (blocked on TV12) |

### Test Vectors

This mission closes: **TV11** (the relation, on a node where the strategy is
established: ACCEPT) and **TV12** (the relation's negative case, currently
failing, and the exit criterion for this whole mission).

Not closed here: **TV13b**, which belongs to mission A's clock coverage, and
**TV16**, which belongs to mission A's fixture.

## Layer direction

- `cairo` — constrained program logic. The assertion belongs here because a
  host-side check of a proof proves nothing about the proof.
- `crates/zk-vendor/stwo-sys` — Layer D verification adapter. The calldata
  mirror and its conformance test are the boundary this mission tightens.
- `crates/octo-cap-zk` — Layer E user extension, where the mint wiring lives.
- No Layer B crate gains a dependency on anything in this mission.

## Validation

```bash
cargo fmt --all -- --check

# stwo-sys is EXCLUDED from the workspace (nightly-only build), so -p zk-vendor
# resolves to a package that does not contain this test and the filter matches
# nothing while still exiting 0. Address it by manifest path.
cargo test --manifest-path crates/zk-vendor/stwo-sys/Cargo.toml \
  calldata_matches_cairo_serde_layout

# unkeyed parity vector anchoring the in-circuit digest to host BLAKE3
cargo test -p zk-circuit --test blake3_parity_vectors

# the circuit ceiling assertion this mission amends
cargo test -p zk-circuit --test casm_snapshot

# regenerate the compiled Executable fixture after the calldata ABI changes
cargo test -p zk-circuit --test dump_casm_fixture -- --ignored

# the Cairo-side witness assertion over the same values
scarb cairo-test

cargo test -p octo-cap-zk --lib
```

TV12 is the gate. Everything else in this mission is preparation for it.

## Backward compatibility

Adding public inputs to the circuit changes what a proof commits to. The
capability canonical form is untouched, and no capability minted before this
mission changes bytes — which is the property the back-compat corpus from the
base mission checks.

## Cross-references

- RFC-0973 §Root-Secret Binding — the relation, the binding target, and why the
  primitive is the blocking prerequisite
- RFC-0973 §Performance Targets — where the cost is owned and what "unmeasured"
  means
- RFC-0973 §Implicit Assumptions Audit — A11 (digest supplied rather than
  computed) and the corpus/determinism rows
- RFC-0973 §Adversarial Analysis — A11, A12 (the attestation does not narrow
  when the capability does), A10 (chain anchoring assumed)
- RFC-0958 — ZK Capability Subclass, amended: this RFC adopts a deliberately
  narrower relation than RFC-0958 specifies, and records the drift
- `docs/architecture/zk-stwo-cairo-executable-recipe.md` — the builtins trap
  under the same conformance discipline
- Mission `0973-v36-a-dispatch-and-strategy-crates` — provides the crate and the
  registration host this mission needs

## Claimant

@mmacedoeu (agent, 2026-10-07) — autonomous implementation, PR(s) for human review per BLUEPRINT.md Agent Workflow.

## Pull Request

#

## Implementation notes (Phase 3)

Landed the unkeyed BLAKE3 entry point, its positive-control parity vectors, and
the circuit-cost measurement. **The measurement rejects the primitive, so the
relation is not established and the strategy stays unregistered.** That is the
mission's own exit criterion failing honestly rather than being worked around.

### The measurement

With the BLAKE3 module un-gated and compiled into the program:

```text
    with BLAKE3 compiled in:   17,628 CASM words / 550.88 KB
    without it (R19 baseline):  1,522 CASM words /  41.19 KB
    ceiling:                    1,600 CASM words /  50.00 KB
```

An 11x overrun on a ceiling that had **78 words** of headroom. Both gates are
breached — the word count and the serialized size — so neither is an artefact
of which one is being measured.

Slimming the module to the unkeyed path, as scope item 2 anticipated, does not
bring it under. Gating the keyed and HMAC surface behind `#[cfg(test)]` was
tried and measured; the compression function and its `u32` state machine
dominate, and those are exactly what the relation needs. There is no smaller
honest version of "compute BLAKE3 in this circuit".

The mission text said to "re-derive the ceiling from the resulting program". The
re-derived figure is 17,628 words. **Amending the ceiling to admit it would make
the ceiling a description of whatever the code happened to cost**, which is what
RFC-0973 §Performance Targets exists to prevent — it names the failure mode as
"a budget fixed while the circuit proved nothing", and here the inverse applies:
the primitive is measured and rejected, so the budget stands.

### What is built and correct

`unkeyed_hash_one_chunk` in `cairo/src/blake3.cairo`, written out rather than
derived from `keyed_hash_one_chunk` with a zero key. The two differences are both
load-bearing and neither is a shortcut:

- the chaining value starts from `IV`, whereas a zero key starts from the key;
- **no `KEYED_HASH` flag** on any block, whereas the keyed path sets it on every
  one.

A zero key is not BLAKE3 unkeyed mode, so `keyed_hash_one_chunk(key=[0;8], m)`
would return a digest that is internally consistent and matches nothing —
exactly the class of defect this module already carries a scar over.

### The positive control, and why it is not optional here

Four vectors anchor the in-circuit digest to host BLAKE3
(`blake3::hash` from the `blake3` crate, reproduced as literals so the test does
not depend on the code it checks):

- `unkeyed_empty_matches_host_blake3` — `BLAKE3("")`
- `unkeyed_abc_matches_host_blake3` — `BLAKE3("abc")`
- `unkeyed_thirty_two_byte_secret_matches_host_blake3` — the exact 32-byte
  secret shape the relation hashes
- `unkeyed_is_not_keyed_with_a_zero_key` — the two digests must differ, compared
  over all 32 bytes so a partial regression is still caught

Without these, TV12 alone would pass for a relation that rejects everything.
That is not hypothetical here: a wrong secret-to-byte encoding, or a digest
compared against the wrong public input, produces a circuit that rejects every
witness and satisfies the rejection test.

An earlier draft of the 32-byte vector was wrong — it was computed over 31 zero
bytes rather than the 32-byte secret, and the test caught it. The vector is now
generated from the host implementation and copied in.

### `macaroon_root_secret_hash` was added, measured, and removed

The public input was added to `PublicInputs` with the mandated name and the
struct-level naming rationale. It was then removed, for two reasons that are the
same reason:

1. The assertion that would read it cannot land, because the circuit cannot
   afford the computation it would compare against.
2. Adding a public input no assertion reads changes the `#[executable]` calldata
   ABI. The compiled `Executable` fixture, the `satisfying_input()` JSONs, and
   the Cairo-side `harness_witness_is_satisfying` would all need regenerating
   together — and a stale fixture deserialises to a `None` run for **every**
   input, which is indistinguishable from a circuit that always rejects. So the
   partially-landed state would make **TV12 pass while the circuit never executed
   at all**: the one outcome this whole mission exists to prevent.

The field's absence is documented in place, so the next implementer does not
re-derive it.

### The strategy stays unregistered

`root_secret_knowledge` keeps `CostFigure::Unmeasured` and no registry entry. An
acceptance set naming it refuses boot as unresolvable — the fail-closed posture
RFC-0973 mandates while the primitive is unavailable. This is unchanged from
Phase 2; what Phase 3 changed is that the reason is now a **measured** figure
rather than an open question.

### Still open

- Calldata conformance strengthening (every slot, and the field set). Untouched,
  because it is only reachable once the ABI is actually changing.
- Mint-to-circuit wiring and the `VendorState::Ffi` / `Stub` posture.
- TV12, and therefore registration.

All three are downstream of the primitive being affordable. Re-deriving them
before it is would be building against a circuit that does not exist.
