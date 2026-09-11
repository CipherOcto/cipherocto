# RFC-0012-v2 — Audit Substrate Amendment v2

| Field        | Value                                                                   |
| ------------ | ----------------------------------------------------------------------- |
| Status       | Draft                                                                   |
| Version      | v2.0.0-draft                                                            |
| Layer        | A (substrate-frozen)                                                    |
| Authors      | CipherOcto Architecture Working Group                                   |
| Maintainers  | CipherOcto Architecture Working Group                                   |
| Parent RFC   | RFC-0012                                                                |
| Supersedes   | RFC-0012 §Data Structures (none yet; v2 codifies the extension surface) |
| Companion    | RFC-0014-v2, RFC-0015-a + RFC-0016-a                                    |
| Target crate | `octo-audit-core` v2.0.0 (semver-major)                                 |

## Summary

RFC-0012-v2 is a **Layer A substrate amendment** to `octo-audit-core` that:

1. **Pins the typed-discriminator extension pattern** for new `AuditEventKind` semantics — extensions land via `cap_root_hash` typed-discriminator namespaces per `cipherocto/audit/extension/<kind>/v1/`, NOT via central `AuditEventKind` enum edits.
2. **Pins the canonical `chain_hash` computation contract** — `AppendOnlyAuditSink::append` implementations MUST call `octo_audit_core::compute_chain_hash` to populate `event.chain_hash` before persistence.
3. **Pins the `AppendOnlyAuditSink::append` invariants** — `&mut self` requirement (already substrate), strict `event_id == last_event_id() + 1` monotonicity, atomic persistence.
4. **Pins `AuditError` 3-variant canonical form** — `SequenceGap`, `AlreadyExists`, `SinkSpecific`. Adapter-specific failures MUST map to `SinkSpecific(String)`; no new variants.
5. **Pins `AuditFilter` substrate contract** — `{ since_unix, until_unix, capability_root, model, limit }` UNCONDITIONAL, no `subject_did`, no `status`.

Per CLAUDE.md §Architectural Principles + §Extension over enumeration, RFC-0012-v2 enables future audit event semantics (agent transitions, redactions, capability mints) without modifying `octo_audit_core::AuditEventKind` (which is `#[non_exhaustive]` and substrate-frozen).

## Authors

- CipherOcto Architecture Working Group

## Maintainers

- CipherOcto Architecture Working Group

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

RFC-0012 defines a 3-variant `AuditEventKind` (`Insert / Revoke / Sync`) marked `#[non_exhaustive]`. The substrate doc-comment states "extension variants land via typed-discriminator namespaces ... NOT via central enum edits". However:

- RFC-0015-a requires an `AgentTransition` event kind for `transition_agent`.
- RFC-0016-a requires a `ChainHash` newtype and canonical-bytes-on-write invariant.
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

§S1 pins the extension pattern as the canonical mechanism for new audit event semantics.

**Pattern:** A new event kind `K` is encoded as:

- `event.event_kind = AuditEventKind::Insert` (or `Revoke` / `Sync` — existing variant closest to the semantics)
- `event.cap_root_hash = BLAKE3-256("cipherocto/audit/extension/<K>/v1/")`

The `cap_root_hash` field acts as a typed-discriminator: the 32-byte digest identifies the extension kind, while the existing 3-variant enum tag identifies the substrate-level action class (Insert = state-changing mutation, Revoke = state-removal, Sync = cross-replica synchronization).

**Why this works:** The `cap_root_hash` field is already present on `AuditEvent` (RFC-0012 §Data Structures). Using it as a typed-discriminator does NOT require adding new fields, does NOT modify the substrate enum, and does NOT change `canonical_bytes` (which already includes `cap_root_hash` in its hash input per `octo-audit-core::canonical_bytes`).

**Extension kind table (canonical):**

| Extension kind      | `event_kind` | `cap_root_hash` (BLAKE3-256 of namespace string)                 | Added in        |
| ------------------- | ------------ | ---------------------------------------------------------------- | --------------- |
| CapabilityInsert    | `Insert`     | `BLAKE3-256("cipherocto/audit/extension/capability-insert/v1/")` | RFC-0012        |
| CapabilityRevoke    | `Revoke`     | `BLAKE3-256("cipherocto/audit/extension/capability-revoke/v1/")` | RFC-0012        |
| Sync                | `Sync`       | `BLAKE3-256("cipherocto/audit/extension/sync/v1/")`              | RFC-0012        |
| **AgentTransition** | `Insert`     | `BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")`  | **RFC-0012-v2** |
| **Redaction**       | `Revoke`     | `BLAKE3-256("cipherocto/audit/extension/redaction/v1/")`         | **RFC-0012-v2** |

**Extension kind enumeration is exhaustive for v2.0.0.** Future extensions (v2.1+) are added to this table via subsequent RFCs and do NOT require substrate enum edits.

### §S2 — `AppendOnlyAuditSink::append` invariants (canonical)

§S2 pins the following substrate-level invariants that all `AppendOnlyAuditSink` implementations MUST enforce:

1. **`&mut self` requirement** — already substrate. The trait takes `&mut self` to enforce type-level append-only.

2. **Strict event_id monotonicity** — `event.event_id` MUST equal `last_event_id() + 1`. On gap, return `AuditError::SequenceGap { event_id, prev: last_event_id }`.

3. **Idempotent re-append** — `event.event_id == last_event_id()` (caller re-submits last event) MUST return `AuditError::AlreadyExists(event_id)`. NOT a success.

4. **Canonical `chain_hash` computation** — implementations MUST call `octo_audit_core::compute_chain_hash(&event)` to compute the canonical chain hash, then verify `event.chain_hash == computed` (rejects tampered `chain_hash` field). On mismatch, return `AuditError::SinkSpecific("chain_hash mismatch on append")`.

5. **Atomic persistence** — implementations MUST persist the event in a transaction-scoped atomic write. The persistence operation MUST be either fully committed (visible to subsequent `last_event_id()` calls) or fully rolled back (no partial persistence observable). Adapter-specific transaction mechanisms (e.g. database `Transaction` wrappers) are adapter-layer concerns (Layer D), not substrate contract.

6. **`SinkSpecific` boundary** — adapter-specific failures (e.g. Stoolap transaction aborted, IO error) MUST map to `AuditError::SinkSpecific(String)`. The `String` payload is the substrate-canonical scrubbed message (no raw error chains, no adapter-type names leaking past the substrate boundary).

**Substrate contract surface (v2.0.0):**

```rust
// crates/octo-audit-core/src/sink.rs (§S2 pinned)
pub trait AppendOnlyAuditSink {
    fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError>;
    fn last_event_id(&self) -> Result<Option<u64>, AuditError>;
}
```

### §S3 — `AuditError` canonical 3-variant form

§S3 pins the existing 3-variant form as the substrate canonical contract:

```rust
// crates/octo-audit-core/src/error.rs (§S3 pinned)
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

### §S4 — `AuditFilter` façade projection contract (canonical)

§S4 pins the façade-level `AuditFilter` projection contract. **`AuditFilter` is a Layer B façade projection, NOT a Layer A substrate type** (per Appendix B). It lives at `crates/octo-audit/src/lib.rs` (Layer B façade re-export surface) and is constructed by the façade for CLI consumers (RFC-0016-a).

```rust
// crates/octo-audit/src/lib.rs (§S4 pinned — Layer B façade projection;
// RFC-0016-a §AuditFilter declaration references this canonical form)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    pub since_unix: Option<u64>,
    pub until_unix: Option<u64>,
    pub capability_root: Option<[u8; 32]>,
    pub model: Option<String>,
    pub limit: Option<usize>,
}
```

**Note:** As of RFC-0012-v2 draft acceptance, `AuditFilter` does NOT yet exist in `crates/octo-audit/src/lib.rs`. Per §Implementation Phases Phase 2, this struct lands in `octo-audit` v2.0.0 at acceptance time. The §S4 declaration pins the canonical surface that RFC-0016-a §AuditFilter definition must match.

**Façade constraints:**

- **NO `subject_did` field** — `subject_did` ACL filtering is RFC-0016-a concern (façade projection). The façade reads audit events without subject-level ACL (per-event ACL is enforced at the domain call boundary).
- **NO `status` field** — `status` filtering requires RFC-0014-v2 `ReceiptStatus` extension (which is itself a façade projection, NOT a substrate field); cross-RFC consistency on audit/receipt status is not in v2.0.0 scope.
- **`capability_root` is `Option<[u8; 32]>`** — typed extension discriminator hash; matches §S1 extension kind table.
- **`limit` is `Option<usize>`** — façade applies a hard ceiling of 1024 (per RFC-0012 §Data Structures).

### §S5 — `AuditEvent` struct (unchanged)

§S5 pins the existing 7-field `AuditEvent` struct:

```rust
// crates/octo-audit-core/src/event.rs (§S5 pinned — no change from RFC-0012)
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

§S6 pins the existing canonical-bytes form:

```
[event_id (BE u64) | node_did (UTF-8) | event_kind (tag byte) |
  cap_root_hash (32 bytes) | at_millis_unix (BE u64) | prev_chain_hash (32 bytes)]
```

`chain_hash` is excluded (it IS the hash of the other fields + the `prev_chain_hash`).

**Extensions do NOT add new bytes** to the canonical form. The `cap_root_hash` already encodes the typed-discriminator; the extension payload (e.g. agent transition fields) is encoded in the canonical hash via extension-specific helpers in the calling domain crate (e.g. `octo-wallet::audit::canonical_bytes_agent_transition(...)`).

## Implicit Assumptions

- **A1. BLAKE3-256 collision resistance.** Extension kinds are encoded as `BLAKE3-256(namespace_string)`. Finding a collision requires ~2^128 hash evaluations (birthday bound on 256-bit output). Blast radius if broken: cross-extension-kind spoofing (one kind can be reinterpreted as another). Mitigation: substrate accepts only the canonical extension kinds per §S1; extension helper functions at the façade validate kind at construction time.
- **A2. Sink atomicity precondition.** Adapter implementations MUST satisfy §S2.5 atomic persistence. Failure to satisfy means two appenders could observe different total orderings of `event_id`. Blast radius: torn writes, lost audit events. Mitigation: §FW4 crash-injection test infrastructure (future mission); adapter conformance gate at Phase 2 acceptance.
- **A3. BLAKE3 domain-separator commitment.** `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/<K>/v1/")` namespace strings are committed at RFC-0012-v2 acceptance time. Future extensions add new namespace strings; existing ones are immutable. Blast radius: namespace collision would let one extension kind impersonate another. Mitigation: 32-byte digest output + namespace version pinning (`v1/` suffix).
- **A4. `node_did` pre-validation.** The substrate assumes `node_did` field on `AuditEvent` has been pre-validated by the domain caller (DID-format check + signature verification at the wire boundary). Substrate does NOT re-validate. Blast radius: malformed DIDs persist into the audit chain. Mitigation: domain-crate validation at the call boundary + RFC-0009 §Identity substrate verification.
- **A5. Single-writer assumption.** `event_id == last_event_id() + 1` monotonicity check assumes a single logical appender. Concurrent appenders with distinct node_did values are out of v2.0.0 scope; cross-replica append coordination is RFC-0855 territory. Blast radius: sequence gap detection may trigger spuriously under concurrent append. Mitigation: AC-10 + external serialization layer at adapter boundary.

## Determinism Requirements

Per RFC-0008 Execution Class Mapping, all substrate operations in this RFC are **Class A** (deterministic, byte-identical across replicas):

- **`canonical_bytes`** — pure function of input fields; no system calls, no allocation outside the returned Vec.
- **`compute_chain_hash`** — unkeyed BLAKE3-256 over `canonical_bytes`; deterministic across replicas.
- **`verify_chain`** — deterministic replay of `compute_chain_hash` over each persisted event; byte-identical verification result.
- **`last_event_id`** — adapter-side, but substrate-level invariant (returns `Some(0)` for empty table); deterministic.
- **Typed-discriminator construction** — `BLAKE3-256("cipherocto/audit/extension/<K>/v1/")` is a pure function of `<K>`; deterministic.
- **`prev_chain_hash` linkage** — cryptographic chain binding; byte-identical across replicas.

Cross-replica consensus invariant: any two replicas observing the same event sequence MUST produce identical `chain_hash` values. Failure indicates either (a) a serialization drift (adapter-side bug) or (b) a field-level non-determinism (e.g. timestamp drift between insert + compute).

## Performance Targets

- **Append latency ceiling:** 1 ms p99 on commodity SSD (single-event append with monotonicity check + chain_hash compute + persistence). **Measurement methodology:** reproducible benchmark harness required; baseline SSD spec (sequential write ≥500 MB/s, fsync ≤1 ms); warm cache; single-threaded. Benchmark suite lands as part of acceptance adapter mission.
- **Chain verification O(n):** `verify_chain(n)` MUST complete in O(n) time over `n` persisted events; no quadratic scans.
- **Adapter throughput floor:** 1,000 appends/second sustained single-writer; 100 appends/second under 4-way concurrent append with monotonicity serialization.
- **Canonical-bytes cost:** `canonical_bytes` MUST NOT allocate more than 256 bytes per event (input fields fit in fixed-width encoding).
- **`compute_chain_hash` cost:** unkeyed BLAKE3-256 over ~96-byte input; expected <10 µs per call on commodity CPU.
- **Storage adapter memory ceiling:** sink implementations MUST NOT hold more than 64 KiB per append in transient buffers.

## Acceptance Criteria

The RFC is Accepted when ALL of the following are true:

- **AC-1.** `octo_audit_core::AuditEvent` retains its 7-field public surface (no field additions; semver-major bump from v1.x to v2.0.0).
- **AC-2.** `octo_audit_core::AuditError` retains its 3-variant form (`SequenceGap`, `AlreadyExists`, `SinkSpecific`); no new variants.
- **AC-3.** `compute_chain_hash(&AuditEvent) == BLAKE3-256(canonical_bytes(event))` — verified by `verify_chain` round-trip.
- **AC-4.** Strict `event_id == last_event_id() + 1` enforced; re-append returns `AlreadyExists`; gap returns `SequenceGap`.
- **AC-5.** Tampered `chain_hash` returns `AuditError::SinkSpecific("chain_hash mismatch on append")` per §S2.4 (NOT raw substrate `AuditChainError::HashMismatch` leaked; `verify_chain` returns `AuditChainError`, `append` returns `AuditError` — distinct error envelopes, no cross-leakage).
- **AC-6.** `AuditFilter` is Layer B façade projection (does NOT exist in substrate; lands at acceptance per Phase 2); 5-field form `{ since_unix, until_unix, capability_root, model, limit }` UNCONDITIONAL, no `subject_did`, no `status`.
- **AC-7.** Adapter implementations call the canonical scrubber before wrapping into `SinkSpecific`; each Layer B façade owns its own scrubber instance (e.g. `octo_audit::scrub::scrub_adapter_error`, `octo_settlement::scrub::scrub_adapter_error` — pattern duplicated per-façade to avoid sibling Layer B coupling). Raw error chains never reach substrate.
- **AC-8.** Paired acceptance with RFC-0014-v2 per BLUEPRINT.md §2-Cycle Atomic Promotion gate.
- **AC-9.** All Test Vectors in §Test Vectors produce expected outputs (verified by `cargo test -p octo-audit`).
- **AC-10.** Layer D adapter implementations (e.g. `StoolapAuditSink`) provide transaction-scoped atomic write with persistence durability; concurrent appenders detected by the substrate monotonicity pre-check (adapter-side serialization is the adapter's responsibility).

## 2-Cycle Atomic Promotion Tag

Per BLUEPRINT.md §RFC Process item 5 + §2-Cycle Atomic Promotion gate:

- **Sibling:** RFC-0014-v2
- **Reviewer board:** 5-lens reviewer board (correctness / security / layer-model / hygiene / spec-completeness)
- **Pairing invariant:** §S7 cross-RFC pairing via `prev_chain_hash = receipt_id_for(receipt)` requires both substrate amendments to land together. RFC-0015-a + RFC-0016-a acceptance gated on this 2-cycle.
- **Atomic promotion gate:** Both RFCs transition Draft → Accepted in the same PR. Neither may be Accepted without the other.

## Security Considerations

**SC1. Typed-discriminator collision resistance.** Extension kinds are encoded as `BLAKE3-256(namespace_string)`. BLAKE3-256 collision resistance is 2^128 operations (birthday bound on 256-bit output). Preimage resistance is 2^256. Cross-extension-kind collisions are cross-prefix second-preimage attacks (~2^256 with one fixed prefix; ~2^128 birthday for attacker-chosen both prefixes).

**SC2. `SinkSpecific` payload scrubbing.** Adapter-specific error messages MUST be scrubbed at the adapter boundary before wrapping into `AuditError::SinkSpecific`. No raw error chains, no adapter-type names, no leaked path fragments. Canonical scrubber pattern is declared at each Layer B façade (`octo_audit::scrub::scrub_adapter_error` + `octo_settlement::scrub::scrub_adapter_error` — duplicated per-façade to avoid sibling Layer B coupling) with the 6-pattern list (per RFC-0014-v2 §S5.1) applied to both `AuditError::SinkSpecific` and `SettlementError::SinkSpecific`.

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
expect: append returns Err(AuditError::SinkSpecific(_))
        payload String contains "chain_hash mismatch"
        (AuditChainError::HashMismatch wrapped at adapter boundary)
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
input: AuditFilter { subject_did_acl: Some("did:example:123"), status_filter: Some("ok"), .. }
expect: Compile error: no field `subject_did_acl` on type `AuditFilter`
        Compile error: no field `status_filter` on type `AuditFilter`
```

(Per §S4, the canonical AuditFilter has 5 fields: `since_unix`, `until_unix`, `capability_root`, `model`, `limit`. Any struct literal referencing a non-canonical field fails at compile time.)

### TV-AUD-v2-7: Typed-discriminator construction (redaction)

```
input: extension_kind = "redaction"
expect: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")
        event_kind = AuditEventKind::Revoke
```

### TV-AUD-v2-8: Sink append atomic persistence

```
input: event with event.chain_hash = compute_chain_hash(&event)
       simulated crash injected mid-transaction (Stoolap Transaction wrapper abandoned)
expect: post-recovery last_event_id() returns None OR Some(prev_event_id)
        (NOT Some(event.event_id) — partial persistence must not be observable)
```

### TV-AUD-v2-9: Typed-discriminator namespace collision (capability-insert)

```
input: extension_kind = "capability-insert"
expect: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/capability-insert/v1/") = 0x...
        (collision with cap_root_hash for any other extension kind is cryptographically infeasible at 2^128 birthday bound)
```

### TV-AUD-v2-10: AuditEvent round-trip canonical_bytes

```
input: AuditEvent { event_id: 1, node_did: "did:example:node", event_kind: Insert, cap_root_hash: [0; 32], at_millis_unix: 1700000000000, prev_chain_hash: [0; 32] }
expect: compute_chain_hash(&event) == BLAKE3-256(canonical_bytes(&event))
        canonical_bytes(&event) contains exactly 8 + 32-byte-UTF8 + 1 + 32 + 8 + 32 = event_id_be + node_did_bytes + kind_tag + cap_root + timestamp_be + prev_chain
```

### TV-AUD-v2-11: verify_chain accepts canonical chain

```
input: persist event with chain_hash = compute_chain_hash(&event)
expect: verify_chain([event]) returns Ok(())
        verify_chain reads back the same chain_hash value
```

### TV-AUD-v2-12: verify_chain rejects tampered event_kind tag byte

```
input: persist event with chain_hash = compute_chain_hash(&event)
       flip event_kind tag byte after persistence
expect: verify_chain returns Err(AuditChainError::HashMismatch)
        (chain_hash no longer matches recomputed)
```

### TV-AUD-v2-13: SequenceGap at event_id boundary (1, 3 skip 2)

```
input: append event_id=1, then event_id=3
expect: first Ok(())
        second returns Err(AuditError::SequenceGap { event_id: 3, prev: 1 })
```

### TV-AUD-v2-14: Replay (idempotency) at event_id 1

```
input: append event_id=1, then append event_id=1 again
expect: first Ok(())
        second returns Err(AuditError::AlreadyExists(1))
```

### TV-AUD-v2-15: AuditFilter compile-fail on extra fields

```
input: AuditFilter { since_unix: Some(0), whatever_field: Some(0), .. }
expect: Compile error: no field `whatever_field` on type `AuditFilter`
```

**DEFERRED:** `AuditFilter` is Layer B façade projection (per §S4); does NOT exist in `octo-audit` v1.x substrate. Lands at acceptance per §Implementation Phases Phase 2. Compile-fail behavior verified at acceptance via substrate-code amendment mission.

### TV-AUD-v2-16: AuditFilter full-field round-trip

```
input: AuditFilter { since_unix: Some(0), until_unix: Some(100), capability_root: Some([1; 32]), model: Some("gpt-4".to_string()), limit: Some(50) }
expect: filter.since_unix == Some(0)
        filter.until_unix == Some(100)
        filter.capability_root == Some([1; 32])
        filter.model == Some("gpt-4")
        filter.limit == Some(50)
```

**DEFERRED:** Same as TV-AUD-v2-15; `AuditFilter` lands at acceptance per Phase 2.

### TV-AUD-v2-17: StoolapAuditSink concurrent appender detection

```
input: two appenders simultaneously call append(event_id=1) on empty table
expect: at most one returns Ok(())
        the other returns Err(AuditError::SequenceGap) or Err(AuditError::AlreadyExists)
        (adapter-side serialization via Stoolap Transaction wrapper + monotonicity pre-check)
```

### TV-AUD-v2-18: scrub_adapter_error pattern 1 (hex digest ≥32 chars)

```
input: scrub_adapter_error("Stoolap error: transaction 0xabcdef0123456789abcdef0123456789abcd failed")
expect: output contains "<redacted-hex-0>"
        output does NOT contain the original hex digest
```

**DEFERRED:** `scrub_adapter_error` declared at Layer B façade per RFC-0014-v2 §S5.1; lands at acceptance per Phase 1 substrate-code amendment mission. Scrubber behavior verified at acceptance.

### TV-AUD-v2-19: scrub_adapter_error pattern 2 (absolute file path)

```
input: scrub_adapter_error("IO error reading /var/lib/octonet/audit/sink.db")
expect: output contains "<redacted-path>"
        output does NOT contain /var/lib/octonet/audit/sink.db
```

**DEFERRED:** Same as TV-AUD-v2-18.

### TV-AUD-v2-20: scrub_adapter_error pattern 3 (table-name reference)

```
input: scrub_adapter_error("relation \"audit_events\" does not exist")
expect: output contains "<redacted-table>"
        output does NOT contain "audit_events"
```

**DEFERRED:** Same as TV-AUD-v2-18.

### TV-AUD-v2-21: scrub_adapter_error pattern 4 (SQLSTATE prefix)

```
input: scrub_adapter_error("SQLSTATE_42P01 undefined_table")
expect: output contains "<redacted-sql-state>"
        output does NOT contain "SQLSTATE_42P01"
```

**DEFERRED:** Same as TV-AUD-v2-18.

### TV-AUD-v2-22: scrub_adapter_error pattern 5 (io error chain fragment)

```
input: scrub_adapter_error("os error 2: no such file or directory")
expect: output contains "<redacted-io>"
        output does NOT contain "os error 2"
```

**DEFERRED:** Same as TV-AUD-v2-18.

### TV-AUD-v2-23: scrub_adapter_error pattern 6 (adapter-type name)

```
input: scrub_adapter_error("StoolapTransactionError: write conflict")
expect: output contains "<redacted-adapter>"
        output does NOT contain "StoolapTransactionError"
```

**DEFERRED:** Same as TV-AUD-v2-18.

### TV-AUD-v2-24: Cross-RFC pairing with RFC-0014-v2 agent-transition-receipt

```
input: audit event with cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")
       paired with receipt having ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" || canonical_ask_id)
expect: audit_event_for_agent_transition_receipt(receipt) returns AuditEvent with cap_root_hash matching above
        prev_chain_hash = receipt_id_for(receipt) (cross-RFC binding via §S7 pairing)
```

**DEFERRED:** `audit_event_for_agent_transition_receipt` is a façade helper per RFC-0014-v2 §S7 + FW2; lands at acceptance per Phase 1 substrate-code amendment mission.

### TV-AUD-v2-25: cap_root_hash input boundary (variable-length)

```
input: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/custom-kind/v1/")
expect: 32-byte digest output, namespace prefix recovered by enumeration
        (no fixed 32-byte boundary; variable-length input)
```

### TV-AUD-v2-26: AuditError::SinkSpecific payload byte cap

```
input: SinkSpecific("a".repeat(10000)) (10 KiB adapter error)
expect: payload retained verbatim (substrate does NOT truncate)
        adapter-side scrub_adapter_error MUST be called before wrapping to enforce payload size limits (scrubber lands at acceptance per Phase 1 substrate-code amendment mission)
```

### TV-AUD-v2-27: AuditEventKind #[non_exhaustive] compile-fail downstream match

```
input: match audit_event.event_kind { Insert => ..., Revoke => ..., Sync => ... }
       (3-variant exhaustive match)
expect: Compile error (non-exhaustive match requires _ arm despite #[non_exhaustive] attribute on substrate enum)
        downstream must add _ arm to be forward-compatible
```

### TV-AUD-v2-28: ChainHash canonical-bytes ordering preservation

```
input: two AuditEvents with field values permuted between them
expect: distinct chain_hash values (canonical_bytes is field-order-sensitive)
```

### TV-AUD-v2-29: cap_root_hash zero digest rejection

```
input: cap_root_hash = [0; 32] (uninitialized / null)
expect: typed-discriminator construction still valid (zero digest is BLAKE3-256 output)
        domain caller SHOULD reject at façade layer; substrate accepts as opaque digest
```

### TV-AUD-v2-30: chain_hash second-preimage resistance

```
input: persisted event with chain_hash = X
       adversary constructs event' with chain_hash == X but different field values
expect: chain_hash collision requires 2^128 operations (BLAKE3-256 birthday bound)
        practical forgery infeasible
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

**FW1. RFC-0012-v3.** Subsequent typed-discriminator extensions (e.g. `CapabilityMint`, `CapabilityAttenuate`) follow the §S1 pattern. Note: `Redaction` is already in RFC-0012-v2 — not future work. Each extension kind is added to the table via a new RFC; no substrate enum edits.

**FW2. Cross-crate extension helpers.** Domain crates (e.g. `octo-wallet`, `octo-vault`) provide typed-discriminator construction helpers (`audit_event_for_agent_transition`, `audit_event_for_redaction`). Façade `octo-audit` re-exports for cross-crate use.

**FW3. Receipt-extension cross-RFC invariants.** RFC-0014-v2 pins the typed-discriminator pattern via `ask_id` namespaces. Cross-RFC consistency: `AuditEventKind::Insert` + `cap_root_hash = agent-transition-typed-discriminator` corresponds to a settlement `Receipt { ask_id = agent-transition-receipt-typed-discriminator }`. Status is recovered at façade projection (RFC-0016-a `ReceiptSummary`), NOT stored as a substrate field.

**FW4. Atomic persistence test infrastructure.** §S2.5 mandates transaction-scoped atomic persistence. RFC-0012-v2 TV-AUD-v2-8 pins the substrate test vector; a corresponding adapter-side crash-injection test infrastructure is needed for `StoolapAuditSink` conformance verification. Future mission: add crash-injection harness to the `octo-audit` test suite.

## Rationale

RFC-0012-v2 codifies the typed-discriminator extension pattern as the canonical substrate-faithful mechanism for new audit event semantics. This achieves three goals:

1. **Layer A frozen preservation** — `AuditEventKind` stays 3-variant; `AuditEvent` stays 7-field; `AuditError` stays 3-variant. No substrate churn from extension additions.
2. **Forward compatibility** — `#[non_exhaustive]` discipline means downstream `match` expressions on `AuditEventKind` continue to compile (substrate-stored events remain forward-compatible).
3. **Type safety** — typed-discriminator namespaces are BLAKE3-256 digests (2^128 collision operations birthday bound; cross-prefix second-preimage attacks at ~2^256 with one fixed prefix, ~2^128 birthday for attacker-chosen both prefixes); cross-extension-kind spoofing requires BLAKE3 cross-prefix collision attack.

The cost is a doc-comment-driven extension pattern that domain crates must follow. This is acceptable per CLAUDE.md §Extension over enumeration — the substrate stays minimal; extension mechanics live at the façade/domain boundary.

## Version History

| Version      | Date       | Author                                | Notes                                                                                                                                                                                                                                                         |
| ------------ | ---------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v2.0.0-draft | 2026-09-11 | CipherOcto Architecture Working Group | Initial draft. Pins typed-discriminator pattern, sink invariants, AuditFilter substrate contract.                                                                                                                                                             |
| v2.0.0-r30.5 | 2026-09-11 | CipherOcto Architecture Working Group | R30.5 prose alignment + substrate-code amendment gap acknowledgements (scrubber, ReceiptId, AuditFilter DEFERRED to Phase 1 acceptance).                                                                                                                      |
| v2.0.0-r31.5 | 2026-09-11 | CipherOcto Architecture Working Group | R31.5 scrubber relocation (Layer A → Layer B), BLAKE3 math correction, AC + 2-Cycle Tag additions.                                                                                                                                                            |
| v2.0.0-r32.5 | 2026-09-11 | CipherOcto Architecture Working Group | R32.5 TV-AUD-v2-3 variant shape revert, Authors/Maintainers H2 sections, Implicit Assumptions + Determinism + Performance Targets additions, cite sweep 160/160.                                                                                              |
| v2.0.0-r33.5 | 2026-09-11 | CipherOcto Architecture Working Group | R33.5 A5 timestamp monotonicity rewrite, compute_settlement_hash → receipt_id_for rename, Rationale 7-variant fix, TV-AUD-v2-6 type leak fix, 5-len → 5-lens.                                                                                                 |
| v2.0.0-r34.5 | 2026-09-11 | CipherOcto Architecture Working Group | R34.5 hygiene parens strip, phantom-substrate TV DEFERRED markers, AC-5 AuditChainError fix, cross-crate scrubber dep → per-façade scrubber, adapter-type registry via scrub_adapter_error_with, §S6 zero-key qualifier, substrate names Layer D adapter fix. |

## Related RFCs

- RFC-0012 — parent RFC; defines substrate `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`.
- RFC-0014-v2 — sibling substrate amendment for settlement extension pattern.
- RFC-0015-a — wallet agent write-path amendment; requires RFC-0012-v2 for typed-discriminator of `AgentTransition` events.
- RFC-0016-a — audit receipt write-path amendment; requires RFC-0012-v2 for `ChainHash` newtype + canonical-bytes-on-write invariant.
- RFC-0011-a — wallet subcommands; cross-RFC reference for scrubber location (Layer B façade, not Layer A substrate).
- RFC-0010 — DID canonical form; substrate stores `node_did: String` (raw canonical wire form per RFC-0010).

## Related Use Cases

- **UC-AUD-001 — Agent lifecycle audit trail.** When an agent transitions state (RFC-0015-a `transition_agent`), an audit event with `event_kind = AuditEventKind::Insert` and `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")` is appended to the sink. The audit event's `prev_chain_hash` binds it to the corresponding `AgentTransitionReceipt`'s `settlement_hash` (per RFC-0014-v2 §S7 pairing invariant).
- **UC-AUD-002 — Capability redaction.** When a capability is redacted (e.g. compromise recovery), an audit event with `event_kind = AuditEventKind::Revoke` and `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")` is appended. The `reason` payload is scrubbed per the canonical scrubber (`octo-settlement::scrub::scrub_adapter_error`, RFC-0014-v2 §S5.1) before being persisted.
- **UC-AUD-003 — CLI receipt listing with `AuditFilter`.** When a CLI consumer runs `octo audit list --since <unix> --until <unix> --capability-root <hex> --model <model> --limit <n>` (RFC-0016-a), the façade constructs an `AuditFilter` per §S4 and applies it to the audit store. Subject-DID ACL and receipt-status filtering are out of v2.0.0 scope.
- **UC-AUD-004 — Cross-replica sync.** When a downstream replica syncs the audit chain, sync events use `event_kind = AuditEventKind::Sync` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/sync/v1/")`. The `prev_chain_hash` field carries the last persisted chain hash from the source replica, enabling chain-integrity verification on receipt.

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
