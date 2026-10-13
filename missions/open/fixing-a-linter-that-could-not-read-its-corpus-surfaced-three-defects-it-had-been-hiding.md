---
name: fixing-a-linter-that-could-not-read-its-corpus-surfaced-three-defects-it-had-been-hiding
description: "lint_status_vh_sync.sh reported its corpus as 135 documents with no Version History rows, and all 135 had rows. It also skipped every RFC whose Version History heading is numbered or section-signed, so those files were never examined at all. Fixing the reader cut NO_VH to 0, raised IN_SYNC from 15 to 33, and exposed one genuine drift that had been invisible: RFC-0855p-c declares version 1.0.0 against a history whose latest row is 0.1.4. Two further defects are recorded here and are not mine to fix: RFC-0001 is filed under rfcs/draft while its Status reads Accepted, and lint_plan_yaml_vh_sync.sh examines a directory outside the repository."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `fixing-a-linter-that-could-not-read-its-corpus-surfaced-three-defects-it-had-been-hiding`

## Status

Open. The linter is fixed and mutation-tested. Three findings it was hiding
are recorded below; one is a real RFC inconsistency, two are author or owner
decisions.

## What the guard claimed

`scripts/lint_status_vh_sync.sh` reported:

```text
NO_VH=135        ## Version History exists but no data rows
IN_SYNC=15
```

Checked by hand, all 135 had data rows. The guard was reading a rendering the
corpus does not use.

## Five reader defects, each measured

| #   | Defect                                                         | Effect                                                         |
| --- | -------------------------------------------------------------- | -------------------------------------------------------------- |
| 1   | `vh_line_num` matched only `^## (Version History\|Changelog)$` | **Numbered and section-signed sections were skipped entirely** |
| 2   | The row scan required a space and pipe right after the version | every column-aligned table read as empty                       |
| 3   | The row scan required at least one dot in the version          | `\| 12 \|` and `\| v5 \|` read as empty                        |
| 4   | Only table rows were considered                                | 27 bullet-rendered histories read as empty                     |
| 5   | Versions were compared as strings                              | `2.14` and `2.14.0` reported as different                      |

Defect 1 is the serious one. `vh_line_num` empty sent the per-file loop into
its `continue` before any comparison, so the guard printed **nothing** for
those files. It was not reporting them clean; it was not reading them. That is
the same defect as the empty-corpus guard already added at the foot of this
script, arriving by a different route: a guard that verifies nothing and
reports nothing in the same shape as one that verifies everything.

## Result

| Measure               | Before | After |
| --------------------- | ------ | ----- |
| `NO_VH`               | 135    | 0     |
| `IN_SYNC`             | 15     | 33    |
| `OUT_OF_SYNC`         | 1      | 1     |
| `NO_VH_VERSION` (new) | —      | 12    |

The one remaining out-of-sync finding is **genuine**. The previous single
finding was not: it was `2.14.0` against `2.14`, the same version written two
ways. The guard now fails for a real reason instead of a false one.

`NO_VH_VERSION` is a new informational class for histories that carry dated
rows but no version column, so there is nothing to compare a Status version
against. Twelve files, all in that shape.

## Mutation-tested in both directions

A relaxed pattern can make a guard too permissive, which is the opposite
failure and equally useless.

| Mutation                                     | Result                            |
| -------------------------------------------- | --------------------------------- |
| frontmatter version on an in-sync RFC bumped | `OUT_OF_SYNC_STATUS_NEWER` raised |
| Version History table emptied                | `NO_VH_ROWS` raised               |
| file restored                                | silent again                      |

The first mutation attempt did nothing, because the file's version came from
frontmatter rather than the Status block and the wrong line was edited.

## A bug of my own, caught only by tracing

The version-normalising helper aborted under `set -u` on exactly the
two-component versions it existed to handle. `${parts[$i]%%[-+]*}` trips
`unbound variable` on an unset element, and this script runs `set -euo
pipefail`. It aborted silently on `2.14`, printed to stderr where the report
does not show it, and left `RFC-0110` still failing after the fix meant to
clear it.

It passed a standalone test because the test did not have `set -u` active. It
surfaced only under `bash -x`, which is an argument for tracing a result that
should have been obvious: the numbers said the fix worked, and they said the
opposite in the same run.

## Finding 1: RFC-0855p-c is genuinely out of sync

`rfcs/accepted/networking/0855p-c-domain-coordinator-role.md` declares
`version: 1.0.0` in frontmatter while the latest row of its Version History
is `0.1.4`. Invisible until defect 1 was fixed, because its section is
numbered.

Which side is wrong is an author decision, and measuring the corpus narrowed it
without settling it.

Only **36** RFCs carry a frontmatter `version:` field at all. Of the 34 whose
Version History has parseable version rows, **32 agree** with it. So the field
is maintained, and this document is an outlier rather than one of a class.

That makes three readings, not two:

| Reading                                  | Evidence for                                                                                                  | Evidence against                     |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| frontmatter drifted                      | its `updated:` is 2026-08-22, earlier than the 0.1.4 row dated 2026-09-03, so the amendments never touched it | the mismatch predates the amendments |
| 1.0.0 is intended, history is incomplete | the field said 1.0.0 from creation                                                                            | no 1.0.0 row was ever written        |
| 1.0.0 is a typo for 0.1.0                | the history starts at 0.1.0 on the creation date                                                              | nothing explains the `1`             |

The third is the one that becomes most likely once the creation date is read
alongside it: the frontmatter has said 1.0.0 since 2026-06-16, and the history
began at 0.1.0 the same day. So this is not drift introduced by the amendments;
the two numbers were never reconciled.

That is as far as the evidence goes. Choosing among the three is the author's,
and each produces a different document.

## Finding 2: RFC-0001 is filed under draft while its Status reads Accepted

`rfcs/draft/process/0001-mission-lifecycle.md` carries `## Status` /
`Accepted`. There is no other `0001` in the tree, so this is not a duplicate
needing reconciliation; the file is simply in the wrong directory for its
status. Moving an RFC between directories is author work.

## Finding 3: the plan guard examines a directory outside the repository

`scripts/lint_plan_yaml_vh_sync.sh` defaults to `/home/mmacedoeu/.claude/plans`.
It examines 6 files, all of them session scratchpads outside CipherOcto, and
reports them as the project corpus:

```text
Plan root: /home/mmacedoeu/.claude/plans
CHECKED=6
NO_VH_NO_YAML=6
```

It is wired against nothing in the repository and its findings are about the
agent's own working files. Either it should point at a project directory or it
should not be counted as a project guard. Not changed here, because the
intended corpus is a decision for a maintainer.

## Acceptance criteria

- [x] Every reader defect measured against the corpus rather than assumed.
- [x] Prove the fixed guard still detects real drift, and still detects an
      empty history.
- [x] Make the comparison numeric, and make the helper safe under `set -u`.
- [x] Separate "has no rows" from "has no version column", so the second is not
      reported as the first.
- [ ] Decide whether RFC-0855p-c is at 1.0.0 or 0.1.4, and record which.
- [ ] Move RFC-0001 to the directory its status implies.
- [ ] Decide what corpus `lint_plan_yaml_vh_sync.sh` is for.
- [ ] Decide whether this guard and `lint_no_vh_accepted.sh` should be wired
      into CI. Both are unwired, and both now report a single accurate finding
      or none.

## Notes

The useful generalisation is not about YAML. It is that a check reading a
document through a regex has two independent failure modes: it can report a
defect that is not there, and it can be so narrow that it never reaches the
document at all. The first is visible and gets reported as noise. The second
is invisible, and produces a clean result indistinguishable from correctness.

This script had both, in the same run, and its summary line made the pair look
like one healthy number.
