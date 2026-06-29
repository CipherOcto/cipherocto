# Mission: 0870g — L3 Cross-Process TCP E2E Tests + Performance Benchmarks

## Status

Claimed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types
- 0870b (must complete first) — gossip, HMAC signing
- 0870c (must complete first) — handler, route API
- 0870d (must complete first) — HMAC verification, rate limiting, metrics
- 0870f (must complete first) — L2 in-process e2e tests (harness reusable here)

## Summary

Extend the `quota-router-e2e-tests/` crate with L3 tests that spawn multiple OS processes, each running a `quota-router-node` binary, connected via real TCP transport. Tests verify the full production stack: TCP serialization, connection management, peer discovery over the wire, request forwarding over TCP, and graceful shutdown. Also includes performance benchmarks targeting the RFC's <100ms p50 3-hop forwarding requirement.

This mirrors the stoolap sync L4 pattern (`sync-e2e-tests/tests/l4_cross_process.rs` + `stoolap-node/` binary).

## Design

### New files in `quota-router-e2e-tests/`

```
quota-router-e2e-tests/
├── src/lib.rs                      # (from 0870f) TestNode, InProcessTransport, etc.
├── tests/
│   ├── l2_basic_routing.rs         # (from 0870f)
│   ├── l2_multi_hop.rs             # (from 0870f)
│   ├── l2_gossip_convergence.rs    # (from 0870f)
│   ├── l2_peer_discovery.rs        # (from 0870f)
│   ├── l2_hmac_across_nodes.rs     # (from 0870f)
│   ├── l2_rate_limiting.rs         # (from 0870f)
│   ├── l2_lifecycle.rs             # (from 0870f)
│   ├── l3_tcp_basic.rs             # NEW: TCP forwarding basics
│   ├── l3_tcp_multi_hop.rs         # NEW: TCP multi-hop chains
│   ├── l3_tcp_partition.rs         # NEW: Partition and heal
│   ├── l3_tcp_lifecycle.rs         # NEW: Crash and restart
│   └── l3_benchmarks.rs            # NEW: Performance benchmarks
└── quota-router-node/
    ├── Cargo.toml
    └── src/
        └── main.rs                 # Minimal binary wrapping QuotaRouterNode
```

### `quota-router-node` binary

A minimal process that:
- Takes `--node-id`, `--listen-addr`, `--peer addr1,addr2,...`, `--provider model1,model2,...`, `--network-key hex`, `--gossip-interval ms`
- Constructs a `QuotaRouterNode` via the builder
- Starts listening on `--listen-addr` via `NodeTransport`
- Connects to peers listed in `--peer`
- Runs until SIGTERM (graceful shutdown with withdraw broadcast)

Binary crate dependencies: `quota-router`, `octo-transport`, `tokio`, `clap` (for CLI parsing), `tracing` + `tracing-subscriber` (for logging to stderr).

Must be added to main workspace `Cargo.toml` `exclude` list (same as `quota-router/`).

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "quota-router-node")]
struct CliArgs {
    #[arg(long)]
    node_id: String,
    #[arg(long)]
    listen_addr: String,
    #[arg(long, value_delimiter = ',')]
    peers: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    providers: Vec<String>,
    #[arg(long)]
    network_key: String,
    #[arg(long, default_value = "10000")]
    gossip_interval: u64,
}
```

### `TcpTransport` for tests

The test harness spawns child processes and communicates via TCP. Use `tokio::net::TcpStream` directly or the `octo-transport` TCP layer if available.

```rust
pub struct TcpTestNode {
    pub process: Child,
    pub addr: SocketAddr,
    pub stream: TcpStream,
}
```

## Concrete Test Cases

### l3_tcp_basic.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L3-T1: two_node_tcp_roundtrip` | Spawn 2 `quota-router-node` processes. Node A has gpt-4o. Node B routes gpt-4o request to A via TCP. Verify response received. | 2 processes (TCP) |
| `L3-T2: three_node_tcp_fan_out` | Spawn 3 processes. Node A has gpt-4o. Node B has claude-3. Node C has gemini-pro. Consumer routes to hub node → hub selects best peer → TCP forward → response. | 3 processes (TCP) |
| `L3-T3: tcp_local_dispatch` | Single process. Consumer routes request → node dispatches locally (no forwarding needed). Verify response without TCP. | 1 process |

### l3_tcp_multi_hop.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L3-T4: tcp_three_hop_chain` | A→B→C chain over TCP. Consumer routes via A → A forwards to B → B forwards to C → C dispatches locally → response returns via B → A. | 3 processes (TCP, line) |
| `L3-T5: tcp_ttl_exhaustion` | A→B→C→D chain. TTL=2. Request dies at B (TTL exhausted). Verify `ForwardReject` returned. | 4 processes (TCP, line) |

### l3_tcp_partition.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L3-T6: tcp_partition_and_heal` | 3 processes. Kill Node B. A keeps forwarding to C. Restart B. Verify B re-joins via gossip after restart. | 3 processes (TCP) |
| `L3-T7: tcp_partial_partition` | 3 processes. Disconnect A↔B. A routes to C (still connected). B routes to C. Verify both work independently. | 3 processes (TCP, partial) |

### l3_tcp_lifecycle.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L3-T8: process_crash_and_restart` | 2 processes. Kill Node B (SIGKILL). A routes to B → timeout → fallback to local or error. Restart B. A routes to B → success. | 2 processes (TCP) |
| `L3-T9: graceful_shutdown_withdraw` | 2 processes. Kill Node A (SIGTERM). Verify B receives withdraw and removes A from peer cache. | 2 processes (TCP) |

### l3_benchmarks.rs

| Test | Description | Target |
|------|-------------|--------|
| `L3-B1: local_dispatch_latency` | 1000 sequential local dispatches. Measure p50/p95/p99. | p50 < 5ms |
| `L3-B2: single_hop_forwarding_latency` | 1000 requests forwarded over TCP to a peer. Measure p50/p95/p99. | p50 < 50ms |
| `L3-B3: three_hop_forwarding_latency` | 1000 requests through A→B→C chain over TCP. Measure p50/p95/p99. | p50 < 100ms |
| `L3-B4: gossip_broadcast_latency` | Broadcast gossip to 8 peers. Measure time to deliver to all. | < 10ms |
| `L3-B5: concurrent_routing_throughput` | 100 concurrent requests through 3-node chain. Measure total throughput (req/s). | > 500 req/s |
| `L3-B6: select_destinations_benchmark` | Score 100 providers. Measure time per call. | < 1ms |

## Acceptance Criteria

- [ ] `quota-router-e2e-tests/quota-router-node/Cargo.toml` exists (minimal binary crate)
- [ ] `quota-router-e2e-tests/quota-router-node/src/main.rs` exists with CLI args and node lifecycle
- [ ] `l3_tcp_basic.rs` — 3 tests (T1–T3)
- [ ] `l3_tcp_multi_hop.rs` — 2 tests (T4–T5)
- [ ] `l3_tcp_partition.rs` — 2 tests (T6–T7)
- [ ] `l3_tcp_lifecycle.rs` — 2 tests (T8–T9)
- [ ] `l3_benchmarks.rs` — 6 benchmarks (B1–B6)
- [ ] All L3 tests pass with `cargo test -p quota-router-e2e-tests --test l3_*`
- [ ] Benchmarks report p50/p95/p99 latencies and throughput
- [ ] `cargo clippy -p quota-router-e2e-tests -- -D warnings` clean
- [ ] CI workflow updated to run L3 tests (may need to be manual/nightly due to process spawning)

## Complexity

High (~1200-1500 lines). Binary crate + 9 L3 tests + 6 benchmarks.

## Implementation Notes

- Use `std::process::Command` to spawn child processes (not `tokio::process` — simpler for test harness)
- Each test should use `tempfile::TempDir` for node data (if persistent state is added later)
- Process cleanup: use `Drop` impl on `TcpTestNode` that sends SIGTERM then waits, with SIGKILL fallback
- For partition tests: drop the `TcpStream` to simulate network failure, then reconnect
- Benchmarks use `std::time::Instant` for timing (not criterion — keep dependencies minimal)
- The `quota-router-node` binary should log to stderr (test harness captures stdout)
- TCP tests need proper port allocation — use `TcpListener::bind("127.0.0.1:0")` to get ephemeral ports
- For gossip convergence in TCP tests, the test harness needs to periodically send messages to trigger gossip delivery (since gossip is push-based)
- Consider adding a `--gossip-interval ms` flag to the binary for faster convergence in tests

## Type Coverage

This is a **testing mission** that exercises the full production stack.

| Component | Types exercised |
|-----------|-----------------|
| TCP transport | `NodeTransport` (TCP mode), `SendContext`, `ReceiveContext` |
| Node lifecycle | `QuotaRouterNode`, `QuotaRouterNodeBuilder`, `RouterNodeLifecycle` |
| Request routing | `RequestContext`, `RoutingPolicy` |
| Forwarding | `ForwardRequestPayload`, `ForwardResponsePayload`, `ForwardRejectPayload` |
| Gossip | `CapacityGossipPayload`, `GossipCache` |
| Peer management | `RouterAnnouncePayload`, `RouterWithdrawPayload` |
| HMAC | `SignedPayload`, `compute_hmac`, `verify_hmac` |
| Metrics | `QuotaRouterMetrics` (observed during benchmarks) |
