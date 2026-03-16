# Mission: DFP Expression VM Opcodes

## Status
Complete

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Add DFP operation opcodes to the expression VM with deterministic execution mode.

## Acceptance Criteria
- [x] OP_DFP_ADD, OP_DFP_SUB, OP_DFP_MUL, OP_DFP_DIV opcodes
- [x] Compile error on DFP * FLOAT without explicit CAST
- [x] DeterministicExecutor mode that enforces DFP-only arithmetic
- [x] INT → DFP implicit promotion in deterministic contexts
- [x] Signed-zero arithmetic per IEEE-754 §6.3

## Location
`src/executor/expression/vm.rs`

## Complexity
Medium

## Prerequisites
- Mission 1: DFP Core Type (complete)
- Mission 2: DFP DataType Integration (complete)

## Implementation

### Changes to `stoolap/src/executor/expression/vm.rs`:

1. **Added deterministic mode to ExprVM**:
   - `deterministic: bool` field
   - `ExprVM::deterministic()` constructor
   - `is_deterministic()` and `set_deterministic()` methods

2. **Added deterministic arithmetic**:
   - `arithmetic_op_deterministic()` method
   - INT → DFP promotion
   - FLOAT causes error when mixed with DFP

3. **DFP operations already implemented**:
   - Uses dfp_add, dfp_sub, dfp_mul, dfp_div from octo-determin
   - Error on FLOAT + DFP mixing

## Features:
- Normal mode: standard FLOAT/INTEGER arithmetic
- Deterministic mode: DFP-only, INT promotes to DFP
