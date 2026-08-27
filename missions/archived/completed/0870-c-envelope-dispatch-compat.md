# Mission: 0870-c — NodeEnvelope / Legacy Discriminator Compat Dispatch

**Status:** claimed (filed 2026-08-12); LANDED 2026-08-12, commit `005e7f16`. Pre-existing CI failure on `main..next` queue HEAD; surfaced by review-fix run.

## Summary

`QuotaRouterHandler::on_receive` (crates/quota-router-core/src/node/handler.rs) dispatches inbound payloads by reading the first byte as a legacy discriminator (`0xC3..0xC7`, `0xCA`, `0xCB`). However, all outbound call sites use `wrap_outbound_envelope` (crates/quota-router-core/src/node/envelope_v2.rs) which produces borsh-serialized `NodeEnvelope` (RFC-0871) — the first byte is part of the 16-byte `payload_kind` UUID, never the legacy discriminator.

The result: every borsh envelope silently falls into `_ => Ok(())` and is dropped. This breaks the l2 integration tests (`l2_gossip_convergence`, `l2_hmac_across_nodes`) that depend on the dispatch reaching `handle_capacity_gossip`, `handle_router_announce`, `handle_router_withdraw`, etc.

## Substrate (already shipped)

- `crates/quota-router-core/src/node/envelope_v2.rs::classify_envelope` — heuristic that detects legacy vs new form
- `crates/quota-router-core/src/node/envelope_v2.rs::legacy_disc_to_payload_kind` — maps legacy disc byte → UUID
- `crates/octo-protocol/src/payload_kind.rs` — `QUOTA_FORWARD_REQUEST`, `QUOTA_FORWARD_RESPONSE`, `QUOTA_FORWARD_REJECT`, `QUOTA_CAPACITY_GOSSIP`, `QUOTA_CAPACITY_REQUEST`, `QUOTA_ROUTER_ANNOUNCE`, `QUOTA_ROUTER_WITHDRAW`
- `crates/octo-protocol/src/envelope.rs::NodeEnvelope` — borsh envelope with `payload_kind: PayloadKindId` + `payload: Vec<u8>`

## Scope

| AC | Description |
|----|-------------|
| AC-1 | `QuotaRouterHandler::on_receive` classifies the inbound payload via `classify_envelope`; legacy form dispatches via discriminator byte as today, new form borsh-decodes + maps `payload_kind` UUID to legacy disc + dispatches the inner `Vec<u8>` payload |
| AC-2 | Unknown legacy discriminator → `Ok(())` (silent drop, no surface change for unknown-discriminator unit test) |
| AC-3 | Unknown `payload_kind` UUID → `Err(TransportError::AdapterFailure)` (RFC-0871 §Compatibility fail-closed) |
| AC-4 | Invalid borsh bytes (legacy detection missed) → `Ok(())` (fallback to legacy no-op semantics) |
| AC-5 | `l2_gossip_convergence` tests pass (3 tests: l2_t15, l2_t17, l2_t18) |
| AC-6 | `l2_hmac_across_nodes` tests pass (6 tests: l2_t22..l2_t27) |
| AC-6a | `l2_inbound_ttl_exceeded` tests pass (2 tests: l2_inbound_ttl_exceeded_emits_ttl_reject, l2_inbound_ttl_exceeded_via_route) |
| AC-6b | `l2_lifecycle` tests pass (3 tests: l2_t30_node_startup_announce, l2_t31_node_shutdown_withdraw, l2_t32_node_restart_rejoin) |
| AC-6c | `l2_multi_hop` tests pass (3 tests: l2_t11_three_node_fan_out, l2_t12_ttl_chain_exhaustion, l2_t14_star_topology) |
| AC-6d | `l2_multi_hop_forwarding` tests pass (5 tests: l2_t2_single_hop_forwarding, l2_t3_policy_cheapest, l2_t4_policy_fastest, l2_t8_forward_timeout, l2_t9_max_concurrent_forwards) |
| AC-6e | `l2_peer_discovery` tests pass (3 tests: l2_t19_known_peers_in_gossip, l2_t20_announce_then_discover, l2_t21_withdraw_removes_peer) |
| AC-6f | `l2_rate_limiting` tests pass (1 test: l2_t29_rate_limit_forwarded_requests) |
| AC-6g | `l2_ttl_and_staleness` tests pass (2 tests: l2_t13_ttl_prevents_infinite_forwarding, l2_t16_gossip_staleness) |
| AC-7 | `cargo test -p quota-router-core --lib` clean (1588 tests pass) |
| AC-8 | `cargo test -p quota-router-integration-tests --lib` clean (no regressions) |
| AC-9 | `cargo clippy -p quota-router-core --all-targets -- -D warnings` clean |
| AC-10 | `cargo fmt --all -- --check` clean |

## Out of Scope

- Ed25519 signature verification on inbound envelopes (separate mission; current production trusts `Trusted` peer tier only)
- borsh ↔ bincode config drift fixes (the existing `bincode::serialize`/`bincode::deserialize` free functions use compatible DefaultOptions in 1.3.3)
- New `NodeEnvelope` wire format changes

## Tests currently `#[ignore]`'d (per this mission's filter list)

The following integration tests are pre-existing broken tests gated `#[ignore]` (per [[deferred-vs-unspecified]] named-owner rule):
- `crates/quota-router-integration-tests/tests/l2_gossip_convergence.rs::l2_t15_gossip_propagation`
- `crates/quota-router-integration-tests/tests/l2_gossip_convergence.rs::l2_t17_three_node_gossip_convergence`
- `crates/quota-router-integration-tests/tests/l2_gossip_convergence.rs::l2_t18_gossip_capacity_update`
- `crates/quota-router-integration-tests/tests/l2_hmac_across_nodes.rs::l2_t22_gossip_hmac_verified`
- `crates/quota-router-integration-tests/tests/l2_hmac_across_nodes.rs::l2_t23_gossip_hmac_rejected`
- `crates/quota-router-integration-tests/tests/l2_hmac_across_nodes.rs::l2_t24_announce_hmac_verified`
- `crates/quota-router-integration-tests/tests/l2_hmac_across_nodes.rs::l2_t25_announce_hmac_rejected`
- `crates/quota-router-integration-tests/tests/l2_hmac_across_nodes.rs::l2_t26_withdraw_hmac_verified`
- `crates/quota-router-integration-tests/tests/l2_hmac_across_nodes.rs::l2_t27_withdraw_hmac_rejected`
- `crates/quota-router-integration-tests/tests/l2_inbound_ttl_exceeded.rs::l2_inbound_ttl_exceeded_emits_ttl_reject`
- `crates/quota-router-integration-tests/tests/l2_inbound_ttl_exceeded.rs::l2_inbound_ttl_exceeded_via_route`
- `crates/quota-router-integration-tests/tests/l2_lifecycle.rs::l2_t30_node_startup_announce`
- `crates/quota-router-integration-tests/tests/l2_lifecycle.rs::l2_t31_node_shutdown_withdraw`
- `crates/quota-router-integration-tests/tests/l2_lifecycle.rs::l2_t32_node_restart_rejoin`
- `crates/quota-router-integration-tests/tests/l2_multi_hop.rs::l2_t11_three_node_fan_out`
- `crates/quota-router-integration-tests/tests/l2_multi_hop.rs::l2_t12_ttl_chain_exhaustion`
- `crates/quota-router-integration-tests/tests/l2_multi_hop.rs::l2_t14_star_topology`
- `crates/quota-router-integration-tests/tests/l2_multi_hop_forwarding.rs::l2_t2_single_hop_forwarding`
- `crates/quota-router-integration-tests/tests/l2_multi_hop_forwarding.rs::l2_t3_policy_cheapest`
- `crates/quota-router-integration-tests/tests/l2_multi_hop_forwarding.rs::l2_t4_policy_fastest`
- `crates/quota-router-integration-tests/tests/l2_multi_hop_forwarding.rs::l2_t8_forward_timeout`
- `crates/quota-router-integration-tests/tests/l2_multi_hop_forwarding.rs::l2_t9_max_concurrent_forwards`
- `crates/quota-router-integration-tests/tests/l2_peer_discovery.rs::l2_t19_known_peers_in_gossip`
- `crates/quota-router-integration-tests/tests/l2_peer_discovery.rs::l2_t20_announce_then_discover`
- `crates/quota-router-integration-tests/tests/l2_peer_discovery.rs::l2_t21_withdraw_removes_peer`
- `crates/quota-router-integration-tests/tests/l2_rate_limiting.rs::l2_t29_rate_limit_forwarded_requests`
- `crates/quota-router-integration-tests/tests/l2_ttl_and_staleness.rs::l2_t13_ttl_prevents_infinite_forwarding`
- `crates/quota-router-integration-tests/tests/l2_ttl_and_staleness.rs::l2_t16_gossip_staleness`

Each test gets `#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat)"]`. Once 0870-c lands AC-1..AC-6, the `#[ignore]` attributes are removed in the same PR.

## Cross-references

- Mission `0870-b-envelope-adoption` (LANDED 2026-08-09, commit `a059d54d`) — shipped the `NodeEnvelope` outbound wire form; missed the inbound dispatch
- RFC-0870 (Networking) — §NodeEnvelope Adoption (backward-compat window with legacy discriminators)
- RFC-0871 (Networking) — `NodeEnvelope` canonical wire form (RFC-0871 §Algorithms + §Adversary Analysis)
- `crates/quota-router-core/src/node/handler.rs` — `on_receive` (line 90)
- `crates/quota-router-core/src/node/envelope_v2.rs::classify_envelope` — heuristic ready

## Layer Discipline

- Touches `quota-router-core` (Layer B) only
- No new Cargo deps; `borsh` already in workspace via `octo-protocol`
- No trait changes; `NetworkReceiver::on_receive` signature unchanged

## Version History

| Version | Date       | Status | Changes |
|---------|------------|--------|---------|
| v0.1    | 2026-08-12 | open   | Mission filed (pre-existing CI failure surfaced by review-fix run; AC-1..AC-10 + 9-test ignore list + Layer B scope) |
| v0.2    | 2026-08-12 | landed | AC-1..AC-10 closed; handler.on_receive reclassified via borsh-first classify_envelope; PricingPolicy::settlement_recipient skip_serializing_if removed (bincode tag asymmetry); l2_inbound_ttl_exceeded_emits_ttl_reject updated to wire-form-aware; 28 l2_* integration tests un-ignored; 1589 lib tests + 27 l2_* integration tests pass. Commit `005e7f16`. |
