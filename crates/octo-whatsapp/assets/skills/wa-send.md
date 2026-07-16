---
name: wa-send
description: Outbound WhatsApp workflow guide. Use when an agent needs to send text, media, reactions, polls, contacts, locations, or revoke/forward/edit messages through octo-whatsapp MCP tools. Triggers: "send a message", "reply to", "revoke", "forward", "react with emoji", "send poll", "share contact", "share location", "delete message", "edit message", "mark read". Complements wa-mcp (full catalog) with a workflow-oriented entry point.
metadata:
  version: "1.0.0"
  tools_covered: 13
  source: crates/octo-whatsapp/assets/skills/wa-mcp.md (sections 2 + 3)
---

# wa-send — Outbound workflow

Goal: take a user instruction like "send X to Y" and produce a sequence of `mcp__octo-whatsapp__*` tool calls that succeed against the live daemon, with proper rate-limit, payload-size, and recipient-format guards.

## When to use this playbook

Trigger on any of:
- "send a message to <peer>"
- "reply to <message>"
- "react with <emoji>"
- "send a poll"
- "share my location" / "share this contact"
- "edit the message I just sent"
- "revoke / delete that message"
- "forward to <peer>"
- "mark messages as read"

If the user only wants to **read** state, use `wa-monitor` instead. If the user wants to **observe events**, `wa-monitor`. If the user wants to **configure** rules/triggers, `wa-config`.

## Ground rules (non-negotiable)

1. **WA rate-limit floor = 2 s.** Sleep ≥ 2000 ms between any two outbound calls. The daemon enforces this internally; do not pre-empt.
2. **Peer format** (see `wa-mcp` §Ground rules):
   - E.164 phone: `+15551234567`
   - LID: `1234567890:12@lid`
   - JID: `1234567890@s.whatsapp.net` or `<groupid>@g.us`
   - Anything else → reject with `InvalidParams`.
3. **Payload limits**: text ≤ 65536 bytes UTF-8; image/video/document see `wa-mcp` §2 per-tool limits.
4. **Never push to git or open PRs** without explicit operator authorization. Local-only worktrees only.
5. **Event table is ground truth.** Every successful send produces a `Message` event with the returned `message_id`. If the event does not appear within 10 s, treat the send as failed even if the RPC returned success.

## Workflow

### A. Text message

```
1. Resolve peer:
   - If user gave a phone, format as +CCNNNN... (E.164).
   - If user gave a name, look up via contacts.is_on_whatsapp (if available)
     or ask the user. Do NOT guess.
2. mcp__octo-whatsapp__send_text { peer, text }
3. Expect response: { message_id, status, peer, ts_unix_ms }.
4. Optionally: events.list { kind: "message", since_ts: now-5s }
   to confirm the receipt chain (server_ack → delivered).
```

If user asked to reply to an existing message, add `reply_to: <message_id>` parameter.

If user asked to mention someone in a group, add `mentions: [jid1, jid2]` (only valid for group peers).

### B. Media

Sub-workflow:

```
1. Confirm file path + media type. Reject mime mismatches:
   - image: image/* (PNG/JPEG/WebP)
   - video: video/* (mp4)
   - audio: audio/* (wav/mp3/ogg)
   - voice: audio/ogg;codecs=opus (voice-note flag set)
   - sticker: image/webp (sticker flag set, animated webp supported)
   - document: application/* (PDF/DOCX/...)
2. Confirm caption ≤ 1024 bytes if provided.
3. mcp__octo-whatsapp__send_{image,video,audio,voice,sticker,document}
   { peer, path, caption?, mime_type? }
4. Expect response: { message_id, status, peer, media_sha256 }.
```

If the file does not exist locally, abort with a clear error before invoking the RPC. The daemon reads from disk.

### C. Reactions

```
1. Get the target message_id (messages.list or messages.search).
2. mcp__octo-whatsapp__send_reaction { peer, message_id, emoji }
3. emoji must be a single grapheme cluster (e.g. "👍", "❤️", "🔥").
   Empty string removes a prior reaction.
```

Rate-limit note: reactions are cheaper than text but the 2 s floor still applies per session.

### D. Polls

```
1. mcp__octo-whatsapp__send_poll
   { peer, question, options: ["opt1","opt2",...], max_selections?: 1..N }
2. question ≤ 256 chars, options 2..12, each ≤ 100 chars.
3. To vote on an existing poll, use polls.vote (Phase 7.B).
```

### E. Contact / location share

```
- send.contact { peer, contact_vcard or contact_phone, contact_name }
- send.location { peer, lat, lon, name?, address? }
```

Both produce `Message` events with kind=contact/location respectively.

### F. Revoke (delete for everyone)

```
1. mcp__octo-whatsapp__send.delete { peer, message_id }
   (the daemon enforces you can only revoke your own messages)
2. On receiver side, expect an inbound Message event with revoke=true.
```

For "delete for me" use `chats.delete` or `messages.delete_for_me` (different RPC; check `wa-mcp` §4 if the user asked for local-only deletion).

### G. Edit

```
- mcp__octo-whatsapp__messages.edit { peer, message_id, new_text }
- Only your own text messages can be edited, within ~15 min of send (WA window).
- Receiver gets a Message event with edit=true.
```

### H. Forward

```
- mcp__octo-whatsapp__messages.forward { source_peer, message_id, target_peer }
- Or use send.forward depending on the daemon version (check wa-mcp §3).
```

### I. Mark read

```
- mcp__octo-whatsapp__messages.mark_read { peer, up_to_message_id }
- Marks every message in the chat up to and including the given id.
- Triggers Receipt events on the sender side.
```

## Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `InvalidParams: peer` | Bad peer format | Strip whitespace, validate E.164/LID/JID |
| `PayloadTooLarge` | Text > 65536 B or media too big | Split into chunks or downscale media |
| RPC returned success but no event in 10 s | Daemon disconnected | Call status.get → reconnect.now if needed |
| `RateLimited` | Burst of calls | Sleep ≥ 2 s, retry once |
| `NotAuthorized` | Account not linked | Surface to operator; do not auto-link |
| Reaction not visible to recipient | Empty emoji or wrong message_id | Re-query messages.list to confirm id |

## Tool reference (subset)

For full schema and examples, see `wa-mcp`:

- `wa-mcp` §2 Send (media + control) — 11 tools: send.text/image/video/audio/voice/sticker/reaction/poll/contact/location/delete
- `wa-mcp` §3 Messages (6) — messages.list/get/search/edit/mark_read/download
- `wa-mcp` §4 Chats (8) — relevant for delete/typing/pin context

## Verification pattern

After every send:

```rust
// Pseudocode for an integration test
let r = send_text(peer, text).await?;
let evt = events::wait_for(|e| {
    matches!(e, InboundEvent::Message { peer, id, .. }
             if id == r.message_id && peer == r.peer)
}, Duration::from_secs(10)).await?;
```

For batched sends, sleep 2 s between each call. The floor is not optional.

## Pointers

- Full tool catalog: `wa-mcp.md`
- Events + wait_for helper: `wa-monitor.md`
- Recovery + reconnect: `wa-recover.md`
- Rules + triggers: `wa-config.md`