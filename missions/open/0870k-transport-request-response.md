# Mission: 0870k-transport-request-response — Layer D request/response substrate

## Status

Open (filed 2026-08-12). Unblocks mission `0871b-cross-node-forwarding` (OPEN, same date).

## Problem

`octo-transport` (Layer D) today exposes only fire-and-forget:
- `NetworkSender::send(&[u8])` (sender.rs:49) — no return data
- `NodeTransport::broadcast` + `send_best` — one-way

Mission `0871b-cross-domain-resolution-impl` (LANDED 2026-08-11, commit `c14c2707`) explicitly deferred cross-node forwarding because "request/response substrate that does not exist in `octo-transport` today" (per `chain.rs:33-41` module docstring). Mission `0871b-cross-node-forwarding` inherits this dependency as its hardest external blocker.

**Key insight (audit 2026-08-12)**: this substrate already exists in quota-router-specific form per RFC-0870 §PendingRequests + §ForwardingConfig + §ForwardResponse (lines 586-633, 1112-1119, 1368, 1431-1434, 1887-1899, 2312+). It just lives in `quota-router-core` and uses RFC-0870-mesh-specific discriminator bytes (`0xC3-0xC5` in `0x0009:0003` sub-namespace per `payload_kind.rs:254`). This mission **generalizes** the RFC-0870 design to Layer D substrate using RFC-0871's `envelope_id: [u8; 32]` as the correlation key — not greenfield design.

## RFC anchor

- **Authoritative pattern source**: RFC-0870 §ForwardRequest / §ForwardResponse / §ForwardReject (lines 586-633) + §PendingRequests (`insert/complete/reject/origin`, lines 1110-1119, 1368, 2312+) + §ForwardingConfig (`max_ttl/max_concurrent_forwards/forward_timeout/max_payload_bytes`, lines 1887-1899) + §ForwardRejectReason (line 1634). Already implemented in `quota-router-core` mesh-specific; 0870k generalizes to Layer D.
- **Correlation key**: RFC-0871 §Algorithms step 2 (`envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`, line 200-201) — semantically equivalent to RFC-0870's `request_id` (also BLAKE3-derived 32 bytes) but already wired through the entire envelope pipeline (Layer A + replay defense). No new wire form needed.
- **Consumer pattern**: RFC-0871 §Algorithms TV7-ish — `WALLET_RESOLVE_DID` envelope sent to wallet node carries asker's DID; wallet reply envelope carries same `envelope_id` field (line 518); sender matches reply by envelope_id. This is the existing cross-node envelope_id correlation pattern that 0870k generalizes to substrate.
- **General transport substrate**: RFC-0850 (Deterministic Overlay Transport) — the foundation RFC; 0870k adds a request/response primitive on top.

## Substrate (already shipped)

- `octo-transport/src/sender.rs:18-39` — `TransportError` enum (5 variants: `AdapterFailure`, `AllTransportsFailed`, `EnvelopeConstruction`, `Unhealthy`, `GovernanceViolation`)
- `octo-transport/src/sender.rs:47-56` — `NetworkSender` trait (`send` + `name` + `is_healthy`)
- `octo-transport/src/sender.rs:5-14` — `SendContext { mission_id: [u8; 32], priority: u8, source_peer: [u8; 32], origin_gateway: [u8; 32] }`
- `octo-transport/src/receiver.rs:7-14` — `ReceiveContext { source_transport: String, mission_id: [u8; 32], sender_id: Option<[u8; 32]> }`
- `octo-transport/src/receiver.rs:22-32` — `NetworkReceiver` trait (`on_receive` + `name`)
- `octo-transport/src/node_transport.rs:14` — `NodeTransport { senders: Vec<Arc<dyn NetworkSender>>, receivers: ... }`
- `octo-transport/src/node_transport.rs:20` — `NodeTransport::new(senders)`
- `octo-transport/src/node_transport.rs:30` — `NodeTransport::register_receiver(receiver)`
- `octo-transport/Cargo.toml:14,18` — `futures = "0.3"`, `tokio = { version = "1", features = ["time", "sync", "net", "io-util", "rt"] }` (tokio::sync::oneshot available)
- `octo-transport/src/lib.rs:1-14` — module structure
- `octo-protocol/src/envelope.rs:154` — `NodeEnvelope.envelope_id: [u8; 32]` (RFC-0871 correlation key)
- `octo-protocol/src/envelope.rs:200-201` — `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))` (RFC-0871 §Algorithms step 2)
- `crates/quota-router-core/src/node/quota_router_node.rs:1368` — existing `pending: PendingRequests` field (RFC-0870 mesh-specific; 0870k generalizes)
- `crates/quota-router-core/src/node/quota_router_node.rs:2312+` — existing `PendingRequests::insert/complete/reject/origin` impl (RFC-0870)

No consumer of `send_request` exists yet at Layer D. Mission `0871b-cross-node-forwarding` (OPEN, same date) is the first general-purpose consumer. Quota-router mesh already uses RFC-0870's mesh-specific request/response (per `quota-router-core`).

## Traits / surface in scope (NEW)

### T1. `TransportError::Unsupported(String)` variant — additive on `sender.rs:18`

```rust
#[error("operation not supported: {0}")]
Unsupported(String),
```

Additive — no existing match arm breaks. Add a test case to `transport_error_display` test at `sender.rs:86`.

### T2. `NetworkSender::send_request` — additive trait method with default body

```rust
#[async_trait]
pub trait NetworkSender: Send + Sync {
    async fn send(&self, payload: &[u8], context: &SendContext) -> Result<(), TransportError>;

    /// Send a request and await a correlated response.
    ///
    /// Default body returns `Err(TransportError::Unsupported(...))` so
    /// existing senders (UDP adapter, PlatformAdapterBridge, …) remain
    /// source-compatible.
    ///
    /// `envelope_id` MUST equal the `NodeEnvelope::envelope_id` field of
    /// the wrapping envelope (RFC-0871 §Algorithms step 2). The receiver
    /// echoes the same `envelope_id` back in its reply envelope;
    /// `NodeTransport::dispatch_response` matches by id.
    ///
    /// `timeout` bounds the total time waiting for the response. On
    /// expiry, returns `Err(TransportError::AllTransportsFailed)` (the
    /// existing variant — no new error).
    async fn send_request(
        &self,
        _payload: &[u8],
        _envelope_id: [u8; 32],
        _context: &SendContext,
        _timeout: std::time::Duration,
    ) -> Result<Vec<u8>, TransportError> {
        Err(TransportError::Unsupported("send_request not implemented".to_owned()))
    }

    fn name(&self) -> &str;
    fn is_healthy(&self) -> bool;
}
```

**Backward-compat note**: existing `impl NetworkSender for …` blocks (UDP adapter at `octo-adapter-udp/`, `PlatformAdapterBridge`, in-process mock senders in tests) inherit the default body — zero edits required outside this mission.

### T3. `NodeTransport::request_response` — high-level async API in `node_transport.rs`

```rust
impl NodeTransport {
    /// Send a request via the first sender that doesn't return
    /// `TransportError::Unsupported`. Await the response until
    /// `timeout` elapses.
    ///
    /// The caller-supplied `envelope` is a fully-formed `NodeEnvelope`
    /// (RFC-0871). The caller is responsible for generating
    /// `envelope.envelope_id` per RFC-0871 §Algorithms step 2 BEFORE
    /// calling `request_response`; the same id is used for
    /// `register_response_handler` and echoed by the receiver.
    ///
    /// Returns the reply envelope bytes (caller decodes per
    /// `reply.payload_kind`).
    pub async fn request_response(
        &self,
        envelope: &NodeEnvelope,
        context: &SendContext,
        timeout: std::time::Duration,
    ) -> Result<Vec<u8>, TransportError>;
}
```

Internal implementation:
1. Iterate `self.senders`; first one whose `send_request` does NOT return `Unsupported` wins.
2. Wrap in `tokio::time::timeout(timeout, send_request_future).await`.
3. On timeout → `Err(AllTransportsFailed)`.
4. Receiver-side correlation: the inbound reply envelope's `envelope_id` must equal `envelope.envelope_id` (validated by `dispatch_response`).

### T4. `NodeTransport::dispatch_response` + `register_response_handler` — receiver-side correlation keyed by RFC-0871 `envelope_id`

```rust
impl NodeTransport {
    /// Register an expectation for a response with the given
    /// `envelope_id` (RFC-0871). Returns a `oneshot::Receiver<Vec<u8>>`
    /// that resolves when `dispatch_response` is called with the
    /// matching id, or the receiver is dropped (timeout / cancellation).
    pub fn register_response_handler(
        &self,
        envelope_id: [u8; 32],
    ) -> tokio::sync::oneshot::Receiver<Vec<u8>>;

    /// Called by the inbound dispatch path when a reply envelope arrives
    /// with `envelope_id` matching a registered handler. Delivers the
    /// payload to the awaiting caller.
    ///
    /// Public so test infrastructure can inject responses directly; in
    /// production the `NodeTransport` receive loop calls it after
    /// extracting `envelope.envelope_id` from the inbound envelope.
    ///
    /// If no handler is registered for the given id, the reply
    /// envelope falls through to `NetworkReceiver::on_receive` (existing
    /// path) — fail-closed for unknown replies.
    pub fn dispatch_response(
        &self,
        envelope_id: [u8; 32],
        payload: Vec<u8>,
    ) -> Result<(), TransportError>;
}
```

Internal state: `Arc<tokio::sync::Mutex<HashMap<[u8; 32], oneshot::Sender<Vec<u8>>>>>`. Auto-cleanup of stale entries on `dispatch_response` failure (`Sender` dropped when the `Receiver` was cancelled).

### T5. `RequestResponseConfig` struct — generalized from RFC-0870 `ForwardingConfig`

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RequestResponseConfig {
    /// Timeout for awaiting a reply. Default: 30s (matches RFC-0870
    /// `ForwardingConfig::forward_timeout`).
    pub forward_timeout: std::time::Duration,

    /// Maximum concurrent in-flight requests per `NodeTransport`.
    /// Default: 64 (matches RFC-0870 `max_concurrent_forwards`).
    pub max_concurrent: u32,

    /// Maximum request payload size in bytes. Default: 1MB (matches
    /// RFC-0870 `max_payload_bytes`).
    pub max_payload_bytes: usize,
}
```

**Layer discipline**: `max_ttl` from RFC-0870's `ForwardingConfig` is forwarding-specific (hop-count bound for the mesh); NOT included here. `RequestResponseConfig` is the substrate-level subset (timeout + concurrency + size only). Forwarding-mesh-specific config stays in `quota-router-core`.

### T6. `PendingRequests` struct — generalized from RFC-0870 `PendingRequests` (quota-router-core:2312+)

```rust
pub struct PendingRequests {
    by_id: HashMap<[u8; 32], PendingEntry>,
}

struct PendingEntry {
    sender: oneshot::Sender<Vec<u8>>,
    registered_at: std::time::Instant,
}

impl PendingRequests {
    pub fn new() -> Self;
    pub fn insert(
        &mut self,
        envelope_id: [u8; 32],
        sender: oneshot::Sender<Vec<u8>>,
    ) -> Result<(), PendingRequestsError>;
    pub fn complete(
        &self,
        envelope_id: [u8; 32],
        response: Vec<u8>,
    ) -> Result<(), PendingRequestsError>;
    pub fn reject(
        &self,
        envelope_id: [u8; 32],
        reason: &str,
    ) -> Result<(), PendingRequestsError>;
    pub fn evict_expired(&mut self, now: std::time::Instant, timeout: Duration) -> usize;
    pub fn len(&self) -> usize;
}
```

**Difference from RFC-0870**: drops the `origin: RouterNodeId` field (mesh-specific routing); RFC-0871 `NodeEnvelope.from_did` already carries the origin identity. Adds `registered_at` for eviction policy.

### Wire form: RFC-0871 `NodeEnvelope` (no new envelope kind)

The reply envelope IS a `NodeEnvelope` whose `envelope_id` field equals the request envelope's `envelope_id`. No magic-byte discriminator. No sidecar struct. The reply's `payload_kind` discriminates request from response (per RFC-0871 convention — `WALLET_RESOLVE_DID` response kind is distinct from request kind).

## Scope (acceptance criteria)

| AC | Description | Type |
|----|-------------|------|
| AC-1 | `TransportError::Unsupported(String)` variant added to `octo-transport/src/sender.rs:18-39`; new test case in `transport_error_display` at `sender.rs:86` | MODIFY enum + test |
| AC-2 | `NetworkSender::send_request` method per T2 with default body returning `Err(Unsupported(...))`; trait stays source-compatible (every existing `impl NetworkSender` in workspace compiles unchanged) | MODIFY trait |
| AC-3 | `NodeTransport::request_response` high-level API per T3; iterate senders, skip `Unsupported`, wrap in `tokio::time::timeout`; accepts fully-formed `NodeEnvelope` per RFC-0871 | NEW method |
| AC-4 | `NodeTransport::register_response_handler` + `dispatch_response` per T4 keyed by RFC-0871 `envelope_id`; `Arc<Mutex<HashMap<[u8;32], oneshot::Sender<Vec<u8>>>>>` internal state | NEW methods + state |
| AC-5 | `RequestResponseConfig` per T5 in `octo-transport/src/request_response.rs` (NEW file); serde round-trip test | NEW struct |
| AC-6 | `PendingRequests` per T6 in same file; `insert/complete/reject/evict_expired` impls + unit tests | NEW struct |
| AC-7 | Unit test: `request_response_unsupported_sender_returns_error` — `NodeTransport` with only UDP sender (no `send_request` impl) returns `Err(AllTransportsFailed)` after timeout | NEW test |
| AC-8 | Unit test: `request_response_round_trip_via_mock_sender` — in-process `MockRequestSender` (test-only `NetworkSender` impl with shared correlation map) + `NodeTransport` round-trips a request + reply where reply envelope's `envelope_id` echoes the request's `envelope_id` | NEW test |
| AC-9 | Unit test: `dispatch_response_unknown_envelope_id_falls_through` — unknown `envelope_id` returns `Err` (logged via `tracing::warn!`), does NOT panic | NEW test |
| AC-10 | Unit test: `register_response_handler_drop_on_caller_cancel` — caller drops the `oneshot::Receiver`; subsequent `dispatch_response` returns `Err` without leaking the entry; `PendingRequests::evict_expired` reaps stale entries | NEW test |
| AC-11 | Integration TV: 3-node mock transport (A → B → C) in `tests/request_response_chain.rs`; A's request propagates B → C via RFC-0871 `envelope_id` correlation; C's reply propagates back; intermediate nodes use `register_response_handler` to bind the forward + return paths | NEW TV |
| AC-12 | `cargo clippy -p octo-transport --all-targets -- -D warnings` clean (verifies back-compat: every `impl NetworkSender` in workspace still compiles) | GATE |
| AC-13 | `cargo fmt --all -- --check` clean | GATE |

## Out of Scope

- **Generalizing `quota-router-core::PendingRequests` to use this new substrate** — RFC-0870's mesh-specific `PendingRequests` stays in `quota-router-core` for now. A follow-on mission migrates the mesh to use `octo-transport::PendingRequests` once the substrate is stable. This keeps 0870k Layer D-only and avoids touching the mesh.
- **Per-hop `max_ttl`** — forwarding-mesh-specific (RFC-0870 `ForwardingConfig::max_ttl`); not part of the generic substrate.
- **Cross-domain signing / chain hash per RFC-0970** — consumer responsibility (`0871b-cross-node-forwarding`); the substrate provides correlation by `envelope_id`, not by chain_hash. The two are independent integrity dimensions (envelope_id = request/response correlation; chain_hash = hop binding).
- **Production-grade timeout/retry policy** — fixed `Duration` timeout, no retries; advanced retry policy lives in the consumer.
- **Authentication on `send_request`** — sender's peer identity is in `SendContext.source_peer` + `NodeEnvelope.from_did` (RFC-0871); auth at the envelope semantic layer (`ReferenceDispatcher::verify_all` per RFC-0871 §Algorithms step 3) is the consumer's responsibility.
- **Magic-byte discriminator / sidecar `CorrelationEnvelope`** — DEPRECATED; RFC-0871 `envelope_id` field is the wire-form correlation primitive (no new envelope kind needed).

## Cross-references

- **RFC-0870** §ForwardRequest / §ForwardResponse / §ForwardReject (lines 586-633) + §PendingRequests (lines 1110-1119, 1368, 2312+) + §ForwardingConfig (lines 1887-1899) + §ForwardRejectReason (line 1634) — authoritative pattern source
- **RFC-0871** §Algorithms step 2 (`envelope_id` derivation, lines 200-201) — correlation key
- **RFC-0871** §Algorithms step 3 (signing preimage `blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE", envelope_id || from_did_wire || payload)`) — for downstream auth at consumer layer
- **RFC-0871** §Roles and Authorities — `WalletNode` example uses cross-node `envelope_id` correlation (`WALLET_RESOLVE_DID`, line 518)
- RFC-0850 (Deterministic Overlay Transport) §Deterministic Overlay substrate — general transport foundation
- RFC-0970 §Algorithms (chain_hash + forwarding-hop pattern) — `chain_hash` is hop-binding integrity (independent from `envelope_id` correlation)
- `octo-transport/src/sender.rs:18-39` — `TransportError` enum (variant added here)
- `octo-transport/src/sender.rs:47-56` — `NetworkSender` trait (method added here)
- `octo-transport/src/sender.rs:5-14` — `SendContext`
- `octo-transport/src/receiver.rs:7-14` — `ReceiveContext`
- `octo-transport/src/node_transport.rs:14,20,30` — `NodeTransport` (high-level API added here)
- `octo-transport/Cargo.toml:14,18` — `futures = "0.3"`, `tokio = "1"` (oneshot + time available)
- `octo-protocol/src/envelope.rs:154,200-201` — `NodeEnvelope.envelope_id` (RFC-0871 correlation key)
- `crates/quota-router-core/src/node/quota_router_node.rs:1368,2312+` — existing `PendingRequests` (mesh-specific; 0870k generalizes the design)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:33-41` — explicit OUT OF SCOPE note for cross-node forwarding; this mission unblocks it
- Mission `0871b-cross-node-forwarding` (OPEN, 2026-08-12) — first consumer of this substrate; `RemoteResolverBackend::resolve_via` switches from `Err(Unsupported)` stub to a real `NodeTransport::request_response` call once this mission lands
- Mission `0871b-cross-domain-resolution-impl` (LANDED 2026-08-11, commit `c14c2707`) — declared this dependency
- Mission `0870j-udp-adapter-for-gossip-broadcast` (CLOSED 2026-08-07) — UDP adapter inherits default `send_request = Err(Unsupported)` (UDP is fire-and-forget by design)
- Mission `0870-c-envelope-dispatch-compat` (OPEN) — sibling; envelope dispatch substrate

## Layer Discipline

- `octo-transport` (Layer D) — substrate only; no business logic; no envelope semantic interpretation
- No new Cargo deps (uses existing `tokio` + `borsh` already in workspace)
- No upper-layer (`octo-protocol` / `octo-ident`) dependency introduced — `NodeEnvelope` is already in scope via existing `octo-protocol` crate dep
- `octo-protocol` (Layer A) UNCHANGED — RFC-0871 `NodeEnvelope` already provides `envelope_id` correlation
- `quota-router-core` UNCHANGED — its mesh-specific `PendingRequests` stays put; a follow-on mission migrates the mesh to the new substrate

## Dependency

**This mission unblocks**:
- `0871b-cross-node-forwarding` — `RemoteResolverBackend::resolve_via` becomes a real `NodeTransport::request_response` call instead of `Err(Unsupported)` stub
- Any future consumer needing cross-node reply (DID write coordinator response, RFC-0970 forwarding-hop envelope return, …)

**Blocked by**:
- None — pure substrate extension; all dependencies (`octo-transport` workspace deps + `octo-protocol::NodeEnvelope`) already present

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v0.1    | 2026-08-12 | open   | Mission filed. RFC-0850 cited as authoritative anchor; RFC-0970 cited as pattern source; `NetworkSender::send_request` with `correlation_id`; sidecar `CorrelationEnvelope` + magic-byte sidecar. **13 ACs.** |
| v0.2    | 2026-08-12 | open   | RFC re-check audit: RFC-0850 has zero req/resp design (only general transport); RFC-0970 has zero hits. RFC-0870 has FULL req/resp substrate design (`ForwardRequest`/`ForwardResponse`/`ForwardReject` + `PendingRequests` + `ForwardingConfig` + `ForwardRejectReason`) in `quota-router-core` mesh-specific. RFC-0871 provides correlation key via `NodeEnvelope.envelope_id` (semantic layer). Rewritten: RFC-0870 promoted to authoritative pattern source; RFC-0871 `envelope_id` is the correlation key (not `correlation_id`); RFC-0870 `PendingRequests` generalized to `octo-transport::PendingRequests` (dropped mesh-specific `origin: RouterNodeId` field, added `registered_at`); `ForwardingConfig` generalized to `RequestResponseConfig` (dropped `max_ttl`, mesh-specific); sidecar `CorrelationEnvelope` + magic-byte dropped (RFC-0871 `NodeEnvelope.envelope_id` IS the wire form, no new envelope kind). 13 ACs (consolidated, no AC count change). |
