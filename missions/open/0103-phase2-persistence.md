# Mission: Phase 2 - Persistence

## Status
Claimed

## RFC
RFC-0103: Unified Vector-SQL Storage Engine

## Blockers / Dependencies

- **Blocked by:** Mission: Phase 1 - Core Engine MVP (must complete first) ✅ COMPLETE

## Acceptance Criteria

- [ ] Implement memory-mapped storage backend
- [ ] Add RocksDB backend (optional, feature flag)
- [ ] Integrate WAL for vector operations
- [ ] Implement crash recovery from WAL
- [ ] Add snapshot shipping for fast recovery
- [ ] MTTR: <5 minutes for typical workloads (<1M vectors)

## Claimant

@claude-code

## Description

Add persistence to the vector engine, enabling crash recovery and durable storage.

## Technical Details

### Persistence Architecture

```
Storage Backends:
├── In-Memory (Phase 1)
├── Memory-Mapped (Phase 2)
└── RocksDB (optional)

Recovery:
├── Load last snapshot
├── Replay WAL entries
└── Rebuild dirty segments
```

### WAL Integration

Vector operations must be WAL-logged:
- VectorInsert, VectorDelete, VectorUpdate
- SegmentCreate, SegmentMerge
- IndexBuild, CompactionStart/Finish
- SnapshotCommit

### Snapshot Shipping

```
Recovery Options:
├── Full WAL replay (slow for large datasets)
└── Snapshot + delta WAL (fast recovery)

Trigger: Every 100K vectors or 5 minutes
```

## Implementation Notes

1. **MTTR SLA**: Target <5min recovery for <1M vectors
2. **WAL Rotation**: Aggressive 64MB rotation for vectors (not 256MB like SQL)
3. **Quantization for WAL**: Apply BQ before WAL to reduce bloat (768-dim → 96 bytes)

## Research References

- [RFC-0103: Unified Vector-SQL Storage Engine](../../rfcs/0103-unified-vector-sql-storage.md)

## Claimant

<!-- Add your name when claiming -->

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0103 Phase 2
