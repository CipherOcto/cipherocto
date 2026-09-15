# RFC-0016-a: Audit Receipt Write-Path Amendment (DEFERRED surface)

## Status

Draft (2026-09-11; v1.3 amendment in flight 2026-09-14)

> **Sibling amendment to RFC-0016.** This document carries the DEFERRED write-path surface whose acceptance is bound by the canonical pairing invariant (see §Pairing invariant).

> **R29 split plan:** RFC-0016 (substrate-faithful read surface) at `rfcs/draft/process/0016-audit-receipt-api.md` was slimmed to 5 KEEP items per §6.1 + substrate-canonical 3-variant `AuditError` re-export per §6.2.5. This amendment carries the remaining 12+ DEFERRED items per §Pairing invariant.

> **v1.3 Substrate Sweep (2026-09-14):** see §Substrate-Faithful Amendment Trail for per-amendment ground-truth + acceptance criteria.

## Authors

- `@cipherocto`
- `@mmacedoeu`

## Maintainers

- `@cipherocto`
- `@mmacedoeu`

## Summary

This amendment specifies the write-path + projection + ACL + scrubber surface that completes RFC-0016 once the required substrate amendments land. The paired-substrate enumeration is canonical at §Pairing invariant.

## Dependencies

- **RFC-0016** — Audit Receipt API (substrate-faithful read surface; parent RFC for this amendment)
- **RFC-0012** + **RFC-0014** + **RFC-0011** — see §Pairing invariant (the canonical statement)

## Pairing invariant

> Acceptance of RFC-0016 REQUIRES paired acceptance of RFC-0012 + RFC-0014 + RFC-0011. Crate-layer mapping (per CLAUDE.md §Crate stability table): RFC-0012 lands at `octo-audit-core` (Layer A frozen); RFC-0014 lands at `octo-settlement-core` (Layer A frozen); RFC-0011 lands at `octo-cli` (Layer C consumer). This amendment adds façade surface at `octo-audit` (Layer B) + `octo-settlement` (Layer B) per §Layer placement. Layer A frozen extensions follow CLAUDE.md §Extension over enumeration pattern — new variants on existing enums (e.g., `AuditEventKind::AgentTransition`) without central edit per RFC-0012 §S5.1; new enum introductions (e.g., `ReceiptStatus` per RFC-0014 §S5) carry `#[non_exhaustive]` from the start per CLAUDE.md §Extension over enumeration; new substrate fields on `Receipt` follow CLAUDE.md §No premature coupling + CLAUDE.md §Stable Abstractions Principle.

## Design Goals

1. **Substrate-faithful at acceptance** — every new type/variant matches an existing RFC-0012 / RFC-0014 substrate amendment
2. **Write-path surface paired with audit substrate** — `append_audit_event` accepts `&mut dyn AppendOnlyAuditSink` (RFC-0012 single-writer lock; `dyn` keyword required for trait-object parameter per Rust 2021)
3. **Canonical-bytes-on-write invariant** — substrate rejects events whose canonical bytes don't match BLAKE3 chain link (R20.5 finding M-4 acceptance criterion)
4. **Read-stalls-while-write** — concurrent readers (`list_receipts` + `get_receipt`) stall for the duration of write per RFC-0012 single-writer lock contract

## Motivation

The items retained in RFC-0016 per §Pairing invariant cover the read surface only. Operators and CLI missions require write-path surface for state-machine transitions (e.g., `octo_wallet::transition_agent` per RFC-0015 writes an `AuditEventKind::AgentTransition` row). The write-path also enables:

- `ReceiptSummary` projection: CLI missions can render `list_receipts` rows as compact summaries without exposing every canonical `Receipt` field
- `AuditFilter.subject_did` ACL: multi-tenant deployments can enforce per-tenant read scoping
- CLI-shape error variants: CLI missions can surface substrate errors with operator-friendly exit codes (17 = ReceiptNotFound, 16 = InvalidFilter, 13 = PermissionDenied, 52 = AuditSubstrateNotReady)
- Defense-in-depth scrubber helper: `redact_substrate_error(raw: &str) -> String` applied at the CLI boundary prevents secret material from leaking through `SinkSpecific(String)` payload even if the substrate-side scrubber pattern misses an edge case; canonical `<REDACTED>` marker idempotency per §6.8

## Roles and Authorities

- **Operator:** consumes `append_audit_event` write surface indirectly via CLI missions (e.g., `octo agent transition` → `octo_wallet::transition_agent` → `octo_audit::append_audit_event`)
- **Auditor:** verifies receipt-store chain integrity + tenant isolation via `AuditFilter.subject_did` ACL
- **Wallet substrate:** sole writer of `AuditEventKind::AgentTransition` rows (via RFC-0015 `transition_agent` → `append_audit_event` paired write path)

## Specification

### §6.1 Public surface additions (paired-with-substrate-amendment)

| Item                                                | Type                                                                                                                              | Substrate amendment required                        |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| `append_audit_event`                                | see §6.2 (substrate 4-step façade sequence per `audit_event_v2.rs`)                                                               | RFC-0012 (AuditEventKind extensions)                |
| `ChainHash(pub [u8; 32])` newtype                   | see §6.3 (substrate-canonical accessors `as_bytes` + `to_hex` per `audit_event_v2.rs`)                                            | RFC-0012                                            |
| `ReceiptId(pub u64)` newtype                        | see §6.4 (paired with `receipt_id_for_digest` reverse-mapping)                                                                    | RFC-0014                                            |
| `StatusRef` type alias (canonical)                  | see §6.6 (Layer B façade type alias; canonical declaration per §6.6 — single source per CLAUDE.md §Stable Abstractions Principle) | RFC-0014                                            |
| `ReceiptSummary` projection struct                  | see §6.5 (substrate-canonical `from_canonical` mapper)                                                                            | RFC-0014                                            |
| `AuditFilter.subject_did`                           | see §6.6 (ACL field; canonical declaration per §6.6)                                                                              | RFC-0016                                            |
| `AuditFilter.status` (multi-valued)                 | see §6.6 (UNION semantics; canonical declaration per §6.6)                                                                        | RFC-0014 (canonical ReceiptStatus enum)             |
| `AuditError::AuditAppendFailed` variant reservation | paired with `append_audit_event` write path                                                                                       | RFC-0012 + RFC-0011                                 |
| CLI-shape error variants                            | see §6.7 (canonical CLI-shape variant declarations)                                                                               | RFC-0011 per-variant `From<AuditError>` conversions |
| `redact_substrate_error` helper function            | see §6.8 (free function form per `scrub_newtypes.rs`; 18-pattern sweep per §6.9 + `<REDACTED>` idempotency)                       | RFC-0012 + RFC-0011                                 |

### §6.2 Function contract: `append_audit_event`

```rust
// RFC-0012 prerequisite: AuditEventKind extends with AgentTransition { agent_id, from, to, reason }
// Substrate-faithful form per `crates/octo-audit/src/audit_event_v2.rs` `pub fn append_audit_event`.
pub fn append_audit_event(
    sink: &mut dyn AppendOnlyAuditSink,
    event: AuditEvent,
) -> Result<ChainHash, octo_audit::AuditError> {
    // Canonical-bytes-on-write invariant per §6.10 acceptance criterion.
    // Returns the canonical chain-hash via Ok(ChainHash(canonical)).
}
```

### §6.3 Newtype: `ChainHash`

```rust
/// Canonical BLAKE3 chain-hash of an AuditEvent row.
/// Returned by `append_audit_event` for downstream verification + cross-substrate reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainHash(pub [u8; 32]);

impl ChainHash {
    /// Substrate canonical accessor: returns the inner 32-byte BLAKE3 chain-hash bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Substrate canonical accessor: returns the canonical 64-char lowercase hex form (no `0x` prefix).
    /// Substrate form per `audit_event_v2.rs` delegates to `hex::encode(self.0)` (canonical hex crate).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for ChainHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Zero-allocation per-byte loop; canonical 64-char lowercase hex (no `0x` prefix).
        // Wire form per RFC-0012; avoids pulling in the `hex` crate at the façade boundary.
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
```

### §6.4 Newtype: `ReceiptId`

```rust
/// Canonical Receipt primary-key newtype (paired with RFC-0014 `receipt_id_for_digest` reverse-mapping).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReceiptId(pub u64);

impl Display for ReceiptId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)  // canonical decimal u64 form
    }
}

// Note: `From<[u8; 32]> for ReceiptId` does NOT exist in substrate — only `From<u64> for ReceiptId` per
// `crates/octo-settlement-core/src/receipt.rs`. Reverse-mapping from raw digest lives at
// `octo_settlement::receipt_id_for_digest(&digest) -> Option<ReceiptId>` (free function returning Option).
// Callers needing digest-to-id resolution call this function directly, not via From.
```

### §6.5 `ReceiptSummary` projection struct (paired with RFC-0014)

```rust
/// Compact summary projection of canonical Receipt for CLI list output.
/// Pairs with RFC-0014 Receipt field extensions: model, cost_dqa, capability_root, subject_did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptSummary {
    pub receipt_id: ReceiptId,
    pub ask_id: String,
    pub model: String,
    pub cost_dqa: u64,
    pub capability_root: [u8; 32],
    pub subject_did: String,  // canonical DID wire form as String (façade free of octo-ident Did dep per §6.6 additive rationale)
    pub executed_at_unix: u64,  // alias for Receipt::timestamp_unix (canonical substrate field)
    pub status: ReceiptStatus,
}

impl ReceiptSummary {
    /// Substrate-faithful projection: maps canonical Receipt → ReceiptSummary preserving substrate sort order.
    pub fn from_canonical(receipt: octo_settlement::Receipt) -> Self {
        Self {
            receipt_id: ReceiptId(receipt.receipt_id),
            ask_id: receipt.ask_id.iter().map(|b| format!("{b:02x}")).collect(),
            model: receipt.model,                    // RFC-0014 field
            cost_dqa: receipt.cost_dqa,              // RFC-0014 field
            capability_root: receipt.capability_root, // RFC-0014 field
            subject_did: receipt.subject_did,        // RFC-0014 field
            executed_at_unix: receipt.timestamp_unix,// canonical substrate field per RFC-0014 §S5
            status: receipt.status,                  // RFC-0014 ReceiptStatus enum
        }
    }
}
```

### §6.6 AuditFilter extension (paired with RFC-0014)

```rust
pub struct AuditFilter {
    pub router_id: Option<String>,
    pub timestamp_unix_gte: Option<u64>,
    pub timestamp_unix_lte: Option<u64>,
    pub since_unix: Option<u64>,
    pub until_unix: Option<u64>,
    pub subject_did: Option<String>,
    pub status: Vec<StatusRef>,
    pub model: Option<String>,
    pub capability_root: Option<[u8; 32]>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

// Type alias for canonical enum re-export via Layer B façade
pub type StatusRef = octo_settlement::ReceiptStatus;
```

### §6.7 CLI-shape error variants (paired with RFC-0011)

Per RFC-0011 canonical `[ADD]` error envelope pattern, `From<AuditError>` conversions land at `octo-cli/src/error.rs`. The 4 substrate variants below collapse to a single `OctoCliError::Internal(reason)` (CLI exit 64) per substrate envelope convention; the 4 distinct CLI-shape variants are RFC-0016 additive surface:

| Substrate variant (paired-with-RFC-0011; row 1 = CLI-shape collapse group, rows 2-5 = one-to-one CLI-shape mapping in substrate enum declaration order)                        | CLI variant                              | CLI exit | RFC-0011 slot                |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------- | -------- | ---------------------------- |
| `SequenceGap { event_id, prev }` / `AlreadyExists(u64)` / `SinkSpecific(String)` / `ChainHashMismatch { event_id }` (4 pre-existing substrate variants; collapse → `Internal`) | `OctoCliError::Internal(reason)`         | 64       | parent reserved              |
| (CLI-shape, RFC-0016) `AuditAppendFailed(reason)`                                                                                                                              | `OctoCliError::AuditSubstrateNotReady`   | 52       | RFC-0011 §Exit Codes slot 52 |
| (CLI-shape, RFC-0016) `ReceiptNotFound(decimal)`                                                                                                                               | `OctoCliError::ReceiptNotFound(decimal)` | 17       | RFC-0011 §Error Handling     |
| (CLI-shape, RFC-0016) `InvalidFilter(reason)`                                                                                                                                  | `OctoCliError::InvalidFilter(reason)`    | 16       | parent reserved              |
| (CLI-shape, RFC-0016) `PermissionDenied(reason)`                                                                                                                               | `OctoCliError::PermissionDenied(reason)` | 13       | RFC-0011 §Exit Codes         |

> **Substrate-faithful note:** row 1 collapse grouping is intentional CLI-shape-bucket mapping (4 pre-existing substrate variants → `Internal(reason)` CLI exit 64); rows 2-5 carry the 4 RFC-0016 additive substrate variants (AuditAppendFailed + ReceiptNotFound + InvalidFilter + PermissionDenied) one-to-one in substrate enum declaration order. Substrate enum order: SequenceGap (1) → AlreadyExists (2) → SinkSpecific (3) → AuditAppendFailed (4) → ReceiptNotFound (5) → InvalidFilter (6) → PermissionDenied (7) → ChainHashMismatch (8). R2.5 collapse rationale documented at §v1.1 amendment #3.

### §6.8 Scrub helper (Layer B façade — `redact_substrate_error` function)

```rust
// crates/octo-audit/src/scrub_newtypes.rs (Layer B façade anchor)
//
// Defense-in-depth second-pass scrubber applied at the Layer B façade boundary.
// Canonical `<REDACTED>` marker preservation per R21 L-2 idempotency rule —
// an already-redacted marker is passed through verbatim (no double-scrub).
//
// The CLI boundary at `octo-cli/src/error.rs` calls this helper before
// constructing `SinkSpecific(String)` payloads per the per-variant
// `From<octo_audit::AuditError>` conversions in §6.7. The 18-pattern
// substrate canonical regex set lives in `octo_audit::scrub` (see §6.9).

/// Scrub `raw` of secret material matching any of the §6.9 canonical patterns.
/// Returns `<REDACTED>` if any pattern matches; otherwise returns `raw` verbatim.
/// Idempotency invariant (R21 L-2): already-redacted payloads collapse to the
/// canonical marker because `scrub_adapter_error` preserves `<REDACTED>` verbatim
/// at the `redact_substrate_error` free function (the marker is itself in the safe
/// alphanumeric set and never re-matches a pattern).
pub fn redact_substrate_error(raw: &str) -> String {
    let scrubbed = scrub_adapter_error(raw);
    if scrubbed == raw {
        raw.to_string()
    } else {
        REDACTED_MARKER.to_string()
    }
}
```

### §6.9 Substrate-canonical scrubber patterns (Layer B façade)

The substrate-side scrubber applies the canonical 18-pattern list before constructing `SinkSpecific(String)` per §Compatibility #4. Pattern numbers below mirror the substrate canonical numbering in `octo-audit::scrub` (substrate pattern number = primary identifier, user-facing label = secondary):

| Substrate # | User-facing label                                                                                                                                                                                                              | Regex / form                                                           | Substrate line                                |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- | --------------------------------------------- |
| 1           | hex digests (≥32 chars) — covers Ed25519 / BLS12-381 Fr scalar / secp256k1 64-char hex                                                                                                                                         | `\b[A-Fa-f0-9]{32,}\b` (word-boundary-anchored)                        | `octo_audit::scrub::RE_HEX`                   |
| 2           | absolute paths (POSIX + Windows + macOS) → `<redacted-path>`                                                                                                                                                                   | path regex → `<redacted-path>` placeholder                             | `octo_audit::scrub::RE_PATH`                  |
| 3           | table-name refs (PostgreSQL + Stoolap/SQLite forms)                                                                                                                                                                            | table regex                                                            | `octo_audit::scrub::RE_TABLE`                 |
| 4           | SQLSTATE prefixes + errno + error-code forms                                                                                                                                                                                   | `(?i)\b(?:SQLSTATE_[A-Z0-9]{5}\|errno \d+\|error code \d+)\b`          | `octo_audit::scrub::RE_SQLSTATE`              |
| 5           | io error chains (`os error N`)                                                                                                                                                                                                 | `(?i)os error \d+`                                                     | `octo_audit::scrub::RE_IO`                    |
| 5b          | URL credentials (`scheme://user:pass@host`, IPv6-literal-aware balanced-bracket)                                                                                                                                               | URL creds regex                                                        | `octo_audit::scrub::RE_URL_CREDS`             |
| 5c          | ANSI-CSI escape sequences (`\x1b\[...m`)                                                                                                                                                                                       | `\x1b\[[0-9;?]*[a-zA-Z]`                                               | `octo_audit::scrub::RE_ANSI`                  |
| 5d          | IPv4 literals (dotted-quad + optional port)                                                                                                                                                                                    | `(?i)\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?::[0-9]{1,5})?\b`                | `octo_audit::scrub::RE_IPV4`                  |
| 5e          | UUIDs (RFC 4122 canonical 8-4-4-4-12)                                                                                                                                                                                          | `(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b` | `octo_audit::scrub::RE_UUID`                  |
| 6           | adapter-type-name substring registry — applies via `scrub_adapter_error_with(s, ADAPTER_TYPES)`; adapter types are NOT a closed set per CLAUDE.md §Extension over enumeration (extension surface, explicit registry parameter) | substring replace (no regex)                                           | `octo_audit::scrub::scrub_adapter_error_with` |
| 11          | PGP private key block (`-----BEGIN PGP PRIVATE KEY BLOCK-----`)                                                                                                                                                                | PGP block regex                                                        | `octo_audit::scrub::RE_PGP_PRIVATE`           |
| 12          | OpenSSH private key (`-----BEGIN OPENSSH PRIVATE KEY-----`)                                                                                                                                                                    | OpenSSH block regex                                                    | `octo_audit::scrub::RE_OPENSSH_PRIVATE`       |
| 13          | PEM block (`-----BEGIN ... PRIVATE KEY-----`)                                                                                                                                                                                  | PEM block regex (cascade-order: OpenSSH before generic PEM)            | `octo_audit::scrub::RE_PEM_PRIVATE`           |
| 14          | capability-secret base64 (padded + unpadded)                                                                                                                                                                                   | cap-secret b64 regex                                                   | `octo_audit::scrub::RE_CAPABILITY_SECRET_B64` |
| 15          | BIP39 mnemonic phrase (12/15/18/21/24-word)                                                                                                                                                                                    | BIP39 word-list regex                                                  | `octo_audit::scrub::RE_BIP39_MNEMONIC`        |
| 16          | JWT three-segment form (`header.payload.signature`)                                                                                                                                                                            | JWT three-segment regex                                                | `octo_audit::scrub::RE_JWT_THREE_SEGMENT`     |
| 17          | WIF base58 private-key form (50-52 chars, canonical 51)                                                                                                                                                                        | `\b[1-9A-HJ-NP-Za-km-z]{50,52}\b`                                      | `octo_audit::scrub::RE_WIF_BASE58`            |
| 18          | X.509 cert serial `0x`-prefixed hex (16-64 hex chars per R6.5 regex relaxation; `0x` prefix breaks Pattern 1 hex ≥32 catch)                                                                                                    | `\b0x[A-Fa-f0-9]{16,64}\b` (word-boundary-anchored)                    | `octo_audit::scrub::RE_X509_SERIAL_HEX`       |

Plus 1 inline guard for `<REDACTED>` marker idempotency (preserve verbatim per R21 L-2, do NOT double-scrub). The 18-pattern substrate canonical count = 17 `Lazy<Regex>` constants + 1 substring registry (Pattern 6) per `octo_audit::scrub` module docstring.

### §6.10 Canonical-bytes-on-write invariant (acceptance criterion — Layer B façade)

`octo_audit::append_audit_event` (Layer B façade) MUST:

1. Re-canonicalize the canonical-form fields (`event_id`, `node_did`, `event_kind` tag byte, `cap_root_hash`, `at_millis_unix`, `prev_chain_hash`) via the Layer A free function `compute_chain_hash(&event)` (BLAKE3 over canonical bytes). Substrate `canonical_bytes` collapses `event_kind` to a single tag byte; variant payload fields (`agent_id`, `from`, `to`, `reason`) are NOT extracted into the canonical-byte buffer.
2. Compare `compute_chain_hash(&event)` against `event.chain_hash`; on mismatch, short-circuit with `Err(AuditError::ChainHashMismatch { event_id })` BEFORE the sink is called.
3. Call `sink.append(&event)?` only after the canonical-bytes check passes (canonical encoding owned by Layer A; façade is read-only with respect to the encoding).

**Layer placement:** the invariant lives at the Layer B façade `append_audit_event`, NOT at the substrate trait `AppendOnlyAuditSink::append`. Substrate trait contract per RFC-0012 §S5 computes chain_hash via the `compute_chain_hash` free function; the façade enforces the invariant at the write boundary as a defense-in-depth check (substrate `audit_event_v2.rs` module-level §6.10 docstring).

**Acceptance criterion:** acceptance of RFC-0012 without canonical-bytes-on-write at the Layer B façade is a regression on the append-rollback attack surface.

### §6.11 Read-stalls-while-write invariant (acceptance criterion — DOMAIN adapter paired-acceptance gate)

**DEFERRED — paired-acceptance of RFC-0012 DOMAIN adapter required.**

The read-stall-while-write acceptance criterion is UNVERIFIABLE at this façade layer because `list_receipts` + `get_receipt` from RFC-0016 touch a separate `RECEIPT_REGISTRY` Mutex from the writer sink (`AUDIT_SINK` Mutex in DOMAIN impl `audit_write.rs`). Per substrate `audit_event_v2.rs` §6.11 module-level docstring, the §6.11 acceptance criterion is gated on the DOMAIN adapter paired-acceptance round (each DOMAIN impl owns the choice of R/W primitive — single shared mutex, `RwLock`, or sharded — and must demonstrate the read-stall property end-to-end at acceptance time).

Substrate-faithful form of the invariant (substrate trait contract — RFC-0012 §S5 substrate spec):

1. `AppendOnlyAuditSink::append(&mut self, event)` enforces single-writer per instance via Rust `&mut self` (type-level; the borrow checker rejects concurrent `&mut` on the same instance).
2. Concurrent readers (`list_receipts` + `get_receipt` from RFC-0016) MUST stall (block) for the duration of the write — DOMAIN adapter paired-acceptance round validates this property end-to-end.
3. Lock released on success OR failure (idempotent cleanup).

**Acceptance criterion:** acceptance of RFC-0012 DOMAIN adapter without demonstrating read-stall at the paired-acceptance round is a regression on the read-during-write race. The façade `append_audit_event` cannot enforce this property in isolation; it is a paired-substrate-acceptance gate.

## Performance Targets

- `append_audit_event` happy path — p95 < 2ms in-process; persists on shutdown
- Concurrent reader stall — bounded by write critical section (canonicalize + BLAKE3 + persist)
- `list_receipts` with `subject_did` ACL — p95 < 110ms (ACL walk adds ~10% over base 100ms)

## Implicit Assumptions Audit

1. **Substrate-amendment pairing** — see §Pairing invariant
2. **`ReceiptStatus::Unknown` arm already canonical** — substrate `octo_settlement_core::ReceiptStatus` declares 4-variant enum (Unknown + Ok + Partial + Reject) with `#[non_exhaustive]` per `crates/octo-settlement-core/src/receipt.rs`; `StatusRef` is a type alias that passes all arms through verbatim without conversion
3. **Multi-tenant trust boundary** — `AuditFilter.subject_did` ACL enforcement is per-process (caller-supplied `subject_did`); does NOT prevent in-process co-tenant reads (CLI-only `subject_did` injection)
4. **BLAKE3 determinism** — RFC-0012 chain-link hash uses BLAKE3-256; canonical bytes MUST include all variant-tagged fields
5. **Single-writer per sink instance** — `&mut dyn AppendOnlyAuditSink` enforces single-writer per Rust borrow checker; concurrent writers must serialize via external mutex (out of scope)
6. **`#[non_exhaustive]` extension surface** — five enums carry `#[non_exhaustive]` per CLAUDE.md §Extension over enumeration: `octo_audit_core::error::AuditError` + `octo_audit_core::error::AuditChainError` + `octo_audit_core::event::AuditEventKind` (Layer A frozen trio; additive variants land without central enum edit per RFC-0012 §S5.1 (per-façade scrubber contract)) + `octo_cli::error::OctoCliError` (Layer C consumer-side additive-growth; `octo-cli` is Layer C per §Layer placement) + `octo_wallet::error::WalletError` (Layer B façade additive-growth). Both `OctoCliError` + `WalletError` carry `#[non_exhaustive]` per CLAUDE.md §Extension over enumeration; asymmetric in layer label, symmetric in additive-growth contract. Per-variant `From<AuditError>` conversions in §6.7 MUST use a catch-all arm (`_ => Self::Internal(sanitize_substrate_error(&format!("audit substrate error: {e}")))` per current `octo-cli/src/error.rs`) to avoid breakage when substrate adds new variants at acceptance-time or at future paired-acceptance amendments.

## Security Considerations

1. **Append-rollback via sink bypass** — canonical-bytes-on-write invariant per §6.10 mitigates forged `AuditEvent` rows with fabricated `at_millis_unix`
2. **Read-during-write race** — single-writer lock + read-stall per §6.11 mitigates torn reads
3. **`subject_did` ACL** — multi-tenant deployments MUST enforce `subject_did` filter at CLI; per-process trust boundary for in-process co-tenants
4. **Scrubber defense-in-depth** — substrate-side scrubber patterns per §6.9 + CLI-side `OctoCliRedactor` per RFC-0011 §Redaction (two-pass)
5. **CLI-shape error variant leakage** — `SinkSpecific` payload carries canonical decimal `u64` for `ReceiptNotFound`, scrubbed paths for `PermissionDenied`; no secret material in CLI-shape variants

## Adversary Analysis (5-Question Test)

| Threat                               | Q1: Who?                          | Q2: What?                                                           | Q3: Why?                                    | Q4: How mitigated?                                                                                                                                           | Q5: Residual risk?                                                        |
| ------------------------------------ | --------------------------------- | ------------------------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------- |
| Append-rollback via sink bypass      | Compromised CLI                   | Forged AuditEvent row                                               | Fabricate transition log                    | Canonical-bytes-on-write invariant per §6.10                                                                                                                 | Substrate bug = total compromise (low)                                    |
| Read-during-write race               | Concurrent reads                  | Read inconsistency                                                  | Data inconsistency                          | Single-writer lock + read-stall per §6.11                                                                                                                    | Transient (retry-safe)                                                    |
| Cross-tenant receipt read            | Co-tenant on shared receipt store | Read another tenant's receipts                                      | Reconnaissance / enumeration                | `AuditFilter.subject_did` ACL per §6.6 + per-process trust boundary via `mode 0700` parent-dir check                                                         | In-process co-tenant reads not blocked (per-process trust boundary)       |
| CLI-shape error variant leakage      | Compromised CLI                   | Surface substrate error payload containing secret                   | Leak private key / path / mnemonic          | Substrate-side scrubber per §6.9 + CLI-side `OctoCliRedactor` per RFC-0011 §Redaction                                                                        | Scrubber pattern miss = leakage window (low, scrubber covers 18 patterns) |
| Downgrade to bare `ReceiptId` lookup | Compromised CLI                   | Bypass `ReceiptId` newtype via `From<[u8; 32]>` for unmapped digest | Denial of service via reverse-mapping panic | `From<[u8; 32]> for ReceiptId` requires `receipt_id_for_digest` reverse-mapping per RFC-0014; unmapped digests return `Option<None>` → substrate-level error | Substrate bug = panic surface (low, RFC-0014 substrate contract enforces) |

## Economic Analysis

DEFER — audit receipt write path has no direct token cost; cite RFC TBD (Role Economics) for any cost implications when registered.

## Compatibility

1. **Substrate-amendment dependency** — see §Pairing invariant
2. **CLI exit-code additions** — slots 17 (ReceiptNotFound), 16 (InvalidFilter), 13 (PermissionDenied), 52 (AuditSubstrateNotReady) are pre-allocated per RFC-0011 §Exit Codes + RFC-0011 §Exit Codes slot 52; this amendment consumes those slots
3. **Façade type-alias additions** — adds `StatusRef = octo_settlement::ReceiptStatus` type alias to `octo-audit` Layer B façade (canonical declaration per §6.6; Layer B aliases Layer A canonical enum per CLAUDE.md §Stable Abstractions Principle)
4. **Backward compat with RFC-0016** — RFC-0016's `list_receipts(filter: &AuditFilter) -> Result<Vec<Receipt>, AuditError>` signature remains valid (canonical `Receipt` projection); this amendment adds `ReceiptSummary` projection as ADDITIVE overload (separate function `list_receipt_summaries`)
5. **Substrate-side scrubber patterns** — canonical 18-pattern list per §6.9 supersedes RFC-0016 §Compatibility #4 8-pattern list

## Test Vectors

Substrate-level test vectors (`crates/octo-audit/src/lib.rs` test module). All DEFERRED vectors from RFC-0016 §Test Vectors land here.

| #                          | Substrate call                                                                                                                                  | Input                                                                                                                              | Expected Output                                                                                                                                                      | Notes                                                                                                                                                                                                                                             |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TV-AUD-3                   | `list_receipts(&AuditFilter { status: vec![StatusRef::Ok], model: None, ... })`                                                                 | 1000-receipt store                                                                                                                 | `Ok(filtered_by_status_ok)`                                                                                                                                          | Server-side filter (using `StatusRef` alias per §6.6; RFC-0014 prerequisite)                                                                                                                                                                      |
| TV-AUD-3-status-multi      | `list_receipts(&AuditFilter { status: vec![StatusRef::Ok, StatusRef::Partial], ... })`                                                          | 1000-receipt store                                                                                                                 | `Ok(filtered_by_status_ok_or_partial)` (UNION semantics)                                                                                                             | Multi-valued status filter per §6.6                                                                                                                                                                                                               |
| TV-AUD-3-subject           | `list_receipts(&AuditFilter { subject_did: Some(<canonical-did>), ... })`                                                                       | 1000-receipt store                                                                                                                 | `Ok(filtered_by_subject_did)`                                                                                                                                        | Subject ACL per §6.6                                                                                                                                                                                                                              |
| TV-AUD-4                   | `list_receipts(&AuditFilter { limit: Some(0) })`                                                                                                | any store                                                                                                                          | `Err(OctoCliError::InvalidFilter("filter: limit out of range".into()))` (CLI exit 16)                                                                                | limit=0 rejection rule per RFC-0016 §6.2.4                                                                                                                                                                                                        |
| TV-AUD-4b                  | `list_receipts(&AuditFilter { since_unix: Some(100), until_unix: Some(50) })`                                                                   | any store                                                                                                                          | `Err(OctoCliError::InvalidFilter("filter: since_unix > until_unix".into()))` (CLI exit 16)                                                                           | Range-inversion validation per RFC-0016 §6.2.4                                                                                                                                                                                                    |
| TV-AUD-4c                  | `list_receipts(&AuditFilter { limit: Some(20000) })`                                                                                            | 20000-receipt store                                                                                                                | `Err(OctoCliError::InvalidFilter("filter: limit must be 1..=10000".into()))` (CLI exit 16)                                                                           | limit > 10000 rejection rule per RFC-0016 §6.2.4                                                                                                                                                                                                  |
| TV-AUD-4d                  | `list_receipts(&AuditFilter { model: Some("ed25519_hex_64_chars_...") })`                                                                       | 1000-receipt store                                                                                                                 | `Err(OctoCliError::InvalidFilter("<REDACTED>".into()))` (CLI exit 16; CLI-side `redact_substrate_error` entire-payload collapse per §6.8)                            | CLI-side scrubber coverage for ed25519 hex; substrate emits `<redacted-hex>` per §6.9 pattern 1, CLI-side collapses to `<REDACTED>` per §6.8                                                                                                      |
| TV-AUD-4e                  | `list_receipts(&AuditFilter { model: Some("WIF: L1aW4aubDFB7yfras2S3mKxL7g2Kz8mN5pQ9rS3tU7vW2xY") })`                                           | 1000-receipt store                                                                                                                 | `Err(OctoCliError::InvalidFilter("<REDACTED>".into()))` (CLI exit 16; CLI-side `redact_substrate_error` entire-payload collapse per §6.8)                            | End-to-end coverage: substrate-side scrub catches WIF pattern per §6.9 pattern 17; CLI-side collapses to `<REDACTED>` per §6.8                                                                                                                    |
| TV-AUD-5                   | `get_receipt(&unknown_id)`                                                                                                                      | Unknown `Receipt::receipt_id: u64`                                                                                                 | `Err(OctoCliError::ReceiptNotFound("<decimal-u64>".into()))` (CLI exit 17)                                                                                           | Canonical decimal `u64` form; CLI-shape variant per §6.7                                                                                                                                                                                          |
| TV-AUD-5-receiptid         | `get_receipt(&ReceiptId(unknown))`                                                                                                              | Unknown digest mapped via `receipt_id_for_digest`                                                                                  | `Err(OctoCliError::ReceiptNotFound("<decimal-u64>".into()))` (CLI exit 17)                                                                                           | `ReceiptId` newtype + reverse-mapping per §6.4                                                                                                                                                                                                    |
| TV-AUD-7                   | `append_audit_event(&mut sink, AgentTransition row)`                                                                                            | Valid event                                                                                                                        | `Ok(ChainHash(<32-bytes>))` + chain-row inserted                                                                                                                     | Happy path; signature takes `&mut dyn AppendOnlyAuditSink` (single-writer lock per §6.11)                                                                                                                                                         |
| TV-AUD-7-canonical-bytes   | `append_audit_event(&mut sink, AgentTransition row)` with caller-supplied `event.chain_hash` differing from `compute_chain_hash(&event)`        | Caller-supplied `event.chain_hash` is wrong (caller bug or forgery attempt); canonical chain-hash via `compute_chain_hash(&event)` | `Err(octo_audit::AuditError::ChainHashMismatch { event_id })` (Layer B façade short-circuit BEFORE sink call per §6.10)                                              | Canonical-bytes-on-write invariant at Layer B façade; substrate form per §6.10                                                                                                                                                                    |
| TV-AUD-7-read-stall        | Concurrent: writer calls `append_audit_event(&mut sink, ...)`; reader calls `list_receipts(&filter)` from another thread                        | Writer holds sink write-side; reader attempts walk on separate `RECEIPT_REGISTRY` Mutex                                            | Reader stalls (blocks) until DOMAIN-adapter paired-acceptance round validates end-to-end read-stall                                                                  | Read-stall invariant per §6.11 DOMAIN-adapter paired-acceptance gate (UNVERIFIABLE at façade layer; substrate `audit_event_v2.rs` §6.11 module-level docstring)                                                                                   |
| TV-AUD-8                   | **DEFERRED — Phase 2 substrate-DESIRED.** `append_audit_event(&mut sink, AgentTransition row)` with `at_millis_unix: 0` outside monotonic range | Non-monotonic timestamp                                                                                                            | `Err(OctoCliError::AuditSubstrateNotReady("non-monotonic timestamp".into()))` (CLI exit 52) — DESIRED, not enforced                                                  | Substrate trait `AppendOnlyAuditSink::append` does NOT currently validate monotonic `at_millis_unix`; sink trusts caller's `prev_chain_hash` chain-link. Phase-2 substrate addition to validate monotonic timestamp at the substrate layer        |
| TV-AUD-11                  | `list_receipts(&AuditFilter::default())` with upstream error containing 32-byte hex private-key-shape string                                    | Substrate error string contains 64-char hex matching `ed25519_private_key` shape                                                   | `Err(octo_audit::AuditError::SinkSpecific("key: <redacted-hex>".into()))` (substrate-side scrubbed)                                                                  | Redaction contract per §6.9 pattern 1                                                                                                                                                                                                             |
| TV-AUD-11a                 | `Internal("BLS12-381 Fr scalar hex: 0x4f6c8b2a...")`                                                                                            | BLS12-381 Fr scalar hex                                                                                                            | `Err(octo_audit::AuditError::SinkSpecific("BLS12-381 Fr scalar hex: <redacted-hex>".into()))`                                                                        | Substrate-side scrub per §6.9 pattern 1 (hex ≥32 catch)                                                                                                                                                                                           |
| TV-AUD-11b                 | `Internal("secp256k1 privkey hex: 0xa1b2c3d4...")`                                                                                              | secp256k1 privkey hex                                                                                                              | `Err(octo_audit::AuditError::SinkSpecific("secp256k1 privkey hex: <redacted-hex>".into()))`                                                                          | Substrate-side scrub per §6.9 pattern 1 (hex ≥32 catch)                                                                                                                                                                                           |
| TV-AUD-11c                 | `Internal("BIP39 mnemonic: abandon ...")`                                                                                                       | 12-word BIP39 mnemonic                                                                                                             | `Err(octo_audit::AuditError::SinkSpecific("BIP39 mnemonic: <redacted-mnemonic>".into()))`                                                                            | Substrate-side scrub per §6.9 pattern 15                                                                                                                                                                                                          |
| TV-AUD-11d                 | `Internal("JWT: eyJ...")`                                                                                                                       | JWT three-segment form                                                                                                             | `Err(octo_audit::AuditError::SinkSpecific("JWT: <redacted-jwt>".into()))`                                                                                            | Substrate-side scrub per §6.9 pattern 16                                                                                                                                                                                                          |
| TV-AUD-11e                 | `Internal("WIF: L1aW4...")`                                                                                                                     | WIF base58                                                                                                                         | `Err(octo_audit::AuditError::SinkSpecific("WIF: <redacted-wif>".into()))`                                                                                            | Substrate-side scrub per §6.9 pattern 17                                                                                                                                                                                                          |
| TV-AUD-11f                 | `Internal("capability-secret base64: ...")`                                                                                                     | Base64 secret                                                                                                                      | `Err(octo_audit::AuditError::SinkSpecific("capability-secret base64: <redacted-secret-b64>".into()))`                                                                | Substrate-side scrub per §6.9 pattern 14                                                                                                                                                                                                          |
| TV-AUD-11h                 | `Internal("PGP private key block: -----BEGIN PGP PRIVATE KEY BLOCK-----\n...")`                                                                 | PGP private key block                                                                                                              | `Err(octo_audit::AuditError::SinkSpecific("PGP private key block: <redacted-pgp-private>".into()))`                                                                  | Substrate-side scrub per §6.9 pattern 11                                                                                                                                                                                                          |
| TV-AUD-11i                 | `Internal("OpenSSH private key: -----BEGIN OPENSSH PRIVATE KEY-----...")`                                                                       | OpenSSH private key                                                                                                                | `Err(octo_audit::AuditError::SinkSpecific("OpenSSH private key: <redacted-openssh-private>".into()))`                                                                | Substrate-side scrub per §6.9 pattern 12                                                                                                                                                                                                          |
| TV-AUD-11j                 | `Internal("PEM block: -----BEGIN RSA PRIVATE KEY-----...")`                                                                                     | PEM private key block                                                                                                              | `Err(octo_audit::AuditError::SinkSpecific("PEM block: <redacted-pem-private>".into()))`                                                                              | Substrate-side scrub per §6.9 pattern 13                                                                                                                                                                                                          |
| TV-AUD-11k                 | `Internal("X.509 cert serial: 0x4f6c8b...")`                                                                                                    | X.509 cert serial `0x`-prefixed hex (16-64 hex chars per §6.9 Pattern 18)                                                          | `Err(octo_audit::AuditError::SinkSpecific("X.509 cert serial: <redacted-x509-serial>".into()))`                                                                      | Substrate-side scrub per §6.9 Pattern 18 (X.509 cert serial)                                                                                                                                                                                      |
| TV-AUD-list-chain-1        | `list_receipts(&AuditFilter::default())` against a 5-row receipt store with row N=2 BLAKE3 chain link corrupted                                 | 5-row store, row N=2 chain_hash does NOT match `compute_chain_hash(row)`                                                           | `Err(octo_audit::AuditError::SinkSpecific("chain verification failed".into()))`                                                                                      | Chain-integrity guard at read boundary                                                                                                                                                                                                            |
| TV-AUD-get-receipt-chain-1 | `get_receipt(&known_id)` against a 5-row receipt store with row N=2 BLAKE3 chain link corrupted                                                 | 5-row store, row N=2 chain_hash does NOT match `compute_chain_hash(row)`; queried row IS corrupted                                 | `Err(octo_audit::AuditError::SinkSpecific("chain verification failed".into()))`                                                                                      | Chain-integrity guard for `get_receipt`                                                                                                                                                                                                           |
| TV-AUD-redact-token-1      | `Internal("key: <REDACTED>")` (literal marker in error string)                                                                                  | substrate error string contains the literal `<REDACTED>` marker                                                                    | `Err(octo_audit::AuditError::SinkSpecific("key: <REDACTED>".into()))` (marker preserved verbatim)                                                                    | Redaction marker idempotency per §6.9 inline guard (no double-scrub; preserves `<REDACTED>` verbatim)                                                                                                                                             |
| TV-AUD-redact-token-2      | `Internal("user-supplied field: <REDACTED>")`                                                                                                   | No plaintext secret present; substrate emits `<REDACTED>` literal                                                                  | `Err(octo_audit::AuditError::SinkSpecific("user-supplied field: <REDACTED>".into()))` (no double-scrub)                                                              | Verifies no accidental double-redaction                                                                                                                                                                                                           |
| TV-AUD-permission-check-1  | `list_receipts(&AuditFilter::default())` when `$OCTO_HOME/audit/receipts` parent dir is NOT mode `0700`                                         | parent dir mode `0755`                                                                                                             | `Err(OctoCliError::PermissionDenied("<REDACTED>".into()))` (CLI exit 13; CLI-side `redact_substrate_error` entire-payload collapse per §6.8)                         | Substrate-enforced per-process trust boundary                                                                                                                                                                                                     |
| TV-AUD-permission-check-2  | **DEFERRED — Phase 2 substrate-DESIRED.** `get_receipt(&known_id)` when parent dir is owned by different UID than process UID                   | parent dir owned by `uid=1000`, process runs as `uid=1001`                                                                         | `Err(OctoCliError::PermissionDenied("<REDACTED>".into()))` (CLI exit 13; CLI-side `redact_substrate_error` entire-payload collapse per §6.8) — DESIRED, not enforced | Substrate DOMAIN adapter does NOT currently enforce UID-ownership check on `get_receipt`; per-process trust boundary only checks `mode 0700` parent-dir perm (TV-AUD-permission-check-1). Phase-2 substrate addition for UID-ownership validation |

CLI-level test vectors live in RFC-0011 §Test Vectors (UNCHANGED at R2; expansion lands at RFC-0011 acceptance with paired RFC-0016).

## Alternatives Considered

- **`SinkSpecific(String)` only at R2 KEEP** — already chosen; no alternative considered
- **CLI-shape error variants as substrate-canonical** — rejected: parallel abstraction per cipherocto-design-principles; substrate remains 3-variant, CLI-shape variants land via RFC-0011 per-variant From conversions
- **`AppendOnlyAuditSink::append` as the public write function** — rejected: type-level `&mut self` requires explicit `append_audit_event(sink: &mut dyn AppendOnlyAuditSink, ...)` façade function for ergonomic substrate-faithful boundary
- **Pre-RFC-0014 `ReceiptSummary` projection without substrate fields** — rejected: would force CLI to backfill field values, violating substrate-faithful principle

## Implementation Phases

- **Phase 0 (RFC-0016 acceptance at R2 KEEP)** — read surface lands; this amendment DEFERRED
- **Phase 1 (RFC-0012 acceptance)** — `AuditEventKind` extensions + single-writer lock on `AppendOnlyAuditSink::append` (substrate trait computes `chain_hash` via the `compute_chain_hash` free function per RFC-0012 §S5; canonical-bytes-on-write invariant lands at Layer B façade per Phase 4)
- **Phase 2 (RFC-0014 acceptance)** — `ReceiptStatus` enum + `Receipt` field extensions + `receipt_id_for_digest` reverse-mapping
- **Phase 3 (RFC-0011 acceptance)** — CLI-shape `[ADD]` error envelope pattern + per-variant From conversions at `octo-cli/src/error.rs`
- **Phase 4 (RFC-0016 acceptance — paired with Phase 1 + 2 + 3)** — `append_audit_event` + `ChainHash` + `ReceiptId` + `ReceiptSummary` + `AuditFilter.subject_did` + `AuditFilter.status` multi-valued + CLI-shape error variants + `redact_substrate_error` helper + substrate-side scrubber patterns land
- **Phase 5 (RFC-0015 acceptance — paired with RFC-0016)** — `octo_wallet::transition_agent` writes `AuditEventKind::AgentTransition` rows via `append_audit_event`

## Key Files to Modify

- `crates/octo-audit/src/lib.rs` — see §6.2 (façade write surface) + §6.8 (`redact_substrate_error` helper) + §6.9 (scrub patterns module)
- `crates/octo-audit/src/scrub.rs` — canonical 18-pattern substrate-side scrubber (pre-existing RFC-0016 substrate)
- `crates/octo-audit-core/src/event.rs` — RFC-0012: `AgentTransition { agent_id: String, from: String, to: String, reason: Option<String> }` (substrate-faithful per the `AuditEventKind` enum declaration; `prev_chain_hash` is on outer `AuditEvent` struct, NOT inside the variant; redaction uses typed-discriminator namespace, NOT a central `Redaction` variant per CLAUDE.md §Extension over enumeration)
- `crates/octo-audit-core/src/chain.rs` — RFC-0012: `pub fn canonical_bytes(event: &AuditEvent) -> Vec<u8>` (canonical byte encoding; Layer A substrate) + `pub fn compute_chain_hash(event: &AuditEvent) -> [u8; 32]` (BLAKE3 over canonical bytes; Layer A substrate). These are FREE FUNCTIONS, NOT trait methods on `AppendOnlyAuditSink`.
- `crates/octo-settlement-core/src/receipt.rs` — RFC-0014: `Receipt` extends with `model: String` + `cost_dqa: u64` + `capability_root: [u8; 32]` + `subject_did: String` + `status: ReceiptStatus` (enum defined in same file at `receipt.rs`; `#[non_exhaustive]` per CLAUDE.md §Extension over enumeration)
- `crates/octo-settlement-core/src/chain.rs` — RFC-0014: `receipt_id_for_digest(digest: &[u8; 32]) -> Option<ReceiptId>` reverse-mapping function (substrate-faithful per the `receipt_id_for_digest` function declaration in the `chain.rs` file; the phantom `id.rs` reference is replaced with the actual file)
- `crates/octo-audit-core/src/sink.rs` — RFC-0012: `AppendOnlyAuditSink::append(&mut self, event: &AuditEvent)` (single-writer type-level via `&mut self`; substrate computes chain_hash via the `compute_chain_hash` free function per RFC-0012 §S5 contract; the canonical-bytes-on-write invariant is enforced at the Layer B façade per §6.10, NOT at this trait)
- `crates/octo-cli/src/error.rs` — RFC-0011: per-variant `#[error(transparent)] From<octo_audit::AuditError>` conversions (Layer B façade form) + CLI-shape variant constructors

**Layer placement table:**

| Crate                  | Layer                     | Substrate anchor                                                                                                                                               | Role at RFC-0016 acceptance                                                             |
| ---------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `octo-audit-core`      | Layer A frozen (RFC-0012) | `AuditEventKind` (extended) + `AppendOnlyAuditSink` (single-writer lock; `&mut self` trust contract) + `canonical_bytes` + `compute_chain_hash` free functions | Canonical substrate write surface (encoding owned by Layer A)                           |
| `octo-settlement-core` | Layer A frozen (RFC-0014) | `Receipt` (extended) + `ReceiptStatus` enum + `ReceiptId` + `receipt_id_for_digest`                                                                            | Canonical substrate projection + ID substrate                                           |
| `octo-audit`           | Layer B façade (RFC-0016) | `append_audit_event` (canonical-bytes-on-write invariant per §6.10) + `ChainHash` + `redact_substrate_error` + scrubber module                                 | Façade write surface (canonical-bytes check + substrate-side scrubber defense-in-depth) |
| `octo-settlement`      | Layer B façade (RFC-0014) | `ReceiptStatus` + `ReceiptId` re-exports                                                                                                                       | Re-exports Layer A frozen extensions                                                    |
| `octo-cli`             | Layer C (RFC-0011)        | `OctoCliError` per-variant From conversions                                                                                                                    | CLI-shape error envelope + exit-code mapping                                            |

Layer direction: `octo-audit` (Layer B) → `octo-settlement` (Layer B) → `octo-settlement-core` (Layer A frozen). `octo-cli` (Layer C) → `octo-audit` (Layer B) → `octo-audit-core` (Layer A). No reverse deps. No C→A direct Cargo edges (octo-cli → octo-audit → octo-audit-core transitive visibility via Layer B façade re-exports).

### §Layer placement

Canonical layer placement of write-path invariants for this amendment:

- **§6.10 canonical-bytes-on-write invariant** — lives at the Layer B façade `append_audit_event` per §6.10; substrate trait `AppendOnlyAuditSink::append` does NOT enforce this property
- **§6.11 read-stalls-while-write invariant** — DOMAIN adapter paired-acceptance gate per §6.11; UNVERIFIABLE at the façade layer
- **§6.7 CLI-shape error variants** — Layer C (`octo-cli::error::OctoCliError`) per §6.7; per-variant `From<AuditError>` conversions at the Layer B → Layer C boundary
- **§6.8 `redact_substrate_error` helper** — Layer B façade (`octo-audit`) per §6.8; CLI boundary invokes this before constructing CLI-shape variants

## Future Work

- **`AuditEventKind::Redaction` variant deferred** — substrate-faithful pattern is typed-discriminator via `cap_root_hash` namespace per CLAUDE.md §Extension over enumeration principle; out of scope for RFC-0016
- **`ReceiptStatus::Unknown` arm already canonical** — substrate declares 4-variant enum with `#[non_exhaustive]`; `StatusRef` type alias passes all arms through verbatim
- **Cursor-streaming `list_receipts`** — for >10000-row stores; reserved for Phase 6
- **`append_audit_event` async variant** — for non-blocking writes; deferred to substrate-faithful async substrate adoption

## Rationale

- **Paired-with-substrate-amendment** — every type/variant requires paired RFC-0012 + RFC-0014 + RFC-0011 acceptance; cannot land in isolation
- **Layer A frozen extensions follow extension-over-enumeration pattern** — `AuditEventKind::AgentTransition` + `ReceiptStatus::{Unknown, Ok, Partial, Reject}` + `Receipt` field additions are additive; no central enum edit
- **Read-stall invariant** — substrate-faithful contract per §6.11 prevents torn reads at canonical substrate boundary
- **Scrubber defense-in-depth** — substrate-side scrubber + CLI-side `OctoCliRedactor` two-pass; covers 18 canonical patterns
- **Split strategy** — write surface paired-with-substrate-amendment moved from RFC-0016 to RFC-0016-a per R29 review plan; halves per-RFC complexity, breaks divergence loop

## Substrate-Faithful Amendment Trail

This section documents per-amendment substrate-faithful sweeps that reconcile RFC text to paired implementation substrate reality. Each amendment entry cites the DRY loop round + reviewer that flagged the drift + the substrate ground-truth identifier (canonical file:section per CLAUDE.md §RFC Reference Conventions Reaffirmed; section refs not line refs in prose).

### v1.1 — Substrate Sweep (2026-09-14)

| #   | Amendment                                                                                                                                                       | Substrate ground truth                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Acceptance criterion                                                                                                                                                                       |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | §6.9 scrubber count 13 → 18 (substrate canonical pattern numbering; restructure user-facing list to mirror substrate `1, 2, 3, 4, 5, 5b, 5c, 5d, 5e, 6, 11-18`) | `octo_audit::scrub` module docstring: "Patterns 1-5e (9 regexes) + Pattern 6 (substring) + Patterns 11-18 (8 additive crypto/secret-form regexes for RFC-0016 §6.9) pre-compiled via `once_cell::sync::Lazy<regex::Regex>`" — corrected via R20.5 R-c HIGH finding (substrate docstring drift: parenthetical "(10 regexes)" → "(9 regexes)" per `octo_audit::scrub` module docstring + `grep -c "Regex::new\|regex!"` = 17 total regex literals matching 9 + 8 patterns). The 18-pattern count holds (9 + 1 substring + 8 = 18). | §6.9 enumerates all 18 substrate patterns with substrate pattern number as primary identifier + user-facing label as secondary; table form with regex literal + substrate line reference   |
| 2   | §6.9 Pattern 18 char range: 64 → 16-64 hex                                                                                                                      | `octo_audit::scrub::RE_X509_SERIAL_HEX` regex `\b0x[A-Fa-f0-9]{16,64}\b` (word-boundary-anchored) per R6.5 reconciliation. Pattern 1 hex ≥32 catch is insufficient because `0x` prefix breaks word-boundary alignment                                                                                                                                                                                                                                                                                                            | §6.9 Pattern 18 row documents 16-64 hex range with R6.5 rationale                                                                                                                          |
| 3   | §6.7 mapping table: add `AuditError::ChainHashMismatch { event_id }` row                                                                                        | `octo_audit_core::error::AuditError::ChainHashMismatch { event_id: u64 }` collapsed variant per R2.5 review; pre-R2.5 carried two variants with raw digest leak surface (`ChainHashMismatch { expected: [u8; 32], got: [u8; 32] }` + `ChainLinkBroken { event_id: u64 }`)                                                                                                                                                                                                                                                        | §6.7 table includes the row mapping substrate → CLI-shape `OctoCliError::Internal(reason)` (CLI exit 64); §6.7 R2.5 note documents the collapse rationale in 2 sentences                   |
| 4   | §Implicit Assumptions: add `#[non_exhaustive]` extension-surface item                                                                                           | `octo_audit_core::error::AuditError enum` + `octo_audit_core::error::AuditChainError enum` + `octo_audit_core::event::AuditEventKind enum` carry `#[non_exhaustive]` per Layer A frozen contract (CLAUDE.md §Extension over enumeration). `octo_cli::error::OctoCliError` carries `#[non_exhaustive]` symmetrically (Layer C consumer-side additive-growth contract; `octo-cli` is Layer C per §Layer placement)                                                                                                                 | §Implicit Assumptions item 6 documents the four-type `#[non_exhaustive]` set; §6.7 substrate column notes the catch-all arm requirement for the per-variant `From<AuditError>` conversions |
| 5   | §Adversary Analysis + §Compatibility #5 + §Key Files + §Rationale: scrubber count 13 → 18 sweep                                                                 | Cross-reference consistency per amendment #1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | All four locations reflect 18-pattern substrate-canonical count                                                                                                                            |

### v1.2 — R2.5 Substrate Sweep (2026-09-14)

| #   | Amendment                                                                                                                                                                           | Substrate ground truth                                                                                                                                                                                                                                                                                                                            | Acceptance criterion                                                                                                                                                                                                                                               |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 6   | §6.8 + §6.1 row + §Implementation Phases + §Key Files + §Compatibility + §Layer placement + §Appendix A + Mermaid: replace phantom `ScrubbedAuditError` / `ScrubbedString` newtypes | `crates/octo-audit/src/scrub_newtypes.rs` `pub fn redact_substrate_error(raw: &str) -> String` — substrate exposes a free function, NOT newtype wrappers. CLI boundary at `octo-cli/src/error.rs` invokes `octo_audit::redact_substrate_error` before constructing `SinkSpecific(String)` per per-variant `From<AuditError>` conversions          | All 8+ locations swept; the `redact_substrate_error` function form is canonical; CLI integration path documented                                                                                                                                                   |
| 7   | §6.9 Pattern 4 regex: `\b[A-Z]{5}\b` → `(?i)\b(?:SQLSTATE_[A-Z0-9]{5}\|errno \d+\|error code \d+)\b`                                                                                | `crates/octo-audit/src/scrub.rs` `RE_SQLSTATE` regex literal `r"(?i)\b(?:SQLSTATE_[A-Z0-9]{5}\|errno \d+\|error code \d+)\b"`. Pre-v1.2 RFC regex would not match any substrate pattern (functional drift, not just cosmetic)                                                                                                                     | §6.9 Pattern 4 row documents the substrate-faithful regex literal with `(?i)` flag                                                                                                                                                                                 |
| 8   | §6.2 + §6.3 + §Appendix A + §Implementation Phases + §Alternatives + §Implicit Assumptions: `append_audit_event` signature gains `dyn` keyword on `&mut AppendOnlyAuditSink`        | `crates/octo-audit/src/audit_event_v2.rs` `pub fn append_audit_event(sink: &mut dyn AppendOnlyAuditSink, event: AuditEvent) -> Result<ChainHash, AuditError>`. Rust 2021 trait-object syntax error otherwise                                                                                                                                      | All `append_audit_event` signature sites use `&mut dyn AppendOnlyAuditSink`                                                                                                                                                                                        |
| 9   | §6.3 ChainHash::Display: replace `hex::encode(self.0)` with zero-allocation per-byte loop                                                                                           | `crates/octo-audit/src/audit_event_v2.rs` `impl fmt::Display for ChainHash { ... for byte in &self.0 { write!(f, "{byte:02x}")?; } ... }`. Substrate avoids pulling in the `hex` crate at the façade boundary                                                                                                                                     | §6.3 displays zero-alloc per-byte loop; matches substrate                                                                                                                                                                                                          |
| 10  | §Test Vectors TV-AUD-7-canonical-bytes expected output: `SinkSpecific("canonical bytes mismatch")` → `ChainHashMismatch { event_id }`                                               | `crates/octo-audit/src/audit_event_v2.rs` `append_audit_event` returns `Err(AuditError::ChainHashMismatch { event_id: event.event_id })` on canonical-byte mismatch per R2.5 collapse                                                                                                                                                             | TV-AUD-7-canonical-bytes expected output reflects collapsed substrate variant                                                                                                                                                                                      |
| 11  | §Test Vectors Notes column: rewrite 10 stale pattern references (TV-AUD-4e, 11a, 11b, 11c, 11d, 11e, 11f, 11h, 11i, 11j) to substrate canonical numbers (§6.9 row mapping)          | Substrate canonical pattern numbering per `octo_audit::scrub` docstring + §6.9 row table: pattern 1 = hex ≥32; 11 = PGP; 12 = OpenSSH; 13 = PEM; 14 = capability-secret base64; 15 = BIP39; 16 = JWT; 17 = WIF; 18 = X.509                                                                                                                        | Each TV-AUD Notes pattern ref cites the substrate-canonical pattern number per §6.9 table                                                                                                                                                                          |
| 12  | §6.7 mapping table: re-order rows to follow substrate enum declaration order                                                                                                        | `crates/octo-audit-core/src/error.rs` `AuditError` declaration order: SequenceGap → AlreadyExists → SinkSpecific → AuditAppendFailed → ReceiptNotFound → InvalidFilter → PermissionDenied → ChainHashMismatch (last collapsed variant per R2.5). Pre-v1.2 RFC ordering placed ChainHashMismatch before SinkSpecific (out of substrate enum order) | §6.7 rows group variants by CLI-shape mapping (row 1: 4 pre-existing substrate variants → `Internal(reason)` exit 64; rows 2-5: 4 RFC-0016 additive substrate variants → one CLI-shape variant each); substrate enum declaration order preserved within each group |
| 13  | §6.6 AuditFilter.limit type: `Option<usize>` → `Option<u32>`                                                                                                                        | `crates/octo-audit/src/receipt_read.rs` `pub limit: Option<u32>` (current substrate ground truth). v1.2 RFC alignment was incorrect (claimed `usize`); v1.3 R19.5 reverses to `u32` per substrate canonical                                                                                                                                       | §6.6 + §Appendix A both declare `pub limit: Option<u32>` per substrate ground truth                                                                                                                                                                                |
| 14  | §Key Files: `crates/octo-settlement-core/src/status.rs` (phantom) → `crates/octo-settlement-core/src/receipt.rs` (actual)                                                           | `crates/octo-settlement-core/src/receipt.rs` `pub enum ReceiptStatus { #[default] Unknown, Ok, Partial, Reject }` at the `ReceiptStatus` declaration (module-internal). Pre-v1.2 RFC claimed a separate `status.rs` file which does not exist                                                                                                     | §Key Files cites actual substrate file path                                                                                                                                                                                                                        |
| 15  | §6.9 Pattern 1 label "(lookaround-anchored)" → "(word-boundary-anchored)"; Pattern 17 label "(51 chars)" → "(50-52 chars, canonical 51)"                                            | Substrate regexes use `\b` (word boundary) not lookahead; WIF regex literal is `\b[1-9A-HJ-NP-Za-km-z]{50,52}\b` (50-52 chars range)                                                                                                                                                                                                              | Pattern 1 + Pattern 17 labels match substrate regex literals                                                                                                                                                                                                       |
| 16  | §Version History: convert bulleted list to 3-column table per BLUEPRINT.md §RFC Process precedent                                                                                   | Per BLUEPRINT.md §RFC Process + RFC-0015 §Version History canonical precedent                                                                                                                                                                                                                                                                     | VH is 3-column `\| Version \| Date \| Changes \|` table with ≤10-word rows                                                                                                                                                                                         |

**Amendment acceptance test (v1.2 cumulative):** every amendment lands at substrate-faithful parity with paired implementation substrate. See per-amendment rows above for ground-truth citations.

### v1.3 — R6.5 Substrate Sweep (2026-09-14)

| #   | Amendment                                                                               | Substrate ground truth                                                                                                                                           | Acceptance criterion                                                                                                       |
| --- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 17  | §6.6 AuditFilter.subject_did: `Option<Did>` → `Option<String>`                          | `crates/octo-audit/src/receipt_read.rs` `AuditFilter::subject_did` field declaration: `pub subject_did: Option<String>` at the `AuditFilter` struct declaration  | §6.1 Status table row + §6.6 struct declaration both declare `Option<String>` (single source per §Single-source principle) |
| 18  | §6.5 ReceiptSummary.subject_did: `Did` → `String`                                       | `crates/octo-audit/src/receipt_summary.rs` `ReceiptSummary::subject_did` field declaration: `pub subject_did: String` at the `ReceiptSummary` struct declaration | §6.5 struct declaration declares `String`; §6.5 from_canonical maps `receipt.subject_did`                                  |
| 19  | §6.1 Status table row for `AuditFilter.subject_did`: attribute to `RFC-0016` (additive) | Per §6.6 additive rationale; `AuditFilter.subject_did` is RFC-0016 additive, NOT an RFC-0014 paired-substrate field                                              | §6.1 Status table row attributes `AuditFilter.subject_did` to RFC-0016 additive                                            |

**Amendment acceptance test (v1.3 cumulative):** every amendment lands at substrate-faithful parity with paired implementation substrate. See per-amendment rows above for ground-truth citations.

### v1.5 — R22.5 Substrate Sweep (2026-09-14)

| #   | Amendment                                                                                                                                        | Substrate ground truth                                                                                                                                                                                                                                                                                                                                                                              | Acceptance criterion                                                                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 20  | §Implicit Assumptions item 6 `#[non_exhaustive]` set extended 4 → 5 enums (add `octo_wallet::error::WalletError` Layer B façade additive-growth) | `crates/octo-wallet/src/error.rs` `pub enum WalletError { ... }` with `#[non_exhaustive]` attribute per `crates/octo-wallet/src/error.rs` (Layer B façade additive-growth symmetric with `OctoCliError` per CLAUDE.md §Extension over enumeration; asymmetric in layer label, symmetric in additive-growth contract). v1.1 row 4 acceptance criterion reflected the historical 4-enum state at v1.1 | §Implicit Assumptions item 6 documents the five-type `#[non_exhaustive]` set: 3 Layer A frozen + `OctoCliError` (Layer C consumer-side) + `WalletError` (Layer B façade additive-growth) |

## Version History

| Version | Date       | Changes          |
| ------- | ---------- | ---------------- |
| v1.3    | 2026-09-14 | subject String.  |
| v1.2    | 2026-09-14 | R2.5 sweep.      |
| v1.1    | 2026-09-14 | Substrate Sweep. |
| v1.0    | 2026-09-11 | Initial draft.   |

## Related RFCs

- RFC-0016 — Audit Receipt API (substrate-faithful read surface; parent RFC for this amendment)
- RFC-0015 — Wallet Agent Operations Substrate (read surface; parent of RFC-0015-a paired write-path amendment)
- RFC-0015-a — Wallet Agent Write-Path Amendment (paired sibling write-path; `transition_agent` consumes `append_audit_event`)
- RFC-0012 — Audit Substrate (provides `AuditEventKind::AgentTransition` + canonical-bytes-on-write invariant + single-writer lock)
- RFC-0014 — Settlement Substrate (provides `ReceiptStatus` enum + `Receipt` field extensions + `ReceiptId` + `receipt_id_for_digest` reverse-mapping)
- RFC-0011 — `octo` CLI Substrate (parent RFC for the `[ADD]` error envelope pattern + CLI-shape error variants via per-variant From conversions)
- RFC-0010 — Canonical DID Codec (DID parsing for `AuditFilter.subject_did`)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping for `append_audit_event` Class B write)

## Memory Anchors

- `cipherocto-design-principles` — Layer model + extension-over-enumeration principle; §6.10 + §6.11 acceptance criteria follow substrate-faithful invariant enforcement

## Related Use Cases

- `docs/use-cases/audit-transparency.md` — receipt read + verify + transition log use case (paired with RFC-0015 `transition_agent`)
- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — runtime attach / run context for transaction logs + audit append

## Appendices

### Appendix A. Substrate function signatures (full Rust surface)

```rust
// crates/octo-audit/src/lib.rs (append to RFC-0016 surface)

// RFC-0016 additions (requires RFC-0012 + RFC-0014 + RFC-0011 paired acceptance):

// §6.2 append_audit_event — write path (Layer B façade 4-step sequence) — see §6.2

// §6.3 ChainHash newtype (canonical accessors) — see §6.3

// §6.4 ReceiptId newtype (paired with RFC-0014 receipt_id_for_digest) — see §6.4

// §6.5 ReceiptSummary projection (paired with RFC-0014 Receipt field extensions) — see §6.5

// §6.6 AuditFilter extension (substrate-canonical 11-field form) — see §6.6

// §6.6 StatusRef type alias (canonical Layer B façade alias) — see §6.6

// §6.8 redact_substrate_error free function (Layer B façade per scrub_newtypes.rs) — see §6.8
```

### Appendix B. Mermaid diagram — write-path flow (RFC-0016)

```mermaid
sequenceDiagram
    participant Wallet as octo-wallet (Layer B)
    participant Aud as octo-audit (Layer B)
    participant Sink as AppendOnlyAuditSink (Layer A)
    participant Core as octo-audit-core (Layer A)
    participant Scrub as octo-audit/scrub.rs (Layer B)

    Wallet->>Aud: append_audit_event(&mut dyn sink, AgentTransition { agent_id, from, to, reason })
    Aud->>Core: compute_chain_hash(&event) — Layer A free function (canonical-bytes-on-write invariant §6.10)
    Core-->>Aud: canonical_chain_hash
    Aud->>Aud: compare canonical vs event.chain_hash; mismatch → ChainHashMismatch short-circuit
    Aud->>Sink: sink.append(&event) — substrate trait owns monotonicity + persistence (RFC-0012 §S5 substrate spec)
    Sink-->>Aud: Ok(())
    Aud-->>Wallet: Ok(ChainHash(canonical))
    Note over Aud: Concurrent readers (list_receipts, get_receipt) stall until DOMAIN-adapter lock release (§6.11 paired-acceptance gate)
```

### Appendix C. Pairing acceptance checklist

// see §Pairing invariant for the canonical pairing rationale + crate-layer anchors.

The five acceptance steps land in canonical order:

1. **RFC-0012 acceptance** — `octo-audit-core::AuditEventKind` extends; `AppendOnlyAuditSink::append` adds single-writer lock (see §6.10 for canonical-bytes-on-write invariant ownership)
2. **RFC-0014 acceptance** — `octo_settlement_core::ReceiptStatus` enum added; `Receipt` field extensions added; `ReceiptId` newtype + `receipt_id_for_digest` reverse-mapping function added
3. **RFC-0011 acceptance** — canonical `[ADD]` error envelope pattern lands at `octo-cli/src/error.rs`; per-variant `#[error(transparent)] From<AuditError>` conversions added; CLI-shape variants `ReceiptNotFound(String)` + `InvalidFilter(String)` + `PermissionDenied(reason)` + `AuditSubstrateNotReady` added
4. **RFC-0016 acceptance** — this amendment; write surface + projection + ACL + scrubber patterns land
5. **RFC-0015 acceptance** (paired sibling) — `octo_wallet::transition_agent` calls `append_audit_event` to write `AuditEventKind::AgentTransition` rows

Pairing invariant: any subset acceptance is a substrate-faithful drift; all five steps must land in order.
