# Networking RFC Cross-Reference Analysis

**Scope:** 11 networking RFCs (0850-0860), 3 key accepted numeric RFCs (0104, 0105, 126), 3 accepted economics RFCs (0902, 0904, 0905), and the Rust implementation in `crates/octo-network/src/dot/`.

**Date:** 2026-05-26

---

## 1. RFC Inventory

### Networking Layer (rfcs/draft/networking/) — All Draft

| RFC | Acronym | Full Name | Size |
|-----|---------|-----------|------|
| 0850 | DOT | Deterministic Overlay Transport | 41KB |
| 0851 | GDP | Gateway Discovery Protocol | 18KB |
| 0852 | DGP | Deterministic Gossip Protocol | 16KB |
| 0853 | OCrypt | Overlay Cryptography | 24KB |
| 0854 | DPS | Deterministic Proof Substrate | 24KB |
| 0855 | MON | Mission Overlay Networks | 51KB |
| 0856 | DRS | Deterministic Route Selection | 36KB |
| 0857 | DOM | Deterministic Overlay Mempool | 19KB |
| 0858 | ORR | Onion Relay Routing | 30KB |
| 0859 | PCE | Proof-Carrying Envelopes | 37KB |
| 0860 | PoRelay | Proof-of-Relay | 31KB |

**Total networking spec:** ~327KB across 11 RFCs.

### Referenced Accepted Numeric RFCs (rfcs/accepted/numeric/)

| RFC | Acronym | Role in Networking |
|-----|---------|-------------------|
| 0104 | DFP | Deterministic floating-point for consensus-critical arithmetic |
| 0105 | DQA | Deterministic quant arithmetic for fee/trust computation |
| 0126 | DCS | Deterministic serialization (canonical encoding for all types) |

### Referenced Accepted Economics RFCs (rfcs/accepted/economics/)

| RFC | Role in Networking |
|-----|-------------------|
| 0902 | Multi-provider routing (quota-router, not networking overlay) |
| 0904 | Real-time cost tracking (pricing model) |
| 0905 | Observability and logging |

---

## 2. Dependency Graph

### Topological Layers (build order)

```
Layer 0 (Foundation):    RFC-0850 (DOT), RFC-0904, RFC-0104 (DFP), RFC-0105 (DQA), RFC-0126 (DCS)
Layer 1 (Discovery):     RFC-0851 (GDP), RFC-0902
Layer 2 (Propagation):   RFC-0852 (DGP)
Layer 3 (Crypto):        RFC-0853 (OCrypt)
Layer 4 (Proof/Mission): RFC-0854 (DPS), RFC-0855 (MON)
Layer 5 (Routing/Proof): RFC-0856 (DRS), RFC-0859 (PCE), RFC-0860 (PoRelay)
Layer 6 (Application):   RFC-0857 (DOM), RFC-0858 (ORR)
```

### Fan-In Analysis (most depended-upon)

| RFC | Dependents | Role |
|-----|-----------|------|
| **0850 (DOT)** | 10 | Universal foundation — every networking RFC depends on it |
| **0851 (GDP)** | 8 | Gateway identity/discovery used across all layers |
| **0853 (OCrypt)** | 8 | Cryptographic primitives required by all security layers |
| **0104 (DFP)** | 6 | Deterministic arithmetic for consensus-critical numerics |
| **0126 (DCS)** | 5 | Canonical serialization — encoding determinism |
| **0852 (DGP)** | 5 | Gossip propagation substrate |
| **0105 (DQA)** | 5 | Quant arithmetic for trust/fee computation |
| **0854 (DPS)** | 7 | Proof abstraction layer (optional for some, required for others) |
| **0860 (PoRelay)** | 4 | Relay verification (optional dependency) |

### Fan-Out Analysis (most dependent)

| RFC | Dependencies | Notes |
|-----|-------------|-------|
| **0856 (DRS)** | 10 | Highest complexity — route selection touches everything |
| **0857 (DOM)** | 10 | Mempool requires full stack |
| **0858 (ORR)** | 10 | Onion routing requires full stack |
| 0855 (MON) | 8 | Mission networks require discovery + crypto + gossip |
| 0859 (PCE) | 8 | Proof-carrying envelopes layer on proof substrate |
| 0860 (PoRelay) | 8 | Relay proofs require discovery + crypto + proof substrate |
| 0853 (OCrypt) | 6 | Crypto depends on transport + discovery + gossip |
| 0854 (DPS) | 6 | Proof substrate depends on crypto + transport |

### Cross-Category Dependencies

The networking RFCs depend on **3 external categories**:

- **Process RFCs:** 0008 (Determinism Boundary), 0009 (Identity Management)
- **Numeric RFCs:** 0102 (Wallet Crypto), 0104 (DFP), 0105 (DQA), 0126 (DCS)
- **Proof System RFCs:** 0630 (Proof-of-Inference), 0631 (Proof-of-Dataset Integrity), 0650 (Proof Aggregation)

---

## 3. Core Claims and Assertions

### RFC-0850 (DOT) — Foundation Layer

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Social messengers (Telegram, Discord, Matrix) can serve as transport fabric for deterministic overlay networking | No existing system does this; feasibility argued by analogy to Nostr relay federation | Novel architectural claim — untested |
| Ed25519 signatures over canonical byte representation provide sufficient authenticity | Standard crypto, well-understood | Low risk |
| BLAKE3-256 for all hashing (envelope_id, payload_hash, route_trace_root) | 14x faster than SHA-256, parallelizable | Consistent with OCrypt rationale |
| Logical timestamps (NOT wall-clock) enable deterministic ordering | Lamport-style ordering; `(epoch, monotonic_counter, gateway_id)` triple | Core determinism claim |
| 6 gateway classes (Edge, Relay, Consensus, Archive, Stealth, Translation) with OCTO-B/N/O/S token incentives | Rich role taxonomy | May be over-specified for initial implementation |
| Replay protection via route_trace_root Merkle + HashMap cache | Novel combination; Merkle route trace enables verification without storing full trace | Unproven at scale |
| Fragment reassembly timeout of 10s for 10-fragment envelope | Arbitrary constant | Needs benchmarking |

### RFC-0851 (GDP) — Discovery Layer

| Claim | Evidence | Significance |
|-------|----------|-------------|
| 5 discovery scopes (Local, Regional, Mission, Global, Private) | Novel scope model for overlay networks | Rich but complex |
| mDNS for local, gossip for global discovery | Standard patterns | Low risk |
| Stake-gated global propagation | Economic Sybil resistance | Ties to PoRelay |

### RFC-0852 (DGP) — Gossip Layer

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Deterministic gossip with anti-entropy via binary Merkle descent | Novel: gossip protocols are typically probabilistic | Core determinism innovation |
| GossipStateSummary exchange for reconciliation | Standard anti-entropy pattern with Merkle twist | Well-grounded |
| Bloom filter compression for bandwidth reduction | Standard technique | Low risk |

### RFC-0853 (OCrypt) — Cryptography Layer

| Claim | Evidence | Significance |
|-------|----------|-------------|
| BLAKE3 over SHA-256 for all cryptographic operations | 14x faster, same security level, Zcash/WireGuard precedent | Consistent choice |
| X25519 + HKDF-BLAKE3 + ChaCha20-Poly1305 for envelope encryption | Signal protocol precedent | Well-grounded |
| Session key establishment via X25519 ECDH | Standard | Low risk |
| EncryptedEnvelope as extension of DeterministicEnvelope | Composition pattern | Clean design |

### RFC-0854 (DPS) — Proof Substrate

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Multiple proof backends (STWO, RISC Zero, SP1, Winterfell, Halo2, Groth16, PLONK, Cairo) should be abstracted behind a common interface | 8 backends is ambitious | High complexity |
| Proof verification is Class A (deterministic); proof generation is Class C (non-deterministic) | Correct determinism boundary classification | Critical invariant |
| BLAKE3-256 for proof_commitment | Consistent with OCrypt rationale | Fixed after SHA-256 inconsistency |

### RFC-0855 (MON) — Mission Networks

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Multiple governance models (Enterprise, DAO, AI, Tactical) for different mission types | Real-world need: different trust assumptions | Rich but complex |
| Mission-scoped encryption and key hierarchy | Isolation between missions | Clean security boundary |
| Members, roles, voting within mission overlays | Full governance stack | Large scope |

### RFC-0856 (DRS) — Route Selection

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Trust score = f(uptime, relay_attestations, stake_weight, mission_trust, consensus_participation) | Composite scoring with cap at 1,000,000 | Deterministic, verifiable |
| Multi-path routing with 3 policies (Failover, Redundant, LoadBalance) | Standard networking pattern applied to overlay | Well-grounded |
| Route commitment via BLAKE3 hash of relay_sequence + transport_vectors + diversity_scores + epoch | Deterministic route attestation | Novel for overlay networks |
| 5 diversity dimensions (transport, geo, trust, org, temporal) | Rich diversity model | Complex to compute |

### RFC-0857 (DOM) — Overlay Mempool

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Deterministic intent admission (<1ms) | Performance target | Needs benchmarking |
| Canonical ordering of intents (deterministic across all nodes) | Core consensus requirement | Critical invariant |
| Deterministic eviction cycle (<10ms) | Performance target | Needs benchmarking |
| Economic prioritization via fee/priority fields | Standard mempool economics | Low risk |

### RFC-0858 (ORR) — Onion Routing

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Multi-transport onion routing (Telegram, Matrix, QUIC, etc.) is feasible | No existing system does this; Tor is TCP-only | **Most novel claim in the entire set** |
| 5-hop onion with ChaCha20-Poly1305 per layer achieves <10ms construction | Performance target | Needs benchmarking |
| Cover traffic at <30% overhead provides meaningful privacy | Tor uses ~3x overhead | Ambitious target |
| Mission-scoped onion domains resist intersection attacks | Smaller anonymity sets vs global | Privacy/security tradeoff |
| Route commitment = BLAKE3(relay_hash || transport_hash || diversity_hash || epoch) | Deterministic route attestation | Consistent with DRS |

### RFC-0859 (PCE) — Proof-Carrying Envelopes

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Proofs should be attached to envelopes (not separate channels) | Co-location reduces round trips | Design choice |
| Recursive proof aggregation achieves O(1) verification | Standard ZK composition | Well-grounded |
| STARK proofs <100KB, SNARK proofs <1KB | Current state of the art | Reasonable |
| 8 proof system backends with uniform interface | Ambitious abstraction | High complexity |
| Token economics: OCTO-A for generation, OCTO-N for verification, OCTO-B for relay, OCTO-O for orchestration, OCTO-S for archival | Full economic model | Rich |

### RFC-0860 (PoRelay) — Relay Proofs

| Claim | Evidence | Significance |
|-------|----------|-------------|
| Ed25519 forwarding proofs + heartbeat proofs + bandwidth proofs | Three proof types for relay accountability | Comprehensive |
| Stake-gated proof generation: `reward = base_reward * min(staked/MINIMUM_STAKE, 1.0)` | Economic Sybil resistance | Clean formula |
| Unbonding period prevents stake flash attacks | Standard PoS mechanism | Well-grounded |
| Composite trust score from multiple relay dimensions | Consistent with DRS trust model | Good alignment |

---

## 4. Key Technical Concepts

### Determinism Boundary (RFC-0008 Integration)

Every networking RFC explicitly maps operations to execution classes:

| Class | Meaning | Examples in Networking |
|-------|---------|----------------------|
| **Class A** | Protocol Deterministic | Envelope serialization, signature verification, route computation, proof verification, session key derivation, nonce construction |
| **Class B** | Deterministic Off-Chain | Transport selection (configurable timeouts), gateway capacity |
| **Class C** | Probabilistic | Cover traffic timing, proof generation, gossip fanout timing |

**Critical invariant:** Consensus depends ONLY on Class A operations. Class C operations are inputs, not consensus-critical.

### Cryptographic Primitive Stack

```
BLAKE3-256 .............. Hashing (everywhere)
Ed25519 ................. Signing (envelopes, gateways, relays)
X25519 .................. Key agreement (onion routing, encrypted envelopes)
HKDF-BLAKE3 ............. Key derivation (session keys per hop)
ChaCha20-Poly1305 ....... AEAD encryption (envelopes, onion layers)
Merkle Trees ............ Commitment (route traces, proof inputs, anti-entropy)
```

### Token Economy Integration

| Token | Purpose | RFCs |
|-------|---------|------|
| OCTO-B | Bandwidth (gateway relay, proof transport) | 0850, 0858, 0859, 0860 |
| OCTO-N | Node operations (consensus gateway, proof verification) | 0850, 0859 |
| OCTO-S | Storage (archive gateway, proof archival) | 0850, 0859 |
| OCTO-O | Orchestration (translation gateway, proof aggregation) | 0850, 0859 |
| OCTO-A | GPU compute (proof generation) | 0859 |

### Multi-Transport Architecture

Supported platform types (from implementation):

| Platform | Type ID | Status |
|----------|---------|--------|
| Telegram | 0x0001 | Specified |
| Discord | 0x0002 | Specified |
| Matrix | 0x0003 | Specified |
| Nostr | 0x0004 | Specified |
| Signal | 0x0005 | Specified |
| IRC | 0x0006 | Specified |
| Slack | 0x0007 | Specified |
| WhatsApp | 0x0008 | Specified |
| Webhook | 0x0009 | Specified |
| NativeP2P | 0x000A | Specified |
| SMTP | 0x000B | Specified |
| Custom | 0x000C | Specified |
| WebRTC | 0x000D | Specified |

---

## 5. Implementation Status (crates/octo-network/src/dot/)

### Files Implemented vs Specified

| File | RFC-0850 Spec | Implemented | Lines |
|------|:---:|:---:|-------|
| `mod.rs` | Yes | Yes | ~100 |
| `envelope.rs` | Yes | Yes | ~200 |
| `domain.rs` | Yes | Yes | ~110 |
| `gateway.rs` | Yes | Yes | ~140 |
| `sequence.rs` | Yes | Yes | ~80 |
| `replay.rs` | Yes | Yes | ~150 |
| `config.rs` | **No** (spec doesn't list it) | Yes | ~90 |
| `error.rs` | **No** (spec doesn't list it) | Yes | ~55 |
| `adapters/mod.rs` | Yes | Yes | ~80 |
| `adapters/native_p2p.rs` | Yes | Yes | ~100 |
| `fragment.rs` | Yes | **No** | — |
| `route.rs` | Yes | **No** | — |
| `canonical.rs` | Yes | **No** | — |
| `adapters/telegram.rs` | Yes | **No** | — |
| `adapters/discord.rs` | Yes | **No** | — |
| `adapters/matrix.rs` | Yes | **No** | — |
| `adapters/nostr.rs` | Yes | **No** | — |

**Coverage: 9/17 specified files implemented (53%)**

### Implementation Depth

The implemented files cover **Phase 1 (Core Types)** of RFC-0850's 4-phase plan:

- `BroadcastDomainId` with BLAKE3-256 hashing (13 platform types)
- `DeterministicEnvelope` with all specified fields
- `GatewayIdentity` with 6 gateway classes
- `OverlaySequence` with (epoch, gateway, counter) ordering
- `ReplayCache` with TTL-based eviction
- `PlatformAdapter` trait with async send/receive/capabilities
- `NativeP2P` adapter (stub implementation)
- `DotConfig` with defaults (not in RFC spec but needed)
- `DotError` with 8 error variants

### Files NOT Implemented (Required for Phases 2-4)

| Missing File | Phase | Priority |
|-------------|-------|----------|
| `fragment.rs` | Phase 2 | HIGH — needed for large envelopes |
| `route.rs` | Phase 3 | HIGH — needed for gateway federation |
| `canonical.rs` | Phase 2 | HIGH — needed for signature determinism |
| `adapters/telegram.rs` | Phase 2 | MEDIUM — first real transport |
| `adapters/discord.rs` | Phase 2 | MEDIUM |
| `adapters/matrix.rs` | Phase 3 | MEDIUM |
| `adapters/nostr.rs` | Phase 3 | MEDIUM |

### Modules Not Started (Other RFCs)

No implementation exists for any RFC beyond 0850. The entire GDP (0851), DGP (0852), OCrypt (0853), DPS (0854), MON (0855), DRS (0856), DOM (0857), ORR (0858), PCE (0859), and PoRelay (0860) remain unimplemented.

---

## 6. Contradictions and Gaps

### 6.1 SHA-256 vs BLAKE3-256 Inconsistency (FIXED in latest commit)

**Status:** Resolved by commit `8071534` ("fix: RFC-0852/0856/0857/0860 remaining issues (SHA-256→BLAKE3-256, exec class mapping)")

The earlier versions of several RFCs used SHA-256 in test vectors while the cryptographic rationale mandated BLAKE3-256. This was systematically fixed. The fix is reflected in the latest versions where:
- RFC-0854 test vectors now use BLAKE3-256 (was SHA-256)
- RFC-0859 test vectors now use BLAKE3-256 (was SHA-256)
- All RFCs consistently use BLAKE3-256 for commitments

### 6.2 Circular Dependency: RFC-0850 <-> RFC-0851

RFC-0850 lists RFC-0851 as **optional** dependency (gateway discovery), but RFC-0851 requires RFC-0850. More subtly, RFC-0851 lists in RFC-0850's "optional" dependencies:

```
RFC-0850 (DOT) --optional--> RFC-0851 (GDP)
RFC-0851 (GDP) --requires--> RFC-0850 (DOT)
```

This is not a true circular dependency (optional vs required), but it creates a design tension: DOT cannot use GDP features during initialization, but GDP needs DOT types. This is correctly handled by the topological sort (DOT in Layer 0, GDP in Layer 1).

### 6.3 RFC-0853 Depends on RFC-0852 (Potentially Premature)

RFC-0853 (OCrypt) lists RFC-0852 (DGP) as a **required** dependency. This means cryptographic primitives depend on the gossip protocol, which seems backwards. The rationale is likely that OCrypt needs gossip propagation for key distribution, but this coupling is unusual. Most systems define crypto primitives independently of their distribution mechanism.

**Recommendation:** Consider making RFC-0852 optional for RFC-0853, with key distribution as a separate concern.

### 6.4 RFC-0855/0856 Mutual Dependency

```
RFC-0855 (MON) --optional--> RFC-0856 (DRS)
RFC-0856 (DRS) --requires--> RFC-0855 (MON)
```

DRS requires MON (mission-scoped routing), but MON only optionally uses DRS. This means route selection cannot exist without mission networks, but missions can exist without deterministic route selection. The topological sort resolves this (MON in Layer 4, DRS in Layer 5), but the tight coupling suggests these could be a single RFC.

### 6.5 Scratch Pad Coverage Gaps

The original research document (`docs/research/deterministic-overlay-transport.md`) contains content not fully captured in the formal RFCs:

| Gap | RFC | Impact |
|-----|-----|--------|
| Canonical replay protection for onion envelopes | ORR | Replay attacks on onion-wrapped messages |
| Reputation decay model | PoRelay | Stale trust scores persist indefinitely |
| OCTO-S token in PoRelay | PoRelay | Archive gateway incentives undefined |

### 6.6 Over-Specification Risk

The networking layer specifies **5 token types** (OCTO-A/B/N/O/S) across **6 gateway classes** with **8 proof backends** and **13 transport platforms**. This is a very large surface area for a first implementation.

**Recommendation:** Prioritize a minimal viable subset:
- 2 gateway classes (Edge, Relay)
- 2-3 transports (NativeP2P, Telegram, Matrix)
- 1 proof backend (STWO)
- 1 token type (OCTO-B for bandwidth)

### 6.7 Missing External RFC References

Several networking RFCs reference external RFCs that are not in the accepted sets analyzed:

| Referenced RFC | Status | Referenced By |
|---------------|--------|---------------|
| RFC-0008 (Determinism Boundary) | Not in this analysis | 0855, 0856, 0859 |
| RFC-0009 (Identity Management) | Not in this analysis | 0851, 0853, 0855 |
| RFC-0102 (Wallet Crypto) | Not in this analysis | 0853, 0856 |
| RFC-0630 (Proof-of-Inference) | Not in this analysis | 0859, 0860 |
| RFC-0631 (Proof-of-Dataset Integrity) | Not in this analysis | 0859 |
| RFC-0650 (Proof Aggregation) | Not in this analysis | 0854, 0859, 0860 |
| RFC-0843 (OCTO-Network Protocol) | In networking dir | 0850, 0851 |

These are likely in `rfcs/accepted/process/` or `rfcs/accepted/proof-systems/` but were not included in this analysis scope.

---

## 7. Complexity Assessment

### Per-RFC Complexity Score

| RFC | Spec Complexity | Implementation Complexity | Risk |
|-----|:---:|:---:|:---:|
| 0850 DOT | Medium | Medium | Low — core types are straightforward |
| 0851 GDP | Medium | Medium | Medium — discovery across heterogeneous transports |
| 0852 DGP | High | High | High — deterministic gossip is novel |
| 0853 OCrypt | Medium | Medium | Medium — standard crypto, integration complexity |
| 0854 DPS | Very High | Very High | High — 8 proof backends, abstraction layer |
| 0855 MON | Very High | Very High | High — governance, missions, roles, voting |
| 0856 DRS | High | High | High — trust scoring, multi-path, diversity |
| 0857 DOM | Medium | Medium | Medium — mempool is well-understood pattern |
| 0858 ORR | Very High | Very High | **Critical** — multi-transport onion routing is uncharted |
| 0859 PCE | High | High | High — proof attachment, aggregation, 8 backends |
| 0860 PoRelay | High | High | Medium — proof generation, Sybil resistance |

### Estimated Implementation Effort (Person-Months)

| Phase | RFCs | Effort | Dependencies |
|-------|------|--------|-------------|
| Phase 1: Foundation | 0850 complete | 2-3 months | None |
| Phase 2: Discovery + Gossip | 0851, 0852 | 3-4 months | Phase 1 |
| Phase 3: Crypto | 0853 | 2-3 months | Phase 2 |
| Phase 4: Proof + Missions | 0854, 0855 | 6-8 months | Phase 3 |
| Phase 5: Routing + Proofs | 0856, 0859, 0860 | 6-8 months | Phase 4 |
| Phase 6: Application | 0857, 0858 | 6-8 months | Phase 5 |
| **Total** | **All 11** | **25-34 months** | — |

---

## 8. Implementation Priority Recommendations

### Immediate (Next 1-2 Months)

1. **Complete RFC-0850 Phase 1** — Implement `fragment.rs`, `canonical.rs`, and at least one real adapter (Telegram)
2. **Fix circular dependency pattern** — Make RFC-0851 optional in RFC-0850 explicit (already done, just document clearly)
3. **Resolve remaining hash inconsistencies** — Audit all test vectors for BLAKE3-256 consistency

### Short-Term (Months 2-4)

4. **Implement RFC-0851 (GDP)** — Gateway discovery is prerequisite for everything
5. **Implement RFC-0852 (DGP)** — Gossip propagation enables distributed operation
6. **Start RFC-0853 (OCrypt)** — Crypto primitives (can overlap with GDP/DGP)

### Medium-Term (Months 4-8)

7. **Implement RFC-0855 (MON)** — Mission networks (core application model)
8. **Implement RFC-0856 (DRS)** — Route selection (enables multi-hop)
9. **Start RFC-0854 (DPS)** — Proof substrate (single backend: STWO)

### Long-Term (Months 8-16)

10. **Implement RFC-0858 (ORR)** — Onion routing (highest novelty, highest risk)
11. **Implement RFC-0859 (PCE)** — Proof-carrying envelopes
12. **Implement RFC-0860 (PoRelay)** — Relay proofs
13. **Implement RFC-0857 (DOM)** — Overlay mempool

---

## 9. Cross-Reference Matrix

### RFC-to-RFC Dependency Matrix

```
      0850 0851 0852 0853 0854 0855 0856 0857 0858 0859 0860
0850   .    o    .    .    .    .    .    .    .    .    .
0851   R    .    .    .    .    .    .    .    .    .    o
0852   R    R    .    o    .    .    .    .    .    .    .
0853   R    R    R    .    o    .    .    .    .    .    .
0854   R    .    .    R    .    .    .    .    .    .    .
0855   R    R    R    R    o    .    o    .    .    .    .
0856   R    R    R    R    o    R    .    .    .    .    o
0857   R    R    R    R    o    R    R    .    .    o    .
0858   R    o    o    R    o    .    R    o    .    .    o
0859   R    .    .    R    R    o    .    .    .    .    o
0860   R    R    .    R    .    .    .    .    .    .    .

R = Required, o = Optional
```

### RFC-to-Numeric RFC Dependencies

| Networking RFC | 0104 (DFP) | 0105 (DQA) | 0126 (DCS) |
|---------------|:---:|:---:|:---:|
| 0850 DOT | | | R |
| 0851 GDP | | | R |
| 0852 DGP | | | R |
| 0854 DPS | R | R | R |
| 0856 DRS | | | R |
| 0857 DOM | R | R | |
| 0858 ORR | o | o | |
| 0860 PoRelay | o | o | |

---

## 10. Summary of Findings

### Strengths

1. **Comprehensive spec** — 327KB of detailed specification with test vectors, performance targets, and adversarial review
2. **Consistent determinism model** — Every RFC explicitly maps operations to RFC-0008 execution classes
3. **Uniform crypto stack** — BLAKE3-256, Ed25519, X25519, ChaCha20-Poly1305 used consistently
4. **Clear dependency graph** — Topological layers are well-defined with no true circular dependencies
5. **Economic integration** — Token incentives specified per gateway role and proof type
6. **Implementation started** — Core types (RFC-0850 Phase 1) partially implemented in Rust

### Weaknesses

1. **Zero implementation beyond Phase 1** — Only DOT core types exist; 10 of 11 RFCs have no code
2. **Over-specified surface area** — 13 transports, 8 proof backends, 5 tokens, 6 gateway classes is too much for initial implementation
3. **Novel claims untested** — Multi-transport onion routing (ORR) and deterministic gossip (DGP) have no precedent
4. **Gaps from scratch pad** — Replay protection for onion envelopes, reputation decay, OCTO-S in PoRelay
5. **Tight coupling** — RFC-0855/0856 mutual dependency, RFC-0853 depending on RFC-0852
6. **All Draft status** — None of the 11 networking RFCs have been accepted yet

### Key Risk: RFC-0858 (ORR)

Onion relay routing over heterogeneous social transports is the most novel and highest-risk claim. No existing system combines:
- Multi-hop onion routing (Tor does this, but TCP-only)
- Multi-transport overlay (Nostr does relay federation, but no onion routing)
- Deterministic route selection (unique to CipherOcto)
- Cover traffic over social messengers (unprecedented)

This RFC should be prototyped early and tested with real Telegram/Discord transports before committing to the full specification.
