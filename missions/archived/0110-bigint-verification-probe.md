# Mission: BigInt Verification Probe

## Status
Archived

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement 56-entry Merkle verification probe for BigInt with deterministic encoding. The probe enables cross-implementation verification and consensus validation.

## Overview

The verification probe is a canonical set of 56 BigInt operations that:
1. Covers all major operations (ADD, SUB, MUL, DIV, MOD, SHL, SHR, CANONICALIZE, CMP, BITLEN, SERIALIZE, DESERIALIZE, I128_ROUNDTRIP)
2. Includes boundary cases (MAX, zero, overflow)
3. Uses deterministic encoding for reproducible verification
4. Merkle tree enables efficient integrity checks

## Phase 1: Probe Encoding

### Acceptance Criteria
- [x] Implement 8-byte compact encoding for probe fields
- [x] Values ≤ 2^56: bytes 0-6 little-endian, byte 7 = 0x00 (positive) or 0x80 (negative)
- [x] Values > 2^56: hash reference (lower 8 bytes of SHA-256)
- [x] Special sentinels: MAX = 0xFFFF_FFFF_FFFF_FFFF, TRAP = 0xDEAD_DEAD_DEAD_DEAD

### Probe Format (RFC-0110 §Canonical Probe Entry Format)
```
┌─────────────────────────────────────────────────────────────┐
│ Probe Entry (24 bytes)                                     │
├─────────────────────────────────────────────────────────────┤
│ bytes 0-7:   operation ID (little-endian u64)             │
│ bytes 8-15:  input A (8-byte encoding)                    │
│ bytes 16-23: input B (8-byte encoding)                    │
└─────────────────────────────────────────────────────────────────────┘
```

### Compact Encoding Rules
```
- Values ≤ 2^56 (MAX_U56 = 0xFFFFFFFFFFFFFF):
  bytes 0-6: little-endian magnitude
  byte 7: 0x00 for positive, 0x80 for negative

- Values > 2^56 (hash reference):
  header = [version: 1, sign_flag: 0/1, 0, 0, num_limbs: n, 0, 0, 0]
  hash = SHA-256(header + limbs)
  Use lower 8 bytes as reference

- Special values:
  MAX: 0xFFFF_FFFF_FFFF_FFFF
  ZERO: 0x0000_0000_0000_0000
  TRAP: 0xDEAD_DEAD_DEAD_DEAD (for overflow results)
```

### Operation IDs (RFC-0110 §Operation IDs)
```
ADD = 1
SUB = 2
MUL = 3
DIV = 4
MOD = 5
SHL = 6
SHR = 7
CANONICALIZE = 8
CMP = 9
BITLEN = 10
SERIALIZE = 11
DESERIALIZE = 12
I128_ROUNDTRIP = 13
```

## Phase 2: Probe Entries

### Acceptance Criteria
- [x] Implement all 56 probe entries
- [x] Match RFC-0110 table exactly
- [x] Handle MAX_BIGINT sentinel correctly
- [x] Handle TRAP results for overflow cases

### Probe Entries Table (RFC-0110 §Probe Entries)

| # | Operation | Input A | Input B | Description |
|---|-----------|---------|---------|-------------|
| 0 | ADD | 0 | 2 | 0 + 2 |
| 1 | ADD | 2^64 | 1 | 64-bit boundary |
| 2 | ADD | MAX_U64 | 1 | 64-bit overflow |
| 3 | ADD | 1 | -1 | Cross-sign |
| 4 | ADD | MAX | MAX | Max + max → TRAP |
| 5 | SUB | -5 | -2 | -5 - (-2) |
| 6 | SUB | 5 | 5 | Equal |
| 7 | SUB | 0 | 0 | Zero - zero |
| 8 | SUB | 1 | -1 | 1 - (-1) |
| 9 | SUB | MAX | 1 | Max - 1 |
| 10 | MUL | 2 | 3 | Basic |
| 11 | MUL | 2^32 | 2^32 | 32-bit × 32-bit |
| 12 | MUL | 0 | 1 | Zero × anything |
| 13 | MUL | MAX_BIGINT | MAX_BIGINT | 64-limb × 64-limb → TRAP |
| 14 | MUL | -3 | 4 | Cross-sign |
| 15 | MUL | -2 | -3 | Negative × negative |
| 16 | DIV | 10 | 3 | Basic |
| 17 | DIV | 100 | 10 | Exact |
| 18 | DIV | MAX | 1 | Max / 1 |
| 19 | DIV | 1 | MAX | 1 / max |
| 20 | DIV | 2^4096 | 2^64 | Large / small |
| 21 | MOD | -7 | 3 | -7 % 3 = -1 |
| 22 | MOD | 10 | 3 | 10 % 3 = 1 |
| 23 | MOD | MAX | 3 | Max % 3 |
| 24 | SHL | 1 | 4095 | Max shift |
| 25 | SHL | 1 | 64 | Limb boundary |
| 26 | SHL | 1 | 1 | Single bit |
| 27 | SHL | MAX | 1 | Max << 1 → TRAP |
| 28 | SHR | 2^4095 | 1 | Large >> 1 |
| 29 | SHR | 2^4095 | 4096 | Large >> max |
| 30 | SHR | 2^4095 | 64 | Large >> limb |
| 31 | SHR | 1 | 0 | Shift zero |
| 32 | CANONICALIZE | [0,0,0] | [0] | Trailing zeros |
| 33 | CANONICALIZE | [5,0,0] | [5] | Multiple zeros |
| 34 | CANONICALIZE | [0] | [0] | Negative zero |
| 35 | CANONICALIZE | [1,0] | [1] | Single trailing |
| 36 | CANONICALIZE | [MAX,0,0] | [MAX] | Max trailing |
| 37 | CMP | -5 | -3 | Both negative |
| 38 | CMP | 0 | 1 | Zero vs pos |
| 39 | CMP | MAX | MAX | Equal |
| 40 | CMP | -1 | 1 | Neg vs pos |
| 41 | CMP | 1 | 2 | Both pos |
| 42 | I128_ROUNDTRIP | 2^127-1 | round-trip | i128::MAX |
| 43 | I128_ROUNDTRIP | -2^127 | round-trip | i128::MIN |
| 44 | I128_ROUNDTRIP | 0 | round-trip | Zero |
| 45 | I128_ROUNDTRIP | 1 | round-trip | One |
| 46 | I128_ROUNDTRIP | -1 | round-trip | Minus one |
| 47 | BITLEN | 0 | 1 | Zero |
| 48 | BITLEN | 1 | 1 | Single bit |
| 49 | BITLEN | MAX | 4096 | Max |
| 50 | BITLEN | 2^63 | 64 | Power of 2 |
| 51 | ADD | MAX_BIGINT | 1 | Overflow TRAP |
| 52 | ADD | 2^64-1 | 1 | Carry |
| 53 | SUB | 0 | 1 | Borrow |
| 54 | SERIALIZE | 1 | hash | Serialize 1 |
| 55 | DESERIALIZE | hash | 1 | Deserialize |

## Phase 3: Merkle Tree

### Acceptance Criteria
- [x] Build Merkle tree from 56 entry hashes
- [x] Use SHA-256 for hashing
- [x] Pairwise combination (if odd, duplicate last)
- [x] Verify root matches reference

### Merkle Root (RFC-0110)
```
Reference Merkle Root:
c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0
```

### Merkle Tree Construction
```
1. For each of 56 entries:
   entry_hash = SHA256(op_id || input_a_encoded || input_b_encoded)

2. While more than 1 hash:
   If odd number of hashes, duplicate last
   For each pair (h1, h2):
     parent = SHA256(h1 || h2)
   Replace level with parent hashes

3. Final hash = Merkle root
```

## Phase 4: Verification Procedures

### Acceptance Criteria
- [x] Two-input verification procedure (ADD, SUB, MUL, DIV, MOD, CMP)
- [x] One-input verification procedure (CANONICALIZE, BITLEN, SHL, SHR)
- [x] Round-trip verification (I128_ROUNDTRIP)
- [x] Serialization verification (SERIALIZE, DESERIALIZE)

### Two-Input Verification Procedure
```
verify_two_input(op, input_a, input_b, expected_output):
  1. Execute: result = op(input_a, input_b)
  2. Encode: probe_entry = encode(op, input_a, input_b)
  3. Hash: entry_hash = SHA256(probe_entry)
  4. Compare: Verify entry_hash in Merkle tree
```

## Implementation Location
- **File**: `determin/src/probe.rs` (new)
- **Script**: `scripts/compute_bigint_probe_root.py` (reference)

## Prerequisites
- Mission 0110-bigint-core-algorithms (complete)
- Mission 0110-bigint-conversions-serialization (complete)

## Dependencies
- sha2 crate for SHA-256
- See `scripts/compute_bigint_probe_root.py` for reference implementation

## Testing Requirements
- All 56 entries must encode without error
- Merkle root must match: c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0
- Verification procedures must work for all entry types

## Reference
- RFC-0110: Deterministic BIGINT (§Verification Probe)
- RFC-0110: Deterministic BIGINT (§Probe Entries)
- RFC-0110: Deterministic BIGINT (§Merkle Hash)
- scripts/compute_bigint_probe_root.py (Python reference implementation)

## Complexity
Medium — Encoding logic requires careful attention to edge cases
