# RFC-0114 (Numeric/Math): Deterministic Activation Functions

## Status

**Version:** 1.0 (2026-03-14)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

## Summary

This RFC defines deterministic neural network activation functions for AI inference in consensus.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | Uses DQA for LUT indexing |
| RFC-0105 (DQA) | Primary numeric type for activations |
| RFC-0112 (DVEC) | Applies element-wise to vectors |
| RFC-0113 (DMAT) | Applies element-wise to matrices |

## Production Status

| Function | Implementation | Status | Gas |
|----------|---------------|--------|-----|
| ReLU | Exact | **STABLE** | 2 |
| Sigmoid | LUT | EXPERIMENTAL | 10 |
| Tanh | LUT | EXPERIMENTAL | 10 |
| LeakyReLU | Exact | EXPERIMENTAL | 3 |

## ReLU — Rectified Linear Unit

```
relu(x: Dqa) -> Dqa

Algorithm:
  if x.value < 0:
    return Dqa { value: 0, scale: x.scale }
  else:
    return x
```

**Properties:**
- Exact: No approximation
- Deterministic: Always produces same output
- Gas: 2 per element

### ReLU6 (Clipped ReLU)

```
relu6(x: Dqa) -> Dqa

Algorithm:
  if x.value < 0:
    return Dqa { value: 0, scale: x.scale }
  else if x.value > 6 * 10^x.scale:
    return Dqa { value: 6 * 10^x.scale, scale: x.scale }
  else:
    return x
```

## Sigmoid — Logistic Function

> ⚠️ **EXPERIMENTAL**: Requires canonical LUT with SHA-256 commitment.

### LUT Specification

**Domain:** x ∈ [-4.0, 4.0]
**Output Range:** y ∈ (0, 1)
**Entries:** 801 (one per 0.01)
**Format:** Q8.8 signed (multiply by 256 to get actual value)

### LUT Generation

```
sigmoid(x) = 1 / (1 + exp(-x))

Algorithm:
  1. Scale input: x_scaled = x * 256  // Convert to Q8.8
  2. Clamp: x_scaled = clamp(x_scaled, -1024, 1024)  // [-4.0, 4.0] in Q8.8
  3. Index: idx = (x_scaled + 1024) / 2  // Map to [0, 800]
  4. Lookup: y = LUT[idx]
  5. Return y as Dqa
```

### LUT Values (Partial)

| Input | Index | Output (Q8.8) | Output (float) |
|-------|-------|---------------|----------------|
| -4.0 | 0 | 5 | 0.0195 |
| -2.0 | 200 | 31 | 0.1211 |
| 0.0 | 400 | 128 | 0.5000 |
| 2.0 | 600 | 225 | 0.8807 |
| 4.0 | 800 | 251 | 0.9805 |

### SHA-256 Commitment

```
SIGMOID_LUT_V1_SHA256 = "9069599354fec1628994a5c7ca7f09d186801a78508cb3bca112696158d3c0e6"
```

## Tanh — Hyperbolic Tangent

> ⚠️ **EXPERIMENTAL**: Requires canonical LUT with SHA-256 commitment.

### LUT Specification

**Domain:** x ∈ [-4.0, 4.0]
**Output Range:** y ∈ (-1, 1)
**Entries:** 801
**Format:** Q8.8 signed

### LUT Generation

```
tanh(x) = (exp(x) - exp(-x)) / (exp(x) + exp(-x))

Algorithm:
  1. Scale input: x_scaled = x * 256
  2. Clamp: x_scaled = clamp(x_scaled, -1024, 1024)
  3. Index: idx = (x_scaled + 1024) / 2
  4. Lookup: y = LUT[idx]
  5. Return y as Dqa
```

### LUT Values (Partial)

| Input | Index | Output (Q8.8) | Output (float) |
|-------|-------|---------------|----------------|
| -4.0 | 0 | -256 | -1.0 |
| -2.0 | 200 | -181 | -0.7070 |
| 0.0 | 400 | 0 | 0.0 |
| 2.0 | 600 | 181 | 0.7070 |
| 4.0 | 800 | 256 | 1.0 |

### SHA-256 Commitment

```
TANH_LUT_V1_SHA256 = "b373014b8d1aa95059c8b3fc773225cc3eaf2c93afe4292323a85776e5c6bc45"
```

## LeakyReLU — Leaky Rectified Linear Unit

```
leaky_relu(x: Dqa, alpha: Dqa) -> Dqa

Default alpha = 0.01

Algorithm:
  if x.value < 0:
    // x * alpha
    return Dqa {
      value: (x.value * alpha.value) / 10^alpha.scale,
      scale: x.scale + alpha.scale
    }
  else:
    return x
```

## Domain Handling

### Out-of-Range Inputs

| Input Range | Sigmoid Behavior | Tanh Behavior |
|-------------|-----------------|---------------|
| x < -4.0 | Clamp to 0 | Clamp to -1 |
| -4.0 ≤ x ≤ 4.0 | LUT lookup | LUT lookup |
| x > 4.0 | Clamp to 1 | Clamp to 1 |

### Error Bounds

| Function | Max Error | Notes |
|----------|-----------|-------|
| ReLU | 0 | Exact |
| Sigmoid (in domain) | ±0.5/256 ≈ ±0.002 | LUT quantization |
| Sigmoid (clamped) | ≤ 0.018 | Domain edge |
| Tanh (in domain) | ±1/256 ≈ ±0.004 | LUT quantization |
| Tanh (clamped) | ≤ 0.007 | Domain edge |

## LUT Indexing (CRITICAL)

> ⚠️ **MUST USE DQA ARITHMETIC**: All LUT indexing must use integer arithmetic, NOT floating-point.

```
# WRONG — uses floating-point (forbidden)
idx = ((x + 4.0) / 0.01).round()

# CORRECT — uses DQA arithmetic (required)
let x_scaled = x * 100;  // DQA multiplication
let idx = (x_scaled + 400) / 1;  // DQA division, integer result
```

## Gas Model

| Operation | Gas | Notes |
|-----------|-----|-------|
| ReLU | 2 | Comparison + select |
| ReLU6 | 3 | Two comparisons + select |
| Sigmoid | 10 | LUT lookup + scale |
| Tanh | 10 | LUT lookup + scale |
| LeakyReLU | 3 | Comparison + 2 multiplies |

## Test Vectors

### ReLU

| Input | Expected |
|-------|----------|
| -5.0 | 0.0 |
| 0.0 | 0.0 |
| 5.0 | 5.0 |
| -1.234 | 0.0 |

### Sigmoid

| Input | Expected (Q8.8) |
|-------|-----------------|
| -4.0 | 5 |
| -2.0 | 31 |
| 0.0 | 128 |
| 2.0 | 225 |
| 4.0 | 251 |

### Tanh

| Input | Expected (Q8.8) |
|-------|-----------------|
| -4.0 | -256 |
| -2.0 | -181 |
| 0.0 | 0 |
| 2.0 | 181 |
| 4.0 | 256 |

## Verification Probe

```rust
struct ActivationProbe {
    /// Entry 0: relu(5.0) = 5.0
    entry_0: [u8; 32],
    /// Entry 1: relu(-5.0) = 0.0
    entry_1: [u8; 32],
    /// Entry 2: sigmoid(0.0) = 0.5
    entry_2: [u8; 32],
    /// Entry 3: sigmoid(4.0) ≈ 0.98
    entry_3: [u8; 32],
    /// Entry 4: tanh(0.0) = 0.0
    entry_4: [u8; 32],
    /// Entry 5: tanh(4.0) = 1.0
    entry_5: [u8; 32],
    /// Entry 6: LUT hash verification
    entry_6: [u8; 32],
}

fn activation_probe_root(probe: &ActivationProbe) -> [u8; 32] {
    sha256(concat!(...))
}
```

## Implementation Checklist

- [ ] ReLU implementation
- [ ] ReLU6 implementation
- [ ] Sigmoid LUT generation tool
- [ ] Sigmoid LUT with SHA-256 commitment
- [ ] Tanh LUT generation tool
- [ ] Tanh LUT with SHA-256 commitment
- [ ] Domain clamping
- [ ] DQA-based indexing
- [ ] Gas calculations
- [ ] Test vectors
- [ ] Verification probe

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0112: Deterministic Vectors
- RFC-0113: Deterministic Matrices
- RFC-0106: Deterministic Numeric Tower (archived)
