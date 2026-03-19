# RFC-0114 (Numeric/Math): Deterministic Activation Functions

## Status

**Version:** 2.2 (2026-03-19)
**Status:** Draft

> **Note:** This RFC defines deterministic, consensus-safe neural network activation functions for AI inference. All LUT-based functions use a strict binary/decimal separation: Q8.8 binary fixed-point for LUT storage, DQA decimal fixed-point for execution.

## Summary

Defines deterministic activation functions for consensus-safe AI inference using DQA arithmetic.

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
| RFC-0112 (DVEC) | Element-wise application |
| RFC-0113 (DMAT) | TRAP propagation invariant |
| RFC-0126 (Serialization) | Canonical serialization format |

## Normative Dependencies

This RFC is **normatively dependent** on RFC-0105 for:
- DqaEncoding format (16 bytes: i64 value + u8 scale + 7 reserved)
- Canonicalization rules
- TRAP sentinel definition
- DQA arithmetic operations

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

**Rounding**: Floor division toward zero. No floating-point allowed.

### Example

```
sigmoid(4.0) → LUT[800] = 251 (Q8.8)
251 * 10^4 // 256 = 9804
Dqa(9804, 4) = 0.9804 (≈ sigmoid(4.0))
```

---

# Activation Functions

## 1. ReLU

```
relu(x: Dqa) -> Dqa

if x.value < 0:
    return Dqa(0, x.scale)
else:
    return x
```

- **Gas**: 2
- **Properties**: Exact, no approximation, no scale change

## 2. ReLU6

```
relu6(x: Dqa) -> Dqa

max_val = Dqa(6 * 10^x.scale, x.scale)

if x.value < 0:
    return Dqa(0, x.scale)
else if x > max_val:
    return max_val
else:
    return x
```

- **Gas**: 3

> **Note**: `max_val` is a DQA value created with `Dqa(6 * 10^x.scale, x.scale)`. The comparison `x > max_val` uses DQA comparison semantics.

## 3. LeakyReLU

```
leaky_relu(x: Dqa, alpha: Dqa = Dqa(1, 2)) -> Dqa

if x.value < 0:
    return multiply(x, alpha)  // RFC-0105 safe multiply
else:
    return x
```

- **Gas**: 3

## 4. Sigmoid

```
sigmoid(x: Dqa) -> Dqa

if x is TRAP: return TRAP

// Phase 1: Normalize to scale=2
x_norm = normalize_to_scale(x, 2)

// Phase 2: Clamp
if x_norm.value < -400: return Dqa(0, 8)
if x_norm.value > 400:  return Dqa(256, 8)

// Phase 3: Index
idx = x_norm.value + 400

// Phase 4: LUT lookup → Q8.8 → DQA
q = LUT_SIGMOID[idx]
result_value, result_scale = (q * 10^4) // 256, 4
return Dqa(result_value, result_scale)
```

- **Gas**: 10

## 5. Tanh

```
tanh(x: Dqa) -> Dqa

if x is TRAP: return TRAP

// Phase 1: Normalize to scale=2
x_norm = normalize_to_scale(x, 2)

// Phase 2: Clamp
if x_norm.value < -400: return Dqa(-256, 8)
if x_norm.value > 400:  return Dqa(256, 8)

// Phase 3: Index
idx = x_norm.value + 400

// Phase 4: LUT lookup → Q8.8 → DQA
q = LUT_TANH[idx]
result_value, result_scale = (q * 10^4) // 256, 4
return Dqa(result_value, result_scale)
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

| x | idx | Q8.8 | DQA (scale=4) | Real value |
|---|-----|------|----------------|------------|
| -4.00 | 0 | 5 | Dqa(195, 4) | 0.0195 |
| -2.00 | 200 | 31 | Dqa(1211, 4) | 0.1211 |
| 0.00 | 400 | 128 | Dqa(5000, 4) | 0.5000 |
| 2.00 | 600 | 225 | Dqa(8789, 4) | 0.8789 |
| 4.00 | 800 | 251 | Dqa(9804, 4) | 0.9804 |

### Tanh

| x | idx | Q8.8 | DQA (scale=4) | Real value |
|---|-----|------|----------------|------------|
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

---

# Domain Handling

| Input Range | Sigmoid | Tanh |
|-------------|---------|------|
| x < -4.0 | Dqa(0, 8) | Dqa(-256, 8) |
| -4.0 ≤ x ≤ 4.0 | LUT + conversion | LUT + conversion |
| x > 4.0 | Dqa(256, 8) | Dqa(256, 8) |

---

# Gas Model

| Operation | Gas | Notes |
|-----------|-----|-------|
| ReLU | 2 | Comparison + select |
| ReLU6 | 3 | Two comparisons + select |
| LeakyReLU | 3 | Comparison + multiply |
| Sigmoid | 10 | Normalize + clamp + index + lookup + convert |
| Tanh | 10 | Normalize + clamp + index + lookup + convert |

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

## Entries

| Index | Description | Serialized Value |
|-------|-------------|-------------------|
| 0 | relu(5.0) | Dqa(500, 2) |
| 1 | relu(-5.0) | Dqa(0, 2) |
| 2 | relu6(10.0) | Dqa(600, 2) → clamp to Dqa(600, 2) = 6.00 |
| 3 | relu6(3.0) | Dqa(300, 2) = 3.00 |
| 4 | sigmoid(0.0) | Dqa(5000, 4) |
| 5 | sigmoid(4.0) | Dqa(9804, 4) |
| 6 | sigmoid(-4.0) | Dqa(195, 4) |
| 7 | tanh(0.0) | Dqa(0, 4) |
| 8 | tanh(2.0) | Dqa(9649, 4) |
| 9 | tanh(-2.0) | Dqa(-9649, 4) |
| 10 | leaky_relu(-1.0) | Dqa(-1, 4) = -0.01 |
| 11 | leaky_relu(1.0) | Dqa(1, 4) = 1.00 |
| 12 | First 4 sigmoid LUT entries | Raw Q8.8 bytes (8 bytes) |
| 13 | First 4 tanh LUT entries | Raw Q8.8 bytes (8 bytes) |
| 14 | Normalization invariant | Dqa(12340, 3) = 12.340 |
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
MERKLE_ROOT = "7a4b3b434b104f33ff823b988d28723fd24730b25f6784fd03d090c0be991eed"
```

Computed as: `root = SHA256(concat(SHA256(leaf[0]), SHA256(leaf[1]), ..., SHA256(leaf[15])))`

### Probe Leaf Hashes

| Index | SHA256(leaf) |
|-------|--------------|
| 0 | 8519ae2801063e3c6a11a4a5d6fe1df6b8f2fd0d6c99ca02848754e1c20ca967 |
| 1 | c571327cb01ac1de6972713cbf6cc1fc3c2cab8b581ee0bc3fe6d8b56963fd5b |
| 2 | 90b34dafb27d7a4806bd5c9fe22a9b266ff436b264a459961e7968d1798b5844 |
| 3 | a9ea18b47683e3a8951439227d8fe29a334c0c37cdfd1f47ef25272cd0a47273 |
| 4 | 74fe94dd1546dfe27d0b8792cb7fc9eae4c099c76f9b3455f346184ca80c2e62 |
| 5 | 06347beb2def35cc4ca2ac894210b148801ba8cb353b9e69c19ae385410e892f |
| 6 | caa8797b8ffa52a9f014c703c8a117fd5bb50d2b32e4b706a2a5ce8b22e20eb8 |
| 7 | f7548c023e431138b11357593f5cceb9dd35eb0b0a2041f0b1560212eeb6f13e |
| 8 | 86dd24f74a250b9938d8231dc6b8329a2b3a0f939085dcf91000ecb1a0dfb61a |
| 9 | 18f133183a6e1bf4eb38c78c0eaeb7f6e2d5e438a4320a5b28dc118e8c88922a |
| 10 | 0954885b7fff6b6ee4a8c04ea74ac109b4ee40fe7fbb8db62157b6678dc585b7 |
| 11 | 81ad0f1a6c467a9d7eadd221465f1353ff319c422603adf37afb633b6899f978 |
| 12 | 4b26af1ad1298d65cf3f851befa62c9dcc4d350fc890d1012bcc109aa33d2af4 |
| 13 | e2cfc77ad4cf3961435d11f523d3dfede302f828cc4f1edb6ab5557f6e6e81bc |
| 14 | 6575060c15e5f2cb44fb9fafedf6bc29b000751248d0ae96a7e0d653d7110f99 |
| 15 | 4451be616d27073d2a040621795ffb90e89aa639b91594cc54e4018453ef9ed7 |

---

# Reference Script

See `scripts/compute_dact_probe_root.py` — authoritative implementation for:
- LUT generation
- SHA-256 commitment computation
- Probe leaf serialization
- Merkle root verification

> **Normative Authority**: This RFC text takes precedence for consensus behavior. The reference script is provided for verification and conformance testing.

---

# Implementation Checklist

- [x] ReLU — exact, no LUT needed
- [x] ReLU6 — exact, uses DQA comparison
- [x] LeakyReLU — exact, uses DQA multiply
- [x] Sigmoid LUT — 801 entries, Q8.8, SHA-256 committed
- [x] Tanh LUT — 801 entries, Q8.8, SHA-256 committed
- [x] Q8.8 → DQA conversion (floor division)
- [x] TRAP sentinel (canonical 16-byte encoding)
- [x] Phase ordering (TRAP → normalize → clamp → index → lookup → convert)
- [x] Merkle probe (16 entries)
- [ ] Independent reproducibility verification

---

# References

- [RFC-0105: Deterministic Quant Arithmetic](../accepted/numeric/0105-deterministic-quant-arithmetic.md)
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
