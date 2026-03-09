# RFC-0106: Deterministic Numeric Tower (DNT)

## Status

Draft → Experimental

## Production Limitations

> ⚠️ **IMPORTANT**: When deployed to production, the following limits apply:

| Feature | Mainnet Limit (v1) | Status | Rationale |
|---------|-------------------|--------|-----------|
| DVEC<DQA> dimension | N ≤ 64 | ALLOWED | Recommended for vector search |
| DVEC<DFP> dimension | DISABLED | FORBIDDEN | Not ZK-friendly, use DQA |
| DMAT<DQA> dimension | M×N ≤ 8×8 | EXPERIMENTAL | After 6-month burn-in |
| DMAT<DFP> dimension | DISABLED | FORBIDDEN | Not ZK-friendly |
| DFP scalar | ALLOWED | RESTRICTED | Scientific only, no vector/matrix |
| DQA scalar | N/A | RECOMMENDED | Default for all production |
| Activation: ReLU | ALLOWED | STABLE | Exact, no bias |
| Activation: Sigmoid | LUT only | EXPERIMENTAL | Requires canonical LUT |
| Activation: Tanh | LUT only | EXPERIMENTAL | Requires canonical LUT |
| Max gas per op | 100,000 | HARD LIMIT | VM resource limits |

**Phased Rollout:**
- **Phase 1 (Launch)**: DQA scalar, DVEC<DQA>≤64, ReLU only
- **Phase 2 (6 months)**: Add Sigmoid/Tanh LUT, DMAT≤8×8
- **Phase 3 (Future)**: Re-evaluate DFP, DVEC128+, DMAT16×16

**Status Definitions:**
- **ALLOWED**: Full support for consensus operations
- **RECOMMENDED**: Preferred type for production workloads
- **RESTRICTED**: Allowed with limitations; not recommended for AI/ZK workloads
- **DISABLED/FORBIDDEN**: Not supported in consensus
- **EXPERIMENTAL**: Available but may change

**Recommendation**: Use DQA as default for all production workloads.

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

##### DQA Division Semantics

> ⚠️ **CRITICAL**: Division in fixed-point requires explicit rounding to maintain determinism.

```rust
/// DQA Division: a / b = (a * 2^scale) / b
///
/// The result MUST use the configured RoundingMode (default: Nearest)
/// to ensure consensus identity a/b == a/b across all nodes.
pub fn dqa_div(a: Dqa, b: Dqa, rounding: RoundingMode) -> Dqa {
    // Scale up, perform integer division, round, scale down
    todo!("Implement with rounding mode")
}

/// Rounding modes for DQA division
pub enum RoundingMode {
    Nearest,   // Default: Round to nearest, ties to even
    Up,        // Always round toward +infinity
    Down,      // Always round toward -infinity
    Truncate,  // Round toward zero (floor for positive)
}
```

> ⚠️ **FMA (Fused Multiply-Add)**: For ML kernels requiring `a * b + c`, use deterministic FMA when available. If not, implement as separate ops: `(a * b) + c` with explicit rounding between stages to maintain determinism.

#### DFP — Deterministic Floating-Point (RFC-0104)

```
value = mantissa × 2^exponent
```

Use cases: Scientific computing, statistics

#### Type Requirements for Generic Numeric Types

> ⚠️ **IMPLEMENTATION NOTE**: For generic `DVecN<T, N>` and `DMat<T, M, N>`, the type parameter `T` must satisfy:

```rust
/// Trait for deterministic scalar operations
/// Implemented by Dqa and Dfp concrete types
pub trait DeterministicScalar:
    Copy +
    Add<Output = Self> +
    Sub<Output = Self> +
    Mul<Output = Self> +
    Div<Output = Self> +
    PartialOrd +
    Zero +
    One
{
    fn zero() -> Self;
    fn one() -> Self;
    fn from_i64(value: i64, scale: u8) -> Self;
}
```

**Concrete type aliases:**
| Alias | Type | Use Case |
|-------|------|----------|
| `DVecN<Dqa, 64>` | Vector of DQA | Consensus, AI inference |
| `DVecN<Dfp, 64>` | Vector of DFP | Scientific computing |
| `DMat<Dqa, 16, 16>` | Matrix of DQA | ML linear layers |
| `DMat<Dfp, 4, 4>` | Matrix of DFP | 3D transforms |

### Layer 3 — Deterministic Vector Domain

```rust
/// Deterministic vector with N elements
///
/// ⚠️ **MEMORY SAFETY**: All vectors use heap allocation in VM runtime.
/// Stack allocation is NOT permitted for consensus safety - different VMs have
/// different stack sizes (Wasm typically 1MB, native can be 8MB).
///
/// ⚠️ **TYPE REQUIREMENT**: T must implement `DeterministicScalar` trait.
/// Use `DVecN<Dqa, N>` for consensus/AI workloads, `DVecN<Dfp, N>` for scientific computing.
pub struct DVecN<T, const N: usize>
where
    [(); N]: Sized,  // Compile-time check: N must be const
{
    elements: Vec<T>,  // ALWAYS heap-allocated for consensus safety
}

/// Compile-time dimension check
/// Note: This const assertion fails at compile time if N exceeds limit
const MAX_DVEC_ELEMENTS: usize = 128;

// Compile-time assert example (use in implementation):
// impl<T, const N: usize> DVecN<T, N> {
//     const _ASSERT_STACK_SAFE: () = assert!(N <= MAX_STACK_ELEMENTS, "N exceeds stack limit");
// }
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

> ⚠️ **MAINNET LIMIT**: DVEC dimension limited to **N ≤ 64** for production. DVEC128+ is experimental.

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

> ⚠️ **ZK OPTIMIZATION**: For ranking/similarity search, prefer **Squared Euclidean Distance**
> (`DOT(a, a)` without SQRT) to preserve rank order while avoiding expensive ZK-friendly SQRT circuits.

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
///
/// ⚠️ **MEMORY SAFETY**: For dimensions > 16, use heap allocation to prevent stack overflow.
/// Storage is a contiguous 1D buffer with strided indexing: `elements[row * N + col]`
///
/// ⚠️ **TYPE REQUIREMENT**: T must implement `DqaOps` or `DfpOps` trait.
/// Use `DMat<Dqa, M, N>` for consensus/AI workloads, `DMat<Dfp, M, N>` for scientific.
pub struct DMat<T, const M: usize, const N: usize> {
    elements: Vec<T>,  // Heap-allocated: M * N elements
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
    require A.cols == B.rows else REVERT(ERR_MATRIX_DIM_MISMATCH)
    for i in 0..M:
        for j in 0..N:
            sum = 0
            for k in 0..K:
                sum = SCALAR_ADD(sum, SCALAR_MUL(A[i][k], B[k][j]))
            C[i][j] = sum
```

> ⚠️ **ERROR CODE**: If `A.cols != B.rows`, transaction **REVERTS** with `ERR_MATRIX_DIM_MISMATCH`.

**Matrix Transpose:**

```
TRANSPOSE(A):
    for i in 0..M:
        for j in 0..N:
            B[j][i] = A[i][j]
```

> ⚠️ **HEAP ALLOCATION COST**: Matrix operations on `DMat` (which uses `Vec<T>`) include:
> - Allocation overhead: +50 gas per allocation
> - Memory expansion: +10 gas per 1KB above baseline

### Layer 5 — Deterministic Tensor Domain (Future)

```rust
/// Deterministic tensor
pub struct DTensor<T: DeterministicScalar, const D: usize> {
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
/// Deterministic ReLU: max(0, x)
/// Correct implementation: returns 0 for x <= 0, returns x otherwise
pub fn relu(x: Dfp) -> Dfp {
    // Check if x <= 0: (sign == 1) OR (value == 0)
    let is_negative_or_zero = x.sign == 1 || (x.mantissa == 0 && x.class == DfpClass::Normal);
    if is_negative_or_zero {
        Dfp::zero(false)  // Return positive zero
    } else {
        x  // Return original value
    }
}

/// Canonical Sigmoid: LUT-based (REQUIRED for consensus)
/// Polynomial approximation is DEPRECATED for consensus due to systematic bias:
/// - Real Sigmoid(0) = 0.5
/// - Polynomial approx: 0/(1+0) = 0  <- 50% systematic bias!
pub fn sigmoid(x: Dfp) -> Dfp {
    // Use SIGMOID_LUT with nearest-neighbor interpolation
    // See "Sigmoid Lookup Table (LUT) Specification" for canonical values
    todo!("Implement LUT-based sigmoid")
}

/// Polynomial sigmoid - DEPRECATED for consensus (use sigmoid() instead)
#[deprecated(note = "Use sigmoid() for consensus. This has systematic bias.")]
pub fn sigmoid_poly(x: Dfp) -> Dfp {
    let abs_x = abs(x);
    let one = Dfp::new(1, 0);
    let denom = add(one, abs_x);
    div(x, denom)
}

/// Canonical Tanh: LUT-based (REQUIRED for consensus)
pub fn tanh(x: Dfp) -> Dfp {
    // Use TANH_LUT with nearest-neighbor interpolation
    todo!("Implement LUT-based tanh")
}

/// Polynomial tanh - DEPRECATED for consensus (use tanh() instead)
#[deprecated(note = "Use tanh() for consensus.")]
pub fn tanh_poly(x: Dfp) -> Dfp {
    let x_sq = mul(x, x);
    let num = add(Dfp::new(27, 0), x_sq);
    let denom = add(Dfp::new(27, 0), mul(Dfp::new(9, 0), x_sq));
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

#### Consensus Activation Status

| Function | Status | Notes |
| -------- | ------ |-------|
| sigmoid | REQUIRED | Must use LUT-based implementation |
| tanh | REQUIRED | Must use LUT-based implementation |
| sigmoid_poly | DEPRECATED | Do not use for consensus |
| tanh_poly | DEPRECATED | Do not use for consensus |

#### Overflow and Saturation Semantics

> ⚠️ **CRITICAL**: Blockchain VMs cannot panic. All activation functions MUST define explicit behavior for edge cases.

| Operation | Behavior |
| --------- | -------- |
| Division by zero | Revert transaction (INVALID_OPERATION) |
| Overflow (multiplication) | Saturate to MAX_VALUE with correct sign |
| Underflow | Saturate to zero |
| NaN input | Return NaN (propagate) |
| ±Infinity input | Saturate to ±1 for sigmoid, ±1 for tanh, 0 for relu |

#### NaN and Special Values Policy

> ⚠️ **CONSENSUS REQUIREMENT**: NaN handling must be deterministic across all nodes.

```rust
/// NaN propagation policy for consensus-critical operations
pub enum NanPolicy {
    /// Default - NaN flows through computation (may cause consensus divergence)
    Propagate,
    /// Return error immediately - transaction fails
    Reject,
    /// Convert NaN to canonical zero (safe for ZK, may hide errors)
    CanonicalZero,
}

/// Special value handling for DFP (IEEE-754 compatible)
#[derive(Clone, Copy, Debug)]
pub enum SpecialValue {
    NaN,
    PositiveInfinity,
    NegativeInfinity,
    PositiveZero,
    NegativeZero,
}

/// Canonical NaN representation for DFP
/// Used for deterministic comparison across all nodes
const DFP_CANONICAL_NAN: u128 = 0x7FF8000000000001;  // Quiet NaN

/// Check if value is canonical NaN (deterministic)
fn is_canonical_nan(value: u128) -> bool {
    // Check: exponent all 1s, mantissa = canonical, sign = 0
    (value & 0xFFF0000000000000) == 0x7FF0000000000000 &&
    (value & 0x000FFFFFFFFFFFFF) == 0x0008000000000001
}
```

**Negative Zero Handling:**
- Equality comparison: `-0.0 == 0.0` returns `true`
- Ordering: `-0.0 < 0.0` returns `false`
- Hash: Both map to same hash value

**NaN in Consensus:**
- If any consensus-critical computation produces NaN, the transaction REVERTS
- Storage/queries may return NaN (non-consensus paths only)

**NaN Propagation Rules (Vector/Matrix):**

| Operation | NaN Behavior |
|----------|--------------|
| `DVEC_ADD(a, b)` | If any element NaN → NaN, REVERT |
| `DOT(a, b)` | If any element NaN → NaN, REVERT |
| `MAT_MUL(A, B)` | If any element NaN → NaN, REVERT |
| `relu(NaN)` | Returns NaN (REVERT in consensus) |
| `sigmoid(NaN)` | Returns NaN (REVERT in consensus) |
| `tanh(NaN)` | Returns NaN (REVERT in consensus) |

> ⚠️ **CONSERVATIVE RULE**: **Any NaN in any consensus-critical path → full transaction REVERT**. This is the safest approach to prevent consensus divergence.

#### Sigmoid Lookup Table (LUT) Specification

> ⚠️ **CANONICAL REQUIREMENT**: For consensus, the LUT must be deterministic across all nodes.

**⚠️ CRITICAL**: Out-of-range values use **hard clamp** (not polynomial), to avoid re-introducing bias.

| Parameter | Value |
| --------- | ----- |
| Version | 1 (wire format includes version) |
| Range | [-4.0, 4.0] |
| Step size | 0.01 (801 entries including endpoints) |
| Interpolation | Nearest neighbor only (linear is NOT consensus-safe) |
| Out-of-range | **Hard clamp** to 0.0 or 1.0 (NOT polynomial) |
| Canonical commitment | `poseidon2([...entries]) = 0x1f4a6b3c8d9e0f12...` |
| Storage | 801 × 2 bytes = 1,602 bytes (small enough for genesis) |

```rust
/// Canonical Sigmoid LUT v1
/// - Range: [-4.0, 4.0], step: 0.01
/// - Values: Q8.8 fixed-point (multiply by 256 to get actual value)
/// - Nearest-neighbor: index = round((x + 4.0) / 0.01)
const SIGMOID_LUT_V1: [u16; 801] = [
    // sigmoid(-4.0) = 0.017986 -> Q8.8 = 4
    // ...
    // sigmoid(0.0) = 0.5 -> Q8.8 = 128
    // ...
    // sigmoid(4.0) = 0.982014 -> Q8.8 = 251
    4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 17, 18, 20, 22, 23, 25, 27, 30, 32,
    // ... (801 entries total)
    // Full table: generate via `python3 -c "for i in range(801): x = -4.0 + i*0.01; v = int(256 / (1 + math.exp(-x))); print(f'{v},', end=' ' if i%20!=19 else '\n')"`
];

/// Canonical Tanh LUT v1
const TANH_LUT_V1: [i16; 801] = [
    // tanh(-4.0) = -0.999329 -> Q8.8 = -256
    // tanh(0.0) = 0.0 -> Q8.8 = 0
    // tanh(4.0) = 0.999329 -> Q8.8 = 256
    // Full table generated same way
];

/// LUT lookup function - uses integer arithmetic for determinism
/// Input: x as DQA (scaled integer with scale=2, i.e., x100)
/// Example: x = 400 means x = 4.0
fn sigmoid(x_scaled: i32) -> u16 {
    // x_scaled = x * 100 (DQA with scale=2)
    // LUT range: -400 to +400 (representing -4.0 to +4.0)
    let idx = (x_scaled + 400) / 1;  // Integer division
    let idx = idx.clamp(0, 800) as usize;
    SIGMOID_LUT_V1[idx]
}

/// Same for tanh
fn tanh_lookup(x_scaled: i32) -> i16 {
) -> i16 {
    let idx = (x_scaled + 400) / 1;
    let idx = idx.clamp(0, 800) as usize;
    TANH_LUT_V1[idx]
}
```

#### LUT Governance and Upgrades

> ⚠️ **UPGRADE PATH**: LUT is a chain parameter, not hard-coded.

1. **Genesis**: LUT v1 committed in genesis (hash in consensus)
2. **Upgrade**: Governance proposal to update LUT (requires 2/3 vote)
3. **Transition**: Old LUT valid for 1 epoch after upgrade (grace period)
4. **Version**: Wire format includes `lut_version: u8`

**Why hard clamp over polynomial?**
- Polynomial re-introduces the bias this LUT was designed to eliminate
- Hard clamp is deterministic, simple, and ZK-friendly

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

#### Explicit Type Conversion API

> ⚠️ **REQUIREMENT**: All conversions are explicit. No implicit narrowing/widening.

```rust
/// Conversion error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionError {
    PrecisionLoss { from_bits: u64, to_bits: u8, lost_info: String },
    ScaleMismatch { expected: u8, actual: u8 },
    OutOfRange { value: i128, min: i128, max: i128 },
    InvalidNaN,
}

/// Rounding mode for numeric conversions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingMode {
    #[default]
    Nearest,      // Round to nearest, ties to even
    Up,           // Always round toward +∞
    Down,         // Always round toward -∞
    Truncate,     // Round toward zero
}

/// Trait for explicit numeric conversion
pub trait NumericCast<Target>: Sized {
    /// Convert with error on precision loss
    fn cast(self) -> Result<Target, ConversionError>;

    /// Convert with truncation (explicit precision loss)
    fn cast_lossy(self, rounding: RoundingMode) -> Target;
}

// === DQA Conversions ===

impl NumericCast<Dqa> for Dfp {
    /// Convert DFP → DQA (may lose precision for extreme exponents)
    fn cast(self) -> Result<Dqa, ConversionError> {
        // Implementation: extract mantissa, compute target scale
        todo!("DFP to DQA conversion")
    }

    fn cast_lossy(self, rounding: RoundingMode) -> Dqa {
        // Implementation with explicit rounding
        todo!()
    }
}

impl NumericCast<Dfp> for Dqa {
    /// Convert DQA → DFP (always exact, no precision loss)
    fn cast(self) -> Result<Dfp, ConversionError> {
        Ok(Dfp::from_mantissa_exponent(self.value as i128, -(self.scale as i32)))
    }

    fn cast_lossy(self, _: RoundingMode) -> Dfp {
        self.cast().unwrap()  // DQA → DFP is always lossless
    }
}

impl NumericCast<Dqa> for i64 {
    /// Convert integer → DQA with specified scale
    fn cast(self) -> Result<Dqa, ConversionError> {
        Ok(Dqa::new(self, 0).unwrap())
    }

    fn cast_lossy(self, _: RoundingMode) -> Dqa {
        Dqa::new(self, 0).unwrap()
    }
}

// === Vector Conversions ===

impl<T: DeterministicScalar, const N: usize> DVecN<T, N> {
    /// Aggregate vector to scalar (e.g., sum, mean, max)
    pub fn sum(&self) -> T;
    pub fn mean(&self) -> T;
    pub fn product(&self) -> T;
}

/// Convert between vector element types
impl<const N: usize> DVecN<Dqa, N> {
    pub fn to_dfp(&self) -> DVecN<Dfp, N> {
        // Convert each element
        todo!()
    }
}
```

### Gas Model

> ⚠️ **CRITICAL**: Gas formulas are O(N) for vectors, O(N³) for matrices.
> - **Max DVEC dimension**: 128 (gas limit)
> - **Max DMAT dimension**: 16×16 (consensus), 64×64 (state access only)
> - **Strassen's algorithm**: FORBIDDEN (non-deterministic)
> - **Gas cap**: 100,000 gas units max per single numeric operation

```rust
/// Gas calculation helpers
const MAX_DVEC_DIM: usize = 128;
const MAX_DMAT_DIM_EXEC: usize = 16;   // Consensus-executable
const MAX_DMAT_DIM_STORAGE: usize = 64; // Storage only, not executable
const MAX_GAS_PER_OP: u64 = 100_000;

/// Scalar operation gas costs
const GAS_INT_ADD: u64 = 1;
const GAS_INT_MUL: u64 = 3;
const GAS_INT_DIV: u64 = 10;
const GAS_DQA_ADD: u64 = 5;
const GAS_DQA_MUL: u64 = 8;
const GAS_DQA_DIV: u64 = 20;
const GAS_DFP_ADD: u64 = 8;
const GAS_DFP_MUL: u64 = 15;
const GAS_DFP_DIV: u64 = 35;
const GAS_SQRT: u64 = 50;  // Newton-Raphson iterations

/// Vector operation gas formula:
/// - ADD: N × GAS_DQA_ADD
/// - DOT: N × (GAS_DQA_MUL + GAS_DQA_ADD) × 2
/// - NORM: N × (GAS_DQA_MUL + GAS_DQA_ADD) × 2 + GAS_SQRT
fn calculate_vec_gas(dim: usize, op: VectorOp) -> Result<u64, GasError> {
    if dim > MAX_DVEC_DIM {
        return Err(GasError::DimensionExceeded);
    }
    let base_gas = match op {
        VectorOp::Add => GAS_DQA_ADD * dim as u64,
        VectorOp::Dot => {
            // N multiplications + (N-1) additions
            (GAS_DQA_MUL * dim as u64) + (GAS_DQA_ADD * (dim - 1) as u64)
        },
        VectorOp::Norm => {
            // DOT + SQRT
            (GAS_DQA_MUL * dim as u64) + (GAS_DQA_ADD * (dim - 1) as u64) + GAS_SQRT
        },
    };
    if base_gas > MAX_GAS_PER_OP {
        return Err(GasError::GasExceeded);
    }
    Ok(base_gas)
}

/// Matrix operation gas formula:
/// - MAT_MUL: M × N × K × GAS_DQA_MUL + M × N × (K-1) × GAS_DQA_ADD
fn calculate_mat_gas(m: usize, n: usize, k: usize, executable: bool) -> Result<u64, GasError> {
    let max_dim = if executable { MAX_DMAT_DIM_EXEC } else { MAX_DMAT_DIM_STORAGE };
    if m > max_dim || n > max_dim || k > max_dim {
        return Err(GasError::DimensionExceeded);
    }
    let mul_ops = m * n * k;
    let add_ops = m * n * (k.saturating_sub(1));
    let gas = (GAS_DQA_MUL * mul_ops as u64) + (GAS_DQA_ADD * add_ops as u64);
    if gas > MAX_GAS_PER_OP {
        return Err(GasError::GasExceeded);
    }
    Ok(gas)
}
```

| Operation | Gas Formula | Example (N=16) | Max Dimension |
| --------- | ----------- |----------------| -------------|
| INT_ADD | 1 | 1 | N/A |
| DQA_ADD | 5 | 5 | N/A |
| DQA_MUL | 8 | 8 | N/A |
| DQA_DIV | 20 | 20 | N/A |
| DFP_ADD | 8 | 8 | N/A |
| DFP_MUL | 15 | 15 | N/A |
| DFP_DIV | 35 | 35 | N/A |
| SQRT | 50 | 50 | N/A |
| DVEC_ADD | 5 × N | 80 | 64 |
| DVEC_DOT | 8N + 5(N-1) | 203 | 64 |
| DVEC_NORM | DOT + 50 | 253 | 64 |
| DMAT_MUL | 8MNK + 5MN(K-1) | 8×4×4×4 + 5×4×4×3 = 608 | 8×8 |
| DMAT_MUL | REJECT | - | >8×8 |

### Storage Encoding

All numeric types use canonical big-endian encoding with version field for forward compatibility:

```rust
/// Version 1 encoding for deterministic scalars
const ENCODING_VERSION: u8 = 1;

/// ⚠️ WIRE FORMAT: Binary layout is byte-defined for protocol safety.
/// DO NOT rely on Rust repr(C) or repr(packed) for wire protocol.
/// Serialization MUST follow this exact byte order.

/// DQA encoding (16 bytes) - byte-defined layout
struct DqaEncoding {
    // Byte[0]: version (must be 1)
    version: u8,
    // Byte[1]: sign (0 = positive, 1 = negative)
    sign: u8,
    // Bytes[2-9]: value (big-endian i64)
    value: i64,
    // Byte[10]: scale (0-18)
    scale: u8,
    // Bytes[11-15]: reserved (must be zero)
    _reserved: [u8; 5],
}
/// Canonical byte layout:
/// | Byte 0 | Byte 1 | Bytes 2-9    | Byte 10 | Bytes 11-15 |
/// |--------|--------|--------------|---------|-------------|
/// | version| sign   | value (BE)   | scale   | reserved    |

/// DFP encoding (24 bytes) - byte-defined layout
struct DfpEncoding {
    // Byte[0]: version (must be 1)
    version: u8,
    // Byte[1]: class (0=zero, 1=normal, 2=inf, 3=nan)
    class: u8,
    // Byte[2]: sign (0 = positive, 1 = negative)
    sign: u8,
    // Bytes[3-18]: mantissa (big-endian i128)
    mantissa: i128,
    // Bytes[19-22]: exponent (big-endian i32)
    exponent: i32,
    // Byte[23]: reserved (must be zero)
    _reserved: u8,
}
/// Canonical byte layout:
/// | Byte 0 | Byte 1 | Byte 2 | Bytes 3-18      | Bytes 19-22   | Byte 23 |
/// |--------|--------|--------|-----------------|---------------|---------|
/// | version| class  | sign   | mantissa (BE)   | exponent (BE) | reserved|

/// DFP Canonical NaN representation
/// In DFP format: class=NAN(3), mantissa=1, exponent=0, sign=0
const DFP_CANONICAL_NAN: (u8, u8, i128, i32) = (1, 3, 1, 0);
/// Where: (version, class=NaN, mantissa=1, exponent=0)

/// DVEC encoding
struct DVecEncoding {
    version: u8,           // = 1
    element_type: u8,      // 0=DQA, 1=DFP
    dimension: u16,        // N
    scale: u8,             // For DQA elements
    _reserved: [u8; 3],
    elements: Vec<u8>,     // Contiguous serialized elements
}

/// DMAT encoding
struct DMatEncoding {
    version: u8,           // = 1
    element_type: u8,      // 0=DQA, 1=DFP
    rows: u16,             // M
    cols: u16,             // N
    scale: u8,             // For DQA elements
    _reserved: [u8; 2],
    elements: Vec<u8>,     // Contiguous serialized elements
}

/// Encoding validation rules:
/// 1. version must equal ENCODING_VERSION
/// 2. All _reserved bytes must be zero
/// 3. scale must be ≤ 18 for DQA
/// 4. mantissa/exponent use big-endian byte order
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

## Security Considerations

### Attack Vectors and Mitigations

| Attack Vector | Description | Mitigation |
|--------------|-------------|------------|
| **Gas Exhaustion** | Large matrix operations consume excessive gas | Hard dimension caps (16×16), gas limits per operation |
| **Stack Overflow** | Deep recursion or large arrays crash nodes | Heap allocation for large types, compile-time assertions |
| **Precision Manipulation** | Adversary exploits rounding to manipulate results | Explicit rounding modes, no implicit conversions |
| **NaN Injection** | NaN values propagate through consensus | Revert on NaN in consensus-critical paths |
| **Timing Attacks** | Variable-time operations leak information | Fixed iteration order, no data-dependent branches |
| **Side-Channel Leakage** | Data-dependent branches in arithmetic leak secrets | ALL operators MUST execute in constant time |

> ⚠️ **BRANCH-FREE REQUIREMENT**: Arithmetic operators MUST NOT branch on secret data (data-dependent branches are FORBIDDEN). Constant-time execution is RECOMMENDED but not strictly required for consensus determinism - what matters is that operations produce identical results across nodes, not that they run in identical cycle counts.

### DoS Prevention

1. **Hard Dimension Limits**: DVEC ≤ 128, DMAT ≤ 16×16 (executable)
2. **Gas Caps**: Max 100,000 gas per numeric operation
3. **Execution Timeouts**: VM-level timeout for long-running computations
4. **Memory Limits**: Wasm memory capped at 256MB

## Testing Strategy

### Determinism Verification

> ⚠️ **CRITICAL**: All numeric operations must produce identical results across all hardware/compilers.

```rust
/// Property-based test: determinism across platforms
#[test]
fn test_vector_add_determinism() {
    let a = DVecN::<Dqa, 64>::random();
    let b = DVecN::<Dqa, 64>::random();

    // Execute on multiple "nodes" (simulated)
    let result_a = execute_on_node_a(&a, &b);
    let result_b = execute_on_node_b(&a, &b);

    assert_eq!(result_a, result_b, "Vector add must be deterministic");
}

/// Property-based test: overflow handling
#[test]
fn test_overflow_saturation() {
    let max = Dqa::MAX_VALUE;
    let result = max.mul(max);  // Should saturate, not panic
    assert!(result <= Dqa::MAX_VALUE);
}
```

### Test Categories

| Category | Description | Tools |
|----------|-------------|-------|
| **Unit Tests** | Per-operation correctness | Standard Rust tests |
| **Property Tests** | Invariant verification | `proptest` or `quickcheck` |
| **Determinism Tests** | Cross-node consistency | Fuzzing with multiple runtimes |
| **Benchmark Tests** | Performance regression detection | `criterion` crate |
| **Fuzz Tests** | Edge case discovery | `cargo-fuzz` |

### Required Test Coverage

- [ ] All arithmetic operations (add, sub, mul, div)
- [ ] All vector operations (add, dot, norm, distance)
- [ ] All matrix operations (mul, transpose)
- [ ] Activation functions (ReLU, Sigmoid, Tanh)
- [ ] Edge cases: zero, max, min, NaN, infinity
- [ ] Conversion precision loss scenarios

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

## Upgrade and Migration Path

> ⚠️ **CRITICAL**: Numeric types require explicit versioning for future compatibility.

### Type Versioning Strategy

Every numeric type includes a version field in its wire encoding:

```rust
/// All numeric types include wire version
struct DqaEncoding {
    version: u8,    // = 1 for v1
    // ... rest of fields
}

struct DfpEncoding {
    version: u8,    // = 1 for v1
    // ... rest of fields
}
```

### Migration Rules

| Version Change | Migration Strategy |
|---------------|-------------------|
| v1 → v2 (same type) | Full backward compatibility. Old values remain valid. |
| v1 → v2 (new scale) | Explicit conversion required; no implicit widening. |
| DFP → DQA migration | Not automatic; requires explicit `NumericCast`. |
| LUT upgrade | Grace period: 1 epoch. Old LUT valid for reading; new required for writing. |

### Future Numeric Formats

This RFC anticipates future quantized formats:

| Future Format | Status | Migration |
|--------------|--------|-----------|
| MX (mixed precision) | Research | Future RFC |
| Block FP | Research | Future RFC |
| NF4 | Research | Future RFC |

**Migration Principles:**
1. **Never break existing stored values** — always provide conversion path
2. **Explicit over implicit** — no silent conversions
3. **Slow governance** — LUT/type upgrades require 2/3 supermajority
4. **Long deprecation** — deprecated features stay for ≥2 major versions

## Related RFCs

- RFC-0104: Deterministic Floating-Point (DFP)
- RFC-0105: Deterministic Quant Arithmetic (DQA)
- RFC-0103: Unified Vector-SQL Storage
- RFC-0108: Verifiable AI Retrieval (ZK circuit integration)
- RFC-0120: Deterministic AI Virtual Machine

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Verifiable AI Agents for DeFi](../../docs/use-cases/verifiable-ai-agents-defi.md)

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
    /// ⚠️ **ZK-FRIENDLY**: Use query_hash (PoseidonDigest) instead of String
    /// to avoid bloating AIR constraints with string data.
    /// function_id: 0=relu, 1=sigmoid, 2=tanh
    SqlExec { query_hash: Digest, rows: u64 },
    VectorSearch { index_hash: Digest, k: u32, distance: Dfp },
    MatMul { rows: u32, cols: u32, elapsed_ms: u32 },
    Activation { function_id: u8, input: DVecN<Dqa, 64>, output: DVecN<Dqa, 64> },
}
```

Traces are converted to AIR constraints for STARK proof generation.

## Use Cases

### Deterministic AI Inference

```sql
-- Logistic regression inference
CREATE TABLE models (
    id INT,
    weights DVEC64,
    bias DQA
);

-- Deterministic inference
SELECT
    sigmoid(dot(weights, input) + bias) as prediction
FROM models;
```

#### End-to-End Gas Cost Example: 2-Layer MLP

> ⚠️ **Realistic gas estimate** for a small 2-layer MLP:

```
Input: DVEC32<Dqa>
Layer 1: DVEC32 × DMAT32x16 → DVEC16 (ReLU)
Layer 2: DVEC16 × DMAT16x2 → DVEC2 (Sigmoid)

Operations:
- Layer 1 matmul: 32 × 16 × 32 = 16,384 scalar muls
- Layer 1 ReLU: 16 elements
- Layer 2 matmul: 16 × 2 × 16 = 512 scalar muls
- Layer 2 Sigmoid: 2 LUT lookups

Gas breakdown:
- MAT_MUL (16,384 + 512 ops): 16,896 × 10 = 168,960 gas
- ReLU (16 ops): 16 × 5 = 80 gas
- Sigmoid LUT (2 lookups): 2 × 20 = 40 gas
- DOT product (for inference): 32 × 10 × 10 = 3,200 gas
- TOTAL: ~172,000 gas

⚠️ **Note**: This exceeds the 100k gas limit for a single op!
Recommendation: Split inference across multiple transactions or reduce model size.
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
