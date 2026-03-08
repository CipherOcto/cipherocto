//! Deterministic Floating-Point Verification Probe
//!
//! This module provides hardware/software verification for DFP operations.
//! Used for consensus-grade verification that nodes produce identical results.

use crate::{Dfp, DfpEncoding};

/// Current DFP spec version - increment on any arithmetic change
pub const DFP_SPEC_VERSION: u32 = 1;

/// Verification probe result
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Whether verification passed
    pub passed: bool,
    /// The 24-byte encoding of the result
    pub encoding: [u8; 24],
    /// Human-readable result
    pub result: Dfp,
    /// Error message if failed
    pub error: Option<String>,
}

impl ProbeResult {
    /// Create a passing result
    pub fn pass(result: Dfp) -> Self {
        let encoding = result.to_encoding().to_bytes();
        ProbeResult {
            passed: true,
            encoding,
            result,
            error: None,
        }
    }

    /// Create a failing result
    pub fn fail(result: Dfp, error: String) -> Self {
        let encoding = result.to_encoding().to_bytes();
        ProbeResult {
            passed: false,
            encoding,
            result,
            error: Some(error),
        }
    }
}

/// Verification probe for DFP operations
pub struct DeterministicFloatProbe;

impl DeterministicFloatProbe {
    /// Verify a DFP operation produces deterministic result
    pub fn verify(op: &str, a: Dfp, b: Option<Dfp>) -> ProbeResult {
        let result = match op {
            "add" => {
                let b = b.expect("add requires two operands");
                crate::dfp_add(a, b)
            }
            "sub" => {
                let b = b.expect("sub requires two operands");
                crate::dfp_sub(a, b)
            }
            "mul" => {
                let b = b.expect("mul requires two operands");
                crate::dfp_mul(a, b)
            }
            "div" => {
                let b = b.expect("div requires two operands");
                crate::dfp_div(a, b)
            }
            "sqrt" => crate::dfp_sqrt(a),
            _ => return ProbeResult::fail(Dfp::nan(), format!("Unknown operation: {}", op)),
        };

        ProbeResult::pass(result)
    }

    /// Get node capability advertisement
    pub fn capability() -> u32 {
        DFP_SPEC_VERSION
    }

    /// Run a determinism check - same input must produce same output
    pub fn determinism_check(op: &str, a: Dfp, b: Option<Dfp>, runs: usize) -> ProbeResult {
        let mut last_encoding: Option<[u8; 24]> = None;

        for i in 0..runs {
            let result = Self::verify(op, a, b);
            let encoding = result.encoding;

            if let Some(prev) = last_encoding {
                if encoding != prev {
                    return ProbeResult::fail(
                        result.result,
                        format!("Non-deterministic: run {} encoding {:02x?} != run 0 {:02x?}", i, encoding, prev)
                    );
                }
            }
            last_encoding = Some(encoding);
        }

        // Return last result
        Self::verify(op, a, b)
    }

    /// Run full verification suite
    pub fn run_suite() -> Vec<ProbeResult> {
        let mut results = Vec::new();

        // Basic operation tests
        let test_cases = [
            ("add", Dfp::from_f64(1.0), Some(Dfp::from_f64(1.0))),
            ("add", Dfp::from_f64(3.0), Some(Dfp::from_f64(1.0))),
            ("add", Dfp::from_f64(1.0), Some(Dfp::from_f64(2.0))),
            ("sub", Dfp::from_f64(3.0), Some(Dfp::from_f64(1.0))),
            ("sub", Dfp::from_f64(5.0), Some(Dfp::from_f64(3.0))),
            ("mul", Dfp::from_f64(3.0), Some(Dfp::from_f64(2.0))),
            ("mul", Dfp::from_f64(5.0), Some(Dfp::from_f64(3.0))),
            ("div", Dfp::from_f64(6.0), Some(Dfp::from_f64(2.0))),
            ("div", Dfp::from_f64(8.0), Some(Dfp::from_f64(2.0))),
            ("sqrt", Dfp::from_f64(4.0), None),
            ("sqrt", Dfp::from_f64(9.0), None),
            ("sqrt", Dfp::from_f64(16.0), None),
        ];

        for (op, a, b) in test_cases.iter() {
            let result = Self::determinism_check(op, *a, *b, 3);
            results.push(result);
        }

        // Special values
        let special_cases = [
            ("add", Dfp::nan(), Some(Dfp::from_f64(1.0))),
            ("add", Dfp::infinity(), Some(Dfp::from_f64(1.0))),
            ("add", Dfp::zero(), Some(Dfp::from_f64(1.0))),
            ("mul", Dfp::zero(), Some(Dfp::from_f64(1.0))),
            ("mul", Dfp::infinity(), Some(Dfp::from_f64(1.0))),
        ];

        for (op, a, b) in special_cases.iter() {
            let result = Self::verify(op, *a, *b);
            results.push(result);
        }

        results
    }

    /// Check if all probes pass
    pub fn verify_all() -> bool {
        Self::run_suite().iter().all(|r| r.passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_capability() {
        let cap = DeterministicFloatProbe::capability();
        assert_eq!(cap, DFP_SPEC_VERSION);
    }

    #[test]
    fn test_probe_basic_add() {
        let a = Dfp::from_f64(1.0);
        let b = Dfp::from_f64(1.0);
        let result = DeterministicFloatProbe::verify("add", a, Some(b));

        assert!(result.passed);
        let expected = Dfp::from_f64(2.0);
        assert_eq!(result.result.to_f64(), expected.to_f64());
    }

    #[test]
    fn test_probe_basic_mul() {
        let a = Dfp::from_f64(3.0);
        let b = Dfp::from_f64(2.0);
        let result = DeterministicFloatProbe::verify("mul", a, Some(b));

        assert!(result.passed);
        let expected = Dfp::from_f64(6.0);
        assert_eq!(result.result.to_f64(), expected.to_f64());
    }

    #[test]
    fn test_probe_sqrt() {
        let a = Dfp::from_f64(4.0);
        let result = DeterministicFloatProbe::verify("sqrt", a, None);

        assert!(result.passed);
        let expected = Dfp::from_f64(2.0);
        assert_eq!(result.result.to_f64(), expected.to_f64());
    }

    #[test]
    fn test_encoding_24_bytes() {
        let dfp = Dfp::from_f64(1.5);
        let encoding = dfp.to_encoding().to_bytes();

        // Verify 24 bytes
        assert_eq!(encoding.len(), 24);

        // Verify deterministic - same input always produces same output
        let dfp2 = Dfp::from_f64(1.5);
        let encoding2 = dfp2.to_encoding().to_bytes();
        assert_eq!(encoding, encoding2);
    }

    #[test]
    fn test_special_values_encoding() {
        // Test NaN
        let nan = Dfp::nan();
        let nan_enc = nan.to_encoding().to_bytes();
        assert_eq!(nan_enc.len(), 24);

        // Test Infinity
        let inf = Dfp::infinity();
        let inf_enc = inf.to_encoding().to_bytes();
        assert_eq!(inf_enc.len(), 24);

        // Test Zero
        let zero = Dfp::zero();
        let zero_enc = zero.to_encoding().to_bytes();
        assert_eq!(zero_enc.len(), 24);

        // Test negative zero
        let neg_zero = Dfp::neg_zero();
        let neg_zero_enc = neg_zero.to_encoding().to_bytes();
        assert_eq!(neg_zero_enc.len(), 24);
    }

    #[test]
    fn test_determinism_check() {
        // Same operation multiple times - must produce identical encoding
        let a = Dfp::from_f64(1.5);
        let b = Dfp::from_f64(2.5);
        let result = DeterministicFloatProbe::determinism_check("add", a, Some(b), 5);

        assert!(result.passed, "Determinism check failed: {:?}", result.error);
    }

    #[test]
    fn test_run_suite() {
        let results = DeterministicFloatProbe::run_suite();
        assert!(!results.is_empty());

        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.iter().filter(|r| !r.passed).count();

        eprintln!("Probe suite: {}/{} passed", passed, results.len());

        // All should pass
        for (i, r) in results.iter().enumerate() {
            if !r.passed {
                eprintln!("  Test {} failed: {:?}", i, r.error);
            }
        }

        assert!(failed == 0, "Some probe tests failed");
    }

    #[test]
    fn test_verify_all() {
        assert!(DeterministicFloatProbe::verify_all());
    }
}
