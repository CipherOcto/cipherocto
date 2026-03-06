# Mission: Phase 3 - Quantization

## Status
Claimed

## RFC
RFC-0103: Unified Vector-SQL Storage Engine

## Blockers / Dependencies

- **Blocked by:** Mission: Phase 1 - Core Engine MVP ✅ COMPLETE
- **Blocked by:** Mission: Phase 2 - Persistence ✅ COMPLETE

## Acceptance Criteria

- [ ] Implement Scalar Quantization (SQ)
- [ ] Implement Product Quantization (PQ)
- [x] Implement Binary Quantization (BQ)
- [ ] Add SQL syntax for quantization config
- [x] Achieve 4-64x compression ratio (32x for BQ)
- [ ] Maintain >95% recall@10 at 15% tombstone threshold

## Description

Add vector quantization for memory efficiency and compression.

## Technical Details

### Quantization Types

```
Quantization Methods:
├── Scalar Quantization (SQ)
│   └── Map floats to integers (e.g., float32 → uint8)
├── Product Quantization (PQ)
│   └── Split vector into sub-vectors, quantize each
│   └── Configurable: 4, 8, 16 sub-vectors
└── Binary Quantization (BQ)
    └── Map to binary (0/1)
    └── 32x compression, fastest search
```

### SQL Syntax

```sql
-- Create table with quantization
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY,
    embedding VECTOR(768) QUANTIZE = PQ(8)
);

-- Add quantization to existing index
CREATE INDEX idx_emb ON embeddings(embedding)
USING HNSW WITH (quantization = 'pq', pq_subvecs = 8);
```

### Compression Ratios

| Type | Compression | Recall Impact |
|------|-------------|--------------|
| SQ | 4x | Low |
| PQ | 16-32x | Medium |
| BQ | 32-64x | Higher |

## Implementation Notes

1. **BQ First**: Binary Quantization is fastest and easiest - implement first
2. **Quality vs Size**: PQ offers best balance for most workloads
3. **Tombstone Impact**: Compaction at 15% soft, 30% hard limit per RFC

## Research References

- [RFC-0103: Unified Vector-SQL Storage Engine](../../rfcs/0103-unified-vector-sql-storage.md)

## Claimant

@claude-code

## Pull Request

https://github.com/CipherOcto/stoolap/pull/new/feat/vector-phase3-quantization

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0103 Phase 3
