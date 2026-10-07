# 0973-v36-c-substrate-corrections — Capability class projection and the recorded specification drift

**Status:** Open
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

- [ ] The capability-class gap is decided and the decision recorded in the
      wallet's documentation
- [ ] The class registry either gains a production call site or is documented
      as a projection
- [ ] The `cap_root_hash` drift has an owner and a recorded decision
- [ ] The rename is explicitly recorded as out of scope and why
- [ ] Nothing in this mission changes the wire format

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

@

## Pull Request

#
