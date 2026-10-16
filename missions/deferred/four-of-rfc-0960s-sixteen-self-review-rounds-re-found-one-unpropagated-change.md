---
name: four-of-rfc-0960s-sixteen-self-review-rounds-re-found-one-unpropagated-change
description: "RFC-0960's grand-design file is the only Accepted RFC carrying R-numbered self-review sections, sixteen of them, and no mission covers it. The review block occupies 422 lines of a 1772 line document and sits ahead of the specification rather than after it. Its clearest feature is that four consecutive rounds, R12 through R15, each recorded exactly one finding and all four were the same defect class: domain-separator literals left stale in RFC-0964 by an earlier round's migration to high-bit prefixes. R13 states outright that the previous round fixed one site and missed another in the same block, and R14 that two prior rounds fixed a block while three more sites in the same RFC remained. Sixteen rounds were spent one location at a time on a change that one exhaustive sweep across all sites would have surfaced at once. R16 declares no defects, and that claim holds: I tested it and it did not reproduce. But its scope is narrower than R14's, sweeping only the high-bit ranges rather than the old literals, so a clean bill after sixteen rounds is a statement about a defined sweep rather than about the document."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
    - RFC-0964
---

# Mission `four-of-rfc-0960s-sixteen-self-review-rounds-re-found-one-unpropagated-change`

## Status

Deferred. This is a review-process observation about a document that is already
Accepted, and the RFC owner decides whether the structure stays. Nothing was
actioned.

## Finding

RFC-0960's grand-design file is the **only** Accepted RFC in the corpus with
`R<n> Self-Review` sections — sixteen of them. Zero draft RFCs have them, and no
mission covered them.

The structure is unusual in two ways before any content is considered. The
review block runs from the R1 heading to just before §Authors, **422 lines of a
1772-line document**, and §Specification does not begin until 112 lines after
that block ends. **The review record is front-loaded ahead of the specification
it reviews**, which is the opposite of the usual order and makes the document
read as a log rather than a spec.

## R12 through R15 are four rounds of one defect

Each of those four rounds recorded exactly one finding, and all four are the
same thing: **domain-separator literals left stale in RFC-0964** after an
earlier round migrated them to high-bit prefixes.

| Round | Its own words                                                                                                 |
| ----- | ------------------------------------------------------------------------------------------------------------- |
| R12   | RFC-0964 Summary still has old `BLAKE3(0x01                                                                   |     | ...)` formula |
| R13   | **R12-F1 fixed (Summary item 3) but missed** — a code-block example in the same Summary block                 |
| R14   | **R12 and R13 fixed the Summary block but three more sites in the same RFC still had the OLD `0x01` formula** |
| R15   | still had `0x04` + `0x03` EIP-712 formulas                                                                    |

The rounds say it themselves. R13's finding is the same defect class as R12's,
one block over. R14's is the same again, three sites further on. R15 is the
same migration in a different separator family.

**Sixteen rounds of review produced four consecutive rounds of re-finding one
unpropagated literal**, each round grepping, fixing what it found, and closing
the pass. R14 called its own sweep "exhaustive" and still found three more.

This is the transferable observation. A change to a literal across several
documents needs **one** sweep of every site in every document. A per-round grep
that finds some instances, fixes them, and declares the pass complete will
always generate another round — because the next round greps again and finds the
ones the last one did not look at.

The cost is visible: R12, R13, R14 and R15 are four round-slots, each opening
and closing, to settle a question R14's own "exhaustive" grep answered in one
command.

## R16's clean bill holds, and I checked

I tested whether R16's "NO DEFECTS" was false, and **it is not.**

The round sweeps `0xA2`/`0xA3` and the remaining `0xA0-0xAF` ranges. I grepped
RFC-0964 for surviving low-bit separator literals and found one: `0x01 ||` at
line 515, inside a `Constraint::hash()` worked example.

That looked like exactly the pattern R12–R14 chased. It is not. RFC-0964 §0
distinguishes two roles explicitly:

> (`0x01-0x19` for Constraint), and the constraint-hash separator (`0xA1`, §5)

`0x01` is a **per-Constraint discriminator byte**; `0xA1` is the
**content-addressing separator** for a `ConstraintSet`. The surviving `0x01` is
a different role, not a stale one.

So the hypothesis failed and R16's claim stands. Recorded because a failed test
is evidence too, and because the alternative — filing "R16's no-defects is
false" — would have been a clean-sounding finding that measurement does not
support.

Whether using a discriminator byte as a hash prefix in that example is _good
design_ is a separate question this audit does not settle. It is not a stale
formula, which is the only claim at issue.

## The observation that does survive about R16

Its scope is **narrower** than R14's.

- R14: "exhaustive grep for remaining `0x01`-stale-formula instances"
- R15: "exhaustive grep for remaining `0x02`/`0x03`/`0x04` stale formulas"
- R16: "`0xA2`/`0xA3` + remaining `0xA0-0xAF` ranges"

Each round narrows the set of literals it is looking for. R16's sweep is over
the _new_ prefixes, so "no defects found" means "no defects in the range I
chose", not "the document is consistent".

That is a general property of a sixteen-round review log, and worth stating
plainly because **"no defects found" after an open-ended loop is a statement
about the final sweep, not about the document.** Anyone citing R16 as evidence
that the stack is consistent is citing a narrower claim than it appears to be.

## What the owner should do

- **If the R-round sections stay, put them after §Specification.** A 422-line
  log ahead of the spec it reviews inverts the document's reading order. Nothing
  depends on the current order.
- **Have R12–R15 collapsed to one entry** if the history is kept, naming the
  single root cause and the single sweep that would have caught it. Four rounds
  of the same finding reads as four independent problems.
- **Write down each round's sweep scope explicitly** — the set of literals or
  paths examined — so that "no defects found" is checkable against what was
  actually searched. R14, R15 and R16 each claim a kind of exhaustiveness and
  each means something different by it.
- **Prefer a mechanical sweep to a manual one** for literal migrations. The
  failure here is not carelessness; four competent rounds each did what they
  said and the process still took four iterations.

## Reproduction

```bash
R=rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md

# the only accepted RFC with this structure, and none in draft
grep -rlE '^## R[0-9]+ .*Self-Review' --include=*.md rfcs/accepted/
grep -rlE '^## R[0-9]+ .*Self-Review' --include=*.md rfcs/draft/ || echo "none in draft"

# sixteen rounds, and where the block sits relative to the spec
grep -cE '^## R[0-9]+ ' $R
grep -n '^## Authors\|^## Specification' $R
wc -l < $R

# four consecutive rounds, one finding each, same defect class
grep -nE '^### R(12|13|14|15)-F1' $R

# R16's scope against R14's
grep -n 'exhaustive grep' $R

# the surviving 0x01, and why it is not stale
grep -n '0x01 ||' rfcs/accepted/economics/0964-constraint-encoding-standard.md
grep -n '0x01-0x19' rfcs/accepted/economics/0964-constraint-encoding-standard.md
```

## Cross-references

- Mission `rfc-0959-claims-a-determinism-verification-that-does-not-exist` — the
  same shape: a completion claim whose scope is narrower than it reads, in a
  different RFC
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27's discipline, which is the alternative to a sixteen-round manual loop
- Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`
  — RFC-0960 appears there in Grade 3, so this mission is the other half of what
  that RFC is and is not doing
