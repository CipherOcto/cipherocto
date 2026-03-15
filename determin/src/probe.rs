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
#[derive(Debug, Clone)]
pub enum BigIntProbeValue {
    /// Integer value
    Int(i128),
    /// BigInt limbs (for CANONICALIZE)
    Limbs(Vec<u64>),
    /// Special sentinel
    Max,
    /// Special sentinel
    Trap,
    /// Hash reference for serialization
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
            description: "2^64 + 1",
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
            "ba5c1357640f1ba5",
            "53afea624a503a0b",
            "78403c84df66c25d",
            "049af6a1bbee3c5a",
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
