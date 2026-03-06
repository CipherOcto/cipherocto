# RFC-0103: Unified Vector-SQL Storage Engine

## Status

Draft

## Summary

This RFC specifies the design for merging Qdrant's vector search capabilities with Stoolap's SQL/MVCC engine to create a unified vector-SQL database. The resulting system preserves Stoolap's blockchain-oriented features (Merkle tries, deterministic values, ZK proofs) while adding Qdrant's quantization, sparse vectors, payload filtering, and GPU acceleration.

## Motivation

### Problem Statement

Current AI applications require multiple systems:
- **Vector database** (Qdrant, Pinecone, Weaviate) for similarity search
- **SQL database** (PostgreSQL, SQLite) for structured data
- **Blockchain** for verification/audit

This creates operational complexity, data consistency challenges, and latency from cross-system queries.

### Why This Matters for CipherOcto

CipherOcto's architecture requires:
1. **Vector similarity search** for agent memory/retrieval
2. **SQL queries** for structured data (quotas, payments, reputation)
3. **Blockchain verification** for provable state (Merkle proofs)
4. **MVCC transactions** for concurrent operations

A unified system reduces infrastructure complexity while maintaining all required capabilities.

## Specification

### Architecture Overview

```mermaid
graph TB
    subgraph "Unified Storage Engine"
        A[SQL Parser] --> B[Query Planner]
        B --> C[Optimizer]
        C --> D[Executor]

        D --> E[MVCC Engine]
        E --> F[Write-Ahead Log]

        D --> G[Vector Engine]
        G --> H[HNSW Index]
        G --> I[Quantization]
        G --> J[Sparse/BM25]

        E --> K[Storage Backends]
        K --> L[In-Memory]
        K --> M[Memory-Mapped]
        K --> N[RocksDB]
    end

    subgraph "Blockchain Layer"
        O[consensus/]
        P[trie/]
        Q[determ/]
        R[zk/]
    end
```

### Storage Backend System

#### Backend Types

| Backend | Use Case | Trade-offs |
|---------|----------|------------|
| **In-Memory** | Low-latency, small datasets | Limited by RAM |
| **Memory-Mapped** | Large datasets, OS-managed caching | Slower than memory |
| **RocksDB** | Persistent, large scale (optional) | C++ dependency, memory overhead |
| **redb** | Pure Rust alternative (future) | Embedded, no C++ dependency |

> **Note**: RocksDB is an optional feature flag (`feature = "rocksdb"`). For pure Rust deployments, consider `redb` or `sled` as alternatives. The "Pure Rust" ethos applies to default configurations.

#### SQL Syntax

```sql
-- Specify storage backend per table
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY,
    content TEXT,
    embedding VECTOR(384)
) STORAGE = mmap;  -- Options: memory, mmap, rocksdb

-- Vector index with quantization
CREATE INDEX idx_emb ON embeddings(embedding)
USING HNSW WITH (
    metric = 'cosine',
    m = 32,
    ef_construction = 400,
    quantization = 'pq',    -- Options: none, sq, pq, bq
    compression = 8         -- Compression ratio
);

-- Hybrid search: vector + sparse
SELECT id, content,
    VEC_DISTANCE_COSINE(embedding, $query) as score,
    BM25_MATCH(description, $keywords) as bm25
FROM embeddings
WHERE category = 'ai'
ORDER BY score + bm25 * 0.3
LIMIT 10;
```

### Vector Engine Specifications

#### HNSW Index

| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `m` | 16 | 2-128 | Connections per node |
| `ef_construction` | 200 | 64-512 | Build-time search width |
| `ef_search` | 200 | 1-512 | Query-time search width |
| `metric` | cosine | l2, cosine, ip | Distance metric |

#### Quantization

| Type | Compression | Quality Loss | Use Case |
|------|-------------|--------------|----------|
| **SQ** (Scalar) | 4x | Low | General use |
| **PQ** (Product) | 4-64x | Medium | Large datasets |
| **BQ** (Binary) | 32x | High | Extreme compression |

#### Sparse Vectors

- BM25-style inverted index
- Combined with dense vectors for hybrid search
- Configurable term weighting

#### Payload Filtering

| Index Type | Use Case |
|------------|----------|
| `bool_index` | Boolean filters |
| `numeric_index` | Range queries |
| `geo_index` | Location filtering |
| `full_text_index` | Text match |
| `facet_index` | Categorical |
| `map_index` | Key-value |

### Blockchain Feature Preservation

The following modules remain unchanged:

| Module | Purpose | Integration |
|--------|---------|-------------|
| `consensus/` | Block/Operation types | Unchanged |
| `trie/` | RowTrie, SchemaTrie | Unchanged |
| `determ/` | Deterministic values | Unchanged |
| `zk/` | ZK proofs | Unchanged |

All blockchain features operate independently of storage backend selection.

### GPU Acceleration

```rust
#[cfg(feature = "gpu")]
pub mod gpu {
    // CUDA kernels for HNSW graph building
    // GPU-accelerated vector operations
    // Memory management for GPU vectors
}
```

- Feature-gated with `#[cfg(feature = "gpu")]`
- Fallback to CPU when GPU unavailable
- CUDA support only (OpenCL future)

### Search Algorithms

| Algorithm | Best For | Implementation |
|-----------|----------|----------------|
| **HNSW** | General ANNS | Default |
| **Acorn** | Memory-constrained | Optional |

### Determinism & Consensus

**Critical Challenge**: Blockchain consensus requires exact determinism. Vector search uses floating-point math which can be non-deterministic across architectures.

#### The Problem

- Floating-point rounding differs between x86 (AVX) and ARM (NEON)
- SIMD instructions may produce slightly different results
- Concurrent HNSW graph construction is non-deterministic

#### Solution: Snapshot-Based Verification

```mermaid
graph LR
    A[Transaction Commits] --> B[Snapshot Vector Index]
    B --> C[Generate Merkle Root]
    C --> D[Store in Blockchain]

    E[Query Request] --> F[Use Committed Snapshot]
    F --> G[Return Results + Proof]
```

**Approach**:
1. **Immutable Snapshots**: After commit, vector index becomes immutable
2. **Merkle Root**: Compute root hash of all vectors at commit time
3. **Stored State**: Store serialized vector data (not graph) for verification
4. **Software Float**: Use strict IEEE 754 for critical comparisons (optional feature)

> **Trade-off**: This means live HNSW graph searches cannot be directly verified. Instead, verified queries use a snapshot. Real-time verification requires async proof generation.

> ⚠️ **Implementation Warning**: Computing Merkle root at commit time for tables with millions of vectors will destroy write throughput.
>
> **Recommendation**: Use incremental hashing - only hash newly inserted/deleted vectors and update branches to root, rather than rehashing entire dataset. The existing `trie/RowTrie` should support this pattern.

#### Software Float Performance

> ⚠️ **Implementation Warning**: Software floating-point emulation (strict IEEE 754) is orders of magnitude slower than hardware SIMD.
>
> **Recommendation**: Isolate software emulation strictly to verification/snapshot phase. Live query nodes should use hardware acceleration (AVX/NEON) to maintain <50ms latency. Only enforce software float on nodes participating in block generation/validation.

### Performance SLAs

| Metric | Target | Measurement |
|--------|--------|-------------|
| Live query latency | <50ms | P50 at 1K QPS |
| Proof generation | <5s (95th percentile) | Async background |
| Merkle root update | <1s | Incremental at commit |
| Auto-compaction trigger | <25% tombstone threshold | Background scheduler |

### Benchmark Targets (Post-Implementation)

| Metric | Target | Notes |
|--------|--------|-------|
| Query latency | <50ms | vs 350ms multi-system baseline |
| Storage reduction | 60% | With BQ compression |
| Compression ratio | 4-64x | PQ/SQ/BQ configurations |
| Recall@10 | >95% | At 25% tombstone threshold |

### MVCC & Vector Index

**Critical Challenge**: HNSW is a connected graph. Concurrent transactions must see consistent vector visibility.

#### The Problem

- Transaction A inserts vector → updates HNSW graph
- Transaction B runs concurrently → should not see A's uncommitted vectors
- Graph traversals may include/exclude nodes incorrectly

#### Solution: Visibility-Aware Vector Layer

```mermaid
graph TB
    subgraph "Vector Storage"
        A[Vector Data Store] --> B[Transaction ID Map]
        A --> C[Tombstone Index]
        B --> D[Visibility Filter]
        C --> D
    end
```

**Approach**:
1. **Separate Index from Data**: HNSW index points to vector IDs, not direct data
2. **Transaction Metadata**: Each vector stores `created_txn_id` and `deleted_txn_id`
3. **Visibility Filter**: During search, filter by MVCC visibility rules
4. **Tombstoning**: Deleted vectors marked, not removed (until GC)

```rust
// Visibility check during vector search
fn is_visible(vector: &VectorEntry, txn: &Transaction) -> bool {
    // Created by this transaction?
    if vector.created_txn_id > txn.start_id {
        return false;
    }
    // Deleted by committed transaction?
    if let Some(deleted_by) = vector.deleted_txn_id {
        if deleted_by <= txn.commit_id && txn.is_committed(deleted_by) {
            return false;
        }
    }
    true
}
```

### Hybrid Query Optimization

**Challenge**: For queries like `WHERE reputation > 0.9 ORDER BY vector_distance`, which plan is optimal?

```sql
SELECT * FROM agents
WHERE reputation_score > 0.9
ORDER BY VEC_DISTANCE_COSINE(embedding, $query)
LIMIT 10;
```

#### Approach: Cost-Based Decision

The optimizer will estimate:

| Factor | Consideration |
|--------|---------------|
| **Selectivity** | How many rows pass `reputation > 0.9`? |
| **Index Selectivity** | HNSW ef_search value vs full scan |
| **Vector Dimension** | Brute-force cost scales with dimension |
| **Quantization** | Quantized search is faster but approximate |

**Plans**:
1. **Index-First**: Use HNSW, filter by reputation post-search (low selectivity)
2. **Filter-First**: Scan with reputation filter, brute-force vector (high selectivity)
3. **Index-Filtered**: Use HNSW with payload filter pre-search (Qdrant-style)

The optimizer will use statistics to pick the cheapest plan.

## Rationale

### Why Multiple Backends?

1. **Flexibility**: Different workloads have different requirements
2. **Optimization**: Per-table/backend choice enables tuning
3. **Migration Path**: Start with memory, migrate to mmap/rocksdb
4. **No Trade-offs**: Users choose what fits their use case

### Why Merge into Stoolap?

1. **Clean Foundation**: Stoolap's HNSW is well-structured, cache-optimized
2. **SQL Integration**: Already has query planner, optimizer, MVCC
3. **Blockchain Ready**: Already has trie, consensus, ZK modules
4. **Default Pure Rust**: No C++ dependencies by default (rocksdb is optional)

> **Correction**: The "Pure Rust" benefit applies to default builds. RocksDB is available as an optional feature for users who need its production-proven persistence.

### Alternative Approaches Considered

#### Option 1: New Codebase
- **Rejected**: Duplication of SQL/MVCC infrastructure
- **Trade-off**: More work, cleaner slate

#### Option 2: Fork Qdrant + Add SQL
- **Rejected**: Qdrant's Rust codebase less modular for SQL addition
- **Trade-off**: Would require significant refactoring

## Implementation

### Phases

```
Phase 1: Storage Backend Abstraction
├── Define StorageBackend enum
├── Implement InMemory backend (current)
├── Implement MmapBackend (from Qdrant)
└── Implement RocksDBBackend (from Qdrant)

Phase 2: Quantization (from Qdrant)
├── Copy lib/quantization to src/storage/quantization
├── Integrate with HNSW index
└── Add SQL syntax for quantization config

Phase 3: Sparse Vectors / BM25
├── Copy lib/sparse to src/storage/sparse
├── Add SPARSE index type
└── Add BM25_MATCH SQL function

Phase 4: Payload Indexes
├── Add field index modules from Qdrant
├── Integrate with query planner
└── Add filter syntax

Phase 5: GPU Support
├── Add GPU feature flag
├── Port CUDA kernels (future)
└── Add runtime GPU detection
```

### Key Files to Modify

| File | Change |
|------|--------|
| `src/storage/mod.rs` | Add backend abstraction |
| `src/storage/index/hnsw.rs` | Add quantization, algorithms |
| `src/storage/index/mod.rs` | Add sparse, field indexes |
| `src/parser/` | Add STORAGE, QUANTIZATION syntax |
| `src/executor/` | Add vector/sparse operators |
| `Cargo.toml` | Add quantization, sparse deps |

### Vector Update/Delete Semantics

#### UPDATE Vector

```sql
UPDATE embeddings SET embedding = $new_vec WHERE id = 5;
```

**Behavior**:
1. Old vector marked with `deleted_txn_id` (tombstone)
2. New vector inserted with `created_txn_id`
3. HNSW graph updated asynchronously (background merge)

#### DELETE Vector

```sql
DELETE FROM embeddings WHERE id = 5;
```

**Behavior**:
1. Vector marked with `deleted_txn_id`
2. Tombstoned until garbage collection
3. HNSW index entry marked deleted (not removed from graph)

#### Background Compaction

```sql
-- Trigger manual compaction
ALTER TABLE embeddings COMPACT;
```

Compaction rebuilds the HNSW graph without tombstones. Recommended after bulk deletes.

> ⚠️ **Implementation Warning**: HNSW search performance degrades rapidly with too many tombstones. Relying on users manually running `COMPACT` will cause slowdowns.
>
> **Recommendation**: Implement auto-vacuum/auto-compact threshold in background (e.g., trigger when tombstone count > 20% of total nodes).
>
> **Strengthening**: Make auto-compaction **mandatory**, not optional. Add configurable per-table threshold (similar to PostgreSQL autovacuum). Log warnings when compaction is delayed.

#### Testing Requirements

The following test matrix must be implemented:

| Test Category | Scenario | Acceptance Criteria |
|---------------|----------|---------------------|
| **MVCC + Vector** | High-concurrency UPDATE/DELETE | No consistency violations |
| **Determinism** | x86 vs ARM execution | Identical results |
| **Merkle Root** | 1M → 100M vectors | <1s incremental update |
| **Tombstone Degradation** | recall@10 vs % deleted | <5% recall loss at 25% deleted |

> **Segment-Merge Philosophy**: Borrow from Qdrant - use immutable segments + background merge. Old segments remain searchable while new ones are built.

### License Compliance

When porting code from Qdrant (Apache 2.0 license):

1. **Copyright Headers**: Maintain all original Apache 2.0 headers
2. **NOTICE File**: Add to repository noting Apache 2.0 code used
3. **Module Attribution**: Add doc comments crediting original Qdrant authors
4. **No Proprietary Changes**: Apache 2.0 allows modification, but changes must be documented

```rust
// Example attribution header
// Originally adapted from Qdrant (Apache 2.0)
// https://github.com/qdrant/qdrant
// Copyright 2024 Qdrant Contributors
```

### Testing Strategy

1. **Unit Tests**: Each component independently
2. **Integration Tests**: SQL + Vector queries
3. **Benchmark Tests**: Performance vs Qdrant, vs standalone Stoolap
4. **Blockchain Tests**: Verify trie/ZK integration unchanged

## Related Use Cases

- [Decentralized Mission Execution](../../docs/use-cases/decentralized-mission-execution.md)
- [Autonomous Agent Marketplace](../../docs/use-cases/agent-marketplace.md)

## Related Research

- [Qdrant Research Report](../../docs/research/qdrant-research.md)
- [Stoolap Research Report](../../docs/research/stoolap-research.md)
