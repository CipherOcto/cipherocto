# RFC-0014-v2 — Settlement Substrate Amendment v2

| Field        | Value                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------- |
| Status       | Draft                                                                                       |
| Version      | v2.0.0-draft                                                                                |
| Layer        | A (substrate-frozen)                                                                        |
| Authors      | CipherOcto core team                                                                        |
| Maintainers  | octo-settlement-core maintainers                                                            |
| Parent RFC   | RFC-0014 (accepted)                                                                         |
| Supersedes   | RFC-0014 §Data Structures (none yet; v2 codifies the extension surface)                     |
| Companion    | RFC-0012-v2 (audit substrate amendment v2), RFC-0015-a + RFC-0016-a (write-path amendments) |
| Target crate | `octo-settlement-core` v2.0.0 (semver-major)                                                |

## Summary

RFC-0014-v2 is a **Layer A substrate amendment** to `octo-settlement-core` that:

1. **Pins the typed-discriminator extension pattern** for new `Receipt` semantics — extensions land via `ask_id` typed-discriminator namespaces per `cipherocto/settlement/extension/<kind>/v1/`, NOT via central `Receipt` field edits.
2. **Pins the canonical 6-field `Receipt` struct** — no new fields; substrate-frozen.
3. **Pins `ReceiptId(pub u64)` newtype** — type-safe wrapper around `Receipt.receipt_id: u64` for façade-level use; canonical hash unchanged.
4. **Pins `AppendOnlyReceiptSink::append` invariants** — strict `receipt_id` monotonicity, canonical `settlement_hash` verification, atomic persistence (parallel to RFC-0012-v2 §S2).
5. **Pins `SettlementError` canonical form** — adapter-specific failures map to `SettlementError::SinkSpecific(String)`; no new variants.

Per CLAUDE.md §Architectural Principles + §Extension over enumeration, RFC-0014-v2 enables future receipt extension kinds (AskSettled, AskPartial, AskRejected, AskRefunded, etc) without modifying `octo_settlement_core::Receipt` (which is substrate-frozen).

## Dependencies

- **RFC-0014** (parent, accepted) — defines `Receipt`, `Ask`, `AskState`, `Reservation`, `ReservationState`, `SettlementStore`, `AppendOnlyReceiptSink`, `SettlementError`, `verify_receipt_chain`, `receipt_id_for`.
- **RFC-0012-v2** (sibling, draft) — pins audit substrate extension pattern; cross-RFC consistency on canonical extension kind table.
- **RFC-0016-a** (write-path amendment, draft) — requires RFC-0014-v2 for `ReceiptId` newtype + `ask_id` typed-discriminator pattern.
- **RFC-0015-a** (write-path amendment, draft) — cross-RFC: agent transition receipt uses `ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/")` typed-discriminator.

## Design Goals

**G1.** Enable future receipt extension kinds without modifying the substrate 6-field `Receipt` struct.
**G2.** Preserve byte-identical canonical-bytes serialization form for existing receipts.
**G3.** Maintain `#[non_exhaustive]` discipline on `Receipt` so substrate-stored receipts remain forward-compatible with future extensions.
**G4.** Provide type-safe `ReceiptId` newtype for façade-level use without changing canonical substrate `Receipt.receipt_id: u64`.
**G5.** Lock the substrate contract surface so adapter implementations (`StoolapReceiptSink` in `octo-settlement/src/storage/stoolap.rs`) have no ambiguity on what they must enforce.

## Motivation

RFC-0014 (accepted) defines a 6-field `Receipt` struct (`receipt_id`, `ask_id`, `settlement_hash`, `router_id`, `router_sig`, `timestamp_unix`). The substrate-frozen nature means:

- Adding fields (e.g. `model`, `cost_dqa`, `capability_root`, `subject_did`, `status`) requires semver-major bump + migration of every existing consumer.
- Future receipt semantics (AskPartial settlements, AskRejected settlements, refund receipts) need a substrate-faithful encoding.

RFC-0014-v2 codifies the typed-discriminator pattern via `ask_id` extension namespaces so write-path amendments can reference a pinned canonical substrate form without forcing field additions to `Receipt`.

## Roles and Authorities

| Role                                        | Authority                                                                                                                                                                                  |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `octo-settlement-core` (substrate, Layer A) | Owns `Receipt`, `Ask`, `AskState`, `Reservation`, `ReservationState`, `SettlementStore`, `AppendOnlyReceiptSink`, `SettlementError`, `ReceiptId`, `verify_receipt_chain`, `receipt_id_for` |
| `octo-settlement` (façade, Layer B)         | Re-exports substrate canonical types + implements `StoolapReceiptSink` (Layer D adapter) + provides `ReceiptSummary` projection + `ReceiptStatus` enum                                     |
| Domain callers (Layer B / C / D)            | Construct `Receipt` instances via typed-discriminator helpers (e.g. `receipt_for_ask_settled(...)` in `octo-settlement/src/receipt.rs`)                                                    |

**Façade projection concerns (NOT substrate):**

- `ReceiptStatus` enum — façade-side (Layer B); recovers status from `ask_id` typed-discriminator at projection time.
- `ReceiptSummary` projection struct — façade-side (Layer B); reads canonical `Receipt` + extension payload.

## Specification

### §S1 — Typed-discriminator extension pattern (canonical)

RFC-0014-v2 §S1 pins the extension pattern as the canonical mechanism for new receipt semantics.

**Pattern:** A new receipt kind `K` is encoded as:

- `Receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/<K>/v1/" || canonical_ask_id_bytes)`

The `ask_id` field is a 32-byte BLAKE3-256 digest. By extending the input with a namespace string prefix, the digest acts as a typed-discriminator: the first 32 bytes of input are the extension namespace identifier, the remainder is the canonical ask identifier.

**Why this works:** The `ask_id` field is already present on `Receipt` (RFC-0014 §Data Structures). Domain crates construct `ask_id` by hashing namespace + canonical ask bytes. Substrate accepts `ask_id` as opaque 32-byte digest (no domain logic). The substrate `verify_receipt_chain` function does NOT interpret `ask_id` semantics — it only validates `receipt_id` monotonicity + `settlement_hash` integrity + `timestamp_unix` monotonicity.

**Extension kind table (canonical):**

| Extension kind             | `ask_id` prefix (BLAKE3-256 input)                                                                             | Substrate version                           |
| -------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| AskSettled                 | `cipherocto/settlement/extension/ask-settled/v1/` (RFC-0014 existing; `ask_id` = digest of canonical ask only) | RFC-0014                                    |
| **AskPartial**             | `cipherocto/settlement/extension/ask-partial/v1/`                                                              | **RFC-0014-v2**                             |
| **AskRejected**            | `cipherocto/settlement/extension/ask-rejected/v1/`                                                             | **RFC-0014-v2**                             |
| **AgentTransitionReceipt** | `cipherocto/settlement/extension/agent-transition-receipt/v1/`                                                 | **RFC-0014-v2** (cross-RFC with RFC-0015-a) |

**Extension kind enumeration is exhaustive for v2.0.0.** Future extensions (v2.1+) are added to this table via subsequent RFCs and do NOT require substrate field additions.

### §S2 — `Receipt` 6-field canonical form

RFC-0014-v2 §S2 pins the existing 6-field `Receipt` struct as the substrate canonical form:

```rust
// crates/octo-settlement-core/src/receipt.rs (RFC-0014-v2 §S2 pinned — no change from RFC-0014)
pub struct Receipt {
    pub receipt_id: u64,
    pub ask_id: [u8; 32],
    pub settlement_hash: [u8; 32],
    pub router_id: String,
    pub router_sig: Vec<u8>,
    pub timestamp_unix: u64,
}
```

**No new fields.** RFC-0014-v2 explicitly rejects adding fields to `Receipt`:

- Adding `model: Option<String>` would require migration of every existing persisted receipt (semver-major + data migration).
- Adding `status: ReceiptStatus` would couple substrate to façade projection enum.
- Adding `capability_root: Option<[u8; 32]>` would change canonical hash computation (`verify_receipt_chain` inputs change → existing chains fail verification).

§S1 typed-discriminator pattern achieves extension semantics without substrate field additions.

### §S3 — `ReceiptId(pub u64)` newtype

RFC-0014-v2 §S3 introduces `ReceiptId` as a type-safe wrapper around `u64`:

```rust
// crates/octo-settlement-core/src/receipt.rs (RFC-0014-v2 §S3 — new newtype)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptId(pub u64);

impl ReceiptId {
    /// Construct from raw monotonic `receipt_id` value.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Extract the raw monotonic value (for substrate persistence).
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}
```

**Substrate contract:**

- `Receipt.receipt_id: u64` field remains substrate-frozen (canonical hash unchanged).
- `ReceiptId` newtype is substrate-side because `receipt_id` is a canonical substrate identifier.
- Façade-level callers use `ReceiptId` for type-safety (e.g. `ReceiptId` parameters to `get_receipt(id: ReceiptId)`); substrate persists raw `u64`.
- Conversion: `ReceiptId::new(receipt.receipt_id)` (façade → substrate boundary).

### §S4 — `AppendOnlyReceiptSink::append` invariants (canonical)

RFC-0014-v2 §S4 pins the following substrate-level invariants that all `AppendOnlyReceiptSink` implementations MUST enforce (parallel to RFC-0012-v2 §S2):

1. **`&mut self` requirement** — already substrate. The trait takes `&mut self` to enforce type-level append-only.

2. **Strict receipt_id monotonicity** — `receipt.receipt_id` MUST equal `last_receipt_id() + 1`. On gap, return `SettlementError::SequenceGap { receipt_id, prev: last_receipt_id }`.

3. **Idempotent re-append** — `receipt.receipt_id == last_receipt_id()` MUST return `SettlementError::AlreadyExists(receipt_id)`. NOT a success.

4. **Canonical `settlement_hash` verification** — implementations MUST recompute `settlement_hash` from canonical ask + extension payload (per §S1) and verify `receipt.settlement_hash == computed`. On mismatch, return `SettlementError::SinkSpecific("settlement_hash mismatch on append")`.

5. **Atomic persistence** — implementations MUST persist the receipt in a transaction-scoped atomic write. Adapter impls (e.g. `StoolapReceiptSink`) MUST wrap persistence in a Stoolap `Transaction`.

6. **`SinkSpecific` boundary** — adapter-specific failures MUST map to `SettlementError::SinkSpecific(String)`. No raw error chains, no adapter-type names leaking past the substrate boundary.

**Substrate contract surface (v2.0.0):**

```rust
// crates/octo-settlement-core/src/sink.rs (RFC-0014-v2 §S4 pinned)
pub trait AppendOnlyReceiptSink {
    fn append(&mut self, receipt: &Receipt) -> Result<(), SettlementError>;
    fn last_receipt_id(&self) -> Result<Option<u64>, SettlementError>;
}
```

### §S5 — `SettlementError` canonical form

RFC-0014-v2 §S5 pins the existing `SettlementError` canonical form:

```rust
// crates/octo-settlement-core/src/error.rs (RFC-0014-v2 §S5 pinned — no change from RFC-0014)
#[derive(Debug, Error)]
pub enum SettlementError {
    #[error("sequence gap: receipt_id {receipt_id} after {prev}")]
    SequenceGap { receipt_id: u64, prev: u64 },

    #[error("receipt_id {0} already persisted")]
    AlreadyExists(u64),

    #[error("sink-specific error: {0}")]
    SinkSpecific(String),
}
```

**Adapter implementations MUST NOT add new variants.** RFC-0014-v2 explicitly rejects:

- New error variants in concrete impls.
- `From<AdapterError> for SettlementError` that leaks adapter-specific structure.
- `SettlementError` payload fields that reference adapter types.

Adapter-specific error chains MUST be scrubbed at the adapter boundary per RFC-0011-a §7.7 Redaction.

### §S6 — Canonical-bytes form (unchanged)

RFC-0014-v2 §S6 pins the existing canonical-bytes form for receipts:

```
[receipt_id (BE u64) | ask_id (32 bytes) | settlement_hash (32 bytes) |
  router_id (UTF-8) | router_sig_len (BE u32) | router_sig (variable bytes) |
  timestamp_unix (BE u64)]
```

**Extensions do NOT add new bytes** to the canonical form. Extension payload (e.g. AskPartial settlement supplemental amount, AskRejected reason) is encoded in domain-crate-side canonical hash inputs and surfaced via façade projection (RFC-0016-a).

### §S7 — Cross-RFC consistency with RFC-0012-v2

RFC-0014-v2 §S7 establishes cross-RFC invariants with RFC-0012-v2 (audit substrate amendment v2):

| RFC-0012-v2 audit event                                                                                    | RFC-0014-v2 receipt extension kind                                                          | Pairing invariant |
| ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ----------------- |
| `AuditEventKind::Insert` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")` | `Receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" |                   | canonical_ask_id)` | Audit event + receipt share the canonical agent transition hash input. Façade-side pairing helper: `audit_event_for_agent_transition_receipt(receipt) -> AuditEvent` |
| `AuditEventKind::Revoke` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")`        | `Receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-rejected/v1/"             |                   | canonical_ask_id)` | Audit redaction event + rejected receipt share the canonical rejection hash input                                                                                    |

**Cross-RFC consistency rule:** For each (audit extension kind, receipt extension kind) pair, the namespace strings are coordinated via RFC-0012-v2 + RFC-0014-v2 paired acceptance. Future extensions follow the same pattern.

## Security Considerations

**SC1. Typed-discriminator collision resistance.** Extension kinds are encoded as `BLAKE3-256(namespace_string || canonical_ask_id)`. Collision resistance equals BLAKE3-256 collision resistance (128-bit security). Cross-extension-kind collisions are equivalent to BLAKE3 preimage attacks.

**SC2. `SinkSpecific` payload scrubbing.** Adapter-specific error messages MUST be scrubbed at the adapter boundary before wrapping into `SettlementError::SinkSpecific`. No raw error chains, no adapter-type names, no leaked path fragments. See RFC-0011-a §7.7 Redaction for canonical 13-pattern list.

**SC3. Settlement hash integrity.** `settlement_hash` field MUST match the canonical computation (substrate-side `verify_receipt_chain` enforces). §S4.4 enforces this on every append; §S6 canonical-bytes form is the substrate-level guarantee.

**SC4. Receipt_id monotonicity.** Strict `receipt_id == last_receipt_id() + 1` requirement prevents both gaps and replay. Re-append returns `AlreadyExists` (NOT silent success) per §S4.3.

**SC5. Atomic persistence.** §S4.5 requires transaction-scoped atomic write. Adapter impls without atomic persistence (e.g. file append without fsync) MUST NOT satisfy this contract.

**SC6. Router signature verification.** Substrate does NOT verify `router_sig` (signature verification is layer B / domain concern per RFC-0014 §Security Considerations). Substrate only persists the bytes; verification happens at domain call boundary (e.g. `octo-settlement::verify_router_sig(receipt, router_pubkey)`).

## Adversary Analysis

**A1. Receipt chain tampering.** Adversary modifies a persisted `Receipt` (e.g. flips `router_id` bytes, modifies `settlement_hash`). `verify_receipt_chain` returns an error for any tampered receipt. Mitigated by §S6 canonical-bytes + §S4.4 settlement hash integrity check.

**A2. Replay attack.** Adversary re-submits last persisted receipt. §S4.3 returns `AlreadyExists`; no duplicate persistence. Adversary cannot bypass via different `receipt_id` because §S4.2 enforces strict monotonicity.

**A3. Sequence gap injection.** Adversary skips `receipt_id` (e.g. submits 5 after 3, skipping 4). §S4.2 returns `SequenceGap { receipt_id: 5, prev: 3 }`. Caller cannot inject gaps.

**A4. Typed-discriminator spoofing.** Adversary constructs receipt with `ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-settled/v1/" || ask)` but with settlement_hash computed from non-canonical partial settlement data. Detection: substrate verifies settlement_hash via §S4.4; if settlement_hash mismatch, append fails. Cross-RFC: domain crate canonicalizes the settlement payload and includes it in `settlement_hash` input.

**A5. Adapter error chain leakage.** Adversary inspects `SettlementError::SinkSpecific(String)` payload to extract adapter-internal structure. Mitigated by §SC2 scrubbing at adapter boundary.

**A6. Atomic persistence failure.** Adapter impl persists receipt but fsync fails. §S4.5 requires transaction-scoped atomic write + fsync; adapter impl without fsync fails SC5 conformance.

**A7. Router signature forgery.** Adversary constructs receipt with valid `receipt_id` (monotonic), valid `settlement_hash` (canonical computation), but forged `router_sig`. Substrate accepts (signature verification is domain concern per SC6). Detection: domain crate calls `verify_router_sig(receipt, router_pubkey)` before treating receipt as authoritative; fails closed.

## Economic Analysis

**E1. Substrate storage cost.** Typed-discriminator pattern reuses existing `ask_id` field. No substrate storage cost increase. Extension payload is encoded in domain-crate-side canonical hash inputs and surfaced via façade projection.

**E2. Adapter implementation cost.** Existing `StoolapReceiptSink` requires no changes for v2.0.0 conformance. §S4 invariants are already substrate-level.

**E3. Domain crate migration cost.** Existing callers that construct `Receipt` directly (e.g. `octo-settlement-core::ask`) must migrate to typed-discriminator helpers for new extension kinds. RFC-0016-a + RFC-0015-a pin the helper signatures.

**E4. `ReceiptId` newtype cost.** Façade-level type-safety improvement at zero substrate cost (newtype wraps existing `u64` field; canonical hash unchanged).

## Compatibility

**C1. Wire format.** `canonical_bytes` unchanged. Existing persisted receipt chains remain verifiable via `verify_receipt_chain`.

**C2. Source compatibility.** `Receipt` retains its 6-field public surface. `SettlementError` retains its 3-variant form. New `ReceiptId` newtype is additive.

**C3. Adapter compatibility.** Existing `StoolapReceiptSink` impl remains RFC-0014-v2 conformant. No adapter rewrite required.

**C4. Crate version.** `octo-settlement-core` advances from v1.x to v2.0.0 (semver-major). Per CLAUDE.md §Layer A stability rules, this is a one-time major bump for the typed-discriminator + sink invariant codification.

## Test Vectors

### TV-SET-v2-1: Typed-discriminator construction

```
input: extension_kind = "ask-partial", canonical_ask_id = "0x..."
expect: Receipt { ask_id: BLAKE3-256("cipherocto/settlement/extension/ask-partial/v1/" || canonical_ask_id), ... }
```

### TV-SET-v2-2: ReceiptId newtype round-trip

```
input: receipt.receipt_id = 42
expect: ReceiptId::new(42).get() == 42
        ReceiptId::new(42) == ReceiptId::new(42)
        ReceiptId::new(42) != ReceiptId::new(43)
```

### TV-SET-v2-3: Sink append accepts canonical settlement_hash

```
input: receipt with receipt.settlement_hash = compute_settlement_hash(&receipt)
expect: append returns Ok(())
        last_receipt_id() returns Some(receipt.receipt_id)
```

### TV-SET-v2-4: Sink append rejects tampered settlement_hash

```
input: receipt with receipt.settlement_hash = [0x00; 32] (not equal to compute_settlement_hash)
expect: append returns Err(SettlementError::SinkSpecific("settlement_hash mismatch on append"))
```

### TV-SET-v2-5: Sink append rejects sequence gap

```
input: append receipt_id=1, then receipt_id=3 (skipping 2)
expect: first append returns Ok(())
        second append returns Err(SettlementError::SequenceGap { receipt_id: 3, prev: 1 })
```

### TV-SET-v2-6: Sink append rejects re-append (idempotency)

```
input: append receipt_id=1, then append receipt_id=1 again
expect: first append returns Ok(())
        second append returns Err(SettlementError::AlreadyExists(1))
```

### TV-SET-v2-7: Cross-RFC pairing (agent transition receipt ↔ audit event)

```
input: canonical_ask_id_bytes = "did:example:agent/123" (UTF-8 bytes)
       receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" || canonical_ask_id_bytes)
expect: audit_event_for_agent_transition_receipt(receipt).cap_root_hash ==
        BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")
        audit_event_for_agent_transition_receipt(receipt).event_kind == AuditEventKind::Insert
```

## Alternatives Considered

**Alt-A: Add `ReceiptStatus` as substrate field.** Rejected per CLAUDE.md §Extension over enumeration + §Substrate-faithful — substrate-frozen struct cannot grow fields. §S1 typed-discriminator via `ask_id` namespace achieves extension semantics without field additions.

**Alt-B: Add `model`, `cost_dqa`, `capability_root`, `subject_did` as `Receipt` fields.** Rejected per §S2 explicit prohibition — adding fields forces semver-major + data migration of every existing persisted receipt.

**Alt-C: Add `ReceiptSummary` projection at substrate.** Rejected — projection structs are façade concerns (Layer B). Substrate stays free of projection logic.

**Alt-D: Add `ReceiptStatus` enum at substrate.** Rejected — status is recovered from `ask_id` typed-discriminator at façade boundary; substrate stays free of status interpretation logic.

**Alt-E: Pin §S4 invariants at adapter layer only.** Rejected — substrate-level invariants ensure adapter impls cannot accidentally weaken the contract. Adapter-level enforcement is unenforced.

## Implementation Phases

### Phase 1: Substrate `octo-settlement-core` v2.0.0 release

- Bump crate version to 2.0.0
- Add §S1 extension kind table to substrate doc-comment
- Add §S2 `Receipt` field boundary doc-comment
- Add §S3 `ReceiptId` newtype (additive — does NOT change `Receipt.receipt_id: u64`)
- Add §S4 invariant doc-comments to `AppendOnlyReceiptSink` trait
- Add §S5 `SettlementError` variant boundary doc-comment
- Add §S6 canonical-bytes form doc-comment
- Add §S7 cross-RFC consistency doc-comment

### Phase 2: Façade `octo-settlement` v1.x → v2.0.0 release

- Bump crate version to 2.0.0
- Add `octo-settlement-core` v2.0.0 dep
- Re-export `ReceiptId` newtype
- Add typed-discriminator helper functions at façade root (e.g. `receipt_for_ask_partial(...)`, `receipt_for_agent_transition_receipt(...)`)
- Add façade-side `ReceiptStatus` enum (recovered from `ask_id` typed-discriminator)
- Add façade-side `ReceiptSummary` projection struct

### Phase 3: Adapter `StoolapReceiptSink` conformance verification

- Verify existing adapter impl satisfies §S4.5 atomic persistence
- Verify existing adapter impl scrubs errors per §SC2
- Add test vectors TV-SET-v2-3 through TV-SET-v2-6

### Phase 4: Cross-crate migration

- RFC-0016-a amendment acceptance depends on Phase 1+2+3 completion
- RFC-0015-a amendment acceptance depends on Phase 1+2+3 completion

## Key Files to Modify

- `crates/octo-settlement-core/src/receipt.rs` — add `ReceiptId` newtype (§S3) + doc-comment updates for §S1 + §S2
- `crates/octo-settlement-core/src/sink.rs` — doc-comment updates for §S4 invariants
- `crates/octo-settlement-core/src/error.rs` — doc-comment updates for §S5 variant boundary
- `crates/octo-settlement-core/src/chain.rs` — doc-comment updates for §S6 canonical-bytes form
- `crates/octo-settlement-core/Cargo.toml` — version bump 1.x → 2.0.0
- `crates/octo-settlement/src/lib.rs` — version bump + `ReceiptStatus` + `ReceiptSummary` projection
- `crates/octo-settlement/Cargo.toml` — `octo-settlement-core` dep bump to 2.0.0

## Future Work

**FW1. RFC-0014-v3.** Subsequent typed-discriminator extensions (e.g. AskRefunded, AskPartialSupplement, AskSettledWithRebate) follow the §S1 pattern. Each extension kind is added to the table via a new RFC; no substrate field additions.

**FW2. Cross-crate extension helpers.** Domain crates (e.g. `octo-wallet`, `octo-vault`) provide typed-discriminator construction helpers (`receipt_for_ask_settled`, `receipt_for_ask_partial`). Façade `octo-settlement` re-exports for cross-crate use.

**FW3. ReceiptStatus state machine.** Façade-side `ReceiptStatus` enum + transition rules (e.g. `Partial` → `Ok` after supplemental settlement). Future RFC pins the state machine; not in RFC-0014-v2 scope.

**FW4. Audit-receipt pairing helpers.** Cross-crate helper `audit_event_for_agent_transition_receipt(receipt) -> AuditEvent` pins the §S7 pairing invariant at the implementation level.

## Rationale

RFC-0014-v2 codifies the typed-discriminator extension pattern as the canonical substrate-faithful mechanism for new receipt semantics. This achieves three goals:

1. **Layer A frozen preservation** — `Receipt` stays 6-field; `SettlementError` stays 3-variant. No substrate churn from extension additions.
2. **Forward compatibility** — `#[non_exhaustive]` discipline means downstream consumers continue to compile (substrate-stored receipts remain forward-compatible).
3. **Type safety** — typed-discriminator namespaces are BLAKE3-256 digests (128-bit collision resistance); cross-extension-kind spoofing requires BLAKE3 preimage attack.

The `ReceiptId` newtype adds façade-level type safety without changing canonical substrate form. `ReceiptStatus` + `ReceiptSummary` move to façade (Layer B) where projection logic naturally lives.

The cost is a doc-comment-driven extension pattern that domain crates must follow. This is acceptable per CLAUDE.md §Extension over enumeration — the substrate stays minimal; extension mechanics live at the façade/domain boundary.

## Version History

| Version      | Date       | Author               | Notes                                                                                                                                                  |
| ------------ | ---------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v2.0.0-draft | 2026-09-11 | CipherOcto core team | Initial draft. Pins typed-discriminator pattern via `ask_id` namespaces, `ReceiptId` newtype, sink invariants, cross-RFC consistency with RFC-0012-v2. |

## Related RFCs

- RFC-0014 (accepted) — parent RFC; defines substrate `Receipt`, `Ask`, `Reservation`, `SettlementStore`, `AppendOnlyReceiptSink`, `SettlementError`.
- RFC-0012-v2 (draft) — sibling substrate amendment for audit extension pattern.
- RFC-0015-a (draft) — wallet agent write-path amendment; requires RFC-0014-v2 for `ReceiptId` + `ask_id` typed-discriminator.
- RFC-0016-a (draft) — audit receipt write-path amendment; requires RFC-0014-v2 for `ReceiptId` newtype + `ReceiptSummary` projection.
- RFC-0011-a (accepted) — wallet subcommands; cross-RFC reference for scrubber location (Layer B façade, not Layer A substrate).
- RFC-0959 (accepted) — settlement data structures + state machines (parent design).

## Appendices

### Appendix A: Extension kind namespace registry

| Extension kind         | Namespace string                                                                                                                       | `ask_id` prefix bytes (BLAKE3-256 input prefix) |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| AskSettled             | `cipherocto/settlement/extension/ask-settled/v1/` (RFC-0014 existing; `ask_id` = digest of canonical ask only — namespace is implicit) | (RFC-0014 existing)                             |
| AskPartial             | `cipherocto/settlement/extension/ask-partial/v1/`                                                                                      | RFC-0014-v2 new                                 |
| AskRejected            | `cipherocto/settlement/extension/ask-rejected/v1/`                                                                                     | RFC-0014-v2 new                                 |
| AgentTransitionReceipt | `cipherocto/settlement/extension/agent-transition-receipt/v1/`                                                                         | RFC-0014-v2 new (cross-RFC with RFC-0015-a)     |

### Appendix B: Substrate-vs-façade boundary

| Concern                                | Substrate (Layer A)           | Façade (Layer B)                               |
| -------------------------------------- | ----------------------------- | ---------------------------------------------- |
| `Receipt` struct                       | owned                         | re-export                                      |
| `ReceiptId` newtype                    | owned                         | re-export                                      |
| `SettlementError` enum                 | owned                         | re-export                                      |
| `AppendOnlyReceiptSink` trait          | owned                         | re-export + concrete `StoolapReceiptSink` impl |
| `verify_receipt_chain`                 | owned                         | re-export                                      |
| `receipt_id_for`                       | owned                         | re-export                                      |
| `ReceiptStatus` enum                   | NOT owned (façade projection) | owned (Layer B projection)                     |
| `ReceiptSummary` projection            | NOT owned (façade projection) | owned (Layer B projection)                     |
| Typed-discriminator helpers            | doc-comment only              | owned (`receipt_for_*` functions)              |
| Storage adapter (`StoolapReceiptSink`) | NOT owned                     | owned (Layer B → Layer D adapter)              |

RFC-0014-v2 explicitly pins this boundary. Substrate stays free of projection logic, status interpretation, and storage adapter logic — all three are Layer B (or Layer D) concerns per CLAUDE.md §Layer direction rule.

### Appendix C: Cross-RFC consistency table

| RFC-0012-v2 audit event kind                                                   | RFC-0014-v2 receipt extension kind                                                        | Façade pairing helper                                             |
| ------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `AgentTransition` (Insert + `cipherocto/audit/extension/agent-transition/v1/`) | `AgentTransitionReceipt` (`cipherocto/settlement/extension/agent-transition-receipt/v1/`) | `audit_event_for_agent_transition_receipt(receipt) -> AuditEvent` |
| `Redaction` (Revoke + `cipherocto/audit/extension/redaction/v1/`)              | `AskRejected` (`cipherocto/settlement/extension/ask-rejected/v1/`)                        | `audit_event_for_ask_rejected(receipt) -> AuditEvent`             |

Cross-RFC pairing rules:

- Audit event + receipt share a canonical hash input prefix (the extension namespace string).
- Façade-side helper functions recover the pairing invariant at the implementation level.
- Substrate stays free of cross-RFC pairing logic — both substrates accept opaque 32-byte digests.
