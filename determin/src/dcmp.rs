//! Free-function comparison helpers on `Dfp`.
//!
//! These are thin wrappers over the method forms in [`crate::dfp_compare`],
//! kept for callers — including RFC-0968's `update_ewma` and the five
//! `Normalizer::normalize` implementations — that prefer a free-function
//! style. Both forms are equally canonical; choose one for consistency
//! per module.
//!
//! **NaN handling for `<`, `<=`, `>`, `>=`:** returns `false` for any
//! NaN operand. Idiomatic-Rust behaviour (cf. `f64` partial_cmp).
//! **`dfp_eq` is structural** (every field must match); use
//! [`dfp_eq_ieee754`] for RFC-0104-§989-990 spec compliance.

use std::cmp::Ordering;

use crate::Dfp;

/// Absolute value: non-negative copy preserving class.
pub fn dfp_abs(d: Dfp) -> Dfp {
    d.abs()
}

/// Negation: flips sign and preserves class. NaN returns canonical NaN.
pub fn dfp_neg(d: Dfp) -> Dfp {
    -d
}

/// Strict less-than (false if either operand is NaN).
pub fn dfp_lt(a: Dfp, b: Dfp) -> bool {
    a < b
}

/// Less-than-or-equal (false if either operand is NaN).
pub fn dfp_le(a: Dfp, b: Dfp) -> bool {
    a <= b
}

/// Strict greater-than (false if either operand is NaN).
pub fn dfp_gt(a: Dfp, b: Dfp) -> bool {
    a > b
}

/// Greater-than-or-equal (false if either operand is NaN).
pub fn dfp_ge(a: Dfp, b: Dfp) -> bool {
    a >= b
}

/// Structural equality: every field must match. NaN != NaN, +0 != -0.
/// Use [`dfp_eq_ieee754`] for spec compliance.
pub fn dfp_eq(a: Dfp, b: Dfp) -> bool {
    a == b
}

/// IEEE-754 equality per RFC-0104 §989-990:
/// all NaN compare equal; -0 == +0; ±Inf distinct from each other;
/// Normal values compared fieldwise.
pub fn dfp_eq_ieee754(a: Dfp, b: Dfp) -> bool {
    a.eq_ieee754(&b)
}

/// Three-way compare returning `Ordering` (total order).
pub fn dfp_cmp(a: Dfp, b: Dfp) -> Ordering {
    a.cmp(&b)
}

/// Smaller of two values under the canonical total order.
pub fn dfp_min(a: Dfp, b: Dfp) -> Dfp {
    a.min(b)
}

/// Larger of two values under the canonical total order.
pub fn dfp_max(a: Dfp, b: Dfp) -> Dfp {
    a.max(b)
}

/// Clamp `d` into `[lo, hi]`. NaN clamps to `lo`.
pub fn dfp_clamp(d: Dfp, lo: Dfp, hi: Dfp) -> Dfp {
    d.clamp(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dfp;

    #[test]
    fn dfp_lt_basic() {
        let one = Dfp::from_f64(1.0);
        let two = Dfp::from_f64(2.0);
        assert!(dfp_lt(one, two));
        assert!(!dfp_lt(two, one));
        assert!(!dfp_lt(one, one));
    }

    #[test]
    fn dfp_lt_returns_false_on_nan() {
        assert!(!dfp_lt(Dfp::nan(), Dfp::from_f64(1.0)));
        assert!(!dfp_lt(Dfp::from_f64(1.0), Dfp::nan()));
        assert!(!dfp_lt(Dfp::nan(), Dfp::nan()));
    }

    #[test]
    fn dfp_le_ge_eq_match_methods() {
        let a = Dfp::from_f64(0.5);
        let b = Dfp::from_f64(0.5);
        assert_eq!(dfp_le(a, b), a <= b);
        assert_eq!(dfp_ge(a, b), a >= b);
        assert_eq!(dfp_eq(a, b), a == b);
        assert_eq!(dfp_cmp(a, b), a.cmp(&b));
    }

    #[test]
    fn dfp_eq_agrees_with_ieee754_for_zero_and_nan() {
        // Both structural and IEEE-754 eq now treat -0 == +0 and
        // NaN == NaN (RFC-0104 spec-compliant equality).
        assert!(dfp_eq(Dfp::neg_zero(), Dfp::zero()));
        assert!(dfp_eq_ieee754(Dfp::neg_zero(), Dfp::zero()));
        assert!(dfp_eq(Dfp::nan(), Dfp::nan()));
        assert!(dfp_eq_ieee754(Dfp::nan(), Dfp::nan()));
    }

    #[test]
    fn dfp_eq_agrees_with_ieee754_for_infinity() {
        // Both methods treat +Inf == +Inf, -Inf == -Inf, +Inf != -Inf.
        assert!(dfp_eq(Dfp::infinity(), Dfp::infinity()));
        assert!(!dfp_eq(Dfp::infinity(), Dfp::neg_infinity()));
        assert!(dfp_eq_ieee754(Dfp::infinity(), Dfp::infinity()));
        assert!(!dfp_eq_ieee754(Dfp::infinity(), Dfp::neg_infinity()));
    }

    #[test]
    fn dfp_min_max_match_methods() {
        let a = Dfp::from_f64(-1.0);
        let b = Dfp::from_f64(1.0);
        assert_eq!(dfp_min(a, b), a.min(b));
        assert_eq!(dfp_max(a, b), a.max(b));
    }

    #[test]
    fn dfp_clamp_match_method() {
        let v = Dfp::from_f64(2.0);
        let lo = Dfp::from_f64(-1.0);
        let hi = Dfp::from_f64(1.0);
        assert_eq!(dfp_clamp(v, lo, hi), v.clamp(lo, hi));
    }

    #[test]
    fn dfp_clamp_nan_returns_lo() {
        let lo = Dfp::from_f64(0.0);
        let hi = Dfp::from_f64(1.0);
        assert_eq!(dfp_clamp(Dfp::nan(), lo, hi), lo);
    }
}
