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

- [ ] One definition of the alternation, sourced by both guards rather than
      written three times. It must cover the four shapes the corpus uses, and
      the comment that names them lives with it.
- [ ] A case that fails when the copies disagree. A mutation harness already
      drives both guards from fixture corpora; what is missing is a case
      asserting that every heading shape in a shared fixture is recognised by
      both. That single case would have caught this defect without either guard
      reporting anything.
- [ ] The shared fixture covers all four shapes in one corpus, so a future
      shape is added in one place and the disagreement case detects a guard
      that did not pick it up.
- [ ] `lint_no_vh_accepted.sh` reports its unplaced files rather than absorbing
      them. Its population already reconciles, but by derivation:
      `NOT_ACCEPTED` is computed as the remainder of the scanned count, so any
      file skipped by a path that increments nothing would be counted as a
      non-Accepted RFC and look entirely legitimate. That is precisely how the
      status guard's six unplaceable files stayed invisible. Deriving the
      remainder is a reasonable choice, provided the remainder is also checked
      against a directly counted figure.

## Notes

The fix and this mission were found in the same session. Fixing the shape
without recording the duplication would leave the cost exactly where it was,
with the added confidence that the shape had been checked.
