# RFC-0104: Deterministic Floating-Point Abstraction (DFP)

## Status

Draft

## Summary

This RFC introduces Deterministic Floating-Point (DFP) — a numeric abstraction that provides floating-point developer ergonomics while guaranteeing bit-identical execution across all nodes participating in CipherOcto consensus. DFP enables floating-point arithmetic in blockchain state transitions and deterministic query execution without sacrificing reproducibility.

The design introduces a two-tier numeric model: non-deterministic FLOAT/DOUBLE for analytics, and deterministic DFP for consensus-critical computations. Type mixing requires explicit casting to prevent ambiguous semantics.

> ⚠️ **EXPERIMENTAL WARNING**: DFP consensus usage is **experimental and carries high technical risk**. Most production blockchains avoid floating-point in consensus paths entirely. DFP should be considered **alpha-stage technology** until:
>
> - Hardware verification is proven robust over years of production
> - Comprehensive test vectors are validated across architectures
> - Transcendental functions (Mission 1b) are implemented
> - Real-world benchmarks demonstrate acceptable performance
>
> **Recommendation**: Start with software-only path. Use hardware fast-path only after extensive validation.

## Motivation

### Problem Statement

IEEE-754 floating-point arithmetic is non-deterministic across hardware architectures. Sources of nondeterminism include:

- CPU extended precision registers (x86 80-bit vs ARM 64-bit)
- Fused multiply-add (FMA) instruction differences
- Compiler optimization variations
- Platform-specific math library implementations
- Rounding mode inconsistencies

For example, `0.1 + 0.2` can produce slightly different bit patterns on different systems, causing state divergence in replicated state machines.

### Current State

Most blockchain systems avoid floating-point entirely:

- Bitcoin: Integer-only arithmetic
- Ethereum: No native float types
- Solana: Integer primitives
- Cosmos SDK: Fixed-point decimals

This creates developer friction for AI, statistical, and scientific workloads that naturally require floating-point semantics.

### Desired State

CipherOcto should support:

- Deterministic float arithmetic for consensus
- Standard SQL float types for analytics
- Explicit type boundaries with no silent conversions
- Hardware acceleration for compliant nodes
- Software fallback for non-compliant nodes

### Use Case Link

- [Hybrid AI-Blockchain Runtime](../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Decentralized Mission Execution](../docs/use-cases/decentralized-mission-execution.md)

## Specification

### Two-Tier Numeric Model

```
Tier 1 — Non-Deterministic (Analytics)
├── FLOAT   — 32-bit IEEE-754
├── DOUBLE  — 64-bit IEEE-754
└── Use: Local queries, ML inference, vector search

Tier 2 — Deterministic (Consensus)
├── DFP     — Deterministic Floating-Point (this RFC)
├── DECIMAL — Deterministic Fixed-Point
└── Use: Blockchain state, smart contracts, replicated queries
```

### Data Structures

```rust
/// DFP class tag to avoid encoding collisions with numeric values
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfpClass {
    /// Normal numeric value
    Normal,
    /// Positive infinity
    Infinity,
    /// Not a Number
    NaN,
    /// Zero (sign preserved)
    Zero,
}

/// Deterministic Floating-Point representation
/// Uses tagged representation to avoid encoding collisions
pub struct Dfp {
    /// Class tag (Normal, Infinity, NaN, Zero)
    class: DfpClass,
    /// Sign bit (0 = positive, 1 = negative)
    sign: bool,
    /// Mantissa (only valid for Normal class)
    mantissa: i128,
    /// Binary exponent (only valid for Normal class)
    exponent: i32,
}

impl Dfp {
    /// Create a normal DFP value
    pub fn new(mantissa: i128, exponent: i32) -> Self {
        Self {
            class: DfpClass::Normal,
            sign: mantissa < 0,
            mantissa: mantissa.abs(),
            exponent,
        }
    }

    /// Create infinity
    pub fn infinity(sign: bool) -> Self {
        Self {
            class: DfpClass::Infinity,
            sign,
            mantissa: 0,
            exponent: 0,
        }
    }

    /// Create canonical NaN
    pub fn nan() -> Self {
        Self {
            class: DfpClass::NaN,
            sign: false,
            mantissa: 0,
            exponent: 0,
        }
    }

    /// Create zero with sign preservation
    pub fn zero(sign: bool) -> Self {
        Self {
            class: DfpClass::Zero,
            sign,
            mantissa: 0,
            exponent: 0,
        }
    }

    /// Create from f64 (with canonical rounding)
    pub fn from_f64(value: f64) -> Self;

    /// Convert to f64 (lossy)
    pub fn to_f64(&self) -> f64;

    /// Arithmetic operations
    pub fn add(self, other: Self) -> Self;
    pub fn sub(self, other: Self) -> Self;
    pub fn mul(self, other: Self) -> Self;
    pub fn div(self, other: Self) -> Self;
}

/// DFP encoding for storage/consensus
/// Uses big-endian canonical encoding to avoid cross-platform ambiguity
#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
pub struct DfpEncoding {
    /// Class tag (0=Normal, 1=Infinity, 2=NaN, 3=Zero)
    class: u8,
    /// Sign bit
    sign: u8,
    /// Reserved for alignment
    _reserved: [u8; 2],
    /// Mantissa in big-endian
    mantissa: i128,
    /// Exponent in big-endian
    exponent: i32,
}

impl DfpEncoding {
    /// Serialize DFP to canonical big-endian encoding
    pub fn from_dfp(dfp: &Dfp) -> Self {
        Self {
            class: match dfp.class {
                DfpClass::Normal => 0,
                DfpClass::Infinity => 1,
                DfpClass::NaN => 2,
                DfpClass::Zero => 3,
            },
            sign: if dfp.sign { 1 } else { 0 },
            _reserved: [0; 2],
            mantissa: dfp.mantissa.to_be(),
            exponent: dfp.exponent.to_be(),
        }
    }

    /// Deserialize from canonical encoding
    pub fn to_dfp(&self) -> Dfp {
        Dfp {
            class: match self.class {
                0 => DfpClass::Normal,
                1 => DfpClass::Infinity,
                2 => DfpClass::NaN,
                3 => DfpClass::Zero,
                _ => DfpClass::NaN, // Invalid encoding
            },
            sign: self.sign != 0,
            mantissa: i128::from_be(self.mantissa),
            exponent: i32::from_be(self.exponent),
        }
    }
}

/// Node capability flags for DFP execution
pub struct NodeCapabilities {
    /// Software deterministic path (always available)
    pub dfp_soft: bool,
}
```

> ⚠️ **ARCHITECTURE CHANGE**: Hardware fast-path has been **removed**. DFP now uses **pure integer arithmetic** only. The CPU accelerates 128-bit integer operations, not floating-point. This ensures true determinism across all hardware.

### APIs/Interfaces

```rust
impl Dfp {
    /// Arithmetic operations using pure integer math
    pub fn add(self, other: Self) -> Self;
    pub fn sub(self, other: Self) -> Self;
    pub fn mul(self, other: Self) -> Self;
    pub fn div(self, other: Self) -> Self;
}
```

#### Addition Algorithm (Deterministic Specification)

For deterministic execution, all implementations must use this exact algorithm:

```
DFP_ADD(a, b):
    1. If a.class != Normal or b.class != Normal:
       → Handle special values per class rules
    2. Align exponents:
       → diff = a.exponent - b.exponent
       → Shift mantissa with smaller exponent by |diff|
    3. Add/Subtract mantissas (respecting sign):
       → result_mantissa = a.mantissa +/- b.mantissa
    4. Apply round-to-nearest-even to precision cap (113 bits)
    5. Normalize: ensure mantissa is odd (mantissa % 2 == 1)
    6. Return result
```

#### Multiplication Algorithm

```
DFP_MUL(a, b):
    1. Handle special values (NaN, Infinity, Zero)
    2. result_sign = a.sign XOR b.sign
    3. result_exponent = a.exponent + b.exponent
    4. result_mantissa = a.mantissa * b.mantissa
    5. Apply RNE rounding to 113-bit precision cap
    6. Normalize
    7. Return
```

#### Division Algorithm

```
DFP_DIV(a, b):
    1. Handle special values
    2. result_sign = a.sign XOR b.sign
    3. result_exponent = a.exponent - b.exponent
    4. result_mantissa = a.mantissa / b.mantissa (long division)
    5. Apply RNE rounding to 113-bit precision cap
    6. Normalize
    7. Return
```

#### Square Root Algorithm

```
DFP_SQRT(a):
    1. Handle special values (NaN, Infinity, Zero, negative)
    2. Initial approximation: integer_sqrt(a.mantissa) << (a.exponent / 2)
    3. Newton-Raphson: 16 fixed iterations
       x_{n+1} = (x_n + a / x_n) / 2
    4. Apply RNE rounding each iteration
    5. Normalize
    6. Return
```

### Expression VM opcodes

pub enum VmOpcode {
// ... existing opcodes
OP_DFP_ADD,
OP_DFP_SUB,
OP_DFP_MUL,
OP_DFP_DIV,
}

/// Executor mode
pub enum ExecutorMode {
/// Standard SQL execution
Standard,
/// Deterministic execution (DFP required)
Deterministic,
}

````

### Canonicalization Rules

After every operation, DFP values are normalized to canonical form:

```rust
/// Canonical normalization algorithm
/// Uses O(1) trailing_zeros for constant-time normalization
/// Only applies to Normal class values
fn normalize(dfp: &mut Dfp) {
    // Only normalize normal values
    if dfp.class != DfpClass::Normal {
        return; // Infinity, NaN, Zero already in canonical form
    }

    // Handle zero - class Zero with sign preserved
    if dfp.mantissa == 0 {
        dfp.class = DfpClass::Zero;
        return;
    }

    // O(1) normalization using trailing zeros count
    // After shifting, mantissa is guaranteed odd (or zero), so no further loop needed
    let trailing = dfp.mantissa.unsigned_abs().trailing_zeros() as i32;
    dfp.mantissa >>= trailing;
    dfp.exponent = dfp.exponent.saturating_add(trailing);

    // Handle overflow - convert to Infinity class
    if dfp.exponent > DFP_MAX_EXPONENT {
        dfp.class = DfpClass::Infinity;
        // Sign is preserved
        dfp.mantissa = 0;
        dfp.exponent = 0;
    }
}
````

### Rounding Rules

All operations use round-to-nearest-even (matches IEEE-754 default):

| Input | Rounded To |
| ----- | ---------- |
| 1.5   | 2          |
| 2.5   | 2          |
| 3.5   | 4          |

**Multi-step expressions:** RNE is applied after **every individual operation**. There are no fused paths. For example, `(a + b) * c` is computed as: `(a + b)` → round → then multiply → round. This ensures deterministic results regardless of evaluation order.

### Special Values

DFP uses tagged representation to avoid encoding collisions:

| Special Value | Class    | Sign | Mantissa | Exponent | Behavior                  |
| ------------- | -------- | ---- | -------- | -------- | ------------------------- |
| NaN           | NaN      | -    | -        | -        | Canonical NaN, propagates |
| +Infinity     | Infinity | 0    | -        | -        | Clamps to MAX_DFP         |
| -Infinity     | Infinity | 1    | -        | -        | Clamps to MIN_DFP         |
| +0.0          | Zero     | 0    | -        | -        | Distinct from -0.0        |
| -0.0          | Zero     | 1    | -        | -        | Distinct from +0.0        |
| Normal        | Normal   | 0/1  | i128     | i32      | Standard value            |

**Conversion from f64:**

- NaN → canonical NaN (class=NaN)
- +Infinity → Infinity (class=Infinity, sign=0)
- -Infinity → Infinity (class=Infinity, sign=1)
- +0.0 → Zero (class=Zero, sign=0)
- -0.0 → Zero (class=Zero, sign=1)
- Subnormal → normalized to DFP precision (class=Normal)

### Range and Precision

DFP provides higher precision than IEEE-754 double:

| Characteristic | DFP         | IEEE-754 Double |
| -------------- | ----------- | --------------- |
| Mantissa bits  | 128         | 53 (implicit)   |
| Exponent bits  | 32          | 11              |
| Decimal digits | ~38         | ~15-17          |
| Exponent range | ±1023       | ±1023           |
| MAX value      | ~1.8×10³⁰⁸  | ~1.8×10³⁰⁸      |
| MIN positive   | ~2.2×10⁻³⁰⁸ | ~2.2×10⁻³⁰⁸     |

**Precision cap:** To ensure stable f64→DFP→f64 round-trips, mantissa is capped at **113 bits** (matching IEEE quad precision). Values requiring more precision are rounded.

**Canonical mantissa invariant:** For all Normal values: `mantissa % 2 == 1` (mantissa is always odd). This ensures unique canonical encoding.

**Constants:**

```rust
pub const DFP_MAX_EXPONENT: i32 = 1023;
pub const DFP_MIN_EXPONENT: i32 = -1074;

/// Maximum finite DFP value
pub const DFP_MAX: Dfp = Dfp {
    class: DfpClass::Normal,
    sign: false,
    mantissa: i128::MAX,
    exponent: DFP_MAX_EXPONENT,
};

/// Minimum positive DFP value
pub const DFP_MIN: Dfp = Dfp {
    class: DfpClass::Normal,
    sign: false,
    mantissa: 1,
    exponent: DFP_MIN_EXPONENT,
};

/// Canonical NaN (all NaN values collapse to this)
pub const DFP_CANONICAL_NAN: Dfp = Dfp {
    class: DfpClass::NaN,
    sign: false,
    mantissa: 0,
    exponent: 0,
};

/// Positive infinity
pub const DFP_POS_INFINITY: Dfp = Dfp {
    class: DfpClass::Infinity,
    sign: false,
    mantissa: 0,
    exponent: 0,
};

/// Negative infinity
pub const DFP_NEG_INFINITY: Dfp = Dfp {
    class: DfpClass::Infinity,
    sign: true,
    mantissa: 0,
    exponent: 0,
};
```

### SQL Integration

```sql
-- Deterministic table
CREATE TABLE trades (
    id INTEGER PRIMARY KEY,
    price DFP NOT NULL,
    quantity DFP,
    executed_at TIMESTAMP
);

-- Deterministic view (enforces DFP)
CREATE DETERMINISTIC VIEW v_portfolio AS
SELECT
    price * quantity AS total
FROM trades;

-- Explicit casting (required for mixed arithmetic)
SELECT
    CAST(price AS DFP) * CAST(quantity AS DFP)
FROM trades;

-- Error: cannot mix DFP and FLOAT
SELECT price * quantity FROM trades;
-- Error: cannot mix DFP and FLOAT
```

### CAST Safety in Deterministic Contexts

> ⚠️ **CRITICAL**: Casting FLOAT/DOUBLE to DFP in deterministic contexts is **FORBIDDEN** because FLOAT values may differ across platforms.

```sql
-- FORBIDDEN: FLOAT values may be non-deterministic across nodes
SELECT CAST(price AS DFP) FROM trades;  -- Error in deterministic context

-- Solution: Use DFP from the start
CREATE TABLE trades (price DFP NOT NULL);
```

**Rationale:** If Node A stores `0.30000000000000004` in a FLOAT column and Node B stores `0.3` for the same logical value, casting to DFP produces different results, breaking consensus determinism.

### Deterministic Context Rules

Inside deterministic execution contexts (blockchain state transitions, deterministic views):

```
FLOAT  → FORBIDDEN
DOUBLE → FORBIDDEN
DFP    → ALLOWED
DECIMAL → ALLOWED
INTEGER → ALLOWED
INT    → ALLOWED (implicit to DFP)
```

### Execution Paths

```
DFP Operation
    │
    └─[Software Path]─→ Deterministic 128-bit integer arithmetic → DFP
```

> ⚠️ **ARCHITECTURE**: Hardware fast-path has been **removed**. DFP uses **pure integer arithmetic** (i128 operations) only. The CPU accelerates 128-bit integer operations, not floating-point. This ensures true determinism across x86, ARM, RISC-V, and virtualized environments.

### Execution Verification

DFP uses software-only deterministic arithmetic. Verification ensures the implementation matches the specification:

> ⚠️ **NOTE**: Verification suite is abbreviated. Full verification requires 265+ test vectors across edge cases, subnormal handling, and cross-platform validation. See `determ/verify/test_vectors.rs` for complete suite.

```rust
/// Verification test vectors
/// Full suite: ~265 vectors covering arithmetic, special values, overflow/underflow
const VERIFICATION_TESTS: &[(&str, f64)] = &[
    ("0.1 + 0.2", 0.3),
    ("sqrt(2)", 1.4142135623730951),
    ("1e300 * 1e-300", 1.0),
];
```

> **Note**: `sin`, `cos`, `log`, `exp` are excluded from initial verification because transcendental functions are deferred to Mission 1b.

#### Continuous Verification

To ensure ongoing deterministic behavior:

| Mechanism                | Description                                   |
| ------------------------ | --------------------------------------------- |
| Periodic re-verification | Re-run probe tests every N blocks             |
| Cross-node spot-checks   | Randomly compare DFP results across nodes     |
| Divergence alerts        | Flag and investigate unexpected differences   |
| Slashing conditions      | Penalize nodes producing inconsistent results |

#### Compiler Flags for Reproducibility

To ensure deterministic software-path execution, nodes must compile with specific flags:

| Platform | Required Flags                                      |
| -------- | --------------------------------------------------- |
| x86      | `-Cf target-feature=+sse2` (disable x87)            |
| ARM      | Standard AAPCS (deterministic by default)           |
| All      | `-C overflow-checks=false` (wrap semantics defined) |

> ⚠️ **Virtualized environments**: Hardware fast-path is permitted only on bare-metal nodes. Cloud VMs and containers must use the software path.

### Storage Encoding

DFP values serialize deterministically:

```rust
impl Serialize for Dfp {
    fn serialize(&self) -> Vec<u8> {
        // Canonical big-endian encoding for Merkle compatibility
        // Uses DfpEncoding internally
        let encoding = DfpEncoding::from_dfp(self);
        let mut bytes = vec![];
        bytes.push(encoding.class);
        bytes.push(encoding.sign);
        bytes.extend_from_slice(&encoding.mantissa.to_be_bytes());
        bytes.extend_from_slice(&encoding.exponent.to_be_bytes());
        bytes
    }
}
```

> Note: Uses big-endian encoding for cross-platform consistency. This matches `DfpEncoding` for consensus safety.

### Constraints

- **Determinism**: All nodes must produce bit-identical DFP results
- **Explicit types**: No implicit FLOAT → DFP conversion
- **Type mixing**: Forbidden without explicit CAST
- **Canonical form**: Every value has exactly one representation
- **Range**: Exponent bounded to prevent overflow/underflow
- **Sign handling**: -0.0 preserved for scientific accuracy; normalized to +0.0 only when mathematically equivalent
- **Gas cost**: DFP operations must be charged higher than integer operations (see Gas/Fee Modeling)

### Gas/Fee Modeling

DFP software arithmetic is slower than native integer operations due to normalization. The VM must charge accordingly:

| Operation    | Relative Gas Cost   |
| ------------ | ------------------- |
| INT_ADD      | 1x (baseline)       |
| DFP_ADD      | 6-10x               |
| DFP_MUL      | 10-15x              |
| DFP_DIV      | 25-40x              |
| DFP_SQRT     | 20-30x              |
| DFP_FROM_I64 | 2x                  |
| DFP_TO_I64   | 2x                  |
| DFP_FROM_F64 | 4-6x (canonicalize) |
| DFP_TO_F64   | 3-4x (roundtrip)    |

**Rationale:** Software DFP uses 128-bit arithmetic with normalization loops. Division and square root are most expensive due to iterative algorithms. f64 conversion requires canonicalization.

### Deterministic Ordering

DFP defines total ordering for sorting and comparison operations:

| Order | Class     | Sign | Mantissa  | Exponent  |
| ----- | --------- | ---- | --------- | --------- |
| 1     | -Infinity | 1    | -         | -         |
| 2     | Zero      | 1    | -         | -         |
| 3     | Normal    | 1    | ascending | ascending |
| ...   | ...       | ...  | ...       | ...       |
| N-2   | Normal    | 0    | ascending | ascending |
| N-1   | Zero      | 0    | -         | -         |
| N     | +Infinity | 0    | -         | -         |
| N+1   | NaN       | -    | -         | -         |

**Total ordering:** `-Infinity < -0.0 < negative values < +0.0 < positive values < +Infinity < NaN`

> Note: All comparisons with NaN return false. For sorting, NaN is placed last.

**Sorting algorithm:** For negative Normal values, comparison must invert mantissa ordering (ascending mantissa = descending value). Implementation:

```
compare(a, b):
    1. Compare class (Infinity < Normal < Zero < NaN)
    2. For same class: compare sign (negative < positive)
    3. For Normal with same sign:
       - If negative: compare (exponent, mantissa) descending
       - If positive: compare (exponent, mantissa) ascending
```

### Error Handling

| Scenario                       | Behavior                    |
| ------------------------------ | --------------------------- |
| FLOAT in deterministic context | Compile error               |
| DFP \* FLOAT                   | Compile error (use CAST)    |
| DFP overflow                   | Clamp to MAX/MIN            |
| DFP underflow                  | Clamp to 0.0                |
| Hardware verification fail     | Silent fallback to software |

## Rationale

### Why This Approach?

The two-tier model balances:

- **Performance**: Analytics use fast native floats
- **Safety**: Consensus requires explicit determinism
- **Compatibility**: Standard SQL float types preserved

### Risk Assessment

This RFC represents an ambitious attempt to bring floating-point into consensus-critical execution. Industry practice overwhelmingly avoids this due to significant challenges:

| Risk                          | Severity | Mitigation                                                        |
| ----------------------------- | -------- | ----------------------------------------------------------------- |
| Hardware determinism fragile  | High     | Software-only path default; hardware opt-in only after validation |
| Software fallback 3-6× slower | Medium   | Acceptable for limited consensus operations                       |
| Transcendental functions      | High     | Deferred to Mission 1b; full AI workloads limited until then      |
| Verification probe coverage   | Medium   | Expand test vectors; continuous cross-node checks                 |
| Exponent overflow/underflow   | Medium   | Align with IEEE double range (±1023)                              |
| NaN/denormal handling         | Medium   | Canonical forms; clear documentation                              |

**Industry Comparison:**

| Chain            | Consensus FP? | Approach              |
| ---------------- | ------------- | --------------------- |
| Ethereum EVM     | No            | 256-bit integers only |
| Solana SVM       | Emulated      | Software FP (slow)    |
| Cosmos SDK       | No            | Fixed-point decimals  |
| CipherOcto (DFP) | Yes (opt-in)  | Custom binary FP      |

This RFC makes CipherOcto a potential outlier. The experimental warning reflects genuine technical risk.

### Alternatives Considered

| Alternative            | Pros          | Cons                                    | Rejection Reason                         |
| ---------------------- | ------------- | --------------------------------------- | ---------------------------------------- |
| Auto-convert FLOAT→DFP | Seamless      | Hidden semantics, consensus risk        | Unacceptable for safety-critical systems |
| Deprecate FLOAT        | Clean         | Breaks SQL compatibility, vector search | Unrealistic — fighting the ecosystem     |
| Fixed-point only       | Deterministic | Poor scientific workloads               | Loses the AI-native value proposition    |
| BigFloat               | Precise       | Extremely slow, VM impractical          | Performance unacceptable                 |
| IEEE-754 only          | Fast          | Non-deterministic                       | Unsafe for consensus                     |

### Trade-offs

| Priority     | Trade-off                       |
| ------------ | ------------------------------- |
| Prioritize   | Determinism, explicit types     |
| Deprioritize | Implicit conversion convenience |
| Accept       | 3-6x slower software fallback   |
| Accept       | Explicit casting required       |

## Implementation

### Mission 1: DFP Core Type

- Location: `determ/dfp.rs`
- Acceptance criteria:
  - [ ] DFP struct with mantissa/exponent
  - [ ] Canonical normalization
  - [ ] Arithmetic: add, sub, mul, div
  - [ ] Round-to-nearest-even
  - [ ] Special values: NaN, ±Infinity, ±0.0 handling
  - [ ] Range bounds and overflow/underflow clamping
  - [ ] From/To f64 conversion
  - [ ] Serialization
  - [ ] sqrt (square root) - Newton-Raphson with 16 iterations
- Estimated complexity: Medium

### Mission 1b: Additional Transcendental Functions (Future Phase)

Deterministic transcendental functions require fixed-iteration algorithms with bounded precision:

| Phase | Functions        | Algorithm                      | Status |
| ----- | ---------------- | ------------------------------ | ------ |
| 1b.1  | cbrt             | Newton-Raphson (16 iterations) | Future |
| 1b.2  | sin, cos, tan    | CORDIC or Chebyshev (bounded)  | Future |
| 1b.3  | log, log2, log10 | Series expansion               | Future |
| 1b.4  | exp, pow         | Series expansion               | Future |

**Determinism requirements for transcendental functions:**

- Fixed iteration count (no early termination)
- Deterministic initial approximations
- Bounded precision guarantees
- Round-to-nearest-even at each step

**Note:** sqrt is included in Mission 1 to support basic AI workloads (e.g., distance calculations, normalization). Full transcendental support is deferred to future phases.

### Mission 2: DataType Integration

- Location: `src/parser/ast.rs`, `src/parser/statements.rs`
- Acceptance criteria:
  - [ ] Add `DataType::DeterministicFloat` variant
  - [ ] SQL parser accepts `DFP` type
  - [ ] CAST(... AS DFP) parsing
  - [ ] Type error for FLOAT in deterministic context
- Estimated complexity: Low

### Mission 3: Expression VM Opcodes

- Location: `src/vm/`
- Acceptance criteria:
  - [ ] OP_DFP_ADD, OP_DFP_SUB, OP_DFP_MUL, OP_DFP_DIV
  - [ ] Compile error on DFP \* FLOAT without CAST
  - [ ] DeterministicExecutor mode
- Estimated complexity: Medium

### Mission 4: Hardware Verification

- Location: `determ/probe.rs`
- Acceptance criteria:
  - [ ] DeterministicFloatProbe test suite
  - [ ] Node capability advertisement
  - [ ] Automatic fallback on verification failure
  - [ ] Comprehensive test vectors (edge cases, cross-platform)
- Estimated complexity: Low

#### Test Vector Suite

Comprehensive verification requires extensive test vectors:

| Category           | Test Cases                       | Count |
| ------------------ | -------------------------------- | ----- |
| Basic arithmetic   | add, sub, mul, div edge cases    | ~50   |
| Special values     | NaN, ±Infinity, ±0.0             | ~20   |
| Overflow/underflow | MAX, MIN boundaries              | ~30   |
| Precision loss     | Decimal precision comparisons    | ~25   |
| Transcendental     | sqrt, sin, cos, log, exp         | ~40   |
| Cross-platform     | Same inputs across architectures | ~100  |

### Mission 5: Consensus Integration

- Location: `src/storage/`, `src/consensus/`
- Acceptance criteria:
  - [ ] DFP encoding in Merkle state
  - [ ] Deterministic view enforcement
  - [ ] Consensus replay validation
- Estimated complexity: High

### Developer Tooling

To ensure smooth adoption, provide tooling support:

| Tool               | Description                                   | Priority |
| ------------------ | --------------------------------------------- | -------- |
| DFP-aware linter   | Warn when FLOAT used in deterministic context | High     |
| IDE type hints     | Show DFP vs FLOAT with context indicator      | High     |
| CAST auto-complete | Suggest `CAST(x AS DFP)` when needed          | Medium   |
| Migration analyzer | Scan code for FLOAT in consensus paths        | Medium   |
| REPL/Playground    | Interactive DFP computation testing           | Low      |

### Documentation Enhancements

Create comprehensive documentation:

| Document            | Content                                    | Priority |
| ------------------- | ------------------------------------------ | -------- |
| Migration Guide     | Step-by-step FLOAT→DFP conversion patterns | High     |
| Performance Guide   | When to use DFP vs FLOAT vs DECIMAL        | High     |
| Precision Reference | Decimal digit equivalence, loss scenarios  | Medium   |
| Cookbook            | Common patterns for AI/ML workloads        | Medium   |
| Troubleshooting     | Debugging type mismatch errors             | Low      |

### Breaking Changes

None. DFP is a new type that does not modify existing FLOAT/DOUBLE behavior.

### Migration Path

1. Existing tables continue using FLOAT/DOUBLE
2. New consensus-critical tables use DFP explicitly
3. Deterministic views require DFP columns
4. Gradual migration as needed

### Dependencies

- RFC-0103: Vector-SQL Storage (uses f32 internally, but separate)
- Expression VM (existing Stoolap component)

### Performance

| Mode                | Relative Speed |
| ------------------- | -------------- |
| Software DFP        | ~3-6x slower   |
| Fixed-point integer | ~1.5x faster   |

> Note: DFP now uses pure integer arithmetic (i128), making performance more predictable across platforms.

## Related RFCs

- RFC-0103: Unified Vector-SQL Storage Engine
- RFC-0100: AI Quota Marketplace
- RFC-0102: Wallet Cryptography

## References

- IEEE-754-2019: IEEE Standard for Floating-Point Arithmetic
- Stoolap Expression VM documentation
- Deterministic execution in replicated state machines

---

**Submission Date:** 2025-03-06
**Last Updated:** 2025-03-06
