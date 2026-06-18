//! Internal BigInt DECIMAL Arithmetic
//!
//! RFC-0111: Deterministic DECIMAL
//! Uses the crate's internal bigint.rs (RFC-0110) instead of num-bigint.
//! Feature: `use-internal-bigint` enables this implementation.

use crate::bigint::{
    bigint_add, bigint_div, bigint_divmod, bigint_mul, bigint_shl, bigint_sub, BigInt, BigIntError,
};
use crate::decimal::{
    Decimal, DecimalError, RoundingMode, MAX_DECIMAL_MANTISSA, MAX_DECIMAL_SCALE, POW10,
};

// ─── Internal BigInt Helpers ───────────────────────────────────────────────────

/// Convert i128 to internal BigInt
fn i128_to_bigint(n: i128) -> BigInt {
    BigInt::from(n)
}

/// Convert internal BigInt back to i128
fn bigint_to_i128(b: &BigInt) -> Result<i128, DecimalError> {
    b.clone()
        .try_into()
        .map_err(|_: BigIntError| DecimalError::Overflow)
}

/// Check if internal BigInt is within DECIMAL mantissa range
fn bigint_in_range(b: &BigInt) -> bool {
    let max_bigint = i128_to_bigint(MAX_DECIMAL_MANTISSA);
    let min_bigint = i128_to_bigint(-MAX_DECIMAL_MANTISSA);
    b.compare(&max_bigint) <= 0 && b.compare(&min_bigint) >= 0
}

/// Scale a Decimal mantissa by 10^diff using internal BigInt
fn scale_mantissa(mantissa: i128, diff: u8) -> Result<BigInt, DecimalError> {
    if diff == 0 {
        return Ok(i128_to_bigint(mantissa));
    }
    let pow10 = i128_to_bigint(POW10[diff as usize]);
    bigint_mul(i128_to_bigint(mantissa), pow10).map_err(|_| DecimalError::Overflow)
}

// ─── Arithmetic Operations ────────────────────────────────────────────────────

/// ADD — Addition using internal bigint
pub fn decimal_add_internal(a: &Decimal, b: &Decimal) -> Result<Decimal, DecimalError> {
    let target_scale = a.scale().max(b.scale());
    let diff_a = target_scale - a.scale();
    let diff_b = target_scale - b.scale();

    let a_val = scale_mantissa(a.mantissa(), diff_a)?;
    let b_val = scale_mantissa(b.mantissa(), diff_b)?;
    let sum = bigint_add(a_val, b_val).map_err(|_| DecimalError::Overflow)?;

    if !bigint_in_range(&sum) {
        return Err(DecimalError::Overflow);
    }
    let sum_i128 = bigint_to_i128(&sum)?;
    Decimal::new(sum_i128, target_scale)
}

/// SUB — Subtraction using internal bigint
pub fn decimal_sub_internal(a: &Decimal, b: &Decimal) -> Result<Decimal, DecimalError> {
    let target_scale = a.scale().max(b.scale());
    let diff_a = target_scale - a.scale();
    let diff_b = target_scale - b.scale();

    let a_val = scale_mantissa(a.mantissa(), diff_a)?;
    let b_val = scale_mantissa(b.mantissa(), diff_b)?;
    let diff = bigint_sub(a_val, b_val).map_err(|_| DecimalError::Overflow)?;

    if !bigint_in_range(&diff) {
        return Err(DecimalError::Overflow);
    }
    let diff_i128 = bigint_to_i128(&diff)?;
    Decimal::new(diff_i128, target_scale)
}

/// MUL — Multiplication using internal bigint
pub fn decimal_mul_internal(a: &Decimal, b: &Decimal) -> Result<Decimal, DecimalError> {
    let raw_scale = a.scale().wrapping_add(b.scale());

    let product = bigint_mul(i128_to_bigint(a.mantissa()), i128_to_bigint(b.mantissa()))
        .map_err(|_| DecimalError::Overflow)?;

    if raw_scale > MAX_DECIMAL_SCALE {
        let scale_reduction = raw_scale - MAX_DECIMAL_SCALE;
        let divisor = i128_to_bigint(POW10[scale_reduction as usize]);
        let (quotient, remainder) =
            bigint_divmod(product, divisor).map_err(|_| DecimalError::Overflow)?;

        // RoundHalfEven: check remainder vs half
        let abs_rem = if remainder.sign() {
            let zero = BigInt::zero();
            bigint_sub(zero, remainder).map_err(|_| DecimalError::Overflow)?
        } else {
            remainder
        };
        let half = i128_to_bigint(POW10[scale_reduction as usize] / 2);
        let cmp = abs_rem.compare(&half);

        let product = if cmp > 0
            || (cmp == 0 && {
                // quotient is odd if remainder == half
                let two = i128_to_bigint(2);
                let (q, _) =
                    bigint_divmod(quotient.clone(), two).map_err(|_| DecimalError::Overflow)?;
                !q.is_zero()
            }) {
            if quotient.sign() {
                bigint_sub(quotient, i128_to_bigint(1)).map_err(|_| DecimalError::Overflow)?
            } else {
                bigint_add(quotient, i128_to_bigint(1)).map_err(|_| DecimalError::Overflow)?
            }
        } else {
            quotient
        };

        if !bigint_in_range(&product) {
            return Err(DecimalError::Overflow);
        }
        let product_i128 = bigint_to_i128(&product)?;
        Decimal::new(product_i128, MAX_DECIMAL_SCALE)
    } else {
        if !bigint_in_range(&product) {
            return Err(DecimalError::Overflow);
        }
        let product_i128 = bigint_to_i128(&product)?;
        Decimal::new(product_i128, raw_scale)
    }
}

/// DIV — Division using internal bigint
pub fn decimal_div_internal(
    a: &Decimal,
    b: &Decimal,
    _target_scale: u8,
) -> Result<Decimal, DecimalError> {
    if b.mantissa() == 0 {
        return Err(DecimalError::DivisionByZero);
    }

    let raw_scale = a.scale().max(b.scale()).wrapping_add(6);
    let target_scale = raw_scale.min(MAX_DECIMAL_SCALE);
    let result_sign = (a.mantissa() < 0) != (b.mantissa() < 0);

    let abs_a = a.mantissa().abs();
    let abs_b = b.mantissa().abs();
    let scale_diff = (target_scale as i32) - (a.scale() as i32) + (b.scale() as i32);

    let scaled_dividend: i128 = if scale_diff > 0 {
        let scaled = bigint_mul(
            i128_to_bigint(POW10[scale_diff as usize]),
            i128_to_bigint(abs_a),
        )
        .map_err(|_| DecimalError::Overflow)?;
        bigint_to_i128(&scaled)?
    } else if scale_diff < 0 {
        let scale_reduction = (-scale_diff) as usize;
        let divisor = POW10[scale_reduction];
        let quotient = abs_a / divisor;
        let remainder = abs_a % divisor;
        let half = divisor / 2;
        if remainder > half || (remainder == half && quotient % 2 != 0) {
            quotient + 1
        } else {
            quotient
        }
    } else {
        abs_a
    };

    let magnitude = scaled_dividend.abs();
    let quotient = magnitude / abs_b;
    let remainder = magnitude % abs_b;
    let half = abs_b / 2;

    let result = if remainder < half {
        quotient
    } else if remainder > half {
        quotient + 1
    } else if quotient % 2 == 0 {
        quotient
    } else {
        quotient + 1
    };

    let result = if result_sign { -result } else { result };
    Decimal::new(result, target_scale)
}

/// SQRT — Square root using internal bigint
pub fn decimal_sqrt_internal(a: &Decimal) -> Result<Decimal, DecimalError> {
    if a.mantissa() < 0 {
        return Err(DecimalError::InvalidScale);
    }
    if a.mantissa() == 0 {
        return Decimal::new(0, 0);
    }

    let p = (a.scale() as u16 + 6).min(MAX_DECIMAL_SCALE as u16) as u8;
    let scale_factor = (2 * p as i32) - (a.scale() as i32);

    let scaled_n = if scale_factor > 36 {
        let lo = i128_to_bigint(POW10[(scale_factor - 36) as usize]);
        let hi = i128_to_bigint(POW10[36]);
        let partial =
            bigint_mul(i128_to_bigint(a.mantissa()), lo).map_err(|_| DecimalError::Overflow)?;
        bigint_mul(partial, hi).map_err(|_| DecimalError::Overflow)?
    } else if scale_factor >= 0 {
        bigint_mul(
            i128_to_bigint(a.mantissa()),
            i128_to_bigint(POW10[scale_factor as usize]),
        )
        .map_err(|_| DecimalError::Overflow)?
    } else {
        return Err(DecimalError::Overflow);
    };

    // Newton-Raphson
    let bit_len = scaled_n.bit_length();
    let mut x = i128_to_bigint(1);
    x = bigint_shl(x, bit_len.div_ceil(2)).map_err(|_| DecimalError::Overflow)?;

    for _ in 0..40 {
        if x.is_zero() {
            break;
        }
        let n_over_x = bigint_divmod(scaled_n.clone(), x.clone())
            .map_err(|_| DecimalError::Overflow)?
            .0;
        let sum = bigint_add(x.clone(), n_over_x).map_err(|_| DecimalError::Overflow)?;
        x = bigint_div(sum, i128_to_bigint(2)).map_err(|_| DecimalError::Overflow)?;
    }

    // Off-by-one correction
    let x_sq = bigint_mul(x.clone(), x.clone()).map_err(|_| DecimalError::Overflow)?;
    if x_sq.compare(&scaled_n) > 0 {
        x = bigint_sub(x, i128_to_bigint(1)).map_err(|_| DecimalError::Overflow)?;
    }

    if !bigint_in_range(&x) {
        return Err(DecimalError::Overflow);
    }
    let mantissa = bigint_to_i128(&x)?;
    Decimal::new(mantissa, p)
}

/// ROUND — Rounding using internal bigint
pub fn decimal_round_internal(
    d: &Decimal,
    target_scale: u8,
    mode: RoundingMode,
) -> Result<Decimal, DecimalError> {
    if target_scale >= d.scale() {
        return Ok(*d);
    }

    let diff = (d.scale() - target_scale) as usize;
    let divisor = POW10[diff];

    let q = d.mantissa() / divisor;
    let r = d.mantissa() % divisor;

    let result = match mode {
        RoundingMode::RoundHalfEven => {
            let abs_r = r.abs();
            let half = divisor / 2;
            if abs_r < half {
                q
            } else if abs_r > half {
                q + d.mantissa().signum()
            } else if q % 2 == 0 {
                q
            } else {
                q + d.mantissa().signum()
            }
        }
        RoundingMode::RoundDown => q,
        RoundingMode::RoundUp => {
            if r > 0 && d.mantissa() > 0 {
                q + 1
            } else if r < 0 && d.mantissa() < 0 {
                q - 1
            } else {
                q
            }
        }
    };

    Decimal::new(result, target_scale)
}

/// CMP — Comparison using internal bigint
pub fn decimal_cmp_internal(a: &Decimal, b: &Decimal) -> i32 {
    if a.scale() == b.scale() {
        if a.mantissa() < b.mantissa() {
            return -1;
        } else if a.mantissa() > b.mantissa() {
            return 1;
        }
        return 0;
    }

    let max_scale = a.scale().max(b.scale());
    let diff_a = (max_scale - a.scale()) as usize;
    let diff_b = (max_scale - b.scale()) as usize;

    let a_big =
        scale_mantissa(a.mantissa(), diff_a as u8).unwrap_or_else(|_| i128_to_bigint(i128::MAX));
    let b_big =
        scale_mantissa(b.mantissa(), diff_b as u8).unwrap_or_else(|_| i128_to_bigint(i128::MAX));

    let diff = match bigint_sub(a_big, b_big) {
        Ok(d) => d,
        Err(_) => return if a.mantissa() >= b.mantissa() { 1 } else { -1 },
    };

    let zero = BigInt::zero();
    if diff.compare(&zero) < 0 {
        -1
    } else if diff.compare(&zero) > 0 {
        1
    } else {
        0
    }
}
