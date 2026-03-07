# RFC-0105: Deterministic Quant Arithmetic (DQA)

## Status

Draft

## Summary

This RFC introduces Deterministic Quant Arithmetic (DQA) — a high-performance deterministic numeric type optimized for quantitative finance, pricing, and AI inference workloads. DQA represents numbers as scaled integers (`value × 10^-scale`), providing float-like ergonomics with integer-speed arithmetic.

DQA complements RFC-0104's DFP type: where DFP handles arbitrary-precision scientific computing, DQA provides bounded-range high-speed deterministic arithmetic suitable for trading, risk calculations, and ML preprocessing.

## Motivation

### Problem Statement

DFP (RFC-0104) provides arbitrary-precision deterministic floating-point, but at significant performance cost:

- DFP operations are 10-40x slower than native integers
- Normalization loops add overhead
- Overkill for bounded-range workloads

Many workloads don't need arbitrary exponents:

- Financial prices: 0.000001 – 1,000,000
- Probabilities: 0 – 1
- Vector embeddings: -10 – 10
- ML activation outputs: typically bounded

### Current State

Quantitative trading systems already use scaled integers:

- Bloomberg terminals
- Goldman Sachs quant engines
- Citadel trading systems
- Rithmic / Interactive Brokers APIs

They do this because it's:

- Deterministic
- Cache-friendly
- SIMD-friendly
- Fast

### Desired State

CipherOcto should provide:

- A SQL type for scaled deterministic arithmetic
- Performance approaching native integers
- Decimal precision control
- Full consensus determinism

## Specification

### Data Structures

```rust
/// Deterministic Quant Arithmetic representation
/// value = mantissa × 10^(-scale)
pub struct Dqa {
    /// Integer value (scaled representation)
    value: i64,
    /// Decimal scale (0-18 digits)
    scale: u8,
}

/// DQA encoding for storage/consensus
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DqaEncoding {
    pub value: i64,
    pub scale: u8,
    pub _reserved: [u8; 7], // Padding to 16 bytes
}

impl DqaEncoding {
    /// Serialize DQA to canonical big-endian encoding
    pub fn from_dqa(dqa: &Dqa) -> Self {
        Self {
            value: dqa.value.to_be(),
            scale: dqa.scale,
            _reserved: [0; 7],
        }
    }

    /// Deserialize from canonical encoding
    pub fn to_dqa(&self) -> Dqa {
        Dqa {
            value: i64::from_be(self.value),
            scale: self.scale,
        }
    }
}
```

### APIs/Interfaces

```rust
impl Dqa {
    /// Create DQA from value and scale
    pub fn new(value: i64, scale: u8) -> Self;

    /// Create from f64 (with rounding to scale)
    pub fn from_f64(value: f64, scale: u8) -> Self;

    /// Convert to f64 (lossy)
    pub fn to_f64(&self) -> f64;

    /// Arithmetic operations
    pub fn add(self, other: Self) -> Self;
    pub fn sub(self, other: Self) -> Self;
    pub fn mul(self, other: Self) -> Self;
    pub fn div(self, other: Self) -> Self;
}

/// Expression VM opcodes
pub enum VmOpcode {
    // ... existing opcodes
    OP_DQA_ADD,
    OP_DQA_SUB,
    OP_DQA_MUL,
    OP_DQA_DIV,
}
```

### SQL Integration

```sql
-- Deterministic quant table
CREATE TABLE trades (
    id INTEGER PRIMARY KEY,
    price DQA(6),       -- 6 decimal places: 1.234567
    quantity DQA(3),    -- 3 decimal places: 123.456
    executed_at TIMESTAMP
);

-- Mixed scale arithmetic requires explicit alignment
SELECT
    price * quantity  -- Error: scale mismatch
FROM trades;

-- Correct: align scales first
SELECT
    DQA_MUL(price, quantity)  -- scale = 6 + 3 = 9
FROM trades;
```

### Scale Alignment Rules

DQA operations require scale alignment:

| Operation | Result Scale          |
| --------- | --------------------- |
| ADD/SUB   | max(scale_a, scale_b) |
| MUL       | scale_a + scale_b     |
| DIV       | scale_a - scale_b     |

### Arithmetic Algorithms

#### Addition

```
DQA_ADD(a, b):
    1. Align scales: extend smaller scale
    2. result_value = a.value + b.value
    3. result_scale = max(a.scale, b.scale)
    4. Return
```

#### Multiplication

```
DQA_MUL(a, b):
    1. result_value = a.value * b.value
    2. result_scale = a.scale + b.scale
    3. If scale > 18: round to 18 decimal places
    4. Return
```

#### Division

```
DQA_DIV(a, b):
    1. Scale up dividend: scaled = a.value * 10^(b.scale)
    2. result_value = scaled / b.value
    3. result_scale = a.scale - b.scale
    4. Return
```

### Constraints

- **Determinism**: All nodes produce identical results
- **Scale limit**: Maximum 18 decimal places
- **Value limit**: i64 range (-9.2×10¹⁸ to 9.2×10¹⁸)
- **Type mixing**: Forbidden without explicit alignment
- **No special values**: No NaN, no Infinity (use DFP for these)

### Error Handling

| Scenario                     | Behavior                               |
| ---------------------------- | -------------------------------------- |
| DQA \* FLOAT                 | Compile error                          |
| DQA + DQA (mismatched scale) | Compile error (use explicit alignment) |
| Division by zero             | Return error                           |
| Scale overflow               | Round to 18 decimal places             |

## Rationale

### Why Scaled Integer?

The quant finance industry has decades of evidence that scaled integers are:

| Property          | Scaled Integer | Binary Float          |
| ----------------- | -------------- | --------------------- |
| Determinism       | ✅ Guaranteed  | ❌ Platform-dependent |
| Speed             | ~1.2x integer  | 10-40x slower         |
| Cache efficiency  | ✅ Excellent   | ❌ Poor               |
| SIMD support      | ✅ Excellent   | ❌ Limited            |
| Decimal precision | ✅ Exact       | ❌ Approximate        |

### Alternatives Considered

| Alternative  | Pros                | Cons               | Rejection Reason               |
| ------------ | ------------------- | ------------------ | ------------------------------ |
| DECIMAL      | SQL standard        | Variable precision | Not deterministic enough       |
| DFP          | Arbitrary precision | 10-40x slower      | Overkill for bounded workloads |
| Fixed-point  | Simple              | Limited range      | Already covered by INTEGER     |
| Binary float | Fast                | Non-deterministic  | Unsafe for consensus           |

### Trade-offs

| Priority   | Trade-off              |
| ---------- | ---------------------- |
| Prioritize | Speed, determinism     |
| Accept     | Limited scale (max 18) |
| Accept     | No special values      |

## Implementation

### Mission 1: DQA Core Type

- Location: `determ/dqa.rs`
- Acceptance criteria:
  - [ ] DQA struct with value/scale
  - [ ] Arithmetic: add, sub, mul, div
  - [ ] Scale alignment rules
  - [ ] From/To f64 conversion
  - [ ] Serialization
- Estimated complexity: Low

### Mission 2: DataType Integration

- Location: `src/parser/ast.rs`, `src/parser/statements.rs`
- Acceptance criteria:
  - [ ] Add `DataType::Quant` variant
  - [ ] SQL parser accepts `DQA(n)` syntax
  - [ ] Type checking for scale alignment
- Estimated complexity: Low

### Mission 3: Expression VM Opcodes

- Location: `src/vm/`
- Acceptance criteria:
  - [ ] OP_DQA_ADD, OP_DQA_SUB, OP_DQA_MUL, OP_DQA_DIV
  - [ ] Scale alignment validation
- Estimated complexity: Low

### Mission 4: Consensus Integration

- Location: `src/storage/`, `src/consensus/`
- Acceptance criteria:
  - [ ] DQA encoding in Merkle state
  - [ ] Consensus replay validation
- Estimated complexity: Medium

## Impact

### Breaking Changes

None. DQA is a new type.

### Performance

| Type    | Relative Speed |
| ------- | -------------- |
| INTEGER | 1x (baseline)  |
| DQA     | 1.2x           |
| DECIMAL | 2-3x           |
| DFP     | 8-20x          |

### Dependencies

- RFC-0104: DFP (complementary type)
- RFC-0103: Vector-SQL Storage

## Related RFCs

- RFC-0104: Deterministic Floating-Point Abstraction (DFP)
- RFC-0103: Unified Vector-SQL Storage Engine

## Use Cases

### Quantitative Finance

- Option pricing
- Portfolio valuation
- Risk metrics (VaR, Greeks)
- Order book calculations

### AI/ML Inference

- Activation function outputs
- Probability distributions
- Normalized embeddings
- Attention weights

### Gaming

- In-game currency
- Item pricing
- Achievement scores

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Verifiable AI Agents for DeFi](../../docs/use-cases/verifiable-ai-agents-defi.md)

---

**Submission Date:** 2025-03-06
**Last Updated:** 2025-03-06
