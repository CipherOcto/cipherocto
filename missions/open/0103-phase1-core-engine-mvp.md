# Mission: Phase 1 - Core Engine MVP

## Status
Claimed

## RFC
RFC-0103: Unified Vector-SQL Storage Engine

## Implementation Location

**Worktree**: `/home/mmacedoeu/_w/ai/cipherocto-vector-impl`
**Branch**: `vector-phase1`
**PR**: https://github.com/CipherOcto/cipherocto/pull/new/vector-phase1

## Claimant

@claude-code

## Acceptance Criteria

- [ ] Implement MVCC + Segment architecture for vectors
- [ ] Implement three-layer verification (HNSW search, software float re-rank, Merkle proof)
- [ ] Add vector ID + content hash for Merkle tree
- [ ] Add basic statistics collection (row counts, null counts)
- [ ] Implement in-memory storage backend
- [ ] Complete WAL enum: IndexBuild, CompactionStart, CompactionFinish, SnapshotCommit
- [ ] Pass test: MVCC + concurrent vector UPDATE/DELETE
- [ ] Performance: <50ms query latency for simple queries

## Description

Build the core vector-SQL unified engine MVP. This is the foundation all other phases depend on.

## Technical Details

### Core Components

```
Segment Architecture:
├── Immutable segments (append-only)
├── Version tracking per segment
├── Segment merge with tombstone tracking
└── MVCC visibility at segment level

Three-Layer Verification:
├── Layer 1: HNSW fast search (AVX/NEON)
├── Layer 2: Software float re-rank (top-K candidates)
└── Layer 3: Merkle proof generation

Merkle Structure:
├── blake3(vector_id || blake3(embedding))
├── Hierarchical: root → segment roots → vector hashes
└── Incremental updates on commit
```

### Key Files (from RFC)

- `src/storage/vector/segment.rs` - Segment management
- `src/storage/vector/mvcc.rs` - MVCC visibility
- `src/storage/vector/merkle.rs` - Merkle tree
- `src/storage/vector/hnsw.rs` - HNSW index
- `src/storage/vector/wal.rs` - WAL with enum entries

### Testing Requirements

1. **MVCC + Concurrent UPDATE/DELETE**: Verify no data corruption under concurrent writes
2. **Segment Visibility**: Verify transaction isolation works correctly
3. **Merkle Proof**: Verify proof generation and verification

## Implementation Notes

1. **P0 Blocker**: WAL enum must be complete before any other work
2. **Prototype First**: Build highest-risk pieces (Merkle at scale, MVCC + concurrent ops) before implementation
3. **Memory Alignment**: Use aligned-vec crate for SIMD (32-byte AVX2, 64-byte AVX-512)

## Research References

- [RFC-0103: Unified Vector-SQL Storage Engine](../../rfcs/0103-unified-vector-sql-storage.md)
- [Qdrant Research Report](../../docs/research/qdrant-research.md)
- [Stoolap Research Report](../../docs/research/stoolap-research.md)

## Claimant

@claude-code

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Implementation
**Priority:** Critical (Foundation for all other phases)
**Phase:** RFC-0103 Phase 1
