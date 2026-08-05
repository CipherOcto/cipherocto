# Mission: ZK Proof Verification System

## Status
Open

## RFC
RFC-0100: AI Quota Marketplace Protocol
RFC-0102: Wallet Cryptography Specification

## Cross-link (2026-07-22 — crypto extraction amendment per [[stoolap-general-purpose-db]])

Shares STWO bridge with sibling mission `missions/claimed/0958-a-zk-capability-circuit.md` (v0.3 amendment 2026-07-22). 0958-a ships the FFI bridge in `crates/zk-vendor/stwo-sys/` (cdylib, nightly `2025-06-23`) plus the cipherocto-side wrapper `crates/zk-vendor/src/lib.rs::loaded_library() -> Option<StwoSys>` and the layered verifier `crates/zk-verifier/src/lib.rs` (FFI > Stub). When this mission implements STWO verifier integration, it should depend on `crates/zk-verifier/` (cargo path dep) and call `zk_verifier::verify_capability_zk(...)` through the public API. The cipherocto workspace does not patch STWO via `[patch.crates-io]`; the bridge is a local cdylib libloaded at runtime. Cross-link is one-directional: this mission depends on 0958-a's STWO bridge; 0958-a does not depend on this mission.

## Blockers / Dependencies

- **Blocked by:** Mission: Stoolap Provider Integration (must complete first)
- **Sibling mission substrate:** 0958-a (claimed 2026-07-22; STWO source vendored in `crates/zk-vendor/`)

## Acceptance Criteria

- [ ] Integrate STWO verifier for STARK proofs
- [ ] Batch multiple proofs into single verification
- [ ] On-chain proof submission to Stoolap
- [ ] Verify proofs before releasing payment
- [ ] Display verification status
- [ ] GPU-accelerated proof generation (optional optimization)

## Description

Enable ZK proof-based verification for marketplace transactions using Stoolap's STARK proving system.

## Technical Details

### Proof Types (from Stoolap)

| Proof Type | Use Case | Verification | Size |
|-----------|-----------|---------------|------|
| HexaryProof | Individual execution | ~2-3 μs | ~68 bytes |
| StarkProof | Batch verification | ~15 ms | 100-500 KB |
| CompressedProof | Multiple batches | ~100ms | ~10 KB |

### Stoolap Integration

```mermaid
flowchart TD
    subgraph STOOLAP["Stoolap ZK Stack"]
        SQL[SQL Query] --> HEX[HexaryProof]
        HEX --> BATCH[Batch]
        BATCH --> STARK[STARK Proof<br/>via stwo-cairo-prover]
    end

    subgraph CIPHER["CipherOcto Integration"]
        REQ[Request] --> VERIFY[Verify Proof]
        VERIFY --> PAY[Release OCTO-W]
    end

    STOOLAP -->|verify| CIPHER
```

### GPU Acceleration (Optional)

For production, consider GPU-accelerated STWO:

| Implementation | Speedup | Notes |
|---------------|---------|-------|
| NitrooZK-stwo | 22x-355x | Cairo AIR support |
| ICICLE-Stwo | 3x-7x | Drop-in backend |
| stwo-gpu | ~193% | Multi-GPU scaling |

See: `docs/research/stwo-gpu-acceleration.md`

### Verification Flow

```mermaid
sequenceDiagram
    participant B as Buyer
    participant S as Seller
    participant P as Stoolap

    B->>S: Request prompt execution
    S->>P: Submit transaction
    P-->>S: Return proofs
    S-->>B: Response + proofs
    B->>P: Verify proofs
    P-->>B: Verified / Failed
    B->>S: Release payment (if verified)
```

### CLI Commands

```bash
# Verify a proof
quota-router verify --proof <proof-id>

# Batch verify multiple proofs
quota-router verify --batch <proof-ids>

# View verification history
quota-router verify history
```

## Implementation Notes

1. **Async verification** - Don't block response, verify in background
2. **Batch for cost** - Combine multiple verifications
3. **Caching** - Cache verified proofs to avoid re-verification
4. **GPU acceleration** - Consider NitrooZK-stwo for production (22x-355x speedup)

## Research References

- [Stoolap vs LuminAIR Comparison](../docs/research/stoolap-luminair-comparison.md)
- [STWO GPU Acceleration](../docs/research/stwo-gpu-acceleration.md)
- [Privacy-Preserving Query Routing](../docs/use-cases/privacy-preserving-query-routing.md)
- [Provable QoS](../docs/use-cases/provable-quality-of-service.md)

## Claimant

<!-- Add your name when claiming -->

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** ZK Proofs
