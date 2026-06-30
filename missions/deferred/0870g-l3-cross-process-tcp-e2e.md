# Mission: 0870g — L3 Cross-Process TCP E2E Tests + Performance Benchmarks

> **STATUS: DEFERRED — pending real design discussion**
>
> This mission previously proposed spawning a separate `quota-router-node` binary from the test harness to exercise cross-process TCP behavior. That binary did not exist in production — it was a test fixture masquerading as production code, and the cross-process tests it "supported" were theatre, not verification.
>
> This mission is preserved here as a record of the original intent (cross-process end-to-end coverage for the quota router mesh) and as the starting point for a future design discussion. It must **not** be re-implemented by re-introducing a test-only binary.

## Status

Deferred (2026-06-30) — pending design discussion; see RFC-0870 v1.13 §Test Policy and §Cross-process boundary.

## RFC

RFC-0870 (Networking): Distributed Quota Router Network

## Why this is deferred

The original mission targeted cross-process behavior — multiple OS processes, each running a `quota-router` node, communicating over real TCP. That is a legitimate goal, but the proposed implementation was not:

- A `quota-router-node` binary was introduced under `quota-router-e2e-tests/quota-router-node/` so that the test harness could spawn it as a child process. **No such binary existed in production.** The library `quota-router` is consumed by the Python SDK (PyO3) and the quota-router CLI; it does not ship a standalone daemon.
- Tests then ran against that fake binary. They verified that the binary started, accepted TCP connections, and forwarded bytes — but they did **not** verify the production library's behavior because the production library was never directly exercised through the cross-process boundary.
- This violates RFC-0870 v1.13 §Test Policy: tests must target the production library. Cross-process behavior, when supported, must be supported **by the library itself** — not by a parallel test-only binary.

## Open design questions (must be resolved before re-scoping)

The mission cannot land as L3 cross-process tests until these questions have design answers:

1. **Cross-process trust boundary.** Who authenticates which process? Currently peer ids (`RouterNodeId`) live in `ReceiveContext.sender_id`; in a real cross-process deployment, that field must be populated from the wire (e.g., from a TLS peer certificate or a signed handshake). There is no spec for that today.
2. **Sender-id wire framing.** L3 wire format must include a sender-id prefix or equivalent authentication mechanism so `QuotaRouterHandler` can look up the sender's `PeerTrust`. The current L2 in-process harness synthesizes sender_id from the mpsc envelope; a real wire format is needed.
3. **Deployment model.** Does CipherOcto ship a `quota-router` daemon binary in the future? If yes, it belongs at `crates/quota-router-node/` (a real workspace member, not in a tests folder), and the production library must support the necessary outbound/inbound transports end-to-end. If no, cross-process behavior is out of scope and the mesh is always in-process.
4. **TCP adapter contract.** RFC-0850 §8.8 defines `PlatformType::Tcp`. The `octo-adapter-tcp` crate exists. But it has not been wired into the quota router library — only into the fake `quota-router-node` binary. Once Mission 0870i (TCP adapter) is properly implemented, cross-process deployment is mechanically possible; until then, it is not.
5. **Test environment cost.** Even after the above are answered, spawning OS processes in CI has cost (memory, time, flakiness). The test design must justify this cost by exercising **production behavior** that the L2 in-process harness cannot exercise (e.g., real serialization across process boundaries, real TCP backpressure, real OS-level connection management).

## Acceptance Criteria

Deliberately left blank. These must be re-derived from a real design discussion that resolves the open questions above. The L2 test harness (`quota-router-e2e-tests/src/lib.rs`) covers the equivalent in-process behaviors and must remain green throughout any cross-process work.

## Complexity

TBD — gated on the design discussion.

## Reference

- RFC-0870 v1.13 — Test Policy (forbids fake tests)
- RFC-0870 v1.13 — Cross-process boundary section (defines what this mission must answer)
- Mission 0870i — TCP adapter for quota router (also deferred)
- Mission 0870f — L2 in-process multi-node e2e (the tests we **do** keep; covers the same behaviors in-process)

## Implementation Notes

- **Do not** reintroduce the `quota-router-node/` binary under `quota-router-e2e-tests/`. If a future design needs a daemon, it lives at `crates/quota-router-node/` and is a real workspace member.
- **Do not** write L3 tests that spawn subprocesses of test-only binaries.
- When cross-process testing is unblocked, the tests must exercise the production library's `PlatformAdapter` integration (e.g., the real `TcpAdapter` from `octo-adapter-tcp`) — not a parallel fixture.
- L2 in-process tests are the canonical coverage for routing, gossip, HMAC, and lifecycle until L3 lands. Any new in-process test that targets behaviors covered by L2 must extend the L2 harness, not spawn a new harness.
