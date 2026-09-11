# RFC-0012-v2 — Audit Substrate Amendment v2

| Field        | Value                                                                                            |
| ------------ | ------------------------------------------------------------------------------------------------ |
| Status       | Draft                                                                                            |
| Version      | v2.0.0-draft                                                                                     |
| Layer        | A (substrate-frozen)                                                                             |
| Authors      | CipherOcto core team                                                                             |
| Maintainers  | octo-audit-core maintainers                                                                      |
| Parent RFC   | RFC-0012 (accepted)                                                                              |
| Supersedes   | RFC-0012 §Data Structures (none yet; v2 codifies the extension surface)                          |
| Companion    | RFC-0014-v2 (settlement substrate amendment v2), RFC-0015-a + RFC-0016-a (write-path amendments) |
| Target crate | `octo-audit-core` v2.0.0 (semver-major)                                                          |

## Summary

RFC-0012-v2 is a **Layer A substrate amendment** to `octo-audit-core` that:

1. **Pins the typed-discriminator extension pattern** for new `AuditEventKind` semantics — extensions land via `cap_root_hash` typed-discriminator namespaces per `cipherocto/audit/extension/<kind>/v1/`, NOT via central `AuditEventKind` enum edits.
2. **Pins the canonical `chain_hash` computation contract** — `AppendOnlyAuditSink::append` implementations MUST call `octo_audit_core::compute_chain_hash` to populate `event.chain_hash` before persistence.
3. **Pins the `AppendOnlyAuditSink::append` invariants** — `&mut self` requirement (already substrate), strict `event_id == last_event_id() + 1` monotonicity, atomic persistence.
4. **Pins `AuditError` 3-variant canonical form** — `SequenceGap`, `AlreadyExists`, `SinkSpecific`. Adapter-specific failures MUST map to `SinkSpecific(String)`; no new variants.
5. **Pins `AuditFilter` substrate contract** — `{ since_unix, until_unix, capability_root, model, limit }` UNCONDITIONAL, no `subject_did`, no `status`.

Per CLAUDE.md §Architectural Principles + §Extension over enumeration, RFC-0012-v2 enables future audit event semantics (agent transitions, redactions, capability mints) without modifying `octo_audit_core::AuditEventKind` (which is `#[non_exhaustive]` and substrate-frozen).

## Dependencies

- **RFC-0012** (parent, accepted) — defines `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`.
- **RFC-0014-v2** (sibling, draft) — pins settlement substrate extension pattern; cross-RFC consistency on canonical receipt-extension interaction with audit-event extensions.
- **RFC-0015-a** (write-path amendment, draft) — requires RFC-0012-v2 to land the typed-discriminator for `AgentTransition` events.
- **RFC-0016-a** (write-path amendment, draft) — requires RFC-0012-v2 for `ChainHash` newtype + canonical-bytes-on-write write-path.

## Design Goals

**G1.** Enable future audit event semantics without modifying the substrate `AuditEventKind` enum.
**G2.** Preserve byte-identical canonical-bytes serialization form for existing `Insert / Revoke / Sync` events.
**G3.** Maintain `#[non_exhaustive]` discipline on `AuditEventKind` so substrate-stored events remain forward-compatible with future extensions.
**G4.** Lock the substrate contract surface so adapter implementations (`StoolapAuditSink` in `octo-audit/src/storage/stoolap.rs`) have no ambiguity on what they must enforce.

## Motivation

RFC-0012 (accepted) defines a 3-variant `AuditEventKind` (`Insert / Revoke / Sync`) marked `#[non_exhaustive]`. The substrate doc-comment states "extension variants land via typed-discriminator namespaces ... NOT via central enum edits". However:

- RFC-0015-a (write-path) requires an `AgentTransition` event kind for `transition_agent`.
- RFC-0016-a (write-path) requires a `ChainHash` newtype and canonical-bytes-on-write invariant.
- R28 review of RFC-0015 + RFC-0016 surfaced 5+ contradictions between substrate-frozen surface and write-path requirements.

RFC-0012-v2 codifies the typed-discriminator pattern so write-path amendments can reference a pinned canonical substrate form. Future extensions (e.g. `Redaction`, `CapabilityMint`, `CapabilityAttenuate`) follow the same pattern without substrate enum edits.

## Roles and Authorities

| Role                                   | Authority                                                                                                                                     |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `octo-audit-core` (substrate, Layer A) | Owns `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`, `compute_chain_hash`, `verify_chain`                                |
| `octo-audit` (façade, Layer B)         | Re-exports substrate canonical types + implements `StoolapAuditSink` (Layer D adapter)                                                        |
| Domain callers (Layer B / C / D)       | Construct `AuditEvent` instances via typed-discriminator helpers (e.g. `audit_event_for_agent_transition(...)` in `octo-wallet/src/audit.rs`) |

## Specification

### §S1 — Typed-discriminator extension pattern (canonical)

RFC-0012-v2 §S1 pins the extension pattern as the canonical mechanism for new audit event semantics.

**Pattern:** A new event kind `K` is encoded as:

- `event.event_kind = AuditEventKind::Insert` (or `Revoke` / `Sync` — existing variant closest to the semantics)
- `event.cap_root_hash = BLAKE3-256("cipherocto/audit/extension/<K>/v1/")`

The `cap_root_hash` field acts as a typed-discriminator: the 32-byte digest identifies the extension kind, while the existing 3-variant enum tag identifies the substrate-level action class (Insert = state-changing mutation, Revoke = state-removal, Sync = cross-replica synchronization).

**Why this works:** The `cap_root_hash` field is already present on `AuditEvent` (RFC-0012 §Data Structures). Using it as a typed-discriminator does NOT require adding new fields, does NOT modify the substrate enum, and does NOT change `canonical_bytes` (which already includes `cap_root_hash` in its hash input per `octo-audit-core::canonical_bytes`).

**Extension kind table (canonical):**

| Extension kind      | `event_kind` | `cap_root_hash` (BLAKE3-256 of namespace string)                 | Added in            |
| ------------------- | ------------ | ---------------------------------------------------------------- | ------------------- |
| CapabilityInsert    | `Insert`     | `BLAKE3-256("cipherocto/audit/extension/capability-insert/v1/")` | RFC-0012 (existing) |
| CapabilityRevoke    | `Revoke`     | `BLAKE3-256("cipherocto/audit/extension/capability-revoke/v1/")` | RFC-0012 (existing) |
| Sync                | `Sync`       | `BLAKE3-256("cipherocto/audit/extension/sync/v1/")`              | RFC-0012 (existing) |
| **AgentTransition** | `Insert`     | `BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")`  | **RFC-0012-v2**     |
| **Redaction**       | `Revoke`     | `BLAKE3-256("cipherocto/audit/extension/redaction/v1/")`         | **RFC-0012-v2**     |

**Extension kind enumeration is exhaustive for v2.0.0.** Future extensions (v2.1+) are added to this table via subsequent RFCs and do NOT require substrate enum edits.

### §S2 — `AppendOnlyAuditSink::append` invariants (canonical)

RFC-0012-v2 §S2 pins the following substrate-level invariants that all `AppendOnlyAuditSink` implementations MUST enforce:

1. **`&mut self` requirement** — already substrate. The trait takes `&mut self` to enforce type-level append-only.

2. **Strict event_id monotonicity** — `event.event_id` MUST equal `last_event_id() + 1`. On gap, return `AuditError::SequenceGap { event_id, prev: last_event_id }`.

3. **Idempotent re-append** — `event.event_id == last_event_id()` (caller re-submits last event) MUST return `AuditError::AlreadyExists(event_id)`. NOT a success.

4. **Canonical `chain_hash` computation** — implementations MUST call `octo_audit_core::compute_chain_hash(&event)` to compute the canonical chain hash, then verify `event.chain_hash == computed` (rejects tampered `chain_hash` field). On mismatch, return `AuditError::SinkSpecific("chain_hash mismatch on append")`.

5. **Atomic persistence** — implementations MUST persist the event in a transaction-scoped atomic write. Adapter impls (e.g. `StoolapAuditSink`) MUST wrap persistence in a Stoolap `Transaction` such that the event is either fully persisted (visible to subsequent `last_event_id()` calls) or not at all.

6. **`SinkSpecific` boundary** — adapter-specific failures (e.g. Stoolap transaction aborted, IO error) MUST map to `AuditError::SinkSpecific(String)`. The `String` payload is the substrate-canonical scrubbed message (no raw error chains, no adapter-type names leaking past the substrate boundary).

**Substrate contract surface (v2.0.0):**

```rust
// crates/octo-audit-core/src/sink.rs (RFC-0012-v2 §S2 pinned)
pub trait AppendOnlyAuditSink {
    fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError>;
    fn last_event_id(&self) -> Result<Option<u64>, AuditError>;
}
```

### §S3 — `AuditError` canonical 3-variant form

RFC-0012-v2 §S3 pins the existing 3-variant form as the substrate canonical contract:

```rust
// crates/octo-audit-core/src/error.rs (RFC-0012-v2 §S3 pinned)
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("sequence gap: event_id {event_id} after {prev}")]
    SequenceGap { event_id: u64, prev: u64 },
    #[error("event_id {0} already persisted")]
    AlreadyExists(u64),
    #[error("sink-specific error: {0}")]
    SinkSpecific(String),
}
```

**Adapter implementations MUST NOT add new variants.** RFC-0012-v2 explicitly rejects:

- New error variants in concrete impls (e.g. `StoolapAuditError::TransactionAborted`).
- `From<AdapterError> for AuditError` that leaks adapter-specific structure via `SinkSpecific`.
- `AuditError` payload fields that reference adapter types.

Adapter-specific error chains MUST be scrubbed at the adapter boundary per RFC-0012 §Trait G3 mitigation.

### §S4 — `AuditFilter` substrate contract (canonical)

RFC-0012-v2 §S4 pins the substrate-level `AuditFilter` projection contract:

```rust
// crates/octo-audit/src/lib.rs (RFC-0012-v2 §S4 pinned — façade projection)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    pub since_unix: Option<u64>,
    pub until_unix: Option<u64>,
    pub capability_root: Option<[u8; 32]>,
    pub model: Option<String>,
    pub limit: Option<usize>,
}
```

**Substrate constraints:**

- **NO `subject_did` field** — `subject_did` ACL filtering is RFC-0016-a concern (façade projection), not substrate. The substrate reads/writes audit events without subject-level ACL (per-event ACL is enforced at the façade boundary).
- **NO `status` field** — `status` filtering requires RFC-0014-v2 `ReceiptStatus` extension; cross-RFC consistency on audit/receipt status is not in v2.0.0 substrate scope.
- **`capability_root` is `Option<[u8; 32]>`** — typed extension discriminator hash; matches §S1 extension kind table.
- **`limit` is `Option<usize>`** — substrate applies a hard ceiling of 1024 (per RFC-0012 §Data Structures).

### §S5 — `AuditEvent` struct (unchanged)

RFC-0012-v2 §S5 pins the existing 7-field `AuditEvent` struct:

```rust
// crates/octo-audit-core/src/event.rs (RFC-0012-v2 §S5 pinned — no change from RFC-0012)
pub struct AuditEvent {
    pub event_id: u64,
    pub node_did: String,
    pub event_kind: AuditEventKind,
    pub cap_root_hash: [u8; 32],
    pub at_millis_unix: u64,
    pub prev_chain_hash: [u8; 32],
    pub chain_hash: [u8; 32],
}
```

**No new fields.** RFC-0012-v2 explicitly rejects adding fields to `AuditEvent` (semver-major bump would force every existing consumer to migrate; typed-discriminator §S1 achieves the same without substrate change).

### §S6 — Canonical-bytes form (unchanged)

RFC-0012-v2 §S6 pins the existing canonical-bytes form:

```
[event_id (BE u64) | node_did (UTF-8) | event_kind (tag byte) |
  cap_root_hash (32 bytes) | at_millis_unix (BE u64) | prev_chain_hash (32 bytes)]
```

`chain_hash` is excluded (it IS the hash of the other fields + the `prev_chain_hash`).

**Extensions do NOT add new bytes** to the canonical form. The `cap_root_hash` already encodes the typed-discriminator; the extension payload (e.g. agent transition fields) is encoded in the canonical hash via extension-specific helpers in the calling domain crate (e.g. `octo-wallet::audit::canonical_bytes_agent_transition(...)`).

## Security Considerations

**SC1. Typed-discriminator collision resistance.** Extension kinds are encoded as `BLAKE3-256(namespace_string)`. Collision resistance equals BLAKE3-256 collision resistance (128-bit security). Cross-extension-kind collisions are equivalent to BLAKE3 preimage attacks.

**SC2. `SinkSpecific` payload scrubbing.** Adapter-specific error messages MUST be scrubbed at the adapter boundary before wrapping into `AuditError::SinkSpecific`. No raw error chains, no adapter-type names, no leaked path fragments. See RFC-0011-a §7.7 Redaction for canonical 13-pattern list.

**SC3. Chain hash integrity.** `chain_hash` field MUST match `compute_chain_hash(event)`. §S2.4 enforces this on every append; §S6 canonical-bytes form is the substrate-level guarantee.

**SC4. Event_id monotonicity.** Strict `event_id == last_event_id() + 1` requirement prevents both gaps (sequence skip) and replay (duplicate). Re-append returns `AlreadyExists` (NOT silent success) per §S2.3.

**SC5. Atomic persistence.** §S2.5 requires transaction-scoped atomic write. Adapter impls without atomic persistence (e.g. file append without fsync) MUST NOT satisfy this contract.

## Adversary Analysis

**A1. Audit chain tampering.** Adversary modifies a persisted `AuditEvent` (e.g. flips `event_kind` tag byte, modifies `node_did`). `verify_chain` returns `AuditChainError::HashMismatch` for any tampered event. Mitigated by §S6 canonical-bytes + §S2.4 chain hash integrity check.

**A2. Replay attack.** Adversary re-submits last persisted event. §S2.3 returns `AlreadyExists`; no duplicate persistence. Adversary cannot bypass via different `event_id` because §S2.2 enforces strict monotonicity.

**A3. Sequence gap injection.** Adversary skips `event_id` (e.g. submits 5 after 3, skipping 4). §S2.2 returns `SequenceGap { event_id: 5, prev: 3 }`. Caller cannot inject gaps.

**A4. Typed-discriminator spoofing.** Adversary constructs event with `event_kind = Insert` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")` but with non-canonical agent-transition payload. Detection: domain crate canonicalizes the agent-transition payload and includes it in extension-specific hash (e.g. `prev_chain_hash` field carries the payload hash). Cross-RFC: RFC-0015-a §Audit row contract pins the canonical payload encoding.

**A5. Adapter error chain leakage.** Adversary inspects `AuditError::SinkSpecific(String)` payload to extract adapter-internal structure (Stoolap transaction IDs, IO paths). Mitigated by §SC2 scrubbing at adapter boundary.

**A6. Atomic persistence failure.** Adapter impl persists event but fsync fails. Adversary reads event from another connection but post-crash the event is missing. §S2.5 requires transaction-scoped atomic write + fsync; adapter impl without fsync fails SC5 conformance.

## Economic Analysis

**E1. Substrate storage cost.** Typed-discriminator pattern reuses existing `cap_root_hash` field. No substrate storage cost increase. Extension payload is encoded in the canonical hash via domain-specific helpers, NOT stored as new columns.

**E2. Adapter implementation cost.** Existing `StoolapAuditSink` requires no changes for v2.0.0 conformance. §S2 invariants are already substrate-level (Rust borrow checker + `compute_chain_hash`).

**E3. Domain crate migration cost.** Existing callers that construct `AuditEvent` directly (e.g. `octo-wallet::capability::audit_log`) must migrate to typed-discriminator helpers for new extension kinds. RFC-0015-a + RFC-0016-a pin the helper signatures.

## Compatibility

**C1. Wire format.** `canonical_bytes` unchanged. Existing persisted audit chains remain verifiable via `verify_chain`.

**C2. Source compatibility.** `AuditEvent`, `AuditEventKind`, `AuditError`, `AppendOnlyAuditSink` retain their public surface. `#[non_exhaustive]` discipline means downstream `match` expressions continue to compile (substrate-stored events stay forward-compatible).

**C3. Adapter compatibility.** Existing `StoolapAuditSink` impl remains RFC-0012-v2 conformant. No adapter rewrite required.

**C4. Crate version.** `octo-audit-core` advances from v1.x to v2.0.0 (semver-major). Per CLAUDE.md §Layer A stability rules, this is a one-time major bump for the typed-discriminator + sink invariant codification.

## Test Vectors

### TV-AUD-v2-1: Typed-discriminator construction

```
input: extension_kind = "agent-transition"
expect: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/") = 0x...
        event_kind = AuditEventKind::Insert
```

### TV-AUD-v2-2: Sink append accepts canonical chain_hash

```
input: event with event.chain_hash = compute_chain_hash(&event)
expect: append returns Ok(())
        last_event_id() returns Some(event.event_id)
```

### TV-AUD-v2-3: Sink append rejects tampered chain_hash

```
input: event with event.chain_hash = [0x00; 32] (not equal to compute_chain_hash)
expect: append returns Err(AuditError::SinkSpecific("chain_hash mismatch on append"))
```

### TV-AUD-v2-4: Sink append rejects sequence gap

```
input: append event_id=1, then event_id=3 (skipping 2)
expect: first append returns Ok(())
        second append returns Err(AuditError::SequenceGap { event_id: 3, prev: 1 })
```

### TV-AUD-v2-5: Sink append rejects re-append (idempotency)

```
input: append event_id=1, then append event_id=1 again
expect: first append returns Ok(())
        second append returns Err(AuditError::AlreadyExists(1))
```

### TV-AUD-v2-6: AuditFilter substrate contract

```
input: AuditFilter { subject_did: Some("did:example:123"), status: Some(ReceiptStatus::Ok), .. }
expect: Compile error: no field `subject_did` on type `AuditFilter`
        Compile error: no field `status` on type `AuditFilter`
```

## Alternatives Considered

**Alt-A: Add `AuditEventKind::AgentTransition` as 4th variant.** Rejected per CLAUDE.md §Extension over enumeration — central enum edits are upgrade-hostile; future extensions (Redaction, CapabilityMint, etc.) would each require substrate enum edits + canonical_bytes encoding changes. Typed-discriminator §S1 achieves the same without substrate churn.

**Alt-B: Add `extension_payload: Vec<u8>` field to `AuditEvent`.** Rejected — adds storage cost to every event (existing events carry empty Vec), and creates a generic byte-vector that loses type-safety. Typed-discriminator encodes extension kind in `cap_root_hash` (already present) and extension payload in extension-specific helpers.

**Alt-C: Add `AuditEventKind` as fully extensible via trait objects.** Rejected — substrate crate cannot depend on `dyn Trait` (no_std environment + Layer A frozen constraint). Tagged-enum + typed-discriminator achieves extensibility without runtime polymorphism.

**Alt-D: Pin §S2 invariants at adapter layer only.** Rejected — substrate-level invariants ensure adapter impls cannot accidentally weaken the contract. Adapter-level enforcement is unenforced.

## Implementation Phases

### Phase 1: Substrate `octo-audit-core` v2.0.0 release

- Bump crate version to 2.0.0
- Add §S1 extension kind table to substrate doc-comment
- Add §S2 invariant doc-comments to `AppendOnlyAuditSink` trait
- Add §S3 `AuditError` variant boundary doc-comment
- Add §S4 `AuditFilter` field constraint doc-comment
- Add §S5 `AuditEvent` field boundary doc-comment
- Add §S6 canonical-bytes form doc-comment
- All substrate changes are doc-only + version bump (no logic changes)

### Phase 2: Façade `octo-audit` v1.x → v2.0.0 release

- Bump crate version to 2.0.0
- Add `octo-audit-core` v2.0.0 dep
- Re-export unchanged canonical types
- Add typed-discriminator helper functions at façade root (e.g. `audit_event_for_agent_transition(...)`)

### Phase 3: Adapter `StoolapAuditSink` conformance verification

- Verify existing adapter impl satisfies §S2.5 atomic persistence
- Verify existing adapter impl scrubs errors per §SC2
- Add test vectors TV-AUD-v2-2 through TV-AUD-v2-5

### Phase 4: Cross-crate migration

- RFC-0015-a amendment acceptance depends on Phase 1+2+3 completion
- RFC-0016-a amendment acceptance depends on Phase 1+2+3 completion

## Key Files to Modify

- `crates/octo-audit-core/src/event.rs` — doc-comment updates for §S1 + §S5
- `crates/octo-audit-core/src/sink.rs` — doc-comment updates for §S2 invariants
- `crates/octo-audit-core/src/error.rs` — doc-comment updates for §S3 variant boundary
- `crates/octo-audit-core/src/chain.rs` — doc-comment update for §S6 canonical-bytes form
- `crates/octo-audit-core/Cargo.toml` — version bump 1.x → 2.0.0
- `crates/octo-audit/src/lib.rs` — version bump + re-exports
- `crates/octo-audit/Cargo.toml` — `octo-audit-core` dep bump to 2.0.0

## Future Work

**FW1. RFC-0012-v3.** Subsequent typed-discriminator extensions (e.g. `Redaction`, `CapabilityMint`, `CapabilityAttenuate`) follow the §S1 pattern. Each extension kind is added to the table via a new RFC; no substrate enum edits.

**FW2. Cross-crate extension helpers.** Domain crates (e.g. `octo-wallet`, `octo-vault`) provide typed-discriminator construction helpers (`audit_event_for_agent_transition`, `audit_event_for_redaction`). Façade `octo-audit` re-exports for cross-crate use.

**FW3. Receipt-extension cross-RFC invariants.** RFC-0014-v2 pins the `ReceiptStatus` extension pattern. Cross-RFC consistency: `AuditEventKind::Insert` + `cap_root_hash = agent-transition-typed-discriminator` corresponds to a settlement `Receipt { status: ReceiptStatus::Ok }` (or `Partial` for redactions).

## Rationale

RFC-0012-v2 codifies the typed-discriminator extension pattern as the canonical substrate-faithful mechanism for new audit event semantics. This achieves three goals:

1. **Layer A frozen preservation** — `AuditEventKind` stays 3-variant; `AuditEvent` stays 7-field; `AuditError` stays 3-variant. No substrate churn from extension additions.
2. **Forward compatibility** — `#[non_exhaustive]` discipline means downstream `match` expressions on `AuditEventKind` continue to compile (substrate-stored events remain forward-compatible).
3. **Type safety** — typed-discriminator namespaces are BLAKE3-256 digests (128-bit collision resistance); cross-extension-kind spoofing requires BLAKE3 preimage attack.

The cost is a doc-comment-driven extension pattern that domain crates must follow. This is acceptable per CLAUDE.md §Extension over enumeration — the substrate stays minimal; extension mechanics live at the façade/domain boundary.

## Version History

| Version      | Date       | Author               | Notes                                                                                             |
| ------------ | ---------- | -------------------- | ------------------------------------------------------------------------------------------------- |
| v2.0.0-draft | 2026-09-11 | CipherOcto core team | Initial draft. Pins typed-discriminator pattern, sink invariants, AuditFilter substrate contract. |

## Related RFCs

- RFC-0012 (accepted) — parent RFC; defines substrate `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`.
- RFC-0014-v2 (draft) — sibling substrate amendment for settlement extension pattern.
- RFC-0015-a (draft) — wallet agent write-path amendment; requires RFC-0012-v2 for typed-discriminator of `AgentTransition` events.
- RFC-0016-a (draft) — audit receipt write-path amendment; requires RFC-0012-v2 for `ChainHash` newtype + canonical-bytes-on-write invariant.
- RFC-0011-a (accepted) — wallet subcommands; cross-RFC reference for scrubber location (Layer B façade, not Layer A substrate).
- RFC-0010 (accepted) — DID canonical form; substrate stores `node_did: String` (raw canonical wire form per RFC-0010).

## Appendices

### Appendix A: Extension kind namespace registry

| Extension kind   | Namespace string                                   | `cap_root_hash` (BLAKE3-256 hex, first 8 bytes) |
| ---------------- | -------------------------------------------------- | ----------------------------------------------- |
| CapabilityInsert | `cipherocto/audit/extension/capability-insert/v1/` | `0x...` (RFC-0012 existing)                     |
| CapabilityRevoke | `cipherocto/audit/extension/capability-revoke/v1/` | `0x...` (RFC-0012 existing)                     |
| Sync             | `cipherocto/audit/extension/sync/v1/`              | `0x...` (RFC-0012 existing)                     |
| AgentTransition  | `cipherocto/audit/extension/agent-transition/v1/`  | `0x...` (RFC-0012-v2 new)                       |
| Redaction        | `cipherocto/audit/extension/redaction/v1/`         | `0x...` (RFC-0012-v2 new)                       |

The full 32-byte hex digests are computed at crate compile time via `const BLAKE3` and exposed as `pub const` items in `octo-audit-core::extension_kinds`. Domain crates reference these constants instead of recomputing.

### Appendix B: Substrate-vs-façade boundary

| Concern                              | Substrate (Layer A)                    | Façade (Layer B)                             |
| ------------------------------------ | -------------------------------------- | -------------------------------------------- |
| `AuditEvent` struct                  | owned                                  | re-export                                    |
| `AuditEventKind` enum                | owned                                  | re-export                                    |
| `AuditError` enum                    | owned                                  | re-export                                    |
| `AppendOnlyAuditSink` trait          | owned                                  | re-export + concrete `StoolapAuditSink` impl |
| `compute_chain_hash`                 | owned                                  | re-export                                    |
| `verify_chain`                       | owned                                  | re-export                                    |
| `AuditFilter` projection             | NOT owned (façade concern)             | owned (Layer B projection)                   |
| Scrubber                             | NOT owned (RFC-0011-a Layer B concern) | NOT owned (RFC-0011-a Layer B concern)       |
| Typed-discriminator helpers          | doc-comment only                       | owned (`audit_event_for_*` functions)        |
| Storage adapter (`StoolapAuditSink`) | NOT owned                              | owned (Layer B → Layer D adapter)            |

RFC-0012-v2 explicitly pins this boundary. Substrate stays free of projection logic, scrubber logic, and storage adapter logic — all three are Layer B (or Layer D) concerns per CLAUDE.md §Layer direction rule.
