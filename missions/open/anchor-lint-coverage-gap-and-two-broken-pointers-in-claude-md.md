---
name: anchor-lint-coverage-gap-and-two-broken-pointers-in-claude-md
description: "The RFC anchor lint resolves references only inside rfcs/, which is 3752 of 9829 in-scope section references, or 38 percent. docs, missions and CLAUDE.md are indexed as heading sources but never scanned as reference sites, so 6077 references are unexamined. The cost is already visible: CLAUDE.md points readers at two BLUEPRINT.md sections that do not exist, §The RFC Process and §Multi-Mission Decomposition, and nothing in the toolchain can see it."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `anchor-lint-coverage-gap-and-two-broken-pointers-in-claude-md`

## Status

Open. No gate has been re-scoped and no citation has been rewritten.

## The finding

`scripts/lint-rfc-anchors.py` does two separate jobs. It builds an index of
resolvable headings from several sources, then walks a _different_, narrower set
of files looking for references. The two sets do not match, and the gap is where
the real damage is.

Indexed as heading sources: `rfcs/`, `docs/`, `missions/`, and `CLAUDE.md`.
Scanned for references: `rfcs/` only.

| Surface     | In-scope references | Dangling | Checked today |
| ----------- | ------------------- | -------- | ------------- |
| `rfcs/`     | 3752                | 593      | yes           |
| `docs/`     | 2452                | 553      | **no**        |
| `missions/` | 3618                | 627      | **no**        |
| `CLAUDE.md` | 7                   | 2        | **no**        |

**Coverage is 38 percent.** 6077 references are never examined. The figures
come from the linter's own matcher and resolver, so they are measured with the
same semantics the tool applies rather than with a second implementation that
could disagree with it.

## What the gap already costs

`CLAUDE.md` is the file every agent reads before anything else, and it names
BLUEPRINT.md as _the operational playbook_. Two of its pointers into that
playbook name sections that are not there:

- It directs the reader to `docs/BLUEPRINT.md` §The RFC Process for the full
  RFC lifecycle. BLUEPRINT.md has no such heading. The nearest is
  `## RFC Acceptance Process`.
- It directs the reader to `docs/BLUEPRINT.md` §Multi-Mission Decomposition for
  the decomposition thresholds. BLUEPRINT.md contains no heading matching
  _Decomposition_ at all.

The third pointer on that line, §Mission Lifecycle, does resolve, and so does
§RFC Process elsewhere in the corpus, which is why the failure is not obvious on
read: most of the file's references land, and the two that do not look like
typos.

This is the exact case the tool exists to catch, and it is invisible to it,
because CLAUDE.md is indexed as a heading source and never visited as a
reference site.

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

Every figure above was produced by importing the linter and calling its own
`ANCHOR`, `is_section_anchor`, and `resolves`. That matters: an earlier pass at
this measurement used an independently written scanner and reported 920 and
1207 dangling references for `docs/` and `missions/`, against 553 and 627 here.
The independent scanner was wrong, mostly because it did not know the linter's
reverse-prefix rule, which resolves an anchor that names a heading plus extra
prose. The smaller figures are the trustworthy ones, and they are still large
enough to matter.

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
