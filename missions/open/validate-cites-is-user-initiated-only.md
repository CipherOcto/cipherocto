---
name: validate-cites-is-user-initiated-only
description: "By standing instruction, scripts/validate_cites.sh is never run automatically. This unbinds the four places that invoked it, records the rule where it governs future sessions, and corrects a header that described the script as a blocking pre-commit hook it never was."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `validate-cites-is-user-initiated-only`

## Status

Implemented and verified in both directions. `scripts/validate_cites.sh` now
has **no automatic invocation path in this repository**.

## The rule

`scripts/validate_cites.sh` is **user-initiated only**. It is not run by a
hook, a workflow, CI, another script, a harness, or an agent during a routine
check. It runs when a person asks for it.

This is a standing instruction. It is recorded in `CLAUDE.md` so that it governs
sessions that did not see the instruction being given.

## What was actually wired

The census was run before anything was changed, not after.

| Caller                                  | State                   | Action      |
| --------------------------------------- | ----------------------- | ----------- |
| `.github/workflows/`                    | **no reference at all** | none needed |
| `.git/hooks/`, `scripts/hooks/`         | no reference            | none needed |
| `scripts/lint_no_vh_accepted.sh`        | comment only            | none needed |
| `scripts/classify_validate_cites.py`    | reads saved output      | none needed |
| `scripts/mutation-verify-doc-guards.sh` | **4 invocations**       | **unbound** |

CI was already clean. The only live automatic path was the doc-guards harness,
which ran the validator as a side effect of verifying the other document guards.

## The change

The three `validate_cites/*` cases are now opt-in behind `--with-cites`.

```
$ scripts/mutation-verify-doc-guards.sh
  validate_cites (3 cases)           skipped  user-initiated only; run with --with-cites
  passed=26 failed=0 skipped=3

  NOTE: 3 case(s) skipped.
        The 3 validate_cites cases are opt-in: validate_cites.sh is
        user-initiated only and is never run automatically.
        Pass --with-cites to include them.
```

26 passed and 3 skipped is the full 29-case inventory, stated rather than
implied. A silent skip would have let the summary read `passed=29 failed=0`
over a population three cases short of the one it claims.

The footer previously said `3 case(s) skipped by filter ''` — attributing a
policy decision to a filter that was never supplied. It now names the cause, and
the wording is accurate whether or not a filter is also in play.

## Verified in both directions

The decisive test was to **move the script aside** rather than trust the code
read:

| Condition                                      | Result                           |
| ---------------------------------------------- | -------------------------------- |
| `validate_cites.sh` **absent**, default run    | `passed=26 failed=0`, **exit 0** |
| `validate_cites.sh` **absent**, `--with-cites` | `passed=28 failed=3`, **exit 1** |

If the default run touched the script at all it could not have exited 0 while
the script did not exist. If `--with-cites` did not reach the script it could
not have failed three cases. The file was restored and `git status` confirms it.

## A header that described an enforcement that did not exist

`validate_cites.sh` opened with:

```
# Guard 2: Pre-commit § cite validation hook
# Blocking on commit: YES.
```

It was not installed as a pre-commit hook, and under the new rule it must not
be. A guard that names itself after an enforcement it does not perform is the
same defect class as a check that reports on a population it never examined: the
name promises coverage the file does not deliver.

The header now states the rule, and records that the old description was wrong.

## Scope: what was deliberately not touched

- **Historical records were not rewritten.** 23 other files mention
  `validate_cites` — archived missions, prior research documents, and the
  existing mission entries. They describe what was true when written. Editing
  them would falsify the record, and the ban is about execution paths, not
  about history.
- **`classify_validate_cites.py` was left working.** It reads a saved output
  file and never invokes the validator. Its usage text now says so explicitly,
  so the two-step usage is not mistaken for an automatic pipeline.
- **The three harness cases were kept, not deleted.** They verify the fix that
  stopped the validator printing `PASS: all cites valid.` on a run that read
  nothing. Deleting them would have removed the only regression cover for a
  real defect. Gating them respects the instruction without discarding the work.

## Two mistakes made while implementing this

1. The first gate was inserted **inside** the harness's filter check rather
   than after it, so it only fired when a filter was supplied and the three
   cases ran anyway — producing `passed=28 skipped=1` with no explanation. Found
   by reading the actual counts, not by re-reading the patch.
2. The first footer wording claimed all skipped cases were the cite cases. With
   a filter set they were skipped by the filter instead. Reworded to state what
   is true in both paths.

## Acceptance criteria

- [x] No `.github/workflows/` file references the validator.
- [x] No hook references or invokes it.
- [x] No script invokes it outside the opt-in harness flag.
- [x] The harness skips the three cases by default and announces the skip.
- [x] `--with-cites` restores them.
- [x] Verified by removing the script: default run unaffected, opt-in run fails.
- [x] The rule is recorded in `CLAUDE.md`.
- [x] The script header no longer claims to be a blocking pre-commit hook.

## Notes

`CLAUDE.md` is the only file in this repository that governs a session which was
not present when the instruction was given. A rule recorded only in a mission
file is read by whoever goes looking for it, which is a different and much
smaller population.

One consequence worth stating plainly: an agent running a routine battery must
no longer include this validator. It was the slowest check in the tree and the
only one whose normal result is a non-zero exit, so removing it from routine runs
removes both the time cost and a standing temptation to treat its failure as
noise.
