# Use Case: DOT Network Bootstrap

**Date:** 2026-06-16
**Status:** Draft

---

## Problem

A new CipherOcto node has no peers. To join the DOT (Deterministic Overlay Transport) mesh, it must discover other peers and verify their identity. The current bootstrap is described at a high level in RFC-0851 §3 "Discovery Modes" but lacks:

1. **Concrete bootstrap mechanisms per mode** (Mode A = bootstrap nodes, Mode B = DHT fallback, Mode C = invite link).
2. **Trust-anchor specification** — how does a new node know which seed list to trust?
3. **Sybil / eclipse resistance** — how does a new node avoid being connected only to a single attacker's nodes?
4. **Health monitoring** — how does the node know when a seed has gone stale (e.g., the seed list service is down)?

Without a clear spec, each implementation would invent its own bootstrap, leading to:
- Inconsistent trust models (some trust IP-based seeds, some don't)
- Eclipse attack vulnerability (no Sybil resistance)
- Operational fragility (no health monitoring)

## Stakeholders

- **Primary:** CipherOcto node operators who bring up new nodes (CI pipelines, gateway operators, mobile app users)
- **Secondary:** Foundation members who run the seed list service
- **Affected:** Existing peers (their bandwidth is consumed by serving seed list requests)

## Motivation

### Why Bootstrap Matters

Bootstrap is the **first network operation** every new node performs. A failed or compromised bootstrap means:

- The node cannot reach the libp2p mesh
- The node is invisible to the mission overlay networks
- The node is vulnerable to eclipse attacks (if connected only to attacker-controlled peers)

A well-designed bootstrap must:
1. Be **resilient** — multiple bootstrap channels, no single point of failure
2. Be **trust-anchored** — the operator decides which seeds to trust
3. Be **Sybil-resistant** — the node connects to many independent peers, not a single attacker's cluster
4. Be **monitored** — health checks detect when the bootstrap channel degrades

### Why Decentralization Matters

At launch, a foundation multi-sig authority signs the seed list. This is appropriate because:
- No slashing exists yet to punish a malicious seed list
- The foundation is known and trusted

Once slashing ships, the authority can transition to a DAO multi-sig (per mission `0851p-a-seed-authority-decentralization`). This is a hard-fork transition (no backward compat) because the trust model fundamentally changes.

## Success Metrics

| Metric | Target | Measurement |
| ------ | ------ | ----------- |
| Bootstrap time (warm) | < 30 seconds | From `start_node` to first libp2p peer connected |
| Bootstrap time (cold) | < 5 minutes | From `start_node` to first mission overlay joined |
| Sybil resistance | > 50% honest peers | Monte Carlo simulation of attacker models |
| Eclipse resistance | > 0% under 33% attack | Trivial guarantee; verified by simulation |
| Health check latency | < 1 second | Stale seed detection per `start_node` |
| Decentralization deadline | Post-MissionSlashing | 1.0 | When DAO multi-sig takes over |

## Constraints

- **Must not:** Trust a seed list without verifying the authority's signature.
- **Must not:** Allow a single bootstrap mode to be the only option (resilience).
- **Limited to:** The 3 bootstrap modes defined in RFC-0851p-a: Mode A (Bootstrap Nodes, default), Mode B (DHT Fallback), Mode C (Invite Link). Mode D (NIP-05 / Nostr) is future work (F5 mission `0851p-a-nostr-mode-d.md`).
- **Limited to:** The slash reason codes allocated in RFC-0855p-b §B (0x000D = `bootstrap_node_misbehavior`).

## Non-Goals

- **Not in scope:** A new bootstrap mode beyond A, B, C (current RFC). Mode D (NIP-05 / Nostr) is post-launch future work; adding any new mode is a new RFC.
- **Not in scope:** IP-based geolocation. (Bootstrap is platform-agnostic.)
- **Not in scope:** Sybil resistance via proof-of-work. (Sybil resistance here is via seed list authority + web-of-trust + cross-referencing.)
- **Not in scope:** Web-of-trust scoring algorithms. (Mission `0851p-a-trust-ux` provides the UX tool; the scoring is per-mission and out of scope for bootstrap.)

## Impact

If this use case is implemented:

1. **New nodes can join quickly.** Multi-channel bootstrap with health checks.
2. **Eclipse attacks are detectable.** Health checks + 2/3 majority requirements for slash votes.
3. **Privacy is preserved.** Tor-only mode is opt-in for operators who need it.
4. **The trust model can evolve.** Foundation → DAO multi-sig transition is a hard-fork event with clear semantics.

## Related RFCs

- RFC-0851: Gateway Discovery Protocol — base; defines discovery modes
- RFC-0851p-a (Networking): Network Bootstrap Protocol — concrete bootstrap spec; this UC motivates it
- RFC-0850: Deterministic Overlay Transport — base protocol
- RFC-0852: Deterministic Gossip Protocol — gossip layer that bootstrap feeds into
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — slash reason codes (0x000D)

## Related Use Cases

- [Social Platform Transport Layer](social-platform-transport-layer.md) — Bootstrap is a prerequisite for any transport
- [Mission Coordinator Lifecycle](mission-coordinator-lifecycle.md) — Missions are joined after bootstrap completes

## Pipeline Position

```
Use Case (DOT Network Bootstrap — this document)
   │
   ▼
RFC-0850: Deterministic Overlay Transport
   │
   ▼
RFC-0851: Gateway Discovery Protocol
   │
   ▼
RFC-0851p-a (Networking): Network Bootstrap Protocol
   │
   ▼
Missions: 0851p-a-{seed-authority-decentralization, tor-seed-list, seed-health-check, trust-ux, nostr-mode-d, bootstrap-slashing}
```

## Related Missions

Under RFC-0851p-a:
- `missions/open/0851p-a-seed-authority-decentralization.md` — Foundation → DAO multi-sig transition
- `missions/open/0851p-a-tor-seed-list.md` — Tor-only bootstrap mode
- `missions/open/0851p-a-seed-health-check.md` — Stale seed detection
- `missions/open/0851p-a-trust-ux.md` — Web-of-trust visualization (CLI)
- `missions/open/0851p-a-nostr-mode-d.md` — Mode D: NIP-05 / Nostr trust-anchored bootstrap (future work, post-launch)
- `missions/open/0851p-a-bootstrap-slashing.md` — Slash reason code 0x000D for misbehaving bootstrap nodes

---

**Category:** Networking
**Priority:** Critical (without bootstrap, no node can join)
**RFCs:** RFC-0851, RFC-0851p-a
**Status:** Defined → Mission phase
