---
name: the-status-sync-guard-reported-one-of-its-nine-drifts-about-a-file-that-is-in-sync
description: "The Status/VH sync guard recovered a document's version by taking the first vN.N token on its ## Status line, and reported a drift in RFC-0862, which is in sync — its Status line narrates v1.1.0 then v1.2.0 and its history's latest row is v1.2.0. Eighteen Status lines in the corpus name more than one version, so the pick was a guess presented as a reading. Mutated to take the last token instead, the guard repairs 0862 and breaks nine other files, so no ordering rule holds. The nine drifts reduce to four causes, and a second of them comes from comparing a frontmatter field the RFC template never defines."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-status-sync-guard-reported-one-of-its-nine-drifts-about-a-file-that-is-in-sync`

## Status

Open. The guard now discloses which token it compared and which field it read.
The verdicts it produces are unchanged, and the policy question behind the
frontmatter comparison is unanswered.

## The finding

`lint_status_vh_sync.sh` decides whether a document's `## Status` version agrees
with its `## Version History`. To get the Status version it ran the Status line
through a regex, kept the **first** `vN.N` token that was not part of an
`RFC-NNNN-vX` cross-reference, and reported the result as though the line had
said so.

These Status blocks are prose, and eighteen of them name more than one version.
Which token is the document's current version is not recoverable from the line
in general, so the first token is a guess. The report did not say it was one.

### The drift that was not a drift

RFC-0862's Status line narrates its own amendment history in ascending order:

> Accepted (2026-06-20) — Updated v1.1.0 (2026-06-21): added the
> `DatabaseSyncAdapter` trait boundary … Updated v1.2.0 (2026-06-25): clarified
> bootstrap integration path via RFC-0863 `BootstrapOrchestrator` …

Its Version History's latest row is `1.2.0`. The guard compared the first token,
`1.1.0`, and reported:

```
OUT_OF_SYNC_STATUS_OLDER: rfcs/accepted/networking/0862-stoolap-data-sync.md (Status=v1.1.0, VH latest=v1.2.0)
```

The file is in sync. The guard reported a drift about a document it had read and
misread, which is the same failure this script already carries two comments
about — a `## Status` heading returned by its own filter, and a version lifted
out of another document's number.

### Why "take the last token" is not the repair

The lines disagree about their own direction:

| RFC        | Tokens on the Status line | Reading order |
| ---------- | ------------------------- | ------------- |
| `0963`     | `v3.1`, `v2.0`, `v3.1`    | descends      |
| `0206-v40` | `v0.1.0`, `v1.0`, `v1.0`  | opens oldest  |
| `0862`     | `v1.1.0`, `v1.2.0`        | ascends       |

Mutated to compare the last token, the guard repairs RFC-0862 and breaks nine
others, taking the drift count on this tree from **9 to 17**: `0206-v40`, `0863`,
`0961`, `0962`, `0964`, `0965`, `0967`, `0968-a2` and `0968-a3`. A second guess
would have traded one class of wrong verdict for another.

## What the guard does now

Both guesses are disclosed on the finding rather than resolved by the guard:

```
OUT_OF_SYNC_STATUS_OLDER: rfcs/accepted/networking/0862-stoolap-data-sync.md (Status=v1.1.0, VH latest=v1.2.0; Status line names 2 versions [1.1.0, 1.2.0], compared the first)
```

and, for the second cause, which field supplied the version at all:

```
OUT_OF_SYNC_STATUS_NEWER: rfcs/accepted/networking/0855p-c-domain-coordinator-role.md (Status=v1.0.0, VH latest=v0.1.4; Status version read from frontmatter version:, not the ## Status block)
```

Two counters state how much of the compared population rests on a guess —
`STATUS_MULTI_TOKEN` (18) and `STATUS_FROM_FRONTMATTER` (32 of 76 compared) —
with an assertion bounding their sum by `CHECKED`. Verdict counts are unchanged:
76 compared, 67 in sync, 9 drifts.

The disclosures are mutation-verified. Zeroing the token count, blanking the
frontmatter provenance, and mutating the comparison to the last token each
change the report in the way the comment predicts. Two accept-direction cases
in `mutation-verify-doc-guards.sh` pin them:
`status_vh/discloses-ambiguous-status-line` and
`status_vh/discloses-frontmatter-source`. Each was confirmed to fail against the
guard with its own disclosure removed, and to pass against the guard with only
the _other_ disclosure removed — so neither case can be satisfied by the other.

## The second cause: a field the template does not define

Where a `## Status` block carries no version, the guard falls back to the YAML
frontmatter `version:` key. Two facts make that comparison questionable:

1. **The RFC template defines no frontmatter.** `rfcs/draft/process/0000-template.md`
   opens with `# RFC-XXXX (Category): [Title]` and a bare status word. It names no
   `version:` field. Thirty-six of 247 RFCs carry frontmatter anyway, and all 36
   carry that key.
2. **The key does not reliably track the history table.** Normalising away
   `-r7`-style review markers, `version:` disagrees with the file's own latest
   VH row in **11 of those 36**. RFC-0855p-c declares `version: 1.0.0` against a
   history whose latest row is `0.1.4`, and it is the only one of the nine drifts
   this fallback produces.

So `STATUS_FROM_FRONTMATTER=32` is load-bearing: removing the fallback would move
32 files into `NO_STATUS_VERSION` and drop coverage from 30% to 17%. Whether
frontmatter `version:` is meant to track the VH table, or is a separate concern
that should not be compared against it, is a corpus-policy question.

## The nine drifts are four causes

| Cause                                                         | Files                                                                 | Nature                   |
| ------------------------------------------------------------- | --------------------------------------------------------------------- | ------------------------ |
| Promotion added a VH row; the Status header was not bumped    | `0855p-d1`, `0855p-d2`, `0855p-d3`, `0855p-e` (Status one behind VH)  | document defect, 4 files |
| VH rows missing for every version after the v1.8 supersession | `0205`, `0206`                                                        | document defect, 2 files |
| Status names a version the Accepted move did not reach        | `0947` (Status `v2`, VH `v4`, whose row records the move to Accepted) | document defect          |
| The guard compared the wrong field or the wrong token         | `0862` (wrong token), `0855p-c` (frontmatter)                         | guard artifact           |

RFC-0206 deserves its own note: its Status block states that the header was
"synced with §Version History v2.4 row per M35", and its history table has no
v2.4 row. The document asserts the agreement the guard could not find.

## Decisions for maintainers

1. **Four Status headers** — bump `0855p-d1`, `0855p-d2`, `0855p-d3`,
   `0855p-e` to match their latest VH row. Mechanical, but it edits Accepted RFCs.
2. **Two missing histories** — `0205` and `0206` need VH rows for v2.x. Only the
   author knows what changed between the versions the Status block narrates.
3. **`0947`** — Status is two majors behind; its v4 row records the Accepted move.
4. **Frontmatter policy** — is `version:` meant to track the VH table? If not,
   the fallback should be removed and its 32 files should be reported as
   `NO_STATUS_VERSION` rather than compared against a field that means something
   else.
5. **The multi-token Status lines** — 18 files whose Status block is ambiguous to
   a line-oriented reader. Whether the Status block should be constrained to a
   single version token is a template question, and it is the only fix that would
   let the guard stop guessing.
