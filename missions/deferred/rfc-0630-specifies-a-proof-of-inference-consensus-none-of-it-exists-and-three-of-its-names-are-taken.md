---
name: rfc-0630-specifies-a-proof-of-inference-consensus-none-of-it-exists-and-three-of-its-names-are-taken
description: "RFC-0630 names thirteen types and implements none. The three that return hits are name collisions: ProofBundle exists three times as opaque Vec<u8> while RFC-0630 specifies a layer_proofs/batch_proof/verified aggregation struct, and RewardDistribution exists as a PoRelay enum variant rather than a struct. All twelve phase boxes are unchecked and the Version History is a retroactive placeholder, though unlike most of the corpus nothing here claims otherwise. RFC-0630 is load-bearing for the marketplace family: RFC-0918's integration chain points at it and its pseudocode silently uses RFC-0630's InferenceTask. This generalizes the RouterConfig collision recorded for RFC-0900 into a corpus pattern."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0630
    - RFC-0918
---

# Mission `rfc-0630-specifies-a-proof-of-inference-consensus-none-of-it-exists-and-three-of-its-names-are-taken`

## Status

Deferred. An unimplemented Accepted RFC, four occupied type names, and a
tooling question about name collisions. Nothing was actioned.

## Finding

RFC-0630 is Accepted, is the substrate the economics RFCs point at, and has
**nothing implemented**. What makes it worth a mission beyond "an Accepted RFC is
unimplemented" is that three of its type names are _already taken_ in the tree by
unrelated definitions — which turns "not built yet" into "not built yet, and the
name you would use is spoken for".

## Nothing of the specification exists

Thirteen named types across §Specification. Zero occurrences in `crates/` for
`PoIBlock`, `Difficulty`, `Validator`, `WorkMetrics`, `InferenceRequest`,
`InferenceResult`, `ExecutionCommitment`, `BatchProof`, `LayerProof`,
`ShardProof`, `VerificationLevel`, `SecurityModel` and `InferenceMarket`.

The remaining three return hits, and every one of them is a collision rather than
an implementation. Those are the finding.

The thirteen §Implementation Phases boxes are **all unchecked**, and that is
accurate. Nothing in the document claims progress, and there is none to claim.

The Version History is the retroactive placeholder class — two rows, both
labelled `1.0`, the second recording that the table was added on 2026-08-22.
That is recorded separately and is not this mission's finding.

## Three names are already occupied, by different things

This is the part worth acting on. Each is a name a developer would reach for,
and each already resolves to something else.

### `ProofBundle` — RFC-0630 wants a structure, the tree has three blobs

RFC-0630 §Proof Structure specifies a `ProofBundle` carrying `layer_proofs:
Vec<Digest>`, `batch_proof: Digest` and `verified: bool` — a hierarchical
aggregation structure.

**Four** `ProofBundle` types already exist, and none is the one RFC-0630 wants:

| Location                                 | Fields                                                                         |
| ---------------------------------------- | ------------------------------------------------------------------------------ |
| `octo-protocol/src/authorization.rs`     | `ProofBundle(pub Vec<u8>)`                                                     |
| `zk-verifier/src/lib.rs`                 | `{ proof_bytes: Vec<u8> }` — "STWO canonical encoding; stub uses opaque bytes" |
| `octo-cap-zk/src/lib.rs`                 | its own `ProofBundle`                                                          |
| `quota-router-core/src/zk_verify/mod.rs` | `{ stark_proof, public_inputs, casm_hash, casm_version, security_bits }`       |

Three carry nothing but proof bytes. The fourth is the closest in shape — it
adds public inputs and CASM rotation metadata — but it still has no
`layer_proofs`, no `batch_proof`, and no `verified` flag. It is a _single_ proof
with metadata, where RFC-0630 wants a _tree_ of layer proofs rolled up into one
batch proof with a verification verdict attached.

Someone told to implement RFC-0630's `ProofBundle` finds four existing
candidates, no two alike, and no recorded reason to prefer any of them over a
fifth.

### `RewardDistribution` — a struct in the RFC, an enum variant in the tree

RFC-0630 §Block Rewards specifies a struct with `block_subsidy`, `inference_fees`
and `config`, all in `TokenAmount`. The tree's only `RewardDistribution` is a
**PoRelay operation enum variant**, re-exported from `economics` and assigned
discriminant `0x000E`.

An enum discriminant and a payout struct are unrelated. The name is taken.

### `InferenceTask` — defined in RFC-0630, used in RFC-0918 without saying so

RFC-0630 defines `struct InferenceTask`. RFC-0918's pseudocode takes
`task: &InferenceTask` in two functions without ever attributing the type.
RFC-0918 does cite RFC-0630 elsewhere, so this is a missing attribution rather
than a wrong reference — but a reader working from RFC-0918 alone has no way to
find the definition.

## This generalizes a recorded finding

`rfc-0900-specifies-a-listing-model-no-type-or-function-survives-and-promise-no-central-storage`
recorded the first instance of this shape: RFC-0900 specifies a `RouterConfig`
with seven fields, a real `RouterConfig` exists at forty call sites belonging to
RFC-0927, and the two share **zero** fields. A reader finds real work and
concludes the RFC shipped.

RFC-0630 is the same failure at three more names, in a document with no
implementation to speak of. Collected:

| Name                 | Specified by | Already exists as                            |
| -------------------- | ------------ | -------------------------------------------- |
| `RouterConfig`       | RFC-0900     | RFC-0927's, 5 fields, zero overlap           |
| `ProofBundle`        | RFC-0630     | 4 types, none with layer proofs              |
| `RewardDistribution` | RFC-0630     | a PoRelay enum variant                       |
| `InferenceTask`      | RFC-0630     | defined there, used unattributed in RFC-0918 |

**The common shape is worth naming:** in a Rust workspace an unimplemented RFC
type name is not an empty namespace. It is a name that will collide with whatever
already claimed it, and the more load-bearing the RFC, the more likely something
claimed it first. All four are in the economics or proof-systems spine.

## RFC-0630 is load-bearing for the marketplace family

It is not an orphan nobody depends on:

- **RFC-0918's §HOW? integration chain** runs RFC-0124 → RFC-0918 → **RFC-0630** →
  RFC-0843, presenting RFC-0630 as the consensus layer beneath the task market.
- **RFC-0955's §Future Work** lists F1 "Proof-of-Inference Consensus" — RFC-0630
  is what that item is waiting for.
- **RFC-0918's pseudocode** already depends on RFC-0630's `InferenceTask`, as
  above.

So the marketplace RFCs describe a stack whose consensus substrate is entirely
aspirational, and one of them has already started writing against it.

## RewardDistribution is the third TokenAmount consumer, and the first that would break

RFC-0630's `RewardDistribution` uses `TokenAmount` for both amount fields,
making RFC-0630 the third Accepted RFC to depend on the type recorded in
`tokenamount-is-used-by-three-accepted-rfcs-and-defined-by-none-of-them`.

The _manner_ of dependence is what matters here. RFC-0918 and RFC-0955 use
`TokenAmount` inside pseudocode for mechanisms declared as future work. RFC-0630
uses it as a **struct field of a type in a consensus-critical payout structure**.
That is the first appearance where an implementer would need a real answer
immediately rather than eventually.

## One code comment names three types that exist in no RFC

`quota-router-core/src/task_market/orders.rs` explains why `TaskType` is
simplified:

> The full RFC sketches richer payloads (`InferenceTask`, `ProofTask`,
> `VerificationTask`, `AggregationTask`); Gap 6 ships the four-variant spine
> that the order book and escrow layer can reason about.

Of those four, `InferenceTask` exists in RFC-0630. The other three —
`ProofTask`, `VerificationTask`, `AggregationTask` — appear in **zero RFC
documents** and in exactly one place in the tree: that comment.

The comment is also wrong about attribution. Its header credits RFC-0918 §Task
Types, which defines four unit variants of `TaskType` rather than any of those
structs. The comment is otherwise good — it names the simplification and the
reason for it — which is why this is a paragraph here rather than a mission.

## What the owner should do

- **Treat the name collisions as the real finding, not the absence.** Nothing is
  implemented, which is at least honest. The hazard is that four names are
  already spoken for and no document records which occupant should win.
- **Decide `ProofBundle` first.** Four existing definitions against one
  aggregation struct is the highest-friction case, and the answer sets a
  precedent for the other three.
- **Attribute `InferenceTask` in RFC-0918**, or state that its pseudocode is
  illustrative. One line.
- **Fix the `task_market/orders.rs` comment.** It cites three types that exist
  nowhere and attributes all four to an RFC that defines none of them.
- **Keep the unchecked phase boxes.** RFC-0630 is honest about being
  unimplemented, which is the correct treatment and rarer than it should be.
- **Consider a lint for RFC type names already claimed in `crates/`.** Same shape
  as the phantom-citation class — a name used in a specification with a
  conflicting definition elsewhere — and no check looks for it. The version that
  catches the dangerous direction (an RFC specifies a name the tree already owns)
  is cheap and has few false positives. **Proposing, not implementing.**

## Reproduction

```bash
# CONTROL — the search works on this tree
grep -rn --include=*.rs 'pub struct Ask\b' crates/quota-router-storage/src/ask.rs
grep -rn --include=*.rs 'pub struct Dqa\b' determin/src/dqa.rs

# the specification's types, none of which exist
for s in PoIBlock Difficulty WorkMetrics InferenceRequest InferenceResult \
         ExecutionCommitment BatchProof LayerProof ShardProof \
         VerificationLevel SecurityModel InferenceMarket; do
  printf '%-22s %s\n' "$s" \
    "$(grep -rc --include=*.rs "\b$s\b" crates/ | grep -vE ':0$' | wc -l)"
done

# all thirteen phase boxes are unchecked, and that is accurate
grep -cE '^- \[ \]' rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md
grep -cE '^- \[x\]' rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md

# --- the collisions ---

# ProofBundle: the RFC's aggregation struct against four existing types
grep -n -A8 'struct ProofBundle' \
  rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md
grep -rn --include=*.rs 'pub struct ProofBundle' crates/
grep -rn --include=*.rs 'ProofBundle(pub Vec<u8>)' \
  crates/octo-protocol/src/authorization.rs

# RewardDistribution: a struct in the RFC, an enum variant in the tree
grep -n -A8 'struct RewardDistribution' \
  rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md
grep -rn --include=*.rs 'RewardDistribution' \
  crates/octo-network/src/porelay/mod.rs

# InferenceTask: defined in RFC-0630, used unattributed in RFC-0918
grep -n 'struct InferenceTask' \
  rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md
grep -n 'InferenceTask' rfcs/accepted/economics/0918-inference-task-market.md

# the other three named in that code comment exist in no RFC at all
for s in ProofTask VerificationTask AggregationTask; do
  printf '%-20s rfcs:%s crates:%s\n' "$s" \
    "$(grep -rl "$s" rfcs/ | wc -l)" \
    "$(grep -rl --include=*.rs "$s" crates/ | wc -l)"
done
grep -n 'richer payloads' crates/quota-router-core/src/task_market/orders.rs

# --- load-bearing ---

grep -n 'RFC-0630' rfcs/accepted/economics/0918-inference-task-market.md
grep -n 'F1: Proof-of-Inference' rfcs/accepted/economics/0955-model-liquidity-layer.md

# RFC-0630 is the third TokenAmount consumer, as a struct field this time
grep -n 'TokenAmount' rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md
grep -rl 'TokenAmount' rfcs/accepted/ --include='*.md'
```

## Cross-references

- Mission `tokenamount-is-used-by-three-accepted-rfcs-and-defined-by-none-of-them`
  — RFC-0630 is the third Accepted user, and the first that needs the answer at
  implementation time
- Mission `rfc-0900-specifies-a-listing-model-no-type-or-function-survives-and-promise-no-central-storage`
  — the `RouterConfig` collision this generalizes
- Mission `rfc-0918-related-rfcs-names-three-of-four-wrongly-including-an-rfc-that-supersedes-itself`
  — RFC-0918's reference hygiene, the neighbouring half of this story
- Mission `nine-accepted-rfcs-have-a-version-history-whose-only-row-is-that-the-table-was-added`
  — RFC-0630's Version History is in that class
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class,
  in the one place where the absence is honestly declared throughout
