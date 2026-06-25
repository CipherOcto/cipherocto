# RFC-0863: General-Purpose Network Integration — `octo-transport`

## Status

Accepted (2026-06-25) — Implemented v1.3: all 4 missions complete (0863a-d), 3 adversarial review rounds converged, 313 tests passing, 13/15 goals met.

## Authors

- Author: CipherOcto research

## Maintainers

- Maintainer: CipherOcto maintainers

## Summary

This RFC defines a general-purpose integration layer (`octo-transport`) that connects CipherOcto Network's 23 platform adapters to any consumer — sync engines, agent runtimes, marketplace services, proof distributors, and beyond. The layer provides a `NetworkSender` trait for outbound transport, a `PlatformAdapterBridge` that adapts `PlatformAdapter` into `NetworkSender`, and a `NodeTransport` configuration that any node can use to declare its transport stack declaratively. This RFC resolves the systemic gap identified in [Research: General-Purpose Network Integration](../../docs/research/multi-home-carrier-integration.md) where the network infrastructure (adapters, gateway, gossip, crypto) is built but no consumer can actually use it.

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

- `PlatformAdapter::send_envelope()` has no production caller
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
        // 3. Call self.adapter.send_envelope(&domain, &env).await
        // 4. Map PlatformAdapterError → TransportError
    }

    fn is_healthy(&self) -> bool { true }
}
```

#### `NodeTransport`

```rust
/// Declarative transport stack for any node.
pub struct NodeTransport {
    senders: Vec<Arc<dyn NetworkSender>>,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self { ... }

    /// Broadcast to all healthy transports (fan-out).
    pub async fn broadcast(&self, payload: &[u8], ctx: &SendContext) -> usize;

    /// Send to the best available transport (failover).
    pub async fn send_best(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError>;
}
```

### Lifecycle Requirements

No stateful actors in this RFC. `NetworkSender` is a stateless transport trait — it sends a payload and returns success/failure. Health tracking is delegated to `MultiCarrierSync`'s existing EMA-based health tracking (RFC-0862). `NodeTransport` holds a list of senders but maintains no state beyond the list itself.

### Roles and Authorities

| Role               | Identifier                                 | Authority Scope                         | Lifecycle                       | Source/Ref              |
| ------------------ | ------------------------------------------ | --------------------------------------- | ------------------------------- | ----------------------- |
| Transport Consumer | Any code calling `NetworkSender::send()`   | Send payloads through adapters          | Stateless — no persistent state | This RFC §Specification |
| Adapter Owner      | Operator who configures and loads adapters | Register adapters in `AdapterRegistry`  | Stateless — config at startup   | RFC-0850 §8             |
| Node Operator      | Operator running a CipherOcto node         | Configure `NodeTransport` with adapters | Stateless — config at startup   | This RFC §Specification |

**Out-of-scope roles:** Platform administrators (Telegram, Discord, etc.) manage their own platforms. This RFC does not define platform-level roles.

### Determinism Requirements

All operations are Class C (Probabilistic). The transport layer handles network I/O which is inherently non-deterministic. Determinism is preserved at the envelope level by `DeterministicEnvelope` (RFC-0850), not by the transport bridge.

| Operation                                | Class | Rationale                              |
| ---------------------------------------- | ----- | -------------------------------------- |
| `NetworkSender::send()`                  | C     | Network I/O is non-deterministic       |
| `NodeTransport::broadcast()`             | C     | Concurrent fan-out order varies        |
| `PlatformAdapterBridge::send()`          | C     | Adapter I/O timing varies              |
| `DeterministicEnvelope::to_wire_bytes()` | A     | Deterministic serialization (RFC-0850) |

### Error Handling

| Error                                  | Source                                                | Recovery                                       |
| -------------------------------------- | ----------------------------------------------------- | ---------------------------------------------- |
| `TransportError::AdapterFailure`       | `PlatformAdapter::send_envelope()` fails              | Failover to next transport in `NodeTransport`  |
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
  3. NodeTransport is now available to any consumer:
     - Sync engine calls node_transport.broadcast(wal_chunks)
     - Agent runtime calls node_transport.send_best(task_data)
     - Marketplace calls node_transport.broadcast(settlement)
     - Proof distributor calls node_transport.send_best(proof)
  4. DotGateway fan-out routes inbound envelopes to handlers
```

## Performance Targets

| Metric                  | Target     | Notes                                                              |
| ----------------------- | ---------- | ------------------------------------------------------------------ |
| Send latency overhead   | <5ms       | Bridge adds minimal overhead to adapter call                       |
| Fan-out to N transports | <2x single | Concurrent broadcast should not exceed 2x single-transport latency |
| Plugin load time        | <100ms     | `.so` loading via `libloading`                                     |
| Failover time           | <100ms     | Skip unhealthy, try next                                           |

## Implicit Assumptions Audit

| Assumption                                              | Where Relied Upon                     | Blast Radius if False             | Mitigation / Status                                                   |
| ------------------------------------------------------- | ------------------------------------- | --------------------------------- | --------------------------------------------------------------------- |
| PlatformAdapter implementations are thread-safe         | §Specification §PlatformAdapterBridge | Race conditions on shared adapter | All adapters implement `Send + Sync` (trait bound)                    |
| DeterministicEnvelope can be constructed from raw bytes | §Specification §PlatformAdapterBridge | Bridge cannot send any data       | Test vectors verify roundtrip; envelope format is stable per RFC-0850 |
| AdapterRegistry returns valid adapters                  | §Specification §Dynamic Loading Flow  | Bridge wraps null/broken adapters | `AdapterRegistry::get()` returns `None` for unhealthy adapters        |
| BroadcastDomainId is stable across restarts             | §Specification §PlatformAdapterBridge | Envelopes routed to wrong domains | BLAKE3-hashed, deterministic per RFC-0850 §5                          |
| Leaf workspace isolation is maintained                  | §Rationale                            | Circular dependencies break build | `octo-transport` depends on both; neither depends on it               |

### Categories to Audit

- **Platform trust:** Adapters trust platform APIs (Telegram, Discord, etc.). If a platform revokes access, the adapter fails. `NodeTransport` failover handles this.
- **Network partition:** During partitions, `NetworkSender::send()` returns errors. Consumers must handle retries.
- **Upgrade safety:** New adapters can be loaded at runtime via `.so` plugins without restart. ABI version check prevents incompatible adapters.
- **Configuration:** Adapter configs are passed at construction time. Misconfigured adapters fail health checks and are skipped.

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
| Carrier → PlatformAdapter in octo-network     | TCP joins DOT overlay naturally              | Requires circular dependency; conceptually backwards |
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

- [x] Implement `NetworkReceiver` for inbound dispatch
- [x] Complete `DotGateway` fan-out (implement adapter dispatch stub)
- [ ] Wire agent runtime to `NodeTransport` (deferred — runtime not implemented)
- [ ] Wire marketplace to `NodeTransport` (deferred — marketplace not implemented)
- [x] Add general-purpose transport tests

## Key Files to Modify

| File                                     | Change                         |
| ---------------------------------------- | ------------------------------ |
| `octo-transport/Cargo.toml`              | New crate manifest             |
| `octo-transport/src/lib.rs`              | New crate root                 |
| `octo-transport/src/sender.rs`           | `NetworkSender` trait          |
| `octo-transport/src/adapter_bridge.rs`   | `PlatformAdapterBridge`        |
| `octo-transport/src/node_transport.rs`   | `NodeTransport` config         |
| `crates/octo-network/src/dot/mod.rs:175` | DotGateway fan-out (Phase 3)   |
| `crates/octo-network/src/lib.rs`         | Export `sync` module (Phase 2) |

## Future Work

- F1: Priority routing in `NodeTransport` (QUIC for large payloads, Webhook for small)
- F2: Transport capability advertisement via GDP discovery
- F3: WASM plugin runtime integration (mission 0850i)
- F4: Transport-level encryption abstraction (beyond adapter-native encryption)
- F5: `AdapterFactory` hot-reload (add/remove adapters at runtime without restart)

## Rationale

The separate `octo-transport` crate follows the established leaf workspace pattern (`octo-determin`, `octo-sync`). It avoids circular dependencies, keeps both `octo-sync` and `octo-network` clean, and provides a reusable pattern that all 27+ use cases can adopt. The `NetworkSender` trait is deliberately simple (3 methods) — complex logic (health tracking, failover, crypto) lives in `NodeTransport` or existing modules.

## Version History

| Version | Date       | Changes                                                                       |
| ------- | ---------- | ----------------------------------------------------------------------------- |
| 1.0     | 2026-06-24 | Initial draft                                                                 |
| 1.1     | 2026-06-24 | Round 1 review: 11 fixes (roles, cross-refs, adversary analysis, terminology) |
| 1.2     | 2026-06-24 | Round 2 review: 1 fix (typo) — 0 findings, loop closed                        |
| 1.3     | 2026-06-25 | Accepted: all 4 missions complete, 3 adversarial review rounds (18 findings fixed), 313 tests, 13/15 goals met |

## Related RFCs

- RFC-0850: Deterministic Overlay Transport (DOT) — defines `PlatformAdapter`, `DeterministicEnvelope`
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
| `PlatformAdapter::send_envelope()`       | 23 implementations                                | NO PRODUCTION CALLER | ✅ Called via bridge    |
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
