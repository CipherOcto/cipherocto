# RFC-0915 (Retrieval): TurboQuant-Enhanced Vector Quantization

## Status

Planned

## Summary

Add three TurboQuant-enhanced quantization types to Stoolap's vector storage engine: TurboScalar (4-bit SQ with zero constant overhead via PolarQuant), ThreeBit (3-bit via polar+QJL), and TurboPQ (no-training PQ via random rotation). This achieves 8-10x compression with ≥95% recall@10, compared to existing SQ (wastes 15-20% on constants), BQ (<85% recall), or PQ (hours of training).

## Why Needed

Stoolap's current quantization options force a tradeoff:

| Type | Compression | Recall | Problem |
|------|-------------|--------|---------|
| BQ | 32x | <85% | Hamming ≠ L2/cosine |
| SQ | ~8x effective | ~98% | 1-2 bits/val overhead for constants |
| PQ | 4-64x | ~95% | Requires k-means training (hours) |

TurboQuant (Google Research, March 2026) demonstrates:
- **3-bit KV cache** without accuracy loss via PolarQuant + QJL
- **Zero constant overhead** via polar coordinate geometry
- **No training** via random Hadamard rotation
- **8x faster H100 search** due to smaller index

CipherOcto quota marketplace, RAG pipelines, and agent memory systems need efficient vector storage without the operational burden of PQ training or the quality loss of BQ.

## Scope

### Quantization Types to Add

```
QuantizationType
├── Scalar        (existing: 4 bits + constant overhead)
├── Product       (existing: 4-64 bits, k-means training)
├── Binary        (existing: 1 bit, Hamming distance)
│
├── TurboScalar   (NEW: 4 bits, 0 constant overhead)
├── ThreeBit      (NEW: 3 bits, polar + QJL residual)
└── TurboPQ      (NEW: 2-8 bits, no training)
```

### TurboScalar (PolarQuant-based SQ)

- Random rotation (Hadamard transform) before quantization
- Polar decomposition: radius + angle vectors
- Angle quantization on known circular grid (no per-dimension constants)
- Deterministic via seeded PRNG (seed = vector_id hash)

### ThreeBit Mode

- 2-bit polar quantization + 1-bit QJL residual error correction
- Achieves 10.7x compression (vs 8x for SQ)
- Recall@10 ≥95% (vs <85% for BQ)
- QJL: Johnson-Lindenstrauss projection to 1-bit sign representation

### TurboPQ (No-Training PQ)

- Random rotation induces coordinate concentration (eliminates k-means need)
- Split into sub-vectors after rotation
- Polar quantize each sub-vector independently
- Preprocessing: seconds vs hours for k-means

### API Changes

```rust
pub enum QuantizationType {
    // Existing variants unchanged
    Scalar,
    Product,
    Binary,
    // New TurboQuant variants
    TurboScalar { bits_per_angle: u8 },  // default 2
    ThreeBit,
    TurboPQ { bits_per_subvector: u8 },  // 2-8
}
```

### Performance Targets

| Config | Compression | Recall@10 | Index Build |
|--------|-------------|-----------|-------------|
| TurboScalar | 8x | ≥97% | <1s |
| ThreeBit | 10.7x | ≥95% | <1s |
| TurboPQ-4bit | 8x | ≥94% | <10s |

## Dependencies

**Requires:**

- RFC-0303 (Draft): HNSW-D — HNSW index for vector storage
- RFC-0109 (Accepted): DLAE — distance primitives for search

**Optional:**

- RFC-0304 (Draft): VVQE — verifiable query execution (for consensus paths)

## Related RFCs

- RFC-0303 (Draft): Deterministic Vector Index (HNSW-D)
- RFC-0304 (Draft): Verifiable Vector Query Execution (VVQE)
- RFC-0109 (Accepted): Deterministic Linear Algebra Engine (DLAE)
- [Use Case: TurboQuant-Enhanced Vector Quantization](../../docs/use-cases/turboquant-vector-quantization.md)
- [Research: TurboQuant-Stoolap Enhancement](../../docs/research/turboquant-stoolap-enhancement.md)

## Next Steps

When ready to implement: move from `rfcs/planned/retrieval/` to `rfcs/draft/retrieval/` with full specification, then open PR for adversarial review per BLUEPRINT.md process.
