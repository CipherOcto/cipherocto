# Wacore Events → First-Class: Comprehensive Overhaul

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** Every wacore `Event` variant wacore itself implements becomes a first-class typed `InboundEvent` variant. `Unknown` is reserved strictly for events we **do not yet have a typed handler for** — it is **gracefully handled, persisted with full payload, metric-counted, and exposed via RPC for future analysis**. Adding a new wacore variant does **not** break compilation; it surfaces as a new `Unknown` variant in stats and analysis tooling, letting maintainers prioritize which typed handlers to add next.

**Non-goals:**
- **No backward compatibility.** Old NDJSON files are unreadable. Old SQL schema is dropped. Existing `InboundEvent` enum shape is replaced wholesale. Old RPC field names change. Existing CLI output changes. **Every old artefact is invalidated.** Operators with existing event stores do `rm -rf ~/.local/share/octo/whatsapp/events.ndjson` (or equivalent) and start fresh. Documented in plan §9.
- **No incremental rollout.** This plan either lands in one shot or doesn't. No two-stage migration, no version bump from v1→v2.
- **No new RPC endpoints beyond what already exists + the new `events.list_kinds` and `events.unknown_stats` for first-class discoverability.** Existing `events.search` / `events.find` / `messages.search` adapt to the new shapes transparently.

**Architecture (post-overhaul):**

```
wacore::types::events::Event (57 variants, all public struct types with serde derive)
  ↓
crate::events::InboundEvent  ← type alias / 1:1 wrapper, no translation
  ↓ Arc<InboundEvent> over broadcast::channel
  ↓
events_router (kind labels for Prometheus)
  ↓ Vec<(id, Arc<InboundEvent>)>
  ↓
events_persister (NDJSON → serde_json of InboundEvent → file append)
  │ parallel sidecar: unknown_stats (per-variant count, first_seen, last_seen)
  ↓
events_buffer (in-memory ring; Arc-cloned for fast list operations)
  ↓
query subsystem (stoolap events table + messages table + tantivy index)
  ↓
RPC layer (existing events.* / messages.* methods expose new kinds + unknown_stats)
```

**Key design decisions:**
1. **`InboundEvent` becomes an `enum` with ~56 variants.** Each wacore-implemented event maps to a typed variant carrying `Arc<wacore::types::events::…>` directly (no serde_json::Value blob, no Debug strings). Variants are projected where the wacore struct semantics align with our consumer semantics (Presence, Connection, Call, Message batch → individual Messages, Receipt, Unavailable, DisappearingModeChanged). Otherwise the variant carries the wacore struct directly.
2. **No Debug-string parsing.** `raw_event_tx: broadcast::Sender<Arc<InboundEvent>>` replaces `broadcast::Sender<String>`. The adapter match produces a typed `Arc<InboundEvent>` directly. The `events.rs::InboundEvent::parse()` function, the `field()` helper, and all `parse_*` functions are **deleted**, not deprecated.
3. **`Unknown` is graceful and first-class observability.** The catch-all arm in adapter.rs is `unknown_event => { emit InboundEvent::Unknown(arc); }` — **not** `unreachable!()`. Each Unknown emission:
   - Increments a Prometheus counter `unknown_event_total{wacore_variant}` (so operators see exactly which wacore variants lack typed handlers).
   - Persists to NDJSON (full event + variant label).
   - Updates a per-variant aggregate stat in a sidecar file `unknown_stats.ndjson` (count, first_seen_ms, last_seen_ms, sample_payload).
   - Logs at `debug` level (cheap, not noisy).
   - Exposed via `events.unknown_stats` RPC for analysis.
   Adding a new wacore variant surfaces in `unknown_stats` immediately — no compile failure, no maintenance burden, fully observable for prioritization.
4. **Persister writes typed JSON.** NDJSON file format: one JSON object per line, `{"id": <u64>, "event": <typed InboundEvent JSON>, "ts_mono_ns": <u64>}`. Schema-versioned (single version, no upgrade path).
5. **Query subsystem ingests typed JSON.** Stoolap `events` table: `(id INTEGER PK, ts_unix_ms INTEGER, ts_mono_ns INTEGER, kind TEXT, variant TEXT, peer TEXT, sender TEXT, chat_jid TEXT, payload JSON)`. Ingester parses each event once to populate kind/variant/peer/sender/chat_jid + full payload.
6. **Tantivy indexes `kind` + `variant` + `payload`** (full JSON → searchable structured fields). Supports `events.search kind:profile_update variant:picture_set` style queries.
7. **Prometheus gets new label values.** `inbound_events_total{kind="…",variant="…"}` for all typed variants. `unknown_event_total{wacore_variant="…"}` for the catch-all observability surface. Existing dashboards adapt by widening label cardinality.

**Coverage matrix (target state):**

| wacore variant | InboundEvent | Notes |
|---|---|---|
| `Messages(MessageBatch)` | `Message { /* typed */ }` per InboundMessage in batch (existing projection) | batch expansion already happens in adapter today |
| `Receipt { … }` | `Receipt { /* typed */ }` | existing projection |
| `ChatPresenceUpdate` | `Presence { kind: Typing/Recording/Paused }` | existing projection |
| `PresenceUpdate` | `Presence { kind: Available/Unavailable, last_seen? }` | existing projection |
| `UndecryptableMessage` | `Unavailable { unavailable_type, is_unavailable }` | existing projection |
| `DisappearingModeChanged` | `DisappearingModeChanged { jid, duration_seconds, ts }` | existing projection |
| `NewsletterLiveUpdate` | `NewsletterUpdate { jid, kind }` | existing projection |
| `Connected` | `Connected` (unit) | simple |
| `LoggedOut { cause, on_connect }` | `LoggedOut { cause: Option<DisconnectCause>, on_connect }` | direct |
| `PairingQrCode { … }` | `PairingQrCode { qr_code, ref string, timeout }` | existing projection |
| `PairingCode { … }` | `PairingCode { code, timeout }` | existing projection |
| `PairPasskeyRequest / Confirmation / Error` | existing typed variants | existing projection |
| `HistorySync(LazyHistorySync)` | internal-only — drives `Messages` fetches | no InboundEvent; consumed by adapter, not emitted |
| `OfflineSyncCompleted` | internal-only — fires `Connected` | no InboundEvent; consumed by adapter |
| `StreamError` | internal log only — no InboundEvent | no InboundEvent; `tracing::error!` |
| **`GroupUpdate { … }`** | **`GroupUpdate(Arc<wacore::types::events::GroupUpdate>)`** | new — direct |
| **`IncomingCall`** | **`IncomingCall(Arc<wacore::IncomingCall>)`** | new — direct |
| **`MissedCall`** | **`MissedCall(Arc<wacore::MissedCall>)`** | new — direct |
| **`CallEndedElsewhere`** | **`CallEndedElsewhere(Arc<wacore::CallEndedElsewhere>)`** | new — direct |
| **`Disconnected`** | **`Disconnected` (unit)** | new — direct |
| **`StreamReplaced`** | **`StreamReplaced` (unit)** | new — direct |
| **`TemporaryBan { reason, expires_at }`** | **`TemporaryBan(Arc<wacore::TemporaryBan>)`** | new — direct |
| **`ConnectFailure { reason, retry_after }`** | **`ConnectFailure(Arc<wacore::ConnectFailure>)`** | new — direct |
| **`PictureUpdate { jid, author, timestamp, removed, picture_id }`** | **`PictureUpdate(Arc<wacore::PictureUpdate>)`** | new — direct |
| **`UserAboutUpdate { jid, status, timestamp }`** | **`UserAboutUpdate(Arc<wacore::UserAboutUpdate>)`** | new — direct |
| **`ContactUpdated { … }`** | **`ContactUpdated(Arc<wacore::ContactUpdated>)`** | new — direct |
| **`ContactNumberChanged { old, new, … }`** | **`ContactNumberChanged(Arc<wacore::ContactNumberChanged>)`** | new — direct |
| **`ContactSyncRequested { type_ }`** | **`ContactSyncRequested(Arc<wacore::ContactSyncRequested>)`** | new — direct |
| **`ContactUpdate { jid, timestamp, action, from_full_sync }`** | **`ContactUpdate(Arc<wacore::ContactUpdate>)`** | new — direct |
| **`PushNameUpdate { jid, message, old_push_name, new_push_name }`** | **`PushNameUpdate(Arc<wacore::PushNameUpdate>)`** | new — direct |
| **`SelfPushNameUpdated { new_push_name, timestamp }`** | **`SelfPushNameUpdated(Arc<wacore::SelfPushNameUpdated>)`** | new — direct |
| **`PinUpdate { jid, pinned }`** | **`PinUpdate(Arc<wacore::PinUpdate>)`** | new — direct |
| **`MuteUpdate { jid, muted, mute_expires_at? }`** | **`MuteUpdate(Arc<wacore::MuteUpdate>)`** | new — direct |
| **`ArchiveUpdate { jid, archived }`** | **`ArchiveUpdate(Arc<wacore::ArchiveUpdate>)`** | new — direct |
| **`StarUpdate { jid, msg_id, pinned, starred }`** | **`StarUpdate(Arc<wacore::StarUpdate>)`** | new — direct |
| **`MarkChatAsReadUpdate { jid, read_until_msg_id?, unread_count }`** | **`MarkChatAsReadUpdate(Arc<wacore::MarkChatAsReadUpdate>)`** | new — direct |
| **`DeleteChatUpdate { jid }`** | **`DeleteChatUpdate(Arc<wacore::DeleteChatUpdate>)`** | new — direct |
| **`ClearChatUpdate { jid, msg_count? }`** | **`ClearChatUpdate(Arc<wacore::ClearChatUpdate>)`** | new — direct |
| **`UserStatusMuteUpdate { jid, muted }`** | **`UserStatusMuteUpdate(Arc<wacore::UserStatusMuteUpdate>)`** | new — direct |
| **`DeleteMessageForMeUpdate { jid, msg_id, only_me }`** | **`DeleteMessageForMeUpdate(Arc<wacore::DeleteMessageForMeUpdate>)`** | new — direct |
| **`ServerAck { ack_type, msg_id, … }`** | **`ServerAck(Arc<wacore::ServerAck>)`** | new — direct |
| **`DeviceListUpdate { user, device_list }`** | **`DeviceListUpdate(Arc<wacore::DeviceListUpdate>)`** | new — direct |
| **`IdentityChange { timestamp, … }`** | **`IdentityChange(Arc<wacore::IdentityChange>)`** | new — direct |
| **`LabelEditUpdate { label_id, name, color, deleted }`** | **`LabelEditUpdate(Arc<wacore::LabelEditUpdate>)`** | new — direct |
| **`LabelAssociationUpdate { label_id, chat_jid, labeled }`** | **`LabelAssociationUpdate(Arc<wacore::LabelAssociationUpdate>)`** | new — direct |
| **`PairSuccess { device_id, business_name, platform }`** | **`PairSuccess(Arc<wacore::PairSuccess>)`** | new — direct |
| **`PairError { code, message }`** | **`PairError(Arc<wacore::PairError>)`** | new — direct |
| **`PairingCodeRefresh { code, timeout }`** | **`PairingCodeRefresh(Arc<wacore::PairingCodeRefresh>)`** | new — direct |
| **`QrScannedWithoutMultidevice`** | **`QrScannedWithoutMultidevice` (unit)** | new — direct |
| **`ClientOutdated`** | **`ClientOutdated` (unit)** | new — direct |
| **`BusinessStatusUpdate { jid, status }`** | **`BusinessStatusUpdate(Arc<wacore::BusinessStatusUpdate>)`** | new — direct |
| **`MexNotification { … }`** | **`MexNotification(Arc<wacore::MexNotification>)`** | new — direct |
| **`OfflineSyncPreview { total, received, app_data_synced, peer_count }`** | **`OfflineSyncPreview(Arc<wacore::OfflineSyncPreview>)`** | new — direct |
| **Any other wacore variant** | **`Unknown { wacore_event: Arc<wacore::Event>, variant_label: String, ts_unix_ms, ts_mono_ns }`** — graceful observability surface, never a compile error | future wacore variants surface here |
| **`Notification(OwnedNodeRef)`** (explicit) | **`Unknown { … }`** — wacore doesn't parse raw nodes | known case, explicit match arm emits this |
| **`RawNode(OwnedNodeRef)`** (explicit) | **`Unknown { … }`** — wacore doesn't parse raw nodes | known case, explicit match arm emits this |

**Total: 56 InboundEvent variants. Unknown is reachable from 3 wacore sources:**
1. Explicit `Notification` / `RawNode` arms (wacore-unparsed raw XML nodes).
2. The graceful catch-all for any wacore variant not yet handled — if wacore adds a 58th variant, the daemon compiles, the event persists, the metric ticks, the operator sees it in `events.unknown_stats`.

---

## Investigation evidence

| Source | Finding |
|---|---|
| `wacore@b637129/src/types/events.rs:628-770` | `pub enum Event` with 57 variants. All inner types are `pub struct` with `pub` fields + `#[derive(Debug, Clone, Serialize, bon::Builder)]`. |
| `wacore@b637129/src/types/events.rs:1095-1170` | `OwnedNodeRef` — raw XML node wrapper for `Notification` / `RawNode`. |
| `crates/octo-adapter-whatsapp/src/adapter.rs:1146-1737` | Adapter match — 17 typed arms, 5 early-return presence bridges, catch-all `_ => {}` at 1737. `raw_event_tx: tokio::sync::broadcast::Sender<String>` at line 310. |
| `crates/octo-adapter-whatsapp/src/adapter.rs:444` | `raw_event_tx: broadcast::channel::<String>(1000).0` — channel construction. |
| `crates/octo-adapter-whatsapp/src/lib.rs:285,317,332` | PairPasskey variants synthesised in `OctoWhatsAppAdapter::passkey_*` tests — independent of main match. |
| `crates/octo-whatsapp/src/events.rs:46-170` | `pub enum InboundEvent` — 14 variants, tagged via `#[serde(tag = "event", rename_all = "snake_case")]`. |
| `crates/octo-whatsapp/src/events.rs:550-595` | `InboundEvent::parse()` — Debug-string prefix matcher. **Delete in this overhaul.** |
| `crates/octo-whatsapp/src/events.rs:613-720` | `field()` helper — `format!("{:?}", ...)` extractor. **Delete.** |
| `crates/octo-whatsapp/src/events.rs:759-1940` | All `parse_*` functions (12 of them). **Delete.** |
| `crates/octo-whatsapp/src/events.rs` (other) | Sub-enums: `GroupChangeKind`, `ConnectionKind`, `CallKind` — **delete** because the variants those sub-enums decorated (`GroupChange`, `Connection`, `Call`) get replaced with direct wacore types. Other sub-enums (MessageKind, PresenceKind, ReceiptKind, LoggedOutCause, etc.) stay where they decorate preserved variants. |
| `crates/octo-whatsapp/src/events_persister.rs:733` | `write_event(file, id, ev)` — currently serialises via serde_json. After: same path, `ev: &InboundEvent` (no parse round-trip). |
| `crates/octo-whatsapp/src/events_persister.rs:440` | `parse` function (reads NDJSON line → `(id, InboundEvent)`). After: `InboundEvent::deserialize` direct via serde_json (drops Debug-string prefix stripping). |
| `crates/octo-whatsapp/src/events_router.rs:350-364` | `event_kind_label(ev: &InboundEvent)` — 13 arms. After: ≥56 arms. |
| `crates/octo-whatsapp/src/events_router.rs:414-440` | Other match-arms on `InboundEvent` — audit each, update to new variant shape. |
| `crates/octo-whatsapp/src/events_buffer.rs:50-300` | `EventsBuffer` — generic enum-agnostic ring; works as-is for new InboundEvent. InboundEvent::Unknown fixtures in tests get replaced with `Unknown { wacore_event: ..., variant_label: ... }`. |
| `crates/octo-whatsapp/src/query/schema.rs:24-42` | `events(kind TEXT, variant TEXT, payload TEXT)` — `payload TEXT` is currently the serialised JSON of the typed InboundEvent's projected fields. After: `payload JSON` holds the full wacore JSON. |
| `crates/octo-whatsapp/src/query/ingester.rs` | Ingests InboundEvent variants into SQL. After: ingest shapes for all new variants. |
| `crates/octo-whatsapp/src/query/service.rs` | `find` / `search` / `context` / `recent` / `semantic_search` — operate on existing SQL schema. After: same shapes, wider kind support. |
| `crates/octo-whatsapp/src/query/tantivy_sidecar.rs` | BM25 over `payload TEXT`. After: BM25 over `payload JSON`, with structured field indexing (kind, variant, jid, etc.). |
| `crates/octo-whatsapp/src/query/embedder_job.rs` | Hash-projection embedder over `payload` text. After: same, now over JSON. |
| `crates/octo-whatsapp/src/daemon.rs:471-475` | Persister subscribes to events_router. After: subscription still works (no Debug-string dependency). |
| `crates/octo-whatsapp/src/ipc/handlers/*.rs` | RPC handlers that match on InboundEvent variants — audit each, update for new variants. |
| `crates/octo-whatsapp/src/cli.rs` | CLI subcommands that match on InboundEvent variants — audit + update. |
| `crates/octo-whatsapp/src/mcp_server.rs` | MCP tool descriptors that filter on `kind` — catalogue values get updated. |
| `crates/octo-whatsapp/assets/skills/wa-mcp.md` | Skill catalog — tables get updated with new kinds/variants. |
| `crates/octo-whatsapp/Cargo.toml` | Add `wacore` dependency (already in `octo-adapter-whatsapp`; need it exposed for `InboundEvent` to carry wacore types). |
| `crates/octo-adapter-whatsapp/Cargo.toml` | Already has `wacore` dep — no change. |
| All test files referencing `InboundEvent::Unknown { raw, ... }` | Search-and-replace fixtures. |
| `assets/skills/wa-mcp.md` | Skill catalog — tables updated. |
| `docs/MEMORY.md` | Add `whatsapp-events-first-class-overhaul.md` index entry. |

### Compatibility impact (no migration path)

| Artefact | Action |
|---|---|
| Existing NDJSON files at `~/.local/share/octo/whatsapp/events*.ndjson` | **Invalidate.** Operators `rm` before upgrade. Documented in `CHANGELOG.md`. |
| Existing `unknown_stats.ndjson` sidecar | **Invalidate.** Rebuilt from scratch on next session. |
| Stoolap `events`, `messages`, `unavailable_messages`, `disappearing_mode_changes`, `unknown_stats` tables in `~/.local/share/octo/whatsapp/query/` | **Invalidate.** Drop tables on boot if schema version mismatch. No migration. |
| Tantivy index `~/.local/share/octo/whatsapp/query/index/` | **Invalidate.** Re-build on boot. |
| Prometheus series for `inbound_events_total{kind="message"}` | Adapt — values for `kind="unknown"` may now come from wacore-raw or catch-all events. New series `unknown_event_total{wacore_variant}` introduced. |
| Any external tool that consumes JSON `payload` shape of NDJSON | **Will break.** Document in CHANGELOG. |
| Operators with running WA session | Must `rm` query NDJSON + SQL tables + Tantivy, restart daemon. |

### UnknownStats (sidecar persistence)

```rust
// crates/octo-whatsapp/src/events_persister.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownStats {
    /// Discriminant label extracted from the wacore event (e.g. "GroupUpdate",
    /// "PictureUpdate", or for a future wacore variant "FooBarUpdate").
    pub wacore_variant: String,
    /// Total emissions observed since stats file creation.
    pub count: u64,
    /// First wall-clock timestamp this variant was seen (ms).
    pub first_seen_ms: i64,
    /// Most recent wall-clock timestamp this variant was seen (ms).
    pub last_seen_ms: i64,
    /// Capped-to-2KB Debug sample of the most recent emission, for inspection.
    pub last_sample: String,
}
```

Persister maintains `BTreeMap<String, UnknownStats>` (key = `wacore_variant`). Updates on every Unknown emission. Persists to `~/.local/share/octo/whatsapp/unknown_stats.ndjson` on every update (cheap — small file, append-only). Loads on startup.

`events.unknown_stats` RPC returns `Vec<UnknownStats>` sorted by `count desc`. CLI/MCP expose this for operator triage.

### Failure modes (and cost)

| Failure | Impact | Mitigation |
|---|---|---|
| Wacore rename of a struct field | `cargo build` breaks for the one direct wrapper | Update one `Arc<wacore::T>` site; test suite catches |
| Wacore adds a new `Event` variant | **No compile error.** Variant surfaces as `Unknown` in `unknown_stats` next session | Operator sees it in `events.unknown_stats`; prioritize adding a typed arm later |
| Wacore changes `OwnedNodeRef` shape | Explicit `Notification`/`RawNode` arms fail to compile | One-line fix |
| Unknown payload grows beyond 2KB | `last_sample` truncated; `UnknownStats` file size bounded | Reasonable truncation strategy: first 2KB + "[truncated]" |
| Persister crash mid-unknown-stats write | Atomic rename on rewrite; load on boot reads last good file | Standard fsync pattern |

---

## Multi-phase shape

This is one comprehensive plan with four phases. Each phase is a coherent commit-and-testable unit. Inter-phase dependencies: each phase requires the previous to be landed (and tests passing) first.

### Phase 1 — InboundEvent type redesign + adapter typed emission + Unknown handling

**Scope**: rewrite `events.rs::InboundEvent` and adapter match arms. Drop Debug-string parser. Drop format!() bridges. Establish Unknown as graceful observability surface.

**Tasks (16 commits):**

| Task | Description | Risk |
|---|---|---|
| T01 | Add `wacore` dep to `octo-whatsapp/Cargo.toml` (re-export public types only). | Low |
| T02 | Rewrite `events.rs::InboundEvent` — delete `parse()`, `field()`, all `parse_*` functions. Delete `GroupChangeKind`, `ConnectionKind`, `CallKind` sub-enums (moved to no-op). Keep `Message`, `Receipt`, `Presence`, `Unavailable`, `DisappearingModeChanged`, `NewsletterUpdate`, `Story`, `CommunityUpdate`, `Reaction`, `PairingQrCode`, `PairingCode` typed variants (current shape). Add new direct-typed variants per the matrix above. | Med |
| T03 | Add `wacore::OwnedNodeRef` re-export to `events.rs` (only needed public surface). | Low |
| T04 | Rewrite `adapter.rs` `raw_event_tx` channel type: `Sender<Arc<InboundEvent>>` instead of `Sender<String>`. | Med |
| T05 | Delete all `format!("{:?}", event)` calls + the 5 presence bridges (1146-1245). Replace with one unified match that produces `Arc<InboundEvent>` per wacore variant. | Med |
| T06 | Add all 44 typed arms for wacore variants not already handled: GroupUpdate, 3 Call variants, 4 Connection variants, 30 new direct-typed variants. | Med |
| T07 | Replace `_ => {}` at 1737 with graceful Unknown emission: `_ => { let label = discriminant_label(&event); metrics::counter!("unknown_event_total", "wacore_variant" => label.clone()).increment(1); tracing::debug!(?event, "emitted InboundEvent::Unknown for analysis"); InboundEvent::Unknown { wacore_event: Arc::new(event.clone()), variant_label: label, ts_unix_ms, ts_mono_ns } }`. **Add `discriminant_label` helper** that returns the enum variant name as a string (e.g. "GroupUpdate", "PictureUpdate", "FutureVariantName"). | Trivial |
| T08 | Add explicit `Event::Notification(arc) | Event::RawNode(arc)` arms that emit `InboundEvent::Unknown { wacore_event: Arc::new(event), variant_label: "Notification" or "RawNode", ... }`. Same metric+log+persistence path as the catch-all. | Low |
| T09 | Drop `tracing::warn!` from the catch-all (was for visibility — now handled by metric + persisted stats). | Trivial |
| T10 | Adapter match arm for `Notification(Arc<OwnedNodeRef>)` and `RawNode(Arc<OwnedNodeRef>)` → emits `InboundEvent::Unknown { wacore_event, variant_label, ... }` per T08. | Low |
| T11 | Update all adapter test fixtures that emit `Event::Variant(...)` through the broadcast — they now receive `Arc<InboundEvent>` instead of `String`. | Low |
| T12 | Replace `InboundEvent` construction sites in daemon.rs + adapter.rs (mostly `raw_event_tx.send(...)` calls — adapt to `Arc<InboundEvent>`). | Med |
| T13 | Update `events_persister.rs::write_event` — same serde_json serialise path, but `ev: &InboundEvent` (no Debug). **Delete** `parse` function — `InboundEvent::deserialize` only. | Med |
| T14 | Update `events_router.rs::event_kind_label` — add 46 new arms (one per new InboundEvent variant). `kind` string format: `inbound_events_total{kind="profile_update",variant="picture_set"}` for wacore types; existing kinds get variant label where applicable. | Low |
| T15 | Update `events_router.rs` other match-on-InboundEvent sites (~5 match arms in `events_router.rs:354-414`) — adapt to new variant shape (call-site audit + edit per arm). | Low |
| T16 | All tests passing: `cargo test --lib --features query`. **Hermetic only, no live tests** for Phase 1. | High |

**Phase 1 success criteria:**
- `cargo build` clean.
- `cargo test --lib --features query` passes.
- `cargo clippy --lib --all-features -- -D warnings` clean.
- No `format!("{:?}", event)` calls remain in adapter.rs (verify via `grep`).
- No `InboundEvent::parse(` calls remain in the codebase (verify via `grep`).
- Catch-all arm emits `InboundEvent::Unknown` with metric + log + per-variant label. Verified by a hermetic test that synthesises a synthetic unknown event and asserts all three effects.

### Phase 2 — Persistence + UnknownStats sidecar + query subsystem rewrite

**Scope**: drop old NDJSON file format, drop old SQL schema, drop old Tantivy indexing rules, write new versions. Add `UnknownStats` sidecar persistence.

**Tasks (14 commits):**

| Task | Description | Risk |
|---|---|---|
| T17 | Add `UnknownStats` struct to `events_persister.rs` (shape as above). | Low |
| T18 | Add `unknown_stats.ndjson` sidecar loader/saver: `load_unknown_stats(path) -> BTreeMap<String, UnknownStats>` and `save_unknown_stats(path, &map)`. Atomic rename on rewrite. | Low |
| T19 | `events_persister.rs::Persister`: add `unknown_stats: Arc<Mutex<BTreeMap<String, UnknownStats>>>` field. Load on startup. Update on every Unknown emission (acquire lock, increment count, set last_seen_ms, append last_sample). | Med |
| T20 | `events_persister.rs::Persister::push`: if `InboundEvent::Unknown { wacore_event, variant_label, ... }`, route to both NDJSON persistence AND unknown_stats sidecar. Add an opt-out flag (default on) for power users who don't want the sidecar. | Med |
| T21 | `events_persister.rs`: rename `PersistenceFormat` from `"events.ndjson"` to `"events-v2.ndjson"`. New file path. Operators with v1 files get a fresh empty v2 file on boot. Log warning if v1 file still on disk. | Low |
| T22 | `events_persister.rs::write_event`: serialise `(id, &Arc<InboundEvent>, ts_mono_ns)` to JSON. New struct shape: `{id, event: <InboundEvent JSON>, ts_mono_ns}`. `InboundEvent` serialises via serde with the new enum's 56-variant derive. | Med |
| T23 | `query/schema.rs`: bump `SCHEMA_VERSION` to `2`. New `events` table layout: `events(id INTEGER PK, ts_unix_ms INTEGER, ts_mono_ns INTEGER, kind TEXT NOT NULL, variant TEXT, peer TEXT, sender TEXT, chat_jid TEXT, payload JSON NOT NULL)`. New `payload` is JSON column (stoolap supports JSON). | Med |
| T24 | `query/schema.rs::migrate`: drop old tables on mismatch (`DROP TABLE IF EXISTS events_v1; DROP TABLE IF EXISTS messages_v1; DROP TABLE IF EXISTS unavailable_messages_v1; DROP TABLE IF EXISTS disappearing_mode_changes_v1; DROP TABLE IF EXISTS query_meta_v1`). Rebuild fresh. No data preservation. | Low |
| T25 | `query/schema.rs::migrate`: create new tables with new layout. Drop old indexes. Add new indexes for `kind`/`variant`/`peer`/`ts_unix_ms`/`chat_jid`. | Low |
| T26 | `query/ingester.rs`: rewrite ingest path to handle all 56 new InboundEvent variants. Compute `kind`/`variant`/`peer`/`sender`/`chat_jid` from each variant's wacore struct fields. Store full InboundEvent JSON in `payload`. Unknown events get `kind="unknown"`, `variant=variant_label`, full payload preserved. | Med |
| T27 | `query/tantivy_sidecar.rs`: register new fields: `kind` (text), `variant` (text), `peer` (text), `sender` (text), `chat_jid` (text), `payload` (json). Drop old field set. Index all wacore-typed events. | Med |
| T28 | `query/embedder_job.rs`: embedder over `payload` JSON — extract structured fields for embedding source (kind + variant + jid + status text + message text for Message variants). Hash-projection mode works on any text source. | Med |
| T29 | `query/service.rs` — adapt queries to new schema. `find` filters by kind/variant/peer — works on existing columns. `search` goes through Tantivy with new fields. `context` / `recent` unchanged. `semantic_search` adapts to new embedder. | Low |
| T30 | `query/subsystem.rs`: rebuild Tantivy index from NDJSON on boot if it's missing or stale (`rebuild_on_boot: bool` config, default `true` for fresh deployments). | Med |

**Phase 2 success criteria:**
- Boot a fresh daemon with empty `~/.local/share/octo/whatsapp/` — creates v2 NDJSON file + fresh SQL tables + fresh Tantivy index.
- Boot a daemon with v1 NDJSON still on disk — emits `WARN` log + ignores v1 file.
- Boot a daemon with v1 SQL tables still on disk — drops them + creates v2 tables. Logged `INFO`.
- Feed a synthetic `Event::UnknownVariant` through the pipeline → assert `unknown_stats.ndjson` updated with the new variant, `unknown_event_total{wacore_variant="UnknownVariant"}` Prometheus metric incremented, and the NDJSON contains the full typed payload.
- `cargo test --lib --features query` passes.

### Phase 3 — RPC + CLI + MCP + skill catalog

**Scope**: update consumer-facing surfaces to match new event shape. Add new RPC/CLI/MCP for `unknown_stats`.

**Tasks (10 commits):**

| Task | Description | Risk |
|---|---|---|
| T31 | Audit all RPC handlers matching on `InboundEvent::*` in `crates/octo-whatsapp/src/ipc/handlers/*.rs`. Each arm updated to handle new variants. Default catch-all (if any) returns error. | Med |
| T32 | New RPC: `events.list_kinds` — returns `Vec<{kind: String, variant: String}>` for all known kinds/variants. | Low |
| T33 | New RPC: `events.unknown_stats` — returns `Vec<UnknownStats>` sorted by `count desc`. Source: persister's in-memory map. | Low |
| T34 | CLI subcommands audit (`crates/octo-whatsapp/src/cli.rs`). Existing `events.list` / `events.find` / `messages.search` adapt — kind values table updated in `--help` text. | Low |
| T35 | New CLI subcommand: `octo-whatsapp events list-kinds` — prints all known `kind`/`variant` pairs. | Low |
| T36 | New CLI subcommand: `octo-whatsapp events unknown-stats` — tabulates per-variant counts, first/last seen, sample payload. Default sort: count desc. Flags: `--sort=first_seen|last_seen|count`, `--limit=N`, `--variant=<label>`. | Low |
| T37 | MCP tool descriptors (`crates/octo-whatsapp/src/mcp_server.rs`). `EXPECTED_TOOL_COUNT` recalculated. Existing tools (`wa_search`, `wa_find`, etc.) get updated kind/variant reference tables in their descriptions. | Low |
| T38 | New MCP tools: `wa_list_event_kinds`, `wa_unknown_event_stats` — mirror of CLI commands. | Low |
| T39 | Skill catalog `assets/skills/wa-mcp.md` — update §20 (event kinds table) with new entries. Update §19 (RPC reference) with `events.list_kinds` + `events.unknown_stats`. Update §22 (unknown analysis) with new operator workflow. | Low |
| T40 | All tests passing. New end-to-end test: hermetic RPC call to `events.list_kinds` returns expected set of 56 kinds. RPC call to `events.unknown_stats` after a synthetic unknown emission returns correct count. | Med |

**Phase 3 success criteria:**
- All RPC handlers compile + handle new variants.
- `cargo test --lib --features query` passes including new tests.
- `cargo clippy --lib --all-features -- -D warnings` clean.
- `cargo fmt` clean.
- `octo-whatsapp events list-kinds` prints the 56 kinds tabulated.
- `octo-whatsapp events unknown-stats` prints per-variant counts after a hermetic test that emits several unknown events.
- `wa_list_event_kinds` + `wa_unknown_event_stats` MCP tools work.

### Phase 4 — Operational guardrails (monitoring + alerting hooks)

**Scope**: make Unknown observability first-class so operators notice when wacore adds new variants.

**Tasks (5 commits):**

| Task | Description | Risk |
|---|---|---|
| T41 | Add `unknown_event_total{wacore_variant}` to Prometheus exposition. Document in `assets/skills/wa-mcp.md` §Metrics. | Low |
| T42 | New daemon config field: `unknown_event_alert_threshold: Option<u64>` (default `None`). When set, daemon emits a structured warning log when any wacore variant crosses the threshold. Operator uses this to alert on "wacore added FooBar 100 times in 1h — consider adding a typed handler". | Low |
| T43 | Add a per-day rotation to `unknown_stats.ndjson`: keep 30 days of history (rename `unknown_stats.ndjson.YYYY-MM-DD` on date change). Aggregate daily. Lets operators see trend. | Med |
| T44 | New RPC: `events.unknown_stats.history <wacore_variant> --days=30` — returns historical daily counts for one variant. | Low |
| T45 | All tests passing. New test: hermetically emit 5 unknown events with different variants, assert history RPC returns correct daily buckets. | Med |

**Phase 4 success criteria:**
- Prometheus exposes `unknown_event_total{wacore_variant}` series with non-zero values after a hermetic test.
- Alert threshold config wired correctly: daemon emits WARN log on threshold breach.
- Daily rotation + history RPC tested.
- `cargo test --lib --features query` passes.

**Total: 45 tasks, ~45 commits, 4 phases, one comprehensive plan.**

---

## Tech Stack

- Rust 1.x stable
- `wacore` from `mmacedoeu/whatsapp-rust@b637129` (already a dep; just expose at top of octo-whatsapp)
- `serde` + `serde_json` (existing) — drives both InboundEvent serialisation and stoolap payload
- `stoolap` (CipherOcto fork at `feat/blockchain-sql`) — JSON column type introduced in this plan; verify support exists or add
- `tantivy` 0.24 (existing) — JSON field support verify or add
- `tokio::sync::broadcast` (existing) — now over typed channel
- `clap` (existing) — new CLI subcommand follows existing pattern
- existing `RpcRegistry` + `tool_descriptors()` pattern
- `metrics` + `metrics-exporter-prometheus` (existing) — new counter `unknown_event_total`

---

## Test plan

### Per-variant roundtrip tests

For each new wacore variant bridge (44 of them):

1. **Synthesise test fixture.** Build a `wacore::types::events::Event::Variant(test_payload(...))` for each variant. Fixtures in `crates/octo-adapter-whatsapp/src/tests/events_v2_fixtures.rs`.
2. **Adapter bridge roundtrip.** Send event through adapter match → assert emitted `Arc<InboundEvent>` is the typed shape with correct field values.
3. **NDJSON serialise/deserialise.** Write to NDJSON via `write_event`, read back, assert equality.
4. **Query subsystem ingest.** Feed JSON to ingester, assert SQL row has correct `kind`/`variant`/`peer`/`payload`.

### Unknown handling tests

1. **`unknown_event_persists_with_full_payload`** — synthesise a wacore event variant NOT in our 56-typed-variant set (mock by directly emitting `InboundEvent::Unknown { wacore_event: synthetic_arc, variant_label: "FooBar", ... }`). Assert: NDJSON row contains the full payload, `unknown_stats.ndjson` updated, Prometheus counter incremented.
2. **`unknown_stats_aggregates_correctly`** — emit 5 events with variant="FooBar" and 3 with variant="QuxQuux". Assert `unknown_stats` returns 2 entries with counts 5 and 3, sorted by count desc.
3. **`catch_all_does_not_break_compilation`** — add a new `pub enum Event { NewFooVariant(NewFoo) }` in a test wacore stub crate. Compile adapter against it. Adapter compiles (no exhaustive match error), emits `InboundEvent::Unknown { variant_label: "NewFooVariant", ... }` on `Event::NewFooVariant(_)`. Verify metric ticks.
4. **`notification_and_raw_node_are_explicit_arms`** — synthesise `Event::Notification(arc)` and `Event::RawNode(arc)`. Assert: emit `InboundEvent::Unknown { variant_label: "Notification" or "RawNode", ... }` via the EXPLICIT arm (not the catch-all). Verify metrics: `unknown_event_total{wacore_variant="Notification"}` increments, NOT a "future variant" path.
5. **`unknown_stats_ndjson_atomic_rewrite`** — simulate persister crash mid-rewrite. Verify on boot, last-good `unknown_stats.ndjson` loads correctly (no corruption).
6. **`unknown_event_alert_threshold_logs`** — set `unknown_event_alert_threshold: u64 = 10`. Emit 11 events with the same wacore_variant. Assert daemon emits a structured WARN log.

### End-to-end hermetic tests

Two tests:

1. **`feed_all_known_wacore_events_through_daemon`** — synthesise all 57 wacore events in sequence, feed through the daemon's event processing loop, assert all land in NDJSON + SQL + Tantivy with typed `kind`/`variant` values. Assert NO `unknown_event_total` increments.
2. **`unknown_only_for_two_intentional_cases_plus_catch_all`** — synthesise the same sequence + 3 synthetic future-variants + Notification + RawNode. Assert that resulting `InboundEvent::Unknown` events come from exactly 5 sources (2 intentional + 3 catch-all), with correct `variant_label`s. `unknown_stats` returns 5 entries (2 intentional labels + 3 future labels), sorted by count desc (all equal at 1).
3. **`unknown_stats_history_rotation`** — simulate time passing. Emit unknown events on day 1, day 2, day 3. Assert daily rotation works, `events.unknown_stats.history` RPC returns correct buckets.

### Live tests (deferred)

None. All work is hermetic. Live verification on next paired WA session:
- `octo-whatsapp events list-kinds` shows 56 kinds.
- `octo-whatsapp events unknown-stats` shows low counts for `Notification` + `RawNode` only (expected volume: ~0).
- `events.find kind=unknown variant=Notification` returns ~0 rows.
- `wa_unknown_event_stats` MCP tool returns clean stats.

---

## Worktree

`feat/whatsapp-runtime-cli-mcp` (current). No push, no PR (operator rule, 2026-07-05).

---

## Operational invariants

- Stay in worktree only
- Every claim `file:line` backed
- `cargo fmt` before each commit
- `cargo clippy --lib --all-features -- -D warnings` clean per commit
- 3-second sleep between WA RPCs in live tests (N/A — no live tests)
- Phase-1/2/3/4 each get sequenced commits on the worktree, no PR until all 4 phases land

---

## Migration (operator-facing, no code)

For operators upgrading from a pre-overhaul daemon:

```bash
# 1. Stop daemon
systemctl --user stop octo-whatsapp  # or equivalent

# 2. Remove old artefacts
rm -rf ~/.local/share/octo/whatsapp/events.ndjson*
rm -rf ~/.local/share/octo/whatsapp/query/
rm -rf ~/.local/share/octo/whatsapp/unknown_stats.ndjson*
# Keep: account config, rules, device sessions

# 3. Upgrade binary
cargo build --release  # or install via your packaging

# 4. Start daemon
systemctl --user start octo-whatsapp

# 5. Verify
octo-whatsapp events list-kinds           # expect 56 kinds tabulated
octo-whatsapp events unknown-stats       # expect ~0 entries (Notification/RawNode only)
octo-whatsapp events find kind=unknown   # expect only raw wacore nodes

# 6. Set up unknown-event alerting (optional)
# Add to daemon config:
#   [events]
#   unknown_event_alert_threshold = 100
# Restart. Daemon logs WARN when any wacore variant crosses 100 emissions.
```

Documented in CHANGELOG.md (independent of this plan; tracked separately).

---

## MEMORY wrap-up

After Phase 4 lands, append summary to project memory (`whatsapp-events-first-class-overhaul.md`) + `MEMORY.md` index entry. Pattern same as Phase 7.A-7.I and Phase 8/9.

---

## Success criteria (overall)

- 56 typed `InboundEvent` variants + `Unknown { wacore_event, variant_label, ts }` for catch-all.
- Catch-all in adapter.rs emits `InboundEvent::Unknown` gracefully. **No compile error on new wacore variants.**
- `events.unknown_stats` exposes per-variant counts. CLI + MCP + history RPC all work.
- Prometheus `unknown_event_total{wacore_variant}` increments per emission.
- `unknown_stats.ndjson` sidecar persists across restarts.
- Daily rotation + history RPC enable trend analysis.
- `cargo test --lib --features query` passes with new tests including catch-all compile test.
- `cargo clippy --lib --all-features -- -D warnings` clean.
- `cargo fmt` clean.
- New `events.list_kinds` + `events.unknown_stats` + `events.unknown_stats.history` RPCs.
- New CLI subcommands: `events list-kinds`, `events unknown-stats`.
- New MCP tools: `wa_list_event_kinds`, `wa_unknown_event_stats`.
- Skill catalog updated.
- Operators can execute the migration in §Migration in 5 commands.
- **Future wacore additions become observable in `unknown_stats` automatically — no maintenance burden, full visibility for prioritisation.**

---

## Out of scope (deferred)

- Live-test chain scripts for any new kind. Existing chains for `Message`/`Receipt` etc. still pass.
- Per-variant specialised CLI subcommands. One generic `events.find kind=chat_state variant=pinned` covers it.
- Stoolap JSON column support if it doesn't exist (verify in T23 prep; if missing, add it as a small pre-T23 commit).
- CHANGELOG.md (separate doc).
- Auto-suggestion engine ("based on `unknown_stats`, here are the next 3 wacore variants that should be typed") — purely a future-comfort feature; defer until pattern stabilises.