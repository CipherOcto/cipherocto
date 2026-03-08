# Mission: DFP Expression VM Opcodes

## Status
Open

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Add DFP operation opcodes to the expression VM with deterministic execution mode.

## Acceptance Criteria
- [ ] OP_DFP_ADD, OP_DFP_SUB, OP_DFP_MUL, OP_DFP_DIV opcodes
- [ ] Compile error on DFP * FLOAT without explicit CAST
- [ ] DeterministicExecutor mode that enforces DFP-only arithmetic
- [ ] INT → DFP implicit promotion in deterministic contexts
- [ ] Signed-zero arithmetic per IEEE-754 §6.3

## Location
`src/vm/`

## Complexity
Medium

## Prerequisites
- Mission 1: DFP Core Type
- Mission 2: DFP DataType Integration
