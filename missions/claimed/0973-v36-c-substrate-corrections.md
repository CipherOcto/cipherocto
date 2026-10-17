# 0973-v36-c-substrate-corrections — Capability class projection and the recorded specification drift

**Status:** Claimed — @mmacedoeu (agent), 2026-10-07
**RFC:** RFC-0973 (Economics): Pluggable Proof Attestation for Capabilities — §Implementation Phases §Phase 4: Cross-cutting substrate corrections
**Substrate:** `octo-wallet` (capability class), the RFC document itself (drift record)
**Depends on:** Mission `0973-v36-b-root-secret-binding` — this phase is cross-cutting and lands after the mechanism is whole.

## Scope

Two corrections the RFC's own review surfaced, neither introduced by it. This is
the thinnest mission in the set and it is honestly thin: one item is a decision
about a pre-existing gap, and one is a record plus an explicit deferral. Both
are claimable units, and leaving them unowned is how they survived a dozen
review rounds.

1. **Capability class is a projection, not a wire field.** `NodeType` is
   retained in the wallet as the mint-time ZK gate, and the class registry that
   enforces it is a production type with no production call site — only its
   tests instantiate it. The role model this RFC links to treats the class as a
   computed projection rather than stored state.

   **Note the interaction with mission D.** `NodeType` is declared in
   `octo-cap-zk`, not in the macaroon crate, so mission D's removal of the
   wallet→strategy dependency cannot re-point the wallet's `to_zk_node_type`
   projection — it has nothing to point at. D owns deciding that projection's
   disposition. If D deletes it, the sentence above is no longer true and the
   mint-time gate needs a home; do not treat "retained in the wallet" as a
   settled fact this mission may rely on.

   Decide and record: either promote the registry to a real call site, or make
   the projection explicit so nothing reads it as authoritative. The gap is
   pre-existing and is not this RFC's to fix, but it is a place where the
   attestation mechanism lands next to something misleading, so closing or
   documenting it belongs with the mechanism rather than after it.

2. **The `cap_root_hash` drift record.** The RFC records, as owned by this
   phase, that the circuit declares `cap_root_hash_lo`/`_hi` but never derives
   them and asserts only that a Poseidon commitment over the witness is
   non-zero — so nothing connects the declared input to the secret — while
   RFC-0958 specifies both the derivation and the comparison.

   Decide what the substrate should do about it. This RFC does not settle it:
   it adopts the narrower root-secret binding and leaves the declared inputs
   untouched, which is a deliberate narrowing of RFC-0958's relation rather
   than an oversight.

3. **The rename is explicitly out of scope.** `cap_root_hash` names two
   unrelated values across the substrate. Renaming is a wire-affecting change
   and is not attempted here. Record that decision so a later reader does not
   read the absence as an oversight.

## Acceptance Criteria

- [x] The capability-class gap is decided and the decision recorded in the
      wallet's documentation
- [x] The class registry either gains a production call site or is documented
      as a projection — **documented**, deliberately not wired
- [x] The `cap_root_hash` drift has an owner (RFC-0958) and a recorded decision
- [x] The rename is explicitly recorded as out of scope and why
- [x] Nothing in this mission changes the wire format

### Type Coverage

This mission adds no specification type. It corrects substrate facts the
attestation mechanism depends on reading correctly:

| RFC item                                  | Implemented by                        |
| ----------------------------------------- | ------------------------------------- |
| Capability class projection decision      | This mission                          |
| `cap_root_hash` drift record and decision | This mission                          |
| `cap_root_hash` rename                    | **Deferred, explicitly out of scope** |

### Test Vectors

This mission closes none of the RFC's numbered vectors directly. The property it
protects is structural: a reader of the wallet should not be misled by a type
that looks authoritative and is not, and a reader of the RFC should find the
drift where it says it is recorded.

## Layer direction

- `octo-wallet` — Layer B years-stable substrate. This is a correction to an
  existing Layer B type's status, not a new capability.
- No strategy crate, no circuit, no wire change.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings
cargo test -p octo-wallet --lib
```

The check that matters is documentary: the decision exists in the wallet and in
the RFC, and a reviewer reading either can find the other.

## Backward compatibility

Nothing here may change wire format, capability bytes, or registry keys. If a
proposed fix for either item would, it is out of scope for this mission.

## Cross-references

- RFC-0973 §Data Structures — the name-collision table and the drift record
  this phase owns
- RFC-0973 §Root-Secret Binding — why the root-secret hash is the binding
  target and the declared inputs are not
- RFC-0973 §Adversarial Analysis — A3 (id collision) and A3b (unallocated id),
  which are about the _attestation_ id rather than the capability-class gap
- RFC-0958 — ZK Capability Subclass, the drift source
- Mission `0973-v36-b-root-secret-binding` — the mechanism these corrections sit
  beside

## Claimant

@mmacedoeu (agent, 2026-10-07) — autonomous implementation, PR(s) for human review per BLUEPRINT.md Agent Workflow.

## Pull Request

#

## Implementation notes (Phase 4)

Three decisions recorded, no code paths added. Both substrate claims were
verified against the code before being written down rather than taken from the
mission text.

### The class registry is documented, not wired

Verified: every instantiation of `CapabilityClassRegistry` is in a test —
`registry.rs`'s own `mod tests` and `tests/zk_vectors.rs`. No production code
constructs one, so the `WholesaleCannotRegisterZK` rule is not enforced at
runtime. The Wholesale fail-closed property rests on the `mint_with_zk` guard
(which does run) and the CI lint.

Wiring it was rejected for two reasons, and the second is the real one:

1. The gap is **pre-existing**. RFC-0973 does not introduce it, and adding a
   production call site would be an unrelated behavioural change to a
   years-stable Layer B type, in a phase whose subject is attestation.

2. Wiring it needs a decision nobody has made. A registry keyed by `node_did` is
   **stored** state: someone must own it, decide its lifetime, and decide who
   may write to it. RFC-0973's own role model treats the capability class as a
   **computed projection**. Until that ownership question is settled, a call
   site means inventing an authority and storing derived data in it.

The documentation now says plainly that the rule is a validated invariant
production does not exercise, so a future reader does not mistake a validated
type for an enforced gate.

### The `cap_root_hash` drift is owned by RFC-0958

The circuit declares `cap_root_hash_lo`/`_hi`, never derives them, and asserts
only that a Poseidon commitment over the witness is non-zero. A prover supplies
both the secret and the declared hash, so every check passes by construction —
nothing about the token is proven.

Not fixable inside RFC-0973, for the reason §Root-Secret Binding gives: the fix
needs a cryptographic hash computed by constrained program logic over
felt-constrained words, and Phase 3 measured that at 17,628 CASM words against a
1,600-word ceiling. Adding the derivation without the primitive would produce a
circuit that **looks bound and is not** — the exact non-zero-commitment failure
mode this RFC exists to stop repeating.

**Owner: RFC-0958.** The relation and the circuit that would have to derive it
are both RFC-0958's. The cost question is settled; the correctness gap is not,
and RFC-0958 §Adversary A3 depends on closing it.

The record states what a reader must not conclude: the presence of those fields
in the public inputs is **not** a claim that they are bound. They are inputs,
and the prover chooses them.

### The rename stays out of scope, with the reason

Verified the collision is real: `compute_capability_id` hashes the canonical
form, so the wire-decode site holds `capability_id`, not `BLAKE3(root_secret)`.

Renaming is wire-affecting in both directions that matter. Changing the _value_
would invalidate every stored capability key, since the wire-decode site derives
a registry key. Changing only the _name_ is still a wire-format break for any
consumer matching on it. The table of both sites is recorded so a reader who
reaches for the name knows which value they hold — which is what the table is
for.

### Scope held

No wire format, capability bytes, or registry keys were touched. 330 wallet lib
tests pass, clippy clean with warnings denied, and the RFC change is additions
only.
