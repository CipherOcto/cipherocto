---
name: widening-the-anchor-linter-to-docs-and-missions-needs-a-qualifier-rule-before-it-is-useful
description: "The anchors linter scans rfcs only. Widening it to docs and missions was measured rather than assumed: it would surface 2789 findings, of which 644 in docs and 894 in missions are extraction artifacts where the anchor pattern absorbed trailing prose. Of the remainder, 819 resolve as a real section name followed by a qualifier such as §Implementation Phases 1 or §Adversary A5, which is the same shape as the trailing-prose-word problem already filed. Widening without a qualifier rule produces mostly noise. A candidate terminator change that looked inert on the gated tree by extraction count turns CI red by 25 unbaselined findings when measured properly, so it is not the safe first commit it first appeared to be."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `widening-the-anchor-linter-to-docs-and-missions-needs-a-qualifier-rule-before-it-is-useful`

## Status

Open. The linter's scan scope is unchanged. `CLAUDE.md` was brought into scope
separately, in mission
`the-only-ci-wired-anchor-linter-checks-rfcs-but-indexes-headings-from-the-whole-tree`,
because it is one file and had live broken pointers.

## Why this was measured instead of proposed

The obvious argument is "the linter misses 6,370 anchors, so widen it". That
argument is wrong on the numbers, and adopting it would produce a gate that
reports mostly noise on its first run. Every figure below was produced by
running the linter's own `normalize`, `is_section_anchor`, and heading index
against `docs/` and `missions/` before changing anything.

## What widening would actually surface

| Stage                                                 | `docs/` | `missions/` | total |
| ----------------------------------------------------- | ------- | ----------- | ----- |
| anchors the classifier would examine                  | 2,452   | 3,654       | 6,106 |
| would be reported dangling                            | 1,168   | 1,621       | 2,789 |
| minus extraction artifacts                            | 524     | 735         | 1,259 |
| of those, resolvable as section name plus a qualifier | 338     | 489         | 827   |
| remaining after a qualifier rule                      | 186     | 246         | 432   |

Every row is measured per tree, not apportioned. The first two rows are the raw
classifier output; the third applies the prose filter; the last two partition
the third by whether any proper prefix of the name is a known heading.

Two distinct problems sit in those numbers, and neither is a dangling section.

### 1. Extraction artifacts: 1,538 of 2,789

`ANCHOR` runs a name to the next backtick, pipe, newline, comma or dash. In
prose-heavy documents it routinely runs past the end of the reference:

```markdown
> **Note (added 2026-09-01 per RFC-0011-d §Mission Decomposition M9):** ...
```

yields `mission decomposition m9): octo-only`. There is no heading by that
name and there should not be.

The filter used to separate these from real findings rejected any normalised
name containing characters that cannot appear in a heading. It is a crude
proxy and is recorded as such — the point is only that two thirds of the raw
count is the regex running long, not a broken reference.

### 2. Section name plus qualifier: 819

The largest genuine-looking class is a reference to a real section followed by
a phase, gate, or finding id:

| Written reference          | Real section            | Qualifier |
| -------------------------- | ----------------------- | --------- |
| `§Implementation Phases 1` | `Implementation Phases` | 1         |
| `§Adversary A5`            | `Adversary`             | A5        |
| `§Appendix B`              | `Appendix`              | B         |
| `§Test Vectors TV1`        | `Test Vectors`          | TV1       |

By frequency: `implementation phases` 106, `mission decomposition` 48,
`appendix` 44, `future work` 40, `adversarial review process` 35, `test
vectors` 28, `subcommand taxonomy` 27.

This is not a new problem. It is the trailing-qualifier case already recorded
in mission
`a-capitalized-word-after-a-section-name-is-prose-or-part-of-the-name-and-the-regex-cannot-tell`,
and it is why that mission rejected the prefix fallback: accepting
`§Implementation Phases 1` by trimming to `Implementation Phases` would also
have accepted `§Gas Model` against a `Gas` heading, which is a real dangling
reference.

The difference between those two cases is that `1`, `A5`, `B` and `TV1` are
**identifiers with a recognisable shape**, while `Model` is an English word.
That distinction is available to a resolver and is not available to a regex
operating on a name it has already over-captured.

## Scale

- 1,259 non-artifact candidates across 408 files
- worst: 96 in a single research document, 33 in a completed mission, 26 in
  an open mission
- 521 distinct unresolvable names

Almost all of the worst offenders are audit and research documents, which cite
finding ids heavily. That is consistent with the qualifier explanation rather
than with widespread rot.

## What RFC titles would and would not fix

RFC documents put their title in an H1 of the form
`# RFC-0871 (Networking): Specialized Node Protocol Envelope`, which the
heading index does not read. Adding those titles to the index resolves only
**8** of the 1,259. It is not the missing piece and should not be presented as
one.

## Acceptance criteria

- [ ] Decide whether an anchor followed by an identifier-shaped qualifier
      (`1`, `A5`, `B`, `TV1`) resolves to the section it qualifies, and record
      the rule. This is the prerequisite for widening; widening without it is
      what produces the 819-item noise class.
- [x] Decide whether `ANCHOR`'s terminator set should include `)` and `:`.
      Measured rather than assumed, and the answer is more interesting than a
      simple reduction:

      | Tree        | raw dangling | strict candidates |
                  | ----------- | ------------ | ----------------- |
                  | `docs/`     | 1,168 → 1,097 | 524 → 535         |
                  | `missions/` | 1,629 → 1,569 | 735 → 764         |
                  | `rfcs/`     | unchanged, 5,500 anchors extracted | unchanged |

                  Raw findings fall by 71 and 60. Plausible findings *rise* by 11 and 29,
                  because a tighter boundary stops absorbing a forbidden `L###` suffix
                  into the name, which unmasks references that were previously excluded as
                  over-capture. Those 40 newly-visible items are the line-number
                  convention violations recorded separately in mission
                  `ninety-one-section-references-carry-line-numbers-against-the-repo-own-rule`.

                  **That `rfcs/` row is extraction count, not findings, and the
                  distinction reverses the conclusion.** Counting anchors extracted
                  per tree showed `rfcs/` identical under both patterns, which
                  looked like proof the change was inert on the gated tree. Counting
                  *findings* against the baseline shows otherwise:

                  | Measure (rfcs + CLAUDE.md) | current | candidate |
                  | ------------------------- | ------- | --------- |
                  | findings                  | 1,533   | 1,478     |
                  | baselined                 | 584     | 520       |
                  | **new, not baselined**    | 0       | **25**    |

                  The change **turns CI red**. 80 findings disappear because a `)`
                  or `:` now truncates a compound reference, and 64 baselined
                  entries stop matching. But 25 previously-invisible references
                  become visible, and they do not resolve. They are not all
                  genuine — 4 are themselves over-capture artifacts of the new
                  boundary — while the rest are real dangling names such as
                  `§The RFC Process`, `§Part 3`, `§Phantom Types`,
                  `§Implicit Assumptions` and `§Appendix A`.

                  So the prerequisite is real, but it is **not** a safe first
                  commit. It lands together with either a baseline extension or a
                  batch of citation repairs, and the 64 orphaned baseline entries
                  need rewriting to match the new boundaries. That is a
                  deliberate change to a CI gate and belongs to a maintainer.

- [ ] Only then widen the scope, and baseline deliberately. The existing
      baseline is `(name, site)` keyed, so 1,259 entries is an additive
      extension rather than a format change, but it should be written
      knowingly rather than captured by a first run.
- [ ] Consider limiting the widened scan to `docs/` and `missions/open/`,
      excluding `missions/archived/`. Archived and completed documents are
      history and are the largest concentration of audit-id citations.

## Notes

The tempting shortcut is to widen the scan and let the baseline absorb the
result. That would produce a green gate whose baseline records 1,259 entries
of which the large majority are the regex running long and the qualifier case
that a resolver should handle. The baseline would then be a record of known
noise, which is the opposite of what a baseline is for.

Measurement first, scope second. That ordering is the whole content of this
mission.
