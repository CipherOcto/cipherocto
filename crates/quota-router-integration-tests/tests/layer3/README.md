# Layer 3: Cross-Process TCP Tests

## Prerequisites

- Built CLI binary: `cargo build -p quota-router-cli`
- Available TCP ports (tests use ephemeral ports in 19100-19400 range)

## Running

```sh
# Run all L3 tests
cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
  -- --ignored l3_

# Run a specific test
cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
  -- --ignored l3_cross_process_gossip
```

## What's Tested

- **l3_cross_process_gossip**: Sends a `CapacityGossip` envelope from an in-process node through `TcpAdapter` to a remote `quota-router serve` process. Verifies the message arrives and is processed by the handler.
- **l3_cross_process_forward**: Routes a request through the TCP mesh, verifying local dispatch works across process boundaries.

## Architecture

```
In-process node ──TcpAdapter──► quota-router serve (process A)
                                      │
                                      ▼
                               quota-router serve (process B)
```

Each `quota-router serve` process runs:
- A `TcpAdapter` bound to an ephemeral port
- A `QuotaRouterNode` with mock provider
- Gossip/announce background loops

The test runner creates an in-process `QuotaRouterNode` with a `TcpAdapter` connected to process A, then sends messages through the mesh.
