# Use Case: TurboQuant-Enhanced Vector Quantization for Stoolap

## Problem

Stoolap's current vector quantization has a memory-accuracy tradeoff that is suboptimal for production AI workloads:

1. **Binary Quantization (BQ)** achieves 32x compression but uses Hamming distance, which does not preserve L2/cosine semantics, resulting in poor recall (typically <85%)
2. **Scalar Quantization (SQ)** preserves distance semantics but carries 1-2 bits per-value overhead for quantization constants, wasting ~15-20% of storage
3. **Product Quantization (PQ)** offers good compression but requires expensive k-means training on representative data—operationally burdensome and slow to rebuild indexes

For AI workloads storing millions of embeddings (RAG, agent memory, semantic search), these limitations force a painful choice: fast-but-inaccurate (BQ), accurate-but-wasteful (SQ), or slow-to-build (PQ).

## Stakeholders

- **Primary**: AI application developers using Stoolap for vector storage (RAG pipelines, agent memory systems)
- **Secondary**: CipherOcto node operators running consensus-critical workloads that require vector search
- **Affected**: Quota marketplace participants relying on vector similarity for provider matching

## Motivation

TurboQuant (Google Research, March 2026) demonstrates that a two-stage quantization approach—PolarQuant for zero-overhead compression plus QJL for residual error correction—achieves **3-bit KV cache quantization without training or accuracy loss**, compared to the 4-bit minimum (with overhead) of traditional SQ. This would enable:

- **10x compression** with >95% recall (vs 32x with <85% recall for BQ)
- **Near-zero preprocessing** (random rotation replaces k-means training)
- **8x faster search** on H100 GPUs due to smaller index footprint

CipherOcto's stoolap integration for the quota marketplace would benefit from TurboQuant's efficiency for storing provider embeddings, model weight approximations, and agent memory vectors—reducing storage costs by 8-10x while maintaining retrieval quality.

## Success Metrics

| Metric | Target | Measurement |
|-------|--------|-------------|
| Compression ratio | ≥8x vs f32 | Stored bytes / (N × dim × 4 bytes) |
| Recall@10 | ≥95% vs unquantized | Top-10 overlap with full-precision search |
| Index build time | <1 minute for 1M vectors | Wall clock, no training phase |
| Search latency | <5ms @ 1K QPS | P99 on H100, 1M vector index |
| Memory overhead | 0 bits per quantized value | Quantization constants eliminated |

## Constraints

- **Must not**: Degrade below 90% recall@10 for production workloads
- **Must not**: Require per-deployment training or hyperparameter tuning
- **Limited to**: Vectors ≤4096 dimensions (Stoolap HNSW-D limit)
- **Must**: Support deterministic execution on consensus paths (via DQA-based preprocessing)
- **Should**: Gracefully fall back to existing SQ/PQ/BQ if TurboQuant unavailable

## Non-Goals

- This use case does NOT address GPU-only optimizations (CUDA kernels for QJL)
- This use case does NOT address vector quantization for ZK circuits (off-chain only)
- This use case does NOT replace HNSW-D deterministic index (TurboHNSW is off-chain optimization)

## Impact

### Positive Outcomes

1. **Storage costs reduced 8-10x** for vector workloads (1M × 768-dim: 24.6 GB → 2.5-3 GB)
2. **Index rebuild time eliminated** (PQ hours → TurboPQ seconds)
3. **Retrieval quality improved** vs BQ (95% vs 85% recall@10)
4. **Consensus compatibility preserved** via deterministic seeded rotation

### Tradeoffs

1. **Increased CPU for quantization** (polar transform per vector insert)
2. **3-bit mode is new territory** (less production validation than 4-bit SQ)
3. **Separate indexes needed**: TurboHNSW for off-chain performance, HNSW-D for consensus

### Infrastructure Changes

- New `TurboQuantType` enum variant in `QuantizationConfig`
- Optional `TurboScalarQuantizer` alongside existing `BinaryQuantizer`
- Index type split: `TurboHNSWIndex` (off-chain) vs `HNSWDIndex` (consensus)

## Related RFCs

- [RFC-0303 (Retrieval)](../rfcs/draft/retrieval/0303-deterministic-vector-index.md): HNSW-D (deterministic index)
- [RFC-0304 (Retrieval)](../rfcs/draft/retrieval/0304-verifiable-vector-query-execution.md): VVQE (verifiable query execution)
- [RFC-0109 (Numeric/Math)](../rfcs/accepted/numeric/0109-deterministic-linear-algebra-engine.md): DLAE (distance primitives)
- [RFC-0104 (Numeric/Math)](../rfcs/accepted/numeric/0104-deterministic-floating-point.md): DFP (deterministic float)

## Future RFC Candidates

- **RFC-0915 (Retrieval)**: TurboQuant-Enhanced Vector Quantization
- **RFC-0916 (Retrieval)**: TurboHNSW Index (quantized HNSW for off-chain)

## Technical Debt Considerations

1. **PolarQuant rotation determinism**: Need to validate seeded PRNG produces consistent results across implementations for consensus paths
2. **3-bit mode not in current stoolap**: Requires new quantization type implementation
3. **QJL integration complexity**: Asymmetric estimation requires careful implementation

## Appendix: TurboQuant vs Current Stoolap

| Aspect | Current Stoolap | TurboQuant-Enhanced |
|--------|----------------|---------------------|
| Scalar Quantization | 4 bits + 1-2 bits constants | 4 bits, 0 constants |
| Binary Quantization | 1 bit, Hamming distance | N/A (better alternatives exist) |
| Product Quantization | Requires k-means training | No training (random rotation) |
| 3-bit mode | Not available | 3-bit via polar+QJL |
| Index size (1M×768) | ~3 GB (SQ) | ~2.3-3 GB |
| Index build time (1M) | Hours (PQ training) | Seconds |
| Recall@10 (vs f32) | BQ: 85%, SQ: 98% | 95%+ at 3-bit |
