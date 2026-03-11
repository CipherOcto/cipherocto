# Phase 3 Design: Binary Quantization

## Overview

Implement Binary Quantization (BQ) for vector storage compression. BQ maps each float dimension to 1 bit (positive → 1, negative → 0), achieving 8-32x compression while enabling extremely fast search via XOR + popcount.

## Architecture

### Core Principles (from Qdrant)

1. **1-bit BQ**: Store 1 bit per dimension
2. **Query encoding**: Same transformation applied to search queries
3. **Hamming distance**: XOR + popcount for similarity scoring

### Data Flow

```
Insert: f32[768] → BinaryQuantizer → bitstream[768 bits = 96 bytes] → store
Search: query_f32[768] → BinaryQuantizer → bitstream[768 bits] → XOR + popcount → scores
```

### Compression Ratio

| Dimension | Original | BQ Compressed | Compression |
| --------- | -------- | ------------- | ----------- |
| 128       | 512B     | 16B           | 32x         |
| 384       | 1536B    | 48B           | 32x         |
| 768       | 3072B    | 96B           | 32x         |
| 1536      | 6144B    | 192B          | 32x         |

## Components

### File Structure

```
src/storage/vector/
├── quantization/
│   ├── mod.rs          # Public API exports
│   ├── config.rs       # QuantizationConfig, QuantizationType
│   ├── quantizer.rs    # BinaryQuantizer trait + implementation
│   ├── encode.rs       # encode_vector(), encode_query()
│   └── distance.rs     # hamming_distance(), xor_popcount()
```

### QuantizationConfig

```rust
#[derive(Clone, Debug)]
pub struct QuantizationConfig {
    pub quantization_type: QuantizationType,
    /// Whether to use quantization for search (vs full precision)
    pub enabled: bool,
    /// Vector dimension for validation
    pub dimension: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuantizationType {
    Binary,      // 1 bit per dimension
    Scalar,      // 4 bits per dimension (future)
    Product,     // Sub-vector quantization (future)
}
```

### BinaryQuantizer

```rust
pub trait Quantizer: Send + Sync {
    fn encode(&self, vector: &[f32]) -> Vec<u8>;
    fn decode(&self, data: &[u8]) -> Vec<f32>;
    fn encode_query(&self, query: &[f32]) -> Vec<u8>;
    fn distance(&self, encoded_a: &[u8], encoded_b: &[u8]) -> f32;
}

pub struct BinaryQuantizer {
    dimension: usize,
}

impl BinaryQuantizer {
    /// Encode vector: positive → 1, negative/zero → 0
    pub fn encode(&self, vector: &[f32]) -> Vec<u8> {
        let bits = vector.len();
        let bytes = (bits + 7) / 8;
        let mut result = vec![0u8; bytes];

        for (i, &v) in vector.iter().enumerate() {
            if v > 0.0 {
                result[i / 8] |= 1 << (i % 8);
            }
        }
        result
    }

    /// Decode: 1 → 1.0, 0 → -1.0
    pub fn decode(&self, data: &[u8]) -> Vec<f32> {
        let mut result = vec![0.0; self.dimension];
        for i in 0..self.dimension {
            let byte = data[i / 8];
            result[i] = if byte & (1 << (i % 8)) != 0 { 1.0 } else { -1.0 };
        }
        result
    }
}
```

### Distance Computation

```rust
/// Compute Hamming distance between two binary vectors
pub fn hamming_distance(a: &[u8], b: &[u8]) -> usize {
    debug_assert_eq!(a.len(), b.len());
    let mut distance = 0;
    for i in 0..a.len() {
        distance += (a[i] ^ b[i]).count_ones() as usize;
    }
    distance
}

/// Convert Hamming distance to similarity score [0, 1]
pub fn hamming_to_similarity(distance: usize, dimension: usize) -> f32 {
    1.0 - (distance as f32 / dimension as f32)
}
```

## Integration Points

### 1. VectorSegment

Add optional quantized storage:

```rust
pub struct VectorSegment {
    // ... existing fields
    /// Quantized vectors (optional, for compressed search)
    quantized: Option<QuantizedVectors>,
}

pub struct QuantizedVectors {
    data: Vec<u8>,  // bitstream
    dimension: usize,
    vector_count: usize,
}
```

### 2. Search

Dual-mode search:

```rust
impl VectorSearch {
    pub fn search(&self, query: &[f32], top_k: usize, use_quantization: bool) -> Vec<SearchResult> {
        if use_quantization {
            self.search_quantized(query, top_k)
        } else {
            self.search_exact(query, top_k)
        }
    }
}
```

### 3. WAL (per RFC)

Apply BQ before WAL write:

```rust
// Before: 768-dim f32 = 3072 bytes
// After BQ: 768 bits = 96 bytes
let quantized = quantizer.encode(embedding);
wal.log_insert(table_name, vector_id, segment_id, &quantized)?;
```

## Testing

1. **Unit tests**: encode/decode roundtrip, hamming distance accuracy
2. **Integration**: verify search results match between quantized and exact
3. **Benchmark**: measure search speedup with quantization

## Acceptance Criteria

- [ ] BinaryQuantizer encode/decode roundtrip preserves semantics
- [ ] Hamming distance correlates with L2/cosine similarity
- [ ] 32x compression ratio achieved
- [ ] Search with quantization enabled returns >95% recall@10
- [ ] All vector tests pass

## Future Phases

- **Scalar Quantization (SQ)**: 4 bits per dimension, 4x compression
- **Product Quantization (PQ)**: Sub-vector quantization, 4-64x configurable
- **Adaptive quantization**: Choose based on dimension/traffic patterns
