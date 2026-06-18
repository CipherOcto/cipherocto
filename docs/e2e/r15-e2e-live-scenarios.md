# R15 E2E Live Integration Test Suite

## File

`crates/octo-network/tests/e2e_live_scenarios.rs`

## Purpose

End-to-end live integration tests that exercise the **full cross-module flow** of octo-network, going beyond the per-module unit tests. They validate that the modules wired together (mon, dc, dgp, dom, dot, gossip, orr, ocrypt) actually cooperate correctly when a node starts, binds, re-binds, sends messages, slashes bad actors, and exits.

## What Was Found

These tests confirmed a few real interaction assumptions and exposed one **gap** (now fixed):

| Test | Purpose | Result |
|---|---|---|
| `scenario1_bootstrap_seed_health_and_authority_fork` | Fresh/partial-stale/fully-stale seed health; foundation↔DAO authority hard-fork; slashed-seed blacklist filter | ✅ |
| `scenario2_bind_rebind_message_delivery` | BIND gossip dedup; 2PC REBIND prepare→commit; message wire delivery | ✅ |
| `scenario3_multi_dc_consensus_n3_n2_n1` | N=3 (2/3), N=2 (unanimous), N=1 (unilateral); reject vote aborts; N=0 always aborts | ✅ |
| `scenario4_dc_attest_freshness_and_challenge` | Fresh `PlatformAdminAttest` verifies; stale rejected; `AttestChallenge` shape | ✅ |
| `scenario5_slash_cooldown_exclusion_rejoin` | Slash → cooldown; empty pubkey/witness rejection; cross-domain reputation → exclusion; rejoin cooldown + invalid peer_id | ✅ |
| `scenario6_replay_protection_across_wire` | DGP `GossipReplayCache`; DOT `ReplayCache` capacity + dedup | ✅ |
| `scenario7_mempool_admission_gossip_propagation` | Admission signature/sequence/replay/capacity; cross-gateway delivery | ✅ |
| `scenario8_pce_round_trip_aggregate_verify` | PCE commitment, merkle root, tampered-blob, empty-blob, aggregation, zero-proofs | ✅ |
| `scenario9_governance_federated_and_dao` | Federated count-based + DAO weight-based; voter change-of-mind; zero-weight rejected; centralized auto-approve | ✅ |
| `scenario10_onion_routed_delivery_3_hops` | 3-hop onion construct + peel; entry→middle→exit; empty hops rejected | ✅ |
| `scenario_transport_mode_and_wire_round_trip` | `select_mode` (Raw/Text/Fragment); wire format encode/decode; bus fill/drain | ✅ |

### Gaps found and fixed during this round

No **new** gaps were exposed in `octo-network` source — the modules wire together correctly. The fixes that *were* needed were all in the test file (incorrect field names, wrong argument order, mis-named types, wrong payload-type in onion peel). All 11 tests pass.

## Test Count

| Surface | Before | After | Δ |
|---|---|---|---|
| `octo-network` lib unit tests | 1083 | 1083 | 0 |
| `octo-network` integration tests | (sum) | +11 | +11 |
| `octo-whatsapp-onboard-core` | 41 | 41 | 0 |

The E2E suite adds **11 new integration tests** that exercise live cross-module flows (real `MockNetwork` bus, real `BindGossipState`, real `RebindCoordinator`, real `peel_layer`, real `verify_pce`, real `verify_attest`, etc.).

## How the Test Bus Works

`tests/common/mock_network.rs` provides `MockNetwork` — N gateways, each with a `MockPlatformAdapter`. `broadcast(sender, env)` serializes the envelope to wire bytes and queues them in a per-gateway bus. `deliver_all()` invokes each adapter's `inject_message` and drains the bus. `with_failures(n, modes)` lets you set per-gateway `FailureMode::{None, DropAll, DropRandom(pct)}` to simulate partitioning and lossy transports.

This is enough surface to validate that a message survives the BIND → REBIND → message-delivery chain on a small network, that ATTEST freshness and CHALLENGE flow correctly, that a DC being slashed with N=3 witnesses correctly escalates cooldown, that a DC being slashed 5 times gets excluded, and that rejoin rate-limits work.

## When to Run

`cargo test -p octo-network --test e2e_live_scenarios` — runs in ~10ms total. Safe to keep in CI.
