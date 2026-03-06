# Phase 2: Persistence Implementation Plan

## Overview

Implement memory-mapped vector storage for persistent vector data, following Qdrant's approach.

## Files to Create/Modify

### New Files

- `src/storage/vector/mmap.rs` - Memory-mapped segment storage
- `src/storage/vector/backend.rs` - Storage backend trait

### Modified Files

- `src/storage/vector/segment.rs` - Add flush_to_disk()
- `src/storage/vector/mvcc.rs` - Add persistent segment handling
- `src/storage/vector/mod.rs` - Export new modules
- `Cargo.toml` - Add memmap2 dependency

---

## Step 1: Add Dependencies

### Cargo.toml

```toml
[dependencies]
memmap2 = { version = "0.9", optional = true }

[features]
vector = ["dep:memmap2", "dep:blake3"]
```

---

## Step 2: Create mmap.rs

### File Format

```
{segment_id}/
├── vectors.bin    # SoA layout: [dim0_all, dim1_all, ...]
├── deleted.bin    # Bit-packed tombstones
├── metadata.json  # {dimension, count, created_tx, version}
└── merkle_root   # Merkle root hash (32 bytes)
```

### Key Structures

```rust
use memmap2::{Mmap, MmapMut};
use std::fs::{File, OpenOptions};
use std::path::Path;

/// Memory-mapped vector segment (immutable, for search)
pub struct MmapVectorSegment {
    pub id: u64,
    vectors: Mmap,
    deleted: Vec<u8>,  // Bit-packed
    dimension: usize,
    count: usize,
}

/// Mutable version for writes
pub struct MmapVectorSegmentMut {
    vectors: MmapMut,
    deleted: Vec<u8>,
    dimension: usize,
    capacity: usize,
    count: usize,
}

impl MmapVectorSegmentMut {
    /// Create new mutable segment in memory
    pub fn new(dimension: usize, capacity: usize) -> Self;

    /// Append vector
    pub fn push(&mut self, vector_id: i64, embedding: &[f32]) -> Result<usize>;

    /// Flush to disk
    pub fn flush_to_disk(&self, path: &Path) -> Result<()>;
}

impl MmapVectorSegment {
    /// Load from disk
    pub fn load_from(path: &Path) -> Result<Self>;

    /// Get embedding by index
    pub fn get_embedding(&self, idx: usize) -> Option<&[f32]>;

    /// Check if deleted
    pub fn is_deleted(&self, idx: usize) -> bool;
}
```

---

## Step 3: Create backend.rs

### Storage Backend Trait

```rust
/// Storage backend for vector segments
pub trait VectorStorage: Send + Sync {
    /// Create new segment
    fn create_segment(&self, dimension: usize) -> Result<u64>;

    /// Get segment
    fn get_segment(&self, id: u64) -> Result<Option<Arc<VectorSegment>>>;

    /// Delete segment
    fn delete_segment(&self, id: u64) -> Result<()>;

    /// List segments
    fn list_segments(&self) -> Result<Vec<u64>>;
}

/// In-memory backend (Phase 1)
pub struct InMemoryVectorStorage { /* ... */ }

/// Memory-mapped backend (Phase 2)
pub struct MmapVectorStorage {
    base_path: PathBuf,
    // ...
}
```

---

## Step 4: Modify segment.rs

### Add Flush Method

```rust
impl VectorSegment {
    /// Flush segment to memory-mapped storage
    pub fn flush_to_mmap(&self, path: &Path) -> Result<()> {
        // 1. Create directory
        // 2. Write vectors.bin (SoA layout)
        // 3. Write deleted.bin (bit-packed)
        // 4. Write metadata.json
        // 5. Write merkle_root
    }

    /// Load from memory-mapped storage
    pub fn load_from_mmap(path: &Path) -> Result<Self> {
        // Reverse of flush
    }
}
```

---

## Step 5: Modify mvcc.rs

### Add Persistent Segments

```rust
pub enum SegmentState {
    Active(Arc<VectorSegment>),
    Immutable(Arc<VectorSegment>),
    /// New: Persisted to disk
    Persisted(u64),  // segment_id for loading
    Merging(Vec<u64>),
}

/// Add method to load persisted segment
impl VectorMvcc {
    pub fn load_segment(&self, segment_id: u64) -> Result<Arc<VectorSegment>> {
        let path = self.storage_path.join(segment_id.to_string());
        VectorSegment::load_from_mmap(&path)
    }
}
```

---

## Step 6: Integration

### VectorSearch with Persistent Segments

```rust
impl VectorSearch {
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        let segments = self.mvcc.visible_segments();

        // Handle Persisted segments - load on demand
        // For MVP: load all, later: cache hot segments
        // ...
    }
}
```

---

## Testing Plan

1. **Unit Tests**
   - `test_mmap_segment_write_read`
   - `test_mmap_deleted_flags`
   - `test_mmap_corruption_recovery`

2. **Integration Tests**
   - `test_persist_and_reload`
   - `test_crash_recovery`

---

## Acceptance Criteria (from Mission)

- [ ] Memory-mapped storage backend
- [ ] WAL integration for vector operations
- [ ] Crash recovery from WAL
- [ ] Snapshot shipping for fast recovery
- [ ] MTTR: <5 minutes

---

## Notes

1. **Chunking**: Qdrant uses 64MB chunks - consider for large dimensions
2. **Alignment**: Use page-aligned allocations for optimal mmap
3. **fsync**: Ensure durability before marking segment "ready"
