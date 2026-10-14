---
name: a-thousand-stale-version-pins-are-ninety-nine-percent-real-but-the-linter-reports-the-wrong-version
description: >-
  validate_cites.sh reported 1007 STALE version-pin findings, naming a "latest" version read as the first version-shaped table cell anywhere in a file. Repaired: the reader now finds the Version History section, takes its rows and keeps the highest. The backlog moved to 1028, and every one of the 1028 is genuine, the cited version being absent from that document's own Version History. An earlier classification put 21 of them at false positives because it reused the linter's own file-wide suppression test, which had been hiding them.

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

After the repair, re-classified directly against each document's Version
History rather than against the linter's verdict:

| Count | Class                                                                          |
| ----- | ------------------------------------------------------------------------------ |
| 1028  | **genuine** — the cited version is absent from that document's Version History |
| 0     | false positive                                                                 |

**The entire backlog is real.** Unlike the `INVALID` and `PHANTOM` classes,
this one is not inflated by a measurement error.

**An earlier figure in this file said 997 genuine and 7 false positives. That
was wrong, and the error is worth recording.** The classification was produced
from the linter's own output, and the linter's second defect — a file-wide "is
this version anywhere in the document" test — was suppressing precisely those 21. Measuring a tool against the tool's verdict inherits the tool's blind spots,
so the classification had to be redone against the Version History directly.

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
not change in kind**: the findings are genuine, and a correct reader reports
them with an actionable version.

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

## Two fixture mistakes, both of which looked like linter bugs

The repaired reader was tested against a fixture RFC numbered `0001` and then
`0900`, each placed in a temp directory. Both times the run reported **no
finding**, which reads as the repair having failed.

Neither was the repair. `0001` and `0900` are real RFC numbers in this
repository, and the citation resolver walked up from the repository root and
found the _real_ file — `rfcs/draft/process/0001-mission-lifecycle.md` and
`rfcs/accepted/economics/0900-ai-quota-marketplace.md` — while the temp fixture
sat unused beside it. The resolver was reading the repository, not the fixture.

The trace that settled it printed `rfc_path=` on its own line, naming the
repository file. Nothing in the summary said which document had been checked.

A second fixture then made the opposite mistake: it cited `v1.0` against a
Version History containing `1.0` and `2.0`, and expected a stale-pin finding.
`v1.0` **is** a row in that table, so not reporting it is correct. The test
needed a pin the table never had, not one it did.

The lesson is the same one the mutation harness exists to enforce, applied to
fixtures: a test that fails for a reason other than the defect under test will
send you repairing the wrong thing. Both times the failure was resolvable only
by reading what the code actually touched, not what it was asked to touch.

## What remains for a maintainer

The 1028 genuine findings need the author-intent decision the whole citation
backlog needs: bump the pin, or accept that the pin refers to the version whose
section is being cited. That is the question in
`one-thousand-nine-hundred-rfc-references-carry-a-version-pin-and-only-a-fifth-are-cross-references`,
and this mission does not answer it.

The reader repair is ready to apply and changes no verdict.
