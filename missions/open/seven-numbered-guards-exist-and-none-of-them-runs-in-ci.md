---
name: seven-numbered-guards-exist-and-none-of-them-runs-in-ci
description: "The repository defines seven numbered guards, each naming itself Guard N and citing the same long-horizon plan. Only two document checks run in CI, and neither is one of the seven. The operator guide documents a pre-commit guard that cannot work: it gives validate_cites.sh a 30 second timeout, that script takes over 120 seconds, and timeout returns 124, which under the guard's own set -euo pipefail aborts every commit. No pre-commit hook is installed at all, and Guard 3 names two different things."
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

**The timeout is shorter than the command by a factor of four.** The script
exceeded a 120-second `timeout` on two separate runs this session, exiting 124. It completes in under 280 seconds.

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
```

`scripts/lint_frontmatter.py` reports 246 of 246 documents parsing and 0
failures, after the 33 unparseable blocks were repaired.
`scripts/lint_no_vh_accepted.sh` reports `PASS=155 FAIL=0`, after four
misfenced RFC headings were renamed and two Version History sections were
added. `scripts/lint_section_cites.sh` reports `REJECTED=0`, after its
numeric-then-alpha rule was found to be inverted and rebuilt to verify that a
subsection heading exists rather than reject a token's shape.

Three are deliberately **not** in the snippet, because wiring them now
would land red for reasons that are not the repository's fault:

| Script                      | Exit | Why not yet                                                             |
| --------------------------- | ---- | ----------------------------------------------------------------------- |
| `lint_status_vh_sync.sh`    | 1    | one genuine drift, RFC-0855p-c at 1.0.0 vs 0.1.4                        |
| `lint_plan_yaml_vh_sync.sh` | 1    | examines `/home/mmacedoeu/.claude/plans`, outside the repository        |
| `validate_cites.sh`         | 1    | 1300 findings, of which 625 are its own path ambiguity and 436 are real |

Wiring a check that fails is legitimate once the failures are understood, and
all three above are understood. They are listed so the decision is informed
rather than made by whoever runs the job first.

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
