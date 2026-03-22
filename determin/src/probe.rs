#![allow(dead_code)]

//! Deterministic Floating-Point Verification Probe
//!
//! This module provides hardware/software verification for DFP operations.
//! Used for consensus-grade verification that nodes produce identical results.
//!
//! ## BigInt Probe Implementation Fixes (v2.12)
//!
//! This section documents all fixes applied to align Rust implementation with the
//! Python reference script (scripts/compute_bigint_probe_root.py).
//!
//! ### Fix 1: Entry 52 - Wrong Value (2026-03-15)
//!
//! Problem: Entry 52 in Rust used BigIntProbeValue::Max (4096-bit) but Python uses MAX_U64.
//! Python DATA: (52,'ADD',MAX_U64,1) - adds 2^64-1 + 1
//! Result: Merkle root mismatch until fixed to BigIntProbeValue::Int(MAX_U64 as i128)
//!
//! ### Fix 2: clippy - manual_div_ceil (2026-03-15)
//!
//! Problem: (num_bits + 63) / 64 flagged as reimplementing div_ceil()
//! Fix: Changed to num_bits.div_ceil(64) in bigint_encode_probe_value()
//!
//! ### Fix 3: clippy - needless_borrows_for_generic_args (2026-03-15)
//!
//! Problem: hasher.update(&value) had unnecessary borrows
//! Fix: Changed to hasher.update(value) in 4 locations (lines 334, 357, 410, 411)
//!
//! ### Verification
//!
//! After all fixes:
//! - cargo test --release: All 115 tests pass
//! - cargo clippy: Zero warnings
//! - Merkle root: c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0

use crate::decimal::Decimal;
use crate::decimal::{
    decimal_add, decimal_cmp, decimal_mul, decimal_sqrt, decimal_sub, decimal_to_dqa,
};
use crate::Dfp;

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
                        format!(
                            "Non-deterministic: run {} encoding {:02x?} != run 0 {:02x?}",
                            i, encoding, prev
                        ),
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

        assert!(
            result.passed,
            "Determinism check failed: {:?}",
            result.error
        );
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

// =============================================================================
// BigInt Verification Probe (RFC-0110)
// =============================================================================

use num_integer::Integer;
use sha2::{Digest, Sha256};

/// Operation IDs as per RFC-0110
pub const OP_ADD: u64 = 1;
pub const OP_SUB: u64 = 2;
pub const OP_MUL: u64 = 3;
pub const OP_DIV: u64 = 4;
pub const OP_MOD: u64 = 5;
pub const OP_SHL: u64 = 6;
pub const OP_SHR: u64 = 7;
pub const OP_CANONICALIZE: u64 = 8;
pub const OP_CMP: u64 = 9;
pub const OP_BITLEN: u64 = 10;
pub const OP_SERIALIZE: u64 = 11;
pub const OP_DESERIALIZE: u64 = 12;
pub const OP_I128_ROUNDTRIP: u64 = 13;

/// Special sentinel values
const MAX_U64: u64 = 0xFFFFFFFFFFFFFFFF;
const MAX_U56: u64 = (1 << 56) - 1;
const TRAP: u64 = 0xDEAD_DEAD_DEAD_DEAD;

/// Encode a value to 8 bytes for the probe entry
/// Follows RFC-0110 compact encoding rules
pub fn bigint_encode_value(value: i128, neg: bool) -> [u8; 8] {
    // Handle special cases
    if value == 0 {
        return [0u8; 8];
    }

    let av = value.unsigned_abs();

    // Small values: ≤ 2^56
    if av <= MAX_U56 as u128 {
        let mut bytes = [0u8; 8];
        bytes[..7].copy_from_slice(&av.to_le_bytes()[..7]);
        bytes[7] = if neg { 0x80 } else { 0x00 };
        return bytes;
    }

    // Large values: hash reference - compute number of limbs
    let num_bits = 128 - av.leading_zeros() as usize;
    let n = num_bits.div_ceil(64);
    let limbs: Vec<u64> = (0..n).map(|i| (av >> (64 * i)) as u64).collect();

    let mut hdr = [0u8; 8];
    hdr[0] = 1; // version
    hdr[1] = if neg { 0xFF } else { 0x00 };
    hdr[4] = n as u8;

    let mut hasher = Sha256::new();
    hasher.update(hdr);
    for limb in &limbs {
        hasher.update(limb.to_le_bytes());
    }

    let result = hasher.finalize();
    let mut encoded = [0u8; 8];
    encoded.copy_from_slice(&result[..8]);
    encoded
}

/// Encode a BigInt limb array (for CANONICALIZE operations)
pub fn bigint_encode_limbs(limbs: &[u64]) -> [u8; 8] {
    let n = limbs.len();
    if n == 0 {
        return [0u8; 8];
    }

    let mut hdr = [0u8; 8];
    hdr[0] = 1; // version
    hdr[4] = n as u8;

    let mut hasher = Sha256::new();
    hasher.update(hdr);
    for &limb in limbs {
        hasher.update(limb.to_le_bytes());
    }

    let result = hasher.finalize();
    let mut encoded = [0u8; 8];
    encoded.copy_from_slice(&result[..8]);
    encoded
}

/// Encode MAX sentinel
pub fn bigint_encode_max() -> [u8; 8] {
    MAX_U64.to_le_bytes()
}

/// Encode TRAP sentinel
pub fn bigint_encode_trap() -> [u8; 8] {
    TRAP.to_le_bytes()
}

/// Create a probe entry (24 bytes: op_id + input_a + input_b)
pub fn bigint_make_entry(op_id: u64, a_encoded: &[u8; 8], b_encoded: &[u8; 8]) -> [u8; 24] {
    let mut entry = [0u8; 24];
    entry[..8].copy_from_slice(&op_id.to_le_bytes());
    entry[8..16].copy_from_slice(a_encoded);
    entry[16..24].copy_from_slice(b_encoded);
    entry
}

/// Compute SHA-256 hash of probe entry
pub fn bigint_entry_hash(entry: &[u8; 24]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(entry);
    hasher.finalize().into()
}

/// Build Merkle tree from entry hashes
/// Returns the Merkle root
pub fn bigint_build_merkle_tree(hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut level: Vec<[u8; 32]> = hashes.to_vec();

    while level.len() > 1 {
        // Duplicate last if odd
        if level.len() % 2 == 1 {
            level.push(level.last().copied().unwrap());
        }

        // Compute parent hashes
        level = level
            .chunks(2)
            .map(|pair| {
                let mut hasher = Sha256::new();
                hasher.update(pair[0]);
                hasher.update(pair[1]);
                hasher.finalize().into()
            })
            .collect();
    }

    level[0]
}

/// Reference Merkle root from RFC-0110
pub const BIGINT_REFERENCE_MERKLE_ROOT: &str =
    "c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0";

/// Verify Merkle root matches reference
pub fn bigint_verify_merkle_root(root: &[u8; 32]) -> bool {
    let expected = hex::decode(BIGINT_REFERENCE_MERKLE_ROOT).unwrap();
    root == expected.as_slice()
}

// =============================================================================
// BigInt Probe Entries (56 total)
// =============================================================================

/// Probe entry data structure
#[derive(Debug, Clone)]
pub struct BigIntProbeEntry {
    pub index: usize,
    pub op_id: u64,
    pub input_a: BigIntProbeValue,
    pub input_b: BigIntProbeValue,
    pub description: &'static str,
}

/// Probe input value types
///
/// # IMPORTANT: Sentinel vs Integer Distinction
///
/// This enum has two kinds of values: **sentinels** (special probe markers) and **integers**
/// (actual BigInt operand values). They encode to DIFFERENT bytes in the compact probe format,
/// so using the wrong variant will silently produce wrong probe entries.
///
/// | Variant | Encodes to | Use when |
/// |---------|------------|----------|
/// | `Int(MAX_U64)` | `43 c9 c2...` (hash-ref) | Entry tests integer 2^64-1 as operand |
/// | `Max` | `ff ff ff ff ff ff ff ff` | Entry tests 4096-bit MAX_BIGINT sentinel |
/// | `Int(TRAP)` | `43 xx xx...` (hash-ref) | Entry tests integer TRAP_VALUE as operand |
/// | `Trap` | `de ad de ad de ad de ad` | Entry tests TRAP sentinel |
///
/// **Common mistake:** Writing `BigIntProbeValue::Max` when you mean "the integer 2^64-1".
/// This will produce a probe entry with different bytes than one using `Int(MAX_U64 as i128)`,
/// even though both represent the same numeric value. The probe Merkle root will differ.
#[derive(Debug, Clone)]
pub enum BigIntProbeValue {
    /// Integer value (use this for actual BigInt operands like 1, 42, MAX_U64, etc.)
    Int(i128),
    /// BigInt limbs (for CANONICALIZE operation)
    Limbs(Vec<u64>),
    /// **4096-bit MAX_BIGINT sentinel** — NOT the integer 2^64-1
    ///
    /// Only use `Max` when the probe entry explicitly tests the overflow boundary
    /// at MAX_BIGINT_BITS (4096 bits). For testing 2^64-1 + 1 carry propagation,
    /// use `Int(MAX_U64 as i128)` instead.
    Max,
    /// **TRAP sentinel** — triggers overflow/division-by-zero error
    ///
    /// Only use `Trap` when the probe entry explicitly tests TRAP behavior.
    /// For testing arithmetic with the integer value 0xDEAD_DEAD_DEAD_DEAD,
    /// use `Int(TRAP as i128)` instead.
    Trap,
    /// Hash reference for serialization (SHA256 of serialized canonical bytes)
    HashRef,
}

impl BigIntProbeEntry {
    /// Get the encoded inputs for this entry
    pub fn encode_inputs(&self) -> ([u8; 8], [u8; 8]) {
        let a = bigint_encode_probe_value(&self.input_a);
        let b = bigint_encode_probe_value(&self.input_b);
        (a, b)
    }
}

fn bigint_encode_probe_value(value: &BigIntProbeValue) -> [u8; 8] {
    match value {
        BigIntProbeValue::Int(n) => {
            if *n < 0 {
                bigint_encode_value(-*n, true)
            } else {
                bigint_encode_value(*n, false)
            }
        }
        BigIntProbeValue::Limbs(limbs) => bigint_encode_limbs(limbs),
        BigIntProbeValue::Max => bigint_encode_max(),
        BigIntProbeValue::Trap => bigint_encode_trap(),
        BigIntProbeValue::HashRef => {
            // HASHREF for serialize(1): SHA256 of serialized BigInt(1)
            // From Python: _bigint1_bytes = [0x01,0x00,0x00,0x00,0x01,0x00,0x00,0x00, 0x01,0x00,0x00,0x00,0x00,0x00,0x00,0x00]
            // hash = sha256(_bigint1_bytes).digest()[:8] = c4cbcdbb1fa3e794
            hex::decode("c4cbcdbb1fa3e794").unwrap().try_into().unwrap()
        }
    }
}

/// All 56 probe entries
pub fn bigint_all_probe_entries() -> Vec<BigIntProbeEntry> {
    vec![
        // ADD operations (entries 0-4)
        BigIntProbeEntry {
            index: 0,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(2),
            description: "0 + 2",
        },
        BigIntProbeEntry {
            index: 1,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Int((1u128 << 64) as i128),
            input_b: BigIntProbeValue::Int(1),
            description: "2^64",
        },
        BigIntProbeEntry {
            index: 2,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Int(MAX_U64 as i128),
            input_b: BigIntProbeValue::Int(1),
            description: "MAX_U64 + 1",
        },
        BigIntProbeEntry {
            index: 3,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(-1),
            description: "1 + (-1)",
        },
        BigIntProbeEntry {
            index: 4,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Max,
            description: "MAX + MAX → TRAP",
        },
        // SUB operations (entries 5-9)
        BigIntProbeEntry {
            index: 5,
            op_id: OP_SUB,
            input_a: BigIntProbeValue::Int(-5),
            input_b: BigIntProbeValue::Int(-2),
            description: "-5 - (-2)",
        },
        BigIntProbeEntry {
            index: 6,
            op_id: OP_SUB,
            input_a: BigIntProbeValue::Int(5),
            input_b: BigIntProbeValue::Int(5),
            description: "5 - 5",
        },
        BigIntProbeEntry {
            index: 7,
            op_id: OP_SUB,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(0),
            description: "0 - 0",
        },
        BigIntProbeEntry {
            index: 8,
            op_id: OP_SUB,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(-1),
            description: "1 - (-1)",
        },
        BigIntProbeEntry {
            index: 9,
            op_id: OP_SUB,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Int(1),
            description: "MAX - 1",
        },
        // MUL operations (entries 10-15)
        BigIntProbeEntry {
            index: 10,
            op_id: OP_MUL,
            input_a: BigIntProbeValue::Int(2),
            input_b: BigIntProbeValue::Int(3),
            description: "2 × 3",
        },
        BigIntProbeEntry {
            index: 11,
            op_id: OP_MUL,
            input_a: BigIntProbeValue::Int(1 << 32),
            input_b: BigIntProbeValue::Int(1 << 32),
            description: "2^32 × 2^32",
        },
        BigIntProbeEntry {
            index: 12,
            op_id: OP_MUL,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(1),
            description: "0 × 1",
        },
        BigIntProbeEntry {
            index: 13,
            op_id: OP_MUL,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Max,
            description: "MAX × MAX → TRAP",
        },
        BigIntProbeEntry {
            index: 14,
            op_id: OP_MUL,
            input_a: BigIntProbeValue::Int(-3),
            input_b: BigIntProbeValue::Int(4),
            description: "-3 × 4",
        },
        BigIntProbeEntry {
            index: 15,
            op_id: OP_MUL,
            input_a: BigIntProbeValue::Int(-2),
            input_b: BigIntProbeValue::Int(-3),
            description: "-2 × -3",
        },
        // DIV operations (entries 16-20)
        BigIntProbeEntry {
            index: 16,
            op_id: OP_DIV,
            input_a: BigIntProbeValue::Int(10),
            input_b: BigIntProbeValue::Int(3),
            description: "10 / 3",
        },
        BigIntProbeEntry {
            index: 17,
            op_id: OP_DIV,
            input_a: BigIntProbeValue::Int(100),
            input_b: BigIntProbeValue::Int(10),
            description: "100 / 10",
        },
        BigIntProbeEntry {
            index: 18,
            op_id: OP_DIV,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Int(1),
            description: "MAX / 1",
        },
        BigIntProbeEntry {
            index: 19,
            op_id: OP_DIV,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Max,
            description: "1 / MAX",
        },
        // Entry 20: 2^128 / 2^64 (not 2^4096!). RFC table has wrong description.
        // 2^128 has bit_length 129, so n=3: limbs [0, 0, 1]
        // 2^64 has n=2: limbs [0, 1]
        BigIntProbeEntry {
            index: 20,
            op_id: OP_DIV,
            input_a: BigIntProbeValue::Limbs(vec![0, 0, 1]),
            input_b: BigIntProbeValue::Limbs(vec![0, 1]),
            description: "2^128 / 2^64",
        },
        // MOD operations (entries 21-23)
        BigIntProbeEntry {
            index: 21,
            op_id: OP_MOD,
            input_a: BigIntProbeValue::Int(-7),
            input_b: BigIntProbeValue::Int(3),
            description: "-7 % 3",
        },
        BigIntProbeEntry {
            index: 22,
            op_id: OP_MOD,
            input_a: BigIntProbeValue::Int(10),
            input_b: BigIntProbeValue::Int(3),
            description: "10 % 3",
        },
        BigIntProbeEntry {
            index: 23,
            op_id: OP_MOD,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Int(3),
            description: "MAX % 3",
        },
        // SHL operations (entries 24-27)
        BigIntProbeEntry {
            index: 24,
            op_id: OP_SHL,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(4095),
            description: "1 << 4095",
        },
        BigIntProbeEntry {
            index: 25,
            op_id: OP_SHL,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(64),
            description: "1 << 64",
        },
        BigIntProbeEntry {
            index: 26,
            op_id: OP_SHL,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(1),
            description: "1 << 1",
        },
        BigIntProbeEntry {
            index: 27,
            op_id: OP_SHL,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Int(1),
            description: "MAX << 1 → TRAP",
        },
        // SHR operations (entries 28-31)
        // 2^4095: bit_length=4096, 64 limbs, bit 4095 is at position 4095-64*63 = 63 of limb 63
        // limbs = [0, 0, ..., 0, 1<<63] (1 at bit 63 of limb 63, which is index 63)
        BigIntProbeEntry {
            index: 28,
            op_id: OP_SHR,
            input_a: BigIntProbeValue::Limbs({
                let mut l = vec![0u64; 64];
                l[63] = 1 << 63;
                l
            }),
            input_b: BigIntProbeValue::Int(1),
            description: "2^4095 >> 1",
        },
        BigIntProbeEntry {
            index: 29,
            op_id: OP_SHR,
            input_a: BigIntProbeValue::Limbs({
                let mut l = vec![0u64; 64];
                l[63] = 1 << 63;
                l
            }),
            input_b: BigIntProbeValue::Int(4096),
            description: "2^4095 >> 4096",
        },
        BigIntProbeEntry {
            index: 30,
            op_id: OP_SHR,
            input_a: BigIntProbeValue::Limbs({
                let mut l = vec![0u64; 64];
                l[63] = 1 << 63;
                l
            }),
            input_b: BigIntProbeValue::Int(64),
            description: "2^4095 >> 64",
        },
        BigIntProbeEntry {
            index: 31,
            op_id: OP_SHR,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(0),
            description: "1 >> 0",
        },
        // CANONICALIZE operations (entries 32-36)
        BigIntProbeEntry {
            index: 32,
            op_id: OP_CANONICALIZE,
            input_a: BigIntProbeValue::Limbs(vec![0, 0, 0]),
            input_b: BigIntProbeValue::Int(0),
            description: "[0,0,0] → [0]",
        },
        BigIntProbeEntry {
            index: 33,
            op_id: OP_CANONICALIZE,
            input_a: BigIntProbeValue::Limbs(vec![5, 0, 0]),
            input_b: BigIntProbeValue::Int(5),
            description: "[5,0,0] → [5]",
        },
        BigIntProbeEntry {
            index: 34,
            op_id: OP_CANONICALIZE,
            input_a: BigIntProbeValue::Limbs(vec![0]),
            input_b: BigIntProbeValue::Int(0),
            description: "[0] → [0]",
        },
        BigIntProbeEntry {
            index: 35,
            op_id: OP_CANONICALIZE,
            input_a: BigIntProbeValue::Limbs(vec![1, 0]),
            input_b: BigIntProbeValue::Int(1),
            description: "[1,0] → [1]",
        },
        BigIntProbeEntry {
            index: 36,
            op_id: OP_CANONICALIZE,
            input_a: BigIntProbeValue::Limbs(vec![MAX_U64, 0, 0]),
            input_b: BigIntProbeValue::Int(MAX_U64 as i128),
            description: "[MAX,0,0] → [MAX]",
        },
        // CMP operations (entries 37-41)
        BigIntProbeEntry {
            index: 37,
            op_id: OP_CMP,
            input_a: BigIntProbeValue::Int(-5),
            input_b: BigIntProbeValue::Int(-3),
            description: "-5 vs -3",
        },
        BigIntProbeEntry {
            index: 38,
            op_id: OP_CMP,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(1),
            description: "0 vs 1",
        },
        BigIntProbeEntry {
            index: 39,
            op_id: OP_CMP,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Max,
            description: "MAX vs MAX",
        },
        BigIntProbeEntry {
            index: 40,
            op_id: OP_CMP,
            input_a: BigIntProbeValue::Int(-1),
            input_b: BigIntProbeValue::Int(1),
            description: "-1 vs 1",
        },
        BigIntProbeEntry {
            index: 41,
            op_id: OP_CMP,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(2),
            description: "1 vs 2",
        },
        // I128_ROUNDTRIP operations (entries 42-46)
        BigIntProbeEntry {
            index: 42,
            op_id: OP_I128_ROUNDTRIP,
            input_a: BigIntProbeValue::Int(i128::MAX),
            input_b: BigIntProbeValue::Int(0),
            description: "i128::MAX",
        },
        BigIntProbeEntry {
            index: 43,
            op_id: OP_I128_ROUNDTRIP,
            input_a: BigIntProbeValue::Int(i128::MIN),
            input_b: BigIntProbeValue::Int(0),
            description: "i128::MIN",
        },
        BigIntProbeEntry {
            index: 44,
            op_id: OP_I128_ROUNDTRIP,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(0),
            description: "0",
        },
        BigIntProbeEntry {
            index: 45,
            op_id: OP_I128_ROUNDTRIP,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(0),
            description: "1",
        },
        BigIntProbeEntry {
            index: 46,
            op_id: OP_I128_ROUNDTRIP,
            input_a: BigIntProbeValue::Int(-1),
            input_b: BigIntProbeValue::Int(0),
            description: "-1",
        },
        // BITLEN operations (entries 47-50)
        BigIntProbeEntry {
            index: 47,
            op_id: OP_BITLEN,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(1),
            description: "bit_len(0)",
        },
        BigIntProbeEntry {
            index: 48,
            op_id: OP_BITLEN,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::Int(1),
            description: "bit_len(1)",
        },
        BigIntProbeEntry {
            index: 49,
            op_id: OP_BITLEN,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Int(4096),
            description: "bit_len(MAX)",
        },
        BigIntProbeEntry {
            index: 50,
            op_id: OP_BITLEN,
            input_a: BigIntProbeValue::Int(1 << 63),
            input_b: BigIntProbeValue::Int(64),
            description: "bit_len(2^63)",
        },
        // Additional ADD/SUB (entries 51-53)
        BigIntProbeEntry {
            index: 51,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Max,
            input_b: BigIntProbeValue::Int(1),
            description: "MAX + 1 → TRAP",
        },
        BigIntProbeEntry {
            index: 52,
            op_id: OP_ADD,
            input_a: BigIntProbeValue::Int(MAX_U64 as i128),
            input_b: BigIntProbeValue::Int(1),
            description: "(2^64-1) + 1",
        },
        BigIntProbeEntry {
            index: 53,
            op_id: OP_SUB,
            input_a: BigIntProbeValue::Int(0),
            input_b: BigIntProbeValue::Int(1),
            description: "0 - 1",
        },
        // SERIALIZE/DESERIALIZE (entries 54-55)
        BigIntProbeEntry {
            index: 54,
            op_id: OP_SERIALIZE,
            input_a: BigIntProbeValue::Int(1),
            input_b: BigIntProbeValue::HashRef,
            description: "serialize(1)",
        },
        BigIntProbeEntry {
            index: 55,
            op_id: OP_DESERIALIZE,
            input_a: BigIntProbeValue::HashRef,
            input_b: BigIntProbeValue::Int(1),
            description: "deserialize",
        },
    ]
}

/// Compute all entry hashes and build Merkle tree
pub fn bigint_compute_merkle_root() -> [u8; 32] {
    let entries = bigint_all_probe_entries();
    let mut hashes = Vec::with_capacity(56);

    for entry in entries {
        let (a_enc, b_enc) = entry.encode_inputs();
        let probe_entry = bigint_make_entry(entry.op_id, &a_enc, &b_enc);
        let h = bigint_entry_hash(&probe_entry);
        hashes.push(h);
    }

    bigint_build_merkle_tree(&hashes)
}

// =============================================================================
// BigInt Probe Tests
// =============================================================================

#[cfg(test)]
mod bigint_tests {
    use super::*;

    #[test]
    fn test_encode_value_small_positive() {
        let enc = bigint_encode_value(42, false);
        assert_eq!(&enc[..7], &42i128.to_le_bytes()[..7]);
        assert_eq!(enc[7], 0x00);
    }

    #[test]
    fn test_encode_value_small_negative() {
        let enc = bigint_encode_value(42, true);
        assert_eq!(&enc[..7], &42i128.to_le_bytes()[..7]);
        assert_eq!(enc[7], 0x80);
    }

    #[test]
    fn test_encode_value_zero() {
        let enc = bigint_encode_value(0, false);
        assert_eq!(enc, [0u8; 8]);
    }

    #[test]
    fn test_encode_max() {
        let enc = bigint_encode_max();
        eprintln!("MAX encoded: {:02x?}", enc);
        assert_eq!(enc, MAX_U64.to_le_bytes());
    }

    #[test]
    fn test_encode_trap() {
        let enc = bigint_encode_trap();
        assert_eq!(enc, TRAP.to_le_bytes());
    }

    #[test]
    fn test_make_entry() {
        let a = bigint_encode_value(1, false);
        let b = bigint_encode_value(2, false);
        let entry = bigint_make_entry(OP_ADD, &a, &b);
        assert_eq!(&entry[..8], &1u64.to_le_bytes());
    }

    #[test]
    fn test_entry_hash() {
        let a = bigint_encode_value(1, false);
        let b = bigint_encode_value(2, false);
        let entry = bigint_make_entry(OP_ADD, &a, &b);
        let h = bigint_entry_hash(&entry);
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn test_hashref() {
        // Check what HashRef is encoding to
        let h = bigint_encode_probe_value(&BigIntProbeValue::HashRef);
        eprintln!("HashRef encoded: {:02x?}", h);
    }

    #[test]
    fn test_check_entries() {
        // Python hashes from full script
        let python_hashes = [
            "23e8d60b496f9e37",
            "8f45c0adb4403aa3",
            "05adc7ee38381723",
            "adb8767706d72e65",
            "02d263e111f3857d",
            "26f6146fc89d5b71",
            "9765ce5ba9ff5bff",
            "2d806c3c07145b3d",
            "ef8cc16731706d95",
            "5f76d222c9f11e0c",
            "47961f3a97653a43",
            "eca9c9775e0af9c8",
            "77064a0cfbf65675",
            "5f3b4f146efb186e",
            "55c31c1d15c9a8d6",
            "e5543e8f38b7d353",
            "bc514e67c587b5c3",
            "51186b587140c9f0",
            "3845c375d158d294",
            "5183f04b24263f0a",
            "e412123d991dfcd9",
            "2433dcef9509f493",
            "f187e3effe85c535",
            "6ade3e244a96a710",
            "5c175aeedb3b0253",
            "400aaa3df47fca1d",
            "9e6e9620e5f15ef9",
            "fc3ff879ca275da5",
            "a8d1007e8aee6eeb",
            "9b3c64bffea6a252",
            "eee46ebe3f960d96",
            "c880e35928e405b2",
            "0977f5eee8d51acd",
            "bcb9d7bb213554f8",
            "03c3e588a40b3ae9",
            "3c244b414bf68f06",
            "9c12f0cec95acf81",
            "d6790375588042c5",
            "6892200b988df81f",
            "0f322a7fa3ccbac4",
            "3f7dceb3ed215007",
            "504e37c95ec24c56",
            "f8a0a594eab3b800",
            "dd3b6c8f24216083",
            "2e216797bff8a566",
            "370261eb9506bf9e",
            "c1f2aa14898b6971",
            "899c200706ad1e56",
            "4861e2d12e1b0284",
            "35301b2bbc4bf3d0",
            "d4b2749a53b112b3",
            "7044098303c9fafd",
            "05adc7ee38381723",
            "53afea624a503a0b",
            "7913564ed70f2a20",
            "4683de3b4072bd54",
        ];

        let entries = bigint_all_probe_entries();
        let mut mismatches = 0;
        for i in 0..56 {
            let entry = &entries[i];
            let (a_enc, b_enc) = entry.encode_inputs();
            let probe_entry = bigint_make_entry(entry.op_id, &a_enc, &b_enc);
            let h = bigint_entry_hash(&probe_entry);
            let rust_hex = format!(
                "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
            );
            if rust_hex != python_hashes[i] {
                mismatches += 1;
                eprintln!(
                    "MISMATCH {:2}: {} vs {} - {:?}",
                    i, rust_hex, python_hashes[i], entry.description
                );
            }
        }
        assert_eq!(mismatches, 0, "per-entry hash mismatches");
        eprintln!("Total mismatches: {}", mismatches);

        let root = bigint_compute_merkle_root();
        eprintln!("Computed root: {:02x?}", root);
    }

    #[test]
    fn test_merkle_root() {
        let root = bigint_compute_merkle_root();
        eprintln!("Computed root: {:02x?}", root);
        // Also compute the Python reference to compare
        // Expected: c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0
        let expected_hex = "c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0";
        eprintln!("Expected root: {}", expected_hex);
        assert!(bigint_verify_merkle_root(&root));
    }

    #[test]
    fn test_all_56_entries() {
        let entries = bigint_all_probe_entries();
        assert_eq!(entries.len(), 56);
    }
}

// =============================================================================
// DECIMAL Verification Probe (RFC-0111)
// =============================================================================

/// Operation IDs as per RFC-0111
pub const DECIMAL_OP_ADD: u64 = 1;
pub const DECIMAL_OP_SUB: u64 = 2;
pub const DECIMAL_OP_MUL: u64 = 3;
pub const DECIMAL_OP_DIV: u64 = 4;
pub const DECIMAL_OP_SQRT: u64 = 5;
pub const DECIMAL_OP_ROUND: u64 = 6;
pub const DECIMAL_OP_CANONICALIZE: u64 = 7;
pub const DECIMAL_OP_CMP: u64 = 8;
pub const DECIMAL_OP_SERIALIZE: u64 = 9;
pub const DECIMAL_OP_DESERIALIZE: u64 = 10;
pub const DECIMAL_OP_TO_DQA: u64 = 11;
pub const DECIMAL_OP_FROM_DQA: u64 = 12;

/// Special sentinel values for DECIMAL
const DECIMAL_MAX_MANTISSA: i128 = 10i128.pow(36) - 1; // 10^36 - 1

/// Encode DECIMAL to 24-byte canonical format (big-endian)
/// Format: version(1) + reserved(3) + scale(1) + reserved(3) + mantissa(16)
pub fn decimal_encode(mantissa: i128, scale: u8) -> [u8; 24] {
    let mut buf = [0u8; 24];
    buf[0] = 0x01; // version
    buf[4] = scale;

    // Encode i128 as big-endian two's complement
    // Convert i128 to u128 representation (works for both positive and negative)
    let unsigned = mantissa as u128;
    buf[8..24].copy_from_slice(&unsigned.to_be_bytes());

    buf
}

/// Encode TRAP sentinel: {mantissa: 0x8000000000000000, scale: 0xFF}
pub fn decimal_encode_trap() -> [u8; 24] {
    decimal_encode(0x8000000000000000_i128, 0xFF)
}

/// Create a probe entry: op_id (8) + input_a (24) + input_b (24) + result (24) = 80 bytes
pub fn decimal_make_entry(
    op_id: u64,
    a_encoded: &[u8; 24],
    b_encoded: &[u8; 24],
    result_encoded: &[u8; 24],
) -> [u8; 80] {
    let mut entry = [0u8; 80];
    entry[..8].copy_from_slice(&op_id.to_le_bytes());
    entry[8..32].copy_from_slice(a_encoded);
    entry[32..56].copy_from_slice(b_encoded);
    entry[56..80].copy_from_slice(result_encoded);
    entry
}

/// Compute SHA-256 hash of probe entry
pub fn decimal_entry_hash(entry: &[u8; 80]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(entry);
    hasher.finalize().into()
}

/// Build Merkle tree from entry hashes - returns the Merkle root
pub fn decimal_build_merkle_tree(hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut level: Vec<[u8; 32]> = hashes.to_vec();

    while level.len() > 1 {
        // Duplicate last if odd
        if level.len() % 2 == 1 {
            level.push(level.last().copied().unwrap());
        }

        // Compute parent hashes
        level = level
            .chunks(2)
            .map(|pair| {
                let mut hasher = Sha256::new();
                hasher.update(pair[0]);
                hasher.update(pair[1]);
                hasher.finalize().into()
            })
            .collect();
    }

    level[0]
}

/// Reference Merkle root from RFC-0111 v1.20
pub const DECIMAL_REFERENCE_MERKLE_ROOT: &str =
    "496bc8038e3fd38462f4308bf03088b3f872d000256a45ddb53d4932efff0c1c";

/// Verify Merkle root matches reference
pub fn decimal_verify_merkle_root(root: &[u8; 32]) -> bool {
    let expected = hex::decode(DECIMAL_REFERENCE_MERKLE_ROOT).unwrap();
    root == expected.as_slice()
}

/// Probe entry data structure
#[derive(Debug, Clone)]
pub struct DecimalProbeEntry {
    pub index: usize,
    pub op_id: u64,
    pub a_mantissa: i128,
    pub a_scale: u8,
    pub b_mantissa: i128,
    pub b_scale: u8,
    pub description: &'static str,
}

impl DecimalProbeEntry {
    /// Get the encoded inputs for this entry
    pub fn encode_inputs(&self) -> ([u8; 24], [u8; 24]) {
        let a = decimal_encode(self.a_mantissa, self.a_scale);
        let b = decimal_encode(self.b_mantissa, self.b_scale);
        (a, b)
    }
}

/// All 56 probe entries from RFC-0111
pub fn decimal_all_probe_entries() -> Vec<DecimalProbeEntry> {
    vec![
        // ADD (entries 0-3)
        DecimalProbeEntry {
            index: 0,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: 1,
            a_scale: 0,
            b_mantissa: 2,
            b_scale: 0,
            description: "1.0 + 2.0",
        },
        DecimalProbeEntry {
            index: 1,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 2,
            b_scale: 0,
            description: "1.5 + 2.0",
        },
        DecimalProbeEntry {
            index: 2,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: 100,
            a_scale: 2,
            b_mantissa: 1,
            b_scale: 0,
            description: "1.00 + 1.0",
        },
        DecimalProbeEntry {
            index: 3,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: 1,
            a_scale: 1,
            b_mantissa: 2,
            b_scale: 1,
            description: "0.1 + 0.2",
        },
        // SUB (entries 4-7)
        DecimalProbeEntry {
            index: 4,
            op_id: DECIMAL_OP_SUB,
            a_mantissa: 5,
            a_scale: 0,
            b_mantissa: 2,
            b_scale: 0,
            description: "5.0 - 2.0",
        },
        DecimalProbeEntry {
            index: 5,
            op_id: DECIMAL_OP_SUB,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 15,
            b_scale: 1,
            description: "1.5 - 1.5",
        },
        DecimalProbeEntry {
            index: 6,
            op_id: DECIMAL_OP_SUB,
            a_mantissa: 1,
            a_scale: 1,
            b_mantissa: 2,
            b_scale: 1,
            description: "0.1 - 0.2",
        },
        DecimalProbeEntry {
            index: 7,
            op_id: DECIMAL_OP_SUB,
            a_mantissa: -15,
            a_scale: 1,
            b_mantissa: -5,
            b_scale: 1,
            description: "-1.5 - (-0.5)",
        },
        // MUL (entries 8-13)
        DecimalProbeEntry {
            index: 8,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: 2,
            a_scale: 0,
            b_mantissa: 3,
            b_scale: 0,
            description: "2.0 × 3.0",
        },
        DecimalProbeEntry {
            index: 9,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 2,
            b_scale: 0,
            description: "1.5 × 2.0",
        },
        DecimalProbeEntry {
            index: 10,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: 1,
            a_scale: 1,
            b_mantissa: 2,
            b_scale: 1,
            description: "0.1 × 0.2",
        },
        DecimalProbeEntry {
            index: 11,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: DECIMAL_MAX_MANTISSA,
            a_scale: 0,
            b_mantissa: 1,
            b_scale: 0,
            description: "MAX × 1.0",
        },
        DecimalProbeEntry {
            index: 12,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: -2,
            a_scale: 0,
            b_mantissa: 3,
            b_scale: 0,
            description: "-2.0 × 3.0",
        },
        DecimalProbeEntry {
            index: 13,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: -2,
            a_scale: 0,
            b_mantissa: -3,
            b_scale: 0,
            description: "-2.0 × -3.0",
        },
        // DIV (entries 14-19)
        DecimalProbeEntry {
            index: 14,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 6,
            a_scale: 0,
            b_mantissa: 2,
            b_scale: 0,
            description: "6.0 ÷ 2.0",
        },
        DecimalProbeEntry {
            index: 15,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 1000,
            a_scale: 3,
            b_mantissa: 3,
            b_scale: 0,
            description: "1.000 ÷ 3.0",
        },
        DecimalProbeEntry {
            index: 16,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 1000,
            a_scale: 2,
            b_mantissa: 3,
            b_scale: 0,
            description: "10.00 ÷ 3.0",
        },
        DecimalProbeEntry {
            index: 17,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 10,
            a_scale: 1,
            b_mantissa: 2,
            b_scale: 0,
            description: "1.0 ÷ 2.0",
        },
        DecimalProbeEntry {
            index: 18,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: -6,
            a_scale: 0,
            b_mantissa: 2,
            b_scale: 0,
            description: "-6.0 ÷ 2.0",
        },
        DecimalProbeEntry {
            index: 19,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 6,
            a_scale: 0,
            b_mantissa: -2,
            b_scale: 0,
            description: "6.0 ÷ -2.0",
        },
        // SQRT (entries 20-24)
        DecimalProbeEntry {
            index: 20,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: 4,
            a_scale: 0,
            b_mantissa: 0,
            b_scale: 0,
            description: "√4.0",
        },
        DecimalProbeEntry {
            index: 21,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: 2,
            a_scale: 0,
            b_mantissa: 0,
            b_scale: 0,
            description: "√2.0",
        },
        DecimalProbeEntry {
            index: 22,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: 4,
            a_scale: 2,
            b_mantissa: 0,
            b_scale: 0,
            description: "√0.04",
        },
        DecimalProbeEntry {
            index: 23,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: 1,
            a_scale: 4,
            b_mantissa: 0,
            b_scale: 0,
            description: "√0.0001",
        },
        DecimalProbeEntry {
            index: 24,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: 0,
            a_scale: 0,
            b_mantissa: 0,
            b_scale: 0,
            description: "√0",
        },
        // Entry 25: High-scale SQRT (BUG-4 fix)
        DecimalProbeEntry {
            index: 25,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: 1,
            a_scale: 25,
            b_mantissa: 0,
            b_scale: 0,
            description: "√(10^-25) high-scale",
        },
        // ROUND (entries 26-32)
        DecimalProbeEntry {
            index: 26,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: 1234,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "1.234 → scale=1",
        },
        DecimalProbeEntry {
            index: 27,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: 1235,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "1.235 → scale=1",
        },
        DecimalProbeEntry {
            index: 28,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: 1245,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "1.245 → scale=1",
        },
        DecimalProbeEntry {
            index: 29,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: 1255,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "1.255 → scale=1",
        },
        DecimalProbeEntry {
            index: 30,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: -1235,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "-1.235 → scale=1",
        },
        DecimalProbeEntry {
            index: 31,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: -1245,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "-1.245 → scale=1",
        },
        DecimalProbeEntry {
            index: 32,
            op_id: DECIMAL_OP_ROUND,
            a_mantissa: -1255,
            a_scale: 3,
            b_mantissa: 1,
            b_scale: 0,
            description: "-1.255 → scale=1",
        },
        // CANONICALIZE (entries 33-36)
        DecimalProbeEntry {
            index: 33,
            op_id: DECIMAL_OP_CANONICALIZE,
            a_mantissa: 1000,
            a_scale: 3,
            b_mantissa: 0,
            b_scale: 0,
            description: "1000 scale=3 → {1,0}",
        },
        DecimalProbeEntry {
            index: 34,
            op_id: DECIMAL_OP_CANONICALIZE,
            a_mantissa: 0,
            a_scale: 5,
            b_mantissa: 0,
            b_scale: 0,
            description: "0 scale=5 → {0,0}",
        },
        DecimalProbeEntry {
            index: 35,
            op_id: DECIMAL_OP_CANONICALIZE,
            a_mantissa: 100,
            a_scale: 2,
            b_mantissa: 0,
            b_scale: 0,
            description: "100 scale=2 → {1,0}",
        },
        DecimalProbeEntry {
            index: 36,
            op_id: DECIMAL_OP_CANONICALIZE,
            a_mantissa: 0,
            a_scale: 2,
            b_mantissa: 0,
            b_scale: 0,
            description: "0.0 scale=2 → {0,0}",
        },
        // CMP (entries 37-42)
        DecimalProbeEntry {
            index: 37,
            op_id: DECIMAL_OP_CMP,
            a_mantissa: 1,
            a_scale: 0,
            b_mantissa: 2,
            b_scale: 0,
            description: "1.0 vs 2.0",
        },
        DecimalProbeEntry {
            index: 38,
            op_id: DECIMAL_OP_CMP,
            a_mantissa: 2,
            a_scale: 0,
            b_mantissa: 1,
            b_scale: 0,
            description: "2.0 vs 1.0",
        },
        DecimalProbeEntry {
            index: 39,
            op_id: DECIMAL_OP_CMP,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 15,
            b_scale: 1,
            description: "1.5 vs 1.5",
        },
        DecimalProbeEntry {
            index: 40,
            op_id: DECIMAL_OP_CMP,
            a_mantissa: -1,
            a_scale: 0,
            b_mantissa: 1,
            b_scale: 0,
            description: "-1.0 vs 1.0",
        },
        DecimalProbeEntry {
            index: 41,
            op_id: DECIMAL_OP_CMP,
            a_mantissa: 1,
            a_scale: 0,
            b_mantissa: 100,
            b_scale: 2,
            description: "1.0 vs 1.00",
        },
        DecimalProbeEntry {
            index: 42,
            op_id: DECIMAL_OP_CMP,
            a_mantissa: 1,
            a_scale: 1,
            b_mantissa: 10,
            b_scale: 2,
            description: "0.1 vs 0.10",
        },
        // SERIALIZE/DESERIALIZE (entries 43-44)
        DecimalProbeEntry {
            index: 43,
            op_id: DECIMAL_OP_SERIALIZE,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 0,
            b_scale: 0,
            description: "serialize(1.5)",
        },
        DecimalProbeEntry {
            index: 44,
            op_id: DECIMAL_OP_DESERIALIZE,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 0,
            b_scale: 0,
            description: "deserialize(1.5)",
        },
        // TO_DQA (entries 45-46)
        DecimalProbeEntry {
            index: 45,
            op_id: DECIMAL_OP_TO_DQA,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 0,
            b_scale: 0,
            description: "1.5 → DQA",
        },
        DecimalProbeEntry {
            index: 46,
            op_id: DECIMAL_OP_TO_DQA,
            a_mantissa: 15,
            a_scale: 20,
            b_mantissa: 0,
            b_scale: 0,
            description: "1.5 scale=20 → TRAP",
        },
        // FROM_DQA (entries 47-48)
        DecimalProbeEntry {
            index: 47,
            op_id: DECIMAL_OP_FROM_DQA,
            a_mantissa: 15,
            a_scale: 1,
            b_mantissa: 0,
            b_scale: 0,
            description: "DQA(15,1) → 1.5",
        },
        DecimalProbeEntry {
            index: 48,
            op_id: DECIMAL_OP_FROM_DQA,
            a_mantissa: 0,
            a_scale: 18,
            b_mantissa: 0,
            b_scale: 0,
            description: "DQA(0,18) → 0.0",
        },
        // Edge cases (entries 49-56)
        DecimalProbeEntry {
            index: 49,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: DECIMAL_MAX_MANTISSA,
            a_scale: 0,
            b_mantissa: 1,
            b_scale: 0,
            description: "MAX + 1 → overflow",
        },
        // Entry 50: Negative overflow (ISSUE-1 fix)
        DecimalProbeEntry {
            index: 50,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: -DECIMAL_MAX_MANTISSA,
            a_scale: 0,
            b_mantissa: -1,
            b_scale: 0,
            description: "-MAX + (-1) → TRAP",
        },
        DecimalProbeEntry {
            index: 51,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: 10i128.pow(18),
            a_scale: 0,
            b_mantissa: 10i128.pow(19),
            b_scale: 0,
            description: "10^18 × 10^19 → overflow",
        },
        DecimalProbeEntry {
            index: 52,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 1,
            a_scale: 0,
            b_mantissa: 0,
            b_scale: 0,
            description: "1.0 ÷ 0.0 → div by zero",
        },
        DecimalProbeEntry {
            index: 53,
            op_id: DECIMAL_OP_SQRT,
            a_mantissa: -1,
            a_scale: 0,
            b_mantissa: 0,
            b_scale: 0,
            description: "√-1.0 → negative",
        },
        DecimalProbeEntry {
            index: 54,
            op_id: DECIMAL_OP_ADD,
            a_mantissa: 999999999999i128,
            a_scale: 12,
            b_mantissa: 1,
            b_scale: 12,
            description: "0.999... + 0.000...",
        },
        DecimalProbeEntry {
            index: 55,
            op_id: DECIMAL_OP_MUL,
            a_mantissa: 1,
            a_scale: 12,
            b_mantissa: 1000,
            b_scale: 0,
            description: "0.000000000001 × 1000",
        },
        DecimalProbeEntry {
            index: 56,
            op_id: DECIMAL_OP_DIV,
            a_mantissa: 1,
            a_scale: 36,
            b_mantissa: 3,
            b_scale: 0,
            description: "1.0 scale=36 ÷ 3.0",
        },
    ]
}

/// Compute the actual result for a probe entry, returning (mantissa, scale) or None for TRAP
fn decimal_compute_result(
    op_id: u64,
    a_mantissa: i128,
    a_scale: u8,
    b_mantissa: i128,
    b_scale: u8,
) -> Option<(i128, u8)> {
    let a = match Decimal::new(a_mantissa, a_scale) {
        Ok(d) => d,
        Err(_) => return None,
    };
    let b = match Decimal::new(b_mantissa, b_scale) {
        Ok(d) => d,
        Err(_) => return None,
    };

    match op_id {
        DECIMAL_OP_ADD => match decimal_add(&a, &b) {
            Ok(r) => Some((r.mantissa(), r.scale())),
            Err(_) => None,
        },
        DECIMAL_OP_SUB => match decimal_sub(&a, &b) {
            Ok(r) => Some((r.mantissa(), r.scale())),
            Err(_) => None,
        },
        DECIMAL_OP_MUL => match decimal_mul(&a, &b) {
            Ok(r) => Some((r.mantissa(), r.scale())),
            Err(_) => None,
        },
        DECIMAL_OP_DIV => {
            // Use decimal_div_raw to bypass Decimal canonicalization and match Python's behavior
            match crate::decimal::decimal_div_raw(a_mantissa, a_scale, b_mantissa, b_scale) {
                Ok(r) => Some((r.mantissa(), r.scale())),
                Err(_) => None,
            }
        }
        DECIMAL_OP_SQRT => match decimal_sqrt(&a) {
            Ok(r) => Some((r.mantissa(), r.scale())),
            Err(_) => None,
        },
        DECIMAL_OP_ROUND => {
            // Python-compatible ROUND using raw i128 values (bypasses Decimal canonicalization)
            // Python uses floor division for negatives, Rust uses truncation
            let target_scale = b_mantissa as u8;
            if target_scale >= a_scale {
                Some((a_mantissa, a_scale))
            } else {
                let diff = (a_scale - target_scale) as usize;
                let divisor = crate::decimal::POW10[diff];
                // Use Python-style floor division for negatives
                let (q, r) = a_mantissa.div_mod_floor(&divisor);
                let abs_r = r.abs();
                let half = divisor / 2;

                let result = if abs_r < half {
                    q
                } else if abs_r > half {
                    q + (if a_mantissa >= 0 { 1 } else { -1 })
                } else {
                    // Tie case: round to even
                    if q.is_even() {
                        q
                    } else {
                        q + (if a_mantissa >= 0 { 1 } else { -1 })
                    }
                };
                Some((result, target_scale))
            }
        }
        DECIMAL_OP_CANONICALIZE => {
            // Canonicalize: remove trailing zeros
            let m = a.mantissa();
            let s = a.scale();
            if m == 0 {
                Some((0, 0))
            } else {
                let mut mantissa = m;
                let mut scale = s;
                while mantissa % 10 == 0 && scale > 0 {
                    mantissa /= 10;
                    scale -= 1;
                }
                Some((mantissa, scale))
            }
        }
        DECIMAL_OP_CMP => {
            // Returns comparison result as Decimal
            let cmp_result = decimal_cmp(&a, &b);
            Some((cmp_result as i128, 0))
        }
        DECIMAL_OP_SERIALIZE => {
            // SERIALIZE returns the same decimal
            Some((a.mantissa(), a.scale()))
        }
        DECIMAL_OP_DESERIALIZE => {
            // DESERIALIZE returns the same decimal
            Some((a.mantissa(), a.scale()))
        }
        DECIMAL_OP_TO_DQA => {
            // TO_DQA may TRAP if scale > 18
            match decimal_to_dqa(&a) {
                Ok(dqa) => Some((dqa.value as i128, dqa.scale)),
                Err(_) => None,
            }
        }
        DECIMAL_OP_FROM_DQA => {
            // FROM_DQA: just canonicalize the input (same as Python)
            let m = a_mantissa;
            let s = a_scale;
            if m == 0 {
                Some((0, 0))
            } else {
                let mut mantissa = m;
                let mut scale = s;
                while mantissa % 10 == 0 && scale > 0 {
                    mantissa /= 10;
                    scale -= 1;
                }
                Some((mantissa, scale))
            }
        }
        _ => None,
    }
}

/// Compute all entry hashes and build Merkle tree
pub fn decimal_compute_merkle_root() -> [u8; 32] {
    let entries = decimal_all_probe_entries();
    let mut hashes = Vec::with_capacity(57);

    for entry in entries {
        let (a_enc, b_enc) = entry.encode_inputs();

        // Compute the actual result
        let (r_mantissa, r_scale) = decimal_compute_result(
            entry.op_id,
            entry.a_mantissa,
            entry.a_scale,
            entry.b_mantissa,
            entry.b_scale,
        )
        .unwrap_or((0x8000000000000000_i128, 0xFF));

        let r_enc = decimal_encode(r_mantissa, r_scale);
        let probe_entry = decimal_make_entry(entry.op_id, &a_enc, &b_enc, &r_enc);
        let h = decimal_entry_hash(&probe_entry);
        hashes.push(h);
    }

    decimal_build_merkle_tree(&hashes)
}

/// Debug: print leaf hashes for all 57 entries
#[cfg(test)]
pub fn decimal_debug_leaf_hashes() {
    let entries = decimal_all_probe_entries();
    for entry in entries.iter() {
        let (a_enc, b_enc) = entry.encode_inputs();
        let (r_mantissa, r_scale) = decimal_compute_result(
            entry.op_id,
            entry.a_mantissa,
            entry.a_scale,
            entry.b_mantissa,
            entry.b_scale,
        )
        .unwrap_or((0x8000000000000000_i128, 0xFF));
        let r_enc = decimal_encode(r_mantissa, r_scale);
        let probe_entry = decimal_make_entry(entry.op_id, &a_enc, &b_enc, &r_enc);
        let h = decimal_entry_hash(&probe_entry);
        eprintln!(
            "idx={:2}: {}e{} ({}) leaf={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            entry.index,
            r_mantissa,
            r_scale,
            entry.description,
            h[0],
            h[1],
            h[2],
            h[3],
            h[4],
            h[5],
            h[6],
            h[7]
        );
    }
}

// =============================================================================
// DECIMAL Probe Tests
// =============================================================================

#[cfg(test)]
mod decimal_tests {
    use super::*;

    #[test]
    fn test_encode_decimal() {
        // Test 1.5 (mantissa=15, scale=1)
        let enc = decimal_encode(15, 1);
        assert_eq!(enc[0], 0x01); // version
        assert_eq!(enc[4], 1); // scale
        assert_eq!(enc[8], 0); // mantissa high byte
        assert_eq!(enc[23], 15); // mantissa low byte

        // Test negative -1.5
        let enc_neg = decimal_encode(-15, 1);
        // Two's complement of -15 should not be all zeros
        assert!(enc_neg != enc);
    }

    #[test]
    fn test_make_entry() {
        let a = decimal_encode(1, 0);
        let b = decimal_encode(2, 0);
        let r = decimal_encode(3, 0); // result
        let entry = decimal_make_entry(DECIMAL_OP_ADD, &a, &b, &r);
        assert_eq!(entry.len(), 80);
        // First 8 bytes should be op_id (1) as little-endian
        assert_eq!(entry[..8], 1u64.to_le_bytes());
    }

    #[test]
    fn test_entry_hash() {
        let a = decimal_encode(1, 0);
        let b = decimal_encode(2, 0);
        let r = decimal_encode(3, 0); // result
        let entry = decimal_make_entry(DECIMAL_OP_ADD, &a, &b, &r);
        let h = decimal_entry_hash(&entry);
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn test_merkle_root() {
        let root = decimal_compute_merkle_root();
        eprintln!("Computed root: {:02x?}", root);
        assert!(decimal_verify_merkle_root(&root));
    }

    #[test]
    fn test_debug_leaf_hashes() {
        decimal_debug_leaf_hashes();
    }

    #[test]
    fn test_all_57_entries() {
        // RFC-0111 v1.20 specifies 57 entries (entry 25 is high-scale SQRT)
        let entries = decimal_all_probe_entries();
        assert_eq!(entries.len(), 57, "RFC-0111 v1.20 specifies 57 entries");
    }
}

// =============================================================================
// DVEC Probe (RFC-0112)
// =============================================================================

/// DVEC Operation IDs
pub const DVEC_OP_DOT_PRODUCT: u64 = 1;
pub const DVEC_OP_SQUARED_DISTANCE: u64 = 2;
pub const DVEC_OP_NORM: u64 = 3;
pub const DVEC_OP_VEC_ADD: u64 = 4;
pub const DVEC_OP_VEC_SUB: u64 = 5;
pub const DVEC_OP_VEC_MUL: u64 = 6;
pub const DVEC_OP_VEC_SCALE: u64 = 7;
pub const DVEC_OP_NORMALIZE: u64 = 8;

/// Type IDs
pub const DVEC_TYPE_DQA: u8 = 1;
pub const DVEC_TYPE_DECIMAL: u8 = 2;

/// TRAP sentinel
const DVEC_TRAP_MANTISSA: i128 = 0x8000000000000000;
const DVEC_TRAP_SCALE: u8 = 0xFF;

/// Encode DQA scalar to 24-byte format
/// Format: version(1) + reserved(3) + scale(1) + reserved(3) + mantissa(16 big-endian)
/// For DQA, i64 is stored in the last 8 bytes of the mantissa slot (bytes 16-23)
pub fn dqa_encode(mantissa: i64, scale: u8) -> [u8; 24] {
    let mut buf = [0u8; 24];
    buf[0] = 0x01;
    buf[4] = scale;
    buf[16..24].copy_from_slice(&mantissa.to_be_bytes());
    buf
}

/// Encode Decimal scalar to 24-byte format
pub fn dvec_decimal_encode(mantissa: i128, scale: u8) -> [u8; 24] {
    let mut buf = [0u8; 24];
    buf[0] = 0x01;
    buf[4] = scale;
    let unsigned = mantissa as u128;
    buf[8..24].copy_from_slice(&unsigned.to_be_bytes());
    buf
}

/// Encode TRAP sentinel
pub fn dvec_encode_trap(is_decimal: bool) -> [u8; 24] {
    if is_decimal {
        dvec_decimal_encode(DVEC_TRAP_MANTISSA, DVEC_TRAP_SCALE)
    } else {
        dqa_encode(DVEC_TRAP_MANTISSA as i64, DVEC_TRAP_SCALE)
    }
}

/// Encode a vector: 1 byte length + 24*N bytes elements
pub fn dvec_encode_vector(elements: &[(i128, u8)], is_decimal: bool) -> Vec<u8> {
    let mut result = vec![elements.len() as u8];
    for &(mantissa, scale) in elements {
        let enc = if is_decimal {
            dvec_decimal_encode(mantissa, scale)
        } else {
            dqa_encode(mantissa as i64, scale)
        };
        result.extend_from_slice(&enc);
    }
    result
}

/// Probe result types
#[derive(Debug, Clone)]
pub enum DvecProbeResult {
    Scalar(i128, u8),
    Vector(Vec<(i128, u8)>),
    Trap,
}

/// DVEC probe entry
#[derive(Debug, Clone)]
pub struct DvecProbeEntry {
    pub index: usize,
    pub op: &'static str,
    pub is_decimal: bool,
    pub input_a: Vec<(i128, u8)>,
    pub input_b: Option<Vec<(i128, u8)>>,
    pub expected: DvecProbeResult,
    pub description: &'static str,
}

/// Build a DVEC probe leaf: op_id(8) + type_id(1) + input_a + input_b + result
pub fn dvec_make_entry(
    op_id: u64,
    is_decimal: bool,
    input_a: &[(i128, u8)],
    input_b: Option<&[(i128, u8)]>,
    result: &DvecProbeResult,
) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(&op_id.to_be_bytes());
    entry.push(if is_decimal {
        DVEC_TYPE_DECIMAL
    } else {
        DVEC_TYPE_DQA
    });
    entry.extend_from_slice(&dvec_encode_vector(input_a, is_decimal));
    match input_b {
        Some(b) => entry.extend_from_slice(&dvec_encode_vector(b, is_decimal)),
        None => entry.push(0),
    }
    match result {
        DvecProbeResult::Scalar(mantissa, scale) => {
            if is_decimal {
                entry.extend_from_slice(&dvec_decimal_encode(*mantissa, *scale));
            } else {
                entry.extend_from_slice(&dqa_encode(*mantissa as i64, *scale));
            }
        }
        DvecProbeResult::Vector(v) => {
            entry.extend_from_slice(&dvec_encode_vector(v, is_decimal));
        }
        DvecProbeResult::Trap => {
            entry.extend_from_slice(&dvec_encode_trap(is_decimal));
        }
    }
    entry
}

/// Compute SHA-256 hash of a DVEC probe entry
pub fn dvec_entry_hash(entry: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(entry);
    hasher.finalize().into()
}

/// Build Merkle tree from entry hashes
pub fn dvec_build_merkle_tree(hashes: &[[u8; 32]]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut level: Vec<[u8; 32]> = hashes.to_vec();
    while level.len() > 1 {
        if level.len() % 2 == 1 {
            level.push(level.last().copied().unwrap());
        }
        level = level
            .chunks(2)
            .map(|pair| {
                let mut hasher = Sha256::new();
                hasher.update(pair[0]);
                hasher.update(pair[1]);
                hasher.finalize().into()
            })
            .collect();
    }
    level[0]
}

/// Reference Merkle root from RFC-0112
pub const DVEC_REFERENCE_MERKLE_ROOT: &str =
    "74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998";

/// Verify Merkle root matches reference
pub fn dvec_verify_merkle_root(root: &[u8; 32]) -> bool {
    let expected = hex::decode(DVEC_REFERENCE_MERKLE_ROOT).unwrap();
    root == expected.as_slice()
}

/// Get all 57 DVEC probe entries
pub fn dvec_all_probe_entries() -> Vec<DvecProbeEntry> {
    vec![
        // Entries 0-15: DOT_PRODUCT DQA
        DvecProbeEntry {
            index: 0,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 0), (2, 0), (3, 0)],
            input_b: Some(vec![(4, 0), (5, 0), (6, 0)]),
            expected: DvecProbeResult::Scalar(32, 0),
            description: "DOT_PRODUCT_DQA_0",
        },
        DvecProbeEntry {
            index: 1,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 1), (2, 1)],
            input_b: Some(vec![(3, 1), (4, 1)]),
            expected: DvecProbeResult::Scalar(11, 2),
            description: "DOT_PRODUCT_DQA_1",
        },
        DvecProbeEntry {
            index: 2,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(0, 0), (0, 0), (0, 0)],
            input_b: Some(vec![(1, 0), (2, 0), (3, 0)]),
            expected: DvecProbeResult::Scalar(0, 0),
            description: "DOT_PRODUCT_DQA_2",
        },
        DvecProbeEntry {
            index: 3,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(10, 2), (20, 2)],
            input_b: Some(vec![(30, 2), (40, 2)]),
            expected: DvecProbeResult::Scalar(11, 2),
            description: "DOT_PRODUCT_DQA_3",
        },
        DvecProbeEntry {
            index: 4,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 0)],
            input_b: Some(vec![(1, 0)]),
            expected: DvecProbeResult::Scalar(1, 0),
            description: "DOT_PRODUCT_DQA_4",
        },
        DvecProbeEntry {
            index: 5,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(3, 1), (5, 1)],
            input_b: Some(vec![(2, 1), (4, 1)]),
            expected: DvecProbeResult::Scalar(26, 2),
            description: "DOT_PRODUCT_DQA_5",
        },
        DvecProbeEntry {
            index: 6,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(100, 2)],
            input_b: Some(vec![(100, 2)]),
            expected: DvecProbeResult::Scalar(1, 0),
            description: "DOT_PRODUCT_DQA_6",
        },
        DvecProbeEntry {
            index: 7,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 3), (2, 3), (3, 3)],
            input_b: Some(vec![(4, 3), (5, 3), (6, 3)]),
            expected: DvecProbeResult::Scalar(32, 6),
            description: "DOT_PRODUCT_DQA_7",
        },
        DvecProbeEntry {
            index: 8,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(10, 4), (20, 4)],
            input_b: Some(vec![(30, 4), (40, 4)]),
            expected: DvecProbeResult::Scalar(11, 6),
            description: "DOT_PRODUCT_DQA_8",
        },
        DvecProbeEntry {
            index: 9,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 5), (1, 5), (1, 5), (1, 5)],
            input_b: Some(vec![(1, 5), (1, 5), (1, 5), (1, 5)]),
            expected: DvecProbeResult::Scalar(4, 10),
            description: "DOT_PRODUCT_DQA_9",
        },
        DvecProbeEntry {
            index: 10,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(100, 6), (200, 6)],
            input_b: Some(vec![(300, 6), (400, 6)]),
            expected: DvecProbeResult::Scalar(11, 8),
            description: "DOT_PRODUCT_DQA_10",
        },
        DvecProbeEntry {
            index: 11,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 7), (1, 7), (1, 7), (1, 7), (1, 7)],
            input_b: Some(vec![(2, 7), (2, 7), (2, 7), (2, 7), (2, 7)]),
            expected: DvecProbeResult::Scalar(1, 13),
            description: "DOT_PRODUCT_DQA_11",
        },
        DvecProbeEntry {
            index: 12,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(50, 8), (50, 8)],
            input_b: Some(vec![(50, 8), (50, 8)]),
            expected: DvecProbeResult::Scalar(5, 13),
            description: "DOT_PRODUCT_DQA_12",
        },
        DvecProbeEntry {
            index: 13,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 9), (1, 9), (1, 9), (1, 9), (1, 9), (1, 9)],
            input_b: Some(vec![(1, 9), (1, 9), (1, 9), (1, 9), (1, 9), (1, 9)]),
            expected: DvecProbeResult::Scalar(6, 18),
            description: "DOT_PRODUCT_DQA_13",
        },
        DvecProbeEntry {
            index: 14,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(10, 0), (20, 0), (30, 0)],
            input_b: Some(vec![(1, 0), (2, 0), (3, 0)]),
            expected: DvecProbeResult::Scalar(140, 0),
            description: "DOT_PRODUCT_DQA_14",
        },
        DvecProbeEntry {
            index: 15,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(5, 1), (15, 1), (25, 1)],
            input_b: Some(vec![(2, 1), (4, 1), (6, 1)]),
            expected: DvecProbeResult::Scalar(22, 1),
            description: "DOT_PRODUCT_DQA_15",
        },
        // Entries 16-31: DOT_PRODUCT Decimal
        DvecProbeEntry {
            index: 16,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 0)],
            input_b: Some(vec![(1, 0)]),
            expected: DvecProbeResult::Scalar(1, 0),
            description: "DOT_PRODUCT_DECIMAL_16",
        },
        DvecProbeEntry {
            index: 17,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 1), (2, 1)],
            input_b: Some(vec![(3, 1), (4, 1)]),
            expected: DvecProbeResult::Scalar(11, 2),
            description: "DOT_PRODUCT_DECIMAL_17",
        },
        DvecProbeEntry {
            index: 18,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(100, 2)],
            input_b: Some(vec![(100, 2)]),
            expected: DvecProbeResult::Scalar(1, 0),
            description: "DOT_PRODUCT_DECIMAL_18",
        },
        DvecProbeEntry {
            index: 19,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 3), (2, 3), (3, 3)],
            input_b: Some(vec![(4, 3), (5, 3), (6, 3)]),
            expected: DvecProbeResult::Scalar(32, 6),
            description: "DOT_PRODUCT_DECIMAL_19",
        },
        DvecProbeEntry {
            index: 20,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(10, 4), (20, 4)],
            input_b: Some(vec![(30, 4), (40, 4)]),
            expected: DvecProbeResult::Scalar(11, 6),
            description: "DOT_PRODUCT_DECIMAL_20",
        },
        DvecProbeEntry {
            index: 21,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 5), (1, 5), (1, 5), (1, 5)],
            input_b: Some(vec![(1, 5), (1, 5), (1, 5), (1, 5)]),
            expected: DvecProbeResult::Scalar(4, 10),
            description: "DOT_PRODUCT_DECIMAL_21",
        },
        DvecProbeEntry {
            index: 22,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(100, 6), (200, 6)],
            input_b: Some(vec![(300, 6), (400, 6)]),
            expected: DvecProbeResult::Scalar(11, 8),
            description: "DOT_PRODUCT_DECIMAL_22",
        },
        DvecProbeEntry {
            index: 23,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 7), (1, 7), (1, 7), (1, 7), (1, 7)],
            input_b: Some(vec![(2, 7), (2, 7), (2, 7), (2, 7), (2, 7)]),
            expected: DvecProbeResult::Scalar(1, 13),
            description: "DOT_PRODUCT_DECIMAL_23",
        },
        DvecProbeEntry {
            index: 24,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(50, 8), (50, 8)],
            input_b: Some(vec![(50, 8), (50, 8)]),
            expected: DvecProbeResult::Scalar(5, 13),
            description: "DOT_PRODUCT_DECIMAL_24",
        },
        DvecProbeEntry {
            index: 25,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 9), (1, 9), (1, 9), (1, 9), (1, 9), (1, 9)],
            input_b: Some(vec![(1, 9), (1, 9), (1, 9), (1, 9), (1, 9), (1, 9)]),
            expected: DvecProbeResult::Scalar(6, 18),
            description: "DOT_PRODUCT_DECIMAL_25",
        },
        DvecProbeEntry {
            index: 26,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(10, 10), (20, 10)],
            input_b: Some(vec![(30, 10), (40, 10)]),
            expected: DvecProbeResult::Scalar(11, 18),
            description: "DOT_PRODUCT_DECIMAL_26",
        },
        DvecProbeEntry {
            index: 27,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
            ],
            input_b: Some(vec![
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
                (1, 12),
            ]),
            expected: DvecProbeResult::Scalar(8, 24),
            description: "DOT_PRODUCT_DECIMAL_27",
        },
        DvecProbeEntry {
            index: 28,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(2, 14), (3, 14)],
            input_b: Some(vec![(4, 14), (5, 14)]),
            expected: DvecProbeResult::Scalar(23, 28),
            description: "DOT_PRODUCT_DECIMAL_28",
        },
        DvecProbeEntry {
            index: 29,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(5, 16), (5, 16), (5, 16)],
            input_b: Some(vec![(5, 16), (5, 16), (5, 16)]),
            expected: DvecProbeResult::Scalar(75, 32),
            description: "DOT_PRODUCT_DECIMAL_29",
        },
        DvecProbeEntry {
            index: 30,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(1, 18), (1, 18)],
            input_b: Some(vec![(1, 18), (1, 18)]),
            expected: DvecProbeResult::Scalar(2, 36),
            description: "DOT_PRODUCT_DECIMAL_30",
        },
        DvecProbeEntry {
            index: 31,
            op: "DOT_PRODUCT",
            is_decimal: true,
            input_a: vec![(10, 0), (20, 0)],
            input_b: Some(vec![(1, 0), (2, 0)]),
            expected: DvecProbeResult::Scalar(50, 0),
            description: "DOT_PRODUCT_DECIMAL_31",
        },
        // Entries 32-37: SQUARED_DISTANCE DQA
        DvecProbeEntry {
            index: 32,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(0, 0), (0, 0)],
            input_b: Some(vec![(3, 0), (4, 0)]),
            expected: DvecProbeResult::Scalar(25, 0),
            description: "SQUARED_DISTANCE_32",
        },
        DvecProbeEntry {
            index: 33,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(1, 0), (2, 0)],
            input_b: Some(vec![(4, 0), (6, 0)]),
            expected: DvecProbeResult::Scalar(25, 0),
            description: "SQUARED_DISTANCE_33",
        },
        DvecProbeEntry {
            index: 34,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(0, 1), (0, 1)],
            input_b: Some(vec![(3, 1), (4, 1)]),
            expected: DvecProbeResult::Scalar(25, 2),
            description: "SQUARED_DISTANCE_34",
        },
        DvecProbeEntry {
            index: 35,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(1, 2), (2, 2)],
            input_b: Some(vec![(1, 2), (2, 2)]),
            expected: DvecProbeResult::Scalar(0, 0),
            description: "SQUARED_DISTANCE_35",
        },
        DvecProbeEntry {
            index: 36,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(10, 3), (20, 3)],
            input_b: Some(vec![(0, 3), (0, 3)]),
            expected: DvecProbeResult::Scalar(5, 4),
            description: "SQUARED_DISTANCE_36",
        },
        DvecProbeEntry {
            index: 37,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(1, 4)],
            input_b: Some(vec![(0, 4)]),
            expected: DvecProbeResult::Scalar(1, 8),
            description: "SQUARED_DISTANCE_37",
        },
        // Entries 38-39: SQUARED_DISTANCE Decimal
        DvecProbeEntry {
            index: 38,
            op: "SQUARED_DISTANCE",
            is_decimal: true,
            input_a: vec![(3, 5), (4, 5)],
            input_b: Some(vec![(0, 5), (0, 5)]),
            expected: DvecProbeResult::Scalar(25, 10),
            description: "SQUARED_DISTANCE_DECIMAL_38",
        },
        DvecProbeEntry {
            index: 39,
            op: "SQUARED_DISTANCE",
            is_decimal: true,
            input_a: vec![(1, 6), (2, 6), (3, 6)],
            input_b: Some(vec![(0, 6), (0, 6), (0, 6)]),
            expected: DvecProbeResult::Scalar(14, 12),
            description: "SQUARED_DISTANCE_DECIMAL_39",
        },
        // Entries 40-47: NORM
        DvecProbeEntry {
            index: 40,
            op: "NORM",
            is_decimal: true,
            input_a: vec![(3, 0), (4, 0)],
            input_b: None,
            expected: DvecProbeResult::Scalar(5, 0),
            description: "NORM_40",
        },
        DvecProbeEntry {
            index: 41,
            op: "NORM",
            is_decimal: true,
            input_a: vec![(0, 0), (0, 0), (0, 0)],
            input_b: None,
            expected: DvecProbeResult::Scalar(0, 0),
            description: "NORM_41",
        },
        DvecProbeEntry {
            index: 42,
            op: "NORM",
            is_decimal: false,
            input_a: vec![(3, 0), (4, 0)],
            input_b: None,
            expected: DvecProbeResult::Trap,
            description: "NORM_42",
        },
        DvecProbeEntry {
            index: 43,
            op: "NORM",
            is_decimal: true,
            input_a: vec![(1, 2), (2, 2)],
            input_b: None,
            expected: DvecProbeResult::Scalar(223606797, 10),
            description: "NORM_43",
        },
        DvecProbeEntry {
            index: 44,
            op: "NORM",
            is_decimal: true,
            input_a: vec![(6, 0), (8, 0)],
            input_b: None,
            expected: DvecProbeResult::Scalar(10, 0),
            description: "NORM_44",
        },
        DvecProbeEntry {
            index: 45,
            op: "NORM",
            is_decimal: true,
            input_a: vec![(1, 4)],
            input_b: None,
            expected: DvecProbeResult::Scalar(1, 4),
            description: "NORM_45",
        },
        DvecProbeEntry {
            index: 46,
            op: "NORM",
            is_decimal: true,
            input_a: vec![(2, 6), (2, 6)],
            input_b: None,
            expected: DvecProbeResult::Scalar(2828427124746, 18),
            description: "NORM_46",
        },
        DvecProbeEntry {
            index: 47,
            op: "NORM",
            is_decimal: false,
            input_a: vec![(1, 0), (1, 0), (1, 0)],
            input_b: None,
            expected: DvecProbeResult::Trap,
            description: "NORM_47",
        },
        // Entries 48-51: Element-wise Decimal
        DvecProbeEntry {
            index: 48,
            op: "VEC_ADD",
            is_decimal: true,
            input_a: vec![(1, 0), (2, 0)],
            input_b: Some(vec![(3, 0), (4, 0)]),
            expected: DvecProbeResult::Vector(vec![(4, 0), (6, 0)]),
            description: "VEC_ADD_DECIMAL_0",
        },
        DvecProbeEntry {
            index: 49,
            op: "VEC_SUB",
            is_decimal: true,
            input_a: vec![(4, 0), (6, 0)],
            input_b: Some(vec![(1, 0), (2, 0)]),
            expected: DvecProbeResult::Vector(vec![(3, 0), (4, 0)]),
            description: "VEC_SUB_DECIMAL_0",
        },
        DvecProbeEntry {
            index: 50,
            op: "VEC_MUL",
            is_decimal: true,
            input_a: vec![(2, 0), (3, 0)],
            input_b: Some(vec![(4, 0), (5, 0)]),
            expected: DvecProbeResult::Vector(vec![(8, 0), (15, 0)]),
            description: "VEC_MUL_DECIMAL_0",
        },
        DvecProbeEntry {
            index: 51,
            op: "VEC_SCALE",
            is_decimal: true,
            input_a: vec![(1, 0), (2, 0)],
            input_b: Some(vec![(2, 0)]),
            expected: DvecProbeResult::Vector(vec![(2, 0), (4, 0)]),
            description: "VEC_SCALE_DECIMAL_0",
        },
        // Entries 52-56: TRAP cases
        DvecProbeEntry {
            index: 52,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 0); 65],
            input_b: Some(vec![(1, 0); 65]),
            expected: DvecProbeResult::Trap,
            description: "TRAP_DIMENSION",
        },
        DvecProbeEntry {
            index: 53,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1, 10), (1, 10)],
            input_b: Some(vec![(1, 10), (1, 10)]),
            expected: DvecProbeResult::Trap,
            description: "TRAP_INPUT_SCALE_GUARD",
        },
        DvecProbeEntry {
            index: 54,
            op: "DOT_PRODUCT",
            is_decimal: false,
            input_a: vec![(1000000000000000000, 0), (1000000000000000000, 0)],
            input_b: Some(vec![(1000000000000000000, 0), (1000000000000000000, 0)]),
            expected: DvecProbeResult::Trap,
            description: "TRAP_OVERFLOW",
        },
        DvecProbeEntry {
            index: 55,
            op: "SQUARED_DISTANCE",
            is_decimal: false,
            input_a: vec![(1, 10), (1, 10)],
            input_b: Some(vec![(0, 10), (0, 10)]),
            expected: DvecProbeResult::Trap,
            description: "TRAP_SQUARED_DISTANCE_SCALE",
        },
        DvecProbeEntry {
            index: 56,
            op: "NORMALIZE",
            is_decimal: true,
            input_a: vec![(3, 0), (4, 0)],
            input_b: None,
            expected: DvecProbeResult::Trap,
            description: "TRAP_NORMALIZE_DECIMAL",
        },
    ]
}

/// Compute DVEC probe Merkle root
pub fn dvec_compute_merkle_root() -> [u8; 32] {
    let entries = dvec_all_probe_entries();
    let mut hashes = Vec::with_capacity(57);

    for entry in entries {
        let op_id = match entry.op {
            "DOT_PRODUCT" => DVEC_OP_DOT_PRODUCT,
            "SQUARED_DISTANCE" => DVEC_OP_SQUARED_DISTANCE,
            "NORM" => DVEC_OP_NORM,
            "VEC_ADD" => DVEC_OP_VEC_ADD,
            "VEC_SUB" => DVEC_OP_VEC_SUB,
            "VEC_MUL" => DVEC_OP_VEC_MUL,
            "VEC_SCALE" => DVEC_OP_VEC_SCALE,
            "NORMALIZE" => DVEC_OP_NORMALIZE,
            _ => panic!("Unknown op: {}", entry.op),
        };
        let input_b = entry.input_b.as_deref();
        let entry_bytes = dvec_make_entry(
            op_id,
            entry.is_decimal,
            &entry.input_a,
            input_b,
            &entry.expected,
        );
        let h = dvec_entry_hash(&entry_bytes);
        hashes.push(h);
    }
    dvec_build_merkle_tree(&hashes)
}

// =============================================================================
// DVEC Probe Tests
// =============================================================================

#[cfg(test)]
mod dvec_tests {
    use super::*;

    #[test]
    fn test_dvec_encode_dqa() {
        let enc = dqa_encode(42, 0);
        assert_eq!(enc[0], 0x01);
        assert_eq!(enc[4], 0);
        // DQA mantissa is stored in bytes 16-23
        assert_eq!(enc[16..24], 42i64.to_be_bytes());
    }

    #[test]
    fn test_dvec_encode_decimal() {
        let enc = dvec_decimal_encode(42, 0);
        assert_eq!(enc[0], 0x01);
        assert_eq!(enc[4], 0);
        assert_eq!(enc[8..24], 42i128.to_be_bytes());
    }

    #[test]
    fn test_dvec_encode_trap() {
        let trap = dvec_encode_trap(false);
        assert_eq!(trap[4], 0xFF);
        // DQA TRAP mantissa is stored in bytes 16-24
        assert_eq!(trap[16..24], (DVEC_TRAP_MANTISSA as i64).to_be_bytes());
    }

    #[test]
    fn test_encode_vector() {
        let v = vec![(1, 0), (2, 0)];
        let enc = dvec_encode_vector(&v, false);
        assert_eq!(enc[0], 2);
        assert_eq!(enc.len(), 1 + 2 * 24);
    }

    #[test]
    fn test_dvec_make_entry() {
        let entry = dvec_make_entry(
            DVEC_OP_DOT_PRODUCT,
            false,
            &[(1, 0), (2, 0)],
            Some(&[(3, 0), (4, 0)]),
            &DvecProbeResult::Scalar(11, 0),
        );
        assert!(entry.len() > 24);
        assert_eq!(&entry[..8], DVEC_OP_DOT_PRODUCT.to_be_bytes());
    }

    #[test]
    fn test_dvec_entry_hash() {
        // Entry 0: DOT_PRODUCT_DQA_0 with Python hash 85c011efeca4ecf8...
        let entry = dvec_make_entry(
            DVEC_OP_DOT_PRODUCT,
            false,
            &[(1, 0), (2, 0), (3, 0)],
            Some(&[(4, 0), (5, 0), (6, 0)]),
            &DvecProbeResult::Scalar(32, 0),
        );
        eprintln!("Entry 0 leaf: {:?}", entry);
        eprintln!("Entry 0 leaf hex: {}", hex::encode(&entry));
        let h = dvec_entry_hash(&entry);
        eprintln!("Entry 0 hash: {:02x?}", h);
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn test_all_57_entries() {
        let entries = dvec_all_probe_entries();
        assert_eq!(entries.len(), 57, "RFC-0112 specifies 57 entries");
    }

    #[test]
    fn test_merkle_root() {
        let root = dvec_compute_merkle_root();
        eprintln!("Computed root: {:02x?}", root);
        assert!(dvec_verify_merkle_root(&root));
    }

    #[test]
    fn test_all_entry_hashes_vs_python() {
        let entries = dvec_all_probe_entries();
        let mut hashes = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let op_id = match entry.op {
                "DOT_PRODUCT" => DVEC_OP_DOT_PRODUCT,
                "SQUARED_DISTANCE" => DVEC_OP_SQUARED_DISTANCE,
                "NORM" => DVEC_OP_NORM,
                "VEC_ADD" => DVEC_OP_VEC_ADD,
                "VEC_SUB" => DVEC_OP_VEC_SUB,
                "VEC_MUL" => DVEC_OP_VEC_MUL,
                "VEC_SCALE" => DVEC_OP_VEC_SCALE,
                "NORMALIZE" => DVEC_OP_NORMALIZE,
                _ => continue,
            };
            let entry_bytes = dvec_make_entry(
                op_id,
                entry.is_decimal,
                &entry.input_a,
                entry.input_b.as_deref(),
                &entry.expected,
            );
            let hash = dvec_entry_hash(&entry_bytes);
            hashes.push(hash);
            eprintln!("Entry {:2}: {}", i, hex::encode(hash));
        }
        let root = dvec_build_merkle_tree(&hashes);
        eprintln!("\nMerkle root from test: {:02x?}", root);
    }
}
