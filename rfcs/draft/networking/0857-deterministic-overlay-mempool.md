---
title: "RFC-0857: Deterministic Overlay Mempool (DOM)"
status: Draft
version: 1.0.0
created: 2026-05-25
updated: 2026-05-25
authors:
  - CipherOcto Core Team
related:
  - RFC-0850 (Networking): DOT
  - RFC-0851 (Networking): GDP
  - RFC-0852 (Networking): DGP
  - RFC-0853 (Networking): OCrypt
  - RFC-0855 (Networking): MON
  - RFC-0856 (Networking): DRS
  - RFC-0104 (Numeric): DFP
  - RFC-0105 (Numeric): DQA
---

# RFC-0857: Deterministic Overlay Mempool (DOM)

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

The Deterministic Overlay Mempool (DOM) defines the canonical pending-state coordination layer for CipherOcto overlays. DOM generalizes "transactions" into Overlay Intents and provides deterministic pending object ordering, replay-safe propagation, mission-scoped pools, censorship-resistant dissemination, canonical admission rules, deterministic eviction, proof-compatible execution queues, and multi-transport mempool federation.

## Dependencies

**Requires:** RFC-0850 (DOT), RFC-0851 (GDP), RFC-0852 (DGP), RFC-0853 (OCrypt), RFC-0855 (MON), RFC-0856 (DRS), RFC-0104 (DFP), RFC-0105 (DQA)

**Optional:** RFC-0854 (DPS), RFC-0859 (PCE)

## Motivation

### CAN WE? — Feasibility Research

The fundamental question: **Can we build a deterministic pending-state coordination layer for heterogeneous overlay intents?**

Research confirms feasibility through:

- **Ethereum mempool** demonstrates transaction ordering with economic prioritization (EIP-1559)
- **RFC-0104/0105** provide deterministic numeric primitives for fee computation
- **RFC-0850/0852** provide deterministic envelope propagation
- **Database partitioning** proves mission-scoped isolation is feasible
- **Priority queue algorithms** provide deterministic ordering with canonical tiebreaking

### WHY? — Why This Matters

Without DOM:

- Traditional mempools only handle transactions — CipherOcto must coordinate 8 intent types
- No mission isolation — all intents compete in a single global pool
- Non-deterministic ordering breaks consensus — different nodes process intents differently
- No economic prioritization — time-sensitive intents get no priority guarantee
- No multi-transport propagation — intents are limited to single-carrier delivery

DOM enables CipherOcto to coordinate heterogeneous overlay activities with deterministic semantics compatible with blockchain consensus.

## Design Goals

| Goal | Target |
|------|--------|
| G1: Deterministic Ordering | Identical ordering under shared state |
| G2: Replay Safety | Canonical mempool reconstruction |
| G3: Mission Isolation | Scoped pending-state coordination |
| G4: Byzantine Resilience | Adversarial propagation tolerance |
| G5: Multi-Transport | Carrier-independent dissemination |
| G6: Economic Prioritization | Incentive-aware inclusion |
| G7: Proof Compatibility | zk-ready pending execution |

## Specification

### 1. Overlay Intent Model

```rust
struct OverlayIntent {
    intent_id: [u8; 32],
    intent_type: u16,
    mission_id: [u8; 32],
    sender_id: [u8; 32],
    sequence: u64,
    logical_timestamp: u64,
    payload_root: [u8; 32],
    economic_weight: u64,
    execution_class: u16,
    signature: [u8; 64],
}

enum IntentType {
    Transaction,        // Economic state transition
    MissionCommand,     // Overlay coordination
    AIExecution,        // Inference/execution request
    ConsensusVote,      // Validator participation
    ProofSubmission,    // ZK proof delivery
    ResourceLease,      // Resource market request
    GovernanceProposal, // Governance coordination
    RelayCommitment,    // Relay participation
}
```

### 2. Mission-Scoped Mempools

Each MON MAY maintain its own mempool. DOM supports layered pools:

```text
GLOBAL → CONSENSUS → MISSION → PRIVATE → LOCAL
```

### 3. Deterministic Admission

Admission MUST validate: signature validity, replay window, sequence validity, mission authorization, resource constraints, canonical serialization.

**Forbidden inputs:** local latency, wall-clock timing, CPU load, thread order, local bandwidth, transport origin.

### 4. Canonical Intent Ordering

Pending intents ordered by: `(execution_class, economic_weight, logical_timestamp, sequence, intent_id)`

Tie-breaking: lowest lexicographic `intent_id` wins.

### 5. Execution Classes

```rust
enum ExecutionClass {
    CriticalConsensus,
    Consensus,
    MissionCritical,
    Economic,
    Standard,
    Bulk,
    Archive,
}
```

Scheduling: CriticalConsensus → Consensus → MissionCritical → Economic → Standard → Bulk

### 6. Mempool Propagation (extends RFC-0852)

DOM objects propagate via deterministic gossip. Nodes SHOULD propagate only unseen intents. Anti-entropy reconciliation via Merkle summaries.

### 7. Mempool Root

```rust
struct MempoolStateRoot {
    mission_id: [u8; 32],
    intent_count: u64,
    pending_root: [u8; 32],
    replay_watermark: u64,
}
```

Given identical inputs, all compliant nodes MUST derive identical mempool state.

### 8. Economic Prioritization

Intent ordering MAY incorporate: fees, stake weight, relay rewards, proof rewards, mission incentives. Fee prioritization MUST remain deterministic — no local heuristics.

### 9. Mempool Eviction

Deterministic eviction order: lowest priority → lowest economic weight → oldest pending. Expired intents MUST be removed identically across nodes.

### 10. Deterministic Numerics

All mempool-critical arithmetic MUST use deterministic numeric semantics (RFC-0104 DFP, RFC-0105 DQA), especially for fee ordering, stake weighting, reward computation, AI execution pricing.

## Performance Targets

| Metric | Target |
|--------|--------|
| Intent admission | <1ms |
| Ordering computation | <1µs |
| Mempool sync | <5s for 10K intents |
| Eviction cycle | <10ms |

## Security Considerations

### Consensus Attacks

| Attack | Impact | Mitigation |
|--------|--------|------------|
| Intent forgery | High | Ed25519 signature verification |
| Replay attack | High | Replay cache + logical timestamp validation |
| Ordering manipulation | High | Canonical ordering by (class, weight, ts, seq, id) |
| Consensus isolation violation | Critical | Deterministic admission rules — platform metadata never in consensus |

### Economic Exploits

| Attack | Impact | Mitigation |
|--------|--------|------------|
| Fee manipulation | Medium | Deterministic fee ordering via RFC-0105 DQA |
| Priority gaming | Medium | Execution class is intent-type-determined, not sender-chosen |
| Mempool flooding | Medium | Economic friction via OCTO-B staking |
| Free-riding | Low | Intent fees required for admission |

## Adversarial Review

| Threat | Impact | Mitigation | Verification |
|--------|--------|------------|--------------|
| Intent forgery | High | Ed25519 signature at admission | Signature verification test |
| Replay attack | High | Replay cache with deterministic eviction | Replay detection test |
| Ordering manipulation | Critical | Canonical ordering invariant | Ordering consistency test |
| Mempool flooding | Medium | Economic friction + rate limiting | Flood resistance test |
| Mission isolation breach | High | Mission-scoped mempool separation | Isolation test |
| Eviction manipulation | Medium | Deterministic eviction order | Eviction consistency test |
| Free-riding | Low | OCTO-B intent fees | Fee enforcement test |
| Priority gaming | Medium | Class determined by intent type | Priority test |

## Economic Analysis

### Token Integration

| Activity | Token | Rationale |
|----------|-------|-----------|
| Intent submission fee | OCTO-B | Economic friction to prevent spam |
| Priority ordering | OCTO-B | Higher fees for higher priority within class |
| Mempool relay | OCTO-B | Bandwidth for intent propagation |
| Consensus intents | OCTO-N | Validator participation rewards |
| Mission intents | OCTO-O | Mission coordination fees |

### Fee Model

```text
intent_fee = base_fee × intent_type_multiplier × (1 + priority_premium)
```

Where `intent_type_multiplier` scales by execution class and `priority_premium` is optional sender-chosen uplift.

## Compatibility

### RFC-0843 Integration

DOM extends RFC-0843's transaction model with overlay intents:

- RFC-0843 handles blockchain transactions — DOM generalizes to overlay intents
- DOM intents propagate via DGP (RFC-0852) over DOT carriers (RFC-0850)
- Consensus intents integrate with RFC-0843 block production

### Forward Compatibility

- Intent types are extensible (values 0x0009-0xFFFF for future types)
- Execution classes are extensible (values 0x0007-0xFFFF)
- Mempool hierarchy is configurable per mission

## Implementation Phases

### Phase 1: Core Mempool (Months 1-3)
- OverlayIntent with DCS serialization
- Canonical admission rules
- Deterministic ordering
- Replay protection

### Phase 2: Mission Scoping (Months 3-5)
- Mission-scoped mempool isolation
- Hierarchical mempool layering
- IntentType routing

### Phase 3: Propagation and Sync (Months 5-8)
- DGP integration for intent propagation
- Anti-entropy mempool reconciliation
- Multi-transport dissemination

### Phase 4: Economics and Proofs (Months 8-12)
- Economic prioritization
- Proof-carrying intents (RFC-0854)
- AI execution queue scheduling
- Resource market integration

## Test Vectors

### Intent Serialization (DCS Canonical)

```
OverlayIntent:
  intent_id       = SHA-256(sender_id || sequence || logical_timestamp)
  intent_type     = 0x0001 (Transaction)
  mission_id      = [0x00; 32] (global)
  sender_id       = [0xAA; 32]
  sequence        = 42
  logical_timestamp = 1000000
  payload_root    = SHA-256(canonical_payload_bytes)
  economic_weight = 1000
  execution_class = 4 (Standard)
  signature       = Ed25519_sign(sender_privkey, canonical_bytes)

Expected canonical bytes: deterministic, identical across all implementations
```

### Canonical Ordering Verification

```
Intent A: class=1 (Consensus), weight=500,  ts=100, seq=1, id=[0x01;32]
Intent B: class=1 (Consensus), weight=1000, ts=200, seq=2, id=[0x02;32]
Intent C: class=4 (Standard),  weight=500,  ts=50,  seq=1, id=[0x03;32]
Intent D: class=1 (Consensus), weight=500,  ts=100, seq=1, id=[0x00;32]

Canonical order: D, A, B, C

Rationale:
  - D and A tie on (class=1, weight=500, ts=100, seq=1), but D.id < A.id
  - B has same class but higher weight (1000 > 500), so B after A
  - C has class=4 > class=1, so C is last
```

### Deterministic Eviction

```
Mempool full (max_entries = 3). New intent E arrives with class=4, weight=100.

Current mempool:
  A: class=1, weight=500,  ts=100
  B: class=4, weight=200,  ts=200
  C: class=4, weight=100,  ts=150

Eviction candidates (lowest class → lowest weight → oldest ts):
  B and C are tied on class=4 (lowest in pool)
  C has lower weight (100 < 200), so C is evicted first

After eviction: [A, B, E]
```

## Alternatives Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **Blockchain mempool only** | Proven (Ethereum), well-understood | Single-chain scope, no mission isolation, fee-market only | Rejected — too narrow for overlay coordination |
| **CRDT-based pending state** | Eventually consistent, conflict-free | No canonical ordering, no economic weighting, non-deterministic merge | Rejected — violates determinism boundary |
| **Centralized queue** | Simple, low latency | Single point of failure, censorship risk, no federation | Rejected — violates decentralization requirement |
| **Optimistic processing** | Lower latency, speculative execution | Requires rollback logic, non-deterministic under conflict | Rejected — determinism violations propagate to consensus |

**Decision:** DOM uses deterministic canonical ordering with mission-scoped isolation. This is the only approach that satisfies: (1) deterministic ordering at consensus boundary, (2) mission isolation, (3) economic prioritization, (4) multi-transport federation.

## Rationale

### Why overlay intents instead of just transactions?

Traditional blockchains model everything as "transactions" — value transfers between accounts. CipherOcto's overlay network coordinates heterogeneous activities: mission commands, AI execution requests, consensus votes, proof submissions, resource leases, governance proposals, and relay commitments. Each intent type has different execution semantics, economic weight, and priority. A unified `OverlayIntent` abstraction with a discriminated `intent_type` field allows the mempool to handle all coordination primitives through a single deterministic admission and ordering pipeline while preserving type-specific semantics in the execution layer.

### Why mission-scoped mempools?

Without mission scoping, all intents compete in a single global pool. This creates problems: (1) mission-critical intents from Mission A can be starved by high-economic-weight intents from Mission B, (2) private mission intents leak metadata to unrelated participants, (3) replay protection windows must be global instead of per-mission. Hierarchical mempools (GLOBAL → CONSENSUS, MISSION, PRIVATE, LOCAL) isolate intent flows while allowing cross-mission coordination at the GLOBAL level when needed.

### Why execution class ordering?

Not all intents are equal. A consensus vote that determines block finality is more urgent than a background archival request. Execution classes (CriticalConsensus > Consensus > MissionCritical > Economic > Standard > Bulk > Archive) provide a deterministic priority hierarchy that ensures time-sensitive intents are processed first, regardless of economic weight. Economic weight serves as a tiebreaker within the same class.

### Why deterministic eviction?

When the mempool reaches capacity, the evicted intent must be identical across all nodes. Non-deterministic eviction (e.g., LRU with wall-clock timestamps) causes state divergence: Node A evicts intent X while Node B evicts intent Y, leading to different mempool state roots. Deterministic eviction by (lowest class → lowest weight → oldest timestamp) ensures convergence even under capacity pressure.

## Future Work

- F1: Adaptive fee markets with dynamic base fee adjustment
- F2: Cross-membridge: intent bridging between separate overlay networks
- F3: Zero-knowledge mempool proofs (prove intent validity without revealing content)
- F4: AI-driven intent scheduling optimization
- F5: Hierarchical economic weighting with nested mission budgets
- F6: Intent batching for throughput optimization
- F7: Stealth mempools for hidden mission coordination
- F8: Integration with hardware security modules for intent signing

## Related Use Cases

- [Decentralized Mission Execution](../../docs/use-cases/decentralized-mission-execution.md)
- [Agent Marketplace](../../docs/use-cases/agent-marketplace.md)
- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)

## Key Files

| File | Change |
|------|--------|
| `crates/octo-network/src/dom/mod.rs` | DOM module root |
| `crates/octo-network/src/dom/intent.rs` | OverlayIntent |
| `crates/octo-network/src/dom/admission.rs` | Admission rules |
| `crates/octo-network/src/dom/ordering.rs` | Canonical ordering |
| `crates/octo-network/src/dom/pool.rs` | Mempool storage |
| `crates/octo-network/src/dom/eviction.rs` | Deterministic eviction |
| `crates/octo-network/src/dom/propagation.rs` | DGP integration |
| `crates/octo-network/src/dom/economics.rs` | Economic prioritization |

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-25 | Initial draft |

## Related RFCs

- RFC-0850-0856 (Networking): All dependency layers
- RFC-0858 (Networking): ORR — privacy for intents
- RFC-0859 (Networking): PCE — proof-carrying intents
