# RFC-0016-a: Audit Receipt Write-Path Amendment (DEFERRED surface)

## Status

Draft (2026-09-11)

> **Sibling amendment to RFC-0016.** This document carries the DEFERRED write-path surface that requires paired acceptance of RFC-0012-v2 (Layer A `octo-audit-core::AuditEventKind` extensions) + RFC-0014-v2 (Layer A `octo-settlement-core::ReceiptStatus` enum + `Receipt` field extensions + `receipt_id_for_digest` reverse-mapping function) + RFC-0011-a (CLI-shape `[ADD]` error envelope). Without all three substrate amendments, the surface described here cannot land.

> **R29 split plan:** RFC-0016 KEEP (substrate-faithful read surface) at `rfcs/draft/process/0016-audit-receipt-api.md` was slimmed to 5 KEEP items per §6.1 + substrate-canonical 3-variant `AuditError` re-export per §6.2.5. This amendment carries the remaining 12+ DEFERRED items per §Dependencies pairing invariant.

## Authors

- `@cipherocto`
- `@mmacedoeu`

## Maintainers

- `@cipherocto`
- `@mmacedoeu`

## Summary

This amendment specifies the write-path + projection + ACL + scrubber surface that completes RFC-0016 once the required substrate amendments land. Pairing invariants:

- **RFC-0012-v2** — `octo-audit-core::AuditEventKind` extends with `AgentTransition { agent_uuid, from, to, reason_hash, prev_chain_hash }` + `Redaction { prev_hash, reason }` (Layer A frozen extension; CLAUDE.md §Extension over enumeration pattern)
- **RFC-0014-v2** — `octo-settlement-core::ReceiptStatus { Ok, Partial, Reject }` enum + `Receipt` field extensions (`model: String`, `cost_dqa: u64`, `capability_root: [u8; 32]`, `subject_did: Did`) + `receipt_id_for_digest(digest: &[u8; 32]) -> Option<ReceiptId>` reverse-mapping function (Layer A frozen extension)
- **RFC-0011-a** — canonical `[ADD]` error envelope pattern: per-variant `#[error(transparent)] From<AuditError>` conversions at `octo-cli/src/error.rs` boundary land CLI-shape variants `ReceiptNotFound(String)` + `InvalidFilter(String)` + `PermissionDenied` + `AuditSubstrateNotReady` (Layer B façade envelope substrate)

## Dependencies

- **RFC-0016** — Audit Receipt API (KEEP substrate-faithful read surface; this amendment is the sibling)
- **RFC-0012-v2** — required for `append_audit_event` (AuditEventKind extensions + canonical-bytes-on-write invariant + single-writer lock)
- **RFC-0014-v2** — required for `ReceiptStatus` enum + `ReceiptSummary` projection + `AuditFilter.subject_did` ACL + `receipt_id_for_digest` reverse-mapping function
- **RFC-0011-a** — required for CLI-shape `[ADD]` error envelope pattern (per-variant `From<AuditError>` conversions at `octo-cli/src/error.rs` boundary)

## Pairing invariant

> Acceptance of RFC-0016-a REQUIRES paired acceptance of RFC-0012-v2 + RFC-0014-v2 + RFC-0011-a. Layer A frozen extensions follow CLAUDE.md §Extension over enumeration pattern — new variants on existing enums (`AuditEventKind`, `ReceiptStatus`) without central edit; new substrate fields on `Receipt` follow §No premature coupling + §Stable Abstractions Principle.

## Design Goals

1. **Substrate-faithful at acceptance** — every new type/variant matches an existing RFC-0012-v2 / RFC-0014-v2 substrate amendment
2. **Write-path surface paired with audit substrate** — `append_audit_event` accepts `&mut AppendOnlyAuditSink` (RFC-0012-v2 single-writer lock)
3. **Canonical-bytes-on-write invariant** — substrate rejects events whose canonical bytes don't match BLAKE3 chain link (R20.5 finding M-4 acceptance criterion)
4. **Read-stalls-while-write** — concurrent readers (`list_receipts` + `get_receipt`) stall for the duration of write per RFC-0012-v2 single-writer lock contract

## Motivation

The KEEP RFC-0016 covers the read surface only. Operators and CLI missions require write-path surface for state-machine transitions (e.g., `octo_wallet::transition_agent` per RFC-0015-a writes an `AuditEventKind::AgentTransition` row). The write-path also enables:

- `ReceiptSummary` projection: CLI missions can render `list_receipts` rows as compact summaries without exposing every canonical `Receipt` field
- `AuditFilter.subject_did` ACL: multi-tenant deployments can enforce per-tenant read scoping
- CLI-shape error variants: CLI missions can surface substrate errors with operator-friendly exit codes (17 = ReceiptNotFound, 16 = InvalidFilter, 13 = PermissionDenied, 52 = AuditSubstrateNotReady)
- Defense-in-depth scrubber newtypes: type-level enforcement prevents secret material from leaking through `SinkSpecific(String)` payload even if the substrate-side scrubber pattern misses an edge case

## Roles and Authorities

- **Operator:** consumes `append_audit_event` write surface indirectly via CLI missions (e.g., `octo agent transition` → `octo_wallet::transition_agent` → `octo_audit::append_audit_event`)
- **Auditor:** verifies receipt-store chain integrity + tenant isolation via `AuditFilter.subject_did` ACL
- **Wallet substrate:** sole writer of `AuditEventKind::AgentTransition` rows (via RFC-0015-a `transition_agent` → `append_audit_event` paired write path)

## Specification

### §6.1 Public surface additions (paired-with-substrate-amendment)

| Item                                                | Type                                                                                                        | Substrate amendment required                                 |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `append_audit_event`                                | `fn(sink: &mut AppendOnlyAuditSink, event: AuditEvent) -> Result<ChainHash, octo_audit_core::AuditError>`   | RFC-0012-v2 (AuditEventKind extensions + single-writer lock) |
| `ChainHash(pub [u8; 32])` newtype                   | paired with `append_audit_event` return type                                                                | RFC-0012-v2                                                  |
| `ReceiptId(pub u64)` newtype                        | paired with `get_receipt(id: &ReceiptId)` signature change + `receipt_id_for_digest` reverse-mapping        | RFC-0014-v2                                                  |
| `ReceiptStatus` re-export                           | `pub use octo_settlement_core::ReceiptStatus`                                                               | RFC-0014-v2                                                  |
| `ReceiptSummary` projection struct                  | paired with `list_receipts(filter: &AuditFilter) -> Result<Vec<ReceiptSummary>, ...>` signature change      | RFC-0014-v2                                                  |
| `AuditFilter.subject_did`                           | `pub subject_did: Option<Did>` (ACL field)                                                                  | RFC-0014-v2 (Receipt.subject_did substrate field)            |
| `AuditFilter.status` (multi-valued)                 | `pub status: Vec<StatusRef>` (UNION semantics)                                                              | RFC-0014-v2 (canonical ReceiptStatus enum)                   |
| `StatusRef` type alias                              | `pub type StatusRef = ReceiptStatus` (canonical Layer A enum re-export)                                     | RFC-0014-v2                                                  |
| `AuditError::AuditAppendFailed` variant reservation | paired with `append_audit_event` write path                                                                 | RFC-0012-v2 + RFC-0011-a                                     |
| CLI-shape error variants                            | `OctoCliError::{ReceiptNotFound(decimal), InvalidFilter(reason), PermissionDenied, AuditSubstrateNotReady}` | RFC-0011-a per-variant `From<AuditError>` conversions        |
| `ScrubbedAuditError` + `ScrubbedString` newtypes    | Layer B façade-only type-level scrub enforcement                                                            | none (façade-only)                                           |

### §6.2 Function contract: `append_audit_event`

```rust
// RFC-0012-v2 prerequisite: AuditEventKind extends with AgentTransition { ... } + Redaction { prev_hash, reason }
pub fn append_audit_event(
    sink: &mut AppendOnlyAuditSink,
    event: AuditEvent,
) -> Result<ChainHash, octo_audit_core::AuditError> {
    // 1. Acquire single-writer lock on sink (RFC-0012-v2 contract; Rust borrow checker rejects concurrent &mut on same instance)
    // 2. Canonicalize event via AppendOnlyAuditSink::canonical_bytes (canonical-bytes-on-write invariant per §6.6 acceptance criterion)
    // 3. Compute BLAKE3-256 chain-hash using prev_chain_hash from sink head + canonical bytes
    // 4. Verify canonical bytes match BLAKE3 chain link (RFC-0012 §Adversary Analysis "Threat: append-rollback via sink bypass")
    // 5. Persist event + chain-hash (canonical bytes, not caller-supplied payload)
    // 6. Release single-writer lock (block concurrent readers per §6.7 read-stalls-while-write invariant)
    // 7. Return ChainHash (32-byte BLAKE3 chain-hash)
}
```

### §6.3 Newtype: `ChainHash`

```rust
/// Canonical BLAKE3 chain-hash of an AuditEvent row.
/// Returned by `append_audit_event` for downstream verification + cross-substrate reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainHash(pub [u8; 32]);

impl Display for ChainHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}
```

### §6.4 Newtype: `ReceiptId`

```rust
/// Canonical Receipt primary-key newtype (paired with RFC-0014-v2 `receipt_id_for_digest` reverse-mapping).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReceiptId(pub u64);

impl Display for ReceiptId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)  // canonical decimal u64 form
    }
}

impl From<[u8; 32]> for ReceiptId {
    /// Requires RFC-0014-v2 `receipt_id_for_digest` reverse-mapping function in `octo_settlement_core`.
    /// Without it, conversion would require full store scan (violates substrate-faithful O(1) primary-key lookup).
    fn from(digest: [u8; 32]) -> Self {
        Self(octo_settlement_core::receipt_id_for_digest(&digest).expect("canonical digest"))
    }
}
```

### §6.5 `ReceiptSummary` projection struct (paired with RFC-0014-v2)

```rust
/// Compact summary projection of canonical Receipt for CLI list output.
/// Pairs with RFC-0014-v2 Receipt field extensions: model, cost_dqa, capability_root, subject_did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptSummary {
    pub receipt_id: ReceiptId,
    pub ask_id: String,
    pub model: String,
    pub cost_dqa: u64,
    pub capability_root: [u8; 32],
    pub subject_did: Did,
    pub executed_at_unix: u64,  // alias for Receipt::timestamp_unix (canonical substrate field)
    pub status: ReceiptStatus,
}

impl ReceiptSummary {
    /// Substrate-faithful projection: maps canonical Receipt → ReceiptSummary preserving substrate sort order.
    pub fn from_canonical(receipt: octo_settlement::Receipt) -> Self {
        Self {
            receipt_id: ReceiptId(receipt.receipt_id),
            ask_id: receipt.ask_id,
            model: receipt.model,                    // RFC-0014-v2 field
            cost_dqa: receipt.cost_dqa,              // RFC-0014-v2 field
            capability_root: receipt.capability_root, // RFC-0014-v2 field
            subject_did: receipt.subject_did,        // RFC-0014-v2 field
            executed_at_unix: receipt.timestamp_unix,// canonical substrate field per RFC-0014 §Data Structures
            status: receipt.status,                  // RFC-0014-v2 ReceiptStatus enum
        }
    }
}
```

### §6.6 AuditFilter extension (paired with RFC-0014-v2)

```rust
pub struct AuditFilter {
    pub since_unix: Option<u64>,
    pub until_unix: Option<u64>,
    pub capability_root: Option<[u8; 32]>,
    pub model: Option<String>,
    pub limit: Option<u32>,

    // RFC-0016-a additions:
    pub subject_did: Option<Did>,    // multi-tenant ACL; pairs with RFC-0014-v2 Receipt.subject_did field
    pub status: Vec<StatusRef>,      // UNION semantics per RFC-0011-a §Filters --include-reject
}

// Type alias for canonical Layer A enum (RFC-0014-v2)
pub type StatusRef = ReceiptStatus;
```

### §6.7 CLI-shape error variants (paired with RFC-0011-a)

Per RFC-0011-a canonical `[ADD]` error envelope pattern, per-variant `#[error(transparent)] From<AuditError>` conversions land at `octo-cli/src/error.rs`:

| Substrate variant (paired-with-RFC-0011-a)                    | CLI variant                              | CLI exit | RFC-0011-a slot            |
| ------------------------------------------------------------- | ---------------------------------------- | -------- | -------------------------- |
| `octo_audit_core::AuditError::SequenceGap { event_id, prev }` | `OctoCliError::Internal(reason)`         | 64       | parent reserved            |
| `octo_audit_core::AuditError::AlreadyExists(u64)`             | `OctoCliError::Internal(reason)`         | 64       | parent reserved            |
| `octo_audit_core::AuditError::SinkSpecific(String)`           | `OctoCliError::Internal(reason)`         | 64       | parent reserved            |
| (CLI-shape, RFC-0016-a) `ReceiptNotFound(decimal)`            | `OctoCliError::ReceiptNotFound(decimal)` | 17       | RFC-0011-a §Error Handling |
| (CLI-shape, RFC-0016-a) `InvalidFilter(reason)`               | `OctoCliError::InvalidFilter(reason)`    | 16       | parent reserved            |
| (CLI-shape, RFC-0016-a) `PermissionDenied(reason)`            | `OctoCliError::PermissionDenied`         | 13       | RFC-0011 §Exit Codes       |
| (CLI-shape, RFC-0016-a) `AuditAppendFailed(reason)`           | `OctoCliError::AuditSubstrateNotReady`   | 52       | RFC-0011-c §9.8 slot 52    |

### §6.8 Scrub newtypes (Layer B façade-only)

```rust
/// Type-level scrub enforcement: substrate-side scrubber MUST pass before constructing this variant.
/// Defense-in-depth second pass at Layer B façade boundary; canonical `<REDACTED>` marker preservation per R21 L-2 idempotency rule.
#[derive(Debug)]
pub struct ScrubbedAuditError(pub octo_audit_core::AuditError);

/// Canonical `<REDACTED>`-marked string payload for `SinkSpecific` variants.
/// Substrate-side scrubber produces this from raw input per §Compatibility #4 scrubber patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrubbedString(pub String);

impl ScrubbedString {
    /// Apply canonical scrubber patterns and produce `<REDACTED>` marker if any match.
    pub fn new(raw: &str) -> Self {
        if crate::scrubber::matches_scrub_pattern(raw) {
            Self("<REDACTED>".to_string())
        } else {
            Self(raw.to_string())
        }
    }
}
```

### §6.9 Substrate-canonical scrubber patterns (Layer B façade)

The substrate-side scrubber applies the canonical 13-pattern list before constructing `SinkSpecific(String)` per §Compatibility #4:

1. Ed25519 private key hex (64 chars)
2. Capability-secret base64 (padded + unpadded)
3. BLS12-381 Fr scalar hex (64 chars = 32 bytes per RFC wire form)
4. secp256k1 64-char hex
5. BIP39 mnemonic (12/15/18/21/24-word)
6. JWT three-segment form (`header.payload.signature`)
7. WIF base58 (51 chars)
8. Filesystem absolute path (POSIX `/...`, Windows `C:\...`, macOS `/Users/...`) → `<OCTO_HOME>/...` canonical placeholder
9. PGP private key block (`-----BEGIN PGP PRIVATE KEY BLOCK-----`)
10. OpenSSH private key (`-----BEGIN OPENSSH PRIVATE KEY-----`)
11. PEM block (`-----BEGIN ... PRIVATE KEY-----`)
12. X.509 cert serial `0x`-prefixed hex (64 chars)
13. `<REDACTED>` marker idempotency (preserve verbatim, do NOT double-scrub per R21 L-2)

### §6.10 Canonical-bytes-on-write invariant (acceptance criterion)

`AppendOnlyAuditSink::append` MUST:

1. Re-canonicalize ALL fields (including `at_unix`, `agent_uuid`, `from`, `to`, `reason_hash`, `prev_chain_hash`) via `canonical_bytes(event)`
2. Compute BLAKE3 chain-hash over canonical bytes (NOT caller-supplied payload)
3. Verify canonical bytes match chain-link hash (reject on mismatch per RFC-0012 §Adversary Analysis)
4. Persist canonical bytes (NOT caller payload)

**Acceptance criterion:** acceptance of RFC-0012-v2 without canonical-bytes-on-write is a regression on the append-rollback attack surface.

### §6.11 Single-writer lock + read-stalls-while-write invariant (acceptance criterion)

`AppendOnlyAuditSink::append` MUST:

1. Acquire per-instance single-writer lock (Rust `&mut self` enforces this at type level)
2. Hold lock for canonicalize + BLAKE3 chain-link + persist sequence
3. Concurrent readers (`list_receipts` + `get_receipt` from RFC-0016 KEEP) MUST stall (block) for the duration of the write
4. Release lock on success OR failure (idempotent cleanup)

**Acceptance criterion:** acceptance of RFC-0012-v2 without read-stall is a regression on the read-during-write race.

## Performance Targets

- `append_audit_event` happy path — p95 < 2ms in-process; persists on shutdown
- Concurrent reader stall — bounded by write critical section (canonicalize + BLAKE3 + persist)
- `list_receipts` with `subject_did` ACL — p95 < 110ms (ACL walk adds ~10% over base 100ms)

## Implicit Assumptions Audit

1. **Substrate-amendment pairing** — every type/variant here requires paired RFC-0012-v2 + RFC-0014-v2 + RFC-0011-a acceptance
2. **`ReceiptStatus::Unknown` Raw escape hatch** — if RFC-0014-v2 adds `Unknown` arm for future-proofing, `StatusRef` resolution via `From<ReceiptStatus>` handles it
3. **Multi-tenant trust boundary** — `AuditFilter.subject_did` ACL enforcement is per-process (caller-supplied `subject_did`); does NOT prevent in-process co-tenant reads (CLI-only `subject_did` injection)
4. **BLAKE3 determinism** — RFC-0012 chain-link hash uses BLAKE3-256; canonical bytes MUST include all variant-tagged fields
5. **Single-writer per sink instance** — `&mut AppendOnlyAuditSink` enforces single-writer per Rust borrow checker; concurrent writers must serialize via external mutex (out of scope)

## Security Considerations

1. **Append-rollback via sink bypass** — canonical-bytes-on-write invariant per §6.10 mitigates forged `AuditEvent` rows with fabricated `at_unix`
2. **Read-during-write race** — single-writer lock + read-stall per §6.11 mitigates torn reads
3. **`subject_did` ACL** — multi-tenant deployments MUST enforce `subject_did` filter at CLI; per-process trust boundary for in-process co-tenants
4. **Scrubber defense-in-depth** — substrate-side scrubber patterns per §6.9 + CLI-side `OctoCliRedactor` per RFC-0011-a §Redaction (two-pass)
5. **CLI-shape error variant leakage** — `SinkSpecific` payload carries canonical decimal `u64` for `ReceiptNotFound`, scrubbed paths for `PermissionDenied`; no secret material in CLI-shape variants

## Adversarial Review

### Threat: append-rollback via sink bypass

**Adversary:** Compromised CLI / wallet binary attempts to invoke `append_audit_event` with a forged `AuditEvent` row carrying a fabricated `at_unix` timestamp.

**Mitigation:** `AppendOnlyAuditSink::canonical_bytes(event)` re-canonicalizes ALL fields including `at_unix`; BLAKE3 hash includes the canonical bytes. Substrate rejects events whose canonical bytes don't match the BLAKE3 chain link. No silent insertion.

### Threat: read-during-write race

**Adversary:** Concurrent reader observes partial write state (some fields updated, others stale).

**Mitigation:** §6.11 single-writer lock holds during canonicalize + BLAKE3 + persist; concurrent readers stall until write completes.

### Threat: cross-tenant receipt read

**Adversary:** Co-tenant on shared receipt store attempts to read another tenant's receipts by omitting `subject_did` filter.

**Mitigation:** `AuditFilter.subject_did: Option<Did>` ACL field enforces per-tenant scoping at façade boundary; CLI missions MUST pass `subject_did` per tenant credentials.

## Adversary Analysis (5-Question Test)

| Threat                               | Q1: Who?                          | Q2: What?                                                           | Q3: Why?                                    | Q4: How mitigated?                                                                                                                                              | Q5: Residual risk?                                                           |
| ------------------------------------ | --------------------------------- | ------------------------------------------------------------------- | ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Append-rollback via sink bypass      | Compromised CLI                   | Forged AuditEvent row                                               | Fabricate transition log                    | Canonical-bytes-on-write invariant per §6.10                                                                                                                    | Substrate bug = total compromise (low)                                       |
| Read-during-write race               | Concurrent reads                  | Read inconsistency                                                  | Data inconsistency                          | Single-writer lock + read-stall per §6.11                                                                                                                       | Transient (retry-safe)                                                       |
| Cross-tenant receipt read            | Co-tenant on shared receipt store | Read another tenant's receipts                                      | Reconnaissance / enumeration                | `AuditFilter.subject_did` ACL per §6.6 + per-process trust boundary via `mode 0700` parent-dir check                                                            | In-process co-tenant reads not blocked (per-process trust boundary)          |
| CLI-shape error variant leakage      | Compromised CLI                   | Surface substrate error payload containing secret                   | Leak private key / path / mnemonic          | Substrate-side scrubber per §6.9 + CLI-side `OctoCliRedactor` per RFC-0011-a §Redaction                                                                         | Scrubber pattern miss = leakage window (low, scrubber covers 13 patterns)    |
| Downgrade to bare `ReceiptId` lookup | Compromised CLI                   | Bypass `ReceiptId` newtype via `From<[u8; 32]>` for unmapped digest | Denial of service via reverse-mapping panic | `From<[u8; 32]> for ReceiptId` requires `receipt_id_for_digest` reverse-mapping per RFC-0014-v2; unmapped digests return `Option<None>` → substrate-level error | Substrate bug = panic surface (low, RFC-0014-v2 substrate contract enforces) |

## Economic Analysis

DEFER — audit receipt write path has no direct token cost; cite RFC-0900+ (Role Economics) for any cost implications.

## Compatibility

1. **Substrate-amendment dependency** — every type/variant here requires paired RFC-0012-v2 + RFC-0014-v2 + RFC-0011-a acceptance
2. **CLI exit-code additions** — slots 17 (ReceiptNotFound), 16 (InvalidFilter), 13 (PermissionDenied), 52 (AuditSubstrateNotReady) are pre-allocated per RFC-0011 §Exit Codes + RFC-0011-c §9.8 slot 52; this amendment consumes those slots
3. **Façade re-export additions** — `pub use octo_settlement_core::ReceiptStatus` adds re-export to `octo-audit` Layer B façade (Layer B → Layer B hop per CLAUDE.md §Architectural Principles)
4. **Backward compat with RFC-0016 KEEP** — RFC-0016 KEEP's `list_receipts(filter: &AuditFilter) -> Result<Vec<Receipt>, AuditError>` signature remains valid (canonical `Receipt` projection); this amendment adds `ReceiptSummary` projection as ADDITIVE overload (separate function `list_receipt_summaries`)
5. **Substrate-side scrubber patterns** — canonical 13-pattern list per §6.9 supersedes RFC-0016 KEEP §Compatibility #4 8-pattern list

## Test Vectors

Substrate-level test vectors (`crates/octo-audit/src/lib.rs` test module). All DEFERRED vectors from RFC-0016 KEEP §Test Vectors land here.

| #                          | Substrate call                                                                                                           | Input                                                                                              | Expected Output                                                                                              | Notes                                                                                 |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| TV-AUD-3                   | `list_receipts(&AuditFilter { status: Some(StatusRef::Ok), model: None, ... })`                                          | 1000-receipt store                                                                                 | `Ok(filtered_by_status_ok)`                                                                                  | Server-side filter (using `StatusRef` alias per §6.6; RFC-0014-v2 prerequisite)       |
| TV-AUD-3-status-multi      | `list_receipts(&AuditFilter { status: vec![StatusRef::Ok, StatusRef::Partial], ... })`                                   | 1000-receipt store                                                                                 | `Ok(filtered_by_status_ok_or_partial)` (UNION semantics)                                                     | Multi-valued status filter per §6.6                                                   |
| TV-AUD-3-subject           | `list_receipts(&AuditFilter { subject_did: Some(<canonical-did>), ... })`                                                | 1000-receipt store                                                                                 | `Ok(filtered_by_subject_did)`                                                                                | Subject ACL per §6.6                                                                  |
| TV-AUD-4                   | `list_receipts(&AuditFilter { limit: Some(0) })`                                                                         | any store                                                                                          | `Err(OctoCliError::InvalidFilter("filter: limit out of range".into()))` (CLI exit 16)                        | limit=0 rejection rule per RFC-0016 §6.2.4                                            |
| TV-AUD-4b                  | `list_receipts(&AuditFilter { since_unix: Some(100), until_unix: Some(50) })`                                            | any store                                                                                          | `Err(OctoCliError::InvalidFilter("filter: since_unix > until_unix".into()))` (CLI exit 16)                   | Range-inversion validation per RFC-0016 §6.2.4                                        |
| TV-AUD-4c                  | `list_receipts(&AuditFilter { limit: Some(20000) })`                                                                     | 20000-receipt store                                                                                | `Err(OctoCliError::InvalidFilter("filter: limit must be 1..=10000".into()))` (CLI exit 16)                   | limit > 10000 rejection rule per RFC-0016 §6.2.4                                      |
| TV-AUD-4d                  | `list_receipts(&AuditFilter { model: Some("ed25519_hex_64_chars_...") })`                                                | 1000-receipt store                                                                                 | `Err(OctoCliError::InvalidFilter("model: <REDACTED>".into()))` (CLI exit 16; substrate-side scrubbed)        | Substrate-side scrubber coverage for ed25519 hex per §6.9 pattern 1                   |
| TV-AUD-4e                  | `list_receipts(&AuditFilter { model: Some("WIF: L1aW4aubDFB7yfras2S3mKxL7g2Kz8mN5pQ9rS3tU7vW2xY") })`                    | 1000-receipt store                                                                                 | `Err(OctoCliError::InvalidFilter("model: <REDACTED>".into()))` (CLI exit 16; substrate-side scrubbed)        | End-to-end coverage: substrate-side scrub catches WIF pattern per §6.9 pattern 7      |
| TV-AUD-5                   | `get_receipt(&unknown_id)`                                                                                               | Unknown `Receipt::receipt_id: u64`                                                                 | `Err(OctoCliError::ReceiptNotFound("<decimal-u64>".into()))` (CLI exit 17)                                   | Canonical decimal `u64` form; CLI-shape variant per §6.7                              |
| TV-AUD-5-receiptid         | `get_receipt(&ReceiptId(unknown))`                                                                                       | Unknown digest mapped via `receipt_id_for_digest`                                                  | `Err(OctoCliError::ReceiptNotFound("<decimal-u64>".into()))` (CLI exit 17)                                   | `ReceiptId` newtype + reverse-mapping per §6.4                                        |
| TV-AUD-7                   | `append_audit_event(&mut sink, AgentTransition row)`                                                                     | Valid event                                                                                        | `Ok(ChainHash(<32-bytes>))` + chain-row inserted                                                             | Happy path; signature takes `&mut AppendOnlyAuditSink` (single-writer lock per §6.11) |
| TV-AUD-7-canonical-bytes   | `append_audit_event(&mut sink, AgentTransition row)` with caller-supplied payload differing from canonical bytes         | Caller payload has `at_unix: 12345`; canonical bytes would have `at_unix: determined_by_sink_head` | `Err(octo_audit_core::AuditError::SinkSpecific("canonical bytes mismatch".into()))`                          | Canonical-bytes-on-write invariant per §6.10 (R20.5 finding M-4 acceptance criterion) |
| TV-AUD-7-read-stall        | Concurrent: writer calls `append_audit_event(&mut sink, ...)`; reader calls `list_receipts(&filter)` from another thread | Writer holds single-writer lock; reader attempts walk                                              | Reader stalls (blocks) until writer releases lock; then reads canonical post-write state                     | Single-writer lock + read-stall per §6.11 (R20.5 finding M-6 acceptance criterion)    |
| TV-AUD-8                   | `append_audit_event(&mut sink, AgentTransition row)` with `at_millis_unix: 0` outside monotonic range                    | Non-monotonic timestamp                                                                            | `Err(OctoCliError::AuditSubstrateNotReady("non-monotonic timestamp".into()))` (CLI exit 52)                  | Monotonic guard per RFC-0012 AuditEvent fields                                        |
| TV-AUD-11                  | `list_receipts(&AuditFilter::default())` with upstream error containing 32-byte hex private-key-shape string             | Substrate error string contains 64-char hex matching `ed25519_private_key` shape                   | `Err(octo_audit_core::AuditError::SinkSpecific("key: <REDACTED>".into()))` (substrate-side scrubbed)         | Redaction contract per §6.9 pattern 1                                                 |
| TV-AUD-11a                 | `Internal("BLS12-381 Fr scalar hex: 0x4f6c8b2a...")`                                                                     | BLS12-381 Fr scalar hex                                                                            | `Err(octo_audit_core::AuditError::SinkSpecific("BLS12-381 Fr scalar hex: <REDACTED>".into()))`               | Substrate-side scrub per §6.9 pattern 3                                               |
| TV-AUD-11b                 | `Internal("secp256k1 privkey hex: 0xa1b2c3d4...")`                                                                       | secp256k1 privkey hex                                                                              | `Err(octo_audit_core::AuditError::SinkSpecific("secp256k1 privkey hex: <REDACTED>".into()))`                 | Substrate-side scrub per §6.9 pattern 4                                               |
| TV-AUD-11c                 | `Internal("BIP39 mnemonic: abandon ...")`                                                                                | 12-word BIP39 mnemonic                                                                             | `Err(octo_audit_core::AuditError::SinkSpecific("BIP39 mnemonic: <REDACTED>".into()))`                        | Substrate-side scrub per §6.9 pattern 5                                               |
| TV-AUD-11d                 | `Internal("JWT: eyJ...")`                                                                                                | JWT three-segment form                                                                             | `Err(octo_audit_core::AuditError::SinkSpecific("JWT: <REDACTED>".into()))`                                   | Substrate-side scrub per §6.9 pattern 6                                               |
| TV-AUD-11e                 | `Internal("WIF: L1aW4...")`                                                                                              | WIF base58                                                                                         | `Err(octo_audit_core::AuditError::SinkSpecific("WIF: <REDACTED>".into()))`                                   | Substrate-side scrub per §6.9 pattern 7                                               |
| TV-AUD-11f                 | `Internal("capability-secret base64: ...")`                                                                              | Base64 secret                                                                                      | `Err(octo_audit_core::AuditError::SinkSpecific("capability-secret base64: <REDACTED>".into()))`              | Substrate-side scrub per §6.9 pattern 2                                               |
| TV-AUD-11h                 | `Internal("PGP private key block: -----BEGIN PGP PRIVATE KEY BLOCK-----\n...")`                                          | PGP private key block                                                                              | `Err(octo_audit_core::AuditError::SinkSpecific("PGP private key block: <REDACTED>".into()))`                 | Substrate-side scrub per §6.9 pattern 9                                               |
| TV-AUD-11i                 | `Internal("OpenSSH private key: -----BEGIN OPENSSH PRIVATE KEY-----...")`                                                | OpenSSH private key                                                                                | `Err(octo_audit_core::AuditError::SinkSpecific("OpenSSH private key: <REDACTED>".into()))`                   | Substrate-side scrub per §6.9 pattern 10                                              |
| TV-AUD-11j                 | `Internal("PEM block: -----BEGIN RSA PRIVATE KEY-----...")`                                                              | PEM private key block                                                                              | `Err(octo_audit_core::AuditError::SinkSpecific("PEM block: <REDACTED>".into()))`                             | Substrate-side scrub per §6.9 pattern 11                                              |
| TV-AUD-11k                 | `Internal("X.509 cert serial: 0x4f6c8b...")`                                                                             | X.509 cert serial `0x`-prefixed hex                                                                | `Err(octo_audit_core::AuditError::SinkSpecific("X.509 cert serial: <REDACTED>".into()))`                     | Substrate-side scrub per §6.9 pattern 12                                              |
| TV-AUD-list-chain-1        | `list_receipts(&AuditFilter::default())` against a 5-row receipt store with row N=2 BLAKE3 chain link corrupted          | 5-row store, row N=2 chain_hash does NOT match `compute_chain_hash(row)`                           | `Err(octo_audit_core::AuditError::SinkSpecific("chain verification failed".into()))`                         | Chain-integrity guard at read boundary                                                |
| TV-AUD-get-receipt-chain-1 | `get_receipt(&known_id)` against a 5-row receipt store with row N=2 BLAKE3 chain link corrupted                          | 5-row store, row N=2 chain_hash does NOT match `compute_chain_hash(row)`; queried row IS corrupted | `Err(octo_audit_core::AuditError::SinkSpecific("chain verification failed".into()))`                         | Chain-integrity guard for `get_receipt`                                               |
| TV-AUD-redact-token-1      | `Internal("key: <REDACTED>")` (literal marker in error string)                                                           | substrate error string contains the literal `<REDACTED>` marker                                    | `Err(octo_audit_core::AuditError::SinkSpecific("key: <REDACTED>".into()))` (marker preserved verbatim)       | Redaction marker idempotency per §6.9 pattern 13                                      |
| TV-AUD-redact-token-2      | `Internal("user-supplied field: <REDACTED>")`                                                                            | No plaintext secret present; substrate emits `<REDACTED>` literal                                  | `Err(octo_audit_core::AuditError::SinkSpecific("user-supplied field: <REDACTED>".into()))` (no double-scrub) | Verifies no accidental double-redaction                                               |
| TV-AUD-permission-check-1  | `list_receipts(&AuditFilter::default())` when `$OCTO_HOME/audit/receipts` parent dir is NOT mode `0700`                  | parent dir mode `0755`                                                                             | `Err(OctoCliError::PermissionDenied("permission denied: <OCTO_HOME>/audit/receipts".into()))` (CLI exit 13)  | Substrate-enforced per-process trust boundary                                         |
| TV-AUD-permission-check-2  | `get_receipt(&known_id)` when parent dir is owned by different UID than process UID                                      | parent dir owned by `uid=1000`, process runs as `uid=1001`                                         | `Err(OctoCliError::PermissionDenied("permission denied: <OCTO_HOME>/audit/receipts".into()))` (CLI exit 13)  | Per-process trust boundary; rejects point lookups on multi-user hosts                 |

CLI-level test vectors live in RFC-0011-a §Test Vectors (UNCHANGED at R2; expansion lands at RFC-0011-a acceptance with paired RFC-0016-a).

## Alternatives Considered

- **`SinkSpecific(String)` only at R2 KEEP** — already chosen; no alternative considered
- **CLI-shape error variants as substrate-canonical** — rejected: parallel abstraction per [[cipherocto-design-principles]]; substrate remains 3-variant, CLI-shape variants land via RFC-0011-a per-variant From conversions
- **`AppendOnlyAuditSink::append` as the public write function** — rejected: type-level `&mut self` requires explicit `append_audit_event(sink: &mut, ...)` façade function for ergonomic substrate-faithful boundary
- **Pre-RFC-0014-v2 `ReceiptSummary` projection without substrate fields** — rejected: would force CLI to backfill field values, violating substrate-faithful principle

## Implementation Phases

- **Phase 0 (RFC-0016 acceptance at R2 KEEP)** — read surface lands; this amendment DEFERRED
- **Phase 1 (RFC-0012-v2 acceptance)** — `AuditEventKind` extensions + canonical-bytes-on-write invariant + single-writer lock + read-stall
- **Phase 2 (RFC-0014-v2 acceptance)** — `ReceiptStatus` enum + `Receipt` field extensions + `receipt_id_for_digest` reverse-mapping
- **Phase 3 (RFC-0011-a acceptance)** — CLI-shape `[ADD]` error envelope pattern + per-variant From conversions at `octo-cli/src/error.rs`
- **Phase 4 (RFC-0016-a acceptance — paired with Phase 1 + 2 + 3)** — `append_audit_event` + `ChainHash` + `ReceiptId` + `ReceiptSummary` + `AuditFilter.subject_did` + `AuditFilter.status` multi-valued + CLI-shape error variants + `ScrubbedAuditError` + substrate-side scrubber patterns land
- **Phase 5 (RFC-0015-a acceptance — paired with RFC-0016-a)** — `octo_wallet::transition_agent` writes `AuditEventKind::AgentTransition` rows via `append_audit_event`

## Key Files to Modify

- `crates/octo-audit/src/lib.rs` — add `append_audit_event` + `ChainHash` newtype + `ScrubbedAuditError` + `ScrubbedString` + scrubber patterns module
- `crates/octo-audit/src/scrubber.rs` — NEW; canonical 13-pattern substrate-side scrubber
- `crates/octo-audit-core/src/event.rs` — RFC-0012-v2: `AuditEventKind` extends with `AgentTransition { agent_uuid: Uuid, from: AgentState, to: AgentState, reason_hash: [u8; 32], prev_chain_hash: [u8; 32] }` + `Redaction { prev_hash: ChainHash, reason: String }`
- `crates/octo-settlement-core/src/receipt.rs` — RFC-0014-v2: `Receipt` extends with `model: String` + `cost_dqa: u64` + `capability_root: [u8; 32]` + `subject_did: Did` + `status: ReceiptStatus`
- `crates/octo-settlement-core/src/status.rs` — RFC-0014-v2: NEW; `ReceiptStatus { Ok, Partial, Reject }` enum (#[non_exhaustive] per CLAUDE.md §Extension over enumeration)
- `crates/octo-settlement-core/src/id.rs` — RFC-0014-v2: NEW; `ReceiptId(pub u64)` newtype + `receipt_id_for_digest(digest: &[u8; 32]) -> Option<ReceiptId>` reverse-mapping
- `crates/octo-audit-core/src/sink.rs` — RFC-0012-v2: `AppendOnlyAuditSink::append` adds `canonical_bytes(event)` + single-writer lock + read-stall
- `crates/octo-cli/src/error.rs` — RFC-0011-a: per-variant `#[error(transparent)] From<octo_audit_core::AuditError>` conversions + CLI-shape variant constructors

**Layer placement table:**

| Crate                  | Layer                        | Substrate anchor                                                                    | Role at RFC-0016-a acceptance                                   |
| ---------------------- | ---------------------------- | ----------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `octo-audit-core`      | Layer A frozen (RFC-0012-v2) | `AuditEventKind` (extended) + `AppendOnlyAuditSink` (lock + canonical-bytes)        | Canonical substrate write surface                               |
| `octo-settlement-core` | Layer A frozen (RFC-0014-v2) | `Receipt` (extended) + `ReceiptStatus` enum + `ReceiptId` + `receipt_id_for_digest` | Canonical substrate projection + ID substrate                   |
| `octo-audit`           | Layer B façade (RFC-0016-a)  | `append_audit_event` + `ChainHash` + `ScrubbedAuditError` + scrubber module         | Façade write surface + substrate-side scrubber defense-in-depth |
| `octo-settlement`      | Layer B façade (RFC-0014-v2) | `ReceiptStatus` + `ReceiptId` re-exports                                            | Re-exports Layer A frozen extensions                            |
| `octo-cli`             | Layer C (RFC-0011-a)         | `OctoCliError` per-variant From conversions                                         | CLI-shape error envelope + exit-code mapping                    |

Layer direction: `octo-audit` (Layer B) → `octo-settlement` (Layer B) → `octo-settlement-core` (Layer A frozen). `octo-cli` (Layer C) → `octo-audit` (Layer B) → `octo-audit-core` (Layer A). No reverse deps. No C→A direct edges.

## Future Work

- **`AuditEventKind::Redaction` variant (paired with redaction surface)** — `Redaction { prev_hash: ChainHash, reason: String }` row appended AFTER the target row; out of scope for RFC-0016-a
- **`ReceiptStatus::Unknown` Raw escape hatch** — if RFC-0014-v2 substrate acceptance adds it, `StatusRef` resolution handles it
- **Cursor-streaming `list_receipts`** — for >10000-row stores; reserved for Phase 6
- **`append_audit_event` async variant** — for non-blocking writes; deferred to substrate-faithful async substrate adoption

## Rationale

- **Paired-with-substrate-amendment** — every type/variant requires paired RFC-0012-v2 + RFC-0014-v2 + RFC-0011-a acceptance; cannot land in isolation
- **Layer A frozen extensions follow extension-over-enumeration pattern** — `AuditEventKind::AgentTransition` + `Redaction` + `ReceiptStatus::{Ok, Partial, Reject}` + `Receipt` field additions are additive; no central enum edit
- **Read-stall invariant** — substrate-faithful contract per §6.11 prevents torn reads at canonical substrate boundary
- **Scrubber defense-in-depth** — substrate-side scrubber + CLI-side `OctoCliRedactor` two-pass; covers 13 canonical patterns
- **Split strategy** — write surface paired-with-substrate-amendment moved from RFC-0016 KEEP to RFC-0016-a per R29 review plan; halves per-RFC complexity, breaks divergence loop

## Version History

- v1.0 (2026-09-11) Initial draft. Carries DEFERRED surface from RFC-0016 v1.0 §6.9 split per R29 review plan.

## Related RFCs

- RFC-0016 — Audit Receipt API (KEEP substrate-faithful read surface; parent RFC for this amendment)
- RFC-0012-v2 — Audit Substrate Layer A frozen extension (provides `AuditEventKind::{AgentTransition, Redaction}` + canonical-bytes-on-write invariant + single-writer lock)
- RFC-0014-v2 — Settlement Substrate Layer A frozen extension (provides `ReceiptStatus` enum + `Receipt` field extensions + `ReceiptId` + `receipt_id_for_digest` reverse-mapping)
- RFC-0011-a — `octo` CLI Substrate amendment (provides canonical `[ADD]` error envelope pattern + CLI-shape error variants via per-variant From conversions)
- RFC-0015-a — Wallet Agent Write-Path Amendment (paired sibling; `transition_agent` consumes `append_audit_event`)
- RFC-0015 — Wallet Agent Operations Substrate (KEEP read surface; parent of RFC-0015-a)
- RFC-0012 — Audit Substrate (Layer A frozen; canonical substrate surface; v2 extension here)
- RFC-0014 — Settlement Substrate (Layer A frozen; canonical substrate surface; v2 extension here)
- RFC-0011 — `octo` CLI Substrate (parent RFC; RFC-0011-a is the canonical amendment)
- RFC-0010 — Canonical DID Codec (DID parsing for `AuditFilter.subject_did`)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping for `append_audit_event` Class B write)
- [[cipherocto-design-principles]] — Layer model + extension-over-enumeration principle; **§6.10 + §6.11 acceptance criteria follow substrate-faithful invariant enforcement**

## Related Use Cases

- `docs/use-cases/audit-transparency.md` — receipt read + verify + transition log use case (paired with RFC-0015-a `transition_agent`)
- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — runtime attach / run context for transaction logs + audit append

## Appendices

### Appendix A. Substrate function signatures (full Rust surface)

```rust
// crates/octo-audit/src/lib.rs (append to RFC-0016 KEEP surface)

// RFC-0016-a additions (requires RFC-0012-v2 + RFC-0014-v2 + RFC-0011-a paired acceptance):

// §6.2 append_audit_event — write path
pub fn append_audit_event(
    sink: &mut AppendOnlyAuditSink,
    event: AuditEvent,
) -> Result<ChainHash, octo_audit_core::AuditError> {
    // 1. Acquire single-writer lock on sink (Rust &mut enforces type-level)
    // 2. Canonicalize event via sink.canonical_bytes(event)
    // 3. Verify canonical bytes match BLAKE3 chain link (§6.10 invariant)
    // 4. Persist canonical bytes
    // 5. Release lock; concurrent readers unblock (§6.11 read-stall)
    // 6. Return ChainHash
}

// §6.3 ChainHash newtype
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainHash(pub [u8; 32]);

impl Display for ChainHash { /* hex::encode */ }

// §6.4 ReceiptId newtype (paired with RFC-0014-v2 receipt_id_for_digest)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReceiptId(pub u64);

impl From<[u8; 32]> for ReceiptId {
    fn from(digest: [u8; 32]) -> Self {
        Self(octo_settlement_core::receipt_id_for_digest(&digest).expect("canonical digest"))
    }
}

// §6.5 ReceiptSummary projection (paired with RFC-0014-v2 Receipt field extensions)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptSummary {
    pub receipt_id: ReceiptId,
    pub ask_id: String,
    pub model: String,
    pub cost_dqa: u64,
    pub capability_root: [u8; 32],
    pub subject_did: Did,
    pub executed_at_unix: u64,  // alias for Receipt::timestamp_unix
    pub status: ReceiptStatus,
}

// §6.6 AuditFilter extension (RFC-0016-a additions; KEEP surface from RFC-0016 preserved)
pub struct AuditFilter {
    pub since_unix: Option<u64>,
    pub until_unix: Option<u64>,
    pub capability_root: Option<[u8; 32]>,
    pub model: Option<String>,
    pub limit: Option<u32>,
    pub subject_did: Option<Did>,
    pub status: Vec<StatusRef>,
}

// §6.6 StatusRef type alias
pub type StatusRef = octo_settlement::ReceiptStatus;

// §6.8 Scrub newtypes
#[derive(Debug)]
pub struct ScrubbedAuditError(pub octo_audit_core::AuditError);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrubbedString(pub String);

// Re-export ReceiptStatus from Layer B settlement façade
pub use octo_settlement::ReceiptStatus;
```

### Appendix B. Mermaid diagram — write-path flow (RFC-0016-a)

```mermaid
sequenceDiagram
    participant Wallet as octo-wallet (Layer B)
    participant Aud as octo-audit (Layer B)
    participant Sink as AppendOnlyAuditSink (Layer A)
    participant Core as octo-audit-core (Layer A)
    participant Scrub as octo-audit/scrubber.rs (Layer B)

    Wallet->>Aud: append_audit_event(&mut sink, AgentTransition { agent_uuid, from, to, ... })
    Aud->>Sink: &mut borrow acquires single-writer lock
    Sink->>Core: canonical_bytes(event) re-canonicalizes ALL fields
    Core-->>Sink: canonical_bytes_buf
    Sink->>Sink: compute BLAKE3 chain-hash over canonical_bytes_buf
    Sink->>Sink: verify canonical_bytes match BLAKE3 chain link (§6.10 invariant)
    Sink->>Sink: persist event + chain-hash (canonical bytes, not caller payload)
    Sink-->>Aud: Ok(ChainHash(<32-bytes>))
    Aud->>Scrub: scrub ChainHash byte representation (defense-in-depth)
    Scrub-->>Aud: ScrubbedString(<hex>)
    Aud-->>Wallet: Ok(ChainHash)
    Note over Sink: Concurrent readers (list_receipts, get_receipt) stall until lock release (§6.11)
```

### Appendix C. Pairing acceptance checklist

RFC-0016-a acceptance REQUIRES all three substrate amendments to land in this order:

1. **RFC-0012-v2 ACCEPTED** — `octo-audit-core::AuditEventKind` extends; `AppendOnlyAuditSink::append` adds canonical-bytes-on-write + single-writer lock + read-stall
2. **RFC-0014-v2 ACCEPTED** — `octo_settlement_core::ReceiptStatus` enum added; `Receipt` field extensions added; `ReceiptId` newtype + `receipt_id_for_digest` reverse-mapping function added
3. **RFC-0011-a ACCEPTED** — canonical `[ADD]` error envelope pattern lands at `octo-cli/src/error.rs`; per-variant `#[error(transparent)] From<AuditError>` conversions added; CLI-shape variants `ReceiptNotFound(String)` + `InvalidFilter(String)` + `PermissionDenied` + `AuditSubstrateNotReady` added
4. **RFC-0016-a ACCEPTED** — this amendment; write surface + projection + ACL + scrubber patterns land
5. **RFC-0015-a ACCEPTED** (paired sibling) — `octo_wallet::transition_agent` calls `append_audit_event` to write `AuditEventKind::AgentTransition` rows

Pairing invariant: any subset acceptance is a substrate-faithful drift; all five steps must land in order.
