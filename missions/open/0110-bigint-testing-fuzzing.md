# Mission: BigInt Testing & Differential Fuzzing

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement comprehensive test vectors and differential fuzzing harness for BigInt against num-bigint reference. Ensures cross-implementation determinism.

## Phase 1: Unit Test Vectors

### Acceptance Criteria
- [ ] 40+ unit test vectors covering all operations
- [ ] Boundary cases: MAX, zero, negative zero
- [ ] Overflow cases: TRAP on overflow
- [ ] All test vectors from RFC-0110 §Test Vectors

### Basic Operations Tests (RFC-0110 §Basic Operations)
| Input A | Op | Input B | Expected |
|---------|-----|---------|----------|
| 0 | ADD | 2 | 2 |
| 2^64 | ADD | 1 | 2^64 + 1 |
| MAX_U64 | ADD | 1 | Overflow |
| 1 | ADD | -1 | 0 |
| 5 | SUB | 5 | 0 |
| 0 | SUB | 0 | 0 |
| 1 | SUB | -1 | 2 |
| 2 | MUL | 3 | 6 |
| 2^32 | MUL | 2^32 | 2^64 |
| 0 | MUL | 1 | 0 |
| -3 | MUL | 4 | -12 |
| 10 | DIV | 3 | 3 (remainder 1) |
| 100 | DIV | 10 | 10 |
| 10 | MOD | 3 | 1 |
| 1 | SHL | 1 | 2 |
| 1 | SHL | 4095 | Overflow |
| 2^4095 | SHR | 1 | 2^4094 |
| 1 | SHR | 0 | 1 |

### Boundary Cases Tests (RFC-0110 §Boundary Cases)
| Input A | Op | Input B | Expected |
|---------|-----|---------|----------|
| MAX | ADD | MAX | TRAP |
| MAX_BIGINT | ADD | 1 | TRAP |
| 2^64-1 | ADD | 1 | 2^64 |
| 0 | SUB | 1 | -1 |
| MAX_BIGINT | MUL | MAX_BIGINT | TRAP |
| MAX | DIV | 1 | MAX |
| 1 | DIV | MAX | 0 |
| MAX | MOD | 3 | MAX % 3 |
| 1 | SHL | 4096 | TRAP |
| 2^4095 | SHR | 4096 | 0 |
| 0 | BITLEN | - | 1 |
| 1 | BITLEN | - | 1 |
| MAX | BITLEN | - | 4096 |

### i128 Round-Trip Tests (RFC-0110 §i128 Round-Trip Test Vectors)
| Input | Expected |
|-------|----------|
| i128::MAX (2^127-1) | round-trip |
| i128::MIN (-2^127) | round-trip |
| 0 | round-trip |
| 1 | round-trip |
| -1 | round-trip |

### Canonical Form Enforcement Tests (RFC-0110 §Canonical Form Enforcement)
| Input | Expected Output | Description |
|-------|----------------|-------------|
| [0,0,0] | [0] | Trailing zeros removed |
| [5,0,0] | [5] | Multiple zeros |
| [1,0] | [1] | Single trailing |
| [MAX,0,0] | [MAX] | Max trailing |

## Phase 2: Differential Fuzzing

### Acceptance Criteria
- [ ] Fuzzing harness against num-bigint (Rust)
- [ ] 100,000+ random input cases
- [ ] All operations fuzzed: ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR
- [ ] All fuzzing cases produce identical results to reference
- [ ] Property-based testing with proptest

### Fuzzing Configuration
```rust
// Number of iterations per operation
const FUZZ_ITERATIONS: usize = 100_000;

// Operation-specific ranges
const MAX_LIMBS: usize = 64;
const MAX_BITLEN: usize = 4096;
```

### Fuzzing Strategy
```
1. Randomly generate BigInt operands within valid ranges
2. Execute operation in both implementation and reference
3. Compare results - must be identical
4. Log failures for debugging
5. Track coverage metrics
```

### Property-Based Testing Properties
```rust
// Additive identity: a + 0 = a
prop_compose! {
    fn additive_identity(a: BigInt) -> bool {
        bigint_add(a, ZERO) == a
    }
}

// Multiplicative identity: a * 1 = a
prop_compose! {
    fn multiplicative_identity(a: BigInt) -> bool {
        bigint_mul(a, BigInt::from(1)) == a
    }
}

// Negation: -(-a) = a
prop_compose! {
    fn double_negation(a: BigInt) -> bool {
        let neg = bigint_sub(ZERO, a);
        bigint_sub(ZERO, neg) == a
    }
}

// Division correctness: (a / b) * b + (a % b) = a
prop_compose! {
    fn divmod_identity(a: BigInt, b: BigInt) -> bool where b != ZERO {
        let (q, r) = bigint_divmod(a, b);
        bigint_add(bigint_mul(q, b), r) == a
    }
}
```

## Phase 3: Gas Verification

### Acceptance Criteria
- [ ] Verify worst-case 64-limb DIV + canonicalization ≤ 15,000 gas
- [ ] Benchmark all operations
- [ ] Document gas consumption per operation

### Gas Benchmarks (Informative)
| Operation | Worst Case Gas |
|-----------|----------------|
| ADD | ~1,500 |
| SUB | ~1,500 |
| MUL | ~8,000 |
| DIV | ~15,000 |
| MOD | ~15,000 |
| CMP | ~500 |
| SHL | ~2,000 |
| SHR | ~2,000 |
| BITLEN | ~500 |
| CANONICALIZE | ~1,000 |

## Phase 4: Probe Verification

### Acceptance Criteria
- [ ] All 56 probe entries produce correct results
- [ ] Merkle root verification passes
- [ ] Integration with test suite

## Implementation Location
- **File**: `determin/src/bigint.rs` (tests module)
- **Fuzz**: `determin/fuzz/` (AFL/libfuzz style)
- **Bench**: `determin/benches/` (criterion)

## Prerequisites
- Mission 0110-bigint-core-algorithms (complete)
- Mission 0110-bigint-conversions-serialization (complete)
- Mission 0110-bigint-verification-probe (complete)

## Dependencies
- num-bigint (reference implementation)
- proptest (property-based testing)
- rand (random generation)
- criterion (benchmarking)

## Testing Requirements Summary
| Category | Count | Required |
|----------|-------|----------|
| Unit tests | 40+ | Yes |
| Fuzz cases | 100,000+ | Yes |
| Property tests | 10+ | Yes |
| Probe entries | 56 | Yes |
| Gas benchmarks | 10 | Informative |

## Reference
- RFC-0110: Deterministic BIGINT (§Test Vectors)
- RFC-0110: Deterministic BIGINT (§Gas Model)
- RFC-0110: Deterministic BIGINT (§Differential Fuzzing Requirement)
- RFC-0110: Deterministic BIGINT (§Verification Probe)
- missions/claimed/0104-dfp-core-type.md (DFP fuzzing pattern)

## Complexity
Medium — Fuzzing infrastructure setup + comprehensive test coverage
