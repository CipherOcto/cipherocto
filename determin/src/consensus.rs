//! DVEC Consensus Integration Layer
//!
//! This module provides gas accounting and consensus-related constants
//! for DVEC operations per RFC-0112.
//!
//! ## Gas Model (RFC-0112 §Gas Model)
//!
//! | Operation | Formula | Max (N=64, scale=9) |
//! |-----------|---------|---------------------|
//! | DOT_PRODUCT | N × (30 + 3 × scale²) | 17,472 |
//! | SQUARED_DISTANCE | N × (30 + 3 × scale²) + 10 | 17,482 |
//! | NORM | DOT_PRODUCT + 280 (GAS_SQRT) | ~17,752 |
//! | VEC_ADD/SUB/MUL/SCALE | 5 × N | 320 |
//! | NORMALIZE | FORBIDDEN | - |
//!
//! ## Consensus Restrictions
//!
//! - NORMALIZE is FORBIDDEN in consensus (returns TRAP with ConsensusRestriction)
//! - DVEC<DFP> is FORBIDDEN (type system prevents this)
//! - N <= 64 enforced (DimensionExceeded beyond)
//! - Scale validation per operation (InputScaleExceeded)

// =============================================================================
// Operation IDs
// =============================================================================

/// DVEC Operation IDs (must match probe.rs DVEC_OP_* constants)
pub mod op_ids {
    /// Dot product of two vectors: Σ a[i] * b[i]
    pub const DOT_PRODUCT: u64 = 1;
    /// Squared Euclidean distance: Σ (a[i] - b[i])²
    pub const SQUARED_DISTANCE: u64 = 2;
    /// L2 norm: sqrt(Σ a[i]²)
    pub const NORM: u64 = 3;
    /// Element-wise addition
    pub const VEC_ADD: u64 = 4;
    /// Element-wise subtraction
    pub const VEC_SUB: u64 = 5;
    /// Element-wise multiplication
    pub const VEC_MUL: u64 = 6;
    /// Scalar multiplication
    pub const VEC_SCALE: u64 = 7;
    /// Vector normalization (FORBIDDEN in consensus)
    pub const NORMALIZE: u64 = 8;
}

// =============================================================================
// Gas Constants
// =============================================================================

/// Base gas cost for DOT_PRODUCT and SQUARED_DISTANCE
pub const GAS_BASE: u64 = 30;

/// Per-element gas multiplier for DOT_PRODUCT and SQUARED_DISTANCE
pub const GAS_SCALE_FACTOR: u64 = 3;

/// Additional gas for SQUARED_DISTANCE (sqrt of squared distance via NORM path)
pub const GAS_SQUARED_DISTANCE_OVERHEAD: u64 = 10;

/// Maximum gas for SQRT operation (Decimal NORM uses this)
pub const GAS_SQRT_MAX: u64 = 280;

/// Base gas for element-wise operations (VEC_ADD, VEC_SUB, VEC_MUL, VEC_SCALE)
pub const GAS_ELEMENT_WISE_PER_N: u64 = 5;

/// Maximum dimension for DVEC operations
pub const MAX_DIMENSION: usize = 64;

/// Maximum input scale for DQA (DOT_PRODUCT, SQUARED_DISTANCE)
pub const DQA_MAX_INPUT_SCALE: u8 = 9;

/// Maximum input scale for Decimal (DOT_PRODUCT, SQUARED_DISTANCE)
pub const DECIMAL_MAX_INPUT_SCALE: u8 = 18;

// =============================================================================
// Gas Calculation Functions
// =============================================================================

/// Calculate gas for DOT_PRODUCT operation.
///
/// Formula: N × (30 + 3 × scale²)
///
/// # Arguments
/// * `n` - Vector dimension (must be <= 64)
/// * `scale` - Input scale (must be <= 9 for DQA, <= 18 for Decimal)
///
/// # Panics
/// Panics if n > MAX_DIMENSION or scale > 18
pub fn gas_dot_product(n: usize, scale: u8) -> u64 {
    assert!(n <= MAX_DIMENSION, "Dimension {} exceeds maximum {}", n, MAX_DIMENSION);
    assert!(scale <= 18, "Scale {} exceeds maximum 18", scale);

    let n = n as u64;
    let scale = scale as u64;
    n * (GAS_BASE + GAS_SCALE_FACTOR * scale * scale)
}

/// Calculate gas for SQUARED_DISTANCE operation.
///
/// Formula: N × (30 + 3 × scale²) + 10
pub fn gas_squared_distance(n: usize, scale: u8) -> u64 {
    assert!(n <= MAX_DIMENSION, "Dimension {} exceeds maximum {}", n, MAX_DIMENSION);
    assert!(scale <= 18, "Scale {} exceeds maximum 18", scale);

    gas_dot_product(n, scale) + GAS_SQUARED_DISTANCE_OVERHEAD
}

/// Calculate gas for NORM operation.
///
/// Formula: DOT_PRODUCT gas + GAS_SQRT_MAX
///
/// Note: NORM for DQA returns Unsupported (DQA has no SQRT per RFC-0105).
/// Decimal NORM uses this gas calculation.
pub fn gas_norm(n: usize, scale: u8) -> u64 {
    assert!(n <= MAX_DIMENSION, "Dimension {} exceeds maximum {}", n, MAX_DIMENSION);
    assert!(scale <= 18, "Scale {} exceeds maximum 18", scale);

    gas_dot_product(n, scale) + GAS_SQRT_MAX
}

/// Calculate gas for element-wise operations (VEC_ADD, VEC_SUB, VEC_MUL, VEC_SCALE).
///
/// Formula: 5 × N
pub fn gas_element_wise(n: usize) -> u64 {
    assert!(n <= MAX_DIMENSION, "Dimension {} exceeds maximum {}", n, MAX_DIMENSION);

    (n as u64) * GAS_ELEMENT_WISE_PER_N
}

/// Calculate gas for NORMALIZE operation.
///
/// Returns None because NORMALIZE is FORBIDDEN in consensus.
/// Use this to check if an operation is allowed before dispatching.
pub fn gas_normalize(_n: usize) -> Option<u64> {
    None // FORBIDDEN
}

/// Check if an operation is allowed in consensus.
///
/// Returns true if the operation can be executed in consensus,
/// false if it returns ConsensusRestriction.
pub fn is_allowed_in_consensus(op_id: u64) -> bool {
    op_id != op_ids::NORMALIZE
}

/// Maximum gas for any single DVEC operation.
/// Calculated as: gas_norm(64, 9) = 64 * (30 + 3 * 81) + 280 = 17_472 + 280 = 17_752
pub const MAX_DVEC_GAS: u64 = 17_752;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_dot_product_max() {
        // N=64, scale=9: 64 * (30 + 3 * 81) = 64 * (30 + 243) = 64 * 273 = 17,472
        assert_eq!(gas_dot_product(64, 9), 17_472);
    }

    #[test]
    fn test_gas_dot_product_scale_zero() {
        // N=1, scale=0: 1 * (30 + 0) = 30
        assert_eq!(gas_dot_product(1, 0), 30);
    }

    #[test]
    fn test_gas_squared_distance_max() {
        // N=64, scale=9: 17,472 + 10 = 17,482
        assert_eq!(gas_squared_distance(64, 9), 17_482);
    }

    #[test]
    fn test_gas_norm_max() {
        // N=64, scale=9: 17,472 + 280 = 17,752
        assert_eq!(gas_norm(64, 9), 17_752);
    }

    #[test]
    fn test_gas_element_wise_max() {
        // N=64: 64 * 5 = 320
        assert_eq!(gas_element_wise(64), 320);
    }

    #[test]
    fn test_gas_normalize_forbidden() {
        assert!(gas_normalize(64).is_none());
    }

    #[test]
    fn test_is_allowed_in_consensus() {
        assert!(is_allowed_in_consensus(op_ids::DOT_PRODUCT));
        assert!(is_allowed_in_consensus(op_ids::SQUARED_DISTANCE));
        assert!(is_allowed_in_consensus(op_ids::NORM));
        assert!(is_allowed_in_consensus(op_ids::VEC_ADD));
        assert!(is_allowed_in_consensus(op_ids::VEC_SUB));
        assert!(is_allowed_in_consensus(op_ids::VEC_MUL));
        assert!(is_allowed_in_consensus(op_ids::VEC_SCALE));
        assert!(!is_allowed_in_consensus(op_ids::NORMALIZE));
    }

    #[test]
    fn test_max_dvec_gas() {
        assert_eq!(MAX_DVEC_GAS, 17_752);
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_gas_dot_product_dimension_exceeded() {
        gas_dot_product(65, 0);
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_gas_element_wise_dimension_exceeded() {
        gas_element_wise(65);
    }
}
