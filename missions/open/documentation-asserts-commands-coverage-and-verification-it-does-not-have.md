---
name: documentation-asserts-commands-coverage-and-verification-it-does-not-have
description: "Four artefacts in this repository state a command, an enforcement location or a verification result that was not true when written: CLAUDE.md documents a script that never existed, validate_cites.sh calls itself a blocking pre-commit hook, the token library asserts a layer blocks things it cannot see, and a skill doc records a passing test row for a rule that could not fire."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `documentation-asserts-commands-coverage-and-verification-it-does-not-have`

## Status

All four instances found so far are corrected or filed. The census is not
exhaustive and the mission stays open for that reason.

## The finding

This repository documents itself carefully and the documentation is mostly
true. Four statements were not, and each one asserts something a reader would
reasonably rely on: a command to run, an enforcement point, or the result of a
check.

| #   | Artefact                                | Claimed                                               | Actual                                       |
| --- | --------------------------------------- | ----------------------------------------------------- | -------------------------------------------- |
| 1   | `CLAUDE.md`                             | "Documentation Script: `scripts/init-docs.sh`"        | never existed in this history                |
| 2   | `scripts/validate_cites.sh`             | "Guard 2: pre-commit hook", "Blocking on commit: YES" | not a hook, and now forbidden from being one |
| 3   | `scripts/wrappers/dangerous-tokens.sh`  | "Layer 7 must block", "Layer 7 catches"               | Layer 7 is not wired in the runtime          |
| 4   | `scripts/wrappers/safe-commit-skill.md` | ten-row table, "Got: OK" on every row                 | nine rows honest, one impossible             |

### 1. The instruction file documents a script that never existed

`CLAUDE.md` is loaded as project instructions by every agent session. Under
"Documentation Script" it gave:

```bash
scripts/init-docs.sh
```

Verified four ways: no file at that path, no tracked file matching
`init*doc*` under any name, `git log --diff-filter=A` empty, and
`--diff-filter=D` empty. It was never added and never removed — it never
existed.

Every other path referenced anywhere in `CLAUDE.md` resolves: the BLUEPRINT,
the engineering rules, the branch-strategy documents, the RFC stage
directories, the token scripts, and all three `docs/`-relative documents. This
was the only one.

The section now states that `/docs` is already in the tree, that no scaffolding
script exists, and that the conventions live in `docs/BLUEPRINT.md`.

### 2. A guard named itself after an enforcement it never performed

`validate_cites.sh` opened with `Guard 2: Pre-commit § cite validation hook`
and `Blocking on commit: YES`. It was not installed as a hook, and by the
standing rule it must never be. Corrected, with the false description
recorded.

### 3. Token rationales assert a layer that is not wired

The library's header lists a `claude-bash-guard` PreToolUse hook as a consumer,
and two category rationales say that layer "must block" or "catches" a specific
vector. That guard is a real, working script — run directly it blocks
`--no-verify`, `commit-tree` and `gh pr create` — but the active agent settings
file carries no `PreToolUse` hook at all, while the backup settings file does.
The header now says those phrases describe each layer's job rather than a
measurement of whether it is running. See the dedicated entry for the wiring
question, which is the maintainer's to decide.

### 4. A verification table recorded an impossible pass

The skill doc's ten-row table marked every row `OK`. Re-running all ten against
the pre-fix library: nine honest, one false. Row H expected
`> redirect attempt` to be refused, and it was allowed — the `>`, `<` and `!`
rules compared literally and could never match. The table now names the two
suites that actually cover each row and records that row H was untrue until the
fix.

## Method

Each instance was found by **running the thing the document describes**, not by
reading the document and not by grepping for the right words:

- `scripts/init-docs.sh` — run it, then search history for it under any name.
- Row H — feed all ten inputs to the pre-fix library and compare.
- Layer 7 — invoke the guard directly, then compare two settings files.
- The pre-receive hook — push to a real bare repository.

A pattern-matching sweep for words like "verified" or "gated" would have found
more candidates and produced more false positives. Each of these four looked
unremarkable in isolation; three of the four documents were correct about
everything else they said.

## Why it matters

Documentation that asserts verification is the same failure as a check that
reports on a population it never examined: a claim about work that did not
happen. It is worse in one respect, because there is no exit code to inspect —
a reader either believes the claim or does not, and a table of green ticks is
the most persuasive form a claim can take.

The verification table is the clearest case. Nine honest rows made the tenth
look like data rather than an assumption, which is exactly why it survived.

## Acceptance criteria

- [x] `CLAUDE.md` no longer instructs an agent to run a nonexistent script.
- [x] Every path referenced in `CLAUDE.md` resolves.
- [x] `validate_cites.sh` no longer describes itself as a blocking hook.
- [x] The token library distinguishes a layer's job from its running state.
- [x] The skill doc's table names the suites that cover each row.
- [ ] The census continues: remaining missions, RFCs and docs have not been
      swept for the same class.

## Notes

The earlier conclusion in this work — that the token library's own suite was
sound and only its corpus incomplete — was itself an unexamined claim about a
document. It took finding the skill doc's false row to establish that the
library had been misdescribed elsewhere too.

A verification result recorded in prose deserves the same scepticism as one
recorded in a test suite, and rather more, because nobody reviews prose against
the code.
