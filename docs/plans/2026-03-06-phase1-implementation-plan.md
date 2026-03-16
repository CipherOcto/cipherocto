# Implementation Plan: Phase 1 - Core Engine MVP

**Date**: March 2026
**Mission**: RFC-0200 (Storage) Phase 1 - Core Engine MVP
**Location**: `/home/mmacedoeu/_w/ai/cipherocto-vector-impl`
**Base**: Stoolap fork at `/home/mmacedoeu/_w/databases/stoolap`

---

## Overview

This plan details the implementation of Phase 1 (Core Engine MVP) of RFC-0200 (Storage): Unified Vector-SQL Storage Engine (archived/superseded). The goal is to build the foundational infrastructure for vector storage with MVCC, segment architecture, and Merkle verification.

**Stoolap already has:**

- ✅ Vector data type
- ✅ HNSW index
- ✅ Vector distance functions (L2, cosine, inner product)
- ✅ MVCC engine with WAL
- ✅ Transaction management

**What's new:**

- Vector segment architecture
- Segment-level MVCC visibility
- Merkle tree integration
- Extended WAL for vectors

---

## Phase 1a: Core Infrastructure

### 1.1 Create Vector Module

**Location**: `src/storage/vector/`

**Files to create**:

```
src/storage/vector/
├── mod.rs          # Module exports
├── segment.rs      # VectorSegment implementation
├── types.rs        # Vector-specific types
└── config.rs      # Vector storage configuration
```

**Tasks**:

- [ ] Create `src/storage/vector/mod.rs` with module exports
- [ ] Define `VectorSegment` struct with SoA layout
- [ ] Add segment configuration (max size: 100K vectors)
- [ ] Implement segment creation and basic operations

### 1.2 Struct of Arrays Layout

```rust
// src/storage/vector/segment.rs

/// Immutable vector segment with SoA layout for SIMD
pub struct VectorSegment {
    pub id: u64,
    pub vector_ids: Vec<i64>,        // Array of vector IDs
    pub embeddings: Vec<f32>,         // SoA: all dimensions contiguous
    pub deleted: Vec<bool>,           // Tombstone flags
    pub dimensions: usize,
    pub capacity: usize,
    pub count: usize,
    // Metadata
    pub created_txn: u64,
    pub is_immutable: bool,
}

impl VectorSegment {
    /// Create new segment
    pub fn new(id: u64, dimensions: usize, capacity: usize) -> Self {
        Self {
            id,
            vector_ids: Vec::with_capacity(capacity),
            // SoA layout: dimensions * capacity floats
            embeddings: vec![0.0; dimensions * capacity],
            deleted: Vec::with_capacity(capacity),
            dimensions,
            capacity,
            count: 0,
            created_txn: 0,
            is_immutable: false,
        }
    }

    /// Add vector to segment
    pub fn push(&mut self, vector_id: i64, embedding: &[f32]) -> Result<()> {
        if self.count >= self.capacity {
            return Err(Error::SegmentFull);
        }
        if embedding.len() != self.dimensions {
            return Err(Error::InvalidDimension);
        }

        let idx = self.count;
        self.vector_ids.push(vector_id);
        // SoA: copy embedding to correct offset
        self.embeddings[idx * self.dimensions..(idx + 1) * self.dimensions]
            .copy_from_slice(embedding);
        self.deleted.push(false);
        self.count += 1;
        Ok(())
    }

    /// Get embedding by index (zero-copy)
    pub fn get_embedding(&self, idx: usize) -> Option<&[f32]> {
        if idx >= self.count { return None; }
        Some(&self.embeddings[idx * self.dimensions..(idx + 1) * self.dimensions])
    }
}
```

**Memory alignment** (for SIMD):

```rust
use std::alloc::{alloc, Layout};

const fn aligned_layout(size: usize, align: usize) -> Layout {
    Layout::from_size_align(size, align).unwrap()
}

// AVX-512: 64-byte alignment
let layout = aligned_layout(dimensions * capacity * mem::size_of::<f32>(), 64);
let ptr = unsafe { alloc(layout) };
```

---

## Phase 1b: MVCC + Visibility

### 2.1 Vector MVCC

**Location**: `src/storage/vector/mvcc.rs`

**Files to create**:

```
src/storage/vector/
├── mvcc.rs         # Vector MVCC implementation
├── visibility.rs   # Visibility rules
└── version.rs      # Version tracking
```

### 2.2 Segment State Machine

```rust
// src/storage/vector/mvcc.rs

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Vector MVCC with segment-level visibility
pub struct VectorMvcc {
    segments: RwLock<HashMap<u64, SegmentState>>,
    active_segment_id: RwLock<Option<u64>>,
    version_tracker: RwLock<VersionTracker>,
    config: VectorConfig,
}

enum SegmentState {
    /// Active segment - new vectors go here
    Active(Arc<VectorSegment>),
    /// Immutable - read-only, can be searched
    Immutable(Arc<VectorSegment>),
    /// Being merged - exclude from queries
    Merging(Vec<u64>),
}

pub struct VersionTracker {
    /// vector_id -> (segment_id, version)
    locations: HashMap<i64, (u64, u64)>,
    next_segment_id: u64,
}

impl VectorMvcc {
    pub fn new(config: VectorConfig) -> Self {
        let mut tracker = VersionTracker {
            locations: HashMap::new(),
            next_segment_id: 1,
        };

        // Create first active segment
        let segment = Arc::new(VectorSegment::new(1, config.dimensions, config.segment_capacity));
        let mut segments = HashMap::new();
        segments.insert(1, SegmentState::Active(segment));

        Self {
            segments: RwLock::new(segments),
            active_segment_id: RwLock::new(Some(1)),
            version_tracker: RwLock::new(tracker),
            config,
        }
    }

    /// Insert vector - always to active segment
    pub fn insert(&self, vector_id: i64, embedding: Vec<f32>) -> Result<()> {
        let active_id = *self.active_segment_id.read();

        if let Some(seg_id) = active_id {
            let mut segments = self.segments.write();
            if let SegmentState::Active(segment) = segments.get_mut(&seg_id).unwrap() {
                segment.push(vector_id, &embedding)?;

                // Update version tracker
                self.version_tracker.write()
                    .locations.insert(vector_id, (seg_id, 1));

                // Check if segment is full, create new one
                if segment.count >= segment.capacity {
                    self.make_immutable(seg_id);
                    self.create_new_active_segment();
                }
                return Ok(());
            }
        }

        Err(Error::NoActiveSegment)
    }

    /// Make segment immutable (called when full)
    fn make_immutable(&self, segment_id: u64) {
        let mut segments = self.segments.write();
        if let Some(state) = segments.get_mut(&segment_id) {
            if let SegmentState::Active(segment) = state {
                segment.is_immutable = true;
                *state = SegmentState::Immutable(segment.clone());
            }
        }
        *self.active_segment_id.write() = None;
    }

    /// Create new active segment
    fn create_new_active_segment(&self) {
        let new_id = self.version_tracker.write().next_segment_id;
        self.version_tracker.write().next_segment_id += 1;

        let segment = Arc::new(VectorSegment::new(
            new_id,
            self.config.dimensions,
            self.config.segment_capacity,
        ));

        self.segments.write().insert(new_id, SegmentState::Active(segment));
        *self.active_segment_id.write() = Some(new_id);
    }

    /// Get all visible segments for a transaction
    pub fn visible_segments(&self, _txn_id: u64) -> Vec<Arc<VectorSegment>> {
        let segments = self.segments.read();
        segments
            .values()
            .filter_map(|state| match state {
                SegmentState::Active(s) | SegmentState::Immutable(s) => Some(s.clone()),
                SegmentState::Merging(_) => None,
            })
            .collect()
    }
}
```

### 2.3 Update Semantics

```rust
impl VectorMvcc {
    /// Update vector - in-place if in active segment, else soft delete + insert
    pub fn update(&self, vector_id: i64, new_embedding: Vec<f32>) -> Result<()> {
        let active_id = *self.active_segment_id.read();
        let mut segments = self.segments.write();
        let mut tracker = self.version_tracker.write();

        if let Some(seg_id) = active_id {
            if let Some((curr_seg, curr_ver)) = tracker.locations.get(&vector_id) {
                if *curr_seg == seg_id {
                    // In-place update in active segment
                    if let SegmentState::Active(segment) = segments.get_mut(&seg_id).unwrap() {
                        // Find and update vector
                        for (i, &id) in segment.vector_ids.iter().enumerate() {
                            if id == vector_id {
                                let offset = i * segment.dimensions;
                                segment.embeddings[offset..offset + segment.dimensions]
                                    .copy_from_slice(&new_embedding);
                                // Increment version
                                tracker.locations.insert(vector_id, (seg_id, curr_ver + 1));
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        // Not in active segment - soft delete + insert
        // (would need to implement soft delete marking)
        drop(tracker);
        drop(segments);

        // Soft delete old, insert new
        self.soft_delete(vector_id)?;
        self.insert(vector_id, new_embedding)
    }

    /// Soft delete vector
    fn soft_delete(&self, vector_id: i64) -> Result<()> {
        let tracker = self.version_tracker.read();
        if let Some((seg_id, _)) = tracker.locations.get(&vector_id) {
            let segments = self.segments.read();
            if let SegmentState::Active(segment) = segments.get(seg_id).unwrap() {
                for (i, &id) in segment.vector_ids.iter().enumerate() {
                    if id == vector_id {
                        // Mark as deleted (would need mutable access)
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}
```

---

## Phase 1c: Merkle Integration

### 3.1 Merkle Tree

**Location**: `src/storage/vector/merkle.rs`

```rust
// src/storage/vector/merkle.rs

use blake3::{Hasher, Hash};
use std::collections::HashMap;

/// Vector Merkle tree for blockchain verification
pub struct VectorMerkle {
    segment_roots: HashMap<u64, Hash>,
    global_root: Hash,
}

impl VectorMerkle {
    pub fn new() -> Self {
        Self {
            segment_roots: HashMap::new(),
            global_root: *b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        }
    }

    /// Compute leaf hash: blake3(vector_id || blake3(embedding))
    pub fn leaf_hash(vector_id: i64, embedding: &[f32]) -> Hash {
        let embedding_hash = blake3::hash(embedding.as_bytes());
        let mut hasher = Hasher::new();
        hasher.update(&vector_id.to_le_bytes());
        hasher.update(embedding_hash.as_bytes());
        *hasher.finalize()
    }

    /// Build segment Merkle root from vectors
    pub fn segment_root(vectors: &[(i64, &[f32])]) -> Hash {
        if vectors.is_empty() {
            return *b"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        }

        // Create leaves
        let mut leaves: Vec<Hash> = vectors
            .iter()
            .map(|(id, emb)| Self::leaf_hash(*id, emb))
            .collect();

        // Build tree bottom-up
        while leaves.len() > 1 {
            if leaves.len() % 2 != 0 {
                leaves.push(leaves.last().unwrap().clone());
            }

            leaves = leaves
                .chunks(2)
                .map(|chunk| {
                    let mut hasher = Hasher::new();
                    hasher.update(&chunk[0]);
                    hasher.update(&chunk[1]);
                    *hasher.finalize()
                })
                .collect();
        }

        leaves[0]
    }

    /// Update segment root
    pub fn update_segment(&mut self, segment_id: u64, vectors: &[(i64, &[f32])]) {
        let root = Self::segment_root(vectors);
        self.segment_roots.insert(segment_id, root);
        self.recompute_global_root();
    }

    /// Recompute global root from segment roots
    fn recompute_global_root(&mut self) {
        let mut roots: Vec<Hash> = self.segment_roots
            .iter()
            .sorted_by_key(|(id, _)| *id)
            .map(|(_, root)| *root)
            .collect();

        if roots.is_empty() {
            self.global_root = *b"CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";
            return;
        }

        while roots.len() > 1 {
            if roots.len() % 2 != 0 {
                roots.push(roots.last().unwrap().clone());
            }
            roots = roots
                .chunks(2)
                .map(|chunk| {
                    let mut hasher = Hasher::new();
                    hasher.update(&chunk[0]);
                    hasher.update(&chunk[1]);
                    *hasher.finalize()
                })
                .collect();
        }

        self.global_root = roots[0];
    }

    /// Generate proof for vector
    pub fn generate_proof(&self, segment_id: u64, vector_id: i64, embedding: &[f32]) -> MerkleProof {
        let leaf = Self::leaf_hash(vector_id, embedding);
        let segment_root = self.segment_roots.get(&segment_id);

        MerkleProof {
            leaf,
            segment_root: segment_root.copied(),
            global_root: self.global_root,
            path: vec![],  // Would build full path
        }
    }
}

#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub leaf: Hash,
    pub segment_root: Option<Hash>,
    pub global_root: Hash,
    pub path: Vec<Hash>,
}
```

---

## Phase 1d: WAL Integration

### 4.1 Extend Existing WAL

**Location**: Modify `src/storage/mvcc/wal_manager.rs`

Add vector-specific WAL operations:

```rust
// Extend existing WalOperationType enum

pub enum WalOperationType {
    // ... existing variants

    // Vector operations
    VectorInsert {
        segment_id: u64,
        vector_id: i64,
        embedding: Vec<f32>,
    },
    VectorDelete {
        segment_id: u64,
        vector_id: i64,
    },
    VectorUpdate {
        segment_id: u64,
        vector_id: i64,
        old_embedding: Vec<f32>,
        new_embedding: Vec<f32>,
    },
    SegmentCreate {
        segment_id: u64,
    },
    SegmentMerge {
        old_segments: Vec<u64>,
        new_segment: u64,
    },
    // P0: Required for crash recovery
    IndexBuild {
        segment_id: u64,
    },
    CompactionStart {
        compaction_id: u64,
        source_segments: Vec<u64>,
    },
    CompactionFinish {
        compaction_id: u64,
        new_segment_id: u64,
        deleted_vector_ids: Vec<i64>,
    },
    SnapshotCommit {
        snapshot_id: u64,
        merkle_root: [u8; 32],
        block_height: u64,
    },
}
```

### 4.2 Recovery

```rust
impl VectorMvcc {
    /// Recover from WAL
    pub fn recover(wal: &VectorWal, config: VectorConfig) -> Result<Self> {
        let mvcc = Self::new(config);

        for entry in wal.entries() {
            match entry {
                VectorWalOp::VectorInsert { vector_id, embedding, .. } => {
                    mvcc.insert(*vector_id, embedding.clone())?;
                }
                VectorWalOp::VectorDelete { vector_id, .. } => {
                    mvcc.soft_delete(*vector_id)?;
                }
                VectorWalOp::VectorUpdate { vector_id, new_embedding, .. } => {
                    mvcc.update(*vector_id, new_embedding.clone())?;
                }
                // ... handle other ops
                _ => {}
            }
        }

        Ok(mvcc)
    }
}
```

---

## Phase 1e: SQL Integration

### 5.1 Vector Column in Tables

Stoolap already supports `Vector` type. Need to ensure it integrates with segment storage.

```rust
// src/storage/mvcc/table.rs - integrate vector columns

impl MVCCTable {
    /// Insert row with vector column
    pub fn insert_vector(&self, row: &Row, vector_col: &str, embedding: Vec<f32>) -> Result<()> {
        // Store in vector MVCC
        let vector_id = row.get_primary_key()?;
        self.vector_mvcc.insert(vector_id, embedding)?;

        // Store other columns in regular MVCC
        self.insert(row)
    }
}
```

---

## Testing Strategy

### Unit Tests

- [ ] VectorSegment: push, get, SoA layout
- [ ] VectorMvcc: insert, update, visibility
- [ ] VectorMerkle: leaf hash, segment root, global root
- [ ] WAL: serialize/deserialize

### Integration Tests

- [ ] SQL: CREATE TABLE with VECTOR, INSERT, SELECT
- [ ] Concurrent: multiple threads doing INSERT/UPDATE
- [ ] Recovery: crash and recover from WAL

### Performance Tests

- [ ] Latency: <50ms for simple queries
- [ ] Throughput: X vectors/second insert
- [ ] Memory: segment memory usage

---

## Acceptance Criteria Checklist

- [ ] Implement MVCC + Segment architecture for vectors
- [ ] Implement three-layer verification (HNSW search, software float re-rank, Merkle proof)
- [ ] Add vector ID + content hash for Merkle tree
- [ ] Add basic statistics collection (row counts, null counts)
- [ ] Implement in-memory storage backend
- [ ] Complete WAL enum: IndexBuild, CompactionStart/Finish, SnapshotCommit
- [ ] Pass test: MVCC + concurrent vector UPDATE/DELETE
- [ ] Performance: <50ms query latency for simple queries

---

## Dependencies

| Component        | Status   | Notes                                         |
| ---------------- | -------- | --------------------------------------------- |
| Stoolap MVCC     | ✅ Ready | Existing in `src/storage/mvcc/`               |
| Stoolap WAL      | ✅ Ready | Existing in `src/storage/mvcc/wal_manager.rs` |
| HNSW Index       | ✅ Ready | Existing in `src/storage/index/hnsw.rs`       |
| Vector Functions | ✅ Ready | Existing in `src/functions/scalar/vector.rs`  |
| blake3 crate     | 🔲 Add   | Add to `Cargo.toml`                           |

---

## File Changes Summary

### New Files

```
src/storage/vector/
├── mod.rs          # 50 lines
├── segment.rs      # 150 lines
├── mvcc.rs         # 200 lines
├── merkle.rs       # 150 lines
└── config.rs       # 50 lines
```

### Modified Files

```
src/storage/mod.rs         # Add vector module
src/storage/mvcc/wal_manager.rs  # Add vector WAL ops
Cargo.toml                 # Add blake3 dependency
```

---

## Timeline

| Week | Focus                  | Deliverable            |
| ---- | ---------------------- | ---------------------- |
| 1    | Module setup + Segment | Vector module with SoA |
| 2    | MVCC + Visibility      | Segment-level MVCC     |
| 3    | Merkle                 | Proof generation       |
| 4    | WAL + Recovery         | Crash recovery         |
| 5    | SQL Integration        | Full table ops         |
| 6    | Testing + Polish       | All tests pass         |

---

**Plan Created**: March 2026
**Last Updated**: March 2026
