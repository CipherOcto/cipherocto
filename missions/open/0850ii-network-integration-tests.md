# Mission: Network Protocol Integration Tests with Mock Transport

## Status

Open

## RFC

RFC-0850 through RFC-0860: Full networking stack integration testing

## Summary

Create a comprehensive integration test suite for the CipherOcto networking protocol using mock-based transports. The tests verify end-to-end protocol behavior across the full stack: DOT envelope creation → DGP gossip propagation → DRS route selection → DOM admission → ORR onion routing, using in-memory mock adapters that simulate real platform transports without network dependencies.

## Why

Current test coverage (876 tests) consists entirely of unit tests within individual modules. There are zero integration tests that verify cross-module behavior, protocol pipeline correctness, or multi-node coordination. This gap means:

- DOT → DGP → DRS pipeline correctness is untested
- Multi-node gossip convergence is unverified
- Failure scenarios (partition, replay, Byzantine) are untested at the protocol level
- Mock-based testing enables CI without external dependencies

## Acceptance Criteria

### Mock Transport Infrastructure

- [ ] `MockPlatformAdapter` implementing `PlatformAdapter` trait with in-memory message queues
- [ ] `MockNetwork` simulating N interconnected gateways with configurable topology
- [ ] Message injection and observation hooks for test assertions
- [ ] Configurable failure modes: drop, delay, duplicate, reorder

### Integration Test Directory

- [ ] `crates/octo-network/tests/` directory with integration test files
- [ ] `tests/common/mod.rs` with shared test utilities and mock infrastructure
- [ ] Each test file covers one protocol pipeline

### DOT Pipeline Tests

- [ ] Envelope creation → wire serialization → deserialization roundtrip
- [ ] Envelope signing → verification across mock adapters
- [ ] Multi-adapter delivery (same envelope sent via 2+ mock transports)

### DGP Gossip Tests

- [ ] 3-node gossip: node A floods → nodes B and C receive
- [ ] Deduplication: same object sent twice → processed once
- [ ] Canonical ordering: objects arrive out of order → processed in canonical order
- [ ] TTL expiry: object with TTL=1 → not forwarded beyond first hop

### DRS Route Selection Tests

- [ ] Route computation determinism: same inputs → same route across nodes
- [ ] Multi-path selection: 3 available paths → best path selected by score
- [ ] Trust-weighted routing: higher trust gateway preferred

### DOM Admission Tests

- [ ] Valid intent admitted → appears in pool
- [ ] Expired intent rejected
- [ ] Replay intent rejected
- [ ] Capacity exceeded → rejection

### ORR Onion Tests

- [ ] 3-hop onion: construct → peel at each hop → final payload recovered
- [ ] Wrong key at hop → decryption fails
- [ ] Cover traffic indistinguishable from real traffic

### Failure Scenario Tests

- [ ] Network partition: 2 nodes disconnected → state diverges → reconnect → state converges
- [ ] Replay attack: same envelope sent twice → second rejected
- [ ] Byzantine node: sends invalid signature → rejected by honest nodes

## Location

- `crates/octo-network/src/test_utils/mock_adapter.rs` — MockPlatformAdapter
- `crates/octo-network/src/test_utils/mock_network.rs` — MockNetwork
- `crates/octo-network/tests/` — Integration tests

## Complexity

High (3-5 days)

## Prerequisites

- All RFC 0850-0860 missions (completed)

## Reference

- RFC-0850 §8.2: PlatformAdapter trait
- RFC-0852 §4: Deterministic propagation rules
- `docs/BLUEPRINT.md`: Process architecture
