//! Comparison and composition operations on `Dfp`.
//!
//! Per RFC-0104, DFP is a tagged representation: each value carries an
//! explicit `class` (Normal / Zero / Infinity / NaN) so every comparison
//! has a defined, deterministic outcome and produces identical ordering
//! across replicas running different compilers / platforms.
//!
//! ## Total ordering
//!
//! Sort key (descending):
//! 1. `class`: NaN > Infinity > Normal > Zero.
//! 2. Within `Infinity` and `Zero`: positive sign > negative sign.
//!    (+0 and −0 are distinguished by sign bit; +Inf and −Inf likewise.)
//! 3. Within `Normal`: exponent descending, then mantissa descending,
//!    then sign (positive > negative of equal magnitude).
//!
//! `NaN == NaN`. Normal numbers are compared by magnitude and then sign
//! to match canonical IEEE-754 `totalOrder` semantics, which is what
//! RFC-0968's EWMA cross-replica equality test requires.
//!
//! Pre-call NaN rejection: callers that want poison-style NaN rejection
//! (e.g., `update_ewma`) MUST call `is_finite()` before relying on
//! `PartialOrd`/`<` outcomes; the total order sorts NaN above all finite
//! values, so `NaN < 1.0` is `false`.

use std::cmp::Ordering;

use crate::{Dfp, DfpClass};

fn class_rank(c: DfpClass) -> u8 {
    match c {
        DfpClass::NaN => 3,
        DfpClass::Infinity => 2,
        DfpClass::Normal => 1,
        DfpClass::Zero => 0,
    }
}

/// Deterministic, cross-replica total order on `Dfp`.
///
/// See module docs for the full ordering rule. NaN is treated as equal
/// to itself and as greater than every non-NaN value.
fn dfp_total_cmp(this: &Dfp, other: &Dfp) -> Ordering {
    let r_self = class_rank(this.class);
    let r_other = class_rank(other.class);
    if r_self != r_other {
        return r_self.cmp(&r_other);
    }
    match this.class {
        DfpClass::NaN => Ordering::Equal,
        DfpClass::Zero | DfpClass::Infinity => {
            // For Zero and Infinity: positive sign sorts greater than negative.
            // (+0 > -0, +Inf > -Inf per IEEE-754 totalOrder.)
            // `sign = true` means negative in our encoding.
            // false.cmp(&true) == Less  → positive (false) < negative (true)? NO.
            // IEEE: +0 > -0 means positive is greater. We want positive to compare Greater.
            // this.sign=false, other.sign=true: positive vs negative → this > other → Greater.
            // this.sign.cmp(&other.sign): false.cmp(&true) = Less. We want Greater.
            // Flip: other.sign.cmp(&this.sign).
            other.sign.cmp(&this.sign)
        }
        DfpClass::Normal => {
            if this.sign != other.sign {
                // Negative (sign=true) < Positive (sign=false). So true → Less.
                return if this.sign {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            // Same sign. Magnitude (exp, mantissa) is compared:
            //   - positive: larger (exp, mantissa) = larger value → ASC
            //   - negative: larger (exp, mantissa) = more-negative = smaller value → DESC
            let exp_cmp = if this.sign {
                other.exponent.cmp(&this.exponent)
            } else {
                this.exponent.cmp(&other.exponent)
            };
            let mant_cmp = if this.sign {
                other.mantissa.cmp(&this.mantissa)
            } else {
                this.mantissa.cmp(&other.mantissa)
            };
            exp_cmp.then(mant_cmp)
        }
    }
}

impl Dfp {
    /// Absolute value: returns a non-negative copy of `self`.
    ///
    /// Preserves the class. `NaN` propagates with sign cleared.
    pub fn abs(self) -> Self {
        if self.sign {
            Dfp {
                sign: false,
                ..self
            }
        } else {
            self
        }
    }

    // Negation is provided by `impl std::ops::Neg for Dfp` below.
    // Use `-d` (Rust unary minus) or the trait method.

    /// Returns `true` if `self` is `NaN`.
    pub fn is_nan(&self) -> bool {
        matches!(self.class, DfpClass::NaN)
    }

    /// Returns `true` if `self` is ±∞.
    pub fn is_infinite(&self) -> bool {
        matches!(self.class, DfpClass::Infinity)
    }

    /// Returns `true` for finite, non-NaN, non-±∞ values (Normal or Zero).
    pub fn is_finite(&self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }

    /// Returns `true` for Zero (either sign).
    pub fn is_zero(&self) -> bool {
        matches!(self.class, DfpClass::Zero)
    }

    /// Read-path sanity check used by `ReputationError::ScoreEncodingInvalid`
    /// (RFC-0968 Round 17). A value is "valid" when it is finite OR a
    /// saturated ±∞ OR NaN. Within Normal class, mantissa must be non-zero
    /// (zero mantissa should be Zero) and exponent must lie within the
    /// canonical DFP exponent range.
    pub fn is_valid(&self) -> bool {
        match self.class {
            DfpClass::NaN | DfpClass::Infinity | DfpClass::Zero => true,
            DfpClass::Normal => {
                self.mantissa != 0
                    && self.exponent >= crate::DFP_MIN_EXPONENT
                    && self.exponent <= crate::DFP_MAX_EXPONENT
            }
        }
    }

    /// Returns the smaller of `self` and `other` under the canonical
    /// total order (NaN included; see module docs).
    pub fn min(self, other: Self) -> Self {
        match dfp_total_cmp(&self, &other) {
            Ordering::Greater => other,
            _ => self,
        }
    }

    /// Returns the larger of `self` and `other` under the canonical
    /// total order.
    pub fn max(self, other: Self) -> Self {
        match dfp_total_cmp(&self, &other) {
            Ordering::Less => other,
            _ => self,
        }
    }

    /// Clamp `self` into `[lo, hi]`. Returns `self` if it lies inside the
    /// range, else the bound that was breached. NaN clamps to `lo` (it
    /// sorts below everything; `lo` is the floor).
    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        if dfp_total_cmp(&lo, &hi) == Ordering::Greater {
            // Inverted bounds: degenerate; return `lo`.
            return lo;
        }
        let below_lo = matches!(dfp_total_cmp(&self, &lo), Ordering::Less);
        let above_hi = matches!(dfp_total_cmp(&self, &hi), Ordering::Greater);
        if below_lo {
            lo
        } else if above_hi {
            hi
        } else {
            self
        }
    }
}

impl PartialOrd for Dfp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Dfp {
    fn cmp(&self, other: &Self) -> Ordering {
        dfp_total_cmp(self, other)
    }
}

/// Unary negation: `(-x).abs() == x.abs()` and `-(-x) == x` for finite
/// values. NaN propagates with sign cleared (consistent with
/// `Dfp::nan()` canonical form).
impl std::ops::Neg for Dfp {
    type Output = Self;
    fn neg(self) -> Self {
        Dfp {
            sign: !self.sign,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Neg;

    #[test]
    fn abs_clears_sign_on_normal() {
        let n = Dfp::from_f64(-3.5);
        assert!(n.abs().sign == false);
        assert_eq!(n.abs(), Dfp::from_f64(3.5));
    }

    #[test]
    fn abs_zero_preserves_class() {
        assert!(Dfp::neg_zero().abs().is_zero());
        assert!(!Dfp::neg_zero().abs().sign);
    }

    #[test]
    fn neg_flips_sign_zero() {
        assert!(Dfp::zero().neg().sign);
        assert!(Dfp::neg_zero().neg().sign == false);
    }

    #[test]
    fn neg_flips_sign_infinity() {
        assert!(Dfp::infinity().neg().is_infinite());
        assert!(Dfp::infinity().neg().sign);
    }

    #[test]
    fn neg_nan_is_nan() {
        assert!(Dfp::nan().neg().is_nan());
    }

    #[test]
    fn is_finite_distinguishes_nan_inf_zero_normal() {
        assert!(!Dfp::nan().is_finite());
        assert!(!Dfp::infinity().is_finite());
        assert!(Dfp::zero().is_finite());
        assert!(Dfp::from_f64(1.0).is_finite());
    }

    #[test]
    fn is_valid_zero_and_nan_and_normal() {
        assert!(Dfp::zero().is_valid());
        assert!(Dfp::from_f64(0.5).is_valid());
        assert!(Dfp::nan().is_valid());
        assert!(Dfp::infinity().is_valid());
    }

    #[test]
    fn is_valid_rejects_zero_mantissa_normal() {
        let bad = Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::Normal,
            sign: false,
        };
        assert!(!bad.is_valid());
    }

    #[test]
    fn cmp_orders_zero_negative_before_positive() {
        // IEEE-754 totalOrder: +0 > -0.
        assert!(Dfp::neg_zero() < Dfp::zero());
    }

    #[test]
    fn cmp_orders_normal_by_magnitude_then_sign() {
        let one = Dfp::from_f64(1.0);
        let two = Dfp::from_f64(2.0);
        let neg_one = Dfp::from_f64(-1.0);
        assert!(one < two);
        assert!(neg_one < one);
        assert_eq!(one.cmp(&Dfp::from_f64(1.0)), Ordering::Equal);
    }

    #[test]
    fn cmp_nan_is_greater_than_all_finite() {
        let inf = Dfp::infinity();
        let one = Dfp::from_f64(1.0);
        let zero = Dfp::zero();
        assert!(zero < Dfp::nan());
        assert!(one < Dfp::nan());
        assert!(inf < Dfp::nan());
        assert_eq!(Dfp::nan().cmp(&Dfp::nan()), Ordering::Equal);
    }

    #[test]
    fn min_max_use_total_order() {
        let a = Dfp::from_f64(-2.0);
        let b = Dfp::from_f64(1.0);
        assert_eq!(a.min(b), Dfp::from_f64(-2.0));
        assert_eq!(a.max(b), Dfp::from_f64(1.0));
    }

    #[test]
    fn clamp_clamps_below_low() {
        let v = Dfp::from_f64(-5.0);
        let lo = Dfp::from_f64(-1.0);
        let hi = Dfp::from_f64(1.0);
        assert_eq!(v.clamp(lo, hi), lo);
    }

    #[test]
    fn clamp_clamps_above_high() {
        let v = Dfp::from_f64(5.0);
        let lo = Dfp::from_f64(-1.0);
        let hi = Dfp::from_f64(1.0);
        assert_eq!(v.clamp(lo, hi), hi);
    }

    #[test]
    fn clamp_passes_through_in_range() {
        let v = Dfp::from_f64(0.5);
        let lo = Dfp::from_f64(-1.0);
        let hi = Dfp::from_f64(1.0);
        assert_eq!(v.clamp(lo, hi), v);
    }

    #[test]
    fn clamp_inverted_returns_low() {
        let v = Dfp::from_f64(0.5);
        let lo = Dfp::from_f64(1.0);
        let hi = Dfp::from_f64(-1.0);
        assert_eq!(v.clamp(lo, hi), lo);
    }
}
