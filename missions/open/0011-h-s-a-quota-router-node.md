# 0011-h-s-a-quota-router-node — Substrate additions for QuotaRouterNode (RFC-0870 substrate)

## Status

Open (2026-09-20) — Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G10 + RFC-0011-p Phase 8 §Substrate-Additions Companion Missions. Substrate slice pending per the Phase 4 paired-substrate completion pattern (companion YAML filled in → substrate lands → YAML Claimed → CLI dispatch lands → YAML Completed paired). RFC-0011-p Phase 8 quota-router-node amendment Draft landed at `next 92b69be3`.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G10 + RFC-0011-p Phase 8 §Substrate-Additions Companion Missions + RFC-0870 Distributed Quota Router Network.

## Summary

Adds `QuotaRouterNode` struct + `RouterStatus` enum + `status()` + `peer_capacity()` substrate surface to `crates/octo-network/src/quota/router_node.rs` (NEW module under `quota/` subdir per RFC-0870 substrate path). Required by `octo network router status` (read) + `octo network router peers <peer_node_id_hex>` (read) per RFC-0011-p Phase 8 §Subcommand Taxonomy.

### Substrate additions target

```rust
// crates/octo-network/src/quota/router_node.rs (NEW module)
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `QuotaRouterNode` — substrate-faithful quota router
/// node state for RFC-0870 Distributed Quota Router
/// Network. Per RFC-0011-p Phase 8 §Substrate Mapping
/// Table, this struct is the Layer B substrate projection
/// consumed by `octo network router status` + `octo
/// network router peers`. Per-extension transport
/// impl crates (Layer D) are OUT OF SCOPE per per-extension
/// crate pattern.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaRouterNode {
    /// Node-local self identifier (32-byte canonical).
    pub node_id: [u8; 32],
    /// Current operational status (Healthy + Degraded +
    /// Offline enum).
    pub status: RouterStatus,
    /// Per-peer remaining capacity in quota units (BTreeMap
    /// for deterministic iteration order per RFC-0011-h
    /// §Output Envelope order determinism).
    pub peer_capacities: BTreeMap<[u8; 32], u64>,
    /// Last capacity sync epoch (operator-side observability).
    pub last_sync_epoch: u64,
}

/// `RouterStatus` — operational status enum for the
/// quota router node per RFC-0870 §Operational States.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouterStatus {
    /// Node is operational and routing quota traffic
    /// normally.
    Healthy,
    /// Node is operational but experiencing degraded
    /// performance (high latency, partial peer reachability,
    /// or quota pressure). Operator should investigate.
    Degraded,
    /// Node is not routing quota traffic. No read or
    /// write path is available.
    Offline,
}

impl QuotaRouterNode {
    /// Substrate-faithful status accessor for `octo
    /// network router status` (RFC-0011-p Phase 8).
    /// Returns the current operational status.
    #[must_use]
    pub fn status(&self) -> RouterStatus {
        self.status
    }

    /// Substrate-faithful peer-capacity accessor for
    /// `octo network router peers <peer_node_id_hex>`
    /// (RFC-0011-p Phase 8). Returns the remaining
    /// quota capacity for the given peer node id, or
    /// `None` if the peer is not in the local routing
    /// table.
    #[must_use]
    pub fn peer_capacity(&self, peer_node_id: &[u8; 32]) -> Option<u64> {
        self.peer_capacities.get(peer_node_id).copied()
    }

    /// Returns the number of peers with non-zero capacity
    /// (operator-side observability helper).
    #[must_use]
    pub fn reachable_peer_count(&self) -> usize {
        self.peer_capacities.values().filter(|&&c| c > 0).count()
    }

    /// Returns the total peer count including zero-capacity
    /// peers (operator-side observability helper).
    #[must_use]
    pub fn total_peer_count(&self) -> usize {
        self.peer_capacities.len()
    }
}
```

Layer B substrate additions land in NEW module `crates/octo-network/src/quota/router_node.rs` (quota subdir per RFC-0870 substrate path). The module is registered via `pub mod router_node;` insertion in `crates/octo-network/src/quota/mod.rs` (NEW mod.rs if not present).

`BTreeMap` chosen over `HashMap` for deterministic iteration order (RFC-0011-h §Output Envelope order determinism) and substrate-faithful ordering on projection output.

Per-extension crate pattern preserved: trait is in `octo-network` Layer B; concrete per-transport impl crates (substrate-ext-quota-router-*) are OUT OF SCOPE for follow-on Layer D adapter missions.

## Acceptance Criteria

- [ ] `QuotaRouterNode` struct lands in NEW module `crates/octo-network/src/quota/router_node.rs` per RFC-0011-h §Substrate-Additions row G10 + RFC-0011-p Phase 8 §Substrate Mapping Table
- [ ] `node_id: [u8; 32]` + `status: RouterStatus` + `peer_capacities: BTreeMap<[u8; 32], u64>` + `last_sync_epoch: u64` fields land
- [ ] `RouterStatus` enum (Healthy + Degraded + Offline) lands at same path with `#[serde(rename_all = "lowercase")]`
- [ ] `status() -> RouterStatus` method lands at same path
- [ ] `peer_capacity(peer_node_id: &[u8; 32]) -> Option<u64>` method lands at same path
- [ ] `reachable_peer_count()` + `total_peer_count()` observability helpers land
- [ ] `pub mod router_node;` insertion in `crates/octo-network/src/quota/mod.rs` (NEW mod.rs if not present)
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green (≥5 unit tests added above Phase 7 baseline of 1472)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥5 unit tests + ≥1 integration test (substrate-faithful boundary tests pin status-default + peer_capacity-miss + peer_capacity-hit + reachable_peer_count-zero + reachable_peer_count-nonzero + BTreeMap deterministic ordering)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-p Phase 8 quota-router-node amendment Draft at `next 92b69be3`
- RFC-0870 Distributed Quota Router Network substrate spec (governing RFC)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-router` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-quota-router-* Layer D follow-on missions, OUT OF SCOPE for this trait-only phase)
- Persistence adapter (in-memory struct only; persistence in follow-on Layer D adapter mission per per-extension crate pattern)
- Real quota routing logic (struct projection only; routing engine in follow-on Layer D adapter mission)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land in Phase 8 stub fill-in commit at `next PENDING` per the Phase 4 paired-substrate completion pattern. Phase 8 follows the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap determinism per RFC-0011-h §Output Envelope order determinism. pastejacking defense via `parse_32_byte_hex` shared helper per RFC-0011-h §Pastejacking Defense pattern.
