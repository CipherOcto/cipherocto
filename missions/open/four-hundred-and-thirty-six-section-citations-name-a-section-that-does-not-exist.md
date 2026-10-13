---
name: four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist
description: "validate_cites.sh reports 1300 INVALID findings. Measured with scripts/classify_validate_cites.py, 436 of them name a section that exists in no file carrying the cited RFC number, across 116 files and 190 distinct section names. The remaining 864 are the linter opening the wrong file because RFC numbers are not unique. Two verified examples: RFC-0110 has no section called Limbs, and RFC-0862 has no section called RaftLike Writer Election. Repairing a citation requires knowing which section was meant, which is author work, so this records the backlog rather than mass-editing RFC text."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist`

## Status

Open. Measured and characterised. No citation has been changed, because each
one needs to know what the author meant, and that is not recoverable from the
text.

## What is actually broken

`scripts/validate_cites.sh` reports **1300 INVALID** findings. Measured by
`scripts/classify_validate_cites.py`, which reads the linter's own output and
builds each file's heading list with the same command `get_rfc_sections` uses:

| Class                                                      | Count   |
| ---------------------------------------------------------- | ------- |
| **genuinely missing — exists in no file with that number** | **436** |
| recoverable: sibling carries that exact heading            | 332     |
| name occurs only inside a sibling heading (substring)      | 293     |
| near-miss: dropping the trailing word resolves it          | 111     |
| near-miss: a two-word prefix is a heading                  | 63      |
| identifier-qualifier rule would resolve it                 | 34      |

The 625 path-ambiguity findings belong to mission
`rfc-numbers-are-not-unique-so-find-rfc-path-resolves-by-directory-order` and
are not counted here. **This mission is about the other 436.**

## The shape of the 436

They are not spread evenly, and the concentration is what makes the backlog
tractable:

- **116 files**, of which one carries 58
- **190 distinct section names**, of which the most repeated are `F3` (26),
  `Wire Format` (24), `Domain Separators` (21), `F1` (12),
  `Cargo.toml Templates Layer` (10), `Replay Protection` (10), `10.2` (10),
  `Risk` (10)

Worst citing files:

| Count | File                                                                 |
| ----- | -------------------------------------------------------------------- |
| 58    | `docs/research/2026-08-21-vault-monetary-representation-redesign.md` |
| 20    | `rfcs/accepted/process/0011-g-governance-subcommands.md`             |
| 16    | `rfcs/accepted/process/0011-c-agent-lifecycle.md`                    |
| 12    | `rfcs/accepted/process/0009-identity-evolution-v12.md`               |
| 12    | `rfcs/accepted/process/0012-audit-substrate.md`                      |
| 12    | `rfcs/accepted/networking/0970-forwarding-hop-auth-envelope.md`      |
| 12    | `rfcs/accepted/economics/0959-a1-market-delivery.md`                 |

### Two sub-classes that are not section citations at all

`§F1` and `§F3` account for **38** findings and are adversarial-review finding
ids, not section names. `§10.2` is a numbered section. Neither can be repaired
by pointing at a heading: the first should be written as a finding reference
rather than a section cite, and the second needs a heading that does not exist.

This is the same distinction `lint-rfc-anchors.py` already draws with
`is_section_anchor`, where a single-token token is treated as a finding id
precisely because a section is multi-word.

## Two verified examples

Checked by hand rather than trusted from the classifier.

**`§Limbs`.** Cited by `rfcs/draft/numeric/0131-bigint-to-dqa.md` as "Per
RFC-0110 §Limbs". RFC-0110 has no heading named `Limbs`. It has `Schoolbook
MUL Limb Reduction`, and nothing else containing the word in a heading
position. The citation is wrong; which section it meant is not recoverable.

**`§RaftLike Writer Election`.** Cited by
`rfcs/draft/networking/0862p-a-writer-election-bootstrap.md` as "base
RFC-0862". RFC-0862's headings include `WriterElection Protocol (sealed
trait)` and nothing called `RaftLike Writer Election`. The cited name appears
to have been derived from the impl type `RaftLikeWriterElection` rather than
from a heading, which is how a citation gets invented.

## Why nothing is repaired automatically

Each repair needs to answer "which section did the author mean", and the three
plausible answers are all different:

- point the citation at the nearest existing heading,
- rename a heading to match the citation, or
- create the section the citation already promises.

Only the third is safe when the citation is the more recent intent, and there
is no signal here that says which is which. Editing 436 citations across 116
files on a guess would trade a visible defect for an invisible one, which is
worse than the defect.

## Acceptance criteria

- [x] Measure the backlog using the linter's own output and heading extraction,
      with no reimplementation that could drift.
- [x] Split the findings by cause, so the linter's own false positives are not
      counted as repository rot.
- [x] Give the concentration by file and by name, so the work can be ordered.
- [x] Verify two instances by hand.
- [ ] Decide whether `§F1`-style finding ids should be excluded from
      `validate_cites.sh` the way `is_section_anchor` excludes them from the
      anchor linter. That is a change to what the linter rejects, so it is a
      maintainer decision.
- [ ] Repair the concentration in descending order, starting with the single
      research document that carries 58.
- [ ] Consider whether a citation naming an impl type rather than a heading is
      worth detecting as its own class, since that is how `§RaftLike Writer
Election` was invented.

## Notes

The headline number for this linter has been "1335 findings" for most of the
session, quoted as though it described rot. It does not: 48 percent of it is
the linter opening the wrong file, and only about a third is a citation that
resolves nowhere.

Separately, the same measurement found and fixed a real bug in the linter: its
section match was case-sensitive, so a correctly cited section written in
different capitalisation was reported missing. That removed 35 findings and
left the genuinely-missing count at 436, unchanged — verified in both
directions rather than assumed.
