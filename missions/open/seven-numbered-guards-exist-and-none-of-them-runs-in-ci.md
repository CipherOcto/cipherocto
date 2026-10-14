---
name: seven-numbered-guards-exist-and-none-of-them-runs-in-ci
description: "The repository defines seven numbered guards, each naming itself Guard N and citing the same long-horizon plan. Only two document checks run in CI, and neither is one of the seven. The operator guide documents a pre-commit guard that cannot work: it gives validate_cites.sh a 30 second timeout, that script takes about four minutes, and timeout returns 124, which under the guard's own set -euo pipefail aborts every commit. No pre-commit hook is installed at all, and Guard 3 names two different things."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `seven-numbered-guards-exist-and-none-of-them-runs-in-ci`

## Status

Open. Nothing here is changed: CI and hook configuration is owner policy, and
the point of this entry is that the choice has never been put to anyone,
because every surface that could have revealed it reports success.

## The inventory

Seven guards, each self-identifying and citing the same source plan:

| Guard | Script                           | Domain                           |
| ----- | -------------------------------- | -------------------------------- |
| 1     | `lint_section_cites.sh`          | §section_name hygiene            |
| 2     | `validate_cites.sh`              | RFC number and section cites     |
| 3     | `lint_status_vh_sync.sh`         | Status header vs VH row sync     |
| 4     | `lint_no_vh_accepted.sh`         | Accepted RFC has Version History |
| 5     | `cross_rfc_propagate.sh`         | cross-RFC propagation            |
| 6     | `projection_inflation_metric.sh` | decomposition inflation metric   |
| 7     | `lint_plan_yaml_vh_sync.sh`      | plan YAML vs VH row sync         |

Document checks actually executed by `.github/workflows/lint.yml`:

```text
scripts/lint-rfc-anchors.py
scripts/lint-rfc-type-refs.py
```

**Zero of the seven run in CI.** The two that do run are not numbered guards.
Two further document checks exist outside the numbering entirely and are also
unwired: `lint-rfc-type-refs.py`'s sibling `lint_frontmatter.py`, added during
this session's frontmatter repair.

## The documented pre-commit guard cannot work

`docs/06-operations/operator-guide.md` §Register — pre-commit guard instructs
the operator to write this:

```bash
#!/bin/bash
set -euo pipefail
# Pre-commit Guard 2: cite validator (RFC numbers + section refs).
timeout 30 ./scripts/validate_cites.sh
# Pre-commit Guard 3: cargo fmt + clippy.
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Three problems, each verified rather than inferred.

**The timeout is shorter than the command by a factor of eight.** The script
exceeded a 120-second `timeout` on two separate runs this session, exiting 124. It completes in under 280 seconds.

**Correction to the figure in this entry's description.** The description said
"over 120 seconds", which is true but understated it by roughly half, and a
description is what a reader sees before deciding whether to open the file.
Measured on this machine: **245 seconds wall**, against a documented 30-second
timeout — eight times over, not four. The range in this section was the accurate
one; only the summary line was loose.

That measurement carries a caveat worth stating: this machine's disk is full,
and the script is I/O-bound across roughly 1267 tracked markdown files, so the
figure is an upper bound for a healthy disk. It does not weaken the finding —
the script is two orders of magnitude past any plausible pre-commit budget —
but the direction of the error is known rather than guessed.

**A `timeout` that fires fails the commit.** Verified directly:

```console
$ bash -c 'set -euo pipefail; timeout 1 sleep 5; echo REACHED'
$ echo $?
124
```

The `echo` never runs. Under the hook's own `set -euo pipefail`, a guard that
runs for four minutes does not slow a commit down — it prevents it.

**No pre-commit hook is installed.** `.git/hooks/pre-commit` does not exist.
`scripts/install-hooks.sh` installs from `scripts/hooks/`, which contains
exactly two files, `commit-msg-no-backtick` and `pre-receive-no-backtick`.
Neither is a document guard.

So the guide describes a guard that is not installed, and could not work if it
were. The `commit-msg` hook is the one that actually fires — it is what has
been rejecting commit messages containing backticks and semicolons throughout
this session.

## Guard 3 names two different things

The operator guide calls `cargo fmt` plus `cargo clippy` **Guard 3**.
`scripts/lint_status_vh_sync.sh` calls itself **Guard 3** for Status/VH sync.

A numbering that resolves to two unrelated checks cannot be cited. This is not
hypothetical friction: the guards reference each other by number in their
headers, and "Guard 3" is not a thing any reader can look up.

## The step that would lock in this session's fixes

Recorded so that approving it is a decision rather than an edit. This is
**not** applied: `.github/workflows/` is owner policy and the choice has been
left open.

Four document checks now exit 0 that are not in CI. Wiring them turns this
session's fixes from "true today" into "cannot silently regress":

```yaml
- name: Lint document frontmatter
  run: python3 scripts/lint_frontmatter.py

- name: Accepted RFCs carry a Version History
  run: bash scripts/lint_no_vh_accepted.sh

- name: Section citation hygiene
  run: bash scripts/lint_section_cites.sh

- name: Document guards can still fail
  run: bash scripts/mutation-verify-doc-guards.sh
```

The fourth step is different in kind from the other three. Those assert facts
about the corpus. This one asserts that the _asserters_ still work, which is the
only step that notices when a guard has been broken into reporting nothing.

It is included because a guard that cannot fail is the failure mode this whole
line of work keeps finding, and none of the other three steps can detect it: a
broken guard that reports zero findings produces exactly the output a clean
corpus produces.

`scripts/lint_frontmatter.py` reports 251 of 251 documents parsing and 0
failures, after the 33 unparseable blocks were repaired.
`scripts/lint_no_vh_accepted.sh` reports `PASS=155 FAIL=0`, after four
misfenced RFC headings were renamed and two Version History sections were
added. `scripts/lint_section_cites.sh` reports `REJECTED=0` over 10276 cite
tokens, after its numeric-then-alpha rule was found to be inverted, its
trailing-punct class was found to match nothing at all, and its extraction was
found to be absorbing sentence punctuation into every cite token.

Three are deliberately **not** in the snippet, because wiring them now
would land red for reasons that are not the repository's fault:

| Script                      | Exit | Why not yet                                                                      |
| --------------------------- | ---- | -------------------------------------------------------------------------------- |
| `lint_status_vh_sync.sh`    | 1    | 9 genuine drift findings, each read individually and left for an author decision |
| `lint_plan_yaml_vh_sync.sh` | 1    | its corpus is a local scratchpad that does not exist on a runner                 |
| `validate_cites.sh`         | 1    | 1300 findings, of which 625 are its own path ambiguity and 436 are real          |

Wiring a check that fails is legitimate once the failures are understood, and
all three above are understood. They are listed so the decision is informed
rather than made by whoever runs the job first.

### Guard 7 cannot run on a runner at all

This one is not merely red; it is unrunnable. Simulated with an empty `HOME`:

```
FAIL: no plan files were examined (PLAN_ROOT=/home/runner/.claude/plans).
This lint verified nothing. Point PLAN_ROOT at the intended corpus
before treating a pass as meaningful.
```

It fails closed, which is the right behaviour and not a false pass — but it
means wiring it would red every run for a corpus that is deliberately outside
the repository. Its default is now `$HOME`-relative rather than hardcoded, so
the guard works for whoever runs it locally; it still cannot gate CI until the
plans directory is either tracked or the guard is given a repository corpus.

This is the reason the snippet above lists three steps and not six. The other
two red guards fail on documents that exist in the repository, and are waiting
on a decision about those documents. Guard 7 is waiting on a decision about
whether it belongs in CI at all.

### The harness is worth wiring even though it is not a gate

`scripts/mutation-verify-doc-guards.sh` runs a set of cases across every guard
covered by `seven-numbered-guards-exist-and-none-of-them-are-mutation-verified`,
and exits 0. Its case count is whatever `--help` prints, and that listing is
self-maintaining: the run asserts that the number of named cases equals the
number of case functions executed, so the two cannot drift apart. Wiring it as
a gating step is the one addition that changes
what CI can catch rather than merely adding a gate that already passes: without
it, a guard can be silently broken and the three gating steps above would still
be green, because a broken guard that reports nothing looks exactly like a
clean corpus.

An earlier version of this section restated the count as a number here, which
guaranteed it would go stale: the figure has needed correcting three times, and
each correction was a separate piece of work for a fact that is already
self-reporting. Nothing in this mission should quote a live count again.

That rule was written for the case count and then broken two paragraphs later,
by a different live count: the number of guards the harness drives. No part of
the harness reports it, so it has no self-maintaining source the way the case
count has, and it was already wrong here. The number is gone rather than
corrected, because correcting it is the third correction of a fact that will go
stale again. Which guards are covered is a question for the sibling mission,
which is where coverage is tracked.

It costs seconds. It touches no tracked file. It is the direct answer to this
mission's own thesis.

### The proposal was validated in a CI-like environment

Not reasoned about — run. A pristine `git clone` of `next` at this commit, with
no `~/.claude`, no build, and no local state:

| Proposed step                                | Exit in a clean clone       |
| -------------------------------------------- | --------------------------- |
| `python3 scripts/lint_frontmatter.py`        | 0                           |
| `bash scripts/lint_no_vh_accepted.sh`        | 0                           |
| `bash scripts/lint_section_cites.sh`         | 0                           |
| `bash scripts/mutation-verify-doc-guards.sh` | 0 — 10 cases at that commit |

The 10 there is what the clone produced when this was validated, and it is
kept rather than updated: a historical measurement is not made truer by editing
it. The harness has grown several cases since, for two separate reasons. Its
case listing counted only the cases written in one call form and omitted those
that assert on a guard's output instead of planting a defect for it to catch.
Cases were then added for defects found afterwards — a population that did not
reconcile, a heading shape neither guard read, and two baselined linters that
did not say what their baselines were doing. The step still exits 0; the count
moved because the harness covers more, not because it began examining something
it had not. The blind result below is likewise a record of that run rather than
a current one.

The harness was then shown to still bite in that environment: blinding
`lint_section_cites.sh`'s phantom-token rule inside the clone produced
`passed=10 failed=1` and exit 1.

That last check is the one that matters. A step that passes in CI has not been
shown to fail in CI, and until now no one had run either state there.

## Why nobody has noticed

Every surface that could have surfaced this reports success:

- CI runs two document checks that pass.
- The seven guards report only when invoked by hand.
- The operator guide's pre-commit block is copy-paste text inside a runbook,
  not something the repository verifies against its own scripts.

This is the same shape as the findings elsewhere in this session's mission set:
a gate that exists, a name that resolves, and a summary that never meets.

## Acceptance criteria

- [x] Inventory every numbered guard and what CI actually runs.
- [x] Verify the documented timeout is shorter than the command, and that a
      firing `timeout` aborts under the guard's own shell options.
- [x] Verify no pre-commit hook is installed and name what
      `install-hooks.sh` does install.
- [x] Identify the Guard 3 collision.
- [ ] Decide which guards belong in CI. This is the decision that has never
      been put, and every item above exists only to inform it.
- [ ] Fix the pre-commit block in the operator guide, or remove it. As written
      it cannot succeed.
- [ ] Resolve the Guard 3 collision so the numbering means one thing.
- [ ] Decide whether Guards 5 and 6 are guards in the CI sense or one-shot
      analysis scripts. `projection_inflation_metric.sh` reads as a metric a
      human runs, not a gate.

## Notes

The useful question is not "should these run in CI", which is a policy
preference. It is "why did seven documented gates accumulate without anyone
being able to say which were live".

The answer is visible in the numbers: two of them now exit 0, and the ones
that fail are unwired, so nothing is red and nothing is asked. A suite that is
entirely green because it is not connected does not look like a suite that has
not been connected.
