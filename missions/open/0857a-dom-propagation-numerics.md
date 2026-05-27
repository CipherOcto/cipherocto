# Mission: DOM Propagation and Deterministic Numerics

## Status

Open

## RFC

RFC-0857: Deterministic Overlay Mempool (DOM) — §6, §7, §11, §13

## Summary

Implement mempool propagation via DGP (RFC-0852), Merkle state root computation, and deterministic numerics integration for fee computation using RFC-0104 (DFP) and RFC-0105 (DQA).

## Acceptance Criteria

- [ ] Mempool propagation: intents propagate via DGP gossip (RFC-0852)
- [ ] Mission-scoped propagation: intents only propagate within mission domain
- [ ] `MempoolStateRoot`: BLAKE3-256 Merkle root of all pending intents
- [ ] State root recomputed on every admission/eviction (deterministic)
- [ ] Fee computation uses RFC-0105 DQA for deterministic arithmetic (DQA provides fixed-point decimal types; DFP/RFC-0104 provides the encoding)
- [ ] Fee ordering: no floating-point, integer-only (DQA fixed-point)
- [ ] Economic weight comparison uses DQA canonical ordering
- [ ] Anti-entropy reconciliation: Merkle summary exchange for mempool sync
- [ ] Unit tests: 10+ tests covering propagation, Merkle root, fee computation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dom/mod.rs` (propagation, numerics)

## Complexity

High

## Prerequisites

- Mission 0857: DOM Deterministic Overlay Mempool
- Mission 0852: DGP Deterministic Gossip

## Implementation Notes

- Propagation reuses DGP infrastructure with mission-scoped domains
- Merkle root is recomputed deterministically on every state change
- Fee computation MUST use DQA (RFC-0105) — no floating-point in consensus path
- Economic weight comparison uses DQA canonical ordering (not native comparison)

## Reference

- RFC-0857 §6: Mempool Propagation
- RFC-0857 §7: Mempool Root
- RFC-0857 §11: Deterministic Numerics
- RFC-0857 §13: Mempool Capacity Limits
