# Plan: Networking Implementation Guide and Missions

**Date:** 2026-05-26
**Status:** Draft
**Goal:** Bridge the gap between networking RFCs (0850-0860) and implementable code by creating a companion implementation guide and comprehensive mission set.

---

## Problem Statement

The networking RFCs (0850-0860) define the protocol specification (WHAT) but lack the concrete implementation detail (HOW) that makes them actionable. Compared to accepted RFCs like RFC-0954:

| Dimension | Accepted RFCs | Networking RFCs |
|-----------|---------------|-----------------|
| Rust code | Compilable snippets with `async fn`, `impl` | Struct definitions only |
| Error types | Concrete `enum` with `thiserror` | Abstract sections |
| Config schemas | YAML/TOML examples | None |
| API surface | Request/response types, endpoints | None |
| Registry pattern | `LazyLock<RwLock<HashMap>>` factory | No mention |
| Test specs | `#[tokio::test]` examples | Hex test vectors |

## Solution

### Part 1: Implementation Guide

**File:** `docs/07-developers/networking-implementation-guide.md`

Contents:
1. **Module tree** — exact `mod.rs` layout for `crates/octo-network/src/` (11 modules)
2. **Error types** — `DotError`, `GdpError`, `CryptoError`, `ProofError` with `thiserror`
3. **Core type implementations** — `BroadcastDomainId`, `DeterministicEnvelope`, `ReplayCache`, `OverlaySequence` with real Rust code
4. **Trait definitions** — `PlatformAdapter`, `DeterministicProofSystem` with concrete signatures
5. **Canonical scoring** — `compute_route_score()` with u64 saturating_mul
6. **Integration code** — how DOT hooks into existing `network.rs`
7. **Config schema** — YAML for gateway configuration
8. **Testing strategy** — unit tests and integration test patterns
9. **Cargo dependencies** — required crates with feature gates

### Part 2: Missions (34 total)

Each RFC gets 1 base mission + N feature missions for complete coverage:

| RFC | Base Mission | Feature Missions | Total |
|-----|-------------|------------------|-------|
| 0850 DOT | Core envelope + NativeP2P | Fragmentation, Federation, Privacy, Failure | 5 |
| 0851 GDP | Gateway discovery | Scopes, Anti-Sybil, Discovery gossip | 4 |
| 0852 DGP | Deterministic gossip | Anti-entropy, Compression/Retention | 3 |
| 0853 OCrypt | Crypto primitives | Attestation/Key rotation, Onion extension | 3 |
| 0854 DPS | Proof trait | Recursive aggregation, STARK/PLONK backends | 3 |
| 0855 MON | Mission lifecycle | Routing, Discovery/Gossip, Execution, Governance | 5 |
| 0856 DRS | Route scoring | Trust/Multi-path, Onion/Mission-aware | 3 |
| 0857 DOM | Mempool intents | Propagation/Numerics | 2 |
| 0858 ORR | Onion routing | Multi-transport/Route rotation | 2 |
| 0859 PCE | Proof envelope | Proof types/Aggregation | 2 |
| 0860 PoRelay | Relay proofs | Registry/Anti-Sybil | 2 |

### Mission Format

Each mission follows the existing format:
```markdown
# Mission: <Name>
## Status / RFC / Summary / Acceptance Criteria / Location / Complexity / Prerequisites / Implementation Notes / Reference
```

## Implementation Steps

### Step 1: Create Implementation Guide
- [ ] Write `docs/07-developers/networking-implementation-guide.md`
- [ ] Include module tree, error types, core implementations, traits, config, testing

### Step 2: Create Base Missions (11)
- [ ] `missions/open/0850-dot-core-envelope.md`
- [ ] `missions/open/0851-gdp-gateway-discovery.md`
- [ ] `missions/open/0852-dgp-deterministic-gossip.md`
- [ ] `missions/open/0853-ocrypt-overlay-cryptography.md`
- [ ] `missions/open/0854-dps-deterministic-proof-substrate.md`
- [ ] `missions/open/0855-mon-mission-overlay-networks.md`
- [ ] `missions/open/0856-drs-deterministic-route-selection.md`
- [ ] `missions/open/0857-dom-deterministic-overlay-mempool.md`
- [ ] `missions/open/0858-orr-onion-relay-routing.md`
- [ ] `missions/open/0859-pce-proof-carrying-envelopes.md`
- [ ] `missions/open/0860-porelay-proof-of-relay.md`

### Step 3: Create Feature Missions (23)
- [ ] DOT: 0850a (fragmentation), 0850b (federation), 0850c (privacy), 0850d (failure)
- [ ] GDP: 0851a (scopes), 0851b (anti-sybil), 0851c (discovery gossip)
- [ ] DGP: 0852a (anti-entropy), 0852b (compression/retention)
- [ ] OCrypt: 0853a (attestation/key rotation), 0853b (onion extension)
- [ ] DPS: 0854a (recursive aggregation), 0854b (STARK/PLONK backends)
- [ ] MON: 0855a (routing), 0855b (discovery/gossip), 0855c (execution), 0855d (governance)
- [ ] DRS: 0856a (trust/multi-path), 0856b (onion/mission-aware)
- [ ] DOM: 0857a (propagation/numerics)
- [ ] ORR: 0858a (multi-transport/route rotation)
- [ ] PCE: 0859a (proof types/aggregation)
- [ ] PoRelay: 0860a (registry/anti-sybil)

### Step 4: Verification
- [ ] All 11 RFCs have corresponding missions
- [ ] All RFC specification sections are covered by at least one mission
- [ ] Mission dependency chain is acyclic
- [ ] Implementation guide code compiles (syntax check)

## Files Created

| File | Purpose |
|------|---------|
| `docs/07-developers/networking-implementation-guide.md` | Concrete Rust implementation guide |
| `missions/open/0850*.md` (5 files) | DOT missions |
| `missions/open/0851*.md` (4 files) | GDP missions |
| `missions/open/0852*.md` (3 files) | DGP missions |
| `missions/open/0853*.md` (3 files) | OCrypt missions |
| `missions/open/0854*.md` (3 files) | DPS missions |
| `missions/open/0855*.md` (5 files) | MON missions |
| `missions/open/0856*.md` (3 files) | DRS missions |
| `missions/open/0857*.md` (2 files) | DOM missions |
| `missions/open/0858*.md` (2 files) | ORR missions |
| `missions/open/0859*.md` (2 files) | PCE missions |
| `missions/open/0860*.md` (2 files) | PoRelay missions |

**Total:** 1 guide + 34 missions = 35 files
