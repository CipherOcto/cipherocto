# Pending Work Plan — Transport + Sync + GDP Integration

**Date:** 2026-06-24
**Scope:** All outstanding work to complete the transport/sync/GDP integration stack
**Baseline:** 233 tests passing, octo-sync leaf workspace complete, octo-transport leaf workspace complete (32 tests), octo-network sync module complete (16 tests), stoolap-node outbound transport wired, L4/L5 E2E tests passing

---

## Current State Summary

### What works

| Component | Status | Tests |
|-----------|--------|-------|
| octo-sync (19 modules) | Complete | 168 |
| octo-transport (7 modules) | Complete | 32 |
| octo-network sync module | Complete | 16 |
| stoolap-node outbound drain | Wired (WAL chunk broadcast) | — |
| L3 in-process E2E | Complete | 12+3+15+5 |
| L4 cross-process E2E | Complete | 7+6 |
| L5 container E2E | Complete | 7+4 |
| TransportDiscovery bridge | Implemented + tested | 6 |

### Bugs in current wiring

1. **`drop(drain_handle)` at `stoolap-node/main.rs:181`** — drain task cancelled immediately after spawn
2. **GossipDispatcher bypass** — inbound receive loop calls `handler.on_wal_tail()` directly, bypassing `GossipDispatcher` → `SyncNetworkBridge` routing
3. **No outbound DGP wrapping** — raw WAL chunks sent without `GossipSnapshotFragment` envelope metadata
4. **`SyncSegment` encode/decode missing** — `octo-sync/src/segment.rs` has the struct but no wire serialization

---

## Phase A: Bug Fixes + Wiring (1-2 days)

Priority: fix broken code, wire existing infrastructure.

### A1. Fix drain_handle drop bug

**File:** `sync-e2e-tests/stoolap-node/src/main.rs:181`

Change `drop(drain_handle)` to keep the handle alive (either `tokio::spawn` with `_drain_handle` naming, or `mem::forget` like the receive handle). Without this fix, the transport outbound path is dead — all broadcast attempts silently stop.

**Effort:** 5 min

### A2. Wire inbound receive loop through GossipDispatcher

**File:** `sync-e2e-tests/stoolap-node/src/main.rs:192-234`

Replace the direct `handler.on_wal_tail(node_id, msg.payload)` call with:
```
dispatcher.on_gossip_object(
    object_type: SYNC_SNAPSHOT_OBJECT_TYPE,
    subtype: SUBTYPE_WAL_TAIL,
    peer_id,
    payload
)
```

This routes through `SyncNetworkBridge.on_dgp_object()` → `DgpSyncBridge.dispatch()` → `SyncHandler.on_wal_tail()`, which already decodes and applies. The `GossipDispatcher` + `SyncNetworkBridge` were created at lines 184-186 but never used — they're dead code until this fix.

**Effort:** 30 min + test update

### A3. Add SyncSegment encode/decode

**File:** `octo-sync/src/segment.rs`

Add binary LE encode/decode methods to `SyncSegment`, matching the envelope convention used by `WalTailChunk` and `SummaryResponse` in `octo-sync/src/envelope.rs`:
- `encode() -> Vec<u8>` — `[table_id: u32][segment_index: u32][segment_root: 32][compression: u8][crc32: u32][lsn_watermark: u64][payload_len: u32][payload...]`
- `decode(bytes: &[u8]) -> Result<Self>` — reverse

Add round-trip unit test.

**Effort:** 1 hour

### A4. Wire outbound DGP envelope wrapping

**File:** `sync-e2e-tests/stoolap-node/src/main.rs:158-179`

Replace direct `broadcaster.broadcast(&encoded, ...)` with:
```
let fragment = sync_bridge.prepare_outbound(SUBTYPE_WAL_TAIL, peer_id, encoded);
let gossip_bytes = fragment.encode();
transport.broadcast(&gossip_bytes, &ctx).await
```

This ensures outbound WAL chunks carry proper DGP object_type/subtype metadata. Inbound (A2) already decodes via the dispatcher. Both directions must agree on the envelope format.

**Effort:** 1 hour + test update

### A5. Add tick() loop

**File:** `sync-e2e-tests/stoolap-node/src/main.rs` (new tokio task)

Spawn a periodic task calling `session.tick(current_unix_secs())` every 5 seconds. Handle returned `TickAction`s:
- `PeerSuspected` — log warning
- `PeerFailed` — `session.unsubscribe_peer(id)`
- `RequestWalTail` — send WAL tail request to peer
- `SendHeartbeat` — encode + broadcast heartbeat

Without this, stale peers accumulate forever and heartbeat timeouts go undetected.

**Effort:** 1 hour + tests

---

## Phase B: Transport-Discovery Integration (2-3 days)

Wire `TransportDiscovery` into stoolap-node for zero-config peer discovery.

### B1. Wire TransportDiscovery into stoolap-node

**File:** `sync-e2e-tests/stoolap-node/src/main.rs`

After transport creation, instantiate `TransportDiscovery`:
```
let discovery = Arc::new(TransportDiscovery::new(
    GdpGatewayIdentity::new(identity),
    mission_id,
    256,
));
```

Build advertisement from local transport:
```
let adv = discovery.build_advertisement(&transport, network_id, current_epoch);
```

**Effort:** 1 hour

### B2. GDP advertisement dissemination

**File:** New module or extension to stoolap-node

When a new peer connects (TCP or transport), exchange GDP advertisements:
- Writer sends its `GatewayAdvertisement` as part of the handshake
- Reader registers the advertisement: `discovery.register_peer(&adv, epoch)`
- Reader builds its own advertisement and sends it back

This enables discovery without a gossip broadcast layer — advertisements propagate through existing TCP/transport connections.

**Effort:** 2-3 hours

### B3. Discovery-driven peer subscription

**File:** `sync-e2e-tests/stoolap-node/src/main.rs`

After registering a peer's advertisement, check transport overlap:
```
for ep in discovery.peer_endpoints(&peer_gateway_id) {
    if discovery.peer_supports_transport(&peer_gateway_id, ep.transport_type) {
        session.subscribe_peer(SyncPeerId(peer_gateway_id));
    }
}
```

This replaces hardcoded `--peers` with discovered peers. The TCP fallback path remains for backward compatibility.

**Effort:** 1-2 hours

### B4. E2E tests for discovery-driven sync

**File:** `sync-e2e-tests/tests/l4_discovery.rs` or `l5_discovery.rs`

Test scenarios:
1. Two nodes discover each other via advertisement exchange, sync 50 rows
2. Node with only webhook adapter only discovers webhook-capable peers
3. New peer joins, advertisement propagated, sync starts automatically
4. Stale advertisement evicted from cache (GatewayCache TTL)

**Effort:** 3-4 hours

---

## Phase C: Transport Resilience (1-2 days)

Improve sync reliability through transport-aware routing.

### C1. Per-peer transport selection (send_best)

**File:** `sync-e2e-tests/stoolap-node/src/main.rs` (drain loop)

Instead of broadcasting to all transports, use `send_best()` for targeted peer sync:
```
let endpoints = discovery.peer_endpoints(&peer_id);
// send_best tries transports in order with failover
transport.send_best(&encoded, &ctx_with_peer_endpoint_priority).await
```

**Effort:** 2 hours

### C2. Health-check integration

**File:** `octo-transport/src/discovery.rs`

Add method to `TransportDiscovery` that periodically re-checks peer transport health by checking adapter health status. Mark peers as unhealthy if their transports fail health checks. This feeds into `NodeTransport`'s skip-unhealthy logic.

**Effort:** 1-2 hours

### C3. PoRelay trust score wiring

**File:** `sync-e2e-tests/stoolap-node/src/main.rs`

When relay messages pass through a peer successfully, call `session.update_relay_score(peer_id, score)`. The scoring module already factors this into `select_gossip_peers()`. Start with a simple increment-on-success model until the full PoRelay module (RFC-0860) is implemented.

**Effort:** 1-2 hours + tests

### C4. Multi-carrier cleanup

**File:** `octo-sync/src/carrier.rs`

Mark `MultiCarrierSync` as `#[deprecated]` with a note pointing to `NodeTransport`. The drain loop already uses `NodeTransport` directly. `MultiCarrierSync` exists for backward compatibility but creates architectural drift with two parallel transport abstractions.

**Effort:** 30 min

---

## Phase D: Testing Hardening (2-3 days)

### D1. L4 transport integration tests

**File:** `sync-e2e-tests/tests/l4_transport_integration.rs`

Tests exercising the full transport→sync→adapter chain in-process:
1. Writer commits → drain → transport broadcast → inbound receive → apply_wal_tail → reader sees data
2. Writer commits → drain → DGP envelope wrapping → dispatcher → handler → apply
3. Transport failover: primary adapter unhealthy → fallback adapter receives
4. Multiple writers → transport → single reader convergence
5. Tick loop detects stale peer → unsubscribes → stops sending

**Effort:** 4-5 hours

### D2. L5 Docker tests for transport discovery

**File:** `sync-e2e-tests/tests/l5_discovery.rs`

Tests with real containers:
1. Two containers discover via TCP handshake advertisement exchange, sync 100 rows
2. Writer starts, reader joins later — discovers writer via advertisement, catches up
3. Writer restarts — reader reconnects, re-discovers, resumes sync
4. Three-container fan-out: writer + 2 readers, each discovers independently

**Effort:** 4-5 hours

### D3. Adversarial review of transport integration

**File:** `docs/reviews/transport-integration-review-r{1,2,...}.md`

Multi-round adversarial review of:
- stoolap-node transport wiring (A1-A5)
- TransportDiscovery integration (B1-B3)
- Transport resilience (C1-C3)
- L4/L5 test coverage

Follow established pattern: review → fix → audit → loop until zero findings.

**Effort:** 1-2 days

---

## Phase E: RFC-0863 Completion (1-2 days)

Close remaining gaps in RFC-0863.

### E1. Update RFC-0863 status

**File:** `rfcs/draft/networking/0863-general-purpose-network-integration.md`

Update the phase completion checkboxes:
- Phase 1: [x] Core Bridge (all items done)
- Phase 2: [x] DGP Integration (export sync module, SyncDgpHandler TODOs complete)
- Phase 3: [ ] General-Purpose NodeTransport (partially done — NetworkReceiver exists, DotGateway fan-out implemented, agent/marketplace wiring deferred)

Update the "Goals Audit" section with current completion percentages.

**Effort:** 1 hour

### E2. SyncSegment encode/decode integration test

**File:** `sync-e2e-tests/tests/l3_transport_wiring.rs`

Add test verifying `SyncSegment` encode/decode round-trip through the transport chain. This is the last code TODO blocking the on_segment path.

**Effort:** 1 hour

---

## Phase F: Future Work (not planned for immediate execution)

These are deferred per RFC-0862 and RFC-0863 future work sections. Documented here for completeness.

| ID | Item | RFC Reference | Blocked By |
|----|------|---------------|------------|
| F1 | Multi-leader / active-active sync | RFC-0862 F1 | Raft overlay (0862i) |
| F2 | Trust-anchored storage checkpoint | RFC-0862 F2 | RFC-0851p-a bootstrap |
| F3 | Proof-of-sync (ZK) | RFC-0862 F3 | RFC-0859 PCE |
| F4 | ZK proof of state equivalence | RFC-0862 F4 | STWO integration |
| F5 | Cairo/Move sync port | RFC-0862 F5 | — |
| F6 | Sync on public network (high-cost carriers) | RFC-0862 F6 | Sybil resistance |
| F7 | Cross-Database flavor sync | RFC-0862 F7 | PostgreSQL compat |
| F8 | Writer election / auto-failover | RFC-0862 F8 | RFC-0855p-c coordinator |
| F9 | Schema migration protocol | RFC-0862 F9 | — |
| F10 | Reed-Solomon erasure coding for first sync | RFC-0862 F10 | RFC-0742 |
| F11 | Priority routing in NodeTransport | RFC-0863 F1 | SendContext.priority usage |
| F12 | WASM plugin runtime integration | RFC-0863 F3 | Mission 0850i |
| F13 | Transport-level encryption abstraction | RFC-0863 F4 | — |
| F14 | AdapterFactory hot-reload | RFC-0863 F5 | Runtime lifecycle mgmt |
| F15 | Full PoRelay module (RFC-0860) | RFC-0860 | Mission 0860a |
| F16 | Deterministic gossip via DGP (RFC-0852) | RFC-0852 | libp2p mesh maturity |

---

## Execution Order

```
Phase A (bugs + wiring) ← do first, unblocks everything
  ├─ A1: fix drain_handle
  ├─ A2: GossipDispatcher inbound
  ├─ A3: SyncSegment encode/decode
  ├─ A4: DGP envelope outbound
  └─ A5: tick() loop

Phase B (discovery) ← enables zero-config mesh
  ├─ B1: wire TransportDiscovery
  ├─ B2: advertisement exchange
  ├─ B3: discovery-driven subscription
  └─ B4: discovery E2E tests

Phase C (resilience) ← improves reliability
  ├─ C1: per-peer transport selection
  ├─ C2: health-check integration
  ├─ C3: PoRelay trust wiring
  └─ C4: MultiCarrierSync deprecation

Phase D (testing) ← validates all the above
  ├─ D1: L4 transport integration tests
  ├─ D2: L5 Docker discovery tests
  └─ D3: adversarial review

Phase E (RFC-0863 closure)
  ├─ E1: RFC status update
  └─ E2: SyncSegment integration test
```

**Total estimated effort:** 8-12 days of focused work
**Critical path:** A1 → A2 → A4 → D1 (fix bugs before adding features)
**Parallelizable:** B1-B3 can start after A1+A2; C1-C3 can start after A4; D2 depends on B4

---

## Test Count Projection

| Phase | New Tests | Running Total |
|-------|-----------|---------------|
| Current baseline | — | 233 |
| Phase A | +8 (bug fix tests, SyncSegment encode/decode, tick loop, DGP envelope) | 241 |
| Phase B | +6 (discovery E2E at L4/L5) | 247 |
| Phase C | +4 (send_best, health check, PoRelay wiring) | 251 |
| Phase D | +15 (L4 integration, L5 Docker, adversarial review fixes) | 266 |
| Phase E | +2 (SyncSegment integration, RFC update) | 268 |
