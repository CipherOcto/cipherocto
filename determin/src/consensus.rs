//! DVEC, DMAT, and DACT Consensus Integration Layer
//!
//! This module provides gas accounting and consensus-related constants
//! for DVEC operations per RFC-0112, DMAT operations per RFC-0113,
//! and DACT operations per RFC-0114.
//!
//! ## Gas Model (RFC-0112 §Gas Model) - DVEC
//!
//! | Operation | Formula | Max (N=64, scale=9) |
//! |-----------|---------|---------------------|
//! | DOT_PRODUCT | N × (30 + 3 × scale²) | 17,472 |
//! | SQUARED_DISTANCE | N × (30 + 3 × scale²) + 10 | 17,482 |
//! | NORM | DOT_PRODUCT + 280 (GAS_SQRT) | ~17,752 |
//! | VEC_ADD/SUB/MUL/SCALE | 5 × N | 320 |
//! | NORMALIZE | FORBIDDEN | - |
//!
//! ## Gas Model (RFC-0113 §Gas Model) - DMAT
//!
//! | Operation | Formula |
//! |-----------|---------|
//! | MAT_ADD/SUB | 10 × M × N |
//! | MAT_MUL | M × N × K × (30 + 3 × s_a × s_b) |
//! | MAT_VEC_MUL | rows × cols × (30 + 3 × s_a × s_v) |
//! | MAT_TRANSPOSE | 2 × M × N |
//! | MAT_SCALE | M × N × (20 + 3 × s_a × s_scalar) |
//!
//! ## Gas Model (RFC-0114 §Gas Model) - DACT
//!
//! | Operation | Gas |
//! |-----------|-----|
//! | ReLU | 2 |
//! | ReLU6 | 3 |
//! | LeakyReLU | 3 |
//! | Sigmoid | 10 |
//! | Tanh | 10 |
//!
//! ## Consensus Restrictions
//!
//! - NORMALIZE is FORBIDDEN in consensus (returns TRAP with ConsensusRestriction)
//! - DVEC<DFP> and DMAT<DFP> are FORBIDDEN (type system prevents this)
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
// DMAT Operation IDs (RFC-0113)
// =============================================================================

/// DMAT Operation IDs (must match probe.rs DMAT_OP_* constants)
pub mod dmat_op_ids {
    /// Matrix addition
    pub const MAT_ADD: u64 = 0x0100;
    /// Matrix subtraction
    pub const MAT_SUB: u64 = 0x0101;
    /// Matrix multiplication
    pub const MAT_MUL: u64 = 0x0102;
    /// Matrix-vector multiplication
    pub const MAT_VEC_MUL: u64 = 0x0103;
    /// Matrix transpose
    pub const MAT_TRANSPOSE: u64 = 0x0104;
    /// Matrix scalar multiplication
    pub const MAT_SCALE: u64 = 0x0105;
}

// =============================================================================
// DACT Operation IDs (RFC-0114)
// =============================================================================

/// DACT Operation IDs (must match probe.rs DACT_OP_* constants)
pub mod dact_op_ids {
    /// ReLU activation
    pub const RELU: u64 = 0x0200;
    /// ReLU6 activation
    pub const RELU6: u64 = 0x0201;
    /// LeakyReLU activation
    pub const LEAKY_RELU: u64 = 0x0202;
    /// Sigmoid activation (LUT-based)
    pub const SIGMOID: u64 = 0x0203;
    /// Tanh activation (LUT-based)
    pub const TANH: u64 = 0x0204;
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

/// Maximum dimension for DMAT operations (M×N ≤ 64, M≤8, N≤8)
pub const MAX_MATRIX_DIMENSION: usize = 8;

/// Maximum matrix elements (M×N)
pub const MAX_MATRIX_ELEMENTS: usize = 64;

/// Base gas for MAT_ADD and MAT_SUB
pub const GAS_MAT_ADD_PER_ELEMENT: u64 = 10;

/// Base gas for MAT_SCALE
pub const GAS_MAT_SCALE_BASE: u64 = 20;

/// Base gas for MAT_MUL and MAT_VEC_MUL
pub const GAS_MAT_MUL_BASE: u64 = 30;

// =============================================================================
// DACT Gas Constants (RFC-0114)
// =============================================================================

/// Gas for ReLU activation
pub const GAS_RELU: u64 = 2;

/// Gas for ReLU6 activation
pub const GAS_RELU6: u64 = 3;

/// Gas for LeakyReLU activation
pub const GAS_LEAKY_RELU: u64 = 3;

/// Gas for Sigmoid activation (LUT-based)
pub const GAS_SIGMOID: u64 = 10;

/// Gas for Tanh activation (LUT-based)
pub const GAS_TANH: u64 = 10;

/// Maximum gas for any single DACT operation (Sigmoid/Tanh)
pub const MAX_DACT_GAS: u64 = 10;

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
    assert!(
        n <= MAX_DIMENSION,
        "Dimension {} exceeds maximum {}",
        n,
        MAX_DIMENSION
    );
    assert!(scale <= 18, "Scale {} exceeds maximum 18", scale);

    let n = n as u64;
    let scale = scale as u64;
    n * (GAS_BASE + GAS_SCALE_FACTOR * scale * scale)
}

/// Calculate gas for SQUARED_DISTANCE operation.
///
/// Formula: N × (30 + 3 × scale²) + 10
pub fn gas_squared_distance(n: usize, scale: u8) -> u64 {
    assert!(
        n <= MAX_DIMENSION,
        "Dimension {} exceeds maximum {}",
        n,
        MAX_DIMENSION
    );
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
    assert!(
        n <= MAX_DIMENSION,
        "Dimension {} exceeds maximum {}",
        n,
        MAX_DIMENSION
    );
    assert!(scale <= 18, "Scale {} exceeds maximum 18", scale);

    gas_dot_product(n, scale) + GAS_SQRT_MAX
}

/// Calculate gas for element-wise operations (VEC_ADD, VEC_SUB, VEC_MUL, VEC_SCALE).
///
/// Formula: 5 × N
pub fn gas_element_wise(n: usize) -> u64 {
    assert!(
        n <= MAX_DIMENSION,
        "Dimension {} exceeds maximum {}",
        n,
        MAX_DIMENSION
    );

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

// =============================================================================
// DMAT Gas Calculation Functions (RFC-0113)
// =============================================================================

/// Calculate gas for MAT_ADD and MAT_SUB operations.
///
/// Formula: 10 × M × N
pub fn gas_mat_add_sub(rows: usize, cols: usize) -> u64 {
    assert!(
        rows <= MAX_MATRIX_DIMENSION,
        "Rows {} exceeds maximum {}",
        rows,
        MAX_MATRIX_DIMENSION
    );
    assert!(
        cols <= MAX_MATRIX_DIMENSION,
        "Cols {} exceeds maximum {}",
        cols,
        MAX_MATRIX_DIMENSION
    );
    let elements = rows * cols;
    assert!(
        elements <= MAX_MATRIX_ELEMENTS,
        "Matrix elements {} exceeds maximum {}",
        elements,
        MAX_MATRIX_ELEMENTS
    );
    (elements as u64) * GAS_MAT_ADD_PER_ELEMENT
}

/// Calculate gas for MAT_TRANSPOSE operation.
///
/// Formula: 2 × M × N
pub fn gas_mat_transpose(rows: usize, cols: usize) -> u64 {
    assert!(
        rows <= MAX_MATRIX_DIMENSION,
        "Rows {} exceeds maximum {}",
        rows,
        MAX_MATRIX_DIMENSION
    );
    assert!(
        cols <= MAX_MATRIX_DIMENSION,
        "Cols {} exceeds maximum {}",
        cols,
        MAX_MATRIX_DIMENSION
    );
    let elements = rows * cols;
    assert!(
        elements <= MAX_MATRIX_ELEMENTS,
        "Matrix elements {} exceeds maximum {}",
        elements,
        MAX_MATRIX_ELEMENTS
    );
    2 * (elements as u64)
}

/// Calculate gas for MAT_SCALE operation.
///
/// Formula: M × N × (20 + 3 × s_a × s_scalar)
pub fn gas_mat_scale(rows: usize, cols: usize, scale_a: u8, scale_scalar: u8) -> u64 {
    assert!(
        rows <= MAX_MATRIX_DIMENSION,
        "Rows {} exceeds maximum {}",
        rows,
        MAX_MATRIX_DIMENSION
    );
    assert!(
        cols <= MAX_MATRIX_DIMENSION,
        "Cols {} exceeds maximum {}",
        cols,
        MAX_MATRIX_DIMENSION
    );
    let elements = rows * cols;
    assert!(
        elements <= MAX_MATRIX_ELEMENTS,
        "Matrix elements {} exceeds maximum {}",
        elements,
        MAX_MATRIX_ELEMENTS
    );
    let elements = elements as u64;
    let scale_a = scale_a as u64;
    let scale_scalar = scale_scalar as u64;
    elements * (GAS_MAT_SCALE_BASE + GAS_SCALE_FACTOR * scale_a * scale_scalar)
}

/// Calculate gas for MAT_MUL operation.
///
/// Formula: M × N × K × (30 + 3 × s_a × s_b)
///
/// # Arguments
/// * `m` - Rows of matrix A
/// * `n` - Columns of matrix A (also rows of matrix B)
/// * `k` - Columns of matrix B
/// * `scale_a` - Scale of matrix A
/// * `scale_b` - Scale of matrix B
pub fn gas_mat_mul(m: usize, n: usize, k: usize, scale_a: u8, scale_b: u8) -> u64 {
    assert!(
        m <= MAX_MATRIX_DIMENSION,
        "M {} exceeds maximum {}",
        m,
        MAX_MATRIX_DIMENSION
    );
    assert!(
        n <= MAX_MATRIX_DIMENSION,
        "N {} exceeds maximum {}",
        n,
        MAX_MATRIX_DIMENSION
    );
    assert!(
        k <= MAX_MATRIX_DIMENSION,
        "K {} exceeds maximum {}",
        k,
        MAX_MATRIX_DIMENSION
    );
    let result_elements = m * k;
    assert!(
        result_elements <= MAX_MATRIX_ELEMENTS,
        "Result elements {} exceeds maximum {}",
        result_elements,
        MAX_MATRIX_ELEMENTS
    );
    let m = m as u64;
    let n = n as u64;
    let k = k as u64;
    let scale_a = scale_a as u64;
    let scale_b = scale_b as u64;
    m * n * k * (GAS_MAT_MUL_BASE + GAS_SCALE_FACTOR * scale_a * scale_b)
}

/// Calculate gas for MAT_VEC_MUL operation.
///
/// Formula: rows × cols × (30 + 3 × s_a × s_v)
///
/// # Arguments
/// * `rows` - Matrix rows (also result vector length)
/// * `cols` - Matrix columns (must equal vector length)
/// * `scale_a` - Scale of matrix
/// * `scale_v` - Scale of vector
pub fn gas_mat_vec_mul(rows: usize, cols: usize, scale_a: u8, scale_v: u8) -> u64 {
    assert!(
        rows <= MAX_MATRIX_DIMENSION,
        "Rows {} exceeds maximum {}",
        rows,
        MAX_MATRIX_DIMENSION
    );
    assert!(
        cols <= MAX_MATRIX_DIMENSION,
        "Cols {} exceeds maximum {}",
        cols,
        MAX_MATRIX_DIMENSION
    );
    let elements = rows * cols;
    assert!(
        elements <= MAX_MATRIX_ELEMENTS,
        "Matrix elements {} exceeds maximum {}",
        elements,
        MAX_MATRIX_ELEMENTS
    );
    let elements = elements as u64;
    let scale_a = scale_a as u64;
    let scale_v = scale_v as u64;
    elements * (GAS_MAT_MUL_BASE + GAS_SCALE_FACTOR * scale_a * scale_v)
}

/// Check if an operation is a DMAT operation.
pub fn is_dmat_op(op_id: u64) -> bool {
    matches!(
        op_id,
        dmat_op_ids::MAT_ADD
            | dmat_op_ids::MAT_SUB
            | dmat_op_ids::MAT_MUL
            | dmat_op_ids::MAT_VEC_MUL
            | dmat_op_ids::MAT_TRANSPOSE
            | dmat_op_ids::MAT_SCALE
    )
}

/// Check if DMAT<DFP> is allowed in consensus (it is FORBIDDEN).
pub fn is_dmat_allowed_with_dfp() -> bool {
    false // DMAT<DFP> is FORBIDDEN
}

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

    // DMAT gas tests

    #[test]
    fn test_gas_mat_add_sub_max() {
        // 8x8 = 64 elements: 64 * 10 = 640
        assert_eq!(gas_mat_add_sub(8, 8), 640);
    }

    #[test]
    fn test_gas_mat_add_sub_2x2() {
        // 2x2 = 4 elements: 4 * 10 = 40
        assert_eq!(gas_mat_add_sub(2, 2), 40);
    }

    #[test]
    fn test_gas_mat_transpose() {
        // 8x8 = 64 elements: 2 * 64 = 128
        assert_eq!(gas_mat_transpose(8, 8), 128);
    }

    #[test]
    fn test_gas_mat_scale() {
        // 2x2 = 4 elements: 4 * (20 + 3 * 0 * 0) = 4 * 20 = 80
        assert_eq!(gas_mat_scale(2, 2, 0, 0), 80);
        // 2x2 = 4 elements: 4 * (20 + 3 * 3 * 5) = 4 * (20 + 45) = 4 * 65 = 260
        assert_eq!(gas_mat_scale(2, 2, 3, 5), 260);
    }

    #[test]
    fn test_gas_mat_mul() {
        // 2x2 * 2x2 = 2x2: 2 * 2 * 2 * 30 = 240 (no scale factor)
        assert_eq!(gas_mat_mul(2, 2, 2, 0, 0), 240);
        // 2x3 * 3x2 = 2x2: 2 * 3 * 2 * (30 + 3 * 0 * 0) = 12 * 30 = 360
        assert_eq!(gas_mat_mul(2, 3, 2, 0, 0), 360);
    }

    #[test]
    fn test_gas_mat_vec_mul() {
        // 2x3 matrix * 3-vector: 6 * (30 + 3 * 0 * 0) = 6 * 30 = 180
        assert_eq!(gas_mat_vec_mul(2, 3, 0, 0), 180);
    }

    #[test]
    fn test_is_dmat_op() {
        assert!(is_dmat_op(dmat_op_ids::MAT_ADD));
        assert!(is_dmat_op(dmat_op_ids::MAT_SUB));
        assert!(is_dmat_op(dmat_op_ids::MAT_MUL));
        assert!(is_dmat_op(dmat_op_ids::MAT_VEC_MUL));
        assert!(is_dmat_op(dmat_op_ids::MAT_TRANSPOSE));
        assert!(is_dmat_op(dmat_op_ids::MAT_SCALE));
        assert!(!is_dmat_op(999)); // Not a DMAT op
    }

    #[test]
    fn test_is_dmat_allowed_with_dfp() {
        assert!(!is_dmat_allowed_with_dfp()); // Always false
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_gas_mat_add_elements_exceeded() {
        gas_mat_add_sub(9, 8); // 72 elements > 64
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_gas_mat_mul_elements_exceeded() {
        gas_mat_mul(9, 8, 1, 0, 0); // 9*1 = 9, but 9 > 8 dimension
    }
}
