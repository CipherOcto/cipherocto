# RFC-0106: Deterministic Numeric Tower (DNT)

## Status

Draft

## Summary

This RFC introduces the Deterministic Numeric Tower (DNT) — a unified numeric architecture for CipherOcto that enables deterministic execution of scientific, financial, and AI workloads across blockchain consensus.

The numeric tower extends RFC-0104 (DFP) and RFC-0105 (DQA) into a hierarchy of deterministic numeric types:

```
Layer 1 — Integer
├── INT
└── BIGINT

Layer 2 — Deterministic Scalar
├── DECIMAL (fixed-point)
├── DQA (quantized - RFC-0105)
└── DFP (floating-point - RFC-0104)

Layer 3 — Deterministic Vector
└── DVEC<N>

Layer 4 — Deterministic Matrix
└── DMAT<M,N>

Layer 5 — Deterministic Tensor (Future)
└── DTENSOR
```

The tower enables deterministic execution across:

- Scalar arithmetic
- Vector similarity search
- AI inference
- Zero-knowledge circuits

> ⚠️ **EXPERIMENTAL**: This RFC extends DFP/DQA with vector, matrix, and tensor types. Core DFP and DQA are stable; higher layers are experimental.

## Motivation

### Problem Statement

Current blockchains cannot efficiently support:

| Workload                   | Limitation                          |
| -------------------------- | ----------------------------------- |
| Floating-point computation | IEEE-754 non-deterministic          |
| Vector search              | No deterministic vector types       |
| Machine learning inference | Requires float + vectors + matrices |
| Scientific workloads       | No arbitrary-precision types        |

### Current State

| Blockchain | Approach                  |
| ---------- | ------------------------- |
| Ethereum   | No floats - integer only  |
| Solana     | Software emulation (slow) |
| Cosmos SDK | Fixed-point decimals only |
| This RFC   | Full numeric tower        |

### Desired State

CipherOcto should provide:

- Deterministic scalar arithmetic (integers, decimals, quantized, floating-point)
- Deterministic vector operations (similarity search)
- Deterministic matrix operations (linear algebra)
- Deterministic tensor operations (AI inference)
- ZK-friendly numeric domains

## Specification

### Numeric Tower Architecture

```
┌─────────────────────────────────────────────┐
│           Tensor Layer (Future)              │
│              DTENSOR<M,N,...>               │
├─────────────────────────────────────────────┤
│             Matrix Layer                     │
│              DMAT<M,N>                      │
├─────────────────────────────────────────────┤
│              Vector Layer                    │
│               DVEC<N>                       │
├─────────────────────────────────────────────┤
│            Scalar Layer                      │
│  DECIMAL  │  DQA  │  DFP  │  INT/BIGINT  │
└─────────────────────────────────────────────┘
```

### Layer 1 — Integer Domain

| Type   | Range         | ZK Efficiency |
| ------ | ------------- | ------------- |
| INT    | -2⁶³ to 2⁶³-1 | Excellent     |
| BIGINT | Arbitrary     | Excellent     |

Properties:

- Deterministic
- Fast
- ZK-friendly

### Layer 2 — Deterministic Scalar Domain

#### DECIMAL — Fixed-Point

```
value = mantissa × 10^-scale
```

Use cases: Finance, payments, tokens

#### DQA — Deterministic Quantized (RFC-0105)

```
value = integer × 2^-scale
```

Use cases: AI weights, embeddings, ML inference

#### DFP — Deterministic Floating-Point (RFC-0104)

```
value = mantissa × 2^exponent
```

Use cases: Scientific computing, statistics

### Layer 3 — Deterministic Vector Domain

```rust
/// Deterministic vector with N elements
pub struct DVecN<T: DfpScalar, const N: usize> {
    elements: [T; N],
}
```

#### Vector Types

| Type    | Elements | Use Case            |
| ------- | -------- | ------------------- |
| DVEC4   | 4        | Small embeddings    |
| DVEC8   | 8        | Image features      |
| DVEC16  | 16       | Audio features      |
| DVEC32  | 32       | NLP embeddings      |
| DVEC64  | 64       | Medium embeddings   |
| DVEC128 | 128      | Standard embeddings |
| DVEC256 | 256      | Large embeddings    |
| DVEC512 | 512      | High-dim embeddings |

> ⚠️ **Storage vs Consensus**: For high-performance vector search (HNSW indexing), use RFC-0103's VECTOR(f32) storage type. For consensus verification or on-chain inference, use DVEC with DQA elements.

> ⚠️ **IMPLEMENTATION RECOMMENDATION**: For initial implementation, limit to **DVEC4–DVEC64** only. Gas costs scale linearly with dimension (DVEC128_ADD = 768× baseline), making larger vectors economically infeasible for most use cases. DVEC128+ should be marked experimental.

#### Vector Operations

All vector operations are defined as ordered scalar operations to ensure determinism:

**Vector Add:**

```
DVEC_ADD(a, b):
    for i in 0..N:
        result[i] = SCALAR_ADD(a[i], b[i])
```

**Dot Product:**

```
DOT(a, b):
    sum = 0
    for i in 0..N:
        sum = SCALAR_ADD(sum, SCALAR_MUL(a[i], b[i]))
    return sum
```

> ⚠️ **Determinism requirement**: Strict iteration order ensures identical results across all hardware.

**Vector L2 Norm:**

```
NORM(a):
    return SQRT(DOT(a, a))
```

**Cosine Similarity:**

```
COS_SIM(a, b):
    return DOT(a, b) / (NORM(a) * NORM(b))
```

**Euclidean Distance:**

```
DISTANCE(a, b):
    diff = DVEC_SUB(a, b)
    return NORM(diff)
```

### Layer 4 — Deterministic Matrix Domain

```rust
/// Deterministic matrix with M rows, N columns
pub struct DMat<T: DfpScalar, const M: usize, const N: usize> {
    elements: [[T; N]; M],
}
```

#### Matrix Types

| Type        | Shape   | Use Case                 |
| ----------- | ------- | ------------------------ |
| DMAT2x2     | 2×2     | 2D transforms            |
| DMAT4x4     | 4×4     | 3D graphics, quaternions |
| DMAT16x16   | 16×16   | Linear layer             |
| DMAT64x64   | 64×64   | Attention heads          |
| DMAT128x128 | 128×128 | Large matrices           |

#### Matrix Operations

**Matrix Multiply:**

```
MAT_MUL(A, B):
    for i in 0..M:
        for j in 0..N:
            sum = 0
            for k in 0..K:
                sum = SCALAR_ADD(sum, SCALAR_MUL(A[i][k], B[k][j]))
            C[i][j] = sum
```

**Matrix Transpose:**

```
TRANSPOSE(A):
    for i in 0..M:
        for j in 0..N:
            B[j][i] = A[i][j]
```

### Layer 5 — Deterministic Tensor Domain (Future)

```rust
/// Deterministic tensor
pub struct DTensor<T: DfpScalar, const D: usize> {
    data: [T; D],
}
```

#### Tensor Types

| Type      | Shape | Use Case          |
| --------- | ----- | ----------------- |
| DTENSOR2  | 2D    | Matrix            |
| DTENSOR3  | 3D    | CNN feature maps  |
| DTENSOR4  | 4D    | CNN images (NCHW) |
| DTENSOR_N | ND    | General           |

### Deterministic Activation Functions

Neural networks require nonlinear functions:

```rust
/// Deterministic ReLU
pub fn relu(x: Dfp) -> Dfp {
    if x.class == DfpClass::Normal && x.sign == false && x.mantissa == 0 {
        Dfp::zero(false) // max(0, x)
    } else {
        x
    }
}

/// Deterministic Sigmoid (polynomial approximation)
pub fn sigmoid(x: Dfp) -> Dfp {
    // sigmoid(x) ≈ x / (1 + |x|)
    let abs_x = abs(x);
    let one = Dfp::new(1, 0);
    let denom = add(one, abs_x);
    div(x, denom)
}

/// Deterministic Tanh (polynomial approximation)
pub fn tanh(x: Dfp) -> Dfp {
    // tanh(x) ≈ x * (27 + x²) / (27 + 9x²)
    let x_sq = mul(x, x);
    let num = add(Dfp::new(27, 0), x_sq);
    let denom = add(Dfp::new(27, 0), mul(Dfp::new(9, 0), x_sq);
    let approx = div(num, denom);
    mul(x, approx)
}
```

#### Activation Error Bounds

| Function | Approximation     | Max Error (typical) | Error at extremes | Use Case              |
| -------- | ----------------- | ------------------- | ----------------- | --------------------- |
| ReLU     | exact             | 0 (exact)           | 0                 | Dropout replacement   |
| Sigmoid  | x/(1+\|x\|)       | ~0.1 at x=0         | Saturates to 0/1  | Binary classification |
| Tanh     | x(27+x²)/(27+9x²) | ~0.1 at x=0         | Saturates to ±1   | RNN, LSTM             |

> ⚠️ **Error Analysis**: Polynomial approximations accumulate error in deep networks. For critical applications, benchmark against higher-precision reference implementations. Consider lookup-table hybrid (LUT for [-4, 4], polynomial for outliers) to reduce error to <0.01.

| Type    | ZK Efficiency | Notes                 |
| ------- | ------------- | --------------------- |
| INT     | Excellent     | Native in circuits    |
| DQA     | Excellent     | Fast in ZK            |
| DECIMAL | Moderate      | Scale adds complexity |
| DFP     | Poor          | Normalization costly  |
| DVEC    | Poor          | More gates            |
| DMAT    | Poor          | Exponential growth    |

> **Recommendation**: ZK circuits should use INT or DQA for efficiency.

#### ZK Circuit Integration

> ⚠️ **Scope Clarification**: This RFC provides deterministic math types. ZK circuit generation is a separate concern.

For verifiable AI inference via ZK proofs:

| Component                | ZK Approach                    | Complexity        |
| ------------------------ | ------------------------------ | ----------------- |
| **INT**                  | Native range checks            | Low               |
| **DQA**                  | Scaled integer + scale witness | Medium            |
| **DVEC dot product**     | Per-element mul + sum          | High (O(N) gates) |
| **Activation (ReLU)**    | Comparison + select            | Low               |
| **Activation (Sigmoid)** | Lookup table                   | Medium            |

**ZK Workflow for AI Inference**:

```
1. Encode model weights as DQA (scaled integers)
2. Encode input as DQA
3. Generate R1CS/PLONK constraints for:
   - Matrix-vector multiply (DVEC dot products)
   - Activation functions (ReLU exact, Sigmoid via LUT)
4. Prove inference result matches on-chain computation
```

> **Note**: DFP is not recommended for ZK due to normalization complexity. Use DQA for bounded-precision ZK proofs.

### Execution Rules

Inside deterministic contexts:

```
FLOAT     → FORBIDDEN
DOUBLE    → FORBIDDEN
INT       → ALLOWED
DECIMAL   → ALLOWED
DQA       → ALLOWED
DFP       → ALLOWED
DVEC      → ALLOWED
DMAT      → ALLOWED
```

No implicit conversions between types.

### Gas Model

| Operation     | Relative Gas |
| ------------- | ------------ |
| INT_ADD       | 1x           |
| DFP_ADD       | 6-10x        |
| DFP_MUL       | 10-15x       |
| DFP_DIV       | 25-40x       |
| DVEC4_ADD     | 24x          |
| DVEC16_ADD    | 96x          |
| DVEC128_ADD   | 768x         |
| DVEC16_DOT    | 1,600x       |
| DMAT4x4_MUL   | 64x          |
| DMAT16x16_MUL | 65,536x      |

### Storage Encoding

All numeric types use canonical big-endian encoding:

```rust
/// DFP encoding (20 bytes)
struct DfpEncoding {
    class: u8,
    sign: u8,
    mantissa: i128,
    exponent: i32,
}

/// DVEC encoding
struct DVecEncoding {
    count: u32,
    elements: [DfpEncoding; N],
}

/// DMAT encoding
struct DMatEncoding {
    rows: u32,
    cols: u32,
    elements: [DfpEncoding; M*N],
}
```

## Rationale

### Why a Tower Architecture?

Each workload has different requirements:

| Workload          | Optimal Type | Precision  | Speed       |
| ----------------- | ------------ | ---------- | ----------- |
| Consensus         | INT          | Exact      | Fastest     |
| Finance           | DECIMAL      | 18 digits  | Fast        |
| AI inference      | DQA          | 8-16 bits  | Very fast   |
| Scientific        | DFP          | ~38 digits | Medium      |
| Similarity search | DVEC         | DFP        | Per element |
| Linear algebra    | DMAT         | DFP        | Per element |

### Alternatives Considered

| Alternative        | Pros        | Cons              | Rejection Reason          |
| ------------------ | ----------- | ----------------- | ------------------------- |
| Single float type  | Simple      | Can't optimize    | Not workload-specific     |
| Only integers      | ZK-friendly | No decimals       | Poor developer ergonomics |
| External libraries | Reuse       | Non-deterministic | Consensus risk            |

## Implementation

### Phase Dependencies

```
Phase 1: RFC-0105 (DQA) ────────────────┐  ← START HERE
                                              │
Phase 2: DVEC4-64 (small) ────────────────┤  ← Second priority
                                              │
Phase 3: RFC-0104 (DFP) ──────────────────┤  ← Higher risk
                                              │
Phase 4: DVEC128+ / DMAT ────────────────┐  ← Experimental
                                              │
Phase 5: DTENSOR (Future) ───────────────┘
```

### Default Deterministic Type

> ⚠️ **DECISION**: **DQA (RFC-0105) is the default deterministic numeric type.** Reserve DFP for specialized scientific workloads only.

| Use Case               | Recommended Type | Rationale                          |
| ---------------------- | ---------------- | ---------------------------------- |
| Finance, payments      | DQA              | Speed, exact decimals, ZK-friendly |
| ML weights, embeddings | DQA              | Bounded range, fast, ZK-efficient  |
| AI inference (bounded) | DQA + DVEC\<N\>  | Deterministic, low gas             |
| Scientific / stats     | DFP              | Wide dynamic range needed          |
| Consensus (general)    | DQA              | Default for any numeric column     |

### Implementation Priority

> ⚠️ **RECOMMENDATION**: Based on risk analysis, implementation should proceed in this order:
>
> 1. **RFC-0105 (DQA) first** — Best determinism/speed/practicality trade-off for largest near-term use cases (quant finance, ML preprocessing)
> 2. **DVEC4–DVEC64** — Small vectors with DQA elements for embeddings and similarity search
> 3. **DFP (RFC-0104)** — Only after DQA is stable; high implementation risk
> 4. **DVEC128+ and DMAT** — Experimental; deferred until proven necessary

### Mission 1: DVEC Implementation

- Location: `determ/dvec.rs`
- Acceptance criteria:
  - [ ] DVecN struct with const N (limit to N ≤ 64 initially)
  - [ ] Vector operations: add, sub, dot, norm
  - [ ] Similarity functions: cos_sim, distance
  - [ ] Serialization
- Estimated complexity: Medium

### Mission 2: DMAT Implementation

- Location: `determ/dmat.rs`
- Acceptance criteria:
  - [ ] DMat struct with const M, N
  - [ ] Matrix multiply
  - [ ] Transpose, inverse (2x2 only)
  - [ ] Serialization
- Estimated complexity: Medium

### Mission 3: Activation Functions

- Location: `determ/activations.rs`
- Acceptance criteria:
  - [ ] relu
  - [ ] sigmoid (polynomial)
  - [ ] tanh (polynomial)
- Estimated complexity: Low

### Mission 4: DTENSOR (Future)

- Location: `determ/tensor.rs`
- Acceptance criteria:
  - [ ] Generic tensor structure
  - [ ] Common operations
- Estimated complexity: High

## Impact

### Breaking Changes

None. DNT adds new types.

### Performance

| Type      | Relative Speed | Use Case   |
| --------- | -------------- | ---------- |
| INT       | 1x             | Consensus  |
| DECIMAL   | 1.2x           | Finance    |
| DQA       | 1.2x           | AI         |
| DFP       | 6-10x          | Scientific |
| DVEC128   | 768x           | Search     |
| DMAT16x16 | 65,536x        | ML         |

### Dependencies

- RFC-0104: DFP
- RFC-0105: DQA

## Related RFCs

- RFC-0104: Deterministic Floating-Point (DFP)
- RFC-0105: Deterministic Quant Arithmetic (DQA)
- RFC-0103: Unified Vector-SQL Storage
- RFC-0108: Verifiable AI Retrieval (ZK circuit integration)

## Research Integration

This RFC connects to the CipherOcto proof system stack:

| Layer         | Research Doc                  | Purpose                   |
| ------------- | ----------------------------- | ------------------------- |
| Numeric Tower | RFC-0106 (this)               | Deterministic computation |
| AIR           | `luminair-air-deep-dive.md`   | Constraint verification   |
| STARK Prover  | `stwo-gpu-acceleration.md`    | Proof generation          |
| Cairo/Orion   | `cairo-ai-research-report.md` | Provable ML inference     |

### CipherOcto Trust Stack

```
┌─────────────────────────────────────────┐
│         AI Agents / Applications         │
└────────────────────┬────────────────────┘
                     │
┌────────────────────▼────────────────────┐
│          Verifiable RAG                  │
│        (RFC-0108 - Transcript Proofs)   │
└────────────────────┬────────────────────┘
                     │
┌────────────────────▼────────────────────┐
│       Retrieval Gateway                  │
│        (RFC-0109 + 0113)                │
└────────────────────┬────────────────────┘
                     │
┌────────────────────▼────────────────────┐
│      Deterministic Execution VM          │
│      (RFC-0106 - DFP/DQA/DVEC)          │
└────────────────────┬────────────────────┘
                     │
┌────────────────────▼────────────────────┐
│        AIR Representation                │
│    (Algebraic Intermediate Representation)│
└────────────────────┬────────────────────┘
                     │
┌────────────────────▼────────────────────┐
│      STARK Prover (STWO GPU)             │
└─────────────────────────────────────────┘
```

### Execution Trace Format

> ⚠️ **Architectural Gap**: The repository lacks a standardized execution trace format.

**CipherOcto Execution Trace (CET)** standardizes traces for:

- SQL execution
- Vector search
- ML inference

Trace structure:

```rust
struct CipherOctoTrace {
    trace_id: u64,
    timestamp: u64,
    operations: Vec<TraceEntry>,
    input_commitment: Digest,
    output_commitment: Digest,
}

enum TraceEntry {
    SqlExec { query: String, rows: u64 },
    VectorSearch { index: String, k: u32, distance: DFP },
    MatMul { rows: u32, cols: u32, elapsed_ms: u32 },
    Activation { function: String, input: DVec, output: DVec },
}
```

Traces are converted to AIR constraints for STARK proof generation.

## Use Cases

### Deterministic AI Inference

```sql
-- Logistic regression inference
CREATE TABLE models (
    id INT,
    weights DVEC128,
    bias DFP
);

-- Deterministic inference
SELECT
    sigmoid(dot(weights, input) + bias) as prediction
FROM models;
```

### Vector Similarity Search

```sql
-- Embedding storage
CREATE TABLE embeddings (
    id INT,
    vector DVEC128
);

-- Deterministic k-NN
SELECT id, dot(vector, :query) as score
FROM embeddings
ORDER BY score DESC
LIMIT 10;
```

### Scientific Computing

```sql
-- Matrix operations
SELECT mat_mul(A, B) from matrices;
```

---

**Submission Date:** 2025-03-06
**Last Updated:** 2025-03-06
