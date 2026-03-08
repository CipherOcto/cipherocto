# Mission: DFP Hardware Verification

## Status
Open

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Implement deterministic verification probe and node capability advertisement for DFP execution.

## Acceptance Criteria
- [ ] DeterministicFloatProbe test suite with 24-byte byte comparison
- [ ] Node capability advertisement with `dfp_spec_version: u32`
- [ ] Automatic fallback to software path on verification failure
- [ ] Comprehensive test vectors covering edge cases and cross-platform validation
- [ ] Probe runs automatically every 100,000 blocks
- [ ] Probe failure handling: node halts, logs diagnostic, awaits manual intervention

## Location
`determ/probe.rs`

## Complexity
Low

## Prerequisites
- Mission 1: DFP Core Type

## Notes
- VERIFICATION_PROBE is the authoritative consensus-grade verification
- VERIFICATION_TESTS is only a developer smoke test (limited precision)
