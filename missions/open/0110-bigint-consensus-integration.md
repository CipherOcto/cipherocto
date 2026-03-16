# Mission: BigInt Consensus Integration

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Integrate BigInt into stoolap's consensus layer with Merkle state encoding, replay validation, and spec version pinning. This mission enables BigInt operations in the consensus-critical path.

## Overview

BigInt integration with consensus requires:
1. Canonical serialization for Merkle hashing
2. Replay validation (deterministic execution verification)
3. Fork detection for divergent BigInt results
4. Spec version pinning for historical replay

## Phase 1: Merkle State Encoding

### Acceptance Criteria
- [ ] BigIntEncoding in Merkle state trie
- [ ] Canonical serialization for state hashing
- [ ] Integration with state trie infrastructure

### Implementation Pattern
```rust
/// BigInt value in state
enum StateValue {
    BigInt(BigIntEncoding),
    // ... other types
}

/// State trie key -> BigInt value
fn get_bigint(state: &State, key: &[u8]) -> Option<BigInt> {
    let encoding = state.get(key)?;
    BigInt::deserialize(&encoding).ok()
}

/// BigInt value -> state trie
fn put_bigint(state: &mut State, key: &[u8], value: &BigInt) {
    let encoding = value.serialize();
    state.put(key, &encoding);
}
```

## Phase 2: Replay Validation

### Acceptance Criteria
- [ ] On replay, re-execute BigInt operations
- [ ] Compare result hashes with committed state
- [ ] Detect divergence within 1 epoch

### Replay Validation Flow
```
1. Load block with BigInt operations
2. For each BigInt operation:
   a. Re-execute using deterministic BigInt
   b. Compute result hash
   c. Compare with stored result hash
3. If mismatch detected:
   a. Flag block as divergent
   b. Trigger fork resolution
```

### Divergence Detection
```rust
/// Check BigInt operation determinism during replay
fn verify_bigint_operation(
    state: &State,
    operation: &BigIntOperation,
    expected_result: &BigIntEncoding,
) -> Result<(), DivergenceError> {
    // Re-execute operation
    let actual = execute_bigint_operation(operation, &state)?;

    // Compare with expected
    if actual.serialize() != expected_result.serialize() {
        return Err(DivergenceError {
            operation: operation.clone(),
            expected: expected_result.clone(),
            actual: actual.serialize(),
        });
    }

    Ok(())
}
```

## Phase 3: Fork Handling

### Acceptance Criteria
- [ ] Detect divergent BigInt results within 1 epoch
- [ ] Fork resolution mechanism
- [ ] Consensus participation

### Fork Detection
```rust
/// Epoch-based BigInt divergence check
struct BigIntConsensusChecker {
    epoch: u64,
    divergent_blocks: Vec<BlockHash>,
}

impl BigIntConsensusChecker {
    /// Check for BigInt divergence in recent epoch
    fn check_epoch(&mut self, epoch: u64) -> Option<Fork> {
        if self.divergent_blocks.len() > 0 {
            Some(Fork {
                reason: ForkReason::BigIntDivergence,
                blocks: self.divergent_blocks.clone(),
            })
        } else {
            None
        }
    }
}
```

## Phase 4: Spec Version Pinning

### Acceptance Criteria
- [ ] NUMERIC_SPEC_VERSION = 1 constant defined
- [ ] Block header numeric_spec_version integration
- [ ] Version check during replay

### Spec Version Constants
```rust
/// Numeric tower unified specification version (DFP, DQA, BigInt)
/// RFC-0110: Initial version
pub const NUMERIC_SPEC_VERSION: u32 = 1;

/// Version in block header
#[derive(Serialize, Deserialize)]
pub struct BlockHeader {
    // ... other fields
    pub numeric_spec_version: u32,
    // ... other fields
}
```

### Version Check Rules (RFC-0110 §Replay Rules)
```
1. Version Check: If block.numeric_spec_version != current NUMERIC_SPEC_VERSION → reject block
2. Historical Replay: Load the exact algorithm version declared in block header
3. Algorithm Pinning: All BIGINT operations inside block MUST use pinned version
4. Canonical Form: State transitions involving BIGINT MUST verify canonical form
```

## Phase 5: Integration with stoolap

### Acceptance Criteria
- [ ] BigInt as Value type in stoolap
- [ ] SQL operators using BigInt
- [ ] Expression VM opcodes for BigInt

### Value Integration
```rust
/// stoolap Value type with BigInt support
pub enum Value {
    // ... existing variants
    BigInt(BigInt),
}

impl Value {
    pub fn bigint(&self) -> Option<&BigInt> {
        match self {
            Value::BigInt(b) => Some(b),
            _ => None,
        }
    }
}

/// BigInt expression in VM
pub enum Expression {
    // ... existing variants
    BigIntLiteral(BigInt),
    BigIntOp(BigIntOp, Box<Expression>, Box<Expression>),
}

pub enum BigIntOp {
    Add, Sub, Mul, Div, Mod,
    Cmp, Shl, Shr, BitLen,
}
```

## Implementation Location
- **stoolap**: `stoolap/src/storage/state.rs`
- **stoolap**: `stoolap/src/consensus/mod.rs`
- **stoolap**: `stoolap/src/vm/mod.rs` (expression integration)

## Prerequisites
- Mission 0110-bigint-core-algorithms (complete)
- Mission 0110-bigint-conversions-serialization (complete)
- Mission 0110-bigint-testing-fuzzing (complete)
- Mission 0110-bigint-verification-probe (complete)

## Dependencies
- stoolap (existing consensus infrastructure)
- determin crate (BigInt implementation)

## Reference
- RFC-0110: Deterministic BIGINT (§Consistency)
- RFC-0110: Deterministic BIGINT (§Spec Version & Replay Pinning)
- RFC-0110: Deterministic BIGINT (§Replay Rules)
- missions/claimed/0104-dfp-consensus-integration.md (DFP pattern)

## Complexity
Medium — Integrates with existing consensus infrastructure, similar pattern to DFP
