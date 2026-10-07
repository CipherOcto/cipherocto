# 0973-v36-d-dependency-direction — Layer-B to Layer-E dependency removal, node-metadata advertisement, registration lint

**Status:** Open
**RFC:** RFC-0973 (Economics): Pluggable Proof Attestation for Capabilities — §Implementation Phases §Phase 5: Dependency direction and deployment properties
**Substrate:** `octo-wallet/Cargo.toml`, `octo-wallet-node`, node metadata, CI lint
**Depends on:** Mission `0973-v36-a-dispatch-and-strategy-crates` — provides the registration host and the strategy links this mission removes the wallet's direct dependency in favour of.

## Scope

Two properties the RFC states that no earlier phase fully delivers. Neither is
optional: an acceptance set that is not advertised makes a downgrade
undetectable, and a registration path living in core inverts the dependency the
layer model forbids. Both are otherwise claims the RFC makes on the strength of
nothing.

1. **Remove the direct dependency** from the Layer-B wallet crate on the
   strategy crate, re-pointing the re-exporting module. Strategy crates are
   reached through registration only.

   This is last rather than first because the lint below fails the build until
   the removal is done, so doing it earlier breaks the tree. The wallet reaches
   strategies through the registry, and a Layer-B core crate must not name a
   Layer-E crate to discover one.

2. **Node-metadata advertisement.** A node advertises its acceptance set in its
   announced metadata, so a peer relying on "this node verifies proofs" can
   detect a node that accepts none.

   The advertised set is the acceptance set **intersected with what the node can
   verify** — the resolved strategy, plus, where the strategy declares a
   verification-side coverage requirement, every kind it needs. That predicate
   is the same condition step 4 evaluates per call, so the advertisement and the
   dispatch gate cannot disagree about which strategies a node claims. Four
   places depend on that predicate and no algorithm computed it before this
   RFC defined it.

   The intersection is evaluated **when the metadata is announced**, so it is a
   claim about the node at that moment rather than an invariant for its whole
   run. A node whose coverage narrows afterwards advertises stale until it
   re-announces. Staleness is a weaker claim than falsehood and is the accepted
   consequence of advertising at all.

   An operator may name a strategy the node cannot verify at a given moment —
   that is what the step-4 gate is for, since coverage can change after boot —
   but the node must not advertise it, and an id the registry does not resolve
   fails at boot rather than at advertisement.

3. **Dependency lint.** A CI check fails the build if a Layer-B core crate gains
   a dependency on a strategy crate. The existing lint that keeps mint calls out
   of a node-type path is the pattern to follow.

   The lint is the property that makes the registration host trustworthy: the
   host is the only place a strategy crate is linked from, which is exactly why
   its link-time observation of the claimant can be believed. Remove that
   property and the allocation-ownership gate in the base mission becomes
   decorative again.

## Acceptance Criteria

- [ ] The wallet crate no longer depends directly on a strategy crate
- [ ] The re-exporting module is re-pointed
- [ ] Strategy crates are reached through registration only
- [ ] A node advertising nothing is distinguishable from one advertising an
      empty set
- [ ] The advertised set is the acceptance set intersected with the
      verifiability predicate
- [ ] A configured-but-unverifiable strategy never appears in the
      advertisement
- [ ] A CI lint fails the build when a Layer-B core crate gains a strategy
      dependency
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes across
      the workspace

### Type Coverage

This mission adds no specification type. It delivers two properties the
already-declared mechanism depends on:

| RFC item                              | Implemented by |
| ------------------------------------- | -------------- |
| Node-metadata advertisement           | This mission   |
| Verifiability predicate computation   | This mission   |
| Layer-B to Layer-E dependency removal | This mission   |
| Registration-host dependency lint     | This mission   |

### Test Vectors

The advertisement has dedicated coverage in this mission: a node accepting no
attestations advertises an empty set rather than an absent one, and a
configured-but-unverifiable strategy never appears. The remaining RFC vectors
were closed by earlier missions in the set.

## Layer direction

This mission exists to make the layer model true in the tree rather than only in
prose. `octo-wallet` is Layer B and years-stable; strategy crates are Layer E.
A Layer-B crate depending on a Layer-E crate inverts Layer direction, which the
layer model forbids, and the lint is what keeps it inverted nowhere else.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p octo-wallet --lib capability
```

The dependency lint must be verified by deliberately introducing the violation
in a scratch branch and confirming the build fails. A lint never seen firing is
not a lint.

## Backward compatibility

Removing the dependency must not change any capability's bytes, registry key, or
verification result. A wallet that verified every pre-RFC capability must still
do so after the module is re-pointed.

## Cross-references

- RFC-0973 §System Architecture — the dispatch whose advertisement is
  published
- RFC-0973 §Data Structures — the verifiability predicate, and the
  type-property argument whose strength differs between cost and soundness
- RFC-0973 §Lifecycle Requirements — requirement 7 (advertisement) and
  requirement 8 (registration host)
- RFC-0973 §Adversarial Analysis — A1 (operator downgrades assurance), which
  the advertisement is its only mitigation
- RFC-0973 §Implicit Assumptions Audit — the operator-administration row
- RFC-0965 §Per-Extension Crate Layout — the layout each strategy crate follows
- Mission `0973-v36-a-dispatch-and-strategy-crates` — provides the host and the
  links whose ownership moves here

## Claimant

@

## Pull Request

#
