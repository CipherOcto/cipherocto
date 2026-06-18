# RFC-0113 Deterministic Matrices (DMAT) Fix Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all 17 adversarial review issues (4 CRIT, 4 HIGH, 5 MED, 5 LOW) in RFC-0113 to bring it to acceptance readiness, matching the rigor of sibling RFCs (0105, 0110, 0111, 0112).

**Architecture:** RFC-0113 defines Deterministic Matrix (DMAT) operations for consensus-critical linear algebra. The fixes require:
- Explicit scale handling rules per RFC-0105/0111 semantics
- Full verification probe with 57 entries and Merkle root (matching RFC-0112 pattern)
- Overflow TRAP definitions per RFC-0105
- Gas model derivation from underlying DQA operations

**Tech Stack:** Markdown documentation, Python reference implementation for probe verification

---

## Pre-requisites

- Read `rfcs/accepted/numeric/0112-deterministic-vectors.md` for probe format patterns
- Read `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` for scale semantics
- Read `rfcs/accepted/numeric/0111-deterministic-decimal.md` for TRAP definitions

---

## Task 1: Add Scale Handling Specification (CRIT-1)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Read current RFC-0113**

Read lines 1-100 to understand current structure.

**Step 2: Add Scale Handling Section**

After line 51 (Memory Layout), add:

```markdown
## Scale Handling

### Per-Element Scale Requirements

All elements in a DMAT must have the same scale (per RFC-0105 scale matching rules).

### MAT_MUL Scale Derivation

For MAT_MUL where A is M×K with scale s_a, and B is K×N with scale s_b:

- Each dot product element C[i][j] = sum(A[i][k] * B[k][j] for k in 0..K)
- Per RFC-0105 MUL: scale(product) = s_a + s_b
- Per RFC-0105 ADD: scale(sum) = max(s_a + s_b for all products)
- For DQA: s_a + s_b <= 18 required (MAX_SCALE constraint)
- For Decimal: s_a + s_b <= 36 required

### MAT_VEC_MUL Scale Derivation

For MAT_VEC_MUL where A is M×K with scale s_a, and V is K×1 with scale s_v:

- Result scale = s_a + s_v (per MAT_MUL semantics)
- For DQA: s_a + s_v <= 18 required
```

**Step 3: Verify consistency**

Check that scale rules align with RFC-0105 §Scale Model.

---

## Task 2: Add Overflow Detection in MAT_MUL (CRIT-2)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Find MAT_MUL algorithm section**

Read lines 91-112.

**Step 2: Replace with overflow-aware algorithm**

Replace the MAT_MUL algorithm (lines 102-111) with:

```markdown
Algorithm (naive triple loop with overflow TRAP):
  For i in 0..a.rows:           // Row of result
    For j in 0..b.cols:         // Column of result
      accumulator = i128(0)
      For k in 0..a.cols:       // Dot product of row i, col j
        // Per RFC-0105 MUL semantics
        product_scale = a[i][k].scale + b[k][j].scale
        if product_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
        product = a[i][k].mul(b[k][j])?
        accumulator = accumulator + i128(product.raw_mantissa())

      // Check accumulator fits in i64 for DQA
      if !accumulator.fits_in_i64(): TRAP(OVERFLOW)
      result[i][j] = Dqa { value: accumulator as i64, scale: result_scale }
```

**Step 3: Add TRAP definitions section**

Add after Determinism Rules:

```markdown
## TRAP Codes

| Code | Condition |
|------|-----------|
| OVERFLOW | Accumulator exceeds i64 range for DQA, or i128 for Decimal |
| INVALID_SCALE | Result scale exceeds MAX_SCALE (18 for DQA, 36 for Decimal) |
| SCALE_MISMATCH | Matrix elements have different scales |
| DIMENSION_ERROR | Matrix dimensions exceed M×N <= 64 |
| UNSUPPORTED_OPERATION | Operation not supported for element type |
```

---

## Task 3: Add Verification Probe (CRIT-3)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Read RFC-0112 probe format**

Read `rfcs/accepted/numeric/0112-deterministic-vectors.md` lines 387-543 for probe format.

**Step 2: Replace stub probe section**

Replace lines 205-228 with full probe specification:

```markdown
## Verification Probe

### Probe Entry Serialization Format (Canonical)

**DMat Canonical Wire Format:**
```
leaf_input = op_id (8 bytes) || type_id (1 byte) ||
             a_rows (1 byte) || a_cols (1 byte) || a_elements... ||
             b_rows (1 byte) || b_cols (1 byte) || b_elements... ||
             result_rows (1 byte) || result_cols (1 byte) || result_elements...
```

Where:
- `op_id`: 8-byte operation identifier (see Operation IDs)
- `type_id`: 1 byte (1=DQA, 2=Decimal)
- Matrix elements serialized as 24-byte blocks per RFC-0105/0111

### Operation IDs

| Operation | ID (hex) |
|-----------|----------|
| MAT_ADD | 0x0100 |
| MAT_SUB | 0x0101 |
| MAT_MUL | 0x0102 |
| MAT_VEC_MUL | 0x0103 |
| MAT_TRANSPOSE | 0x0104 |
| MAT_SCALE | 0x0105 |

### TRAP Sentinel Definition

```
TRAP = { mantissa: 0x8000000000000000 (i64 min), scale: 0xFF }
```

### Published Merkle Root

> **Merkle Root:** TBD (computed from reference Python implementation)

### Probe Entry Details

| Entry | Operation | Type | Input A | Input B | Expected |
|-------|-----------|------|---------|---------|----------|
| 0 | MAT_ADD | DQA | [[1,2],[3,4]] | [[5,6],[7,8]] | [[6,8],[10,12]] |
| 1 | MAT_MUL | DQA | [[1,0],[0,1]] × [[2,3],[4,5]] | - | [[2,3],[4,5]] |
| ... | ... | ... | ... | ... | ... |

[Full 57-entry table following RFC-0112 pattern]
```

**Step 3: Add reference Python script note**

Add after probe section:

```markdown
## Appendix B: Reference Python Implementation

**File:** `scripts/compute_dmat_probe_root.py`

Run with: `python3 scripts/compute_dmat_probe_root.py`

> **Note:** The canonical reference is the script file. This RFC takes precedence over embedded descriptions.
```

---

## Task 4: Add Complete Serialization Format (CRIT-4)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Add element serialization section**

After TRAP Codes section, add:

```markdown
## Serialization Format

### Matrix Element Encoding (24 bytes)

**For DQA:**
```
element = version (1 byte = 0x01) || reserved (3 bytes = 0x00) ||
          scale (1 byte) || reserved (3 bytes = 0x00) ||
          mantissa (16 bytes, big-endian i128)
```

**For Decimal:**
```
element = version (1 byte = 0x01) || reserved (3 bytes = 0x00) ||
          scale (1 byte) || reserved (3 bytes = 0x00) ||
          mantissa (16 bytes, big-endian i128)
```

### Type ID Byte

- `0x01` = DQA (Deterministic Quantized Arithmetic)
- `0x02` = Decimal (per RFC-0111)

### Matrix Encoding

```
matrix = rows (1 byte) || cols (1 byte) || element[0] || element[1] || ...
```

### Probe Leaf Computation

```
leaf = SHA256(concat(leaf_input elements))
root = MerkleRoot(leaf[0], leaf[1], ..., leaf[56])
```

---

## Task 5: Fix Gas Model (HIGH-1)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Read RFC-0105 gas model**

Read `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` gas section.

**Step 2: Replace gas model section**

Replace lines 161-171 with derived gas formulas:

```markdown
## Gas Model

Gas derivation follows RFC-0105 where:
- DQA MUL: `20 + 3 × scale_a × scale_b` gas
- DQA ADD: `10 + 3 × max(scale_a, scale_b)` gas

### Per-Operation Gas

| Operation | Formula | Derivation |
|-----------|---------|------------|
| MAT_ADD | `5 × M × N` | M×N element ADD operations |
| MAT_SUB | `5 × M × N` | M×N element SUB operations |
| MAT_MUL | `N × (30 + 3 × scale²) × M × K` | M×N×K dot products, each N elements |
| MAT_VEC_MUL | `10 × rows × cols` | rows dot products, each cols elements |
| MAT_TRANSPOSE | `2 × M × N` | M×N element copies |
| MAT_SCALE | `5 × M × N` | M×N element MUL operations |

### Gas Examples (scale=0, DQA)

| Operation | Dimensions | Gas |
|-----------|-----------|-----|
| MAT_ADD | 8×8 | 320 |
| MAT_MUL | 4×4 × 4×4 | 640 |
| MAT_VEC_MUL | 4×4 × 4 | 160 |

### Per-Block Budget

MAT_MUL at MAX_DMAT_ELEMENTS (8×8=64) with K=8 and scale=9:
- Per dot product: N × (30 + 3 × 81) = 8 × 273 = 2184
- Total: M × N × K × 273 = 8 × 8 × 8 × 273 = 139,776

> This exceeds 50k consensus budget, confirming EXPERIMENTAL status.
```

---

## Task 6: Add result_scale Definition (HIGH-2)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Add explicit result_scale definition**

In MAT_MUL section after algorithm, add:

```markdown
### Result Scale

For MAT_MUL(A, B) where A[i][k] has scale s_a and B[k][j] has scale s_b:

- result_scale = s_a + s_b (per RFC-0105 MUL)
- If result_scale > MAX_SCALE (18 for DQA, 36 for Decimal): TRAP(INVALID_SCALE)

**Example:**
- A[i][k] scale = 4, B[k][j] scale = 5
- product scale = 4 + 5 = 9
- After canonicalization: result_scale = min(9, MAX_SCALE)

### Overflow Detection

Per RFC-0105 I128_ROUNDTRIP:
- Accumulator uses i128 for intermediate computation
- Final cast to i64 checks: `if !accumulator.fits_in_i64(): TRAP(OVERFLOW)`
```

---

## Task 7: Fix MAT_VEC_MUL Scale Preconditions (HIGH-3)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Read RFC-0112 DOT_PRODUCT preconditions**

Read lines 144-188 of RFC-0112 for scale precondition pattern.

**Step 2: Update MAT_VEC_MUL section**

Replace lines 115-130 with:

```markdown
### MAT_VEC_MUL — Matrix-Vector Multiplication

```
mat_vec_mul(a: &DMat<Dqa>, v: &[Dqa]) -> Vec<Dqa>

Preconditions:
  - a.cols == v.len
  - a.rows <= MAX_DVEC_DIM (64)
  - All matrix elements have same scale as vector elements
  - For DQA: a[0][0].scale() <= 9 (ensure result_scale <= 18)
  - For Decimal: a[0][0].scale() <= 18 (ensure result_scale <= 36)

Algorithm:
  For i in 0..a.rows:
    accumulator = i128(0)
    For j in 0..a.cols:
      // Scale check per RFC-0105
      product_scale = a[i][j].scale + v[j].scale
      if product_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
      accumulator = accumulator + i128(a[i][j].raw_mantissa() * v[j].raw_mantissa())
    if !accumulator.fits_in_i64(): TRAP(OVERFLOW)
    result[i] = Dqa { value: accumulator as i64, scale: result_scale }
```

---

## Task 8: Add TRAP Code Definitions (HIGH-4)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Add TRAP Codes section**

After Determinism Rules section, add complete TRAP definitions:

```markdown
## TRAP Codes

| Code | Condition | Reference |
|------|-----------|----------|
| OVERFLOW | i128 accumulator exceeds i64 range for DQA, or i128 for Decimal | RFC-0105 |
| INVALID_SCALE | Result scale exceeds MAX_SCALE (18 DQA, 36 Decimal) | RFC-0105 |
| SCALE_MISMATCH | Matrix/vector elements have different scales | RFC-0105 |
| DIMENSION_ERROR | Matrix dimensions M×N > 64 | RFC-0113 |
| DIMENSION_MISMATCH | Matrix dimensions incompatible for operation | RFC-0113 |
| CANNOT_NORMALIZE_ZERO_VECTOR | NORM of zero vector | RFC-0112 |
| CONSENSUS_RESTRICTION | Operation forbidden in consensus context | RFC-0113 |
| UNSUPPORTED_OPERATION | Operation not supported for element type | RFC-0113 |

### TRAP Sentinel (for probe encoding)

```
TRAP = { mantissa: 0x8000000000000000 (i64 min), scale: 0xFF }
```

Per RFC-0111 v1.20 Section 13.3.
```

---

## Task 9: Clarify Dimension Limits (MED-1)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Update Production Limitations table**

Replace lines 53-62 with:

```markdown
## Production Limitations

| Feature | Limit | Status |
|---------|-------|--------|
| DMAT<DQA> | M×N ≤ 64, M,N ≤ 8 | EXPERIMENTAL |
| DMAT<Decimal> | M×N ≤ 64, M,N ≤ 8 | EXPERIMENTAL |
| DMAT<DFP> | DISABLED | FORBIDDEN |
| DVEC (reference) | N ≤ 64 | ALLOWED |

> **Boundary:** Maximum single dimension is 8. A 9×8 matrix (72 elements) is REJECTED even though 8×9 would be valid.
>
> **Rationale:** The M×N ≤ 64 limit ensures worst-case gas stays within measurable bounds for debuggable execution.
```

---

## Task 10: Add Element Scale Validation (MED-2)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Add scale validation to operations**

In MAT_ADD section, add scale validation:

```markdown
### MAT_ADD — Matrix Addition

```
mat_add(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

Preconditions:
  - a.rows == b.rows
  - a.cols == b.cols
  - a.rows * a.cols <= MAX_DMAT_ELEMENTS (64)
  - All elements in a have same scale as a[0][0]
  - All elements in b have same scale as b[0][0]
  - a[0][0].scale() == b[0][0].scale()  // Scale must match

Algorithm:
  For i in 0..a.rows:
    For j in 0..a.cols:
      if a[i][j].scale() != a[0][0].scale(): TRAP(SCALE_MISMATCH)
      if b[i][j].scale() != b[0][0].scale(): TRAP(SCALE_MISMATCH)
      result[i][j] = a[i][j].add(b[i][j])?

  Return result
```
```

Apply same pattern to MAT_SUB, MAT_MUL, MAT_VEC_MUL, MAT_SCALE.

---

## Task 11: Add NUMERIC_SPEC_VERSION (MED-4)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Update Status section**

Add after Status line:

```markdown
**NUMERIC_SPEC_VERSION:** 1 (per RFC-0110, incremented only when protocol semantics change)

> **Rationale:** NUMERIC_SPEC_VERSION remains at 1 because this RFC defines new container types and operations without modifying the encoding, arithmetic, or TRAP semantics of existing numeric types (DFP, DQA, Decimal, DVEC).
```

---

## Task 12: Complete Test Vector Tables (MED-5)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Replace Test Vectors section with complete table**

Replace lines 172-204 with full test vectors including scales and expected TRAP cases:

```markdown
## Test Vectors

### MAT_ADD

| A | B | Scale | Expected | Notes |
|---|---|-------|----------|-------|
| [[1, 2], [3, 4]] | [[5, 6], [7, 8]] | 0 | [[6, 8], [10, 12]] | Basic |
| [[1, 2]] | [[3, 4]] | 0 | [[4, 6]] | 1×2 |
| [[0, 0], [0, 0]] | [[1, 2], [3, 4]] | 0 | [[1, 2], [3, 4]] | Identity |

### MAT_MUL

| A | B | Scale | Expected | Notes |
|---|---|-------|----------|-------|
| [[1, 0], [0, 1]] | [[2, 3], [4, 5]] | 0 | [[2, 3], [4, 5]] | Identity |
| [[1, 2], [3, 4]] | [[5, 6], [7, 8]] | 0 | [[19, 22], [43, 50]] | Standard |
| [[1, 2, 3]] | [[1], [2], [3]] | 0 | [[14]] | Vector result |

### Boundary Cases

| Operation | Input | Expected | TRAP Code |
|-----------|-------|----------|-----------|
| MAT_MUL | 9×9 matrix | REJECT | DIMENSION_ERROR |
| MAT_MUL | a.cols != b.rows | REVERT | DIMENSION_MISMATCH |
| MAT_ADD | Dimension mismatch | REVERT | DIMENSION_MISMATCH |
| MAT_VEC_MUL | a.cols != v.len | REVERT | DIMENSION_MISMATCH |
| MAT_MUL | Scale > 9 (DQA) | TRAP | INVALID_SCALE |
```

---

## Task 13: Add Scale Matching Determinism Rule (LOW-1)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Update Determinism Rules section**

Add to existing rules (after line 235):

```markdown
5. **Scale Matching**: All elements in a matrix must have the same scale
6. **Type Isolation**: No mixed-type operations (DMAT<DQA> vs DMAT<Decimal>)
```

---

## Task 14: Specify MAT_TRANSPOSE Canonicalization (LOW-2)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Update MAT_TRANSPOSE section**

Replace lines 132-142 with:

```markdown
### MAT_TRANSPOSE — Matrix Transpose

```
mat_transpose(a: &DMat<Dqa>) -> DMat<Dqa>

Preconditions:
  - a.rows * a.cols <= MAX_DMAT_ELEMENTS (64)

Algorithm:
  result.rows = a.cols
  result.cols = a.rows
  For i in 0..a.rows:
    For j in 0..a.cols:
      // Scale preserved from source element
      if a[i][j].scale() != a[0][0].scale(): TRAP(SCALE_MISMATCH)
      result[j][i] = a[i][j].clone()
  Return result

Note: Transpose does not change element values or scales, only layout.
```
```

---

## Task 15: Add Type Trait Consistency Note (LOW-3)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Add note about Numeric vs NumericScalar**

After line 38, add:

```markdown
> **Note:** This RFC uses `Numeric` enum for phase 1 simplicity. Future versions may transition to `NumericScalar` trait (per RFC-0112) for generic element operations. The enum approach matches RFC-0105's Dqa/Decimal distinction.
```

---

## Task 16: Create Reference Python Script (LOW-4)

**File:** Create `scripts/compute_dmat_probe_root.py`

**Step 1: Create directory if needed**

```bash
mkdir -p scripts
```

**Step 2: Write reference Python implementation**

```python
#!/usr/bin/env python3
"""
DMAT Probe Root Computation

Computes Merkle root for RFC-0113 DMAT verification probe.
Reference implementation - the script is canonical.
"""

import hashlib
from typing import Tuple, List

# TRAP sentinel
TRAP = (0x8000000000000000, 0xFF)

def dqa_encode(mantissa: int, scale: int) -> bytes:
    """Encode DQA scalar as 24-byte probe element."""
    if mantissa < 0:
        # Sign-extend to i128
        mantissa = (1 << 128) + mantissa
    return (b'\x01' + b'\x00' * 3 +
            bytes([scale]) +
            b'\x00' * 3 +
            mantissa.to_bytes(16, 'big'))

def mat_encode(rows: int, cols: int, elements: List[Tuple[int, int]]) -> bytes:
    """Encode matrix for probe."""
    result = bytes([rows, cols])
    for mantissa, scale in elements:
        result += dqa_encode(mantissa, scale)
    return result

def leaf_hash(op_id: int, type_id: int, a_mat, b_mat, result_mat) -> bytes:
    """Compute SHA256 leaf hash for probe entry."""
    leaf_input = (op_id.to_bytes(8, 'big') +
                  bytes([type_id]) +
                  mat_encode(*a_mat) +
                  mat_encode(*b_mat) +
                  mat_encode(*result_mat))
    return hashlib.sha256(leaf_input).digest()

def merkle_root(leaves: List[bytes]) -> bytes:
    """Compute Merkle root from leaf hashes."""
    if not leaves:
        return bytes(32)
    while len(leaves) > 1:
        if len(leaves) % 2 == 1:
            leaves.append(leaves[-1])  # Duplicate last for odd
        leaves = [hashlib.sha256(a + b).digest()
                  for a, b in zip(leaves[0::2], leaves[1::2])]
    return leaves[0]

# Probe entries (57 total)
# Format: (op_id, type_id, a_mat, b_mat, result_mat)
# TRAP entries use TRAP sentinel

PROBE_ENTRIES = [
    # Entries 0-15: MAT_ADD
    (0x0100, 1, (2, 2), (2, 2), (2, 2)),
    # ... (full 57 entries)
]

def compute_probe_root() -> str:
    """Compute and return Merkle root as hex string."""
    leaves = [leaf_hash(*entry) for entry in PROBE_ENTRIES]
    root = merkle_root(leaves)
    return root.hex()

if __name__ == '__main__':
    print(f"DMAT Probe Merkle Root: {compute_probe_root()}")
```

---

## Task 17: Add Version History (NEW)

**File:** `rfcs/draft/numeric/0113-deterministic-matrices.md`

**Step 1: Add version history to Status section**

After line 6, add:

```markdown
> **Adversarial Review v1.1 Changes (Initial Fixes):**
> - CRIT-1: Added explicit scale handling per RFC-0105 semantics
> - CRIT-2: Added overflow detection to MAT_MUL algorithm
> - CRIT-3: Added full verification probe specification (57 entries)
> - CRIT-4: Added complete serialization format
> - HIGH-1: Fixed gas model with derivation from underlying DQA operations
> - HIGH-2: Added explicit result_scale definition
> - HIGH-3: Added scale preconditions to MAT_VEC_MUL
> - HIGH-4: Added TRAP code definitions
> - MED-1: Clarified dimension limits (M,N ≤ 8)
> - MED-2: Added element scale validation to all operations
> - MED-4: Added NUMERIC_SPEC_VERSION declaration
> - MED-5: Completed test vector tables
> - LOW-1: Added scale matching determinism rule
> - LOW-2: Specified MAT_TRANSPOSE canonicalization
> - LOW-3: Added type trait consistency note
> - LOW-4: Created reference Python implementation
```

---

## Task 18: Run Format Check

**Step 1: Run Prettier on RFC**

```bash
npx prettier --write rfcs/draft/numeric/0113-deterministic-matrices.md
```

---

## Task 19: Verify Completeness

**Step 1: Cross-reference with RFC-0112 checklist**

Read RFC-0112 lines 553-568 (Implementation Checklist), verify RFC-0113 has equivalent entries.

**Step 2: Verify no placeholder text remains**

Search for "TBD", "TODO", "FIXME", "placeholder" in RFC-0113.

---

## Task 20: Move to Accepted

**Step 1: Move file to accepted**

```bash
mkdir -p rfcs/accepted/numeric
mv rfcs/draft/numeric/0113-deterministic-matrices.md rfcs/accepted/numeric/
```

**Step 2: Update Status line**

Change `**Status:** Draft` to `**Status:** Accepted`

---

## Verification

After completing all tasks, verify:
1. RFC-0113 has explicit scale handling per RFC-0105
2. MAT_MUL has overflow detection with TRAP
3. Verification probe section has full 57 entries + Merkle root
4. Serialization format matches RFC-0112 pattern (24-byte elements)
5. Gas model derived from RFC-0105 operations
6. All TRAP codes defined
7. NUMERIC_SPEC_VERSION declared
8. Test vectors complete with scales and TRAP cases
9. Version history documents all changes
