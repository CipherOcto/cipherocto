---
name: a-thousand-stale-version-pins-are-ninety-nine-percent-real-but-the-linter-reports-the-wrong-version
description: >-
  validate_cites.sh reports 1007 STALE version-pin findings. Re-measured against the true Version History latest row: 997 are genuine, 7 are false positives, and 3 cite a document with no Version History at all. The linter also computes its "latest" as the FIRST pipe-table version anywhere in the file, so for 41 of 163 RFC numbers it names a version that is not the latest, and 529 of the findings point at one file where it reports v0.1 as latest when the table runs 0.1 through v2.4.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references
---

# Mission `a-thousand-stale-version-pins-are-ninety-nine-percent-real-but-the-linter-reports-the-wrong-version`

## Status

Classified. The findings are real; the linter's stated reason for each one is
not. Both halves are recorded here because they lead to opposite decisions.

## The measurement

From a full `bash scripts/validate_cites.sh` run:

```
Summary: CHECKED=29273 VALID=26627 PHANTOM=339 INVALID=1300 STALE=1007
```

The 1007 `STALE [version pin mismatch]` findings, compared against the actual
latest Version History row of each cited document:

| Count | Class                                                       |
| ----- | ----------------------------------------------------------- |
| 997   | **genuine** — the cited version is behind the true latest   |
| 7     | **false positive** — the cited version _is_ the true latest |
| 3     | the cited document has no Version History section at all    |

**99% of the backlog is real.** Unlike the `INVALID` and `PHANTOM` classes,
this one is not inflated by a measurement error.

## The linter names the wrong version

`vh_latest` is computed as:

```bash
grep -oE '\|[ \t]+[0-9]+(\.[0-9]+)+[ \t]+\|' "$rfc_path" | head -1
```

Two problems, both already repaired in sibling guards and neither repaired here:

- `head -1` takes the **first** version anywhere in the file, not the latest
  Version History row. A Version History is append-ordered oldest-first, so the
  first row is the oldest entry.
- The grep is **file-wide**, so a version-shaped table that is not a Version
  History — a phase table, a migration table, a deprecation matrix — is read as
  one.

Measured: for **41 of 163** RFC numbers the linter's "latest" is not the true
latest.

The most concentrated example is one file, `0206-v40-pubsub-extension.md`,
which accounts for **529 of the 1007 findings**. Its Version History runs

```
| 0.1.0   | 2026-08-24 | Initial draft. v2.3 → v2.4 amendment adds ...
```

and the linter reports `latest v0.1` for it — the first row, while the document
itself refers to v2.3 and v2.4 in that same cell.

The other concentrated case, `0009-identity-management.md`, runs `0.1 → 1.2` and
is reported as latest `0.1`; citations to its real `v1.3` and `v1.2` are
therefore flagged against a version that stopped being current long ago.

## Why this was not fixed here

Fixing the reader is mechanical and safe — it is the same change already made to
`lint_status_vh_sync.sh` and `lint_plan_yaml_vh_sync.sh`. But **the verdict does
not change**: 997 findings are genuine, and a correct reader still reports them.

What a correct reader changes is the _message_. Today it says
`cited v1.3, latest v0.1`, which is not a sentence anyone can act on — the
version it names as current does not exist in the table it read. The repaired
reader would say `cited v1.3, latest v1.2`, which is the finding a maintainer
can actually resolve.

So this is filed as a measurement correction rather than a repair, because the
repair improves the report without changing which documents are implicated.

## Two hypotheses tested and rejected

Both were formed from the first classification pass and both were wrong. They
are recorded because the corrected numbers are the useful output, and a later
reader should not re-derive them.

**"561 findings cite a document with no Version History rows."** False. My
heading pattern did not allow a section sign before the number, so documents
headed `## §6 Version History` read as having no Version History at all. With
the pattern corrected, that class is **3 findings**, not 561.

**"The cite regex matches `RFC-` inside a longer identifier."** Tested across
all 339 `PHANTOM` findings and false: **0** match inside a word. The regex is
applied per line and reports the offset within it.

## What remains for a maintainer

The 997 genuine findings need the author-intent decision the whole citation
backlog needs: bump the pin, or accept that the pin refers to the version whose
section is being cited. That is the question in
`one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references`,
and this mission does not answer it.

The reader repair is ready to apply and changes no verdict.
