# Mission: 0870k-transport-request-response — Layer D request/response substrate

## Status

Open (filed 2026-08-12). Unblocks mission `0871b-cross-node-forwarding` (OPEN, same date).

## Problem

`octo-transport` (Layer D) today exposes only fire-and-forget:
- `NetworkSender::send(&[u8])` (sender.rs:49) — no return data
- `NodeTransport::broadcast` + `send_best` — one-way

Anything that needs a reply (cross-node resolver hop return path, identity-resolver chain forwarding, RFC-0970 forwarding-hop auth envelope return, RFC-0009 DID write coordinator response, …) is blocked. Mission `0871b-cross-domain-resolution-impl` (LANDED 2026-08-11, commit `c14c2707`) explicitly deferred cross-node forwarding because "request/response substrate that does not exist in `octo-transport` today" (per `chain.rs:33-41` module docstring). Mission `0871b-cross-node-forwarding` inherits this dependency as its hardest external blocker.

This mission lands the request/response substrate. Pure Layer D — no business logic, no envelope semantic interpretation.

## RFC anchor

- **Authoritative**: RFC-0850 (Deterministic Overlay Transport) §Deterministic Overlay substrate — general transport primitives; this mission extends the substrate with a request/response primitive.
- **Pattern reference**: RFC-0970 §Algorithms (forwarding-hop chain hash + per-hop signature) for the correlation-id pattern. RFC-0970 uses `chain_hash` to bind a forwarded request to its expected response; this mission uses the same `mission_id: [u8; 32]` field already present in `SendContext` / `ReceiveContext` as the correlation id.
- **Consumption**: RFC-0871 §Algorithms (envelope receive flow) for `ReferenceDispatcher::verify_all` integration at `node.rs:283`; the `NodeTransport::request_response` high-level API routes the response back through the same `ReceiveContext` shape.

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

No consumer of `send_request` exists yet. Mission `0871b-cross-node-forwarding` (OPEN, same date) is the first consumer (it currently has phantom references that this mission makes real).

## Traits / surface in scope (NEW)

### T1. `TransportError::Unsupported(String)` variant — additive on `sender.rs:18`

```rust
#[error("operation not supported: {0}")]
Unsupported(String),
```

Additive — no existing match arm breaks. Add a test case to `transport_error_display` test at `sender.rs:86`.

### T2. `NetworkSender::send_request` method — additive trait method with default body

```rust
#[async_trait]
pub trait NetworkSender: Send + Sync {
    async fn send(&self, payload: &[u8], context: &SendContext) -> Result<(), TransportError>;

    /// Send a request and await a correlated response.
    ///
    /// Default body returns `Err(TransportError::Unsupported("send_request not implemented"))`
    /// so existing senders (UDP adapter, PlatformAdapterBridge, …) remain
    /// source-compatible. Concrete TCP / QUIC / in-process senders override
    /// this to perform the request/response.
    ///
    /// `correlation_id` MUST be unique across in-flight requests from this
    /// node. Convention: BLAKE3(canonical_ser((source_peer, nonce))) truncated
    /// to 32 bytes — matches RFC-0970 §Algorithms chain_hash construction.
    async fn send_request(
        &self,
        _payload: &[u8],
        _context: &SendContext,
        _correlation_id: [u8; 32],
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
    /// `timeout` elapses; if timeout fires, return
    /// `TransportError::AllTransportsFailed` (existing variant — no new error).
    ///
    /// `correlation_id` is the value the receiver echoes back via
    /// `dispatch_response` (T4). Caller is responsible for generating it
    /// (RFC-0970 chain_hash construction recommended).
    pub async fn request_response(
        &self,
        payload: &[u8],
        context: &SendContext,
        correlation_id: [u8; 32],
        timeout: std::time::Duration,
    ) -> Result<Vec<u8>, TransportError>;
}
```

Internal implementation: iterate `self.senders`; first one whose `send_request` does NOT return `Unsupported` wins; call it. Wrap in `tokio::time::timeout(timeout, send_request_future).await`. On timeout → `Err(TransportError::AllTransportsFailed)`.

### T4. `NodeTransport::dispatch_response` + `register_response_handler` — receiver-side correlation

```rust
impl NodeTransport {
    /// Register an expectation for a response with the given
    /// `correlation_id`. Returns a `oneshot::Receiver<Vec<u8>>` that
    /// resolves when `dispatch_response` is called with the matching id,
    /// or the receiver is dropped (timeout / cancellation).
    pub fn register_response_handler(
        &self,
        correlation_id: [u8; 32],
    ) -> tokio::sync::oneshot::Receiver<Vec<u8>>;

    /// Called by the inbound dispatch path when a payload arrives with
    /// a correlation-id prefix. Looks up the registered handler and
    /// delivers the payload. If no handler is registered, the payload
    /// falls through to `NetworkReceiver::on_receive` (existing path).
    ///
    /// Public so test infrastructure can inject responses directly; in
    /// production the `NodeTransport` receive loop calls it after
    /// extracting the correlation id from the inbound envelope.
    pub fn dispatch_response(
        &self,
        correlation_id: [u8; 32],
        payload: Vec<u8>,
    ) -> Result<(), TransportError>;
}
```

Internal state: `Arc<tokio::sync::Mutex<HashMap<[u8; 32], oneshot::Sender<Vec<u8>>>>>`. Auto-cleanup of stale entries on `dispatch_response` failure (`Sender` dropped when the `Receiver` was cancelled).

### T5. `NodeEnvelope` correlation prefix convention — Layer A wire form

A new convention (NOT a new payload kind) — the request/response substrate reuses existing `DeterministicEnvelope` discriminator bytes:

- Outbound request payload format: `[correlation_id: 32B][inner_payload: …]`. The first 32 bytes are the correlation id; remainder is the inner envelope. `NetworkReceiver::on_receive` checks: if the payload starts with `[magic_byte = 0xRR]` (request-response marker, registered in `octo-protocol`), strip the prefix and route to `dispatch_response` if a handler is registered; else log + drop (fail-closed: unknown correlation = no delivery).
- Response payload format: same `[0xRR][correlation_id: 32B][response_payload: …]`. The receiver side uses the same prefix detection.

Magic byte `0xRR` (Request-Response) is registered in `octo-protocol/src/discriminator.rs` (or wherever RFC-0871 discriminator bytes live). Layer A change — coordinated with the protocol team; RFC-0871 amendment may follow. **If the magic-byte registration is blocked by Layer A review, fall back to a sidecar `CorrelationId { [u8; 32] }` struct passed alongside the payload** (cleaner, no wire-form change).

**Decision**: ship T5 with the sidecar `CorrelationId` shape first (no Layer A coordination); add the magic-byte convention as a follow-on amendment once `octo-protocol` reviewers confirm. This keeps 0870k Layer D-only.

### T6. Wire form = `CorrelationEnvelope { correlation_id: [u8; 32], inner: Vec<u8> }`

Layer D internal struct (NOT wire-stable, NOT in `octo-protocol`):

```rust
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct CorrelationEnvelope {
    pub correlation_id: [u8; 32],
    pub inner: Vec<u8>,
}
```

Used as the carrier between `NetworkSender::send_request` impls and `NodeTransport::dispatch_response`. Concrete TCP / QUIC senders serialize this on the wire; UDP returns `Unsupported`. In-process mock senders round-trip via a shared `Arc<Mutex<HashMap<[u8; 32], oneshot::Sender<CorrelationEnvelope>>>>`.

## Scope (acceptance criteria)

| AC | Description | Type |
|----|-------------|------|
| AC-1 | `TransportError::Unsupported(String)` variant added to `octo-transport/src/sender.rs:18-39`; new test case in `transport_error_display` at `sender.rs:86` | MODIFY enum + test |
| AC-2 | `NetworkSender::send_request` method per T2 with default body returning `Err(Unsupported(...))`; trait stays source-compatible (existing `impl NetworkSender for …` blocks in workspace compile unchanged) | MODIFY trait |
| AC-3 | `NodeTransport::request_response` high-level API per T3; iterate senders, skip `Unsupported`, wrap in `tokio::time::timeout` | NEW method |
| AC-4 | `NodeTransport::register_response_handler` + `dispatch_response` per T4 with `Arc<Mutex<HashMap<[u8;32], oneshot::Sender<Vec<u8>>>>>` internal state | NEW methods + state |
| AC-5 | `CorrelationEnvelope { correlation_id, inner }` per T6 in `octo-transport/src/correlation.rs` (NEW file); borsh round-trip test | NEW struct |
| AC-6 | Unit test: `request_response_unsupported_sender_returns_error` — `NodeTransport` with only UDP sender (no `send_request` impl) returns `Err(AllTransportsFailed)` after timeout | NEW test |
| AC-7 | Unit test: `request_response_round_trip_via_mock_sender` — in-process `MockRequestSender` (test-only `NetworkSender` impl with shared correlation map) + `NodeTransport` round-trips a `[u8; 32] correlation_id + payload` request/response pair within timeout | NEW test |
| AC-8 | Unit test: `dispatch_response_no_handler_does_not_panic` — unknown correlation id dropped silently (logged via `tracing::warn!`) | NEW test |
| AC-9 | Unit test: `register_response_handler_drop_on_caller_cancel` — caller drops the `oneshot::Receiver`; subsequent `dispatch_response` returns `Err(Unsupported("no handler"))` (or new variant — TBD) without leaking memory | NEW test |
| AC-10 | Integration TV: 3-node mock transport (A → B → C) in `tests/request_response_chain.rs`; A's request propagates B → C, C's response propagates back via correlation id; intermediate nodes use `register_response_handler` to bind the forward + return paths | NEW TV |
| AC-11 | `cargo clippy -p octo-transport --all-targets -- -D warnings` clean (verifies back-compat: every `impl NetworkSender` in workspace still compiles) | GATE |
| AC-12 | `cargo fmt --all -- --check` clean | GATE |

## Out of Scope

- **Cross-domain signing / chain hash per RFC-0970** — `correlation_id` generation follows the RFC-0970 `chain_hash` construction (documented in T2 docstring) but actual signature verification happens in the consumer (`0871b-cross-node-forwarding`), not here. This mission lands the substrate; RFC-0970 adoption is downstream.
- **Magic-byte discriminator in `octo-protocol`** — sidecar `CorrelationEnvelope` first; magic-byte registration is a follow-on Layer A amendment (deferred per T5).
- **Wire-stable protocol surface for the response** — the in-process + TCP impls use `CorrelationEnvelope`; cross-language interop (gRPC / HTTP) is a follow-on.
- **Production-grade timeout/retry policy** — fixed `Duration` timeout, no retries. Production retry policy lives in the consumer (`0871b-cross-node-forwarding` for the resolver chain case).
- **Authentication on `send_request`** — sender's peer identity is in `SendContext.source_peer`; the response is correlated by id, not authenticated by sender. Auth at the envelope semantic layer (RFC-0871 `ReferenceDispatcher::verify_all`) is the consumer's responsibility.

## Cross-references

- RFC-0850 (Deterministic Overlay Transport) §Deterministic Overlay substrate — authoritative anchor
- RFC-0871 §Algorithms (envelope receive flow) — for downstream `ReferenceDispatcher::verify_all` integration
- RFC-0970 §Algorithms (chain_hash + forwarding-hop pattern) — `correlation_id` generation pattern
- RFC-0009 §Identity substrate — requester identity in `SendContext.source_peer`
- `octo-transport/src/sender.rs:18-39` — `TransportError` enum (variant added here)
- `octo-transport/src/sender.rs:47-56` — `NetworkSender` trait (method added here)
- `octo-transport/src/sender.rs:5-14` — `SendContext { mission_id, priority, source_peer, origin_gateway }` (correlation_id sourced from `mission_id`)
- `octo-transport/src/receiver.rs:7-14` — `ReceiveContext { source_transport, mission_id, sender_id }` (correlation_id echoed via `mission_id`)
- `octo-transport/src/receiver.rs:22-32` — `NetworkReceiver` trait (response dispatch via existing `on_receive` path; correlation extracted upstream)
- `octo-transport/src/node_transport.rs:14,20,30` — `NodeTransport` (high-level API added here)
- `octo-transport/Cargo.toml:14,18` — `futures = "0.3"`, `tokio = "1"` (oneshot + time available)
- `crates/octo-identity-resolver-node/src/handlers/chain.rs:33-41` — explicit OUT OF SCOPE note for cross-node forwarding; this mission unblocks it
- Mission `0871b-cross-node-forwarding` (OPEN, 2026-08-12) — first consumer of this substrate; `RemoteResolverBackend::resolve_via` switches from `Err(Unsupported)` stub to a real `send_request` call once this mission lands
- Mission `0871b-cross-domain-resolution-impl` (LANDED 2026-08-11, commit `c14c2707`) — declared this dependency
- Mission `0870j-udp-adapter-for-gossip-broadcast` (CLOSED 2026-08-07) — UDP adapter inherits default `send_request = Err(Unsupported)` (UDP is fire-and-forget by design)
- Mission `0870-c-envelope-dispatch-compat` (OPEN) — sibling; envelope dispatch substrate

## Layer Discipline

- `octo-transport` (Layer D) — substrate only; no business logic
- No new Cargo deps (uses existing `tokio` + `borsh` already in workspace)
- No upper-layer (`octo-protocol` / `octo-ident`) dependency introduced
- `octo-protocol` (Layer A) UNCHANGED — magic-byte registration deferred to follow-on amendment

## Dependency

**This mission unblocks**:
- `0871b-cross-node-forwarding` — `RemoteResolverBackend::resolve_via` becomes a real `NodeTransport::request_response` call instead of `Err(Unsupported)` stub
- Any future consumer needing cross-node reply (DID write coordinator response, RFC-0970 forwarding-hop envelope return, …)

**Blocked by**:
- None — pure substrate extension; all dependencies (`octo-transport` workspace deps) already present

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v0.1    | 2026-08-12 | open   | Mission filed. Unblocks `0871b-cross-node-forwarding`. Substrate: `NetworkSender::send_request` (default `Unsupported`) + `NodeTransport::request_response` + correlation-id dispatch via `register_response_handler` + `CorrelationEnvelope` sidecar (no Layer A magic-byte registration this round). 12 ACs. |
