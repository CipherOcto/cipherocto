---
name: tokenamount-is-used-by-three-accepted-rfcs-and-defined-by-none-of-them
description: "TokenAmount appears 111 times across 13 RFC documents and is defined in none of them, implemented nowhere in the tree, and never mentioned by RFC-0105. Three of the thirteen are Accepted: RFC-0918 with 22 uses, RFC-0955 Model Liquidity Layer with 21, and RFC-0630 Proof-of-Inference with 7. Meanwhile RFC-0105 DQA is Accepted and describes itself as the deterministic numeric type for pricing, and determin/src/dqa.rs implements it. RFC-0918 uses TokenAmount in f64 arithmetic throughout, which is the round-trip ambiguity RFC-0105 exists to remove. RFC-0918 Phases 2 to 4 are declared future work, so this is the last cheap moment to settle the amount type before someone implements from the pseudocode."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0918
    - RFC-0105
---

# Mission `tokenamount-is-used-by-three-accepted-rfcs-and-defined-by-none-of-them`

## Status

Deferred. An amount type is a Layer A decision with semver consequences, so it
is not mine to make. Nothing was actioned.

## Finding

`TokenAmount` is a type name that thirteen RFC documents use as though it were
a shared primitive. No document defines it. No code implements it. The canonical
amount type that does exist — `Dqa` — is never mentioned in connection with it.

| Measure                                                 | Count   |
| ------------------------------------------------------- | ------- |
| Occurrences of `TokenAmount` across `rfcs/` and `docs/` | **111** |
| Files using it                                          | **13**  |
| Files defining it (struct, enum, type alias, or prose)  | **0**   |
| Occurrences in `crates/` or `determin/`                 | **0**   |
| **Accepted** RFCs using it                              | **3**   |

The three Accepted users:

| Document                              | Uses |
| ------------------------------------- | ---- |
| RFC-0918 Inference Task Market        | 22   |
| RFC-0955 Model Liquidity Layer        | 21   |
| RFC-0630 Proof-of-Inference Consensus | 7    |

The other ten are Draft: RFC-0416, RFC-0521, RFC-0522, RFC-0742, RFC-0843,
RFC-0845, RFC-0108, RFC-0616, RFC-0631, RFC-0650.

## What the corpus already decided

RFC-0105, **Deterministic Quant Arithmetic**, is Accepted at v2.3 and describes
its own remit in terms that leave no room for a second amount type:

> DQA represents numbers as scaled integers (`value × 10^-scale`), providing
> float-like ergonomics with integer-speed arithmetic.

and it names pricing first among its target domains — "optimized for quantitative
finance, **pricing**, and AI inference workloads".

The type is real and lives in this repository at `determin/src/dqa.rs`, wired in
through the workspace path override, with a substantial mission family behind it
(`0105-dqa-core-type`, `0105-dqa-literal-syntax`, `0105-dqa-test-vectors` and
others). It is the settled answer to a question `TokenAmount` is being asked
twenty-two times in RFC-0918 and twenty-one times in RFC-0955.

**RFC-0105 never mentions `TokenAmount`.** Not once. So the corpus has not
recorded a decision to have both — it has recorded a decision about `Dqa` and a
parallel, undefined habit around `TokenAmount` that nobody wrote down.

That is `CLAUDE.md`'s "no parallel abstractions" principle arriving through the
back door: a general codec that already exists, reinvented as a local name in
each document that needs it.

## The f64 problem, which is the part that actually bites

RFC-0105 exists to replace float arithmetic with scaled-integer arithmetic. The
RFC-0918 pseudocode that uses `TokenAmount` does the opposite, throughout.
`TokenAmount` appears in 21 `f64` computations across the document:

- `self.base_price_per_flop * multiplier * difficulty_factor`
- `let decay = 1.0 - (decay_rate * elapsed as f64); let price = start_price * decay;`
- `let adjustment = target_block_time as f64 / avg_time as f64`
- `let new_difficulty = (current_difficulty as f64 * adjustment) as u64`
- `let ratio = demand as f64 / supply as f64`, with `f64::MAX` as the degenerate case

The `f64` here is not incidental. `supply_demand_ratio` returns `f64::MAX` when
`active_workers == 0`, and `calculate_price` multiplies straight through it
before clamping — so a division-by-zero guard resolves to "most expensive price
in the market", silently, by way of the largest finite float. Whatever
`TokenAmount` turns out to be, these are the semantics it would have to
reproduce.

This is the same failure mode already measured in RFC-0900, where a `cumulative_loss_pct_micro` column was kept as an integer specifically "to keep the column Eq-comparable without f64 round-trip ambiguity", and where the in-memory struct reintroduced `f64` one layer up. **There the fix was cheap because the column already existed. Here there is nothing yet, which is why this one is worth catching now.**

## Why the timing matters

RFC-0918 §Version History v1.1 is explicit:

> Phases 2-4 (dynamic pricing, worker selection, reward distribution) remain
> future work.

So `PricingMechanism`, `DifficultyAdjuster`, `NetworkStats` and `VickreyBid` are
correctly absent from the tree, and their absence is **declared, not a defect**.
Verified: all four return zero occurrences, alongside `TokenAmount` and
`PublicKey`.

That disclosure is also what makes this the right moment. §Pricing Mechanism,
§Difficulty Adjustment, §Worker Selection and §Reward Distribution are written
out in enough detail — field lists, formulas, clamp bounds — that someone will
eventually implement from them. At that point the amount type gets chosen by
whatever the first implementer types, in whatever module they are in, under
whatever local name. Three Accepted RFCs will then have a primitive that no
standard describes and no other document expects.

**Nothing here is a defect today.** It is an unmade decision with a written
downstream consequence, which is a different and cheaper thing to fix.

## What the owner should do

- **Decide whether `TokenAmount` is `Dqa` under a domain alias, or a new
  primitive.** If it is `Dqa`, the alias needs a canonical encoding entry in
  RFC-0964 and a single definition site. If it is genuinely a distinct
  quantity — stake versus price versus reward share are arguably three — say so
  and define it once, in RFC-0105 or RFC-0964, rather than in three documents.
- **Rewrite the RFC-0918 pricing pseudocode against the chosen type**, replacing
  the `f64` chain with scaled-integer arithmetic. The formulas can stay; only
  the representation changes.
- **Give `supply_demand_ratio`'s degenerate case a real answer.** `f64::MAX` as
  "supply is exhausted, charge the ceiling" is a business rule wearing a
  floating-point costume. Whatever it should be, it should be a named case.
- **Sweep the ten Draft users too.** They are cheap to fix while Draft and
  expensive afterwards, and two of them (RFC-0742, RFC-0108) already carry
  `TokenAmount::from_f64`, which will not survive a scaled-integer decision.
- **Consider a lint for undefined cross-RFC type names.** This is the same
  defect class as the phantom citations — a name used across documents with no
  definition behind it — but for types rather than RFC numbers, and no existing
  check looks for it.

## Reproduction

```bash
# the canonical type exists, in this repo, and says it is for pricing
sed -n '1,6p' rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md
grep -n 'pub struct Dqa\b' determin/src/dqa.rs
grep -m1 -A3 '^## Summary' rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md

# CONTROL — DQA has a mission family behind it
ls missions/*/ | grep -c '0105-dqa'
# 8

# TokenAmount: used widely, defined nowhere
grep -rl 'TokenAmount' rfcs/ docs/ | wc -l
# 13
grep -roh 'TokenAmount' rfcs/ docs/ | wc -l
# 111

# the three Accepted users, with counts
grep -rl 'TokenAmount' rfcs/accepted/ --include='*.md'
for f in rfcs/accepted/economics/0918-inference-task-market.md \
         rfcs/accepted/economics/0955-model-liquidity-layer.md \
         rfcs/accepted/proof-systems/0630-proof-of-inference-consensus.md; do
  printf '%-46s %s\n' "$(basename "$f")" "$(grep -oc TokenAmount "$f")"
done

# ...and defined nowhere. No struct, no enum, no alias, no prose definition.
grep -rniE '(struct|enum|type|define|defined as) .?TokenAmount' rfcs/ docs/
# empty

# ...and implemented nowhere
grep -rc --include=*.rs 'TokenAmount' crates/ determin/ | grep -vE ':0$'
# empty

# RFC-0105 never mentions it
grep -c 'TokenAmount' rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md
# 0

# the f64 chain that would have to be rewritten
grep -n 'f64' rfcs/accepted/economics/0918-inference-task-market.md | wc -l
# 21, all inside the Phase 2-4 sections
grep -n 'f64::MAX' rfcs/accepted/economics/0918-inference-task-market.md

# and the disclosure that makes this a timing question, not a defect
grep -o 'Phases 2-4.*remain future work' \
  rfcs/accepted/economics/0918-inference-task-market.md
grep -cE '^- \[ \]' rfcs/accepted/economics/0918-inference-task-market.md
# 9
```

## Cross-references

- Mission `rfc-0918-related-rfcs-names-three-of-four-wrongly-including-an-rfc-that-supersedes-itself`
  — the same document, and the sweep that surfaced this while reading its
  Phase 2–4 sections
- Mission `the-marketplace-facade-has-no-production-caller` — amended to v1.1;
  RFC-0918 Phase 1, the part that did ship, is equally unreachable
- Mission `dqa-cmp-and-dqa-sub-disagree-on-canonicalization` — `Dqa`'s own open
  canonicalization question, which an unmade `TokenAmount` decision would sit on
  top of
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  this joins, with a twist: nobody built it _and_ nobody specified it
