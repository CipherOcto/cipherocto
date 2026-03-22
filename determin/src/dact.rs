//! Deterministic Activation Functions (DACT) Implementation
//!
//! This module implements RFC-0114: Deterministic Activation Functions
//! for the CipherOcto protocol.
//!
//! Activation functions are:
//! - ReLU: exact, no LUT needed
//! - ReLU6: exact, uses DQA comparison
//! - LeakyReLU: exact, uses DQA multiply
//! - Sigmoid: LUT-based with Q8.8→DQA conversion
//! - Tanh: LUT-based with Q8.8→DQA conversion

use crate::Dqa;

/// DACT error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DactError {
    /// TRAP sentinel detected
    TrapInput,
    /// Invalid scale (must be 0-18)
    InvalidScale,
    /// Integer overflow during arithmetic
    Overflow,
}

/// Check if a Dqa is the TRAP sentinel
#[inline]
fn is_trap(dqa: Dqa) -> bool {
    dqa.value == i64::MIN && dqa.scale == 0xFF
}

/// Check scale is valid (0-18)
#[inline]
fn validate_scale(scale: u8) -> Result<(), DactError> {
    if scale > 18 {
        return Err(DactError::InvalidScale);
    }
    Ok(())
}

// =============================================================================
// ReLU
// =============================================================================

/// ReLU activation function.
///
/// Returns x if x >= 0, else 0.
///
/// # Arguments
/// * `x` - Input Dqa value
///
/// # Returns
/// * `Ok(Dqa)` - Canonicalized result
/// * `Err(DactError::TrapInput)` - TRAP sentinel detected
/// * `Err(DactError::InvalidScale)` - Scale > 18
///
/// Gas: 2
pub fn relu(x: Dqa) -> Result<Dqa, DactError> {
    // Phase 0: TRAP check
    if is_trap(x) {
        return Err(DactError::TrapInput);
    }
    // Scale validation
    validate_scale(x.scale)?;

    // ReLU: if x.value < 0, return 0; else return x
    // Note: ReLU/ReLU6 skip canonicalization/scale normalization per RFC-0114
    if x.value < 0 {
        Ok(Dqa::new(0, x.scale).unwrap())
    } else {
        Ok(x)
    }
}

// =============================================================================
// ReLU6
// =============================================================================

/// ReLU6 activation function.
///
/// Returns clamp(x, 0, 6).
///
/// # Arguments
/// * `x` - Input Dqa value
///
/// # Returns
/// * `Ok(Dqa)` - Canonicalized result
/// * `Err(DactError::TrapInput)` - TRAP sentinel detected
/// * `Err(DactError::InvalidScale)` - Scale > 18
///
/// Gas: 3
pub fn relu6(x: Dqa) -> Result<Dqa, DactError> {
    // Phase 0: TRAP check
    if is_trap(x) {
        return Err(DactError::TrapInput);
    }
    // Scale validation
    validate_scale(x.scale)?;

    // Create max_val = 6 * 10^x.scale at same scale
    // This is the internal comparison value (NOT canonicalized per RFC-0114)
    let max_val = Dqa::new(6 * 10_i64.pow(x.scale as u32), x.scale).unwrap();

    if x.value < 0 {
        // Return 0
        Ok(Dqa::new(0, x.scale).unwrap())
    } else if x.value > max_val.value {
        // Return max_val (6 at input scale)
        Ok(max_val)
    } else {
        Ok(x)
    }
}

// =============================================================================
// LeakyReLU
// =============================================================================

/// LeakyReLU activation function.
pub fn leaky_relu(x: Dqa) -> Result<Dqa, DactError> {
    if is_trap(x) {
        return Err(DactError::TrapInput);
    }
    validate_scale(x.scale)?;
    if x.value < 0 {
        let alpha = Dqa::new(1, 2).unwrap();
        crate::dqa::dqa_mul(x, alpha).map_err(|_| DactError::Overflow)
    } else {
        Ok(x)
    }
}

// =============================================================================
// Q8.8 → DQA Conversion
// =============================================================================

mod dact_lut {
    include!("dact_lut.rs");
}

/// Q8.8 → DQA conversion using floor division (Python // semantics)
fn q88_to_dqa(q: i16, target_scale: u8) -> (i64, u8) {
    let numerator = (q as i128) * 10_i128.pow(target_scale as u32);
    let result = if numerator >= 0 {
        (numerator / 256) as i64
    } else {
        -(((-numerator + 255) / 256) as i64)
    };
    (result, target_scale)
}

/// Normalize DQA to target scale (RFC-0114 §normalize_to_scale)
pub fn normalize_to_scale(x: Dqa, target_scale: u8) -> Dqa {
    if x.scale == target_scale {
        return x;
    }
    let delta = target_scale as i32 - x.scale as i32;
    if delta > 0 {
        Dqa::new(x.value * 10_i64.pow(delta as u32), target_scale).unwrap()
    } else {
        let factor = 10_i64.pow((-delta) as u32);
        // Floor division: -153 / 100 = -2 (floor), not -1 (truncate toward zero)
        let result = if x.value >= 0 {
            x.value / factor
        } else {
            -((-x.value + factor - 1) / factor)
        };
        Dqa::new(result, target_scale).unwrap()
    }
}

/// Canonicalize DQA (RFC-0105)
fn canonicalize(dqa: Dqa) -> Dqa {
    if dqa.value == 0 {
        return Dqa { value: 0, scale: 0 };
    }
    let mut value = dqa.value;
    let mut scale = dqa.scale;
    while value % 10 == 0 && scale > 0 {
        value /= 10;
        scale -= 1;
    }
    Dqa { value, scale }
}

// =============================================================================
// Sigmoid
// =============================================================================

/// Sigmoid activation function.
///
/// Gas: 10
pub fn sigmoid(x: Dqa) -> Result<Dqa, DactError> {
    use dact_lut::SIGMOID_LUT;

    if is_trap(x) {
        return Err(DactError::TrapInput);
    }
    validate_scale(x.scale)?;

    // Phase 1: Normalize to scale=2
    let x_norm = normalize_to_scale(x, 2);

    // Phase 2: Clamp
    if x_norm.value < -400 {
        return Ok(canonicalize(Dqa::new(0, 4).unwrap()));
    }
    if x_norm.value > 400 {
        return Ok(canonicalize(Dqa::new(10000, 4).unwrap()));
    }

    // Phase 3: Index
    let idx = (x_norm.value + 400) as usize;

    // Phase 4: LUT lookup → Q8.8 → DQA
    let q = SIGMOID_LUT[idx];
    let (val, scale) = q88_to_dqa(q, 4);

    Ok(canonicalize(Dqa::new(val, scale).unwrap()))
}

// =============================================================================
// Tanh
// =============================================================================

/// Tanh activation function.
///
/// Gas: 10
pub fn tanh_dqa(x: Dqa) -> Result<Dqa, DactError> {
    use dact_lut::TANH_LUT;

    if is_trap(x) {
        return Err(DactError::TrapInput);
    }
    validate_scale(x.scale)?;

    let x_norm = normalize_to_scale(x, 2);

    if x_norm.value < -400 {
        return Ok(canonicalize(Dqa::new(-10000, 4).unwrap()));
    }
    if x_norm.value > 400 {
        return Ok(canonicalize(Dqa::new(10000, 4).unwrap()));
    }

    let idx = (x_norm.value + 400) as usize;
    let q = TANH_LUT[idx];
    let (val, scale) = q88_to_dqa(q, 4);

    Ok(canonicalize(Dqa::new(val, scale).unwrap()))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // ReLU Tests
    // =====================================================================

    #[test]
    fn test_relu_positive() {
        let x = Dqa::new(5, 0).unwrap();
        let result = relu(x).unwrap();
        assert_eq!(result, Dqa::new(5, 0).unwrap());
    }

    #[test]
    fn test_relu_negative() {
        let x = Dqa::new(-5, 0).unwrap();
        let result = relu(x).unwrap();
        assert_eq!(result, Dqa::new(0, 0).unwrap());
    }

    #[test]
    fn test_relu_zero() {
        let x = Dqa::new(0, 0).unwrap();
        let result = relu(x).unwrap();
        assert_eq!(result, Dqa::new(0, 0).unwrap());
    }

    #[test]
    fn test_relu_trap() {
        // TRAP sentinel: value = i64::MIN, scale = 0xFF
        // Cannot use Dqa::new() because it validates scale <= 18
        let trap = Dqa {
            value: i64::MIN,
            scale: 0xFF,
        };
        let result = relu(trap);
        assert_eq!(result, Err(DactError::TrapInput));
    }

    #[test]
    fn test_relu_invalid_scale() {
        // Scale > 18 should fail - construct directly bypassing validation
        let x = Dqa {
            value: 5,
            scale: 19,
        };
        let result = relu(x);
        assert_eq!(result, Err(DactError::InvalidScale));
    }

    #[test]
    fn test_relu_with_scale() {
        // ReLU should preserve scale
        let x = Dqa::new(123, 4).unwrap(); // 0.0123
        let result = relu(x).unwrap();
        assert_eq!(result.value, 123);
        assert_eq!(result.scale, 4);
    }

    // =====================================================================
    // ReLU6 Tests
    // =====================================================================

    #[test]
    fn test_relu6_negative() {
        let x = Dqa::new(-5, 0).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result, Dqa::new(0, 0).unwrap());
    }

    #[test]
    fn test_relu6_zero() {
        let x = Dqa::new(0, 0).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result, Dqa::new(0, 0).unwrap());
    }

    #[test]
    fn test_relu6_in_range() {
        let x = Dqa::new(3, 0).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result, Dqa::new(3, 0).unwrap());
    }

    #[test]
    fn test_relu6_above_range() {
        let x = Dqa::new(10, 0).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result, Dqa::new(6, 0).unwrap());
    }

    #[test]
    fn test_relu6_at_boundary() {
        // ReLU6(6) should return 6
        let x = Dqa::new(6, 0).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result, Dqa::new(6, 0).unwrap());
    }

    #[test]
    fn test_relu6_trap() {
        let trap = Dqa {
            value: i64::MIN,
            scale: 0xFF,
        };
        let result = relu6(trap);
        assert_eq!(result, Err(DactError::TrapInput));
    }

    #[test]
    fn test_relu6_with_scale() {
        // ReLU6(3.5) at scale 1 = Dqa(35, 1) should return 35
        let x = Dqa::new(35, 1).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result.value, 35);
        assert_eq!(result.scale, 1);
    }

    #[test]
    fn test_relu6_clamp_with_scale() {
        // ReLU6(7.5) at scale 1 = Dqa(75, 1) should clamp to 6.0 = Dqa(60, 1)
        let x = Dqa::new(75, 1).unwrap();
        let result = relu6(x).unwrap();
        assert_eq!(result.value, 60); // 6.0 at scale 1
        assert_eq!(result.scale, 1);
    }

    // =====================================================================
    // LeakyReLU Tests
    // =====================================================================

    #[test]
    fn test_leaky_relu_positive() {
        let x = Dqa::new(1, 0).unwrap();
        let result = leaky_relu(x).unwrap();
        assert_eq!(result, Dqa::new(1, 0).unwrap());
    }

    #[test]
    fn test_leaky_relu_negative() {
        // leaky_relu(-1.0) = -1.0 * 0.01 = -0.01 = Dqa(-1, 2)
        let x = Dqa::new(-100, 2).unwrap(); // -1.00 at scale 2
        let result = leaky_relu(x).unwrap();
        assert_eq!(result.value, -1);
        assert_eq!(result.scale, 2);
    }

    #[test]
    fn test_leaky_relu_zero() {
        let x = Dqa::new(0, 0).unwrap();
        let result = leaky_relu(x).unwrap();
        assert_eq!(result, Dqa::new(0, 0).unwrap());
    }

    #[test]
    fn test_leaky_relu_trap() {
        let trap = Dqa {
            value: i64::MIN,
            scale: 0xFF,
        };
        let result = leaky_relu(trap);
        assert_eq!(result, Err(DactError::TrapInput));
    }

    #[test]
    fn test_leaky_relu_invalid_scale() {
        let x = Dqa {
            value: 5,
            scale: 19,
        };
        let result = leaky_relu(x);
        assert_eq!(result, Err(DactError::InvalidScale));
    }

    #[test]
    fn test_leaky_relu_canonical_output() {
        // leaky_relu(-1.0) should return Dqa(-1, 2) which canonicalizes to Dqa(-1, 2)
        // Since -1 * 10^2 = -100 and multiply with Dqa(1, 2) gives -100/100 = -1
        let x = Dqa::new(-100, 2).unwrap();
        let result = leaky_relu(x).unwrap();
        // The result should be canonicalized
        assert_eq!(result.value, -1);
        assert_eq!(result.scale, 2);
    }

    // =====================================================================
    // Sigmoid Tests
    // =====================================================================

    #[test]
    fn test_sigmoid_zero() {
        // sigmoid(0) = 0.5 = Dqa(5, 1)
        let x = Dqa::new(0, 0).unwrap();
        let result = sigmoid(x).unwrap();
        assert_eq!(result.value, 5);
        assert_eq!(result.scale, 1);
    }

    #[test]
    fn test_sigmoid_positive() {
        // sigmoid(4) ≈ 0.9804
        let x = Dqa::new(400, 2).unwrap(); // 4.00 at scale 2
        let result = sigmoid(x).unwrap();
        assert_eq!(result.value, 9804);
        assert_eq!(result.scale, 4);
    }

    #[test]
    fn test_sigmoid_negative() {
        // sigmoid(-4) ≈ 0.0195
        let x = Dqa::new(-400, 2).unwrap();
        let result = sigmoid(x).unwrap();
        assert_eq!(result.value, 195);
        assert_eq!(result.scale, 4);
    }

    #[test]
    fn test_sigmoid_clamp_low() {
        // sigmoid(< -4) = 0
        let x = Dqa::new(-500, 2).unwrap();
        let result = sigmoid(x).unwrap();
        assert_eq!(result.value, 0);
        assert_eq!(result.scale, 0);
    }

    #[test]
    fn test_sigmoid_clamp_high() {
        // sigmoid(> 4) = 1 (canonicalized to scale 0)
        let x = Dqa::new(500, 2).unwrap();
        let result = sigmoid(x).unwrap();
        assert_eq!(result.value, 1);
        assert_eq!(result.scale, 0);
    }

    #[test]
    fn test_sigmoid_trap() {
        let trap = Dqa {
            value: i64::MIN,
            scale: 0xFF,
        };
        assert_eq!(sigmoid(trap), Err(DactError::TrapInput));
    }

    // =====================================================================
    // Tanh Tests
    // =====================================================================

    #[test]
    fn test_tanh_zero() {
        let x = Dqa::new(0, 0).unwrap();
        let result = tanh_dqa(x).unwrap();
        assert_eq!(result.value, 0);
        assert_eq!(result.scale, 0);
    }

    #[test]
    fn test_tanh_positive() {
        // tanh(2) ≈ 0.9648 (Python script authoritative)
        let x = Dqa::new(200, 2).unwrap(); // 2.00 at scale 2
        let result = tanh_dqa(x).unwrap();
        assert_eq!(result.value, 9648);
        assert_eq!(result.scale, 4);
    }

    #[test]
    fn test_tanh_negative() {
        // tanh(-2) ≈ -0.9649
        let x = Dqa::new(-200, 2).unwrap();
        let result = tanh_dqa(x).unwrap();
        assert_eq!(result.value, -9649);
        assert_eq!(result.scale, 4);
    }

    #[test]
    fn test_tanh_clamp_low() {
        // tanh(< -4) = -1 (canonicalized to scale 0)
        let x = Dqa::new(-500, 2).unwrap();
        let result = tanh_dqa(x).unwrap();
        assert_eq!(result.value, -1);
        assert_eq!(result.scale, 0);
    }

    #[test]
    fn test_tanh_clamp_high() {
        // tanh(> 4) = 1 (canonicalized to scale 0)
        let x = Dqa::new(500, 2).unwrap();
        let result = tanh_dqa(x).unwrap();
        assert_eq!(result.value, 1);
        assert_eq!(result.scale, 0);
    }

    #[test]
    fn test_tanh_trap() {
        let trap = Dqa {
            value: i64::MIN,
            scale: 0xFF,
        };
        assert_eq!(tanh_dqa(trap), Err(DactError::TrapInput));
    }
}
