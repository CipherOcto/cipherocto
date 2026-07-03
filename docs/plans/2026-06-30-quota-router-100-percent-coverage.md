# 100% Coverage of Production Code — Quota Router Network

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Test every production code path (every function body that runs in deployment) by exercising it through tests that run production code. No parallel fixtures, no theatrical tests, no opt-outs. Coverage ratio: production code lines executed by any test / total production code lines = **100%**.

**Architecture (unified):** A single library `crates/quota-router-core/` exposes the full routing surface — HTTP proxy, provider integration (native_http / py_bridge), routing strategies, and the mesh networking layer (`QuotaRouterNode`, gossip, forward, handler). Two production consumers:

```
                            ┌──────────────────────────────┐
                            │   crates/quota-router-core    │
                            │   (the library, single src)   │
                            │                                │
   requests come in          │   ┌────────────────────────┐  │
   via one of three          │   │  HTTP proxy (RFC-0917) │  │
   consumers:                │   │  provider integration  │  │
                            │   │  routing strategies    │  │
   ┌────────────────────┐    │   │  QuotaRouterNode mesh  │◄─┤── unified: same routing
   │ quota-router-cli    │───►│   │  gossip / forward /    │  │   logic for all three
   │   (binary)         │    │   │  scorer / handler      │  │   consumers
   └────────────────────┘    │   └────────────────────────┘  │
   ┌────────────────────┐    │                                │
   │ quota-router-pyo3   │───►│  Python SDK ← same routing ──►│
   │   (PyO3 binding)   │    │                                │
   └────────────────────┘    └──────────────────────────────┘
   ┌────────────────────┐
   │ HTTP clients        │───►  HTTP proxy listener (port)  │
   │   (any)            │    │                                │
   └────────────────────┘    │                                │
                            │  routing outcomes:              │
                            │  ├── Local provider? Forward   │
                            │  │   directly (native_http or  │
                            │  │   py_bridge)                │
                            │  └── Remote peer provider?     │
                            │      Forward via mesh          │
                            └──────────────────────────────┘
```

A request enters through one of three ingress paths (HTTP proxy listener, CLI subcommand, Python SDK call). All three reach the same routing logic in `quota-router-core`. The routing logic decides local-direct vs mesh-forward using `QuotaRouterNode`. Tests across all four layers (unit, in-process, cross-process TCP, cross-host Docker) exercise this same library surface.

**Tech Stack:** Rust (tokio, async-trait, hyper, clap), Docker (compose v2), `quota-router-core`, `octo-transport`, `octo-network`, `crates/octo-adapter-tcp`.

---

## 1. Non-negotiable invariants

These constraints are fixed and must not be relaxed in any task:

1. **No workspace member, directory, file, dependency, or identifier named `quota-router-node`.** The three words `quota-router-node` in sequence (anywhere in `crates/`, `bin/`, target names, dependency identifiers, doc filenames, anywhere in the repo) are forbidden. They cause real-world name-clash confusion. The CLI binary is `quota-router`; the CLI's long-running subcommand is `serve`; no separate daemon binary exists.
2. **`crates/quota-router-core/` IS the production library.** It is the only canonical home for production code in the `quota-router` product. Its consumers are exactly two: `crates/quota-router-cli/` (binary) and `crates/quota-router-pyo3/` (Python binding). No third production consumer exists.
3. **Node abstraction lives inside `quota-router-core`.** `QuotaRouterNode`, `QuotaRouterHandler`, gossip, forward, scorer — all internal to `quota-router-core`. Callers do not import a separate node crate. The name stays `quota-router-core` (the network/mesh features are an extension of the existing core, not a separate product).
4. **No production code is ever placed inside docker or test infrastructure.** All Docker artifacts (`Dockerfile`, `compose.yaml`, `entrypoint.sh`, mocks, fakes) live under test directories. Production code lives under `crates/*` (workspace members) and `octo-*` (workspace members).
5. **Every test runs production code.** No parallel implementations of production logic in test files. Mocks exist only at boundaries with external dependencies (real LLM providers, real third-party APIs). The mesh production code is the same code the tests exercise — same crate, same binaries.
6. **Multi-process / multi-host testing uses real processes.** Docker containers are real processes on real Linux kernels with real TCP and real sockets. They share the binary built from `crates/quota-router-cli/`. They are NOT mocks.
7. **The CLI binary is the docker daemon.** Docker tests use `quota-router serve --mock-provider`, not a separate `quota-router-node` process. The CLI is the one executable.

---

## 2. Architectural context — the unified flow

### 2.1 Three ingress paths, one routing library

A request enters the `quota-router` product through one of three paths and is processed by the same library:

| Ingress | Mechanism | In `quota-router-core` |
|---|---|---|
| HTTP client → HTTP proxy | hyper listens on a configurable port | `proxy.rs` |
| Python SDK call | PyO3 binding exposes `route()` / `serve()` to Python | `quota-router-pyo3` → `core::route` |
| CLI subcommand | clap parses args; CLI calls into core | `quota-router-cli` → `core::route` / `core::serve` |

All three reach the same routing entry points in `core`: `route(ctx, payload)` and `serve(config) -> !` (the long-running daemon mode for the mesh).

### 2.2 The "gap" that produced two codebases

The split between `crates/quota-router-core/` (HTTP proxy only) and root `quota-router/` (mesh only) was an implementation gap — both halves describe parts of the same product, but the integration between them was missing, so they ended up as separate (and the mesh layer ended up excluded from workspace). The plan corrects this:

- All mesh code (root `quota-router/src/{lib,handler,forward,gossip,scorer,announce,request,provider,metrics,ratelimit}.rs`) migrates into `crates/quota-router-core/src/node/`.
- `proxy.rs` continues to exist at `crates/quota-router-core/src/proxy.rs`, importing node types from `crate::node`.
- The router.rs routing strategies remain in `crates/quota-router-core/src/router.rs`, available for both single-proxy use (no mesh) and mesh-augmented use.

### 2.3 Routing outcomes

Once a request reaches the routing layer in core, exactly two outcomes occur:

```
                ┌──────────────────────────────┐
   request ────►│  Router (router.rs)            │
                │  selects destination            │
                └──────────────────────────────┘
                          │
            ┌─────────────┴────────────┐
            ▼                          ▼
   ┌──────────────────┐       ┌──────────────────┐
   │ Local Provider    │       │ Remote peer      │
   │ (native_http or   │       │ provider          │
   │ py_bridge)        │       │                  │
   └──────────────────┘       └──────────────────┘
            │                          │
            ▼                          ▼
        respond                  forward via
        directly to              QuotaRouterNode
        caller                   mesh (forward.rs,
                                  gossip.rs, etc.)
```

Both outcomes flow through `QuotaRouterNode::route(...)` (or equivalent internal entrypoint). Tests at every layer exercise both branches.

### 2.4 The mesh layer in production

When `quota-router` is configured for multi-node operation (CLI flag, config file entry, or environment), `core::serve(config)` constructs:

- A `QuotaRouterNode` with the configured providers, peers, network key
- A `NodeTransport` wrapping a `TcpAdapter` (or `InMemoryChannelAdapter` for tests) via `PlatformAdapterBridge`
- A background driver task polling `TcpAdapter::receive_messages` and feeding `node.receive(payload, ctx)` for inbound dispatch

This is the daemon mode the CLI's `serve` subcommand runs.

### 2.5 What existed before (gap state)

| Crate | Had | Lacked |
|---|---|---|
| `crates/quota-router-core/` | HTTP proxy, routing strategies, providers, auth, balance, storage | Mesh layer (no `QuotaRouterNode`, no gossip, no forward, no scorer, no handler) |
| Root `quota-router/` (excluded from workspace) | Mesh layer (`QuotaRouterNode` + supporting code) | HTTP proxy, CLI integration, PyO3 integration, `PlatformAdapter` impl |
| Neither had | `InMemoryChannelAdapter: PlatformAdapter`, `InProcessSender` was a fake `NetworkSender` | — |
| Neither had | Wired `PlatformAdapterBridge` going through `NodeTransport` to consumer code | — |

The prior cleanup touched root `quota-router/` (the excluded crate). This plan moves that work into the canonical `crates/quota-router-core/` and closes the integration gaps.

---

## 3. Open questions — proposed answers ready

I am raising these now per your directive. Mark each approve / correct-to before the relevant task starts.

### Q1. Migration of root `quota-router/` → `crates/quota-router-core/`

**Proposed: A (copy + adapt into `crates/quota-router-core/src/node/`).** Keep recent cleanup commits' semantics; treat root `quota-router/` as deprecated once core has the migrated code. Delete the root crate after integration tests are re-homed. Migration preserves recent cleanup commits on the canonical surface (`b4280f1a` … `ca5b68eb`).

### Q2. CLI daemon invocation for Layer 4 docker tests

**Proposed: `quota-router serve` subcommand.** Add to `crates/quota-router-cli/src/cli.rs`:

```rust
Serve {
    /// Listen address for the mesh TCP transport (RFC-0850 §8.8)
    #[arg(long, default_value = "0.0.0.0:9100")]
    listen_addr: SocketAddr,
    /// Path to network config (node_id, network_id, peer addresses, providers).
    #[arg(long)]
    network_config: PathBuf,
    /// Mock-provider mode: returns deterministic responses instead of
    /// calling a real LLM provider. Required for docker tests.
    #[arg(long)]
    mock_provider: bool,
    /// Peer endpoints (comma-separated `node_id:addr`).
    #[arg(long, value_delimiter = ',')]
    peers: Vec<String>,
}
```

The `quota-router` CLI binary is the docker daemon. No separate `quota-router-node` binary. The mock provider path uses an in-crate `MockLocalProvider` exposed for tests.

### Q3. Docker test infra

**Proposed: docker-compose v2**, two services (`node-a`, `node-b`), optional third (`node-c`) for 3-node tests. Each runs `quota-router serve --mock-provider --network-config /etc/qr/mesh.toml --peers <other>` from a shared image. Healthcheck = TCP connect to listen port.

### Q4. TLS for cross-process / cross-host

**Proposed: plaintext TCP for now**, TLS as future concern. Sender-id plumbing via HMAC + a shared `network_key` already in production. TLS adds cert handling unrelated to the actual auth edge (sender-id mapping). Defer TLS design to a later RFC.

**Q4 GAP surfaced during investigation — must be raised NOW, not later:**

The existing `PlatformAdapterBridge` in `octo-transport/src/adapter_bridge.rs` implements **`NetworkSender` only**. There is no `NetworkReceiver` impl, no `PlatformAdapter::receive_messages` poller, and no way for the mesh to feed inbound data from a `TcpAdapter` (or any other `PlatformAdapter`) into `NodeTransport::dispatch` → `QuotaRouterHandler`.

This means:
- `TcpAdapter` exists and can `send_message` to peers — but the receiving `TcpAdapter` has no way to feed what it reads into the handler.
- The mesh is **send-only** across the `PlatformAdapter` boundary.

This is a real production code gap that blocks the 100% coverage goal for the receive path. Layer 3 (cross-process TCP) cannot pass without it.

**Resolution — three new tasks, all production code, not test scaffolding:**

| Task | What | Why |
|---|---|---|
| **T-pre5** | Add `NetworkReceiver` impl to `PlatformAdapterBridge` (or a new sibling type `PlatformAdapterReceiver`). The impl polls `adapter.receive_messages(domain)`, calls `canonicalize`, extracts `envelope.source_peer` as the sender-id, builds `ReceiveContext`, and feeds the payload into the inner `NetworkReceiver` chain. | Closes the send-only gap. Makes the bridge a complete trait bridge, not a half-bridge. |
| **T-pre6** | Fix `TcpAdapter` 2-frame wire inconsistency. Two options: (a) write `[4-byte env_len][envelope][4-byte payload_len][payload]` as 1 logical frame with internal framing, (b) carry a "this is the payload of envelope X" hint on `RawPlatformMessage`. Pick (a) for simplicity — adapter writes `[4-byte env_len][envelope]...[4-byte payload_len][payload]` as a SINGLE concatenated length-prefixed frame: `[8-byte total_len][env_len][envelope][payload_len][payload]`. Reader reads 1 frame, splits internally. | Eliminates the consumer-pairing hazard. |
| **T-pre7** | Update RFC-0850 §8.8 (TCP) to specify the unified frame format. Update the TcpAdapter test `l5_payload_over_wire.rs` to verify the new format. | Wire-format change documented in RFC. Test asserts the format. |

**Status:** T-pre5, T-pre6, T-pre7 are required before Layer 3 (cross-process TCP tests) can land. They are unblocking tasks for the original T-pre1.

### Q5. Wire format (cross-process) — **RESOLVED via existing infrastructure**

Investigation of RFC-0850 §8.6 (transport mode selection) and the existing `TcpAdapter` revealed the right answer is to **use the existing format, not invent a new one**.

**What exists in production code today:**

1. **RFC-0850 §8.6** defines 4 wire-format modes:
   - `DOT/1/{base64}` — Text (chat apps)
   - `DOT/2/{msg_id}` — Native (platform media upload)
   - `DOT/F/{base64_fragment}` — Fragment
   - `RAW/{binary}` — Raw (QUIC, WebRTC, NativeP2P, **TCP**)

2. **`TcpAdapter` (existing in `crates/octo-adapter-tcp/src/lib.rs:147-156`)** uses Raw mode with format:
   ```
   [4-byte env_len][envelope wire bytes][4-byte payload_len][payload bytes]
   ```
   Sender writes 2 length-prefixed frames per logical message.

3. **`DeterministicEnvelope` (existing in `crates/octo-network/src/dot/envelope.rs`)** carries `source_peer: [u8; 32]` — a structured 32-byte sender-id field. `PlatformAdapterBridge::build_envelope` already populates this from `SendContext.source_peer` (line 38 of `octo-transport/src/adapter_bridge.rs`). No wire change needed.

4. **`RawPlatformMessage.platform_id: String`** is a debug-format String (`format!("{:?}", peer_id)` at `crates/octo-adapter-tcp/src/lib.rs:126`). Not a structured identity. Should not be used as the source of truth for sender_id.

**Decision (Q5 answer):** **No wire format change.** Sender-id plumbing uses `DeterministicEnvelope.source_peer` (already on the wire inside the envelope frame). The receive-side code path:

1. `adapter.receive_messages(domain)` returns `Vec<RawPlatformMessage>`
2. For each raw, `adapter.canonicalize(raw)` returns `DeterministicEnvelope`
3. `envelope.source_peer` is the 32-byte sender-id
4. `ReceiveContext.sender_id = Some(envelope.source_peer)`
5. The actual payload is the second frame (paired with the envelope frame) OR carried by some other means (TBD per the TcpAdapter gap below)

**Secondary gap surfaced:** TcpAdapter's reader (`crates/octo-adapter-tcp/src/lib.rs:103-135`) reads 1 frame at a time but the writer writes 2 frames per logical message. The receiver's `receive_messages` returns the envelope as one `RawPlatformMessage` and the payload as a second `RawPlatformMessage` with no semantic link between them. This is fragile. **Resolution:** the TcpAdapter should be fixed to either (a) write 1 frame containing both envelope and payload with internal framing, or (b) carry a "this is the payload of envelope X" hint on `RawPlatformMessage`. The first option is simpler. **This is a TcpAdapter production change**, not a wire-format invention.

**Implication for the plan:** T-pre2 (RFC-0870 v1.14 amendment) does NOT need to specify a new wire format. It needs to (a) document the existing Raw mode for TCP, (b) add a §"PlatformAdapter receiver" section specifying how the bridge polls `PlatformAdapter::receive_messages` and feeds `NodeTransport::dispatch`, (c) require the TcpAdapter to fix the 2-frame inconsistency.

### Q6. Bootstrap orchestrator

**Proposed: implement it.** `BootstrapOrchestrator` in `octo-network/src/sync/` (wherever the stub is) becomes production code that:
- Deserializes `SeedListEnvelope` per RFC-0851p-a
- Outbound connects to each seed
- Async waits for first `min_peers` responses with timeout
- Pull-gossip integrated

Preceded by RFC-0851p-a amendment that specifies the contract.

### Q7. CapacityExhausted path

**Proposed: RFC-0870 v1.14 first**, then `scorer::SelectionState` enum + `handle_forward_request` switch. Small (~30 lines in scorer.rs + handler.rs) but wire-protocol-adjacent (re-encodes `ForwardRejectPayload.reason`).

### Q8. Test runtime / CI budget

**Approved by user: CI runs only L1 and L2. L3 and L4 are manual only.**

- **Layer 1 (unit):** always runs in CI, <30s total
- **Layer 2 (in-process mesh):** always runs in CI, <60s
- **Layer 3 (cross-process TCP):** `#[ignore]`-gated, manual only. Developer runs locally with `cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml -- --ignored l3_*`
- **Layer 4 (docker):** `#[ignore]`-gated, manual only. Developer runs with docker engine available.

CI matrix gates:
- Linux: required
- macOS / Windows: L1 + L2 only (no process spawning, no docker)

Manual run instructions are documented in `crates/quota-router-integration-tests/tests/layer3/` and `crates/quota-router-integration-tests/tests/layer4/` README files.

### Q9. Library entry point for non-CLI consumers

**Proposed:** `quota_router_core::run_node(config) -> impl Future<Output=!>` — thin async loop, callable from PyO3 binding or tests. The CLI's `serve` subcommand calls into this.

### Q10. Test crate restructuring

**Proposed: re-home** root `quota-router-e2e-tests/` into `crates/quota-router-core/tests/integration/` (or as a separate `crates/quota-router-integration-tests/` workspace-member crate). Both paths viable; pick the one that keeps `cargo test --workspace` simple. Default: separate crate `quota-router-integration-tests` (workspace member) so `cargo test -p quota-router-integration-tests` covers layers 1–3 and layer 4 is `#[ignore]`-gated.

---

## 4. Production code inventory — single canonical surface

After T-pre1 (migration), the production code lives in one logical place:

```
crates/quota-router-core/        ← THE library (HTTP proxy + mesh + routing)
crates/quota-router-cli/         ← binary consumer
crates/quota-router-pyo3/        ← Python binding consumer
crates/octo-transport/           ← NetworkSender/Receiver, NodeTransport, GovernedTransport, adapter_bridge
crates/octo-network/             ← PlatformAdapter trait, DotGateway, DGP, BootstrapOrchestrator
crates/octo-adapter-tcp/         ← TcpAdapter: PlatformAdapter for TCP
crates/octo-adapter-{telegram-mtproto,telegram,matrix,whatsapp}/  ← other PlatformAdapter impls
octo-transport/                  ← legacy workspace-excluded; absorbed into crates/octo-transport/ by T-pre1 (?)
```

`crates/quota-router-core/` is the focus. Modules:

| Module | Purpose |
|---|---|
| `proxy.rs` | HTTP proxy server |
| `router.rs` | Routing strategies (single-proxy) |
| `providers.rs` | Provider trait + impls |
| `admin.rs` | Admin API |
| `auth/`, `keys/`, `key_rate_limiter.rs` | Auth + keys |
| `balance.rs`, `cache.rs` | Rate limit + caching |
| `fallback.rs`, `guardrails/` | Fallback + safety |
| `pre_call_checks.rs`, `prompts/`, `pricing.rs` | Request handling |
| `middleware.rs`, `health.rs`, `metrics.rs` | Cross-cutting |
| `mode.rs`, `config.rs`, `schema.rs`, `storage.rs` | Boot/config |
| `secret_manager.rs`, `logging.rs`, `tracing.rs`, `rate_limit/` | Ops |
| `native_http/`, `py_bridge/`, `proxy_bridge/` (feature-gated) | Provider strategy per RFC-0917 |
| **`node/`** (migrated) | **Mesh layer: `QuotaRouterNode`, gossip, forward, scorer, handler, announce, request, provider, metrics, ratelimit** |

After migration, every test target exercises production surface in `crates/quota-router-core/src/node/`.

---

## 5. Layered test architecture

```
Layer 4: docker compose (2-3 containers, single host)
         ├── Container A: quota-router serve --mock-provider --network-config /etc/qr/a.toml
         ├── Container B: quota-router serve --mock-provider --network-config /etc/qr/b.toml
         └── Test runner: in-process Rust binary with TcpAdapter to A's port
                         asserts cross-host gossip convergence, forwarding

Layer 3: cross-process TCP (single host, real processes)
         ├── Process A: std::process::Command → quota-router serve --mock-provider
         ├── Process B: std::process::Command → quota-router serve --mock-provider
         └── Test runner: in-process TcpAdapter to A's port
                         asserts process-to-process dispatch

Layer 2: in-process mesh (production code path)
         ├── tokio runtime, single process
         ├── InMemoryChannelAdapter: PlatformAdapter (mpsc)
         ├── PlatformAdapterBridge wraps InMemoryChannelAdapter
         ├── NodeTransport::new(vec![bridge.as_network_sender()])
         ├── QuotaRouterNode constructed; handler registered via builder
         └── Tests directly exercise core::node::* production code

Layer 1: trait-level and helper-level
         ├── MockReceiver for NetworkReceiver
         ├── NodeTransport unit tests
         ├── GovernedTransport unit tests
         └── Module-level tests for proxy / router / providers / auth / etc.
```

Each layer exercises production code. Mocks exist only for `NetworkReceiver` (test observer) and external provider APIs. There are no parallel implementations of production logic.

### Layer 2 — replacing `InProcessSender`

`InProcessSender` directly implements `NetworkSender`. Layer 2 replaces it with a real `PlatformAdapter`:

```rust
// crates/quota-router-core/src/node/testing/in_memory_adapter.rs (test-only)
pub struct InMemoryChannelAdapter { peers: PeerMap, ... }

impl PlatformAdapter for InMemoryChannelAdapter {
    async fn send_message(&self, domain, envelope, payload) -> Result<DeliveryReceipt, _>;
    async fn receive_messages(&self, domain) -> Result<Vec<RawPlatformMessage>, _>;
    fn canonicalize(&self, raw: &RawPlatformMessage) -> Result<DeterministicEnvelope, _>;
    fn capabilities(&self) -> CapabilityReport { ... }
    fn platform_type(&self) -> PlatformType { PlatformType::InProcess } // synthetic
    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId { ... }
}
```

Then `PlatformAdapterBridge::new(Box::new(adapter))` produces `NetworkSender + NetworkReceiver` outputs that go into `NodeTransport`. The `envelope()` helper produces `[discriminator, bincode]` body for the inner layer.

This exercises:
- `PlatformAdapter` trait (5 methods)
- `PlatformAdapterBridge` adaptation
- `DeterministicEnvelope` construction (RFC-0850 / RFC-0126)
- DOT wire format end-to-end
- `RawPlatformMessage` shaping
- Sender-id plumbing through `Bridge → NodeTransport::dispatch → handler::on_receive`

---

## 6. Implementation tasks (dependency-ordered)

### Phase pre: Migration

| # | Task | Acceptance |
|---|---|---|
| **T-pre1** | Migrate root `quota-router/src/*` → `crates/quota-router-core/src/node/*`. Update `crates/quota-router-core/Cargo.toml` (no feature gate for `node` module — must compile in all 3 modes). Add `pub mod node;` to `crates/quota-router-core/src/lib.rs`. Update `crates/quota-router-cli` and `crates/quota-router-pyo3` to depend on the migrated surface. Delete root `quota-router/`. | `cargo build -p quota-router-core --features litellm-mode` succeeds. `cargo build -p quota-router-core --features any-llm-mode` succeeds. `cargo build -p quota-router-core --features full` succeeds. Behavior matches the recent cleanup commits (53d97988 → ca5b68eb). |
| **T-pre2** | RFC-0870 v1.14 amendment: codify (a) the `SelectionState` for capacity-exhausted routing (Q7), (b) the §"PlatformAdapter receiver" section specifying how the bridge polls `PlatformAdapter::receive_messages` and feeds `NodeTransport::dispatch`. No new wire format. | RFC merged. Test references by section number only. |
| **T-pre3** | RFC-0851p-a amendment: codify `BootstrapOrchestrator` contract. Replace the orchestrator stub in `octo-network` with real code per Q6. Wire into `QuotaRouterNode::build_with_bootstrap()`. | Orchestrator implemented. `build_with_bootstrap` no longer `#[allow(unused)]`. Tests cover happy path + timeout + min_peers-not-reached. |
| **T-pre4** | Re-home root `quota-router-e2e-tests/` into a new workspace member `crates/quota-router-integration-tests/` (or similar, per Q10). Update deps to use `quota-router-core` (not root `quota-router`). | `cargo test -p quota-router-integration-tests` runs without the excluded root crate. The 222+ tests still pass. |
| **T-pre5** | **NEW — addresses Q4 gap.** Add `NetworkReceiver` impl to `PlatformAdapterBridge` (or a new sibling type `PlatformAdapterReceiver`). The impl polls `adapter.receive_messages(domain)`, calls `canonicalize`, extracts `envelope.source_peer` as the sender-id, builds `ReceiveContext { source_transport: adapter.platform_type().name(), mission_id: envelope.mission_id, sender_id: Some(envelope.source_peer) }`, and feeds the payload into a target `NetworkReceiver` (configurable at construction). | Bridge is now a complete trait bridge. Tests in `octo-transport` exercise both sender and receiver paths. |
| **T-pre6** | **NEW — addresses Q4 secondary gap.** Fix `TcpAdapter` 2-frame wire inconsistency. Switch the wire to a single logical frame: `[4-byte total_len][env_len:u32][envelope bytes][payload_len:u32][payload bytes]`. Reader reads `total_len`, then reads each sub-frame. Each `RawPlatformMessage` carries BOTH envelope bytes and payload bytes (consolidated in a single struct or via accessor methods on the message). | Reader returns 1 `RawPlatformMessage` per logical send. Consumer no longer needs to pair 2 messages. |
| **T-pre7** | **NEW — addresses Q4 documentation.** Update RFC-0850 §8.8 (TCP) to specify the unified frame format. Update the TcpAdapter test `l5_payload_over_wire.rs` to verify the new format. | Wire format documented in RFC. Test asserts the format byte-by-byte. |

### Phase A: Layer 1 — line-by-line audit

| # | Task | Acceptance |
|---|---|---|
| **T-A1** | Audit `crates/quota-router-core/src/{admin,auth,balance,cache,callbacks,config,fallback,guardrails,health,key_rate_limiter,keys,logging,metrics,middleware,mode,pre_call_checks,pricing,prompts,providers,proxy,rate_limit,router,schema,secret_manager,storage,tracing}.rs`. For every public function: identify which test exercises it. Add unit tests where absent. | Each `pub fn` called from at least one test. `cargo tarpaulin --lib -p quota-router-core` shows no uncovered lines in non-feature-gated production code. |
| **T-A2** | Audit `crates/quota-router-core/src/node/{lib,handler,forward,gossip,scorer,announce,request,provider,metrics,ratelimit}.rs`. Add unit tests for: scorer filter functions (each branch), envelope wire-format round-trip, every `handle_*` happy path AND each `ForwardRejectReason` variant. | Same. The Phase-4 `#[ignore]` test `l2_inbound_capacity_exhausted_emits_reject` is enabled (requires T-pre2 capacity-exhausted work). |
| **T-A3** | Audit `crates/octo-transport/src/{sender,receiver,node_transport,governed_transport,adapter_bridge,adapter_factory,dom_bootstrap}.rs`. | `cargo tarpaulin --lib -p octo-transport` shows 100% line coverage. |
| **T-A4** | Audit `crates/octo-network/`: dot adapters/envelope/domain/error, sync (BootstrapOrchestrator), dgp, gateway. | Same. |
| **T-A5** | Audit `crates/octo-adapter-tcp/` (TcpAdapter already has 1 test). Add coverage for: outbound framing on TcpStream, inbound `RawPlatformMessage` shape, connection management, health, reconnection, error paths. | Same. |

### Phase B: Layer 2 — in-process via real `PlatformAdapter`

| # | Task | Acceptance |
|---|---|---|
| **T-B1** | Implement `InMemoryChannelAdapter: PlatformAdapter` in `crates/quota-router-core/src/node/testing/in_memory_adapter.rs` (test-only, `#[cfg(test)]` + `feature = "test-helpers"`). | Compiles; trait methods covered by unit tests. |
| **T-B2** | Replace `InProcessSender` in the integration test crate with `PlatformAdapterBridge::new(Box::new(InMemoryChannelAdapter::new(...)))` (or equivalent — depends on adapter builder API). Drop the bridge's `NetworkSender` / `NetworkReceiver` outputs into `NodeTransport`. | Layer 2 tests pass; the integration tests now exercise `PlatformAdapterBridge`, `DeterministicEnvelope::canonicalize`, and the wire-format bytes. |
| **T-B3** | Add Layer 2 integration tests for each `ForwardRejectReason` variant (TtlExpired, NoProvider, CapacityExhausted, ModelNotSupported, ContextWindowExceeded, BudgetExceeded, AuthFailure, PayloadTooLarge). | Each variant reachable and asserted. |
| **T-B4** | Add sender-id plumbing tests: source-peer → `RawPlatformMessage.source_peer` → `PlatformAdapterBridge` → `ReceiveContext.sender_id` → handler trust check. | Coverage of the auth-edge via test fixture peer_id mapping. |

### Phase C: CLI daemon subcommand

| # | Task | Acceptance |
|---|---|---|
| **T-CLI1** | Add `Commands::Serve { listen_addr, network_config, mock_provider, peers }` to `crates/quota-router-cli/src/cli.rs`. Implement handler in `commands.rs`. The handler: parses `network_config` (TOML), constructs `QuotaRouterNode`, builds a `TcpAdapter` bound to `listen_addr`, wraps via `PlatformAdapterBridge`, calls `node.serve()` long-term until SIGTERM. The mock-provider path exposes a `MockLocalProvider` from core's testing module. | `cargo run -p quota-router-cli -- serve --listen-addr 127.0.0.1:9100 --mock-provider --network-config /tmp/mesh.toml` boots successfully. SIGTERM exits cleanly. |
| **T-CLI2** | Unit tests for the CLI argument parsing and the `serve` subcommand dispatch. | CLI flag combinations covered. |

### Phase D: Layer 3 — cross-process TCP

| # | Task | Acceptance |
|---|---|---|
| **T-C1** | Layer 3 test harness in `crates/quota-router-integration-tests/tests/layer3_cross_process_tcp.rs`: spawns two `quota-router serve --mock-provider` processes via `std::process::Command` (each on a different ephemeral port), constructs a third in-process `QuotaRouterNode` with `TcpAdapter` connecting to one of them, asserts message flow. | Tests pass. Two processes exchange real bytes. |
| **T-C2** | Wire-format round-trip: send a `ForwardRequest` from process A, receive at process B, send `ForwardResponse` from B back to A, verify body matches handler output. | Tests pass. Frame format `[len:u32 BE][sender_id:32][discriminator:1][body]` matches Q5. |
| **T-C3** | HMAC validation across processes: process B rejects a request with tampered HMAC originated from process A. | Tests pass. |
| **T-C4** | Process lifecycle: SIGTERM to one process; other process observes withdraw (RFC-0870 v1.13 §Lifecycle). | Tests pass. |

### Phase E: Layer 4 — cross-host Docker (manual only, gated)

**Per Q3:** request-exercising tests must originate from a real consumer (CLI / PyO3 / HTTP), not a parallel test fixture. Some tests need 3+ nodes (not just 2).

| # | Task | Acceptance |
|---|---|---|
| **T-D1** | `crates/quota-router-integration-tests/tests/layer4/Dockerfile`: builds the CLI in a Rust slim image. Entrypoint: `quota-router serve` with env vars `LISTEN_ADDR`, `NETWORK_CONFIG`, `MOCK_PROVIDER=1`, `PEERS`. | Image builds. |
| **T-D2** | `tests/layer4/compose-2node.yaml`: two `quota-router` services, each with `--mock-provider`, shared network config, healthcheck = TCP connect. | `docker compose up` brings both services healthy. |
| **T-D2b** | `tests/layer4/compose-3node.yaml`: three `quota-router` services for gossip convergence tests. | Same. |
| **T-D2c** | `tests/layer4/compose-N-node.yaml.template`: parameterized template (3, 5, 8) for fan-out / gossip-stress tests. | Generated compose files work for each N. |
| **T-D3** | `tests/layer4/layer4_2node.rs`: spins up compose-2node. **Request from a real consumer**: the test runner starts a 3rd `quota-router serve --mock-provider` process (or `quota-router cli route ...` call) that uses the HTTP proxy of one container. Asserts the request routes through the mesh to the other container's mock provider and returns. | Test runs end-to-end against real containers. `#[ignore]`-gated. linux-only. |
| **T-D4** | `tests/layer4/layer4_disconnect_heal.rs`: stop container B (in compose-2node), observe A continues to serve; restart B, observe rejoin via gossip. | Test runs end-to-end. |
| **T-D5** | `tests/layer4/layer4_3node_gossip.rs`: compose-3node. All three nodes broadcast gossip; runner asserts gossip converges across all three. | Test runs end-to-end. |
| **T-D6** | `tests/layer4/layer4_fanout.rs`: compose-N-node (N=5 or 8). Single origin node routes a request; runner asserts the mesh fans out correctly. | Test runs end-to-end. |
| **T-D7** | **CI gate (Q8):** Layer 4 is `#[ignore]`-gated. No CI runs them automatically. Developer runs with `cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml -- --ignored layer4_*` after `docker compose up`. | Manual-only invariant enforced. No CI workflow auto-runs Layer 4. |
| **T-D8** | `tests/layer4/README.md`: manual run instructions, prerequisites (docker, compose v2, port allocation), debug recipes (`docker compose logs node-a`, etc.). | Documented. |

### Phase F: Other PlatformAdapter crates

| # | Task | Acceptance |
|---|---|---|
| **T-E1** | `cargo tarpaulin -p octo-adapter-matrix --lib`; fill gaps. | Coverage: 100% of its `PlatformAdapter` impl. |
| **T-E2** | Same for `octo-adapter-whatsapp`. | Same. |
| **T-E3** | `octo-adapter-telegram-mtproto` (workspace member) and `octo-adapter-telegram` (TDLib, excluded) — audit per their own gating. | Same. |

### Phase G: Python binding + CLI sweep

| # | Task | Acceptance |
|---|---|---|
| **T-F1** | Audit `crates/quota-router-cli/`. Each subcommand variant has at least one integration test. | All `Commands::*` variants exercised. |
| **T-F2** | Audit `crates/quota-router-pyo3/`. PyO3 init / teardown / each exposed Python function. | Tests via Python invocation (separate concern; may need a Phase H plan). |

### Phase H: CI gate

| # | Task | Acceptance |
|---|---|---|
| **T-G1** | CI gate: `cargo tarpaulin --workspace --timeout 10m`. Failure if coverage drops below 100%. | CI blocks merges that reduce coverage. |
| **T-G2** | Add `make coverage` and `make coverage-diff` (vs baseline) for developer ergonomics. | Convenience. |
| **T-G3** | Documentation: `docs/coverage-policy.md` codifying the 100% goal, the four layers, the docker opt-in, the no-fake-tests rule. | Policy document. |

---

## 7. Verification criteria — how we know we're at 100%

After all tasks complete:

1. `cargo tarpaulin --workspace` reports **100% line coverage** of production `.rs` files (test-only `#[cfg(test)]` modules excluded by tarpaulin's defaults).
2. Every `pub fn` in production code has at least one test that runs the production body.
3. Test layering: Layer 1 always-on, Layer 2 always-on (in-process mesh), Layer 3 always-on (linux, cross-process), Layer 4 opt-in (docker).
4. `cargo test --workspace` succeeds (modulo Layer 4 `#[ignore]`).
5. No file/directory/dependency in the repo has the name `quota-router-node` anywhere.
6. The only two production consumers of `quota-router-core` are `quota-router-cli` (binary) and `quota-router-pyo3` (PyO3 binding).
7. No production code lives under `tests/`, `docker/`, `infra/`, or any other test-infrastructure directory.
8. All RFCs cross-referenced by tests use bare numbers; no version pins in prose.
9. Docker tests use the `quota-router` CLI binary running `serve`, not a separate daemon.

---

## 8. Execution order after sign-off

Once Q1–Q10 are answered:

1. **T-pre1 (migration)** — most unblocking
2. **T-pre3 (Bootstrap orchestrator)** — depends on T-pre1
3. **T-pre2 (RFC-0870 v1.14 + RFC-0851p-a)** — small amendment, unblocks T-pre3 design questions
4. **T-pre4 (test re-home)** — unblocks every Layer 2/3/4 task
5. **T-CLI1 (Serve subcommand)** — unblocks Layer 3/4 tests
6. **Phase A audits** — can run in parallel with Phase B once T-pre1 settles
7. **Phase B (Layer 2 PlatformAdapter)** — replaces InProcessSender
8. **Phase C (Layer 3 cross-process)** — requires T-CLI1
9. **Phase D (Layer 4 docker)** — requires T-CLI1 + T-pre4 + the upper layers
10. **Phase E/F/G** — sweeps

After sign-off, I dispatch the first batch of **T-pre1, T-pre2, T-pre3, T-pre4, T-CLI1** in parallel where dependencies permit, with full two-stage review per the subagent-driven-development skill.

I will not touch production code until Q1–Q10 are answered.
