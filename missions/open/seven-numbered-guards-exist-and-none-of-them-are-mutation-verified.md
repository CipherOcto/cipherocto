---
name: seven-numbered-guards-exist-and-none-of-them-are-mutation-verified
description: >-
  Seven numbered document guards live in scripts/. None was mutation-verified, so "this guard catches its defect" was a process claim no reviewer could re-run. A guard that cannot report its finding passes an exit-code-only test. Measured directly: silencing only a guard's echo, leaving EXIT_CODE=1, left the harness reporting 3 of 4 cases green against guards that could no longer name anything they detect. Five of the seven numbered guards were uncovered and are now driven by scripts/mutation-verify-doc-guards.sh from temp fixture corpora, writing no tracked file. Guard 5 joined them once its corpus override was repaired, and Guard 6 joined once it was recognised that a guard taking inputs rather than a corpus can still be driven by a case. All seven numbered guards are covered, along with three unnumbered linters — ten guards in all. Every assertion requires the guard to name its specific finding, not merely to exit non-zero. Covering them exposed a ValueError crash, an UnboundLocalError that exited 0 while printing a traceback, and two guards with no corpus override.
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

Delivered. All seven are now covered by a re-runnable harness, after adding
`--root` and `--baseline` to the two that computed their corpus from `__file__`.
Both additions are additive and default to today's behaviour.

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

## Coverage: all seven

| Guard                    | Covered | Corpus override           |
| ------------------------ | ------- | ------------------------- |
| `lint_section_cites.sh`  | yes     | `RFC_ROOT`                |
| `lint_status_vh_sync.sh` | yes     | `RFC_ROOT`                |
| `lint_no_vh_accepted.sh` | yes     | `RFC_ROOT`                |
| `validate_cites.sh`      | yes     | `RFC_ROOT` plus file args |
| `lint_frontmatter.py`    | yes     | `--tree` (existing)       |
| `lint-rfc-anchors.py`    | yes     | `--root`, `--baseline`    |
| `lint-rfc-type-refs.py`  | yes     | `--root`, `--baseline`    |

The last two had no override, so they were added. Both are **additive and
default to today's behaviour**: without the flags, the module-level roots are
never rebound and the run is byte-identical, verified against the repository
after the change. Neither baseline file was modified.

`--baseline` exists because a fixture corpus has a different set of
known-unresolved sites. Pointing `--root` at a fixture while leaving the
baseline at the repository's 588 anchor and 652 type entries would make every
fixture finding read as _baselined_ and every fixture fix read as a _shrink_ —
the linter would report clean on a deliberately broken corpus.

### `global` in a `main()` that only reads the name

The first attempt at the type-refs override assigned `ROOT` inside `main()`
without declaring it `global`. Python parsed it, and it failed at runtime with
`UnboundLocalError: cannot access local variable 'ROOT'` — because assigning to
a name anywhere in a function makes it local to that function, shadowing the
module-level `ROOT` every other statement in `main()` reads.

Worse, the script **exited 0 while printing that traceback**. A crash that
reports success is the worst failure mode available: it is invisible to every
caller that checks an exit code, and it happened in a gate.

### A clean fixture must actually be clean

The first type-refs fixture referenced a type name invented for the fixture.
The linter correctly reported it as undeclared, and the case failed — the
fixture was wrong, not the guard. The clean corpus now declares the type it
references.

This is the same discipline the harness exists to enforce, applied to the
harness: a fixture that trips the rule it is supposed to be clean under is
testing the wrong thing.

### Adding `lint_frontmatter.py` crashed the linter

It takes `--tree`, so it was the cheapest of the three to cover. But the tree is
resolved against the repository root, so a `/tmp` fixture is not reachable by a
relative name — it is reachable by an _absolute_ one, because `pathlib`'s `/`
operator returns the right operand when that operand is absolute.

Pointing it there crashed:

```
ValueError: '/tmp/.../bad.md' is not in the subpath of '/home/.../cipherocto'
```

`path.relative_to(ROOT)` assumes every path under examination is inside the
repository. The run died with a traceback _after_ computing the classified cause
and _before_ printing a single counter, so:

- the exit code was 1 because of an unhandled exception rather than a finding,
  and a caller could not tell a detected defect from a crash;
- the classified cause was lost, along with every counter.

Fixed in `display_path`, which falls back to the absolute path. The defect is
reachable through any tree argument that is not a repository subdirectory, which
is precisely what a mutation harness wants to pass — so the harness and the fix
were the same piece of work.

## The harness matched a counter and called it a finding

Adding a clean-only case for a bullet-rendered Version History exposed a defect
in the harness itself.

`assert_red` matched the expected substring against the guard's **entire
output**. Two expected strings are also counter names, and every guard prints
its counters on a clean run:

```
PASS=1 FAIL=0 MISSING_VH=0
OUT_OF_SYNC_STATUS_OLDER=0
```

So `grep -qF MISSING_VH` succeeded against `MISSING_VH=0`, on a guard that had
found nothing. The assertion passed. A counter is a statement about the run; a
finding is the run's output, and asserting on the output alone lets a guard
report zero and still be believed.

The matcher now filters to finding lines first — shell-guard prefixes
(`REJECT`, `INVALID`, `MISSING_VH`, …), the Python guards' `NEW ` headings, and
indented or count-prefixed cause lines — and excludes bare `NAME=value`
counters.

Narrowing that filter was itself wrong twice, and each failure was informative:

- Requiring only shell-guard prefixes rejected three genuine detections. The
  Python guards report findings in their own shapes, none of which start with a
  shell prefix.
- Requiring indentation missed `lint_frontmatter.py`, which groups findings by
  cause and prefixes each group with a **count**, so its cause lines begin with
  a digit.

Both failures were loud and named the case, which is what made them cheap.

## Regression pins in the clean fixture

Each case plants one defect and asserts the guard catches it. That leaves the
_other_ direction — a guard rejecting a document it should accept — resting on
the clean fixture alone, which is why the clean fixture was strengthened with
three constructs that exist only to pin false positives already fixed:

| Construct in the fixture                                | Defect it pins                                                                                                 |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `§5a. Platform Loss`, with a matching `### 5a.` heading | the inverted numeric-then-alpha rule, which rejected real subsections while passing all 344 review-finding ids |
| `\| v1.0.0 \|`, a `v`-prefixed version column           | the mandatory-bare-digit rule, which reported 28 populated Version Histories as empty                          |
| Status version _after_ the heading, matching the row    | the heading-line read, which skipped 210 of 247 files and still reported PASS                                  |

Reintroducing any of the three fails the relevant case's **clean** assertion,
verified by doing exactly that:

| Regression reintroduced          | Result                                            |
| -------------------------------- | ------------------------------------------------- |
| subsection rule re-inverted      | `section_cites` clean → `FALSE POSITIVE`, exit 1  |
| `v`-prefix made mandatory        | `no_vh_accepted` clean → `FALSE POSITIVE`, exit 1 |
| Status read back to heading-only | `status_vh` clean → fails, exit 1                 |

Two of those three patches silently failed to apply on the first attempt — a
wrong anchor and a misplaced argument — and the harness reported
`passed=1 failed=0`. A green result from a mutation that never landed is
indistinguishable from a green result from one that did, which is why each row
above was checked for having actually changed the file.

## The two scripts this mission listed as unassessed

Both have now been checked. Neither needed a fix.

### `verify_canonical_blobs.py` — verified, and it can fail

It reimplements the reputation batch digest in Python and compares the result
against the three vectors pinned in the Rust test file. That is a genuine
cross-implementation check, not a self-consistency check.

- **Detect direction.** Perturbing one argument of the Python implementation
  (`batch_size=0` to `batch_size=1` in the 0-leaf vector) produced exactly one
  mismatch, printed both digests, and exited 1. Restoring returned
  `All 3 vectors match byte-identically`.
- **Corpus guards.** `parse_pinned_vectors` exits with a message when the Rust
  file does not contain exactly three vectors, and when any vector is not
  exactly 32 bytes. Both were exercised against synthetic fixtures.

The `pinned[name]` lookup that looks unguarded cannot in fact miss: the parser
refuses to return unless all three names are present.

### `mutate-zk-guards.py` — verified, 38/38

This one already had the discipline this mission is arguing for. Its `--list`
derives the coverage figure from the entry table rather than from a hand-kept
number in prose, and it distinguishes three outcomes that are easy to collapse:

| Outcome        | Meaning                                                |
| -------------- | ------------------------------------------------------ |
| `RED ok`       | the guard fired, and fired for the right reason        |
| `BAD-MUTATION` | the mutation did not compile, so the guard never ran   |
| `VACUOUS`      | the guard stayed green — it does not detect its defect |

The `BAD-MUTATION` case is the one that matters most: a mutation that fails to
compile makes cargo exit non-zero, which an exit-code-only harness reads as a
red guard. Collapsing that into success would let a broken mutation certify a
guard forever.

The cheap tier was run rather than the full one, because the full tier
recompiles the workspace and the disk is at 100% with 5.2G free:

```
mutations: 38/73  red: 38  vacuous: 0  bad-mutation: 0  bad-anchor: 0
```

Zero vacuous and zero bad-mutation across 38 entries. The remaining 35 are
`rebuild`-tier and were not run.

## A counter case needs an accept-direction assertion

The eleventh case pins the population counters of `lint_status_vh_sync.sh`,
which reported `CHECKED=247` for 74 files actually compared — a summary whose
gap reads as unexamined work rather than a skipped population.

It is expressed as an ACCEPT assertion, not a `run_case`, and it took three
attempts to get right:

1. Written as a `run_case`, it **passed against the regressed counter**. A
   counter that lies is not a defect the guard detects — the guard is green
   precisely when it is lying, so the planted-defect phase never fires.
2. Rewritten to forbid the literal string `COVERAGE=100%`, it still passed. A
   counter that inflates `CHECKED` past `SCANNED` never produces `100%`; it
   produces `150%`. **An assertion written against a value the broken code
   cannot produce detects nothing.**
3. The assertion is numeric: coverage must not exceed 100, and must be under
   100 for a fixture containing an uncomparable file. Reverting the counter now
   reports `COVERAGE=166%` and the case fails with exit 1.

This is the first guard case in this suite that is not a defect-detection case,
and it needed a different assertion shape because a miscounting guard and a
detecting guard cannot both be red for the same input.

## Follow-ups

- No guard runs in CI. See
  `seven-numbered-guards-exist-and-none-of-them-runs-in-ci` for the drafted step.
- Each case plants ONE defect per guard, and the clean fixtures pin six
  regression classes between them. The guards implement more rules than that.
  One verified rule is not the same as a verified guard, and the count of cases
  should not be read as coverage. `lint_section_cites.sh` has an over-deep case
  and `lint_no_vh_accepted.sh` has the bullet-rendered Version History.
- Guard 5 was uncoverable for a reason that looked like coverage. It reads
  `RFC_ROOT` from the environment, which made it appear fixture-drivable, and
  then scanned a hardcoded `docs` alongside it — so pointing it at a fixture
  still read the repository's real documentation tree. The override controlled
  half the corpus and said nothing about the other half. A second root is now
  overridable, with the default unchanged, and the guard has a case.
- Guard 6 was uncovered for a reason that looked permanent. It takes a projection
  string and has no corpus parameter, so there was nothing to point at a fixture.
  It is now covered anyway, because the absence of a corpus parameter is not the
  absence of an input: the case drives the guard directly with chosen projections
  under a comma-decimal locale and requires the verdicts to be the ones it gives
  under a dot-decimal one — a 100 percent realization rate passes and a 35 percent
  rate fails — plus a check that the output never carries a comma decimal
  separator. That is the case that would have caught the locale-dependent verdict
  this metric actually shipped, where a `100,0` realization percentage came back
  as a string and compared lexicographically. The reasoning error was assuming a
  corpus-scanning guard was the only kind a harness can drive.
- Every accept-direction case in the harness has been mutation-verified. That
  is a rule rather than a count, because counts go stale: a case is not
  verified because it was written carefully, it is verified because a known
  regression was applied and the case was seen to fail. Two cases here had
  never been checked that way and were found to fail correctly when checked.
- Assert against the presence of what a fix produced, never against the absence
  of harm it prevents. Two cases in this harness asserted that nothing leaks or
  goes wrong, and both passed against the regressed guard, because the harm
  they were guarding against was unobservable in the fixture. Both now assert
  that the guard finds something that only exists in the fixture, which is
  observable in both directions.
- The free tier of `mutate-zk-guards.py` (38 of 73 entries) is now run and
  clean: **38 red, 0 vacuous, 0 bad-mutation, 0 bad-anchor**. It needs no
  recompile, so it is affordable on a constrained disk and there was no reason
  for it to have waited behind the expensive half.
- The `rebuild` tier of `mutate-zk-guards.py` (35 of 73 entries) was recorded
  for a long time as unaffordable: it recompiles, and the disk sits at 100%. That
  was an assumption, and it was wrong. The tier runs a _targeted_
  `cargo test -p <crate>`, and with 294 GB of compiled dependencies already in
  `target/` the incremental build needs no meaningful new space. Run in full:
  **73 of 73 red, 0 vacuous, 0 bad-mutation, 0 bad-anchor**, in 175 seconds at
  1.79 GB peak resident, with free disk unchanged before and after and the tree
  clean afterwards.
  Nothing was verified about the zk guards until this run, so for that period
  every statement about them rested on the 38 free entries alone. The lesson is
  the one this mission keeps arriving at from different directions: a cost
  estimated rather than measured is a claim about a condition nobody examined,
  and this one sat in a mission for hours asserting a blocker that was not
  there.
