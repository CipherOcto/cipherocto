---
name: the-commit-message-ci-gate-reports-shell-safe-after-scanning-nothing
description: "Layer 6 of the commit-message defence runs its scan loop over the output of git rev-list. When that command fails, the loop iterates zero times and the job prints All commit messages are shell-safe and exits 0. Reproduced locally against an unresolvable base SHA: a fatal git error on stderr, a clean bill of health on stdout."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0011-c
---

# Mission `the-commit-message-ci-gate-reports-shell-safe-after-scanning-nothing`

## Status

**Open, not implemented.** Workflow files are owner-policy surface, so this is
a proposal with a reproduction attached rather than a patch. The measurement is
reproducible today.

## The finding

`.github/workflows/commit-message-lint.yml` is Layer 6 — the commit-message
guard for GitHub, which cannot run server-side pre-receive hooks. Its scan step
builds a rev range and iterates it:

```bash
BAD=""
for rev in $(git rev-list "$RANGE"); do
  ...
done
if [[ -n "$BAD" ]]; then
  exit 1
fi
echo "All commit messages are shell-safe."
```

`for rev in $(...)` runs a command substitution whose **exit status is
discarded**. When `git rev-list` fails, the substitution yields nothing, the
loop body never runs, `BAD` stays empty, and the job reports success.

`set -euo pipefail` does not help: the substitution is not the command under
test, so its non-zero status never reaches the shell.

### Reproduced

The scan step was copied verbatim and run against a real repository, changing
only the base SHA:

```
$ BASE=<sha that does not exist locally> HEAD=<real sha> ./scan.sh
Scanning commits between deadbeef...deadbeef and 96dca3b6...d43d...
fatal: Invalid revision range deadbeef...deadbeef..96dca3b6...d43d
All commit messages are shell-safe.
$ echo $?
0
```

A `fatal` from git and a clean bill of health on adjacent lines, exit 0. On a
pull request, that annotation is not a step that turned red; it is a line
buried in a log that otherwise succeeded.

## When the base SHA is unresolvable

The base comes from `github.event.pull_request.base.sha` on pull requests and
`github.event.before` on pushes. The all-zeros case — a newly created branch —
is already handled explicitly. The **unresolvable but non-zero** case is not:

- `pull_request.base.sha` naming a commit on a branch that was deleted after
  the pull request was opened. Checkout runs with `--prune` over
  `refs/heads/*`, so a pruned branch's commits are not fetched.
- `github.event.before` naming a commit removed by a force-push and no longer
  reachable from any fetched ref.

Whether either occurs in this repository's history was **not determined**. The
structural defect stands regardless: a guard whose failure mode is to report
success is one step away from being wrong on a real repository, and the
reproduction above does not depend on the trigger being common.

The empty-string case is safe by accident rather than by design: an empty base
yields the range `..HEAD`, which is valid and scans everything.

## Proposed change

Not applied. Offered for a maintainer to accept or reject.

```bash
if ! REVS=$(git rev-list "$RANGE"); then
  echo "::error file=commit-msg,line=1::Could not resolve commit range $RANGE;" \
       "refusing to report commit messages as clean."
  exit 1
fi

COUNT=0
[ -z "$REVS" ] || COUNT=$(printf '%s\n' "$REVS" | wc -l | tr -d ' ')

BAD=""
for rev in $REVS; do
  msg=$(git log -1 --format='%B' "$rev")
  if offenders=$(dangerous_present "$msg"); then
    ...
  fi
done

if [[ -n "$BAD" ]]; then
  exit 1
fi

if [ "$COUNT" -eq 0 ]; then
  echo "Range $RANGE contains no new commits; nothing to scan."
  exit 0
fi
echo "All $COUNT scanned commit message(s) in $RANGE are shell-safe."
```

Three changes, each matching a pattern already used elsewhere in this
framework:

1. **Fail closed on an unresolvable range.** A guard that cannot resolve its
   population must say so rather than report it clean.
2. **Disclose the count.** The final line currently claims safety for an
   unstated number of commits. Every other guard in this repository now states
   its coverage after the fix to the pre-receive hook.
3. **Distinguish zero from clean.** An empty range is legitimate — nothing new
   was pushed — so it must not fail, but it must not be described as verified.

## Acceptance criteria

- [ ] A pull request whose base commit is unresolvable fails the job with a
      named reason, and does not print the shell-safe line.
- [ ] The shell-safe line states how many commits were scanned.
- [ ] An empty range exits 0 and says it scanned nothing.
- [ ] A push carrying a dangerous commit still fails, with the existing
      `lint-commit-message-failure.sh` banner intact.

## Notes

This is the same defect class as
`two-dangerous-token-rules-could-not-fire-and-one-was-silenced-by-any-earlier-word`
and `the-installer-reported-two-hooks-installed-while-one-of-them-could-not-run`,
in the one layer that runs on every pull request.

Layer 6 is also the layer that was repaired once already: the bare
`github.before` in this same file is not a GitHub context property and evaluated
to the empty string. That defect was found by reading the workflow. This one was
found by **running** it, and neither reading nor review surfaced it.

The commit-message defence is seven layers deep and every layer so far has been
inspected by running it. This is the last wired layer. Layer 7, the
`claude-bash-guard` PreToolUse hook, lives outside the repository and has not
been examined.
