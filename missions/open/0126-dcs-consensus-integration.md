# Mission: RFC-0126 DCS Consensus Integration

## Status
Open

## RFC
RFC-0126 v2.5.1 (Numeric): Deterministic Serialization

## Summary
Integrate DCS with consensus: add gas constants, error codes, and ensure all DCS operations are registered in the consensus module.

## Acceptance Criteria
- [x] Add DCS error codes to consensus module:
  - [x] DCS_INVALID_BOOL
  - [x] DCS_INVALID_SCALE
  - [x] DCS_NON_CANONICAL
  - [x] DCS_OVERFLOW
  - [x] DCS_INVALID_UTF8
  - [x] DCS_LENGTH_OVERFLOW
- [x] Document DCS gas constants (if any per RFC-0126)
- [x] Register DCS operations in consensus module
- [x] Verify probe module loads correctly
- [x] Clippy clean, cargo fmt applied
- [x] All tests pass

## Dependencies
Mission 0126-dcs-core-types
Mission 0126-dcs-composite-types
Mission 0126-dcs-verification-probe

## Location
`determin/src/consensus.rs`
`determin/src/lib.rs`

## Complexity
Low — integration work

## Implementation Notes
- DCS errors are fatal (TRAP) — no recovery
- Error codes defined in RFC-0126 §DCS Serialization Errors
- Ensure dcs module is publicly exported from lib.rs

## Reference
- RFC-0126 §Error Handling
- RFC-0126 §DCS Serialization Errors

## Changes Made
- Added `DcsErrorCode` enum with all 6 DCS error codes
- Added `dcs_op_ids` module with 13 DCS operation IDs (0x0300-0x030C)
- Added DCS gas constants: `DCS_GAS_PER_BYTE`, `DCS_GAS_MIN`, `DCS_GAS_MAX`
- Added `gas_dcs_serialize()` function for consensus gas accounting
- Added `is_dcs_op()` function to check if op_id is a DCS operation
- Added comprehensive tests for all new functionality
