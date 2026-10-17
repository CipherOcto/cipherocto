---
name: the-installer-reported-two-hooks-installed-while-one-of-them-could-not-run
description: "scripts/install-hooks.sh prints installed and done for every hook it links, without ever running one. Its two hooks resolve the shared token library by different rules, so the symlink install left pre-receive pointing at a directory that does not exist. The installer reported success for a hook that failed on first use."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0011-c
---

# Mission `the-installer-reported-two-hooks-installed-while-one-of-them-could-not-run`

## Status

Both defects are **fixed and mutation-verified**. The installer's verification
was confirmed to fail against the broken hook and pass against the fixed one, in
a scratch repository, in both install layouts.

## The finding

`scripts/install-hooks.sh` links both hooks into `.git/hooks` as symlinks and
prints a line per hook followed by `done.` It never executed any of them.

Installing a hook and having a working hook are different things. Every hook in
this framework resolves its token list by a path relative to its own location,
so a hook whose resolution rule disagrees with its sibling installs cleanly and
then fails on first use — while the installer has already reported success.

### The two hooks disagreed

| Hook          | Resolves its own path           | Symlink install points at |
| ------------- | ------------------------------- | ------------------------- |
| `commit-msg`  | `dirname "$(readlink -f "$0")"` | `scripts/hooks`           |
| `pre-receive` | `cd "$(dirname "$0")" && pwd`   | `.git/hooks`              |

Under the symlink install that `install-hooks.sh` performs, `readlink -f` follows
the link back to the tracked script and finds `scripts/wrappers/`. Plain
`dirname` does not, and looks for `.git/hooks/../wrappers/`, which does not
exist. Run in place:

```
.git/hooks/pre-receive: line 31: .../.git/hooks/../wrappers/dangerous-tokens.sh:
                        No such file or directory
```

**Fixed.** `pre-receive` now uses `readlink -f` like its sibling. One rule, two
hooks, and the real-file server install is unaffected because `readlink -f`
returns a plain file's own path.

### The installer now proves each hook runs

Every installed hook is executed once with the minimal input its own contract
requires — an empty message file for `commit-msg`, empty stdin for the rest —
and the result is reported:

```
installed commit-msg -> .../commit-msg-no-backtick (verified)
installed pre-receive -> .../pre-receive-no-backtick BUT IT DOES NOT RUN (exit 1)

install-hooks: 1 of 2 hook(s) installed but unusable:
  /path/.git/hooks/pre-receive
A hook that cannot run is worse than no hook: it looks like the
guard is present. Check that the hook resolves its token library.
```

and the installer **exits 1**. Failing closed while printing `done.` would have
been the same defect one layer down.

## A bug in the fix, caught by testing it

The first version of the verification ran the probe as a bare command and read
`$?` on the next line. Under `set -e` the non-zero probe aborted the script
_before_ the diagnostic printed, so the installer exited 1 having said nothing
about which hook was broken. That is failing closed with no explanation, which
is the defect restated.

The probe now runs inside `if ...; then rc=0; else rc=$?; fi`. Verified: the
broken case prints the full report, the fixed case prints `done.` and exits 0.

## A cosmetic bug found on the way

The preserve message printed `$base`, the source filename, rather than the
destination the file was actually renamed from:

```
preserved existing commit-msg-no-backtick -> commit-msg-no-backtick.preserved
```

Nothing named `commit-msg-no-backtick` existed in `.git/hooks`. It now prints
the real name, `commit-msg`.

## An environment finding that is NOT the repository's to fix

`init.templateDir` is set to `/home/mmacedoeu/.git-template/`, which contains
`hooks/commit-msg` and `hooks/pre-receive` dated 2026-09-08. **Every `git init`
on this machine installs those copies**, and they are a self-contained older
implementation — no `source`, no `wrappers`, no shared library.

They are also weaker than the repository's guard. On a commit message whose body
contains a plain semicolon:

| Hook                                             | Result     |
| ------------------------------------------------ | ---------- |
| `/home/mmacedoeu/.git-template/hooks/commit-msg` | **allows** |
| `scripts/hooks/commit-msg-no-backtick`           | refuses    |

This is outside the repository and affects every repository this user creates,
so it is reported rather than changed. The two candidate remedies — refresh the
template copies, or drop `init.templateDir` and rely on `install-hooks.sh` — are
environment decisions for the maintainer, not repository ones.

## Acceptance criteria

- [x] Both hooks resolve their own path by the same rule.
- [x] `pre-receive` works as a symlink installed by `install-hooks.sh`.
- [x] `pre-receive` still works as a real file in a bare repository, verified by
      a real push.
- [x] `install-hooks.sh` executes each hook it installs and reports the result.
- [x] A hook that cannot run makes the installer exit 1 with a named reason.
- [x] Verified in both directions: the check fires on the broken hook and is
      silent on the working one.
- [x] The preserve message names the file actually preserved.

## Notes

The question that produced this was deliberately narrow: having just corrected
one wrong install instruction, ask whether the installer that automates the same
job had been run at all.

It had not. The installer had never been executed in this repository — the
hooks in `.git/hooks` were symlinks created by something else — so the
divergence between the two hooks had survived since 2026-09-08.

A guard that has never been executed is a hypothesis about a guard. Every claim
above comes from running it, and one claim above was wrong on the first attempt
because the installer aborted before it could explain itself.
