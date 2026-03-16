//! BigInt: Deterministic Arbitrary-Precision Integer Implementation
//!
//! Implements RFC-0110: Deterministic BIGINT
//!
//! Key design principles:
//! - Canonical form: no leading zeros, zero = {limbs: [0], sign: false}
//! - 128-bit intermediate arithmetic for carry/borrow
//! - TRAP on overflow (result exceeds MAX_BIGINT_BITS)
//! - Explicit canonicalization after every operation
//!
//! ## Implementation Fixes Log
//!
//! This section documents fixes applied during implementation for future reference.
//! See source code for details of each fix.
//!
//! ### Phase 4: Conversions & Serialization (2026-03-15)
//!
//! - TryFrom signature: Changed from fn try_from(&BigInt) to fn try_from(BigInt)
//! - i64::MIN handling: Changed from i64::MAX.unsigned_abs() to i64::MIN.unsigned_abs()
//! - clippy unnecessary_cast: Removed redundant as u128 cast
//! - clippy needless_range_loop: Changed to iterator with enumerate
//! - bit_length on u128: Used 128 - leading_zeros() instead of non-existent method
//!
//! ### Phase 5: Verification Probe (2026-03-15)
//!
//! - Entry 52: Changed from BigIntProbeValue::Max to BigIntProbeValue::Int(MAX_U64 as i128)
//! - clippy manual_div_ceil: Changed (num_bits + 63) / 64 to num_bits.div_ceil(64)
//! - clippy needless_borrows: Removed & from hasher.update() calls
//! - Merkle root verification: Added BIGINT_REFERENCE_MERKLE_ROOT constant
//!
//! Reference: scripts/compute_bigint_probe_root.py for Python reference implementation

use serde::{Deserialize, Serialize};

/// Maximum bit width for BIGINT operations (4096 bits)
pub const MAX_BIGINT_BITS: usize = 4096;

/// Maximum number of 64-bit limbs (4096 / 64 = 64)
pub const MAX_LIMBS: usize = 64;

/// Maximum gas cost per BIGINT operation (worst case)
pub const MAX_BIGINT_OP_COST: u64 = 15000;

/// BigInt errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BigIntError {
    /// Result exceeds MAX_BIGINT_BITS
    Overflow,
    /// Division by zero (b == ZERO)
    DivisionByZero,
    /// Input fails canonicalization check
    NonCanonicalInput,
    /// Value out of i128 range for conversion
    OutOfI128Range,
    /// Value out of range for target type (i64/u64)
    OutOfRange,
}

/// Deterministic BIGINT representation
/// Uses little-endian u64 limbs
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BigInt {
    /// Little-endian limbs, least significant first
    /// No leading zero limbs (canonical form)
    limbs: Vec<u64>,
    /// Sign: true = negative, false = positive
    sign: bool,
}

impl BigInt {
    /// Create a new BigInt with the given limbs and sign
    /// Caller should ensure input is canonical or call canonicalize()
    pub fn new(limbs: Vec<u64>, sign: bool) -> Self {
        debug_assert!(!limbs.is_empty(), "BigInt limbs must not be empty");
        BigInt { limbs, sign }
    }

    /// Get the limbs (little-endian)
    pub fn limbs(&self) -> &[u64] {
        &self.limbs
    }

    /// Get the sign (true = negative, false = positive)
    pub fn sign(&self) -> bool {
        self.sign
    }

    /// Check if this BigInt is zero
    /// RFC-0110: is_zero(x) = x.limbs == [0] && x.sign == false
    pub fn is_zero(&self) -> bool {
        self.limbs == [0] && !self.sign
    }

    /// Get the number of limbs
    pub fn len(&self) -> usize {
        self.limbs.len()
    }

    /// Check if the BigInt is empty (shouldn't happen for canonical values)
    pub fn is_empty(&self) -> bool {
        self.limbs.is_empty()
    }

    /// Create a canonical zero BigInt
    pub fn zero() -> Self {
        BigInt {
            limbs: vec![0],
            sign: false,
        }
    }
}

impl BigInt {
    /// Canonical form enforcement
    /// RFC-0110 Canonical Form:
    /// 1. No leading zero limbs
    /// 2. Zero represented as single zero limb with sign = false
    /// 3. Minimum number of limbs for the value
    pub fn canonicalize(mut self) -> Self {
        // Remove leading zero limbs
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }

        // Canonical zero: {limbs: [0], sign: false}
        if self.limbs == [0] {
            self.sign = false;
        }

        self
    }

    /// Canonical form check (for deserialization)
    pub fn is_canonical(&self) -> bool {
        // No leading zero limbs
        if self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            return false;
        }
        // Zero must have sign = false
        if self.limbs == [0] && self.sign {
            return false;
        }
        true
    }

    /// Compute bit length (number of bits needed to represent)
    /// RFC-0110: bit_length() returns the position of the most significant bit + 1
    pub fn bit_length(&self) -> usize {
        if self.is_zero() {
            return 1; // RFC: bit_length(0) = 1
        }

        let last_limb = *self.limbs.last().unwrap();
        let limb_bits = 64 - last_limb.leading_zeros() as usize;

        // Add bits from lower limbs
        let lower_limb_bits = (self.limbs.len() - 1) * 64;

        lower_limb_bits + limb_bits
    }

    /// Compare absolute values (magnitudes)
    /// RFC-0110 magnitude_cmp: returns -1 if |a| < |b|, 0 if equal, +1 if |a| > |b|
    pub fn magnitude_cmp(&self, other: &BigInt) -> i32 {
        use std::cmp::Ordering;

        // Compare limb counts
        match self.limbs.len().cmp(&other.limbs.len()) {
            Ordering::Greater => return 1,
            Ordering::Less => return -1,
            Ordering::Equal => {}
        }

        // Compare from most significant limb
        for i in (0..self.limbs.len()).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Greater => return 1,
                Ordering::Less => return -1,
                Ordering::Equal => continue,
            }
        }

        // All limbs equal
        0
    }

    /// Compare two BigInt values
    /// RFC-0110: CMP returns -1, 0, or +1
    pub fn compare(&self, other: &BigInt) -> i32 {
        // Different signs: negative < positive
        if self.sign != other.sign {
            return if self.sign { -1 } else { 1 };
        }

        // Same sign: compare magnitudes, then flip if negative
        let mag_cmp = self.magnitude_cmp(other);
        if self.sign {
            -mag_cmp // Flip for negative values
        } else {
            mag_cmp
        }
    }
}

// =============================================================================
// ADD — Addition
// RFC-0110 §ADD
// =============================================================================

/// Add two BigInt values
/// RFC-0110: bigint_add(a: BigInt, b: BigInt) -> BigInt
pub fn bigint_add(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    // RFC: TRAP on non-canonical input
    if !a.is_canonical() || !b.is_canonical() {
        return Err(BigIntError::NonCanonicalInput);
    }

    // Handle same sign addition
    if a.sign == b.sign {
        let result_limbs = limb_add(&a.limbs, &b.limbs);
        let result = BigInt {
            limbs: result_limbs,
            sign: a.sign,
        };
        let result = result.canonicalize();

        // Check overflow
        if result.bit_length() > MAX_BIGINT_BITS {
            return Err(BigIntError::Overflow);
        }

        return Ok(result);
    }

    // Different signs: subtract magnitudes
    let cmp = a.magnitude_cmp(&b);

    if cmp == 0 {
        // |a| == |b| => result is zero
        return Ok(BigInt::zero());
    }

    let (result_limbs, result_sign) = if cmp > 0 {
        // |a| > |b|: result = |a| - |b|, sign = a.sign
        (limb_sub(&a.limbs, &b.limbs), a.sign)
    } else {
        // |a| < |b|: result = |b| - |a|, sign = b.sign
        (limb_sub(&b.limbs, &a.limbs), b.sign)
    };

    let result = BigInt {
        limbs: result_limbs,
        sign: result_sign,
    };
    let result = result.canonicalize();

    Ok(result)
}

/// Add two limb vectors (same sign)
fn limb_add(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut result = vec![0; std::cmp::max(a.len(), b.len()) + 1];
    let mut carry = 0u128;

    for (i, slot) in result.iter_mut().enumerate() {
        let a_val = a.get(i).copied().unwrap_or(0) as u128;
        let b_val = b.get(i).copied().unwrap_or(0) as u128;
        let sum = a_val + b_val + carry;
        *slot = sum as u64;
        carry = sum >> 64;
    }

    result
}

// =============================================================================
// SUB — Subtraction
// RFC-0110 §SUB
// =============================================================================

/// Subtract two BigInt values: a - b
/// RFC-0110: bigint_sub(a: BigInt, b: BigInt) -> BigInt
pub fn bigint_sub(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    // Negate b and add
    let b_neg = BigInt {
        limbs: b.limbs,
        sign: !b.sign,
    };
    bigint_add(a, b_neg)
}

// =============================================================================
// Limb subtraction (a >= b, magnitudes)
// =============================================================================

/// Subtract limb vectors where |a| >= |b|
fn limb_sub(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut result = vec![0u64; a.len()];
    let mut borrow: u64 = 0;

    for i in 0..a.len() {
        let b_val = b.get(i).copied().unwrap_or(0);
        let (d1, borrow1) = a[i].overflowing_sub(b_val);
        let (d2, borrow2) = d1.overflowing_sub(borrow);
        result[i] = d2;
        borrow = (borrow1 as u64) | (borrow2 as u64);
    }

    result
}

// =============================================================================
// MUL — Multiplication
// RFC-0110 §MUL
// =============================================================================

/// Multiply two BigInt values
/// RFC-0110: bigint_mul(a: BigInt, b: BigInt) -> BigInt
/// Uses schoolbook O(n²) multiplication - NO Karatsuba, NO SIMD
pub fn bigint_mul(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    // RFC: TRAP on non-canonical input
    if !a.is_canonical() || !b.is_canonical() {
        return Err(BigIntError::NonCanonicalInput);
    }

    // Handle zero early
    if a.is_zero() || b.is_zero() {
        return Ok(BigInt::zero());
    }

    // Preconditions per RFC
    if a.bit_length() > MAX_BIGINT_BITS || b.bit_length() > MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
    }

    let result_limbs = limb_mul(&a.limbs, &b.limbs);

    let result = BigInt {
        limbs: result_limbs,
        sign: a.sign != b.sign, // XOR for product sign
    };

    let result = result.canonicalize();

    // Check overflow
    if result.bit_length() > MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
    }

    Ok(result)
}

/// Schoolbook multiplication O(n²)
/// Uses 128-bit intermediate arithmetic
fn limb_mul(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut result = vec![0u64; a.len() + b.len()];

    for (i, &ai) in a.iter().enumerate() {
        for (j, &bj) in b.iter().enumerate() {
            // 128-bit intermediate multiplication
            let product = (ai as u128) * (bj as u128);
            let low = product as u64;
            let high = (product >> 64) as u64;

            let k = i + j;

            // Add low part to result[k] with carry
            let acc = (result[k] as u128) + (low as u128);
            result[k] = acc as u64;
            let mut carry = (acc >> 64) + (high as u128);

            // Propagate carry to result[k+1], result[k+2], ...
            let mut k2 = k + 1;
            while carry > 0 {
                debug_assert!(k2 < result.len());
                let s = (result[k2] as u128) + carry;
                result[k2] = s as u64;
                carry = s >> 64;
                k2 += 1;
            }
        }
    }

    result
}

// =============================================================================
// DIV — Division
// RFC-0110 §bigint_divmod
// =============================================================================

/// Divide two BigInt values and return (quotient, remainder).
///
/// RFC-0110: bigint_divmod(a, b) -> (BigInt, BigInt)
/// Algorithm: Knuth Vol.2 §4.3.1 Algorithm D (multi-precision division).
/// Iteration count: exactly m+1 outer iterations where m = dividend.len() - divisor.len() —
/// no early exit (Determinism Rule 4).
pub fn bigint_divmod(a: BigInt, b: BigInt) -> Result<(BigInt, BigInt), BigIntError> {
    // RFC: TRAP on non-canonical input
    if !a.is_canonical() || !b.is_canonical() {
        return Err(BigIntError::NonCanonicalInput);
    }

    // Division by zero
    if b.is_zero() {
        return Err(BigIntError::DivisionByZero);
    }

    // Preconditions
    if a.bit_length() > MAX_BIGINT_BITS || b.bit_length() > MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
    }

    // |a| < |b| → quotient = 0, remainder = a (sign of a preserved)
    if a.magnitude_cmp(&b) < 0 {
        return Ok((BigInt::zero(), a));
    }

    // Single-limb divisor fast path
    let (q_limbs, r_limbs) = if b.limbs.len() == 1 {
        knuth_single_limb_div(&a.limbs, b.limbs[0])
    } else {
        knuth_d(&a.limbs, &b.limbs)
    };

    // Apply signs — BEFORE canonicalize (Determinism Rule 7)
    let q_sign = a.sign != b.sign; // XOR
    let r_sign = a.sign; // remainder sign matches dividend

    let quotient = BigInt {
        limbs: q_limbs,
        sign: q_sign,
    }
    .canonicalize();
    let remainder = BigInt {
        limbs: r_limbs,
        sign: r_sign,
    }
    .canonicalize();

    Ok((quotient, remainder))
}

/// Divide dividend by a single-limb divisor.
/// Returns (quotient_limbs, remainder_limbs).
/// O(n) where n = dividend.len().
fn knuth_single_limb_div(dividend: &[u64], divisor: u64) -> (Vec<u64>, Vec<u64>) {
    debug_assert!(divisor != 0);

    let mut remainder: u128 = 0;
    let mut result = vec![0u64; dividend.len()];

    // Process from most-significant to least-significant
    for i in (0..dividend.len()).rev() {
        let current = (remainder << 64) | (dividend[i] as u128);
        result[i] = (current / divisor as u128) as u64;
        remainder = current % divisor as u128;
    }

    // Trim quotient leading zeros
    while result.len() > 1 && *result.last().unwrap() == 0 {
        result.pop();
    }

    let rem_limbs = if remainder == 0 {
        vec![0u64]
    } else {
        vec![remainder as u64]
    };

    (result, rem_limbs)
}

/// Knuth Algorithm D — multi-precision division.
///
/// Preconditions (enforced by caller):
///   - dividend.len() >= divisor.len() >= 2
///   - divisor.last() != 0  (canonical)
///   - |dividend| >= |divisor|
///
/// Returns (quotient_limbs, remainder_limbs), both positive (unsigned).
/// Signs are applied by bigint_divmod after this function returns.
///
/// Algorithm reference: Knuth TAOCP Vol.2, §4.3.1 Algorithm D.
/// Fixed iteration count: exactly (dividend.len() - divisor.len() + 1)
/// outer iterations — no early exit.
fn knuth_d(dividend: &[u64], divisor: &[u64]) -> (Vec<u64>, Vec<u64>) {
    const BASE: u128 = 1u128 << 64;

    let n = divisor.len(); // divisor digit count (n >= 2)
    let m = dividend.len() - n; // quotient has m+1 digits

    // D1: Normalize — shift divisor left until its MSB is 1.
    // d_shift = number of leading zero bits in divisor[n-1].
    let d_shift = divisor[n - 1].leading_zeros() as usize;

    // v = normalized divisor (n limbs).
    // u = normalized dividend (n + m + 1 limbs).
    // When d_shift == 0, these are copies; no bits are moved.
    let v = shl_limbs_n(divisor, d_shift, n);
    let mut u = shl_limbs_n(dividend, d_shift, n + m + 1);

    debug_assert_eq!(v.len(), n);
    debug_assert_eq!(u.len(), n + m + 1);
    debug_assert!(
        v[n - 1] >= (1u64 << 63),
        "MSB of v must be 1 after normalization"
    );

    let mut q = vec![0u64; m + 1];

    // D2-D7: Main loop — exactly m+1 iterations, no early exit.
    // j counts DOWN from m to 0 (most-significant quotient digit first).
    for j in (0..=m).rev() {
        // D3: Calculate trial quotient digit q_hat.
        //
        // u[j+n] and u[j+n-1] are the two most significant words of the
        // current partial remainder at offset j.
        let u_top = u[j + n] as u128;
        let u_mid = u[j + n - 1] as u128;
        let v_top = v[n - 1] as u128;
        let v_next = v[n - 2] as u128; // safe: n >= 2

        let mut q_hat: u128 = if u_top == v_top {
            // q_hat = BASE - 1 (Knuth: this is the maximum possible value)
            BASE - 1
        } else {
            // Standard two-digit estimate
            (u_top * BASE + u_mid) / v_top
        };

        // D3 refinement: correct q_hat by at most 2 via Knuth's 3-digit test.
        // This guarantees q_hat - true_digit ∈ {0, 1, 2}.
        {
            let u_low = if j + n >= 2 { u[j + n - 2] as u128 } else { 0 };
            // while q_hat*v[n-2] > BASE*(u_top*BASE+u_mid - q_hat*v_top) + u[j+n-2]
            loop {
                let rhat = u_top * BASE + u_mid - q_hat * v_top;
                if rhat >= BASE {
                    break; // rhat overflows: q_hat is already correct
                }
                if q_hat * v_next > BASE * rhat + u_low {
                    q_hat -= 1;
                } else {
                    break;
                }
            }
        }

        // D4: Multiply and subtract: u[j..j+n+1] -= q_hat * v.
        //
        // Two-pass approach using pure u128 arithmetic to avoid i128 overflow
        // when q_hat * v[i] > 2^127.
        {
            // Pass 1: Compute q_hat * v into qv[]
            let mut qv = vec![0u64; n + 1];
            let mut mul_carry: u128 = 0;
            for i in 0..n {
                let prod = q_hat * (v[i] as u128) + mul_carry;
                qv[i] = prod as u64;
                mul_carry = prod >> 64;
            }
            qv[n] = mul_carry as u64;

            // Pass 2: Subtract qv[] from u[j..j+n+1] with overflow tracking
            let mut sub_borrow: u64 = 0;
            for i in 0..=n {
                let (d1, b1) = u[j + i].overflowing_sub(qv[i]);
                let (d2, b2) = d1.overflowing_sub(sub_borrow);
                u[j + i] = d2;
                sub_borrow = (b1 as u64) | (b2 as u64);
            }

            if sub_borrow != 0 {
                // D6: Add back — q_hat was 1 too large (probability ~2/BASE).
                q_hat -= 1;
                let mut add_carry: u128 = 0;
                for i in 0..n {
                    let s = u[j + i] as u128 + v[i] as u128 + add_carry;
                    u[j + i] = s as u64;
                    add_carry = s >> 64;
                }
                u[j + n] = u[j + n].wrapping_add(add_carry as u64);
            }
        }

        q[j] = q_hat as u64;
    }

    // D8: Denormalize remainder.
    // The remainder is u[0..n] shifted right by d_shift bits.
    let rem = shr_limbs_n(&u[..n], d_shift);

    // Trim leading zeros from quotient
    while q.len() > 1 && *q.last().unwrap() == 0 {
        q.pop();
    }

    (q, rem)
}

/// Shift a limb slice left by `shift` bits and return exactly `output_len` limbs.
/// Limbs are little-endian. Extra high limbs are zero-extended.
/// When shift == 0, returns a copy truncated or zero-padded to output_len.
fn shl_limbs_n(limbs: &[u64], shift: usize, output_len: usize) -> Vec<u64> {
    let mut out = vec![0u64; output_len];
    if shift == 0 {
        let copy_len = limbs.len().min(output_len);
        out[..copy_len].copy_from_slice(&limbs[..copy_len]);
        return out;
    }
    let rshift = 64 - shift;
    for (i, &v) in limbs.iter().enumerate() {
        if i < output_len {
            out[i] |= v << shift;
        }
        if i + 1 < output_len {
            out[i + 1] |= v >> rshift;
        }
    }
    out
}

/// Shift a limb slice right by `shift` bits.
/// Returns canonical result (no leading zero limbs, at least one limb).
fn shr_limbs_n(limbs: &[u64], shift: usize) -> Vec<u64> {
    if shift == 0 {
        let mut r = limbs.to_vec();
        while r.len() > 1 && *r.last().unwrap() == 0 {
            r.pop();
        }
        return r;
    }
    let lshift = 64 - shift;
    let mut out = vec![0u64; limbs.len()];
    for i in 0..limbs.len() {
        out[i] = limbs[i] >> shift;
        if i + 1 < limbs.len() {
            out[i] |= limbs[i + 1] << lshift;
        }
    }
    while out.len() > 1 && *out.last().unwrap() == 0 {
        out.pop();
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

/// Division: a / b (quotient only)
pub fn bigint_div(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    Ok(bigint_divmod(a, b)?.0)
}

/// Modulo: a % b (remainder only)
/// RFC-0110: remainder sign matches dividend (same as RFC-0105 convention).
pub fn bigint_mod(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    Ok(bigint_divmod(a, b)?.1)
}

// =============================================================================
// SHL — Left Shift
// RFC-0110 §SHL
// =============================================================================

/// Left shift: a << shift
/// RFC-0110: bigint_shl(a: BigInt, shift: usize) -> BigInt
pub fn bigint_shl(a: BigInt, shift: usize) -> Result<BigInt, BigIntError> {
    // RFC: TRAP on non-canonical input
    if !a.is_canonical() {
        return Err(BigIntError::NonCanonicalInput);
    }

    // shift == 0 is a no-op, return a
    if shift == 0 {
        return Ok(a);
    }

    // Check overflow
    if a.bit_length() + shift > MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
    }

    let result = bigint_shl_internal(&a.limbs, shift, a.sign);
    let result = result.canonicalize();

    Ok(result)
}

/// Internal left shift (assumes validated)
fn bigint_shl_internal(limbs: &[u64], bit_shift: usize, sign: bool) -> BigInt {
    if bit_shift == 0 {
        return BigInt {
            limbs: limbs.to_vec(),
            sign,
        };
    }

    let limb_shift = bit_shift / 64;
    let bit_shift_rem = bit_shift % 64;

    let mut result_limbs = vec![0u64; limbs.len() + limb_shift + 1];

    for (i, &limb) in limbs.iter().enumerate() {
        result_limbs[i + limb_shift] |= limb << bit_shift_rem;
        if bit_shift_rem > 0 && i + limb_shift + 1 < result_limbs.len() {
            result_limbs[i + limb_shift + 1] = limb >> (64 - bit_shift_rem);
        }
    }

    BigInt {
        limbs: result_limbs,
        sign,
    }
}

// =============================================================================
// SHR — Right Shift
// RFC-0110 §SHR
// =============================================================================

/// Right shift: a >> shift
/// RFC-0110: bigint_shr(a: BigInt, shift: usize) -> BigInt
pub fn bigint_shr(a: BigInt, shift: usize) -> Result<BigInt, BigIntError> {
    // RFC: TRAP on non-canonical input
    if !a.is_canonical() {
        return Err(BigIntError::NonCanonicalInput);
    }

    if shift == 0 {
        return Ok(a);
    }

    // If shifting zero by any amount, return zero
    if a.is_zero() {
        return Ok(BigInt::zero());
    }

    let limb_shift = shift / 64;
    let bit_shift_rem = shift % 64;

    // If shifting more than limb count, result is zero
    if limb_shift >= a.limbs.len() {
        return Ok(BigInt::zero());
    }

    let mut result_limbs = vec![0u64; a.limbs.len() - limb_shift];

    for (i, slot) in result_limbs.iter_mut().enumerate() {
        if bit_shift_rem == 0 {
            *slot = a.limbs[i + limb_shift];
        } else {
            *slot = a.limbs[i + limb_shift] >> bit_shift_rem;
            if i + limb_shift + 1 < a.limbs.len() {
                *slot |= a.limbs[i + limb_shift + 1] << (64 - bit_shift_rem);
            }
        }
    }

    let result = BigInt {
        limbs: result_limbs,
        sign: a.sign,
    };
    let result = result.canonicalize();

    Ok(result)
}

// =============================================================================
// Primitive Conversions
// =============================================================================

use std::convert::{From, TryFrom};

impl From<i64> for BigInt {
    fn from(n: i64) -> BigInt {
        if n == 0 {
            return BigInt::zero();
        }
        let sign = n < 0;
        let mag = n.unsigned_abs();
        BigInt::new(vec![mag], sign).canonicalize()
    }
}

impl TryFrom<BigInt> for i64 {
    type Error = BigIntError;

    fn try_from(b: BigInt) -> Result<i64, Self::Error> {
        if b.limbs.len() > 1 {
            return Err(BigIntError::OutOfRange);
        }
        let mag = b.limbs.first().copied().unwrap_or(0);
        if b.sign {
            // For negative, check against i64::MIN.unsigned_abs()
            // i64::MIN = -9223372036854775808, so unsigned_abs = 9223372036854775808
            if mag > i64::MIN.unsigned_abs() {
                return Err(BigIntError::OutOfRange);
            }
            Ok(-(mag as i64))
        } else {
            if mag > i64::MAX.unsigned_abs() {
                return Err(BigIntError::OutOfRange);
            }
            Ok(mag as i64)
        }
    }
}

impl From<i128> for BigInt {
    fn from(n: i128) -> BigInt {
        if n == 0 {
            return BigInt::zero();
        }
        let sign = n < 0;
        let mag = n.unsigned_abs();
        let lo = mag as u64;
        let hi = (mag >> 64) as u64;
        let limbs = if hi == 0 { vec![lo] } else { vec![lo, hi] };
        BigInt::new(limbs, sign).canonicalize()
    }
}

impl TryFrom<BigInt> for i128 {
    type Error = BigIntError;

    fn try_from(b: BigInt) -> Result<i128, Self::Error> {
        if b.limbs.len() > 2 {
            return Err(BigIntError::OutOfI128Range);
        }
        let lo = b.limbs.first().copied().unwrap_or(0);
        let hi = b.limbs.get(1).copied().unwrap_or(0);
        let mag = ((hi as u128) << 64) | (lo as u128);
        if b.sign {
            // For negative, check against i128::MIN.unsigned_abs()
            if mag > i128::MIN.unsigned_abs() {
                return Err(BigIntError::OutOfI128Range);
            }
            Ok(-(mag as i128))
        } else {
            if mag > i128::MAX.unsigned_abs() {
                return Err(BigIntError::OutOfI128Range);
            }
            Ok(mag as i128)
        }
    }
}

impl From<u64> for BigInt {
    fn from(n: u64) -> BigInt {
        if n == 0 {
            return BigInt::zero();
        }
        BigInt::new(vec![n], false).canonicalize()
    }
}

impl TryFrom<BigInt> for u64 {
    type Error = BigIntError;

    fn try_from(b: BigInt) -> Result<u64, Self::Error> {
        if b.sign {
            return Err(BigIntError::OutOfRange);
        }
        if b.limbs.len() > 1 {
            return Err(BigIntError::OutOfRange);
        }
        Ok(b.limbs.first().copied().unwrap_or(0))
    }
}

impl From<u128> for BigInt {
    fn from(n: u128) -> BigInt {
        if n == 0 {
            return BigInt::zero();
        }
        let lo = n as u64;
        let hi = (n >> 64) as u64;
        let limbs = if hi == 0 { vec![lo] } else { vec![lo, hi] };
        BigInt::new(limbs, false).canonicalize()
    }
}

impl TryFrom<BigInt> for u128 {
    type Error = BigIntError;

    fn try_from(b: BigInt) -> Result<u128, Self::Error> {
        if b.sign {
            return Err(BigIntError::OutOfI128Range);
        }
        if b.limbs.len() > 2 {
            return Err(BigIntError::OutOfI128Range);
        }
        let lo = b.limbs.first().copied().unwrap_or(0);
        let hi = b.limbs.get(1).copied().unwrap_or(0);
        Ok(((hi as u128) << 64) | (lo as u128))
    }
}

// =============================================================================
// Serialization (BigIntEncoding)
// RFC-0110 §BigIntEncoding
// =============================================================================

/// BigInt wire encoding
/// Format: [version: u8, sign: u8, reserved: 2 bytes, num_limbs: u8, reserved: 3 bytes, limbs: little-endian u64[]]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BigIntEncoding {
    /// Version (0x01)
    pub version: u8,
    /// Sign: 0x00 = positive, 0xFF = negative
    pub sign: u8,
    /// Number of limbs
    pub num_limbs: u8,
    /// Limbs (little-endian)
    pub limbs: Vec<u64>,
}

impl BigIntEncoding {
    /// Convert to wire format bytes
    /// Format: [version, sign, 0, 0, num_limbs, 0, 0, 0, limbs...]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.limbs.len() * 8);
        bytes.push(self.version);
        bytes.push(self.sign);
        bytes.push(0); // reserved
        bytes.push(0); // reserved
        bytes.push(self.num_limbs);
        bytes.push(0); // reserved
        bytes.push(0); // reserved
        bytes.push(0); // reserved

        for &limb in &self.limbs {
            bytes.extend_from_slice(&limb.to_le_bytes());
        }

        bytes
    }
}

impl BigInt {
    /// Serialize to BigIntEncoding
    pub fn serialize(&self) -> BigIntEncoding {
        BigIntEncoding {
            version: 0x01,
            sign: if self.sign { 0xFF } else { 0x00 },
            num_limbs: self.limbs.len() as u8,
            limbs: self.limbs.clone(),
        }
    }

    /// Deserialize from BigIntEncoding
    pub fn deserialize(data: &[u8]) -> Result<BigInt, BigIntError> {
        // Minimum length: 8 bytes header + at least 1 limb
        if data.len() < 8 {
            return Err(BigIntError::NonCanonicalInput);
        }
        let version = data[0];
        if version != 0x01 {
            return Err(BigIntError::NonCanonicalInput);
        }
        let sign_byte = data[1];
        if sign_byte != 0x00 && sign_byte != 0xFF {
            return Err(BigIntError::NonCanonicalInput);
        }
        let sign = sign_byte == 0xFF;

        // Validate reserved bytes (bytes 2 and 3 should be 0)
        if data[2] != 0 || data[3] != 0 {
            return Err(BigIntError::NonCanonicalInput);
        }

        // num_limbs is at byte 4
        let num_limbs = data[4] as usize;
        if num_limbs == 0 || num_limbs > 64 {
            return Err(BigIntError::NonCanonicalInput);
        }

        // Validate reserved bytes (bytes 5, 6, 7 should be 0)
        if data[5] != 0 || data[6] != 0 || data[7] != 0 {
            return Err(BigIntError::NonCanonicalInput);
        }

        // Total length: 8 bytes header + num_limbs * 8 bytes
        let expected_len = 8 + num_limbs * 8;
        if data.len() != expected_len {
            return Err(BigIntError::NonCanonicalInput);
        }

        let mut limbs = Vec::with_capacity(num_limbs);
        for i in 0..num_limbs {
            let offset = 8 + i * 8;
            let limb = u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            limbs.push(limb);
        }

        let b = BigInt { limbs, sign };
        if !b.is_canonical() {
            return Err(BigIntError::NonCanonicalInput);
        }
        Ok(b)
    }
}

// =============================================================================
// i128 Round-Trip Conversion
// RFC-0110 §bigint_to_i128_bytes
// =============================================================================

/// Convert BigInt to 16-byte two's complement big-endian representation
/// Precondition: b fits in i128 range
pub fn bigint_to_i128_bytes(b: BigInt) -> Result<[u8; 16], BigIntError> {
    // Check range: -2^127 to 2^127-1
    if b.limbs.len() > 2 {
        return Err(BigIntError::OutOfI128Range);
    }
    if b.limbs.len() == 2 {
        let hi = b.limbs[1];
        if b.sign {
            // For negative, check if magnitude exceeds 2^127
            if hi > 0x8000_0000_0000_0000 {
                return Err(BigIntError::OutOfI128Range);
            }
        } else {
            // For positive, check if magnitude >= 2^127
            if hi >= 0x8000_0000_0000_0000 {
                return Err(BigIntError::OutOfI128Range);
            }
        }
    }

    // Zero case
    if b.is_zero() {
        return Ok([0u8; 16]);
    }

    // Reconstruct magnitude as u128
    let lo = b.limbs.first().copied().unwrap_or(0);
    let hi = b.limbs.get(1).copied().unwrap_or(0);
    let magnitude = ((hi as u128) << 64) | (lo as u128);

    // Convert to two's complement
    let val: u128 = if !b.sign {
        magnitude
    } else {
        // Two's complement: !magnitude + 1
        (!magnitude).wrapping_add(1)
    };

    // Encode as big-endian bytes (per RFC spec)
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = ((val >> (120 - i * 8)) & 0xFF) as u8;
    }

    Ok(bytes)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        assert!(BigInt::zero().is_zero());
        assert!(!BigInt::zero().sign);
        assert_eq!(BigInt::zero().limbs, vec![0]);
    }

    #[test]
    fn test_canonicalize() {
        // Non-canonical: leading zeros
        let x = BigInt::new(vec![1, 0, 0], false);
        let x = x.canonicalize();
        assert_eq!(x.limbs, vec![1]);

        // Non-canonical: negative zero
        let x = BigInt::new(vec![0], true);
        let x = x.canonicalize();
        assert!(!x.sign);
        assert_eq!(x.limbs, vec![0]);
    }

    #[test]
    fn test_add_same_sign() {
        // 1 + 1 = 2
        let a = BigInt::new(vec![1], false);
        let b = BigInt::new(vec![1], false);
        let result = bigint_add(a, b).unwrap();
        assert_eq!(result.limbs, vec![2]);

        // -1 + -1 = -2
        let a = BigInt::new(vec![1], true);
        let b = BigInt::new(vec![1], true);
        let result = bigint_add(a, b).unwrap();
        assert_eq!(result.limbs, vec![2]);
        assert!(result.sign);
    }

    #[test]
    fn test_add_different_sign() {
        // 5 + -3 = 2
        let a = BigInt::new(vec![5], false);
        let b = BigInt::new(vec![3], true);
        let result = bigint_add(a, b).unwrap();
        assert_eq!(result.limbs, vec![2]);

        // 3 + -5 = -2
        let a = BigInt::new(vec![3], false);
        let b = BigInt::new(vec![5], true);
        let result = bigint_add(a, b).unwrap();
        assert_eq!(result.limbs, vec![2]);
        assert!(result.sign);

        // 5 + -5 = 0
        let a = BigInt::new(vec![5], false);
        let b = BigInt::new(vec![5], true);
        let result = bigint_add(a, b).unwrap();
        assert!(result.is_zero());
    }

    #[test]
    fn test_sub() {
        // 5 - 3 = 2
        let a = BigInt::new(vec![5], false);
        let b = BigInt::new(vec![3], false);
        let result = bigint_sub(a, b).unwrap();
        assert_eq!(result.limbs, vec![2]);

        // 3 - 5 = -2
        let a = BigInt::new(vec![3], false);
        let b = BigInt::new(vec![5], false);
        let result = bigint_sub(a, b).unwrap();
        assert_eq!(result.limbs, vec![2]);
        assert!(result.sign);
    }

    #[test]
    fn test_compare() {
        // Positive comparisons
        let a = BigInt::new(vec![5], false);
        let b = BigInt::new(vec![3], false);
        assert_eq!(a.compare(&b), 1);

        let a = BigInt::new(vec![3], false);
        let b = BigInt::new(vec![5], false);
        assert_eq!(a.compare(&b), -1);

        let a = BigInt::new(vec![5], false);
        let b = BigInt::new(vec![5], false);
        assert_eq!(a.compare(&b), 0);

        // Negative comparisons
        let a = BigInt::new(vec![5], true);
        let b = BigInt::new(vec![3], true);
        assert_eq!(a.compare(&b), -1); // -5 < -3

        let a = BigInt::new(vec![3], true);
        let b = BigInt::new(vec![5], true);
        assert_eq!(a.compare(&b), 1); // -3 > -5

        // Cross-sign
        let a = BigInt::new(vec![1], false);
        let b = BigInt::new(vec![1], true);
        assert_eq!(a.compare(&b), 1); // 1 > -1
    }

    #[test]
    fn test_bit_length() {
        assert_eq!(BigInt::zero().bit_length(), 1);
        assert_eq!(BigInt::new(vec![1], false).bit_length(), 1);
        assert_eq!(BigInt::new(vec![2], false).bit_length(), 2);
        assert_eq!(BigInt::new(vec![0xFF], false).bit_length(), 8);
    }

    #[test]
    fn test_mul_basic() {
        // 2 * 3 = 6
        let a = BigInt::new(vec![2], false);
        let b = BigInt::new(vec![3], false);
        let result = bigint_mul(a, b).unwrap();
        assert_eq!(result.limbs, vec![6]);

        // 0 * 5 = 0
        let a = BigInt::zero();
        let b = BigInt::new(vec![5], false);
        let result = bigint_mul(a, b).unwrap();
        assert!(result.is_zero());

        // 5 * 0 = 0
        let a = BigInt::new(vec![5], false);
        let b = BigInt::zero();
        let result = bigint_mul(a, b).unwrap();
        assert!(result.is_zero());
    }

    #[test]
    fn test_mul_cross_sign() {
        // -3 * 4 = -12
        let a = BigInt::new(vec![3], true);
        let b = BigInt::new(vec![4], false);
        let result = bigint_mul(a, b).unwrap();
        assert_eq!(result.limbs, vec![12]);
        assert!(result.sign);

        // -2 * -3 = 6
        let a = BigInt::new(vec![2], true);
        let b = BigInt::new(vec![3], true);
        let result = bigint_mul(a, b).unwrap();
        assert_eq!(result.limbs, vec![6]);
        assert!(!result.sign);
    }

    #[test]
    fn test_mul_64bit_boundary() {
        // (2^32-1) * (2^32-1) = 2^64 - 2^33 + 1 = 0xfffffffe00000001
        let a = BigInt::new(vec![0xFFFFFFFF], false);
        let b = BigInt::new(vec![0xFFFFFFFF], false);
        let result = bigint_mul(a, b).unwrap();
        // Result is 0xfffffffe00000001 which fits in single limb
        assert_eq!(result.limbs, vec![0xfffffffe00000001]);
    }

    #[test]
    fn test_div_basic() {
        // 10 / 3 = 3 (remainder 1)
        let a = BigInt::new(vec![10], false);
        let b = BigInt::new(vec![3], false);
        let result = bigint_div(a, b).unwrap();
        assert_eq!(result.limbs, vec![3]);
    }

    #[test]
    fn test_divmod() {
        // 10 / 3 = 3 remainder 1
        let a = BigInt::new(vec![10], false);
        let b = BigInt::new(vec![3], false);
        let (q, r) = bigint_divmod(a, b).unwrap();
        assert_eq!(q.limbs, vec![3]);
        assert_eq!(r.limbs, vec![1]);
    }

    #[test]
    fn test_mod() {
        // 10 % 3 = 1
        let a = BigInt::new(vec![10], false);
        let b = BigInt::new(vec![3], false);
        let result = bigint_mod(a, b).unwrap();
        assert_eq!(result.limbs, vec![1]);
    }

    #[test]
    fn test_div_by_zero() {
        let a = BigInt::new(vec![10], false);
        let b = BigInt::zero();
        let result = bigint_div(a, b);
        assert!(result.is_err());
    }

    #[test]
    fn test_div_small_dividend() {
        // 3 / 10 = 0 remainder 3
        let a = BigInt::new(vec![3], false);
        let b = BigInt::new(vec![10], false);
        let (q, r) = bigint_divmod(a, b).unwrap();
        assert!(q.is_zero());
        assert_eq!(r.limbs, vec![3]);
    }

    #[test]
    fn test_shl() {
        // 1 << 1 = 2
        let a = BigInt::new(vec![1], false);
        let result = bigint_shl(a, 1).unwrap();
        assert_eq!(result.limbs, vec![2]);
    }

    #[test]
    fn test_shr() {
        // 4 >> 1 = 2
        let a = BigInt::new(vec![4], false);
        let result = bigint_shr(a, 1).unwrap();
        assert_eq!(result.limbs, vec![2]);
    }

    // Phase 4: Conversion Tests
    #[test]
    fn test_from_i64() {
        let cases = vec![
            (0i64, vec![0], false),
            (1, vec![1], false),
            (-1, vec![1], true),
            (42, vec![42], false),
            (-42, vec![42], true),
            (i64::MAX, vec![i64::MAX as u64], false),
            (i64::MIN, vec![i64::MIN.unsigned_abs()], true),
        ];
        for (n, expected_limbs, expected_sign) in cases {
            let bigint = BigInt::from(n);
            assert_eq!(bigint.limbs, expected_limbs, "from_i64({}) limbs", n);
            assert_eq!(bigint.sign, expected_sign, "from_i64({}) sign", n);
        }
    }

    #[test]
    fn test_try_from_i64() {
        // Positive cases - convert BigInt -> i64
        let big0 = BigInt::zero();
        let result: Result<i64, _> = big0.try_into();
        assert_eq!(result, Ok(0i64));

        let big1 = BigInt::from(1i64);
        let result: Result<i64, _> = big1.try_into();
        assert_eq!(result, Ok(1i64));

        let big_neg1 = BigInt::from(-1i64);
        let result: Result<i64, _> = big_neg1.try_into();
        assert_eq!(result, Ok(-1i64));

        let big42 = BigInt::from(42i64);
        let result: Result<i64, _> = big42.try_into();
        assert_eq!(result, Ok(42i64));

        let big_max = BigInt::from(i64::MAX);
        let result: Result<i64, _> = big_max.try_into();
        assert_eq!(result, Ok(i64::MAX));

        let big_min = BigInt::from(i64::MIN);
        let result: Result<i64, _> = big_min.try_into();
        assert_eq!(result, Ok(i64::MIN));

        // Negative cases - too large
        let big = BigInt::new(vec![u64::MAX, u64::MAX], false);
        let result: Result<i64, _> = big.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_from_u64() {
        let cases = vec![
            (0u64, vec![0]),
            (1, vec![1]),
            (42, vec![42]),
            (u64::MAX, vec![u64::MAX]),
        ];
        for (n, expected_limbs) in cases {
            let bigint = BigInt::from(n);
            assert_eq!(bigint.limbs, expected_limbs, "from_u64({}) limbs", n);
            assert!(!bigint.sign, "from_u64({}) should be positive", n);
        }
    }

    #[test]
    fn test_from_i128() {
        let cases = vec![
            (0i128, vec![0], false),
            (1, vec![1], false),
            (-1, vec![1], true),
            // i128::MAX = 0x7FFF...FF (127 ones): lower=u64::MAX, upper=0x7FFFFFFFFFFFFFFF
            (i128::MAX, vec![u64::MAX, 0x7FFFFFFFFFFFFFFF], false),
            // i128::MIN = -0x8000...000 (magnitude has 1 bit at position 127)
            (i128::MIN, vec![0, 0x8000000000000000], true),
        ];
        for (n, expected_limbs, expected_sign) in cases {
            let bigint = BigInt::from(n);
            assert_eq!(bigint.limbs, expected_limbs, "from_i128({}) limbs", n);
            assert_eq!(bigint.sign, expected_sign, "from_i128({}) sign", n);
        }
    }

    #[test]
    fn test_try_from_i128() {
        let big0 = BigInt::zero();
        let result: Result<i128, _> = big0.try_into();
        assert_eq!(result, Ok(0i128));

        let big1 = BigInt::from(1i128);
        let result: Result<i128, _> = big1.try_into();
        assert_eq!(result, Ok(1i128));

        let big_neg1 = BigInt::from(-1i128);
        let result: Result<i128, _> = big_neg1.try_into();
        assert_eq!(result, Ok(-1i128));

        let big_max = BigInt::from(i128::MAX);
        let result: Result<i128, _> = big_max.try_into();
        assert_eq!(result, Ok(i128::MAX));

        let big_min = BigInt::from(i128::MIN);
        let result: Result<i128, _> = big_min.try_into();
        assert_eq!(result, Ok(i128::MIN));

        // Too large magnitude
        let big = BigInt::new(vec![0, 0x8000000000000001], false);
        let result: Result<i128, _> = big.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_from_u128() {
        // u128::MAX = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF (all 128 bits set)
        // Both lower and upper 64 bits are u64::MAX
        let bigint = BigInt::from(u128::MAX);
        assert_eq!(bigint.limbs, vec![u64::MAX, u64::MAX]);
        assert!(!bigint.sign);
    }

    #[test]
    fn test_try_from_u128() {
        let big0 = BigInt::zero();
        let result: Result<u128, _> = big0.try_into();
        assert_eq!(result, Ok(0u128));

        let big1 = BigInt::from(1u128);
        let result: Result<u128, _> = big1.try_into();
        assert_eq!(result, Ok(1u128));

        let big_max = BigInt::from(u128::MAX);
        let result: Result<u128, _> = big_max.try_into();
        assert_eq!(result, Ok(u128::MAX));

        // Too large - needs 3 limbs (exceeds u128)
        let big = BigInt::new(vec![0, 0, 1], false); // 2^128
        let result: Result<u128, _> = big.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_deserialize() {
        // Test serialization to bytes using serde
        let bigint = BigInt::from(42i64);
        let encoded = bigint.serialize();
        // Verify encoding structure
        assert_eq!(encoded.version, 0x01);
        assert_eq!(encoded.sign, 0x00); // positive
        assert_eq!(encoded.num_limbs, 1);
        assert_eq!(encoded.limbs, vec![42]);

        // Test negative
        let bigneg = BigInt::from(-42i64);
        let enc_neg = bigneg.serialize();
        assert_eq!(enc_neg.sign, 0xFF);

        // Test multi-limb
        let big128 = BigInt::from(u128::MAX);
        let enc128 = big128.serialize();
        assert_eq!(enc128.num_limbs, 2);
    }

    #[test]
    fn test_bigint_to_i128_bytes() {
        // Zero
        let bytes = bigint_to_i128_bytes(BigInt::zero()).unwrap();
        assert_eq!(bytes, [0u8; 16]);

        // One (big-endian: 0x00...01)
        let bytes = bigint_to_i128_bytes(BigInt::from(1i64)).unwrap();
        assert_eq!(bytes[15], 1); // Last byte is 1

        // Negative one (big-endian two's complement: all 0xFF)
        let bytes = bigint_to_i128_bytes(BigInt::from(-1i64)).unwrap();
        assert_eq!(bytes, [0xFFu8; 16]);

        // i128::MAX = 0x7FFF...FF
        let bytes = bigint_to_i128_bytes(BigInt::from(i128::MAX)).unwrap();
        assert_eq!(bytes[0], 0x7F);
        assert_eq!(bytes[15], 0xFF);

        // i128::MIN = 0x80...00
        let bytes = bigint_to_i128_bytes(BigInt::from(i128::MIN)).unwrap();
        assert_eq!(bytes[0], 0x80);
        assert_eq!(bytes[1], 0x00);

        // Too large returns error
        let big = BigInt::from(u128::MAX);
        assert!(bigint_to_i128_bytes(big).is_err());
    }
}

// =============================================================================
// RFC-0110 BigInt Regression Tests
//
// Regression coverage for all 7 bugs identified in the code review:
//
//  Bug 1 [CRITICAL] — limb_sub missing borrow propagation
//  Bug 2 [CRITICAL] — limb_mul uses |= instead of proper addition
//  Bug 3 [CRITICAL] — bigint_divmod uses naive repeated subtraction
//  Bug 4 [HIGH]     — Serialization wire format uses wrong byte offsets
//  Bug 5 [HIGH]     — bigint_shr returns Err for large shifts (should return ZERO)
//  Bug 6 [HIGH]     — bigint_shl returns Err for shift == 0 (should return a)
//  Bug 7 [HIGH]     — No input canonicalization enforcement
//
// Each test block is labelled with the bug number it covers.
// All expected values are independently computed and annotated.
// =============================================================================

#[cfg(test)]
mod regression_tests {
    use super::*;

    // =========================================================================
    // Bug 1 — limb_sub: missing borrow propagation across limb boundaries
    // =========================================================================

    /// 2^64 − 1: requires borrow from limb[1] into limb[0]
    #[test]
    fn bug1_sub_borrow_across_limb_boundary_simple() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![1], false);
        let result = bigint_sub(a, b).expect("sub should succeed");

        assert_eq!(
            result.limbs(),
            &[0xFFFF_FFFF_FFFF_FFFF],
            "2^64 - 1 should be a single limb 0xFFFF...FFFF"
        );
        assert!(!result.sign(), "result should be positive");
    }

    /// 2^64 − (2^32 − 1)
    #[test]
    fn bug1_sub_borrow_across_limb_boundary_partial() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![0xFFFF_FFFF], false);
        let result = bigint_sub(a, b).expect("sub should succeed");

        assert_eq!(
            result.limbs(),
            &[0xFFFF_FFFF_0000_0001],
            "2^64 - (2^32-1) = 0xFFFFFFFF00000001"
        );
    }

    /// 2^64 − 2^32
    #[test]
    fn bug1_sub_borrow_across_limb_power_of_two() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![0x0000_0001_0000_0000], false); // 2^32
        let result = bigint_sub(a, b).expect("sub should succeed");

        assert_eq!(result.limbs(), &[0xFFFF_FFFF_0000_0000]);
        assert!(!result.sign());
    }

    /// 2^128 − 1: borrow propagates two levels
    #[test]
    fn bug1_sub_borrow_three_limb_chain() {
        let a = BigInt::new(vec![0, 0, 1], false); // 2^128
        let b = BigInt::new(vec![1], false);
        let result = bigint_sub(a, b).expect("sub should succeed");

        assert_eq!(
            result.limbs(),
            &[0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF_FFFF_FFFF],
            "2^128 - 1 should be two all-ones limbs"
        );
    }

    /// 2^128 − 2^64: borrow through zero limb
    #[test]
    fn bug1_sub_borrow_zero_limb_bridge() {
        let a = BigInt::new(vec![0, 0, 1], false); // 2^128
        let b = BigInt::new(vec![0, 1], false); // 2^64
        let result = bigint_sub(a, b).expect("sub should succeed");

        assert_eq!(
            result.limbs(),
            &[0, 0xFFFF_FFFF_FFFF_FFFF],
            "2^128 - 2^64 = [0, 0xFFFF...FFFF]"
        );
    }

    /// 2 * 2^64 − 1
    #[test]
    fn bug1_sub_borrow_from_second_limb() {
        let a = BigInt::new(vec![0, 2], false); // 2 * 2^64
        let b = BigInt::new(vec![1], false);
        let result = bigint_sub(a, b).expect("sub should succeed");

        assert_eq!(
            result.limbs(),
            &[0xFFFF_FFFF_FFFF_FFFF, 1],
            "2*2^64 - 1 = [MAX_U64, 1]"
        );
    }

    /// add(a, -b) where subtraction requires borrow
    #[test]
    fn bug1_add_dispatches_sub_correctly_with_borrow() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![1], true); // -1 (negative)
        let result = bigint_add(a, b).expect("add should succeed");

        assert_eq!(result.limbs(), &[0xFFFF_FFFF_FFFF_FFFF]);
        assert!(!result.sign());
    }

    // =========================================================================
    // Bug 2 — limb_mul: uses |= instead of proper addition for carry/high
    // =========================================================================

    /// MAX_U64 * MAX_U64
    #[test]
    fn bug2_mul_max_u64_squared() {
        let a = BigInt::new(vec![u64::MAX], false);
        let b = BigInt::new(vec![u64::MAX], false);
        let result = bigint_mul(a, b).expect("mul should succeed");

        // (2^64-1)^2 = 2^128 - 2^65 + 1
        assert_eq!(
            result.limbs(),
            &[0x0000_0000_0000_0001, 0xFFFF_FFFF_FFFF_FFFE],
            "MAX_U64^2 should be [1, 0xFFFFFFFFFFFFFFFE]"
        );
        assert!(!result.sign());
    }

    /// (2^65-1)^2
    #[test]
    fn bug2_mul_two_limb_max_squared() {
        let a2 = BigInt::new(vec![u64::MAX, 1], false); // 2^65 - 1
        let b2 = BigInt::new(vec![u64::MAX, 1], false);
        let result = bigint_mul(a2, b2).expect("mul should succeed");

        // (2^65-1)^2 = 2^130 - 2^66 + 1
        assert_eq!(
            result.limbs(),
            &[1, 0xFFFF_FFFF_FFFF_FFFC, 3],
            "(2^65-1)^2 should be [1, 0xFFFFFFFFFFFFFFFC, 3]"
        );
    }

    /// 2^64 * 2^64 = 2^128
    #[test]
    fn bug2_mul_power_of_two_64_squared() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![0, 1], false); // 2^64
        let result = bigint_mul(a, b).expect("2^64 * 2^64 should not overflow");

        assert_eq!(
            result.limbs(),
            &[0, 0, 1],
            "2^64 * 2^64 = 2^128 should be [0, 0, 1]"
        );
    }

    /// (2^128-1)^2 = 2^256 - 2^129 + 1 fits within MAX_BIGINT_BITS (4096)
    #[test]
    fn bug2_mul_max_two_limb_correct_result() {
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        // This is 256 bits — well within MAX_BIGINT_BITS (4096). Must NOT overflow.
        let a = BigInt::new(vec![u64::MAX, u64::MAX], false); // 2^128 - 1
        let b = BigInt::new(vec![u64::MAX, u64::MAX], false);
        let result = bigint_mul(a, b);

        assert!(
            result.is_ok(),
            "(2^128-1)^2 = 256 bits, must not overflow MAX_BIGINT_BITS=4096"
        );

        let r = result.unwrap();
        // (2^128-1)^2 = 2^256 - 2^129 + 1
        // LE limbs: [1, 0, 0xFFFFFFFFFFFFFFFE, 0xFFFFFFFFFFFFFFFF]
        assert_eq!(
            r.len(), // use public .len(), not private .limbs.len()
            4,
            "(2^128-1)^2 should have exactly 4 limbs"
        );
        assert_eq!(
            r.limbs(),
            &[0x1, 0x0, 0xFFFF_FFFF_FFFF_FFFE, 0xFFFF_FFFF_FFFF_FFFF]
        );
        assert!(!r.sign());
    }

    /// Multiplication by 1 is identity
    #[test]
    fn bug2_mul_single_limb_multiplier_identity() {
        let a = BigInt::new(vec![0xDEAD_BEEF_CAFE_1234, 0x1234_5678_9ABC_DEF0], false);
        let b = BigInt::new(vec![1], false);
        let result = bigint_mul(a, b).expect("mul by 1 should succeed");
        assert_eq!(
            result.limbs(),
            &[0xDEAD_BEEF_CAFE_1234, 0x1234_5678_9ABC_DEF0],
            "multiplying by 1 must be identity"
        );
    }

    // =========================================================================
    // Bug 3 — bigint_divmod: division correctness
    // =========================================================================

    /// 2^64 / 3
    #[test]
    fn bug3_div_two_limb_by_one_limb() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![3], false);
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert_eq!(
            q.limbs(),
            &[0x5555_5555_5555_5555],
            "2^64 / 3 = 0x5555555555555555"
        );
        assert_eq!(r.limbs(), &[1], "2^64 mod 3 = 1");
    }

    /// 2^64 / 2^32
    #[test]
    fn bug3_div_power_of_two_quotient() {
        let a = BigInt::new(vec![0, 1], false); // 2^64
        let b = BigInt::new(vec![0x1_0000_0000], false); // 2^32
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert_eq!(q.limbs(), &[0x1_0000_0000], "2^64 / 2^32 = 2^32");
        assert!(r.is_zero(), "2^64 / 2^32 has zero remainder");
    }

    /// 2^128 / 2^64 = 2^64 (probe entry 20)
    #[test]
    fn bug3_div_2_to_128_by_2_to_64() {
        let a = BigInt::new(vec![0, 0, 1], false); // 2^128
        let b = BigInt::new(vec![0, 1], false); // 2^64
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert_eq!(q.limbs(), &[0, 1], "2^128 / 2^64 = 2^64 = [0, 1]");
        assert!(r.is_zero(), "2^128 / 2^64 remainder is zero");
    }
    /// -7 / 3: quotient negative, remainder negative
    #[test]
    fn bug3_div_negative_dividend() {
        let a = BigInt::new(vec![7], true); // -7
        let b = BigInt::new(vec![3], false); // 3
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert_eq!(q.limbs(), &[2], "|-7 / 3| = 2");
        assert!(q.sign(), "quotient of (-7)/3 should be negative");
        assert_eq!(r.limbs(), &[1], "|-7 % 3| = 1");
        assert!(r.sign(), "remainder sign must match dividend (negative)");
    }

    /// 7 / -3
    #[test]
    fn bug3_div_negative_divisor() {
        let a = BigInt::new(vec![7], false); // 7
        let b = BigInt::new(vec![3], true); // -3
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert_eq!(q.limbs(), &[2]);
        assert!(q.sign(), "quotient of 7/(-3) should be negative");
        assert_eq!(r.limbs(), &[1]);
        assert!(!r.sign(), "remainder sign must match dividend (positive)");
    }

    /// -7 / -3
    #[test]
    fn bug3_div_both_negative() {
        let a = BigInt::new(vec![7], true); // -7
        let b = BigInt::new(vec![3], true); // -3
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert_eq!(q.limbs(), &[2]);
        assert!(!q.sign(), "quotient of (-7)/(-3) should be positive");
        assert_eq!(r.limbs(), &[1]);
        assert!(r.sign(), "remainder sign must match dividend (negative)");
    }

    /// |a| < |b|: quotient = 0, remainder = a
    #[test]
    fn bug3_div_dividend_smaller_than_divisor() {
        let a = BigInt::new(vec![3], false); // 3
        let b = BigInt::new(vec![0, 1], false); // 2^64 (larger)
        let (q, r) = bigint_divmod(a, b).expect("divmod should succeed");

        assert!(q.is_zero(), "quotient must be zero when |a| < |b|");
        assert_eq!(r.limbs(), &[3], "remainder must equal a when |a| < |b|");
    }

    /// Division by zero
    #[test]
    fn bug3_div_by_zero_returns_error() {
        let a = BigInt::new(vec![10], false);
        let result = bigint_divmod(a, BigInt::zero());
        assert_eq!(result.unwrap_err(), BigIntError::DivisionByZero);
    }

    /// Algebraic invariant: q * b + r == a
    #[test]
    fn bug3_div_algebraic_invariant_multi_limb() {
        let a_val: u128 = 0xDEAD_BEEF_CAFE_1234_5678_9ABC;
        let b_val: u128 = 0x1234_5678_9ABC_DEF0;

        let a = BigInt::from(a_val);
        let b = BigInt::from(b_val);
        let (q, r) = bigint_divmod(a.clone(), b.clone()).expect("divmod should succeed");

        let qb = bigint_mul(q, b).expect("q * b");
        let reconstructed = bigint_add(qb, r).expect("q*b + r");
        assert_eq!(
            reconstructed, a,
            "quotient * divisor + remainder must equal dividend"
        );
    }
    // =========================================================================
    // Bug 4 — Serialization: wire format
    // =========================================================================
    /// BigInt(1) serializes to RFC canonical bytes
    #[test]
    fn bug4_serialize_bigint_1_matches_rfc_canonical() {
        let b = BigInt::from(1i64);
        let encoding = b.serialize();

        let expected = vec![
            0x01u8, 0x00, 0x00, 0x00, // version, sign, reserved, reserved
            0x01, 0x00, 0x00, 0x00, // num_limbs=1, reserved, reserved, reserved
            0x01, 0x00, 0x00, 0x00, // limb[0] LE u64
            0x00, 0x00, 0x00, 0x00,
        ];
        let actual = encoding.to_bytes();
        assert_eq!(
            actual, expected,
            "BigInt(1) must serialize to RFC canonical bytes"
        );
    }

    /// Negative sign byte at position 1
    #[test]
    fn bug4_serialize_negative_sign_byte_at_position_1() {
        let b = BigInt::from(-1i64);
        let bytes = b.serialize().to_bytes();
        assert_eq!(bytes[0], 0x01, "byte 0 must be version 0x01");
        assert_eq!(bytes[1], 0xFF, "byte 1 must be sign 0xFF for negative");
    }

    /// num_limbs at byte 4
    #[test]
    fn bug4_serialize_num_limbs_at_byte_4() {
        let b = BigInt::new(vec![0, 1], false); // 2^64: 2 limbs
        let bytes = b.serialize().to_bytes();

        assert_eq!(bytes[2], 0x00, "byte 2 is reserved, must be 0x00");
        assert_eq!(bytes[3], 0x00, "byte 3 is reserved, must be 0x00");
        assert_eq!(bytes[4], 2, "byte 4 must be num_limbs=2");
        assert_eq!(
            bytes.len(),
            8 + 2 * 8,
            "total length = 8 header + 2 limbs * 8 bytes"
        );
    }

    /// Deserialize RFC canonical BigInt(42)
    #[test]
    fn bug4_deserialize_rfc_canonical_bigint_42() {
        let bytes: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, // version, sign, res, res
            0x01, 0x00, 0x00, 0x00, // num_limbs=1, res, res, res
            42, 0x00, 0x00, 0x00, // limb[0] LE u64
            0x00, 0x00, 0x00, 0x00,
        ];
        let b = BigInt::deserialize(&bytes).expect("valid RFC canonical bytes should deserialize");
        assert_eq!(b.limbs(), &[42]);
    }

    /// Reject non-zero reserved byte 2
    #[test]
    fn bug4_deserialize_rejects_nonzero_reserved_byte_2() {
        let bytes: Vec<u8> = vec![
            0x01, 0x00, 0xFF, 0x00, // byte 2 = 0xFF (invalid reserved)
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "non-zero reserved byte must be rejected");
    }

    /// Reject non-zero reserved bytes 5-7
    #[test]
    fn bug4_deserialize_rejects_nonzero_reserved_bytes_5_to_7() {
        let bytes: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01,
            0x00, // byte 6 = 0x01 (invalid reserved)
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "non-zero reserved byte 6 must be rejected");
    }

    /// Reject unknown version
    #[test]
    fn bug4_deserialize_rejects_unknown_version() {
        let bytes: Vec<u8> = vec![
            0x02, 0x00, 0x00, 0x00, // version = 0x02 (unknown)
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "unknown version must be rejected");
    }

    /// Reject invalid sign byte
    #[test]
    fn bug4_deserialize_rejects_invalid_sign_byte() {
        let bytes: Vec<u8> = vec![
            0x01, 0x80, 0x00, 0x00, // sign = 0x80 (invalid)
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "sign byte 0x80 must be rejected");
    }

    /// Reject num_limbs = 0
    #[test]
    fn bug4_deserialize_rejects_zero_num_limbs() {
        let bytes: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // num_limbs = 0 (invalid)
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "num_limbs=0 must be rejected");
    }

    /// Reject num_limbs > 64
    #[test]
    fn bug4_deserialize_rejects_too_many_limbs() {
        let bytes: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, 65, 0x00, 0x00, 0x00, // num_limbs = 65 (exceeds MAX_LIMBS)
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "num_limbs=65 must be rejected");
    }

    /// Reject length mismatch
    #[test]
    fn bug4_deserialize_rejects_length_mismatch() {
        let bytes: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, // num_limbs = 2
            // Only 1 limb worth of data provided
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = BigInt::deserialize(&bytes);
        assert!(result.is_err(), "truncated limb data must be rejected");
    }

    /// Round-trip serialize/deserialize
    #[test]
    fn bug4_roundtrip_serialize_deserialize() {
        let values: Vec<BigInt> = vec![
            BigInt::zero(),
            BigInt::from(1i64),
            BigInt::from(-1i64),
            BigInt::from(i128::MAX),
            BigInt::from(i128::MIN),
            BigInt::new(vec![0xDEAD_BEEF, 0xCAFE_BABE], false),
        ];
        for original in values {
            let bytes = original.serialize().to_bytes();
            let recovered = BigInt::deserialize(&bytes)
                .unwrap_or_else(|_| panic!("roundtrip failed for {:?}", original));
            assert_eq!(
                original, recovered,
                "serialize → deserialize must be identity"
            );
        }
    }

    // =========================================================================
    // Bug 5 — bigint_shr: large shifts should return ZERO
    // =========================================================================

    /// SHR(2^4095, 4096) = ZERO (probe entry 29)
    #[test]
    fn bug5_shr_shift_equals_bit_length_returns_zero() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63; // 2^4095
        let a = BigInt::new(limbs, false);

        let result = bigint_shr(a, 4096).expect("SHR with large shift must not return Err");
        assert!(
            result.is_zero(),
            "SHR(2^4095, 4096) must return ZERO, not Err"
        );
    }

    /// SHR(1, MAX_BIGINT_BITS) = ZERO
    #[test]
    fn bug5_shr_shift_far_exceeds_value_returns_zero() {
        let a = BigInt::from(1i64);
        let result = bigint_shr(a, MAX_BIGINT_BITS)
            .expect("SHR with shift == MAX_BIGINT_BITS must not return Err");
        assert!(result.is_zero(), "shifting 1 by 4096 bits must give ZERO");
    }

    /// SHR(1, MAX_BIGINT_BITS - 1) = ZERO
    #[test]
    fn bug5_shr_shift_much_larger_than_bit_length_returns_zero() {
        let a = BigInt::from(1i64);
        let result =
            bigint_shr(a, MAX_BIGINT_BITS - 1).expect("large shift on 1-bit value must not Err");
        assert!(result.is_zero(), "1 >> 4095 must be zero");
    }

    /// SHR(2^4095, 4095) = 1
    #[test]
    fn bug5_shr_shift_one_less_than_bit_length_gives_one() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs, false);

        let result = bigint_shr(a, 4095).expect("SHR(2^4095, 4095) should succeed");
        assert_eq!(result.limbs(), &[1], "2^4095 >> 4095 = 1");
    }

    /// SHR(2^4095, 1) within top limb
    #[test]
    fn bug5_shr_shift_by_one_within_top_limb() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs, false);

        let result = bigint_shr(a, 1).expect("SHR by 1 should succeed");
        assert_eq!(result.limbs()[63], 1 << 62);
    }

    /// SHR(2^4095, 64) = 2^4031
    #[test]
    fn bug5_shr_shift_by_full_limb_width() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs, false);

        let result = bigint_shr(a, 64).expect("SHR by 64 should succeed");
        assert_eq!(result.limbs().len(), 63, "2^4031 needs 63 limbs");
    }

    /// SHR(x, 0) = x
    #[test]
    fn bug5_shr_shift_zero_is_identity() {
        let a = BigInt::from(42i64);
        let result = bigint_shr(a.clone(), 0).expect("SHR by 0 should succeed");
        assert_eq!(result, a, "SHR(x, 0) must return x unchanged");
    }

    /// SHR(1, 1) = 0
    #[test]
    fn bug5_shr_shift_one_gives_zero() {
        let a = BigInt::from(1i64);
        let result = bigint_shr(a, 1).expect("SHR(1, 1) should succeed");
        assert!(result.is_zero(), "1 >> 1 = 0");
    }

    // =========================================================================
    // Bug 6 — bigint_shl: zero shift should return a
    // =========================================================================

    /// SHL(x, 0) = x
    #[test]
    fn bug6_shl_shift_zero_is_identity() {
        let a = BigInt::from(42i64);
        let result = bigint_shl(a.clone(), 0).expect("SHL(x, 0) must not return Err");
        assert_eq!(result, a, "SHL(x, 0) must return x unchanged");
    }

    /// SHL(0, 0) = 0
    #[test]
    fn bug6_shl_zero_value_zero_shift() {
        let a = BigInt::zero();
        let result = bigint_shl(a, 0).expect("SHL(0, 0) must not Err");
        assert!(result.is_zero());
    }

    /// SHL(1, 1) = 2
    #[test]
    fn bug6_shl_shift_one() {
        let a = BigInt::from(1i64);
        let result = bigint_shl(a, 1).expect("SHL(1, 1) should succeed");
        assert_eq!(result.limbs(), &[2]);
    }

    /// SHL(1, 4095) = 2^4095 (max legal shift)
    #[test]
    fn bug6_shl_max_legal_shift() {
        let a = BigInt::from(1i64);
        let result = bigint_shl(a, 4095).expect("SHL(1, 4095) should succeed");
        assert_eq!(result.limbs().len(), 64, "2^4095 needs 64 limbs");
        assert_eq!(result.limbs()[63], 1u64 << 63);
    }

    /// SHL(1, 4096) must Err(Overflow)
    #[test]
    fn bug6_shl_overflow_trap() {
        let a = BigInt::from(1i64);
        let result = bigint_shl(a, 4096);
        assert_eq!(result.unwrap_err(), BigIntError::Overflow);
    }

    /// SHL(2, 4095) must Err(Overflow)
    #[test]
    fn bug6_shl_overflow_when_value_has_more_than_one_bit() {
        let a = BigInt::from(2i64);
        let result = bigint_shl(a, 4095);
        assert_eq!(result.unwrap_err(), BigIntError::Overflow);
    }

    /// SHL(2^4094, 1) at exact boundary
    #[test]
    fn bug6_shl_exactly_at_max_bits_is_ok() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 62; // 2^4094
        let a = BigInt::new(limbs, false);
        let result =
            bigint_shl(a, 1).expect("SHL at exact MAX_BIGINT_BITS boundary should succeed");
        assert_eq!(result.limbs().len(), 64);
    }

    // =========================================================================
    // Bug 7 — Input canonicalization enforcement
    // =========================================================================

    /// bigint_add rejects non-canonical input A
    #[test]
    fn bug7_add_rejects_non_canonical_input_a_trailing_zero() {
        let a = BigInt::new(vec![1, 0], false); // non-canonical: trailing zero
        let b = BigInt::from(1i64);
        let result = bigint_add(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_add rejects non-canonical input B
    #[test]
    fn bug7_add_rejects_non_canonical_input_b_trailing_zero() {
        let a = BigInt::from(1i64);
        let b = BigInt::new(vec![1, 0], false);
        let result = bigint_add(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_add rejects negative zero
    #[test]
    fn bug7_add_rejects_negative_zero_input() {
        let a = BigInt::new(vec![0], true); // negative zero
        let b = BigInt::from(1i64);
        let result = bigint_add(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_sub rejects non-canonical input
    #[test]
    fn bug7_sub_rejects_non_canonical_input() {
        let a = BigInt::new(vec![5, 0, 0], false);
        let b = BigInt::from(3i64);
        let result = bigint_sub(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_mul rejects non-canonical input
    #[test]
    fn bug7_mul_rejects_non_canonical_input() {
        let a = BigInt::new(vec![2, 0], false);
        let b = BigInt::from(3i64);
        let result = bigint_mul(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_divmod rejects non-canonical dividend
    #[test]
    fn bug7_divmod_rejects_non_canonical_dividend() {
        let a = BigInt::new(vec![10, 0], false);
        let b = BigInt::from(3i64);
        let result = bigint_divmod(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_divmod rejects non-canonical divisor
    #[test]
    fn bug7_divmod_rejects_non_canonical_divisor() {
        let a = BigInt::from(10i64);
        let b = BigInt::new(vec![3, 0], false);
        let result = bigint_divmod(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_shl rejects non-canonical input
    #[test]
    fn bug7_shl_rejects_non_canonical_input() {
        let a = BigInt::new(vec![1, 0], false);
        let result = bigint_shl(a, 1);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_shr rejects non-canonical input
    #[test]
    fn bug7_shr_rejects_non_canonical_input() {
        let a = BigInt::new(vec![4, 0], false);
        let result = bigint_shr(a, 1);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    /// bigint_shl rejects negative zero
    #[test]
    fn bug7_shl_rejects_negative_zero() {
        let a = BigInt::new(vec![0], true);
        let result = bigint_shl(a, 1);
        assert_eq!(result.unwrap_err(), BigIntError::NonCanonicalInput);
    }

    // =========================================================================
    // Cross-cutting: overflow boundary tests
    // =========================================================================

    /// ADD at exact MAX_BIGINT_BITS is OK
    #[test]
    fn boundary_add_at_max_bigint_bits_is_ok() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs, false);
        let result = bigint_add(a, BigInt::zero());
        assert!(result.is_ok(), "2^4095 + 0 must not overflow");
    }

    /// ADD(2^4095, 2^4095) = 2^4096 exceeds MAX_BIGINT_BITS → TRAP
    #[test]
    fn boundary_add_overflow_by_one_bit() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs.clone(), false);
        let b = BigInt::new(limbs, false);
        let result = bigint_add(a, b);
        assert_eq!(result.unwrap_err(), BigIntError::Overflow);
    }

    /// MUL(4096-bit, 1) is OK
    #[test]
    fn boundary_mul_by_one_at_max_bits() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs, false);
        let result = bigint_mul(a, BigInt::from(1i64));
        assert!(result.is_ok(), "4096-bit * 1 must not overflow");
    }

    // =========================================================================
    // Probe entry verification
    // =========================================================================

    /// Probe entry 0: ADD(0, 2) = 2
    #[test]
    fn probe_entry_0_add_zero_and_two() {
        let result = bigint_add(BigInt::zero(), BigInt::from(2i64)).unwrap();
        assert_eq!(result.limbs(), &[2]);
    }

    /// Probe entry 3: ADD(1, -1) = 0
    #[test]
    fn probe_entry_3_add_one_and_neg_one() {
        let a = BigInt::from(1i64);
        let b = BigInt::from(-1i64);
        let result = bigint_add(a, b).unwrap();
        assert!(result.is_zero());
    }

    /// Probe entry 5: SUB(-5, -2) = -3
    #[test]
    fn probe_entry_5_sub_neg5_neg2() {
        let a = BigInt::from(-5i64);
        let b = BigInt::from(-2i64);
        let result = bigint_sub(a, b).unwrap();
        assert_eq!(result.limbs(), &[3]);
        assert!(result.sign());
    }

    /// Probe entry 10: MUL(2, 3) = 6
    #[test]
    fn probe_entry_10_mul_two_three() {
        let result = bigint_mul(BigInt::from(2i64), BigInt::from(3i64)).unwrap();
        assert_eq!(result.limbs(), &[6]);
    }

    /// Probe entry 16: DIV(10, 3) = 3 (remainder 1)
    #[test]
    fn probe_entry_16_div_10_by_3() {
        let (q, r) = bigint_divmod(BigInt::from(10i64), BigInt::from(3i64)).unwrap();
        assert_eq!(q.limbs(), &[3]);
        assert_eq!(r.limbs(), &[1]);
    }

    /// Probe entry 21: MOD(-7, 3) = -1
    #[test]
    fn probe_entry_21_mod_neg7_by_3() {
        let result = bigint_mod(BigInt::from(-7i64), BigInt::from(3i64)).unwrap();
        assert_eq!(result.limbs(), &[1]);
        assert!(result.sign());
    }

    /// Probe entry 24: SHL(1, 4095)
    #[test]
    fn probe_entry_24_shl_1_by_4095() {
        let result = bigint_shl(BigInt::from(1i64), 4095).unwrap();
        assert_eq!(result.limbs().len(), 64);
        assert_eq!(result.limbs()[63], 1u64 << 63);
    }

    /// Probe entry 29: SHR(2^4095, 4096) = ZERO
    #[test]
    fn probe_entry_29_shr_2_to_4095_by_4096() {
        let mut limbs = vec![0u64; 64];
        limbs[63] = 1 << 63;
        let a = BigInt::new(limbs, false);
        let result = bigint_shr(a, 4096).unwrap();
        assert!(result.is_zero());
    }

    /// Probe entry 53: SUB(0, 1) = -1
    #[test]
    fn probe_entry_53_sub_zero_one() {
        let result = bigint_sub(BigInt::zero(), BigInt::from(1i64)).unwrap();
        assert_eq!(result.limbs(), &[1]);
        assert!(result.sign());
    }
}
