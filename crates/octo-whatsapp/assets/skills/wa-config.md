---
name: wa-config
description: Rules + triggers + audit + accounts configuration guide. Use when an operator needs to define automation rules, schedule triggers, inspect the audit hash chain, manage multiple linked accounts, or run a one-shot action. Triggers: "create a rule", "list rules", "dry-run a rule", "schedule a trigger", "verify the audit chain", "switch account", "run action". Operator-facing; usually out of band for end-user agents.
metadata:
  version: "1.0.0"
  tools_covered: 24
  source: crates/octo-whatsapp/assets/skills/wa-mcp.md (sections 11-18)
---

# wa-config — Rules, triggers, audit, accounts, actions

Goal: configure automation rules, schedule cron-like triggers, inspect the tamper-evident audit hash chain, manage linked accounts, and run one-shot actions. Operator surface; not for end-user message flows.

## When to use this playbook

Trigger on any of:
- "list automation rules" / "create a rule" / "delete a rule"
- "dry-run this rule against the current state"
- "list scheduled triggers" / "schedule a trigger"
- "verify the audit chain" / "show audit tail"
- "list linked accounts" / "switch to account X"
- "run a one-shot action"

If the user wants to **send** messages, use `wa-send`. To **observe** events, `wa-monitor`. To **recover** from disconnects, `wa-recover`.

## Ground rules

1. **Rules and triggers mutate persistent state.** They are stored in `$OCTO_WHATSAPP_PERSIST_DIR/rules.toml` and `triggers.toml`. Always confirm with the operator before `delete`.
2. **Audit chain is append-only.** Never edit it; verification is read-only.
3. **Multi-account switching affects all subsequent RPCs.** The change is process-global. Confirm intent.
4. **No push/PR without operator authorization.** Local-only.
5. **Lifecycle-style rate limits do not apply to config RPCs** (they hit local SQLite/TOML, not WA). However, do not spam rule CRUD — each call holds a write lock.

## Tools at a glance

### Rules (12 — Phase 5 Part E)

| Tool | Purpose |
|---|---|
| `rules.list` | List all rules |
| `rules.get` | Fetch one rule by id |
| `rules.create` | Add a rule |
| `rules.update` | Patch an existing rule |
| `rules.delete` | Remove a rule |
| `rules.enable` / `rules.disable` | Toggle without delete |
| `rules.dry_run` | Simulate a rule against the current events table |
| `rules.export` | Dump rules to TOML |
| `rules.import` | Load rules from TOML (additive) |
| `rules.validate` | Check a TOML blob without persisting |
| `rules.test` | Run a rule's predicate against a synthetic event |

### Triggers (6 — Phase 5 Part E)

| Tool | Purpose |
|---|---|
| `triggers.list` | List scheduled triggers |
| `triggers.get` | Fetch one trigger |
| `triggers.create` | Add a cron-style trigger |
| `triggers.update` | Patch a trigger |
| `triggers.delete` | Remove a trigger |
| `triggers.run` | Force-run a trigger now |

### Audit (2 — Phase 5 Part E)

| Tool | Purpose |
|---|---|
| `audit.verify` | Walk the audit hash chain, return first inconsistency if any |
| `audit.tail` | Return the last N entries |

### Actions (1 — Phase 5 Part E)

| Tool | Purpose |
|---|---|
| `actions.run` | Execute a named action (e.g. `rotate_session`, `purge_old_events`) |

### Accounts (3 — Phase 6.1)

| Tool | Purpose |
|---|---|
| `daemon.accounts.list` | List linked accounts |
| `daemon.accounts.use` | Switch active account |
| `daemon.accounts.info` | Metadata for one account |

For full schemas, see `wa-mcp` §11 Rules, §12 Triggers, §13 Audit, §14 Actions, §18 Accounts.

## Workflow

### A. "Show me all automation rules"

```
1. mcp__octo-whatsapp__rules.list { enabled_only?: false }
2. For each rule, optionally rules.get { id } for full body.
3. To see what a rule would do against the current state:
   rules.dry_run { id, since_ts?: now-3600 } → returns matches.
```

### B. "Add a new rule"

```
1. Compose a rule spec — see $OCTO_WHATSAPP_PERSIST_DIR/rules.example.toml.
2. rules.validate { toml: "<spec>" } → returns OK or list of errors.
3. If valid, rules.create { toml: "<spec>" } → returns { id }.
4. Optionally rules.dry_run to confirm the rule fires on historical events.
5. Back up rules.toml before any bulk import:
   cp rules.toml rules.toml.bak.$(date +%s)
```

### C. "Schedule a trigger"

```
1. mcp__octo-whatsapp__triggers.create
   { cron: "<5-field>", action: "<named-action>", payload?: {...} }
2. cron is local-time 5-field: "M H DoM Mon DoW".
3. Confirm via triggers.list that the new trigger is present and enabled.
4. To test without waiting, triggers.run { id } → returns execution receipt.
```

Triggers are owned by the daemon process. On daemon restart, the trigger schedule is reloaded from `triggers.toml`; in-flight triggers are dropped.

### D. "Verify the audit chain"

```
1. mcp__octo-whatsapp__audit.verify
   → { ok: true, last_index: N } or { ok: false, first_bad_index: K, ... }.
2. If ok=false, surface to operator immediately. Do NOT auto-repair.
   The chain is tamper-evident; repairing it requires a documented migration.
3. For forensics, audit.tail { limit: 100 } → list recent entries with hashes.
```

### E. "Run a maintenance action"

```
1. mcp__octo-whatsapp__actions.run { name: "purge_old_events", payload: { older_than_days: 30 } }
2. Returns { started_at, completed_at, rows_affected }.
3. Actions are synchronous; some may take seconds. Plan timeout accordingly.
```

Available action names: see `wa-mcp` §14 Actions. Names are stable; new actions may be added in minor versions.

### F. "Manage linked accounts"

```
1. daemon.accounts.list → returns [{ id, label, phone, is_default }].
2. To inspect one: daemon.accounts.info { id } → returns full metadata.
3. To switch: daemon.accounts.use { id }
   → subsequent RPCs target this account.
4. To add a new linked account, use the CLI (not MCP):
   `octo-whatsapp pair --account <label>` (out of band).
```

## Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `rules.create` returns `InvalidSpec` | TOML syntax error | rules.validate first; fix; retry |
| `rules.dry_run` returns 0 matches | Predicate too narrow | Loosen; or expand since_ts |
| `triggers.create` rejects cron | Bad 5-field format | Use `man 5 crontab` syntax |
| `audit.verify` ok=false | DB corruption or partial restore | Operator investigate; do not retry |
| `daemon.accounts.use` returns `NotImplemented` | Single-account mode | Document the limitation |
| `actions.run` returns `UnknownAction` | Typo in name | Cross-check wa-mcp §14 |

## Tool reference (subset)

For full schema and examples, see `wa-mcp`:

- `wa-mcp` §11 Rules (12) — list/get/create/update/delete/enable/disable/dry_run/export/import/validate/test
- `wa-mcp` §12 Triggers (6) — list/get/create/update/delete/run
- `wa-mcp` §13 Audit (2) — verify/tail
- `wa-mcp` §14 Actions (1) — run
- `wa-mcp` §18 Accounts (3) — list/use/info

## Configuration files on disk

```
$OCTO_WHATSAPP_PERSIST_DIR/
├── session.db          # encrypted WA session (do not edit)
├── events.db           # events table (SQLite, append-mostly)
├── events.ndjson       # event mirror (newline-delimited JSON)
├── rules.toml          # rule specs
├── triggers.toml       # trigger specs
├── audit.log           # append-only hash chain
└── accounts.toml       # linked-account metadata
```

Default `$OCTO_WHATSAPP_PERSIST_DIR` = `~/.local/share/octo/whatsapp/`.

The daemon hermeticity test asserts that running the test suite does NOT touch any of these files (mtime check). Do not point tests at the live persist dir.

## Pointers

- Full tool catalog: `wa-mcp.md`
- Outbound workflow: `wa-send.md`
- Observation + queries: `wa-monitor.md`
- Recovery + lifecycle: `wa-recover.md`