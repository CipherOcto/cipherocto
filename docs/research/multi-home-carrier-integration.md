# Research: General-Purpose Network Integration — Bringing CipherOcto Network to Every Use Case

**Layer:** Research (Feasibility — "CAN WE?")
**Status:** Draft v0.9 (Round 3: 0 findings — adversarial review complete)
**Date:** 2026-06-24
**Author:** CipherOcto research
**See also:** [RFC-0850 DOT](../../rfcs/accepted/networking/0850-deterministic-overlay-transport.md), [RFC-0862 Stoolap Sync](../../rfcs/accepted/networking/0862-stoolap-data-sync.md), [Sync via CipherOcto Network](stoolap-data-sync-via-cipherocto-network.md), [Social Platform Transport Patterns](social-platform-transport-patterns.md)

---

## Executive Summary

CipherOcto Network is designed to be the transport backbone for the entire platform — agent communication, database sync, marketplace operations, proof distribution, task dispatch, memory replication, and more. **27 documented use cases** across 4 tiers depend on it. Yet the network has **no general-purpose integration path**. The only working transport is raw TCP in a test binary. The `DotGateway` — intended as the central dispatch hub — has an unimplemented fan-out stub. `PlatformAdapter::send_message()` has no production caller. 23 adapter implementations exist but none are wired into any consumer.

The core problem is **not** a missing bridge between two specific traits — it's that **the CipherOcto Network was built as infrastructure but never integrated as a service**. The adapters implement a trait. The gateway exists. The gossip protocol is specified. But no code path connects "something that wants to send data" to "an adapter that can send it."

| Abstraction       | Crate        | Methods                    | Data Model              | Purpose                                      |
| ----------------- | ------------ | -------------------------- | ----------------------- | -------------------------------------------- |
| `PlatformAdapter` | octo-network | 13 (6 required, 7 default) | `DeterministicEnvelope` | DOT overlay transport (consensus, gossip)    |
| `Carrier`         | octo-sync    | 2                          | `&[u8]` raw bytes       | Database sync envelope transport             |
| `DotGateway`      | octo-network | 5 (1 stub)                 | `DeterministicEnvelope` | Central dispatch hub (UNIMPLEMENTED fan-out) |

**The question:** How do we create a general-purpose integration layer that any code — sync engines, agent runtimes, marketplace services, proof distributors — can use to send and receive data through the CipherOcto Network, via any combination of platform adapters?

**Recommendation:** Create `octo-transport`, a new leaf workspace that provides:

1. **`NetworkSender`** — a general-purpose send trait (not sync-specific) that any consumer uses
2. **`PlatformAdapterBridge`** — adapts `PlatformAdapter` → `NetworkSender` for outbound
3. **`NetworkReceiver`** — a general-purpose receive/dispatch trait for inbound
4. **`NodeTransport`** — declarative configuration of a node's transport stack
5. **`DotGateway` completion** — implement the fan-out stub so the gateway actually dispatches

See §8.2 for the phased approach.

---

## 1. The Gap

### 1.1 Current Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    octo-network                          │
│                                                          │
│  PlatformAdapter (13 methods: 6 required, 7 default)  │
│    ├── send_message(domain, envelope)                   │
│    ├── receive_messages(domain)                          │
│    ├── canonicalize(raw)                                 │
│    ├── capabilities() -> CapabilityReport                │
│    ├── domain_id(platform_id)                            │
│    ├── platform_type()                                   │
│    ├── health_check()                                    │
│    ├── shutdown()                                        │
│    └── ... (media, coordinator_admin)                    │
│                                                          │
│  AdapterRegistry                                         │
│    ├── register_builtin(adapter)                         │
│    ├── discover_and_load() -> .so plugins via libloading │
│    └── get(platform_type) -> &dyn PlatformAdapter        │
│                                                          │
│  23 adapter implementations (Telegram, Discord, QUIC...) │
└─────────────────────────────────────────────────────────┘

         ╳ NO BRIDGE ╳

┌─────────────────────────────────────────────────────────┐
│                    octo-sync                              │
│                                                          │
│  Carrier (2 methods, async)                              │
│    ├── name() -> &str                                    │
│    └── send(envelope: &[u8]) -> Result<(), SyncError>    │
│                                                          │
│  MultiCarrierSync                                        │
│    ├── broadcast(envelope) -> usize (success count)      │
│    ├── health tracking (u64 basis points, EMA)           │
│    └── crypto integration (PRIVATE mission encryption)   │
│                                                          │
│  stoolap-node: only raw TCP, no platform adapters      │
└─────────────────────────────────────────────────────────┘
```

### 1.2 What's Missing

1. **No `PlatformAdapter` → `Carrier` bridge.** The 23 adapters in octo-network cannot be used as sync carriers. An operator who wants sync over Telegram, Discord, or QUIC must manually write a wrapper.

2. **No `Carrier` → `PlatformAdapter` bridge.** A raw TCP carrier (like the `stoolap-node` transport) cannot participate in the DOT overlay, DGP gossip, or multi-carrier propagation.

3. **No `NodeTransport` configuration.** No declarative way for a node to say "I want sync via [TCP + QUIC + Telegram]" — the carriers must be wired manually in code.

4. **Leaf workspace isolation prevents direct dependency.** `octo-sync` is a leaf workspace (excluded from the main workspace) to avoid circular deps with stoolap. It cannot import `octo-network::PlatformAdapter` directly.

5. **`stoolap-node` is transport-locked.** The E2E test binary only does raw TCP. No mechanism to test or run sync over platform adapters.

6. **Plugin loading disconnected from consumers.** `AdapterRegistry::discover_and_load()` loads `.so` plugins, but there's no code path to feed loaded adapters into any consumer — sync engines, agent runtimes, marketplace services, or proof distributors.

### 1.3 Who Needs the Network — Use Case Inventory

**27 documented use cases** across 4 tiers depend on CipherOcto Network transport:

#### Tier 1 — Blocked Now (no integration path exists)

| Use Case                 | Consumer             | What It Needs                                       | File                                                 |
| ------------------------ | -------------------- | --------------------------------------------------- | ---------------------------------------------------- |
| Database sync (RFC-0862) | `SyncSessionManager` | Send/receive WAL chunks via platform adapters       | `rfcs/accepted/networking/0862-stoolap-data-sync.md` |
| Cross-carrier sync       | `MultiCarrierSync`   | Fan-out sync envelopes to 2+ carriers with failover | `missions/with-pr/0862g-cross-carrier-sync.md`       |
| Stoolap-node binary      | E2E test infra       | Platform adapter transport (not just raw TCP)       | `sync-e2e-tests/stoolap-node/`                       |
| DotGateway fan-out       | All DOT consumers    | Central dispatch to adapters (stub)                 | `crates/octo-network/src/dot/mod.rs:175`             |

#### Tier 2 — Will Benefit Immediately

| Use Case                                 | Consumer               | What It Needs                               | File                                                          |
| ---------------------------------------- | ---------------------- | ------------------------------------------- | ------------------------------------------------------------- |
| DGP gossip (RFC-0852)                    | `SyncNode` (dead code) | Gossip objects riding platform adapters     | `rfcs/draft/networking/0852-deterministic-gossip-protocol.md` |
| Onion relay routing (RFC-0858)           | ORR module             | Multi-transport onion paths across carriers | `rfcs/draft/networking/0858-onion-relay-routing.md`           |
| Proof-of-Relay (RFC-0860)                | PoRelay module         | Cross-platform relay forwarding             | `rfcs/draft/networking/0860-proof-of-relay.md`                |
| Overlay mempool (RFC-0857)               | DOM module             | Multi-transport mempool propagation         | `rfcs/draft/networking/0857-deterministic-overlay-mempool.md` |
| Deterministic route selection (RFC-0856) | DRS module             | Route across multiple PlatformAdapters      | `rfcs/draft/networking/0856-deterministic-route-selection.md` |

#### Tier 3 — Platform-Level Use Cases

| Use Case              | Consumer            | What It Needs                                | File                                              |
| --------------------- | ------------------- | -------------------------------------------- | ------------------------------------------------- |
| Agent marketplace     | Agent runtime       | Cross-platform agent communication           | `docs/use-cases/agent-marketplace.md`             |
| Agent memory layer    | Memory system       | Persistent memory replicated via network     | `docs/use-cases/verifiable-agent-memory-layer.md` |
| DeFi agents           | Agent runtime       | Cross-platform task dispatch                 | `docs/use-cases/verifiable-ai-agents-defi.md`     |
| Enterprise private AI | Enterprise deployer | Private network mode with carrier diversity  | `docs/use-cases/enterprise-private-ai.md`         |
| Bandwidth providers   | P2P layer           | Bandwidth sharing via adapters               | `docs/use-cases/bandwidth-provider-network.md`    |
| Storage providers     | Storage layer       | Storage node discovery via network           | `docs/use-cases/storage-provider-network.md`      |
| Quota marketplace     | Quota router        | Quota trading settlement transport           | `docs/use-cases/ai-quota-marketplace.md`          |
| Data marketplace      | Marketplace         | Trustless data exchange via network          | `docs/use-cases/data-marketplace.md`              |
| Orchestrator runtime  | Orchestrator        | Task routing across heterogeneous transports | `missions/orchestrator-runtime.md`                |

#### Tier 4 — Already Implemented (but architecturally coupled)

| Use Case              | Consumer                | Status                             | File                                                        |
| --------------------- | ----------------------- | ---------------------------------- | ----------------------------------------------------------- |
| 23 platform adapters  | `PlatformAdapter` trait | Implemented, no production callers | `crates/octo-adapter-*/`                                    |
| `CoordinatorAdmin`    | Group management        | Implemented, not wired to sync     | `crates/octo-network/src/dot/adapters/coordinator_admin.rs` |
| GDP discovery         | Peer bootstrap          | Implemented, used by QUIC adapter  | `crates/octo-network/src/gdp/`                              |
| OCrypt key derivation | Per-mission encryption  | Implemented, used by sync keyring  | `crates/octo-network/src/ocrypt/`                           |

**Key insight:** The network infrastructure is built (adapters, gateway, gossip, crypto, discovery), but **no consumer can actually use it** because the integration layer — the code that connects "I want to send data" to "adapter X can send it" — doesn't exist.

---

## 2. Deep Analysis — The Gap Is Deeper Than Two Traits

The v0.1 analysis identified the missing bridge between `PlatformAdapter` and `Carrier`. Deep investigation reveals the gap is **systemic** — the entire production data flow between DOT adapters, DGP gossip, and the sync engine is disconnected. Multiple components exist as dead code or stubs.

### 2.1 Production Call Path Audit

| Component                         | Location                                          | Status                   | Notes                                                                                                                                  |
| --------------------------------- | ------------------------------------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| `DotGateway::process_envelope()`  | `crates/octo-network/src/dot/mod.rs:175`          | **STUB**                 | Fan-out to adapters is a TODO: _"In production, this would iterate over connected domains and forward to the appropriate adapter(s)."_ |
| `PlatformAdapter::send_message()` | 23 implementations                                | **NO PRODUCTION CALLER** | Only called by tests and adapter internal code. No gateway, no DGP, no sync layer calls it.                                            |
| `SyncNode`                        | `crates/octo-network/src/sync/mod.rs`             | **DEAD CODE**            | Module not exported from `lib.rs`. Unreachable from crate public API.                                                                  |
| `SyncNetworkBridge`               | `crates/octo-network/src/sync/dgp_integration.rs` | **DEAD CODE**            | Same — `pub mod sync` not in `lib.rs`.                                                                                                 |
| `MultiCarrierSync`                | `octo-sync/src/carrier.rs`                        | **UNUSED**               | Exported from octo-sync but never referenced by `SyncSessionManager` or any production code. Only used in E2E tests.                   |
| `SyncSessionManager::on_commit()` | `octo-sync/src/session.rs:214`                    | **IN-MEMORY ONLY**       | Fans out `WalTailChunk` to subscribers via in-memory channels. No serialization to envelope bytes, no carrier broadcast.               |
| `DgpSyncBridge::dispatch()`       | `octo-sync/src/dgp_bridge.rs`                     | **UNREACHABLE**          | Called only by dead code (`SyncNode`, `SyncNetworkBridge`).                                                                            |

### 2.2 The Three Missing Links

**Link 1: SyncSessionManager → Carrier broadcast**

```
SyncSessionManager::on_commit(txn_id, from_lsn, to_lsn)
  → streamer.on_commit()  ← ✅ EXISTS (in-memory fan-out)
  → serialize chunks       ← ❌ MISSING (no code serializes WalTailChunk → bytes)
  → MultiCarrierSync::broadcast()  ← ❌ MISSING (no carrier reference in session)
```

**Link 2: DotGateway → PlatformAdapter dispatch**

```
DotGateway::process_envelope()
  → version/flags/signature verify  ← ✅ EXISTS
  → replay cache check              ← ✅ EXISTS
  → forward to adapters             ← ❌ STUB (TODO comment, not implemented)
  → adapter.send_message()         ← ❌ NO PRODUCTION CALLER
```

**Link 3: DGP gossip → Sync engine**

```
DGP GossipObject(subtype=0x0008)
  → SyncNode::on_snapshot_fragment()  ← ❌ DEAD CODE (module not exported)
  → DgpSyncBridge::dispatch()         ← ❌ DEAD CODE (reachable only from dead code)
  → SyncHandler::on_wal_tail()        ← ❌ DEAD CODE (same)
```

### 2.3 Why This Matters

The existing architecture has **all the pieces** but they're **not wired together**:

| Piece                        | Where          | What It Does                                                 |
| ---------------------------- | -------------- | ------------------------------------------------------------ |
| `PlatformAdapter` (23 impls) | octo-network   | Send/receive via Telegram, Discord, QUIC, Webhook, P2P, etc. |
| `AdapterRegistry`            | octo-network   | Dynamic plugin loading from `.so` files                      |
| `DotGateway`                 | octo-network   | Envelope verification, replay protection                     |
| `DgpSyncBridge`              | octo-sync      | Route DGP objects to sync engine                             |
| `SyncSessionManager`         | octo-sync      | Session lifecycle, peer state machines                       |
| `MultiCarrierSync`           | octo-sync      | Multi-carrier broadcast with health tracking                 |
| `Carrier`                    | octo-sync      | Minimal send trait                                           |
| `stoolap-node`               | sync-e2e-tests | Only raw TCP transport                                       |

The **current working path** for data sync is:

```
stoolap-node (TCP only)
  → raw TCP send (custom protocol: [u32 len][u8 type][payload])
  → stoolap-node (TCP reader)
  → adapter.apply_wal_entry()
```

This is **entirely separate** from the DOT/DGP/PlatformAdapter stack. The sync system and the network system are two parallel implementations that never meet.

### 2.4 Real Adapter Transport Details

| Adapter            | Transport               | Serialization                                                      | Max Payload | Binary | Notes                                |
| ------------------ | ----------------------- | ------------------------------------------------------------------ | ----------- | ------ | ------------------------------------ |
| `QuicAdapter`      | QUIC (quinn)            | `envelope.to_wire_bytes()` → `[u32 len][u16 type=0x0001][payload]` | 1MB         | ✅     | TLS 1.3, 0-RTT, connection migration |
| `WebhookAdapter`   | HTTP POST/PUT (reqwest) | `envelope.to_wire_bytes()` → `application/octet-stream` body       | 1MB         | ✅     | Retry with backoff, HMAC-SHA256      |
| `NativeP2PAdapter` | libp2p gossipsub        | `envelope.to_wire_bytes()`                                         | 64KB        | ✅     | **STUB** — logs but doesn't publish  |
| `TelegramAdapter`  | Telegram Bot API        | `envelope.to_wire_bytes()` → base64 in message                     | 4KB         | ❌     | Text-based, needs DOT/1/{b64}        |

All adapters serialize via `DeterministicEnvelope::to_wire_bytes()`. The bridge from raw payload bytes to a `DeterministicEnvelope` is the key missing conversion.

### 2.5 PlatformType — All 21 Variants

```rust
#[repr(u16)]
pub enum PlatformType {
    Telegram=0x0001, Discord=0x0002, Matrix=0x0003, Nostr=0x0004,
    Signal=0x0005, IRC=0x0006, Slack=0x0007, WhatsApp=0x0008,
    Webhook=0x0009, NativeP2P=0x000A, Bluetooth=0x000B, LoRa=0x000C,
    WebRTC=0x000D, Bluesky=0x000E, Twitter=0x000F, Reddit=0x0010,
    WeChat=0x0011, DingTalk=0x0012, Lark=0x0013, QQ=0x0014, Quic=0x0015,
}
```

Transport-relevant: **Webhook** (0x0009), **NativeP2P** (0x000A), **Quic** (0x0015).

### 2.6 AdapterRegistry — One Per Platform Type

The `AdapterRegistry` enforces **one adapter per platform type** (`registry.rs:67-68`):

```rust
if self.adapters.contains_key(&platform_type) {
    return Err(AdapterLoadError::DuplicatePlatform { platform_type });
}
```

A gateway instance typically has 1 adapter per platform type. Each adapter handles **multiple broadcast domains** (multiple groups/channels on that platform). The `BroadcastDomainId` parameter in `send_message` distinguishes which specific domain within a platform to target.

### 2.7 Dependency Graph

```
octo-network
  depends on → octo-sync (via path = "../../octo-sync")
  imports from → octo_sync::dgp_bridge, octo_sync::session (in sync/ module)
  sync/ module: NOT EXPORTED (dead code)

octo-sync (leaf workspace, excluded from main workspace)
  depended on by → octo-network, stoolap
  does NOT depend on → octo-network
```

This means:

- octo-network CAN import octo-sync types (it already does in the dead `sync/` module)
- octo-sync CANNOT import octo-network types (no dependency declared)
- A bridge crate that depends on BOTH is the clean way to connect them

---

## 3. Candidate Approaches

### Approach A: Feature-Gated Bridge in octo-sync

Add an optional `network` feature to `octo-sync` that enables a `PlatformAdapterCarrier` wrapper:

```rust
// octo-sync/src/carrier.rs (behind #[cfg(feature = "network")])
pub struct PlatformAdapterCarrier {
    adapter: Arc<dyn PlatformAdapter>,
    domain: BroadcastDomainId,
}

#[async_trait]
impl Carrier for PlatformAdapterCarrier {
    fn name(&self) -> &str { self.adapter.platform_type().name() }
    async fn send(&self, envelope: &[u8]) -> Result<(), SyncError> {
        // Wrap raw bytes into a DeterministicEnvelope, send via adapter
    }
}
```

**Pros:**

- Clean separation; no circular deps (feature-gated)
- Reuses existing 23 adapters instantly

**Cons:**

- Introduces octo-network as optional dependency of octo-sync
- DeterministicEnvelope construction requires DOT types (envelope ID, mission ID, etc.)
- The raw `&[u8]` → `DeterministicEnvelope` conversion is lossy (no domain, no envelope metadata)

### Approach B: Transport Bridge in octo-network (Reverse Direction)

Add a `CarrierAdapter` in octo-network that wraps a raw TCP `Carrier` as a `PlatformAdapter`:

```rust
// crates/octo-network/src/dot/adapters/carrier_adapter.rs
pub struct CarrierAdapter {
    carrier: Arc<dyn Carrier>,
    platform_type: PlatformType,
}
```

**Pros:**

- TCP carriers join the DOT overlay naturally
- Multi-carrier propagation works for TCP out of the box

**Cons:**

- Requires octo-network to depend on octo-sync (circular!)
- A `PlatformAdapter` has rich semantics (domains, replay, capabilities) that a raw `Carrier` doesn't have
- Conceptually backwards: carriers are simpler than adapters

### Approach C: Shared Transport Crate (New `octo-transport`)

Create a new leaf workspace `octo-transport` that depends on both `octo-sync` and `octo-network` and provides the bridge:

```rust
// octo-transport/src/lib.rs
pub mod adapter_bridge;    // PlatformAdapter → NetworkSender bridge
pub mod receiver;          // NetworkReceiver for inbound dispatch
pub mod node_transport;    // NodeTransport configuration
pub mod sender;            // NetworkSender trait
```

**Pros:**

- Both octo-sync and octo-network remain clean leaf crates
- Bridge logic lives in a dedicated crate with no circular deps
- Can be feature-gated per direction

**Cons:**

- Third workspace to maintain
- More complex build graph

### Approach D: Trait Object Bridge via `dyn Any` / Type Erasure

Define a minimal `TransportEnvelope` type in a shared primitives crate, and use type-erased bridges:

```rust
// Shared type
pub struct TransportEnvelope {
    pub payload: Vec<u8>,
    pub mission_id: [u8; 32],
    pub sender_id: [u8; 32],
}
```

Both `PlatformAdapter` and `Carrier` implementations accept/return this type.

**Pros:**

- Minimal coupling
- Both sides can evolve independently

**Cons:**

- Another shared crate
- Type erasure loses compile-time guarantees

### Approach E: Node-Level Wiring

Keep `octo-sync` and `octo-network` independent. The bridge happens at the **node level** — the binary that assembles the system (e.g., `stoolap-node`) does the wiring:

```rust
// In stoolap-node or any node binary
let registry = AdapterRegistry::new(plugin_dirs);
registry.discover_and_load();

let carriers: Vec<Arc<dyn Carrier>> = registry.registered_types()
    .iter()
    .filter_map(|&pt| {
        let adapter: &dyn PlatformAdapter = registry.get(pt)?;
        // Note: need Arc conversion — registry owns Box<dyn PlatformAdapter>,
        // so this requires Either registry restructure or a wrapper
        Some(Arc::new(PlatformAdapterCarrier::new(adapter)) as Arc<dyn Carrier>)
    })
    .collect();

let broadcaster = MultiCarrierSync::new(carriers);
```

**Pros:**

- No new crates, no feature flags, no circular deps
- Node operator controls exactly which adapters become carriers
- Both octo-sync and octo-network stay clean

**Cons:**

- The bridge wrapper (`PlatformAdapterCarrier`) still needs to live somewhere
- Each node binary reimplements the wiring

---

## 4. Recommended Architecture

**Approach C (Shared Transport Crate):** A new `octo-transport` leaf workspace that depends on both `octo-sync` and `octo-network`, providing a general-purpose integration layer. Both upstream crates remain clean leaf workspaces with no new dependencies.

The key design principle: **the integration layer is not sync-specific**. It provides generic send/receive primitives that any consumer — sync engines, agent runtimes, marketplace services, proof distributors — can use.

### 4.1 The NetworkSender Trait

```rust
// octo-transport/src/sender.rs

/// General-purpose outbound transport trait.
///
/// Any code that wants to send data through the CipherOcto Network
/// implements this trait or uses a provided adapter bridge.
#[async_trait]
pub trait NetworkSender: Send + Sync {
    /// Send a payload through this transport.
    async fn send(&self, payload: &[u8], context: &SendContext) -> Result<(), TransportError>;

    /// Return the transport name for diagnostics.
    fn name(&self) -> &str;

    /// Whether this transport is healthy and available.
    fn is_healthy(&self) -> bool;
}

/// Context for a send operation.
pub struct SendContext {
    /// The mission ID (determines encryption keys, routing).
    pub mission_id: [u8; 32],
    /// Optional target domain (for domain-specific adapters).
    pub domain: Option<BroadcastDomainId>,
    /// Priority level (for mempool/routing decisions).
    pub priority: u8,
}
```

### 4.2 The PlatformAdapterBridge

```rust
// octo-transport/src/adapter_bridge.rs

/// Bridges any PlatformAdapter into a NetworkSender.
///
/// Handles DeterministicEnvelope construction, domain resolution,
/// and error mapping. This is the primary integration point.
pub struct PlatformAdapterBridge {
    adapter: Arc<dyn PlatformAdapter>,
    domain: BroadcastDomainId,
}

#[async_trait]
impl NetworkSender for PlatformAdapterBridge {
    fn name(&self) -> &str {
        &format!("{:?}", self.adapter.platform_type())
    }

    async fn send(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError> {
        // 1. Construct DeterministicEnvelope from payload + context
        // 2. Resolve target domain from ctx.domain or self.domain
        // 3. Call self.adapter.send_message(&domain, &env).await
        // 4. Map PlatformAdapterError → TransportError
    }

    fn is_healthy(&self) -> bool { true }
}
```

### 4.3 NodeTransport Configuration

```rust
// octo-transport/src/node_transport.rs

/// Declarative transport stack for any node.
///
/// Consumers declare which transports are available; the transport
/// layer handles routing, failover, and health tracking.
pub struct NodeTransport {
    /// Named transports (QUIC, Webhook, P2P, Telegram, etc.)
    senders: Vec<Arc<dyn NetworkSender>>,
    /// Default send timeout
    send_timeout: Duration,
    /// Health check interval
    health_check_interval: Duration,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self { ... }

    /// Broadcast to all healthy transports (fan-out).
    pub async fn broadcast(&self, payload: &[u8], ctx: &SendContext) -> usize {
        // Concurrent send to all healthy transports
        // Returns count of successful sends
    }

    /// Send to the best available transport (failover).
    pub async fn send_best(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError> {
        // Try transports in priority order, failover on error
    }
}
```

### 4.4 Dynamic Loading Flow

```
Node Startup:
  1. AdapterRegistry::discover_and_load()  // loads .so plugins
  2. For each loaded adapter:
     a. Create PlatformAdapterBridge wrapper
     b. Add to NodeTransport
  3. NodeTransport is now available to any consumer:
     - Sync engine calls node_transport.broadcast(wal_chunks)
     - Agent runtime calls node_transport.send_best(task_data)
     - Marketplace calls node_transport.broadcast(settlement)
     - Proof distributor calls node_transport.send_best(proof)
  4. DotGateway fan-out routes inbound envelopes to handlers
```

---

## 5. Design Decisions

### 5.1 Why Not Carrier → PlatformAdapter?

A `Carrier` is send-only raw bytes. A `PlatformAdapter` is bidirectional with rich semantics (domains, replay protection, capabilities, media). Going from simple → complex requires filling in all the semantics, which is fragile. Going from complex → simple (PlatformAdapter → Carrier) naturally drops capabilities — the adapter just sends bytes.

### 5.2 Why Not Feature-Gating in octo-sync?

Feature-gating `network` in `octo-sync` would introduce `octo-network` as an optional dependency of the leaf workspace. This is undesirable: the leaf workspace pattern (also used by `octo-determin`) exists to keep downstream crates free of transitive deps. A separate `octo-transport` crate avoids this cleanly — `octo-sync` never sees `octo-network` types.

### 5.3 Why a Separate Crate (Approach C)?

A separate `octo-transport` leaf workspace keeps both `octo-sync` and `octo-network` free of new dependencies. The bridge requires `DeterministicEnvelope` construction (envelope ID, source key, TTL, flags) plus `BroadcastDomainId` handling — likely 200-400 lines. A dedicated crate with its own `Cargo.toml` avoids feature-flag complexity and keeps the dependency graph clean. The pattern already exists: `octo-determin` is a leaf workspace consumed by both `cipherocto` and `stoolap`.

### 5.4 What About the Existing Carrier Trait?

The `Carrier` trait in octo-sync remains available for sync-specific use cases (e.g., `MultiCarrierSync`). The new `NetworkSender` trait in octo-transport is the general-purpose alternative. Sync consumers that already use `Carrier` can continue doing so; new consumers should use `NetworkSender`.

---

## 6. Impact on E2E Testing

With the bridge in place, E2E tests can exercise transport over any adapter — not just sync:

| Level              | Current                    | With Bridge                                                                         |
| ------------------ | -------------------------- | ----------------------------------------------------------------------------------- |
| L3 (in-process)    | MockAdapter + TestCarrier  | Same (no change)                                                                    |
| L4 (cross-process) | stoolap-node TCP only      | stoolap-node with PlatformAdapterBridge(QUIC), PlatformAdapterBridge(Webhook), etc. |
| L5 (Docker)        | Docker containers TCP only | Containers with .so adapter plugins loaded dynamically                              |

General-purpose transport tests (Phase 3):

| Test                         | What It Verifies                                           |
| ---------------------------- | ---------------------------------------------------------- |
| Send through QUIC adapter    | `PlatformAdapterBridge` → `QuicAdapter` → remote peer      |
| Send through Webhook adapter | `PlatformAdapterBridge` → `WebhookAdapter` → HTTP endpoint |
| Multi-transport failover     | QUIC fails → automatic fallback to Webhook                 |
| Plugin-loaded adapter        | `.so` adapter loaded at runtime, used as transport         |
| Inbound dispatch (Phase 3)   | Remote sends → `DotGateway` → `NetworkReceiver` → handler  |

The `stoolap-node` binary would gain `--adapter` flags:

```
stoolap-node --dsn file://... --listen 3333 --adapter p2p --adapter webhook
```

Each `--adapter` loads the corresponding platform adapter and wraps it as a `NetworkSender`.

---

## 7. Open Questions

| #   | Question                                                                             | Impact                                                                              | New Finding                                                                                                 |
| --- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Q1  | Should `NetworkSender` be a trait or just a function?                                | Traits enable mocking and testing; functions are simpler.                           | **Trait preferred** — enables unit tests with mock transports, matches `PlatformAdapter` pattern.           |
| Q2  | Should the bridge handle inbound (receive → dispatch)?                               | Inbound matters for agent comms, marketplace, gossip. Not just sync.                | **Yes in Phase 3** — Phase 1 is outbound-only; Phase 3 adds `NetworkReceiver` for inbound dispatch.         |
| Q3  | How to handle adapters that need async initialization (login, token refresh)?        | Adapter lifecycle management must happen before transport use.                      | **AdapterRegistry** has `health_check()` and `shutdown()`. Bridge can expose lifecycle.                     |
| Q4  | Should the WASM plugin runtime (mission 0850i) also produce transports?              | WASM adapters would need the same bridge.                                           | **Yes** — WASM adapters implement PlatformAdapter via C ABI, bridge works identically.                      |
| Q5  | How does this relate to DGP gossip (RFC-0852)?                                       | DGP uses libp2p mesh; PlatformAdapters are DOT-layer. Two separate transport paths. | **Phase 1 bypasses DGP** (direct adapter transport). Phase 2 adds DGP path for gossip-compatible scenarios. |
| Q6  | Should `NodeTransport` support priority routing (QUIC for large, Webhook for small)? | Different payloads have different transport requirements.                           | **Yes** — `SendContext.priority` field enables routing decisions. Phase 3 feature.                          |
| Q7  | How do marketplace agents discover available transports on remote nodes?             | Agents need to know what transports a peer supports before sending.                 | **CapabilityReport** already exists per-adapter. Gateway can advertise supported transports via GDP.        |
| Q8  | Should the integration layer be async or sync?                                       | Some consumers (stoolap) are sync; others (agent runtime) are async.                | **Async with sync wrapper** — `NetworkSender` is async; `tokio::task::sync_blocking` bridge for sync code.  |

---

## 8. Rethinking: The Fundamental Question

The deep analysis reveals that the issue isn't just "missing a bridge" — it's that **the CipherOcto Network was built as infrastructure but never integrated as a service**. We have:

1. A working sync engine (octo-sync) with session management, WAL streaming, carrier broadcasting
2. A working network layer (octo-network) with 23 adapters, dynamic plugin loading, gateway
3. Dead-code bridges between them (`SyncNode`, `SyncNetworkBridge` — not exported)
4. A TCP-only test binary (stoolap-node) that bypasses both systems
5. **27 documented use cases** across 4 tiers that depend on the network but have no integration path

### 8.1 Three Possible Directions

**Direction A: Complete the sync→DGP path (top-down)**

- Export the `sync` module from octo-network
- Wire `SyncSessionManager` → `SyncNode` → DGP gossip → remote gateway → `SyncNetworkBridge` → `DgpSyncBridge`
- Sync rides on the gossip protocol, which rides on platform adapters
- **Pro:** Leverages the full DOT/DGP stack as designed
- **Con:** Heavy dependency chain; every sync node needs the full network stack

**Direction B: Complete the adapter→transport path (bottom-up)**

- Implement `PlatformAdapterBridge` general-purpose bridge
- Wire any consumer → `NodeTransport` → adapters
- Code sends directly through platform adapters, bypassing DGP
- **Pro:** Lightweight; consumers only need adapter trait objects; works for ALL use cases (sync, agents, marketplace, proofs)
- **Con:** Bypasses DGP anti-entropy and gossip-compatible multi-node convergence

**Direction C: Unified transport layer (both directions)**

- Create an `octo-transport` crate that connects both paths
- Consumers can go through DGP (for gossip-compatible scenarios) OR directly through adapters (for point-to-point)
- **Pro:** Maximum flexibility; serves all 27 use cases
- **Con:** Most complex; two code paths to maintain

### 8.2 Recommended Direction

**Direction B first, Direction C later.**

Phase 1: General-purpose bottom-up bridge. Implement `NetworkSender` trait and `PlatformAdapterBridge` in `octo-transport`, wire it for sync first (proves the pattern). This immediately unlocks:

- Database sync over QUIC, Webhook, P2P (not just TCP)
- Dynamic transport loading from `.so` plugins
- Multi-transport failover
- No changes to octo-sync or octo-network internals
- **Pattern reusable by all 27 use cases**

Phase 2: DGP integration. Complete the sync→DGP path for gossip-compatible scenarios (N-node convergence, anti-entropy). Export `SyncNode` and `SyncNetworkBridge`.

Phase 3: General-purpose NodeTransport. A `NodeTransport` configuration that any consumer — agent runtime, marketplace, proof distributor — uses to send/receive via the network.

### 8.3 Impact on Existing Code

| Component      | Phase 1 Change                                         | Phase 2 Change           | Phase 3 Change                      |
| -------------- | ------------------------------------------------------ | ------------------------ | ----------------------------------- |
| `octo-sync`    | No changes (Carrier trait stays for sync-specific use) | No changes               | Adapter bridge as optional dep      |
| `octo-network` | No changes                                             | Export `sync` module     | DotGateway fan-out completion       |
| `stoolap-node` | Add `--adapter` flags, use PlatformAdapterBridge       | Also wire DGP path       | Use NodeTransport                   |
| E2E tests      | Add L4 cross-transport tests (QUIC + Webhook)          | Add DGP-based sync tests | Add general-purpose transport tests |
| New crate      | `octo-transport` (bridge)                              | Same                     | Same                                |
| Agent runtime  | —                                                      | —                        | Use NodeTransport for agent comms   |
| Marketplace    | —                                                      | —                        | Use NodeTransport for settlement    |

---

## 9. Next Steps

### Phase 1 (General-Purpose Bridge — Proves the Pattern)

1. **Create `octo-transport` crate** (leaf workspace) — depends on both octo-sync and octo-network
2. **Implement `NetworkSender` trait** — general-purpose outbound transport (not sync-specific)
3. **Implement `PlatformAdapterBridge`** — wraps any `PlatformAdapter` as a `NetworkSender`
4. **Implement `AdapterFactory`** — takes `AdapterRegistry`, produces `Vec<Arc<dyn NetworkSender>>`
5. **Wire sync as first consumer** — `SyncSessionManager` uses `NodeTransport` for broadcast (proves the pattern)
6. **Update `stoolap-node`** — add `--adapter p2p --adapter webhook` flags
7. **Add L4 cross-transport E2E tests** — sync over QUIC + Webhook simultaneously

### Phase 2 (DGP Integration)

8. **Export `sync` module from octo-network** — make `SyncNode`, `SyncNetworkBridge` public
9. **Wire `SyncSessionManager` → DGP** — complete the gossip-compatible sync path
10. **Add DGP-based sync tests** — multi-node convergence via gossip

### Phase 3 (General-Purpose NodeTransport)

11. **Implement `NodeTransport`** — declarative transport stack for any consumer
12. **Complete `DotGateway` fan-out** — implement the adapter dispatch stub
13. **Wire agent runtime** — agents communicate via `NodeTransport`
14. **Wire marketplace** — settlement and discovery via `NodeTransport`
15. **Create RFC** — formalize general-purpose network integration architecture

---

## 10. Appendix A: File Reference

### Core Traits and Types

| File                                                      | Line | Relevance                                                   |
| --------------------------------------------------------- | ---- | ----------------------------------------------------------- |
| `crates/octo-network/src/dot/adapters/mod.rs:75-181`      | 75   | `PlatformAdapter` trait (13 methods, 6 required, 7 default) |
| `crates/octo-network/src/dot/adapters/registry.rs:37-278` | 37   | `AdapterRegistry` with dynamic `.so` loading                |
| `crates/octo-network/src/dot/adapters/abi.rs:1-60`        | 1    | C ABI plugin types, `ADAPTER_ABI_VERSION = 1`               |
| `crates/octo-network/src/dot/domain.rs:6-30`              | 6    | `PlatformType` enum (21 variants)                           |
| `crates/octo-network/src/dot/domain.rs:62-143`            | 62   | `BroadcastDomainId` (BLAKE3-hashed)                         |
| `octo-sync/src/carrier.rs:23-36`                          | 23   | `Carrier` trait (2 methods)                                 |
| `octo-sync/src/carrier.rs:128-266`                        | 128  | `MultiCarrierSync` broadcaster                              |

### Dead Code / Stubs

| File                                                      | Line | Status                                           |
| --------------------------------------------------------- | ---- | ------------------------------------------------ |
| `crates/octo-network/src/dot/mod.rs:175`                  | 175  | `DotGateway::process_envelope` fan-out TODO stub |
| `crates/octo-network/src/sync/mod.rs:38-95`               | 38   | `SyncNode` — NOT EXPORTED (dead code)            |
| `crates/octo-network/src/sync/dgp_integration.rs:119-198` | 119  | `SyncNetworkBridge` — NOT EXPORTED (dead code)   |
| `octo-sync/src/session.rs:214-216`                        | 214  | `on_commit` — in-memory only, no carrier         |

### Real Adapter Implementations

| File                                            | Adapter            | Transport           |
| ----------------------------------------------- | ------------------ | ------------------- |
| `crates/octo-adapter-quic/src/lib.rs:204-726`   | `QuicAdapter`      | QUIC (quinn)        |
| `crates/octo-adapter-webhook/src/lib.rs:84-338` | `WebhookAdapter`   | HTTP POST (reqwest) |
| `crates/octo-adapter-p2p/src/lib.rs:56-325`     | `NativeP2PAdapter` | libp2p gossipsub    |

### Integration Points

| File                                      | Line | Relevance                                                           |
| ----------------------------------------- | ---- | ------------------------------------------------------------------- |
| `octo-sync/src/carrier.rs:23-36`          | 23   | `Carrier` trait — doc says "Implementations wrap a PlatformAdapter" |
| `octo-sync/src/lib.rs:61`                 | 61   | `carrier` module re-export                                          |
| `octo-sync/Cargo.toml`                    | 1    | Leaf workspace, no octo-network dependency                          |
| `crates/octo-network/Cargo.toml:28`       | 28   | octo-network depends on octo-sync                                   |
| `sync-e2e-tests/stoolap-node/src/main.rs` | 1    | TCP-only transport binary                                           |

### Research and RFCs

| File                                                               | Relevance                             |
| ------------------------------------------------------------------ | ------------------------------------- |
| `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` | DOT RFC — multi-carrier design        |
| `rfcs/accepted/networking/0862-stoolap-data-sync.md`               | Sync RFC — references carriers        |
| `docs/research/stoolap-data-sync-via-cipherocto-network.md`        | Sync research (975 lines)             |
| `docs/research/social-platform-transport-patterns.md`              | Adapter patterns from 5 architectures |
| `missions/archived/0850e-dot-adapter-registry.md`                  | Adapter registry mission              |
| `missions/archived/0850i-dot-wasm-plugin-runtime.md`               | WASM plugin runtime mission           |
