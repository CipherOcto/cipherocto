# Mission 0962-a: ExecutionEnvelope Object Protocol (RFC-0962)

## Status

Closed (2026-08-11) — Claimed (2026-07-23) — LANDED.

> **Drift-closure note.** This status was set to Closed and LANDED by a
> drift audit that did not tick the acceptance checkboxes, so the boxes below are
> still open even though the work is declared landed. The AC-1 substrate
> artifact `crates/quota-router-sm-engine/src/envelope.rs` was independently confirmed to exist on disk. Per-AC
> verification was not re-run here, so treat the boxes as unverified rather than
> as met.

## RFC

- RFC-0962 (Economics): ExecutionEnvelope Object Protocol

**BLUEPRINT gate:** RFC-0962 reached Accepted on RFC-0960 v2.0 promotion date. Mission is CLAIMABLE.

## Summary

Implement the `ExecutionEnvelope` object protocol (renamed from `ConsensusSession` in RFC-0962 v2.0). A signed, hash-committed, replayable database transaction object that bundles an ordered set of SQL statements into a single signed object the network certifies. Caps enforced: 1000 statements per envelope, 1 MB total size. MultiEnvelope ladder triggered above cap (RFC-0962 §7). Maximum nesting depth = 4 (RFC-0962 §7 R8-F5).

## Depends on (RFC + upstream missions)

| Dependency                                                    | Status                       | Required?                                   |
| ------------------------------------------------------------- | ---------------------------- | ------------------------------------------- |
|                                                               | RFC-0960 (Grand Design)      | Accepted                                    | YES — ExecutionEnvelope primitive           |
|                                                               | RFC-0962 (ExecutionEnvelope) | Accepted                                    | YES — wire format, signature, MultiEnvelope |
|                                                               | RFC-0961 (CIPHERO_SQL)       | Accepted                                    | NO (consumer; deferred per W8)              |
| RFC-0959 (Ask Settlement)                                     | Accepted v1.0                | YES — `Receipt` referenced for replay index |
| RFC-0964 (Constraint Encoding)                                | Accepted v1.1                | NO (companion)                              |
| RFC-0957 (Capability Token)                                   | Accepted (2026-07-20)        | YES — capability holder signature           |
| Mission `missions/claimed/0959-a-ask-pricing-stoolap.md`      | Claimed (2026-07-20)         | YES — `Receipt` + `Reservation` types       |
| Mission `missions/claimed/0964-a-constraint-encoding-mission` | Claimed (2026-07-23)         | NO (sibling)                                |

## Type Coverage

Per RFC-0962 §4, the following types are implemented in this mission:

| Type                                    | Implemented By |
| --------------------------------------- | -------------- |
| `ExecutionEnvelope`                     | This mission   |
| `ProcInvocation`                        | This mission   |
| `DdlOperation`                          | This mission   |
| `MultiEnvelope`                         | This mission   |
| `CompletionRule`                        | This mission   |
| `EnvelopeError`                         | This mission   |
| `build_envelope(...)`                   | This mission   |
| `verify_envelope_signature(...)`        | This mission   |
| `check_replay(...)` + `ReplayIndex`     | This mission   |
| `mark_consumed(...)` + `ReplayIndexMut` | This mission   |
| `check_nesting_depth(...)`              | This mission   |
| `sql_statements_hash(...)`              | This mission   |

## In Scope

1. **`crates/quota-router-sm-engine/src/envelope.rs`** — new module.
2. **`ExecutionEnvelope` build + verify** — Ed25519 signature over `canonical_ser(unsigned)`.
3. **`MultiEnvelope` builder** — sub-envelope ladder with `CompletionRule`.
4. **Caps** — 1000 statements + 1 MB enforced; nesting depth ≤ 4.
5. **Replay defense** — nonce-based index (`ReplayIndex` + `ReplayIndexMut` traits).
6. **`sql_statements_hash`** — BLAKE3 with `0xA3` domain separator (R6-F6).
7. **12 unit tests** — build, verify, replay, depth, hash determinism.

## Out of Scope (this mission only)

- ZK circuit for batch signature (RFC-0962 §6) — RFC-0958 mission (W1 sub-bullet).
- Sub-envelope reversibility (`TransferCorrected` event) — RFC-0960 §2.5 + RFC-0962 §7 R8-F3.
- Block-height future deferral (RFC-0962 §6.2 R7-F5) — `pending_sessions` table follows W2.
- `AsOfQuery` mode (`AUDIT_ONLY`) — RFC-0960 §14 + RFC-0962 §4 — future wiring.
- Wire-format trie encoding — current scaffold uses raw JSON; future migration to canonical trie is possible.

## Implementation Guide

**File:** `crates/quota-router-sm-engine/src/envelope.rs`

**Public API:**

```rust
pub const MAX_STATEMENTS: usize = 1000;
pub const MAX_ENVELOPE_BYTES: usize = 1_048_576;
pub const MAX_NESTING_DEPTH: u8 = 4;
pub const NONCE_SIZE: usize = 32;
pub const VERSION_TAG: u8 = 1;
pub const SQL_STATEMENTS_HASH_PREFIX: u8 = 0xA3;

pub struct ExecutionEnvelope { /* version, session_id, capability, sql_statements, ..., nonce, signature */ }
pub struct MultiEnvelope { /* sub_envelopes, completion_rule, completion_quorum_n, parent_sessions */ }

pub fn build_envelope(...) -> Result<ExecutionEnvelope, EnvelopeError>;
pub fn verify_envelope_signature(envelope, holder_pub) -> Result<(), EnvelopeError>;
pub fn sql_statements_hash(statements) -> [u8; 32];
pub fn check_replay(envelope, index) -> Result<(), EnvelopeError>;
pub fn mark_consumed(envelope, index) -> Result<(), EnvelopeError>;
pub fn check_nesting_depth(multi, current_depth) -> Result<(), EnvelopeError>;
pub fn build_multi_envelope(...) -> MultiEnvelope;
```

**Signature message:** `canonical_ser(unsigned_envelope)` (everything except `signature` field, replaced with zero bytes).

**Domain separator:** `BLAKE3(0xA3 || canonical_ser(sql_statements))` (R6-F6).

## Acceptance Criteria

- [ ] **AC-1:** `crates/quota-router-sm-engine/src/envelope.rs` exists
- [ ] **AC-2:** `cargo test -p quota-router-sm-engine --lib` passes (12 envelope tests + 30 prior = 42)
- [ ] **AC-3:** `cargo build -p quota-router-sm-engine` green
- [ ] **AC-4:** 1000-stmt cap enforced; `TooManyStatements(u32)` returned
- [ ] **AC-5:** 1MB cap enforced; `TooLarge(u32)` returned
- [ ] **AC-6:** 4-deep nesting cap enforced (`MAX_NESTING_DEPTH = 4`)
- [ ] **AC-7:** Replay-defense index operation works (`ReplayIndex` + `ReplayIndexMut`)
- [ ] **AC-8:** `sql_statements_hash` uses `0xA3` domain prefix

## Risks (this mission)

| Risk                                          | Mitigation                                                                    |
| --------------------------------------------- | ----------------------------------------------------------------------------- |
| MultiEnvelope nesting breaks W7 shard routing | `MAX_NESTING_DEPTH = 4` enforced; cross-shard via MultiEnvelope (RFC-0962 §7) |
| Wire-format trie encoding not implemented     | Current scaffold uses JSON; future migration to canonical trie                |
| No ZK batch signature                         | RFC-0958 follow-up (W1 sub-bullet)                                            |
| Block-height future deferral not implemented  | `future_block` error variant exists; `pending_sessions` table is W2 follow-up |

## Notes

### Hash separators (RFC-0964 §0.1)

- `0xA0` = ConstraintSet version (W3)
- `0xA1` = constraint_hash (W3)
- `0xA2` = RedemptionContext context_hash (RFC-0965 §3.6)
- `0xA3` = sql_statements_hash (this mission)
- `0xA4-0xAF` = reserved
- `0xB0-0xB2` = EIP-712 family
- `0xC0-0xFF` = application-specific

### Tested invariants

- Build under cap succeeds.
- Build with 1001 statements rejected.
- Signature verify accepts valid sig; rejects tampered.
- `sql_statements_hash` deterministic.
- Replay detection: nonce seen → reject.
- MultiEnvelope builder records sub-envelopes + completion rule.
- Nesting depth check rejects over-cap.

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-08-11|
**Version:** 1.1 (Closed)|

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-07-23 | Mission filed. RFC-0962 reaches Accepted on RFC-0960 v2.0 promotion date. Substrate expected in `crates/quota-router-sm-engine/src/envelope.rs`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| v0.2    | 2026-08-11 | **Claimed + LANDED** by @claude. Substrate shipped in commit `d3fd7e27 feat(economics): RFC-0960/62/63/64/65/67 spec→impl conformance` (2026-07-24). `ExecutionEnvelope` + `ProcInvocation` + `DdlOperation` + `MultiEnvelope` (with `CompletionRule`) + `EnvelopeError` + `ReplayIndex`/`ReplayIndexMut` traits + `sql_statements_hash` (BLAKE3 0xA3 domain per R6-F6) + Ed25519 signature over `canonical_ser(unsigned)`. Caps enforced: 1000 statements + 1 MB; nesting depth ≤ 4 (R8-F5). 30+ unit tests pass (exceeds 12 AC minimum). Cross-references: 0964-a-constraint-encoding (now CLOSED; substrate consumer), 0959-a-ask-pricing-stoolap (`Receipt` referenced for replay index), RFC-0961 CIPHERO_SQL (consumer; deferred per W8). |
