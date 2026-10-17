---
name: the-qualifier-rule-is-not-the-prerequisite-for-widening-the-extraction-boundary-is
description: "The mission that gated widening the anchor linter on an identifier-qualifier rule measured that rule and found it addresses 20 of 394 unresolved names on the gated tree, against 98 for the extraction boundary. Of the two-word names the rule rescues, three classes are rescued wrongly: a bare common word as head, an L### line-number tail, and an over-capture. The rule cannot turn CI red but it is also not the prerequisite; the closing-paren and colon terminators are."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-qualifier-rule-is-not-the-prerequisite-for-widening-the-extraction-boundary-is`

## Status

Open. Supersedes mission
`widening-the-anchor-linter-to-docs-and-missions-needs-a-qualifier-rule-before-it-is-useful`,
whose premise this measurement contradicts. That mission is archived rather than
deleted, because the reasoning that led to the premise is the record of why it
was reasonable to hold.

## The premise that did not survive measurement

The superseded mission ordered the work as _decide the qualifier rule first,
widen second_, on the argument that a qualifier rule is what separates the
useful findings from the noise when the scan leaves `rfcs/`. The supporting
figure was a count of names the rule would rescue.

The figure was never measured. It was read off a table whose own row label
described the number the **existing** prefix fallback already resolves, and the
two were conflated. The real number is roughly a twentieth of it.

Measured with `python3 scripts/measure_anchor_lint.py --all --qualifier`, which
calls the linter's own `normalize`, `is_section_anchor` and `resolves` rather
than a copy of the loop:

| Tree        | unresolved names | names whose trailing token is identifier-shaped | of those, unresolvable after the existing fallback |
| ----------- | ---------------- | ----------------------------------------------- | -------------------------------------------------- |
| `rfcs/`     | 394              | 26                                              | **0**                                              |
| `docs/`     | 394              | 35                                              | 4                                                  |
| `missions/` | 415              | 30                                              | 2                                                  |

The third column is the finding. On the gated tree **the class the qualifier
rule was proposed to handle is already fully resolved by the bidirectional
word-prefix fallback** added earlier in the session. The six residual items in
`docs/` and `missions/` are not qualifier cases at all — they are over-captures
and line-number references, both of which the terminator change addresses.

## What the rule actually addresses

`resolves` never trims an anchor below two words. That floor is what stops
`§Gas Model` resolving against a `§Gas` heading, and it is right. Its one wrong
consequence is that a two-word anchor whose second word is an identifier never
resolves: `§Adversary A5` does not resolve against a heading `Adversary`.

That is the whole population. Partitioning it by verdict, two-word names only:

| Verdict                         | `rfcs/` | `docs/` | `missions/` | Example             |
| ------------------------------- | ------- | ------- | ----------- | ------------------- |
| rescued                         | 20      | 15      | 22          | `adversary a5`      |
| refused, generic head           | 1       | 0       | 1           | `top k`, `the v2`   |
| refused, head is not a heading  | 1       | 0       | 2           | `principle 12`      |
| excluded, `L###` line reference | 0       | 10      | 0           | `risk l158`         |
| excluded, over-capture          | 0       | 1       | 0           | `attenuation); 24h` |

## Three reasons the rule cannot be shape-only

Each of these was found by listing the names the rule rescues, not by counting
them. A count of 20 would have read as a clean win; the items are not clean.

1. **A bare common word is not a section name.** `§Top K` is the section `Top`
   plus the letter K, and `§The V2` is nothing at all. Both are in the corpus.
   This is the laundering the originating mission warned against when it
   rejected prefix fallback, reached by a different route than the one it
   considered. The guard that prevents it — never trimming below two words — is
   the same guard the rule has to relax, so the rule must replace it rather
   than bypass it.

2. **`L###` tails are convention violations, not qualifiers.** Ten of the
   `docs/` items are `§Security L158`-shaped: a real section followed by a line
   number, which `CLAUDE.md` forbids. Accepting them makes the anchor linter
   stop reporting them, and the only other tool that reports them is
   `measure_anchor_lint.py --line-refs`, which is a measurement and is not
   wired into CI. The rule would delete the sole signal.

3. **An over-capture can look rescued.** `§attenuation); 24h` trims to a name
   that resolves, but the defect is the extraction boundary, and the finding it
   produces would be attributed to the wrong cause.

The rule as written in `scripts/measure_anchor_lint.py` therefore carries all
three exclusions, with the reason inline at each.

## Cost on the gated tree

Measured rather than assumed, by applying the rule to the gated findings and
re-testing them against the baseline:

| Measure                     | before | after |
| --------------------------- | ------ | ----- |
| findings                    | 588    | 523   |
| would turn CI red           | 0      | 0     |
| distinct names rescued      | —      | 20    |
| baseline entries left stale | —      | 65    |

It cannot turn CI red, because every finding it removes was already baselined.
Sixty-five baseline entries stop matching, which is inert: the gate tests
findings minus baseline, never the reverse.

## What this changes about the ordering

The superseded mission made the qualifier rule the prerequisite. It is not. The
dominant class of unresolved name is over-capture — a name containing a
closing paren or a colon, where `ANCHOR` ran past the end of the reference:

| Tree        | names containing `(` `)` or `:` | share |
| ----------- | ------------------------------- | ----- |
| `rfcs/`     | 98                              | 25 %  |
| `docs/`     | 117                             | 30 %  |
| `missions/` | 119                             | 29 %  |

Against 20, 15 and 22 for the qualifier rule. The terminator change is worth
roughly five times the qualifier rule on every tree, and it is already
measured at 12 new findings on the gated tree with 2 of them genuine broken
citations, both since fixed.

So the order becomes: terminator first, qualifier second or never, widening
third.

## Acceptance criteria

- [x] Measure the rule against the linter's own functions rather than a copy of
      the loop, and record the verdict partition per tree.
- [x] List the names the rule rescues and check them individually.
- [x] Establish whether the rule can turn CI red. It cannot.
- [ ] Decide whether the rule is implemented in `lint-rfc-anchors.py` at all.
      It is cheap and safe as measured, but it addresses 20 baselined findings
      on the gated tree, which is a small return for a change to the resolver
      that every future citation depends on. The argument for it is that it
      stops a live gate from being wrong rather than merely noisy.
- [ ] Carry the `L###` exclusion into whichever tool is wired into CI, or
      accept that the convention violation has no gate at all.

## Notes

The correction to the superseded mission went through two wrong states before
this one. The first measurement filtered trailing tokens with an uppercase
pattern, against names that `normalize` had already lowercased, and reported
the rule as rescuing 11 items — a number too small and too clean, because it
was measuring the wrong set. The second filtered on three or more words and so
excluded the two-word case entirely, reporting the rule as addressing nothing.

Both were found by the same check: the population did not look like the
corpus. `§Adversary A5` was in the first table and absent from the second, and
that absence is what exposed the length filter.

The lesson recorded by mission
`a-capitalized-word-after-a-section-name-is-prose-or-part-of-the-name-and-the-regex-cannot-tell`
still holds and is unaffected: the ambiguity is not in the pattern, and
resolving it needs the heading list. The heading list is what `resolves` uses,
and it already answers almost all of these.
