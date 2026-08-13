# Mission: BigInt Consensus Integration

## Status

DEFERRED 2026-08-13 (close-as-DEFERRED, 0871b pattern). Mission partially-landed: BigInt SQL type integration landed via missions `0202-c-bigint-decimal-persistence` + `0202-e-bigint-decimal-integration-testing` + `0110-bigint-core-algorithms` + `0110-bigint-conversions-serialization`. Consensus-layer (Merkle encoding + replay validation + fork detection + block-header spec-version pinning) is blocked by DFP consensus integration (parent mission `0104-dfp-consensus-integration.md`, status: Blocked) which establishes the cross-stack pattern.

**Landed scope (Phase 5 partial + Phase 1 partial):**

- `Value::bigint()` constructor (`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs:222`) using `BigIntEncoding` wire format `[version:1][sign:1][reserved:2][num_limbs:1][reserved:3][limb0:8]...[limbN:8]`
- `encode_bigint_lexicographic()` (`/home/mmacedoeu/_w/databases/stoolap/src/storage/index/btree.rs:1771`) for B-tree index ordering (Phase 1 partial)
- 23 BIGINT integration tests in `/home/mmacedoeu/_w/databases/stoolap/tests/bigint_decimal_integration_test.rs`

**Blockers identified (per phase):**

1. **Phase 1 — Merkle state trie BigInt encoding:** BLOCKED. No Merkle state trie infrastructure exists in cipherocto `octo-protocol` crate; needs DFP first per `0104-dfp-consensus-integration.md`. Storage-side BigInt value type + wire format landed; Merkle hashing infra missing.
2. **Phase 2 — Replay validation (DQA op result-hash storage in `stoolap/src/consensus/operation.rs`):** BLOCKED. Same infra gap as DQA replay (no result-hash commit pattern in `consensus/operation.rs`).
3. **Phase 3 — Fork handling (1-epoch divergence detection):** BLOCKED. No divergence detection infra; needs DFP first.
4. **Phase 4 — Spec version pinning:** PARTIALLY LANDED. `NUMERIC_SPEC_VERSION = 2` exists (`/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/persistence.rs:54`) per RFC-0202-A §4a — supersedes mission's proposed `= 1`. Block header `numeric_spec_version: u32` integration BLOCKED (no `BlockHeader` struct in cipherocto `octo-protocol` yet).
5. **Phase 5 — VM opcodes + SQL operators:** PARTIALLY LANDED. SQL `BIGINT '...'` literal + CAST + arithmetic + cross-type comparison work (covered by 23 integration tests). VM BigInt opcodes (`BigIntLiteral`, `BigIntOp(Add/Sub/Mul/Div/Mod/Cmp/Shl/Shr/BitLen)`) per mission pseudocode — NOT YET IMPLEMENTED in `/home/mmacedoeu/_w/databases/stoolap/src/vm/`.

**Follow-on missions filed:**

- **`0110-bigint-consensus-unblocker-1-merkle-encoding`** — Phase 1 once DFP unblocks first
- **`0110-bigint-consensus-unblocker-2-replay-validation`** — Phase 2 (same blocker as DQA replay)
- **`0110-bigint-consensus-unblocker-3-fork-handling`** — Phase 3 (same blocker as DQA fork)
- **`0110-bigint-consensus-unblocker-4-block-header-spec-version`** — Phase 4 (needs `BlockHeader.numeric_spec_version` integration)
- **`0110-bigint-consensus-unblocker-5-vm-opcodes`** — Phase 5 VM opcodes (BigIntLiteral + BigIntOp variant in `/home/mmacedoeu/_w/databases/stoolap/src/vm/mod.rs`)

**Drift disclosure:** Per [[deferred-vs-unspecified]] discipline — explicitly DEFERRED with filed follow-on missions, concrete unblockers, and per-phase rationale. Mission's proposed `NUMERIC_SPEC_VERSION = 1` is superseded by RFC-0202-A's `= 2`; no "future / post-v1.0" placeholders.

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

- [ ] BigIntEncoding in Merkle state trie — **DEFERRED** (Merkle state trie infrastructure missing in cipherocto `octo-protocol`; needs DFP first)
- [x] Canonical serialization for state hashing — **LANDED** (`Value::bigint()` uses `BigIntEncoding` wire format at `/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs:222`; `encode_bigint_lexicographic()` at `/home/mmacedoeu/_w/databases/stoolap/src/storage/index/btree.rs:1771`)
- [ ] Integration with state trie infrastructure — **DEFERRED** (same blocker as Merkle encoding)

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

- [ ] On replay, re-execute BigInt operations — **DEFERRED** (no DQA op result-hash storage in `stoolap/src/consensus/operation.rs`; same blocker as DQA replay)
- [ ] Compare result hashes with committed state — **DEFERRED** (same blocker)
- [ ] Detect divergence within 1 epoch — **DEFERRED** (same blocker as DQA divergence detection)

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

- [ ] Detect divergent BigInt results within 1 epoch — **DEFERRED** (no divergence detection infra; needs DFP pattern first)
- [ ] Fork resolution mechanism — **DEFERRED** (same blocker)
- [ ] Consensus participation — **DEFERRED** (same blocker)

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

- [x] NUMERIC_SPEC_VERSION constant defined — **LANDED** (as `= 2` per RFC-0202-A §4a, at `/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/persistence.rs:54`; supersedes mission's proposed `= 1`)
- [ ] Block header numeric_spec_version integration — **DEFERRED** (no `BlockHeader` struct in cipherocto `octo-protocol` yet)
- [ ] Version check during replay — **DEFERRED** (same blocker)

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

- [x] BigInt as Value type in stoolap — **LANDED** (`Value::bigint()` constructor at `/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs:222`)
- [x] SQL operators using BigInt — **LANDED** (BIGINT typed literals + CAST + arithmetic + cross-type comparison covered by 23 integration tests in `bigint_decimal_integration_test.rs`)
- [ ] Expression VM opcodes for BigInt — **DEFERRED** (`BigIntLiteral` + `BigIntOp(Add/Sub/Mul/Div/Mod/Cmp/Shl/Shr/BitLen)` not yet implemented in `/home/mmacedoeu/_w/databases/stoolap/src/vm/mod.rs`; SQL-level BigInt arithmetic works via existing operator dispatch)

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

Medium (per phase); High (cross-stack: requires stoolap fork + cipherocto `octo-protocol` Merkle infrastructure + Phase 5 VM opcodes)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed. 5 phases / 15 ACs: Merkle state encoding + canonical serialization + state trie integration + replay validation + divergence detection + spec version pinning + Value type + SQL operators + VM opcodes.                                                                                                                                                                                                                                                                                               |
| v0.2    | 2026-08-13 | **DEFERRED (close-as-DEFERRED, 0871b pattern).** 4/15 ACs LANDED (Phase 1 canonical serialization + Phase 4 NUMERIC_SPEC_VERSION constant + Phase 5 Value type + Phase 5 SQL operators). 11/15 ACs DEFERRED across 5 follow-on missions: `0110-bigint-consensus-unblocker-1-merkle-encoding`, `unblocker-2-replay-validation`, `unblocker-3-fork-handling`, `unblocker-4-block-header-spec-version`, `unblocker-5-vm-opcodes`. All blocked by DFP consensus integration (parent mission) or VM opcode implementation. |

Last Updated: 2026-08-13
Version: 0.2 (DEFERRED)
