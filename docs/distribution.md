# octo-whatsapp Distribution Guide

How to install the `octo-whatsapp` runtime so it is available as an MCP
server (Claude Code / Cursor / Continue.dev / Windsurf) or shell shim
(Aider). One command installs the binary + drops the right config for
every detected AI-agent environment.

## TL;DR

```bash
git clone https://github.com/cipherocto/cipherocto.git
cd cipherocto
bash scripts/install.sh                    # install everything
bash scripts/install.sh --with-aider       # also install Aider shim
bash scripts/install.sh --skip-binary      # config-only upgrade
bash scripts/install.sh --dry-run          # see the plan, change nothing
bash scripts/install.sh --uninstall        # remove everything
```

The installer is idempotent. Run it twice in a row — the second run says
"nothing to do" (well, produces the same filesystem state).

## What the installer does

| Step                | Effect                                                                                                                                                                                                            |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Platform detect** | Reads `uname -s/m`; supported: linux/macos × x86_64/aarch64.                                                                                                                                                      |
| **Env detect**      | For each AI-agent environment, checks well-known config dirs: `~/.claude/`, `~/.cursor/`, `~/.continue/`, `~/.config/Codium/User/`, `~/.codeium/windsurf/`.                                                       |
| **Binary install**  | If `cargo` is on PATH: `cargo install --path crates/octo-whatsapp --root ~/.cargo/bin --quiet`. Otherwise: copy prebuilt `target/release/octo-whatsapp` if present. `--skip-binary` skips this entirely.          |
| **MCP config emit** | For each detected env, merges `crates/octo-whatsapp/assets/mcp-configs/<env>.json` into the env's config file using jq. Existing MCP server entries are preserved; only the `octo-whatsapp` block is overwritten. |
| **Skills emit**     | When Claude Code is detected, copies `crates/octo-whatsapp/assets/skills/wa-*.md` (5 files: 1 fat reference + 4 thin playbooks) into `~/.claude/skills/`.                                                         |
| **Aider shim**      | With `--with-aider`, copies `crates/octo-whatsapp/assets/mcp-configs/aider.sh` to `${AIDER_DEST:-~/.local/bin/wa}` and `chmod 755`.                                                                               |

## Per-environment paths

| Environment      | Config file touched                                                                                                                                       | Skills copied?                    |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| **Claude Code**  | `~/.claude/.mcp.json` (user-level) or `<project>/.mcp.json` (project-level — see [Manual setup](#manual-setup-without-the-installer) for project variant) | yes, into `~/.claude/skills/`     |
| **Cursor**       | `~/.cursor/mcp.json`                                                                                                                                      | n/a (Cursor does not have skills) |
| **Continue.dev** | `~/.continue/config.json` — under `experimental.mcpServers` for legacy v0.x compat                                                                        | n/a                               |
| **Windsurf**     | `~/.config/Codium/User/mcp_config.json` (preferred) or `~/.codeium/windsurf/mcp_config.json` (legacy codeium path)                                        | n/a                               |
| **Aider**        | (none — Aider has no native MCP support; installer drops a `wa` shell shim at `${AIDER_DEST:-~/.local/bin/wa}`)                                           | n/a                               |

The installer auto-detects which env(s) are installed. To target only
one env, pre-create its config dir (the installer uses presence of the
dir as a positive signal).

## Per-environment setup verification

After installation, restart the agent and confirm:

### Claude Code

```bash
# Restart Claude Code, then in any session:
/wa-mcp                          # loads the fat MCP reference
# or invoke a thin playbook:
/wa-send /wa-monitor /wa-recover /wa-config
```

```bash
# Direct probe (any stdio MCP client works):
octo-whatsapp mcp <<<'{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
# Expect: a `result` object containing 100 tool descriptors.
```

### Cursor

Restart Cursor. Open **Settings → MCP Servers**. The `octo-whatsapp`
server should be listed and connected.

### Continue.dev

Reload VS Code (Continue re-reads `config.json` on restart). The MCP
server should appear in Continue's tool list.

### Windsurf

Restart Windsurf. The `octo-whatsapp` MCP server should appear in the
MCP panel.

### Aider

```bash
wa send-text +15551234567 "hello from aider"
wa status
wa messages-list +15551234567 50
```

Unknown verbs pass through to `octo-whatsapp`:

```bash
wa capabilities          # → octo-whatsapp capabilities
wa groups list           # → octo-whatsapp groups list
```

## Manual setup (without the installer)

If the installer cannot run on your host (no bash, no jq, no cargo,
read-only filesystem), copy snippets by hand. Each snippet lives at
`crates/octo-whatsapp/assets/mcp-configs/<env>.json`.

| Environment                     | Steps                                                                                                                                                                                                              |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Claude Code** (user-level)    | `mkdir -p ~/.claude/skills && cp crates/octo-whatsapp/assets/skills/wa-*.md ~/.claude/skills/ && cp crates/octo-whatsapp/assets/mcp-configs/claude-code.json ~/.claude/.mcp.json && chmod 600 ~/.claude/.mcp.json` |
| **Claude Code** (project-level) | Copy `claude-code.json` to `<project>/.mcp.json`; copy skills to `~/.claude/skills/` (still user-wide).                                                                                                            |
| **Cursor**                      | `mkdir -p ~/.cursor && cp crates/octo-whatsapp/assets/mcp-configs/cursor.json ~/.cursor/mcp.json`                                                                                                                  |
| **Continue.dev**                | Append the JSON from `continue.json` to your existing `~/.continue/config.json` under the `experimental.mcpServers` key.                                                                                           |
| **Windsurf**                    | `mkdir -p ~/.config/Codium/User && cp crates/octo-whatsapp/assets/mcp-configs/windsurf.json ~/.config/Codium/User/mcp_config.json`                                                                                 |
| **Aider**                       | `cp crates/octo-whatsapp/assets/mcp-configs/aider.sh ~/.local/bin/wa && chmod +x ~/.local/bin/wa`                                                                                                                  |

See `crates/octo-whatsapp/assets/mcp-configs/README.md` for the full
per-environment guide with troubleshooting.

## Uninstall

```bash
bash scripts/install.sh --uninstall
```

The uninstaller is the inverse of install:

- Removes `octo-whatsapp` binary from `~/.cargo/bin/` (if installed).
- Strips the `octo-whatsapp` block from each detected env's config file;
  preserves any other MCP server entries. If the config file ends up
  empty of MCP-relevant keys, the file itself is removed.
- Removes the `wa-*.md` skill files from `~/.claude/skills/`.
- Removes the Aider shim at `~/.local/bin/wa`.

## Security considerations

**Persist directory permissions.** `OCTO_WHATSAPP_PERSIST_DIR` (default
`~/.local/share/octo/whatsapp`) holds the encrypted WA session database.
It must be `chmod 700`. The installer does NOT set this automatically —
do it once on first install:

```bash
chmod 700 ~/.local/share/octo/whatsapp
```

**Config file permissions.** The installer sets MCP config files to
`chmod 600` after writing. Config files contain the persist-dir path,
not secrets — but 600 is the safer default for any file under
`~/.claude/`, `~/.cursor/`, etc.

**Bearer tokens.** MCP config files contain NO bearer tokens. Tokens
are issued by `octo-whatsapp tokens rotate` and stored under
`${OCTO_WHATSAPP_PERSIST_DIR}/security/`. They are referenced by the
daemon directly; agents authenticate by unix-socket peer permissions.

**Aider shim permissions.** `wa` is installed at `chmod 755`. It does
not read or write secrets — it only invokes the daemon's CLI surface.

**Source artifacts.** The installer does not embed any secret material
in the configs it copies. The snippets under
`crates/octo-whatsapp/assets/mcp-configs/` are checked into the repo
and contain only `command`, `args`, and `env.OCTO_WHATSAPP_PERSIST_DIR`.

## How the installer stays safe

| Property                      | Mechanism                                                                                                                                                                                                   |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Idempotent**                | JSON merge reads existing config, overlays `octo-whatsapp`, writes back. Running N times = running once.                                                                                                    |
| **Atomic JSON writes**        | All JSON edits go through `mktemp` + `mv -f`, so a crash mid-write leaves the previous file intact.                                                                                                         |
| **Hermetic dry-run**          | `--dry-run` skips `mkdir -p`, skips `cargo install`, writes nothing. Suitable for CI gates.                                                                                                                 |
| **Uninstall preserves peers** | The strip path uses `jq 'del(.mcpServers["octo-whatsapp"])'`, leaving other MCP servers untouched.                                                                                                          |
| **Bounded scope**             | The installer never writes outside `~/.cargo/bin/`, `~/.local/bin/`, the detected env config dir, and `${XDG_STATE_HOME:-$HOME/.local/state}/octo-whatsapp-install/install.log`.                            |
| **No network**                | The installer makes no HTTP calls. (A future release-download fallback would only run when cargo is absent and `target/release/octo-whatsapp` is absent — currently the installer just warns in that case.) |

## What the installer does NOT do

- It does not start the daemon. The MCP server is spawned by the agent
  on first call to any tool.
- It does not onboard a WhatsApp account. Run `octo-whatsapp-onboard`
  (separate CLI) for first-time QR / phone-pair.
- It does not edit `Cargo.toml` or any Rust source.
- It does not create new files under the repo. All writes go under `$HOME`.

## CI usage

The hermetic dry-run is suitable for CI:

```bash
# In .github/workflows/distribution.yml:
- name: installer dry-run
  run: bash scripts/install.sh --dry-run --skip-binary

- name: installer hermetic tests
  run: bash scripts/install_test.sh
```

Both commands exit 0 on success and do not require network access.

## Troubleshooting

| Symptom                               | Likely cause                         | Fix                                                               |
| ------------------------------------- | ------------------------------------ | ----------------------------------------------------------------- |
| `jq: command not found`               | jq missing                           | `apt install jq` (linux) / `brew install jq` (macos)              |
| `cargo install` hangs                 | large crate, cold cache              | wait; first run is ~5min                                          |
| `~/.claude/skills/` not created       | Claude Code env not detected         | `mkdir -p ~/.claude` and re-run                                   |
| Aider shim `command not found: wa`    | `~/.local/bin` not on PATH           | `export PATH="$HOME/.local/bin:$PATH"`                            |
| Config file ends up `[{...}]` (array) | old installer with first-install bug | re-run `bash scripts/install.sh` to fix; the bug is fixed in 1.0+ |
| `Permission denied` on config write   | dir not writable                     | `chmod u+w ~/.claude` (etc.) and retry                            |

## See also

- `crates/octo-whatsapp/assets/mcp-configs/README.md` — per-env setup
  guide (troubleshooting matrix, security notes).
- `crates/octo-whatsapp/assets/skills/wa-mcp.md` — fat reference of all
  100 MCP tools (load via `/wa-mcp` in Claude Code).
- `crates/octo-whatsapp/assets/skills/{wa-send,wa-monitor,wa-recover,wa-config}.md`
  — thin playbooks for outbound / inbound / recovery / config gotchas.
- `docs/plans/2026-07-10-octo-whatsapp-skills-mcp-distribution.md` —
  the 4-session distribution plan this installer closes out.
