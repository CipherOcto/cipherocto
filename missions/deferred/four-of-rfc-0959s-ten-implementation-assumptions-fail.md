---
name: four-of-rfc-0959s-ten-implementation-assumptions-fail
description: "RFC-0959 states ten Implementation Assumptions, IA-1 to IA-10, six of which name a verification. Four of the six fail. IA-6 fails on its CLI half by measurement: the CLI accepts and signs an Ask whose axis id is INPUT_TOKENS_PER_1K at exit 0. Its parser half passes and is covered by green tests, corrects an earlier v1.0 claim in this mission that is_snake_case had no callers and so the parser rejected nothing. IA-10 fails, the named consumed_receipt_rebuild_determinism_test does not exist and the rebuild it covers does not exist. IA-5 fails, the Ask content address is BLAKE3 over serde_json rather than the RFC-0126 canonical_ser the invariant names, although that canonicalizer is implemented in octo-ident and used by other subsystems, so the risk IA-5 describes is live. IA-7 fails on both halves, and the protocol rule its missing RFC-0871 lint is written to forbid, an authorization decision taken from node_type alone, is present in the tree, unit-tested, and has zero production callers, so it is correct-until-wired. IA-9 passes, since declaring CACHE_KEY_DOMAIN as [u8; 32] is exactly the compile-time assertion the assumption asks for. IA-4 is met, but as a sequencing precondition it cannot fail. IA-1 through IA-3 are satisfied because their dependency RFCs are accepted."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0959
    - RFC-0126
    - RFC-0853
    - RFC-0009
    - RFC-0957
---

# Mission `four-of-rfc-0959s-ten-implementation-assumptions-fail`

## Status

Deferred — IA-5 and IA-6 change content addressing and the publish
interface, IA-10 needs a persistence decision, and IA-7 is the persistence
claim already recorded elsewhere. All are owner calls. Nothing was actioned;
no probe was required, since the one measurement used the built binary.

This is the third systematic sweep of a structure RFC-0959 points at, after
§Roles and the acceptance gates. The three together cover the document's own
claims about responsibility, correctness and dependency.

## Finding

RFC-0959 states ten Implementation Assumptions. **Six name a verification**
(`Test:` in the last column) and four do not — IA-1 to IA-3 are dependency-status
claims and IA-8 is marked `Explicit:`. **Of the six, four fail, one passes, and
one is a sequencing precondition that cannot fail.**

| IA    | Claim                                          | Verification named                          | Verdict                          |
| ----- | ---------------------------------------------- | ------------------------------------------- | -------------------------------- |
| IA-1  | RFC-0853 reaches Accepted first                | `ACCEPTED RISK`                             | **satisfied**                    |
| IA-2  | RFC-0009 reaches Accepted first                | `ACCEPTED RISK`                             | **satisfied**                    |
| IA-3  | RFC-0957 reaches Accepted first                | `CLOSED`                                    | **satisfied**                    |
| IA-4  | Stoolap fork branch is canonical persistence   | RFC-0862 sync test must pass                | precondition met; **not a test** |
| IA-5  | canonical_ser version byte is `0x01`           | `canonical_ser_roundtrip_test`              | **fails**                        |
| IA-6  | axis IDs are snake_case                        | parser rejects; **CLI rejects publish**     | **parser passes, CLI fails**     |
| IA-7  | `node_type` is a projection, not authorization | projection round-trip + lint                | **fails, both halves**           |
| IA-8  | wholesale spread excluded from hash            | `Explicit:`; `node_revenue` table           | **vacuous — table absent**       |
| IA-9  | keyed-hash keys are exactly 32 bytes           | const-assert                                | **passes**                       |
| IA-10 | consumed index rebuilt deterministically       | `consumed_receipt_rebuild_determinism_test` | **fails**                        |

## IA-4 — the one correction worth making explicit

IA-4's verification is "RFC-0862 sync layer integration test must pass before
RFC-0959 acceptance" — a sequencing precondition on another RFC, not a claim
about this code. The substrate it names is real: `octo-sync` is a workspace
member and ships RFC-0862 phase test vectors.

Two earlier passes of this audit read IA-4 as "no sync crate in the tree" by
searching `crates/` only. That was wrong — `octo-sync` was moved out of the
`workspace.exclude` list when the Stoolap cycle was broken, so its tests run in
`cargo test --workspace`. Recorded because it is the kind of error a bounded
search produces and nothing in the tree corrects.

IA-4 therefore passes on its own terms and still teaches the same lesson as
IA-9: a precondition that was met leaves no assertion behind to catch its
later breakage.

## IA-6 — measured on the built binary

The assumption is that PricingAxis IDs are snake_case, with the blast radius
"Axis-ID casing drift → `UnknownAxis` at compute_cost", and the test is that
the parser rejects mixed-case IDs **and the CLI rejects publish**.

**The parser half passes. The CLI half fails.** v1.0 of this mission claimed
both failed; that was wrong and is corrected here.

The validator exists and is wired into the TOML loader:

```rust
for ax in parsed.axes {
    if !is_snake_case(&ax.id) {
        return Err(AxisRegistryTomlError::NotSnakeCase(ax.id));
    }
```

with green tests `rejects_mixed_case_axis_id` and
`rejects_kebab_case_axis_id` covering exactly the two cases the assumption names.
My error was reading "zero callers outside its own module" as "no callers" —
the loader that calls it _is_ the parser, and it lives in the same module.

The CLI half fails by measurement:

```text
$ octo-wallet ask publish --node-type wholesale --model openai/gpt-4 \
    --ttl-unix 1900000000 --seed ./seed.bin \
    --axes INPUT_TOKENS_PER_1K:30000 --jurisdiction US \
    --nonce-hex 42424242424242424242424242424242

exit=0
accepted axis id: INPUT_TOKENS_PER_1K
```

The Ask is built, signed and emitted with an all-caps axis id. The two halves
disagree, which is the interesting part: **a registry loaded from TOML could
never contain this id, while an Ask published through the CLI can.** The
validator and the publisher enforce different rules about the same field.

### Why the blast radius is worse than stated

IA-6 predicts casing drift surfaces as `UnknownAxis` at `compute_cost` — a
loud failure. Measured, it does not. A casing-mismatched axis is absent from
both the Ask's rate table and the registry, so it takes the registry's
**default rate** and settles silently.

That composes with the existing axis finding: the same typo axis measured
there (`input_tokens_per_1K`) settled at the 30,000 default rather than
erroring. So IA-6's stated consequence is the optimistic one, and the
failure it does not mention is the one that happens.

The RFC's own checklist lists this twice — a "jurisdiction gate" and a
"cache classification" test at minimum — and neither the gate nor this one
exists in the form described.

## IA-10 — the named test does not exist

The assumption states `ConsumedReceiptIndex` is "rebuilt deterministically
from stoolap ledger on router restart", with the test
`consumed_receipt_rebuild_determinism_test` covering "10K random receipt
insertions", and names the determinism requirement precisely: "rebuild order =
ledger commit order".

That test does not exist. Neither does the rebuild. `ConsumedReceiptIndex` is
an in-memory `HashMap`; the CLI's default path opens a fresh empty one per
invocation, and the production path is in-memory with no ledger to rebuild
from. The assumption describes a mechanism with no implementation, and names
a test that would have caught its absence.

IA-10 is the sharpest instance of the pattern this audit keeps finding,
because it is a _determinism_ guarantee. A non-deterministic rebuild would let
replays through, and the guard against it is specified precisely enough to be
implemented exactly — which makes its absence harder to explain as deferral.

## IA-5 — the content address is not the canonical serialisation

IA-5 assumes RFC-0126 `canonical_ser` at version byte `0x01`, with blast
radius "Cross-version canonical_ser drift → ask_id collision".

`AskUnsignedPayload::ask_id()` computes:

```rust
let canonical = serde_json::to_vec(self).expect("serializable");
*blake3::hash(&canonical).as_bytes()
```

Plain serde JSON, not the RFC-0126 canonicaliser — and with no version byte.

**The canonicaliser is implemented.** `octo-ident` has it (with a test named
`canonical_ser_is_the_rfc0126_substrate_canonicalizer`) and
`octo-cap-macaroon::bundle_v2` uses it for a content hash. So this is not a
missing capability; the Ask path uses a different serialisation from the one
the assumption names, while other subsystems use the named one.

The risk IA-5 describes is therefore live rather than theoretical: `ask_id`
depends on Rust's derive layout and serde's serialisation details. Adding a
field, reordering one, or changing a serde attribute changes every content
address with no version bump to signal it. This is a distinct cause from the
two-divergent-`ask_id` finding — that one is two formulas disagreeing; this
one is a formula that is not the specified one.

## IA-7 — both halves of the test are absent, and the rule it forbids is written

IA-7 withdrew `Ask.node_type` as an authorization input, on the grounds that a
node which is not serving the in-boundary inference attestation kind must not
mint a ZK-bearing cap. Its test is two-part: a **round-trip of the
`#[repr(u8)]` projection encoding**, and **a lint asserting no protocol rule
branches on the projection in place of the served payload-kind set**.

Neither exists. No test in the tree round-trips the projection's encoding —
the only `#[repr(u8)]` on a node-adjacent type is `RouterNodeLifecycle`, a
different enum in a different crate. And there is no lint; the RFC-0871
reference is to a lint that was never written.

The second half is the one worth reporting, because the rule IA-7's lint is
written to forbid is in the tree and is unit-tested:

```rust
pub fn check_zk_mint_allowed(node_type: NodeType) -> Result<(), ZkMintError> {
    if node_type.permits_zk_mint() { Ok(()) }
    else { Err(ZkMintError::NodeTypeCannotMintZKCap) }
}
```

That is an authorization decision taken from the projection _alone_ — exactly
"in place of the served payload-kind set". It is also correct against RFC-0958's
own gating matrix, and it is the intended shape of that RFC. The tension is
real: RFC-0958 makes `NodeType` a gate, and IA-7 says `node_type` must not be
one. **Two accepted RFCs disagree about whether the same field may authorize a
mint**, and the lint that would have surfaced the disagreement does not exist.

Latent, not live: `check_zk_mint_allowed` and `check_zk_mint_preflight` have
**zero callers outside their own tests**. The function is correct-until-wired.
Whichever way the owner resolves the disagreement, the resolution needs to
happen before the first caller, not after.

## IA-8 — vacuous rather than failed

IA-8 records that wholesale spread is excluded from the settlement hash, and
cites a stoolap `node_revenue` table as where `spread_bps` lives. Neither
`spread_bps` nor `node_revenue` appears anywhere in the tree. The _exclusion_
half holds trivially — the field is in no hash because the field does not
exist. This is the weakest form of a passing assumption: it is true for a
reason that would stop being true the moment the feature landed, with nothing
to notice.

The related persistence claim in IA-7's neighbourhood — that `node_type`
"remains unchanged as persisted storage layout" — also fails: it appears in no
migration and in neither `Ask` nor `AskRow`. That is recorded in
`rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields`.

## IA-9 passes, and it is the model to copy

```rust
pub const CACHE_KEY_DOMAIN: [u8; 32] = *b"cipherocto/cache-key/v1.........";
```

The assumption asks for a const-assert that the key is 32 bytes, and says
"compile-time enforcement via type signature". **The type annotation is the
assertion.** No runtime check, no test needed, and it cannot drift: changing
the length would not compile.

That is what a satisfied assumption looks like in this codebase, and it is
the right thing to point at. IA-9 has one line and enforces a Class-A
invariant permanently. The assumptions that failed did so by specifying a
mechanism in prose rather than a type.

## A note on the shape of this table

Four assumptions name no verification at all (IA-1 to IA-3, IA-8) and two name
something that cannot fail (IA-4's sequencing precondition, IA-5's
cross-RFC-drift watch). That is reasonable for dependency status. But it means
**six of ten rows in the table are unfalsifiable**, which is the same property
as G1's missing digest and as the absence of any CI check on the gates.

The four failures share one cause: each specifies a mechanism in prose where a
type or a test could have carried it. IA-9 is the counterexample in the same
table — one line, enforced by construction, unfalsifiable in the good way
because it cannot be violated rather than because it is not checked.

Combining the three sweeps: RFC-0959's §Roles is not a map of the
implementation, four of six acceptance gates are unmet, and four of ten
implementation assumptions fail. Each is a bounded, individually reasonable
document. Together they mean the RFC describes a system that is more complete
than the one in the tree — which is not a criticism of the code, whose own
documentation is largely accurate where someone has looked. It is a
statement about where the work stopped.

## What the RFC owner should do

- **IA-6**: should publish reject non-snake-case axis ids? The parser already
  does, so this is only a CLI-side question — and the sharper version of it is
  that the two paths enforce different rules about the same field. Separately,
  decide whether the registry's default-rate fallback should apply to an axis
  that was declared but misspelled. The silent-settle behaviour is the more
  urgent half.
- **IA-6**: if casing drift is meant to be loud, that is a change to the
  unknown-axis policy already recorded in
  `marketplace-advertised-price-ignores-default-rate-fallback`.
- **IA-10**: is the ledger rebuild in scope, and does the CLI default need to
  persist? Both are already recorded in the replay mission; this is the
  determinism requirement they were missing.
- **IA-5**: should `ask_id` use the RFC-0126 canonicaliser that
  `octo-ident` already provides? If so the content addresses all change, which
  is the same migration as binding `cost` into the settlement hash. Doing both
  in one versioned change would be cheaper than two.
- **IA-7**: RFC-0958's gating matrix and IA-7's withdrawal of `node_type` as
  an authorization input cannot both stand. Which one governs a ZK mint?
  Whoever decides, the decision belongs in RFC-0871's lint rather than in a
  function that is currently correct-until-wired.
- **All**: marking each assumption's status the way the threat table marks
  A1–A5, and the way IA-9 demonstrates it can be enforced by construction,
  would cost little and tell a reader which are real.

## Reproduction

```bash
# the assumptions table
grep -nE '\*\*IA-[0-9]+:\*\*' rfcs/accepted/economics/0959-ask-settlement-chain.md

# IA-6: the parser rejects mixed case (green tests), the CLI does not
grep -n 'is_snake_case(&ax.id)' -B 2 \
  crates/quota-router-storage/src/axis_registry_toml.rs
cargo test -p quota-router-storage --lib axis_registry_toml
./target/debug/octo-wallet ask publish --node-type wholesale \
  --model openai/gpt-4 --ttl-unix 1900000000 --seed ./seed.bin \
  --axes INPUT_TOKENS_PER_1K:30000 --jurisdiction US \
  --nonce-hex 42424242424242424242424242424242

# IA-10: the named test does not exist
grep -rn 'consumed_receipt_rebuild_determinism' --include=*.rs crates/ || echo absent

# IA-7: neither half of the test exists; the forbidden rule is present
grep -rn 'fn check_zk_mint_allowed' --include=*.rs crates/
grep -rn 'check_zk_mint_allowed' --include=*.rs crates/ | grep -vc 'node_type_gating.rs|zk_vectors.rs'
grep -rn 'repr(u8)' --include=*.rs crates/ | grep -i node

# IA-4: the substrate exists and is a workspace member (not excluded)
ls octo-sync/tests/phase1_tv_0862.rs
grep -n 'octo-sync' Cargo.toml | head -3

# IA-5: serde_json, not the canonicaliser the assumption names
grep -n -A3 'pub fn ask_id(&self)' crates/quota-router-storage/src/ask.rs
grep -rn 'canonical_ser' --include=*.rs crates/octo-ident/src/ | head -3

# IA-9: the type is the assertion
grep -n 'pub const CACHE_KEY_DOMAIN' crates/quota-router-storage/src/cache_key.rs

# IA-8: neither field nor table
grep -rn 'spread_bps\|node_revenue' --include=*.rs --include=*.sql crates/ || echo absent

# dependencies are accepted
ls rfcs/accepted/networking/0853-*.md rfcs/accepted/process/0009-*.md \
   rfcs/accepted/economics/0957-*.md
```

## Cross-references

- RFC-0959 §Implementation Assumptions — IA-1 through IA-10
- RFC-0126 §Deterministic Serialization — the canonicaliser IA-5 names and
  the Ask path does not use
- Mission `four-of-rfc-0959s-six-acceptance-gates-are-not-met` — the gates
  sweep, and the same "half the table cannot fail" property
- Mission `the-rfc-0959-role-table-is-not-a-map-of-the-implementation` —
  the roles sweep; the three together are the document's self-describing
  structures
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` — the
  default-rate fallback that turns IA-6's casing drift into a silent settle
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` — the
  persistence half of IA-10
- Mission `rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields`
  — the persistence half of IA-7
- Mission `the-same-ask-has-two-different-content-addresses` — a _different_
  cause from IA-5, though the same `ask_id`
