# MCP Config Snippets — Per-Environment Setup

This directory contains ready-to-paste MCP server configurations for each
major AI-agent environment that supports the stdio MCP transport.

## File map

| File | Environment | Target path |
|---|---|---|
| `claude-code.json` | Claude Code | `<project>/.mcp.json` (project) or `~/.claude/.mcp.json` (user) |
| `cursor.json` | Cursor | `~/.cursor/mcp.json` |
| `continue.json` | Continue.dev | `~/.continue/config.json` |
| `windsurf.json` | Windsurf | `~/.codeium/windsurf/mcp_config.json` |
| `aider.sh` | Aider | `~/.local/bin/wa` (shell shim — no MCP) |

All four JSON snippets share the same payload:

```json
{
  "mcpServers": {
    "octo-whatsapp": {
      "command": "octo-whatsapp",
      "args": ["mcp"],
      "env": {
        "OCTO_WHATSAPP_PERSIST_DIR": "${HOME}/.local/share/octo/whatsapp"
      }
    }
  }
}
```

Continue nests under `experimental.mcpServers` for v0.x compatibility; modern
Continue (≥ 0.9) accepts the same shape at the top level.

## Per-environment setup

### Claude Code

1. Install the binary:
   ```bash
   cargo install --path crates/octo-whatsapp
   ```
2. Copy the project-level config:
   ```bash
   cp claude-code.json .mcp.json
   ```
   Or user-level (applies to every project):
   ```bash
   mkdir -p ~/.claude
   cp claude-code.json ~/.claude/.mcp.json
   ```
3. Restart Claude Code. The `octo-whatsapp` server should appear in the
   MCP server list.
4. Verify:
   - In Claude Code, run `/wa-mcp` to load the fat skill reference.
   - Invoke `tools/list` against the MCP server — expect 100 tools.

### Cursor

1. Install the binary (same as above).
2. Copy the config:
   ```bash
   mkdir -p ~/.cursor
   cp cursor.json ~/.cursor/mcp.json
   ```
3. Restart Cursor. The `octo-whatsapp` server should be visible in
   Settings → MCP Servers.

### Continue.dev

1. Install the binary.
2. Merge the snippet into your existing `~/.continue/config.json` (the
   snippet uses `experimental.mcpServers` so it does not collide with
   Continue's other settings).
3. Reload VS Code (Continue re-reads on restart).

### Windsurf

1. Install the binary.
2. Copy the config:
   ```bash
   mkdir -p ~/.codeium/windsurf
   cp windsurf.json ~/.codeium/windsurf/mcp_config.json
   ```
3. Restart Windsurf.

### Aider (no native MCP)

Aider does not support stdio MCP servers. Use the shell shim to drive the
daemon CLI:

1. Install the binary (still required — the shim wraps the CLI).
2. Install the shim:
   ```bash
   cp aider.sh ~/.local/bin/wa
   chmod +x ~/.local/bin/wa
   ```
3. Use:
   ```bash
   wa send-text +15551234567 "hello from aider"
   wa status
   wa messages-list +15551234567 50
   ```

Unknown verbs pass through to `octo-whatsapp` verbatim, so:

```bash
wa capabilities          # → octo-whatsapp capabilities
wa groups list           # → octo-whatsapp groups list
```

## Validation

After installing on any env, confirm connectivity:

```bash
# Direct probe (works for any MCP client with a stdio backend):
octo-whatsapp mcp <<<'{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Expected response: a `result` object containing 100 tool descriptors.

For programmatic verification (no daemon needed), see
`crates/octo-whatsapp/tests/mcp_config_snippets.rs` — it asserts each
JSON snippet parses, has the correct `mcpServers.octo-whatsapp` block,
and that `command` / `args` / `env` match the contract.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `command not found: octo-whatsapp` | Binary not in PATH | `cargo install --path crates/octo-whatsapp` then restart shell |
| `env OCTO_WHATSAPP_PERSIST_DIR: directory not found` | Persist dir missing | `mkdir -p ~/.local/share/octo/whatsapp` |
| `tools/list` returns zero tools | Daemon failed to start | Run `octo-whatsapp status` to inspect `last_error` |
| Continue warns "experimental.mcpServers deprecated" | Old Continue warns on the legacy path | Move the block to top-level `mcpServers`; same JSON otherwise |
| Aider shim reports `octo-whatsapp: command not found` | Binary not on PATH for the calling shell | Same as Claude Code fix above |

## Security notes

- The `OCTO_WHATSAPP_PERSIST_DIR` should be `chmod 700` — it holds the
  encrypted WA session database. Other users on the same host MUST NOT
  be able to read it.
- MCP config files contain no secrets; they only reference the binary
  and the persist directory path.
- Bearer tokens (Phase 5 Part A) are issued via `octo-whatsapp tokens
  rotate` and stored in `${OCTO_WHATSAPP_PERSIST_DIR}/security/`. They
  are NOT placed in MCP config files.

## Versioning

These snippets are versioned alongside `octo-whatsapp` releases. When
`daemon.api.version` bumps, re-run the installer (`scripts/install.sh`,
forthcoming in Session 4) or re-copy this directory.
