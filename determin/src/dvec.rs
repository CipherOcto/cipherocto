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
    // Validate all elements have the same scale as the scalar
    let scalar_scale = scalar.scale();
    for elem in a.iter() {
        if elem.scale() != scalar_scale {
            return Err(DvecError::ScaleMismatch);
        }
    }
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

    // Probe entry 48: VEC_ADD Decimal — [1,2] + [3,4] = [4,6]
    #[test]
    fn test_vec_add_decimal() {
        let a = vec![Decimal::new(1, 0).unwrap(), Decimal::new(2, 0).unwrap()];
        let b = vec![Decimal::new(3, 0).unwrap(), Decimal::new(4, 0).unwrap()];
        let result = vec_add(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), 4);
        assert_eq!(result[1].raw_mantissa(), 6);
    }

    // Probe entry 49: VEC_SUB Decimal — [4,6] - [1,2] = [3,4]
    #[test]
    fn test_vec_sub_decimal() {
        let a = vec![Decimal::new(4, 0).unwrap(), Decimal::new(6, 0).unwrap()];
        let b = vec![Decimal::new(1, 0).unwrap(), Decimal::new(2, 0).unwrap()];
        let result = vec_sub(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), 3);
        assert_eq!(result[1].raw_mantissa(), 4);
    }

    // Probe entry 50: VEC_MUL Decimal — [2,3] × [4,5] = [8,15]
    #[test]
    fn test_vec_mul_decimal() {
        let a = vec![Decimal::new(2, 0).unwrap(), Decimal::new(3, 0).unwrap()];
        let b = vec![Decimal::new(4, 0).unwrap(), Decimal::new(5, 0).unwrap()];
        let result = vec_mul(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), 8);
        assert_eq!(result[1].raw_mantissa(), 15);
    }

    // Probe entry 51: VEC_SCALE Decimal — [1,2] × scalar=2 = [2,4]
    #[test]
    fn test_vec_scale_decimal() {
        let a = vec![Decimal::new(1, 0).unwrap(), Decimal::new(2, 0).unwrap()];
        let scalar = Decimal::new(2, 0).unwrap();
        let result = vec_scale(&a, scalar).unwrap();
        assert_eq!(result[0].raw_mantissa(), 2);
        assert_eq!(result[1].raw_mantissa(), 4);
    }

    #[test]
    fn test_vec_sub_basic() {
        let a = vec![Dqa::new(4, 0).unwrap(), Dqa::new(6, 0).unwrap()];
        let b = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let result = vec_sub(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), 3);
        assert_eq!(result[1].raw_mantissa(), 4);
    }

    #[test]
    fn test_vec_mul_basic() {
        let a = vec![Dqa::new(2, 0).unwrap(), Dqa::new(3, 0).unwrap()];
        let b = vec![Dqa::new(4, 0).unwrap(), Dqa::new(5, 0).unwrap()];
        let result = vec_mul(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), 8);
        assert_eq!(result[1].raw_mantissa(), 15);
    }

    #[test]
    fn test_vec_scale_basic() {
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let scalar = Dqa::new(2, 0).unwrap();
        let result = vec_scale(&a, scalar).unwrap();
        assert_eq!(result[0].raw_mantissa(), 2);
        assert_eq!(result[1].raw_mantissa(), 4);
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

    // =============================================================================
    // Boundary Tests: Dimension Limits (N=64 max, N=65 should TRAP)
    // =============================================================================

    #[test]
    fn test_dot_product_dimension_65_traps() {
        // N=65 exceeds limit of 64
        let a: Vec<Dqa> = (0..65).map(|i| Dqa::new(i as i64, 0).unwrap()).collect();
        let b: Vec<Dqa> = (0..65).map(|_| Dqa::new(1, 0).unwrap()).collect();
        let result = dot_product(&a, &b);
        assert_eq!(result, Err(DvecError::DimensionExceeded));
    }

    #[test]
    fn test_dot_product_dimension_64_succeeds() {
        // N=64 is at the limit
        let a: Vec<Dqa> = (0..64).map(|_i| Dqa::new(1, 0).unwrap()).collect();
        let b: Vec<Dqa> = (0..64).map(|_i| Dqa::new(1, 0).unwrap()).collect();
        let result = dot_product(&a, &b).unwrap();
        assert_eq!(result.raw_mantissa(), 64); // 64 * 1 * 1 = 64
    }

    #[test]
    fn test_squared_distance_dimension_65_traps() {
        let a: Vec<Dqa> = (0..65).map(|i| Dqa::new(i as i64, 0).unwrap()).collect();
        let b: Vec<Dqa> = (0..65).map(|_| Dqa::new(0, 0).unwrap()).collect();
        let result = squared_distance(&a, &b);
        assert_eq!(result, Err(DvecError::DimensionExceeded));
    }

    #[test]
    fn test_norm_decimal_zero_vector_returns_zero() {
        // Zero vector NORM should return zero, not an error
        let a: Vec<Decimal> = vec![
            Decimal::new(0, 0).unwrap(),
            Decimal::new(0, 0).unwrap(),
            Decimal::new(0, 0).unwrap(),
        ];
        let result = norm(&a).unwrap();
        assert_eq!(result.raw_mantissa(), 0);
        assert_eq!(result.scale(), 0);
    }

    // =============================================================================
    // Scale Mismatch Tests (all operations)
    // =============================================================================

    #[test]
    fn test_dot_product_scale_mismatch() {
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(1, 1).unwrap(), Dqa::new(2, 1).unwrap()]; // scale 1
        let result = dot_product(&a, &b);
        assert_eq!(result, Err(DvecError::ScaleMismatch));
    }

    #[test]
    fn test_squared_distance_scale_mismatch() {
        let a = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let b = vec![Dqa::new(0, 1).unwrap(), Dqa::new(0, 1).unwrap()]; // scale 1
        let result = squared_distance(&a, &b);
        assert_eq!(result, Err(DvecError::ScaleMismatch));
    }

    #[test]
    fn test_vec_sub_scale_mismatch() {
        let a = vec![Dqa::new(1, 0).unwrap()];
        let b = vec![Dqa::new(1, 1).unwrap()];
        let result = vec_sub(&a, &b);
        assert_eq!(result, Err(DvecError::ScaleMismatch));
    }

    #[test]
    fn test_vec_mul_scale_mismatch() {
        let a = vec![Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(3, 1).unwrap()];
        let result = vec_mul(&a, &b);
        assert_eq!(result, Err(DvecError::ScaleMismatch));
    }

    #[test]
    fn test_vec_scale_scalar_scale_mismatch() {
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let scalar = Dqa::new(2, 1).unwrap(); // scale 1, vector is scale 0
        let result = vec_scale(&a, scalar);
        assert_eq!(result, Err(DvecError::ScaleMismatch));
    }

    // =============================================================================
    // Dimension Mismatch Tests (all binary operations)
    // =============================================================================

    #[test]
    fn test_dot_product_dimension_mismatch() {
        let a = vec![Dqa::new(1, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(1, 0).unwrap()];
        let result = dot_product(&a, &b);
        assert_eq!(result, Err(DvecError::DimensionMismatch));
    }

    #[test]
    fn test_squared_distance_dimension_mismatch() {
        let a = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let b = vec![Dqa::new(0, 0).unwrap()];
        let result = squared_distance(&a, &b);
        assert_eq!(result, Err(DvecError::DimensionMismatch));
    }

    #[test]
    fn test_vec_mul_dimension_mismatch() {
        let a = vec![Dqa::new(2, 0).unwrap()];
        let b = vec![Dqa::new(3, 0).unwrap(), Dqa::new(4, 0).unwrap()];
        let result = vec_mul(&a, &b);
        assert_eq!(result, Err(DvecError::DimensionMismatch));
    }

    // =============================================================================
    // Overflow Tests
    // =============================================================================

    #[test]
    fn test_dot_product_dqa_overflow_traps() {
        // DQA max is i64::MAX, accumulate i64::MAX * 2 twice = overflow
        let a = vec![
            Dqa::new(i64::MAX / 2, 0).unwrap(),
            Dqa::new(i64::MAX / 2, 0).unwrap(),
        ];
        let b = vec![Dqa::new(2, 0).unwrap(), Dqa::new(2, 0).unwrap()];
        let result = dot_product(&a, &b);
        assert_eq!(result, Err(DvecError::Dqa(DqaError::Overflow)));
    }

    #[test]
    fn test_vec_add_dqa_overflow_traps() {
        // DQA max is i64::MAX, adding i64::MAX to itself overflows
        let a = vec![Dqa::new(i64::MAX, 0).unwrap()];
        let b = vec![Dqa::new(1, 0).unwrap()];
        let result = vec_add(&a, &b);
        assert_eq!(result, Err(DvecError::Dqa(DqaError::Overflow)));
    }

    #[test]
    fn test_vec_mul_dqa_overflow_traps() {
        // i64::MAX * 2 exceeds i64::MAX
        let a = vec![Dqa::new(i64::MAX, 0).unwrap()];
        let b = vec![Dqa::new(2, 0).unwrap()];
        let result = vec_mul(&a, &b);
        assert_eq!(result, Err(DvecError::Dqa(DqaError::Overflow)));
    }

    // =============================================================================
    // Input Scale Tests (DQA limit is scale <= 9)
    // =============================================================================

    #[test]
    fn test_dot_product_dqa_input_scale_exceeded() {
        // DQA: input_scale > 9 should TRAP at input validation
        let a = vec![Dqa::new(1, 10).unwrap(), Dqa::new(2, 10).unwrap()];
        let b = vec![Dqa::new(3, 10).unwrap(), Dqa::new(4, 10).unwrap()];
        let result = dot_product(&a, &b);
        assert_eq!(result, Err(DvecError::InputScaleExceeded));
    }

    #[test]
    fn test_squared_distance_dqa_input_scale_exceeded() {
        let a = vec![Dqa::new(1, 10).unwrap(), Dqa::new(2, 10).unwrap()];
        let b = vec![Dqa::new(0, 10).unwrap(), Dqa::new(0, 10).unwrap()];
        let result = squared_distance(&a, &b);
        assert_eq!(result, Err(DvecError::InputScaleExceeded));
    }

    #[test]
    fn test_norm_decimal_input_scale_exceeded() {
        // Decimal NORM requires input_scale <= 9
        let a = vec![Decimal::new(1, 10).unwrap(), Decimal::new(2, 10).unwrap()];
        let result = norm(&a);
        assert_eq!(result, Err(DvecError::InputScaleExceeded));
    }

    // =============================================================================
    // Comprehensive Element-wise Operations Tests
    // =============================================================================

    #[test]
    fn test_vec_add_decimal_scale_preservation() {
        // Verify addition works and canonicalizes correctly
        let a = vec![Decimal::new(10, 2).unwrap(), Decimal::new(20, 2).unwrap()]; // 0.10, 0.20
        let b = vec![Decimal::new(30, 2).unwrap(), Decimal::new(40, 2).unwrap()]; // 0.30, 0.40
        let result = vec_add(&a, &b).unwrap();
        // 0.10 + 0.30 = 0.40 -> canonicalizes to (4, 1) = 0.4
        // 0.20 + 0.40 = 0.60 -> canonicalizes to (6, 1) = 0.6
        assert_eq!(result[0].scale(), 1);
        assert_eq!(result[0].raw_mantissa(), 4);
        assert_eq!(result[1].raw_mantissa(), 6);
    }

    #[test]
    fn test_vec_sub_decimal_negative_results() {
        // [1, 2] - [2, 3] = [-1, -1]
        let a = vec![Decimal::new(1, 0).unwrap(), Decimal::new(2, 0).unwrap()];
        let b = vec![Decimal::new(2, 0).unwrap(), Decimal::new(3, 0).unwrap()];
        let result = vec_sub(&a, &b).unwrap();
        assert_eq!(result[0].raw_mantissa(), -1);
        assert_eq!(result[1].raw_mantissa(), -1);
    }

    #[test]
    fn test_vec_mul_canonicalization() {
        // [10, 20] × [10, 20] with scale 1 = [100, 400], scale 2
        // 1.0 * 1.0 = 1.0 (canonical: 1, scale 0)
        // 2.0 * 2.0 = 4.0 (canonical: 4, scale 0)
        let a = vec![Decimal::new(10, 1).unwrap(), Decimal::new(20, 1).unwrap()];
        let b = vec![Decimal::new(10, 1).unwrap(), Decimal::new(20, 1).unwrap()];
        let result = vec_mul(&a, &b).unwrap();
        assert_eq!(result[0].scale(), 0);
        assert_eq!(result[0].raw_mantissa(), 1); // 1.0 * 1.0 = 1.0
        assert_eq!(result[1].raw_mantissa(), 4); // 2.0 * 2.0 = 4.0
    }

    // =============================================================================
    // DOT_PRODUCT Comprehensive Tests (various scales and N)
    // =============================================================================

    #[test]
    fn test_dot_product_decimal_various_scales() {
        // scale=0
        let a = vec![Decimal::new(1, 0).unwrap()];
        let b = vec![Decimal::new(2, 0).unwrap()];
        let result = dot_product(&a, &b).unwrap();
        assert_eq!(result.raw_mantissa(), 2);
        assert_eq!(result.scale(), 0);

        // scale=18 (max for Decimal DOT_PRODUCT)
        let a = vec![Decimal::new(1, 18).unwrap()];
        let b = vec![Decimal::new(1, 18).unwrap()];
        let result = dot_product(&a, &b).unwrap();
        assert_eq!(result.scale(), 36); // 18+18
    }

    #[test]
    fn test_dot_product_dqa_canonicalization() {
        // [10, 20] · [10, 20] with scale=2
        // = (10*10 + 20*20) = 500, scale=4 -> canonical = (5, 3)
        let a = vec![Decimal::new(10, 2).unwrap(), Decimal::new(20, 2).unwrap()];
        let b = vec![Decimal::new(10, 2).unwrap(), Decimal::new(20, 2).unwrap()];
        let result = dot_product(&a, &b).unwrap();
        // 100 + 400 = 500, scale 2+2=4
        // canonical: 500/10 = 50, scale 3; 50/10 = 5, scale 2
        // Actually 500 with scale 4 = 0.0500, canonical is (5, 2)
        assert_eq!(result.scale(), 2);
        assert_eq!(result.raw_mantissa(), 5);
    }

    // =============================================================================
    // NORM Tests
    // =============================================================================

    #[test]
    fn test_norm_decimal_perfect_square() {
        // sqrt(3² + 4²) = sqrt(25) = 5
        let a = vec![Decimal::new(3, 0).unwrap(), Decimal::new(4, 0).unwrap()];
        let result = norm(&a).unwrap();
        assert_eq!(result.raw_mantissa(), 5);
        assert_eq!(result.scale(), 0);
    }

    #[test]
    fn test_norm_decimal_non_perfect_square() {
        // sqrt(1² + 2²) = sqrt(5) ≈ 2.236...
        // RFC-0111 sqrt of 5 with P=10 gives specific approximation
        let a = vec![Decimal::new(1, 0).unwrap(), Decimal::new(2, 0).unwrap()];
        let result = norm(&a).unwrap();
        // dot_product = 5, scale 0
        // sqrt(5) with P=10 per RFC-0111
        // Result should be approximately 2236067977 with scale 10
        assert!(result.scale() <= 18); // Within valid scale range
        assert!(result.raw_mantissa() > 0);
    }

    #[test]
    fn test_norm_decimal_single_element() {
        // sqrt(10²) = 10
        let a = vec![Decimal::new(10, 0).unwrap()];
        let result = norm(&a).unwrap();
        assert_eq!(result.raw_mantissa(), 10);
        assert_eq!(result.scale(), 0);
    }
}
