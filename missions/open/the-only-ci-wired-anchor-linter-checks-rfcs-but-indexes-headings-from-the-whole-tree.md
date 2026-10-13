---
name: the-only-ci-wired-anchor-linter-checks-rfcs-but-indexes-headings-from-the-whole-tree
description: "scripts/lint-rfc-anchors.py builds its heading index from rfcs, docs, missions and CLAUDE.md, but its dangling-anchor scan only walks rfcs. A section reference that is plainly wrong in docs, in a mission, or in CLAUDE.md is never checked. Proven by planting the identical bogus anchor in both trees: it is reported in rfcs and silently passes in docs with exit 0. That is roughly 6370 multi-word anchors outside rfcs that no CI gate inspects."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-only-ci-wired-anchor-linter-checks-rfcs-but-indexes-headings-from-the-whole-tree`

## Status

Partially resolved. `CLAUDE.md` is now scanned as well as indexed, and its
three dangling anchors are fixed. The wider `docs/` and `missions/` scope
decision is untouched and remains a maintainer's.

Nothing in `.github/` was changed.

## The finding

`scripts/lint-rfc-anchors.py` is one of only two check scripts wired into CI,
and it is the most trusted gate in the repository. It resolves section anchors
against a heading index built from four sources:

```python
INDEX_SOURCES = [RFC_ROOT, ROOT / "docs", ROOT / "missions", ROOT / "CLAUDE.md"]
```

It then checks citations only inside `rfcs/`:

```python
for path in rfc_files:      # rfc_files = sorted(RFC_ROOT.rglob("*.md"))
    for lineno, line in enumerate(path.read_text(...)):
        for raw in ANCHOR.findall(line):
```

So the linter knows every heading in the repository, then verifies references
in about a third of it.

## Proven, not inferred

The identical sentence was planted in both trees:

```markdown
See §Totally Bogus Section Name here.
```

| Location                    | Result                       |
| --------------------------- | ---------------------------- |
| `rfcs/_tmp_anchor_probe.md` | reported, with file and line |
| `docs/_tmp_anchor_probe.md` | **not reported, exit 0**     |

Both probe files were removed immediately afterwards. The control matters: the
anchor was not skipped because it was somehow valid, and it was not skipped
because the linter was not running. The same string, the same run, two
different verdicts decided purely by directory.

## Scale of the blind spot

Multi-word section anchors, the class `is_section_anchor` treats as real
references rather than review-finding ids:

| Tree        | Multi-word anchors |
| ----------- | ------------------ |
| `rfcs/`     | 3,714              |
| `docs/`     | 2,417              |
| `missions/` | 3,953              |
| `CLAUDE.md` | 6                  |

**6,370 anchors sit outside the checked tree.** `missions/` is the second
largest population and is written by exactly the agents this repository's
process is built for.

## Why this matters more than the other coverage gaps

The other unwired linters are unwired, so nobody relies on them. This one is
in CI, green on every run, and the natural place a reader would look to check a
citation. A green run currently means "no _new_ dangling anchors inside
`rfcs/`", which reads as "no dangling anchors".

`CLAUDE.md` is the sharpest case. It is the first file any agent reads, and it
carried three dangling references, none of which resolved to any heading:

- `§The RFC Process` cited a BLUEPRINT.md section that does not exist. The
  heading is local to CLAUDE.md, so the fix points at the local section.
- `§Multi-Mission Decomposition` exists in no document at all. The content the
  sentence describes lives under `§Mission Lifecycle` and
  `§Consistency Checklist`, which is what it now cites.
- `§RFC Reference Conventions Reaffirmed` missed its heading because the
  heading carries a parenthesised suffix the normaliser does not strip.

All three are fixed and `CLAUDE.md` is now scanned. Planting an invented anchor
in the file is reported with a line number and exits 1.

Related, recorded separately in mission
`anchor-lint-coverage-gap-and-two-broken-pointers-in-claude-md`.

## Note on the other wired linter

`scripts/lint-rfc-type-refs.py` also walks `rfcs/` only. That one is narrower
by design: it resolves Rust and Cairo symbols appearing inside RFC code
blocks, so there is nothing for it to check outside `rfcs/`. It is not the
same defect. The distinction is that the anchors linter builds its index from
four trees and then reads from one.

## Acceptance criteria

- [ ] A decision is recorded on the anchors linter's scan scope: `rfcs/` only,
      or the trees it already indexes.
- [ ] If widened, the newly checked anchors are measured before adoption. The
      6,370 figure is a population count, not a finding count, and widening
      the scope will surface findings that then need baselining deliberately.
- [x] `CLAUDE.md` included in the scan scope regardless of the wider
      decision. Done, together with fixing its three broken references.
- [ ] The summary states which trees were scanned, so "no new dangling
      anchors" cannot be read as a repository-wide claim.

## Notes

This is the fourth instance of the same shape recorded in mission
`four-linters-report-clean-while-their-counters-describe-a-different-population`:
a gate whose verdict is true about a population narrower than the reader
assumes. The two wired linters print corpus size, which is what made this one
auditable at all — the counts above came from reading its output next to a
direct count of the repository.
