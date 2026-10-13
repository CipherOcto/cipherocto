---
name: seven-numbered-guards-exist-and-none-of-them-are-mutation-verified
description: >-
  Seven numbered document guards live in scripts/. None was mutation-verified, so "this guard catches its defect" was a process claim no reviewer could re-run. A guard that cannot report its finding passes an exit-code-only test. Measured directly: silencing only a guard's echo, leaving EXIT_CODE=1, left the harness reporting 3 of 4 cases green against guards that could no longer name anything they detect. All seven are now covered by scripts/mutation-verify-doc-guards.sh, which drives them from temp fixture corpora and writes no tracked file. Every assertion requires the guard to name its specific finding, not merely to exit non-zero. Covering them exposed a ValueError crash, an UnboundLocalError that exited 0 while printing a traceback, and two guards with no corpus override.
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

## Follow-ups

- No guard runs in CI. See
  `seven-numbered-guards-exist-and-none-of-them-runs-in-ci` for the drafted step.
- `verify_canonical_blobs.py` and `mutate-zk-guards.py` are also unverified by
  this harness and were not assessed.
- Each case currently asserts one defect per guard. The guards have more rules
  than that; one verified rule is not the same as a verified guard, and the
  count of cases should not be read as coverage.
