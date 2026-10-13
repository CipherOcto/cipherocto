---
name: one-rfc-has-two-version-history-tables-claiming-the-same-version
description: "rfcs/accepted/numeric/0114-deterministic-activation-functions.md carries two Version History sections. The first has 14 rows reaching version 2.12, the second has 2 rows that also start at 2.12 with different content. A reader cannot tell which is authoritative, and no linter looks: the accepted-VH linter reads the first heading it finds and the anchors linter does not examine tables. A corpus census found exactly one file with conflicting duplicate tables and one with a benign duplicate."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `one-rfc-has-two-version-history-tables-claiming-the-same-version`

## Status

Open. Nothing merged, nothing deleted. Choosing which table is authoritative
is author knowledge, not a mechanical repair.

## The finding

`rfcs/accepted/numeric/0114-deterministic-activation-functions.md` has two
sections headed Version History:

| Heading              | Line | Rows | Versions         |
| -------------------- | ---- | ---- | ---------------- |
| `# Version History`  | 639  | 14   | 1.0 through 2.12 |
| `## Version History` | 658  | 2    | 2.12, then 1.0   |

Both claim version **2.12**, with different content. The 14-row table's 2.12
reads "Advisory review fixes: Issue 1-4 documentation clarity". The 2-row
table's 2.12 reads "Status block: v2.12 (2026-03-19) — DQA decimal
fixed-point LUT activation functions." And the second table additionally lists
1.0 _after_ 2.12, which is not a version order any reader should have to
interpret.

A reader who wants to know what changed in 2.12 gets two answers and no way to
tell which is current.

## Why no gate sees it

`lint_no_vh_accepted.sh` asks whether an Accepted RFC has a Version History
with at least one data row. This file answers yes twice over. Its `found`
flag in the row check is set on the first heading and never reset, so it
scans to end of file — the check cannot distinguish one good table from a good
table plus a contradictory one.

`lint-rfc-anchors.py` reads headings and references, not table contents. Two
sections with the same name are not a dangling anchor.

The 33 empty-Version-History findings recorded in
`six-accepted-rfcs-have-no-version-history-after-three-linter-fixes`
are the opposite failure — a section with no rows. A section with the wrong
rows is invisible to both.

## Census

Scanned every RFC for more than one Version History or Changelog heading. Two
files, and only one is a conflict:

| File                                         | Duplicate | Verdict                                             |
| -------------------------------------------- | --------- | --------------------------------------------------- |
| `0114-deterministic-activation-functions.md` | yes       | **conflict**, both tables populated and overlapping |
| `0011-octo-cli-substrate.md`                 | yes       | benign                                              |

`0011-octo-cli-substrate.md` has a `## Changelog` whose version column is
written `v1.0`, `v1.1` rather than `1.0`, `1.1`, and a separate
`## Version History` 1,200 lines later with 8 rows. It initially looked like
a second instance of the same defect. It is not: the linter's row pattern
excludes the `v` prefix, so it finds no rows under the Changelog, keeps
scanning because its `found` flag is never reset, and reaches the real table.
The verdict is correct by accident rather than by design, which is worth
recording.

## Acceptance criteria

- [ ] The 14-row and 2-row tables in RFC-0114 are reconciled. Whichever is
      authoritative, the other is removed rather than left to be read.
- [ ] If the 2-row table carries content the 14-row table lacks, it is merged
      rather than discarded.
- [ ] The result keeps one Version History section per RFC. Two sections with
      one name is not a format this repository supports.
- [ ] Considered: whether a check should report duplicate section names
      within a file. That is a different rule from the existing one and is
      not implied by it.

## Notes

This was found while verifying a claim about heading shapes rather than by
auditing version history. Census before filing mattered: the obvious second
candidate turned out to be benign, and the linter passed it for a reason
nobody designed — a `found` flag that is set once and never cleared. Both
facts are recorded so the next reader does not re-derive them.

A `v`-prefixed version column is a separate matter from a numbered section
heading, which was the shape fixed in
`six-accepted-rfcs-have-no-version-history-after-three-linter-fixes`.
Both are the same underlying habit: the corpus decorates its headings and
columns in ways the patterns do not anticipate.
