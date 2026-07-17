---
name: wa-monitor
description: Inbound observation + query workflow guide. Use when an agent needs to watch the events table, query recent messages or chats, search history, or wait for a specific event condition. Triggers: "watch for messages from X", "list recent chats", "search for messages containing Y", "what happened in the last hour", "wait until peer Z comes online", "show unread count". Read-only — never sends or modifies state.
metadata:
  version: "1.0.0"
  tools_covered: 15
  source: crates/octo-whatsapp/assets/skills/wa-mcp.md (sections 4 + 9 + 10)
---

# wa-monitor — Inbound observation + query workflow

Goal: answer "what is happening" / "what happened" questions by querying the daemon's events table and message store. Pure read-only; no state mutation.

## When to use this playbook

Trigger on any of:
- "watch for messages from <peer>"
- "list recent chats" / "list unread chats"
- "search for messages containing <text>"
- "what messages arrived in the last hour"
- "wait until <peer> is online"
- "show the latest message in <chat>"
- "what's the unread count for <peer>"

If the user wants to **send** anything, use `wa-send` instead. If they want to **configure** behavior (rules/triggers), use `wa-config`. If they want to **recover** from a disconnect, use `wa-recover`.

## Ground rules

1. **Event table is the source of truth.** Never infer state from poll responses alone; always confirm via `events.list` or `events.wait_for`.
2. **Read-only — never call mutating RPCs** from this workflow. If you need to send, mark read, or delete, switch playbooks.
3. **2 s floor between queries is NOT required** for read-only calls, but **avoid tight loops**: poll ≤ 1 Hz unless waiting on a critical event.
4. **Backpressure**: if `events.list` returns > 1000 entries, narrow with `since_ts` / `until_ts` / `kind` filters before paginating.
5. **No push/PR without operator authorization.** Local-only.

## Tools at a glance

| Tool | Purpose |
|---|---|
| `events.list` | List events with filters |
| `events.wait_for` | Block until predicate matches an event |
| `events.subscribe` | Long-lived stream subscription |
| `events.unsubscribe` | Stop a subscription |
| `messages.list` | List messages in a chat |
| `messages.get` | Fetch a single message by id |
| `messages.search` | Full-text search across all chats |
| `chats.list` | List all chats (sorted by recent activity) |
| `chats.info` | Metadata for one chat |
| `agent.discover` | List other agents on the network (Phase 3) |
| `agent.whoami` | Identity of the current account |
| `agent.peers` | Active peer connections |
| `capabilities.list` | What the daemon supports |
| `domain.compute-hash` | Hash arbitrary bytes |
| `envelope.open` | Decrypt a sealed envelope |

For full schemas and examples, see `wa-mcp` §3 (Messages), §4 (Chats), §9 (Events), §10 (Agent discovery), §7 (Capabilities), §6 (Envelope).

## Workflow

### A. "What just happened?" — tail the events table

```
1. mcp__octo-whatsapp__events.list { limit: 50, since_ts: now-300s }
2. Filter client-side by event.kind / event.peer as needed.
3. If empty and the user wants real-time, jump to B (wait_for).
```

### B. "Watch for X" — block on a predicate

```
1. Determine the predicate. Examples:
   - "message from <peer>" → InboundEvent::Message { peer: .. }
   - "peer online"        → InboundEvent::Presence { peer, state: Available }
   - "delivered receipt"  → InboundEvent::Receipt { state: Delivered, .. }
2. mcp__octo-whatsapp__events.wait_for
   { predicate_json: "<serialized predicate>", timeout_secs: 30 }
3. If timeout, surface to the caller. Do not retry indefinitely.
```

Predicates are JSON-serialized; see `wa-mcp` §9 for the exact schema.

### C. "What chats do I have?" — chat inventory

```
1. mcp__octo-whatsapp__chats.list { limit: 100 }
2. For each chat, optionally:
   mcp__octo-whatsapp__chats.info { chat_jid }
3. To find unread only, filter client-side on chat.unread_count > 0.
```

### D. "Show me messages in <chat>" — chat history

```
1. mcp__octo-whatsapp__messages.list
   { peer, limit: 50, before_ts?: <cursor> }
2. Paginate by passing the oldest message's ts as `before_ts`.
3. To fetch a single message: messages.get { peer, message_id }.
```

### E. "Search for messages about <topic>" — full-text

```
1. mcp__octo-whatsapp__messages.search { query: "<text>", limit: 50 }
2. Results include peer + ts + snippet. Use messages.get for full body.
```

Search is case-insensitive substring match over decrypted text bodies. Encrypted media metadata is searchable but the body of media-only messages is not indexed.

### F. "Is peer X online?" — presence query

```
1. mcp__octo-whatsapp__events.list { kind: "presence", peer: <jid>, limit: 1 }
2. If empty, peer presence is unknown (subscribed but no recent update).
3. To force a refresh, use `chats.subscribe_presence` then re-query.
```

### G. Long-lived observation — stream

```
1. mcp__octo-whatsapp__events.subscribe { kinds: ["message","receipt"] }
   → returns subscription_id.
2. In your agent loop, periodically poll events.list
   { since_ts: <last_seen>, subscription_id } to drain.
3. On shutdown, events.unsubscribe { subscription_id }.
```

Subscriptions are daemon-side: a daemon restart loses them, but events are still persisted to NDJSON + DB, so you can recover by replaying from `since_ts`.

## Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `events.list` returns 0 events | Daemon not running OR filters too narrow | status.get → check `connected`; drop filters |
| `events.wait_for` times out | Predicate never matched | Re-evaluate predicate; broaden scope; increase timeout |
| `messages.search` empty for known text | Text was in a quoted/forwarded message with no indexed body | messages.list the chat directly |
| `chats.list` huge (>10k) | Long-running account, no DB prune | Apply `since_ts` filter; consider archive workflow |
| Presence stale by hours | Peer backgrounded their app | Subscribe again; presence is best-effort |

## Tool reference (subset)

For full schema and examples, see `wa-mcp`:

- `wa-mcp` §3 Messages — list/get/search/edit/mark_read/download
- `wa-mcp` §4 Chats — list/info/pin/unpin/mute/archive/delete/typing
- `wa-mcp` §9 Events — list/wait_for/subscribe/unsubscribe
- `wa-mcp` §10 Agent discovery — discover/whoami/peers
- `wa-mcp` §7 Capabilities + domain — capabilities.list + domain.compute-hash
- `wa-mcp` §6 Envelope — open/seal/verify/inspect

## Pattern: wait_for with retry

```rust
// Pseudocode for an integration test
async fn wait_for_message(peer: &str, text_contains: &str, timeout: Duration)
    -> Result<InboundEvent, WaitError>
{
    events::wait_for(
        |e| matches!(e,
            InboundEvent::Message { peer: p, text, .. }
            if p == peer && text.contains(text_contains)
        ),
        timeout,
    ).await
}
```

For multi-condition predicates (e.g. "delivered to peer X AND read by peer Y"), chain two `wait_for` calls in series — the daemon does not support AND-of-predicates natively.

## Pointers

- Full tool catalog: `wa-mcp.md`
- Outbound workflow: `wa-send.md`
- Recovery + reconnect: `wa-recover.md`
- Rules + triggers + audit: `wa-config.md`