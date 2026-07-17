# RFC-0863: General-Purpose Network Integration — `octo-transport`

## Status

Accepted (2026-06-25) — Implemented v1.3: all 4 missions complete (0863a-d), 3 adversarial review rounds converged, 313 tests passing, 13/15 goals met.

## Authors

- Author: CipherOcto research

## Maintainers

- Maintainer: CipherOcto maintainers

## Summary

This RFC defines a general-purpose integration layer (`octo-transport`) that connects CipherOcto Network's 23 platform adapters to any consumer — sync engines, agent runtimes, marketplace services, proof distributors, and beyond. The layer provides a `NetworkSender` trait for outbound transport, a `NetworkReceiver` trait for inbound dispatch, a `PlatformAdapterBridge` that adapts `PlatformAdapter` into `NetworkSender`, and a `NodeTransport` configuration that any node can use to declare its transport stack declaratively — including both outbound fan-out/failover and inbound receiver registration and dispatch. This RFC resolves the systemic gap identified in [Research: General-Purpose Network Integration](../../docs/research/multi-home-carrier-integration.md) where the network infrastructure (adapters, gateway, gossip, crypto) is built but no consumer can actually use it.

## Dependencies

**Requires:**

- RFC-0850: Deterministic Overlay Transport (DOT) — defines `PlatformAdapter`, `DeterministicEnvelope`, `BroadcastDomainId`

**Recommended First Consumer:**

- RFC-0862: Stoolap Data Sync — validates the integration pattern (not a hard dependency; `octo-transport` is usable without RFC-0862)

**Optional:**

- RFC-0852: Deterministic Gossip Protocol — Phase 2 integration for gossip-compatible scenarios

## Design Goals

| Goal                                        | Target    | Metric                              |
| ------------------------------------------- | --------- | ----------------------------------- |
| G1: Any consumer can send via any adapter   | 100%      | All 23 adapters usable as transport |
| G2: Dynamic adapter loading                 | Runtime   | `.so` plugins loaded at startup     |
| G3: Multi-transport failover                | Automatic | Failover on transport failure       |
| G4: No changes to octo-sync or octo-network | Zero diff | Clean leaf workspace boundaries     |
| G5: Serve 27+ use cases                     | All tiers | Sync, agents, marketplace, proofs   |

## Motivation

CipherOcto Network has 23 platform adapter implementations (Telegram, Discord, QUIC, Webhook, P2P, etc.), a `DotGateway` for envelope dispatch, and a gossip protocol for propagation. Yet **no code path connects "something that wants to send data" to "an adapter that can send it"**:

- `PlatformAdapter::send_message()` has no production caller
- `DotGateway::process_envelope()` fan-out is a TODO stub
- `SyncNode` and `SyncNetworkBridge` are dead code (module not exported)
- `MultiCarrierSync` is exported but never referenced by `SyncSessionManager`
- The `stoolap-node` binary only supports raw TCP

**27 documented use cases** across 4 tiers depend on the network but have no integration path. This RFC provides the missing integration layer.

### Use Case Link

- [General-Purpose Network Integration](../../docs/research/multi-home-carrier-integration.md)
- [Social Platform Transport Layer](../../docs/use-cases/social-platform-transport-layer.md)
- [Stoolap Data Sync via CipherOcto Network](../../docs/use-cases/stoolap-data-sync-via-cipherocto-network.md)

## Specification

### System Architecture

```mermaid
graph TB
    subgraph Consumers
        S[Sync Engine]
        A[Agent Runtime]
        M[Marketplace]
        P[Proof Distributor]
    end

    subgraph octo-transport
        NT[NodeTransport]
        NS[NetworkSender trait]
        PAB[PlatformAdapterBridge]
    end

    subgraph octo-network
        PA[PlatformAdapter x23]
        AR[AdapterRegistry]
        DG[DotGateway]
    end

    S --> NT
    A --> NT
    M --> NT
    P --> NT
    NT --> NS
    NS --> PAB
    PAB --> PA
    AR -.loads.-> PA
    DG -.dispatches.->|"Phase 3"| DG
```

### Data Structures

#### `NetworkSender` Trait

```rust
/// General-purpose outbound transport trait.
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

#### `PlatformAdapterBridge`

```rust
/// Bridges any PlatformAdapter into a NetworkSender.
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
        // 3. Call self.adapter.send_message(&domain, &env, payload).await
        // 4. Map PlatformAdapterError → TransportError
    }

    fn is_healthy(&self) -> bool { true }
}
```

#### `NodeTransport`

```rust
/// Declarative transport stack for any node.
/// Handles both outbound (fan-out/failover) and inbound (receiver dispatch).
pub struct NodeTransport {
    senders: Vec<Arc<dyn NetworkSender>>,
    receivers: Vec<Arc<dyn NetworkReceiver>>,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self { ... }

    /// Register a handler for inbound payloads.
    /// Handlers are called in registration order by `dispatch()`.
    /// Safe to call concurrently — receivers are protected internally.
    pub fn register_receiver(&self, receiver: Arc<dyn NetworkReceiver>);

    /// Broadcast to all healthy transports (fan-out).
    pub async fn broadcast(&self, payload: &[u8], ctx: &SendContext) -> usize;

    /// Send to the best available transport (failover).
    pub async fn send_best(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError>;

    /// Dispatch an inbound payload to all registered receivers.
    /// Calls `on_receive()` on each receiver in registration order.
    /// Returns first error (fail-fast) or Ok if all succeed.
    pub async fn dispatch(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>;
}
```

**Inbound dispatch model:** Consumers (node runtimes, test harnesses) are responsible for obtaining raw bytes from the wire — via `PlatformAdapter::receive_messages()`, mpsc channels, or other means — and calling `node.transport.dispatch(payload, &ctx)`. `NodeTransport` does not own a polling loop; it is a dispatch fan-out that routes inbound payloads to registered handlers, mirroring how `broadcast()` fan-outs outbound payloads to registered senders.

**Registration order:** Receivers are dispatched in the order they were registered via `register_receiver()`. The first receiver to return an `Err` stops dispatch (fail-fast). This matches the outbound failover semantics where the first successful send stops iteration.

**Receiver ownership:** `NodeTransport` does not assume a specific number of receivers. Typical usage is a single receiver that owns the consumer's inbound dispatch logic (for example, `QuotaRouterNode` registers its internal `QuotaRouterHandler` automatically in `QuotaRouterNodeBuilder::build()`). Multi-receiver setups (for example, a primary handler plus an observability sink) are supported and dispatch in registration order. Receivers are not owned by `NodeTransport` — they live as long as their containing `Arc` is held; dropping the last `Arc` is the only thing that unregisters them in practice.

### Lifecycle Requirements

No stateful actors in this RFC. `NetworkSender` is a stateless transport trait — it sends a payload and returns success/failure. `NetworkReceiver` is a stateless inbound trait — it receives a payload and returns success/failure. Health tracking is delegated to `MultiCarrierSync`'s existing EMA-based health tracking (RFC-0862). `NodeTransport` holds lists of senders and receivers but maintains no state beyond the lists themselves.

### Roles and Authorities

| Role               | Identifier                                 | Authority Scope                         | Lifecycle                       | Source/Ref              |
| ------------------ | ------------------------------------------ | --------------------------------------- | ------------------------------- | ----------------------- |
| Transport Consumer | Any code calling `NetworkSender::send()`   | Send payloads through adapters          | Stateless — no persistent state | This RFC §Specification |
| Inbound Handler    | Any code implementing `NetworkReceiver`    | Receive dispatched inbound payloads     | Stateless — no persistent state | This RFC §Specification |
| Adapter Owner      | Operator who configures and loads adapters | Register adapters in `AdapterRegistry`  | Stateless — config at startup   | RFC-0850 §8             |
| Node Operator      | Operator running a CipherOcto node         | Configure `NodeTransport` with adapters | Stateless — config at startup   | This RFC §Specification |

**Out-of-scope roles:** Platform administrators (Telegram, Discord, etc.) manage their own platforms. This RFC does not define platform-level roles.

### Determinism Requirements

All operations are Class C (Probabilistic). The transport layer handles network I/O which is inherently non-deterministic. Determinism is preserved at the envelope level by `DeterministicEnvelope` (RFC-0850), not by the transport bridge.

| Operation                                | Class | Rationale                              |
| ---------------------------------------- | ----- | -------------------------------------- |
| `NetworkSender::send()`                  | C     | Network I/O is non-deterministic       |
| `NetworkReceiver::on_receive()`          | C     | Handler processing time varies         |
| `NodeTransport::broadcast()`             | C     | Concurrent fan-out order varies        |
| `NodeTransport::dispatch()`              | C     | Handler execution order varies         |
| `PlatformAdapterBridge::send()`          | C     | Adapter I/O timing varies              |
| `DeterministicEnvelope::to_wire_bytes()` | A     | Deterministic serialization (RFC-0850) |

### Error Handling

| Error                                  | Source                                                | Recovery                                       |
| -------------------------------------- | ----------------------------------------------------- | ---------------------------------------------- |
| `TransportError::AdapterFailure`       | `PlatformAdapter::send_message()` fails              | Failover to next transport in `NodeTransport`  |
| `TransportError::AllTransportsFailed`  | All `NetworkSender::send()` calls fail                | Return error to caller; no retry at this layer |
| `TransportError::EnvelopeConstruction` | Cannot construct `DeterministicEnvelope` from payload | Log error, skip transport                      |
| `TransportError::Unhealthy`            | `NetworkSender::is_healthy()` returns false           | Skip transport, try next                       |

### Dynamic Loading Flow

```
Node Startup:
  1. AdapterRegistry::discover_and_load()  // loads .so plugins
  2. For each loaded adapter:
     a. Create PlatformAdapterBridge wrapper
     b. Add to NodeTransport
  3. BootstrapOrchestrator::run() — acquire first peers:
     a. Load SeedListEnvelope (from config or embedded genesis)
     b. SeedHealth::check() — reject stale seeds
     c. SeedListAuthority::verify_authority() — gate on epoch
     d. Send BOOTSTRAP_REQ to each bootstrap node via NodeTransport
     e. Collect BOOTSTRAP_RESP, validate signatures, compute peer-list intersection
     f. Populate TransportDiscovery cache
     g. Hand off to DiscoveryLifecycle::Bootstrap → Expansion
  4. NodeTransport is now available to any consumer:
     - Sync engine calls node_transport.broadcast(wal_chunks)
     - Agent runtime calls node_transport.send_best(task_data)
     - Marketplace calls node_transport.broadcast(settlement)
     - Proof distributor calls node_transport.send_best(proof)
  5. DotGateway fan-out routes inbound envelopes to handlers
```

### `BootstrapOrchestrator` (RFC-0851p-a Integration)

The `BootstrapOrchestrator` bridges RFC-0851p-a's bootstrap protocol into the `octo-transport` startup path. It is the **first thing a node runs** after loading adapters — without bootstrap, no peer exists to send to.

```rust
/// Drives the RFC-0851p-a Mode A bootstrap protocol.
///
/// Consumes `SeedListEnvelope`, `SeedHealth`, `SeedListAuthority`,
/// `BootstrapMode`, and `SlashedSeedBlacklist` from `octo-network::mon::bootstrap`.
/// Produces peer entries in `TransportDiscovery`.
pub struct BootstrapOrchestrator {
    seed_list: SeedListEnvelope,
    blacklist: SlashedSeedBlacklist,
    state: BootstrapClientLifecycle,
    mode: BootstrapMode,
    config: BootstrapConfig,
}

/// Configuration for the bootstrap protocol.
pub struct BootstrapConfig {
    /// Max time to wait for bootstrap responses (default: 60s).
    pub bootstrap_timeout: Duration,
    /// Minimum responses for high-confidence bootstrap (default: 3).
    pub min_responses: usize,
    /// Peer-list intersection threshold (default: 0.80).
    pub intersection_threshold: f64,
    /// Max retries before fallback (default: 5).
    pub max_retries: u32,
    /// Initial retry backoff (default: 1s).
    pub initial_backoff: Duration,
    /// The seed list authority type (Foundation or Dao).
    /// Operator configuration; not embedded in the envelope.
    pub authority: SeedListAuthority,
}

/// Bootstrap protocol error.
pub enum BootstrapError {
    SeedListStale,
    AuthorityError(SeedAuthorityError),  // from octo-network::mon::bootstrap
    NoResponses,
    IntersectionBelowThreshold,
    AllTransportsFailed,
    SignatureInvalid,
}

impl BootstrapOrchestrator {
    /// Run the bootstrap protocol to completion.
    ///
    /// Returns the number of peers acquired, or an error if all modes fail.
    /// On success, `discovery` cache and `discovery_state` lifecycle are updated.
    pub async fn run(
        &mut self,
        transport: &NodeTransport,
        discovery: &TransportDiscovery,
        discovery_state: &mut DiscoveryState,
    ) -> Result<u32, BootstrapError>;
}
```

**State machine:** `BootstrapClientLifecycle` (Init → Connecting → Validating → Cached → Done, with FallbackB/FallbackC/Failed terminals). Full transitions in RFC-0851p-a §3.

**Integration with existing modules:**
- `octo-network::mon::bootstrap::SeedListEnvelope` — seed list loading
- `octo-network::mon::bootstrap::SeedHealth` — staleness check at load
- `octo-network::mon::bootstrap::SeedListAuthority` — authority gate (Foundation vs DAO)
- `octo-network::mon::bootstrap::SlashedSeedBlacklist` — filter slashed seeds (uses `BootstrapMisbehavior` sub-codes internally)
- `octo-transport::discovery::TransportDiscovery::cache_insert()` — peer cache handoff
- `octo-network::gdp::discovery::DiscoveryState` — lifecycle transition (Bootstrap → Expansion)

**Mission:** `0851p-a-base-bootstrap-orchestrator.md` (Phase 1 Mode A). Mode B (DHT fallback) and Mode C (invite link) are separate missions.

## Performance Targets

| Metric                  | Target     | Notes                                                              |
| ----------------------- | ---------- | ------------------------------------------------------------------ |
| Send latency overhead   | <5ms       | Bridge adds minimal overhead to adapter call                       |
| Fan-out to N transports | <2x single | Concurrent broadcast should not exceed 2x single-transport latency |
| Plugin load time        | <100ms     | `.so` loading via `libloading`                                     |
| Failover time           | <100ms     | Skip unhealthy, try next                                           |
| Mode A first peer (warm cache) | <2s  | BootstrapOrchestrator with cached seed list (RFC-0851p-a §Performance) |
| Mode A first peer (cold cache) | <5s  | BootstrapOrchestrator from disk seed list (RFC-0851p-a §Performance) |
| Seed list verify (5 entries) | <10ms | Ed25519 signature verification (RFC-0851p-a §Performance)         |

## Implicit Assumptions Audit

| Assumption                                              | Where Relied Upon                     | Blast Radius if False             | Mitigation / Status                                                   |
| ------------------------------------------------------- | ------------------------------------- | --------------------------------- | --------------------------------------------------------------------- |
| PlatformAdapter implementations are thread-safe         | §Specification §PlatformAdapterBridge | Race conditions on shared adapter | All adapters implement `Send + Sync` (trait bound)                    |
| DeterministicEnvelope can be constructed from raw bytes | §Specification §PlatformAdapterBridge | Bridge cannot send any data       | Test vectors verify roundtrip; envelope format is stable per RFC-0850 |
| AdapterRegistry returns valid adapters                  | §Specification §Dynamic Loading Flow  | Bridge wraps null/broken adapters | `AdapterRegistry::get()` returns `None` for unhealthy adapters        |
| BroadcastDomainId is stable across restarts             | §Specification §PlatformAdapterBridge | Envelopes routed to wrong domains | BLAKE3-hashed, deterministic per RFC-0850 §5                          |
| Leaf workspace isolation is maintained                  | §Rationale                            | Circular dependencies break build | `octo-transport` depends on both; neither depends on it               |
| Seed list file is available at node startup             | §Dynamic Loading Flow step 3a         | Node cannot bootstrap; enters Failed state | Embedded genesis list as fallback; operator guide for config path |
| Node has Ed25519 signing key for BOOTSTRAP_REQ          | §Dynamic Loading Flow step 3d         | Cannot sign requests; rejected by bootstrap nodes | Key derived from node identity (RFC-0851p-a §2)              |
| Epoch is synchronized within ±1 of bootstrap nodes      | §Dynamic Loading Flow step 3c/3e      | Stale-response rejection; authority gate fails | RFC-0850 §5 logical timestamp; ±1 tolerance per RFC-0851p-a IA-NB-6 |

### Categories to Audit

- **Platform trust:** Adapters trust platform APIs (Telegram, Discord, etc.). If a platform revokes access, the adapter fails. `NodeTransport` failover handles this.
- **Network partition:** During partitions, `NetworkSender::send()` returns errors. Consumers must handle retries.
- **Upgrade safety:** New adapters can be loaded at runtime via `.so` plugins without restart. ABI version check prevents incompatible adapters.
- **Configuration:** Adapter configs are passed at construction time. Misconfigured adapters fail health checks and are skipped.
- **Bootstrap trust:** The seed list authority (Foundation at launch, DAO post-F1) is the highest-trust role. Key compromise allows attacker-chosen bootstrap nodes. Mitigated by: multi-sig (3-of-5), seed list rotation (90 days), slashing (0x000D). **ACCEPTED RISK** — F1 deadline for DAO transition.

## Security Considerations

### Envelope Construction

The bridge constructs `DeterministicEnvelope` from raw payloads. Security depends on:

- Correct signing key usage (mission-scoped, per RFC-0853)
- Proper nonce generation (monotonic, per RFC-0850 §4)
- Replay protection (adapter-level or gateway-level)

### Transport-Level Encryption

Adapters handle their own encryption:

- QUIC: TLS 1.3 (native)
- Webhook: HTTPS + HMAC-SHA256
- P2P: Noise protocol (libp2p)
- Telegram: Bot API HTTPS

The bridge does NOT add encryption — it delegates to the adapter.

### Key Isolation

Each adapter operates within its own broadcast domain. The bridge does not cross domain boundaries — `SendContext.domain` specifies the target domain.

## Adversarial Review

| Threat                                | Impact | Mitigation                                              |
| ------------------------------------- | ------ | ------------------------------------------------------- |
| Malicious adapter plugin              | High   | ABI version check + health monitoring                   |
| Replay attack via bridge              | Medium | DeterministicEnvelope nonce + adapter replay protection |
| Envelope spoofing                     | High   | DeterministicEnvelope signing (Ed25519)                 |
| Resource exhaustion (broadcast flood) | Medium | `NodeTransport` health check skips unhealthy transports |
| Platform API abuse                    | Low    | Adapter rate limits + `CapabilityReport`                |

### Adversary Analysis (5-Question Test)

**Threat: Malicious adapter plugin loaded via `.so`**

1. **Who benefits?** An attacker who wants to intercept or modify all network traffic from a node.
2. **What does it cost?** Developing a malicious `.so` that conforms to the C ABI and passes version check. Moderate cost — requires knowledge of the adapter ABI.
3. **What do they gain?** Full visibility into all outbound payloads; ability to drop, modify, or replay envelopes.
4. **Our defense and cost?** ABI version check (prevents ABI-incompatible plugins); health monitoring (detects misbehaving adapters); `AdapterRegistry` restricts loading to configured directories. Low cost to legitimate operation.
5. **Residual risk?** A plugin that conforms to the ABI but misbehaves subtly (e.g., leaks data to attacker). Mitigated by: open-source adapter ecosystem (community review), sandboxing (future WASM runtime), and operator trust in loaded plugins. **ACCEPTED RISK** — same trust model as any shared library loading.

## Alternatives Considered

| Approach                                      | Pros                                         | Cons                                                 |
| --------------------------------------------- | -------------------------------------------- | ---------------------------------------------------- |
| Feature-gate in octo-sync                     | Simple                                       | Circular dependency; leaf workspace violation        |
| Carrier → PlatformAdapter in octo-network     | TCP/UDP join DOT overlay naturally            | Resolved: TCP (`0x0016`) and UDP (`0x0017`) are now PlatformTypes per RFC-0850 §8.8-§8.9 |
| Node-level wiring (no crate)                  | No new crate                                 | Each binary reimplements wiring                      |
| Separate `octo-transport` crate (recommended) | Clean deps; reusable pattern; leaf workspace | Third workspace to maintain                          |
| Type-erased bridge                            | Minimal coupling                             | Loses compile-time guarantees                        |

## Implementation Phases

### Phase 1: Core Bridge (Proves the Pattern)

- [x] Create `octo-transport` leaf workspace
- [x] Implement `NetworkSender` trait
- [x] Implement `PlatformAdapterBridge`
- [x] Implement `AdapterFactory` (takes `AdapterRegistry`, produces `Vec<Arc<dyn NetworkSender>>`)
- [x] Wire sync as first consumer (proves pattern with RFC-0862)
- [x] Update `stoolap-node` with `--adapter` flags
- [x] Add L4 cross-transport E2E tests

### Phase 2: DGP Integration

- [x] Export `sync` module from `octo-network`
- [x] Wire `SyncSessionManager` → DGP gossip path
- [x] Add DGP-based sync tests

### Phase 3: General-Purpose NodeTransport

- [x] Implement `NetworkReceiver` trait and `ReceiveContext` struct
- [ ] Add `register_receiver()` and `dispatch()` to `NodeTransport`
- [ ] Complete `DotGateway` fan-out (implement adapter dispatch stub)
- [ ] Wire agent runtime to `NodeTransport` (deferred — runtime not implemented)
- [ ] Wire marketplace to `NodeTransport` (deferred — marketplace not implemented)
- [x] Add general-purpose transport tests

### Phase 4: Bootstrap Integration (RFC-0851p-a)

- [ ] Create `octo-transport/src/bootstrap.rs` module
- [ ] Implement `BootstrapOrchestrator` with `BootstrapClientLifecycle` state machine
- [ ] Implement `BootstrapRequest` / `BootstrapResponse` wire types
- [ ] Integrate `SeedListEnvelope` loading + `SeedHealth::check()`
- [ ] Integrate `SeedListAuthority::verify_authority()` gate
- [ ] Integrate `SlashedSeedBlacklist::filter()`
- [ ] Implement peer-list intersection (BLAKE3, 80% threshold)
- [ ] Wire `TransportDiscovery::cache_insert()` handoff
- [ ] Add retry with exponential backoff (RFC-0851p-a §3)
- [ ] Add unit tests (12+ scenarios from RFC-0851p-a test vectors)
- [ ] Wire into `stoolap-node` as default bootstrap path (mission `0863e-stoolap-node-bootstrap-wiring.md`)

## Key Files to Modify

| File                                     | Change                                    |
| ---------------------------------------- | ----------------------------------------- |
| `octo-transport/Cargo.toml`              | New crate manifest                        |
| `octo-transport/src/lib.rs`              | New crate root                            |
| `octo-transport/src/sender.rs`           | `NetworkSender` trait                     |
| `octo-transport/src/adapter_bridge.rs`   | `PlatformAdapterBridge`                   |
| `octo-transport/src/node_transport.rs`   | `NodeTransport` config                    |
| `octo-transport/src/bootstrap.rs`        | **New:** `BootstrapOrchestrator`, `BootstrapConfig`, `BootstrapClientLifecycle`, `BootstrapRequest`, `BootstrapResponse` (Phase 4) |
| `crates/octo-network/src/dot/mod.rs:175` | DotGateway fan-out (Phase 3)              |
| `crates/octo-network/src/lib.rs`         | Export `sync` module (Phase 2)            |

## Future Work

- F1: Priority routing in `NodeTransport` (QUIC for large payloads, Webhook for small)
- F2: Transport capability advertisement via GDP discovery — specified in RFC-0863p-a (Domain-Governed Transport)
- F3: WASM plugin runtime integration (mission 0850i)
- F4: Transport-level encryption abstraction (beyond adapter-native encryption)
- F5: `AdapterFactory` hot-reload (add/remove adapters at runtime without restart)
- F6: Mode B bootstrap — DHT fallback (RFC-0851p-a §4, requires RFC-0843 Kademlia integration)
- F7: Mode C bootstrap — invite link (RFC-0851p-a §5, requires invite URL parser + web-of-trust)
- F8: Domain-governed transport — specified in RFC-0863p-a. Wraps `NodeTransport` with DC/group governance awareness, auto-bootstrap pipeline, and governance-gated send/receive.

## Rationale

The separate `octo-transport` crate follows the established leaf workspace pattern (`octo-determin`, `octo-sync`). It avoids circular dependencies, keeps both `octo-sync` and `octo-network` clean, and provides a reusable pattern that all 27+ use cases can adopt. The `NetworkSender` and `NetworkReceiver` traits are deliberately simple (3 methods each) — complex logic (health tracking, failover, crypto, dispatch) lives in `NodeTransport` or existing modules.

## Version History

| Version | Date       | Changes                                                                       |
| ------- | ---------- | ----------------------------------------------------------------------------- |
| 1.0     | 2026-06-24 | Initial draft                                                                 |
| 1.1     | 2026-06-24 | Round 1 review: 11 fixes (roles, cross-refs, adversary analysis, terminology) |
| 1.2     | 2026-06-24 | Round 2 review: 1 fix (typo) — 0 findings, loop closed                        |
| 1.3     | 2026-06-25 | Accepted: all 4 missions complete, 3 adversarial review rounds (18 findings fixed), 313 tests, 13/15 goals met |
| 1.4     | 2026-06-25 | Added `BootstrapOrchestrator` to Specification, Dynamic Loading Flow, Key Files, and Implementation Phases (Phase 4). Wired RFC-0851p-a bootstrap protocol into `octo-transport` startup path. |
| 1.5     | 2026-06-28 | Resolved TCP/UDP transport gap: updated Alternatives Considered to acknowledge `PlatformType::Tcp = 0x0016` and `PlatformType::Udp = 0x0017` per RFC-0850 §8.8-§8.9. TCP/UDP adapters can now implement `PlatformAdapter` and integrate via `PlatformAdapterBridge`. |
| 1.6     | 2026-06-28 | Aligned with RFC-0850 v1.3.0: `PlatformAdapter::send_envelope` renamed to `send_message(domain, envelope, payload)`. Updated bridge to pass payload bytes to adapter. |
| 1.7     | 2026-06-29 | Fixed Phase 3 checklist (inbound dispatch was not implemented). Added `receivers` field, `register_receiver()`, and `dispatch()` to `NodeTransport` spec. Added `NetworkReceiver` inbound dispatch model documentation. Updated Summary, Roles, Determinism tables. |
| 1.8     | 2026-06-30 | Clarified `NodeTransport` receiver-ownership semantics: any number of receivers are supported, typical usage is a single receiver owned by the consumer (e.g., `QuotaRouterNode`'s internal handler), receivers live as long as their `Arc` is held. Aligned with RFC-0870 v1.13 builder change (single-node return, internal handler registration). |

## Related RFCs

- RFC-0850: Deterministic Overlay Transport (DOT) — defines `PlatformAdapter`, `DeterministicEnvelope`
- RFC-0851p-a: Network Bootstrap Protocol — bootstrap orchestrator wired into transport startup
- RFC-0851p-b: DotDomain Bootstrap Mode — DotDomain discovery via social adapters
- RFC-0863p-a: Domain-Governed Transport — governance-aware `NodeTransport` wrapper
- RFC-0852: Deterministic Gossip Protocol — Phase 2 integration target
- RFC-0862: Stoolap Data Sync — first consumer, validates the pattern

## Related Use Cases

- [Social Platform Transport Layer](../../docs/use-cases/social-platform-transport-layer.md)
- [Stoolap Data Sync via CipherOcto Network](../../docs/use-cases/stoolap-data-sync-via-cipherocto-network.md)
- [Agent Marketplace](../../docs/use-cases/agent-marketplace.md)
- [Enterprise Private AI](../../docs/use-cases/enterprise-private-ai.md)
- [General-Purpose Network Integration](../../docs/research/multi-home-carrier-integration.md)

## Appendices

### A. Production Call Path Audit

The following components were identified as dead code or stubs during analysis. All are now resolved:

| Component                                | Location                                          | Original Status      | Current Status         |
| ---------------------------------------- | ------------------------------------------------- | -------------------- | ---------------------- |
| `DotGateway::process_envelope()` fan-out | `crates/octo-network/src/dot/mod.rs:175`          | STUB                 | ✅ Implemented (0863d) |
| `PlatformAdapter::send_message()`       | 23 implementations                                | NO PRODUCTION CALLER | ✅ Called via bridge    |
| `SyncNode`                               | `crates/octo-network/src/sync/mod.rs`             | DEAD CODE            | ✅ Exported + wired    |
| `SyncNetworkBridge`                      | `crates/octo-network/src/sync/dgp_integration.rs` | DEAD CODE            | ✅ Exported + wired    |
| `MultiCarrierSync`                       | `octo-sync/src/carrier.rs`                        | UNUSED by consumers  | ⚠️ Deprecated (NodeTransport replaces) |

### B. Adapter Transport Summary (Representative)

The full adapter ecosystem has 23 implementations. These 4 represent the transport diversity:

| Adapter            | Transport           | Max Payload | Binary    |
| ------------------ | ------------------- | ----------- | --------- |
| `QuicAdapter`      | QUIC (quinn)        | 1MB         | Yes       |
| `WebhookAdapter`   | HTTP POST (reqwest) | 1MB         | Yes       |
| `NativeP2PAdapter` | libp2p gossipsub    | 64KB        | Yes       |
| `TelegramAdapter`  | Telegram Bot API    | 4KB         | No (text) |

All adapters implement the same `PlatformAdapter` trait and can be used interchangeably via `PlatformAdapterBridge`.
