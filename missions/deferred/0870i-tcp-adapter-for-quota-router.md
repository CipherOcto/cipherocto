# Mission: 0870i — TCP Adapter for Quota Router E2E Tests

> **STATUS: DEFERRED — depends on Mission 0870g**

This mission was originally scoped as the TCP transport adapter supporting the L3 cross-process tests in Mission 0870g. Because Mission 0870g is itself deferred (the original implementation relied on a fake `quota-router-node` binary), this mission has the same blocking issue plus several others, and is deferred with it.

## Status

Deferred (2026-06-30) — depends on Mission 0870g being unblocked; see that mission for the open design questions.

## RFCs

- RFC-0850 (Networking): Deterministic Overlay Transport — §8.8 TCP Transport Profile
- RFC-0863 (Networking): General-Purpose Network Integration — `PlatformAdapter` bridge
- RFC-0870 (Networking): Distributed Quota Router Network — transport integration

## Why this is deferred

- TCP transport adapter work is meaningful in its own right. The original problem is that this mission's value was framed as "needed to make L3 cross-process tests work" — and those tests were theatre against a fake binary. With Mission 0870g deferred, this mission no longer has a forcing function and must be re-scoped around a real cross-process deployment model.
- The original mission proposed `crates/octo-adapter-tcp/` (a real workspace member — that part was correct). The fake-binary dependency was the problem, not the adapter crate.

## Open design questions

In addition to the ones in Mission 0870g:

1. **Where does the `TcpAdapter` plug into the library?** Options: (a) directly into `QuotaRouterNode::builder()` as one of several `NetworkSender` / `NetworkReceiver` impls; (b) via `PlatformAdapterBridge` per the original mission; (c) deferred entirely until a deployment binary exists. Each has different effects on `QuotaRouterNodeBuilder::build()` (which today constructs `LocalProviderSender` senders).
2. **Inbound handling.** Today the library has `QuotaRouterNode::receive()` as the inbound public API (RFC-0870 v1.13). A real `TcpAdapter` would feed that API. Until the adapter exists, the inbound API is exercised only by the L2 in-process harness, which is sufficient for in-process testing.
3. **TLS.** RFC-0853 is a separate concern. For a real cross-process deployment, the TCP adapter likely needs TLS via `rustls` (already in the workspace). This adds complexity that the original mission deferred to "future" but should be a first-class part of any re-scoping.

## Acceptance Criteria

Deliberately left blank. These must be re-derived from a real design discussion that resolves the open questions above and in Mission 0870g.

## Complexity

TBD — gated on the design discussion.

## Reference

- Mission 0870g — L3 cross-process TCP (also deferred)
- RFC-0870 v1.13 — §Public API, §Test Policy
- RFC-0850 §8.8 — TCP Transport Profile (the underlying platform type definition)

## Implementation Notes

- **Do not** wire `TcpAdapter` into a `quota-router-node/` test binary. Until the library supports TCP natively, this is not a real test.
- The `crates/octo-adapter-tcp/` directory and a stub `TcpAdapter` may already exist. If they do, they should be left untouched (or completed as a library component), but integration into the quota router library must wait for the design discussion.
- TLS via `rustls` is a prerequisite for any production cross-process deployment, even though the original mission deferred it.
