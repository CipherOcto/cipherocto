# Mission: BigInt Core Algorithms (Phase 1-3)

## Status
Completed

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement core BigInt arithmetic algorithms: ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR with full deterministic specification. This mission implements the core numeric tower type per RFC-0110.

## Architecture

### Data Structure
```rust
/// Deterministic BIGINT representation
/// Uses little-endian u64 limbs
pub struct BigInt {
    /// Little-endian limbs, least significant first
    /// No leading zero limbs (canonical form)
    limbs: Vec<u64>,
    /// Sign: true = negative, false = positive
    sign: bool,
}

/// Canonical form invariants:
/// 1. No leading zero limbs
/// 2. Zero represented as single zero limb with sign = false (NOT empty limbs)
/// 3. Minimum number of limbs for the value
```

### Constants (RFC-0110 §Constants)
```rust
/// Maximum bit width for BIGINT operations
const MAX_BIGINT_BITS: usize = 4096;

/// Maximum number of 64-bit limbs
/// 4096 bits / 64 bits = 64 limbs
const MAX_LIMBS: usize = 64;

/// Maximum gas cost per BIGINT operation (worst case)
const MAX_BIGINT_OP_COST: u64 = 15000;
```

## Phase 1: ADD, SUB, CMP (Entry-Level)

### Acceptance Criteria
- [ ] BigInt struct with Vec<u64> limbs + sign: bool
- [ ] ZERO constant: `BigInt { limbs: vec![0], sign: false }`
- [ ] is_zero() function: `x.limbs == [0] && x.sign == false`
- [ ] canonicalize() function enforcing all three invariants
- [ ] magnitude_cmp() for unsigned comparison of |a| vs |b|
- [ ] ADD algorithm with signed arithmetic + canonicalization
- [ ] SUB algorithm with signed arithmetic + canonicalization
- [ ] CMP returning -1, 0, or +1

### ADD Algorithm (RFC-0110 §ADD)
```
bigint_add(a: BigInt, b: BigInt) -> BigInt

1. If a.sign == b.sign:
   result_sign = a.sign
   result_limbs = limb_add(a.limbs, b.limbs) // with carry propagation
2. If a.sign != b.sign:
   cmp = magnitude_cmp(a.limbs, b.limbs)
   if cmp == 0: return ZERO
   if cmp > 0:  result_sign = a.sign, result_limbs = limb_sub(a.limbs, b.limbs)
   if cmp < 0:  result_sign = b.sign, result_limbs = limb_sub(b.limbs, a.limbs)

3. result = BigInt { limbs: result_limbs, sign: result_sign }
4. return canonicalize(result)
```

### SUB Algorithm (RFC-0110 §SUB)
```
bigint_sub(a: BigInt, b: BigInt) -> BigInt

1. Negate b: b_neg = BigInt { limbs: b.limbs, sign: !b.sign }
2. return bigint_add(a, b_neg)
```

### CMP Algorithm (RFC-0110 §CMP)
```
bigint_cmp(a: BigInt, b: BigInt) -> i32

1. If a.sign != b.sign: return -1 if a.sign else +1
2. cmp = magnitude_cmp(a.limbs, b.limbs)
3. return -cmp if a.sign else cmp  // flip for negative values
```

### 128-bit Intermediate Arithmetic (REQUIRED)
All limb arithmetic MUST use 128-bit intermediate precision:
```rust
sum = (a_limb as u128) + (b_limb as u128) + (carry as u128);
result_limb = sum as u64;
carry = (sum >> 64) as u64;
```

## Phase 2: MUL (Intermediate)

### Acceptance Criteria
- [ ] MUL: schoolbook O(n²) multiplication with canonicalization
- [ ] Upper-carry handling with |= (NOT assignment)
- [ ] Post-MUL canonicalization
- [ ] MAX_BIGINT_BITS overflow check (TRAP if exceeded)

### MUL Algorithm (RFC-0110 §MUL)
```
bigint_mul(a: BigInt, b: BigInt) -> BigInt

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS

1. result_limbs = vec![0; a.limbs.len() + b.limbs.len()]

2. For each limb i in a.limbs:
   For each limb j in b.limbs:
     // 128-bit intermediate multiplication
     product = (a.limbs[i] as u128) * (b.limbs[j] as u128);
     low = product as u64;
     high = (product >> 64) as u64;

     // Add to result with carry propagation
     k = i + j;
     sum = (result_limbs[k] as u128) + (low as u128) + (carry as u128);
     result_limbs[k] = sum as u64;
     carry = (sum >> 64) as u64;

     // Upper carry (USE |= NOT =)
     result_limbs[k+1] |= high;
     result_limbs[k+1] |= carry;

3. result = BigInt {
     limbs: result_limbs,
     sign: a.sign != b.sign,  // XOR for product sign
   }

4. if result.bits() > MAX_BIGINT_BITS: TRAP

5. return canonicalize(result)
```

### MUL Determinism Rule (CRITICAL)
- NO Karatsuba multiplication
- NO SIMD vectorized operations
- NO hardware carry flags
- Schoolbook O(n²) algorithm ONLY

## Phase 3: DIV, MOD (Advanced)

### Acceptance Criteria
- [ ] DIV: binary long division (Knuth Algorithm D) with canonicalization
- [ ] MOD: remainder operation using divmod
- [ ] Division uses exactly `a_norm.limbs.len()` outer iterations (NO early exit)
- [ ] D1 normalization with double restore per Knuth Algorithm D
- [ ] MAX_BIGINT_BITS overflow check

### bigint_divmod Algorithm (RFC-0110 §bigint_divmod)
```
bigint_divmod(a: BigInt, b: BigInt) -> (BigInt, BigInt)

Preconditions:
  - a.bits() <= MAX_BIGINT_BITS
  - b.bits() <= MAX_BIGINT_BITS
  - b != ZERO
  - b.limbs.len <= MAX_LIMBS

Algorithm: Restoring division with D1 normalization

1. If |a| < |b|: return (ZERO, a)
   // When |a| < |b|: quotient = 0, remainder = a (preserving a's sign)

2. Normalize: Shift b left until MSB is 1
   norm_shift = count_leading_zeros(b.limbs[b.limbs.len - 1])
   b_norm = b << norm_shift
   a_norm = a << norm_shift
   // CRITICAL: a_norm MUST be a fresh copy (not in-place modification)

3. Initialize quotient limbs: vec![0; a_norm.limbs.len()]

4. Main loop (for j from a_norm.limbs.len - 1 down to 0):
   a. Form estimate (D1):
      if j == 0:
        // Degenerate single-limb case
        q_estimate = (a_norm.limbs[0] as u128) / (b_norm.limbs[b_norm.limbs.len - 1] as u128)
      else if a_norm.limbs[j] == b_norm.limbs[b_norm.limbs.len - 1]:
        q_estimate = 0xFFFF_FFFF_FFFF_FFFFu128
      else:
        // Standard D1: ((r[j] << 64) | r[j-1]) / d[m-1]
        q_estimate = ((a_norm.limbs[j] as u128) << 64 |
                      a_norm.limbs[j-1] as u128) /
                      b_norm.limbs[b_norm.limbs.len - 1] as u128

   b. Clamp estimate to u64 max

   c. Multiply and subtract (restoring):
      // In-place subtraction with borrow tracking
      // Use two-step overflowing_sub to prevent borrow overflow

   d. If subtraction overflowed: restore and decrement q_estimate

5. Apply signs:
   quotient.sign = a.sign != b.sign  // XOR
   remainder.sign = a.sign  // remainder inherits dividend sign

6. return (canonicalize(quotient), canonicalize(remainder))
```

### DIV Determinism Rule (CRITICAL)
- Division MUST execute exactly `a_norm.limbs.len()` outer iterations
- NO early exit permitted
- This equals `ceil(bitlen(a_norm) / 64)` and may exceed `ceil(bitlen(a) / 64)` by one

## Phase 4: SHL, SHR (Advanced)

### Acceptance Criteria
- [ ] SHL: left shift with overflow TRAP if result exceeds MAX_BIGINT_BITS
- [ ] SHR: right shift preserving sign (arithmetic shift)
- [ ] Shift amounts validated (0 <= shift < MAX_BIGINT_BITS)
- [ ] Canonicalization after shift

### SHL Algorithm (RFC-0110 §SHL)
```
bigint_shl(a: BigInt, shift: usize) -> BigInt

Preconditions:
  - 0 < shift < MAX_BIGINT_BITS
  - a.bits() + shift <= MAX_BIGINT_BITS

1. limb_shift = shift / 64
2. bit_shift = shift % 64

3. result_limbs = vec![0; a.limbs.len() + limb_shift + 1]

4. For each limb i in a.limbs:
   result_limbs[i + limb_shift] |= a.limbs[i] << bit_shift
   if bit_shift > 0 && i + limb_shift + 1 < result_limbs.len():
     result_limbs[i + limb_shift + 1] |= a.limbs[i] >> (64 - bit_shift)

5. if result.bits() > MAX_BIGINT_BITS: TRAP

6. result = BigInt { limbs: result_limbs, sign: a.sign }
7. return canonicalize(result)
```

### SHR Algorithm (RFC-0110 §SHR)
```
bigint_shr(a: BigInt, shift: usize) -> BigInt

Preconditions:
  - 0 <= shift < MAX_BIGINT_BITS

1. If shift == 0: return canonicalize(a)

2. limb_shift = shift / 64
3. bit_shift = shift % 64

4. result_limbs = vec![0; a.limbs.len() - limb_shift]

5. For each limb i in result_limbs:
   result_limbs[i] = a.limbs[i + limb_shift] >> bit_shift
   if bit_shift > 0 && i + limb_shift + 1 < a.limbs.len():
     result_limbs[i] |= a.limbs[i + limb_shift + 1] << (64 - bit_shift)

6. result = BigInt { limbs: result_limbs, sign: a.sign }
7. return canonicalize(result)
```

## Shared Requirements (All Phases)

### Determinism Rules (RFC-0110 §Determinism Rules)
1. **Algorithm Locked**: All implementations MUST use the algorithms specified
2. **No Karatsuba**: Multiplication uses schoolbook O(n²) algorithm
3. **No SIMD**: Vectorized operations are forbidden
4. **Fixed Iteration**: Division executes exactly `a_norm.limbs.len()` outer iterations, no early exit
5. **Determinism Over Constant-Time**: Consensus determinism ≠ constant-time
6. **No Hardware**: CPU carry flags, SIMD, or FPU are forbidden
7. **Post-Operation Canonicalization**: Every algorithm MUST call canonicalize before returning

### Overflow Handling
- MAX_BIGINT_BITS = 4096 (64 limbs)
- Any operation resulting in bits > 4096 MUST TRAP
- See RFC-0110 §Boundary Cases for MAX, overflow, and edge cases

### Gas Model (Informative)
- Worst-case 64-limb DIV + canonicalization ≤ 15,000 gas
- MAX_BIGINT_OP_COST = 15,000

## Implementation Location
- **Crate**: Create `determin/` crate (per DFP pattern, outside workspace to avoid circular dependency)
- **File**: `determin/src/bigint.rs`
- **Entry point**: `determin/src/lib.rs` exports BigInt

## Dependencies
- None (pure Rust implementation)

## Testing Requirements
- Unit tests for each operation
- Boundary case tests: MAX, zero, negative zero, overflow
- Integration with 56-entry probe (Phase 5)

## Compiler Flags (CRITICAL)
- Use `release` profile (overflow checks OFF)
- Do NOT use debug profile for BigInt operations
- LTO enabled for optimization
- Run tests in release mode: `cargo test --release`

## Reference
- RFC-0110: Deterministic BIGINT (§Data Structure, §Canonical Form, §Algorithms)
- RFC-0110: Deterministic BIGINT (§Determinism Rules)
- RFC-0110: Deterministic BIGINT (§Constants)
- missions/claimed/0104-dfp-core-type.md (DFP pattern for structure)

## Complexity
High — Division (Knuth Algorithm D) is the most complex component

## Claimant
@claude-code (AI Agent)
