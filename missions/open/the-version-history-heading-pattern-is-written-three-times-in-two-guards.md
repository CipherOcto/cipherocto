---
name: the-version-history-heading-pattern-is-written-three-times-in-two-guards
description: "The Version History heading alternation exists in three places: VH_HEADING in lint_status_vh_sync.sh, and twice more in lint_no_vh_accepted.sh for the presence check and the row reader. Each copy carries its own comment history and they have silently diverged in the past. The section-sign-before-number shape was missing from all three at once, so both guards were blind to the same two files. Adding a shape means editing three places and missing one is silent."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-version-history-heading-pattern-is-written-three-times-in-two-guards`

## Status

Open. One missing shape is fixed in all three copies. The duplication that let
the same shape be missing from all three at once is not.

## The finding

The alternation that recognises a Version History heading is written out three
times:

| Location                              | Role                         |
| ------------------------------------- | ---------------------------- |
| `lint_status_vh_sync.sh` `VH_HEADING` | locate the section           |
| `lint_no_vh_accepted.sh`              | presence check               |
| `lint_no_vh_accepted.sh`              | row reader, in an `awk -v h` |

Each carries its own comment describing its own history. `lint_no_vh_accepted.sh`
records that it was taught the signed and numbered shapes after reporting
Accepted RFCs with populated Version Histories as having none. That fix was
written into its two copies. It was not written into `lint_status_vh_sync.sh`,
which reached its own version of the same fix separately.

**A shape was then missing from all three at once.** `## §6 Version History` —
section sign before the number — matched none of them. The status guard's
`vh_line_num` came back empty, its NO_VH branch counted only Accepted files, and
the loop reached a `continue` having incremented nothing. Two files in the
corpus write that form. Neither guard read either file's Version History.

The status guard found this only because it now reconciles its population and
names what it cannot place. The no-VH guard had no such check, and a blindness
it shares with the other guard is invisible to both.

## Why the duplication is the defect

Not the missed shape. That is fixed and was worth one edit in three places.

The duplication is what makes the next shape cost three edits, with no compiler
and no test to notice the one that was forgotten. A guard that cannot read a
heading reports nothing about the file, so the omission is indistinguishable
from a clean corpus from the outside. That is the same failure this repository's
document guards keep exhibiting, and here it is present in the mechanism meant to
prevent it.

## Acceptance criteria

- [x] A case that fails when the copies disagree. `vh_heading/all-four-shapes`
      writes one Accepted RFC per heading shape into a single fixture and
      asserts that both guards read all four. Reverting either guard's copy
      alone makes it fail, in both directions, which is verified by mutation.
      It is deliberately not shape-specific: it does not enumerate the shapes,
      so a shape neither guard has seen yet still has to be readable for the
      case to pass.
- [x] The shared fixture covers all four shapes in one corpus. The per-shape
      cases remain alongside it, because the all-shapes case reports that a
      shape failed without saying which, and a guard that can read three of
      four is easier to diagnose from a named case.
- [ ] One definition of the alternation, sourced by both guards rather than
      written three times. It must cover the four shapes the corpus uses, and
      the comment that names them lives with it. The disagreement is now
      detectable rather than silent, which lowers the cost of leaving the
      duplication in place but does not remove the three edits.
- [x] `lint_no_vh_accepted.sh` reports its unplaced files rather than absorbing
      them. `NOT_ACCEPTED` is counted on the path that skips non-Accepted RFCs
      rather than derived as a remainder of the scanned count, and the summary
      asserts that the three groups add back up to the population. A file
      skipped by a path that increments nothing can no longer be absorbed into
      an ordinary non-Accepted Draft. The comment standing beside the summary
      claimed a reconciliation that did not hold — it read "155 plus 91 is
      246, and every file in the corpus lands in exactly one of those two
      groups" against a corpus of 247 with one file in a third group — and is
      corrected to say why the assertion is there.

## Notes

The fix and this mission were found in the same session. Fixing the shape
without recording the duplication would leave the cost exactly where it was,
with the added confidence that the shape had been checked.
