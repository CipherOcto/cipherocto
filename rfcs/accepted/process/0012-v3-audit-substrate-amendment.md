# RFC-0012-v3 — Audit Substrate Amendment v3 (paired-acceptance DEFERRED defects)

| Field        | Value                                                                           |
| ------------ | ------------------------------------------------------------------------------- |
| Status       | Accepted                                                                        |
| Version      | v3.0.0                                                                          |
| Layer        | A + B (substrate-frozen + façade)                                               |
| Authors      | CipherOcto Architecture Working Group                                           |
| Maintainers  | CipherOcto Architecture Working Group                                           |
| Parent RFC   | RFC-0012                                                                        |
| Supersedes   | RFC-0012-v2 §FW6 (canonical scrubber pattern list moves to substrate amendment) |
| Companion    | RFC-0014-v3 (paired acceptance — settlement side)                               |
| Target crate | `octo-audit-core` v2.1.0 (additive — semver-minor, NOT semver-major)            |

## Summary

RFC-0012-v3 is a **Layer A + B substrate amendment** to `octo-audit-core` + `octo-audit` (façade) that lands the **paired-acceptance DEFERRED** substrate defects identified in R48-s review of RFC-0012-v2 + RFC-0014-v2 (DRY CLOSED 2026-09-12):

1. **§S5.1 — Per-façade scrubber at `octo-audit` (defect 1a).** Canonical 10-pattern scrubber (Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e, 6) re-implemented at the `octo-audit` façade per RFC-0014-v2 §FW6 (single source of truth). DOMAIN adapters (e.g. `StoolapAuditSink`) wrap every raw `format!("{e}")` chain with `octo_audit::scrub_adapter_error_with(s, ADAPTER_TYPES)`. Per-façade duplication with `octo_settlement::scrub` accepted at v2.0.0 per the R34.5 trade-off (cross-RFC shared-utility extraction deferred to v2.1+).
2. **§S6.2 — `TimestampOpaque` newtype for `AuditChainError::TimestampRegression` (defect 4).** `prev: u64` + `current: u64` fields wrapped in `TimestampOpaque` newtype whose `Display` impl emits `<redacted-timestamp>`. `event_id` retained at Display (NOT a chronological side-channel). Raw u64 values retained at `Debug` + `source()` for programmatic chain-integrity verification.

Per CLAUDE.md §Architectural Principles + §Extension over enumeration, RFC-0012-v3 ships ADDITIVE-only substrate changes; no field removals, no variant additions.

## Status

**Accepted (2026-09-12)** — codifies the **audit-side substrate defects (1a + 4) of the 4 surfaced by joint R48-s review** of RFC-0012-v2 + RFC-0014-v2 (defect 1a scrubber, 4 TimestampRegression redaction; defects 2 redacted hash + 3 cap-at-scrubber are settlement-side RFC-0014-v3 scope, NOT this RFC). Substrate code changes have LANDED in working tree via commits `f33410ce` + `934242ce`. Paired atomic promotion with RFC-0014-v3 per §2-Cycle Atomic Promotion Tag.

## Authors

- CipherOcto Architecture Working Group

## Maintainers

- CipherOcto Architecture Working Group

## Dependencies

- **RFC-0012** — defines `AuditEvent`, `AuditEventKind`, `AppendOnlyAuditSink`, `AuditError`, `AuditChainError`.
- **RFC-0012-v2** — pins substrate-frozen extension pattern; this amendment is a paired-acceptance DEFERRED defect fix.
- **RFC-0014-v2 §FW6** — canonical scrubber pattern list (single source of truth for §S5.1).
- **RFC-0014-v3** — paired acceptance (settlement-side scrubber + `SettlementHashOpaque` + SinkSpecific posture).

## Design Goals

**G1.** Eliminate DOMAIN-adapter error-chain redaction bypass (defect 1a) by mandating `scrub_adapter_error_with` wrapping on every raw `format!("{e}")` site.
**G2.** Eliminate chronological side-channel in `AuditChainError::TimestampRegression` (defect 4) by redacting `prev` + `current` timestamps at `Display`.
**G3.** Preserve programmatic chain-integrity verification access to the underlying u64 / byte values via `Debug` + `source()` + explicit accessor methods (`TimestampOpaque::as_millis_unix`).
**G4.** Stay additive — semver-minor bump (v2.0.0 → v2.1.0), no breaking changes to substrate-faithful callers.

## Motivation

R48-s review of RFC-0012-v2 + RFC-0014-v2 surfaced **4 paired-acceptance DEFERRED substrate defects** that landed in the working tree but were NOT codified at spec level:

1. **Scrubber-bypass** — DOMAIN adapters wrap raw `format!("{e}")` chains (13 sites in `StoolapAuditSink`, 20 sites in `StoolapStore`). Bypasses the §S5.1 scrubber contract.
2. **`SettlementHashMismatch` oracle** — shadow 8-variant `SettlementError` at `quota-router-sm-engine` (Layer C specialized node) leaks 32-byte hashes via Display.
3. **`SinkSpecific` payload cap** — unbounded `String` in error variant (per RFC-0014-v2 §FW6 substrate-faithful posture: cap lives at scrubber, NOT substrate; defect is the absence of scrubber-side cap).
4. **`TimestampRegression` unix-ms leak** — Display exposes chronological side-channel (`prev` + `current` u64).

RFC-0012-v3 fixes **defects 1a + 4** (audit side); RFC-0014-v3 fixes **defects 1b + 2 + 3** (settlement side). Paired acceptance required — both amendments must promote simultaneously per §2-Cycle Atomic Promotion Tag.

## Roles and Authorities

| Role                                   | Authority                                                                                                                                                          |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `octo-audit-core` (substrate, Layer A) | Owns `AuditError`, `AuditChainError`, `TimestampOpaque` newtype; `Display` redaction policy lives at substrate                                                     |
| `octo-audit` (façade, Layer B)         | Owns `scrub` module (Pattern 1-5 + 5b-5e + 6 registry); re-exports `scrub_adapter_error`, `scrub_adapter_error_with`, `scrub_registry_validate`                    |
| DOMAIN adapters (Layer B-faithful)     | MUST call `octo_audit::scrub_adapter_error_with(s, ADAPTER_TYPES)` on every raw `format!("{e}")` site; raw `e.to_string()` chains are FORBIDDEN at DOMAIN boundary |

## §S5 — Specification (substrate-faithful field extensions)

Wrapper heading aggregating §S5.1 (per-façade scrubber at `octo-audit`) + §S5.1.1 (DOMAIN adapter migration contract). Substrate-faithful additions per CLAUDE.md §Architectural Principles: no field removals, no variant additions.

### §S5.1 — Per-façade scrubber at `octo-audit`

The canonical scrubber pattern list (Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e, 6) lives at `octo-audit::scrub` per RFC-0014-v2 §FW6 (single source of truth). Pattern specs verbatim:

- **Pattern 1 (hex digests):** lookaround-anchored `≥32 chars` → `<redacted-hex-N>`.
- **Pattern 2 (absolute paths):** POSIX `/a/b/...` AND Windows `C:\path\to\file` alternation → `<redacted-path>`.
- **Pattern 3 (table-name refs):** PostgreSQL `table 'X'`, `relation "X"` AND Stoolap/SQLite `no such table: X`, `no such column: X` (case-insensitive alternation) → `<redacted-table>`.
- **Pattern 4 (SQLSTATE prefixes):** `SQLSTATE_XXXXX`, `errno N`, `error code N` (case-insensitive) → `<redacted-sql-state>`.
- **Pattern 5 (io error chains):** `(?i)os error \d+` → `<redacted-io>`.
- **Pattern 5b (URL credentials):** `scheme://(user:pass@)?(\[[0-9a-fA-F:.]+\]|[^:@]+)(:[^@]+)?@` → `<redacted-creds>`.
- **Pattern 5c (ANSI-CSI escape sequences):** → `<redacted-ansi>`.
- **Pattern 5d (IPv4 literals):** `(?i)\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?::[0-9]{1,5})?\b` → `<redacted-ipv4>`.
- **Pattern 5e (UUIDs):** `(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b` → `<redacted-uuid>`.
- **Pattern 6 (adapter-type names):** per-call registry via `scrub_adapter_error_with(s, ADAPTER_TYPES)` → `<redacted-adapter>`.

**Caps:** input 4 KiB → `<redacted-too-long>` single token, no payload retained. Output 4 KiB truncates with `<redacted-too-long>` marker on truncation.

**Compilation posture:** Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e pre-compiled via `once_cell::sync::Lazy<regex::Regex>`. Pattern 6 is a substring-replace (registry entries are adapter-type names like `StoolapAuditSink`); no regex compilation needed.

**Empty-registry precondition:** `scrub_adapter_error_with(s, [])` panics on non-empty `s`. Use `scrub_adapter_error(s)` for no-registry entry point.

### §S5.1.1 — DOMAIN adapter migration contract

DOMAIN (Layer B-faithful) storage adapters MUST migrate every raw `format!("{e}")` site to `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)`. Migration is enforced via clippy lint (deferred to v3.x) and reviewed at acceptance.

Adapter-type registry (`const ADAPTER_TYPES: &[&str]`) MUST be declared at adapter module scope:

```rust
const ADAPTER_TYPES: &[&str] = &["StoolapAuditSink"];
```

## §S6 — TimestampOpaque newtype

Wrapper heading for audit-side substrate-faithful newtype spec (parallel to RFC-0014-v3 §S5.2 `SettlementHashOpaque` newtype). Lives in §S6 chapter for historical numbering inheritance from RFC-0012-v2.

### §S6.2 — `TimestampOpaque` newtype (defect 4 oracle)

```rust
/// Opaque unix-millis timestamp newtype whose `Display` impl emits
/// `<redacted-timestamp>` (RFC-0012-v3 §S6.2 — paired substrate
/// amendment; defect 4 oracle).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimestampOpaque(u64);

impl TimestampOpaque {
    #[must_use]
    pub const fn new(millis_unix: u64) -> Self { Self(millis_unix) }

    #[must_use]
    pub const fn as_millis_unix(&self) -> u64 { self.0 }
}

impl std::fmt::Debug for TimestampOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TimestampOpaque(<redacted-timestamp>)")
    }
}

impl std::fmt::Display for TimestampOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted-timestamp>")
    }
}

impl std::error::Error for TimestampOpaque {}
```

The `AuditChainError::TimestampRegression` variant becomes:

```rust
#[error("timestamp regression at event_id {event_id} (<redacted-timestamp>)")]
TimestampRegression {
    event_id: u64,
    prev: TimestampOpaque,
    current: TimestampOpaque,
},
```

**Why `event_id` retained:** `event_id` is monotonic and recoverable from chain-hash lookup. NOT a chronological side-channel.

**Why raw u64 retained at `Debug` + `source()`:** programmatic chain-integrity verification (e.g. `octo-audit::verify_chain`) needs the actual values. Source-retain + Display-redact is the canonical split per R48-s.

**Why single `<redacted-timestamp>` sentinel for both `prev` + `current`:** Distinguishing which field is `prev` vs `current` at the Display level would itself leak ordering information — an attacker observing `AuditChainError::TimestampRegression` could infer which event regressed (the smaller of the two timestamps) by comparing the two emitted values. A single sentinel for both fields eliminates this ordering side-channel. Programmatic callers needing the prev/current distinction use `as_millis_unix()` on each field.

## 2-Cycle Atomic Promotion Tag

**Paired with:** RFC-0014-v3 (settlement side — paired acceptance).

**Reviewer board:** CipherOcto Architecture Working Group (CAWG).

**Atomic promotion gate:** Neither amendment may be Accepted without the other; both must reach `Accept` state in the same review cycle. This is a 2-cycle atomic promotion gate per BLUEPRINT.md §2-Cycle Atomic Promotion Tag.

**Acceptance review coordination:** Cross-reviewers (one reviewer holding pen on each side) must clear BOTH amendments before either is promoted. Single-reviewer sign-off on one amendment is INSUFFICIENT for that amendment's promotion — the paired amendment must have the matching sign-off first. If either amendment is rejected at any review round, the other amendment is automatically held at the same state until the rejection is resolved.

**Why paired acceptance:** The 4 substrate defects (1a, 1b, 2, 3, 4) split across the audit + settlement sides form a single design intent — DOMAIN-adapter redaction + Layer A substrate oracle elimination. Promoting one without the other leaves the substrate-faithful posture asymmetric (one side has `TimestampOpaque`/`SettlementHashOpaque` newtypes at Layer A; the other does not) and forces a follow-on amendment to land the missing side.

## Security Considerations

**SC-1.** `TimestampOpaque` Display redaction eliminates the chronological side-channel. An attacker observing `AuditChainError::TimestampRegression { ... }` cannot recover the actual unix-ms values from `to_string()`, `format!("{}", err)`, or `anyhow!("{}", err)` chains.

**SC-2.** Per-façade scrubber eliminates hex/path/SQLSTATE/io-error chain leakage via `format!("{e}")`. All DOMAIN adapters wrap with `scrub_adapter_error_with`.

**SC-3.** `Debug` intentionally ALSO redacts `TimestampOpaque` (symmetric with `Display`) so `dbg!()` / `unwrap_or_else(|e| panic!("{:?}", e))` paths cannot leak via accidental Debug formatting either.

## Adversarial Review

**A-1.** Side-channel via `as_millis_unix()`: an attacker with code execution can call `as_millis_unix()`. Mitigation: the function name signals "programmatic only; never log"; doc-comment explicitly forbids logging. Source code review at adapter boundary catches misuse.

**A-2.** Side-channel via `chain_hash` lookup: an attacker who can compute `compute_chain_hash(&event)` can recover `at_millis_unix` (since `chain_hash` derives from `at_millis_unix`). This is OUT OF SCOPE — the redaction prevents accidental leakage via Display, not malicious access to cryptographic substrate.

## Adversary Analysis (5-Question Test)

Per BLUEPRINT.md §Adversary Analysis, the following adversary profiles are evaluated against the 5-question test for the substrate defects addressed by this amendment.

### AA-1 — Display-log scraping adversary (defect 1a)

**1. What does the adversary want?** Recover hex digests, paths, SQLSTATE codes, io error numbers, IPv4 literals, UUIDs, and adapter-type names from log streams that scrape `Display` output of `AuditError` + `AuditChainError` variants.

**2. How does the adversary observe the system?** Read access to logs (Loki, CloudWatch, journald) or any system that captures and serializes error chains via `to_string()`, `format!("{}", err)`, `anyhow!("{}", err)`, or `panic!("{:?}", err)` paths.

**3. What capabilities does the adversary have?** Read-only log access. No code execution. Cannot call `as_millis_unix()` (code-execution-required accessor — see A-1 in §Adversarial Review).

**4. What is the adversary's cost model?** Zero marginal cost per observed error — logs are passive. Cost bounded by log retention (typically 30-90 days).

**5. What is the adversary's probability of success?** Pre-fix: HIGH (raw `format!("{e}")` chains leak Pattern 1-5 + 5b-5e substrings directly). Post-fix: NEGLIGIBLE — Patterns 1-5 + 5b-5e + 6 are scrubbed; `<redacted-*>` sentinels replace sensitive substrings. Residual risk: a log site accidentally bypasses `scrub_adapter_error_with` and uses raw `format!("{e}")`. Mitigation: §FW2 clippy lint (deferred v3.x) + adapter-boundary review.

### AA-2 — Chronological-reconstruction adversary (defect 4)

**1. What does the adversary want?** Recover unix-ms timestamps from `AuditChainError::TimestampRegression { prev, current }` to reconstruct the chronological ordering of audit events, enabling correlation with external events (off-chain activity, side-channel timing attacks).

**2. How does the adversary observe the system?** Read access to error logs that capture the `Display` chain when a `TimestampRegression` triggers during chain verification.

**3. What capabilities does the adversary have?** Read-only log access. Cannot call `as_millis_unix()` (code-execution-required accessor).

**4. What is the adversary's cost model?** Zero marginal cost per observed regression event. Cost bounded by log retention + frequency of regression events (rare in production).

**5. What is the adversary's probability of success?** Pre-fix: HIGH — `#[error("timestamp regression at event_id {event_id} (prev: {prev}, current: {current})")]` emits raw u64 unix-ms values directly. Post-fix: NEGLIGIBLE — `TimestampOpaque::Display` emits `<redacted-timestamp>`; `Debug` ALSO redacts (symmetric, per §Security Considerations). `as_millis_unix()` requires code execution. Residual risk: any caller that uses `unwrap_or_else(|e| format!("{:#?}", e))` and bypasses the symmetric Debug redaction via custom Debug impl — currently NONE in substrate; covered by §Security Considerations design decision.

## Economic Analysis

No economic impact. Substrate redaction is a defense-in-depth measure; no on-chain state changes.

## Compatibility

**C-1.** `TimestampOpaque` is an additive newtype — semver-minor bump (v2.0.0 → v2.1.0). Existing callers using `AuditChainError::TimestampRegression { prev: u64, current: u64 }` MUST update to wrap with `TimestampOpaque::new(...)`.

**C-2.** Per-façade scrubber is purely additive. Existing callers that build `format!("{e}")` chains continue to work; they are now SUBOPTIMAL but not broken. Migration to `scrub_adapter_error_with` is recommended but not required at compile time.

**C-3.** Adapter-type registry `ADAPTER_TYPES` MUST be declared. Empty registry + non-empty input panics at runtime — guard against silent bypass.

## Test Vectors

### TV-AUD-v3-1: `TimestampOpaque` Display redacts

```text
input: TimestampOpaque::new(1_700_000_000_000)
expect: format!("{}", op) == "<redacted-timestamp>"
        op.as_millis_unix() == 1_700_000_000_000 (programmatic access preserved)
```

### TV-AUD-v3-2: `AuditChainError::TimestampRegression` Display

```text
input: TimestampRegression { event_id: 42, prev: TimestampOpaque::new(2000), current: TimestampOpaque::new(1000) }
expect: format!("{}", err) == "timestamp regression at event_id 42 (<redacted-timestamp>)"
        no numeric timestamps leaked
```

### TV-AUD-v3-3: scrubber Pattern 1 (hex digest)

```text
input: scrub_adapter_error_with("blake3 chain_hash 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", &["StoolapAuditSink"])
expect: output contains "<redacted-hex>"
        no hex substring retained
```

### TV-AUD-v3-4: scrubber Pattern 5d (IPv4)

```text
input: scrub_adapter_error_with("connect 192.168.1.42:5432 failed", &["StoolapAuditSink"])
expect: output contains "<redacted-ipv4>"
```

### TV-AUD-v3-5: scrubber input cap

```text
input: 5 KiB string
expect: scrub_adapter_error_with returns "<redacted-too-long>"
        no payload retained
```

### TV-AUD-v3-6: scrubber Pattern 6 (adapter-type)

```text
input: scrub_adapter_error_with("StoolapAuditSink open_in_memory failed", &["StoolapAuditSink"])
expect: output contains "<redacted-adapter>"
        "StoolapAuditSink" not present in output
```

### TV-AUD-v3-7: DOMAIN adapter wraps error chain

```text
input: StoolapAuditSink::open_in_memory() against a malformed path
expect: StorageError::Stoolap variant
        message contains scrubber sentinel(s) where applicable
        raw stoolap error string NOT present (or scrubbed)
```

### TV-AUD-v3-8: scrubber Pattern 2 (absolute paths)

```text
input: scrub_adapter_error_with("open /var/lib/db/stoolap/audit failed", &["StoolapAuditSink"])
expect: output contains "<redacted-path>"
        no "/var/lib" substring retained
```

### TV-AUD-v3-9: scrubber Pattern 3 (table-name refs)

```text
input: scrub_adapter_error_with("no such table: audit_events", &["StoolapAuditSink"])
expect: output contains "<redacted-table>"
        "audit_events" not present in output
```

### TV-AUD-v3-10: scrubber Pattern 4 (SQLSTATE prefixes)

```text
input: scrub_adapter_error_with("SQLSTATE_23000 unique violation", &["StoolapAuditSink"])
expect: output contains "<redacted-sql-state>"
```

### TV-AUD-v3-11: scrubber Pattern 5 (io error chains)

```text
input: scrub_adapter_error_with("os error 2 (no such file or directory)", &["StoolapAuditSink"])
expect: output contains "<redacted-io>"
```

### TV-AUD-v3-12: scrubber Pattern 5b (URL credentials)

```text
input: scrub_adapter_error_with("connect postgres://user:secret@db.example.com/audit failed", &["StoolapAuditSink"])
expect: output contains "<redacted-creds>"
        "secret" not present in output
```

### TV-AUD-v3-13: scrubber Pattern 5c (ANSI-CSI escape sequences)

```text
input: scrub_adapter_error_with("color \x1b[31mred\x1b[0m end", &["StoolapAuditSink"])
expect: output contains "<redacted-ansi>"
```

### TV-AUD-v3-14: scrubber Pattern 5e (UUIDs)

```text
input: scrub_adapter_error_with("tx 550e8400-e29b-41d4-a716-446655440000 aborted", &["StoolapAuditSink"])
expect: output contains "<redacted-uuid>"
        "550e8400-e29b-41d4-a716-446655440000" not present in output
```

### TV-AUD-v3-15: scrubber output cap (under-cap input)

```text
input: 4 KiB - 1 byte string (just under MAX_OUTPUT_BYTES input cap; output remains under cap after scrubbing)
expect: scrub_adapter_error_with returns the input (no "<redacted-too-long>" marker)
        payload retained in full
```

### TV-AUD-v3-16: scrubber output cap (over-cap input)

```text
input: 4 KiB - 1 byte string of an unmatched non-pattern payload (output expands via Pattern 1 hex match on a 64-char hex substring that pushes output over MAX_OUTPUT_BYTES)
expect: scrub_adapter_error_with truncates output at MAX_OUTPUT_BYTES
        "<redacted-too-long>" marker appended at truncation boundary
```

### TV-AUD-v3-17: empty-registry precondition panic

```text
input: scrub_adapter_error_with("some error message", &[])
expect: PANIC with message containing "empty ADAPTER_TYPES registry"
        (vs scrub_adapter_error_with("", &[]) which returns "" without panic)
```

### TV-AUD-v3-18: `AuditChainError::SequenceGap` Display format

```text
input: SequenceGap { event_id: 42, prev: 41 }
expect: format!("{}", err) == "sequence gap at event_id 42 (previous was 41)"
        (no redaction needed; event_id + prev are chain-hash recoverable, NOT a chronological side-channel)
```

### TV-AUD-v3-19: `AuditChainError::HashMismatch` Display format

```text
input: HashMismatch { event_id: 42 }
expect: format!("{}", err) == "chain_hash mismatch at event_id 42"
        (no raw chain_hash bytes leaked at Display; mismatch is detectable without oracle)
```

## Alternatives Considered

**Alt-A: Substrate-level scrubber (Layer A frozen).** Rejected — Layer A stays free of adapter concerns. Scrubber lives at façade per RFC-0014-v2 §FW6 — Canonical Scrubber Patterns.

**Alt-B: Drop `prev` + `current` entirely from `TimestampRegression`.** Rejected — programmatic chain-integrity verification needs them. Source-retain + Display-redact is the canonical split.

**Alt-C: Use `secrecy::Secret<u64>` for `TimestampOpaque`.** Rejected — overkill for this redaction posture; `Debug` + `Display` redacting newtype is sufficient per R48-s.

## Implementation Phases

### Phase 1: Substrate code amendment

- ✅ Bump `octo-audit-core` to v2.1.0
- ✅ Add `TimestampOpaque` newtype + `Display` + `Debug` + `Error` impls
- ✅ Migrate `AuditChainError::TimestampRegression` to use `TimestampOpaque`
- ✅ Update `chain.rs` `verify_chain` to wrap with `TimestampOpaque::new(...)`

### Phase 2: Façade scrubber landing

- ✅ Add `octo-audit::scrub` module with 10-pattern canonical scrubber
- ✅ Re-export `scrub_adapter_error`, `scrub_adapter_error_with`, `scrub_registry_validate`
- ✅ Migrate `StoolapAuditSink` 13 raw `format!("{e}")` sites to `scrub_adapter_error_with`
- ✅ Add `scrub` module unit tests (14 tests covering all 10 patterns + caps + empty-registry precondition)

### Phase 3: Acceptance

- Prettier pass
- Cite sweep
- Promote Draft → Accepted

## Key Files to Modify

- `crates/octo-audit-core/src/error.rs` — `TimestampOpaque` newtype + `TimestampRegression` migration
- `crates/octo-audit-core/src/chain.rs` — `verify_chain` callsite update
- `crates/octo-audit-core/Cargo.toml` — version bump 2.0.0 → 2.1.0
- `crates/octo-audit/src/scrub.rs` — NEW (per-façade 10-pattern scrubber)
- `crates/octo-audit/src/lib.rs` — re-export `scrub` module
- `crates/octo-audit/src/storage/stoolap.rs` — 13 `format!("{e}")` → `scrub_adapter_error_with` migrations
- `crates/octo-audit/Cargo.toml` — `regex` + `once_cell` workspace deps (see Cargo.toml dep rationale comments)

## Future Work

### §FW1 — Cross-RFC scrubber shared utility

**DEFERRED — lands at acceptance** At v2.1+, extract `octo_audit::scrub` + `octo_settlement::scrub` into `octo-foundation::scrub` (Layer A frozen shared utility). Per-façade duplication accepted at v2.0.0 per R34.5 trade-off; consolidation deferred.

### §FW2 — Clippy lint for raw `format!("{e}")` in DOMAIN adapters

**DEFERRED — lands at acceptance** Add clippy lint that flags raw `format!("{e}")` chains in `crates/*/src/storage/` modules, requiring `scrub_adapter_error_with` wrapping. Deferred to v3.x.

## Rationale

RFC-0012-v3 codifies the paired-acceptance DEFERRED substrate defects from R48-s review. The defects are:

1. **Defect 1a (DOMAIN adapter scrubber-bypass):** raw `format!("{e}")` chains leak hex digests, paths, SQLSTATE codes, io error numbers, IPv4 literals, UUIDs. Fix: per-façade scrubber + DOMAIN adapter migration contract.
2. **Defect 4 (chronological side-channel):** `TimestampRegression` Display leaks `prev` + `current` u64 values, enabling timing reconstruction. Fix: `TimestampOpaque` newtype with Display-redact.

Both fixes are ADDITIVE (semver-minor), preserve programmatic access via explicit accessor methods, and stay at the right layer (scrubber at Layer B façade, newtype at Layer A substrate). Per CLAUDE.md §Architectural Principles, no breaking changes.

## Version History

| Version      | Date       | Author                                | Notes                                                     |
| ------------ | ---------- | ------------------------------------- | --------------------------------------------------------- |
| v3.0.0       | 2026-09-12 | CipherOcto Architecture Working Group | Promoted Draft → Accepted; paired with RFC-0014-v3        |
| v3.0.0-draft | 2026-09-12 | CipherOcto Architecture Working Group | Initial draft — paired-acceptance DEFERRED defects 1a + 4 |

## Related RFCs

- **RFC-0012** — defines parent audit substrate; `octo-audit-core` owns `AuditError`, `AuditChainError`, `AuditEvent`, `AppendOnlyAuditSink`.
- **RFC-0012-v2** — pins substrate-frozen audit extension pattern; RFC-0014-v2 §FW6 canonical scrubber pattern list referenced by this amendment.
- **RFC-0014** — defines parent settlement substrate; sibling RFC under paired acceptance.
- **RFC-0014-v2** — pins substrate-frozen settlement extension pattern; RFC-0014-v2 §FW6 canonical scrubber pattern list (single source of truth for §S5.1).
- **RFC-0014-v3** — paired acceptance (settlement-side scrubber + `SettlementHashOpaque` Layer A newtype).
- **RFC-0960** — vault substrate; `chain_hash` derivation referenced in RFC-0960 vault-substrate Appendix (OUT OF SCOPE redaction posture per cross-RFC separation of concerns).

## Related Use Cases

- **UC-1 — DOMAIN adapter error-chain redaction at audit log emission.** `StoolapAuditSink` migrates every raw `format!("{e}")` chain to `scrub_adapter_error_with` (Pattern 6 registry) — eliminates hex/path/SQLSTATE/io-error chain leakage at the DOMAIN adapter boundary (defect 1a).
- **UC-2 — Audit chain-integrity verification with redacted timestamp oracle.** `AuditChainError::TimestampRegression` field types migrate `u64` → `TimestampOpaque` (Layer A frozen substrate newtype); Display emits `<redacted-timestamp>`, `as_millis_unix()` retains raw u64 for programmatic chain-integrity verification (defect 4).
- **UC-3 — Symmetric Debug + Display redaction for chain hashes.** `TimestampOpaque::Debug` ALSO emits `<redacted-timestamp>` (symmetric with `Display` per §Security Considerations) so `dbg!()` / `panic!("{:?}", err)` paths cannot leak via accidental Debug formatting.

## Appendices

### Appendix A — Pattern regex specifications (verbatim from RFC-0014-v2 §FW6)

The canonical 10-pattern scrubber pattern list lives at RFC-0014-v2 §FW6 (single source of truth) and is re-implemented per-façade at `octo-audit::scrub` + `octo-settlement::scrub` per R34.5 trade-off (cross-RFC shared-utility extraction deferred to v2.1+ per §FW1):

- **Pattern 1 (hex digests):** lookaround-anchored `≥32 chars` → `<redacted-hex-N>`.
- **Pattern 2 (absolute paths):** POSIX `/a/b/...` AND Windows `C:\path\to\file` alternation → `<redacted-path>`.
- **Pattern 3 (table-name refs):** PostgreSQL `table 'X'`, `relation "X"` AND Stoolap/SQLite `no such table: X`, `no such column: X` (case-insensitive alternation) → `<redacted-table>`.
- **Pattern 4 (SQLSTATE prefixes):** `SQLSTATE_XXXXX`, `errno N`, `error code N` (case-insensitive) → `<redacted-sql-state>`.
- **Pattern 5 (io error chains):** `(?i)os error \d+` → `<redacted-io>`.
- **Pattern 5b (URL credentials):** `scheme://(user:pass@)?(\[[0-9a-fA-F:.]+\]|[^:@]+)(:[^@]+)?@` → `<redacted-creds>`.
- **Pattern 5c (ANSI-CSI escape sequences):** → `<redacted-ansi>`.
- **Pattern 5d (IPv4 literals):** `(?i)\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?::[0-9]{1,5})?\b` → `<redacted-ipv4>`.
- **Pattern 5e (UUIDs):** `(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b` → `<redacted-uuid>`.
- **Pattern 6 (adapter-type names):** per-call registry via `scrub_adapter_error_with(s, ADAPTER_TYPES)`; substring-replace (`String::replace(adapter_type, "<redacted-adapter>")`) — no regex compilation needed. Registry entries are adapter-type names like `StoolapAuditSink`.

**Caps:** input 4 KiB → `<redacted-too-long>` single token, no payload retained. Output 4 KiB truncates with `<redacted-too-long>` marker on truncation.

**Compilation posture:** Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e pre-compiled via `once_cell::sync::Lazy<regex::Regex>`. Pattern 6 is a substring-replace (registry entries are adapter-type names like `StoolapAuditSink`); no regex compilation needed.

**Empty-registry precondition:** `scrub_adapter_error_with(s, [])` panics on non-empty `s`. Use `scrub_adapter_error(s)` for no-registry entry point.

### Appendix B — Migration manifest (per-site table)

The following raw `format!("{e}")` / `e.to_string()` sites in `crates/octo-audit/src/storage/stoolap.rs` were migrated to scrub-wrapped callsites. Counts verified by `grep -cE "scrub_adapter_error_with|scrub_adapter_error\("` audit (audit-side defect 1a; 13 sites per RFC-0014-v2 §FW6 baseline, audit-side mirror count confirmed in R48-s review):

| Migration class                          | Callsites | Wrapper                                                   | Pattern 6 registry |
| ---------------------------------------- | --------- | --------------------------------------------------------- | ------------------ |
| `format!("StoolapAuditSink: {e}")` chain | 13        | `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)` | yes                |
| **Total**                                | **13**    | —                                                         | —                  |

**Cargo.toml dependency rationale:** `regex` + `once_cell` workspace deps added per RFC-0014-v2 §FW6 substrate-faithful posture. See Cargo.toml dep rationale comments for layer assignment (Layer B façade, not Layer A substrate).
