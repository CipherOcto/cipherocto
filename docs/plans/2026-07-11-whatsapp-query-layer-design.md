# WhatsApp Query Layer — Design

## Context

`octo-whatsapp` ships three read surfaces today, all with gaps:

1. **`events.list` / `events.show` / `events.replay`** — live but in-memory only (`EventsBuffer` ring). Survives restart via `events_persister.rs` NDJSON hydrate.
2. **`events.tail`** — live but always returns `lagged: 0`; streaming + Lagged counter is **deferred to Phase 3 Part B** (events_router + per-sink mpsc). **Out of scope for this plan.**
3. **`messages.list`** — **stub**, returns `{"messages":[], "phase":"phase2"}`.
4. **`messages.search`** — exists but scans chat JID/name substrings via `StoolapStore::list_conversations()`; the `since` param is dead code (`#[allow(dead_code)]`). The Phase 2.5 comment at `inherent.rs:1278` says: "Without a message-text index we can only match on the JID itself or the chat name."
5. **`messages.get`** — calls `adapter.message_search(&msg_id, None)` and post-filters for exact id match. Works because of the post-filter, but the adapter call itself is the broken one.
6. **No message-text FTS, no semantic search, no coverage visibility.**

This plan delivers a comprehensive query layer backed by the CipherOcto `stoolap` fork (file-based embedded SQL) + a Tantivy sidecar (FTS) + local candle embeddings (semantic) + OpenAI-compatible remote fallback (semantic, opt-in). Dual-write: NDJSON stays canonical; the new stores are derived views, rebuildable from NDJSON on boot.

Scope is **messages + their embeddings** + a small set of `query.*` ops. `events.*` read surface is unchanged; `events.tail` deferred work is acknowledged but not addressed.

## Goals (numbered, verifiable)

1. Comprehensive message read surface: filter by peer / chat / sender / kind / time range / from_me / is_group, with sort + pagination.
2. Full-text search across message bodies (BM25 via Tantivy with `simple()` tokenizer — language-agnostic, no English Porter bias).
3. Semantic search across message bodies (cosine similarity via local `all-MiniLM-L6-v2` quantized Q4, 384 dims).
4. Hybrid retrieval: RRF fusion of FTS top-K + semantic top-K with alpha blend, default 0.5.
5. Existing stubs replaced without breaking wire contracts.
6. Persistence across restart via boot-time reseed from NDJSON.
7. Coverage observability: `messages.coverage` returns messages vs embeddings counts + by-model + by-provider breakdowns.
8. Feature-gated behind a single `query` cargo feature so existing builds stay untouched.

## Non-goals (explicit deferrals)

- `events.tail` `lagged: N` counter and per-sink mpsc streaming → Phase 3 Part B (events_router).
- CJK + Romance-language stemming → follow-up contrib session via `tantivy-analysis-contrib` / `jieba-rs` / `lindera`. `simple()` ships v1.
- HNSW-indexed vector search at scale → blocked on `stoolap` fork shipping the integration (TODOs at `stoolap/src/storage/vector/search.rs:79,93,139`). v1 brute-force is correct up to ~500k embeddings.
- Multi-account / cross-daemon query → out of scope; one embedded DB per daemon.
- Migrating existing `events_persister.rs` NDJSON to stoolap canonical → out of scope; NDJSON stays canonical, dual-write only.
- Live test infrastructure overhaul (`live_chain_*` regex/fixture shape stays the same).

## Architecture

```
[Inbound events from wacore]
            │
            ▼
   ┌───────────────────────┐
   │ Persister (existing)  │ ── write ──► NDJSON (canonical)  ◄─── boot reseed
   └───────────────────────┘
            │ fan-out (mpsc broadcast)
            ▼
   ┌───────────────────────┐
   │ QueryIngester (NEW)   │ ── sync write ──► stoolap embedded DB (B-tree)
   │                       │ ── sync write ──► Tantivy sidecar (FTS)
   │                       │ ── sync enqueue ─► EmbedderJob queue
   └───────────────────────┘
            │                     │
            ▼                     ▼
   ┌──────────────────────┐ ┌─────────────────┐
   │ Embedder worker      │ │ QueryService    │
   │ (local candle +      │ │ (NEW) — text +  │
   │  remote fallback)    │ │ semantic +      │
   └──────────────────────┘ │ hybrid + filter │
            │                └─────────────────┘
            ▼                        │
   embeddings table                  ▼
   (event_id, model_id,        IPC + MCP + CLI
    dims, vec BLOB)            handlers
```

**Boot sequence** (per daemon):

1. Open NDJSON (existing).
2. Open `stoolap::Database::open(persist_dir/"query.sql")` — file mode.
3. Run schema migration (idempotent `CREATE TABLE IF NOT EXISTS ...`).
4. Open `tantivy::Index::create_or_open(...)` at `persist_dir/"tantivy/"`.
5. **Rebuild phase**: scan NDJSON, replay into stoolap + tantivy. Skip if `last_rebuilt_id == buffer.largest_id()`.
6. Open `QueryService` handle exposed via `DaemonHandle` (mirrors `events_buffer()`).

**Rebuild policy**: `on_change` by default, `always` for tests, `never` for prod trust. Tantivy is 5–10× faster to rebuild than the full NDJSON replay path because we bypass the persister's atomic-write dance.

**Write-path idempotency**: replay uses `(event.id, kind)` as natural dedupe key — `INSERT OR IGNORE` makes a crash-mid-rebuild self-healing.

## Schema (stoolap)

Boot-time idempotent `CREATE`. All columns nullable except PKs and `kind`:

```sql
CREATE TABLE events (
    id           INTEGER PRIMARY KEY,
    ts_unix_ms   INTEGER  NOT NULL,
    ts_mono_ns   INTEGER  NOT NULL,
    kind         TEXT     NOT NULL,           -- 'message' | 'reaction' | 'receipt' | 'group_change' | 'presence' | 'call' | 'story' | 'connection' | 'unknown'
    variant      TEXT,
    peer         TEXT,
    sender       TEXT,
    chat_jid     TEXT,
    payload      TEXT     NOT NULL
);
CREATE INDEX idx_events_kind_ts  ON events(kind, ts_unix_ms);
CREATE INDEX idx_events_peer_ts  ON events(peer, ts_unix_ms);
CREATE INDEX idx_events_chat_ts  ON events(chat_jid, ts_unix_ms);

CREATE TABLE messages (
    event_id     INTEGER PRIMARY KEY,
    peer         TEXT     NOT NULL,
    sender       TEXT     NOT NULL,
    ts_unix_ms   INTEGER  NOT NULL,
    kind         TEXT     NOT NULL,           -- MessageKind enum as str
    text         TEXT     NOT NULL,           -- bounded 65 KB; longer bodies via messages.get
    media_token  TEXT,                        -- optional, populated for media-bearing messages
    from_me      INTEGER  NOT NULL,
    is_group     INTEGER  NOT NULL,
    FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
);
CREATE INDEX idx_messages_peer_ts   ON messages(peer, ts_unix_ms);
CREATE INDEX idx_messages_chat_ts   ON messages(chat_jid, ts_unix_ms);
CREATE INDEX idx_messages_ts        ON messages(ts_unix_ms);
CREATE INDEX idx_messages_kind_ts   ON messages(kind, ts_unix_ms);
CREATE INDEX idx_messages_sender    ON messages(sender, ts_unix_ms);

CREATE TABLE embeddings (
    event_id     INTEGER PRIMARY KEY,
    model_id     TEXT     NOT NULL,           -- 'all-minilm-l6-v2-q4' | 'openai-text-embedding-3-small@<ver>'
    dims         INTEGER  NOT NULL,
    provider     TEXT     NOT NULL,           -- 'local' | 'remote' | 'failed'
    vec          BLOB     NOT NULL,           -- raw f32 little-endian, dims * 4 bytes
    ts_embed_ms  INTEGER  NOT NULL,
    FOREIGN KEY (event_id) REFERENCES messages(event_id) ON DELETE CASCADE
);
CREATE INDEX idx_embeddings_model ON embeddings(model_id);
CREATE INDEX idx_embeddings_hnsw  ON embeddings(vec) USING hnsw
    WITH distance=cosine, m=16, ef_construction=100, ef_search=50;
```

**Derivation rules** at ingest (`QueryIngester::ingest(&InboundEvent)`):

- Always insert into `events` (one row per event).
- If `InboundEvent::Message`, also insert into `messages`.
- Enqueue embed job; on completion, UPSERT into `embeddings` by `event_id` (idempotent).
- Presence / Receipt / Connection: events row only, no embedding.

**Write path called from**: `events_persister::run_actor` already broadcasts via mpsc. The same broadcast feeds `QueryIngester`. **Zero new `await` on existing sender path**; ingester sits in its own tokio task.

## Tantivy sidecar

**One index**, schema:

```rust
Schema {
  msg_id:    i64   | indexed, stored           -- InboundEvent::Message.id hashed to i64
  event_id:  i64   | indexed, stored
  ts:        i64   | indexed, stored           -- range filter
  peer:      text  | indexed, stored
  chat_jid:  text  | indexed, stored
  sender:    text  | indexed, stored
  kind:      text  | indexed, stored           -- MessageKind
  text:      text  | indexed, stored           -- full body, simple() tokenizer
  from_me:   u64   | indexed, stored (0/1)
  is_group:  u64   | indexed, stored (0/1)
}
```

**Tokenizer**: `simple()` (lowercase only — language-agnostic, no English Porter bias). Configurable via `[query] fts_tokenizer = "default" | "simple"` for opt-in to English Porter stemming.

**Filters** applied at tantivy query time as `Occur::Must` terms: peer, chat_jid, sender, kind, from_me, is_group, ts range.

**Ranking**: BM25 with `text` weight=2.0, other fields weight=0.5.

**Operations**:

- `index_message(msg)` — debounced via 100ms micro-batch on a per-daemon writer (tantivy recommendation).
- `commit()` on shutdown + every 30s tick + every 500 docs.
- `delete_by_event_id(id)` rare, only on cascade.

**Schema migration**: tantivy auto-bumps via `IndexMeta`. We pin schema hash in `INDEX_VERSION`; if hash changes on boot, drop index and rebuild from NDJSON (one-time op, logged).

**Documented limitation**: messages with `text.len() > 65_000` get truncated in the tantivy index; the full body is stored in stoolap `messages.text`. FTS won't match the truncated tail — `messages.get` retrieves the full body via `QueryService::by_event_id`.

**Documented limitation**: media-only messages (kind=Image|Video|Audio|Voice|Sticker|Document with empty text) are indexed but produce no FTS matches. Users filter by `kind:image` for those. Semantic search still works because the embedding captures `kind` via synthetic text.

## Embedding layer

**Embedder trait** — one impl per source:

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &'static str;
    fn dims(&self) -> usize;
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
}
```

**Two impls** behind a `HybridEmbedder`:

- **`LocalCandleEmbedder`** (default): candle-core + candle-nn + candle-transformers + tokenizers + hf-hub. Model: `sentence-transformers/all-MiniLM-L6-v2` quantized Q4, 384 dims, ~25 MB. Pulled on first run into `~/.cache/octo/models/all-MiniLM-L6-v2-q4/`. Batches 16 at a time.
- **`RemoteEmbedder`** (opt-in via `[query.embed] provider = "remote"`): `reqwest` to OpenAI-compatible `POST {url}/v1/embeddings`. Config: `remote_url`, `remote_api_key_env` (env var name, not the key), `remote_model`.

**Behavior**: `HybridEmbedder::embed` tries `primary` first. On `EmbedError::Transient`, tries `fallback` if configured. Failure paths write to `embeddings` with `provider='failed'` + `ts_embed_ms` so callers can compute coverage via `messages.coverage`.

**Embedding job queue**:

- Single-consumer tokio task reading from `mpsc::Sender<EmbedJob>`.
- Batches up to 16 jobs or 50ms whichever first → one `embed(&[texts])` call.
- Drops oldest with logged counter if queue > 8192 (backpressure escape hatch).
- Replays from NDJSON on boot for messages that arrived before the daemon started (or after a crash).

**Single-flight** for remote: deduplicates identical query strings in-flight via a small in-mutex map; off-thread worker pools calls.

## Query service

**Single service**, three modes, one entry point:

```rust
pub enum SearchMode {
    Text,                // tantivy BM25 only
    Semantic,            // vector cosine only
    Hybrid { alpha: f32 }, // 0.0 = pure semantic, 1.0 = pure text, default 0.5
}

pub struct SearchQuery {
    pub q: String,
    pub mode: SearchMode,
    pub peer: Option<String>,
    pub chat_jid: Option<String>,
    pub sender: Option<String>,
    pub kind: Option<MessageKind>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub from_me: Option<bool>,
    pub is_group: Option<bool>,
    pub limit: usize,        // default 50, max 500
    pub offset: usize,       // default 0
}

pub struct SearchHit {
    pub event_id: i64,
    pub msg_id: String,
    pub peer: String,
    pub sender: String,
    pub ts_unix_ms: i64,
    pub kind: String,
    pub text: String,        // truncated to 200 bytes for snippet
    pub score: f32,          // normalized 0..1
    pub score_breakdown: ScoreBreakdown,
}

pub enum ScoreBreakdown {
    Text { bm25: f32 },
    Semantic { cosine: f32 },
    Hybrid { bm25: f32, cosine: f32, alpha: f32 },
}
```

**Query flow** (`QueryService::search(&SearchQuery)`):

```
mode=Text:
  tantivy.search(filter+query) → top-N event_ids
  SELECT * FROM messages WHERE event_id IN (...) ORDER BY ts DESC

mode=Semantic:
  embed(query) → vec
  SELECT event_id, vec FROM embeddings WHERE filters apply
  brute-force cosine top-K
  SELECT * FROM messages WHERE event_id IN (...)

mode=Hybrid:
  parallel {
    tantivy top-K_N (K_N=200)
    embed(query) + brute-force top-K_M (K_M=200)
  }
  RRF fusion: score(i) = Σ 1/(60 + rank_i) over both lists
  → take top-N by RRF
  → hydrate from messages
```

**RRF** chosen over alpha blending because:

- No need to normalize BM25 + cosine to the same scale (RRF is rank-based).
- Proven pattern (Elasticsearch hybrid retrievers, IR benchmarks).
- Default `k_const = 60` (Cormack et al. 2009).

**Filter pushdown**: tantivy + stoolap apply the same filter set. Otherwise hybrid fusion merges incompatible top-K spaces.

**Performance budget**:

- Tantivy top-200: 5–20ms typical.
- Embed query: 5–20ms (local) / 50–200ms (remote).
- Stoolap brute-force cosine top-200 over 100k embeddings: 30–50ms.
- Hydrate 200 rows: ~5ms.
- **Hybrid total: 50–100ms typical, 200ms p99.**

## IPC / MCP / CLI surface

**Strategy**: add new RPCs, keep existing as wrappers. Existing callers stay valid; new surface is the comprehensive layer.

**New IPC RPCs** (in `crates/octo-whatsapp/src/ipc/handlers/messages_query.rs`):

| RPC                        | Params                                                                                                | Returns                                                                                                                                                                                            |
| -------------------------- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `messages.search_text`     | `{query, peer?, chat_jid?, sender?, kind?, since?, until?, from_me?, is_group?, limit?, offset?}`     | `{hits, total, took_ms, mode:"text"}`                                                                                                                                                              |
| `messages.search_semantic` | same shape                                                                                            | `{hits, total, took_ms, mode:"semantic"}`                                                                                                                                                          |
| `messages.search_hybrid`   | same + `alpha?: f32`                                                                                  | `{hits, total, took_ms, mode:"hybrid"}`                                                                                                                                                            |
| `messages.filter`          | `{peer?, chat_jid?, sender?, kind?, since?, until?, from_me?, is_group?, limit?, offset?, order_by?}` | `{messages: MessageRow[], total, took_ms}`                                                                                                                                                         |
| `messages.recent`          | `{chat_jid?, peer?, limit?}`                                                                          | `{messages: MessageRow[]}`                                                                                                                                                                         |
| `messages.coverage`        | `{}`                                                                                                  | `{messages_total, embeddings_total, coverage_pct, by_model, by_provider, pending_estimate}`                                                                                                        |
| `query.rebuild`            | `{since_id?: u64}`                                                                                    | `{status, since_id, last_id, events_replayed}`                                                                                                                                                     |
| `query.stats`              | `{}`                                                                                                  | `{db_size_bytes, tantivy_size_bytes, ndjson_size_bytes, events_total, messages_total, embeddings_total, last_rebuild_at_unix_ms, fts_tokenizer, embedder_model, embedder_dims, embedder_provider}` |

**Existing RPCs** updated (no breaking wire change):

- **`messages.list`** (was stub) → delegates to `messages.filter(limit=50)`. Removes `phase: "phase2"`.
- **`messages.search`** (was broken chat-JID scan) → delegates to `messages.search_text`. New behavior is **strictly better** for existing callers.
- **`messages.get`** (was adapter probe + post-filter) → calls `QueryService::by_event_id(msg_id)` → `SELECT * FROM messages WHERE event_id = ?`. **Adapter `message_search` no longer probed.**
- **`adapter.message_search`** becomes unused. Either delete or route to `QueryService::search_text`. TBD in Phase 2.

**MCP tools** mirror IPC — new entries in `tool_descriptors()`.

**CLI** (mirror IPC):

```bash
octo-whatsapp messages search-text <query> [--peer X] [--chat X] [--since TS] [--until TS] [--kind image] [--from-me] [--group-only] [--limit N] [--offset N]
octo-whatsapp messages search-semantic <query> [same flags]
octo-whatsapp messages search-hybrid  <query> [--alpha 0.5] [same flags]
octo-whatsapp messages filter  [--peer X] [--chat X] [--sender X] [--kind X] [--since TS] [--until TS] [--from-me] [--group-only] [--limit N] [--offset N] [--order asc|desc]
octo-whatsapp messages recent [--chat X] [--peer X] [--limit N]
octo-whatsapp messages coverage
octo-whatsapp query rebuild [--since-id N]
octo-whatsapp query stats
```

**Skills** (`assets/skills/wa-mcp.md` + `wa-monitor.md`): add new tools to the tool catalog section. The fat `wa-mcp` skill needs a section "Searching your message history" describing the three modes.

## Failure modes

| Scenario                                       | Behavior                                                                                                           |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Tantivy index corrupt on boot                  | Auto-rebuild from NDJSON; log warn; expose `query.rebuild` for manual retry                                        |
| Stoolap DB corrupt on boot                     | Refuse to start; surface `Database::open` error with the path                                                      |
| Embedding queue overflow (>8192)               | Drop oldest, increment `embedder_dropped_total` counter; log warn every 100 drops; surface in `query.stats`        |
| Remote embedder timeout (>5s)                  | One retry, then fallback to local if configured, else mark `provider='failed'`                                     |
| Local embedder OOM                             | Catch `candle_core::Error::OutOfMemory`, drop batch, mark `provider='failed'`, log error                           |
| Tantivy schema change (`INDEX_VERSION` bumped) | Detect on boot, drop index, rebuild from NDJSON; one-time cost, logged                                             |
| Stoolap schema change                          | Idempotent `CREATE IF NOT EXISTS` for additive; breaking changes require operator-initiated migration (never auto) |
| NDJSON desync (event id collision after crash) | `INSERT OR IGNORE` makes replay safe                                                                               |
| Persister down, query layer up                 | Query layer keeps working on last-known state; new events lost until persister recovers                            |
| `events.tail` lagged=0                         | **Deferred to events_router Part B (separate workstream). Out of scope for this plan.**                            |

## Configuration (`~/.config/octo/whatsapp/query.toml`, optional)

```toml
[query]
enabled = true
fts_tokenizer = "simple"          # or "default"
rebuild_policy = "on_change"      # or "always" or "never"

[query.embed]
provider = "local"                # "local" | "remote"
remote_url = "https://api.openai.com"
remote_api_key_env = "OPENAI_API_KEY"
remote_model = "text-embedding-3-small"
batch_size = 16
queue_capacity = 8192
```

All fields optional — defaults match the v1 recommendation.

## Test plan

### Live tests (`--features live-whatsapp`)

| Test                                       | Asserts                                                                               |
| ------------------------------------------ | ------------------------------------------------------------------------------------- |
| `live_messages_search_text`                | send 3 messages, BM25 query returns all 3 with descending scores                      |
| `live_messages_search_semantic`            | send "hello world" → query "greetings" returns it via embedding similarity            |
| `live_messages_search_hybrid`              | RRF fusion returns both literal + semantic near-hits in correct order                 |
| `live_messages_filter_by_peer`             | `messages.filter{peer:A}` excludes messages from peer B                               |
| `live_messages_coverage`                   | after 5 sends, `coverage` shows `messages_total=5`, `embeddings_total >= 5` within 5s |
| `live_query_rebuild`                       | `query.rebuild` returns `status=started` then `noop` on second call                   |
| `live_query_stats`                         | returns sane sizes + counts after sustained traffic                                   |
| `live_messages_persistence_across_restart` | kill daemon, restart, query returns prior messages                                    |

### Hermetic tests (always-on, no live WA)

- Schema migration idempotent (run twice, no error)
- `simple()` tokenizer doesn't drop "andando"
- `INSERT OR IGNORE` on duplicate `event_id` (replay-safe)
- FTS + semantic + hybrid all return correct results on a seeded mini-corpus (10 docs)
- `ScoreBreakdown` populated correctly per mode
- Filter pushdown: tantivy + stoolap return identical `event_id` sets given identical filters
- `messages.coverage` math (totals + by-model + by-provider sums match)
- Embedding queue overflow → oldest-dropped, counter incremented
- RRF fusion math on synthetic ranked lists

### Hermetic CLI/MCP tests

- Each new RPC has a hermetic test (mocked `QueryService`).
- CLI argument parsing for all flags (`--alpha`, `--order`, `--until`).
- MCP `tools/list` includes all new tools with correct schema.

## Implementation phases

### Phase 0 — foundation (1 session, ~6 commits)

1. `feat(octo-whatsapp): add stoolap + tantivy + candle deps` — Cargo.toml + `query` cargo feature (default off in v1).
2. `feat(octo-whatsapp): QueryIngester + boot-time schema migration` — idempotent `CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`, `INSERT OR IGNORE`. Hermetic: replay same NDJSON twice → no errors.
3. `feat(octo-whatsapp): HybridEmbedder + LocalCandleEmbedder` — model download, batched embed. Hermetic: embed "hello world" twice, vectors equal (deterministic).
4. `feat(octo-whatsapp): EmbedderJob queue + worker` — bounded mpsc, batch coalescing, drop counter. Hermetic: overflow test.
5. `feat(octo-whatsapp): Tantivy sidecar + Simple tokenizer` — schema, `index_message`, `delete_by_event_id`, `INDEX_VERSION` stamp. Hermetic: index 5 docs, search "foo" returns them.
6. `feat(octo-whatsapp): wire QueryIngester into persister broadcast` — zero-`await`-on-sender-path, queue + worker. Hermetic: enqueue event, both stores updated.

### Phase 1 — search service (1 session, ~5 commits)

7. `feat(octo-whatsapp): SearchQuery + SearchHit + ScoreBreakdown types`.
8. `feat(octo-whatsapp): QueryService::search_text` — tantivy + stoolap hydrate.
9. `feat(octo-whatsapp): QueryService::search_semantic` — embed + brute-force cosine.
10. `feat(octo-whatsapp): QueryService::search_hybrid` — RRF fusion.
11. `feat(octo-whatsapp): messages.filter + messages.recent` — SQL-only short-circuits.

### Phase 2 — IPC + MCP + CLI (1 session, ~5 commits)

12. `feat(octo-whatsapp): 8 new IPC handlers + messages.get rewritten to QueryService::by_event_id`.
13. `feat(octo-whatsapp): MCP tool_descriptors entries + handlers`.
14. `feat(octo-whatsapp): CLI subcommands + flag parsing`.
15. `refactor(octo-whatsapp): messages.list + messages.search delegate to new RPCs; delete or route adapter.message_search`.
16. `docs(octo-whatsapp): wa-mcp.md + wa-monitor.md tool catalog updates`.

### Phase 3 — live tests (1 session, ~3 commits)

17. `test(octo-whatsapp): 8 live tests for new query surface`.
18. `test(octo-whatsapp): live_messages_persistence_across_restart`.
19. `chore(octo-whatsapp): integration suite green + clippy + fmt`.

### Phase 4 — ops + observability (1 session, ~3 commits)

20. `feat(octo-whatsapp): query.toml config + env overrides`.
21. `feat(octo-whatsapp): drop-counter + coverage metrics in query.stats`.
22. `docs(octo-whatsapp): ops runbook for the query layer`.

**Total: 22 commits, 5 sessions, ~12–15 hours.** Each session ends with `Ready for feedback`.

## Reuse — what already works

- `events_persister.rs` NDJSON canonical path — unchanged, proven (843 lib tests pass).
- `EventsBuffer::list` / `list_recent` / `get` / `hydrate_from_entries` — events.* read surface continues to use these.
- `EventsPersisterHandle::ingress()` + `tx_clone()` — broadcast seam for `QueryIngester` to subscribe without modifying the sender.
- `inter_call_delay_for(method)` — extend registry for new RPCs.
- `LiveFixture` + `RpcStream::call_unchecked()` + `events_query::wait_for` — live test template already supports the assertion shape we need.
- `wait_for_with` — useful for waiting on coverage counters to catch up.
- `Cargo.toml` — `octo-whatsapp` already lists the workspace's `stoolap` git dep (currently unused beyond `octo-sync`).

## Critical files

| File                                                            | Why                                                                                         |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `crates/octo-whatsapp/Cargo.toml`                               | Add `query` feature, candle/tantivy/stoolap deps                                            |
| `crates/octo-whatsapp/src/lib.rs`                               | Export new modules                                                                          |
| `crates/octo-whatsapp/src/query/` (NEW)                         | `ingester.rs`, `embedder.rs`, `tantivy_index.rs`, `service.rs`, `schema.rs`, `bootstrap.rs` |
| `crates/octo-whatsapp/src/daemon.rs`                            | Boot sequence: open stores, hydrate, start workers                                          |
| `crates/octo-whatsapp/src/ipc/handlers/messages_query.rs` (NEW) | 8 new handlers                                                                              |
| `crates/octo-whatsapp/src/ipc/handlers/messages_get.rs`         | Rewrite to `QueryService::by_event_id`                                                      |
| `crates/octo-whatsapp/src/ipc/handlers/messages_list.rs`        | Delegate to `messages.filter`                                                               |
| `crates/octo-whatsapp/src/ipc/handlers/messages_search.rs`      | Delegate to `messages.search_text`                                                          |
| `crates/octo-whatsapp/src/mcp_server.rs`                        | New tool entries                                                                            |
| `crates/octo-whatsapp/src/cli.rs`                               | New subcommands                                                                             |
| `crates/octo-whatsapp/src/events_persister.rs`                  | Add broadcast subscriber for `QueryIngester`                                                |
| `crates/octo-adapter-whatsapp/src/inherent.rs`                  | Update or delete `message_search` (TBD in Phase 2)                                          |
| `crates/octo-whatsapp/tests/live_daemon_test.rs`                | 8 new live tests                                                                            |
| `crates/octo-whatsapp/tests/it_daemon_chain.rs`                 | Update chain tests if `messages.search` semantics change                                    |
| `crates/octo-whatsapp/assets/skills/wa-mcp.md`                  | Add "Searching your message history" section                                                |
| `crates/octo-whatsapp/assets/skills/wa-monitor.md`              | Add new tools to catalog                                                                    |
| `docs/distribution.md`                                          | Note new config + storage path                                                              |

## Verification end-to-end

- `cargo test -p octo-whatsapp --lib` — all unit + hermetic tests green (~860+).
- `cargo test -p octo-whatsapp --features live-whatsapp,query -- --include-ignored --test-threads=1` — full live suite including 8 new query tests.
- `cargo clippy -p octo-whatsapp --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- Manual: `cargo run -p octo-whatsapp -- query stats` — shows live counts.
- Manual: send 10 messages in WA, run `cargo run -p octo-whatsapp -- messages search-text "any"` — returns hits.
- Manual: kill daemon, restart, re-run the same query — same hits (persistence proven).

## Local-only / no push

Per user 2026-07-05, no `git push`, no PR. All commits land on `feat/whatsapp-runtime-cli-mcp` locally. Push only on explicit request.
