---
name: anchor-lint-coverage-gap-claude-md-pointers-found-and-fixed
description: "The RFC anchor lint resolves references only inside rfcs/, which is 3752 of 9881 in-scope section references, or 38 percent. docs and missions are indexed as heading sources but never scanned as reference sites, so 6119 references are unexamined. CLAUDE.md was in the same position and is now scanned. The gap was real and already cost something: CLAUDE.md carried three pointers to sections that do not exist, all now fixed, while the tool trusted to catch that could not see them. Figures re-measured with scripts/measure_anchor_lint.py, which calls the linter rather than copying it."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `anchor-lint-coverage-gap-claude-md-pointers-found-and-fixed`

## Status

Partially resolved. `CLAUDE.md` is scanned and its three broken pointers are
fixed. `docs/` and `missions/` remain unscanned, which is the open part.

## The finding

`scripts/lint-rfc-anchors.py` does two separate jobs. It builds an index of
resolvable headings from several sources, then walks a _different_, narrower set
of files looking for references. The two sets do not match, and the gap is where
the real damage is.

Indexed as heading sources: `rfcs/`, `docs/`, `missions/`, and `CLAUDE.md`.
Scanned for references: `rfcs/` only.

| Surface     | In-scope references | Dangling | Checked today |
| ----------- | ------------------- | -------- | ------------- |
| `rfcs/`     | 3,752               | 588      | yes           |
| `docs/`     | 2,452               | 538      | **no**        |
| `missions/` | 3,667               | 644      | **no**        |
| `CLAUDE.md` | 10                  | 0        | yes           |

**Coverage is 38 percent.** 6,119 references are never examined.

Re-measured with `scripts/measure_anchor_lint.py`, which calls the linter's own
matcher, resolver and heading index rather than reimplementing them. The
original figures in this table came from a hand-rolled copy of the resolution
loop and were close but not exact: `missions/` anchors were 3,618 rather than
3,667, and `CLAUDE.md` showed 2 dangling rather than 0 because its three broken
pointers have since been fixed and it is now scanned.

Reproduce with:

```bash
python3 scripts/measure_anchor_lint.py --all
```

The gap itself is unchanged: three trees are indexed as sources of valid anchor
names and two of them are never read for references.

## What the gap already cost, and what was fixed

`CLAUDE.md` is the file every agent reads before anything else, and it names
BLUEPRINT.md as _the operational playbook_. It carried three pointers into that
playbook that named sections which are not there:

- `§The RFC Process` — BLUEPRINT.md has no such heading, and the heading it
  pointed at is actually local to CLAUDE.md
- `§Multi-Mission Decomposition` — BLUEPRINT.md contains no heading matching
  _Decomposition_ at all
- `§RFC Reference Conventions Reaffirmed` — the heading carries a parenthesised
  suffix the normaliser does not strip

All three are fixed, and CLAUDE.md is now scanned rather than only indexed. It
reports 0 dangling.

What the episode established, and it is the reason this mission mattered: the
file every agent reads first was carrying references to sections that do not
exist, and the one tool trusted to catch that was structurally incapable of
looking. Two thirds of the file's references landed, which is exactly why the
failure was not obvious on read.

The scope gap itself is unchanged. `docs/` and `missions/` are still indexed as
heading sources and never read for references.

## Why the scope should not simply be widened

Extending the reference scan to `docs/` and `missions/` is a one-line change
and is deliberately left alone. Three reasons:

1. It would add roughly 1180 dangling references to a 395-entry baseline, most
   of them unreviewed. A baseline nobody read is an unverified claim, and the
   lint's own design note records that the corpus once carried 1.6k references
   resolving nowhere, which is exactly why findings are baselined rather than
   failed on.
2. `missions/` contains archived and completed missions. Those are historical
   record, and a citation that was correct when written may be worth leaving
   alone, in the same way an RFC version history is.
3. Which surfaces are gated is a policy decision about what this repository
   promises to keep true over time, and it belongs to the maintainers rather
   than to whoever finds the gap.

## A note on the numbers

The figures in the table are now produced by `scripts/measure_anchor_lint.py`,
which imports the linter and calls its own `ANCHOR`, `is_section_anchor` and
`resolves`. The original figures came from a hand-rolled copy of that loop and
were close but not exact.

Two scanner disagreements have now been recorded in this repository, and they
resolved in opposite directions. An independent scanner once reported 920 and
1,207 dangling references for `docs/` and `missions/` against 553 and 627 here;
it was wrong, because it did not know the linter's reverse-prefix rule. The
simulation behind the figures in the widening mission went the other way and
was too low, because it dropped the numeric-anchor gate and replaced `resolves`
with a plain membership test.

Both errors are the same mistake: reimplementing the resolution logic instead of
calling it. The measured figures are 538 for `docs/` and 644 for `missions/`.

## Acceptance Criteria

- [ ] The two CLAUDE.md pointers resolve, or are removed — a reader following
      either today is sent to a section that does not exist
- [ ] A decision is recorded on which surfaces the anchor lint gates, with the
      archived-mission question answered separately from the live ones
- [ ] If `docs/` and `missions/` are gated, the new findings are reviewed by
      class rather than absorbed into the baseline wholesale
- [ ] The lint's own header describes the set it indexes and the set it scans,
      since the mismatch between them is the whole finding and is currently
      invisible in the source

## Method note

The two CLAUDE.md pointers were checked by listing every heading in
BLUEPRINT.md and searching it, not by assuming the citation was right or wrong
from the shape of the prose. Both were confirmed absent, and the nearest real
heading was identified so the choice of replacement stays with the maintainer
rather than being guessed at here.
