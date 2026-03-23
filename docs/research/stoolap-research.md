# Stoolap Research Report

**Project**: Stoolap - Modern Embedded SQL Database
**Location**: https://github.com/stulast/stoolap
**Original Date**: March 2026
**Last Updated**: March 2026 (with Quant/DFP types, Pub/Sub, Rollup, Gas Metering)

---

## Executive Summary

Stoolap is a modern embedded SQL database written entirely in pure Rust (Apache 2.0 license). It targets low-latency transactional workloads and real-time analytical queries with modern SQL features and no external server process.

### Key Differentiators

| Feature                      | Stoolap  | SQLite    | DuckDB    | PostgreSQL |
| ---------------------------- | -------- | --------- | --------- | ---------- |
| **Time-Travel Queries**      | Built-in | No        | No        | Extension  |
| **MVCC Transactions**        | Yes      | No        | No        | Yes        |
| **Cost-Based Optimizer**     | Yes      | No        | Yes       | Yes        |
| **Adaptive Query Execution** | Yes      | No        | No        | Partial    |
| **Semantic Query Caching**   | Yes      | No        | No        | No         |
| **Parallel Query Execution** | Yes      | No        | Yes       | Yes        |
| **Native Vector Search**     | Yes      | Extension | Extension | Extension  |
| **Pure Rust**                | Yes      | No        | No        | No         |

---

## 1. Architecture

### 1.1 Layered Architecture

```mermaid
graph TB
    subgraph "API Layer"
        A[Database]
        B[Statement]
        C[Transaction]
        D[Rows]
    end

    subgraph "Execution Layer"
        E[Query Planner]
        F[Expression VM]
        G[Operators]
        H[Caches]
        I[Gas Meter]
    end

    subgraph "Optimizer Layer"
        J[Cost Estimation]
        K[Join Optimization]
        L[AQE]
        M[Bloom Filters]
    end

    subgraph "Storage Layer"
        N[MVCC Engine]
        O[Indexes]
        P[WAL]
        Q[Persistence]
        R[Vector Storage]
    end

    subgraph "Blockchain Layer"
        S[Rollup]
        T[Consensus]
        U[ZK Proofs]
    end

    subgraph "Core"
        V[Parser]
        W[Functions]
        X[Core Types]
        Y[Deterministic Types]
    end

    subgraph "Events"
        Z[Pub/Sub]
        AA[Event Bus]
    end

    A --> E
    E --> J
    J --> N
    N --> V
    E --> F
    F --> G
    G --> H
    E --> I
    N --> R
    N --> Z
    Z --> AA
```

### 1.2 Main Source Modules

| Module         | Purpose                                                      |
| -------------- | ------------------------------------------------------------ |
| `api/`         | Public database interface (Database, Statement, Transaction) |
| `executor/`    | Query execution engine with parallel execution               |
| `optimizer/`   | Cost-based optimization, AQE, join planning                  |
| `storage/`     | MVCC engine, indexes, WAL, persistence, vector storage     |
| `parser/`      | SQL parser (lexer, AST, statements)                          |
| `functions/`   | 101+ built-in SQL functions                                  |
| `core/`        | Core types (DataType, Value, Row, Schema)                    |
| `execution/`   | Execution engine with gas metering                            |
| `pubsub/`      | Event bus and WAL-based cache invalidation                   |
| `rollup/`     | L2 rollup protocol (batch, fraud proof, withdrawal)         |
| `consensus/`   | Blockchain operation log (blocks, operations)                |
| `trie/`        | Merkle trie for state verification                           |
| `determ/`      | Deterministic value types for blockchain (no Arc/pointers)    |
| `zk/`          | Zero-knowledge proof integration (STWO plugin)              |

---

## 2. Core Features

### 2.1 MVCC Transactions

**Semantic Purpose**: Provides snapshot isolation allowing consistent reads without locking, enabling concurrent read/write operations without blocking.

**Implementation** (`src/storage/mvcc/engine.rs`):

```rust
pub struct MVCCEngine {
    // Version store: tracks multiple row versions
    versions: BTreeMap<RowKey, Vec<RowVersion>>,
    // Transaction registry: tracks active transactions
    tx_registry: TransactionRegistry,
    // Write set: modifications within transactions
    write_sets: HashMap<TxId, WriteSet>,
}
```

**Components**:

| Component             | Purpose                                     |
| --------------------- | ------------------------------------------- |
| `MvccTransaction`     | Transaction context                         |
| `TransactionRegistry` | Global transaction tracking                 |
| `RowVersion`          | Individual row version with metadata        |
| `VisibilityChecker`   | Determines visible versions per transaction |

**Isolation Levels**:

- `ReadUncommitted`: No isolation
- `ReadCommitted`: See committed changes (default)
- `Snapshot`: See snapshot at transaction start (MVCC)

### 2.2 Multiple Index Types

**Semantic Purpose**: Different access patterns require different index structures for optimal performance.

**Implementation** (`src/storage/index/mod.rs`):

| Index Type           | Use Case                     | Implementation  |
| -------------------- | ---------------------------- | --------------- |
| **BTreeIndex**       | Range queries, sorted access | Standard B-tree |
| **HashIndex**        | O(1) equality lookups        | Hash map based  |
| **BitmapIndex**      | Low-cardinality columns      | Roaring bitmaps |
| **HnswIndex**        | Vector similarity search     | HNSW algorithm  |
| **MultiColumnIndex** | Composite queries            | Composite keys  |
| **PkIndex**          | Primary key lookups          | Virtual index   |

### 2.3 Cost-Based Optimizer

**Semantic Purpose**: Selects optimal query execution plans based on data statistics rather than heuristic rules.

**Implementation** (`src/optimizer/mod.rs`):

```rust
pub struct Optimizer {
    // Statistics collector
    stats: Statistics,
    // Cost estimation
    cost_model: CostModel,
    // Join reordering
    join_optimizer: JoinOptimizer,
    // Adaptive query execution
    aqe: AdaptiveQueryExecution,
}
```

**Features**:

- **Statistics Collection**: Table/column statistics via `ANALYZE`
- **Histogram Support**: Range selectivity estimation
- **Zone Maps**: Segment pruning for columnar storage
- **Join Optimization**: Multiple join algorithms with cost estimation
- **Adaptive Query Execution (AQE)**: Runtime plan switching
- **Cardinality Feedback**: Learn from actual execution stats

### 2.4 Semantic Query Caching

**Semantic Purpose**: Intelligent caching that understands query semantics, not just exact string matches. A cached query with broader predicates can serve results for more specific queries.

**Implementation** (`src/executor/semantic_cache.rs`):

```rust
pub struct SemanticCache {
    // Cached query results
    cache: HashMap<QueryKey, CachedResult>,
    // Predicate analysis
    predicate_analyzer: PredicateAnalyzer,
}

impl SemanticCache {
    /// Predicate subsumption: broader query covers narrower one
    /// If cached: amount > 100, new query: amount > 150
    /// Filter cached results with additional predicate
    pub fn get_or_execute<F>(&self, query: &str, pred: Predicate) -> Option<Vec<Row>> {
        // Check if new predicate subsumes cached predicate
        if pred.subsumes(&cached_pred) {
            // Apply additional filter to cached results
            return Some(filter_results(cached_results, pred));
        }
        None
    }
}
```

**Capabilities**:

- **Predicate Subsumption**: `amount > 100` covers `amount > 150`
- **Numeric Range Tightening**: Narrow `>` and `<` predicates
- **Equality Subset**: `IN` clause narrowing
- **AND Conjunction Strengthening**: Adding more filters

### 2.5 Time-Travel Queries

**Semantic Purpose**: Access historical data at any point in time without maintaining separate history tables.

**SQL Syntax**:

```sql
-- Query data as of specific timestamp
SELECT * FROM accounts AS OF TIMESTAMP '2024-01-15 10:30:00';

-- Query data as of specific transaction
SELECT * FROM inventory AS OF TRANSACTION 1234;
```

**Implementation**: The MVCC engine maintains all row versions with timestamps and transaction IDs, enabling point-in-time queries.

### 2.6 Parallel Query Execution

**Semantic Purpose**: Utilize multi-core processors for faster query execution on large datasets.

**Implementation** (`src/executor/parallel.rs`):

Uses Rayon for parallel operations:

```rust
// Parallel hash join
pub fn parallel_hash_join(
    left: Vec<Row>,
    right: Vec<Row>,
    left_key: Expr,
    right_key: Expr,
) -> JoinResult {
    // Build phase: create hash table in parallel
    let hash_table = left.par_chunks(CHUNK_SIZE)
        .flat_map(|chunk| build_hash_table(chunk))
        .collect::<HashMap<_, _>>();

    // Probe phase: lookup in parallel
    right.par_chunks(CHUNK_SIZE)
        .flat_map(|chunk| probe_hash_table(chunk, &hash_table))
        .collect()
}
```

**Parallel Operations**:

- Parallel hash join (build and probe phases)
- Parallel ORDER BY
- Parallel aggregation
- Configurable chunk sizes and thresholds

### 2.7 Vector Search (HNSW)

**Semantic Purpose**: Native vector similarity search for AI/ML applications without external services.

**Implementation** (`src/storage/index/hnsw.rs`):

```sql
-- Create table with vector column
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY,
    content TEXT,
    embedding VECTOR(384)
);

-- Create HNSW index
CREATE INDEX idx_emb ON embeddings(embedding)
USING HNSW WITH (metric = 'cosine', m = 32, ef_construction = 400);

-- Query with cosine distance
SELECT id, content, VEC_DISTANCE_COSINE(embedding, '[0.1, 0.2, ...]') AS dist
FROM embeddings ORDER BY dist LIMIT 10;
```

---

## 3. Storage Layer

### 3.1 Write-Ahead Log (WAL)

**Implementation** (`src/storage/mvcc/wal_manager.rs`):

```rust
pub struct WalManager {
    // Log file handle
    log_file: File,
    // Current log position
    position: u64,
    // Sync mode
    sync_mode: SyncMode,
    // Compression
    compressor: Lz4Compressor,
}
```

**Features**:

- **Durable Logging**: All operations logged before applying
- **Configurable Sync Modes**: None, Normal, Full
- **WAL Rotation**: Automatic rotation at 64MB
- **Compression**: LZ4 compression for large entries
- **Checkpoint Support**: Periodic snapshots

### 3.2 Persistence

**Implementation** (`src/storage/mvcc/persistence.rs`):

```rust
pub struct PersistenceManager {
    // Snapshot manager
    snapshots: SnapshotManager,
    // Recovery log
    recovery: RecoveryManager,
    // Zone maps for pruning
    zone_maps: ZoneMapStore,
    // Statistics
    statistics: StatisticsStore,
}
```

**Features**:

- Periodic full database snapshots
- Recovery: Load snapshot + replay WAL entries
- Zone Maps: Column-level min/max statistics for segment pruning
- Statistics: Table and column statistics for optimizer

### 3.3 Data Types

**Implementation** (`src/core/types.rs`):

| Type        | Description                      |
| ----------- | -------------------------------- |
| `Null`      | NULL value                       |
| `Integer`   | 64-bit signed integer (i64)      |
| `Float`     | 64-bit floating point (IEEE-754)|
| `Text`      | UTF-8 string                     |
| `Boolean`   | true/false                       |
| `Timestamp` | Timestamp with timezone          |
| `Json`      | JSON document                    |
| `Vector`    | f32 vector for similarity search |
| `DFP`       | Deterministic Float (RFC-0104)   |
| `DQA`       | Deterministic Quant (RFC-0105)  |

> **Note:** The SQL keyword `DECIMAL` or `NUMERIC` maps to `Float` (IEEE-754), not to DFP. Use the explicit `DFP` keyword for Deterministic Float per RFC-0104.

---

## Numeric Type System

### SQL Keyword to Stoolap Type Mapping

| SQL Keyword(s) | Stoolap DataType | Internal Type | Notes |
|---------------|------------------|--------------|-------|
| `INTEGER`, `INT`, `BIGINT`, `SMALLINT`, `TINYINT` | `Integer` | i64 | All integer types map to i64 |
| `FLOAT`, `DOUBLE`, `REAL`, `DECIMAL`, `NUMERIC` | `Float` | IEEE-754 f64 | Standard floating-point |
| `DFP`, `DETERMINISTICFLOAT` | `DeterministicFloat` | DFP (RFC-0104) | Explicit keyword required |
| `DQA`, `DQA(n)` | `Quant` | DQA (RFC-0105) | Scale stored in `SchemaColumn.quant_scale` |
| `TEXT`, `VARCHAR`, `CHAR`, `STRING` | `Text` | UTF-8 | |
| `BOOLEAN`, `BOOL` | `Boolean` | bool | |
| `TIMESTAMP`, `DATETIME`, `DATE`, `TIME` | `Timestamp` | UTC | |
| `JSON`, `JSONB` | `Json` | JSON doc | |
| `VECTOR`, `VECTOR(n)` | `Vector` | f32[] | Dimensions in `SchemaColumn.vector_dimensions` |
| `NULL` | `Null` | — | |

### CipherOcto Numeric Tower (RFCs)

| RFC | Type | Base | Scale | Status |
|-----|------|------|-------|--------|
| RFC-0104 | DFP (Deterministic Float) | 113-bit mantissa | variable | ✅ Implemented |
| RFC-0105 | DQA (Deterministic Quant) | i64 | 0-18 | ✅ Implemented |
| RFC-0110 | BIGINT (Arbitrary Precision) | ≤4096 bits | N/A | ❌ Not in Stoolap |
| RFC-0111 | DECIMAL (High Precision) | i128 | 0-36 | ❌ Not in Stoolap |

### Type Gap Matrix: Stoolap vs Numeric Tower

| Feature | Stoolap | RFC-0104 (DFP) | RFC-0105 (DQA) | RFC-0110 (BIGINT) | RFC-0111 (DECIMAL) | Gap Severity |
|---------|---------|----------------|-----------------|-------------------|-------------------|--------------|
| i64 Integer | ✅ | — | — | — | — | None |
| IEEE-754 Float | ✅ | — | — | — | — | None |
| DFP (113-bit) | ✅ `DFP` | ✅ | — | — | — | None |
| DQA (scale 0-18) | ✅ `DQA` | — | ✅ | — | — | None |
| BIGINT (≤4096 bit) | ❌ | — | — | ✅ | — | **Missing** |
| DECIMAL (i128, 0-36) | ❌ | — | — | — | ✅ | **Missing** |
| DFP ↔ DQA conversion | ❌ | — | — | — | — | **Missing** |
| BIGINT ↔ DECIMAL | ❌ | — | — | ✅ | ✅ | **Missing in Stoolap** |
| DQA ↔ DECIMAL | ❌ | — | ✅ | — | ✅ | **Missing in Stoolap** |

### Conversion Matrix (RFC-0110, RFC-0111)

Conversions are defined in the RFCs but NOT implemented in Stoolap:

| From | To | RFC | Stoolap Status | Notes |
|------|----|-----|----------------|-------|
| BIGINT | DECIMAL | RFC-0110 | ❌ Missing | Uses I128_ROUNDTRIP |
| DECIMAL | BIGINT | RFC-0110 | ❌ Missing | Requires scale = 0 |
| DQA | DECIMAL | RFC-0111 | ❌ Missing | Requires scale ≤ 18 |
| DECIMAL | DQA | RFC-0111 | ❌ Missing | May lose precision if scale > 18 |
| BIGINT | DQA | RFC-0110/0105 | ❌ Missing | Not documented in RFCs |
| DFP | DQA | RFC-0104 | ❌ Missing | Deterministic lowering pass not in DB |
| DFP | DECIMAL | RFC-0104/0111 | ❌ Missing | Would require lowering pass |

### Required Extensions

1. **BIGINT (RFC-0110)**: Add arbitrary precision integer type up to 4096 bits (64×u64 limbs)
2. **DECIMAL (RFC-0111)**: Add i128 scaled integer with scale 0-36 (not IEEE-754)
3. **Conversion functions**: Implement explicit conversion operators between numeric types

---

## 4. Query Execution Pipeline

### 4.1 Execution Flow

```mermaid
sequenceDiagram
    participant C as Client
    participant P as Parser
    participant O as Optimizer
    participant E as Executor
    participant S as Storage

    C->>P: SQL String
    P->>P: Lex & Parse
    P-->>O: AST

    O->>O: Cost Estimation
    O->>O: Plan Optimization
    O->>O: AQE Decision
    O-->>E: Execution Plan

    E->>S: Read Data
    S-->>E: Rows
    E->>E: Expression VM
    E->>E: Operators
    E-->>C: Results
```

### 4.2 Expression Compilation

**Implementation** (`src/executor/expression/vm.rs`):

```rust
pub struct ExpressionVM {
    // Stack-based execution (SmallVec<16>)
    stack: SmallVec<[StackValue; STACK_INLINE_CAPACITY]>,
    // Deterministic mode flag
    deterministic: bool,
}

pub struct ExecuteContext<'a> {
    row: &'a Row,
    row2: Option<&'a Row>,
    outer_row: Option<&'a FxHashMap<CompactArc<str>, Value>>,
    params: &'a [Value],
    // ... subquery executor, transaction ID
}
```

**Features**:
- **Zero allocation** in hot path (SmallVec inline storage)
- **Linear instruction dispatch** (switch-based opcode)
- **Stack-based VM** with 16-slot inline capacity
- **Deterministic mode**: Enforces DQA/DFP-only arithmetic, rejects FLOAT mixing
- **INT → DFP promotion** in deterministic contexts

**VM Opcodes**:
| Category | Operations |
| -------- | ---------- |
| Standard | `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg` |
| DQA | `DqaAdd`, `DqaSub`, `DqaMul`, `DqaDiv`, `DqaNeg`, `DqaAbs`, `DqaCmp` |
| Comparison | `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`, `IsNull`, `Like`, `Between` |
| Logical | `And`, `Or`, `Not`, `Xor` |
| Load | `LoadColumn`, `LoadConst`, `LoadParam`, `LoadNull` |

**DFP Arithmetic** (lines 3310-3359):
```rust
// DFP arithmetic - deterministic floating-point
if let (Some(dfp_a), Some(dfp_b)) = (dfp_a, dfp_b) {
    match op {
        ArithmeticOp::Add => dfp_add(dfp_a, dfp_b),
        ArithmeticOp::Sub => dfp_sub(dfp_a, dfp_b),
        ArithmeticOp::Mul => dfp_mul(dfp_a, dfp_b),
        ArithmeticOp::Div => dfp_div(dfp_a, dfp_b),
        ArithmeticOp::Mod => dfp_mod(dfp_a, dfp_b),
    }
}
```

### 4.3 Join Algorithms

**Implementation** (`src/executor/operators/`):

| Algorithm        | Best For          | Implementation          |
| ---------------- | ----------------- | ----------------------- |
| **Hash Join**    | Large datasets    | Build hash table, probe |
| **Merge Join**   | Pre-sorted inputs | Sorted merge            |
| **Nested Loop**  | Small tables      | Index-optimized variant |
| **Bloom Filter** | Runtime filtering | Probabilistic filter    |

---

## 5. API/Interfaces

### 5.1 Database API

**Implementation** (`src/api/database.rs`):

```rust
// Open database
let db = Database::open("memory://")?;  // In-memory
let db = Database::open("file:///tmp/mydb")?;  // Persistent

// Execute DDL/DML
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", ())?;

// Insert with parameters
db.execute("INSERT INTO users VALUES ($1, $2)", (1, "Alice"))?;

// Query
for row in db.query("SELECT * FROM users WHERE id = $1", (1,))? {
    let name: String = row.get("name")?;
}

// Query single value
let count: i64 = db.query_one("SELECT COUNT(*) FROM users", ())?;

// Transactions
let tx = db.begin()?;
tx.execute("UPDATE users SET name = $1 WHERE id = $2", ("Bob", 1))?;
tx.commit()?;
```

### 5.2 Prepared Statements

```rust
let stmt = db.prepare("SELECT * FROM users WHERE id = $1")?;
for row in stmt.query((1,))? { }
```

### 5.3 Named Parameters

```rust
db.execute_named(
    "INSERT INTO users VALUES (:id, :name)",
    named_params! { id: 1, name: "Alice" }
)?;
```

### 5.4 CLI

**Implementation** (`src/bin/stoolap.rs`):

```bash
./stoolap              # Interactive REPL
./stoolap -e "SELECT 1"  # Execute single query
./stoolap --db "file://./mydb"  # Persistent database
```

---

## 6. Additional Components

### 6.1 Blockchain Integration

Stoolap includes components for blockchain integration:

| Module       | Purpose                                        |
| ------------ | ---------------------------------------------- |
| `consensus/` | Block and operation types for operation logs   |
| `trie/`      | Merkle tries for state verification            |
| `determ/`    | Deterministic value types                      |
| `rollup/`    | L2 rollup types                                |
| `zk/`        | Zero-knowledge proof integration (STWO plugin) |

### 6.2 Merkle Trie

**Implementation** (`src/trie/`):

```rust
// RowTrie for state verification
pub struct RowTrie {
    root: TrieNode,
    hasher: Hasher,
}

// Hexary proofs for light clients
pub struct HexaryProof {
    siblings: Vec<Hash>,
    path: Vec<u8>,
}
```

### 6.3 WASM Support

**Implementation** (`src/wasm.rs`):

Can be compiled to WebAssembly for browser and edge execution.

---

## 6.5 Pub/Sub Module (NEW)

**Implementation** (`src/pubsub/mod.rs`):

Provides distributed cache invalidation through two mechanisms:

| Component    | Purpose                                              |
| ------------ | ---------------------------------------------------- |
| `EventBus`   | Local broadcast for same-process cache invalidation    |
| `WalPubSub`  | WAL-based pub/sub for cross-process cache invalidation |

**Features**:
- Event-driven cache invalidation on DML operations
- Idempotency tracking to prevent duplicate event processing
- Generates unique event IDs via SHA256 of timestamp

### 6.6 Execution Gas Metering (NEW)

**Implementation** (`src/execution/gas.rs`):

Provides gas metering for transaction execution with configurable pricing:

```rust
pub struct GasMeter {
    limit: u64,
    used: u64,
    price: GasPrice,
}

pub struct GasPrices {
    pub byte_storage: u64,
    pub write: u64,
    pub read: u64,
    pub compute: u64,
}
```

### 6.7 FOR UPDATE Row Locking (NEW)

**Implementation**: Recent commits added `FOR UPDATE` syntax and row locking support:

```sql
SELECT * FROM accounts WHERE id = 1 FOR UPDATE;
```

**Features**:
- Blocking row locks
- `FOR UPDATE NOWAIT` (fail immediately if locked)
- `FOR UPDATE SKIP LOCKED` (skip locked rows)

### 6.8 L2 Rollup Protocol (NEW)

**Implementation** (`src/rollup/mod.rs`):

Provides L2 rollup data structures for blockchain integration:

| Component         | Purpose                                      |
| ---------------- | -------------------------------------------- |
| `RollupBatch`   | Batch of operations for L2 submission        |
| `FraudProof`    | Fraud proof for invalid state transitions     |
| `Withdrawal`     | User withdrawal requests                      |
| `Submission`     | Batch submission to L1                       |

**Parameters**:
- `BATCH_INTERVAL`: Blocks between batches
- `CHALLENGE_PERIOD`: Time window for fraud proofs
- `MAX_BATCH_SIZE`: Maximum operations per batch
- `SEQUENCER_BOND`: Bond required to be sequencer

### 6.9 Deterministic Types (NEW)

**Implementation** (`src/determ/mod.rs`):

Provides deterministic types for blockchain SQL that:
- Use no `Arc`/pointers for predictable memory layout
- Support Merkle hashing for consistent state across nodes
- Are fully serializable for network transmission
- Have deterministic ordering for consensus

```rust
pub struct DetermValue { /* ... */ }
pub struct DetermRow { /* ... */ }
pub struct DetermMap { /* ... */ }
pub struct DetermSet { /* ... */ }
```

### 6.10 Vector Storage with Quantization (EXPANDED)

**Implementation** (`src/storage/vector/`):

Full vector storage with multiple quantization strategies:

| Component         | Purpose                                      |
| ---------------- | -------------------------------------------- |
| `VectorSegment`  | Immutable segments with Struct-of-Arrays layout |
| `VectorMerkle`   | Merkle tree for blockchain verification         |
| `VectorMvcc`     | Segment-level MVCC visibility                  |

**Quantization Types**:

| Type              | Description                           |
| ---------------- | ------------------------------------- |
| `ScalarQuantizer` | Linear quantization                   |
| `ProductQuantizer`| PQ for high-dimensional vectors       |
| `BinaryQuantizer` | Binary hashing for hamming distance    |

---

## 7. Why Stoolap Works

### 7.1 Design Decisions

| Decision                 | Rationale                                              |
| ------------------------ | ------------------------------------------------------ |
| **Pure Rust**            | Memory safety, no C dependencies, WASM support         |
| **MVCC**                 | Concurrent reads/writes without locking                |
| **Cost-Based Optimizer** | Better plans than rule-based optimizers                |
| **Semantic Caching**     | Higher cache hit rates through predicate understanding |
| **Time-Travel**          | Built-in temporal queries without application logic    |
| **Vector Search**        | Single database for SQL + AI workloads                 |
| **Gas Metering**         | Deterministic execution cost for blockchain             |
| **FOR UPDATE Locks**     | Serialized writes for critical operations             |

### 7.2 Performance Features

| Feature             | Benefit                               |
| ------------------- | ------------------------------------- |
| MVCC                | Lock-free reads, consistent snapshots |
| Parallel Execution  | Multi-core utilization                |
| Semantic Caching    | Reduced redundant computation         |
| AQE                 | Runtime plan adaptation               |
| Zone Maps           | Reduced I/O for analytical queries    |
| Vector Quantization | Memory-efficient similarity search    |

### 7.3 Recent Commits (March 2026)

| Commit | Feature |
|--------|---------|
| `f5c76e7` | Event emission for DML operations |
| `3075b8d` | Pub/Sub module for WAL-based cache invalidation |
| `83ca0b8` | FOR UPDATE row locking |
| `b8d20e5` | DQA opcodes to expression VM |
| `f519d92` | Quant (DQA) arithmetic to expression VM |
| `0d7031d` | DFP arithmetic into VM + RFC-0104 profiles |
| `7b535f6` | DeterministicFloat (DFP) type added |

---

## 8. Conclusion

Stoolap is a comprehensive embedded SQL database that combines:

- **Modern SQL**: CTEs, window functions, recursive queries, JSON, vectors
- **High Performance**: MVCC, parallel execution, semantic caching, cost-based optimization
- **Developer Experience**: Simple embedded API, prepared statements, rich type system
- **Persistence**: WAL, snapshots, crash recovery
- **Advanced Features**: Time-travel queries, vector search, adaptive execution
- **Deterministic Execution**: DQA and DFP types for blockchain-compatible computation
- **Blockchain Ready**: L2 rollup support, fraud proofs, ZK proof integration
- **Event System**: Pub/Sub for cache invalidation, event emission
- **Row Locking**: FOR UPDATE for serialized writes
- **Pure Rust**: Memory-safe, no external dependencies, WASM-compatible

The architecture demonstrates a well-thought-out balance between simplicity (embedded, no server) and sophistication (MVCC, cost-based optimizer, semantic cache, deterministic types).

### Key Differentiators (Updated)

| Feature                      | Stoolap  | SQLite    | DuckDB    | PostgreSQL |
| ---------------------------- | -------- | --------- | --------- | ---------- |
| **Time-Travel Queries**      | Built-in | No        | No        | Extension  |
| **MVCC Transactions**        | Yes      | No        | No        | Yes        |
| **Cost-Based Optimizer**     | Yes      | No        | Yes       | Yes        |
| **Adaptive Query Execution** | Yes      | No        | No        | Partial    |
| **Semantic Query Caching**   | Yes      | No        | No        | No         |
| **Parallel Query Execution** | Yes      | No        | Yes       | Yes        |
| **Native Vector Search**     | Yes      | Extension | Extension | Extension  |
| **Pure Rust**               | Yes      | No        | No        | No         |
| **Deterministic Types**     | DQA/DFP  | No        | No        | No         |
| **L2 Rollup Support**       | Yes      | No        | No        | No         |
| **FOR UPDATE Locks**        | Yes      | No        | Yes       | Yes        |
| **Gas Metering**            | Yes      | No        | No        | No         |

---

## References

- Repository: https://github.com/stulast/stoolap
- Documentation: https://stulast.github.io/stoolap/
