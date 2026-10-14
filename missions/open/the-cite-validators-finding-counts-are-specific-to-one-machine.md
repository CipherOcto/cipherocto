---
name: the-cite-validators-finding-counts-are-specific-to-one-machine
description: "validate_cites.sh walks the filesystem but its SCOPE line described tracked files. 711 of the 1133 markdown files under (rfcs docs) are gitignored local scratchpads — docs/reviews 409, docs/audits 197, docs/plans 105 — so the guard examines 1133 files on a developer's machine and 422 in a clean clone. The same guard at the same commit reports 1300 INVALID and 1028 STALE locally and 972 and 460 in a clone. Every finding count this guard prints is a statement about a corpus no CI runner can reproduce, and nothing in the summary said so. The mission set already carries two different figures for this guard, 1300 INVALID and 979, with no explanation of which corpus either was measured over."
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist
---

# Mission `the-cite-validators-finding-counts-are-specific-to-one-machine`

## Status

The corpus divergence is now stated in the guard's own summary. Whether the
guard should scan only tracked files is a maintainer decision and is not made
here.

## The finding

`validate_cites.sh` finds its corpus by walking the filesystem:

```
find "$RFC_ROOT" "$DOCS_ROOT" -maxdepth "$CITE_MAXDEPTH" -type f -name '*.md'
```

Its `SCOPE` line described a different corpus, computed from `git ls-files`:
"422 of 1268 tracked markdown files lie under (rfcs docs); 846 unexamined". A
number about tracked files, printed beside findings drawn from whatever is on
disk.

Those are not the same set of files here.

| Directory                   | Walked   | Tracked by git |
| --------------------------- | -------- | -------------- |
| `docs/reviews/`             | 409      | 0              |
| `docs/audits/`              | 197      | 0              |
| `docs/plans/`               | 105      | 0              |
| `docs/research/`            | 78       | 78             |
| **total under (rfcs docs)** | **1133** | **422**        |

The three gitignored directories are deliberate. `.gitignore` records
`docs/reviews/` and `docs/audits/` as local-only scratchpads never committed,
with a comment noting twelve files in `docs/audits/` were untracked from the
repository on 2026-08-20. Nothing is broken about that. The consequence is what
went unstated.

## What the same guard reports in each place

| Run                            | CHECKED | VALID | PHANTOM | INVALID | STALE |
| ------------------------------ | ------- | ----- | ------- | ------- | ----- |
| This machine                   | 29273   | 26606 | 339     | 1300    | 1028  |
| Clean clone of the same commit | 19359   | 17610 | 317     | 972     | 460   |

Same script, same commit. A CI runner reproduces neither the count nor the
proportion: INVALID falls by 25 percent and STALE by more than half, because
the local corpus is two and a half times larger and its extra files are
scratchpads dense with cross-references.

So every figure quoted about this guard — the 1300, the 1028, the 933 that
remain after mechanical resolution, the 339 phantoms — is a statement about one
developer's scratchpad directory. A maintainer reading them and then seeing CI
report something else has no way to tell whether the corpus moved or the
findings did.

## The figures already disagree with each other

The mission set quotes two different results for this guard at similar times:

- `four-hundred-and-thirty-six-section-citations-name-a-section-that-does-not-exist`
  records `INVALID=1300` over `CHECKED=29273`.
- `citation-gate-audit-two-unwired-guards-one-failing-on-false-positives`
  records `INVALID=979` and `STALE=1036` over `CHECKED=29277`.

Those two are not a corpus difference — 29273 and 29277 are the same walk. They
are guard-version drift, the STALE count moving when the Version History reader
was repaired. Two independent reasons a figure about this guard moves, and
neither was recorded as a reason, is why the set reads as though the numbers are
unstable for no stated cause.

## What the guard prints now

Alongside `SCOPE`, the summary states what the run actually walked:

```
         CORPUS (what this run walked): 1133 markdown files under (rfcs docs), of which 711 are not tracked by git
         and are absent from a clean clone. The counts above are for this machine's corpus
         and will not reproduce in CI.
```

When every walked file is tracked, it says so instead, and when the run happens
outside a repository it prints neither line rather than guessing. The finding
counts are untouched.

## Decisions for maintainers

1. **Should this guard scan only tracked files?** `git ls-files` instead of
   `find` would make a local run and a CI run the same corpus, and it would drop
   711 files whose findings nobody has ever been asked to fix. It also changes
   what the guard rejects, which is why it is a decision and not a fix.
2. **Or are the scratchpads wanted in scope?** They are the densest source of
   citations in the repository. Excluding them by default and adding them
   behind a flag is the other shape of the same decision.
3. **Should the mission set say which corpus its figures were measured over?**
   Several missions quote 1300 as a property of the repository. It is a property
   of one machine.

## Note on what was not done

The guard was not changed to scan only tracked files, and the roots were not
widened. Both change what this lint rejects. Making the difference visible
between the two corpora is not a decision, and that is what was done.
