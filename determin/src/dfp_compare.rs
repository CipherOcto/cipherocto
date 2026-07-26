//! Comparison and composition operations on `Dfp`.
//!
//! Per RFC-0104, DFP is a tagged representation: each value carries an
//! explicit `class` (Normal / Zero / Infinity / NaN) so every comparison
//! has a defined, deterministic outcome and produces identical ordering
//! across replicas running different compilers / platforms.
//!
//! ## Total ordering (`Ord::cmp`)
//!
//! Sort key (descending):
//! 1. `class`: NaN > Infinity > Normal > Zero.
//! 2. Within `Infinity` and `Zero`: positive sign > negative sign.
//!    (+0 > −0, +Inf > −Inf per IEEE-754 `totalOrder`.)
//! 3. Within `Normal`: sign first (positive > negative), then magnitude
//!    (exponent, then mantissa). Magnitude is reversed for negatives so
//!    that −2 sorts before −1.
//!
//! `NaN.cmp(&NaN) == Equal`. The total order is for `BTreeMap` keys,
//! sort, and `[a, b].sort()`; it is NOT the same as `<` semantics
//! (see `PartialOrd` below).
//!
//! ## Partial-ordering (`<`, `>`, `<=`, `>=`)
//!
//! `PartialOrd::partial_cmp` returns `None` whenever either operand is
//! NaN. This matches the Rust idiom for floating-point types (cf.
//! `f64::partial_cmp`) and lets callers distinguish "incomparable" from
//! "greater"/"less" without crashing.
//!
//! ## IEEE-754 equality (`eq_ieee754`)
//!
//! The derived `PartialEq` (`==`) is **structural**: it compares every
//! field, so `−0 == +0` is `false` and `NaN_1 == NaN_2` is `false`
//! (since neither satisfies reflexivity). For RFC-0104-§989-990
//! equality semantics (`−0 == +0`, all NaN equal, ±Inf distinct from
//! each other), use the explicit `eq_ieee754` method.
//!
//! ## `DfpEncoding::to_bytes()` and total order
//!
//! The 24-byte canonical encoding is **NOT** a lexicographic byte-order
//! of the total order. The encoding layout (mantissa-first, sign-last)
//! is chosen for hash-bucket distribution, not sortedness. Never use
//! `to_bytes()` to sort a collection of `Dfp` values — use the `Ord`
//! impl. The cross-replica byte-equality test in
//! `tests/ewma_vectors.rs::rfc0968_section23_replica_byte_equality`
//! checks identity, not ordering.
//!
//! ## Canonical-form helper
//!
//! `is_structurally_canonical()` (alias: `is_valid()`) is the canonical
//! read-path predicate. It returns `false` for NaN, `false` for
//! ±Infinity, `true` for canonical Normal (non-zero mantissa, exponent
//! in `[DFP_MIN_EXPONENT, DFP_MAX_EXPONENT]`), and `true` for Zero.
//! RFC-0968 R17 reads `is_valid() == !matches!(class, NaN)`; this
//! impl is strictly stronger (also rejects Infinity + malformed Normal).

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
            // +0 > -0, +Inf > -Inf per IEEE-754 totalOrder.
            other.sign.cmp(&this.sign)
        }
        DfpClass::Normal => {
            if this.sign != other.sign {
                return if this.sign {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
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
    /// Absolute value: returns a non-negative copy of `self`. Preserves
    /// the class. NaN and Infinity are propagated unchanged; calling
    /// `abs()` on a NaN does NOT clear the sign bit of a payload-built
    /// NaN — use `Dfp::nan().abs()` to canonicalize.
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

    // Negation: see `impl std::ops::Neg for Dfp` below.

    /// Returns `true` if `self` is NaN.
    pub fn is_nan(&self) -> bool {
        matches!(self.class, DfpClass::NaN)
    }

    /// Returns `true` if `self` is ±∞.
    pub fn is_infinite(&self) -> bool {
        matches!(self.class, DfpClass::Infinity)
    }

    /// Returns `true` for finite (non-NaN, non-±∞) values: Normal or Zero.
    pub fn is_finite(&self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }

    /// Returns `true` for Zero (either sign).
    pub fn is_zero(&self) -> bool {
        matches!(self.class, DfpClass::Zero)
    }

    /// Read-path sanity check: `true` iff the value is safe to consume
    /// in deterministic arithmetic. Returns:
    /// - `false` for NaN;
    /// - `false` for ±Infinity (RFC-0104 says Infinity is unreachable
    ///   in compliant implementations, and `dfp_add` rejects it);
    /// - `true` for Zero (either sign);
    /// - `true` for a structurally canonical Normal (non-zero mantissa,
    ///   exponent in `[DFP_MIN_EXPONENT, DFP_MAX_EXPONENT]`);
    /// - `false` for a malformed Normal (e.g., constructed via
    ///   `Dfp::new(0, _, Normal, _)` before `normalize()` runs).
    ///
    /// Alias: `is_valid()` (kept for back-compat with RFC-0968 R17).
    pub fn is_structurally_canonical(&self) -> bool {
        if self.is_nan() || self.is_infinite() {
            return false;
        }
        match self.class {
            DfpClass::Zero => true,
            DfpClass::Normal => {
                self.mantissa != 0
                    && self.exponent >= crate::DFP_MIN_EXPONENT
                    && self.exponent <= crate::DFP_MAX_EXPONENT
            }
            DfpClass::Infinity | DfpClass::NaN => unreachable!(),
        }
    }

    /// Back-compat alias for [`is_structurally_canonical`].
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.is_structurally_canonical()
    }

    /// Returns `true` if `self` and `other` are equal under the IEEE-754
    /// equality specification used by RFC-0104 §989-990:
    /// - `NaN == NaN` (any two NaN payloads compare equal — useful for
    ///   "is this a canonical NaN" checks);
    /// - `−0 == +0` (sign-blind for Zero);
    /// - `+Inf == +Inf` and `−Inf == −Inf`, but `+Inf != −Inf`;
    /// - Normal values: structurally equal iff mantissa, exponent, sign,
    ///   and class all match.
    ///
    /// Distinct from the derived `PartialEq` (`==`), which compares every
    /// field strictly (so `−0 == +0` is `false` and `NaN == NaN` is
    /// `false`). Use `eq_ieee754` for spec-compliance checks; use `==`
    /// for fast structural hashing.
    pub fn eq_ieee754(&self, other: &Self) -> bool {
        if self.class != other.class {
            return false;
        }
        match self.class {
            DfpClass::NaN => true,  // IEEE-754 equality: all NaN equal.
            DfpClass::Zero => true, // Sign-blind for zero: +0 == -0.
            DfpClass::Infinity => self.sign == other.sign, // +Inf == +Inf, +Inf != -Inf.
            DfpClass::Normal => {
                self.mantissa == other.mantissa
                    && self.exponent == other.exponent
                    && self.sign == other.sign
            }
        }
    }

    /// Returns the smaller of `self` and `other` under the canonical
    /// total order. NaN sorts greatest, so the non-NaN operand wins.
    pub fn min(self, other: Self) -> Self {
        match dfp_total_cmp(&self, &other) {
            Ordering::Greater => other,
            _ => self,
        }
    }

    /// Returns the larger of `self` and `other` under the canonical
    /// total order. NaN sorts greatest, so it wins.
    pub fn max(self, other: Self) -> Self {
        match dfp_total_cmp(&self, &other) {
            Ordering::Less => other,
            _ => self,
        }
    }

    /// Clamp `self` into `[lo, hi]`.
    ///
    /// Behaviour summary:
    /// - NaN clamps to `lo` (defensive default; `lo` is the floor,
    ///   matching the Rust `f64::clamp` convention adopted by
    ///   `update_ewma`'s input validation).
    /// - If `lo > hi` (inverted bounds), returns `lo`.
    /// - Otherwise returns `self` clamped to the range.
    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        if self.is_nan() {
            return lo;
        }
        if dfp_total_cmp(&lo, &hi) == Ordering::Greater {
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

#[allow(
    clippy::non_canonical_partial_ord_impl,
    reason = "Dfp has a true partial order: PartialOrd::partial_cmp returns \
              None for NaN, while Ord::cmp returns a total order with NaN \
              sorted greatest. The two impls differ semantically and both \
              must exist; this is not a typo of delegating to Ord::cmp."
)]
impl PartialOrd for Dfp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Rust idiom: NaN is incomparable. Returns None when either
        // operand is NaN so callers can distinguish "incomparable"
        // from "greater" / "less". The clippy lint flags this because
        // most PartialOrd impls delegate to Ord::cmp; we genuinely have
        // a partial order here, so the allow is documented.
        if self.is_nan() || other.is_nan() {
            return None;
        }
        Some(dfp_total_cmp(self, other))
    }
}

impl Ord for Dfp {
    fn cmp(&self, other: &Self) -> Ordering {
        dfp_total_cmp(self, other)
    }
}

impl std::ops::Neg for Dfp {
    type Output = Self;
    fn neg(self) -> Self {
        // NaN negation MUST preserve the canonical NaN form (sign =
        // false, mantissa = 0, exponent = 0). Any other payload is
        // promoted to the canonical form.
        if self.is_nan() {
            return Dfp::nan();
        }
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
        assert!(!n.abs().sign);
        assert_eq!(n.abs(), Dfp::from_f64(3.5));
    }

    #[test]
    fn abs_zero_preserves_class() {
        assert!(Dfp::neg_zero().abs().is_zero());
        assert!(!Dfp::neg_zero().abs().sign);
    }

    #[test]
    fn abs_infinity_preserves_class_and_clears_sign() {
        assert!(Dfp::neg_infinity().abs().is_infinite());
        assert!(!Dfp::neg_infinity().abs().sign);
    }

    #[test]
    fn neg_flips_sign_zero() {
        assert!(Dfp::zero().neg().sign);
        assert!(!Dfp::neg_zero().neg().sign);
    }

    #[test]
    fn neg_flips_sign_infinity() {
        assert!(Dfp::infinity().neg().is_infinite());
        assert!(Dfp::infinity().neg().sign);
    }

    #[test]
    fn neg_nan_is_canonical() {
        // NaN.neg() must produce the canonical Dfp::nan() form (not
        // a sign-flipping payload-built NaN).
        let n = Dfp::nan();
        let neg_n = n.neg();
        assert!(neg_n.is_nan());
        assert!(!neg_n.sign);
        assert_eq!(neg_n.mantissa, 0);
        assert_eq!(neg_n.exponent, 0);
    }

    #[test]
    fn neg_zero_double_neg_round_trip() {
        assert_eq!(Dfp::zero().neg().neg(), Dfp::zero());
    }

    #[test]
    fn is_finite_distinguishes_nan_inf_zero_normal() {
        assert!(!Dfp::nan().is_finite());
        assert!(!Dfp::infinity().is_finite());
        assert!(Dfp::zero().is_finite());
        assert!(Dfp::from_f64(1.0).is_finite());
    }

    #[test]
    fn is_structurally_canonical_zero_and_normal() {
        assert!(Dfp::zero().is_structurally_canonical());
        assert!(Dfp::from_f64(0.5).is_structurally_canonical());
    }

    #[test]
    fn is_structurally_canonical_rejects_nan_and_infinity() {
        assert!(!Dfp::nan().is_structurally_canonical());
        assert!(!Dfp::infinity().is_structurally_canonical());
        assert!(!Dfp::neg_infinity().is_structurally_canonical());
    }

    #[test]
    fn is_structurally_canonical_rejects_zero_mantissa_normal() {
        let bad = Dfp {
            mantissa: 0,
            exponent: 0,
            class: DfpClass::Normal,
            sign: false,
        };
        assert!(!bad.is_structurally_canonical());
    }

    #[test]
    fn is_structurally_canonical_rejects_out_of_range_exponent() {
        let bad_high = Dfp {
            mantissa: 1,
            exponent: crate::DFP_MAX_EXPONENT + 1,
            class: DfpClass::Normal,
            sign: false,
        };
        let bad_low = Dfp {
            mantissa: 1,
            exponent: crate::DFP_MIN_EXPONENT - 1,
            class: DfpClass::Normal,
            sign: false,
        };
        assert!(!bad_high.is_structurally_canonical());
        assert!(!bad_low.is_structurally_canonical());
    }

    #[test]
    fn is_valid_aliases_structurally_canonical() {
        assert!(Dfp::zero().is_valid());
        assert!(!Dfp::nan().is_valid());
        assert!(!Dfp::infinity().is_valid());
    }

    #[test]
    fn eq_ieee754_zero_sign_blind() {
        assert!(Dfp::neg_zero().eq_ieee754(&Dfp::zero()));
        assert!(Dfp::zero().eq_ieee754(&Dfp::neg_zero()));
    }

    #[test]
    fn eq_ieee754_infinity_sign_aware() {
        assert!(Dfp::infinity().eq_ieee754(&Dfp::infinity()));
        assert!(Dfp::neg_infinity().eq_ieee754(&Dfp::neg_infinity()));
        assert!(!Dfp::infinity().eq_ieee754(&Dfp::neg_infinity()));
    }

    #[test]
    fn eq_ieee754_nan_always_equal() {
        // Spec compliant: all NaN equal.
        assert!(Dfp::nan().eq_ieee754(&Dfp::nan()));
        let payload_nan = Dfp {
            mantissa: 0xDEAD,
            exponent: 99,
            class: DfpClass::NaN,
            sign: true,
        };
        assert!(payload_nan.eq_ieee754(&Dfp::nan()));
    }

    #[test]
    fn eq_ieee754_normal_structural() {
        assert!(Dfp::from_f64(1.0).eq_ieee754(&Dfp::from_f64(1.0)));
        assert!(!Dfp::from_f64(1.0).eq_ieee754(&Dfp::from_f64(-1.0)));
        assert!(!Dfp::from_f64(1.0).eq_ieee754(&Dfp::from_f64(2.0)));
    }

    #[test]
    fn partial_cmp_nan_returns_none() {
        // Rust idiom: NaN is incomparable.
        assert_eq!(Dfp::nan().partial_cmp(&Dfp::nan()), None);
        assert_eq!(Dfp::from_f64(1.0).partial_cmp(&Dfp::nan()), None);
        assert_eq!(Dfp::nan().partial_cmp(&Dfp::from_f64(1.0)), None);
    }

    #[test]
    fn partial_cmp_finite_returns_some() {
        let one = Dfp::from_f64(1.0);
        let two = Dfp::from_f64(2.0);
        assert_eq!(one.partial_cmp(&two), Some(Ordering::Less));
        assert_eq!(two.partial_cmp(&one), Some(Ordering::Greater));
        assert_eq!(one.partial_cmp(&one), Some(Ordering::Equal));
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
        let neg_two = Dfp::from_f64(-2.0);
        assert!(one < two);
        assert!(neg_one < one);
        // Negative-magnitude DESC: -2 < -1 (more-negative sorts earlier).
        assert!(neg_two < neg_one);
        assert_eq!(one.cmp(&Dfp::from_f64(1.0)), Ordering::Equal);
    }

    #[test]
    fn cmp_nan_is_greater_than_all_finite() {
        // Total order (Ord::cmp): NaN sorts greatest.
        let inf = Dfp::infinity();
        let one = Dfp::from_f64(1.0);
        let zero = Dfp::zero();
        assert_eq!(zero.cmp(&Dfp::nan()), Ordering::Less);
        assert_eq!(one.cmp(&Dfp::nan()), Ordering::Less);
        assert_eq!(inf.cmp(&Dfp::nan()), Ordering::Less);
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
    fn min_max_nan_loses_in_min_wins_in_max() {
        // NaN sorts greatest, so it loses in min and wins in max.
        let one = Dfp::from_f64(1.0);
        assert_eq!(one.min(Dfp::nan()), one);
        assert_eq!(Dfp::nan().max(one), Dfp::nan());
    }

    #[test]
    fn clamp_nan_clamps_to_lo() {
        let lo = Dfp::from_f64(0.0);
        let hi = Dfp::from_f64(1.0);
        assert_eq!(Dfp::nan().clamp(lo, hi), lo);
    }

    #[test]
    fn clamp_finite_zero_in_zero_out() {
        let v = Dfp::nan();
        let lo = Dfp::zero();
        let hi = Dfp::zero();
        assert_eq!(v.clamp(lo, hi), lo);
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
