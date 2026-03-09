# Mission: DQA Expression VM Opcodes

## Status
Open

## RFC
RFC-0105: Deterministic Quant Arithmetic (DQA)

## Summary
Add VM opcodes for DQA arithmetic operations, enabling deterministic expression evaluation in stoolap.

## Acceptance Criteria
- [ ] OP_DQA_ADD opcode implementation
- [ ] OP_DQA_SUB opcode implementation
- [ ] OP_DQA_MUL opcode implementation
- [ ] OP_DQA_DIV opcode implementation
- [ ] OP_DQA_NEG (unary negation)
- [ ] OP_DQA_ABS (absolute value)
- [ ] OP_DQA_CMP (compare: returns -1, 0, or 1)
- [ ] Scale alignment validation at runtime
- [ ] Overflow/division-by-zero error handling

## Location
`stoolap/src/vm/`, `stoolap/src/execution/`

## Complexity
Low

## Prerequisites
- Mission 2: DQA DataType Integration

## Implementation Notes
- Import DQA from determin crate
- Each opcode calls corresponding DQA method
- Scale alignment done per RFC-0105 ALIGN_SCALES algorithm
- Return DqaError as VM error variants
- Compare operation must canonicalize operands first

## Reference
- RFC-0105: Deterministic Quant Arithmetic (§VM Opcodes)
- stoolap/src/execution/ (existing VM execution patterns)
