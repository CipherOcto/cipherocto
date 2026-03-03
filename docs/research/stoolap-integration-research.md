# Research: Stoolap Integration with AI Quota Marketplace

## Executive Summary

This research investigates integrating the Stoolap blockchain SQL database (with its ZK proof capabilities) into the CipherOcto AI Quota Marketplace system. Stoolap provides verifiable state proofs, compressed STARK proofs, and confidential query operations that could enhance the quota marketplace's trust, transparency, and functionality.

## Problem Statement

The current AI Quota Marketplace design (RFC-0100, RFC-0101) faces challenges:
1. **Trust** - Buyers must trust sellers to execute prompts correctly
2. **Verification** - No cryptographic proof that work was completed
3. **Dispute Resolution** - Relies on reputation, not cryptographic verification
4. **State Management** - Centralized/off-chain listing registry

Stoolap's blockchain SQL database with ZK proofs could address these.

## Research Scope

- What's included: Stoolap capabilities, integration points, protocol changes
- What's excluded: Full implementation details (belongs in RFC)

---

## Stoolap Current Capabilities

### Phase 1: Foundation (Complete)

| Feature | Implementation | Details |
|--------|---------------|---------|
| **HexaryProof** | ✅ Implemented | 16-way Merkle trie, ~68 byte proofs |
| **Deterministic Types** | ✅ Implemented | DetermValue with inline/heap optimization |
| **Blockchain Consensus** | ✅ Implemented | Gas-metered transaction execution |
| **RowTrie** | ✅ Implemented | Hexary Merkle trie with proof generation |

**Performance:**
- Proof size: ~68 bytes (target <100)
- Verification time: ~2-3 μs (target <5)
- Batch verification: ~50 μs for 100 proofs

### Phase 2: Zero-Knowledge Proofs (Complete)

| Feature | Implementation | Details |
|--------|---------------|---------|
| **STWO Integration** | ✅ Implemented | Circle STARK prover/verifier in Rust |
| **Cairo Programs** | ✅ Implemented | 3 Cairo programs |
| **Compressed Proofs** | ✅ Implemented | Aggregate multiple proofs |
| **Confidential Queries** | ✅ Implemented | Pedersen commitments |
| **L2 Rollup** | ✅ Implemented | Off-chain execution, on-chain verification |
| **STWO Plugin** | ✅ Implemented | Modular architecture |

**Deliverables:**
- `stwo-plugin/` - STWO verification plugin (C-compatible FFI)
- `stwo-bench/` - Benchmarks
- `cairo/hexary_verify.cairo` - Proof verification
- `cairo/state_transition.cairo` - State transitions
- `cairo/merkle_batch.cairo` - Batch verification

### Phase 3: Protocol Enhancement (Planned)

| Feature | Status | RFC |
|--------|--------|-----|
| Block Production | Draft | RFC-0301 |
| Block Validation | Draft | RFC-0302 |
| Network Protocol | Draft | RFC-0303 |
| Signature Schemes | Draft | RFC-0304 |

---

## Integration Opportunities

### 1. Verifiable Quote Execution

**Current Problem:** No cryptographic proof that seller executed the prompt.

**Stoolap Solution:**
- Seller submits transaction to Stoolap with prompt hash
- Stoolap generates HexaryProof of execution
- Buyer verifies proof without trusting seller

```rust
// Seller submits execution
let tx = Transaction::new(
    prompt_hash,
    response_hash,
    seller_wallet,
);

// Stoolap generates proof
let proof = hexary_proof::verify(&tx);

// Buyer verifies
hexary_proof::verify(proof).unwrap();
```

### 2. Compressed Proof Marketplace

**Current Problem:** Each prompt verification requires individual proof.

**Stoolap Solution:**
- Aggregate multiple prompt executions into single STARK proof
- Dramatically reduce on-chain verification costs

```rust
// Batch of 1000 executions
let batch = PromptBatch::new(prompts);

// Compress to single STARK
let stark_proof = stwo_prover::prove(batch);

// On-chain verification: single proof vs 1000
```

### 3. Confidential Query Operations

**Current Problem:** Marketplace sees all listing details, pricing.

**Stoolap Solution:**
- Use Pedersen commitments for listing details
- Prove listing validity without revealing specifics
- Enable private bidding

```rust
// Encrypted listing
let commitment = pedersen::commit(listing.price, listing.quantity);

// Prove price in range without revealing
let range_proof = pedersen::range_proof(commitment, 0..1000);
```

### 4. Decentralized Listing Registry

**Current Problem:** Centralized or off-chain registry.

**Stoolap Solution:**
- On-chain listing registry with Stoolap
- Each listing is a blockchain record
- Verifiable state transitions

```rust
// Listing as blockchain record
struct Listing {
    id: u64,
    seller: Address,
    provider: String,
    quantity: u64,
    price_per_prompt: u64,
    commitment: Hash,
}

// Create listing transaction
let tx = ListingTx::Create(Listing { ... });

// Stoolap generates state proof
let proof = row_trie::prove(&tx);
```

### 5. L2 Rollup for Scale

**Current Problem:** High on-chain costs for small transactions.

**Stoolap Solution:**
- Execute marketplace on L2
- Batch thousands of operations
- Submit single proof to L1

---

## Required Changes to Current Design

### Use Case Changes

| Current | Proposed | Rationale |
|---------|----------|-----------|
| Off-chain registry | Stoolap on-chain registry | Verifiable, decentralized |
| Reputation-based trust | Proof-based trust | Cryptographic, not social |
| Manual dispute | Automated proof verification | Faster resolution |
| Fixed pricing | Confidential auctions | Privacy-preserving |

### RFC-0100 Changes

| Section | Change |
|---------|--------|
| **Registry** | Add Stoolap on-chain option |
| **Settlement** | Add ZK proof verification step |
| **Dispute** | Add "submit proof" resolution path |
| **Escrow** | Use Stoolap state for escrow |

### RFC-0101 Changes

| Section | Change |
|---------|--------|
| **Provider** | Add Stoolap provider type |
| **Verification** | Add proof submission after execution |
| **Balance** | Read from Stoolap state |

---

## Architecture Proposal

```
┌─────────────────────────────────────────────────────────────┐
│                   CipherOcto Network                        │
│  ┌─────────────────┐    ┌─────────────────────────────┐ │
│  │  Quota Router   │───▶│   Stoolap L2                │ │
│  │    Agent        │    │  ┌─────────────────────────┐ │ │
│  │  (RFC-0101)     │    │  │  Listing Registry      │ │ │
│  └─────────────────┘    │  │  - Create listing     │ │ │
│         │                │  │  - Update quantity    │ │ │
│         ▼                │  │  - Verify execution   │ │ │
│  ┌─────────────────┐    │  └─────────────────────────┘ │ │
│  │  Market Client   │    │  ┌─────────────────────────┐ │ │
│  │                 │───▶│  │  ZK Proof Layer       │ │ │
│  └─────────────────┘    │  │  - HexaryProof        │ │ │
│         │                │  │  - STARK compression │ │ │
│         ▼                │  │  - Confidential ops  │ │ │
│  ┌─────────────────┐    │  └─────────────────────────┘ │ │
│  │   Stoolap      │◀───│                             │ │
│  │   Node          │    │                             │ │
│  └─────────────────┘    └─────────────────────────────┘ │
│         │                                                  │
│         ▼                                                  │
│  ┌─────────────────┐                                      │
│  │  L1 Settlement  │ (Ethereum/other)                    │
│  └─────────────────┘                                      │
└─────────────────────────────────────────────────────────────┘
```

---

## Risk Assessment

| Risk | Mitigation | Severity |
|------|------------|----------|
| Integration complexity | Phased rollout | Medium |
| Performance overhead | Use L2, batch proofs | Low |
| ZK proof generation time | Pre-compute, async | Medium |
| Stoolap not production-ready | Run on testnet first | Medium |

---

## Recommendations

### Recommended Approach

1. **Phase 1 (MVE):** Keep current design, add Stoolap as future upgrade path
2. **Phase 2:** Add Stoolap provider type to router (optional verification)
3. **Phase 3:** Migrate to on-chain registry with Stoolap
4. **Phase 4:** Enable confidential queries, L2 rollup

### Key Integration Points

| Priority | Integration | Impact |
|----------|-------------|--------|
| High | Stoolap provider type | Enable proof-based verification |
| High | On-chain listing registry | Decentralize registry |
| Medium | STARK proof batching | Reduce costs |
| Medium | Confidential queries | Privacy |
| Low | L2 rollup | Scale |

---

## Next Steps

- [ ] Create Use Case for Stoolap integration?
- [ ] Draft RFC for Stoolap provider type
- [ ] Define migration path from off-chain to on-chain

---

## References

- Parent Document: BLUEPRINT.md
- Stoolap: `/home/mmacedoeu/_w/databases/stoolap`
- Stoolap RFCs: `rfcs/0101`, `rfcs/0201-0205`
- CipherOcto Use Case: `docs/use-cases/ai-quota-marketplace.md`
- CipherOcto RFCs: `rfcs/0100`, `rfcs/0101`

---

**Research Status:** Complete
**Recommended Action:** Proceed to Use Case update
