# Mission: Phase 5 - Hybrid Query Planner

## Status
Open

## RFC
RFC-0103: Unified Vector-SQL Storage Engine

## Blockers / Dependencies

- **Blocked by:** Mission: Phase 1 - Core Engine MVP (must complete first)

## Acceptance Criteria

- [ ] Implement cost-based planning (index-first vs filter-first)
- [ ] Implement statistics collection (selectivity, histograms)
- [ ] Integrate payload filters with vector search
- [ ] Pass test: Hybrid queries with various selectivities

## Description

Add intelligent query planning for hybrid SQL + vector queries.

## Technical Details

### Query Planning Strategies

```
Index-First:
1. Run vector search
2. Apply SQL filters to results
→ Best when vector selectivity is high

Filter-First:
1. Apply SQL filters
2. Run vector search on filtered subset
→ Best when filter selectivity is low
```

### Statistics Collection

```
Required Statistics:
├── vector_norm_histogram: Distribution of vector magnitudes
├── payload_histograms: Value distributions per column
├── segment_sizes: Vectors per segment
├── index_density: HNSW connectivity density
└── tombstone_ratio: Deleted/total vectors

Collection Strategy:
├── 1% random sample per segment (min 1000 vectors)
├── Update triggers: bulk inserts >10K, ANALYZE command, every 5 min
├── Staleness policy: Mark stale if >20% change
└── K-means (k=10) for clustered embeddings
```

### Selectivity Estimation

```
Challenge: Embedding distributions are often non-uniform
Solution: K-means to detect semantic clusters
```

## Implementation Notes

1. **P1 Gate**: Statistics collection must be specified before Phase 5
2. **Clustered Data**: Standard histograms may be unreliable - use k-means
3. **Plan Selection**: Choose index-first vs filter-first based on estimated selectivity

## Research References

- [RFC-0103: Unified Vector-SQL Storage Engine](../../rfcs/0103-unified-vector-sql-storage.md)

## Claimant

<!-- Add your name when claiming -->

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** RFC-0103 Phase 5
