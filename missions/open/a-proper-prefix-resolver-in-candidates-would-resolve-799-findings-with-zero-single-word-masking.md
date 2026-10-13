---
name: a-proper-prefix-resolver-in-candidates-would-resolve-799-findings-with-zero-single-word-masking
description: "The anchors linter already resolves a section name through progressively shorter forms in its candidates function, but that truncation only strips parentheticals. Extending it to drop trailing qualifier tokens resolves 799 of 1237 findings on the gated tree with zero cases resolved by a single-word heading. Combined with the previously measured terminator change it takes findings to 401. It still turns CI red, but the new surface is only 11 distinct names across 15 occurrences, all verified as genuinely unresolvable."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `a-proper-prefix-resolver-in-candidates-would-resolve-799-findings-with-zero-single-word-masking`

## Status

Open. Not implemented. It changes a CI-wired gate and belongs to a maintainer.

## The mechanism already exists

`scripts/lint-rfc-anchors.py` does not resolve a name against headings
directly. It calls `candidates`, which produces progressively shorter forms:

```python
stripped = re.sub(r"\s*\(.*$", "", current).strip().rstrip(".,;:)]}")
```

So `§Adversarial Review (12-finding table)` resolves via the shortened
`adversarial review`. The infrastructure for truncation is present and
deliberate.

It handles exactly one truncation: a parenthetical. It does not drop a
trailing qualifier, which is the shape most of the outstanding findings have.

## What the extension does

```python
def candidates2(name: str) -> list[str]:
    out = list(candidates(name))          # existing behaviour, unchanged
    parts = name.split()
    for k in range(len(parts) - 1, 0, -1):
        c = " ".join(parts[:k])
        if c not in out:
            out.append(c)
    return out
```

`implicit assumptions audit 15` gains `implicit assumptions audit`,
`implicit assumptions` and `implicit`. The parenthetical path is untouched, so
this is additive rather than a rewrite.

## Measured on the gated tree

`rfcs/` plus `CLAUDE.md`, the population CI actually enforces:

| Measure                        | current | extended |
| ------------------------------ | ------- | -------- |
| findings                       | 1,237   | 438      |
| resolved by the prefix rule    | —       | 799      |
| baselined entries that go dead | —       | 242      |

## The masking check that decided it

The prefix fallback rejected in mission
`a-capitalized-word-after-a-section-name-is-prose-or-part-of-the-name-and-the-regex-cannot-tell`
was rejected because it accepted `§Gas Model` against a `Gas` heading. That is
the failure mode any prefix resolver has, so it was measured directly rather
than assumed absent:

> **Findings resolved by a single-word heading: 0**

Every resolution lands on a multi-word heading. The `Gas` failure cannot occur
under this rule, because no single word is ever accepted on its own.

## Combined with the terminator change

Adding the `)` and `:` terminators recorded in
`widening-the-anchor-linter-to-docs-and-missions-needs-a-qualifier-rule-before-it-is-useful`:

| Measure            | current | terminator + resolver |
| ------------------ | ------- | --------------------- |
| findings           | 1,237   | 401                   |
| new, not baselined | 0       | 108                   |

The 108 needs care, and the breakdown was measured rather than assumed:

| Of the 108 unbaselined after the change | Count |
| --------------------------------------- | ----- |
| already unbaselined on today's tree     | 93    |
| genuinely new                           | 15    |

93 of them are a pre-existing property of this tree, not something these
changes create. Today's linter already carries 653 unbaselined findings on the
gated tree. The genuinely new surface is:

> **11 distinct names across 15 occurrences**

```
part 3                                   (3)
phantom types                            (2)
the rfc process                          (2)
implicit assumptions
pattern b
nodetype gating matrix
sc2 (this rfc
g5 "gateway federation
phantom types + rfc-0959-a1 §algorithms
roles/§adversary (5-question a1-a5
capability" → "rfc-0957 §capabilitytoken state machine"; "rfc-0964 §0
```

Each was checked against the heading index. None resolves, and several are
themselves over-capture:

- `the rfc process` — the heading was removed from `CLAUDE.md` in an earlier
  change; references to it elsewhere remain and are genuinely broken
- `pattern b`, `part 3` — no heading by that name in any tree
- `nodetype gating matrix` — the tree has `nodetype gating rule`, a different
  section
- the last two are cross-reference text swallowed by the boundary

## Why this is the highest-value single change

The same resolver addresses three recorded problems at once:

1. The **827 qualified citations** in `docs/` and `missions/`
   (`§Implementation Phases 1`, `§Adversary A5`, `§Appendix B`)
2. The **23 near-miss findings** the terminator change alone would surface
3. **242 baselined entries** that exist only because the matcher could not
   truncate

It is also a prerequisite for both other changes rather than an independent
step: without it, neither the terminator change nor widening the scan scope
produces a usable gate.

## Acceptance criteria

- [ ] The prefix rule is reviewed for the one case it could mask: a reference
      that shares a leading phrase with a heading but means a different
      section. The single-word check shows the gross failure is absent; a
      multi-word collision remains possible and is worth spot-checking.
- [ ] The 242 dead baseline entries are pruned deliberately, not left to
      accumulate as silent suppressions that no longer match anything.
- [ ] The 11 new names are resolved or baselined knowingly.
- [ ] Decided together with the terminator change rather than separately, since
      the measured surface of each depends on the other.

## Notes

The masking check is the part worth keeping whatever is decided. The rejected
prefix fallback and this proposal are the same mechanism, and the difference
between them is a single constraint: never accept a single-word heading as a
resolution. That constraint is what turns a gate that hides errors into one
that removes noise, and it is checkable, which is why it was checked rather
than argued.
