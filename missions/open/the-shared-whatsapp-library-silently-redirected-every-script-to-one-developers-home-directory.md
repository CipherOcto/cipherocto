---
name: the-shared-whatsapp-library-silently-redirected-every-script-to-one-developers-home-directory
description: "scripts/lib-octo-wa.sh is sourced by eight scripts and passed OCTO_WHATSAPP_DATA_DIR as a literal absolute path inside an env invocation, which overrides whatever the operator configured. An operator who set the variable was silently pointed at another user's data directory, where the corpus does not exist."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-14T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `the-shared-whatsapp-library-silently-redirected-every-script-to-one-developers-home-directory`

## Status

Fixed and verified with a stub daemon that reports the directory it was
actually handed. No hardcoded personal path remains in any tracked shell script.

## The finding

`scripts/lib-octo-wa.sh` is the shared library for the WhatsApp tooling. Eight
scripts source it. Its `mcp_call` primitive — the single function every one of
those eight routes through — passed the data directory like this:

```bash
env XDG_RUNTIME_DIR=/tmp/octo-wa-run \
    OCTO_WHATSAPP_DATA_DIR=/home/mmacedoeu/.local/share/octo/whatsapp \
    "$OCTO_WA_BIN" ...
```

`env NAME=value` **overrides** the inherited environment. The assignment was
not a default; it was a replacement.

### Measured

A stub binary was substituted for `octo-wa` that echoes the directory it
received:

```
caller set OCTO_WHATSAPP_DATA_DIR=/tmp/watest/OPERATOR-CHOSEN-DIR
binary received:               /home/mmacedoeu/.local/share/octo/whatsapp
```

The operator's choice was discarded silently. There is no warning and no log
line — the library does not know it did it.

### Why this is the same defect as everything else in this repository

This is a check reporting a result about a corpus it never opened. The daemon
is handed a directory that does not exist on the operator's machine, reads
nothing, and the calling script reports an empty account — "0 contacts",
"no groups found" — as though the account were genuinely empty. The operator's
next move is to distrust their own data.

Two independent signals say the hardcode is wrong rather than intended:

- `scripts/run-octo-whatsapp.sh` uses the correct pattern for the same setting:
  `DATA_DIR="${OCTO_WHATSAPP_DATA_DIR:-$HOME/.local/share/octo/whatsapp}"`
- `crates/octo-whatsapp/src/config.rs` documents `OCTO_WHATSAPP_DATA_DIR` as
  honoured, so the variable is part of the daemon's public configuration.

The library was the only place that defeated it.

### The same file pinned a binary to one workspace

```bash
: "${OCTO_WA_BIN:=/home/mmacedoeu/_w/ai/cipherocto/target/debug/octo-whatsapp}"
```

Every script sourcing the library pointed at a path inside one developer's
checkout. Three more scripts carried their own copies of that literal as a
`${OCTO_WA_BIN:-...}` fallback — including one pointing at a **`.worktrees/`
directory that does not exist on this machine at all**.

## The fix

- `OCTO_WHATSAPP_DATA_DIR` defaults from `$HOME`, matching `run-octo-whatsapp.sh`,
  and `mcp_call` passes the resolved value through.
- `OCTO_WA_BIN` resolves from the library's own location: an explicit setting,
  then a debug build in this checkout, then `octo-whatsapp` on `PATH`.
- The three dead `${OCTO_WA_BIN:-/home/mmacedoeu/...}` fallbacks are removed.
  They were unreachable once the library started setting the variable, and
  wrong if ever reached.
- `join-enrich-table.sh`, which does its own `env` invocation outside the
  library, passes the resolved variable too.

## Verified

| Condition                                       | Daemon received                                |
| ----------------------------------------------- | ---------------------------------------------- |
| Operator sets the variable (fixed)              | `/tmp/watest/OPERATOR-CHOSEN-DIR`              |
| Operator sets nothing, `HOME=/tmp/.../fakehome` | `/tmp/.../fakehome/.local/share/octo/whatsapp` |
| Operator sets the variable (before)             | `/home/mmacedoeu/.local/...` **discarded**     |

`OCTO_WA_BIN` resolves to the checkout's debug build when present, and to
`octo-whatsapp` on `PATH` when the library is copied somewhere without one. All
eight consuming scripts still source the library cleanly.

## Not a defect

`run-live-telegram-tests.sh` and `telegram-onboard-qr.sh` reference `/home/ci`.
Those are paths **inside a container**, mounted deliberately, not host paths.
`telegram-onboard-qr.sh` also cites a local tool checkout in a provenance
comment, which is the correct place for that fact. Both were left alone.

`scripts/__pycache__/` contains compiled bytecode with embedded build paths, but
it is untracked and gitignored, so no personal path is committed.

## Acceptance criteria

- [x] An operator-supplied `OCTO_WHATSAPP_DATA_DIR` reaches the daemon.
- [x] The default derives from `$HOME`, not a fixed path.
- [x] `OCTO_WA_BIN` resolves from the checkout, then `PATH`.
- [x] No `/home/<user>/_w` path remains in any tracked shell script.
- [x] All eight consuming scripts still source the library.

## Notes

The same lesson as the token library, one layer down: a file that exists to be
the single source of truth is the highest-leverage place for a wrong constant,
because every consumer inherits it silently. Here the blast radius was eight
scripts and the failure was an empty result presented as a fact.

It was found by asking what the shared library in this repository actually does,
not by reading the eight scripts that use it — each of those eight looks
unremarkable on its own.
