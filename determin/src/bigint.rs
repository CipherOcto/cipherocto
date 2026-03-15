//! BigInt: Deterministic Arbitrary-Precision Integer Implementation
//!
//! Implements RFC-0110: Deterministic BIGINT
//!
//! Key design principles:
//! - Canonical form: no leading zeros, zero = {limbs: [0], sign: false}
//! - 128-bit intermediate arithmetic for carry/borrow
//! - TRAP on overflow (result exceeds MAX_BIGINT_BITS)
//! - Explicit canonicalization after every operation

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
    let mut result = vec![0; a.len()];

    for i in 0..a.len() {
        let a_val = a[i] as i128;
        let b_val = b.get(i).copied().unwrap_or(0) as i128;
        // Subtract with borrow: (a - b - borrow)
        let diff = a_val - b_val;

        if diff >= 0 {
            result[i] = diff as u64;
        } else {
            // Borrow: add 2^64
            result[i] = (diff + (1 << 64)) as u64;
        }
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
    let mut result = vec![0; a.len() + b.len()];

    for (i, &ai) in a.iter().enumerate() {
        let mut carry = 0u128;

        for (j, &bj) in b.iter().enumerate() {
            // 128-bit intermediate multiplication
            let product = (ai as u128) * (bj as u128);
            let low = product as u64;
            let high = (product >> 64) as u64;

            let k = i + j;

            // Add to result with carry propagation
            let sum = (result[k] as u128) + (low as u128) + carry;
            result[k] = sum as u64;
            carry = sum >> 64;

            // Upper carry (USE |= NOT =)
            result[k + 1] |= high;
            result[k + 1] |= carry as u64;
        }
    }

    result
}

// =============================================================================
// DIV — Division
// RFC-0110 §bigint_divmod
// =============================================================================

/// Divide two BigInt values and return quotient and remainder
/// RFC-0110: bigint_divmod(a: BigInt, b: BigInt) -> (BigInt, BigInt)
/// Uses binary long division
pub fn bigint_divmod(a: BigInt, b: BigInt) -> Result<(BigInt, BigInt), BigIntError> {
    // Division by zero check
    if b.is_zero() {
        return Err(BigIntError::DivisionByZero);
    }

    // |a| < |b| => quotient = 0, remainder = a
    if a.magnitude_cmp(&b) < 0 {
        return Ok((BigInt::zero(), a));
    }

    // Preconditions
    if a.bit_length() > MAX_BIGINT_BITS || b.bit_length() > MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
    }

    // Work with absolute values
    let mut a_abs = a.limbs.clone();
    let b_abs = b.limbs.clone();

    // Simple binary division: find how many times b fits into a
    let mut quotient_limbs: Vec<u64> = vec![0];

    // Compare and subtract approach
    while a_abs.len() > 1 || (a_abs.len() == 1 && a_abs[0] > 0) {
        // Compare a_abs vs b_abs
        if limb_cmp(&a_abs, &b_abs) >= 0 {
            // Subtract b from a
            a_abs = limb_sub_vec(&a_abs, &b_abs);
            // Add 1 to quotient (this is very naive - works but slow)
            quotient_limbs = limb_add_scalar(&quotient_limbs, 1);
        } else {
            break;
        }
    }

    // Handle single limb quotient case
    let quotient = if quotient_limbs.len() == 1 && quotient_limbs[0] == 0 {
        BigInt::zero()
    } else {
        BigInt {
            limbs: quotient_limbs,
            sign: a.sign != b.sign,
        }
    };
    let quotient = quotient.canonicalize();

    let remainder = if a_abs == vec![0] {
        BigInt::zero()
    } else {
        BigInt {
            limbs: a_abs,
            sign: a.sign,
        }
    };
    let remainder = remainder.canonicalize();

    Ok((quotient, remainder))
}

/// Division: a / b
pub fn bigint_div(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    Ok(bigint_divmod(a, b)?.0)
}

/// Modulo: a % b
pub fn bigint_mod(a: BigInt, b: BigInt) -> Result<BigInt, BigIntError> {
    Ok(bigint_divmod(a, b)?.1)
}

// =============================================================================
// Helper functions for DIV
// =============================================================================

/// Compare limb vectors (unsigned)
fn limb_cmp(a: &[u64], b: &[u64]) -> i32 {
    if a.len() != b.len() {
        return if a.len() > b.len() { 1 } else { -1 };
    }

    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return if a[i] > b[i] { 1 } else { -1 };
        }
    }

    0
}

/// Subtract b from a where a >= b (vectors)
fn limb_sub_vec(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut result = vec![0; a.len()];
    let mut borrow = 0i128;

    for i in 0..a.len() {
        let a_val = a[i] as i128;
        let b_val = b.get(i).copied().unwrap_or(0) as i128;
        let diff = a_val - b_val - borrow;

        if diff >= 0 {
            result[i] = diff as u64;
            borrow = 0;
        } else {
            result[i] = (diff + (1 << 64)) as u64;
            borrow = 1;
        }
    }

    // Remove leading zeros
    while result.len() > 1 && *result.last().unwrap() == 0 {
        result.pop();
    }

    result
}

/// Add scalar to limb vector
fn limb_add_scalar(a: &[u64], scalar: u64) -> Vec<u64> {
    let mut result = a.to_vec();
    let mut carry = scalar as u128;

    for slot in result.iter_mut() {
        let sum = (*slot as u128) + carry;
        *slot = sum as u64;
        carry = sum >> 64;
        if carry == 0 {
            break;
        }
    }

    if carry > 0 {
        result.push(carry as u64);
    }

    result
}

// =============================================================================
// SHL — Left Shift
// RFC-0110 §SHL
// =============================================================================

/// Left shift: a << shift
/// RFC-0110: bigint_shl(a: BigInt, shift: usize) -> BigInt
pub fn bigint_shl(a: BigInt, shift: usize) -> Result<BigInt, BigIntError> {
    // Validate shift amount
    if shift == 0 || shift >= MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
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
    // Validate shift amount
    if shift >= MAX_BIGINT_BITS {
        return Err(BigIntError::Overflow);
    }

    if shift == 0 {
        return Ok(a);
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
}
