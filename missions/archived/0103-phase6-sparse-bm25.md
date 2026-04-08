# Mission: Phase 6 - Sparse Vectors / BM25

## Status
Archived

## RFC
RFC-0103: Unified Vector-SQL Storage Engine

## Blockers / Dependencies

- **Blocked by:** Mission: Phase 1 - Core Engine MVP (must complete first)

## Acceptance Criteria

- [ ] Copy lib/sparse to src/storage/sparse
- [ ] Add SPARSE index type
- [ ] Add BM25_MATCH SQL function
- [ ] Support hybrid search (dense + sparse + SQL filters)

## Description

Add sparse vector support and BM25 hybrid search.

## Technical Details

### Sparse Vectors

```
Sparse vs Dense:
├── Dense: Most values are non-zero (e.g., [0.1, 0.5, 0.8, ...])
└── Sparse: Most values are zero (e.g., {"term1": 0.5, "term100": 0.3})

Storage: Only store non-zero values
Use cases: Text search, keyword matching
```

### BM25

```
BM25 (Best Matching 25):
├── Ranking function for text search
├── Used by Elasticsearch, OpenSearch
└── Formula: score = sum IDF(qi) * (f(qi) * (k1 + 1)) / (f(qi) + k1 * (1 - b + b * dl/avdl))

Integration:
├── BM25_MATCH(table, column, query) -> score
└── Combine with vector search via UNION or JOIN
```

### SQL Syntax

```sql
-- Create sparse index
CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    content TEXT,
    sparse_embedding SPARSE VECTOR
);

CREATE INDEX idx_doc ON documents(sparse_embedding) USING SPARSE;

-- BM25 search
SELECT id, content, BM25_MATCH(documents, content, 'search query') as score
FROM documents
ORDER BY score DESC
LIMIT 10;

-- Hybrid: BM25 + vector
SELECT * FROM (
    SELECT id, content, score FROM (
        SELECT id, content, BM25_MATCH(documents, content, 'AI') as score
        FROM documents WHERE category = 'tech'
    ) bm25
    UNION ALL
    SELECT id, content, VEC_DISTANCE_COSINE(embedding, $query) as score
    FROM documents WHERE category = 'tech'
) combined
ORDER BY score DESC
LIMIT 10;
```

## Implementation Notes

1. **lib/sparse**: Copy from existing library or implement new
2. **Hybrid Queries**: Combine with Phase 5 query planner
3. **Ranking Fusion**: BM25 + vector score combination strategies

## Research References

- [RFC-0103: Unified Vector-SQL Storage Engine](../../rfcs/0103-unified-vector-sql-storage.md)

## Claimant

<!-- Add your name when claiming -->

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** RFC-0103 Phase 6
