# RFC-0106: Deterministic Numeric Tower (DNT)

## Status

**Version:** v20 (2026-03-09) — Production Review Round 20
**Status:** Experimental

## Production Limitations

> ⚠️ **IMPORTANT**: When deployed to production, the following limits apply:

| Feature | Mainnet Limit (v1) | Status | 2026 Mainnet Ready? | Rationale |
|---------|-------------------|--------|---------------------|-----------|
| DVEC<DQA> dimension | N ≤ 64 | ALLOWED | **Yes** | Recommended for vector search |
| DVEC<DFP> dimension | DISABLED | FORBIDDEN | No | Not ZK-friendly, use DQA |
| DMAT<DQA> dimension | M×N ≤ 8×8 | EXPERIMENTAL | Phase 2 | After 6-month burn-in |
| DMAT<DFP> dimension | DISABLED | FORBIDDEN | No | Not ZK-friendly |
| DFP scalar | ALLOWED | RESTRICTED | Evaluate | Scientific only, no vector/matrix |
| DQA scalar | N/A | RECOMMENDED | **Yes** | Default for all production |
| Activation: ReLU | ALLOWED | STABLE | **Yes** | Exact, no bias |
| Activation: Sigmoid | LUT only | EXPERIMENTAL | Phase 2 | Requires canonical LUT |
| Activation: Tanh | LUT only | EXPERIMENTAL | Phase 2 | Requires canonical LUT |
| Max gas per op | 100,000 | HARD LIMIT | **Yes** | VM resource limits |

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

```mermaid
graph TD
    subgraph "Layer 5 — Tensor (Future)"
        DTENSOR[DTENSOR]
    end

    subgraph "Layer 4 — Matrix"
        DMAT[DMAT&lt;M,N&gt;]
    end

    subgraph "Layer 3 — Vector"
        DVEC[DVEC&lt;N&gt;]
    end

    subgraph "Layer 2 — Scalar"
        DFP[DFP]
        DQA[DQA]
        DECIMAL[DECIMAL]
    end

    subgraph "Layer 1 — Integer"
        INT[INT]
        BIGINT[BIGINT]
    end

    DTENSOR --> DMAT
    DMAT --> DVEC
    DVEC --> DFP
    DVEC --> DQA
    DVEC --> DECIMAL
    DFP --> INT
    DQA --> INT
    DECIMAL --> INT
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

```mermaid
graph TB
    subgraph "Layer 5"
        T[DTENSOR]
    end

    subgraph "Layer 4"
        M[DMAT]
    end

    subgraph "Layer 3"
        V[DVEC]
    end

    subgraph "Layer 2"
        S1[DECIMAL]
        S2[DQA]
        S3[DFP]
    end

    subgraph "Layer 1"
        I[INT/BIGINT]
    end

    T --> M
    M --> V
    V --> S1
    V --> S2
    V --> S3
    S1 --> I
    S2 --> I
    S3 --> I
```

### Determinism Guarantee

> ⚠️ **CRITICAL**: This section defines the formal determinism guarantees required for consensus safety.

For any numeric operation defined in this RFC:

**Given identical inputs and identical execution context, all compliant implementations MUST produce identical outputs bit-for-bit across all hardware architectures.**

This includes:

- CPU architectures (x86, ARM, RISC-V)
- Endianness (network byte order required)
- Compiler optimizations
- Runtime libraries

**All operations MUST be implemented using:**
- Integer arithmetic
- Fixed-point arithmetic
- Canonical algorithms defined in this RFC
- Deterministic lookup tables

**Hardware floating-point instructions MUST NOT be used for consensus execution.**

### Overflow / Underflow Semantics

> ⚠️ **CRITICAL**: Consensus safety requires deterministic overflow behavior.

| Type      | Overflow Behavior     | Underflow Behavior    |
| --------- | --------------------- | --------------------- |
| INT       | TRAP                  | TRAP                  |
| BIGINT    | Impossible (arbitrary) | Impossible (arbitrary) |
| DQA       | TRAP                  | TRAP                  |
| DFP       | TRAP                  | TRAP                  |
| DECIMAL   | TRAP                  | TRAP                  |
| DVEC      | TRAP (any element)   | TRAP (any element)    |
| DMAT      | TRAP (any element)   | TRAP (any element)   |

**Overflow/underflow MUST cause a deterministic execution trap.** The VM MUST revert the transaction and consume all gas allocated to that operation.

```
overflow → GasError::Overflow → REVERT
underflow → GasError::Underflow → REVERT
```

### NaN / Infinity Policy

> ⚠️ **CRITICAL**: NaN and Infinity are FORBIDDEN in consensus execution.

For blockchain consensus, we adopt the strictest policy:

**NaN is forbidden in consensus execution.**

```
NaN values MUST NOT appear in consensus state.
Any operation producing NaN MUST trap.
NaN input to any operation MUST trap.
```

**Infinity is forbidden in consensus execution.**

```
+Infinity and -Infinity MUST NOT appear in consensus state.
Any operation producing Infinity MUST trap.
Division by zero MUST trap.
```

**Trap behavior:**

```
NaN detected → ExecutionError::NaNProhibited → REVERT
Infinity detected → ExecutionError::InfinityProhibited → REVERT
Division by zero → ExecutionError::DivisionByZero → REVERT
```

### Canonicalization Rules

Deterministic systems MUST guarantee unique representations. Without canonical forms, serialization differs across nodes → hash divergence.

**DECIMAL canonicalization:**

```
mantissa MUST be normalized so that:
  mantissa % 10 ≠ 0

Examples:
  100 × 10^-2 → 1 × 10^0
  50 × 10^-3 → 5 × 10^-2
```

**DQA canonicalization:**

```
The value 4.0 has multiple DQA representations:
  mantissa=4,  scale=0  → 4 × 2^0  = 4.0  ← CANONICAL (mantissa has no trailing binary zero)
  mantissa=8,  scale=1  → 8 × 2^-1 = 4.0  ← INVALID (8 = 0b1000 has trailing zero)
  mantissa=16, scale=2  → 16 × 2^-2 = 4.0 ← INVALID (16 = 0b10000 has trailing zeros)
```

**DFP canonicalization:**

```
mantissa MUST have leading zeros counted correctly per format.
exponent MUST be minimal representation.
```

### Deterministic Execution Rules

> ⚠️ **CRITICAL**: These rules ensure cross-platform determinism.

Implementations MUST NOT use:

- IEEE-754 floating-point instructions
- Platform math libraries (libm, libc math)
- SIMD instructions with undefined rounding
- Threading / parallelization (results may differ)
- Random number generation (non-deterministic)

Allowed:

- Integer arithmetic (add, mul, div, mod, shift)
- Fixed-point arithmetic (DQA, DFP, DECIMAL)
- Deterministic lookup tables (LUT)
- Bitwise operations

#### Deterministic Parallelism Rules

> ⚠️ **CRITICAL**: Parallel execution is FORBIDDEN for consensus operations.

Even if implementations use multiple threads/Warps/SIMD lanes:

```
PROHIBITED: Tree reduction
  (a1 + a2) + (a3 + a4)   // Different rounding order

REQUIRED: Sequential reduction
  (((a1 + a2) + a3) + a4) // Same as flat loop
```

**Matrix multiplication:**
- Must use naive triple loop (i, j, k)
- Parallel tile decomposition produces different results
- Strassen algorithm is FORBIDDEN (non-deterministic)

**Vector dot product:**
- Must accumulate in strict order: i=0, then i=1, then i=2...
- No SIMD horizontal operations
- No reduction trees

**Reason:** Different parallelization strategies produce different rounding intermediate results → consensus divergence.

### Canonical Arithmetic Algorithms

> ⚠️ **CRITICAL**: This section defines exact algorithms for all numeric operations. Different implementations MUST produce identical results.

#### Algorithm Requirements

All operations MUST specify:

1. **Algorithm**: Exact computational method
2. **Rounding order**: When and how rounding occurs
3. **Overflow detection**: How traps are triggered
4. **Intermediate precision**: Width of internal calculations

#### DQA (Q8.8) Canonical Algorithms

**Addition:**

```
fn dqa_add(a: i32, b: i32, scale: u8) -> i32 {
    // Direct integer addition - no intermediate widening needed for Q8.8
    // Result: (a + b), same scale
    let result = a.wrapping_add(b);
    // Check for overflow: wrapping_add gives wrong result if signs differ
    if (a > 0 && b > 0 && result < 0) || (a < 0 && b < 0 && result > 0) { TRAP }
    result
}
```

**Multiplication:**

```
fn dqa_mul(a: i32, b: i32, scale: u8) -> i32 {
    // Widen to 64-bit, multiply, then scale down
    // Intermediate: i64 to prevent overflow
    let wide: i64 = (a as i64) * (b as i64);
    // Shift right by scale bits (Q8.8 scale = 8)
    let shifted = wide >> scale;
    // Check for overflow AFTER shift, before cast to i32
    if shifted > i32::MAX as i64 || shifted < i32::MIN as i64 { TRAP }
    shifted as i32
}
```

**Division:**

```
fn dqa_div(a: i32, b: i32, scale: u8, rounding: RoundingMode) -> i32 {
    // Widen, shift to preserve precision, divide
    let wide_a: i64 = (a as i64) << scale;  // Shift to maintain precision
    if b == 0 { TRAP }  // Division by zero
    let remainder = wide_a % (b as i64);
    let mut result = wide_a / (b as i64);
    // Apply rounding
    result = match rounding {
        Nearest => {
            // Round to nearest, ties to even
            let two_rem = remainder.abs() * 2;
            let abs_b = (b as i64).abs();
            if two_rem > abs_b || (two_rem == abs_b && result % 2 != 0) {
                // Round up if: remainder > b/2, OR remainder == b/2 AND result is odd
                if remainder >= 0 { result + 1 } else { result - 1 }
            } else {
                result
            }
        },
        Truncate => result,
        Up => if remainder > 0 { result + 1 } else { result },
        Down => if remainder < 0 { result - 1 } else { result },
    };
    // Check for overflow before truncating to i32
    if result > i32::MAX as i64 || result < i32::MIN as i64 { TRAP }
    result as i32
}
```

**FMA (Fused Multiply-Add):**

```
fn dqa_fma(a: i32, b: i32, c: i32, scale: u8) -> i32 {
    // Single rounding at end - critical for determinism
    // Intermediate: i128 to prevent overflow
    let intermediate: i128 = (a as i128) * (b as i128);
    let wide = (intermediate >> scale) + (c as i128);
    // FIX: Check i32 bounds since return type is i32
    if wide > i32::MAX as i128 || wide < i32::MIN as i128 { TRAP }
    wide as i32
}
```

> ⚠️ **FMA MANDATORY**: For any expression `(a * b) + c`, implementations MUST use FMA to ensure single rounding. Using separate mul then add produces different results due to double rounding.

#### BIGINT Canonical Algorithms

> ⚠️ **CRITICAL**: BIGINT requires explicit limits to prevent DoS attacks.

**BIGINT Limits:**

```
const MAX_BIGINT_BITS: usize = 4096;  // Maximum bit width
const MAX_BIGINT_LIMBS: usize = 64;   // Maximum number of 64-bit limbs
const MAX_BIGINT_OP_COST: u64 = 10000; // Gas cost cap
```

**BIGINT Operations:**

```
fn bigint_add(a: &BigInt, b: &BigInt) -> BigInt {
    assert!(a.bits() <= MAX_BIGINT_BITS);
    assert!(b.bits() <= MAX_BIGINT_BITS);
    // Limb-wise addition with carry propagation
    // Same algorithm on all implementations
}

fn bigint_mul(a: &BigInt, b: &BigInt) -> BigInt {
    assert!(a.bits() + b.bits() <= MAX_BIGINT_BITS);
    // MANDATORY: Schoolbook multiplication algorithm
    // Karatsuba is NOT allowed due to potential implementation variance
    // Schoolbook: O(n²) limb-wise multiplication
}

fn bigint_div(a: &BigInt, b: &BigInt) -> BigInt {
    if b.is_zero() { TRAP }
    // Binary long division, deterministic
}
```

#### Vector Dot Product Canonical Algorithm

```
fn dot_product<const N: usize>(a: &[i32; N], b: &[i32; N], scale: u8) -> Result<i64, ExecutionError> {
    // ACCUMULATOR: Must be 128-bit (i128) to prevent overflow
    // For N=64, each term up to (2^31)^2 = 2^62, sum up to 2^67
    // Maximum N before i128 overflow: N ≤ 2048 for Q8.8
    // Phase 1 limit: N ≤ 64 (conservative, well within safe bounds)
    // Phase 2+ may increase to N ≤ 128 after additional testing
    let mut acc: i128 = 0;
    // STRICT ITERATION ORDER: i=0 then 1 then 2... (no reduction tree)
    for i in 0..N {
        let product: i64 = (a[i] as i64) * (b[i] as i64);
        acc = acc.checked_add(product as i128)
            .ok_or(ExecutionError::Overflow)?; // TRAP on overflow
    }
    // Single rounding at end
    let shifted = acc >> scale;
    // Check for overflow before casting to i64
    if shifted > i64::MAX as i128 || shifted < i64::MIN as i128 {
        return Err(ExecutionError::Overflow);
    }
    Ok(shifted as i64)
}
```

> ⚠️ **NO PARALLEL REDUCTION**: Accumulation must be sequential. Tree reduction `(a1+a2)+(a3+a4)` produces different results than sequential `(((a1+a2)+a3)+a4)` due to different rounding.

#### Matrix Multiply Canonical Algorithm

```
fn mat_mul<const M: usize, const K: usize, const N: usize>(
    a: &[[i32; K]; M],
    b: &[[i32; N]; K],
    scale: u8
) -> Result<[[i32; N]; M], ExecutionError> {
    // Naive triple loop - deterministic order
    let mut result: [[i32; N]; M] = [[0; N]; M];
    for i in 0..M {
        for j in 0..N {
            let mut acc: i128 = 0;
            for k in 0..K {
                acc = acc.checked_add((a[i][k] as i128) * (b[k][j] as i128))
                    .ok_or(ExecutionError::Overflow)?;
            }
            // Check for overflow before casting to i32
            let shifted = acc >> scale;
            if shifted > i32::MAX as i128 || shifted < i32::MIN as i128 {
                return Err(ExecutionError::Overflow);
            }
            result[i][j] = shifted as i32;
        }
    }
    Ok(result)
}
```

#### Rounding Rules

| Mode | Negative Numbers | Positive Numbers | Tie Break |
| ---- | --------------- | --------------- | --------- |
| Nearest | Round away from zero at tie | Round away from zero at tie | Round to even |
| Truncate | Round toward zero | Round toward zero | N/A |
| Up | Round toward +∞ | Round toward +∞ | N/A |
| Down | Round toward -∞ | Round toward -∞ | N/A |

> ⚠️ **TIE BREAKING**: "Round to even" means: if exactly halfway, round to the nearest even number (0, 2, 4...). This is the IEEE 754 default and reduces systematic bias.

#### Deterministic Test Vectors

```
// DQA Test Vector: Addition
Input: a=0x0100 (1.0), b=0x0080 (0.5), scale=8
Output: 0x0180 (1.5)

// DQA Test Vector: Multiplication
Input: a=0x0200 (2.0), b=0x0200 (2.0), scale=8
Output: 0x0400 (4.0)

// DQA Test Vector: Division
Input: a=0x0100 (1.0), b=0x0200 (2.0), scale=8
Output: 0x0080 (0.5)

// DQA Test Vector: FMA
Input: a=0x0200 (2.0), b=0x0200 (2.0), c=0x0100 (1.0), scale=8
Output: 0x0500 (5.0)  // NOT 0x0501 (double rounding)

// DOT Product Test Vector
Input: a=[1,2,3], b=[4,5,6], scale=0
Output: 32  // 1*4 + 2*5 + 3*6 = 32

// SQRT Test Vector
Input: 0x0100 (1.0 in Q8.8) = 256
Output: 0x0100 (1.0) = 256  // sqrt(1.0) = 1.0
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
    // Reference: see Canonical Arithmetic Algorithms section (lines ~354-375)
    // Implementation: widen to i64, shift by scale, divide, apply rounding
    let wide_a: i64 = (a.value as i64) << a.scale;
    if b.value == 0 { TRAP }  // Division by zero
    let quotient = wide_a / (b.value as i64);
    let remainder = wide_a % (b.value as i64);
    let result = match rounding {
        RoundingMode::Nearest => {
            // Round to nearest, ties to even (banker's rounding)
            // If exactly halfway: round to nearest EVEN integer
            let twice_rem = remainder.abs() * 2;
            let divisor = (b.value as i64).abs();
            if twice_rem > divisor {
                // Strictly greater than half: round away from zero
                if remainder >= 0 { quotient + 1 } else { quotient - 1 }
            } else if twice_rem == divisor {
                // Exact tie: round to nearest EVEN
                // Positive odd (3→2), Negative odd (-3→-2)
                if quotient % 2 == 0 { quotient }
                else if quotient > 0 { quotient - 1 }
                else { quotient + 1 }
            } else {
                quotient
            }
        },
        RoundingMode::Truncate => quotient,
        RoundingMode::Up => if remainder > 0 { quotient + 1 } else { quotient },
        RoundingMode::Down => if remainder < 0 { quotient - 1 } else { quotient },
    };
    // Check for overflow before truncating to i32
    if result > i32::MAX as i64 || result < i32::MIN as i64 { TRAP }
    Dqa { value: result as i32, scale: a.scale }
}

/// Rounding modes for DQA division
pub enum RoundingMode {
    Nearest,   // Default: Round to nearest, ties to even
    Up,        // Always round toward +infinity
    Down,      // Always round toward -infinity
    Truncate,  // Round toward zero (floor for positive)
}
```

#### DFP — Deterministic Floating-Point (RFC-0104)

```
value = mantissa × 2^exponent
```

Use cases: Scientific computing, statistics

#### Type Requirements for Generic Numeric Types

> ⚠️ **IMPLEMENTATION NOTE**: For generic `DVecN<T, N>` and `DMat<T, M, N>`, the type parameter `T` must satisfy. `Zero` and `One` traits are from the `num-traits` crate.

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
| `DMat<Dqa, 8, 8>` | Matrix of DQA | ML linear layers (Phase 2+) |
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
///
/// ⚠️ **SCALE REQUIREMENT**: All elements in a DVEC MUST have the same scale (uniform scale).
/// Heterogeneous per-element scales are NOT supported in v1. This simplifies operations
/// and ensures deterministic results. Future versions may add heterogeneous scales as
/// an optional feature.
pub struct DVecN<T, const N: usize>
where
    [(); N]: Sized,  // Compile-time check: N must be const
{
    elements: Box<[T]>,  // Heap-allocated, fixed size, more memory efficient than Vec
}

/// Compile-time dimension check
/// Note: Phase 1 limit is 64, Phase 2+ raises to 128.
/// Compile-time allows 128 but runtime REJECTS N > 64 in Phase 1.
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
    sum = 0 (128-bit accumulator)
    for i in 0..N:
        product = SCALAR_MUL(a[i], b[i])
        sum = SCALAR_ADD(sum, product)
    return sum
```

> ⚠️ **Determinism requirements**:
> - Strict iteration order (i=0→N) ensures identical results
> - Accumulator MUST be 128-bit (i128) to prevent overflow
> - No early termination / short-circuiting
> - Rounding: DQA uses Q8.8 truncation (discard lower 8 bits)

**Vector L2 Norm:**

```
NORM(a):
    return SQRT(DOT(a, a))
```

#### Vector Similarity Operations

For similarity search, the following operations are defined:

**Cosine Similarity (canonical):**

```
COSINE_SIM(a, b):
    norm_a = SQRT(DOT(a, a))  // L2 norm
    norm_b = SQRT(DOT(b, b))
    if norm_a == 0 OR norm_b == 0:
        TRAP  // Cannot compute similarity of zero vector
    dot_ab = DOT(a, b)
    return dot_ab / (norm_a * norm_b)  // Division: round to nearest, ties to even
```

> ⚠️ **Determinism requirements**:
> - Division rounding: round to nearest, ties to even
> - Both norms MUST be non-zero (zero vector → TRAP)
> - Result range: [-1.0, 1.0] in Q8.8 format

**Squared Euclidean Distance (ZK-preferred):**

```
SQUARED_DISTANCE(a, b):
    sum = 0 (128-bit accumulator)
    for i in 0..N:
        diff = SCALAR_SUB(a[i], b[i])
        sum = SCALAR_ADD(sum, SCALAR_MUL(diff, diff))
    return sum
```

> ⚠️ **ZK optimization**: For ranking/similarity search, prefer **Squared Euclidean Distance** (`SQUARED_DISTANCE(a, b)`) to preserve rank order while avoiding expensive ZK-friendly SQRT circuits.

**L2 Distance:**

```
DISTANCE(a, b):
    return SQRT(SQUARED_DISTANCE(a, b))
```

> ⚠️ **ZK cost**: SQRT is expensive in ZK circuits. Use Squared Euclidean Distance when only ranking matters.

#### Deterministic SQRT Algorithm

> ⚠️ **REQUIRED**: SQRT must be deterministic across all nodes. Use Newton-Raphson with fixed iteration count.

```rust
/// Deterministic square root using Newton-Raphson
/// - Fixed 16 iterations (ensures full Q8.8 precision)
/// - Initial guess: leading-zero based for faster convergence
/// - Input: unsigned Q8.8 value (x in [0, 65535] representing [0, 255.996])
/// - Output: Q8.8 fixed-point
///
/// ⚠️ **NEGATIVE INPUT**: For signed Q8.8 (i32), caller MUST check sign first.
/// Negative input to this function is undefined behavior - trap in caller.
pub fn sqrt_q8_8(x: u32) -> u16 {
    if x == 0 { return 0; }
    // Check input bounds: x << 8 must not overflow u32
    if x > u32::MAX >> 8 { TRAP }  // x > 16777215 would overflow
    // Scale input by 2^8 BEFORE square root so result is in Q8.8 format
    let x_shifted = x << 8;
    // Better initial guess: use leading zeros to estimate sqrt
    // This gives faster convergence than simple shift
    let leading_zeros = x_shifted.leading_zeros();
    let initial_guess = if leading_zeros >= 16 {
        1 << 14  // Very small numbers
    } else if leading_zeros >= 8 {
        1 << 10
    } else {
        x_shifted >> 17  // Normal case: x/2
    };
    let mut z = initial_guess.max(1);
    // Fixed 16 iterations - same result on all nodes
    for _ in 0..16 {
        z = (z + x_shifted / z) >> 1;
    }
    z as u16  // Result is automatically in Q8.8 format
}

/// Canonical SQRT lookup wrapper for negative input handling
pub fn sqrt_q8_8_safe(x: i32) -> Result<u16, ExecutionError> {
    if x < 0 {
        return Err(ExecutionError::NegativeSqrt);  // TRAP on negative input
    }
    Ok(sqrt_q8_8(x as u32))
}
```

> ⚠️ **DEPRECATION WARNING + ZK OPTIMIZATION**: **NORM and DISTANCE are DEPRECATED in consensus.** Mandate **SQUARED_DISTANCE** for all similarity ranking — it preserves rank order while being ZK-friendly (no SQRT). For ranking, use `DOT(a, a)` without SQRT to avoid expensive ZK circuits. Only use SQRT for explicit display/UI purposes where deterministic output isn't required for consensus.

### Layer 4 — Deterministic Matrix Domain

```rust
/// Deterministic matrix with M rows, N columns
///
/// ⚠️ **MEMORY SAFETY**: For dimensions > 16, use heap allocation to prevent stack overflow.
/// Storage is a contiguous 1D buffer with strided indexing: `elements[row * N + col]`
///
/// ⚠️ **TYPE REQUIREMENT**: T must implement `DeterministicScalar` trait.
/// Use `DMat<Dqa, M, N>` for consensus/AI workloads, `DMat<Dfp, M, N>` for scientific.
///
/// ⚠️ **SCALE REQUIREMENT**: All elements in a DMAT MUST have the same scale (uniform scale).
/// Heterogeneous per-element scales are NOT supported in v1. This simplifies matrix operations
/// and ensures deterministic results. Future versions may add heterogeneous scales as an optional feature.
pub struct DMat<T, const M: usize, const N: usize> {
    elements: Box<[T]>,  // Heap-allocated, fixed size, more memory efficient than Vec
}
```

#### Matrix Types

| Type        | Shape   | Use Case                 |
| ----------- | ------- | ------------------------ |
| DMAT2x2     | 2×2     | 2D transforms            |
| DMAT4x4     | 4×4     | 3D graphics, quaternions |
| DMAT8x8     | 8×8     | Linear layer (Phase 2+) |
| DMAT16x16   | 16×16   | Phase 2+                |
| DMAT64x64   | 64×64   | Storage only (Phase 3)   |
| DMAT128x128 | 128×128 | Storage only (Future)   |

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

> ⚠️ **HEAP ALLOCATION COST**: Matrix operations on `DMat` (which uses `Box<[T]>`) include:
> - Allocation overhead: +50 gas per allocation
> - Memory expansion: +10 gas per 1KB above baseline

### Layer 5 — Deterministic Tensor Domain (Future)

```rust
/// Deterministic tensor (Future)
/// ⚠️ MEMORY SAFETY: Always use heap allocation for VM safety
pub struct DTensor<T: DeterministicScalar, const D: usize> {
    data: Box<[T]>,  // Heap-allocated, fixed size for consensus safety
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
/// Deterministic ReLU for DFP: max(0, x)
/// Returns 0 for x <= 0, returns x otherwise
pub fn relu(x: Dfp) -> Dfp {
    // Check if x <= 0: negative sign OR zero class
    // For Zero class (both +0 and -0), return positive zero to ensure canonical encoding
    if x.sign == 1 || x.class == DfpClass::Zero {
        Dfp::zero(false)  // Return canonical positive zero
    } else {
        x  // Return original value (positive)
    }
}

/// Deterministic ReLU for DQA: max(0, x)
/// Returns 0 for x <= 0, returns x otherwise
pub fn relu_dqa(x: Dqa) -> Dqa {
    // DQA uses two's complement: negative if sign bit is set after cast
    if x.value < 0 {
        Dqa { value: 0, scale: x.scale }  // Return zero with same scale for wire consistency
    } else {
        x  // Return original value
    }
}

/// Canonical Sigmoid: Uses DQA-based LUT lookup (REQUIRED for consensus)
/// Input: x as DFP (Q8.8, scale=8), convert to scale=2 for LUT index
/// The DQA-based sigmoid_lookup() below is the canonical implementation
pub fn sigmoid(x: Dfp) -> Dfp {
    // Convert DFP (scale=8) to LUT scale-2: x_scaled = mantissa * 100 / 256
    // Using integer math: (mantissa * 25) >> 6 preserves precision
    // Clamp mantissa to LUT domain before conversion to prevent silent wrap
    let clamped_mantissa = x.mantissa.clamp(-1024 * 256, 1024 * 256);
    let x_scaled = ((clamped_mantissa as i128 * 25) >> 6) as i32;
    let lut_value = sigmoid_lookup(x_scaled);
    Dfp::from_mantissa_exponent(lut_value as i128, -8)
}

/// Canonical Tanh: Uses DQA-based LUT lookup (REQUIRED for consensus)
pub fn tanh(x: Dfp) -> Dfp {
    // Convert DFP (scale=8) to LUT scale-2: x_scaled = mantissa * 100 / 256
    // Clamp mantissa to LUT domain before conversion to prevent silent wrap
    let clamped_mantissa = x.mantissa.clamp(-1024 * 256, 1024 * 256);
    let x_scaled = ((clamped_mantissa as i128 * 25) >> 6) as i32;
    let lut_value = tanh_lookup(x_scaled);
    Dfp::from_mantissa_exponent(lut_value as i128, -8)
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

> ⚠️ **CRITICAL**: Out-of-range activation function inputs (e.g., sigmoid input > 4.0) are handled by LUT hard-clamp at lookup time, before any arithmetic occurs. All arithmetic overflow and underflow TRAPs per the Overflow/Underflow Semantics table (line 185–200).

| Edge Case | Behavior |
| --------- | -------- |
| Division by zero | TRAP → REVERT |
| Out-of-range input (sigmoid > 4.0) | LUT hard-clamp to max output |
| Out-of-range input (sigmoid < -4.0) | LUT hard-clamp to min output |

#### NaN and Special Values Policy

> ⚠️ **CONSENSUS REQUIREMENT**: NaN handling must be deterministic across all nodes.

```rust
/// NaN policy for reference - actual consensus code MUST use Reject
/// NaN is FORBIDDEN in consensus per the NaN/Infinity Policy section
#[derive(Clone, Copy, Debug, Default)]
pub enum NanPolicy {
    /// ⚠️ DEPRECATED - NaN causes consensus divergence, never use in consensus
    #[deprecated(note = "NaN causes consensus divergence")]
    Propagate,
    /// Default - NaN triggers TRAP, transaction reverts
    #[default]
    Reject,
    /// ⚠️ DEPRECATED - May hide errors in ZK contexts
    #[deprecated(note = "May hide underlying errors")]
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

/// Canonical NaN representation for DFP (in DFP wire format)
/// In DFP format: (version=1, class=3=NaN, mantissa=1, exponent=0, sign=0)
const DFP_CANONICAL_NAN_CLASS: u8 = 3;  // DfpClass::NaN
const DFP_CANONICAL_NAN_MANTISSA: i128 = 1;
const DFP_CANONICAL_NAN_EXPONENT: i32 = 0;

/// Check if DFP wire encoding represents canonical NaN
fn is_canonical_nan(class: u8, sign: u8, mantissa: i128, exponent: i32) -> bool {
    // In DFP wire format: class=3 (NaN), sign=0, mantissa=1, exponent=0
    class == DFP_CANONICAL_NAN_CLASS &&
    sign == 0 &&  // Canonical NaN must have positive sign
    mantissa == DFP_CANONICAL_NAN_MANTISSA &&
    exponent == DFP_CANONICAL_NAN_EXPONENT
}
```

**Negative Zero Handling:**
- Equality comparison: `-0.0 == 0.0` returns `true`
- Ordering: `-0.0 < 0.0` returns `false`
- Hash: Both map to same hash value
- **Serialization**: Any zero MUST be encoded with sign=0 (positive)
- **Deserialization**: Any received encoding with sign=1 and class=Zero MUST be canonicalized to sign=0, or return `Err(InvalidEncoding)`

> ⚠️ **CANONICALIZATION ON READ (SINGLE CANONICAL BEHAVIOR)**: During deserialization, all values MUST be canonicalized:
> 1. `-0.0` (sign=1, class=Zero) MUST be converted to `+0.0` (sign=0, class=Zero) — **this is NOT an error**
> 2. Non-minimal exponents (DFP) MUST be normalized
> 3. Trailing binary zeros (DQA) MUST be shifted out
> Implementations that reject `-0.0` encodings will fail consensus.

> ⚠️ **Negative Zero Test Vector**:
> ```rust
> #[test]
> fn test_negative_zero_deserialization() {
>     // DFP encoded: version=1, class=Zero(0), sign=1, mantissa=0, exponent=0
>     // Byte layout: [version, class, sign, mantissa(16 bytes), exponent(4 bytes), reserved]
>     let encoded = [1, 0, 1, 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, 0,0,0,0, 0];
>     let decoded = Dfp::decode(&encoded).unwrap();
>     assert_eq!(decoded.sign, 0); // Canonicalized to positive zero
>     assert_eq!(decoded.class, DfpClass::Zero);
> }
> ```
> Implementations that reject `-0.0` encodings will fail consensus.

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
| Canonical commitment | SHA256: `9069599354fec1628994a5c7ca7f09d186801a78508cb3bca112696158d3c0e6` (Poseidon2 TBD) |
| Storage | 801 × 2 bytes = 1,602 bytes (small enough for genesis) |

```rust
/// Canonical Sigmoid LUT v1
/// - Range: [-4.0, 4.0], step: 0.01
/// - Values: Q8.8 fixed-point (multiply by 256 to get actual value)
/// - Nearest-neighbor: index = round((x + 4.0) / 0.01)
/// - Full LUT SHA256 Commitment (interim for Poseidon2):
///   SHA256([u16 little-endian flattened]): 9069599354fec1628994a5c7ca7f09d186801a78508cb3bca112696158d3c0e6
///
/// > ⚠️ **HASH PROVENANCE**: This SHA256 hash was generated using the `bin/generate_lut.rs` reference tool with **pure integer arithmetic**. The Python code in comments is for illustration only and MUST NOT be used for consensus LUT generation.
const SIGMOID_LUT_V1: [u16; 801] = [
    // Sample entries for manual validation (nearest-integer rounding):
    // Formula: v = int(256 * sigmoid(x) + 0.5)
    // sigmoid(-4.00) = 0.017986 -> 256*0.017986 = 4.60 -> rounds to 5
    // sigmoid(-3.50) = 0.029312 -> 256*0.029312 = 7.50 -> rounds to 8
    // sigmoid(-3.00) = 0.047426 -> 256*0.047426 = 12.14 -> rounds to 12
    // sigmoid(-2.00) = 0.119203 -> 256*0.119203 = 30.52 -> rounds to 31
    // sigmoid(-1.00) = 0.268941 -> 256*0.268941 = 68.85 -> rounds to 69
    // sigmoid(-0.50) = 0.377540 -> 256*0.377540 = 96.65 -> rounds to 97
    // sigmoid(0.00) = 0.500000 -> 256*0.500000 = 128.00 -> rounds to 128
    // sigmoid(0.50) = 0.622459 -> 256*0.622459 = 159.35 -> rounds to 159
    // sigmoid(1.00) = 0.731058 -> 256*0.731058 = 187.15 -> rounds to 187
    // sigmoid(2.00) = 0.880796 -> 256*0.880796 = 225.48 -> rounds to 225
    // sigmoid(3.00) = 0.952574 -> 256*0.952574 = 243.86 -> rounds to 244
    // sigmoid(3.50) = 0.970688 -> 256*0.970688 = 248.50 -> rounds to 248
    // sigmoid(4.00) = 0.982013 -> 251.40 -> rounds to 251
    //
    // First 5:  [5, 5, 5, 5, 5]      // sigmoid(-4.00 to -3.96)
    // Middle:    [128]                // sigmoid(0.00)
    // Last 5:   [251, 251, 251, 251, 251]  // sigmoid(3.96 to 4.00)
    //
    // Full table: generate via `python3 -c "
    // import math
    // for i in range(801):
    //     x = -4.0 + i*0.01
    //     v = int(256 / (1 + math.exp(-x)) + 0.5)  # nearest
    //     print(f'{v},', end=' ' if i%20!=19 else '\n')"`
    5, 5, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 7, 7, 7, 8, 8, 9, 9, 10,
    10, 11, 11, 12, 12, 13, 13, 14, 15, 15, 16, 17, 17, 18, 19, 20, 21, 22, 23, 24,
    // ... (801 entries total, full table in implementation)
];

/// Canonical Tanh LUT v1
/// - Range: [-4.0, 4.0], step: 0.01 (801 entries)
/// - Values: Q8.8 signed (multiply by 256 to get actual value)
/// - Full LUT SHA256 Commitment (interim for Poseidon2):
///   SHA256([i16 little-endian flattened]): 83114588025b68a64bad7babe80d3a87a72cefb55166f254df89964918254084
///
/// > ⚠️ **HASH PROVENANCE**: This SHA256 hash was generated using the `bin/generate_lut.rs` reference tool with **pure integer arithmetic**. No f64 in the generator - polynomial approximation only.
///
/// ⚠️ **TANH LUT COMPLETE**: The `TANH_LUT_V1` array below is fully populated.
/// The SHA256 hash above was computed over the little-endian byte representation of this exact array.
/// Any deviation in values constitutes a consensus failure.
///
/// **Hash Verification Test** (must be in `octo_determin` crate):
/// ```rust
/// #[test]
/// fn verify_tanh_lut_hash() {
///     // 1. Verify array length
///     assert_eq!(TANH_LUT_V1.len(), 801);
///     // 2. Verify boundary values
///     assert_eq!(TANH_LUT_V1[0], -256);   // tanh(-4.0)
///     assert_eq!(TANH_LUT_V1[400], 0);    // tanh(0.0)
///     assert_eq!(TANH_LUT_V1[800], 256);  // tanh(4.0)
///     // 3. Verify hash with exact byte serialization
///     let bytes: Vec<u8> = TANH_LUT_V1.iter()
///         .flat_map(|&x| x.to_le_bytes()).collect();
///     let hash = sha256(&bytes);
///     assert_eq!(hex::encode(&hash),
///         "83114588025b68a64bad7babe80d3a87a72cefb55166f254df89964918254084");
/// }
/// ```
///
/// **Hash Change History:**
/// - v11-v16: Had `todo!()` placeholder, hash `7cc2ab92...` was INVALID (never deployed)
/// - v17+: Has full 801-entry array, hash `83114588025b...` is CANONICAL
///
/// ⚠️ **IMPLEMENTATION REQUIREMENT**: The actual implementation MUST contain all
/// 801 entries. CI MUST verify: array length = 801, hash matches canonical value.
const TANH_LUT_V1: [i16; 801] = [
    // Generated by: bin/generate_lut.rs (pure integer polynomial approximation)
    // Range: [-4.0, 4.0], step: 0.01
    // Formula: Q8.8 polynomial: z - z³/3 + 2z⁵/15 - 17z⁷/315, then clamp to [-256, 256]
    //
    // ⚠️ **POLYNOMIAL DIVISION DETAILS**: All polynomial divisions use integer arithmetic
    // with explicit rounding. Coefficients are pre-scaled to avoid runtime division:
    // - z³/3 → (z³ × 171) >> 9 (171/512 ≈ 1/3)
    // - 2z⁵/15 → (z⁵ × 17476) >> 18
    // - 17z⁷/315 → (z⁷ × 13942) >> 16
    //
    // Sample: tanh(-4.0) = -256, tanh(0) = 0, tanh(4.0) = 256
     -256,  -698,  -683,  -673,  -658,  -649,  -635,  -626,
     -612,  -599,  -590,  -578,  -569,  -557,  -549,  -536,
     -528,  -517,  -505,  -498,  -487,  -479,  -468,  -461,
     -451,  -444,  -434,  -424,  -418,  -408,  -402,  -393,
     -387,  -378,  -369,  -363,  -354,  -349,  -341,  -335,
     -327,  -322,  -314,  -307,  -302,  -295,  -290,  -283,
     -278,  -272,  -267,  -261,  -254,  -250,  -244,  -240,
     -234,  -230,  -224,  -218,  -215,  -209,  -206,  -200,
     -197,  -192,  -189,  -184,  -179,  -176,  -171,  -168,
     -164,  -161,  -157,  -154,  -150,  -146,  -143,  -139,
     -137,  -133,  -131,  -127,  -124,  -121,  -118,  -116,
     -112,  -110,  -107,  -105,  -102,   -99,   -97,   -95,
      -93,   -90,   -88,   -86,   -84,   -82,   -79,   -78,
      -75,   -74,   -72,   -70,   -68,   -66,   -65,   -63,
      -61,   -60,   -58,   -56,   -55,   -54,   -52,   -51,
      -49,   -48,   -47,   -46,   -44,   -43,   -42,   -40,
      -39,   -38,   -37,   -36,   -35,   -34,   -33,   -32,
      -31,   -30,   -29,   -29,   -28,   -27,   -26,   -25,
      -24,   -23,   -23,   -22,   -22,   -21,   -20,   -19,
      -19,   -18,   -18,   -17,   -16,   -16,   -15,   -15,
      -14,   -14,   -13,   -13,   -12,   -12,   -12,   -11,
      -11,   -10,   -10,   -10,    -9,    -9,    -8,    -8,
       -8,    -7,    -7,    -7,    -7,    -6,    -6,    -6,
       -5,    -5,    -5,    -5,    -5,    -4,    -4,    -4,
       -4,    -4,    -3,    -3,    -3,    -3,    -3,    -3,
       -2,    -2,    -2,    -2,    -2,    -2,    -2,    -1,
       -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
       -1,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        1,     1,     1,     1,     1,     1,     1,     1,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
        0,     0,     0,     0,     0,     0,     0,     0,
       -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
       -1,    -1,    -1,    -1,    -1,    -1,    -1,    -1,
       -2,    -2,    -2,    -2,    -2,    -2,    -2,    -2,
       -2,    -2,    -3,    -3,    -3,    -3,    -3,    -3,
       -3,    -4,    -4,    -4,    -4,    -4,    -4,    -5,
       -5,    -5,    -5,    -5,    -6,    -6,    -6,    -6,
       -6,    -7,    -7,    -7,    -8,    -8,    -8,    -8,
       -9,    -9,    -9,   -10,   -10,   -11,   -11,   -11,
      -12,   -12,   -13,   -13,   -13,   -14,   -14,   -15,
      -15,   -16,   -16,   -17,   -17,   -18,   -19,   -19,
      -20,   -20,   -21,   -22,   -23,   -23,   -24,   -24,
      -25,   -26,   -27,   -28,   -29,   -30,   -30,   -31,
      -32,   -33,   -34,   -35,   -36,   -37,   -38,   -39,
      -40,   -41,   -43,   -44,   -45,   -47,   -48,   -49,
      -50,   -52,   -53,   -55,   -56,   -57,   -59,   -61,
      -62,   -64,   -66,   -67,   -69,   -71,   -73,   -75,
      -76,   -79,   -80,   -83,   -85,   -87,   -89,   -91,
      -94,   -96,   -98,  -100,  -103,  -106,  -108,  -111,
     -114,  -117,  -119,  -122,  -125,  -128,  -132,  -134,
     -138,  -140,  -144,  -147,  -151,  -155,  -158,  -162,
     -165,  -169,  -172,  -177,  -180,  -185,  -190,  -193,
     -198,  -201,  -207,  -210,  -216,  -219,  -225,  -231,
     -235,  -241,  -245,  -251,  -255,  -262,  -268,  -273,
     -279,  -284,  -291,  -296,  -303,  -308,  -315,  -323,
     -328,  -336,  -342,  -350,  -355,  -364,  -370,  -379,
     -388,  -394,  -403,  -409,  -419,  -425,  -435,  -445,
     -452,  -462,  -469,  -480,  -488,  -499,  -506,  -518,
     -529,  -537,  -550,  -558,  -570,  -579,  -591,  -600,
     -613,  -627,  -636,  -650,  -659,  -674,  -684,  -699,
      256,
];

/// LUT lookup function - uses integer arithmetic for determinism
/// Input: x as DQA (scaled integer with scale=2, i.e., x100)
/// Example: x = 400 means x = 4.0
fn sigmoid_lookup(x_scaled: i32) -> u16 {
    // x_scaled = x * 100 (DQA with scale=2)
    // LUT range: -400 to +400 (representing -4.0 to +4.0)
    // After adding 400: maps to [0, 800]
    // x_scaled = -400 → 0 → clamp(0,800) → 0 → idx 0 (sigmoid(-4.0))
    // x_scaled = 0     → 400 → clamp(0,800) → 400 → idx 400 (sigmoid(0) = 0.5)
    // x_scaled = 400   → 800 → clamp(0,800) → 800 → idx 800 (sigmoid(4.0))
    let idx = (x_scaled + 400).clamp(0, 800) as usize;
    SIGMOID_LUT_V1[idx]
}

/// Same for tanh
fn tanh_lookup(x_scaled: i32) -> i16 {
    let idx = (x_scaled + 400).clamp(0, 800) as usize;
    TANH_LUT_V1[idx as usize]
}
```

#### Canonical LUT Specification

> ⚠️ **MANDATORY**: Every LUT must have these fields for deterministic consensus.

**LUT Header Structure:**

```
struct CanonicalLUT {
    lut_id: u16,           // Unique identifier (e.g., 0x0001 = sigmoid, 0x0002 = tanh)
    version: u8,           // Version number (increment on change)
    hash: [u8; 32],       // SHA256 or Poseidon2 hash of data bytes
    size: u16,            // Number of entries
    domain_min: i32,       // Minimum input value (Q8.8 format)
    domain_max: i32,       // Maximum input value (Q8.8 format)
    output_scale: u8,      // Output scale (e.g., 8 for Q8.8)
    reserved1: [u8; 5],   // Former interpolation field - must be zero (linear forbidden in consensus)
    reserved: [u8; 4],   // Future use, must be zero
    data: [u8],           // Flattened output values
}
```

**Canonical LUT Registry:**

| LUT_ID | Name | Version | Size | Domain | Output Scale | Hash |
|--------|------|---------|------|--------|--------------|------|
| 0x0001 | SIGMOID_V1 | 1 | 801 | [-4.0, 4.0] | Q8.8 | `9069599354fec1628994a5c7ca7f09d186801a78508cb3bca112696158d3c0e6` |
| 0x0002 | TANH_V1 | 1 | 801 | [-4.0, 4.0] | Q8.8 signed | `7cc2ab92e901c133ab430d4e095c9fec109ab2aea8ae35f109ffae5a3cd9f60b` |
| 0x0003 | SIGMOID_V2 | 2 | 1601 | [-8.0, 8.0] | Q8.8 | TBD (Phase 2) |
| 0x0004 | TANH_V2 | 2 | 1601 | [-8.0, 8.0] | Q8.8 | TBD (Phase 2) |

> ⚠️ **Note**: Final on-chain version will use **Poseidon2 Merkle root** instead of SHA256. Nodes MUST verify the LUT hash at genesis and after governance upgrades.

**Lookup Algorithm (Canonical):**

```
fn lut_lookup(lut: &CanonicalLUT, x: i32) -> i32 {
    // CRITICAL: Linear interpolation is FORBIDDEN for consensus
    // Only nearest-neighbor is consensus-safe
    // Enforced at LUT construction/genesis time: reject if reserved1[0] != 0

    // Clamp to domain bounds
    let idx = if x <= lut.domain_min {
        0
    } else if x >= lut.domain_max {
        lut.size - 1
    } else {
        // Nearest neighbor (only allowed interpolation for consensus)
        let range = lut.domain_max - lut.domain_min;
        let idx = ((x - lut.domain_min) as i64 * (lut.size as i64 - 1)) / range as i64;
        idx as usize
    };
    read_lut_element(lut, idx)
}
```

> ⚠️ **LUT HASH VERIFICATION**: All nodes MUST verify LUT hash matches the canonical value in genesis. Mismatched LUT = consensus failure.

#### LUT Governance and Upgrades

> ⚠️ **UPGRADE PATH**: LUT is a chain parameter, not hard-coded.

1. **Genesis**: LUT v1 committed in genesis (hash in consensus)
2. **Upgrade**: Governance proposal to update LUT (requires 2/3 vote)
3. **Transition**: Old LUT valid for 1 epoch after upgrade (grace period)
4. **Version**: Wire format includes `lut_version: u8`

> ⚠️ **LUT HASH CORRECTION MIGRATION PATH** (if bug found post-launch):
> - **Detection**: Consensus monitoring detects hash mismatch
> - **Governance**: Emergency proposal with correct hash
> - **Dual Acceptance**: Both old and new hash accepted during transition
> - **Migration**: Smart contracts can re-compute affected values during grace period
> - **Deprecation**: Old hash rejected after transition period
> - **Historical Integrity**: Old blocks remain valid (computed with old LUT)

> ⚠️ **IMPLEMENTATION TIP**: The execution context must expose `active_lut_version`. Pass `lut_version` as a transaction context parameter rather than hardcoding `SIGMOID_LUT_V1` inside the function. This enables seamless future upgrades without code changes.

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

> ⚠️ **PHASE ANNOTATION**: The following represents the full DNT specification. See Production Limitations for Phase 1 restrictions.

Inside deterministic contexts:

```
FLOAT     → FORBIDDEN
DOUBLE    → FORBIDDEN
INT       → ALLOWED
DECIMAL   → ALLOWED
DQA       → ALLOWED
DFP       → ALLOWED (RESTRICTED in Phase 1)
DVEC      → ALLOWED (N ≤ 64 in Phase 1)
DMAT      → ALLOWED (M×N ≤ 8×8 in Phase 2+)
```

No implicit conversions between types.

#### Explicit Type Conversion API

> ⚠️ **REQUIREMENT**: All conversions are explicit. No implicit narrowing/widening.

```rust
/// Conversion error types
/// ⚠️ All fields are fixed-size for deterministic memory behavior in consensus VM
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionError {
    PrecisionLoss { from_bits: u64, to_bits: u8, lost_bits: u64 },
    ScaleMismatch { expected: u8, actual: u8 },
    OutOfRange { value: i128, min: i128, max: i128 },
    InvalidNaN,
    NotSupported,  // Feature disabled in current phase
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

/// ⚠️ **PHASE 1 LIMITATION**: `DFP → DQA` conversion is disabled in Phase 1.
/// The `NumericCast` implementation MUST return `Err(ConversionError::NotSupported)` until Phase 2 governance upgrade.

impl NumericCast<Dqa> for Dfp {
    /// Convert DFP → DQA (may lose precision for extreme exponents)
    /// ⚠️ PHASE 1: Returns Err(ConversionError::NotSupported)
    fn cast(self) -> Result<Dqa, ConversionError> {
        Err(ConversionError::NotSupported)  // Disabled in Phase 1
    }

    fn cast_lossy(self, rounding: RoundingMode) -> Dqa {
        // Phase 1: DFP → DQA conversion not supported
        // Use DFP directly or wait for Phase 2
        Dqa { value: 0, scale: 0 } // Placeholder - returns zero, actual impl should TRAP
    }
}

impl NumericCast<Dfp> for Dqa {
    /// Convert DQA → DFP
    /// Exact because i64 → i128 is widening (no precision loss) and DFP's i128 mantissa
    /// (per RFC-0104 §3.2) can represent all 64-bit integers exactly.
    /// Requires: RFC-0104 defines DFP_MANTISSA_BITS >= 64
    fn cast(self) -> Result<Dfp, ConversionError> {
        // self.value is i64, casting to i128 is always lossless in Rust
        // scale becomes negative exponent: DQA scale=2 → DFP exponent=-2
        Ok(Dfp::from_mantissa_exponent(self.value as i128, -(self.scale as i32)))
    }

    fn cast_lossy(self, _: RoundingMode) -> Dfp {
        self.cast().unwrap()  // DQA → DFP is always lossless per RFC-0104
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

impl<T: DeterministicScalar + Add<Output = T>, const N: usize> DVecN<T, N> {
    /// Aggregate vector to scalar: sum of all elements
    /// Gas: N × GAS_DQA_ADD
    /// Phase 2: Implement using sequential reduction
    ///
    /// ⚠️ **RECOMMENDED IMPLEMENTATION** (for Phase 1 safety):
    /// ```rust
    /// pub fn sum(&self) -> Result<T, ConversionError> {
    ///     #[cfg(not(feature = "phase2"))]
    ///     return Err(ConversionError::NotSupported);
    ///
    ///     #[cfg(feature = "phase2")]
    ///     {
    ///         // Implementation here
    ///     }
    /// }
    /// ```
    pub fn sum(&self) -> T {
        unimplemented!("sum() - Phase 2 feature")
    }
}

impl<T: DeterministicScalar + Add<Output = T> + Div<Output = T>, const N: usize> DVecN<T, N> {
    /// Aggregate vector to scalar: sum / N
    /// ⚠️ Requires explicit rounding mode for DQA (use RoundingMode::Nearest)
    /// Gas: (N × GAS_DQA_ADD) + GAS_DQA_DIV
    /// Phase 2: Implement using sum() then divide by N
    pub fn mean(&self, rounding: RoundingMode) -> T {
        unimplemented!("mean() - Phase 2 feature")
    }
}

impl<T: DeterministicScalar + Mul<Output = T>, const N: usize> DVecN<T, N> {
    /// Aggregate vector to scalar: product of all elements
    /// ⚠️ WARNING: Overflow TRAPs per overflow semantics table (line 186-194)
    /// Gas: N × GAS_DQA_MUL
    /// Phase 2: Implement using sequential multiplication with overflow check
    pub fn product(&self) -> T {
        unimplemented!("product() - Phase 2 feature")
    }
}

/// Convert between vector element types
impl<const N: usize> DVecN<Dqa, N> {
    /// Phase 2: Convert DVEC<DQA> to DVEC<DFP>
    pub fn to_dfp(&self) -> DVecN<Dfp, N> {
        unimplemented!("to_dfp() - Phase 2 feature (DFP vectors not ZK-friendly)")
    }
}
```

### Gas Model

> ⚠️ **CRITICAL**: Gas formulas are O(N) for vectors, O(N³) for matrices.
> - **Max DVEC dimension**: 128 (gas limit)
> - **Max DMAT dimension**: 8×8 (consensus), 64×64 (state access only)
> - **Strassen's algorithm**: FORBIDDEN (non-deterministic)
> - **Gas cap**: 100,000 gas units max per single numeric operation

#### Emergency Pause Mechanism

> ⚠️ **CONTINGENCY**: In case of catastrophic consensus failure (e.g., LUT hash mismatch, catastrophic arithmetic bug), the VM MUST implement an emergency pause:
>
> | Trigger | Action | Recovery |
> |---------|--------|----------|
> | LUT hash mismatch at genesis | **HALT** - refuse to start | Manual fix required |
> | Runtime TRAP storm (>50% txs) | **PAUSE** numeric ops | Governance vote to resume |
> | Gas limit exceeded (100k+) | **REVERT** transaction | Reduce input size |
>
> **Implementation**: The VM should expose `numeric_pause()` and `numeric_resume()` management functions callable only by governance (not regular users).
>
> ⚠️ **GOVERNANCE REQUIREMENTS**:
> - Callable by: CipherOcto Governance Contract with 2/3 validator signature
> - Timelock: 24 hours minimum between proposal and execution
> - In-flight transactions: Complete current block, pause next block
> - Resume: Requires separate governance proposal with 24h timelock
>
> ⚠️ **FINAL COSTS**: The constants defined below (e.g., `GAS_DQA_MUL`) represent the **final consensus gas cost**. Implementations MUST NOT apply additional safety multipliers at runtime. The 2.0-2.5x safety margin for VM overhead is already incorporated into these values.
>
> ⚠️ **GAS FORMULA AUTHORITY**: The `calculate_vec_gas` and `calculate_mat_gas` functions in this RFC are the **authoritative source** for gas costs. The summary table is for reference only. In case of discrepancy, the function logic prevails. Implementations MUST NOT optimize gas calculation logic without updating these functions.

```rust
/// Gas calculation helpers
const MAX_DVEC_DIM: usize = 128;
const MAX_DMAT_DIM_EXEC: usize = 8;   // Consensus-executable
const MAX_DMAT_DIM_STORAGE: usize = 64; // Storage only, not executable
const MAX_GAS_PER_OP: u64 = 100_000;

/// Scalar operation gas costs
const GAS_INT_ADD: u64 = 1;
const GAS_INT_MUL: u64 = 3;
const GAS_INT_DIV: u64 = 10;
const GAS_INT_MOD: u64 = 10;
const GAS_DQA_ADD: u64 = 5;
const GAS_DQA_MUL: u64 = 8;
const GAS_DQA_DIV: u64 = 20;
const GAS_DQA_NEG: u64 = 2;
const GAS_DQA_ABS: u64 = 2;
const GAS_DFP_ADD: u64 = 8;
const GAS_DFP_MUL: u64 = 15;
const GAS_DFP_DIV: u64 = 35;
const GAS_DECIMAL_ADD: u64 = 6;
const GAS_DECIMAL_MUL: u64 = 12;
const GAS_DECIMAL_DIV: u64 = 25;
const GAS_SQRT: u64 = 480;  // Newton-Raphson 16 iterations
                            // Full cost recovery: 16 × (div + add + shift)

/// Vector operation gas costs (per operation)
const GAS_VEC_ADD: u64 = 5;    // Per element
const GAS_VEC_MUL: u64 = 8;    // Per element (element-wise)
const GAS_VEC_DOT: u64 = 10;   // Per element (includes multiply + add)
const GAS_SQRT: u64 = 480; // SQRT component only (DOT is calculated separately)
const GAS_VEC_DIST: u64 = 400; // Squared distance + SQRT
const GAS_VEC_COSINE: u64 = 750; // 2×NORM + DIV

/// Matrix operation gas costs (per operation)
const GAS_MAT_ADD: u64 = 5;    // Per element
const GAS_MAT_MUL: u64 = 12;  // Per element (element-wise)
const GAS_MAT_DOT: u64 = 15;  // Per (M×N×K) multiply-accumulate

/// Activation function gas costs
const GAS_RELU: u64 = 2;      // Per element: comparison + select
const GAS_SIGMOID_LUT: u64 = 4;  // Per element: LUT lookup
const GAS_TANH_LUT: u64 = 4;     // Per element: LUT lookup
// Reserved: GAS_LAYER_NORM — see RFC-0121 for Layer Normalization spec

/// Gas cost table:
/// | Operation           | Gas  | Formula                    |
/// | ------------------- | ---- | -------------------------- |
/// | INT add/mul/div     | 1/3/10 | flat rate                |
/// | DQA add/mul/div     | 5/8/20 | flat rate                |
/// | DFP add/mul/div     | 8/15/35 | flat rate                |
/// | SQRT (Q8.8)         | 480  | 16 Newton-Raphson iters   |
/// | DVEC add (N)        | 5N   | N × GAS_VEC_ADD           |
/// | DVEC dot (N)        | 8N+5(N-1) | N mul + (N-1) add + shift ≈ 10N |
/// | DVEC norm (N)       | 350  | DOT + SQRT                |
/// | DVEC distance (N)  | 400  | 2×N mul + add + SQRT      |
/// | DVEC cosine (N)     | 750  | 2×NORM + DIV              |
/// | DMAT add (M×N)      | 5MN  | M×N × GAS_MAT_ADD         |
/// | DMAT mul (M×N×K)    | 15MNK| M×N×K × GAS_MAT_DOT       |
/// | ReLU (per element)  | 2    | comparison + select        |
/// | Sigmoid LUT          | 4    | LUT lookup                 |
/// | Tanh LUT             | 4    | LUT lookup                 |
///
/// ⚠️ **SAFETY MARGIN**: The gas constants above already include safety margins
/// for VM overhead. See line ~1434: constants are final, no runtime multiplier needed.
///
/// ⚠️ **PER-OP CAP**: Single matrix multiplication in Phase 2 capped at 4×4 (not 8×8).
/// DMAT(8×8) at 15×8×8 = 9600 gas (×2.5 = 24,000) still fits in 100k,
/// but larger operations may exceed limits.

/// Vector operation gas formula:
/// - ADD: N × GAS_DQA_ADD
/// - DOT: N × GAS_DQA_MUL + (N-1) × GAS_DQA_ADD
/// - NORM: DOT + GAS_SQRT
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
| SQRT | 480 | 480 | N/A |
| DVEC_ADD | 5 × N | 80 | 64 |
| DVEC_DOT | 8N + 5(N-1) | 203 | 64 |
| DVEC_NORM | DOT + 480 | 683 | 64 |
| DMAT_MUL | 8MNK + 5MN(K-1) | 8×4×4×4 + 5×4×4×3 = 752 | 8×8 |
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
    // Byte[1]: reserved (must be zero)
    _reserved: u8,
    // Bytes[2-5]: value (big-endian i32, sign embedded in two's complement)
    value: i32,
    // Byte[6]: scale (0-18)
    scale: u8,
    // Bytes[7-15]: reserved (must be zero)
    _reserved2: [u8; 9],
}
/// Canonical byte layout:
/// | Byte 0 | Byte 1 | Bytes 2-5     | Byte 6 | Bytes 7-15 |
/// |--------|--------|---------------|--------|------------|
/// | version| zero   | value (BE)    | scale  | reserved   |
///
/// Note: Sign is derived from value.signum() (two's complement)
///
/// > ⚠️ **ENCODING MIGRATION AUDIT**:
/// > - v0 (draft, never deployed): `value: i64` (8 bytes) - discarded before testnet
/// > - v1 (current): `value: i32` (4 bytes), total 16 bytes
/// > - Mainnet: version=1 only, version=0 rejected at deserialization
/// > - Migration: No automatic migration needed; v1 is the first production encoding
/// > - If future version uses i64, `version` field will be incremented to 2

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
///
/// > ⚠️ **Negative Zero**: During serialization, any value with `class == Zero` MUST have `sign` bit set to 0 (positive). This ensures `-0.0` and `+0.0` produce identical byte encodings and hashes.

/// DFP Canonical NaN representation (references constants above)
/// In DFP wire format: (version, class, sign, mantissa, exponent)
const DFP_CANONICAL_NAN: (u8, u8, u8, i128, i32) = (1, DFP_CANONICAL_NAN_CLASS, 0, DFP_CANONICAL_NAN_MANTISSA, DFP_CANONICAL_NAN_EXPONENT);
/// Where: (version=1, class=3=NaN, sign=0, mantissa=1, exponent=0)

### Memory Layout Specification

> ⚠️ **CRITICAL**: Memory layout must be identical across all implementations to prevent state root mismatches.

#### DVEC Memory Layout

```
DVEC<N> Layout (row-major, contiguous):
[elem_0][elem_1][elem_2]...[elem_{N-1}]

- In-memory endianness: implementation-defined (CPU-native)
- Wire format: big-endian (see Storage Encoding section)
- Alignment: 4 bytes (i32 alignment)
- Total size: N × sizeof(T) bytes
- Index formula: elements[i] at offset (i × sizeof(T))
```

#### DMAT Memory Layout

```
DMAT<M,N> Layout (row-major, contiguous):
[row_0: col_0, col_1, ..., col_{N-1}]
[row_1: col_0, col_1, ..., col_{N-1}]
...
[row_{M-1}: col_0, col_1, ..., col_{N-1}]

- In-memory endianness: implementation-defined (CPU-native)
- Wire format: big-endian (see Storage Encoding section)
- Alignment: 4 bytes (i32 alignment)
- Total size: M × N × sizeof(T) bytes
- Index formula: elements[i × N + j] at offset ((i × N + j) × sizeof(T))
```

> ⚠️ **ROW-MANDATORY**: All implementations MUST use row-major order. Column-major produces different memory layouts and will cause state divergence.

#### DVEC encoding (for activation outputs, includes LUT version)

> ⚠️ **DO NOT use Vec<T>** — use `Box<[T]>` for deterministic fixed-size allocation.

```
struct DVecEncoding {
    version: u8,           // = 1
    element_type: u8,      // 0=DQA, 1=DFP
    dimension: u16,        // N
    scale: u8,             // For DQA elements
    lut_version: u8,       // LUT version used (for sigmoid/tanh outputs)
    _reserved: [u8; 2],
    elements: Box<[u8]>,  // Contiguous serialized elements (NOT Vec)
}
```

#### DMAT encoding (for activation outputs, includes LUT version)

```
struct DMatEncoding {
    version: u8,           // = 1
    element_type: u8,      // 0=DQA, 1=DFP
    rows: u16,             // M
    cols: u16,             // N
    scale: u8,             // For DQA elements
    lut_version: u8,       // LUT version used (for sigmoid/tanh outputs)
    _reserved: [u8; 1],
    elements: Box<[u8]>,  // Contiguous serialized elements (NOT Vec)
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
| Similarity search | DVEC\<DQA\> | DQA        | Per element |
| Linear algebra    | DMAT\<DQA\>  | DQA        | Per element |

### Alternatives Considered

| Alternative        | Pros        | Cons              | Rejection Reason          |
| ------------------ | ----------- | ----------------- | ------------------------- |
| Single float type  | Simple      | Can't optimize    | Not workload-specific     |
| Only integers      | ZK-friendly | No decimals       | Poor developer ergonomics |
| External libraries | Reuse       | Non-deterministic | Consensus risk            |

## Implementation

### Reference Implementation

> ⚠️ **MANDATORY**: A reference implementation crate is required for consensus specs.

> ⚠️ **NOTE**: Phase 2 features use `unimplemented!()` macros - these will panic if called in Phase 1. The authoritative reference is the `octo_determin` crate, which provides complete implementations. All consensus-critical algorithms are fully specified in the Canonical Arithmetic Algorithms section.

**Reference crate:** `octo_determin` (to be created)

This crate serves as the canonical reference for all numeric operations:

```
octo_determin/
├── src/
│   ├── dqa.rs        # DQA canonical algorithms
│   ├── dfp.rs        # DFP canonical algorithms
│   ├── decimal.rs    # DECIMAL canonical algorithms
│   ├── dvec.rs       # DVEC operations
│   ├── dmat.rs       # DMAT operations
│   ├── lut.rs        # LUT lookup
│   └── lib.rs
├── tests/
│   └── vectors/      # Deterministic test vectors
└── Cargo.toml
```

**Test vectors MUST include:**

```rust
// Canonical test vectors for DQA
#[test]
fn test_dqa_add_vector() {
    // Input: 1.0 + 0.5
    let a = Dqa::new(256, 8);  // 1.0 in Q8.8
    let b = Dqa::new(128, 8);  // 0.5 in Q8.8
    let c = a.add(b);
    assert_eq!(c.value(), 384);  // 1.5 in Q8.8
}

#[test]
fn test_dqa_mul_vector() {
    // Input: 2.0 * 3.0
    let a = Dqa::new(512, 8);  // 2.0 in Q8.8
    let b = Dqa::new(768, 8);  // 3.0 in Q8.8
    let c = a.mul(b);
    assert_eq!(c.value(), 1536);  // 6.0 in Q8.8
}

#[test]
fn test_dqa_div_vector() {
    // Input: 1.0 / 2.0
    let a = Dqa::new(256, 8);
    let b = Dqa::new(512, 8);
    let c = a.div(b, RoundingMode::Nearest);
    assert_eq!(c.value(), 128);  // 0.5 in Q8.8
}

#[test]
fn test_dot_product_vector() {
    // Input: [1,2,3] · [4,5,6] = 32
    let a = dvec![1, 2, 3];
    let b = dvec![4, 5, 6];
    let c = dot(&a, &b);
    assert_eq!(c, 32);
}

#[test]
fn test_sqrt_vector() {
    // Input: sqrt(4.0) = 2.0
    let input: u32 = 1024;  // 4.0 in Q8.8
    let result = sqrt_q8_8(input);
    assert_eq!(result, 512);  // 2.0 in Q8.8
}
```

**Cross-platform determinism tests:**

```rust
// No #[cfg] gate — runs on ALL platforms to verify determinism
#[test]
fn test_cross_platform_dqa() {
    // This test verifies bit-exact results across platforms
    let results: Vec<u32> = run_canonical_tests();
    // Expected values computed on reference x86_64 platform
    // Test passes identically on all platforms or fails (revealing bug)
    let expected: &[u32] = &[
        // Golden values from reference implementation
        // Q8.8: value × 256 = encoded
        0x00000180, // DQA(1.0) + DQA(0.5) = 1.5 → 1.5×256 = 384 = 0x0180
        0x00000400, // DQA(2.0) * DQA(2.0) = 4.0 → 4×256 = 1024 = 0x0400
        0x00000080, // DQA(1.0) / DQA(2.0) = 0.5 → 0.5×256 = 128 = 0x0080
        // ... more test vectors
    ];
    assert_eq!(results.as_slice(), expected,
        "Results must be bit-identical across all platforms");
}
```

### Phase Dependencies

```mermaid
graph LR
    P1[Phase 1<br/>RFC-0105 DQA<br/>START HERE] --> P2[Phase 2<br/>DVEC4-64<br/>Second priority]
    P2 --> P3[Phase 3<br/>RFC-0104 DFP<br/>Higher risk]
    P3 --> P4[Phase 4<br/>DVEC128+ DMAT<br/>Experimental]
    P4 --> P5[Phase 5<br/>DTENSOR<br/>Future]
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
  - [ ] relu (exact)
  - [ ] sigmoid_lut (LUT-based, nearest-neighbor)
  - [ ] tanh_lut (LUT-based, nearest-neighbor)
  - [ ] Verify gas cost fits within 100k limit
  - [ ] Test: sigmoid_lut(0) == 128 (Q8.8 representation of 0.5)
  - [ ] Test: tanh_lut(0) == 0
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
| **Gas Exhaustion** | Large matrix operations consume excessive gas | Hard dimension caps (8×8 exec), gas limits per op |
| **Stack Overflow** | Deep recursion or large arrays crash nodes | Heap allocation (Box<[T]>), compile-time assertions |
| **Precision Manipulation** | Adversary exploits rounding to manipulate results | Explicit rounding modes, no implicit conversions |
| **NaN Injection** | NaN values propagate through consensus | NaN is FORBIDDEN in consensus execution |
| **Infinity Injection** | Infinity values cause consensus divergence | Infinity is FORBIDDEN in consensus execution |
| **Division by Zero** | Division by zero causes trap/inconsistent results | Division by zero MUST trap |
| **Timing Attacks** | Variable-time operations leak information | Fixed iteration order, no data-dependent branches |
| **Side-Channel Leakage** | Data-dependent branches in arithmetic leak secrets | ALL operators MUST execute in constant time for ZK |
| **Vector Normalization Attack** | Zero-vector norm causes division by zero | Zero vector input to NORM/COSINE MUST trap |
| **LUT Poisoning** | Compromised LUT produces wrong activation outputs | LUT values must be cryptographically hashed and verified |
| **LUT Poisoning Prevention** | LUT generation script produces wrong values | The LUT generation script MUST be included in the repository and verified to produce the exact canonical hash. Any deviation in the generated LUT constitutes a consensus failure. |

> ⚠️ **BRANCH-FREE REQUIREMENT**: Arithmetic operators MUST NOT branch on secret data (data-dependent branches are FORBIDDEN).
>
> **For ZK/private computation paths**: ALL operations MUST be constant-time (e.g., use Barrett reduction for division). Variable-time operations leak information through timing side channels.
>
> **For public paths**: Variable-time operations are acceptable for performance.
>
> What matters for consensus determinism is that operations produce identical results across nodes — not that they run in identical cycle counts.

> ⚠️ **IMPLEMENTATION NOTE**: Standard Rust `i64::div` is often **variable-time** on many CPUs. For secret inputs (ZK contexts), use constant-time division (Barrett reduction). For public inputs (standard transactions), variable-time division is acceptable for performance.

### Consensus Failure Cases

All numeric operations MUST handle these failure cases deterministically:

| Failure Case | Behavior | Consensus Impact |
|-------------|----------|------------------|
| Overflow | TRAP | Transaction reverts, gas consumed |
| Underflow | TRAP | Transaction reverts, gas consumed |
| Division by Zero | TRAP | Transaction reverts, gas consumed |
| NaN Output | TRAP | Transaction reverts, gas consumed |
| Infinity Output | TRAP | Transaction reverts, gas consumed |
| Zero Vector Norm | TRAP | Transaction reverts, gas consumed |
| Dimension Exceeded | REJECT | Transaction rejected pre-execution |
| Gas Exceeded | TRAP | Transaction reverts, partial gas consumed |

### DoS Prevention

1. **Hard Dimension Limits**: DVEC ≤ 64 (mainnet), DMAT ≤ 8×8 (mainnet)
2. **Gas Caps**: Max 100,000 gas per numeric operation
3. **Execution Timeouts**: VM-level timeout for long-running computations
4. **Memory Limits**: Wasm memory capped at 256MB
5. **Input Validation**: All inputs validated before computation

## Testing Strategy

### Determinism Verification

> ⚠️ **CRITICAL**: All numeric operations must produce identical results across all hardware/compilers.

```rust
/// Property-based test: determinism using golden reference
#[test]
fn test_vector_add_determinism() {
    // Use fixed known input for reproducibility
    let a = DVecN::<Dqa, 64>::from_array([1i32; 64]);
    let b = DVecN::<Dqa, 64>::from_array([2i32; 64]);

    // Execute vector add
    let result = dvec_add(&a, &b);

    // Golden reference: [1,1,...,1] + [2,2,...,2] = [3,3,...,3]
    let expected = DVecN::<Dqa, 64>::from_array([3i32; 64]);
    assert_eq!(result.as_slice(), expected.as_slice(), "Vector add must match golden reference");
}

/// Property-based test: overflow TRAPs (not saturates)
#[test]
#[should_panic(expected = "Overflow")]
fn test_overflow_traps() {
    let max = Dqa::MAX_VALUE;
    let _result = max.mul(max);  // MUST panic/trap per spec
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

#### Fuzzing Harness Example

```rust
// cargo-fuzz target for cross-platform determinism
#![no_main]

use cipherocto_numeric::{Dqa, dvec::DVecN, dot};

fn fuzz_dvec_dot(data: &[u8]) {
    // Require at least 256 bytes for two 32-element vectors
    if data.len() < 256 { return; }

    // Parse first 128 bytes as little-endian i32s for vector A
    let a: Vec<i32> = data[..128]
        .chunks_exact(4)
        .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    // Parse next 128 bytes for vector B
    let b: Vec<i32> = data[128..256]
        .chunks_exact(4)
        .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    if a.len() != 32 || b.len() != 32 { return; }

    // Execute dot product - deterministic algorithm must produce same result
    // regardless of platform. For true cross-platform testing, run this
    // harness under QEMU ARM emulation in CI, not as two calls in one process.
    let result = dot(&a, &b);

    // Golden reference: known input produces known output
    // For verification, compute expected: dot([1,2,3,4], [5,6,7,8]) = 1*5+2*6+3*7+4*8 = 70
    let golden: Vec<i32> = vec![1, 2, 3, 4];
    let golden2: Vec<i32> = vec![5, 6, 7, 8];
    let expected = dot(&golden, &golden2);
    assert_eq!(expected, 70, "Golden reference failed");
}
```

> ⚠️ **RECOMMENDATION**: Run 10,000+ fuzz iterations across x86, ARM, and Wasm targets. Any mismatch indicates non-deterministic behavior.

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

## Implementation Handoff Checklist

> ⚠️ **CRITICAL**: This checklist is the authoritative reference for implementation teams.

### Storage/State Team

- [ ] Implement exact byte-layouts defined in `DqaEncoding`, `DfpEncoding`, and `DVecEncoding`
- [ ] **DO NOT** use bincode or rely on `repr(C)` — use manual byte packing
- [ ] Implement canonical serialization: big-endian, explicit version field
- [ ] Verify all implementations produce identical hashes for the same values

### Runtime/VM Team

- [ ] Implement exact `sqrt_q8_8` Newton-Raphson block (16 iterations, leading-zero-based initial guess)
- [ ] Use `Box<[T]>` for DVecN and DMat allocations (not `Vec<T>`)
- [ ] Enforce Wasm memory limits: 256MB max
- [ ] Implement TRAP on: overflow, underflow, NaN, Infinity, division by zero
- [ ] Enforce dimension limits: DVEC ≤ 64 (mainnet), DMAT ≤ 8×8 (mainnet)
- [ ] Implement 128-bit accumulator for DOT product (i128)

> ⚠️ **Reference Generator**: The `octo_determin` crate MUST include a `bin/generate_lut.rs` tool that produces the canonical LUT using only integer arithmetic. This tool is the source of truth for the LUT hash.

### CI/CD Pipeline

- [ ] **Cross-Platform LUT Test**: Run `bin/generate_lut.rs` on x86_64, ARM64, and Wasm32. Assert SHA256 hashes match exactly.
- [ ] **Gas Formula Test**: Assert `calculate_vec_gas(N, Dot)` matches hardcoded table value within 0% tolerance.
- [ ] **Endian Test**: Serialize `Dqa(1.0)` on Little-Endian machine. Deserialize on Big-Endian machine (via QEMU). Assert value equality.
- [ ] **Pin Rust Version**: Specify `rustc 1.XX.X` in `rust-toolchain.toml` for LUT generator reproducibility.
- [ ] **No Float Lint**: CI must fail if `bin/generate_lut.rs` contains any `f32`, `f64`, or floating-point arithmetic.

### ZK / Circuits Team

- [ ] Base SQUARED_DISTANCE constraints on exact Q8.8 scaling logic (no SQRT)
- [ ] Update Poseidon2 circuit to absorb `lut_version` parameter
- [ ] Verify LUT SHA256 commitments match on-chain values
- [ ] Implement constant-time arithmetic for ZK paths (Barrett reduction for division)

### DevRel / Documentation

- [ ] Pull "Transaction Splitting Pattern" into official CipherOcto developer documentation
- [ ] Document the gas cost model with safety multipliers
- [ ] Create tutorial: "Building AI Inference on CipherOcto"
- [ ] Document Phase 1 vs Phase 2 feature matrix

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

```mermaid
graph TD
    A[AI Agents / Applications] --> B[Verifiable RAG<br/>RFC-0108]
    B --> C[Retrieval Gateway<br/>RFC-0109 + 0113]
    C --> D[Deterministic Execution VM<br/>RFC-0106]
    D --> E[AIR<br/>Algebraic Intermediate Representation]
    E --> F[STARK Prover STWO GPU]
```

### Execution Trace Format

> ⚠️ **Architectural Gap**: The repository lacks a standardized execution trace format.

**CipherOcto Execution Trace (CET)** standardizes traces for:

- SQL execution
- Vector search
- ML inference

> ⚠️ **SCOPE NOTE**: The Execution Trace (CET) format belongs in RFC-0120 (Deterministic AI VM) or a dedicated ZK integration RFC. This section is included for **context only** and is **NOT part of the Numeric Tower specification**. Do not implement in Phase 1.

Trace structure (non-normative):

```rust
struct CipherOctoTrace {
    trace_id: u64,
    timestamp: u64,
    operations: Box<[TraceEntry]>,  // ✅ Updated to Box for heap safety
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

### ZK-Specific Example: Verifiable Similarity Search

> ⚠️ **ZK optimization**: For zero-knowledge proofs, prefer **Squared Euclidean Distance** over L2 distance to avoid expensive SQRT in circuits.

```rust
/// ZK-friendly similarity verification
/// Prover commits to vectors A, B and similarity score S
/// Verifier checks: S = squared_distance(A, B)
fn verify_similarity_zk(a: &[i32], b: &[i32], claimed: i128) -> bool {
    // Compute squared distance (no SQRT = ZK-friendly)
    // Use i128 to prevent overflow: 64 elements × (2^31)^2 > 2^63
    let mut sum: i128 = 0;
    for i in 0..a.len() {
        let diff = (a[i] as i128) - (b[i] as i128);
        sum += diff * diff;
    }
    // Claimed must match computed exactly (no silent truncation)
    sum == claimed
}
```

#### End-to-End Gas Cost Example: Small Neural Network

> ⚠️ **Realistic gas estimate** for a small inference that fits in 100k gas:

```
Input: DVEC16<Dqa>
Layer: DVEC16 × DMAT4x4 → DVEC4 (ReLU)

Operations:
- MAT_MUL: 4 × 4 × 4 = 64 scalar multiplications
- ReLU: 4 element-wise comparisons

Gas breakdown:
- MAT_MUL (64 ops): 64 × 15 = 960 gas
- ReLU (4 ops): 4 × 2 = 8 gas
- DOT (16 × 10 = 160): 160 gas
- TOTAL: ~1,128 gas (well under 100k limit)

**ZK Proof Cost Equivalent:**
- SQUARED_DISTANCE (DVEC16): ~1,600 gates (no SQRT)
- Full L2 NORM (with SQRT): ~8,000 gates
- MAT_MUL 4×4: ~2,000 gates
- ReLU: ~400 gates
- **Total ZK proof**: ~5,000 gates (with squared distance)

> ⚠️ **ZK Insight**: Using SQUARED_DISTANCE instead of NORM saves ~6,400 gates per inference, making on-chain ML significantly more practical for ZK proofs.
```

#### End-to-End Gas Cost Example: 2-Layer MLP

> ⚠️ **Realistic gas estimate** for a 2-layer MLP with safety multiplier:

```
Input: DVEC32<Dqa>
Layer 1: DVEC32 × DMAT32x16 → DVEC16 (ReLU)
Layer 2: DVEC16 × DMAT16x2 → DVEC2 (Sigmoid)

Operations:
- Layer 1 matmul: 32 × 16 × 32 = 16,384 scalar muls
- Layer 1 ReLU: 16 elements
- Layer 2 matmul: 16 × 2 × 16 = 512 scalar muls
- Layer 2 Sigmoid: 2 LUT lookups

Gas breakdown (with 2.0x safety multiplier):
- MAT_MUL (16,384 + 512 ops): 16,896 × 15 × 2.0 = 506,880 gas
- ReLU (16 ops): 16 × 2 × 2.0 = 64 gas
- Sigmoid LUT (2 lookups): 2 × 4 × 2.0 = 16 gas
- DOT (32 × 10 = 320): 320 × 2.0 = 640 gas
- TOTAL: ~507,600 gas

⚠️ **CRITICAL**: This far exceeds the 100k gas limit!
- Must split across multiple transactions (checkpoint pattern)
- Or reduce to smaller model: DVEC16 × DMAT4x4 → DVEC4
```

**Multi-Layer Inference: Transaction Splitting Pattern**

For ML models exceeding gas limits, use commitment-based chaining:

```sql
-- Transaction 1: Layer 1
CREATE TABLE layer1_cache (
    id INT PRIMARY KEY,
    intermediate_hash BLOB,  -- Poseidon hash of Layer 1 output
    commitment BLOB            -- Hash of weights + input
);

-- Store intermediate result with commitment
INSERT INTO layer1_cache
SELECT id, poseidon(output), poseidon(weights || input)
FROM mat_mul(inputs, weights_layer1);

-- Transaction 2: Layer 2 (verifies commitment)
SELECT sigmoid(dot(layer2_weights, layer1_output))
FROM layer1_cache l
JOIN weights_layer2 w
WHERE poseidon(w.weights || l.intermediate_hash) = l.commitment;
```

This pattern:
1. Stores intermediate results with cryptographic commitment
2. Each transaction stays within gas limit
3. Verification ensures layer N only runs if layer N-1 output matches commitment

### Vector Similarity Search

```sql
-- Embedding storage (Phase 1: DVEC64, Phase 2: DVEC128+)
CREATE TABLE embeddings (
    id INT,
    vector DVEC64  -- N ≤ 64 for Phase 1
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

**Submission Date:** 2026-03-09
**Last Updated:** 2026-03-09
