# Reputation Federation Guide

> Mission 0855p-b (cross-mission reputation) + RFC-0968-A1 amendments 22, 28, 29.
> Status: canonical (mission claimed 2026-06-16; 0968 Phase 4 storage substrate shipped 2026-07-27).

This guide explains how reputation events flow across the CipherOcto
mesh. It is the operator-facing companion to RFC-0968-A1 §12 and
mission 0855p-b's acceptance criteria.

## Architecture

```mermaid
graph LR
    RA[Recorder A<br/>mon:test] -->|publish| GS[/dot/reputation/{did}/]
    RA -->|record_signal| STO_A[(Stoolap store<br/>Node A)]
    GS -->|gossipsub| SUB[Substrate<br/>Node B]
    ATT1[Attestor 1] -->|Attestation| SUB
    ATT2[Attestor 2] -->|Attestation| SUB
    ATT3[Attestor 3] -->|Attestation| SUB
    SUB -->|record_signal| STO_B[(Store<br/>Node B)]
    SUB -->|record_attestation| STO_B
    Q[attestor_quorum_reached?] -->|≥ 3| EL[Election eligible]
```

The recorder publishes a `SignalEvent` wrapped in a `GossipEnvelope`
on its DID-keyed topic. Every other node subscribed to that topic
ingests the envelope, validates shape + (eventually) signature,
records the event, and accumulates attestations. Once a node has
recorded ≥ `MIN_ATTESTOR_QUORUM` distinct attestations for an
event, `attestor_quorum_reached(event_id)` returns `true` and the
event is considered confirmed for that node.

## Topic naming

Gossipsub topics are **DID-keyed**, NOT pubkey-keyed:

```
/dot/reputation/{recorder_did_hex}
```

Where `recorder_did_hex` is the 52-byte `RecorderDid` rendered as
104 lowercase hex characters. The canonical helper lives in
`octo_reputation::gossip::topic_for_recorder(did)`.

Legacy pubkey-keyed topics (RFC-0855p-b pre-amendment 29) are
removed. Any ingress bearing a stale pubkey mapping is rejected with
`ReputationError::GossipEnvelopeInvalid` (discriminant `0x3A`).

## Authority model

| Field | Authority |
|---|---|
| `recorder_signature` | **Authoritative** — single source of truth for the event |
| `coordinator_signature` | Transport metadata only — non-authoritative |
| `attestor_signature` | Transport metadata only — non-authoritative |

The recorder's signature is the only signal that the event actually
happened. Attestors merely confirm "I observed this event too";
they do not contribute authority. This separation matters because
it lets us fail-closed on quorum (no quorum = event is unconfirmed)
without invalidating recorder signatures.

## Election integration

`SlashReputationStoreCompat` (in `octo-network::reputation`) reads
the persisted attestations and computes the canonical RFC-0968 §10
`election_priority`:

```
priority = (stake_saturated × effective) / MAX_ELECTION_STAKE
```

where `stake_saturated = min(stake, MAX_ELECTION_STAKE)` and
`effective = score_clamped × min(1.0, samples / MIN_CONFIDENCE_SAMPLES)`.

The canonical formula is monotonic in `stake` when `effective > 0`.
The legacy `priority_legacy = stake / (1 + global_slash_count)` is
preserved for back-compat and the AC L33 1000-candidate
differential test (both orderings must agree when both reduce to
monotonic-in-stake, e.g. zero slashes, score=1.0, samples=100).

## Operator runbook

### Starting an attestor node

```rust
use std::sync::Arc;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::gossip::reputation::start_reputation_gossip;
use octo_reputation::gossip::RateLimitedAttestor;
use octo_reputation::InMemoryReputationStore;
use octo_adapter_p2p::NativeP2PAdapter;

let store = Arc::new(InMemoryReputationStore::new());
let adapter = NativeP2PAdapter::new(NativeP2PConfig {
    listen_addr: "/ip4/0.0.0.0/tcp/4001".into(),
    bootstrap_peers: vec![/* peer multiaddrs */],
});
adapter.start_swarm().await?;

let (tx, rx) = tokio::sync::mpsc::channel(4096);
let _join = start_reputation_gossip(rx, store);
// Drain the adapter's inbound channel into `tx` on your event loop.
```

### Inspecting gossip ingress

Every ingress message produces an `IngressOutcome` (5 variants:
`Accepted`, `DuplicateEvent`, `InvalidShape`, `NonReputationTopic`,
`Unparseable`, `RateLimited`). The substrate emits a `tracing::debug!`
log per message with the topic + outcome. Wire your observability
stack to those logs to build per-topic ingress metrics.

### Rotation lineage audit

When a recorder rotates its DID, the new events carry a
`RotationProvenance` pointing to the tombstoned predecessor. The
substrate rejects any envelope whose `rotation_provenance.new_did ==
envelope.event.recorder_did` (that would be a no-op rotation). The
audit path runs `read_aggregate(did, kind, layer)` for the
tombstoned DID — aggregates on a tombstoned DID freeze at their
last value and are excluded from election priority.

### Rate-limit tuning

`RateLimitedAttestor::with_capacity(cap, window_secs)` controls
the per-attestor budget. The defaults are `cap=10`,
`window_secs=1` (RFC-0968 §12). To tune for a higher-throughput
deployment:

```rust
let rl = Arc::new(RateLimitedAttestor::with_capacity(100, 1));
let _join = start_reputation_gossip_with_rate_limit(rx, store, rl);
```

`rl.tracked_attestors()` is a diagnostics helper — it returns the
number of attestors the limiter has seen at least one event from.

## Schema reference

The federation substrate persists to two stoolap tables (mission
0968 Phase 4):

| Table | Created | Purpose |
|---|---|---|
| `reputation_attestors` | v004 | Attestor registry (DID + pubkey + peer_set_id) |
| `reputation_attestations` | v004 | Per-event attestations (composite dedup) |
| `reputation_gossip_seen` | v005 | Catch-up ledger for late-joining attestors |

See `crates/octo-reputation/migrations/v004__reputation_attestations.sql`
and `v005__reputation_gossip_seen.sql` for the canonical schema.

## Test surface

| Layer | Location | Purpose |
|---|---|---|
| Unit | `octo-reputation/src/{auth,gossip,store/{memory,stoolap}}.rs` | Type + schema + per-backend contract |
| Lib | `octo-network/src/gossip/reputation.rs` | Substrate ingress + rate-limit + catch-up |
| Integration | `octo-reputation/tests/{stoolap_integration,cross_backend_integration}.rs` | Schema + cross-backend determinism |
| Integration | `octo-network/tests/cross_mission_federation.rs` | 2-node mesh + differential ordering |

The 2-node live-mesh test (`two_node_mesh_substrate_receives_via_real_swarm`)
is `#[ignore]`-d because the upstream
`NativeP2PAdapter::send_message` publish path is currently stubbed.
Run with `cargo test --ignored` once the publish path is wired.

## Cross-references

- RFC-0968-A1 §12 — federation machinery
- RFC-0968-A1 amendment 22 — `MIN_ATTESTOR_QUORUM`
- RFC-0968-A1 amendment 28 — authority model (recorder signature authoritative)
- RFC-0968-A1 amendment 29 — DID-keyed topics (no pubkey mapping)
- Mission 0855p-b — `missions/claimed/0855p-b-cross-mission-reputation.md`
- Mission 0968-b — `missions/open/0968-b-marketplace-integration.md`