# RFC-0129 (Numeric/Math): Deterministic RNG

## Status

Planned

## Summary

Defines cryptographically secure deterministic random number generation for protocol operations.

## Why Needed

Many operations require randomness that must be reproducible:

- HNSW graph construction
- Mixture-of-Experts routing
- Training data shuffling
- Key derivation

Without deterministic RNG, proofs cannot be verified.

## Scope

- Seed derivation from block hash
- PRNG algorithm specification
- Random sampling methods
- Security considerations

## Dependencies

- RFC-0106 (Numeric/Math): Deterministic Numeric Tower

## Related RFCs

- RFC-0303 (Retrieval): Deterministic Vector Index
- RFC-0522 (AI Execution): Mixture-of-Experts

---

**Note:** This RFC is planned but not yet written. It is a placeholder for future work.
