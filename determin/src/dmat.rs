//! Deterministic Matrix (DMAT) Implementation
//!
//! This module implements RFC-0113 v1.21: Deterministic Matrices (DMAT)
//!
//! ## Trait Architecture
//!
//! `NumericScalar` (RFC-0113) and `DvecScalar` (RFC-0112) are **sibling traits** with
//! semantic overlap. Both are implemented by `Dqa` and `Decimal`. The sibling relationship
//! avoids receiver conflicts: `DvecScalar` uses consuming `self` receivers while
//! `NumericScalar` uses `&self` receivers per RFC-0113 convention.
//!
//! ```text
//! NumericScalar (RFC-0113)     DvecScalar (RFC-0112)
//!       ↑                              ↑
//!       |                              |
//!     Dqa, Decimal <-- both types ---> Dqa, Decimal
//! ```
//!
//! The RFC's §Trait Version Enforcement explicitly permits this:
//! "A type MAY additionally implement the RFC-0112 trait methods...provided those
//! methods are not invoked during DMAT operation execution."
//!
//! ## Memory Layout
//!
//! Row-major: Index(i, j) = i * cols + j
//!
//! ```text
//! 2×3 matrix:
//! [ a00, a01, a02 ]
//! [ a10, a11, a12 ]
//!
//! data: [a00, a01, a02, a10, a11, a12]
//! ```

use crate::decimal::DecimalError;
use crate::decimal::{self as decimal_mod, Decimal};
use crate::dqa::Dqa;
use crate::dqa::DqaError;

// =============================================================================
// DMatError
// =============================================================================

/// Unified DMAT error type — covers scalar errors and matrix-level TRAP conditions.
///
/// **Distinct error origins:**
/// - `Dqa(DqaError)` / `Decimal(DecimalError)`: Scalar arithmetic errors
/// - `Overflow`: Matrix-level accumulator overflow (i128 > MAX_MANTISSA)
/// - `TrapInput`: TRAP sentinel in input data
/// - `Dimension*`: Matrix dimension violations
/// - `ScaleMismatch` / `InvalidScale`: Scale constraint violations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmatError {
    /// Scalar error from DQA operation
    Dqa(DqaError),
    /// Scalar error from Decimal operation
    Decimal(DecimalError),
    /// Matrix dimensions incompatible for operation
    DimensionMismatch,
    /// Matrix exceeds size limits (M×N > 64, M > 8, N > 8, M < 1, N < 1)
    DimensionError,
    /// Element scales within a matrix are not uniform
    ScaleMismatch,
    /// Result scale exceeds MAX_SCALE (18 for DQA, 36 for Decimal)
    InvalidScale,
    /// i128 accumulator exceeds MAX_MANTISSA during MAT_MUL/MAT_VEC_MUL
    Overflow,
    /// Input contains TRAP sentinel (Phase 0 pre-check failure)
    TrapInput,
}

impl From<DqaError> for DmatError {
    fn from(e: DqaError) -> Self {
        // Note: DqaError::Overflow is distinct from DmatError::Overflow.
        // DqaError::Overflow comes from scalar ops; DmatError::Overflow
        // comes from i128 accumulator exceeding MAX_MANTISSA in MAT_MUL.
        DmatError::Dqa(e)
    }
}

impl From<DecimalError> for DmatError {
    fn from(e: DecimalError) -> Self {
        DmatError::Decimal(e)
    }
}

// =============================================================================
// NumericScalar Trait (RFC-0113)
// =============================================================================

/// Trait for numeric scalar types that can be elements of DMat (RFC-0113).
///
/// This is a **sibling trait** to `DvecScalar` (RFC-0112), not a subtrait.
/// Both traits are implemented by `Dqa` and `Decimal` for their respective
/// operation contexts.
///
/// ## RFC-0113 §Trait Version Enforcement
///
/// The `NumericScalar` trait defined here is the **canonical and exclusive**
/// trait definition for all consensus-critical numeric operations involving DMAT.
///
/// ## Semantic Equivalence with DvecScalar::from_parts
///
/// `NumericScalar::new(mantissa, scale)` and `DvecScalar::from_parts(mantissa, scale)`
/// are semantically identical — both construct a scalar from raw parts.
/// Both exist due to receiver convention differences between RFC-0112 (consuming `self`)
/// and RFC-0113 (`&self`). Implementations MUST be functionally identical.
pub trait NumericScalar: Clone {
    /// Associated error type for arithmetic operations.
    type Error: Into<DmatError> + std::fmt::Debug + PartialEq;

    /// Maximum representable mantissa value for overflow detection.
    ///
    /// For DQA: `i64::MAX = 2^63 - 1`
    /// For Decimal: `MAX_DECIMAL_MANTISSA`
    const MAX_MANTISSA: i128;

    /// Maximum allowed scale.
    ///
    /// For DQA: 18 (per RFC-0105)
    /// For Decimal: 36 (per RFC-0111)
    const MAX_SCALE: u8;

    /// Construct from raw mantissa and scale.
    ///
    /// Semantically equivalent to `DvecScalar::from_parts` — both exist due to
    /// receiver convention differences between RFC-0112 and RFC-0113.
    /// Implementations MUST be identical.
    fn new(mantissa: i128, scale: u8) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Returns true if this value is the TRAP sentinel.
    ///
    /// TRAP sentinel encoding: `{ mantissa: -(1 << 63), scale: 0xFF }`
    ///
    /// Phase 0 of every DMAT operation MUST check this before any other validation.
    fn is_trap(&self) -> bool;

    /// Return the decimal scale.
    fn scale(&self) -> u8;

    /// Return the raw mantissa as i128.
    ///
    /// For DQA: sign-extend i64 → i128 (two's complement).
    /// For Decimal: return mantissa directly.
    fn raw_mantissa(&self) -> i128;

    /// Add with `&self` receiver (RFC-0113 convention).
    fn add(&self, other: &Self) -> Result<Self, Self::Error>;

    /// Subtract with `&self` receiver (RFC-0113 convention).
    fn sub(&self, other: &Self) -> Result<Self, Self::Error>;

    /// Multiply with `&self` receiver (RFC-0113 convention).
    fn mul(&self, other: &Self) -> Result<Self, Self::Error>;
}

// =============================================================================
// DMat Type
// =============================================================================

/// Deterministic Matrix — generic over any `NumericScalar` type.
///
/// ## Protocol Invariant (CRIT-4)
///
/// It is a protocol invariant that `data.len() == rows * cols` for any well-formed
/// `DMat`. Implementations **MUST** enforce this at construction time.
///
/// ## Production Limitations
///
/// | Feature | Limit | Status |
/// |---------|-------|--------|
/// | DMAT<DQA> | M×N ≤ 64, M ≤ 8, N ≤ 8, M ≥ 1, N ≥ 1 | ALLOWED |
/// | DMAT<Decimal> | M×N ≤ 64, M ≤ 8, N ≤ 8, M ≥ 1, N ≥ 1 | ALLOWED |
/// | DMAT<DFP> | DISABLED | FORBIDDEN |
///
/// ## Memory Layout
///
/// Row-major order: `Index(i, j) = i * cols + j`
pub struct DMat<T: NumericScalar> {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<T>,
}

impl<T: NumericScalar> DMat<T> {
    /// Create a new DMat with validation.
    ///
    /// Enforces protocol invariant: `data.len() == rows * cols`
    pub fn new(rows: usize, cols: usize, data: Vec<T>) -> Result<Self, DmatError> {
        if rows * cols != data.len() {
            return Err(DmatError::DimensionError);
        }
        Ok(Self { rows, cols, data })
    }

    /// Number of elements (M × N).
    pub fn len(&self) -> usize {
        self.rows * self.cols
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.rows == 0 || self.cols == 0
    }

    /// Get element at (i, j) — row-major index.
    pub fn get(&self, i: usize, j: usize) -> Option<&T> {
        if i >= self.rows || j >= self.cols {
            return None;
        }
        self.data.get(i * self.cols + j)
    }

    /// Validate dimension constraints per Production Limitations.
    pub fn validate_dims(&self) -> Result<(), DmatError> {
        if self.rows == 0 || self.cols == 0 {
            return Err(DmatError::DimensionError);
        }
        if self.rows * self.cols > 64 {
            return Err(DmatError::DimensionError);
        }
        if self.rows > 8 || self.cols > 8 {
            return Err(DmatError::DimensionError);
        }
        Ok(())
    }
}

// =============================================================================
// MAT_ADD and MAT_SUB — Shared Validation
// =============================================================================

/// Validate inputs for MAT_ADD and MAT_SUB operations.
///
/// Phase 0: TRAP sentinel pre-check (scan a fully, then b — per RFC Global TRAP Invariant)
/// Phase 1: Dimension validation
/// Phase 2: Scale validation (uniform within each matrix, cross-matrix equality)
///
/// Returns `(rows, cols, common_scale)` on success.
///
/// # Global TRAP Invariant (CRITICAL)
/// TRAP sentinel detection MUST iterate elements in strict row-major order using
/// index `(i * cols + j)`. For binary operations, all elements of operand `a`
/// MUST be scanned before any element of operand `b`. This ensures deterministic
/// TRAP detection order across implementations.
fn validate_additive_op<T: NumericScalar>(
    a: &DMat<T>,
    b: &DMat<T>,
) -> Result<(usize, usize, u8), DmatError> {
    // Phase 0: TRAP sentinel pre-check — scan a fully, then b
    // Global TRAP Invariant: row-major order, a before b
    for i in 0..a.rows {
        for j in 0..a.cols {
            if a.data[i * a.cols + j].is_trap() {
                return Err(DmatError::TrapInput);
            }
        }
    }
    for i in 0..b.rows {
        for j in 0..b.cols {
            if b.data[i * b.cols + j].is_trap() {
                return Err(DmatError::TrapInput);
            }
        }
    }

    // Phase 1: Dimension validation
    if a.rows != b.rows || a.cols != b.cols {
        return Err(DmatError::DimensionMismatch);
    }
    if a.rows * a.cols > 64 {
        return Err(DmatError::DimensionError);
    }
    if a.rows > 8 || a.cols > 8 {
        return Err(DmatError::DimensionError);
    }
    if a.rows < 1 || a.cols < 1 {
        return Err(DmatError::DimensionError);
    }

    // Phase 2: Scale validation — uniform within each matrix, cross-matrix equality
    let common_scale = a.data[0].scale();
    for i in 0..a.rows {
        for j in 0..a.cols {
            if a.data[i * a.cols + j].scale() != common_scale {
                return Err(DmatError::ScaleMismatch);
            }
        }
    }
    for i in 0..b.rows {
        for j in 0..b.cols {
            if b.data[i * b.cols + j].scale() != common_scale {
                return Err(DmatError::ScaleMismatch);
            }
        }
    }
    // Cross-matrix scale check
    if b.data[0].scale() != common_scale {
        return Err(DmatError::ScaleMismatch);
    }

    Ok((a.rows, a.cols, common_scale))
}

// =============================================================================
// MAT_ADD — Matrix Addition
// =============================================================================

/// Matrix addition: C = A + B
///
/// Both matrices must have the same dimensions and element scales.
pub fn mat_add<T: NumericScalar>(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, DmatError> {
    let (rows, cols, _scale) = validate_additive_op(a, b)?;

    let mut result_data = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            let sum = a.data[i * cols + j]
                .add(&b.data[i * cols + j])
                .map_err(|e| e.into())?;
            result_data.push(sum);
        }
    }

    DMat::new(rows, cols, result_data).map_err(|_| DmatError::DimensionError)
}

// =============================================================================
// MAT_SUB — Matrix Subtraction
// =============================================================================

/// Matrix subtraction: C = A - B
///
/// Both matrices must have the same dimensions and element scales.
pub fn mat_sub<T: NumericScalar>(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, DmatError> {
    let (rows, cols, _scale) = validate_additive_op(a, b)?;

    let mut result_data = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            let diff = a.data[i * cols + j]
                .sub(&b.data[i * cols + j])
                .map_err(|e| e.into())?;
            result_data.push(diff);
        }
    }

    DMat::new(rows, cols, result_data).map_err(|_| DmatError::DimensionError)
}

// =============================================================================
// Implement NumericScalar for Dqa
// =============================================================================

impl NumericScalar for Dqa {
    type Error = DqaError;

    const MAX_MANTISSA: i128 = i64::MAX as i128;

    const MAX_SCALE: u8 = 18;

    fn new(mantissa: i128, scale: u8) -> Result<Self, Self::Error> {
        // Identical to DvecScalar::from_parts - see trait docstring
        if mantissa > i64::MAX as i128 || mantissa < i64::MIN as i128 {
            return Err(DqaError::Overflow);
        }
        Dqa::new(mantissa as i64, scale)
    }

    fn is_trap(&self) -> bool {
        // TRAP sentinel: { mantissa: -(1 << 63), scale: 0xFF }
        // For Dqa, this means value == i64::MIN and scale == 0xFF
        self.value == i64::MIN && self.scale == 0xFF
    }

    fn scale(&self) -> u8 {
        self.scale
    }

    fn raw_mantissa(&self) -> i128 {
        i128::from(self.value)
    }

    fn add(&self, other: &Self) -> Result<Self, Self::Error> {
        crate::dqa::dqa_add(*self, *other)
    }

    fn sub(&self, other: &Self) -> Result<Self, Self::Error> {
        crate::dqa::dqa_sub(*self, *other)
    }

    fn mul(&self, other: &Self) -> Result<Self, Self::Error> {
        crate::dqa::dqa_mul(*self, *other)
    }
}

// =============================================================================
// Implement NumericScalar for Decimal
// =============================================================================

impl NumericScalar for Decimal {
    type Error = DecimalError;

    const MAX_MANTISSA: i128 = crate::decimal::MAX_DECIMAL_MANTISSA;

    const MAX_SCALE: u8 = 36;

    fn new(mantissa: i128, scale: u8) -> Result<Self, Self::Error> {
        // Identical to DvecScalar::from_parts - see trait docstring
        Decimal::new(mantissa, scale)
    }

    fn is_trap(&self) -> bool {
        // TRAP sentinel: { mantissa: -(1 << 63), scale: 0xFF }
        // For Decimal, this means mantissa == i64::MIN and scale == 0xFF
        self.mantissa() == i64::MIN as i128 && self.scale() == 0xFF
    }

    fn scale(&self) -> u8 {
        Decimal::scale(self)
    }

    fn raw_mantissa(&self) -> i128 {
        Decimal::mantissa(self)
    }

    fn add(&self, other: &Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_add(self, other)
    }

    fn sub(&self, other: &Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_sub(self, other)
    }

    fn mul(&self, other: &Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_mul(self, other)
    }
}

// =============================================================================
// Index trait for convenience: mat[(i, j)] syntax
// =============================================================================

/// Index type for DMat: (row, col)
pub struct DMatIndex(usize, usize);

impl From<(usize, usize)> for DMatIndex {
    fn from((r, c): (usize, usize)) -> Self {
        DMatIndex(r, c)
    }
}

impl<T: NumericScalar, I: Into<DMatIndex>> std::ops::Index<I> for DMat<T> {
    type Output = T;

    fn index(&self, index: I) -> &Self::Output {
        let idx = index.into();
        &self.data[idx.0 * self.cols + idx.1]
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dmat_creation_valid() {
        let data = vec![
            Dqa::new(1, 0).unwrap(),
            Dqa::new(2, 0).unwrap(),
            Dqa::new(3, 0).unwrap(),
            Dqa::new(4, 0).unwrap(),
        ];
        let mat = DMat::new(2, 2, data).unwrap();
        assert_eq!(mat.rows, 2);
        assert_eq!(mat.cols, 2);
        assert_eq!(mat.len(), 4);
    }

    #[test]
    fn test_dmat_creation_invalid_dims() {
        let data = vec![
            Dqa::new(1, 0).unwrap(),
            Dqa::new(2, 0).unwrap(),
            Dqa::new(3, 0).unwrap(),
        ];
        // 2×2 matrix but 3 elements
        let result = DMat::new(2, 2, data);
        assert!(matches!(result, Err(DmatError::DimensionError)));
    }

    #[test]
    fn test_dmat_get() {
        let data = vec![
            Dqa::new(1, 0).unwrap(),
            Dqa::new(2, 0).unwrap(),
            Dqa::new(3, 0).unwrap(),
            Dqa::new(4, 0).unwrap(),
        ];
        let mat = DMat::new(2, 2, data).unwrap();
        // Row-major: [a00, a01, a10, a11]
        assert_eq!(mat.get(0, 0).unwrap(), &Dqa::new(1, 0).unwrap());
        assert_eq!(mat.get(0, 1).unwrap(), &Dqa::new(2, 0).unwrap());
        assert_eq!(mat.get(1, 0).unwrap(), &Dqa::new(3, 0).unwrap());
        assert_eq!(mat.get(1, 1).unwrap(), &Dqa::new(4, 0).unwrap());
    }

    #[test]
    fn test_dmat_get_out_of_bounds() {
        let data = vec![Dqa::new(1, 0).unwrap()];
        let mat = DMat::new(1, 1, data).unwrap();
        assert!(mat.get(1, 0).is_none());
        assert!(mat.get(0, 1).is_none());
    }

    #[test]
    fn test_dmat_validate_dims_valid() {
        let data = vec![Dqa::new(1, 0).unwrap(); 64];
        let mat = DMat::new(8, 8, data).unwrap();
        assert!(mat.validate_dims().is_ok());
    }

    #[test]
    fn test_dmat_validate_dims_too_large() {
        let data = vec![Dqa::new(1, 0).unwrap(); 65];
        let mat = DMat::new(1, 65, data).unwrap();
        assert!(matches!(
            mat.validate_dims(),
            Err(DmatError::DimensionError)
        ));
    }

    #[test]
    fn test_dmat_validate_dims_exceeds_8() {
        // 8×8 = 64, but 9>8 so should fail even though M×N ≤ 64
        let data = vec![Dqa::new(1, 0).unwrap(); 64];
        let mat = DMat::new(8, 8, data).unwrap();
        assert!(mat.validate_dims().is_ok()); // 8×8 is valid
                                              // Now test a 9-row matrix (9>8)
        let data2 = vec![Dqa::new(1, 0).unwrap(); 63];
        let mat2 = DMat::new(9, 7, data2).unwrap();
        assert!(matches!(
            mat2.validate_dims(),
            Err(DmatError::DimensionError)
        ));
    }

    #[test]
    fn test_dmat_validate_dims_zero() {
        let data: Vec<Dqa> = vec![];
        let mat = DMat::new(0, 0, data).unwrap();
        assert!(matches!(
            mat.validate_dims(),
            Err(DmatError::DimensionError)
        ));
    }

    #[test]
    fn test_numeric_scalar_dqa_trap() {
        // TRAP sentinel: value == i64::MIN, scale == 0xFF
        // Cannot use Dqa::new() because it validates scale <= 18
        let trap = Dqa {
            value: i64::MIN,
            scale: 0xFF,
        };
        assert!(trap.is_trap());

        let normal = Dqa::new(42, 0).unwrap();
        assert!(!normal.is_trap());
    }

    #[test]
    fn test_numeric_scalar_dqa_new() {
        let d = Dqa::new(12345, 3).unwrap();
        let constructed = <Dqa as NumericScalar>::new(12345, 3).unwrap();
        assert_eq!(d, constructed);
    }

    #[test]
    fn test_numeric_scalar_dqa_new_overflow() {
        // i64::MAX + 1 overflows
        let result = <Dqa as NumericScalar>::new(i64::MAX as i128 + 1, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_scalar_dqa_arithmetic() {
        let a = Dqa::new(5, 0).unwrap();
        let b = Dqa::new(3, 0).unwrap();

        // Use trait syntax to call &self methods
        let sum = NumericScalar::add(&a, &b).unwrap();
        assert_eq!(sum, Dqa::new(8, 0).unwrap());

        let diff = NumericScalar::sub(&a, &b).unwrap();
        assert_eq!(diff, Dqa::new(2, 0).unwrap());

        let prod = NumericScalar::mul(&a, &b).unwrap();
        assert_eq!(prod, Dqa::new(15, 0).unwrap());
    }

    #[test]
    fn test_numeric_scalar_decimal_trap() {
        // Cannot easily construct Decimal with scale=0xFF (InvalidScale) through public API
        // Test that normal values return false for is_trap()
        let normal = Decimal::new(42, 0).unwrap();
        assert!(!normal.is_trap());

        // Also test that scale=0 is not a trap
        let zero = Decimal::new(0, 0).unwrap();
        assert!(!zero.is_trap());
    }

    #[test]
    fn test_numeric_scalar_decimal_new() {
        let d = Decimal::new(12345, 3).unwrap();
        let constructed = <Decimal as NumericScalar>::new(12345, 3).unwrap();
        assert_eq!(d, constructed);
    }

    #[test]
    fn test_numeric_scalar_decimal_arithmetic() {
        let a = Decimal::new(5, 0).unwrap();
        let b = Decimal::new(3, 0).unwrap();

        let sum = a.add(&b).unwrap();
        assert_eq!(sum, Decimal::new(8, 0).unwrap());

        let diff = a.sub(&b).unwrap();
        assert_eq!(diff, Decimal::new(2, 0).unwrap());

        let prod = a.mul(&b).unwrap();
        assert_eq!(prod, Decimal::new(15, 0).unwrap());
    }

    #[test]
    fn test_dmat_decimal_creation() {
        let data = vec![
            Decimal::new(1, 0).unwrap(),
            Decimal::new(2, 0).unwrap(),
            Decimal::new(3, 0).unwrap(),
            Decimal::new(4, 0).unwrap(),
        ];
        let mat = DMat::new(2, 2, data).unwrap();
        assert_eq!(mat.rows, 2);
        assert_eq!(mat.cols, 2);
    }

    #[test]
    fn test_dmat_error_from_dqa() {
        let overflow = DqaError::Overflow;
        let dmat_err: DmatError = overflow.into();
        assert!(matches!(dmat_err, DmatError::Dqa(DqaError::Overflow)));
    }

    #[test]
    fn test_dmat_error_from_decimal() {
        let err = DecimalError::InvalidScale;
        let dmat_err: DmatError = err.into();
        assert!(matches!(
            dmat_err,
            DmatError::Decimal(DecimalError::InvalidScale)
        ));
    }

    #[test]
    fn test_max_scale_constants() {
        assert_eq!(<Dqa as NumericScalar>::MAX_SCALE, 18);
        assert_eq!(<Decimal as NumericScalar>::MAX_SCALE, 36);
    }

    #[test]
    fn test_max_mantissa_constants() {
        assert_eq!(<Dqa as NumericScalar>::MAX_MANTISSA, i64::MAX as i128);
        assert_eq!(
            <Decimal as NumericScalar>::MAX_MANTISSA,
            crate::decimal::MAX_DECIMAL_MANTISSA
        );
    }

    // =============================================================================
    // MAT_ADD Tests
    // =============================================================================

    #[test]
    fn test_mat_add_dqa_basic() {
        // [[1, 2], [3, 4]] + [[5, 6], [7, 8]] = [[6, 8], [10, 12]]
        let a_data = vec![
            Dqa::new(1, 0).unwrap(),
            Dqa::new(2, 0).unwrap(),
            Dqa::new(3, 0).unwrap(),
            Dqa::new(4, 0).unwrap(),
        ];
        let b_data = vec![
            Dqa::new(5, 0).unwrap(),
            Dqa::new(6, 0).unwrap(),
            Dqa::new(7, 0).unwrap(),
            Dqa::new(8, 0).unwrap(),
        ];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        let result = mat_add(&a, &b).unwrap();
        assert_eq!(result[(0, 0)], Dqa::new(6, 0).unwrap());
        assert_eq!(result[(0, 1)], Dqa::new(8, 0).unwrap());
        assert_eq!(result[(1, 0)], Dqa::new(10, 0).unwrap());
        assert_eq!(result[(1, 1)], Dqa::new(12, 0).unwrap());
    }

    #[test]
    fn test_mat_add_decimal_basic() {
        let a_data = vec![
            Decimal::new(1, 0).unwrap(),
            Decimal::new(2, 0).unwrap(),
            Decimal::new(3, 0).unwrap(),
            Decimal::new(4, 0).unwrap(),
        ];
        let b_data = vec![
            Decimal::new(5, 0).unwrap(),
            Decimal::new(6, 0).unwrap(),
            Decimal::new(7, 0).unwrap(),
            Decimal::new(8, 0).unwrap(),
        ];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        let result = mat_add(&a, &b).unwrap();
        assert_eq!(result[(0, 0)], Decimal::new(6, 0).unwrap());
        assert_eq!(result[(1, 1)], Decimal::new(12, 0).unwrap());
    }

    #[test]
    fn test_mat_add_with_scale() {
        // [[1, 2], [3, 4]] + [[0.1, 0.2], [0.3, 0.4]] with scales 0 and 1
        // Can't mix scales - should TRAP
        let a_data = vec![
            Dqa::new(1, 0).unwrap(),
            Dqa::new(2, 0).unwrap(),
            Dqa::new(3, 0).unwrap(),
            Dqa::new(4, 0).unwrap(),
        ];
        let b_data = vec![
            Dqa::new(1, 1).unwrap(), // scale 1
            Dqa::new(2, 1).unwrap(),
            Dqa::new(3, 1).unwrap(),
            Dqa::new(4, 1).unwrap(),
        ];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        let result = mat_add(&a, &b);
        assert!(matches!(result, Err(DmatError::ScaleMismatch)));
    }

    #[test]
    fn test_mat_add_dimension_mismatch() {
        let a_data = vec![Dqa::new(1, 0).unwrap(); 4];
        let b_data = vec![Dqa::new(1, 0).unwrap(); 6];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 3, b_data).unwrap();
        assert!(matches!(mat_add(&a, &b), Err(DmatError::DimensionMismatch)));
    }

    #[test]
    fn test_mat_add_trap_sentinel() {
        // Create matrix with TRAP sentinel
        let trap = Dqa { value: i64::MIN, scale: 0xFF };
        let normal = Dqa::new(1, 0).unwrap();
        let a_data = vec![normal, normal, normal, normal];
        let b_data = vec![trap, normal, normal, normal];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        assert!(matches!(mat_add(&a, &b), Err(DmatError::TrapInput)));
    }

    #[test]
    fn test_mat_add_1x2() {
        // 1×2 matrix: [[1, 2]] + [[3, 4]] = [[4, 6]]
        let a_data = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b_data = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let a = DMat::new(1, 2, a_data).unwrap();
        let b = DMat::new(1, 2, b_data).unwrap();
        let result = mat_add(&a, &b).unwrap();
        assert_eq!(result[(0, 0)], Dqa::new(4, 0).unwrap());
        assert_eq!(result[(0, 1)], Dqa::new(6, 0).unwrap());
    }

    // =============================================================================
    // MAT_SUB Tests
    // =============================================================================

    #[test]
    fn test_mat_sub_dqa_basic() {
        // [[5, 6], [7, 8]] - [[1, 2], [3, 4]] = [[4, 4], [4, 4]]
        let a_data = vec![
            Dqa::new(5, 0).unwrap(),
            Dqa::new(6, 0).unwrap(),
            Dqa::new(7, 0).unwrap(),
            Dqa::new(8, 0).unwrap(),
        ];
        let b_data = vec![
            Dqa::new(1, 0).unwrap(),
            Dqa::new(2, 0).unwrap(),
            Dqa::new(3, 0).unwrap(),
            Dqa::new(4, 0).unwrap(),
        ];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        let result = mat_sub(&a, &b).unwrap();
        assert_eq!(result[(0, 0)], Dqa::new(4, 0).unwrap());
        assert_eq!(result[(0, 1)], Dqa::new(4, 0).unwrap());
        assert_eq!(result[(1, 0)], Dqa::new(4, 0).unwrap());
        assert_eq!(result[(1, 1)], Dqa::new(4, 0).unwrap());
    }

    #[test]
    fn test_mat_sub_decimal_basic() {
        let a_data = vec![
            Decimal::new(5, 0).unwrap(),
            Decimal::new(6, 0).unwrap(),
            Decimal::new(7, 0).unwrap(),
            Decimal::new(8, 0).unwrap(),
        ];
        let b_data = vec![
            Decimal::new(1, 0).unwrap(),
            Decimal::new(2, 0).unwrap(),
            Decimal::new(3, 0).unwrap(),
            Decimal::new(4, 0).unwrap(),
        ];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        let result = mat_sub(&a, &b).unwrap();
        assert_eq!(result[(0, 0)], Decimal::new(4, 0).unwrap());
        assert_eq!(result[(1, 1)], Decimal::new(4, 0).unwrap());
    }

    #[test]
    fn test_mat_sub_dimension_mismatch() {
        let a_data = vec![Dqa::new(1, 0).unwrap(); 4];
        let b_data = vec![Dqa::new(1, 0).unwrap(); 4];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(4, 1, b_data).unwrap();
        assert!(matches!(mat_sub(&a, &b), Err(DmatError::DimensionMismatch)));
    }

    #[test]
    fn test_mat_sub_trap_sentinel() {
        let trap = Dqa { value: i64::MIN, scale: 0xFF };
        let normal = Dqa::new(1, 0).unwrap();
        let a_data = vec![normal, normal, normal, normal];
        let b_data = vec![trap, normal, normal, normal];
        let a = DMat::new(2, 2, a_data).unwrap();
        let b = DMat::new(2, 2, b_data).unwrap();
        assert!(matches!(mat_sub(&a, &b), Err(DmatError::TrapInput)));
    }

    #[test]
    fn test_mat_sub_zero_result() {
        // [[1, 1], [1, 1]] - [[1, 1], [1, 1]] = [[0, 0], [0, 0]]
        let data = vec![Dqa::new(1, 0).unwrap(); 4];
        let a = DMat::new(2, 2, data.clone()).unwrap();
        let b = DMat::new(2, 2, data).unwrap();
        let result = mat_sub(&a, &b).unwrap();
        assert_eq!(result[(0, 0)], Dqa::new(0, 0).unwrap());
        assert_eq!(result[(1, 1)], Dqa::new(0, 0).unwrap());
    }


}
