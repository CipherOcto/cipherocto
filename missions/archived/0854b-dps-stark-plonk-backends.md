# Mission: DPS STARK and PLONK Backend Implementations

## Status

Implemented (3 new files, 26 tests: STARK/STWO backend, PLONK backend, backend registry)

## RFC

RFC-0854: Deterministic Proof Substrate (DPS) — §3, Phase 2

## Summary

Implement concrete proof backends for STARK (STWO) and PLONK, providing real prove/verify implementations behind the DeterministicProofSystem trait. STWO is StarkWare's STARK implementation; STARK is the proof system category.

## Acceptance Criteria

- [x] STARK (STWO) backend: prove(), verify(), proof_commitment() using Cairo traces
- [x] PLONK backend: prove(), verify(), proof_commitment() using PLONKish circuits
- [x] Backend registry: register backends by ProofSuiteId
- [x] STARK properties: transparent (no trusted setup), AIR constraints, massive parallelism
- [x] PLONK properties: succinct proofs, universal setup
- [x] Backend selection per mission configuration
- [x] Benchmark: proving time, verification time, proof size per backend
- [x] Unit tests: 10+ tests covering each backend, cross-backend compatibility
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dps/backends/`

## Claimant

@agent (Jcode)

## Key Files

| File | Change |
|------|--------|
| `backends/mod.rs` | Backend module root |
| `backends/stark.rs` | STARK (STWO) backend implementation |
| `backends/plonk.rs` | PLONK backend implementation |

## Complexity

High (3-5 days)

## Prerequisites

- Mission 0854: DPS Deterministic Proof Substrate

## Implementation Notes

- STARK (STWO): Cairo execution traces, AIR constraints, SIMD-friendly
- PLONK: PLONKish circuits, universal trusted setup, succinct proofs
- Backend selection is mission-configured, not consensus-ordered
- Each backend implements DeterministicProofSystem trait independently

## Reference

- RFC-0854 §3: Proof Suite Identification
- `docs/research/cairo-ai-research-report.md` (STWO properties)
