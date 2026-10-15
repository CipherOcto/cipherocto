---
name: layer-seven-of-the-commit-message-defence-is-not-wired-in-the-active-runtime
description: "The seven-layer defence against the 2026-09-08 commit-message incident names seven layers. Layer 7, the claude-bash-guard PreToolUse hook, is a working script that is not configured in the active agent runtime. Its backup settings file still contains the hook. Three token rationales in the shared library assert that Layer 7 blocks something."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0011-c
---

# Mission `layer-seven-of-the-commit-message-defence-is-not-wired-in-the-active-runtime`

## Status

**This is an environment finding, not a repository defect.** The guard is
installed and works; the runtime is simply not calling it. Re-enabling it
changes how every tool call in the environment behaves, so it is a decision for
the maintainer rather than an agent.

The repository-side consequence — token rationales asserting coverage that is
not running — has been corrected.

## The finding

`scripts/wrappers/dangerous-tokens.sh` opens by naming six consumers and
describing a seven-layer defence. The token categories carry phrases like
_"Layer 7 must block"_ and _"Layer 7 catches via substring match"_.

Five of the six consumers are files in this repository and were each verified
by running them during this work. The sixth is not:

| Layer | Consumer                                     | Where it lives           | Verified      |
| ----- | -------------------------------------------- | ------------------------ | ------------- |
| 2     | `scripts/hooks/commit-msg-no-backtick`       | this repository          | yes           |
| 3     | `scripts/wrappers/safe-commit`               | this repository          | yes           |
| 5     | `scripts/hooks/pre-receive-no-backtick`      | this repository          | yes           |
| 6     | `.github/workflows/commit-message-lint.yml`  | this repository          | yes           |
| 6b    | memory card `no-backtick-in-commit-messages` | agent profile            | present       |
| **7** | **`claude-bash-guard` PreToolUse hook**      | **agent runtime config** | **not wired** |

### The script is fine. The wiring is not.

`/home/mmacedoeu/.local/bin/claude-bash-guard.sh` exists, is executable, and
resolves its token library from a hardcoded repository path. Run directly with
a Claude Code hook payload:

| Command                         | Result              |
| ------------------------------- | ------------------- |
| `echo hi`                       | allowed             |
| `git commit --no-verify -m fix` | **blocked**, exit 2 |
| `git commit-tree`               | **blocked**, exit 2 |
| `gh pr create next:main`        | **blocked**, exit 2 |
| `git commit -F /tmp/msg.txt`    | allowed             |

It does exactly what it claims. Two settings files were compared:

| File                               | PreToolUse hooks                                  |
| ---------------------------------- | ------------------------------------------------- |
| `~/.claude/settings.json` (active) | **none** — only a `SessionStart` hook             |
| `~/.claude/settings.json.blank`    | `/home/mmacedoeu/.local/bin/claude-bash-guard.sh` |

The backup carries the hook and the active file does not. Nothing in the
repository can detect this: Layer 7 is not a tracked file, and no gate can fail
because a runtime stopped calling it.

### Why it matters here

The guard's whole purpose is the tool-call layer — catching `--no-verify`,
`commit-tree` and `gh pr create` **before** execution, which is the one place a
bypass can be stopped after the local hooks have already been skipped. The
memory card relies on it directly: _"Base64-encoded metachars (Layer 7
decode-and-rescan catches these)"_. That decode-and-rescan is the branch fixed
earlier today in the shared library; today nothing calls it from the tool layer.

Re-enabling is low-risk: `CIPHEROCTO_ROOT` is assigned literally inside the
script rather than read from the environment, so there is no variable to forget
and no configuration step beyond adding the `PreToolUse` entry back.

## What was changed in the repository

The shared library's header now states that the "must block" phrases describe
each layer's **job**, not a measurement of whether it is running, and that
Layer 7 is a separately installed script effective only while the runtime has
it configured. The wording is accurate whether or not the layer is later
re-enabled.

No token entry was removed. The list is correct; the enforcement location is
what moved.

## Options for the maintainer

1. **Re-enable.** Add the `PreToolUse` entry from `settings.json.blank` back
   into the active settings. Restores the layer; the script is present and
   verified working.
2. **Leave off, record why.** The in-repo layers (2, 3, 5, 6) all function, so
   the defence is not absent — it is four layers, not seven. That is a
   legitimate posture and should be written down somewhere durable.
3. **Make it detectable.** A check that fails when a documented layer is not
   wired, so this cannot go unnoticed again. It cannot live in this repository,
   because the fact it needs is in the agent profile.

## Acceptance criteria

- [x] The guard's existence, location and wiring state are established by
      inspection of both settings files, not inferred from documentation.
- [x] The guard is shown to work when invoked directly.
- [x] The repository no longer asserts that Layer 7 is currently enforcing.
- [ ] The maintainer chooses one of the three options above.

## Notes

This is the same failure shape as every other finding in this work, one level
up: a component described as part of an enforcement net, with rationales
asserting what it catches, that nothing verifies is present.

The difference is that this one cannot be fixed from the repository, and cannot
be tested by any gate in it. A check that reports on a population it never
examined is normally fixable; here the population is a file outside version
control that the repository has no way to observe.

Recorded rather than changed, because a guard that blocks tool calls is a
decision about how an environment behaves, and enabling one silently would be
the same class of mistake as this finding.
