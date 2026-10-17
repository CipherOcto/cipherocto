---
name: rfc-0918-related-rfcs-names-three-of-four-wrongly-including-an-rfc-that-supersedes-itself
description: "RFC-0918 §Related RFCs lists four RFCs and three are wrong. RFC-0124 is cited twice under two different titles, both false: the only RFC-0124 in the tree is a Numeric/Math DFP-to-DQA lowering pass at Proposed status, not an Economics proof market, and it lists Supersedes: RFC-0124 in its own status block, declaring itself as superseding itself. RFC-0140 was renumbered to RFC-0740 per that document's own header, and RFC-0918 and RFC-0630 both cite the dead number. RFC-0843 is Draft, cited as a bare peer of an Accepted RFC. RFC-0630 is correct. Credit where due: this document's Phase 1 checklist is the best phase table in the family, and its five checked items all verify."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0918
---

# Mission `rfc-0918-related-rfcs-names-three-of-four-wrongly-including-an-rfc-that-supersedes-itself`

## Status

Deferred. Three reference entries, one self-referential status field, and one
corpus-wide renumber question. Nothing was actioned.

## Finding

RFC-0918 §Related RFCs lists four RFCs. One is right.

| Cited    | Cited as                                             | Actually                                                                   |
| -------- | ---------------------------------------------------- | -------------------------------------------------------------------------- |
| RFC-0124 | "(Economics): Proof Market and Hierarchical Network" | **Numeric/Math**, "Deterministic Lowering Pass — DFP to DQA", **Proposed** |
| RFC-0630 | "(Proof Systems): Proof-of-Inference Consensus"      | correct                                                                    |
| RFC-0140 | "(Consensus): Sharded Consensus Protocol"            | **renumbered to RFC-0740**; no RFC-0140 exists                             |
| RFC-0843 | "(Networking): OCTO-Network Protocol"                | correct title, but **Draft**                                               |

## RFC-0124 is wrong twice, and cites itself twice

RFC-0918 refers to RFC-0124 twice, under **two different titles in one file**:

- In the §HOW? integration diagram: `RFC-0124 (Proof Market)`
- In §Related RFCs: `RFC-0124 (Economics): Proof Market and Hierarchical Network`

There is exactly one RFC-0124 file in the tree. It is
`rfcs/draft/numeric/0124-deterministic-numeric-lowering.md`, titled "RFC-0124
(Numeric/Math): Deterministic Lowering Pass — DFP to DQA", with
`**Version:** 1.7 (Draft)`, `**Status:** Proposed`, `**Category:** Numeric/Math`.

So the category is wrong (Economics versus Numeric/Math), both titles are wrong,
and the status is a Draft proposal rather than a peer. The "Proof Market" name
suggests a legacy RFC-0124 that existed before the renumbering and has since been
replaced — which is very likely how this happened, and which the RFC-0918 §HOW?
diagram half-acknowledges by marking itself "canonical replacement per numeric-tier
phantom sweep".

### RFC-0124 declares that it supersedes itself

The same file's status block reads:

```
**Version:** 1.7 (Draft)
**Status:** Proposed
**Supersedes:** RFC-0124 (legacy, incomplete)
```

A document cannot supersede its own number. The field almost certainly intends
the legacy RFC-0124 — the proof market — which would be a true and useful
statement, but as written it says a file supersedes itself. Unlike a stale
citation, this one is not covered by the classified phantom-citation work: it
resolves perfectly. The file exists, the number is right, and the sentence is
still wrong. That makes it a genuinely separate defect rather than another
instance of the renumbering class.

## RFC-0140 no longer exists

`rfcs/draft/consensus/0740-sharded-consensus-protocol.md` carries its own
renumbering note:

> This RFC was renumbered from RFC-0140 to RFC-0740 as part of the
> category-based numbering system.

No RFC-0140 file exists. The citation spread is wider than RFC-0918:

| Status             | Files citing `RFC-0140`    |
| ------------------ | -------------------------- |
| **Accepted**       | **2** — RFC-0918, RFC-0630 |
| Draft              | 8                          |
| knows the renumber | 1 (RFC-0740 itself)        |

Against that, only 3 files anywhere cite `RFC-0740` correctly.

**This is inside classified territory, so it is recorded rather than proposed for
repair.** The existing mission
`three-hundred-and-thirty-nine-phantom-citations-are-ninety-one-percent-pre-renumbering-references`
measured 308 of 339 phantom citations as pre-renumbering `0xxx` references and
classified them **deliberately unfixed**, on the grounds that mass-editing would
corrupt historical record and external references alike. That judgement is
sound and this mission does not disturb it.

What this mission adds is one distinction the classified work does not draw: it
never mentions Accepted. A Draft RFC citing a dead number is a draft artefact. An
**Accepted** RFC is a specification, and two of them carry a reference to a
document that cannot be found. Whether that changes the calculus is an owner's
call, not mine — but it is the question the existing analysis leaves open.

## The document that gets it right, and does it well

RFC-0918's §Implementation Phases is the best phase table in the economics
family, and it is worth saying why, because it is the opposite of everything above.

- **Five `[x]` items in Phase 1, nine `[ ]` across Phases 2–4.**
- **§Version History v1.1 and the checkbox table agree exactly.** The history row
  says "Phases 2-4 (dynamic pricing, worker selection, reward distribution)
  remain future work", and the three unchecked phases are precisely dynamic
  pricing, worker selection and reward distribution.
- **Every checked item verifies.** `task_market::orders::TaskMarket`,
  `task_market::orders::TaskSpec`, `task_market::escrow::TaskEscrow`,
  `task_market::slashing::TaskMarketSlashing` and `match_top` all exist where the
  checklist says they do.

The code block above §Order Book even points forward at its own scope, noting the
four-variant `TaskType` "matches the Phase 1 (Core Market) implementation" and
that later phases "may extend each variant". The specification and the
implementation are kept in step **in the body** of the document.

Which makes §Related RFCs the outlier. One section of this RFC is exemplary about
what shipped and what did not, and the next section lists a superseded RFC under
two invented titles.

## A measurement I attempted and abandoned

I tried to quantify the Accepted-versus-Draft split across the corpus's
pre-renumbering citations, to support the point above. The first attempt searched
for `RFC-0[0-9]{3}` and returned all 157 accepted RFCs with 12,949 hits — because
that pattern matches _every_ four-digit zero-prefixed number, including current
ones like RFC-0630 and RFC-0956. It measures nothing.

Isolating genuinely dead references means resolving each citation against the
filesystem, which is exactly what `scripts/validate_cites.sh` already does. Per
standing practice I do not reimplement a linter to measure it, and
`CLAUDE.md` reserves that script for user-initiated runs. **So this mission
makes no corpus-wide Accepted/Draft claim.** The two-Accepted / eight-Draft
figure above is scoped to `RFC-0140` alone, which is verifiable by inspection
because RFC-0740 documents its own renumbering.

The failed attempt is recorded because the temptation is real: a plausible-looking
grep that returns a large confident number is exactly the kind of thing that
becomes a finding if nobody checks what the pattern actually matches.

## What the owner should do

- **Fix RFC-0124's two titles in RFC-0918**, or drop the entry. There is no proof
  market at RFC-0124.
- **Fix `Supersedes: RFC-0124` in RFC-0124 itself.** If it means the legacy proof
  market, name that document. As written a file supersedes itself. This one is
  outside the phantom-citation classification and is not covered by it.
- **Decide whether Accepted RFCs get renumber repair when Draft ones do not.**
  RFC-0918 and RFC-0630 both cite a number that RFC-0740 says no longer exists.
  The existing mission's "do not mass-edit" judgement is right for historical
  record; whether a live specification gets a narrower rule is a separate
  question it does not currently ask.
- **Qualify RFC-0843 as Draft** in RFC-0918's related list, or move it.
- **Leave the rest of the document alone.** The phase table and the v1.1 history
  are the model. Three of four related-RFC entries going stale does not
  generalise to the body, which is in better shape than most of the corpus.

## Reproduction

```bash
# CONTROL — RFC-0630 is cited correctly, so the list is not uniformly wrong
grep -n 'RFC-0630' rfcs/accepted/economics/0918-inference-task-market.md
find rfcs -name '0630-*'

# RFC-0124: one file, and it is not a proof market
find rfcs -name '0124-*'
sed -n '1,10p' rfcs/draft/numeric/0124-deterministic-numeric-lowering.md
grep -n 'RFC-0124' rfcs/accepted/economics/0918-inference-task-market.md
# two entries, two different titles, both wrong

# ...and it declares that it supersedes itself
grep -n 'Supersedes' rfcs/draft/numeric/0124-deterministic-numeric-lowering.md
# **Supersedes:** RFC-0124 (legacy, incomplete)

# RFC-0140: renamed, and RFC-0918 cites the dead number
sed -n '5,9p' rfcs/draft/consensus/0740-sharded-consensus-protocol.md
find rfcs -name '0140-*'
# empty — no RFC-0140 file exists
grep -rln 'RFC-0140' rfcs/accepted/ --include='*.md'
# RFC-0918 and RFC-0630, both Accepted
grep -rln 'RFC-0140' rfcs/draft/ --include='*.md' | wc -l
# 8

# RFC-0843 is Draft
sed -n '1,6p' rfcs/draft/networking/0843-octo-network-protocol.md

# --- what this document gets right ---

# five checked, nine unchecked, and they agree with the v1.1 history row
grep -cE '^- \[x\]' rfcs/accepted/economics/0918-inference-task-market.md
grep -cE '^- \[ \]' rfcs/accepted/economics/0918-inference-task-market.md
grep -o 'Phases 2-4.*remain future work' \
  rfcs/accepted/economics/0918-inference-task-market.md

# and every checked item verifies
grep -rn 'pub struct TaskMarket\b\|pub struct TaskSpec\b' \
  crates/quota-router-core/src/task_market/orders.rs
grep -rn 'pub struct TaskEscrow\b' crates/quota-router-core/src/task_market/escrow.rs
grep -rn 'fn match_top' crates/quota-router-core/src/task_market/orders.rs

# --- the measurement that does not work, and why ---
# matches EVERY zero-prefixed 4-digit number, current ones included
grep -rohE 'RFC-0[0-9]{3}' rfcs/accepted/ --include='*.md' | wc -l
grep -roh 'RFC-0630' rfcs/accepted/ --include='*.md' | wc -l
# the second line proves the pattern matches live RFC numbers too
```

## Cross-references

- Mission `three-hundred-and-thirty-nine-phantom-citations-are-ninety-one-percent-pre-renumbering-references`
  — the classified renumbering work this sits inside, and the Accepted/Draft
  distinction it does not draw
- Mission `the-marketplace-facade-has-no-production-caller` — amended to v1.1 for
  RFC-0918's Phase 1, which is real, complete, and equally unreachable
- Mission `rfc-0900-specifies-a-listing-model-no-type-or-function-survives-and-promise-no-central-storage`
  — the sibling finding on the marketplace RFC's own reference sections
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  these reference entries join
