# Mission: DQA Consensus Integration

## Status

DEFERRED 2026-08-13 (close-as-DEFERRED, 0871b pattern). Mission cannot be implemented standalone — blocked by DFP consensus integration (`missions/claimed/0104-dfp-consensus-integration.md`, status: Blocked), which establishes the cross-stack pattern (stoolap `src/consensus/` infrastructure + cipherocto `octo-protocol` Merkle state encoding + replay validation framework + divergence detection).

**Blockers identified:**

1. **`DETERMINISTIC VIEW` SQL syntax not parsed.** AC-2 (CREATE DETERMINISTIC VIEW enforcement) requires SQL parser support that doesn't exist in `/home/mmacedoeu/_w/databases/stoolap/src/parser/`. No DFP or DQA view-flag storage in the planner.
2. **DQA replay-validation framework absent.** AC-3 (replay-validation result hash compare) requires a transaction-replay pipeline + result-hash commit pattern. Stoolap's `/home/mmacedoeu/_w/databases/stoolap/src/consensus/operation.rs` does not store DQA op result hashes.
3. **DQA divergence detection mechanism missing.** AC-4 (1-epoch divergence detection) requires DFP first to establish the soft-fork + probe pattern; DQA borrows the same infra.
4. **`DQA_SPEC_VERSION` constant undefined.** AC-5 — current code has only the unified `NUMERIC_SPEC_VERSION = 2` per RFC-0202-A §4a (`/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/persistence.rs:54`). No DQA-specific version constant exists.

**Follow-on missions filed:**

- **`0105-dqa-consensus-unblocker-1-deterministic-view-syntax`** — once DFP unblocks first (per `0104-dfp-consensus-integration.md` unblocker list), this picks up AC-2 (CREATE DETERMINISTIC VIEW parser support + planner enforcement + DQA-only type gating).
- **`0105-dqa-consensus-unblocker-2-replay-validation`** — picks up AC-3 (DQA op result-hash storage in `consensus/operation.rs` + replay-hash compare on block validation).
- **`0105-dqa-consensus-unblocker-3-divergence-detection`** — picks up AC-4 (DQA Merkle divergence detection + soft-fork trigger).
- **`0105-dqa-consensus-unblocker-4-dqa-spec-version-constant`** — picks up AC-5 (add `pub const DQA_SPEC_VERSION: u32 = 1` alongside `NUMERIC_SPEC_VERSION`; document in RFC-0105).

**Drift disclosure:** Per [[deferred-vs-unspecified]] discipline — explicitly DEFERRED with filed follow-on missions, concrete unblockers, and per-AC mapping. No "future / post-v1.0" placeholders.

## RFC

RFC-0105: Deterministic Quant Arithmetic (DQA)

## Summary

Integrate DQA into stoolap's consensus layer with Merkle state encoding, replay validation, and divergence detection.

## Acceptance Criteria

- [ ] DQA encoding in Merkle state — **DEFERRED** (no Merkle state trie infrastructure exists in cipherocto `octo-protocol` crate; needs DFP first per `0104-dfp-consensus-integration.md`)
- [ ] Deterministic view enforcement — **DEFERRED** (`CREATE DETERMINISTIC VIEW` syntax not parsed; no DFP/DQA view-flag storage in stoolap planner)
- [ ] Consensus replay validation — **DEFERRED** (no DQA op result-hash storage in `stoolap/src/consensus/operation.rs`)
- [ ] Fork handling — **DEFERRED** (divergence detection mechanism missing; needs DFP pattern first)
- [ ] Spec version pinning — **DEFERRED** (no `DQA_SPEC_VERSION` constant; only unified `NUMERIC_SPEC_VERSION = 2` exists per RFC-0202-A §4a)

## Location

`stoolap/src/storage/`, `stoolap/src/consensus/`

## Complexity

Medium (per AC); High (cross-stack: requires stoolap fork + cipherocto `octo-protocol` Merkle infrastructure)

## Prerequisites

- Mission 1: DQA Core Type — **LANDED** (`determin/src/dqa.rs`, archived `0105-dqa-core-type`)
- Mission 2: DQA DataType Integration — **LANDED** (archived `0105-dqa-datatype-integration`)
- Mission 3: DQA Expression VM Opcodes — **LANDED** (archived `0105-dqa-expression-vm`)
- Mission 4: **DFP Consensus Integration** (parent pattern) — **BLOCKED** (`missions/claimed/0104-dfp-consensus-integration.md`) — must land first to establish consensus-layer Merkle + replay + divergence infra

## Implementation Notes

- Use DQA's canonical serialization for Merkle hashing
- Similar pattern to DFP consensus integration (RFC-0104)
- DQA is simpler than DFP (no special values, fixed range)
- Probe/hardware verification may not be needed for DQA (bounded range)

## Reference

- RFC-0105: Deterministic Quant Arithmetic (§Consistency)
- missions/claimed/0104-dfp-consensus-integration.md (DFP pattern — parent mission)
- RFC-0202-A §4a (NUMERIC_SPEC_VERSION = 2)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed. 5 ACs covering DQA Merkle encoding + DETERMINISTIC VIEW syntax + replay validation + fork handling + DQA_SPEC_VERSION constant.                                                                                                                                                                                                                                                                                |
| v0.2    | 2026-08-13 | **DEFERRED (close-as-DEFERRED, 0871b pattern).** All 5 ACs blocked by DFP consensus integration (parent mission status: Blocked). 4 follow-on missions filed: `0105-dqa-consensus-unblocker-1-deterministic-view-syntax`, `0105-dqa-consensus-unblocker-2-replay-validation`, `0105-dqa-consensus-unblocker-3-divergence-detection`, `0105-dqa-consensus-unblocker-4-dqa-spec-version-constant`. Per-AC rationale documented. |

Last Updated: 2026-08-13
Version: 0.2 (DEFERRED)
