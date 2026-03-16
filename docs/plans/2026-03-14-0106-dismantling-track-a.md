# RFC-0106 Dismantling — Track A: Critical Fixes Design

**Date:** 2026-03-14
**Status:** Design Approved

## Overview

This document defines Track A of the RFC-0106 (Deterministic Numeric Tower) dismantling effort. Track A addresses critical contradictions between RFC-0106 and the final RFC-0104 (DFP) and RFC-0105 (DQA) specifications. These contradictions would cause consensus fork risks if not resolved.

## Critical Contradictions and Resolutions

### Issue 1: DFP Overflow/Underflow Semantics

| Aspect | RFC-0106 | RFC-0104 Final | Resolution |
|--------|----------|----------------|-------------|
| Overflow | TRAP | SATURATION to DFP_MAX_MANTISSA | **SATURATION** |
| Underflow | TRAP | SATURATION to DFP_MIN_NORMAL | **SATURATION** |
| Division by Zero | TRAP | SATURATE to MAX with sign | **SATURATION** |

**Trade-off Analysis:**

| Approach | Pros | Cons |
|----------|------|------|
| TRAP | Explicit failure detection, simple gas model | Diverges on overflow — consensus fork risk |
| SATURATION | Converges to MAX, consistent state | May produce large values silently |

**Rationale:** SATURATION adopted because:
1. Financial use cases prefer continuation over failure
2. Deterministic state hash is preserved
3. Matches 0104's explicit design decision to "prevent NaN poisoning"

---

### Issue 2: NaN/Infinity Policy

| Aspect | RFC-0106 | RFC-0104 Final | Resolution |
|--------|----------|----------------|-------------|
| NaN | Forbidden | Canonical NaN allowed | **ALLOW + DQA LUT indexing** |
| Infinity | Forbidden | Canonical Infinity allowed | **ALLOW + SATURATION** |

**Trade-off Analysis:**

| Approach | Pros | Cons |
|----------|------|------|
| Forbidden | Avoids LUT FP indexing problem | Contradicts 0104, mathematically dishonest |
| Allowed | Mathematically honest, enables verification | Requires DQA-based LUT indexing |

**Rationale:** Adopt 0104's canonical NaN BUT enforce DQA-based LUT indexing:

```rust
// WRONG — floating-point arithmetic (forbidden)
let idx = ((x + 4.0) / 0.01).round();

// CORRECT — DQA arithmetic (required)
let idx = (x_scaled + 400) / 1;  // where x_scaled = x * 100
```

This resolves the original 0106 concern (LUT indexing determinism) without contradicting 0104.

---

### Issue 3: DQA Multiplication Rounding

| Aspect | RFC-0106 | RFC-0105 Final | Resolution |
|--------|----------|----------------|-------------|
| Rounding Mode | Floor (toward -∞) | Round-to-Nearest-Even (RNE) | **RNE** |

**Trade-off Analysis:**

| Approach | Pros | Cons |
|----------|------|------|
| Floor | Simple (arithmetic right-shift) | Statistical negative bias over many ops |
| RNE | Industry standard, unbiased | Requires tie-handling logic |

**Rationale:** RNE adopted because:
1. Financial regulations prefer unbiased rounding
2. Industry standard for quantitative finance
3. 0105 provides explicit RNE algorithm with remainder handling

---

### Issue 4: DQA Scale Support

| Aspect | RFC-0106 | RFC-0105 Final | Resolution |
|--------|----------|----------------|-------------|
| Supported Scales | Q8.8 only (scale=8) | Scales 0-18 | **0-18** |
| Non-Q8.8 | TRAP | Allowed | **Allow with domain docs** |

**Rationale:** Full scale support (0-18) adopted because:
1. Financial use cases need different scales (prices: 8, quantities: 4)
2. i64 handles all scales uniformly
3. Must explicitly document SQRT domain: `sqrt(x)` requires `x >= 0`

**SQRT Domain Rule:**
```
SQRT(x): If x < 0, result is undefined. Implementations MUST:
  - TRAP on negative input, OR
  - Return canonical NaN
```

---

### Issue 5: DECIMAL vs DQA Overlap

| Type | Internal | Scale Range | Resolution |
|------|----------|-------------|------------|
| DQA | i64 | 0-18 | Keep — high-performance default |
| DECIMAL | i128 | 0-18 | Keep — extended precision |
| BIGDECIMAL (future) | i256 | TBD | Revisit when use case emerges |

**Rationale:** Keep both because:
1. DQA (i64) is faster for most operations
2. DECIMAL (i128) needed for high-precision financial calculations
3. Don't premature optimize — wait for concrete i256 use case

---

## Action Items

### 1. RFC-0104 (DFP) Errata

None required. RFC-0106 must be amended to match 0104's saturation semantics.

### 2. RFC-0105 (DQA) Clarification

Add explicit SQRT domain documentation:
```
SQRT Domain: x >= 0 (required for deterministic index calculation)
Negative input: TRAP or return canonical NaN
```

### 3. RFC-0106 Amendment

Create amendment document (RFC-0106-amend-1) with:
- [x] Overflow: SATURATION (not TRAP)
- [x] Division by Zero: SATURATION to MAX (not TRAP)
- [x] NaN: Allowed with canonical form + DQA LUT indexing requirement
- [x] Infinity: Allowed with saturation semantics
- [x] DQA: RNE rounding (not Floor), scales 0-18 (not Q8.8)
- [x] DECIMAL vs DQA: Documented distinction

### 4. Track B: New Modular RFCs

| RFC | Title | Dependencies |
|-----|-------|--------------|
| 0110 | Deterministic BIGINT | Base |
| 0111 | Deterministic DECIMAL | 0105 |
| 0112 | Deterministic Vectors (DVEC) | 0104, 0105 |
| 0113 | Deterministic Matrices (DMAT) | 0112 |
| 0114 | Deterministic Activation Functions | 0112 |
| 0115 | Deterministic Tensors (DTENSOR) | 0113 |

---

## Summary

Track A resolves all critical contradictions between RFC-0106 and RFC-0104/0105:

1. **DFP Overflow** → SATURATION (matches 0104)
2. **NaN/Infinity** → ALLOWED with DQA LUT indexing (solves original concern)
3. **DQA Rounding** → RNE (matches 0105)
4. **DQA Scales** → 0-18 with explicit domain docs
5. **DECIMAL** → Keep as i128 companion to DQA (i64)

Once Track B RFCs are drafted, RFC-0106 will be archived as "Superseded by 0110-0115".
