//! Free-function comparison helpers on `Dfp`.
//!
//! These are thin wrappers over the method forms in [`crate::dfp_compare`],
//! kept for callers — including RFC-0968's `update_ewma` and the five
//! `Normalizer::normalize` implementations — that prefer a free-function
//! style. Both forms are equally canonical; choose one for consistency
//! per module.

use std::cmp::Ordering;

use crate::Dfp;

/// Absolute value: non-negative copy preserving class.
pub fn dfp_abs(d: Dfp) -> Dfp {
    d.abs()
}

/// Negation: flips sign and preserves class. Thin wrapper over `Neg::neg`.
pub fn dfp_neg(d: Dfp) -> Dfp {
    -d
}

/// Strict less-than (canonical total order; NaN sorts above all finite).
pub fn dfp_lt(a: Dfp, b: Dfp) -> bool {
    a < b
}

/// Less-than-or-equal.
pub fn dfp_le(a: Dfp, b: Dfp) -> bool {
    a <= b
}

/// Strict greater-than.
pub fn dfp_gt(a: Dfp, b: Dfp) -> bool {
    a > b
}

/// Greater-than-or-equal.
pub fn dfp_ge(a: Dfp, b: Dfp) -> bool {
    a >= b
}

/// Equality. NaN compares equal to NaN.
pub fn dfp_eq(a: Dfp, b: Dfp) -> bool {
    a == b
}

/// Three-way compare returning `Ordering`.
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

/// Clamp `d` into `[lo, hi]`.
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
    fn dfp_le_ge_eq_match_methods() {
        let a = Dfp::from_f64(0.5);
        let b = Dfp::from_f64(0.5);
        assert_eq!(dfp_le(a, b), a <= b);
        assert_eq!(dfp_ge(a, b), a >= b);
        assert_eq!(dfp_eq(a, b), a == b);
        assert_eq!(dfp_cmp(a, b), a.cmp(&b));
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
}
