---
name: safe-commit
description: ALWAYS invoke `safe-commit` from ~/.local/bin/ for `git commit`. NEVER pass `-m` with potential shell metachars. Verified by 10/10 test cases.
---

# safe-commit skill — Layer 4 of [no-backtick-in-commit-messages]

## When to use this skill

Every time you (Claude) are about to invoke `git commit` in the
Bash tool, use `safe-commit` INSTEAD.

## Why

On 2026-09-08 (RFC-0011-c close-out), the shell heredoc for a commit
message body contained the literal backticks `` `git push origin next` ``
as _stylistic_ code-style markdown. Bash performed POSIX §2.6.3
command substitution BEFORE handing the string to git, executing
the embedded commands as a side effect. 4 fix commits landed on
`origin/next` inadvertently.

Bash CANNOT be told "this is markdown". Backticks = command substitution.
Period.

## How

```sh
# WRONG — bash expands `git push origin next` as command substitution:
git commit -m "User owns \`git push origin next\` per init rule."

# WRONG — same bug class:
git commit -m "$(date)"

# RIGHT — pipe via tempfile (no shell expansion of contents):
cat > /tmp/commit-msg.txt <<'EOF'
chore(missions): archive 0011-c after DRY closure

Per multi-round DRY review pattern: R7 + R8 both zero-finding rounds.

User owns push per `docs/05-process/engineering-rules.md` §Git workflow.
EOF
git commit -F /tmp/commit-msg.txt

# ALSO RIGHT — use the Layer 3 wrapper which validates -m args:
safe-commit -m "User owns push per `docs/05-process/engineering-rules.md` §Git workflow"
safe-commit -F /tmp/commit-msg.txt
```

## Mandatory pre-commit self-check

Before ANY `git commit` invocation in the Bash tool, run these in order:

1. If the message contains backticks: REFUSE. Use `safe-commit` with
   `-F <tempfile>` so the body never enters shell expansion context.
2. If the message contains `$(`, `;`, `&&`, `||`, or `|`: REFUSE.
   Same fix.
3. If the message is clean, prefer `safe-commit` over `git commit` —
   the wrapper adds defense at zero cost.

## Tempfile workflow (canonical, copy-pasteable)

```sh
cat > /tmp/commit-msg.txt <<'EOF'
<type>(<scope>): <subject>

<body>

<footer refs>
EOF
git commit -F /tmp/commit-msg.txt
```

The `<<'EOF'` (single-quoted heredoc tag) is mandatory — bash leaves
the body literal, no `$VAR` or `$(cmd)` expansion. This is the
bulletproof path that Layer 1 + Layer 2 + Layer 3 + Layer 4 all
agree on.

## What this skill is NOT

- Not a replacement for the memory rule [no-backtick-in-commit-messages].
  Cite that memory rule FIRST; this skill is the tooling reference.
- Not a hook installer. Layer 2 (commit-msg hook) lives at
  `scripts/hooks/commit-msg-no-backtick`. Layer 3 (safe-commit wrapper)
  lives at `scripts/wrappers/safe-commit` and `~/.local/bin/safe-commit`.

## Test coverage

These cases are exercised by two suites, both runnable by hand:

| Suite                              | What it covers                                      |
| ---------------------------------- | --------------------------------------------------- |
| `scripts/test-dangerous-tokens.sh` | the token library these cases go through (68)       |
| `scripts/test-safe-commit.sh`      | how this wrapper recognises a message argument (10) |

| #   | Input                               | Want   | Covered by       | Got  |
| --- | ----------------------------------- | ------ | ---------------- | ---- |
| A   | ``fix: `echo HI` ``                 | refuse | token suite      | pass |
| B   | `fix: clean body`                   | accept | token suite      | pass |
| C   | `fix: has $(date)`                  | refuse | token suite      | pass |
| D   | `fix: a && b`                       | refuse | token suite      | pass |
| E   | `fix: a \|\| b`                     | refuse | token suite      | pass |
| F   | `fix: a \| b`                       | refuse | token suite      | pass |
| G   | `fix: per RFC-0011-c §Roles`        | accept | token suite      | pass |
| H   | `> redirect attempt`                | refuse | `LEADING_GT_TOP` | pass |
| I   | `fix: update docs`                  | accept | token suite      | pass |
| J   | `fix: crates/octo-cli/src/commands` | accept | token suite      | pass |

Row H is worth a note. This table previously recorded it as passing, and it
was not: the `>`, `<` and `!` rules were written with a quoted right-hand side,
which bash compares literally, so they could never match anything. Row H was a
recorded verification of a behaviour that did not exist. The rules were fixed
and `LEADING_GT_TOP` added to the token suite, and the row is honest only from
that point on.
