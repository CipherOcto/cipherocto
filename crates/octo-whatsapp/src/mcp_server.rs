//! MCP server (stdio JSON-RPC). Phase 1: thin proxy to the daemon.
//!
//! Receives JSON-RPC on stdin, forwards `tools/list` / `tools/call` /
//! `initialize` / `ping` to daemon-side counterparts (or directly answers
//! initialize + ping), writes responses on stdout. Multiple MCP clients
//! may share one daemon.

use std::io::{self, BufRead, Write};
use std::path::Path;

use serde_json::Value;

/// Number of MCP tools registered (Phase 1 + Phase 2 + Phase 3 + Phase 5
/// Part A + Phase 5 Part E RPC surfaces + Phase 6.12 groups coordinator
/// surface + Phase 6.12.1 groups completion surface + Phase 6.1 multi-
/// account surface + Phase 7.H groups gap list + Session A lifecycle /
/// rules / triggers read-only operators). Used by integration tests to
/// assert `tools/list` advertises the full set. The Phase 4 / Phase 5
/// Part E additions are 17 tools (10 rule CRUD/dry-run + 4 trigger
/// CRUD/run + 2 audit + 1 action). The Phase 6.12 additions are 14
/// `groups.*` coordinator tools (destroy, resolve_invite, add_member,
/// add_members, remove_member, remove_members, promote, demote, ban,
/// approve_join, rename, set_description, set_locked, transfer_ownership).
/// The Phase 6.12.1 completion surface adds 6 more (set_announce,
/// set_ephemeral, set_require_approval, list_with_invites, join_by_invite,
/// join_by_id). The Phase 6.1 multi-account surface adds 3
/// `daemon.accounts.*` tools (list, use, info). The Phase 7.H gap-list
/// surface adds 5 more (get_invite_link / update_member_label /
/// get_profile_pictures / set_profile_picture / remove_profile_picture).
/// The Session A parity-closure surface adds 6 more (reconnect.now /
/// shutdown / rules.list / rules.get / triggers.list / triggers.get).
/// The query layer (gated by the `query` cargo feature) adds 3
/// more (daemon.search, messages.context, events.find). The dynamic
/// SQL surface (Phase 9) adds 3 more (sql.execute, sql.query,
/// sql.tables). The contact + identity surface (Phase 7.J — 10 tools)
/// adds: contacts.is_on_whatsapp, contacts.get_user_info,
/// contacts.get_business_profile, contacts.get_profile_picture,
/// contacts.save_contact, contact.block, contact.unblock,
/// identity.get_pn, identity.get_lid, identity.is_lid_migrated.
#[cfg(feature = "query")]
pub const EXPECTED_TOOL_COUNT: usize = 148;
#[cfg(not(feature = "query"))]
pub const EXPECTED_TOOL_COUNT: usize = 142;

pub async fn serve(socket: &Path) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(());
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("parse: {e}")},
                });
                writeln!(stdout, "{}", err)?;
                stdout.flush()?;
                continue;
            }
        };
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = req.get("id").cloned().unwrap_or(Value::Null);

        let response = match method {
            "initialize" => handle_initialize(id, &req).await?,
            "ping" => handle_ping(id).await?,
            "tools/list" => handle_tools_list(id, socket).await?,
            "tools/call" => handle_tools_call(id, &req, socket).await?,
            _ => jsonrpc_error(
                id,
                -32601,
                &format!("method {:?} not implemented in Phase 1", method),
            ),
        };
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }
}

async fn handle_initialize(id: Value, _req: &Value) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-06-18",
            "serverInfo": {"name": "octo-whatsapp", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"tools": {}},
        },
    }))
}

async fn handle_ping(id: Value) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {},
    }))
}

async fn handle_tools_list(id: Value, _socket: &Path) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tool_descriptors(),
            "_phase": "phase2",
        },
    }))
}

/// Canonical tool descriptor list mirrored from the daemon's RPC method
/// registry. Each tool's `name` is forwarded as the RPC `method` field;
/// arguments map 1:1 to RPC params (the JSON Schema is the source of truth).
#[allow(clippy::vec_init_then_push)]
pub fn tool_descriptors() -> Vec<Value> {
    let mut v: Vec<Value> = Vec::new();
    // ─── Lifecycle (3) ────────────────────────────────────────────────
    v.push(td(
        "version",
        "Get the daemon version info.",
        schema_empty(),
    ));
    v.push(td(
        "status",
        "Get daemon runtime status (boot state, bot state, session health).",
        schema_empty(),
    ));
    v.push(td(
        "health",
        "Get daemon health (liveness/readiness summary).",
        schema_empty(),
    ));
    // Session A of the parity-closure plan surfaced the two lifecycle
    // operators (`reconnect.now` / `shutdown`) on the MCP surface; they
    // already had CLI subcommands but no MCP tool.
    v.push(td(
        "reconnect.now",
        "Force a reconnect of the underlying WebSocket. Tears down the current session and re-authenticates.",
        schema_empty(),
    ));
    v.push(td(
        "shutdown",
        "Gracefully shut down the daemon. Any in-flight RPC returns its result before the daemon exits.",
        schema_empty(),
    ));
    // ─── Send media + control (11) ────────────────────────────────────
    v.push(td(
        "send.text",
        "Send a text message to a peer.",
        schema_props_required(&[("peer", "string"), ("text", "string")], &["peer", "text"]),
    ));
    v.push(td(
        "send.image",
        "Send an image with optional caption.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("file", "string"),
                ("caption", "string"),
            ],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.video",
        "Send a video with optional caption.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("file", "string"),
                ("caption", "string"),
            ],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.audio",
        "Send an audio file.",
        schema_props_required(&[("peer", "string"), ("file", "string")], &["peer", "file"]),
    ));
    v.push(td(
        "send.voice",
        "Send a voice-note (PTT) audio file.",
        schema_props_required(&[("peer", "string"), ("file", "string")], &["peer", "file"]),
    ));
    v.push(td(
        "send.sticker",
        "Send a sticker image (WEBP).",
        schema_props_required(&[("peer", "string"), ("file", "string")], &["peer", "file"]),
    ));
    v.push(td(
        "send.reaction",
        "React to a message with an emoji.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("msg_id", "string"),
                ("emoji", "string"),
            ],
            &["peer", "msg_id", "emoji"],
        ),
    ));
    v.push(td(
        "send.poll",
        "Send a poll to a peer.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("question", "string"),
                ("options", "array"),
                ("multi", "boolean"),
            ],
            &["peer", "question", "options"],
        ),
    ));
    v.push(td(
        "send.contact",
        "Send a vCard contact.",
        schema_props_required(
            &[("peer", "string"), ("vcard", "string")],
            &["peer", "vcard"],
        ),
    ));
    v.push(td(
        "send.location",
        "Send a location pin.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("lat", "number"),
                ("lon", "number"),
                ("name", "string"),
            ],
            &["peer", "lat", "lon"],
        ),
    ));
    v.push(td(
        "send.delete",
        "Delete (revoke) a previously sent message.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("msg_id", "string"),
                ("msg_timestamp", "integer"),
            ],
            &["peer", "msg_id", "msg_timestamp"],
        ),
    ));
    // ─── Messages (6) ─────────────────────────────────────────────────
    v.push(td(
        "messages.list",
        "List recent messages, optionally filtered by peer.",
        schema_props_optional(&[
            ("peer", "string"),
            ("since", "integer"),
            ("limit", "integer"),
        ]),
    ));
    v.push(td(
        "messages.get",
        "Get a single message by id.",
        schema_props_required(&[("msg_id", "string")], &["msg_id"]),
    ));
    v.push(td(
        "messages.search",
        "Full-text search across message history.",
        schema_props_required(&[("query", "string"), ("peer", "string")], &["query"]),
    ));
    v.push(td(
        "messages.edit",
        "Edit a previously sent text message.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("msg_id", "string"),
                ("msg_timestamp", "integer"),
                ("new_text", "string"),
            ],
            &["peer", "msg_id", "msg_timestamp", "new_text"],
        ),
    ));
    v.push(td(
        "messages.mark_read",
        "Mark messages up to a given id as read.",
        schema_props_required(
            &[("peer", "string"), ("up_to", "string")],
            &["peer", "up_to"],
        ),
    ));
    v.push(td(
        "messages.download",
        "Download a media reference to a local path.",
        schema_props_required(
            &[("media_ref_token", "string"), ("out", "string")],
            &["media_ref_token", "out"],
        ),
    ));
    // ─── Phase 7.K — View-Once + Disappearing (3) ─────────────────────
    v.push(td(
        "messages.read_view_once",
        "Read the media body for a view-once message (one-shot). Subsequent reads return consumed. Returns {event_id, media_b64, mime, caption, consumed_at_unix_ms, status}.",
        schema_props_required(&[("event_id", "integer")], &["event_id"]),
    ));
    v.push(td(
        "messages.list_unavailable",
        "List messages whose content is unavailable (companion view-once fanouts, bot/hosted). Filters: kind, peer, since_ts_unix_ms, until_ts_unix_ms, limit.",
        schema_props_optional(&[
            ("kind", "string"),
            ("peer", "string"),
            ("since_ts_unix_ms", "integer"),
            ("until_ts_unix_ms", "integer"),
            ("limit", "integer"),
        ]),
    ));
    v.push(td(
        "messages.list_ephemeral",
        "List ephemeral (disappearing) messages currently in flight. Filters: peer, kind, limit.",
        schema_props_optional(&[("peer", "string"), ("kind", "string"), ("limit", "integer")]),
    ));
    // ─── Chats (8) ────────────────────────────────────────────────────
    v.push(td(
        "chats.list",
        "List known chats (optionally filtered by kind/limit).",
        schema_props_optional(&[("kind", "string"), ("limit", "integer")]),
    ));
    v.push(td(
        "chats.info",
        "Get info about a single chat by JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "chats.pin",
        "Pin a chat to the top of the list.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "chats.unpin",
        "Unpin a previously pinned chat.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "chats.mute",
        "Mute a chat until a given epoch-seconds timestamp.",
        schema_props_required(
            &[("jid", "string"), ("until_epoch_secs", "integer")],
            &["jid", "until_epoch_secs"],
        ),
    ));
    v.push(td(
        "chats.archive",
        "Archive a chat (hide from default list).",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "chats.delete",
        "Delete a chat and its history locally.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "chats.typing",
        "Set or clear the typing indicator on a chat.",
        schema_props_required(&[("jid", "string"), ("on", "boolean")], &["jid", "on"]),
    ));
    // ─── Groups (4) ───────────────────────────────────────────────────
    v.push(td(
        "groups.create",
        "Create a new group.",
        schema_props_required(
            &[("subject", "string"), ("members", "array")],
            &["subject", "members"],
        ),
    ));
    v.push(td(
        "groups.list",
        "List groups the daemon belongs to.",
        schema_empty(),
    ));
    v.push(td(
        "groups.info",
        "Show info about a single group.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "groups.leave",
        "Leave a group.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    // ─── Groups coordinator (Phase 6.12 — 14) ────────────────────────
    v.push(td(
        "groups.destroy",
        "Destroy (delete) a group. Irreversible server-side.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "groups.resolve_invite",
        "Resolve an invite link or short code to a group handle.",
        schema_props_required(&[("code", "string")], &["code"]),
    ));
    v.push(td(
        "groups.add_member",
        "Add a single member to a group.",
        schema_props_required(
            &[
                ("jid", "string"),
                ("member", "string"),
                ("is_admin", "boolean"),
            ],
            &["jid", "member"],
        ),
    ));
    v.push(td(
        "groups.add_members",
        "Add multiple members to a group (partial-success per element).",
        schema_props_required(
            &[
                ("jid", "string"),
                ("members", "array"),
                ("is_admin", "boolean"),
            ],
            &["jid", "members"],
        ),
    ));
    v.push(td(
        "groups.remove_member",
        "Remove a single member from a group.",
        schema_props_required(
            &[("jid", "string"), ("member", "string")],
            &["jid", "member"],
        ),
    ));
    v.push(td(
        "groups.remove_members",
        "Remove multiple members from a group (partial-success per element).",
        schema_props_required(
            &[("jid", "string"), ("members", "array")],
            &["jid", "members"],
        ),
    ));
    v.push(td(
        "groups.promote",
        "Promote a member to admin.",
        schema_props_required(
            &[("jid", "string"), ("member", "string")],
            &["jid", "member"],
        ),
    ));
    v.push(td(
        "groups.demote",
        "Demote an admin back to member.",
        schema_props_required(
            &[("jid", "string"), ("member", "string")],
            &["jid", "member"],
        ),
    ));
    v.push(td(
        "groups.ban",
        "Ban a member. Default indefinite; pass duration_seconds for timed.",
        schema_props_required(
            &[
                ("jid", "string"),
                ("member", "string"),
                ("duration_seconds", "integer"),
            ],
            &["jid", "member"],
        ),
    ));
    v.push(td(
        "groups.approve_join",
        "Approve a pending join request.",
        schema_props_required(
            &[("jid", "string"), ("member", "string")],
            &["jid", "member"],
        ),
    ));
    v.push(td(
        "groups.rename",
        "Rename the group subject.",
        schema_props_required(
            &[("jid", "string"), ("subject", "string")],
            &["jid", "subject"],
        ),
    ));
    v.push(td(
        "groups.set_description",
        "Set the group description.",
        schema_props_required(
            &[("jid", "string"), ("description", "string")],
            &["jid", "description"],
        ),
    ));
    v.push(td(
        "groups.set_locked",
        "Lock or unlock the group (admins-only messaging when locked).",
        schema_props_required(
            &[("jid", "string"), ("locked", "boolean")],
            &["jid", "locked"],
        ),
    ));
    v.push(td(
        "groups.transfer_ownership",
        "Transfer group ownership to another member. Irreversible.",
        schema_props_required(
            &[("jid", "string"), ("member", "string")],
            &["jid", "member"],
        ),
    ));
    // ─── Groups completion (Phase 6.12.1 — 6) ─────────────────────────
    v.push(td(
        "groups.set_announce",
        "Set announce-only mode (only admins can post when on).",
        schema_props_required(
            &[("jid", "string"), ("announce", "boolean")],
            &["jid", "announce"],
        ),
    ));
    v.push(td(
        "groups.set_ephemeral",
        "Set message expiry timer. Omit ttl_seconds to disable.",
        schema_props_required(&[("jid", "string"), ("ttl_seconds", "integer")], &["jid"]),
    ));
    v.push(td(
        "groups.set_require_approval",
        "Require admin approval for new joiners.",
        schema_props_required(
            &[("jid", "string"), ("require", "boolean")],
            &["jid", "require"],
        ),
    ));
    v.push(td(
        "groups.list_with_invites",
        "List groups the daemon belongs to plus pending invites.",
        schema_empty(),
    ));
    v.push(td(
        "groups.join_by_invite",
        "Join a group via invite link or short code.",
        schema_props_required(&[("code", "string")], &["code"]),
    ));
    v.push(td(
        "groups.join_by_id",
        "Join a group by JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    // ─── Phase 7.H gap list (5) — surfaced in Session A ───────────────
    v.push(td(
        "groups.get_invite_link",
        "Fetch (or rotate, with reset=true) a group's invite link.",
        schema_props_required(&[("jid", "string"), ("reset", "boolean")], &["jid"]),
    ));
    v.push(td(
        "groups.update_member_label",
        "Set or clear a per-member label (e.g. nickname) within a group. Pass an empty string for label to clear.",
        schema_props_required(
            &[("jid", "string"), ("label", "string")],
            &["jid", "label"],
        ),
    ));
    v.push(td(
        "groups.get_profile_pictures",
        "Fetch profile pictures for one or more groups. Pass preview=true to request the small preview variant.",
        schema_props_required(
            &[("jids", "array"), ("preview", "boolean")],
            &["jids"],
        ),
    ));
    v.push(td(
        "groups.set_profile_picture",
        "Set the group icon. `image_data_b64` must be base64-encoded JPEG/PNG bytes.",
        schema_props_required(
            &[("jid", "string"), ("image_data_b64", "string")],
            &["jid", "image_data_b64"],
        ),
    ));
    v.push(td(
        "groups.remove_profile_picture",
        "Remove the group icon.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    // ─── Media (1) ────────────────────────────────────────────────────
    v.push(td(
        "media.info",
        "Return metadata for a media-ref token.",
        schema_props_required(&[("media_ref_token", "string")], &["media_ref_token"]),
    ));
    // ─── Envelope (4) ─────────────────────────────────────────────────
    v.push(td(
        "envelope.encode",
        "Wrap raw bytes in a DOT/1 envelope (stdin or --file).",
        schema_props_optional(&[("file", "string")]),
    ));
    v.push(td(
        "envelope.decode",
        "Decode a DOT/1 envelope from stdin (prints payload).",
        schema_empty(),
    ));
    v.push(td(
        "envelope.send",
        "Send a DOT/1 envelope file as a message.",
        schema_props_required(&[("peer", "string"), ("file", "string")], &["peer", "file"]),
    ));
    v.push(td(
        "envelope.send-native",
        "Send a DOT/1 envelope via the native transport.",
        schema_props_required(&[("peer", "string"), ("file", "string")], &["peer", "file"]),
    ));
    // ─── Capabilities + domain (2) ────────────────────────────────────
    v.push(td(
        "capabilities",
        "Return platform capabilities (payload sizes, media caps, flags).",
        schema_empty(),
    ));
    v.push(td(
        "domain.compute-hash",
        "Compute the deterministic domain id for a group JID.",
        schema_props_required(&[("group_jid", "string")], &["group_jid"]),
    ));
    // ─── Events (6) — Phase 3 + first-class overhaul ────────────────
    v.push(td(
        "events.list",
        "List recent events (most recent first).",
        schema_props_optional(&[("limit", "integer")]),
    ));
    v.push(td(
        "events.show",
        "Show a single event by id.",
        schema_props_required(&[("id", "integer")], &["id"]),
    ));
    v.push(td(
        "events.replay",
        "Replay events since a given id (Loss recovery).",
        schema_props_optional(&[("since_id", "integer"), ("limit", "integer")]),
    ));
    v.push(td(
        "events.tail",
        "Tail the event stream (returns recent buffer snapshot; per-sink stream + Lagged arrives with the live router).",
        schema_props_optional(&[("limit", "integer")]),
    ));
    v.push(td(
        "events.list_kinds",
        "List every known (kind) the daemon can produce. Drives first-class discoverability after the wacore events overhaul.",
        schema_empty(),
    ));
    v.push(td(
        "events.unknown_stats",
        "Per-variant aggregate of InboundEvent::Unknown emissions (count, first/last_seen_ms, sample). Operators inspect this to prioritise new typed handlers when wacore adds a variant.",
        schema_empty(),
    ));
    v.push(td(
        "events.unknown_stats.history",
        "Per-day historical snapshots of unknown_stats (last `days` days, default 30, max 90). Powered by the daily-rotation sidecar files.",
        schema_props_optional(&[("days", "integer")]),
    ));
    // ─── Agent discovery (3) — Phase 3 ────────────────────────────────
    v.push(td(
        "clients.list",
        "List active MCP client sessions.",
        schema_empty(),
    ));
    v.push(td(
        "daemon.methods.list",
        "List every daemon RPC method (agent discovery).",
        schema_empty(),
    ));
    v.push(td(
        "daemon.methods.help",
        "Return schema + one-line help for a single RPC method.",
        schema_props_required(&[("method", "string")], &["method"]),
    ));
    // ─── Security tokens (3) — Phase 5 Part A ─────────────────────────
    v.push(td(
        "security.rotate_token",
        "Rotate the active bearer token; old token remains valid through grace window.",
        schema_props_required(
            &[
                ("old_token_id", "string"),
                ("new_secret_hex", "string"),
                ("grace_ms", "integer"),
                ("label", "string"),
            ],
            &["old_token_id", "new_secret_hex"],
        ),
    ));
    v.push(td(
        "security.revoke_all_tokens",
        "Revoke every active bearer token (incident response).",
        schema_empty(),
    ));
    v.push(td(
        "security.list_tokens",
        "List active and grace-period tokens.",
        schema_empty(),
    ));
    // ─── Rules CRUD + dry-run (12) — Phase 5 Part E (Phase 4 RPC) ─────
    // Session A of the parity-closure plan added the Phase 1 read-only
    // tools (`rules.list` / `rules.get`) to the MCP surface so an MCP
    // client can enumerate rules.
    v.push(td(
        "rules.list",
        "List all rules in the live ruleset.",
        schema_empty(),
    ));
    v.push(td(
        "rules.get",
        "Fetch a single rule by id.",
        schema_props_required(&[("id", "string")], &["id"]),
    ));
    v.push(td(
        "rules.create",
        "Create a new rule. The body is the full rule object (id, enabled, priority, predicate, actions, cooldown_ms, ttl_until).",
        schema_props_optional(&[
            ("id", "string"),
            ("enabled", "boolean"),
            ("priority", "integer"),
            ("predicate", "object"),
            ("actions", "array"),
            ("cooldown_ms", "integer"),
            ("ttl_until", "integer"),
        ]),
    ));
    v.push(td(
        "rules.update",
        "Replace an existing rule (etag-guarded optimistic concurrency).",
        schema_props_required(
            &[
                ("id", "string"),
                ("etag", "string"),
                ("predicate", "object"),
                ("actions", "array"),
                ("priority", "integer"),
                ("enabled", "boolean"),
                ("cooldown_ms", "integer"),
                ("ttl_until", "integer"),
            ],
            &["id", "etag"],
        ),
    ));
    v.push(td(
        "rules.patch",
        "Apply a subset patch to a rule (etag-guarded).",
        schema_props_required(
            &[
                ("id", "string"),
                ("etag", "string"),
                ("predicate", "object"),
                ("actions", "array"),
                ("priority", "integer"),
                ("enabled", "boolean"),
                ("cooldown_ms", "integer"),
                ("ttl_until", "integer"),
            ],
            &["id", "etag"],
        ),
    ));
    v.push(td(
        "rules.delete",
        "Delete a rule (etag-guarded).",
        schema_props_required(&[("id", "string"), ("etag", "string")], &["id", "etag"]),
    ));
    v.push(td(
        "rules.enable",
        "Enable a rule (no etag required).",
        schema_props_required(&[("id", "string")], &["id"]),
    ));
    v.push(td(
        "rules.disable",
        "Disable a rule (no etag required).",
        schema_props_required(&[("id", "string")], &["id"]),
    ));
    v.push(td(
        "rules.approve",
        "Transition a Draft rule to Approved.",
        schema_props_required(&[("id", "string")], &["id"]),
    ));
    v.push(td(
        "rules.reload",
        "Re-read rules.toml from disk and atomically swap into the live ruleset.",
        schema_empty(),
    ));
    v.push(td(
        "rules.flush",
        "Force a sync of any debounced pending rule mutations to disk.",
        schema_empty(),
    ));
    v.push(td(
        "rules.test",
        "Dry-run: evaluate an inbound event against the live ruleset without executing actions.",
        schema_props_required(&[("event", "object")], &["event"]),
    ));
    // ─── Triggers CRUD + run (6) — Phase 5 Part E (Phase 4 RPC) ────────
    // Session A of the parity-closure plan added the Phase 1 read-only
    // tools (`triggers.list` / `triggers.get`) to the MCP surface so an
    // MCP client can enumerate triggers.
    v.push(td(
        "triggers.list",
        "List all triggers in the live triggerset.",
        schema_empty(),
    ));
    v.push(td(
        "triggers.get",
        "Fetch a single trigger by id.",
        schema_props_required(&[("id", "string")], &["id"]),
    ));
    v.push(td(
        "triggers.create",
        "Create a new trigger.",
        schema_props_optional(&[
            ("id", "string"),
            ("enabled", "boolean"),
            ("runner", "object"),
            ("rate_limit", "object"),
            ("timeout_ms", "integer"),
            ("retries", "integer"),
            ("history_cap", "integer"),
        ]),
    ));
    v.push(td(
        "triggers.update",
        "Update an existing trigger (etag-guarded optimistic concurrency).",
        schema_props_required(
            &[
                ("id", "string"),
                ("etag", "string"),
                ("runner", "object"),
                ("rate_limit", "object"),
                ("timeout_ms", "integer"),
                ("retries", "integer"),
                ("history_cap", "integer"),
                ("enabled", "boolean"),
            ],
            &["id", "etag"],
        ),
    ));
    v.push(td(
        "triggers.delete",
        "Delete a trigger (etag-guarded).",
        schema_props_required(&[("id", "string"), ("etag", "string")], &["id", "etag"]),
    ));
    v.push(td(
        "triggers.run",
        "Invoke a trigger and return the RunRecord.",
        schema_props_optional(&[("id", "string"), ("event", "object")]),
    ));
    // ─── Audit hash chain (2) — Phase 5 Part E (Phase 4 RPC) ───────────
    v.push(td(
        "audit.tail",
        "Tail audit log entries since a given sequence number (loss-recovery).",
        schema_props_optional(&[("since_seq", "integer"), ("limit", "integer")]),
    ));
    v.push(td(
        "audit.verify",
        "Walk the in-memory audit hash chain and verify each row's prev_hash matches the previous row's this_hash.",
        schema_empty(),
    ));
    // ─── Actions (1) — Phase 5 Part E (Phase 4 RPC) ────────────────────
    v.push(td(
        "actions.escalate",
        "Dispatch an escalation to a target (e.g. oncall) with a reason.",
        schema_props_required(
            &[("target", "string"), ("reason", "string")],
            &["target", "reason"],
        ),
    ));
    // ─── Accounts (3) — Phase 6.1 ──────────────────────────────────────
    v.push(td(
        "daemon.accounts.list",
        "List all linked WhatsApp accounts.",
        schema_empty(),
    ));
    v.push(td(
        "daemon.accounts.use",
        "Set the active WhatsApp account (writes the `active` symlink).",
        schema_props_required(&[("account_id", "string")], &["account_id"]),
    ));
    v.push(td(
        "daemon.accounts.info",
        "Show details for one linked WhatsApp account.",
        schema_props_required(&[("account_id", "string")], &["account_id"]),
    ));
    // Phase 1 task 15: query layer MCP tools. Gated by the
    // `query` cargo feature so absent builds see no schema leak.
    #[cfg(feature = "query")]
    {
        v.push(td(
            "daemon.search",
            "Full-text + semantic search over the persisted messages view. \
             Returns hits with BM25 score. Filters: peer, kind, since_ts_unix_ms, \
             until_ts_unix_ms, limit (default 50, max 200).",
            schema_props_required(
                &[
                    ("query", "string"),
                    ("peer", "string"),
                    ("kind", "string"),
                    ("since_ts_unix_ms", "integer"),
                    ("until_ts_unix_ms", "integer"),
                    ("limit", "integer"),
                ],
                &["query"],
            ),
        ));
        v.push(td(
            "messages.context",
            "Surrounding messages around an event_id (before + after). \
             Used to render a thread view around a hit.",
            schema_props_required(
                &[
                    ("event_id", "integer"),
                    ("before", "integer"),
                    ("after", "integer"),
                ],
                &["event_id"],
            ),
        ));
        v.push(td(
            "events.find",
            "Filter `events` rows by kind / variant / peer / ts window. \
             Returns denormalized rows (no Tantivy involvement).",
            schema_props_required(
                &[
                    ("kind", "string"),
                    ("variant", "string"),
                    ("peer", "string"),
                    ("since_ts_unix_ms", "integer"),
                    ("until_ts_unix_ms", "integer"),
                    ("limit", "integer"),
                ],
                &[],
            ),
        ));
        // Dynamic SQL surface (Phase 9) — mirrors the daemon's
        // sql.* RPCs. Safety rails (single-statement, write/read
        // allow-list) live on the daemon side.
        v.push(td(
            "sql.execute",
            "Run a single DDL/DML statement on the daemon's embedded \
             SQL store. INSERT/UPDATE/DELETE/CREATE/DROP/ALTER only. \
             Returns {rows_affected, sql, first_keyword}.",
            schema_props_required(&[("sql", "string")], &["sql"]),
        ));
        v.push(td(
            "sql.query",
            "Run a read-only SELECT/WITH/SHOW/EXPLAIN against the \
             daemon's SQL store. Returns {columns, rows, count, limit, \
             truncated}. Hard cap: 10000 rows.",
            schema_props_required(&[("sql", "string"), ("limit", "integer")], &["sql"]),
        ));
        v.push(td(
            "sql.tables",
            "List existing tables in the daemon's SQL store (SHOW TABLES).",
            schema_props_optional(&[]),
        ));
    }
    // ─── Contacts (7) + Identity (3) — Phase 7.J ────────────────────────
    // Long-lived gap: handlers were registered on the daemon RPC server
    // since Tier 4 / Tier 6.4 but the MCP tool descriptor list never
    // grew to advertise them. Adding the 10 here closes the gap; CLI
    // subcommands (`contacts` / `identity`) live in cli.rs.
    // Phase 7.J.1 adds `contacts.get_pn_lid_mappings` (batch PN→LID
    // resolution via the WA server's `usync` IQ). Phase 7.J.2 adds
    // `contacts.get_lid_pn_mappings` — the inverse direction — using
    // wacore's `Contacts::is_on_whatsapp` with LID-form JIDs. Both
    // directions of the LID↔PN mapping are now reachable through
    // public RPCs.
    v.push(td(
        "contacts.is_on_whatsapp",
        "Check whether a peer JID is a registered WhatsApp user. \
         Returns {peer, jid, on_whatsapp}. JID must be canonical \
         `<digits>@s.whatsapp.net`; LIDs/E.164 are normalized server-side.",
        schema_props_required(&[("peer", "string")], &["peer"]),
    ));
    v.push(td(
        "contacts.get_user_info",
        "Fetch rich user info for one peer: status text, picture id, \
         business flag, verified business name, linked device ids, LID \
         (when known). Returns {peer, found, info} where `info` is null \
         when the WA server has no record (privacy-hidden).",
        schema_props_required(&[("peer", "string")], &["peer"]),
    ));
    // Phase 7.J.1: batch PN → LID resolution via the WA server's
    // `usync` IQ with the `<lid>` subprotocol. Replaces N individual
    // `contacts.get_user_info` round-trips when the caller only needs
    // LIDs. The inverse direction (LID → PN) is `contacts.get_lid_pn_mappings`
    // below — uses wacore's `Contacts::is_on_whatsapp` with LID-form
    // JIDs and reads the server's `pn_jid` response attribute.
    v.push(td(
        "contacts.get_pn_lid_mappings",
        "Batch-resolve phone-number JIDs to their corresponding LIDs \
         via the WA server's `usync` IQ. Returns \
         {mappings:[{phone, lid}], not_resolved:[...], requested_count, \
         resolved_count}. Privacy-hidden phones land in `not_resolved`. \
         Max 100 phones per call (matches WA server `usync` batch limit).",
        schema_props_required(&[("phones", "array")], &["phones"]),
    ));
    // Phase 7.J.2: batch LID → PN resolution. Inverse of
    // `contacts.get_pn_lid_mappings`. Uses the same `Contacts::is_on_whatsapp`
    // wire shape WA Web's ExistsJob sends for the LID direction — server
    // returns `<user jid="NN@lid" pn_jid="MM@s.whatsapp.net">`.
    v.push(td(
        "contacts.get_lid_pn_mappings",
        "Batch-resolve LID JIDs to their corresponding phone numbers \
         via the WA server's `usync` IQ. Returns \
         {mappings:[{lid, phone_number}], not_resolved:[...], \
         requested_count, resolved_count}. Privacy-hidden LIDs land in \
         `not_resolved` (the WA server refuses to disclose the \
         associated phone number for an account the operator is not \
         authorized to see). Max 100 LIDs per call.",
        schema_props_required(&[("lids", "array")], &["lids"]),
    ));
    v.push(td(
        "groups.participants_lid_to_phone",
        "Group-scoped LID → phone-number resolution. One `w:g2` \
         GroupQueryIq (<query request=\"interactive\"/>) to the operator's \
         group JID; the server populates `phone_number=` on every LID \
         participant for LID-addressed groups (live capture: 948/948). \
         Returns {mappings:[{lid, phone_number}], not_resolved:[...], \
         resolved_count, requested_count, group_jid}. Server returns \
         Forbidden for non-member groups. \
         Orthogonal to `contacts.get_lid_pn_mappings` (usync, business-only).",
        schema_props_required(&[("group_jid", "string")], &["group_jid"]),
    ));
    v.push(td(
        "contacts.get_business_profile",
        "Fetch a peer's public business profile (description, address, \
         categories, hours). Returns {status: found|not_found, jid, \
         profile?}. JID must be a phone-number JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "contacts.get_profile_picture",
        "Fetch the profile-picture URL for a peer. `preview=true` \
         (default) requests the thumbnail; `preview=false` requests the \
         full image. Returns {peer, jid, preview, url, found} — url is \
         null when the peer has no picture or hides it via privacy.",
        schema_props_required(&[("peer", "string"), ("preview", "boolean")], &["peer"]),
    ));
    v.push(td(
        "contacts.save_contact",
        "Save or rename a contact in the local address book. Cross-device \
         sync via the WA server's app-state. JID must be a phone-number \
         JID (LIDs rejected by WA server).",
        schema_props_required(
            &[("peer", "string"), ("full_name", "string")],
            &["peer", "full_name"],
        ),
    ));
    v.push(td(
        "contact.block",
        "Add a peer to the local blocklist. Propagates to all linked \
         devices via the WA server's blocklist IQ.",
        schema_props_required(&[("peer", "string")], &["peer"]),
    ));
    v.push(td(
        "contact.unblock",
        "Remove a peer from the local blocklist. Reverses contact.block.",
        schema_props_required(&[("peer", "string")], &["peer"]),
    ));
    v.push(td(
        "identity.get_pn",
        "Return this device's PN (phone-number) JID as a string, or null \
         if not signed in. Read from the in-memory device snapshot — no \
         WA server roundtrip.",
        schema_empty(),
    ));
    v.push(td(
        "identity.get_lid",
        "Return this device's LID (local-identifier) JID as a string, or \
         null if LID migration has not occurred.",
        schema_empty(),
    ));
    v.push(td(
        "identity.is_lid_migrated",
        "Return true if the device has completed the LID migration.",
        schema_empty(),
    ));
    // ─── Communities (Tier 7.G — Phase 2026-07-15) ─────────────────
    v.push(td(
        "community.create",
        "Create a new WhatsApp community (parent group). Optionally creates \
         a default 'general' chat and links it.",
        schema_props_required(
            &[
                ("name", "string"),
                ("description", "string"),
                ("closed", "boolean"),
                ("allow_non_admin_sub_group_creation", "boolean"),
                ("create_general_chat", "boolean"),
            ],
            &["name"],
        ),
    ));
    v.push(td(
        "community.deactivate",
        "Deactivate (delete) a community by JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "community.link_subgroups",
        "Link one or more existing subgroups under a community parent.",
        schema_props_required(
            &[("community_jid", "string"), ("subgroup_jids", "array")],
            &["community_jid", "subgroup_jids"],
        ),
    ));
    v.push(td(
        "community.unlink_subgroups",
        "Unlink subgroups from a community. Optionally remove orphan members.",
        schema_props_required(
            &[
                ("community_jid", "string"),
                ("subgroup_jids", "array"),
                ("remove_orphan_members", "boolean"),
            ],
            &["community_jid", "subgroup_jids"],
        ),
    ));
    v.push(td(
        "community.get_subgroups",
        "List the subgroups linked under a community.",
        schema_props_required(&[("community_jid", "string")], &["community_jid"]),
    ));
    v.push(td(
        "community.get_subgroup_participant_counts",
        "Return a per-subgroup participant count map for a community.",
        schema_props_required(&[("community_jid", "string")], &["community_jid"]),
    ));
    v.push(td(
        "community.query_linked_group",
        "Query the metadata of one linked subgroup via the parent community.",
        schema_props_required(
            &[("community_jid", "string"), ("subgroup_jid", "string")],
            &["community_jid", "subgroup_jid"],
        ),
    ));
    v.push(td(
        "community.join_subgroup",
        "Join a linked subgroup via its parent community.",
        schema_props_required(
            &[("community_jid", "string"), ("subgroup_jid", "string")],
            &["community_jid", "subgroup_jid"],
        ),
    ));
    v.push(td(
        "community.get_linked_groups_participants",
        "Return participants across every group linked under a community.",
        schema_props_required(&[("community_jid", "string")], &["community_jid"]),
    ));
    // ─── Channels / Newsletter (14 RPCs) — Phase 7.E+ bridge ─────
    v.push(td(
        "newsletter.list_subscribed",
        "List every newsletter (channel) this account is subscribed to.",
        schema_empty(),
    ));
    v.push(td(
        "newsletter.get_metadata",
        "Fetch metadata for one newsletter/channel by JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "newsletter.create",
        "Create a new newsletter (channel).",
        schema_props_required(&[("name", "string")], &["name"]),
    ));
    v.push(td(
        "newsletter.join",
        "Join (subscribe to) a newsletter by its JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "newsletter.leave",
        "Leave (unsubscribe from) a newsletter by its JID.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "newsletter.send_reaction",
        "Send a reaction emoji to a newsletter message.",
        schema_props_required(
            &[
                ("jid", "string"),
                ("server_id", "integer"),
                ("reaction", "string"),
            ],
            &["jid", "server_id", "reaction"],
        ),
    ));
    v.push(td(
        "newsletter.edit_message",
        "Edit a message in a newsletter (channel owner only).",
        schema_props_required(
            &[
                ("jid", "string"),
                ("message_id", "string"),
                ("new_text", "string"),
            ],
            &["jid", "message_id", "new_text"],
        ),
    ));
    v.push(td(
        "newsletter.revoke_message",
        "Revoke (delete) a message in a newsletter (channel owner only).",
        schema_props_required(
            &[("jid", "string"), ("message_id", "string")],
            &["jid", "message_id"],
        ),
    ));
    v.push(td(
        "newsletter.update",
        "Update name/description of a newsletter you own.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "newsletter.set_follower_mute",
        "Mute / unmute a channel you subscribe to.",
        schema_props_required(
            &[("jid", "string"), ("muted", "boolean")],
            &["jid", "muted"],
        ),
    ));
    v.push(td(
        "newsletter.set_admin_mute",
        "Mute / unmute notifications on a channel you own.",
        schema_props_required(
            &[("jid", "string"), ("muted", "boolean")],
            &["jid", "muted"],
        ),
    ));
    v.push(td(
        "newsletter.get_metadata_by_invite",
        "Look up a channel by its invite-code (e.g. ABCD1234).",
        schema_props_required(&[("invite", "string")], &["invite"]),
    ));
    v.push(td(
        "newsletter.subscribe_live_updates",
        "Subscribe to live-update push notifications for a channel. \
         Server returns the duration (in seconds) the subscription is active.",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v.push(td(
        "newsletter.get_messages",
        "Fetch recent messages of a channel (history backfill). \
         Optional: count (default 20), before (server-id cursor).",
        schema_props_required(&[("jid", "string")], &["jid"]),
    ));
    v
}

async fn handle_tools_call(id: Value, req: &Value, socket: &Path) -> anyhow::Result<Value> {
    let tool_name = req
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    // MCP spec: client passes tool args under `params.arguments`. Fall back
    // to the whole `params` object for legacy clients that inline args.
    let arguments = req
        .get("params")
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or(Value::Null);
    let daemon_method = match tool_name {
        "version" => "version.get",
        "status" => "status.get",
        "health" => "health.get",
        // Session A: lifecycle operators surfaced on MCP.
        "reconnect.now" => "reconnect.now",
        "shutdown" => "shutdown",
        "send.text" => "send.text",
        "send.image" => "send.image",
        "send.video" => "send.video",
        "send.audio" => "send.audio",
        "send.voice" => "send.voice",
        "send.sticker" => "send.sticker",
        "send.reaction" => "send.reaction",
        "send.poll" => "send.poll",
        "send.contact" => "send.contact",
        "send.location" => "send.location",
        "send.delete" => "send.delete",
        "messages.list" => "messages.list",
        "messages.get" => "messages.get",
        "messages.search" => "messages.search",
        "messages.edit" => "messages.edit",
        "messages.mark_read" => "messages.mark_read",
        "messages.download" => "messages.download",
        "messages.read_view_once" => "messages.read_view_once",
        "messages.list_unavailable" => "messages.list_unavailable",
        "messages.list_ephemeral" => "messages.list_ephemeral",
        "chats.list" => "chats.list",
        "chats.info" => "chats.info",
        "chats.pin" => "chats.pin",
        "chats.unpin" => "chats.unpin",
        "chats.mute" => "chats.mute",
        "chats.archive" => "chats.archive",
        "chats.delete" => "chats.delete",
        "chats.typing" => "chats.typing",
        "groups.create" => "groups.create",
        "groups.list" => "groups.list",
        "groups.info" => "groups.info",
        "groups.leave" => "groups.leave",
        // Phase 6.12 groups coordinator surface.
        "groups.destroy" => "groups.destroy",
        "groups.resolve_invite" => "groups.resolve_invite",
        "groups.participants_lid_to_phone" => "groups.participants_lid_to_phone",
        "groups.add_member" => "groups.add_member",
        "groups.add_members" => "groups.add_members",
        "groups.remove_member" => "groups.remove_member",
        "groups.remove_members" => "groups.remove_members",
        "groups.promote" => "groups.promote",
        "groups.demote" => "groups.demote",
        "groups.ban" => "groups.ban",
        "groups.approve_join" => "groups.approve_join",
        "groups.rename" => "groups.rename",
        "groups.set_description" => "groups.set_description",
        "groups.set_locked" => "groups.set_locked",
        "groups.transfer_ownership" => "groups.transfer_ownership",
        // Phase 6.12.1: groups completion surface.
        "groups.set_announce" => "groups.set_announce",
        "groups.set_ephemeral" => "groups.set_ephemeral",
        "groups.set_require_approval" => "groups.set_require_approval",
        "groups.list_with_invites" => "groups.list_with_invites",
        "groups.join_by_invite" => "groups.join_by_invite",
        "groups.join_by_id" => "groups.join_by_id",
        // Phase 7.H gap list — surfaced in Session A.
        "groups.get_invite_link" => "groups.get_invite_link",
        "groups.update_member_label" => "groups.update_member_label",
        "groups.get_profile_pictures" => "groups.get_profile_pictures",
        "groups.set_profile_picture" => "groups.set_profile_picture",
        "groups.remove_profile_picture" => "groups.remove_profile_picture",
        "media.info" => "media.info",
        "envelope.encode" => "envelope.encode",
        "envelope.decode" => "envelope.decode",
        "envelope.send" => "envelope.send",
        "envelope.send-native" => "envelope.send-native",
        "capabilities" => "capabilities",
        "domain.compute-hash" => "domain.compute-hash",
        "events.list" => "events.list",
        "events.show" => "events.show",
        "events.replay" => "events.replay",
        "events.tail" => "events.tail",
        "events.list_kinds" => "events.list_kinds",
        "events.unknown_stats" => "events.unknown_stats",
        "events.unknown_stats.history" => "events.unknown_stats.history",
        "clients.list" => "clients.list",
        // Phase 1 task 15: query layer tool routing. Names match the
        // tool_descriptors() entries above.
        #[cfg(feature = "query")]
        "daemon.search" => "daemon.search",
        #[cfg(feature = "query")]
        "messages.context" => "messages.context",
        #[cfg(feature = "query")]
        "events.find" => "events.find",
        #[cfg(feature = "query")]
        "sql.execute" => "sql.execute",
        #[cfg(feature = "query")]
        "sql.query" => "sql.query",
        #[cfg(feature = "query")]
        "sql.tables" => "sql.tables",
        // Contacts (7) + Identity (3) — Phase 7.J. See the long-lived
        // gap note in tool_descriptors().
        "contacts.is_on_whatsapp" => "contacts.is_on_whatsapp",
        "contacts.get_user_info" => "contacts.get_user_info",
        "contacts.get_pn_lid_mappings" => "contacts.get_pn_lid_mappings",
        "contacts.get_lid_pn_mappings" => "contacts.get_lid_pn_mappings",
        "contacts.get_business_profile" => "contacts.get_business_profile",
        "contacts.get_profile_picture" => "contacts.get_profile_picture",
        "contacts.save_contact" => "contacts.save_contact",
        "contact.block" => "contact.block",
        "contact.unblock" => "contact.unblock",
        "identity.get_pn" => "identity.get_pn",
        "identity.get_lid" => "identity.get_lid",
        "identity.is_lid_migrated" => "identity.is_lid_migrated",
        // Communities (Tier 7.G — Phase 2026-07-15).
        "community.create" => "community.create",
        "community.deactivate" => "community.deactivate",
        "community.link_subgroups" => "community.link_subgroups",
        "community.unlink_subgroups" => "community.unlink_subgroups",
        "community.get_subgroups" => "community.get_subgroups",
        "community.get_subgroup_participant_counts" => "community.get_subgroup_participant_counts",
        "community.query_linked_group" => "community.query_linked_group",
        "community.join_subgroup" => "community.join_subgroup",
        "community.get_linked_groups_participants" => "community.get_linked_groups_participants",
        // Channels / Newsletter (14 RPCs) — Phase 7.E+ bridge.
        "newsletter.list_subscribed" => "newsletter.list_subscribed",
        "newsletter.get_metadata" => "newsletter.get_metadata",
        "newsletter.create" => "newsletter.create",
        "newsletter.join" => "newsletter.join",
        "newsletter.leave" => "newsletter.leave",
        "newsletter.send_reaction" => "newsletter.send_reaction",
        "newsletter.edit_message" => "newsletter.edit_message",
        "newsletter.revoke_message" => "newsletter.revoke_message",
        "newsletter.update" => "newsletter.update",
        "newsletter.set_follower_mute" => "newsletter.set_follower_mute",
        "newsletter.set_admin_mute" => "newsletter.set_admin_mute",
        "newsletter.get_metadata_by_invite" => "newsletter.get_metadata_by_invite",
        "newsletter.subscribe_live_updates" => "newsletter.subscribe_live_updates",
        "newsletter.get_messages" => "newsletter.get_messages",
        "daemon.accounts.list" => "daemon.accounts.list",
        "daemon.accounts.use" => "daemon.accounts.use",
        "daemon.accounts.info" => "daemon.accounts.info",
        "daemon.methods.list" => "daemon.methods.list",
        "daemon.methods.help" => "daemon.methods.help",
        "security.rotate_token" => "security.rotate_token",
        "security.revoke_all_tokens" => "security.revoke_all_tokens",
        "security.list_tokens" => "security.list_tokens",
        // Phase 4 RPC surface exposed via Phase 5 Part E wrappers.
        // Session A: Phase 1 read-only rules added so MCP clients can
        // enumerate rules before mutating them.
        "rules.list" => "rules.list",
        "rules.get" => "rules.get",
        "rules.create" => "rules.create",
        "rules.update" => "rules.update",
        "rules.patch" => "rules.patch",
        "rules.delete" => "rules.delete",
        "rules.enable" => "rules.enable",
        "rules.disable" => "rules.disable",
        "rules.approve" => "rules.approve",
        "rules.reload" => "rules.reload",
        "rules.flush" => "rules.flush",
        "rules.test" => "rules.test",
        // Session A: Phase 1 read-only triggers added so MCP clients can
        // enumerate triggers before mutating them.
        "triggers.list" => "triggers.list",
        "triggers.get" => "triggers.get",
        "triggers.create" => "triggers.create",
        "triggers.update" => "triggers.update",
        "triggers.delete" => "triggers.delete",
        "triggers.run" => "triggers.run",
        "audit.tail" => "audit.tail",
        "audit.verify" => "audit.verify",
        "actions.escalate" => "actions.escalate",
        other => {
            return Ok(jsonrpc_error(
                id,
                -32601,
                &format!("tool {:?} not implemented", other),
            ));
        }
    };
    let daemon_result = forward_to_daemon(socket, daemon_method, arguments).await?;
    // The tag lives on the parent object; reading `code`/`message` from
    // the parent (not from `get("__rpc_error__")`, which would yield the
    // Bool value of the tag instead of the parent object).
    let is_error = daemon_result
        .as_object()
        .and_then(|o| o.get("__rpc_error__"))
        .is_some();
    if is_error {
        let code = daemon_result.get("code").cloned().unwrap_or(Value::Null);
        let message = daemon_result.get("message").cloned().unwrap_or(Value::Null);
        let body = serde_json::json!({ "code": code, "message": message });
        let text = serde_json::to_string(&body)
            .unwrap_or_else(|_| format!("daemon RPC failed (code={code}, message={message})"));
        return Ok(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "isError": true,
                "content": [{"type": "text", "text": text}],
            },
        }));
    }
    let text = if daemon_result.is_null() {
        "null".to_string()
    } else {
        serde_json::to_string(&daemon_result)?
    };
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"content": [{"type": "text", "text": text}]},
    }))
}

async fn forward_to_daemon(socket: &Path, method: &str, params: Value) -> anyhow::Result<Value> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    let mut s = UnixStream::connect(socket)?;
    let req = serde_json::json!({"id": 1, "method": method, "params": params});
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    s.write_all(line.as_bytes())?;
    // Server keeps connections open for further requests, so read exactly
    // one line via BufReader::read_line instead of read_to_string (which
    // would block until EOF).
    let mut reader = BufReader::new(s);
    let mut buf = String::new();
    reader.read_line(&mut buf)?;
    let resp: Value = serde_json::from_str(buf.trim())?;
    // Classify the JSON-RPC response. Errors get tagged with
    // `__rpc_error__: true` so `handle_tools_call` can flip
    // `isError: true` on the MCP tool result; otherwise the
    // legitimate result (which may itself be `null`) is forwarded.
    // Pre-fix bug: errors were silently dropped to `Value::Null`
    // here, so a parse error from `sql.execute` (e.g. trying to
    // create a table with `TEXT PRIMARY KEY` inline, which stoolap
    // rejects) appeared to the operator as a successful `null` return.
    Ok(classify_daemon_response(resp))
}

/// Reduce a raw JSON-RPC response from the daemon to either a tagged
/// error value (when the response has an `error` field) or the
/// `result` value (when it doesn't). The tagged-error shape is
/// `{__rpc_error__: true, code, message}`; the caller
/// (`handle_tools_call`) branches on the tag to flip `isError: true`
/// on the MCP tool result.
///
/// Pure function — no socket, no async, no globals. The hermetic
/// tests in `mod tests` exercise every branch (error, result, both,
/// neither) without needing a live daemon.
fn classify_daemon_response(resp: Value) -> Value {
    if let Some(err) = resp.get("error") {
        let code = err.get("code").cloned().unwrap_or(Value::Null);
        let message = err.get("message").cloned().unwrap_or(Value::Null);
        return serde_json::json!({
            "__rpc_error__": true,
            "code": code,
            "message": message,
        });
    }
    resp.get("result").cloned().unwrap_or(Value::Null)
}

fn jsonrpc_error(id: Value, code: i32, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
}

// ── Tool descriptor helpers ────────────────────────────────────────────

fn td(name: &str, description: &str, input_schema: Value) -> Value {
    serde_json::json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn schema_empty() -> Value {
    serde_json::json!({"type": "object", "properties": {}})
}

fn schema_props_required(props: &[(&str, &str)], required: &[&str]) -> Value {
    let mut p = serde_json::Map::new();
    for (k, ty) in props {
        p.insert((*k).to_string(), serde_json::json!({"type": *ty}));
    }
    serde_json::json!({
        "type": "object",
        "properties": Value::Object(p),
        "required": required,
    })
}

fn schema_props_optional(props: &[(&str, &str)]) -> Value {
    let mut p = serde_json::Map::new();
    for (k, ty) in props {
        p.insert((*k).to_string(), serde_json::json!({"type": *ty}));
    }
    serde_json::json!({
        "type": "object",
        "properties": Value::Object(p),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 51: send.* tool descriptors must be present.
    #[test]
    fn send_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "send.text",
            "send.image",
            "send.video",
            "send.audio",
            "send.voice",
            "send.sticker",
            "send.reaction",
            "send.poll",
            "send.contact",
            "send.location",
            "send.delete",
        ] {
            assert!(names.contains(m), "send tool {m:?} not advertised");
        }
    }

    /// Task 52: messages.* + chats.* + media.info tool descriptors.
    #[test]
    fn messages_chats_media_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "messages.list",
            "messages.get",
            "messages.search",
            "messages.edit",
            "messages.mark_read",
            "messages.download",
            "messages.read_view_once",
            "messages.list_unavailable",
            "messages.list_ephemeral",
            "chats.list",
            "chats.info",
            "chats.pin",
            "chats.unpin",
            "chats.mute",
            "chats.archive",
            "chats.delete",
            "chats.typing",
            "media.info",
        ] {
            assert!(names.contains(m), "tool {m:?} not advertised");
        }
    }

    /// Task 53: envelope.* + capabilities + domain.compute-hash.
    #[test]
    fn envelope_capabilities_domain_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "envelope.encode",
            "envelope.decode",
            "envelope.send",
            "envelope.send-native",
            "capabilities",
            "domain.compute-hash",
        ] {
            assert!(names.contains(m), "tool {m:?} not advertised");
        }
    }

    /// Phase 5 Part E: 17 Phase 4 RPC methods now exposed as MCP tools.
    /// The exact count is locked via `EXPECTED_TOOL_COUNT` (80 = 49 + 17
    /// Phase 5 Part E + 14 Phase 6.12 coordinator). This test asserts
    /// each name is present so a typo in a tool descriptor fails fast in
    /// CI rather than at consumer time.
    #[test]
    fn phase4_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "rules.create",
            "rules.update",
            "rules.patch",
            "rules.delete",
            "rules.enable",
            "rules.disable",
            "rules.approve",
            "rules.reload",
            "rules.flush",
            "rules.test",
            "triggers.create",
            "triggers.update",
            "triggers.delete",
            "triggers.run",
            "audit.tail",
            "audit.verify",
            "actions.escalate",
        ] {
            assert!(names.contains(m), "Phase 4 tool {m:?} not advertised");
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    /// Phase 6.12: 14 `groups.*` coordinator RPCs now exposed as MCP
    /// tools. Mirrors the Phase 5 Part E test — asserts each name is
    /// advertised so a typo in a tool descriptor fails in CI rather than
    /// at consumer time.
    #[test]
    fn phase612_groups_coordinator_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "groups.destroy",
            "groups.resolve_invite",
            "groups.add_member",
            "groups.add_members",
            "groups.remove_member",
            "groups.remove_members",
            "groups.promote",
            "groups.demote",
            "groups.ban",
            "groups.approve_join",
            "groups.rename",
            "groups.set_description",
            "groups.set_locked",
            "groups.transfer_ownership",
        ] {
            assert!(
                names.contains(m),
                "Phase 6.12 groups coordinator tool {m:?} not advertised"
            );
        }
    }

    /// Phase 6.12.1: 6 `groups.*` completion RPCs (announce / ephemeral /
    /// require_approval / list_with_invites / join_by_invite / join_by_id)
    /// now exposed as MCP tools. Mirrors the prior coordinator test.
    #[test]
    fn phase612_1_groups_completion_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "groups.set_announce",
            "groups.set_ephemeral",
            "groups.set_require_approval",
            "groups.list_with_invites",
            "groups.join_by_invite",
            "groups.join_by_id",
        ] {
            assert!(
                names.contains(m),
                "Phase 6.12.1 groups completion tool {m:?} not advertised"
            );
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    /// Phase 6.1: 3 `daemon.accounts.*` RPCs (list, use, info) now exposed
    /// as MCP tools. Mirrors the prior coordinator / completion tests.
    #[test]
    fn phase61_accounts_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "daemon.accounts.list",
            "daemon.accounts.use",
            "daemon.accounts.info",
        ] {
            assert!(
                names.contains(m),
                "Phase 6.1 accounts tool {m:?} not advertised"
            );
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    /// Session A of the parity-closure plan: 5 Phase 7.H `groups.*` gap
    /// list RPCs (get_invite_link / update_member_label /
    /// get_profile_pictures / set_profile_picture /
    /// remove_profile_picture) now exposed as MCP tools.
    #[test]
    fn session_a_phase7h_groups_gap_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "groups.get_invite_link",
            "groups.update_member_label",
            "groups.get_profile_pictures",
            "groups.set_profile_picture",
            "groups.remove_profile_picture",
        ] {
            assert!(
                names.contains(m),
                "Session A Phase 7.H groups gap tool {m:?} not advertised"
            );
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    /// Phase 7.K — View-Once + Disappearing messages. 3 new tools:
    /// `messages.read_view_once` (one-shot CDN fetch; marks consumed),
    /// `messages.list_unavailable` (companion fanout audit by kind),
    /// `messages.list_ephemeral` (active disappearing-message timers).
    #[test]
    fn phase7k_view_once_disappearing_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "messages.read_view_once",
            "messages.list_unavailable",
            "messages.list_ephemeral",
        ] {
            assert!(
                names.contains(m),
                "Phase 7.K view-once/disappearing tool {m:?} not advertised"
            );
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    /// Session A of the parity-closure plan: 6 read-only / lifecycle
    /// operators that already had CLI subcommands but no MCP surface.
    /// (reconnect.now / shutdown / rules.list / rules.get /
    /// triggers.list / triggers.get).
    #[test]
    fn session_a_lifecycle_and_readonly_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "reconnect.now",
            "shutdown",
            "rules.list",
            "rules.get",
            "triggers.list",
            "triggers.get",
        ] {
            assert!(
                names.contains(m),
                "Session A lifecycle/readonly tool {m:?} not advertised"
            );
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    /// Phase 7.J: the 10 contact + identity tools that were long-lived
    /// RPC handlers on the daemon but never had MCP tool descriptors.
    /// Phase 7.J.1 adds one (`contacts.get_pn_lid_mappings`); Phase 7.J.2
    /// adds the inverse (`contacts.get_lid_pn_mappings`). Asserts each
    /// name shows up in `tools/list` so the `tool ... not implemented`
    /// MCP error disappears for clients.
    #[test]
    fn phase7j_contact_identity_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "contacts.is_on_whatsapp",
            "contacts.get_user_info",
            // Phase 7.J.1: batch PN → LID resolution via usync IQ.
            "contacts.get_pn_lid_mappings",
            // Phase 7.J.2: batch LID → PN resolution via Contacts::is_on_whatsapp.
            "contacts.get_lid_pn_mappings",
            "contacts.get_business_profile",
            "contacts.get_profile_picture",
            "contacts.save_contact",
            "contact.block",
            "contact.unblock",
            "identity.get_pn",
            "identity.get_lid",
            "identity.is_lid_migrated",
        ] {
            assert!(
                names.contains(m),
                "Phase 7.J contact/identity tool {m:?} not advertised"
            );
        }
        assert_eq!(
            descs.len(),
            EXPECTED_TOOL_COUNT,
            "EXPECTED_TOOL_COUNT drift: descriptors={} expected={}",
            descs.len(),
            EXPECTED_TOOL_COUNT
        );
    }

    // ── classify_daemon_response (2026-07-13) ────────────────────────
    //
    // Background: the MCP wrapper used to silently drop JSON-RPC
    // errors from the daemon by collapsing `resp.error` to
    // `Value::Null`. The operator would see a content text of
    // `"null"` and assume the call had succeeded. Real failure
    // example: `sql.execute("CREATE TABLE t (id TEXT PRIMARY KEY)")`
    // returns a parse error from stoolap ("PRIMARY KEY column 'id'
    // must be INTEGER"), but the MCP wrapper ate it. The fix is the
    // tagged-value pattern in `classify_daemon_response` — the
    // caller flips `isError: true` on the tool result, and the
    // operator sees the actual error message.

    #[test]
    fn classify_daemon_response_tags_error_with_code_and_message() {
        let resp = serde_json::json!({
            "id": 1,
            "error": {
                "code": -32603,
                "message": "sql.execute: parse error: PRIMARY KEY column 'id' must be INTEGER",
            },
        });
        let out = classify_daemon_response(resp);
        assert_eq!(out["__rpc_error__"], serde_json::json!(true));
        assert_eq!(out["code"], serde_json::json!(-32603));
        assert!(
            out["message"].as_str().unwrap().contains("parse error"),
            "message should propagate verbatim; got: {:?}",
            out["message"]
        );
    }

    #[test]
    fn classify_daemon_response_forwards_result_when_present() {
        let resp = serde_json::json!({
            "id": 1,
            "result": {"first_keyword": "SELECT", "rows": [[1, 2, 3]], "count": 1},
        });
        let out = classify_daemon_response(resp);
        assert!(out.get("__rpc_error__").is_none());
        assert_eq!(out["first_keyword"], serde_json::json!("SELECT"));
        assert_eq!(out["rows"], serde_json::json!([[1, 2, 3]]));
    }

    #[test]
    fn classify_daemon_response_preserves_explicit_null_result() {
        // The daemon can legitimately return `result: null` for DDL
        // statements whose rows-affected is unit-typed. The helper
        // must NOT confuse that with an error.
        let resp = serde_json::json!({"id": 1, "result": null});
        let out = classify_daemon_response(resp);
        assert!(out.is_null(), "explicit null must stay null; got {out:?}");
        assert!(out.get("__rpc_error__").is_none());
    }

    #[test]
    fn classify_daemon_response_handles_missing_both_fields() {
        // Defensive: a malformed daemon response (no `result` and no
        // `error`) used to map to `null` under the old code. The
        // helper preserves that — the caller can't differentiate
        // "null result" from "garbage response" without more
        // context, but that's fine because legitimate responses
        // always have exactly one of the two fields.
        let resp = serde_json::json!({"id": 1});
        let out = classify_daemon_response(resp);
        assert!(out.is_null());
    }

    #[test]
    fn classify_daemon_response_tolerates_missing_message_field() {
        // Some error paths may have `code` but no `message` (e.g.
        // library-internal errors). The helper must not panic; it
        // should set `message: null` and still tag the error.
        let resp = serde_json::json!({"id": 1, "error": {"code": -1}});
        let out = classify_daemon_response(resp);
        assert_eq!(out["__rpc_error__"], serde_json::json!(true));
        assert_eq!(out["code"], serde_json::json!(-1));
        assert!(out["message"].is_null());
    }

    // ── handle_tools_call end-to-end with a mock daemon socket ────────
    //
    // The classify tests above prove the response-classification
    // helper is correct in isolation. But the live symptom was a
    // different failure mode: the `code`/`message` fields came
    // through as `null` in the final MCP response even though
    // `classify_daemon_response` produces them correctly. So we
    // need an end-to-end test that drives the full path:
    //   handle_tools_call -> forward_to_daemon -> classify_daemon_response
    //
    // We bind a real unix socket, spawn a server thread that reads
    // one JSON-RPC request and writes a hardcoded error response,
    // then call `handle_tools_call` against that socket and assert
    // the shape of the final MCP tool result.

    #[cfg(feature = "query")]
    #[tokio::test(flavor = "multi_thread")]
    async fn handle_tools_call_propagates_rpc_error_via_is_error() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::time::Duration;

        let tmp = tempfile::tempdir().expect("tempdir");
        let sock_path = tmp.path().join("fake-daemon.sock");
        let listener = UnixListener::bind(&sock_path).expect("bind socket");

        // Spawn a one-shot daemon thread: accept one connection,
        // read one JSON-RPC line, write the hardcoded error
        // response, then drop the stream.
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request");
            let resp = r#"{"id":1,"error":{"code":-32603,"message":"sql.execute: parse error: PRIMARY KEY column 'id' must be INTEGER type, got Text."}}"#;
            writeln!(stream, "{}", resp).expect("write response");
            stream.flush().expect("flush");
        });

        // Drive the full MCP path. The req mimics what `serve`
        // would forward from a stdio MCP client.
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": "sql.execute",
                "arguments": {"sql": "CREATE TABLE x (id TEXT PRIMARY KEY)"},
            },
        });
        let resp = handle_tools_call(serde_json::json!(99), &req, &sock_path)
            .await
            .expect("handle_tools_call");

        // Assert MCP result shape: isError=true, content text contains
        // the actual error code and message (NOT literal "null").
        let result = &resp["result"];
        assert_eq!(
            result["isError"],
            serde_json::json!(true),
            "expected isError:true, got: {resp}"
        );
        let text = result["content"][0]["text"]
            .as_str()
            .expect("content[0].text is a string");
        assert!(
            text.contains("parse error"),
            "content text should carry the daemon error message; got: {text:?}"
        );
        assert!(
            text.contains("-32603"),
            "content text should carry the JSON-RPC error code -32603; got: {text:?}"
        );
        assert!(
            !text.contains("\"code\":null"),
            "code field must not be null; got: {text:?}"
        );
        assert!(
            !text.contains("\"message\":null"),
            "message field must not be null; got: {text:?}"
        );

        // Give the server thread a moment to exit cleanly.
        let join = std::thread::Builder::new()
            .spawn(move || server.join())
            .expect("spawn join");
        // Bound the wait so a stuck server doesn't hang the test.
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            let _ = tokio::task::spawn_blocking(move || join.join()).await.ok();
        })
        .await;
    }
}
