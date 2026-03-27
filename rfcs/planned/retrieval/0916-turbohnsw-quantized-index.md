# RFC-0916 (Retrieval): TurboHNSW Quantized Index

## Status

Planned

## Summary

Build HNSW graph indices directly on quantized vectors (TurboScalar, ThreeBit, TurboPQ) rather than on raw f32 values. This enables HNSW search on compressed data, reducing index memory footprint by 8x and achieving 8x search speedup on H100 GPUs while maintaining ≥95% recall@10.

## Why Needed

Current stoolap HNSW search flow:

1. **Insert**: raw f32 → stored as f32 → HNSW built on f32
2. **Search**: query f32 → HNSW traversal on f32 → candidates → distance re-rank

This means the HNSW graph stores full-precision vectors. For 1M × 768-dim vectors at f32:
- **Index memory**: ~24.6 GB (just for vectors in HNSW)
- **Search bandwidth**: Full f32 vectors moved through cache

TurboQuant achieves 3-bit KV cache (10.7x compression). The missing piece: **HNSW on quantized data** so the graph itself is smaller and search operates on compressed representations.

Google benchmarks show 8x H100 speedup when HNSW operates on quantized vs f32 vectors.

## Scope

### Architecture

```
Traditional HNSW:
┌─────────────────────────────────────┐
│  Vectors stored as f32              │
│  HNSW edges point to f32 values     │
│  Search: load f32 → compare          │
└─────────────────────────────────────┘

TurboHNSW:
┌─────────────────────────────────────┐
│  Vectors stored as quantized (3-4 bits) │
│  HNSW edges point to quantized values  │
│  Search: quantized compare → re-rank    │
└─────────────────────────────────────┘
```

### Dual-Phase Search

**Phase 1: Quantized HNSW traversal**
- Traverse HNSW graph using quantized vector representations
- Use quantized distance metric (approximate)
- Returns candidate set (may include false positives)

**Phase 2: Re-ranking**
- Fetch raw f32 vectors for candidates
- Compute exact f32 distance
- Return top-K by exact distance

### Index Structure

```rust
pub struct TurboHNSWIndex {
    /// Quantized vector storage (TurboScalar, ThreeBit, or TurboPQ)
    quantizer: Box<dyn TurboQuantizer>,

    /// HNSW graph on quantized vectors
    graph: HNSWGraph<QuantizedNode>,

    /// Optional: store raw f32 for re-ranking
    raw_storage: Option<VectorStorage<f32>>,

    /// Enable re-ranking after HNSW traversal
    rerank: bool,
}

pub struct QuantizedNode {
    id: u64,
    /// Quantized vector representation
    quantized: QuantizedVector,
    /// Precomputed approximate for initial graph traversal
    approx: Vec<f32>,
}
```

### Distance Metrics

| Search Phase | Distance Metric | Rationale |
|--------------|-----------------|-----------|
| HNSW traversal | Quantized (polar/angular) | Fast, approximate |
| Re-rank | L2Squared via DLAE | Exact, deterministic |

### Memory Savings

| Index Type | Memory for 1M × 768 | Savings |
|------------|----------------------|---------|
| f32 HNSW | 24.6 GB | baseline |
| TurboScalar HNSW | ~3 GB | 8.2x |
| ThreeBit HNSW | ~2.3 GB | 10.7x |

### Search Latency Targets (H100)

| Config | Index Size | QPS | Recall@10 |
|--------|------------|-----|-----------|
| f32 | 24.6 GB | baseline | 1.0 |
| TurboScalar + rerank | 3 GB | 4-6x | ≥97% |
| ThreeBit + rerank | 2.3 GB | 6-8x | ≥95% |

## Dependencies

**Requires:**

- RFC-0915 (Planned): TurboQuant Vector Quantization — quantization types
- RFC-0303 (Draft): HNSW-D — base HNSW implementation

**Optional:**

- RFC-0304 (Draft): VVQE — verifiable query execution

## Relationship to HNSW-D

| Index Type | Purpose | Location |
|------------|---------|----------|
| HNSW-D | Deterministic consensus paths | On-chain / consensus-critical |
| TurboHNSW | Performance optimization | Off-chain / production |

TurboHNSW is NOT deterministic (uses f32 approximation for graph traversal). HNSW-D remains the canonical index for consensus. TurboHNSW provides the off-chain performance path with re-ranking to maintain accuracy.

## Related RFCs

- RFC-0915 (Planned): TurboQuant Vector Quantization
- RFC-0303 (Draft): Deterministic Vector Index (HNSW-D)
- RFC-0304 (Draft): Verifiable Vector Query Execution (VVQE)
- RFC-0109 (Accepted): Deterministic Linear Algebra Engine (DLAE)
- [Use Case: TurboQuant-Enhanced Vector Quantization](../../docs/use-cases/turboquant-vector-quantization.md)
- [Research: TurboQuant-Stoolap Enhancement](../../docs/research/turboquant-stoolap-enhancement.md)

## Next Steps

When ready to implement: move from `rfcs/planned/retrieval/` to `rfcs/draft/retrieval/` with full specification including dual-phase search algorithm, re-ranking strategy, and benchmark targets, then open PR for adversarial review.
