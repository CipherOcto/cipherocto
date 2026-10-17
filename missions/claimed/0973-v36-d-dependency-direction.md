# 0973-v36-d-dependency-direction — Layer-B to Layer-E dependency removal, node-metadata advertisement, registration lint

**Status:** Claimed — @mmacedoeu (agent), 2026-10-07
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
   strategy crate. Strategy crates are reached through registration only.

   This is a removal **plus** a re-point, and the two halves are not the same
   shape:

   - **Re-pointable.** `capability/zk_mint.rs`'s `pub use octo_cap_zk::*`
     shim and `capability/registry.rs`'s `CapabilityClass` import (point at
     `octo_cap_macaroon::CapabilityClass`, which `octo-cap-zk` already
     re-exports).
   - **Not re-pointable.** The node-type projection. `NodeType` is declared in
     `octo-cap-zk`, _not_ in the macaroon crate, so `to_zk_node_type` has
     nothing to point at once the dependency is gone — the orphan rule is
     exactly why the mirror exists. Its disposition is a **decision this
     mission must make and record**: delete it as dead code with the
     rationale, or move the mint-time ZK gate somewhere it can still reach a
     strategy. Silently dropping it would dispose of the wallet-side carrier of
     the RFC-0958 §NodeType mint gate, and it contradicts mission C, which
     asserts `NodeType` is retained in the wallet as that gate. Reconcile the
     two in this mission.

   The blast radius is wider than the shim file: `capability/mod.rs`
   re-exports the wallet's whole public ZK mint API from it
   (`mint_with_zk`, `ProofBundle`, `PublicInputs`, `PrivateWitness`,
   `ZkMintError`, `proof_bundle_to_wire`, …), and six integration test files
   import the shim or the crate directly. Each must be re-pointed, moved, or
   given a sanctioned dependency — see item 3 for whether a
   `[dev-dependencies]` edge is permitted, because that decision determines
   where those tests live.

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
   a dependency on a strategy crate **to discover or register it**. The existing
   lint that keeps mint calls out of a node-type path is the pattern to follow.

   The qualifier is load-bearing, and this mission must state the dependency
   tables the lint scans — `[dependencies]` only, or `[dependencies]` plus
   `[dev-dependencies]` — and why. Without that decision the lint has two
   plausible wrong builds, both consistent with "a dependency": scanning
   `[dependencies]` only, which tolerates a dev-edge that re-opens the property
   in the test suite; or scanning both, which fires on the wallet's own ZK
   tests and gets "fixed" with an exemption that quietly re-opens it. Decide
   first, then let the decision determine where the wallet's ZK tests live.

   The lint is the property that makes the registration host trustworthy: the
   host is the only place a strategy crate is linked from, which is exactly why
   its link-time observation of the claimant can be believed. Remove that
   property and the allocation-ownership gate in the base mission becomes
   decorative again.

## Acceptance Criteria

- [x] The wallet crate no longer depends directly on a strategy crate —
      `cargo tree -p octo-wallet --depth 1 -e normal` returns zero
      `octo-cap-zk` edges
- [x] The re-pointable carriers are re-pointed: the `zk_mint` shim and the
      `registry.rs` `CapabilityClass` import (the latter re-pointed to
      `octo_cap_macaroon::CapabilityClass`, the **declaring** crate)
- [x] The node-type projection's disposition is decided and recorded, and
      reconciled with mission C's claim that `NodeType` remains the wallet's
      mint-time ZK gate
- [x] The wallet's public ZK mint API in `capability/mod.rs` is re-pointed, and
      each of the six ZK integration test files is re-pointed, moved, or given
      the sanctioned dependency chosen in item 3 — all six re-pointed to
      `octo_cap_zk::`
- [x] The lint's dependency-table scope is stated explicitly, and the wallet is
      not exempted from the lint by that scope
- [x] Strategy crates are reached through registration only
- [x] A node advertising nothing is distinguishable from one advertising an
      empty set
- [x] The advertised set is the acceptance set intersected with the
      verifiability predicate
- [x] A configured-but-unverifiable strategy never appears in the
      advertisement
- [x] A CI lint fails the build when a Layer-B core crate gains a strategy
      dependency — **verified by deliberately reintroducing the violation**
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes across
      every touched crate

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

@mmacedoeu (agent, 2026-10-07) — autonomous implementation, PR(s) for human review per BLUEPRINT.md Agent Workflow.

## Pull Request

#

## Implementation notes (Phase 5)

The dependency is gone, the advertisement predicate is shared with step 4, and
the lint has been seen firing.

### The dependency removal

`octo-cap-zk` moved out of `[dependencies]`. `cargo tree -p octo-wallet --depth 1
-e normal` returns zero `octo-cap-zk` edges.

What it cost: the wallet no longer **re-exports** the ZK mint API. That is a
deliberate public-API change, and it is the point — re-exporting a Layer-E crate
is the dependency. The API itself is unchanged at `octo_cap_zk::{mint_with_zk,
ProofBundle, PublicInputs, …}`, which is the import path RFC-0965 §Per-Extension
Crate Layout gives an extension crate anyway.

Blast radius, measured rather than assumed: exactly the six test files the
mission named, all inside the wallet. No other crate imported those paths — the
only external hits were two doc comments. All six now import from `octo_cap_zk`.

### `to_zk_node_type` is deleted, and the mint-time gate survives

The mission required a decision here rather than a silent drop. **It had no
production consumer** — every call site in the tree was inside its own `mod
tests`.

The concern was that dropping it would dispose of the wallet-side carrier of the
RFC-0958 §NodeType mint gate. **It does not.** The gate that actually runs is
`NodeType::permits_zk_mint` inside `mint_with_zk_and_signers`, which returns
`NodeTypeCannotMintZKCap` for Wholesale. The wallet keeps `crate::node::NodeType`
and its own `permits_zk_mint`; what was deleted is the _mirror_ into the
extension crate, which existed only because both types were foreign to either
crate and the orphan rule forbade a `From` impl.

This reconciles with mission C, which recorded that `NodeType` is retained in the
wallet as the mint-time ZK gate. Still true.

The `zk_mint` module is kept as a migration note, so the old path resolves to a
compile error that says where to import from instead of a bare unresolved import.

### The advertisement shares one predicate with step 4

Step 4's condition was extracted into `can_verify`, and both the dispatch and
`advertised_acceptance_set` now call it. Two copies of that condition would
eventually diverge, and a node advertising a strategy its own dispatch refuses is
precisely the downgrade the advertisement exists to detect — so the agreement is
asserted directly: `the_advertisement_agrees_with_dispatch_step_4` runs both over
the same registry under two coverage states and requires them to track.

The predicate is the **verification-side** requirement only. The producing-side
requirement is a boot question answered once by `validate_policy`; re-asking it
per call would refuse strategies a booted node is entitled to serve.

### The lint's table scope is decided, not assumed

**`[dependencies]` only.** Both wrong builds are consistent with "a dependency":

- Scanning dev-deps too fires on the wallet's own ZK tests and gets "fixed" with
  an exemption — and the exemption becomes the hole, because the lint can see
  the table but not whether the import reached `src/`.
- Scanning normal deps only is chosen. A dev-edge cannot make a shipped binary
  link a strategy crate, because cargo does not link dev-dependencies into a
  non-test build.

The asymmetry decides it: choosing this scope makes a dev-edge that grew into a
production import **fail the build loudly** (`octo_cap_zk::` is not in scope for
`cargo build -p octo-wallet`), while the other choice makes it fail _silently_
behind an exemption.

**The lint was verified by deliberately reintroducing the violation** — the
wallet's dependency was restored, the lint fired with exit 1 and named the crate
and the strategy, and the manifest was restored and re-checked clean. A lint
never seen firing is not a lint.

One correction along the way: the lint initially listed `octo-wallet-node` among
the core crates and immediately flagged its `octo-cap-poi` edge. That was wrong
— the node **is** the registration host, and linking strategy crates is the
property being protected. It is Layer D and is now deliberately excluded, with
the reason in the lint header.

### Scope note

The mission's clippy AC says "across the workspace". This was run across every
touched crate, not the full workspace: the standing constraint on this work is
never to run the full workspace suite. One pre-existing failure,
`eleven_step_batch_zk_round_trip`, was confirmed identical at HEAD by stashing,
so it is not from this work.
