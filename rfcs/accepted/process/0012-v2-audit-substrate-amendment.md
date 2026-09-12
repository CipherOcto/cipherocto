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

## Status

**Accepted (2026-09-12)** — DRY CLOSED at R48 + R49 consecutive zero-finding rounds across all 5 lenses (correctness, security, layer-model, hygiene, spec-completeness). Paired atomic promotion with RFC-0014-v2 per §2-Cycle Atomic Promotion Tag.

> **Mission decomposition note.** Per `no-phantom-mission-pointers` rule, no standalone mission file is required for the v2.0.0 amendment acceptance cycle. Phase 1 substrate-code amendment work is captured inline via DEFERRED markers throughout this RFC; the DEFERRED markers are the canonical forward-pointer form per CLAUDE.md §Mission Lifecycle.

## Authors

- CipherOcto Architecture Working Group

## Maintainers

- CipherOcto Architecture Working Group

## Dependencies

- **RFC-0012** — defines `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`.
- **RFC-0014-v2** — pins settlement substrate extension pattern; cross-RFC consistency on canonical receipt-extension interaction with audit-event extensions.
- **RFC-0015-a** — requires RFC-0012-v2 to land the typed-discriminator for `AgentTransition` events.
- **RFC-0016-a** — requires RFC-0012-v2 for `ChainHash` newtype + canonical-bytes-on-write write-path.

## Design Goals

**G1.** Enable future audit event semantics without modifying the substrate `AuditEventKind` enum.
**G2.** Preserve byte-identical canonical-bytes serialization form for existing `Insert / Revoke / Sync` events.
**G3.** Maintain `#[non_exhaustive]` discipline on `AuditEventKind` so substrate-stored events remain forward-compatible with future extensions.
**G4.** Lock the substrate contract surface so adapter implementations have no ambiguity on what they must enforce. Canonical adapter-location reference: see Appendix §Layer Direction Note.

## Motivation

RFC-0012 defines a 3-variant `AuditEventKind` (`Insert / Revoke / Sync`) marked `#[non_exhaustive]`. The substrate doc-comment states "extension variants land via typed-discriminator namespaces ... NOT via central enum edits". However:

- RFC-0015-a requires an `AgentTransition` event kind for `transition_agent`.
- RFC-0016-a requires a `ChainHash` newtype and canonical-bytes-on-write invariant.
- R28 review of RFC-0015 + RFC-0016 surfaced 5+ contradictions between substrate-frozen surface and write-path requirements.

RFC-0012-v2 codifies the typed-discriminator pattern so write-path amendments can reference a pinned canonical substrate form. Future extensions (e.g. `Redaction`, `CapabilityMint`, `CapabilityAttenuate`) follow the same pattern without substrate enum edits.

## Roles and Authorities

| Role                                   | Authority                                                                                                                                                                                                                     |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `octo-audit-core` (substrate, Layer A) | Owns `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`, `compute_chain_hash`, `verify_chain` (no `extension_kinds` module; typed-discriminator digests are constructed inline at the façade per Appendix B) |
| `octo-audit` (façade, Layer B)         | Re-exports substrate canonical types + implements `StoolapAuditSink` (DOMAIN (Layer B-faithful) storage adapter, not Layer D transport adapter; see Appendix §Layer Direction Note)                                           |
| Domain callers (Layer C / E)           | Construct `AuditEvent` instances via façade typed-discriminator helpers — **DEFERRED — lands at acceptance** per §Implementation Phases Phase 2                                                                               |

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

2. **Strict event_id monotonicity** — When `last_event_id()` returns `Some(prev)` (non-empty table), `event.event_id` MUST equal `prev + 1`. On gap, return `AuditError::SequenceGap { event_id, prev }`. Empty-table branch: when `last_event_id()` returns `None`, the substrate accepts any first `event_id` (the "first id" invariant lives at the sink caller per RFC-0012; `verify_chain` does not enforce it on the first event per substrate empty-table branch in `crates/octo-audit/src/storage/stoolap.rs`).

3. **Idempotent re-append** — `event.event_id == last_event_id()` (caller re-submits last event) MUST return `AuditError::AlreadyExists(event_id)`. NOT a success.

4. **Canonical `chain_hash` computation** — implementations MUST call `octo_audit_core::compute_chain_hash(&event)` to compute the canonical chain hash, then verify `event.chain_hash == computed` (rejects tampered `chain_hash` field). On mismatch, return `AuditError::SinkSpecific(format!("chain_hash mismatch at event_id {}", event.event_id))` (canonical format string, substrate-faithful to the Stoolap adapter implementation).

5. **Atomic persistence** — implementations MUST persist the event in a transaction-scoped atomic write. The persistence operation MUST be either fully committed (visible to subsequent `last_event_id()` calls) or fully rolled back (no partial persistence observable). Adapter-specific transaction mechanisms (e.g. database `Transaction` wrappers) are adapter-layer concerns (DOMAIN (Layer B-faithful)), not substrate contract.

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

**§S3 scope:** This section pins `AuditError` only. `AuditEvent` Debug redaction discipline (32-byte hashes) is documented at §S5. The two are separate substrate invariants — `AuditError` Debug is per-variant `#[error("...")]` (no hash payload); `AuditEvent` Debug is a substrate-faithful redaction policy for the 3 hash fields.

### §S4 — `AuditFilter` façade projection contract (canonical)

§S4 pins the façade-level `AuditFilter` projection contract. **`AuditFilter` is a Layer B façade projection, NOT a Layer A substrate type** (per Appendix B). **DEFERRED — lands at acceptance** per §Implementation Phases Phase 2 (`AuditFilter` does NOT yet exist in `crates/octo-audit/src/lib.rs`; it lands in `octo-audit` v2.0.0 at acceptance time). It will live at `crates/octo-audit/src/lib.rs` (Layer B façade re-export surface) and is constructed by the façade for CLI consumers (RFC-0016-a). The §S4 declaration pins the canonical surface that RFC-0016-a §AuditFilter definition must match.

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

**Note (substrate-faithful pointer):** The §S4 declaration above pins the canonical surface that RFC-0016-a §AuditFilter definition must match.

**Façade constraints:**

- **NO `subject_did` field** — `subject_did` ACL filtering is RFC-0016-a concern (façade projection). The façade reads audit events without subject-level ACL (per-event ACL is enforced at the domain call boundary). `subject_did` ACL field on `AuditFilter` is **DEFERRED — lands at paired acceptance with RFC-0014-v2**.
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

**Debug redaction note:** `AuditEvent`'s substrate `Debug` impl redacts the three 32-byte hash fields (`cap_root_hash`, `prev_chain_hash`, `chain_hash`) to prevent accidental leakage of chain-internal state into logs, error messages, and adapter `SinkSpecific` payloads. The redaction is at substrate level (`octo-audit-core::event::AuditEvent`); façade projections retain the redaction discipline. Adapter `Debug` impls MAY expose the redaction summary format `"AuditEvent { event_id, event_kind, at_millis_unix, node_did, hashes_redacted: true }"` and MUST NOT expose raw hash bytes.

### §S6 — Canonical-bytes form (unchanged)

§S6 pins the existing canonical-bytes form:

```text
[event_id (BE u64) | node_did (UTF-8) | event_kind (tag byte) |
  cap_root_hash (32 bytes) | at_millis_unix (BE u64) | prev_chain_hash (32 bytes)]
```

`chain_hash` is excluded (it IS the hash of the other fields + the `prev_chain_hash`).

**Extensions do NOT add new bytes** to the canonical form. The `cap_root_hash` already encodes the typed-discriminator; the extension payload (e.g. agent transition fields) is encoded in the canonical hash via extension-specific helpers in the calling domain crate (e.g. `octo-wallet::audit::canonical_bytes_agent_transition(...)`).

### Error Handling

The substrate pins the cross-trait error envelope separation:

- `AppendOnlyAuditSink::append` returns `AuditError` — exactly 3 variants per §S3 (`SequenceGap`, `AlreadyExists`, `SinkSpecific`).
- `verify_chain` returns `AuditChainError` — a distinct 3-variant envelope (`SequenceGap`, `HashMismatch`, `TimestampRegression`); distinct variant set from `AuditError`, no cross-leakage.
- Adapter-specific failures map to `AuditError::SinkSpecific(String)` (the `AuditError` envelope, not `AuditChainError`); the `String` payload is the substrate-canonical scrubbed message per §SC2 above (**DEFERRED — lands at acceptance**). Note: the cross-RFC audit-scrubber canonical spec is RFC-0012-v2 §SC2 (this RFC); RFC-0014-v2 §S5.1 owns the settlement-side scrubber canonical spec for `SettlementError::SinkSpecific`.
- Raw adapter error chains NEVER cross the substrate boundary (RFC-0012 §Trait G3 mitigation).
- **Timestamp regression leak (substrate-level defense-in-depth).** `AuditChainError::TimestampRegression { event_id, prev, current }` exposes `prev` and `current` unix-ms timestamps via its `#[error]` formatter. The timestamps leak chronological side-channel information to log observers (adversary timing-correlation). Substrate-level mitigation: redact timestamps in the `#[error]` formatter to `<event_id>`, `<prev-ts>`, `<current-ts>` (substrate-code amendment at paired acceptance). Façade-side redactor does NOT see this leak because the substrate returns `AuditChainError` BEFORE the façade re-projects.
- **`SinkSpecific` payload size (unbounded pre-acceptance).** `AuditError::SinkSpecific(String)` accepts an unbounded `String` payload at the substrate level — the substrate does NOT enforce a payload size cap. The 4 KiB scrubber cap (input cap + output cap per RFC-0014-v2 §FW6) is enforced at the façade-level scrubber, NOT at the substrate trait boundary. Pre-acceptance substrate code carries no size invariant; paired-acceptance substrate amendment to add a substrate-level cap is DEFERRED.

CLI maps substrate errors to exit codes per RFC-0011-a §Error Handling.

## Implicit Assumptions Audit

- **A1. BLAKE3-256 collision resistance.** Extension kinds are encoded as `BLAKE3-256(namespace_string)`. Finding a collision requires ~2^128 hash evaluations (birthday bound on 256-bit output). Blast radius if broken: cross-extension-kind spoofing (one kind can be reinterpreted as another). Mitigation: substrate accepts only the canonical extension kinds per §S1; extension helper functions at the façade validate kind at construction time.
- **A2. Sink atomicity precondition.** Adapter implementations MUST satisfy §S2.5 atomic persistence. Failure to satisfy means two appenders could observe different total orderings of `event_id`. Blast radius: torn writes, lost audit events. Mitigation: §FW4 crash-injection test infrastructure (future mission); adapter conformance gate at Phase 2 acceptance.
- **A3. BLAKE3 domain-separator commitment.** `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/<K>/v1/")` namespace strings are committed at RFC-0012-v2 acceptance time. Future extensions add new namespace strings; existing ones are immutable. Blast radius: namespace collision would let one extension kind impersonate another. Mitigation: 32-byte digest output + namespace version pinning (`v1/` suffix).
- **A4. `node_did` pre-validation.** The substrate assumes `node_did` field on `AuditEvent` has been pre-validated by the domain caller (DID-format check + signature verification at the wire boundary). Substrate does NOT re-validate. Blast radius: malformed DIDs persist into the audit chain. Mitigation: domain-crate validation at the call boundary + RFC-0009 §Identity substrate verification.
- **A5. Single-writer assumption.** `event_id == last_event_id() + 1` monotonicity check assumes a single logical appender. Concurrent appenders with distinct node_did values are out of v2.0.0 scope; cross-replica append coordination is RFC-0855 territory. Blast radius: sequence gap detection may trigger spuriously under concurrent append. Mitigation: AC-10 + external serialization layer at adapter boundary. **`at_millis_unix` monotonicity is enforced only on the read-path, not the append-path:** `AppendOnlyAuditSink::append` per §S2 does NOT enforce `at_millis_unix` monotonicity — the sink caller is responsible for supplying monotonically increasing timestamps; `verify_chain` per `crates/octo-audit-core/src/chain.rs` DOES enforce strict `at_millis_unix` monotonicity on every persisted event and returns `AuditChainError::TimestampRegression` on regression (timestamp regression is observable at the façade via `AuditChainError::TimestampRegression` per §Error Handling (AuditChainError envelope)). **Adapter NTP prerequisite:** monotonic `at_millis_unix` values at append-time require clock-discipline at the adapter host (NTP or equivalent); without NTP discipline, appends may persist out-of-order and `verify_chain` will reject them at read-time even though `append` accepted them.

**Documented adapter-layer concerns (out of v2.0.0 substrate contract scope):**

- **Adapter-side insertion-error detection heuristic.** Adapter impls MAY use string-matching against the canonical 8-pattern list (§SC2) to detect adapter-specific insertion errors before wrapping into `AuditError::SinkSpecific`. This heuristic is documented as adapter-layer best-effort; substrate does NOT enforce the heuristic at the trait boundary (DOMAIN (Layer B-faithful) concern per Appendix C).

## Determinism Requirements

### RFC-0008 Execution Class Mapping

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

- **AC-1.** `octo_audit_core::AuditEvent` retains its 7-field public surface (no field additions; semver-major bump from v1.x to v2.0.0). Covered by TV-AUD-v2-10 (implicit 7-field canonical-bytes structure).
- **AC-2.** `octo_audit_core::AuditError` retains its 3-variant form (`SequenceGap`, `AlreadyExists`, `SinkSpecific`); no new variants. Covered by TV-AUD-v2-26 (implicit 3-variant payload byte-cap).
- **AC-3.** `compute_chain_hash(&AuditEvent) == BLAKE3-256(canonical_bytes(event))` — verified by `verify_chain` round-trip. Covered by TV-AUD-v2-2 + TV-AUD-v2-11 (append accepts canonical chain_hash; verify_chain accepts canonical chain).
- **AC-4.** Strict `event_id == last_event_id() + 1` enforced when `last_event_id()` returns `Some(prev)` (non-empty table); re-append returns `AlreadyExists`; gap returns `SequenceGap`. Empty-table acceptance: when `last_event_id()` returns `None`, the substrate accepts any first `event_id` per the §S2.2 empty-table branch; `verify_chain` does not enforce the first id invariant on the first event. Covered by TV-AUD-v2-4 + TV-AUD-v2-5 + TV-AUD-v2-13 + TV-AUD-v2-14 (sequence gap + idempotency coverage).
- **AC-5.** Tampered `chain_hash` returns `AuditError::SinkSpecific` with payload matching the canonical format-string `"chain_hash mismatch at event_id <id>"` (where `<id>` is `event.event_id`) per §S2.4 (NOT raw substrate `AuditChainError::HashMismatch` leaked; `verify_chain` returns `AuditChainError`, `append` returns `AuditError` — distinct error envelopes, no cross-leakage). Covered by TV-AUD-v2-3 + TV-AUD-v2-12 (append rejects tampered chain_hash; verify_chain rejects tampered event_kind tag byte — distinct envelope coverage).
- **AC-6.** `AuditFilter` is Layer B façade projection (does NOT exist in substrate; lands at acceptance per Phase 2); 5-field form `{ since_unix, until_unix, capability_root, model, limit }` UNCONDITIONAL, no `subject_did`, no `status`. Covered by TV-AUD-v2-6 + TV-AUD-v2-15 + TV-AUD-v2-16 (5-field compile-fail on extra fields + full-field round-trip).
- **AC-7.** Adapter-side scrubber call before wrapping into `SinkSpecific` — **DEFERRED — lands at acceptance** The canonical `scrub_adapter_error` per façade is declared at each Layer B façade (pattern duplicated per-façade to avoid sibling Layer B coupling); adapter conformance gate runs at acceptance per §Implementation Phases. Pre-acceptance adapter-side redactor is out of scope for RFC-0012-v2. Covered by TV-AUD-v2-18 through TV-AUD-v2-23 (scrubber pattern coverage at acceptance).
- **AC-8.** Paired acceptance with RFC-0014-v2 per BLUEPRINT.md §2-Cycle Atomic Promotion gate.
- **AC-9.** All 30/30 Test Vectors (TV-AUD-v2-1 through TV-AUD-v2-30) in §Test Vectors produce expected outputs (verified by `cargo test -p octo-audit`; pass criterion: 30/30 TVs pass).
- **AC-10.** DOMAIN (Layer B-faithful) adapter implementations (e.g. `StoolapAuditSink`) provide transaction-scoped atomic write with persistence durability; concurrent appenders detected by the substrate monotonicity pre-check (adapter-side serialization is the adapter's responsibility). Covered by TV-AUD-v2-8 + TV-AUD-v2-17 (atomic persistence + concurrent appender detection — both DEFERRED to acceptance per §Implementation Phases adapter conformance gate).
- **AC-11.** Runtime check at façade layer — the façade helper `octo_audit::audit_event` MUST include a mandatory runtime check returning `Result<(), AuditError>` (NEVER `debug_assert!` which is stripped at `-O`; release builds MUST enforce the check via `Result`-returning validation; `assert!` may be used only for invariant-impossible conditions per Rust convention). Substrate-side `verify_chain` remains the source of truth for `prev_chain_hash` chain linkage; the runtime check is a façade-side early-fail check documented in §A4. _(no-runtime-TV; release-build enforcement.)_
- **AC-12.** Façade-internal const registry self-validation — `octo_audit::extension_kinds` (façade-side) const table MUST be validated against the canonical extension kind namespace registry declared at §S1 + Appendix A (substrate lacks an `extension_kinds` module per Appendix B); this is façade-internal self-validation, and cross-crate consistency risk is accepted as a substrate-code amendment at acceptance. _(no-runtime-TV; compile-time const registry self-validation.)_
- **AC-13.** Cross-RFC pairing invariants — `audit_event_for_agent_transition_receipt` (per §FW2 + RFC-0014-v2 §FW4) lands at acceptance as a paired DEFERRED invariant per §AC-8 2-cycle atomic promotion gate. The audit-side paired-acceptance adapter is `crates/octo-audit/src/storage/stoolap.rs` (audit-side DOMAIN (Layer B-faithful) adapter; see Appendix §Layer Direction Note); the settlement-side paired-acceptance adapter is `crates/quota-router-sm-engine/src/store.rs` (settlement DOMAIN (Layer B-faithful) hosted in Layer C crate storage adapter per substrate preamble `crates/quota-router-sm-engine/src/lib.rs` §Module layout preamble ("Layer C specialized node"); the adapter contract is DOMAIN-faithful per RFC-0014-v2 §Layer Direction Note, but the host crate is physically Layer C; verified via `ls crates/quota-router-sm-engine/src/` showing `store.rs`; the substrate `crates/octo-settlement-core/` (Layer A frozen) hosts no storage module; the legacy `crates/octo-settlement/` (Layer B façade) contains only `lib.rs`) per RFC-0014-v2 AC-7. Cross-RFC pairing is **DEFERRED — lands at paired acceptance**; the substrate amendment itself ships independently of the façade helper.

## 2-Cycle Atomic Promotion Tag

Per BLUEPRINT.md §RFC Process item 5 + §2-Cycle Atomic Promotion gate:

- **Sibling:** RFC-0014-v2
- **Reviewer board:** 5-lens reviewer board (correctness / security / layer-model / hygiene / spec-completeness)
- **Pairing invariant:** RFC-0014-v2 §S7 cross-RFC pairing via canonical façade helpers (`audit_event_for_agent_transition_receipt`, `audit_event_for_ask_rejected`) per §FW2 requires both substrate amendments to land together. RFC-0015-a + RFC-0016-a acceptance gated on this 2-cycle. Audit-event `prev_chain_hash` remains substrate-load-bearing for audit chain verification (chain-hash linkage, NOT receipt linkage); settlement-binding lives in the settlement-side index only.
- **Atomic promotion gate:** Both RFCs transition Draft → Accepted in the same PR. Neither may be Accepted without the other.

## Adversarial Review

This RFC has been reviewed across 5 reviewer lenses over multiple iterations (R30 through R45.5):

- **R30:** initial substrate-amendment draft review across 5 lenses.
- **R30.5 → R31.5:** Initial 5-lens review; pattern convergence + substrate-faithful pass.
- **R32.5:** Layer-model + spec-completeness sweep; DOMAIN contract expansion.
- **R33.5:** Cross-file sync + 5-lens canonical form fix.
- **R34.5:** High + medium cascade; per-façade scrubber pattern.
- **R35.5:** Cross-RFC DOMAIN cascade; 17 trailing-period DEFERRED fixes.
- **R36:** file:line + KeyedHasher + Layer Direction notes.
- **R36.5:** 8 DOMAIN misattributions + Appendix C direction rule.
- **R37:** A4 prev_chain_hash payload-hash wording fix; AC-7 rephrase.
- **R38:** Cross-RFC wording drift corrections; hygiene em-dash sweep.
- **R38.5:** Scrubber code-fence audit; §S5.1/§S6.1 collapse check.
- **R39:** VH numbering fill; AuditFilter DEFERRED marker alignment.
- **R39.5:** extension_kinds drop; subject_did DEFERRED; A4 chain-linkage-only reassert.
- **R40:** Substrate-truth sweep; phantom path + newtype claims dropped.
- **R40.5:** R41 fix: VH r40/r40.5 + §FW6 + AC-11 + AC-12.
- **R41:** 5-lens review surfaced 15 findings; -29% drop.
- **R41.5:** cite sweep + Layer B hygiene check; cross-RFC fixes.
- **R42:** AC-13 paired adapter location verification (audit + settlement DOMAIN).
- **R42.5:** 13 findings fixed; debug_assert banned; VH sequence gap closed.
- **R43:** §AR narrative extended through R36.5; missing bullet recovery.
- **R43.5:** 0014-v2 Layer-D cascade miss; 0012-v2 subset fix.
- **R44:** 5-lens review: 65 findings; +242% spike.
- **R44.5:** 9 findings fixed; 0014-v2 cascade missed (R45 input).
- **R45:** 5-lens review: 34 findings; -48% recovery.
- **R45.5:** 8 findings fixed; partial cross-file sync.

Reviewer board membership per CLAUDE.md §Review Process: correctness, security, layer-model, hygiene, spec-completeness. Critical-lens reviewers may surface CRITICAL/HIGH findings post-DRY-CLOSED if substrate code or external threat-model changes.

## Security Considerations

### §SC1 — Typed-discriminator collision resistance

Extension kinds are encoded as `BLAKE3-256(namespace_string)`. BLAKE3-256 collision resistance is 2^128 operations (birthday bound on 256-bit output). Preimage resistance is 2^256. Cross-extension-kind collisions are cross-prefix second-preimage attacks (~2^256 with one fixed prefix; ~2^128 birthday for attacker-chosen both prefixes).

### §SC2 — `SinkSpecific` payload scrubbing

Adapter-specific error messages MUST be scrubbed at the adapter boundary before wrapping into `AuditError::SinkSpecific`. No raw error chains, no adapter-type names, no leaked path fragments. Canonical scrubber pattern is declared at each Layer B façade (canonical `scrub_adapter_error` per façade — **DEFERRED — lands at acceptance**; duplicated per-façade to avoid sibling Layer B coupling) with the canonical 8-pattern list (5 base patterns: hex / path / table / SQLSTATE / io + 2 hardened variants + 1 adapter-type-name registry pattern = 5 + 2 + 1 = 8) per RFC-0014-v2 §S5.1, applied to both `AuditError::SinkSpecific` and `SettlementError::SinkSpecific`.

### §SC3 — Chain hash integrity

`chain_hash` field MUST match `compute_chain_hash(event)`. §S2.4 enforces this on every append; §S6 canonical-bytes form is the substrate-level guarantee.

### §SC4 — Event_id monotonicity

Strict `event_id == last_event_id() + 1` requirement prevents both gaps (sequence skip) and replay (duplicate). Re-append returns `AlreadyExists` (NOT silent success) per §S2.3.

### §SC5 — Atomic persistence

§S2.5 requires transaction-scoped atomic write. Adapter impls without atomic persistence (e.g. file append without fsync) MUST NOT satisfy this contract.

## Adversary Analysis

**A1. Audit chain tampering.** Adversary modifies a persisted `AuditEvent` (e.g. flips `event_kind` tag byte, modifies `node_did`). `verify_chain` returns `AuditChainError::HashMismatch` for any tampered event. Mitigated by §S6 canonical-bytes + §S2.4 chain hash integrity check.

**A2. Replay attack.** Adversary re-submits last persisted event. §S2.3 returns `AlreadyExists`; no duplicate persistence. Adversary cannot bypass via different `event_id` because §S2.2 enforces strict monotonicity.

**A3. Sequence gap injection.** Adversary skips `event_id` (e.g. submits 5 after 3, skipping 4). §S2.2 returns `SequenceGap { event_id: 5, prev: 3 }`. Caller cannot inject gaps.

**A4. Typed-discriminator spoofing.** Adversary constructs event with `event_kind = Insert` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")` but with non-canonical agent-transition payload. Detection: domain crate canonicalizes the agent-transition payload and includes it in the extension-specific canonical-hash input at the façade (the `cap_root_hash` typed-discriminator combined with the domain-crate canonical bytes is the binding; `prev_chain_hash` remains substrate-load-bearing for chain linkage ONLY — NOT receipt linkage). Cross-RFC: RFC-0015-a §Audit row contract pins the canonical payload encoding.

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

```text
input: extension_kind = "agent-transition"
expect: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/") = 0x...
        event_kind = AuditEventKind::Insert
```

### TV-AUD-v2-2: Sink append accepts canonical chain_hash

```text
input: event with event.chain_hash = compute_chain_hash(&event)
expect: append returns Ok(())
        last_event_id() returns Some(event.event_id)
```

### TV-AUD-v2-3: Sink append rejects tampered chain_hash

```text
input: event with event.chain_hash = [0x00; 32] (not equal to compute_chain_hash)
expect: append returns Err(AuditError::SinkSpecific(_))
        payload String contains "chain_hash mismatch at event_id <id>"
        (AuditChainError::HashMismatch wrapped at adapter boundary)
```

### TV-AUD-v2-4: Sink append rejects sequence gap

```text
input: append event_id=1, then event_id=3 (skipping 2)
expect: first append returns Ok(())
        second append returns Err(AuditError::SequenceGap { event_id: 3, prev: 1 })
```

### TV-AUD-v2-5: Sink append rejects re-append (idempotency)

```text
input: append event_id=1, then append event_id=1 again
expect: first append returns Ok(())
        second append returns Err(AuditError::AlreadyExists(1))
```

### TV-AUD-v2-6: AuditFilter substrate contract

```text
input: AuditFilter { subject_did_acl: Some("did:example:123"), status_filter: Some("ok"), .. }
expect: Compile error: no field `subject_did_acl` on type `AuditFilter`
        Compile error: no field `status_filter` on type `AuditFilter`
```

(Per §S4, the canonical AuditFilter has 5 fields: `since_unix`, `until_unix`, `capability_root`, `model`, `limit`. Any struct literal referencing a non-canonical field fails at compile time.)

**DEFERRED — lands at acceptance** `AuditFilter` is Layer B façade projection (per §S4); does NOT exist in `octo-audit` v1.x substrate. Lands at acceptance per §Implementation Phases Phase 2. Compile-fail behavior verified at acceptance per §Implementation Phases.

### TV-AUD-v2-7: Typed-discriminator construction (redaction)

```text
input: extension_kind = "redaction"
expect: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")
        event_kind = AuditEventKind::Revoke
```

### TV-AUD-v2-8: Sink append atomic persistence

```text
input: event with event.chain_hash = compute_chain_hash(&event)
       simulated crash injected mid-transaction (DOMAIN (Layer B-faithful) adapter's transaction mechanism abandoned)
expect: post-recovery last_event_id() returns None OR Some(prev_event_id)
        (NOT Some(event.event_id) — partial persistence must not be observable)
```

**DEFERRED — lands at acceptance** DOMAIN (Layer B-faithful) adapter atomic-persistence behavior verified at acceptance per §Implementation Phases (adapter-side crash-injection test infrastructure).

### TV-AUD-v2-9: Typed-discriminator namespace collision (capability-insert)

```text
input: extension_kind = "capability-insert"
expect: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/capability-insert/v1/") = 0x...
        (collision with cap_root_hash for any other extension kind is cryptographically infeasible at 2^128 birthday bound)
```

### TV-AUD-v2-10: AuditEvent round-trip canonical_bytes

```text
input: AuditEvent { event_id: 1, node_did: "did:example:node", event_kind: Insert, cap_root_hash: [0; 32], at_millis_unix: 1700000000000, prev_chain_hash: [0; 32] }
expect: compute_chain_hash(&event) == BLAKE3-256(canonical_bytes(&event))
        canonical_bytes(&event) contains exactly 8 + node_did_utf8_bytes + 1 + 32 + 8 + 32 = event_id_be + node_did_bytes + kind_tag + cap_root + timestamp_be + prev_chain, where node_did_utf8_bytes = node_did.as_bytes().len() (variable-length per String field per `crates/octo-audit-core/src/event.rs` §AuditEvent struct)
```

### TV-AUD-v2-11: verify_chain accepts canonical chain

```text
input: persist event with chain_hash = compute_chain_hash(&event)
expect: verify_chain([event]) returns Ok(())
        verify_chain reads back the same chain_hash value
```

### TV-AUD-v2-12: verify_chain rejects tampered event_kind tag byte

```text
input: persist event with chain_hash = compute_chain_hash(&event)
       flip event_kind tag byte after persistence
expect: verify_chain returns Err(AuditChainError::HashMismatch)
        (chain_hash no longer matches recomputed)
```

### TV-AUD-v2-13: SequenceGap at event_id boundary (1, 3 skip 2)

```text
input: append event_id=1, then event_id=3
expect: first Ok(())
        second returns Err(AuditError::SequenceGap { event_id: 3, prev: 1 })
```

### TV-AUD-v2-14: Replay (idempotency) at event_id 1

```text
input: append event_id=1, then append event_id=1 again
expect: first Ok(())
        second returns Err(AuditError::AlreadyExists(1))
```

### TV-AUD-v2-15: AuditFilter compile-fail on extra fields

```text
input: AuditFilter { since_unix: Some(0), whatever_field: Some(0), .. }
expect: Compile error: no field `whatever_field` on type `AuditFilter`
```

**DEFERRED — lands at acceptance** `AuditFilter` is Layer B façade projection (per §S4); does NOT exist in `octo-audit` v1.x substrate. Lands at acceptance per §Implementation Phases Phase 2. Compile-fail behavior verified at acceptance per §Implementation Phases.

### TV-AUD-v2-16: AuditFilter full-field round-trip

```text
input: AuditFilter { since_unix: Some(0), until_unix: Some(100), capability_root: Some([1; 32]), model: Some("gpt-4".to_string()), limit: Some(50) }
expect: filter.since_unix == Some(0)
        filter.until_unix == Some(100)
        filter.capability_root == Some([1; 32])
        filter.model == Some("gpt-4")
        filter.limit == Some(50)
```

**DEFERRED — lands at acceptance** Same as TV-AUD-v2-15; `AuditFilter` lands at acceptance per §Implementation Phases Phase 2.

### TV-AUD-v2-17: DOMAIN (Layer B-faithful) adapter concurrent appender detection

```text
input: two appenders simultaneously call append(event_id=1) on empty table
expect: at most one returns Ok(())
        the other returns Err(AuditError::SequenceGap) or Err(AuditError::AlreadyExists)
        (substrate monotonicity pre-check + adapter-side serialization)
```

**DEFERRED — lands at acceptance** DOMAIN (Layer B-faithful) adapter concurrent-appender behavior verified at acceptance per §Implementation Phases.

### TV-AUD-v2-18: scrub_adapter_error pattern 1 (hex digest ≥32 chars)

```text
input: scrub_adapter_error("Stoolap error: transaction 0xabcdef0123456789abcdef0123456789abcd failed")
expect: output contains "<redacted-hex-0>"
        output does NOT contain the original hex digest
```

**DEFERRED — lands at acceptance** `scrub_adapter_error` declared at Layer B façade per RFC-0014-v2 §S5.1; lands at acceptance per §Implementation Phases. Scrubber behavior verified at acceptance.

### TV-AUD-v2-19: scrub_adapter_error pattern 2 (absolute file path)

```text
input: scrub_adapter_error("IO error reading /var/lib/octonet/audit/sink.db")
expect: output contains "<redacted-path>"
        output does NOT contain /var/lib/octonet/audit/sink.db
```

**DEFERRED — lands at acceptance** Same as TV-AUD-v2-18.

### TV-AUD-v2-20: scrub_adapter_error pattern 3 (table-name reference)

```text
input: scrub_adapter_error("relation \"audit_events\" does not exist")
expect: output contains "<redacted-table>"
        output does NOT contain "audit_events"
```

**DEFERRED — lands at acceptance** Same as TV-AUD-v2-18.

### TV-AUD-v2-21: scrub_adapter_error pattern 4 (SQLSTATE prefix)

```text
input: scrub_adapter_error("SQLSTATE_42P01 undefined_table")
expect: output contains "<redacted-sql-state>"
        output does NOT contain "SQLSTATE_42P01"
```

**DEFERRED — lands at acceptance** Same as TV-AUD-v2-18.

### TV-AUD-v2-22: scrub_adapter_error pattern 5 (io error chain fragment)

```text
input: scrub_adapter_error("os error 2: no such file or directory")
expect: output contains "<redacted-io>"
        output does NOT contain "os error 2"
```

**DEFERRED — lands at acceptance** Same as TV-AUD-v2-18.

### TV-AUD-v2-23: scrub_adapter_error pattern 6 (adapter-type name)

```text
input: scrub_adapter_error("StoolapTransactionError: write conflict")
expect: output contains "<redacted-adapter>"
        output does NOT contain "StoolapTransactionError"
```

**DEFERRED — lands at acceptance** Same as TV-AUD-v2-18.

### TV-AUD-v2-24: Cross-RFC pairing with RFC-0014-v2 agent-transition-receipt

```text
input: audit event with cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")
       paired with receipt having ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" || canonical_ask_id)
expect: audit_event_for_agent_transition_receipt(receipt) returns AuditEvent with cap_root_hash matching above
        prev_chain_hash = compute_chain_hash(prev_event) (audit-chain linkage, NOT receipt linkage)
        (settlement-binding lives in settlement-side index per RFC-0014-v2 §S2 ACL via separate index)
```

**DEFERRED — lands at acceptance** `audit_event_for_agent_transition_receipt` is a façade helper per RFC-0014-v2 §S7 + FW2; lands at acceptance per §Implementation Phases.

### TV-AUD-v2-25: cap_root_hash input boundary (variable-length)

```text
input: cap_root_hash = BLAKE3-256("cipherocto/audit/extension/custom-kind/v1/")
expect: 32-byte digest output, namespace prefix recovered by enumeration
        (no fixed 32-byte boundary; variable-length input)
```

### TV-AUD-v2-26: AuditError::SinkSpecific payload byte cap

```text
input: SinkSpecific("a".repeat(10000)) (10 KiB adapter error)
expect: payload retained verbatim (substrate does NOT truncate)
        adapter-side scrub_adapter_error MUST be called before wrapping to enforce payload size limits (scrubber lands at acceptance per §Implementation Phases)
```

### TV-AUD-v2-27: AuditEventKind #[non_exhaustive] compile-fail downstream match

```text
input: match audit_event.event_kind { Insert => ..., Revoke => ..., Sync => ... }
       (3-variant exhaustive match)
expect: Compile error (non-exhaustive match requires _ arm despite #[non_exhaustive] attribute on substrate enum)
        downstream must add _ arm to be forward-compatible
```

### TV-AUD-v2-28: ChainHash canonical-bytes ordering preservation

```text
input: two AuditEvents with field values permuted between them
expect: distinct chain_hash values (canonical_bytes is field-order-sensitive)
```

### TV-AUD-v2-29: cap_root_hash zero digest rejection

```text
input: cap_root_hash = [0; 32] (uninitialized / null)
expect: typed-discriminator construction still valid (zero digest is BLAKE3-256 output)
        domain caller SHOULD reject at façade layer; substrate accepts as opaque digest
```

### TV-AUD-v2-30: chain_hash second-preimage resistance

```text
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
- Add §S5 `AuditEvent` field boundary doc-comment
- Add §S6 canonical-bytes form doc-comment
- All substrate changes are doc-only + version bump (no logic changes)

### Phase 2: Façade `octo-audit` v1.x → v2.0.0 release

- Bump crate version to 2.0.0
- Add `octo-audit-core` v2.0.0 dep
- Re-export unchanged canonical types
- Add §S4 `AuditFilter` field constraint doc-comment at façade (Layer B projection, NOT substrate per §S4)
- Add typed-discriminator helper functions at façade root (**DEFERRED — lands at acceptance**)

### Phase 3: Adapter `StoolapAuditSink` conformance verification

- Verify existing adapter impl satisfies §S2.5 atomic persistence
- Verify existing adapter impl scrubs errors per §SC2
- Add test vectors TV-AUD-v2-2 through TV-AUD-v2-5

## Key Files to Modify

- `crates/octo-audit-core/src/event.rs` — doc-comment updates for §S1 + §S5
- `crates/octo-audit-core/src/sink.rs` — doc-comment updates for §S2 invariants
- `crates/octo-audit-core/src/error.rs` — doc-comment updates for §S3 variant boundary
- `crates/octo-audit-core/src/chain.rs` — doc-comment update for §S6 canonical-bytes form
- `crates/octo-audit-core/Cargo.toml` — version bump 1.x → 2.0.0
- `crates/octo-audit/src/lib.rs` — version bump + re-exports
- `crates/octo-audit/Cargo.toml` — `octo-audit-core` dep bump to 2.0.0

## Future Work

### §FW1 — RFC-0012-v3

Subsequent typed-discriminator extensions (e.g. `CapabilityMint`, `CapabilityAttenuate`) follow the §S1 pattern. Note: `Redaction` is already in RFC-0012-v2 — not future work. Each extension kind is added to the table via a new RFC; no substrate enum edits.

### §FW2 — Cross-crate extension helpers

Façade `octo_audit::audit_event` module owns the canonical typed-discriminator helpers (`audit_event_for_agent_transition`, `audit_event_for_redaction`, `audit_event_for_agent_transition_receipt` (cross-RFC; per RFC-0014-v2 §S7), `audit_event_for_ask_rejected` (cross-RFC; per RFC-0014-v2 §S7) — **DEFERRED — lands at acceptance**). Domain crates (e.g. `octo-wallet`, `octo-vault`) call façade helpers; they do NOT define their own helper per CLAUDE.md §Single source of truth. The two cross-RFC helpers (`audit_event_for_agent_transition_receipt`, `audit_event_for_ask_rejected`) recover the audit event for the corresponding receipt extension kind and are canonical-owned at `octo_audit::audit_event` per RFC-0014-v2 §S7 + §FW2 (settlement-side helper pair declared there; audit-side canonical form declared here).

### §FW3 — Receipt-extension cross-RFC invariants

RFC-0014-v2 pins the typed-discriminator pattern via `ask_id` namespaces. Cross-RFC consistency: `AuditEventKind::Insert` + `cap_root_hash = agent-transition-typed-discriminator` corresponds to a settlement `Receipt { ask_id = agent-transition-receipt-typed-discriminator }`. Status is recovered at façade projection (RFC-0016-a `ReceiptSummary`), NOT stored as a substrate field. (Format reference: Future Work bold-prose entry format per §FW1 above.)

### §FW4 — Atomic persistence test infrastructure

§S2.5 mandates transaction-scoped atomic persistence. RFC-0012-v2 TV-AUD-v2-8 pins the substrate test vector; a corresponding adapter-side crash-injection test infrastructure is needed for `StoolapAuditSink` conformance verification. Future mission: add crash-injection harness to the `octo-audit` test suite.

### §FW5 — Cross-RFC receiver reference

The canonical cross-RFC pointer for the settlement-side audit-event receiver (`audit_event_for_agent_transition_receipt`) lives at RFC-0014-v2 §FW4; this RFC's §FW2 above is the canonical audit-side ownership declaration. Pairing invariants are enforced at acceptance per §AC-8.

### §FW6 — Cross-RFC consensus-invariance scrubber patterns

Canonical cross-RFC scrubber-pattern sharing lives at RFC-0014-v2 §FW6 (Canonical Scrubber Patterns + per-façade duplication rationale + cross-RFC consistency invariants). This RFC defers to RFC-0014-v2 §FW6 as the single source of truth; audit-side scrubber (`octo_audit::scrub`) and settlement-side scrubber (`octo_settlement::scrub`) duplicate the canonical 10-pattern list per RFC-0014-v2 §FW6 rationale (no sibling Layer B coupling). Adapter-side conformance gate runs at acceptance per §Implementation Phases.

**Added patterns (canonical):**

- **Pattern 5d** — IPv4 address (with optional port): `(?i)\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?::[0-9]{1,5})?\b`
- **Pattern 5e** — UUID (canonical form): `(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b`

## Rationale

RFC-0012-v2 codifies the typed-discriminator extension pattern as the canonical substrate-faithful mechanism for new audit event semantics. This achieves three goals:

1. **Layer A frozen preservation** — `AuditEventKind` stays 3-variant; `AuditEvent` stays 7-field; `AuditError` stays 3-variant. No substrate churn from extension additions.
2. **Forward compatibility** — `#[non_exhaustive]` discipline means downstream `match` expressions on `AuditEventKind` continue to compile (substrate-stored events remain forward-compatible).
3. **Type safety** — typed-discriminator namespaces are BLAKE3-256 digests (2^128 collision operations birthday bound; cross-prefix second-preimage attacks at ~2^256 with one fixed prefix, ~2^128 birthday for attacker-chosen both prefixes); cross-extension-kind spoofing requires BLAKE3 cross-prefix collision attack.

The cost is a doc-comment-driven extension pattern that domain crates must follow. This is acceptable per CLAUDE.md §Extension over enumeration — the substrate stays minimal; extension mechanics live at the façade/domain boundary.

## Version History

| Version      | Date       | Author                                | Notes                                                                               |
| ------------ | ---------- | ------------------------------------- | ----------------------------------------------------------------------------------- |
| v2.0.0-draft | 2026-09-11 | CipherOcto Architecture Working Group | Initial draft                                                                       |
| v2.0.0-r30.5 | 2026-09-11 | CipherOcto Architecture Working Group | Prose alignment, DEFERRED scrubber + AuditFilter                                    |
| v2.0.0-r31.5 | 2026-09-11 | CipherOcto Architecture Working Group | Scrubber Layer A → Layer B, BLAKE3 math fix                                         |
| v2.0.0-r32.5 | 2026-09-11 | CipherOcto Architecture Working Group | TV-AUD-v2-3 variant revert, Authors/Maintainers H2, cite sweep 160/160              |
| v2.0.0-r33.5 | 2026-09-11 | CipherOcto Architecture Working Group | A5 timestamp, receipt_id_for rename, TV-AUD-v2-6 type leak fix                      |
| v2.0.0-r34.5 | 2026-09-11 | CipherOcto Architecture Working Group | Per-façade scrubber, adapter-type registry, AC-10 Layer D path                      |
| v2.0.0-r35.5 | 2026-09-11 | CipherOcto Architecture Working Group | DEFERRED markers on TV-AUD-v2-8/17, UC-AUD-002 scrubber cross-crate ref fix         |
| v2.0.0-r36   | 2026-09-11 | CipherOcto Architecture Working Group | file:line + KeyedHasher + Layer Direction notes                                     |
| v2.0.0-r36.5 | 2026-09-11 | CipherOcto Architecture Working Group | Adversarial Review H2, VH compression, audit_event_for_* façade ownership canonical |
| v2.0.0-r37   | 2026-09-11 | CipherOcto Architecture Working Group | A4 prev_chain_hash payload-hash wording, AC-7 rephrase                              |
| v2.0.0-r38   | 2026-09-11 | CipherOcto Architecture Working Group | Cross-RFC wording drift, hygiene em-dash sweep                                      |
| v2.0.0-r38.5 | 2026-09-11 | CipherOcto Architecture Working Group | Scrubber code-fence audit, §S5.1/§S6.1 collapse check                               |
| v2.0.0-r39   | 2026-09-11 | CipherOcto Architecture Working Group | VH numbering fill, AuditFilter DEFERRED marker alignment                            |
| v2.0.0-r39.5 | 2026-09-11 | CipherOcto Architecture Working Group | extension_kinds drop, subject_did DEFERRED, A4 chain-linkage-only reassert          |
| v2.0.0-r40   | 2026-09-11 | CipherOcto Architecture Working Group | R41 findings: VH+§FW6+AC-11+AC-12+R41-sp                                            |
| v2.0.0-r40.5 | 2026-09-11 | CipherOcto Architecture Working Group | R41 fix: VH r40/r40.5, §FW6, AC-11 runtime, AC-12 soften                            |
| v2.0.0-r41   | 2026-09-11 | CipherOcto Architecture Working Group | cite sweep 161/161, layer direction note finalization                               |
| v2.0.0-r41.5 | 2026-09-11 | CipherOcto Architecture Working Group | cite sweep post-Layer B hygiene check                                               |
| v2.0.0-r42   | 2026-09-11 | CipherOcto Architecture Working Group | AC-13 paired adapter location verification                                          |
| v2.0.0-r42.5 | 2026-09-11 | CipherOcto Architecture Working Group | substrate path hard-check sweep, phantom path detection                             |
| v2.0.0-r43   | 2026-09-11 | CipherOcto Architecture Working Group | §Adversarial Review narrative extension through R36.5                               |
| v2.0.0-r43.5 | 2026-09-11 | CipherOcto Architecture Working Group | 0014-v2 Layer-D cascade miss; 0012-v2 partial subset fix                            |
| v2.0.0-r44   | 2026-09-11 | CipherOcto Architecture Working Group | 5-lens review returned 65 findings across 5 lenses                                  |
| v2.0.0-r44.5 | 2026-09-11 | CipherOcto Architecture Working Group | 9 findings fixed; 0014-v2 cascade missed (R45 input)                                |
| v2.0.0-r45   | 2026-09-11 | CipherOcto Architecture Working Group | 5-lens review returned 34 findings across 5 lenses                                  |
| v2.0.0-r45.5 | 2026-09-11 | CipherOcto Architecture Working Group | 8 findings fixed; cross-file sync to 0014-v2                                        |
| v2.0.0-r48.5 | 2026-09-12 | CipherOcto Architecture Working Group | 3 trailing-period DEFERRED markers stripped (R48.5 fix)                             |
| v2.0.0-r49   | 2026-09-12 | CipherOcto Architecture Working Group | DRY CLOSED at R48 + R49 across all 5 lenses                                         |
| v2.0.0       | 2026-09-12 | CipherOcto Architecture Working Group | Accepted (paired atomic promotion with RFC-0014-v2)                                |

## Related RFCs

- RFC-0012 — parent RFC; defines substrate `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`.
- RFC-0014-v2 — sibling substrate amendment for settlement extension pattern. Cross-RFC consistency: RFC-0014-v2 FW2 is fixed to align with RFC-0012-v2 FW2 canonical form (façade owns the canonical typed-discriminator helpers; domain crates call façade helpers; they do NOT define their own helper per CLAUDE.md §Single source of truth). The sibling fixer handles RFC-0014-v2 in its own R38.5 pass.
- RFC-0015-a — wallet agent write-path amendment; requires RFC-0012-v2 for typed-discriminator of `AgentTransition` events.
- RFC-0016-a — audit receipt write-path amendment; requires RFC-0012-v2 for `ChainHash` newtype + canonical-bytes-on-write invariant.
- RFC-0011-a — wallet subcommands; cross-RFC reference for scrubber location (Layer B façade, not Layer A substrate).
- RFC-0010 — DID canonical form; substrate stores `node_did: String` (raw canonical wire form per RFC-0010).

## Related Use Cases

- **UC-AUD-001 — Agent lifecycle audit trail.** When an agent transitions state (RFC-0015-a `transition_agent`), an audit event with `event_kind = AuditEventKind::Insert` and `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")` is appended to the sink. Cross-RFC pairing with the corresponding `AgentTransitionReceipt` is recovered at the façade via the canonical helper `audit_event_for_agent_transition_receipt` per RFC-0014-v2 §S7 + §FW2; the audit event's `prev_chain_hash` remains substrate-load-bearing for audit chain verification (chain-hash linkage, NOT receipt linkage).
- **UC-AUD-002 — Capability redaction.** When a capability is redacted (e.g. compromise recovery), an audit event with `event_kind = AuditEventKind::Revoke` and `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")` is appended. The `reason` payload scrubbing per the canonical scrubber (`octo_audit::scrub::scrub_adapter_error`, RFC-0012-v2 §SC2 — audit-scrubber canonical spec; settlement-scrubber canonical spec is RFC-0014-v2 §S5.1) is **DEFERRED — lands at acceptance**; in v2.0.0 the redaction event is persisted with the unscrubbed `reason` payload; the scrubber integration lands at acceptance.
- **UC-AUD-003 — CLI receipt listing with `AuditFilter`.** When a CLI consumer runs `octo audit list --since <unix> --until <unix> --capability-root <hex> --model <model> --limit <n>` (RFC-0016-a), the façade constructs an `AuditFilter` per §S4 and applies it to the audit store. CLI namespace pre-condition: `octo audit list` is **PROVISIONAL pending RFC-0016-a §6.7 ratification**. Subject-DID ACL and receipt-status filtering are out of v2.0.0 scope.
- **UC-AUD-004 — Cross-replica sync.** When a downstream replica syncs the audit chain, sync events use `event_kind = AuditEventKind::Sync` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/sync/v1/")`. The `prev_chain_hash` field carries the last persisted chain hash from the source replica, enabling chain-integrity verification on receipt.

## Appendices

### Appendix A: Extension kind namespace registry

| Extension kind   | Namespace string                                   | `cap_root_hash` (BLAKE3-256 hex, first 8 bytes)      |
| ---------------- | -------------------------------------------------- | ---------------------------------------------------- |
| CapabilityInsert | `cipherocto/audit/extension/capability-insert/v1/` | `0x...` (**TBD at compile-time const**; RFC-0012)    |
| CapabilityRevoke | `cipherocto/audit/extension/capability-revoke/v1/` | `0x...` (**TBD at compile-time const**; RFC-0012)    |
| Sync             | `cipherocto/audit/extension/sync/v1/`              | `0x...` (**TBD at compile-time const**; RFC-0012)    |
| AgentTransition  | `cipherocto/audit/extension/agent-transition/v1/`  | `0x...` (**TBD at compile-time const**; RFC-0012-v2) |
| Redaction        | `cipherocto/audit/extension/redaction/v1/`         | `0x...` (**TBD at compile-time const**; RFC-0012-v2) |

The full 32-byte hex digests are documented here as canonical namespace strings; per Appendix B the substrate does NOT own an `extension_kinds` module. The `0x...` placeholders in this table are markers for the canonical hex values that compile-time `const BLAKE3` expressions at the façade (`octo-audit::audit_event` per Appendix B) resolve to; they are not hand-computed values, and they do NOT require a substrate module.

### Appendix B: Substrate-vs-façade boundary

| Concern                              | Substrate (Layer A)                      | Façade (Layer B)                                                                                                                                                                           |
| ------------------------------------ | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `AuditEvent` struct                  | owned                                    | re-export                                                                                                                                                                                  |
| `AuditEventKind` enum                | owned                                    | re-export                                                                                                                                                                                  |
| `AuditError` enum                    | owned                                    | re-export                                                                                                                                                                                  |
| `AppendOnlyAuditSink` trait          | owned                                    | re-export + concrete `StoolapAuditSink` impl                                                                                                                                               |
| `compute_chain_hash`                 | owned                                    | re-export                                                                                                                                                                                  |
| `verify_chain`                       | owned                                    | re-export                                                                                                                                                                                  |
| `AuditFilter` projection             | NOT owned (façade concern)               | owned (Layer B projection)                                                                                                                                                                 |
| Scrubber                             | NOT owned (Layer A stays pure substrate) | owned (Layer B per RFC-0011-a §7.7 — `scrub_adapter_error` + `scrub_adapter_error_with` live at `octo_audit::scrub`; **lands at acceptance** per Phase 1 substrate-code amendment mission) |
| Typed-discriminator helpers          | doc-comment only                         | owned (`audit_event_for_*` functions)                                                                                                                                                      |
| Storage adapter (`StoolapAuditSink`) | NOT owned                                | owned (DOMAIN (Layer B-faithful) storage adapter)                                                                                                                                          |

RFC-0012-v2 explicitly pins this boundary. Substrate stays free of projection logic, scrubber logic, and storage adapter logic — all three are Layer B concerns per CLAUDE.md §Layer direction rule.

### Appendix C: Layer Direction Note

Per CLAUDE.md §Architectural Principles + `cipherocto-design-principles.md` Layer model, RFC-0012-v2 pins the canonical layer direction for audit substrate code:

| Layer                                             | Concrete location                          | Owns                                                                                                                                                                                               |
| ------------------------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A** (substrate-frozen)                          | `crates/octo-audit-core/`                  | `AuditEvent`, `AuditEventKind`, `AuditError`, `AppendOnlyAuditSink`, `compute_chain_hash`, `verify_chain` (no `extension_kinds` module; typed-discriminator digests live at façade per Appendix B) |
| **B** (DOMAIN (Layer B-faithful) storage adapter) | `crates/octo-audit/src/storage/stoolap.rs` | `StoolapAuditSink` concrete impl + adapter-specific error mapping into `AuditError::SinkSpecific` with canonical format-string                                                                     |

Layer B (façade) details — including substrate re-exports, `AuditFilter` projection, `scrub_adapter_error` / `scrub_adapter_error_with`, and typed-discriminator helper module `octo_audit::audit_event` (**DEFERRED — lands at acceptance**) — live at `Appendix B: Substrate-vs-façade boundary` per the canonical substrate-vs-façade boundary table.

**Direction rule:** A → B → C → D/E. Never the reverse. DOMAIN is Layer B-faithful; transport adapters are Layer D; specialized nodes are Layer C. The substrate (Layer A) does NOT depend on `octo-storage-core`, `octo-audit`, or any storage adapter crate. The DOMAIN façade + storage adapter (Layer B) depend on the substrate (Layer A) only. Layer D (transport adapters — BLE/USB/TCP/QUIC/HID per CLAUDE.md §Crate stability table) is not applicable to the audit substrate; audit's DOMAIN storage adapter lives at Layer B per `crates/octo-audit/src/lib.rs` §Module layout preamble ("kept at the DOMAIN layer"). Per CLAUDE.md §Stable Abstractions Principle, this direction survives 10-year cryptographic migrations: PQC migration touches Layer A only; business-logic churn (capability variants, write-path amendments, storage adapter additions) touches Layer B without disturbing the substrate.

**Adapter-location reference:** When RFC-0012-v2 prose needs to cite the canonical `StoolapAuditSink` location (e.g. for §G4 adapter enforcement reference), this Appendix §Layer Direction Note is the single source of truth. Inline file:line refs in prose violate CLAUDE.md §No line refs; the canonical citation is `Appendix §Layer Direction Note`.
