# Plan — Skills + MCP Distribution for octo-whatsapp

## Context

`octo-whatsapp` exposes ~115 RPC handlers via a single binary in 3 modes: `daemon`, CLI subcommands, and `mcp` (stdio JSON-RPC). The surface is comprehensive (~115 RPCs, 100 advertised as MCP tools after Session A of the parity-closure plan) but operators — humans and AI agents — need curated entry points to use it effectively.

**Goal:** package the existing surface so other Claude Code instances (and Cursor / Continue.dev / Windsurf / Aider) can use it without rediscovering every gotcha.

**Out of scope for this plan:**
- **Layer 3 (WA APK proto teardown)** — deferred. The `whatsapp-rust` crate already provides 80% of the proto coverage; APK exploration yields catalog-completeness audits, not capability expansion.
- **Layer 4 (cross-env adapter layer)** — deferred. The unix socket + JSON-RPC already works for any HTTP-capable agent. No new surface needed until a specific non-Claude agent requires it.

**In scope:** Skills (Claude Code `Skill` tool), cross-env MCP config snippets, single installer script.

## Strategy

**Skill taxonomy (1 fat + 4 thin):**

| Skill | Type | Purpose |
|---|---|---|
| `wa-mcp` | fat reference | Comprehensive catalog of all 100 MCP tools, organized by category, with examples + JSON Schema references. Loaded once; agent uses MCP `tools/list` for live discovery. |
| `wa-send` | thin playbook | Outbound gotchas: peer format, text ceiling, media type selection, rate-limit floor. |
| `wa-monitor` | thin playbook | Inbound gotchas: events.ndjson tail pattern, event correlation, what events fire per op. |
| `wa-recover` | thin playbook | Bot state machine, recovery paths, reconnect vs re-onboard. |
| `wa-config` | thin playbook | daemon.toml structure, multi-account setup, bearer token lifecycle. |

**File layout** (all owned by the `octo-whatsapp` crate, installed by the installer):
```
crates/octo-whatsapp/
├── assets/
│   ├── skills/
│   │   ├── wa-mcp.md            # fat reference
│   │   ├── wa-send.md           # thin playbook
│   │   ├── wa-monitor.md        # thin playbook
│   │   ├── wa-recover.md        # thin playbook
│   │   └── wa-config.md         # thin playbook
│   └── mcp-configs/
│       ├── claude-code.json     # .mcp.json snippet
│       ├── cursor.json          # ~/.cursor/mcp.json snippet
│       ├── continue.json        # ~/.continue/config.json snippet
│       ├── windsurf.json        # VSCodium-compatible mcp.json
│       └── aider.sh             # Aider CLI shim (no native MCP)
└── scripts/
    └── install.sh               # the installer
```

**Installer behavior:**
1. Detect platform (linux / macos), architecture (x86_64 / aarch64), package manager (brew / apt / none).
2. Install `octo-whatsapp` binary to `~/.cargo/bin/` (via `cargo install`) or `~/.local/bin/` (manual download).
3. Detect existing AI-agent installations:
   - Claude Code: `~/.claude/` (and `.claude/` per project)
   - Cursor: `~/.cursor/`
   - Continue.dev: `~/.continue/`
   - Windsurf: `~/.config/Codium/User/` or platform-specific
   - Aider: no MCP — emit shell alias / wrapper script instead
4. For each detected env, drop the corresponding MCP config snippet into the right path (merging with existing config if present).
5. For Claude Code specifically, also copy the 5 skill files to `~/.claude/skills/`.
6. Print operator-facing summary: "Restart [env]. The octo-whatsapp MCP server is now available."

**Naming convention:** skills kebab-case (`wa-mcp`), MCP tool names dot.underscore (`groups.get_invite_link`), CLI subcommand tree (kebab between, no dots). Same convention as existing surfaces.

**Local-only** per 2026-07-05: no push, no PR. All commits land on `feat/whatsapp-runtime-cli-mcp`.

---

## Sessions

| Session | Scope | Files | Commits |
|---|---|---|---|
| **1** | Fat `wa-mcp` skill reference | 1 | ~3 |
| **2** | 4 thin playbooks (`wa-send`, `wa-monitor`, `wa-recover`, `wa-config`) | 4 | ~8 |
| **3** | Cross-env MCP config snippets + docs | 5 + 1 README | ~7 |
| **4** | Installer script + verification + docs | 1 + 1 README + 1 plan doc | ~6 |
| **Total** | | 13 | ~24 |

All sessions are additive and self-contained. Each ends with `cargo test` + clippy clean + commit.

---

## Session 1 — Fat `wa-mcp` reference skill (~3h, ~3 commits)

**Goal:** comprehensive catalog of all 100 MCP tools, organized by category, with examples. Loaded once when the operator invokes `wa-mcp`; agent uses `tools/list` for live schema discovery.

### Tasks

1. **Author `crates/octo-whatsapp/assets/skills/wa-mcp.md`** (~400-600 lines):
   - Header: name, version (matches `daemon.api.version`), when to invoke
   - Section per category matching MCP tool groupings:
     - Lifecycle (version, status, health, reconnect, shutdown) — 5 tools
     - Send (text/image/video/audio/voice/sticker/reaction/poll/contact/location/delete) — 11 tools
     - Messages (list/get/search/edit/mark_read/download + Phase 7: pin/unpin/forward/edit_encrypted) — 11 tools
     - Chats (list/info/pin/unpin/mute/archive/delete/typing/clear) — 9 tools
     - Groups (24 base + Phase 6.12 coordinator + Phase 6.12.1 completion + Phase 7.H gap) — 24 tools
     - Contacts (is_on_whatsapp/get_profile_picture/get_user_info/save_contact/get_business_profile) + contact.block/unblock — 7 tools
     - Profile (set_push_name/set_status/set_picture/remove_picture) — 4 tools
     - Presence (subscribe/unsubscribe/set_available/set_unavailable) — 4 tools
     - Privacy (get/set) + Blocking (get_blocklist/is_blocked) — 4 tools
     - Labels (create/delete/add_chat_label/remove_chat_label) — 4 tools
     - Media (info/fetch_sticker_pack) — 2 tools
     - Status story (send_text/send_image/send_video/revoke) — 4 tools
     - Polls (vote/aggregate) — 2 tools
     - Newsletter (list_subscribed/get_metadata/leave/create/join/send_reaction/edit_message/revoke_message) — 9 tools
     - Passkey (send_response/send_confirmation) — 2 tools
     - TcToken (issue/get/prune_expired/get_all_jids) — 4 tools
     - Events (create/respond) — 2 tools (note: events.list/show/replay/tail are operational, not domain events)
     - Identity (get_pn/get_lid/is_lid_migrated) — 3 tools
     - Daemon ops (methods.list/help, accounts.list/use/info, set_passive, set_force_active_delivery_receipts, set_client_profile, set_skip_history_sync, set_wanted_pre_key_count, set_resend_rate_limit) — 11 tools
     - Rules (list/get/create/update/patch/delete/enable/disable/approve/reload/flush/test) — 12 tools
     - Triggers (list/get/create/update/delete/run) — 6 tools
     - Audit (tail/verify) — 2 tools
     - Actions (escalate) — 1 tool
     - Envelope (encode/decode/send/send-native) — 4 tools
     - Domain (compute-hash) + Capabilities — 2 tools
   - Each section: tool list + one-line purpose + parameter shape + minimal JSON example
   - Cross-references to thin playbooks for gotchas

2. **Frontmatter** (Claude Code skill format):
   ```yaml
   ---
   name: wa-mcp
   description: Comprehensive catalog of all octo-whatsapp MCP tools. Invoke when starting work with the octo-whatsapp runtime to load the full surface; refer back when unsure which tool fits a given operation.
   ---
   ```

3. **Self-validation**: an integration test in `crates/octo-whatsapp/tests/` that loads the skill file and asserts:
   - All 100 tool names appear at least once in the body
   - Each section header matches an `EXPECTED_TOOL_COUNT`-derived category
   - File parses as valid markdown (basic check: line count > 100, headers balance)

### Verification
- `cargo test -p octo-whatsapp --features test-helpers --lib`: new self-validation test passes
- `cargo clippy ... -D warnings`: clean
- `cargo fmt --check`: clean
- Manual: open the .md in a markdown viewer, spot-check every tool appears

### Files
- `crates/octo-whatsapp/assets/skills/wa-mcp.md` (new, ~500 lines)
- `crates/octo-whatsapp/tests/skills_wa_mcp.rs` (new, ~50 lines)

---

## Session 2 — 4 thin playbook skills (~4h, ~8 commits)

**Goal:** operator gotchas beyond MCP tool descriptions. Each playbook ~100-200 lines.

### Tasks

1. **`wa-send.md`** (~150 lines):
   - Peer formats: E.164 (`+15551234567`), JID (`1234567890:12@us.s.whatsapp.net`, `120363@g.us`), contact `name` lookup
   - Text size ceiling: 65536 bytes (UTF-8), pre-flight returns `-32004 PayloadTooLarge`
   - Media type selection matrix: image (jpg/png/webp), video (mp4/3gp), audio (mp3/aac/ogg), voice (opus/ogg, `ptt=true`), sticker (webp, 512x512 max)
   - Caption: max 1024 chars; appended only on image/video/document
   - **2-second WA rate-limit floor** (`inter_call_delay_for(method)`) — non-negotiable
   - Quote / reply: `reply_to` field on send.text
   - Revoke: send.delete takes (peer, msg_id, msg_timestamp) — msg_timestamp is unix-seconds, must match the original
   - Reaction: send.reaction takes emoji (single grapheme)
   - Poll: max 12 options, max question 256 chars
   - Document send: file size cap matches envelope (16 MB); larger files fail pre-flight

2. **`wa-monitor.md`** (~150 lines):
   - `events.ndjson` location: `$OCTO_WHATSAPP_PERSIST_DIR/data/events/events.ndjson`
   - JSONL format: one `InboundEvent` per line; reverse-chronological by event id
   - Event correlation: every `send.*` returns `{message_id, peer, ts_unix_ms}`; the matching `InboundEvent::Receipt` carries the same `message_id`
   - RPC alternatives: `events.list` (paginated), `events.tail` (last N), `events.show {id}` (single), `events.replay {since_id, limit}` (catch-up)
   - Polling interval: events table is append-only; use `events.largest_id` as watermark, then `events.replay` since watermark every 1-2s
   - InboundEvent variants and what fires them (Message / Receipt / Connection / GroupChange / Presence / PushNameUpdate / etc.)
   - Backpressure: events buffer is bounded (~10k); old events age out
   - "I've seen nothing for 5 minutes" — likely SessionLost/Disconnected; check `daemon.status`

3. **`wa-recover.md`** (~200 lines):
   - Bot state machine from `daemon::BotStateMirror`:
     - `Disconnected` — transient (WS dropped, retry in 30s); `daemon.reconnect.now`
     - `PairingQr` — first-time onboarding; `octo-whatsapp-onboard qr-link`
     - `PairingCode` — phone-pair-code; `octo-whatsapp-onboard pair-link <phone>`
     - `Connected` — healthy
     - `Replaced` — another device took over; this session is dead, must re-onboard
     - `LoggedOut` — server forcibly logged out; must re-onboard from scratch (session file invalidated)
     - `SessionExpired` — refresh failed; `daemon.reconnect.now` may recover; if not, re-onboard
     - `AwaitingUserAction` — phone-side 2FA required; operator must act in phone app; hint in `bot_state_hint`
     - `AwaitingPasskey` — WA-driven WebAuthn assertion request; operator must approve on phone
   - Recovery flow decision tree (ASCII or mermaid)
   - When `daemon.reconnect.now` works vs requires `octo-whatsapp-onboard`
   - Multi-account recovery: `daemon.accounts.use <account_id>` switches the bound adapter; recovery state is per-account
   - Security token revocation: `security.revoke_all_tokens` after suspected compromise

4. **`wa-config.md`** (~150 lines):
   - `daemon.toml` location: `$OCTO_WHATSAPP_PERSIST_DIR/config/daemon.toml`
   - TOML sections: `[daemon]`, `[storage]`, `[media]`, `[events]`, `[security]`, `[observability]`, `[rules]`, `[accounts.*]`
   - Multi-account: `[[accounts]]` array; each has `id`, `persist_dir`, optional `display_name`
   - Persist dir convention: `~/.local/share/octo/whatsapp/{name}/`
   - Bearer token lifecycle: `security.rotate_token` returns new token + grace period; old token still valid for `grace_ms`
   - Hermetic mode: `hermetic_bypass = true` skips auth — for tests only; production must NOT set this
   - Event persistence tuning: `events.buffer_size`, `events.flush_interval_ms`, `events.archive_dir`
   - Reload patterns: `rules.reload` reads rules.toml from disk atomically; `daemon.reconnect.now` for runtime config changes that need restart

### Verification
- `cargo test -p octo-whatsapp --features test-helpers --lib`: per-skill self-validation tests
- Each test asserts: frontmatter parses, tool names referenced are real (cross-check against `EXPECTED_TOOL_COUNT` tool list)
- clippy + fmt clean

### Files
- `crates/octo-whatsapp/assets/skills/wa-send.md` (new)
- `crates/octo-whatsapp/assets/skills/wa-monitor.md` (new)
- `crates/octo-whatsapp/assets/skills/wa-recover.md` (new)
- `crates/octo-whatsapp/assets/skills/wa-config.md` (new)
- `crates/octo-whatsapp/tests/skills_playbooks.rs` (new, 4 self-validation tests)

---

## Session 3 — Cross-env MCP config snippets + docs (~3h, ~7 commits)

**Goal:** emit ready-to-paste MCP config for Claude Code / Cursor / Continue.dev / Windsurf / Aider.

### Tasks

1. **`assets/mcp-configs/claude-code.json`** — the `.mcp.json` shape for Claude Code:
   ```json
   {
     "mcpServers": {
       "octo-whatsapp": {
         "command": "octo-whatsapp",
         "args": ["mcp", "--name", "default"],
         "env": {
           "OCTO_WHATSAPP_PERSIST_DIR": "${HOME}/.local/share/octo/whatsapp"
         }
       }
     }
   }
   ```
   Plus a project-scope variant `.mcp.json` (relative paths)

2. **`assets/mcp-configs/cursor.json`** — Cursor's `~/.cursor/mcp.json` shape (same JSON, different filename):
   ```json
   { "mcpServers": { "octo-whatsapp": { ... } } }
   ```

3. **`assets/mcp-configs/continue.json`** — Continue.dev's `~/.continue/config.json` shape (nested under `experimental.mcpServers` or top-level `mcpServers` depending on Continue version):
   ```json
   { "mcpServers": { "octo-whatsapp": { ... } } }
   ```

4. **`assets/mcp-configs/windsurf.json`** — Windsurf's `~/.codeium/windsurf/mcp_config.json` shape:
   ```json
   { "mcpServers": { "octo-whatsapp": { ... } } }
   ```

5. **`assets/mcp-configs/aider.sh`** — Aider has no native MCP. Emit a shell wrapper that translates common subcommands:
   ```bash
   #!/usr/bin/env bash
   # Aider shell shim: route common commands to octo-whatsapp
   case "$1" in
     send-text) shift; octo-whatsapp send text "$@" ;;
     status) shift; octo-whatsapp status "$@" ;;
     # ...
   esac
   ```
   Plus docs explaining: "Aider has no MCP; this shim translates common subcommands."

6. **`docs/mcp-configs/README.md`** — operator-facing guide:
   - Per-env path table (where each env reads its config from)
   - Per-env setup steps (copy the snippet to the right path, restart env)
   - Validation: how to verify the MCP server is connected (status call, tools/list count)
   - Troubleshooting: socket errors, permission errors, version mismatches

7. **Self-validation**: a `tests/mcp_config_snippets.rs` test that:
   - Loads each JSON snippet
   - Asserts the `mcpServers.octo-whatsapp` block exists
   - Asserts `command` is `octo-whatsapp`
   - Asserts `args` starts with `mcp`
   - JSON is valid + structurally identical across envs (modulo filename)

### Verification
- `cargo test -p octo-whatsapp --features test-helpers --lib`: snippet validation tests pass
- `python3 -c "import json; json.load(open('...'))"` style sanity check on each snippet (manual)
- Each snippet parses with `jq` (manual)

### Files
- `crates/octo-whatsapp/assets/mcp-configs/claude-code.json` (new)
- `crates/octo-whatsapp/assets/mcp-configs/cursor.json` (new)
- `crates/octo-whatsapp/assets/mcp-configs/continue.json` (new)
- `crates/octo-whatsapp/assets/mcp-configs/windsurf.json` (new)
- `crates/octo-whatsapp/assets/mcp-configs/aider.sh` (new, executable)
- `crates/octo-whatsapp/assets/mcp-configs/README.md` (new)
- `crates/octo-whatsapp/tests/mcp_config_snippets.rs` (new)

---

## Session 4 — Installer script + verification + docs (~3h, ~6 commits)

**Goal:** one command to install octo-whatsapp + drop configs + drop skills for any detected env.

### Tasks

1. **`scripts/install.sh`** — bash script (POSIX-ish, no bashisms that would break macOS):
   - Section 1: **Platform detection** — `uname -s` → linux/macos; `uname -m` → x86_64/aarch64
   - Section 2: **Binary install** — prefer `cargo install --path crates/octo-whatsapp` if cargo present; fallback to `curl -fsSL https://github.com/.../releases/latest/download/octo-whatsapp-{platform}-{arch}.tar.gz | tar xz` (placeholder URL until releases exist)
   - Section 3: **Env detection** — for each of Claude Code / Cursor / Continue.dev / Windsurf, check if the config dir exists and is writable; mark "detected" set
   - Section 4: **MCP config emit** — for each detected env, copy the matching snippet to the right path, merging with existing JSON if present (preserve other MCP servers)
   - Section 5: **Skill emit (Claude Code only)** — copy the 5 skill files to `~/.claude/skills/` (mkdir -p first), merging with existing skills
   - Section 6: **Aider shim** — install `aider.sh` to `~/.local/bin/octo-aider` (or similar) with operator opt-in via `--with-aider` flag
   - Section 7: **Print summary** — list detected envs, paths written, next-step instruction ("Restart Claude Code. Run `/wa-mcp` to load the skill reference.")

2. **`scripts/install.sh` flags**:
   - `--dry-run` — print what would happen, do nothing
   - `--with-aider` — also install the Aider shim
   - `--skip-binary` — don't install the binary (config-only mode for upgrades)
   - `--uninstall` — remove configs + skills + binary (idempotent)

3. **`scripts/install.sh` tests** — bash unit tests using a tmp dir + fake config dirs:
   - Platform detection: mock uname output
   - JSON merge: existing config + new snippet = both MCP servers present
   - Idempotency: running twice produces the same state
   - --dry-run: filesystem unchanged after run

4. **`docs/distribution.md`** — operator-facing guide:
   - What the installer does (high-level)
   - Manual installation (if installer not usable)
   - Per-env setup verification (status call, tools/list count)
   - Uninstall procedure
   - Security considerations (token permissions, config file permissions)

5. **Update root `README.md`** with a "Quick start" section pointing to the installer

6. **Final verification**:
   - `bash scripts/install.sh --dry-run` in CI — exits 0, prints summary
   - End-to-end manual: install in a tmp HOME, assert all expected files exist
   - `cargo test -p octo-whatsapp --features test-helpers --lib`: all 4 sessions' tests pass

### Verification
- `bash scripts/install.sh --dry-run --skip-binary` exits 0 in this worktree
- Idempotency test: run twice in a row, second run says "no changes needed"
- Uninstall test: --uninstall removes everything the installer created

### Files
- `scripts/install.sh` (new, ~200-300 lines)
- `scripts/install_test.sh` (new, bash tests)
- `docs/distribution.md` (new)
- `README.md` (update with "Quick start" pointing to installer)
- `docs/plans/2026-07-10-octo-whatsapp-skills-mcp-distribution.md` (this file)

---

## Cross-cutting verification (after Session 4)

```bash
# All hermetic tests
cargo test -p octo-whatsapp --features test-helpers

# Live MCP integration (Session A's extended sweep still works)
cargo test -p octo-whatsapp --features live-whatsapp,test-helpers --test it_daemon_chain -- live_mcp_integration

# Installer dry-run
bash scripts/install.sh --dry-run --skip-binary

# Format + lint
cargo fmt --check
cargo clippy -p octo-whatsapp --all-targets --features live-whatsapp,test-helpers -- -D warnings
```

Final state:
- **5 skill files** in `crates/octo-whatsapp/assets/skills/`
- **5 MCP config snippets** + 1 README in `crates/octo-whatsapp/assets/mcp-configs/`
- **1 installer** + bash tests in `scripts/`
- **1 operator-facing guide** in `docs/distribution.md`
- **README.md** updated with installer pointer

Distribution story complete: operator runs `curl -fsSL ... | sh`, gets a working MCP server + skills across every detected AI agent in <60 seconds.

---

## Critical files (modified across all sessions)

| File | Why |
|---|---|
| `crates/octo-whatsapp/assets/skills/wa-mcp.md` | Fat reference, ~500 lines |
| `crates/octo-whatsapp/assets/skills/wa-send.md` | Outbound gotchas |
| `crates/octo-whatsapp/assets/skills/wa-monitor.md` | Inbound gotchas |
| `crates/octo-whatsapp/assets/skills/wa-recover.md` | Bot state machine |
| `crates/octo-whatsapp/assets/skills/wa-config.md` | Config + multi-account |
| `crates/octo-whatsapp/assets/mcp-configs/*.json` | Per-env MCP snippets |
| `crates/octo-whatsapp/assets/mcp-configs/aider.sh` | Aider shim (no native MCP) |
| `crates/octo-whatsapp/assets/mcp-configs/README.md` | Per-env setup guide |
| `scripts/install.sh` | Single-command installer |
| `scripts/install_test.sh` | Bash tests for installer |
| `docs/distribution.md` | Operator-facing distribution guide |
| `README.md` | Update with installer pointer |

## Reuse — what already works

- `tool_descriptors()` in `crates/octo-whatsapp/src/mcp_server.rs` is the authoritative list of all 100 MCP tools — Session 1's `wa-mcp.md` content is derived from this
- `EXPECTED_TOOL_COUNT = 100` constant — self-validation tests cross-check against this
- `inter_call_delay_for(method)` in `tests/it_daemon_chain.rs` — the 2s rate-limit floor pattern is reused in `wa-send.md` documentation
- Session A's `live_mcp_integration` sweep — proves the MCP bridge works; Session 4's installer verifies end-to-end via the same sweep

## Verification end-to-end

After Session 4:
- 100 MCP tools accessible via Claude Code / Cursor / Continue.dev / Windsurf (5 skill files + 4 MCP config snippets installed)
- Aider gets a shell shim for common subcommands (1 bash script)
- Single installer command: `curl -fsSL ... | sh` (or `bash scripts/install.sh`)
- Operator-facing docs in `docs/distribution.md` + per-env setup README
- ~24 commits, ~13 new files, additive on top of Session A's parity closure
- All hermetic + live tests still pass; clippy + fmt clean

## Deferred (separate future plans)

- **Layer 3 (WA APK proto teardown)** — one-shot `wa-proto-catalog.md` audit. Not blocking; the 100-tool surface is already comprehensive and the `whatsapp-rust` crate covers 80% of the proto surface.
- **Layer 4 (cross-env adapter)** — only when a specific non-Claude agent (e.g., Codex, custom Python/Node SDK) needs structured access beyond the unix-socket JSON-RPC. No work needed today.
- **5th playbook (`wa-triage`, `wa-groups`)** — deferred until operator usage signals demand. Add as Session 5+ if needed.
- **Skill auto-update mechanism** — when `daemon.api.version` bumps, skills should re-emit. Deferred; operator-driven update is sufficient for v1.

## Local-only / no push

Per user 2026-07-05, no `git push`, no PR. All commits land on `feat/whatsapp-runtime-cli-mcp` locally. Push only on explicit request.