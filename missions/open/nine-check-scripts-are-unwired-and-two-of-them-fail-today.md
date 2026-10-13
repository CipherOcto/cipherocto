---
name: nine-check-scripts-are-unwired-and-two-of-them-fail-today
description: "Nine check and lint scripts exist in scripts/. Two are wired into CI. The remaining seven are unwired. lint_status_vh_sync.sh and lint_plan_yaml_vh_sync.sh fail today, lint_plan_yaml_vh_sync.sh lints a hardcoded path outside the repository so it cannot run anywhere else, and the remaining findings are dominated by files that legitimately have no Status version or no Version History rows. A gate that is unwired and red is worse than no gate, because red teaches people to ignore it."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `nine-check-scripts-are-unwired-and-two-of-them-fail-today`

## Status

Open. One defect fixed (see below). Nothing wired, nothing deleted, nothing in
`.github/` changed — wiring is owner policy.

## The census

Eleven scripts in `scripts/` are checks or linters. Two are referenced by a
workflow.

| Script                      | In CI | Exit today | Notes                                            |
| --------------------------- | ----- | ---------- | ------------------------------------------------ |
| `lint-rfc-anchors.py`       | yes   | 0          | 590-entry baseline                               |
| `lint-rfc-type-refs.py`     | yes   | 0          | 729-entry baseline                               |
| `lint_no_vh_accepted.sh`    | no    | 1          | 51 real findings after the detection fix         |
| `lint_status_vh_sync.sh`    | no    | 1          | 1 real finding after the fix below               |
| `lint_plan_yaml_vh_sync.sh` | no    | 1          | lints a path outside the repository              |
| `lint_section_cites.sh`     | no    | 1          | 58 findings, all false positives                 |
| `validate_cites.sh`         | no    | 1          | 1,323 unique findings, several defect classes    |
| `verify_canonical_blobs.py` | no    | 0          | RFC-mandated, independently verified by mutation |
| `mutation-verify-guards.sh` | no    | 0          | 10/10 pass — companion to the mutation harness   |

Two scripts named in this census turned out not to be gates and are excluded:
`cross_rfc_propagate.sh` is a maintenance automation tool that emits a diff for
human review ("Manual review required: YES"), and
`test-dangerous-tokens.sh` is a test fixture runner. Neither is a check that
claims to police the tree.

One measurement note, recorded because it nearly produced a wrong table:
`mutation-verify-guards.sh --help` used to report `passed=0 skipped=10`, because
the script treated any argument as a name filter and `--help` matched nothing.
With no argument it passed 10/10. A script that treats its own help flag as a
filter silently measures nothing, which is worth knowing before wiring it
anywhere.

**Since fixed.** `--help` prints usage and lists every guard, and a filter that
matches no guard exits 1 instead of reporting a successful empty run. Recorded
in mission
`four-linters-report-clean-while-their-counters-describe-a-different-population`.
The 10/10 figure in the table above is unchanged and re-verified after that
fix.

## Fixed here: the status linter compared against the oldest row

`lint_status_vh_sync.sh` extracted the "latest" Version History version with an
awk that printed the first matching versioned row and exited:

```bash
awk '/^## (Version History|Changelog)$/{found=1; next} found && /^\| [0-9]+(\.[0-9]+)+ \|/ {print; exit}'
```

A Version History table is append-ordered oldest-first, so the first row is the
**oldest** entry. Every RFC whose table held more than one row was compared
against its initial version and reported out of sync.

9 findings, 8 of them this bug on files that are correctly in sync. Example:
`rfcs/draft/networking/0859-proof-carrying-envelopes.md` declares
`version: 1.1.0` in frontmatter, its VH table starts at `1.0.0`, and its newest
row is `1.1.0`. The linter reported `Status=v1.1.0, VH latest=v1.0.0`.

Fixed by collecting every versioned row and taking `sort -V | tail -1`. Version
sorting is required rather than lexicographic: `1.10.0` must beat `1.9.0`.

After the fix the tree reports **1** finding, down from 9.

Verified by mutation in both directions:

- reverting to the first-row form returns the count to **11**, higher than the
  original 9, because removing `sort -V` also misorders versions
- injecting genuine drift (VH row `1.3.0` against Status `v1.1.0`) is still
  detected, and still classified in the correct direction as
  `OUT_OF_SYNC_STATUS_OLDER`

The ninth original finding is real and untouched: `0110-deterministic-bigint.md`
declares `**Version:** 2.14.0` against a VH row of `2.14`. Same version,
different precision.

## The plan linter cannot run anywhere but this machine

`lint_plan_yaml_vh_sync.sh` defaults to an absolute path outside the repository:

```bash
PLAN_ROOT="${PLAN_ROOT:-/home/mmacedoeu/.claude/plans}"
```

It reports 6 failures, all of which are agent session plan files. None of them
are in this repository; the repo has no `plans/` directory and the two tracked
`plans/` paths are elsewhere.

A gate whose target is a developer's home directory is not a gate on the
project. It is a personal scratchpad check wearing a linter's clothes. It is
also the only linter that hardcodes a path in this way, though
`lib-octo-wa.sh` and several `persist-*.sh` scripts hardcode
`/home/mmacedoeu/...` binary paths, which is a separate portability concern for
scripts that are not checks.

## The bulk findings are probably not defects

`lint_status_vh_sync.sh` still reports, after the fix:

| Class                      | Count | Reading                                           |
| -------------------------- | ----- | ------------------------------------------------- |
| `NO_STATUS_VERSION`        | 210   | files whose `## Status` header carries no version |
| `NO_VH`                    | 135   | files with no Version History section at all      |
| `OUT_OF_SYNC_STATUS_NEWER` | 1     | the genuine format mismatch above                 |
| `IN_SYNC`                  | 15    |                                                   |

345 findings against 247 files, from a linter that is not wired and whose
severity model treats a missing optional section as a failure. An archived RFC
with no `## Status` version is not drift. These counts are recorded as
observations, **not** as a work queue — resolving them means deciding what the
linter should require of an archived document, which is an owner decision.

## Acceptance criteria

- [ ] A decision is recorded for each unwired check: wire it, or record why it
      is satisfied without running in CI.
- [ ] `lint_plan_yaml_vh_sync.sh` either targets something inside the repository
      or is marked as a local-only tool. A hardcoded home path in a script that
      presents itself as a project guard is a defect either way.
- [ ] `NO_STATUS_VERSION` / `NO_VH` are classified as required or exempt
      before the status linter is considered for wiring. A gate that reports 345
      findings on a clean tree cannot be adopted as-is.
- [ ] Anything adopted reports zero on the tree at the moment of adoption.

## Notes

The census was run excluding `.claude/worktrees/` copies, which otherwise
inflate reference counts by 3 to 4x and make dead scripts look wired. Seven
scripts have zero references anywhere in the tree; this mission covers the six
that are checks or linters, and the seventh
(`compute_decimal_config_hash.py`) is not a check.
