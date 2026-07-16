---
name: wa-recover
description: Connection lifecycle + pairing + session recovery guide. Use when the daemon is disconnected, the user wants to relink a phone, the session is stale, or health/version/status checks are needed. Triggers: "reconnect", "is the daemon up", "why am I disconnected", "link a new phone", "restore session", "shutdown gracefully", "what version is running", "ping health check". Diagnostic + lifecycle oriented — does not touch message content.
metadata:
  version: "1.0.0"
  tools_covered: 5
  source: crates/octo-whatsapp/assets/skills/wa-mcp.md (section 1)
---

# wa-recover — Connection + pairing + lifecycle

Goal: diagnose connection state, recover from disconnect, link/replace accounts, and shut the daemon down cleanly. Lifecycle surface, not message surface.

## When to use this playbook

Trigger on any of:
- "is the daemon connected?"
- "reconnect to WhatsApp"
- "why did we disconnect?"
- "link a new phone" / "pair with QR"
- "restore the previous session"
- "shutdown the daemon"
- "what version is running?"
- "health check" / "ping"
- "switch to account X"

If the user wants to **send** anything, use `wa-send`. If they want to **observe** events, `wa-monitor`. If they want to **configure** rules/triggers, `wa-config`.

## Tools at a glance

| Tool | Purpose |
|---|---|
| `version` | Daemon + WA crate version, build commit, build time |
| `status` | Connection state, account, last error, uptime |
| `health` | Liveness probe (returns 200 if event loop responsive) |
| `reconnect.now` | Force a reconnect (drops current session, re-handshakes) |
| `shutdown` | Graceful shutdown (flushes events + NDJSON, closes WS) |

For full schemas and examples, see `wa-mcp` §1 Lifecycle.

## Ground rules

1. **Pairing is operator-driven.** The QR/link code is shown via the CLI / log file; the agent cannot scan a QR from a phone. Surface the code to the human; do not attempt to auto-link from a screenshot.
2. **Reconnect is destructive to the active WS** but not to on-disk state. The session is preserved; reconnect re-handshakes using stored credentials.
3. **Shutdown is final.** Once `shutdown` returns OK, the daemon exits. The operator must restart it. Confirm with the caller before invoking if there is any chance they meant "pause".
4. **No push/PR without operator authorization.** Local-only.
5. **All lifecycle calls are exempt from the 2 s WA rate-limit floor** (they do not contact the WA server). However, do not call `reconnect.now` more than once per 30 s — the WA backend flags rapid reconnect storms.

## Workflow

### A. "Is everything OK?" — health probe

```
1. mcp__octo-whatsapp__health
   → returns { ok: true, ts } if the event loop is responsive.
2. If timeout or non-200, the daemon process is hung or dead.
   Surface to operator; do not attempt further RPCs.
3. On success, mcp__octo-whatsapp__status { verbose: false }
   → { connected: bool, account: <id>, last_error, uptime_secs, ... }
4. mcp__octo-whatsapp__version → for log correlation.
```

### B. "We disconnected — reconnect"

```
1. status.get { verbose: true } → identify last_error.
2. If last_error is transient (network blip, WS close 1006):
   reconnect.now → wait up to 30 s for status.connected == true.
3. If last_error is auth (401, logged out):
   DO NOT auto-reconnect. Surface to operator; session needs re-pairing.
4. If reconnect fails twice, fall back to D (full restart).
```

### C. "Link a new phone" — first-time pairing

```
1. Verify no session is loaded: status.get { verbose: true }
   → session_loaded must be false.
2. Start the daemon in pair mode (CLI flag, not RPC — see octo-whatsapp CLI docs).
3. Daemon prints QR / link code to stderr / log file.
4. Operator scans with phone.
5. Wait for status.connected == true (poll ≤ 1 Hz, timeout 120 s).
6. Persist session: the daemon auto-persists to $OCTO_WHATSAPP_PERSIST_DIR.
```

This workflow does not use any MCP tool to drive pairing; the daemon owns that loop. The MCP tools only observe state.

### D. "Full restart" — stop, restart, verify

```
1. mcp__octo-whatsapp__shutdown → wait for process exit (≤ 10 s).
2. Operator starts the daemon again (out of band).
3. health → version → status to confirm.
4. If the new daemon comes up disconnected, jump to B.
```

### E. "Switch account" — multi-account

```
1. mcp__octo-whatsapp__daemon.accounts.list
   → returns [{ id, label, is_default }].
2. mcp__octo-whatsapp__daemon.accounts.use { id }
   → switches the active account for subsequent RPCs.
3. status.get to confirm the new account is connected.
```

Multi-account support is gated on the daemon running in multi-account mode. Single-account daemons return `NotImplemented` for `accounts.use`.

### F. "What version are we on?" — diagnostic snapshot

```
1. mcp__octo-whatsapp__version
   → { daemon: "1.0.0+phase5", whatsapp_rust: "<sha>", wacore: "<sha>", ... }
2. Cross-check against docs/CHANGELOG.md for known issues.
```

Always include `version` output in bug reports.

## Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `health` timeout | Daemon deadlocked | SIGTERM → restart |
| `status.connected == false` after fresh start | WA backend slow | Wait 30 s; if still down, B |
| `reconnect.now` returns 401 | Logged out | Operator re-pair (C) |
| `shutdown` returns but process still alive | Stuck flush | SIGKILL after 10 s |
| `daemon.accounts.use` says `NotImplemented` | Single-account mode | Document; do not retry |

## Tool reference (subset)

For full schema and examples, see `wa-mcp`:

- `wa-mcp` §1 Lifecycle — version/status/health/reconnect.now/shutdown
- `wa-mcp` §18 Accounts — daemon.accounts.list/use/info (Phase 6.1)

## Recovery runbook (TL;DR)

```
Connected + healthy?    → no action needed.
Connected but stale?    → status.get → reconnect.now.
Disconnected, auth?     → operator re-pair (C).
Disconnected, network?  → reconnect.now → wait 30 s → status.get.
Daemon dead?            → operator restart (D).
Multiple accounts?      → daemon.accounts.list/use.
```

Always end any recovery with: `version` + `status` + `health` snapshot to confirm.

## Pointers

- Full tool catalog: `wa-mcp.md`
- Outbound workflow: `wa-send.md`
- Observation + queries: `wa-monitor.md`
- Rules + triggers + audit: `wa-config.md`