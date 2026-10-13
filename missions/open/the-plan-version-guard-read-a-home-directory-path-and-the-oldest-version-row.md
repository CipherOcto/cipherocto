---
name: the-plan-version-guard-read-a-home-directory-path-and-the-oldest-version-row
description: Guard 7 defaulted its corpus to /home/mmacedoeu/.claude/plans, an absolute path in one developer's home directory committed to a shared repository. It then read the FIRST Version History row as the latest, and required exactly one space around the version, so an aligned table matched nothing. Both defects make a correct plan report as out of sync with itself.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - seven-numbered-guards-exist-and-none-of-them-are-mutation-verified
---

# Mission `the-plan-version-guard-read-a-home-directory-path-and-the-oldest-version-row`

## Status

Three defects fixed in `scripts/lint_plan_yaml_vh_sync.sh` and
mutation-verified in both directions. The guard now has a harness case, which
makes it the eighth guard covered.

This guard was the one outstanding item from the CI-wiring mission that was not
waiting on a maintainer decision. It is a script defect, not a policy question.

## Defect one: the default corpus was one developer's home directory

```bash
PLAN_ROOT="${PLAN_ROOT:-/home/mmacedoeu/.claude/plans}"
```

An absolute path with a username in it, committed to a shared repository. On any
other machine and on a CI runner, that path does not exist.

It failed closed rather than silently — the empty-corpus guard exits 1 with
`no plan files were examined` — so the failure mode was a permanently red guard
everywhere except one workstation, which is a check nobody can act on. Not a
false pass, but not a working guard either.

The default now resolves under `$HOME`. The plans directory is a local scratchpad
and is deliberately not in the repository, so there is no in-repo default that
would be correct; a caller without one gets the existing honest failure.

Verified with a synthetic `HOME`: absent directory gives the "examined nothing"
failure, present directory is scanned.

## Defect two: the oldest row was read as the latest

```bash
found && /^\| [0-9]+(\.[0-9]+)+ \|/ {print; exit}
```

`print; exit` stops at the **first** matching row. A Version History is
append-ordered oldest-first, so the first row is the oldest entry.

Reproduced directly: a plan with `version: 2.0` and a table running `| 1.0 |`
then `| 2.0 |` was reported `YAML=v2.0, VH latest=v1.0` — out of sync with
itself, against a document that agrees.

This is the same defect `lint_status_vh_sync.sh` had. That one was repaired;
this one was copied from the pattern before the repair and never revisited.

## Defect three: an aligned table matched nothing

The row pattern required exactly one space either side of the version, so a
column-aligned table — `| 2.0     | 2026-02-01 |` — matched nothing and the
plan was reported as having no Version History at all.

Both defects had to be fixed together. Fixing only the row pattern would have
made more plans find a first row that was still the wrong row, turning a
false negative into a false positive.

The reader now takes every versioned row, sorts by version, and keeps the
highest — `1.10.0` correctly outranks `1.9.0`, which lexicographic ordering
gets backwards. The scan is also bounded to the table, because an unbounded one
reads every later pipe table in the document as Version History rows.

## Why the real corpus never showed this

Running the fixed guard against the real plans directory produces exactly the
same result as before: 6 findings, all `NO_VH_NO_YAML`, no drift. None of those
plans has a Version History section, so both defects were masked — the row
reader returned empty either way, and empty fell into the `NO_VH` branch.

A defect that the current corpus cannot exercise is a defect that survives
review indefinitely. It was found by constructing the document it breaks on,
which is what the harness case now keeps in place permanently.

## Verification

| Mutation                          | Result                                                             |
| --------------------------------- | ------------------------------------------------------------------ |
| first-row read restored           | in-sync plan reported `OUT_OF_SYNC_YAML_NEWER`, case fails, exit 1 |
| single-space row pattern restored | aligned table missed, `IN_SYNC=0` where `IN_SYNC=1` expected       |
| both restored                     | `IN_SYNC=1`, `PASS`, exit 0                                        |

## Coverage

`lint_plan_yaml_vh_sync.sh` honours `PLAN_ROOT`, so it joins the fixture-driven
set. The harness is now **ten cases across eight guards**.

The three guards that remain uncovered are `verify_canonical_blobs.py`,
`mutate-zk-guards.py` (already assessed and sound) and none other — every
`RFC_ROOT`- or environment-driven guard in `scripts/` is now covered.
