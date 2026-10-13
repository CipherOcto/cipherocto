---
name: the-status-version-sync-guard-compares-one-file-in-seven-and-its-no-vh-branch-can-never-fire
description: lint_status_vh_sync.sh read the document version off the ## Status HEADING LINE, which carries a version 0 times in 246 files. 210 of 247 files were reported "no Status version" and skipped, so the guard compared 37 files and reported drift on 1. Fixing the read exposed two further defects: the Version History row scan ran to end-of-file and read later unrelated tables, and a cross-reference like RFC-0012-v2 was read as this document's version. All four defects are fixed and mutation-verified; 9 findings are now reported where 1 was, and every one has been read individually.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0855p-c
---

# Mission `the-status-version-sync-guard-compares-one-file-in-seven-and-its-no-vh-branch-can-never-fire`

## Status

Four defects found and fixed in `scripts/lint_status_vh_sync.sh`. The guard now
reports **9 findings where it reported 1**, and all 9 have been read individually
and confirmed to be real document drift. The corpus was not modified.

The mission remains open on one point: the 9 findings need an owner decision
per file, because each has at least two correct resolutions.

## The defect

`lint_status_vh_sync.sh` exists to compare an RFC's Status version against the
latest row of its Version History. It read the Status version like this:

```bash
status_line=$(grep -m1 -E '^## (Status|## Status|Status:)' "$rfc_file")
status_version=$(grep -oE 'v[0-9]+(\.[0-9]+)*' <<< "$status_line" | head -1)
```

That first `grep` returns the heading **line**. This corpus writes the version on
the lines _after_ it:

```markdown
## Status

Accepted (v1.1.0) — promoted 2026-02-01
```

Measured over the whole corpus: the matched heading line carries a version
**0 times in 246 files**. The extraction ran against a string that never
contained a version.

The guard still reported anything at all only because of a YAML `version:`
fallback, which reaches the 25 Accepted RFCs that carry one. Everything else fell
through:

| Counter             | Before | After | What moved                     |
| ------------------- | ------ | ----- | ------------------------------ |
| `IN_SYNC`           | 33     | 65    | files actually compared        |
| `NO_STATUS_VERSION` | 210    | 160   | files the guard could not read |
| `OUT_OF_SYNC_*`     | 1      | 9     | findings                       |

210 of 247 files were reported as having no Status version — including RFC-0903,
whose Status reads `Final (v35 - Stoolap compatibility)` in plain sight.

## Why the sibling guard disagreed

`lint_no_vh_accepted.sh` has always read the Status **block**, and was corrected
to do so earlier. `lint_status_vh_sync.sh` reads the heading **line**. Two guards
implementing adjacent rules over the same corpus disagreed about the same
documents, and neither noted the other's answer.

A guard that cannot reach its own counter is not a guard. The `NO_VH` branch made
this concrete: its test was `^## (Status|...).*Accepted`, requiring "Accepted" on
the heading line — again 0 occurrences corpus-wide. `NO_VH` was structurally `0`,
not measured `0`. Fixed and proven reachable with a fixture; on the real tree it
reports `0` again, now corroborated by `lint_no_vh_accepted.sh` independently
reporting `155 Accepted RFCs, all with Version History`.

## Three further defects, each found only after the first was fixed

Fixing the read surfaced 23 findings. Reading them showed most were artifacts of
_my_ fix, which is the part worth recording.

**Version History read to end-of-file.** `found` latched on at the heading and
never cleared, so both row readers scanned the rest of the document. RFC-0011-w
has a Version History of `| 1.0 |` and `| 1.1 |`, then ~900 lines later a CLI
error-code slot table running `| 79 |` … `| 91 |`. The guard took `91` as the
latest version and reported `Status=v1.1, VH latest=v91` for a document that is
correctly in sync. The table now ends at its last row.

**Cross-references read as own-version.** RFC-0012 and RFC-0014 open their Status
by naming what they amend — `... of RFC-0012-v2 + RFC-0014-v3`. The extractor took
the first `v`-token on the line and reported `Status=v2` against a Version History
whose latest row is `3.0.0`. Two of the findings were another document's version.

**The heading line is the trap in any filter on this block.** `grep -A5` emits the
matched heading as its first output line. A filter dropping blanks, `>` asides and
`|` rows leaves `## Status` at the head of the list; `head -1` returns the heading,
which by construction never carries a version. This reproduced the original defect
exactly, behind a filter that looked correct. My first two attempts hit this, and
the counter reading `NO_STATUS_VERSION=210` again is what exposed it — the filter
"worked" and did nothing.

## Verification

Each defect mutation-verified in both directions, so a reintroduction is caught
rather than assumed caught:

| Mutation                             | Symptom reproduced                                      |
| ------------------------------------ | ------------------------------------------------------- |
| Status read reverted to heading-only | `NO_STATUS_VERSION` 160 → 210, `IN_SYNC` 65 → 33        |
| Table-boundary `exit` removed        | findings 9 → **53** (44 bogus reappear)                 |
| Cross-reference exclusion removed    | `0012`/`0014` artifacts reappear, 9 → 11                |
| `NO_VH` reverted to heading-only     | branch goes dead, `NO_VH=0` on a fixture that must fail |

Positive and negative directions both tested on fixture corpora via `RFC_ROOT`:
a file whose Status and Version History agree passes; one that genuinely drifts
fails. The guard reads its corpus root from the environment, so this costs
kilobytes and touches no tracked file — worth knowing on a full disk.

**A green aggregate proved nothing here.** After the first and second fixes
together, the summary read `OUT_OF_SYNC_STATUS_OLDER=0, OUT_OF_SYNC_STATUS_NEWER=1`
— identical to the pre-existing output. Two bugs cancelled. Only reading the
individual findings showed 23 problems where the counter said 1.

## Why the 9 findings were not fixed here

Each is real drift, and each has at least two correct resolutions:

| RFC                | Reported                    | Resolutions                                    |
| ------------------ | --------------------------- | ---------------------------------------------- |
| 0855p-d1 / d2 / d3 | Status v1.3, VH v1.4        | bump Status, or the v1.4 row is not this RFC's |
| 0855p-e            | Status v1.2, VH v1.3        | as above                                       |
| 0862               | Status v1.1.0, VH v1.2.0    | as above                                       |
| 0205 / 0206        | Status v2.0 / v2.4, VH v1.8 | VH is missing rows, or Status is ahead         |
| 0855p-c            | Status v1.0.0, VH v0.1.4    | Status rewritten later, VH never updated       |
| 0947               | Status v2, VH v4            | same                                           |

Choosing is an author-intent question, not a mechanical one. Guessing would trade a
visible defect for an invisible one, so the corpus is untouched.

## Follow-ups not taken

- The other five numbered guards are still unverified by mutation. See
  `seven-numbered-guards-exist-and-none-of-them-runs-in-ci`.
- `NO_STATUS_VERSION=160` remains. Those documents genuinely write no version in
  their Status block; whether that should be a finding is a policy question, and
  the guard currently reports it without failing. Making it blocking would gate
  the repository on 160 documents.
- No guard runs in CI. See the CI-wiring proposal in
  `seven-numbered-guards-exist-and-none-of-them-runs-in-ci`.
