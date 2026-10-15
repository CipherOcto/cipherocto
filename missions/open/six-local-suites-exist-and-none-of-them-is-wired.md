---
name: six-local-suites-exist-and-none-of-them-is-wired
description: "Six suites in this repository each caught a real defect and each is runnable only by hand. No workflow invokes any of them, so the set of checks is implicit and a session can report the battery green while skipping two. One entry point now runs the default set, states what it did not run, and encodes the standing rule that the cite validator is never automatic."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `six-local-suites-exist-and-none-of-them-is-wired`

## Status

The entry point is implemented and mutation-verified. **Wiring it into CI is not
done** — that is owner policy, and this entry is the proposal's input rather
than the wiring.

## The finding

Six suites in this repository are runnable by hand and none is invoked by any
workflow. Every one of them caught a real defect the day it was run.

| Suite                                   | Covers                                  | Found                                                             |
| --------------------------------------- | --------------------------------------- | ----------------------------------------------------------------- |
| `scripts/test-dangerous-tokens.sh`      | the token library (68 cases)            | two rules that could never fire; one silenced by any earlier word |
| `scripts/test-safe-commit.sh`           | the wrapper's argument handling (10)    | a combined short flag bypassed the scan entirely                  |
| `scripts/install_test.sh`               | the installer (17)                      | a misspelled flag exited 0; a test passing for the wrong reason   |
| `scripts/mutation-verify-doc-guards.sh` | ten document guards (29)                | four guards reporting on populations they never examined          |
| `scripts/verify_canonical_blobs.py`     | the cross-implementation anchor vectors | an RFC-mandated check nothing executed                            |
| `scripts/mutate-zk-guards.py`           | the zk guard set (73 mutations)         | thirty-five rebuild-tier entries never run                        |

The problem is not that the suites are weak. It is that **the set is
implicit**. A session can run four of six and report the battery green with no
record that two were skipped, and nothing in the tree can contradict it.

Measured: no workflow references `guard_*`, `mutation-verify*.sh`,
`test-dangerous-tokens`, `install_test`, `test-safe-commit`, or
`verify_canonical_blobs`. The negative claim in
`seven-numbered-guards-exist-and-none-of-them-are-mutation-verified` — "no
guard runs in CI" — remains accurate.

## What was added

`scripts/run-local-checks.sh` runs the default set, prints one line per check,
names anything that failed, and **states what it did not run**:

```
local checks (mode: fast)
=========================================

guard suites
  dangerous tokens                   ok
  safe-commit arguments              ok
  installer                          ok
  document guards                    ok

document linters
  rfc anchors                        ok
  rfc type refs                      ok
  section cites                      ok
  accepted rfc vh                    ok
  frontmatter                        ok

skipped: canonical blobs, zk guard mutations
  both compile or walk the whole tree; pass --with-slow to include them
  the zk harness needs several GB of build headroom while it runs

not run: scripts/validate_cites.sh
  user-initiated only; never automatic, including here.
  pass --with-cites to include its three harness cases.

=========================================
9 passed, 0 failed
```

The `validate_cites` line is the point of the file as much as the aggregation
is. The standing rule that the cite validator is never run automatically is now
structural rather than remembered: the entry point for every local check states
that it does not run it, and why.

## Verified in both directions

| Condition                                   | Result                                                                   |
| ------------------------------------------- | ------------------------------------------------------------------------ |
| Everything green                            | `9 passed, 0 failed`, exit 0                                             |
| Token suite expectation flipped so it fails | `dangerous tokens FAIL`, exit 1, `8 passed, 1 failed`, names the failure |

The mutation was reverted and the tree confirmed clean afterwards. A runner
that cannot go red would be the same defect as everything else in this file.

## A count I removed rather than fixed

The first version scraped each suite's own total out of its output text. Six
suites print six different formats — `Pass=68 Fail=0`, `17  passed` with ANSI
bold codes, `10 passed` — and the scraper produced a blank count for four of
them and a wrong-shaped one for the rest.

Hand-rolled parsing of another tool's output is how this repository produced a
clean green light four times. The count was deleted: each suite prints its own
number when run directly, and this reports only whether it passed.

## Scope

Not wired into CI. `.github/workflows/` is owner-policy surface, and the four
suites that take seconds are the natural candidate while the zk harness is not.

## Acceptance criteria

- [x] One command runs every fast local check.
- [x] Anything skipped is named, with the reason.
- [x] The runner fails, with a non-zero exit, when a check fails.
- [x] The standing `validate_cites` rule is encoded in the entry point.
- [x] No count is inferred by parsing another tool's output.
- [ ] A maintainer decides whether to wire it, and which suites belong in CI.

## Notes

The two slow checks are excluded by default for a reason worth stating: the zk
mutation harness peaks around 1.8 GB of build output, and this working volume
sits at 100%. A default that fills someone's disk is not a default.
