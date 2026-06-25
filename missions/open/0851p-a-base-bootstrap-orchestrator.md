# Mission: 0851p-a — Bootstrap Orchestrator (Mode A Core)

## Status

Open (2026-06-25) — pre-public-launch

## RFC

RFC-0851p-a (Networking): Network Bootstrap Protocol — §"Implementation Phases" Phase 1

## Summary

Implement the core bootstrap orchestrator that wires the existing `mon::bootstrap` data models (`SeedListEnvelope`, `SeedHealth`, `SeedListAuthority`, `BootstrapMode`, `SlashedSeedBlacklist`) into the `octo-transport` startup path. This is the **missing link** between the RFC-0851p-a specification and the transport/sync stack: without it, the rich bootstrap protocol types are unused and nodes connect only via raw `--peer` CLI args.

The orchestrator drives the `BootstrapClientLifecycle` state machine (Init → Connecting → Validating → Cached → Done), sends `GDP/1/BOOTSTRAP_REQ` envelopes to seed bootstrap nodes, validates `GDP/1/BOOTSTRAP_RESP` responses, computes peer-list intersection (80% Sybil defense), and populates `TransportDiscovery` with the resulting peer cache. On completion, it hands off to `DiscoveryLifecycle::Bootstrap` → Expansion per RFC-0851 §M-GDP-3.

## Design

### 1. `BootstrapOrchestrator` struct (new module: `octo-transport/src/bootstrap.rs`)

```rust
pub struct BootstrapOrchestrator {
    /// Parsed seed list (from config file or embedded genesis list).
    seed_list: SeedListEnvelope,
    /// Blacklist of slashed bootstrap nodes.
    blacklist: SlashedSeedBlacklist,
    /// Current lifecycle state.
    state: BootstrapClientLifecycle,
    /// Bootstrap mode (Direct / TorOnly / TorWithIpFallback).
    mode: BootstrapMode,
    /// Collected peer advertisements from BOOTSTRAP_RESP.
    collected_peers: Vec<GatewayAdvertisement>,
    /// Configuration (timeouts, thresholds).
    config: BootstrapConfig,
}
```

### 2. `BootstrapConfig` (configurable parameters)

```rust
pub struct BootstrapConfig {
    /// Max time to wait for bootstrap responses (default: 60s).
    pub bootstrap_timeout: Duration,
    /// Minimum responses for high-confidence bootstrap (default: 3).
    pub min_responses: usize,
    /// Peer-list intersection threshold (default: 0.80).
    pub intersection_threshold: f64,
    /// Max retries before fallback (default: 5).
    pub max_retries: u32,
    /// Initial retry backoff (default: 1s).
    pub initial_backoff: Duration,
}
```

### 3. `BootstrapClientLifecycle` state machine

```rust
#[repr(u8)]
pub enum BootstrapClientLifecycle {
    Init = 0x01,
    Connecting = 0x02,
    Validating = 0x03,
    Cached = 0x04,
    FallbackB = 0x05,
    FallbackC = 0x06,
    Done = 0x07,
    Failed = 0x08,
}
```

### 4. Core flow

```text
1. load_seed_list(config_path) → SeedListEnvelope
2. blacklist.filter(seed_list) → filtered SeedListEnvelope
3. SeedHealth::check(&seed_list, current_epoch) → reject if FullyStale
4. verify_authority(seed_list.authority, current_epoch) → reject if wrong phase
5. For each seed in seed_list:
     send BOOTSTRAP_REQ via adapter (QUIC/TCP/Webhook)
6. Collect BOOTSTRAP_RESP (min_responses within bootstrap_timeout)
7. Validate signatures (Ed25519)
8. Compute peer-list intersection (≥80% agreement)
9. Merge into TransportDiscovery cache
10. Hand off to DiscoveryLifecycle::Bootstrap → Expansion
```

### 5. Integration points

- **`octo-transport/src/discovery.rs`** — `TransportDiscovery::cache_insert()` is the handoff target
- **`octo-network/src/mon/bootstrap.rs`** — Consumes `SeedListEnvelope`, `SeedHealth`, `SeedListAuthority`, `BootstrapMode`, `SlashedSeedBlacklist`
- **`octo-network/src/mon/slash.rs`** — Consumes `BootstrapMisbehavior` sub-codes for blacklist filtering
- **`octo-network/src/gdp/discovery.rs`** — Transitions `DiscoveryState` from Bootstrap to Expansion

### 6. Envelope types (from RFC-0851p-a §2)

Implement `BootstrapRequest` and `BootstrapResponse` as wire types in `octo-transport/src/bootstrap.rs`:

```rust
/// GDP/1/BOOTSTRAP_REQ
pub struct BootstrapRequest {
    pub requester_id: [u8; 32],
    pub requester_pubkey: [u8; 32],
    pub nonce: [u8; 16],
    pub epoch: u64,
    pub capability_filter: u64,
    pub max_peers: u16,
    pub requester_signature: [u8; 64],
}

/// GDP/1/BOOTSTRAP_RESP
pub struct BootstrapResponse {
    pub requester_id: [u8; 32],
    pub request_nonce: [u8; 16],
    pub epoch: u64,
    pub responder_id: [u8; 32],
    pub advertisements: Vec<GatewayAdvertisement>,
    pub responder_signature: [u8; 64],
}
```

## Acceptance Criteria

- [ ] `BootstrapOrchestrator` struct with state machine
- [ ] `BootstrapConfig` with all RFC-0851p-a constants
- [ ] `BootstrapRequest` / `BootstrapResponse` wire types with canonical serialization (RFC-0126)
- [ ] `BootstrapClientLifecycle` state machine with all transitions from RFC-0851p-a §3
- [ ] Seed list loading + `SeedHealth::check()` integration
- [ ] `SeedListAuthority::verify_authority()` integration
- [ ] `SlashedSeedBlacklist::filter()` integration
- [ ] Peer-list intersection computation (BLAKE3 of sorted intersection)
- [ ] `TransportDiscovery` cache population on bootstrap success
- [ ] `DiscoveryLifecycle::Bootstrap` → Expansion transition
- [ ] Retry with exponential backoff (1s, 2s, 4s, 8s, 16s, max 60s)
- [ ] Unit tests: 5-of-5 success, 3-of-5 partial, 2-of-5 low-confidence, 0-of-5 failure, Sybil detection, stale seed rejection, slashed seed filtering
- [ ] Integration test: mock bootstrap nodes + full lifecycle

### Type Coverage

| RFC-0851p-a Type | Implemented By |
|-----------------|----------------|
| `BootstrapNode` registry | This mission (loading from config) |
| `SeedListEnvelope` | Already in `mon::bootstrap` (consumed) |
| `BootstrapRequest` / `BootstrapResponse` | This mission (wire types) |
| `BootstrapClientLifecycle` state machine | This mission |
| `SeedHealth::check()` | Already in `mon::bootstrap` (consumed) |
| `SeedListAuthority::verify_authority()` | Already in `mon::bootstrap` (consumed) |
| `SlashedSeedBlacklist` | Already in `mon::bootstrap` (consumed) |
| `BootstrapMode` | Already in `mon::bootstrap` (consumed) |
| `DiscoveryLifecycle::Bootstrap` transition | This mission |
| Mode B (DHT fallback) | **Not this mission** (Phase 2) |
| Mode C (invite link) | **Not this mission** (Phase 3) |
| `BootstrapNodeLifecycle` (server-side) | **Not this mission** (server infra) |

## Dependencies

- RFC-0851p-a status: Accepted
- RFC-0863 status: Accepted (provides `octo-transport` crate, `TransportDiscovery`)
- `mon::bootstrap` module: Implemented (data models + tests in `crates/octo-network/src/mon/bootstrap.rs`)
- `mon::slash` module: Implemented (`BootstrapMisbehavior` sub-codes)
- `gdp::discovery` module: Implemented (`DiscoveryState`, `BootstrapMethod`, `DiscoveryLifecycle`)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

| File | Action |
|------|--------|
| `octo-transport/src/bootstrap.rs` | **New**: `BootstrapOrchestrator`, `BootstrapConfig`, `BootstrapClientLifecycle`, `BootstrapRequest`, `BootstrapResponse` |
| `octo-transport/src/lib.rs` | Add `pub mod bootstrap; pub use bootstrap::BootstrapOrchestrator;` |
| `octo-transport/src/discovery.rs` | No changes needed (`cache_insert` already exists) |

## Complexity

Medium (~400-600 lines; state machine, envelope types, intersection logic, retry loop, 12+ unit tests).

## Prerequisites

- RFC-0851p-a: Accepted (done)
- RFC-0863: Accepted (done)
- `mon::bootstrap` data models: Implemented (done)
- `TransportDiscovery::cache_insert()`: Implemented (done)

## Notes

### Why this mission exists

The 6 existing F1-F6 missions (`0851p-a-seed-health-check`, etc.) implement the **supporting features** (health checks, slashing, authority decentralization, Tor, trust UX, Nostr). None of them implement the **core bootstrap protocol** — the state machine that loads a seed list, contacts bootstrap nodes, validates responses, and populates the peer cache. This mission fills that gap.

### Why Mode A only

Mode A (bootstrap nodes) is the default and simplest mode. Mode B (DHT fallback) and Mode C (invite link) are separate phases with different dependencies (RFC-0843 Kademlia, invite URL parser). Shipping Mode A first gives immediate bootstrap capability.

### Why octo-transport (not octo-network)

The orchestrator belongs in `octo-transport` because:
1. It is a **consumer** of `octo-network` types (bootstrap, GDP, discovery) — placing it in `octo-network` would create a circular dependency
2. It produces `TransportDiscovery` cache entries — the transport layer owns discovery state
3. RFC-0863 established `octo-transport` as the integration layer for all consumers

### Relationship to existing stoolap-node --peer path

The `--peer` CLI path (raw `TcpStream::connect`) remains as a development/testing shortcut. The `BootstrapOrchestrator` is the production path. The stoolap-node should use `BootstrapOrchestrator` when no `--peer` args are provided (RFC-0862 update, separate mission).

## Mitigates

RFC-0851p-a §"Implementation Phases" Phase 1 — the entire bootstrap protocol specification has no implementation path without this mission.

## Deadline

Pre-public-launch

## Related Missions

- `0851p-a-seed-health-check.md` — F3: seed staleness check at load (data model done, wiring depends on this mission)
- `0851p-a-bootstrap-slashing.md` — F6: bootstrap node slashing (data model done, blacklist filtering depends on this mission)
- `0851p-a-seed-authority-decentralization.md` — F1: DAO multi-sig (data model done, authority verification consumed by this mission)
- `0851p-a-tor-seed-list.md` — F2: Tor mode (enum done, Tor adapter is future work)
- `0851p-a-trust-ux.md` — F4: trust graph visualization (independent CLI tool)
- `0851p-a-nostr-mode-d.md` — F5: Nostr bootstrap (stub done, full integration is future work)
