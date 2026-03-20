# RFC-0114 (Numeric/Math): Deterministic Activation Functions

## Status

**Version:** 2.12 (2026-03-19)
**Status:** Accepted
**NUMERIC_SPEC_VERSION:** 1 (per RFC-0110)

> **Note:** This RFC defines deterministic, consensus-safe neural network activation functions for AI inference. All LUT-based functions use a strict binary/decimal separation: Q8.8 binary fixed-point for LUT storage, DQA decimal fixed-point for execution.

## Summary

Defines deterministic activation functions for consensus-safe AI inference using DQA arithmetic.

### What's New in v2.12

- **FIXED**: Issue 1 Entry 23 corrected from "upscale" to "downscale (positive value)"
- **FIXED**: Issue 2 [S] marker defined in checklist legend
- **FIXED**: Issue 3 LUT Values table DQA column relabelled as "pre-canonicalize intermediate"
- **FIXED**: Issue 4 Phase 2 canonicalization clarified for sigmoid/tanh (deferred to output construction)

### What's New in v2.7

- **FIXED**: All DQA return values now explicitly canonicalized per RFC-0105 before serialization (addressing reviewer CRIT-2 canonicalization concern)
- **UPDATED**: Merkle root and all affected leaf hashes recomputed with canonical values
- **ADDED**: explicit canonicalization in all function pseudocode
- **ADDED**: §canonicalization note in Phase Ordering clarifying RFC-0105 VM rule applies to activation function returns

### What's New in v2.6

- **ADDED**: TRAP checks to ReLU, ReLU6, LeakyReLU (addressing reviewer concern)
- **ADDED**: §normalize_to_scale definition (addressing reviewer concern)
- **Clarified**: Canonicalization scope per RFC-0105 (ADD/SUB/MUL/DIV only, not comparison/selection ops)

### What's New in v2.5

- **FIXED**: Q8.8→DQA rounding clarified as floor division (Python // semantics), not truncation
- **FIXED**: LeakyReLU entries 10-11 corrected (Dqa(-1,4) was wrong, should be Dqa(-100,4))
- **FIXED**: Example (-127 case) corrected to -4961
- **ADDED**: Rust floor-division implementation for Q8.8→DQA conversion

### What's New in v2.4

- **Clarified**: Q8.8→DQA rounding uses truncation toward zero (Rust integer division semantics)
- **Clarified**: TRAP_INPUT_ERROR code and inline check vs. RFC-0113 Phase 0 pre-check
- **Added**: Probe entry count rationale (16 entries vs. 57 in RFC-0111/0112/0113)
- **Added**: Explicit script reference with commit hash

### What's New in v2.3

- **FIXED**: Clamp return scales now use scale=4 (consistent with Q8.8→DQA target_scale)
- **ADDED**: NUMERIC_SPEC_VERSION declaration per RFC-0110
- **FIXED**: Merkle construction described as pairwise tree (matching RFC-0113)

### What's New in v2.2

- **FIXED**: Q8.8 vs DQA incompatibility — explicit conversion boundary introduced
- **FIXED**: Tanh LUT catastrophic data corruption (v1.0 entries at idx 200/600 were wrong)
- **FIXED**: TRAP probe corruption — canonical sentinel enforced
- **FIXED**: Probe entries now include computed outputs, not inputs
- **FIXED**: LUT commitment now over raw Q8.8 bytes (content-verifiable)

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0105 (DQA) | **MANDATORY**: DQA encoding, canonicalization, TRAP |
| RFC-0110 (Numeric Spec) | NUMERIC_SPEC_VERSION declaration |
| RFC-0112 (DVEC) | Element-wise application |
| RFC-0113 (DMAT) | TRAP propagation invariant |
| RFC-0126 (Serialization) | Informative reference (serialization algorithm) |

## Normative Dependencies

This RFC is **normatively dependent** on:
- **RFC-0105**: DqaEncoding format (16 bytes: i64 value + u8 scale + 7 reserved), canonicalization rules, TRAP sentinel definition, DQA arithmetic operations
- **RFC-0110**: NUMERIC_SPEC_VERSION declaration (per §Status)

---

# Canonical Numeric Model (CRITICAL)

## Binary LUT ≠ DQA Execution

This RFC mandates strict separation between LUT storage and execution:

| Layer | Representation |
|-------|---------------|
| LUT Storage | **Q8.8 signed i16** (binary fixed-point, raw 801×2 bytes) |
| Execution Input/Output | **DQA** (decimal fixed-point per RFC-0105) |

**These are incompatible fixed-point systems. No value may be stored in DQA format inside the LUT.**

## Q8.8 → DQA Conversion (MANDATORY)

When a LUT lookup produces a Q8.8 value `q ∈ [-256, 256]`, it MUST be converted to DQA before returning:

```
result_value, result_scale = (q * 10^target_scale) // 256, target_scale
```

Where `target_scale = 4` (default).

**Rounding**: Floor division (Python `//` semantics). Deterministic, no floating-point allowed.

> **Note**: This conversion uses floor division, NOT RFC-0105's RoundHalfEven and NOT Rust's truncation toward zero, because Q8.8 is a binary fixed-point format. The floor behavior matches Python's `//` operator:
> - `(-128 * 10000) // 256 = -5000` (exact division, no rounding)
> - `(-127 * 10000) // 256 = -4961` (floor: -4960.9375 → -4961)
>
> For Rust implementations, use explicit floor division for negative values:
> ```rust
> fn q88_to_dqa(q: i16, target_scale: u8) -> (i64, u8) {
>     let numerator = (q as i128) * 10_i128.pow(target_scale as u32);
>     // Floor division (matches Python // semantics)
>     let result = if numerator >= 0 {
>         (numerator / 256) as i64
>     } else {
>         -(((-numerator + 255) / 256) as i64)  // floor for negative values
>     };
>     (result, target_scale)
> }
> ```

### Example

```
sigmoid(4.0) → LUT[800] = 251 (Q8.8)
251 * 10^4 // 256 = 9804
Dqa(9804, 4) = 0.9804 (≈ sigmoid(4.0))
```

## normalize_to_scale Helper

For sigmoid and tanh, the input must be normalized to scale=2 before domain evaluation:

```
normalize_to_scale(x: Dqa, target_scale: u8) -> Dqa

// If already at target scale, return as-is (per RFC-0105 lazy canonicalization)
if x.scale == target_scale:
    return x

// Otherwise, align mantissa to target scale
// Multiply or divide mantissa by 10^(target_scale - x.scale)
delta = target_scale - x.scale
if delta > 0:
    // Multiply mantissa by 10^delta
    // NOTE: If x.value * 10^delta would overflow i64, return TRAP
    // (Bounded by RFC-0105 scale ≤ 18 and max DQA mantissa magnitude)
    return Dqa(x.value * 10^delta, target_scale)
else:
    // Divide mantissa by 10^|delta| (floor division)
    return Dqa(x.value // 10^|delta|, target_scale)
```

> **Note**: This is a DQA scale-alignment operation, NOT canonicalization. It does not strip trailing zeros — it only adjusts the decimal scale. The result is used internally; final outputs from activation functions are still subject to canonicalization by the caller if needed.
>
> **Important**: `normalize_to_scale` is an internal helper. Callers MUST validate `scale ≤ 18` at the entry point before calling. This function does not perform scale validation.

---

# Activation Functions

## 1. ReLU

```
relu(x: Dqa) -> Dqa

if x is TRAP: return TRAP
if x.scale > 18: return TRAP(INVALID_SCALE)

if x.value < 0:
    return CANONICALIZE(Dqa(0, x.scale))
else:
    return CANONICALIZE(x)
```

- **Gas**: 2
- **Properties**: Exact, no approximation, no scale change

## 2. ReLU6

```
relu6(x: Dqa) -> Dqa

if x is TRAP: return TRAP
if x.scale > 18: return TRAP(INVALID_SCALE)

max_val = Dqa(6 * 10^x.scale, x.scale)  // Internal; do NOT canonicalize

if x.value < 0:
    return CANONICALIZE(Dqa(0, x.scale))
else if x > max_val:
    return CANONICALIZE(max_val)
else:
    return CANONICALIZE(x)
```

- **Gas**: 3

> **Note**: `max_val` is a DQA value created with `Dqa(6 * 10^x.scale, x.scale)`. The comparison `x > max_val` uses DQA comparison semantics.

## 3. LeakyReLU

```
leaky_relu(x: Dqa, alpha: Dqa = Dqa(1, 2)) -> Dqa

// alpha is a PROTOCOL CONSTANT: Dqa(1, 2) = 0.01
// Callers MUST NOT supply custom alpha values

if x is TRAP: return TRAP
if x.scale > 18: return TRAP(INVALID_SCALE)

if x.value < 0:
    return multiply(x, alpha)  // RFC-0105 safe multiply (canonicalizes result)
else:
    return CANONICALIZE(x)
```

- **Gas**: 3

## 4. Sigmoid

```
sigmoid(x: Dqa) -> Dqa

if x is TRAP: return TRAP
if x.scale > 18: return TRAP(INVALID_SCALE)

// Phase 1: Normalize to scale=2
x_norm = normalize_to_scale(x, 2)  // See §normalize_to_scale

// Phase 2: Clamp
if x_norm.value < -400: return CANONICALIZE(Dqa(0, 4))
if x_norm.value > 400:  return CANONICALIZE(Dqa(10000, 4))

// Phase 3: Index
idx = x_norm.value + 400

// Phase 4: LUT lookup → Q8.8 → DQA
q = LUT_SIGMOID[idx]
result_value, result_scale = (q * 10^4) // 256, 4
return CANONICALIZE(Dqa(result_value, result_scale))
```

- **Gas**: 10

## 5. Tanh

```
tanh(x: Dqa) -> Dqa

if x is TRAP: return TRAP
if x.scale > 18: return TRAP(INVALID_SCALE)

// Phase 1: Normalize to scale=2
x_norm = normalize_to_scale(x, 2)  // See §normalize_to_scale

// Phase 2: Clamp
if x_norm.value < -400: return CANONICALIZE(Dqa(-10000, 4))
if x_norm.value > 400:  return CANONICALIZE(Dqa(10000, 4))

// Phase 3: Index
idx = x_norm.value + 400

// Phase 4: LUT lookup → Q8.8 → DQA
q = LUT_TANH[idx]
result_value, result_scale = (q * 10^4) // 256, 4
return CANONICALIZE(Dqa(result_value, result_scale))
```

- **Gas**: 10

---

# LUT Specification

## Canonical Indexing

| Parameter | Value |
|-----------|-------|
| Domain | x ∈ [-4.00, 4.00] |
| Step | 0.01 |
| Entries | 801 |
| Index formula | `idx = x_int + 400` where `x_int ∈ [-400, 400]` |
| Index range | [0, 800] |

**Index computation MUST use integer arithmetic. No division. No rounding ambiguity.**

## LUT Encoding

- **Format**: Raw Q8.8 signed i16, big-endian
- **Total size**: 801 × 2 = 1602 bytes per LUT
- **No padding, no header**

## LUT Generation

```python
for i in range(-400, 401):       # x_int = -400 .. 400
    x = i / 100.0                # x = -4.00 .. 4.00
    y = sigmoid(x) or tanh(x)
    q = round(y * 256)           # Q8.8 quantization
    table.append(q)              # i16
```

> **Off-chain note**: Float is permitted only in LUT generation (off-chain). Consensus path uses integer indexing only.

## LUT Values (Selected)

### Sigmoid

| x | idx | Q8.8 | DQA intermediate (pre-canonicalize) | Real value |
|---|-----|------|-------------------------------------|------------|
| -4.00 | 0 | 5 | Dqa(195, 4) | 0.0195 |
| -2.00 | 200 | 31 | Dqa(1211, 4) | 0.1211 |
| 0.00 | 400 | 128 | Dqa(5000, 4) | 0.5000 |
| 2.00 | 600 | 225 | Dqa(8789, 4) | 0.8789 |
| 4.00 | 800 | 251 | Dqa(9804, 4) | 0.9804 |

> **Note**: DQA intermediate values shown above are pre-canonicalization. Final output is `canonicalize(DQA intermediate)`.

### Tanh

| x | idx | Q8.8 | DQA intermediate (pre-canonicalize) | Real value |
|---|-----|------|-------------------------------------|------------|
| -4.00 | 0 | -256 | Dqa(-10000, 4) | -1.0000 |
| -2.00 | 200 | **-247** | Dqa(-9649, 4) | **-0.9649** |
| 0.00 | 400 | 0 | Dqa(0, 4) | 0.0000 |
| 2.00 | 600 | **247** | Dqa(9649, 4) | **0.9649** |
| 4.00 | 800 | 256 | Dqa(10000, 4) | 1.0000 |

> **v1.0 correction**: Tanh entries at idx 200 and 600 were catastrophically wrong in v1.0 (showing -181 and 181 instead of -247 and 247). These are now corrected.

---

# SHA-256 Commitments

```
SIGMOID_LUT_V2_SHA256 = "7af8a570e86bf433bc558d66473b2460663d3be98c85f258e98dc93dc3aff5df"
TANH_LUT_V2_SHA256    = "dc92c87e65f8fe3b0070daa09d0d5a8a97b15b39e5f6040e280052605389b379"
```

Hash is over raw concatenation of 801 × i16 big-endian Q8.8 values (1602 bytes).

---

# TRAP Invariant

From RFC-0113:

> If any input is TRAP → output MUST be TRAP

## TRAP Sentinel (Canonical)

```
value = -(1 << 63)  // i64::MIN
scale = 0xFF
reserved = [0; 7]
TOTAL = 16 bytes (matches DqaEncoding per RFC-0105)
```

Any operation receiving a non-canonical DQA or TRAP sentinel must TRAP immediately.

### TRAP Error Code

For scalar activation functions, TRAP propagation uses inline checks:

```
if x.scale() == 0xFF and x.raw_mantissa() == i64::MIN as i128:
    TRAP(TRAP_INPUT_ERROR)
```

This is functionally equivalent to RFC-0113's Phase 0 pre-check but optimized for scalar inputs.

**Error code**: `TRAP_INPUT_ERROR` (per RFC-0113 TRAP Codes table).

## Phase Ordering (MANDATORY)

```
1. TRAP check (RFC-0105)
2. Canonicalization (RFC-0105)
3. Scale normalization (→ scale=2 for LUT functions)
4. Domain clamp
5. Index computation (integer only)
6. LUT lookup
7. Q8.8 → DQA conversion
8. Output construction (canonical)
```

> **Note on ReLU/ReLU6**: These functions skip Phases 2–3 (canonicalization and scale normalization) because they operate element-wise with direct comparison. TRAP check is still mandatory.
>
> **Note on Sigmoid/Tanh Phase 2**: These functions do NOT canonicalize the input before Phase 3 (normalize_to_scale). Phase 2 canonicalization is deferred to Phase 8 (output construction). The input `x` to `normalize_to_scale` may be non-canonical; this is intentional as normalize_to_scale operates on the mantissa regardless of trailing zeros.
>
> **Note on Canonicalization**: Per RFC-0105 §Canonical Form and VM Canonicalization Rule, **all DQA return values must be canonicalized before serialization/hashing**. For activation functions:
> - **ReLU/ReLU6**: Return `canonicalize(Dqa(0, x.scale))` → `Dqa(0, 0)` or `canonicalize(x)` → canonical value.
> - **LeakyReLU**: Invokes RFC-0105 `multiply`, which canonicalizes internally.
> - **Sigmoid/Tanh**: clamp returns and LUT conversions: canonicalize at Phase 8 (output construction), not before normalize_to_scale.
>
> All probe entries use **canonical DQA values** per RFC-0105 CANONICALIZE rules.

---

# Domain Handling

| Input Range | Sigmoid | Tanh |
|-------------|---------|------|
| x < -4.0 | Dqa(0, 0) | Dqa(-1, 0) |
| -4.0 ≤ x ≤ 4.0 | LUT + conversion | LUT + conversion |
| x > 4.0 | Dqa(1, 0) | Dqa(1, 0) |

---

# Gas Model

| Operation | Gas | Notes |
|-----------|-----|-------|
| ReLU | 2 | Comparison + select + CANONICALIZE (0 gas) |
| ReLU6 | 3 | Two comparisons + select + CANONICALIZE (0 gas) |
| LeakyReLU | 3 | Comparison + multiply |
| Sigmoid | 10 | Normalize + clamp + index + lookup + convert (normalize_to_scale included) |
| Tanh | 10 | Normalize + clamp + index + lookup + convert (normalize_to_scale included) |

#### Detailed Gas Breakdown (Sigmoid/Tanh)

| Sub-operation | Gas | Notes |
|--------------|-----|-------|
| TRAP check + scale validation | 1 | |
| normalize_to_scale | 2 | Scale comparison + multiply/divide |
| Domain clamp | 1 | Two comparisons |
| Index computation | 1 | Integer addition |
| LUT lookup | 1 | |
| Q8.8→DQA conversion | 2 | Multiplication + floor division |
| Return construction | 2 | DQA construction |
| CANONICALIZE | 0 | Representation normalization (zero-cost) |
| **Total per call** | **10** | |

### Gas Budget Proof

AI inference workloads execute layers of activation functions over tensors. Conservative per-block gas budget: 50,000 gas (RFC-0110 numeric per-block allocation).

**Per-neuron worst case**: sigmoid/tanh = 10 gas

**Per-layer worst case (1,000 neurons, dense layer)**:
```
1,000 × 10 = 10,000 gas per activation layer
```

**Typical MLP inference (3-layer, 1,000 neurons each)**:
```
Layer 1: 1,000 × 10 = 10,000 (activation)
Layer 2: 1,000 × 10 = 10,000
Layer 3: 1,000 × 10 = 10,000
Total: 30,000 gas
```

**With ReLU layers (cheaper, 2 gas)** replacing some sigmoid/tanh:
```
Mixed MLP: 20,000 gas
```

**Conclusion**: A 3-layer MLP with 1,000 neurons each stays well under 50,000 gas per block. For larger models, activation functions are typically the minority of computation (matrix multiply dominates). Even a 10-layer model with 2,000 neurons each: `10 × 2,000 × 10 = 200,000` — would require multiple blocks, which is acceptable. The fixed gas costs ensure predictable metering.

---

# Error Bounds

| Function | Max Error | Notes |
|----------|-----------|-------|
| ReLU | 0 | Exact |
| Sigmoid (in domain) | ≤ 0.004 | Q8.8 quantization (1/256) + floor division |
| Tanh (in domain) | ≤ 0.004 | Q8.8 quantization (1/256) + floor division |
| Sigmoid (clamped) | ≤ 0.019 | Boundary |
| Tanh (clamped) | ≤ 0.035 | Boundary |

---

# Verification Probe

## Structure

- **Entries**: 16
- **Indexing**: [0..15], zero-indexed, no gaps
- **Leaf serialization**: See §Serialization

> **Note on Entry Count**: This RFC uses 16 probe entries (vs. 57 in RFC-0111/0112/0113) because:
> - DACT defines 5 activation functions (vs. 7 arithmetic operations in Decimal)
> - Each function has a bounded input domain, reducing combinatorial test cases
> - LUT-based functions have deterministic outputs fully specified by the LUT
> - This deviation from the 57-entry convention is intentional and documented.

## Entries

All DQA values are **canonicalized per RFC-0105** before serialization (trailing zeros stripped, zero → scale=0).

| Index | Description | Serialized Value |
|-------|-------------|-------------------|
| 0 | relu(5.0) | Dqa(5, 0) |
| 1 | relu(-5.0) | Dqa(0, 0) |
| 2 | relu6(10.0) | Dqa(6, 0) → clamp to Dqa(6, 0) = 6.00 |
| 3 | relu6(3.0) | Dqa(3, 0) = 3.00 |
| 4 | sigmoid(0.0) | Dqa(5, 1) = 0.5 |
| 5 | sigmoid(4.0) | Dqa(9804, 4) = 0.9804 |
| 6 | sigmoid(-4.0) | Dqa(195, 4) = 0.0195 |
| 7 | tanh(0.0) | Dqa(0, 0) = 0.00 |
| 8 | tanh(2.0) | Dqa(9649, 4) = 0.9649 |
| 9 | tanh(-2.0) | Dqa(-9649, 4) = -0.9649 |
| 10 | leaky_relu(-1.0) | Dqa(-1, 2) = -0.01 |
| 11 | leaky_relu(1.0) | Dqa(1, 0) = 1.00 |
| 12 | First 4 sigmoid LUT entries | Raw Q8.8 bytes (8 bytes) |
| 13 | First 4 tanh LUT entries | Raw Q8.8 bytes (8 bytes) |
| 14 | Normalization invariant | Dqa(1234, 2) = 12.34 |
| 15 | TRAP sentinel | Dqa(-2^63, 0xFF) |

## Serialization

### DQA (16 bytes)

```python
def serialize_dqa(value: int, scale: int) -> bytes:
    return value.to_bytes(8, "big", signed=True) + bytes([scale]) + bytes(7)
```

### TRAP (16 bytes)

```python
def serialize_trap() -> bytes:
    return serialize_dqa(-(1 << 63), 0xFF)
```

### Raw Q8.8 (2 bytes per entry)

```python
def serialize_i16(v: int) -> bytes:
    return v.to_bytes(2, "big", signed=True)
```

## Merkle Root

```
MERKLE_ROOT = "4904af886aac5b581fefcf5d275c0753a0f804bc749d47bdd5bed74565c09fce"
```

Computed as pairwise Merkle tree: leaves are individually hashed, then pairs are hashed iteratively until root. If odd number of nodes at any level, last node is duplicated (RFC-0113 convention).

```
level[0] = [SHA256(leaf[0]), SHA256(leaf[1]), ..., SHA256(leaf[15])]
while len(level[n]) > 1:
    if len(level[n]) is odd: append duplicate of last node
    level[n+1] = [SHA256(level[n][i] + level[n][i+1]) for i in range(0, len(level[n]), 2)]
root = level[last][0]
```

### Probe Leaf Hashes

| Index | SHA256(leaf) |
|-------|--------------|
| 0 | 2c1b906867d313e9ee07fe22afc47439ef2a276c007fd555da428d975b6247fc |
| 1 | 374708fff7719dd5979ec875d56cd2286f6d3cf7ec317a3b25632aab28ec37bb |
| 2 | f616620950c4139db66ec7b7c82d2ae0acf39f4817a29b8c7bd335099b12783d |
| 3 | 2b7ebe6c2639dc181ae42be423f1e13278e408c48cc41d71d9a8c823df46b62e |
| 4 | 81c47a3d9ced8ab73e1cbde91cb76cbc467ae9e51729b07bf74575b6620af5b7 |
| 5 | 06347beb2def35cc4ca2ac894210b148801ba8cb353b9e69c19ae385410e892f |
| 6 | caa8797b8ffa52a9f014c703c8a117fd5bb50d2b32e4b706a2a5ce8b22e20eb8 |
| 7 | 374708fff7719dd5979ec875d56cd2286f6d3cf7ec317a3b25632aab28ec37bb |
| 8 | 86dd24f74a250b9938d8231dc6b8329a2b3a0f939085dcf91000ecb1a0dfb61a |
| 9 | 18f133183a6e1bf4eb38c78c0eaeb7f6e2d5e438a4320a5b28dc118e8c88922a |
| 10 | f063fde67de22b6151b1049a822d081e5ef373d20ddbc3c9d3bd72115e6e13d1 |
| 11 | 783825822a6f9e62da2190e828e4c9d2576e5977e3a0b3620b092dfb9e9996fa |
| 12 | 4b26af1ad1298d65cf3f851befa62c9dcc4d350fc890d1012bcc109aa33d2af4 |
| 13 | e2cfc77ad4cf3961435d11f523d3dfede302f828cc4f1edb6ab5557f6e6e81bc |
| 14 | 24b7777bb88b85e57eaff1df2f4b5db54c771779e06becee232185630889a75c |
| 15 | 4451be616d27073d2a040621795ffb90e89aa639b91594cc54e4018453ef9ed7 |

### Extended Probe Entries (16–24, Non-Normative)

These additional entries are provided for extended verification and do not affect the committed Merkle root. Implementations MAY use these for self-verification but are not required to pass them for consensus compliance.

| Index | Description | Serialized Value |
|-------|-------------|-------------------|
| 16 | sigmoid(-4.5) | Dqa(0, 0) = 0 (clamp boundary) |
| 17 | sigmoid(4.5) | Dqa(1, 0) = 1 (clamp boundary) |
| 18 | tanh(-4.5) | Dqa(-1, 0) = -1 (clamp boundary) |
| 19 | tanh(4.5) | Dqa(1, 0) = 1 (clamp boundary) |
| 20 | ReLU(TRAP) | Dqa(-2^63, 0xFF) = TRAP |
| 21 | ReLU6(TRAP) | Dqa(-2^63, 0xFF) = TRAP |
| 22 | LeakyReLU(TRAP) | Dqa(-2^63, 0xFF) = TRAP |
| 23 | normalize_to_scale downscale: Dqa(25000,4)→Dqa(25,1) | Dqa(25, 1) = 2.5 (positive downscale: 25000//10^3=25) |
| 24 | normalize_to_scale downscale: Dqa(-153, 3)→Dqa(-16, 2) | Dqa(-16, 2) = -0.16 (floor: -15.3 → -16) |

---

# Reference Script

**File**: `scripts/compute_dact_probe_root.py`
**Repository**: `scripts/compute_dact_probe_root.py` (root-relative path)
**Version**: v2.12 (adversarial review fixes)
**Reference**: c2e3ebc (introduced v2.2); updated c2e3ebc+6 for canonicalization (v2.7)

Authoritative implementation for:
- LUT generation
- SHA-256 commitment computation
- Probe leaf serialization
- Merkle root verification

**Run with**: `python3 scripts/compute_dact_probe_root.py`
**Expected output**: `MERKLE_ROOT (16 entries) = 4904af886aac5b581fefcf5d275c0753a0f804bc749d47bdd5bed74565c09fce`

> **Normative Authority**: This RFC text takes precedence for consensus behavior. The reference script is provided for verification and conformance testing.

---

# Implementation Checklist

**Legend**: `[x]` = implemented and verified; `[S]` = self-verified (independent reproducibility confirmed by author; awaiting external audit)

- [x] ReLU — exact, no LUT needed
- [x] ReLU6 — exact, uses DQA comparison
- [x] LeakyReLU — exact, uses DQA multiply
- [x] Sigmoid LUT — 801 entries, Q8.8, SHA-256 committed
- [x] Tanh LUT — 801 entries, Q8.8, SHA-256 committed
- [x] Q8.8 → DQA conversion (floor division)
- [x] TRAP sentinel (canonical 16-byte encoding)
- [x] Phase ordering (TRAP → normalize → clamp → index → lookup → convert)
- [x] Merkle probe (16 entries)
- [S] Self-verified — independent reproducibility via `compute_dact_probe_root.py` (v2.12); external parties encouraged to verify

---

# References

- [RFC-0105: Deterministic Quant Arithmetic](../accepted/numeric/0105-deterministic-quant-arithmetic.md)
- [RFC-0110: Deterministic Numeric Specification](../accepted/numeric/0110-deterministic-numeric-spec.md)
- [RFC-0112: Deterministic Vectors](../accepted/numeric/0112-deterministic-vectors.md)
- [RFC-0113: Deterministic Matrices](../accepted/numeric/0113-deterministic-matrices.md)
- [RFC-0126: Deterministic Serialization](../draft/numeric/0126-deterministic-serialization.md)

---

# Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-14 | Initial draft, extracted from RFC-0106 |
| 2.0 | 2026-03-19 | Major rewrite — phase ordering, TRAP semantics, unified indexing |
| 2.1 | 2026-03-19 | Fixed DQA serialization (i64/16 bytes), TRAP sentinel |
| 2.2 | 2026-03-19 | Fixed Q8.8/DQA separation, corrected tanh LUT, fixed probe entries |
| 2.3 | 2026-03-19 | Adversarial review fixes: clamp scale=4 (HIGH-2), NUMERIC_SPEC_VERSION (HIGH-5), Merkle construction (HIGH-6) |
| 2.4 | 2026-03-19 | Clarified Q8.8→DQA rounding (truncation toward zero), TRAP_INPUT_ERROR code, 16-entry rationale, script reference |
| 2.5 | 2026-03-19 | Fixed Q8.8→DQA rounding (floor division), corrected LeakyReLU entries 10-11, fixed Rust implementation note |
| 2.6 | 2026-03-19 | Added TRAP checks to ReLU/ReLU6/LeakyReLU, added normalize_to_scale definition, clarified canonicalization scope |
| 2.7 | 2026-03-19 | Fixed canonicalization: all DQA returns now canonicalized per RFC-0105, Merkle root and leaf hashes recomputed |
| 2.8 | 2026-03-19 | Added gas budget proof, extended probe entries 16-23 (non-normative) |
| 2.9 | 2026-03-19 | Adversarial review fixes: CRIT-1/2/4/6, HIGH-2/4/5/6, MED-4/5 |
| 2.10 | 2026-03-19 | Adversarial review fixes: NEW-1/2, CRIT-3, HIGH-3, MED-6, LOW-2/5 |
| 2.11 | 2026-03-19 | Adversarial review fixes: CRIT-3 logic error, NEW-A/B/C/D, LOW-6 |
| 2.12 | 2026-03-19 | Advisory review fixes: Issue 1-4 documentation clarity |
