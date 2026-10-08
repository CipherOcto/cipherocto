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

- [ ] An unkeyed BLAKE3 entry point exists above the compression function and
      is not `keyed_hash` with a zero key
- [ ] The module exposes only the path the relation calls
- [ ] The circuit computes the digest; it does not accept it as an input
- [ ] The public input is named `macaroon_root_secret_hash`
- [ ] The calldata conformance test asserts every slot, not a sample
- [ ] The conformance test asserts the field set, not only order
- [ ] The circuit ceiling is re-derived and amended in
      `crates/zk-circuit/tests/casm_snapshot.rs` (`casm_words_under_1600_after_stage2`),
      as an explicit recorded decision with its rationale, not absorbed by
      trimming elsewhere
- [ ] **Positive control:** a witness whose secret _does_ hash to the declared
      `macaroon_root_secret_hash` is **accepted**, and the in-circuit unkeyed
      digest is anchored to the host-side `blake3::hash` of the same bytes
      (extend `crates/zk-circuit/tests/blake3_parity_vectors.rs` with an unkeyed
      vector). Without this, a relation that rejects everything — a wrong
      secret-to-byte encoding, or a digest compared against the wrong public
      input — passes the rejection test, and so would a self-consistent but
      wrong permutation
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
