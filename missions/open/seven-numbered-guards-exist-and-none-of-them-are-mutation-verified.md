---
name: seven-numbered-guards-exist-and-none-of-them-are-mutation-verified
description: >-
  Seven numbered document guards live in scripts/. None is mutation-verified, so "this guard catches its defect" is a process claim no reviewer can re-run. A guard that cannot report its finding passes an exit-code-only test. Measured directly: silencing only a guard's echo, leaving EXIT_CODE=1, leaves the harness reporting 3 of 4 cases green against guards that can no longer name anything they detect. Now fixed by scripts/mutation-verify-doc-guards.sh, which drives four guards from temp fixture corpora and writes no tracked file. Every assertion requires the guard to name its specific finding, not merely to exit non-zero.
metadata:
  node_type: mission
  type: tooling
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - seven-numbered-guards-exist-and-none-of-them-runs-in-ci
---

# Mission `seven-numbered-guards-exist-and-none-of-them-are-mutation-verified`

## Status

Supersedes the mutation-verification half of
`seven-numbered-guards-exist-and-none-of-them-runs-in-ci`, which recorded that
the guards exist and that none runs in CI. That mission is still accurate about
CI. This one covers what CI alone would not have caught.

Partially delivered. Four of the seven are now covered by a re-runnable harness.
The other three are not, and the reasons are listed below.

## The gap

`scripts/mutation-verify-guards.sh` mutation-verifies ten cargo-test guards in
`crates/octo-cli/tests/security_regressions.rs`. It covers none of the numbered
document guards.

So for every one of these, "the guard catches the defect it exists to catch" is
a statement about what someone did once, by hand. Nothing re-runs it. Every
defect found in this area over the past several weeks was found by reading, and
a defect reintroduced next year would be found the same way or not at all.

## A guard that cannot fail passes an exit-code-only test

This is the finding that shaped the harness, and it is worth stating plainly
because it is easy to trip over.

When first written, the harness asserted two things per case: the guard passes a
clean fixture, and the guard exits non-zero on a fixture with a defect. Blinding
the four guards — by replacing each finding's `echo` with a no-op — left
`EXIT_CODE=1` in place. The guards still went red. So the harness reported:

```
passed=4 failed=1
```

Three of four cases passed against guards that could no longer name a single
thing they detect. The harness was measuring "did the process exit non-zero",
which is a property of `set -e` bookkeeping, not of the guard.

The fix is that each assertion requires the guard's output to contain the
specific finding string, not merely a non-zero status:

| Guard                    | Expected substring          |
| ------------------------ | --------------------------- |
| `lint_section_cites.sh`  | `REJECT [phantom-token]`    |
| `lint_status_vh_sync.sh` | `OUT_OF_SYNC_STATUS_OLDER`  |
| `lint_no_vh_accepted.sh` | `MISSING_VH`                |
| `validate_cites.sh`      | `INVALID [section missing]` |

Blinding each guard properly — removing the emission _and_ the exit code
together — now fails all four cases. Verified in both directions: all four
detected when blinded, all four pass when restored.

## Why fixtures instead of mutating tracked files

The cargo harness backs up tracked files and restores them. For document guards
that is both risky and unnecessary: mutating `rfcs/` to test a document linter
risks leaving a defect in the corpus, and it cannot be done at all while the
corpus is itself red.

Four guards take their corpus root from the environment (`RFC_ROOT`), and
`validate_cites.sh` additionally accepts explicit file arguments. So each case
writes a small fixture corpus to a temp directory and points the guard at it.
No tracked file is written, an interrupted run has nothing to restore, and the
cost is kilobytes rather than the 1.79GB the cargo harness measures.

This matters concretely: the development disk was at 100% full with 5.9G free
while this was built.

## Four harness defects found by running it

A harness that has only ever passed proves nothing, so the first runs were the
real work. All four of these were caught by the harness failing loudly rather
than silently, which is the property worth keeping:

1. **Guard invocation was a quoted command string.** `"$guard"` where the value
   was `env RFC_ROOT=... bash scripts/lint_section_cites.sh` searched for one
   file with that entire name. All four cases reported "FALSE POSITIVE on a
   clean fixture" and exited 1.
2. **The rewrite deleted the case definitions.** Editing the block that held the
   guard-invocation shims also removed the four `case_*` functions, so the run
   reported `no cases were defined` and exited 1.
3. **Cases shared one fixture directory.** Case 1's planted defect was still
   present when case 2 measured its "clean" phase, so case 2 reported a false
   positive against a defect case 1 had planted.
4. **The defect hook was named `before_defect` and called before the clean
   assertion.** It planted the defect first, then asserted the guard was green
   on a corpus containing it. All four cases failed for this reason. The name
   was the bug: it said "before" and was invoked before, with the clean
   assertion sitting after it.

Each was loud. None would have been caught by reading the script.

## Coverage: four of seven

| Guard                    | Covered | Note                                                    |
| ------------------------ | ------- | ------------------------------------------------------- |
| `lint_section_cites.sh`  | yes     |                                                         |
| `lint_status_vh_sync.sh` | yes     |                                                         |
| `lint_no_vh_accepted.sh` | yes     |                                                         |
| `validate_cites.sh`      | yes     |                                                         |
| `lint-rfc-anchors.py`    | no      | no corpus-root override; would need a `--root` argument |
| `lint-rfc-type-refs.py`  | no      | same, and it also compares against a baseline file      |
| `lint_frontmatter.py`    | no      | has `--tree`, so it is the easiest of the three to add  |

`lint_frontmatter.py` already takes `--tree` and needs only a case. The other two
compute their corpus root from `__file__` and read a sibling baseline file, so
covering them means adding a root override first — a change to a CI-wired
script, which is a maintainer decision rather than a tooling detail.

## Follow-ups

- No guard runs in CI. See
  `seven-numbered-guards-exist-and-none-of-them-runs-in-ci` for the drafted step.
- `verify_canonical_blobs.py` and `mutate-zk-guards.py` are also unverified by
  this harness and were not assessed.
- Each case currently asserts one defect per guard. The guards have more rules
  than that; one verified rule is not the same as a verified guard, and the
  count of cases should not be read as coverage.
