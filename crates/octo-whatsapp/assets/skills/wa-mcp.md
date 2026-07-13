---
name: wa-mcp
description: Comprehensive reference for all 100 MCP tools exposed by `octo-whatsapp` daemon. Use when an MCP-capable agent (Claude Code, Cursor, Continue.dev, Windsurf) needs to discover, call, or reason about WhatsApp operations through the `octo-whatsapp mcp` stdio transport. Covers tool name, parameters, return shape, and one working example per tool. Load this skill before calling any `mcp__octo-whatsapp__*` tool.
metadata:
  version: "1.0.0"
  tool_count: 100
  source: crates/octo-whatsapp/src/mcp_server.rs (EXPECTED_TOOL_COUNT=100)
---

# wa-mcp — full MCP tool reference (100 tools)

The `octo-whatsapp` daemon speaks MCP over stdio. When the MCP client is wired
up (Claude Code, Cursor, Continue.dev, Windsurf, Aider via the bash shim in
`assets/mcp-configs/`), tools appear under the prefix `mcp__octo-whatsapp__*`
(or whatever the client names the server). This file is the canonical catalog.

**Ground rules** (read once, internalize forever):

1. Tool names are `dot.underscore`. The first segment is the namespace
   (`send`, `groups`, `messages`, `daemon`, `rules`, ...).
2. Every tool returns a JSON object. Successful results land in the response
   payload; errors come back as a JSON-RPC error envelope with `code` +
   `message`. Treat `404 item-not-found`, `409 conflict`, and `429
   rate-limited` as recoverable — peer-dependent and timing-dependent.
3. **WA rate-limit floor**: 2 seconds between any pair of WA calls. The
   daemon enforces this on outbound IQ/send traffic; floods of MCP
   `send.*` calls in one turn will start rejecting past the floor. If a call
   429s, sleep then retry.
4. **Peer format**: a peer is one of three shapes:
   - `E164` (international phone): `+15551234567` (US), `+5511999999999` (BR).
   - `LID` (WA-issued): bare numeric string `<digits>@lid` after the
     `lid_normalize` step (the daemon returns the LID form on first contact).
   - `JID` (group or broadcast): `<id>@g.us`, `<id>@broadcast`, `<id>@newsletter`.
5. **Bot state**: before any `send.*` or outbound IQ, check `status.get`. If
   `bot_state` is `Disconnected` or `NeedsQr`, calls return `503` — wait for
   `Connected`.
6. **Event-table ground truth**: every send, every inbound, every receipt,
   every group change produces a row in the `events` table (NDJSON on disk
   under `~/.local/share/octo/whatsapp/events.ndjson`). `events.list`,
   `events.show`, `events.replay`, `events.tail` are how you replay state.
7. **No push, no PR** for the daemon crate per operator instruction
   2026-07-05. Work happens on `feat/whatsapp-runtime-cli-mcp` locally.

---

## Table of contents

| § | Category | Count | Tool prefix(es) |
|---|---|---:|---|
| 1 | Lifecycle | 5 | `version`, `status`, `health`, `reconnect.now`, `shutdown` |
| 2 | Send (media + control) | 11 | `send.*` |
| 3 | Messages | 6 | `messages.*` |
| 4 | Chats | 8 | `chats.*` |
| 5 | Groups (basic) | 4 | `groups.{create,list,info,leave}` |
| 6 | Groups coordinator | 14 | `groups.{destroy,…}` |
| 7 | Groups completion | 6 | `groups.{set_announce,…}` |
| 8 | Groups gap list (7.H) | 5 | `groups.{get_invite_link,…}` |
| 9 | Media | 1 | `media.info` |
| 10 | Envelope (DOT/1) | 4 | `envelope.*` |
| 11 | Capabilities + domain | 2 | `capabilities`, `domain.compute-hash` |
| 12 | Events (loss recovery) | 4 | `events.{list,show,replay,tail}` |
| 13 | Agent discovery | 3 | `clients.list`, `daemon.methods.list/help` |
| 14 | Security tokens | 3 | `security.*` |
| 15 | Rules CRUD + dry-run | 12 | `rules.{list,get,create,…}` |
| 16 | Triggers CRUD + run | 6 | `triggers.{list,get,…}` |
| 17 | Audit hash chain | 2 | `audit.{tail,verify}` |
| 18 | Actions | 1 | `actions.escalate` |
| 19 | Accounts (multi-bot) | 3 | `daemon.accounts.{list,use,info}` |
| **Total** | | **100** | |

---

## 1. Lifecycle (5)

Read these first. Probe daemon state before any send.

### `version`

Get the daemon version info.

- **Input**: `{}`
- **Returns**: `{ "version": "0.1.0", "api_version": "1.0.0+phase5", "git_sha": "…" }`
- **Example**:
  ```json
  {}
  ```
  → `{"version":"0.1.0","api_version":"1.0.0+phase5","git_sha":"deadbeef"}`

### `status`

Daemon runtime status — boot state, bot state, session health.

- **Input**: `{}`
- **Returns**:
  ```json
  {
    "boot_state": "Connected",   // Booting | Connected | Degraded | ShuttingDown
    "bot_state":  "Connected",   // Connected | NeedsQr | Disconnected
    "session_health": "ok",      // ok | degraded | broken
    "uptime_secs": 12345,
    "linked_phone": "+15551234567",
    "active_lid":   "1234567890@lid"
  }
  ```

### `health`

Liveness/readiness summary.

- **Input**: `{}`
- **Returns**: `{ "ready": true, "checks": { "ws": "ok", "db": "ok", "fs": "ok" } }`
- **Use**: when orchestrating, gate `send.*` calls behind
  `health.ready == true`.

### `reconnect.now`

Force a reconnect of the underlying WebSocket. Tears down the current session
and re-authenticates.

- **Input**: `{}`
- **Returns**: `{ "ok": true, "previous_session": "abc", "new_session": "def" }`
- **When**: rate-limit 429s persist, the bot is in `Disconnected` state, or
  you rotated credentials and want a fresh WS handshake.

### `shutdown`

Gracefully shut down the daemon. In-flight RPC returns its result first.

- **Input**: `{}`
- **Returns**: `{ "ok": true }` (then daemon exits)
- **When**: end of automation run; release the unix socket cleanly so the
  next `daemon` invocation can bind it.

---

## 2. Send (media + control) (11)

All `send.*` require the daemon to be `bot_state == Connected`. Returns include
`{message_id, status, peer, ts_unix_ms}`. The `message_id` is the receipt
ground truth — wait for the matching `Receipt` event in `events.list` with
`state == ServerAck` to confirm delivery to the WA server.

### `send.text`

Send a text message to a peer.

- **Required**: `peer: string` (E164, LID, or JID), `text: string`
- **Limits**: text max 65536 UTF-8 bytes; 65537 → `PayloadTooLarge`
- **Returns**: `{message_id, status: "queued", peer, ts_unix_ms}`
- **Example**:
  ```json
  {"peer": "+15551234567", "text": "hello"}
  ```

### `send.image`

Send an image with optional caption.

- **Required**: `peer`, `file: string` (absolute path, max 16 MB)
- **Optional**: `caption: string`
- **Returns**: `{message_id, status, peer, ts_unix_ms, mime_type: "image/jpeg"}`

### `send.video`

Send a video with optional caption.

- **Required**: `peer`, `file` (max 16 MB)
- **Optional**: `caption`

### `send.audio`

Send an audio file.

- **Required**: `peer`, `file`

### `send.voice`

Send a voice-note (PTT) audio file.

- **Required**: `peer`, `file`
- **Note**: WA wraps the audio as a PTT bubble (single-tap play).

### `send.sticker`

Send a sticker image (WEBP).

- **Required**: `peer`, `file`
- **Note**: must be WEBP; PNG/JPEG gets rejected with
  `400 unsupported-mime-type`.

### `send.reaction`

React to a message with an emoji.

- **Required**: `peer`, `msg_id: string`, `emoji: string` (single emoji)

### `send.poll`

Send a poll to a peer.

- **Required**: `peer`, `question: string`, `options: array<string>` (2-12 entries)
- **Optional**: `multi: boolean` (multi-select)
- **Returns**: `{poll_id, message_id, peer}`

### `send.contact`

Send a vCard contact.

- **Required**: `peer`, `vcard: string` (full BEGIN:VCARD … END:VCARD text)

### `send.location`

Send a location pin.

- **Required**: `peer`, `lat: number`, `lon: number`
- **Optional**: `name: string`

### `send.delete`

Delete (revoke) a previously sent message.

- **Required**: `peer`, `msg_id`, `msg_timestamp: integer` (epoch seconds)
- **Returns**: `{revoked: true, peer, msg_id}`
- **Note**: revocation is permanent on the receiver side too.

---

## 3. Messages (6)

### `messages.list`

List recent messages, optionally filtered by peer.

- **Optional**: `peer`, `since: integer` (epoch ms), `limit: integer`
- **Returns**: `{messages: [{id, peer, from_me, text, ts_unix_ms, kind}, ...]}`
- **Note**: `kind` = `text|image|video|audio|voice|sticker|document|contact|location|poll|revoked`.

### `messages.get`

Get a single message by id.

- **Required**: `msg_id: string`
- **Returns**: full `Message` record.

### `messages.search`

Full-text search across message history.

- **Required**: `query: string`, `peer: string`
- **Returns**: `{matches: [{id, peer, snippet, score}, ...]}`

### `messages.edit`

Edit a previously sent text message.

- **Required**: `peer`, `msg_id`, `msg_timestamp`, `new_text`

### `messages.mark_read`

Mark messages up to a given id as read.

- **Required**: `peer`, `up_to: string` (message id)
- **Returns**: `{updated: <count>}`

### `messages.download`

Download a media reference to a local path.

- **Required**: `media_ref_token: string`, `out: string` (absolute path)
- **Returns**: `{path, sha256, size}`

---

## 4. Chats (8)

### `chats.list`

List known chats (optionally filtered by kind/limit).

- **Optional**: `kind: string` (`dm|group|status|broadcast`), `limit`

### `chats.info`

Get info about a single chat by JID.

- **Required**: `jid`

### `chats.pin`

Pin a chat to the top of the list.

- **Required**: `jid`

### `chats.unpin`

Unpin a previously pinned chat.

- **Required**: `jid`

### `chats.mute`

Mute a chat until a given epoch-seconds timestamp.

- **Required**: `jid`, `until_epoch_secs: integer`

### `chats.archive`

Archive a chat (hide from default list).

- **Required**: `jid`

### `chats.delete`

Delete a chat and its history locally.

- **Required**: `jid`
- **Warning**: deletes local copy only; peer still has history.

### `chats.typing`

Set or clear the typing indicator on a chat.

- **Required**: `jid`, `on: boolean`
- **Rate-limit**: 2s floor (subsumed by global WA floor).

---

## 5. Groups — basic (4)

### `groups.create`

Create a new group.

- **Required**: `subject: string`, `members: array<string>` (E164 or LID,
  must include self)
- **Returns**: `{group_jid, subject, members: [...]}` 

### `groups.list`

List groups the daemon belongs to.

- **Input**: `{}`
- **Returns**: `{groups: [{jid, subject, member_count, ephemeral_ts}, ...]}`

### `groups.info`

Show info about a single group.

- **Required**: `jid`

### `groups.leave`

Leave a group.

- **Required**: `jid`

---

## 6. Groups coordinator (Phase 6.12) (14)

These were the "must-do" group operations before 6.12. Most require admin on
the group.

### `groups.destroy`

Destroy (delete) a group. Irreversible server-side.

- **Required**: `jid`
- **Constraint**: only the group owner can destroy.

### `groups.resolve_invite`

Resolve an invite link or short code to a group handle.

- **Required**: `code: string` (full `https://chat.whatsapp.com/…` URL or
  short code)

### `groups.add_member`

Add a single member to a group.

- **Required**: `jid`, `member: string`
- **Optional**: `is_admin: boolean`

### `groups.add_members`

Add multiple members to a group (partial-success per element).

- **Required**: `jid`, `members: array<string>`
- **Optional**: `is_admin`
- **Returns**: `{added: [...], rejected: [{member, reason}, ...]}`

### `groups.remove_member`

Remove a single member from a group.

- **Required**: `jid`, `member`

### `groups.remove_members`

Remove multiple members from a group (partial-success per element).

- **Required**: `jid`, `members`

### `groups.promote`

Promote a member to admin.

- **Required**: `jid`, `member`

### `groups.demote`

Demote an admin back to member.

- **Required**: `jid`, `member`

### `groups.ban`

Ban a member. Default indefinite; pass `duration_seconds` for timed.

- **Required**: `jid`, `member`
- **Optional**: `duration_seconds: integer` (omit for indefinite)

### `groups.approve_join`

Approve a pending join request.

- **Required**: `jid`, `member`
- **Constraint**: group must be in `require_approval` mode.

### `groups.rename`

Rename the group subject.

- **Required**: `jid`, `subject`

### `groups.set_description`

Set the group description.

- **Required**: `jid`, `description`

### `groups.set_locked`

Lock or unlock the group (admins-only messaging when locked).

- **Required**: `jid`, `locked: boolean`

### `groups.transfer_ownership`

Transfer group ownership to another member. Irreversible.

- **Required**: `jid`, `member`
- **Warning**: only the current owner can call; after transfer, the
  original owner becomes admin.

---

## 7. Groups completion (Phase 6.12.1) (6)

### `groups.set_announce`

Set announce-only mode (only admins can post when on).

- **Required**: `jid`, `announce: boolean`

### `groups.set_ephemeral`

Set message expiry timer. Omit `ttl_seconds` to disable.

- **Required**: `jid`
- **Optional**: `ttl_seconds: integer` (one of 86400, 604800, 7776000, …)

### `groups.set_require_approval`

Require admin approval for new joiners.

- **Required**: `jid`, `require: boolean`

### `groups.list_with_invites`

List groups the daemon belongs to plus pending invites.

- **Input**: `{}`
- **Returns**: `{groups: [...], pending_invites: [{code, jid, subject, inviter}, ...]}`

### `groups.join_by_invite`

Join a group via invite link or short code.

- **Required**: `code`

### `groups.join_by_id`

Join a group by JID.

- **Required**: `jid`
- **Constraint**: group must be open or you must be pre-approved.

---

## 8. Groups gap list (Phase 7.H, 5)

Surfaced in the parity-closure Session A. Were CLI-only before.

### `groups.get_invite_link`

Fetch (or rotate, with `reset=true`) a group's invite link.

- **Required**: `jid`
- **Optional**: `reset: boolean` (rotate; old link stops working)
- **Returns**: `{link: "https://chat.whatsapp.com/…"}`

### `groups.update_member_label`

Set or clear a per-member label (e.g. nickname) within a group. Pass
`""` for `label` to clear.

- **Required**: `jid`, `label: string`
- **Note**: labels are private to the bot; other members don't see them.

### `groups.get_profile_pictures`

Fetch profile pictures for one or more groups. Pass `preview=true` for the
small preview variant.

- **Required**: `jids: array<string>`
- **Optional**: `preview: boolean`
- **Returns**: `{pictures: {<jid>: {url, sha256, size}}}` or `{preview_url: …}`

### `groups.set_profile_picture`

Set the group icon. `image_data_b64` must be base64-encoded JPEG/PNG bytes.

- **Required**: `jid`, `image_data_b64: string`

### `groups.remove_profile_picture`

Remove the group icon.

- **Required**: `jid`

---

## 9. Media (1)

### `media.info`

Return metadata for a media-ref token.

- **Required**: `media_ref_token: string`
- **Returns**: `{mime_type, size, sha256, url}`

---

## 10. Envelope (DOT/1) (4)

Wraps raw bytes in DOT/1 envelopes (RFC-0850 §8.6) so an agent can ship
hermetic, replayable messages through WA. The format is
`DOT/1\n<headers>\n\n<payload-utf8>\n.`

### `envelope.encode`

Wrap raw bytes in a DOT/1 envelope.

- **Optional**: `file: string` (path; else reads stdin)

### `envelope.decode`

Decode a DOT/1 envelope from stdin (prints payload).

- **Input**: `{}`

### `envelope.send`

Send a DOT/1 envelope file as a message.

- **Required**: `peer`, `file`

### `envelope.send-native`

Send a DOT/1 envelope via the native transport.

- **Required**: `peer`, `file`

---

## 11. Capabilities + domain (2)

### `capabilities`

Return platform capabilities (payload sizes, media caps, flags).

- **Input**: `{}`
- **Returns**:
  ```json
  {
    "text_max_bytes": 65536,
    "media_max_bytes": 16777216,
    "polling_max_options": 12,
    "supports_polls_quiz": true,
    "supports_status_story": true,
    "supports_newsletter": true
  }
  ```

### `domain.compute-hash`

Compute the deterministic domain id for a group JID.

- **Required**: `group_jid: string`
- **Returns**: `{hash: "blake3-256-hex"}`
- **Use**: when persisting group references in your own data — the JID
  can rotate between LID-migration, but the hash stays stable.

---

## 12. Events (4) — loss recovery

The event stream is the ground truth. Polling `events.list` is your last
resort when an action's side effects aren't visible in tool responses.

### `events.list`

List recent events (most recent first).

- **Optional**: `limit: integer`
- **Returns**: `{events: [{id, ts_unix_ms, kind, peer?, payload}, ...]}`

### `events.show`

Show a single event by id.

- **Required**: `id: integer`

### `events.replay`

Replay events since a given id (loss recovery).

- **Optional**: `since_id: integer`, `limit`

### `events.tail`

Tail the event stream (returns recent buffer snapshot; per-sink stream +
`Lagged` arrives with the live router).

- **Optional**: `limit`

---

## 13. Agent discovery (3)

### `clients.list`

List active MCP client sessions.

- **Input**: `{}`
- **Returns**: `{clients: [{id, peer_addr, since}, ...]}`

### `daemon.methods.list`

List every daemon RPC method (agent discovery).

- **Input**: `{}`
- **Returns**: `{methods: ["send.text", "send.image", ...]}`

### `daemon.methods.help`

Return schema + one-line help for a single RPC method.

- **Required**: `method: string` (e.g. `"send.text"`)

---

## 14. Security tokens (3)

Phase 5 Part A. The daemon exposes JSON-RPC over a unix socket and gates
calls behind bearer tokens.

### `security.rotate_token`

Rotate the active bearer token; old token remains valid through grace window.

- **Required**: `old_token_id: string`, `new_secret_hex: string`
- **Optional**: `grace_ms: integer` (default 86_400_000 = 24h), `label: string`

### `security.revoke_all_tokens`

Revoke every active bearer token (incident response).

- **Input**: `{}`

### `security.list_tokens`

List active and grace-period tokens.

- **Input**: `{}`
- **Returns**: `{tokens: [{id, label, active_ms, grace_until_unix_ms}, ...]}`

---

## 15. Rules CRUD + dry-run (12)

The rules engine (Phase 4 → exposed Phase 5 Part E). Rules live in
`~/.local/share/octo/whatsapp/rules.toml`. `rules.*` write through
optimistic concurrency using `etag`.

### `rules.list`

List all rules in the live ruleset.

- **Input**: `{}`
- **Returns**: `{rules: [{id, enabled, priority, predicate, actions, etag}, ...]}`

### `rules.get`

Fetch a single rule by id.

- **Required**: `id: string`

### `rules.create`

Create a new rule. The body is the full rule object
(`id, enabled, priority, predicate, actions, cooldown_ms, ttl_until`).

- **Optional**: all of the above.

### `rules.update`

Replace an existing rule (etag-guarded optimistic concurrency).

- **Required**: `id`, `etag`, `predicate`, `actions`, `priority`, `enabled`,
  `cooldown_ms`, `ttl_until`

### `rules.patch`

Apply a subset patch to a rule (etag-guarded).

- **Required**: same fields as `update`.

### `rules.delete`

Delete a rule (etag-guarded).

- **Required**: `id`, `etag`

### `rules.enable`

Enable a rule (no etag required).

- **Required**: `id`

### `rules.disable`

Disable a rule (no etag required).

- **Required**: `id`

### `rules.approve`

Transition a Draft rule to Approved.

- **Required**: `id`

### `rules.reload`

Re-read `rules.toml` from disk and atomically swap into the live ruleset.

- **Input**: `{}`

### `rules.flush`

Force a sync of any debounced pending rule mutations to disk.

- **Input**: `{}`

### `rules.test`

Dry-run: evaluate an inbound event against the live ruleset without
executing actions.

- **Required**: `event: object` (the inbound event you want to test)
- **Returns**: `{matches: [{rule_id, actions_fired}], errors: [...]}`

---

## 16. Triggers CRUD + run (6)

### `triggers.list`

List all triggers in the live triggerset.

- **Input**: `{}`

### `triggers.get`

Fetch a single trigger by id.

- **Required**: `id`

### `triggers.create`

Create a new trigger.

- **Optional**: `id`, `enabled`, `runner: object`, `rate_limit: object`,
  `timeout_ms: integer`, `retries: integer`, `history_cap: integer`

### `triggers.update`

Update an existing trigger (etag-guarded).

- **Required**: `id`, `etag`, `runner`, `rate_limit`, `timeout_ms`,
  `retries`, `history_cap`, `enabled`

### `triggers.delete`

Delete a trigger (etag-guarded).

- **Required**: `id`, `etag`

### `triggers.run`

Invoke a trigger and return the RunRecord.

- **Optional**: `id`, `event: object`

---

## 17. Audit hash chain (2)

### `audit.tail`

Tail audit log entries since a given sequence number (loss-recovery).

- **Optional**: `since_seq: integer`, `limit`

### `audit.verify`

Walk the in-memory audit hash chain and verify each row's `prev_hash`
matches the previous row's `this_hash`.

- **Input**: `{}`
- **Returns**: `{ok: true, last_seq: <n>}` or
  `{ok: false, broken_at_seq: <n>}`.

---

## 18. Actions (1)

### `actions.escalate`

Dispatch an escalation to a target (e.g. oncall) with a reason.

- **Required**: `target: string`, `reason: string`

---

## 19. Accounts (3) — multi-bot

Phase 6.1 introduced multi-account support. A linked WA account is one
"bot"; you can have multiple under `~/.local/share/octo/whatsapp/accounts/`.

### `daemon.accounts.list`

List all linked WhatsApp accounts.

- **Input**: `{}`
- **Returns**:
  ```json
  {
    "accounts": [
      {"id": "personal", "phone": "+15551234567", "active": true},
      {"id": "work",     "phone": "+15557654321", "active": false}
    ]
  }
  ```

### `daemon.accounts.use`

Set the active WhatsApp account (writes the `active` symlink).

- **Required**: `account_id: string`

### `daemon.accounts.info`

Show details for one linked WhatsApp account.

- **Required**: `account_id: string`

---

## 20. Dynamic SQL (3) — `query` cargo feature

Phase 9 surface that lets the operator (or any agent) drive arbitrary
DDL/DML against the daemon's embedded stoolap database without
rebuilding. Useful for ad-hoc persistence (e.g. snapshotting
`groups.info` members into a `group_members` table) and introspection.
The `query` cargo feature must be enabled at compile time; absent
builds do not register these tools.

### `sql.execute`

Run a single DDL/DML statement. Allowed first keywords:
`INSERT`, `UPDATE`, `DELETE`, `REPLACE`, `CREATE`, `DROP`, `ALTER`,
`TRUNCATE`, `BEGIN`, `COMMIT`, `ROLLBACK`, `PRAGMA`, `ANALYZE`,
`VACUUM`. Multi-statement strings and unrecognized verbs are
rejected at the daemon before the SQL reaches stoolap.

- **Required**: `sql: string`
- **Returns**: `{sql, first_keyword, rows_affected}`
- **Example**:
  ```json
  {"sql": "CREATE TABLE demo (k INTEGER PRIMARY KEY, v TEXT)",
   "first_keyword": "CREATE", "rows_affected": 0}
  ```

### `sql.query`

Run a read-only `SELECT` / `WITH` / `SHOW` / `EXPLAIN` / `DESCRIBE` /
`DESC`. Write verbs are rejected. Hard cap: 10000 rows.

- **Required**: `sql: string`
- **Optional**: `limit: integer` (smaller client-side cap, defaults
  to 10000).
- **Returns**: `{sql, first_keyword, columns, rows, count, limit, truncated}`
- **Example**:
  ```json
  {"sql": "SELECT k, v FROM demo ORDER BY k",
   "first_keyword": "SELECT",
   "columns": ["k", "v"],
   "rows": [[1, "hello"], [2, "world"], [3, "foo"]],
   "count": 3, "limit": 10000, "truncated": false}
  ```

### `sql.tables`

Shortcut for `SHOW TABLES`. No input.

- **Returns**: `{tables: [string], count: integer}`

**Safety rails (non-negotiable):**

1. Single-statement enforcement — split on `;`, reject >1.
2. Allow-list per handler — execute admits writes only, query admits
   reads only. `SHUTDOWN` / `ATTACH DATABASE` / `DETACH DATABASE`
   are blocked.
3. Per-RPC future runs the SQL on `spawn_blocking` so cancellation
   is clean (stoolap is single-threaded; long queries would
   otherwise wedge the daemon's read path).
4. Bearer-token gate inherited from `ipc/server.rs` — same as every
   other RPC.
5. Hard `MAX_ROWS = 10000` cap on `sql.query` payload.

---

## Bootstrapping cheat-sheet

When you wire `octo-whatsapp` into a fresh MCP client, walk this list:

1. `version` — confirm the daemon is alive.
2. `status` — confirm `bot_state == "Connected"`.
3. `capabilities` — discover payload limits.
4. `daemon.methods.list` / `daemon.methods.help` — discover anything you
   think is missing from this catalog.
5. `daemon.accounts.list` + `daemon.accounts.use` — if multi-account.

For the canonical daemon RPC surface (not the MCP-stripped one), the
daemon-side JSON-RPC has the same method names plus internal-only entries
(`events.persister.*`, `daemon.internal.*`). Use `daemon.methods.list` to
enumerate.

## Where the source of truth lives

| File | What it controls |
|---|---|
| `crates/octo-whatsapp/src/mcp_server.rs` | the 100-tool catalog + names + descriptions |
| `crates/octo-whatsapp/src/cli.rs` | CLI subcommand dispatch (kebab-case) |
| `crates/octo-whatsapp/src/ipc/handlers/*.rs` | daemon-side IPC handlers |
| `crates/octo-whatsapp/src/lib.rs` | module re-exports |
| `crates/octo-adapter-whatsapp/src/inherent.rs` | WA-crate inherent wrappers |
| `docs/plans/2026-07-10-octo-whatsapp-skills-mcp-distribution.md` | the distribution plan |
| `crates/octo-whatsapp/assets/skills/*.md` | this skill + playbooks |
| `crates/octo-whatsapp/assets/mcp-configs/*.json` | cross-env MCP server config |
| `scripts/install.sh` | one-shot installer |

If a tool name here and in `mcp_server.rs::EXPECTED_TOOL_COUNT` disagree,
`mcp_server.rs` wins. The CLI/MCP parity-closure test
`session_a_lifecycle_and_readonly_tools_are_advertised` (and friends) pin
the count and names.
