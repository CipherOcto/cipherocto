---
name: a-capitalized-word-after-a-section-name-is-prose-or-part-of-the-name-and-the-regex-cannot-tell
description: "validate_cites.sh matches multi-word section names by capitalisation boundary, but the corpus contains both RFC-0909 §Adversary Analysis (multi-word name) and RFC-0011-h §Implementation Phases Phase 6 (one-word name followed by the prose word Phase). The same surface shape means two different things. 120 newly-flagged findings are trailing-word artifacts of the boundary rule. Resolving it needs a heading lookup, not a better regex."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `a-capitalized-word-after-a-section-name-is-prose-or-part-of-the-name-and-the-regex-cannot-tell`

## Status

Open. The replacement pattern is in place and measured; this records the class
it does not resolve, rather than the class it does.

## The finding

`scripts/validate_cites.sh` matches a section name as one or more words, each
starting with a capital and ending on an alphanumeric:

```
RFC_SECTION='§[A-Za-z0-9._]*[A-Za-z0-9]([ ][A-Z][A-Za-z0-9._]*[A-Za-z0-9])*'
```

This repo writes `§section_name` in Title Case, so capitalisation is the only
available boundary. The corpus contains both shapes, and they are
indistinguishable to the pattern:

| Source text                                     | Real section name       | Rest                      |
| ----------------------------------------------- | ----------------------- | ------------------------- |
| `RFC-0011-h §Implementation Phases Phase 6`     | `Implementation Phases` | `Phase 6` — a phase label |
| `RFC-0011-k §Subcommand Taxonomy Phase 3 rows:` | `Subcommand Taxonomy`   | `Phase 3 rows:`           |
| `RFC-0011-h §Test Vectors Phase 3`              | `Test Vectors`          | `Phase 3`                 |
| `RFC-0909 §Adversary Analysis`                  | `Adversary Analysis`    | —                         |

The first three are in the repository today, verbatim. The pattern resolves all
four to the longest match, so the first three are reported as
`looking for 'Implementation Phases Phase'`.

Measured: **120** of the 577 newly-flagged unique findings in a completed
full-tree run are this shape.

## Why a better regex does not fix it

Truncation was the previous behaviour and it is worse — it reported
`looking for 'Implementation'`, which is also not a heading, and it broke the
307 citations that genuinely name a multi-word section.

Matching the _shortest_ viable prefix was implemented and measured. It rescues
402 findings, and every rescued case is a real dangling citation laundered into
a pass: `§Gas Model` accepted against a `Gas` heading, `§Writer Election`
against `Writer`, `§Key Derivation` against `Key`. A regex that resolves by
truncating until something matches is a gate that passes, which is the failure
mode this repository already has two instances of.

## What would actually resolve it

The ambiguity is not in the pattern; it is that the decision "is this word part
of the name" requires the list of headings in the cited RFC. That list is
already loaded in `get_rfc_sections`. The section name could be resolved
against it by longest-match-first, which distinguishes all four rows above:
`Implementation Phases Phase` matches nothing, `Implementation Phases` matches
a heading, so the citation resolves and the trailing `Phase 6` is prose.

The hazard is that longest-match-first must not silently accept a citation whose
full name is wrong. That is why the prefix fallback above was rejected — it
accepts `Gas Model` against `Gas`. The distinction is whether a match is
required to be a complete heading or merely a substring of one, and
`get_rfc_sections` currently returns whole heading text with substring matching
in the caller.

## Acceptance criteria

- [ ] The section lookup resolves against complete headings, longest match
      first, instead of substring matching.
- [ ] A citation that matches only a prefix of a heading stays a finding.
      Verified by mutation: `§Gas Model` must still fail when `Gas` exists.
- [ ] The 120 findings are re-measured and the before/after counts reported.

## Notes

Found by calibration, not by reading the regex. The first replacement attempt
matched greedily and consumed the first word of the following sentence
(`§Adversary Analysis. The attacker` → `§Adversary Analysis. The`). Requiring
each word to end on an alphanumeric fixed that and the trailing-punctuation case
together. The residual 120 are a different problem: not a wrong boundary but an
ambiguous one.
