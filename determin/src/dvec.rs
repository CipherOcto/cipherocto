//! Deterministic Vector (DVEC) Implementation
//!
//! This module implements RFC-0112 v1.14: Deterministic Vectors (DVEC)
//!
//! Key design principles:
//! - Generic over any `DvecScalar` type (DQA or Decimal)
//! - DVEC<DFP> is FORBIDDEN (no impl of DvecScalar for Dfp)
//! - All operations use sequential loops (no SIMD, no tree reduction)
//! - BigInt accumulator prevents i128 overflow during accumulation
//! - Canonical form required for all inputs/outputs
//!
//! ## Trait Version Note (RFC-0112 → RFC-0113)
//!
//! `DvecScalar` is the RFC-0112 original trait. RFC-0113 defines the canonical
//! `NumericScalar` trait with two additional members: `const MAX_MANTISSA` and
//! `fn new(mantissa: i128, scale: u8) -> Self`. When RFC-0113 (DMAT) is
//! implemented, `NumericScalar` should extend `DvecScalar`:
//!
//!   pub trait NumericScalar: DvecScalar {
//!       const MAX_MANTISSA: i128;
//!       fn new(mantissa: i128, scale: u8) -> Self;
//!   }
//!
//! DVEC algorithms do not use the RFC-0113 additions, so this migration
//! is additive and non-breaking for DVEC.

use crate::decimal::DecimalError;
use crate::decimal::{self as decimal_mod, Decimal};
use crate::dqa::DqaError;
use crate::dqa::{self as dqa_mod, Dqa};

// =============================================================================
// Error Types
// =============================================================================

/// Unified DVEC error type — covers scalar errors and vector-level TRAP conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DvecError {
    Dqa(DqaError),
    Decimal(DecimalError),
    DimensionMismatch,
    DimensionExceeded,
    ScaleMismatch,
    /// DQA: scale > 9 (DOT_PRODUCT/SQUARED_DISTANCE input validation)
    /// Maps to INPUT_VALIDATION_ERROR or INPUT_SCALE depending on operation.
    InputScaleExceeded,
    Overflow,
    /// DQA NORM — DQA has no SQRT per RFC-0105.
    Unsupported,
    /// NORMALIZE is forbidden in consensus (exceeds gas budget).
    ConsensusRestriction,
    CannotNormalizeZeroVector,
    DivisionByZero,
}

impl From<DqaError> for DvecError {
    fn from(e: DqaError) -> Self {
        DvecError::Dqa(e)
    }
}

impl From<DecimalError> for DvecError {
    fn from(e: DecimalError) -> Self {
        DvecError::Decimal(e)
    }
}

// =============================================================================
// DvecScalar Trait
// =============================================================================

/// DVEC-compatible scalar trait (RFC-0112 original).
///
/// Implementors: `Dqa` (RFC-0105) and `Decimal` (RFC-0111).
///
/// **Not** the same as RFC-0113's `NumericScalar` — see module-level docstring
/// for the relationship and migration path.
pub trait DvecScalar: Clone {
    /// Associated error type for arithmetic operations.
    type Error: Into<DvecError> + std::fmt::Debug + PartialEq;

    /// Return the decimal scale.
    fn scale(&self) -> u8;

    /// Return the raw mantissa as i128.
    ///
    /// For DQA: sign-extend i64 → i128 (two's complement).
    /// For Decimal: return mantissa directly.
    fn raw_mantissa(&self) -> i128;

    fn mul(self, other: Self) -> Result<Self, Self::Error>;
    fn add(self, other: Self) -> Result<Self, Self::Error>;
    fn sub(self, other: Self) -> Result<Self, Self::Error>;
    fn div(self, other: Self) -> Result<Self, Self::Error>;

    /// Square root. Returns `Err(Unsupported)` for DQA (no SQRT per RFC-0105).
    fn sqrt(self) -> Result<Self, Self::Error>;

    fn is_zero(&self) -> bool;

    /// Construct a scalar from raw mantissa and scale.
    ///
    /// Used by DVEC operations (dot_product, squared_distance) to construct
    /// results from accumulated i128 values after overflow checking.
    ///
    /// For DQA: `mantissa` must fit in i64 (enforced by overflow check upstream).
    /// For Decimal: delegates to `Decimal::new(mantissa, scale)`.
    fn from_parts(mantissa: i128, scale: u8) -> Result<Self, Self::Error>;
}

// =============================================================================
// MaxScale Trait
// =============================================================================

/// Maximum scale for a DVEC scalar type.
/// DQA: 18 (per RFC-0105). Decimal: 36 (per RFC-0111).
pub trait MaxScale {
    const MAX_SCALE: u8;
}

impl MaxScale for Dqa {
    const MAX_SCALE: u8 = 18;
}

impl MaxScale for Decimal {
    const MAX_SCALE: u8 = 36;
}

// =============================================================================
// DVec Type
// =============================================================================

/// Deterministic Vector — generic over any `DvecScalar` type.
pub struct DVec<T: DvecScalar> {
    pub data: Vec<T>,
}

impl<T: DvecScalar> DVec<T> {
    /// Create a new DVEC from a Vec of scalars.
    pub fn new(data: Vec<T>) -> Self {
        Self { data }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// =============================================================================
// Validation Helpers
// =============================================================================

/// Validate that both vectors have the same length.
fn validate_len(a_len: usize, b_len: usize) -> Result<(), DvecError> {
    if a_len != b_len {
        Err(DvecError::DimensionMismatch)
    } else {
        Ok(())
    }
}

/// Validate that N <= 64.
fn validate_max_dim(n: usize) -> Result<(), DvecError> {
    if n > 64 {
        Err(DvecError::DimensionExceeded)
    } else {
        Ok(())
    }
}

/// Validate all elements in `a` and `b` share the same scale as `a[0]`.
/// Returns the common scale on success.
fn validate_uniform_scale<T: DvecScalar>(a: &[T], b: &[T]) -> Result<u8, DvecError> {
    let common_scale = a[0].scale();
    for elem in a.iter().skip(1) {
        if elem.scale() != common_scale {
            return Err(DvecError::ScaleMismatch);
        }
    }
    for elem in b.iter() {
        if elem.scale() != common_scale {
            return Err(DvecError::ScaleMismatch);
        }
    }
    Ok(common_scale)
}

// =============================================================================
// Implement DvecScalar for Dqa
// =============================================================================

impl DvecScalar for Dqa {
    type Error = DqaError;

    fn scale(&self) -> u8 {
        self.scale
    }

    fn raw_mantissa(&self) -> i128 {
        i128::from(self.value)
    }

    fn mul(self, other: Self) -> Result<Self, Self::Error> {
        dqa_mod::dqa_mul(self, other)
    }

    fn add(self, other: Self) -> Result<Self, Self::Error> {
        dqa_mod::dqa_add(self, other)
    }

    fn sub(self, other: Self) -> Result<Self, Self::Error> {
        dqa_mod::dqa_sub(self, other)
    }

    fn div(self, other: Self) -> Result<Self, Self::Error> {
        dqa_mod::dqa_div(self, other)
    }

    fn sqrt(self) -> Result<Self, Self::Error> {
        // DQA has no SQRT per RFC-0105.
        Err(DqaError::InvalidInput)
    }

    fn is_zero(&self) -> bool {
        self.value == 0
    }

    fn from_parts(mantissa: i128, scale: u8) -> Result<Self, Self::Error> {
        // The caller guarantees mantissa fits in i128, but Dqa stores i64.
        // Overflow should have been caught upstream, but we validate anyway.
        if mantissa > i64::MAX as i128 || mantissa < i64::MIN as i128 {
            return Err(DqaError::Overflow);
        }
        Dqa::new(mantissa as i64, scale)
    }
}

// =============================================================================
// Implement DvecScalar for Decimal
// =============================================================================

impl DvecScalar for Decimal {
    type Error = DecimalError;

    fn scale(&self) -> u8 {
        Decimal::scale(self)
    }

    fn raw_mantissa(&self) -> i128 {
        Decimal::mantissa(self)
    }

    fn mul(self, other: Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_mul(&self, &other)
    }

    fn add(self, other: Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_add(&self, &other)
    }

    fn sub(self, other: Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_sub(&self, &other)
    }

    fn div(self, other: Self) -> Result<Self, Self::Error> {
        decimal_mod::decimal_div(&self, &other, 0)
    }

    fn sqrt(self) -> Result<Self, Self::Error> {
        // dot_product returns a canonical Decimal; no raw variant needed.
        decimal_mod::decimal_sqrt(&self)
    }

    fn is_zero(&self) -> bool {
        Decimal::is_zero(self)
    }

    fn from_parts(mantissa: i128, scale: u8) -> Result<Self, Self::Error> {
        Decimal::new(mantissa, scale)
    }
}

// =============================================================================
// Operation: DOT_PRODUCT
// =============================================================================

/// Dot product of two vectors: Σ a[i] * b[i]
///
/// Algorithm (per RFC-0112 §DOT_PRODUCT):
/// 1. Input scale precondition (first check)
/// 2. Uniform scale validation
/// 3. Sequential i128 accumulation with overflow detection
/// 4. result_scale = input_scale * 2
/// 5. Construct result via T::from_parts
pub fn dot_product<T: DvecScalar + MaxScale>(a: &[T], b: &[T]) -> Result<T, DvecError>
where
    T::Error: Into<DvecError>,
{
    let n = a.len();
    validate_len(n, b.len())?;
    validate_max_dim(n)?;

    if n == 0 {
        return Err(DvecError::DimensionMismatch);
    }

    // Step 1: Input scale precondition (must be first check per RFC)
    let input_scale = a[0].scale();
    // DQA: input_scale <= 9; Decimal: input_scale <= 18
    let input_scale_max = if T::MAX_SCALE == 18 { 9 } else { 18 };
    if input_scale > input_scale_max {
        return Err(DvecError::InputScaleExceeded);
    }

    // Step 2: Validate uniform scale
    validate_uniform_scale(a, b)?;

    // Step 3: Sequential i128 accumulation with overflow detection
    // Per the RFC, each product fits in i128 given the scale constraints:
    // - DQA (scale ≤ 9): max |product| ≈ (10^10)^2 = 10^20, sum of 64 ≈ 10^21 << i128::MAX
    // - Decimal (scale ≤ 18): max |product| ≈ (10^18)^2 = 10^36, sum of 64 ≈ 10^37 << i128::MAX
    // But we still check to be safe and to TRAP deterministically.
    let mut acc: i128 = 0;
    for i in 0..n {
        let a_mant = a[i].raw_mantissa();
        let b_mant = b[i].raw_mantissa();
        // Check overflow: |acc + a_mant * b_mant| > i128::MAX
        let prod = a_mant.checked_mul(b_mant).ok_or(DvecError::Overflow)?;
        acc = acc.checked_add(prod).ok_or(DvecError::Overflow)?;
    }

    // Step 4: Result scale = a_scale + b_scale (both vectors have same scale)
    let result_scale = input_scale * 2;

    // Step 5: Construct result
    T::from_parts(acc, result_scale).map_err(|e| e.into())
}

impl<T: DvecScalar + MaxScale> DVec<T>
where
    T::Error: Into<DvecError>,
{
    /// Dot product of two DVECs.
    pub fn dot_product(&self, other: &Self) -> Result<T, DvecError> {
        dot_product(&self.data, &other.data)
    }
}

// =============================================================================
// Remaining operations (stubs — fill in arithmetic mission)
// =============================================================================

/// Squared Euclidean distance: Σ (a[i] - b[i])²
///
/// Algorithm (per RFC-0112 §SQUARED_DISTANCE):
/// 1. Input scale precondition (first check)
/// 2. Uniform scale validation
/// 3. Sequential i128 accumulation of squared differences
/// 4. result_scale = input_scale * 2
/// 5. Construct result via T::from_parts
pub fn squared_distance<T: DvecScalar + MaxScale>(a: &[T], b: &[T]) -> Result<T, DvecError>
where
    T::Error: Into<DvecError>,
{
    let n = a.len();
    validate_len(n, b.len())?;
    validate_max_dim(n)?;

    if n == 0 {
        return Err(DvecError::DimensionMismatch);
    }

    // Step 1: Input scale precondition (must be first check per RFC)
    let input_scale = a[0].scale();
    let input_scale_max = if T::MAX_SCALE == 18 { 9 } else { 18 };
    if input_scale > input_scale_max {
        return Err(DvecError::InputScaleExceeded);
    }

    // Step 2: Validate uniform scale
    validate_uniform_scale(a, b)?;

    // Step 3: Sequential i128 accumulation of squared differences
    let mut acc: i128 = 0;
    for i in 0..n {
        let a_mant = a[i].raw_mantissa();
        let b_mant = b[i].raw_mantissa();
        let diff = a_mant - b_mant;
        let sq = diff.checked_mul(diff).ok_or(DvecError::Overflow)?;
        acc = acc.checked_add(sq).ok_or(DvecError::Overflow)?;
    }

    // Step 4: Result scale = input_scale * 2
    let result_scale = input_scale * 2;

    // Step 5: Construct result
    T::from_parts(acc, result_scale).map_err(|e| e.into())
}

/// L2 norm: sqrt(Σ a[i]²)
///
/// Algorithm (per RFC-0112 §NORM):
/// 1. Input scale precondition: a[0].scale <= 9 (for SQRT precision)
/// 2. Uniform scale validation
/// 3. dot_product(a, a) → squared_sum
/// 4. squared_sum.sqrt() → norm
///
/// DQA: TRAPs with Unsupported at step 4 (no SQRT per RFC-0105).
/// Decimal: scale precondition enforced at step 1.
/// Zero vector: returns zero (not an error — sqrt(0) = 0 is well-defined).
pub fn norm<T: DvecScalar + MaxScale>(a: &[T]) -> Result<T, DvecError>
where
    T::Error: Into<DvecError>,
{
    let n = a.len();
    if n == 0 {
        return Err(DvecError::DimensionMismatch);
    }
    validate_max_dim(n)?;

    // Step 1: Input scale precondition (must be FIRST per RFC)
    let input_scale = a[0].scale();
    // For SQRT precision: Decimal scale <= 9, DQA scale <= 9 (but DQA fails at sqrt anyway)
    if input_scale > 9 {
        return Err(DvecError::InputScaleExceeded);
    }

    // Step 2: Validate uniform scale
    for elem in a.iter().skip(1) {
        if elem.scale() != input_scale {
            return Err(DvecError::ScaleMismatch);
        }
    }

    // Step 3: dot_product(a, a) = Σ a[i]²
    let squared_sum = dot_product(a, a)?;

    // Step 4: sqrt(squared_sum) — DQA TRAPs with Unsupported (no SQRT per RFC-0105)
    squared_sum.sqrt().map_err(|e| {
        let err: DvecError = e.into();
        match err {
            DvecError::Dqa(DqaError::InvalidInput) => DvecError::Unsupported,
            _ => err,
        }
    })
}

/// Normalize vector: [a[i] / norm(a)] for each element.
///
/// FORBIDDEN in consensus (exceeds 50k gas budget).
/// This function TRAPs with ConsensusRestriction to enforce this at the API level.
/// Off-chain callers should use the gas-metered path instead.
pub fn normalize<T: DvecScalar>(a: &[T]) -> Result<Vec<T>, DvecError>
where
    T::Error: Into<DvecError>,
{
    let _ = a;
    Err(DvecError::ConsensusRestriction)
}

/// Element-wise addition.
pub fn vec_add<T: DvecScalar>(a: &[T], b: &[T]) -> Result<Vec<T>, DvecError>
where
    T::Error: Into<DvecError>,
{
    validate_len(a.len(), b.len())?;
    validate_uniform_scale(a, b)?;
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.clone().add(y.clone()).map_err(|e| e.into()))
        .collect()
}

/// Element-wise subtraction.
pub fn vec_sub<T: DvecScalar>(a: &[T], b: &[T]) -> Result<Vec<T>, DvecError>
where
    T::Error: Into<DvecError>,
{
    validate_len(a.len(), b.len())?;
    validate_uniform_scale(a, b)?;
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.clone().sub(y.clone()).map_err(|e| e.into()))
        .collect()
}

/// Element-wise multiplication.
pub fn vec_mul<T: DvecScalar>(a: &[T], b: &[T]) -> Result<Vec<T>, DvecError>
where
    T::Error: Into<DvecError>,
{
    validate_len(a.len(), b.len())?;
    validate_uniform_scale(a, b)?;
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.clone().mul(y.clone()).map_err(|e| e.into()))
        .collect()
}

/// Scale: multiply all elements by a scalar.
pub fn vec_scale<T: DvecScalar>(a: &[T], scalar: T) -> Result<Vec<T>, DvecError>
where
    T::Error: Into<DvecError>,
{
    a.iter()
        .map(|x| x.clone().mul(scalar.clone()).map_err(|e| e.into()))
        .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // DFP does NOT implement DvecScalar — verify DVec<Dfp> doesn't compile.
    // This is a compile-time guarantee via the type system.

    #[test]
    fn test_dvec_scalar_impl_dqa() {
        let dqa = Dqa::new(123, 2).unwrap();
        assert_eq!(dqa.scale(), 2);
        assert_eq!(dqa.raw_mantissa(), 123);
        assert!(!dqa.is_zero());
    }

    #[test]
    fn test_dvec_scalar_impl_decimal() {
        let dec = Decimal::new(12345, 3).unwrap();
        assert_eq!(Decimal::scale(&dec), 3); // scale not canonicalized here
        assert_eq!(Decimal::mantissa(&dec), 12345);
        assert!(!Decimal::is_zero(&dec));
    }

    #[test]
    fn test_vec_add_basic() {
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let result = vec_add(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), 4);
        assert_eq!(result[1].raw_mantissa(), 6);
    }

    #[test]
    fn test_vec_add_scale_mismatch() {
        let a = vec![Dqa::new(1, 0).unwrap()];
        let b = vec![Dqa::new(1, 1).unwrap()]; // different scale
        let result = vec_add(&a, &b);
        assert_eq!(result, Err(DvecError::ScaleMismatch));
    }

    #[test]
    fn test_vec_add_dimension_mismatch() {
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(3, 0).unwrap()];
        let result = vec_add(&a, &b);
        assert_eq!(result, Err(DvecError::DimensionMismatch));
    }

    #[test]
    fn test_dot_product_basic() {
        // [1, 2] · [3, 4] = 1*3 + 2*4 = 11
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let result = dot_product(&a, &b).unwrap();
        assert_eq!(result.raw_mantissa(), 11);
        assert_eq!(result.scale(), 0);
    }

    #[test]
    fn test_dot_product_scale_2() {
        // [1.0, 2.0] · [3.0, 4.0] with scale=1 -> [10, 20] · [30, 40] = 10*30 + 20*40 = 300 + 800 = 1100, scale=2 -> 11.00
        let a = vec![Dqa::new(10, 1).unwrap(), Dqa::new(20, 1).unwrap()];
        let b = vec![Dqa::new(30, 1).unwrap(), Dqa::new(40, 1).unwrap()];
        let result = dot_product(&a, &b).unwrap();
        assert_eq!(result.raw_mantissa(), 1100);
        assert_eq!(result.scale(), 2);
    }

    #[test]
    fn test_squared_distance_basic() {
        // [3, 4] vs [0, 0] -> 3² + 4² = 9 + 16 = 25
        let a = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let b = vec![Dqa::new(0, 0).unwrap(), Dqa::new(0, 0).unwrap()];
        let result = squared_distance(&a, &b).unwrap();
        assert_eq!(result.raw_mantissa(), 25);
        assert_eq!(result.scale(), 0);
    }

    #[test]
    fn test_squared_distance_same_vector() {
        // [1, 2] vs [1, 2] -> 0
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let result = squared_distance(&a, &b).unwrap();
        assert_eq!(result.raw_mantissa(), 0);
        assert_eq!(result.scale(), 0);
    }

    #[test]
    fn test_norm_dqa_returns_unsupported() {
        let a = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let result = norm(&a);
        assert_eq!(result, Err(DvecError::Unsupported));
    }

    #[test]
    fn test_normalize_returns_consensus_restriction() {
        let a = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let result = normalize::<Dqa>(&a);
        assert_eq!(result, Err(DvecError::ConsensusRestriction));
    }

    #[test]
    fn test_dvec_new_and_len() {
        let dvec: DVec<Dqa> = DVec::new(vec![Dqa::new(1, 0).unwrap()]);
        assert_eq!(dvec.len(), 1);
        assert!(!dvec.is_empty());
    }
}
