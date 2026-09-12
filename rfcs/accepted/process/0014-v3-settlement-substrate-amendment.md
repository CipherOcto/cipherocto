# RFC-0014-v3 — Settlement Substrate Amendment v3 (paired-acceptance DEFERRED defects)

| Field        | Value                                                                           |
| ------------ | ------------------------------------------------------------------------------- |
| Status       | Accepted                                                                        |
| Version      | v3.0.0                                                                          |
| Layer        | A + B + C (substrate-frozen + façade + specialized-node)                        |
| Authors      | CipherOcto Architecture Working Group                                           |
| Maintainers  | CipherOcto Architecture Working Group                                           |
| Parent RFC   | RFC-0014                                                                        |
| Supersedes   | RFC-0014-v2 §FW6 (canonical scrubber pattern list moves to substrate amendment) |
| Companion    | RFC-0012-v3 (paired acceptance — audit side)                                    |
| Target crate | `octo-settlement-core` v2.1.0 + `octo-settlement` v1.1.0 (semver-minor)         |

## Summary

RFC-0014-v3 is a **Layer A + B + C substrate amendment** to `octo-settlement-core` (Layer A frozen substrate) + `octo-settlement` (Layer B façade) + `quota-router-sm-engine` (Layer C specialized node) that lands the **paired-acceptance DEFERRED** settlement-side substrate defects identified in R48-s review:

1. **§S5.1 — Per-façade scrubber at `octo-settlement` (defect 1b).** Canonical 10-pattern scrubber (Patterns 1-5 + 5b/5c/5d/5e + 6 registry) re-implemented at the `octo-settlement` façade per RFC-0014-v2 §FW6 (canonical scrubber patterns). DOMAIN adapters (e.g. `StoolapStore`, `StoolapReceiptSink`) wrap every raw `.to_string()` chain with `octo_settlement::scrub_adapter_error_with(s, ADAPTER_TYPES)`.
2. **§S5.2 — `SettlementHashOpaque` newtype for `SettlementError::SettlementHashMismatch` + `AskNotFound` + `AlreadyConsumed` (defect 2).** 32-byte hash fields wrapped in `SettlementHashOpaque` newtype whose `Display` impl emits `<redacted-hash>`. Raw bytes retained at `Debug` + `source()` for programmatic chain-integrity verification.
3. **§S5.3 — `SinkSpecific` payload cap posture (defect 3).** Per RFC-0014-v2 §FW6 substrate-faithful posture: substrate DOES NOT enforce a length cap (Layer A frozen, no `debug_assert!` discipline per AC-11). Cap lives at the scrubber (4 KiB input cap, 4 KiB output cap with marker) — DOMAIN adapters MUST call `scrub_adapter_error_with` which enforces both caps. **DEFERRED — lands at acceptance** runtime enforcement of a substrate-level length cap on `SinkSpecific` payload (acceptance mission MAY amend §S5 if 4 KiB scrubber is found insufficient in production).

Per CLAUDE.md §Architectural Principles + §Extension over enumeration, RFC-0014-v3 ships ADDITIVE-only substrate changes; no field removals, no variant additions, no breaking Display semantics (substrate-faithful callers continue to compile).

## Status

**Accepted (2026-09-12)** — codifies the 4 substrate defects surfaced by R48-s review of RFC-0012-v2 + RFC-0014-v2 (defects 1b scrubber, 2 SettlementHashMismatch redacted, 3 SinkSpecific cap-at-scrubber, 4 TimestampRegression redacted cross-RFC) at spec level. Substrate code changes have LANDED in working tree via commits `f33410ce` + `934242ce`. Paired atomic promotion with RFC-0012-v3 per §2-Cycle Atomic Promotion Tag.

## Authors

- CipherOcto Architecture Working Group

## Maintainers

- CipherOcto Architecture Working Group

## Dependencies

- **RFC-0014** — defines `Receipt`, `Reservation`, `Ask`, `AskState`, `ReservationState`, `AppendOnlyReceiptSink`, `SettlementError`, `SettlementStore`.
- **RFC-0014-v2** — pins substrate-frozen extension pattern; this amendment is a paired-acceptance DEFERRED defect fix.
- **RFC-0014-v2 §FW6** — canonical scrubber pattern list (single source of truth for §S5.1).
- **RFC-0012-v3** — paired acceptance (audit-side scrubber + `TimestampOpaque`).

## Design Goals

**G1.** Eliminate DOMAIN-adapter error-chain redaction bypass (defect 1b) by mandating `scrub_adapter_error_with` wrapping on every raw `.to_string()` site.
**G2.** Eliminate 32-byte hash oracle in `SettlementError` (defect 2) by redacting hash fields at `Display` via `SettlementHashOpaque` newtype.
**G3.** Establish canonical posture for `SinkSpecific` payload size (defect 3): cap lives at scrubber, NOT substrate. Substrate-faithful = no cap at substrate boundary; cap enforced at adapter boundary.
**G4.** Preserve programmatic chain-integrity verification access to underlying 32-byte hashes via `Debug` + `source()` + explicit accessor methods (`SettlementHashOpaque::as_bytes`).
**G5.** Stay additive — semver-minor bump (v2.0.0 → v2.1.0), no breaking changes to substrate-faithful callers.

## Motivation

R48-s review of RFC-0012-v2 + RFC-0014-v2 surfaced **4 paired-acceptance DEFERRED substrate defects** that landed in the working tree but were NOT codified at spec level. RFC-0014-v3 fixes the **settlement-side defects** (1b + 2 + 3); RFC-0012-v3 fixes the **audit-side defects** (1a + 4). Paired acceptance required — both amendments must promote simultaneously per §2-Cycle Atomic Promotion Tag.

**Defect 1b (scrubber-bypass on settlement side):** `StoolapStore` + `StoolapReceiptSink` wrap 28 raw `.to_string()` sites — 24 with `scrub_adapter_error_with` (Pattern 6 registry site) plus 4 bare `scrub_adapter_error(` callsites (no-registry delegate for adapter types outside the DOMAIN registry, per §S5.1.1 registry contract). Bypasses the §S5.1 scrubber contract pre-migration.

**Defect 2 (32-byte hash oracle):** The shadow 8-variant `SettlementError` at `quota-router-sm-engine` (Layer C specialized node) leaks 32-byte hashes via `#[error("settlement hash mismatch: expected {expected}, got {got}")]` where `{expected}` and `{got}` are `String` (hex-encoded 32-byte hash). `Display` chain → log → observable side-channel.

**Defect 3 (SinkSpecific payload cap missing):** Per RFC-0014-v2 §FW6, `SinkSpecific(String)` at substrate boundary is UNBOUNDED. The cap lives at the scrubber (4 KiB input cap, 4 KiB output cap). Substrate-faithful posture = verbatim payload retention; runtime enforcement of a substrate-level length cap is **DEFERRED — lands at acceptance** per §FW3.

## Roles and Authorities

| Role                                        | Authority                                                                                                                                                             |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `octo-settlement-core` (substrate, Layer A) | Owns `SettlementError`, `SettlementHashOpaque` newtype; `Display` redaction policy lives at substrate                                                                 |
| `octo-settlement` (façade, Layer B)         | Owns `scrub` module (Pattern 1-5 + 5b-5e + 6 registry); re-exports `scrub_adapter_error`, `scrub_adapter_error_with`, `scrub_registry_validate`                       |
| `quota-router-sm-engine` (Layer C)          | Owns shadow 8-variant `SettlementError`; references substrate newtype via `pub use octo_settlement_core::SettlementHashOpaque` (Layer A single source of truth)       |
| DOMAIN adapters (Layer B-faithful)          | MUST call `octo_settlement::scrub_adapter_error_with(s, ADAPTER_TYPES)` on every raw `.to_string()` site; raw `e.to_string()` chains are FORBIDDEN at DOMAIN boundary |

## §S5 — Specification (substrate-faithful field extensions)

Wrapper heading aggregating §S5.1 (per-façade scrubber) + §S5.2 (`SettlementHashOpaque` newtype) + §S5.2.1 (Layer-model rationale) + §S5.3 (`SinkSpecific` payload cap posture) + §S5.1.1 (DOMAIN adapter migration contract). Substrate-faithful additions per CLAUDE.md §Architectural Principles: no field removals, no variant additions.

### §S5.1 — Per-façade scrubber at `octo-settlement`

The canonical scrubber pattern list (Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e, 6) lives at `octo-settlement::scrub` per RFC-0014-v2 §FW6 (single source of truth). Pattern specs verbatim — see RFC-0012-v3 §S5.1 for full spec (per-façade duplication accepted at v2.0.0 per R34.5 trade-off).

**Cross-RFC consistency:** `octo-settlement::scrub` mirrors `octo-audit::scrub` for the Pattern 1-5 + 5b-5e + 6 logic (single source of truth per RFC-0014-v2 §FW6). Per-façade duplication accepted at v2.0.0 per R34.5 trade-off (cross-RFC shared-utility extraction deferred to v2.1+ per §FW1). Module header doc-block differs per layer context; test surface differs per façade coverage.

### §S5.2 — `SettlementHashOpaque` newtype (defect 2 oracle)

`SettlementHashOpaque` lives at `octo-settlement-core` (Layer A frozen substrate) — it is a substrate-faithful newtype primitive, not a façade extension. Display impl emits `<redacted-hash>`. Raw bytes retained at `Debug` + `source()` + `SettlementHashOpaque::as_bytes` for programmatic chain-integrity verification.

```rust
/// Opaque 32-byte settlement-hash newtype whose `Display` impl emits
/// `<redacted-hash>` (RFC-0014-v3 §S5.2 — paired substrate amendment;
/// defect 2 oracle).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettlementHashOpaque([u8; 32]);

impl SettlementHashOpaque {
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self { Self(bytes) }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}

impl std::fmt::Debug for SettlementHashOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettlementHashOpaque(<redacted-hash>)")
    }
}

impl std::fmt::Display for SettlementHashOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted-hash>")
    }
}

impl std::error::Error for SettlementHashOpaque {}
```

The shadow 8-variant `SettlementError` at `quota-router-sm-engine` (Layer C) becomes:

```rust
#[error("settlement hash mismatch: <redacted-hash>")]
SettlementHashMismatch {
    expected: SettlementHashOpaque,
    got: SettlementHashOpaque,
},

#[error("ask not found: <redacted-hash>")]
AskNotFound(#[source] SettlementHashOpaque),

#[error("receipt already consumed: <redacted-hash>")]
AlreadyConsumed(#[source] SettlementHashOpaque),
```

The shadow `SettlementError` references the substrate newtype via `pub use octo_settlement_core::SettlementHashOpaque;` at `quota-router-sm-engine` — a single canonical Layer A primitive consumed across layers, not a Layer C extension.

**Cross-RFC consistency note:** `octo-settlement-core::SettlementError` (Layer A substrate) uses `[u8; 32]` directly for `AskNotFound` + `AlreadyConsumed` variants. The `Debug` impl on `[u8; 32]` leaks the raw bytes — this is a SUBSTRATE-LEVEL gap that the §S5.2 newtype addresses for the SHADOW variants at `quota-router-sm-engine` (Layer C). Substrate-level `[u8; 32]` Debug redaction is **DEFERRED — lands at acceptance** per §FW2.

### §S5.2.1 — Layer-model rationale

- `SettlementHashOpaque` is a newtype for redaction (mirror of `TimestampOpaque` in RFC-0012-v3 §S6.2).
- Newtype primitives with Display/Debug redaction are Layer A substrate concerns (substrate-faithful, no façade extension required).
- Layer B (shadow 8-variant `SettlementError`) and Layer C (façade) reference the substrate newtype via `pub use` re-export — the substrate newtype is the single canonical redaction primitive.
- This is a forward-pointer to the RFC-0012-v3 §S6.2 mirror pattern (paired acceptance: `TimestampOpaque` + `SettlementHashOpaque` both at Layer A).

### §S5.3 — `SinkSpecific` payload cap posture (defect 3)

Per RFC-0014-v2 §FW6 substrate-faithful posture:

**Substrate boundary:** `SettlementError::SinkSpecific(String)` remains UNBOUNDED pre-acceptance. No `debug_assert!(s.len() <= N)` at substrate (per AC-11 no-`debug_assert!` discipline + §S5 explicit substrate-faithful form).

**Adapter boundary:** DOMAIN adapters MUST call `octo_settlement::scrub_adapter_error_with(s, ADAPTER_TYPES)` which enforces 4 KiB input cap + 4 KiB output cap. Oversize input returns `<redacted-too-long>` single token, no payload retained.

**DEFERRED — lands at acceptance** runtime enforcement of a substrate-level length cap on `SinkSpecific` payload. Acceptance mission MAY amend §S5 to declare a substrate-side byte cap if 4 KiB scrubber is found insufficient in production.

### §S5.1.1 — DOMAIN adapter migration contract

DOMAIN (Layer B-faithful) storage adapters MUST migrate every raw `.to_string()` site to `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)`. Migration is enforced via clippy lint (deferred to v3.x) and reviewed at acceptance.

Adapter-type registry (`const ADAPTER_TYPES: &[&str]`) MUST be declared at adapter module scope:

```rust
const ADAPTER_TYPES: &[&str] = &["StoolapStore", "StoolapReceiptSink"];
```

## 2-Cycle Atomic Promotion Tag

**Paired with:** RFC-0012-v3 (audit side — paired acceptance).

**Reviewer board:** CipherOcto Architecture Working Group (CAWG).

**Atomic promotion gate:** Neither amendment may be Accepted without the other; both must reach `Accept` state in the same review cycle. This is a 2-cycle atomic promotion gate per BLUEPRINT.md §2-Cycle Atomic Promotion Tag.

**Acceptance review coordination:** Cross-reviewers (one reviewer holding pen on each side) must clear BOTH amendments before either is promoted. Single-reviewer sign-off on one amendment is INSUFFICIENT for that amendment's promotion — the paired amendment must have the matching sign-off first. If either amendment is rejected at any review round, the other amendment is automatically held at the same state until the rejection is resolved.

**Why paired acceptance:** The 4 substrate defects (1a, 1b, 2, 3, 4) split across the audit + settlement sides form a single design intent — DOMAIN-adapter redaction + Layer A substrate oracle elimination. Promoting one without the other leaves the substrate-faithful posture asymmetric (one side has `TimestampOpaque`/`SettlementHashOpaque` newtypes at Layer A; the other does not) and forces a follow-on amendment to land the missing side.

## Security Considerations

**SC-1.** `SettlementHashOpaque` Display redaction eliminates the 32-byte hash oracle. An attacker observing `SettlementError::SettlementHashMismatch { expected: ..., got: ... }` cannot recover the raw 32-byte hashes from `to_string()`, `format!("{}", err)`, or `anyhow!("{}", err)` chains.

**SC-2.** Per-façade scrubber eliminates hex/path/SQLSTATE/io-error chain leakage via `.to_string()`. All DOMAIN adapters wrap with `scrub_adapter_error_with`.

**SC-3.** `Debug` intentionally ALSO redacts `SettlementHashOpaque` (symmetric with `Display`) so `dbg!()` / `unwrap_or_else(|e| panic!("{:?}", e))` paths cannot leak via accidental Debug formatting either.

**SC-4.** Substrate-level `[u8; 32]` Debug redaction is DEFERRED (per §S5.2 cross-RFC consistency note). Programmatic access via `SettlementHashOpaque::as_bytes()` is the canonical redaction-bypass for legitimate callers.

## Adversarial Review

**A-1.** Side-channel via `as_bytes()`: an attacker with code execution can call `as_bytes()`. Mitigation: the function name signals "programmatic only; never log"; doc-comment explicitly forbids logging.

**A-2.** Side-channel via substrate `Debug` on `[u8; 32]`: substrate-level `[u8; 32]` Debug leaks raw bytes. Mitigation: DEFERRED to §FW2 acceptance; shadow variants at Layer C use `SettlementHashOpaque` to bypass this gap.

**A-3.** Side-channel via `chain_hash` lookup: an attacker who can compute `compute_receipt_hash(&receipt)` can recover `settlement_hash` (since `chain_hash` derives from `settlement_hash`). This is OUT OF SCOPE — redaction prevents accidental leakage via Display, not malicious access to cryptographic substrate.

## Adversary Analysis (5-Question Test)

Per BLUEPRINT.md §Adversary Analysis, the following adversary profiles are evaluated against the 5-question test for the substrate defects addressed by this amendment.

### AA-1 — Display-log scraping adversary (defects 1b + 3)

**1. What does the adversary want?** Recover hex digests, paths, SQLSTATE codes, io error numbers, IPv4 literals, UUIDs, and adapter-type names from log streams or observability feeds that scrape `Display` output of `SettlementError` variants.

**2. How does the adversary observe the system?** Read access to logs (Loki, CloudWatch, journald) or to any system that captures and serializes error chains via `to_string()`, `format!("{}", err)`, `anyhow!("{}", err)`, or `panic!("{:?}", err)` paths.

**3. What capabilities does the adversary have?** Read-only access to log streams. No code execution. Cannot call `as_bytes()` or `as_millis_unix()` (those require code execution — see A-1 in §Adversarial Review).

**4. What is the adversary's cost model?** Zero marginal cost per observed error — logs are passive. Cost is bounded by log retention (typically 30-90 days).

**5. What is the adversary's probability of success?** Pre-fix: HIGH (raw `e.to_string()` leaks Pattern 1-5 + 5b-5e substrings directly). Post-fix: NEGLIGIBLE — Patterns 1-5 + 5b-5e + 6 are scrubbed; `<redacted-*>` sentinels replace sensitive substrings. Residual risk: a log site accidentally bypasses `scrub_adapter_error_with` and uses raw `.to_string()`. Mitigation: §FW4 clippy lint (deferred v3.x) + adapter-boundary review.

### AA-2 — Chain-tampering adversary (defects 2 + 3)

**1. What does the adversary want?** Recover 32-byte settlement hashes from `SettlementError::SettlementHashMismatch { expected, got }` to identify target receipts for replay or double-spend.

**2. How does the adversary observe the system?** Read access to error logs that capture the `Display` chain when a `SettlementHashMismatch` triggers during consensus verification.

**3. What capabilities does the adversary have?** Read-only log access. Cannot call `as_bytes()` (code-execution-required accessor).

**4. What is the adversary's cost model?** Zero marginal cost per observed mismatch. Cost bounded by log retention.

**5. What is the adversary's probability of success?** Pre-fix: HIGH — `#[error("settlement hash mismatch: expected {expected}, got {got}")]` emits hex-encoded 32-byte hash strings directly. Post-fix: NEGLIGIBLE — `SettlementHashOpaque::Display` emits `<redacted-hash>`; `Debug` ALSO redacts (symmetric). `as_bytes()` requires code execution. Residual risk: substrate-level `octo-settlement-core::SettlementError` variants (`AskNotFound`, `AlreadyConsumed`) still use raw `[u8; 32]` for `Debug` — see §FW2 substrate-level `[u8; 32]` Debug redaction (**DEFERRED — lands at acceptance**).

## Economic Analysis

No economic impact. Substrate redaction is a defense-in-depth measure; no on-chain state changes.

## Compatibility

**C-1.** `SettlementHashOpaque` is an additive newtype at `octo-settlement-core` (Layer A frozen substrate). `quota-router-sm-engine` (Layer C) re-exports via `pub use` for shadow variant field types. Substrate `octo-settlement-core::SettlementError` (Layer A) continues to use `[u8; 32]` directly — semver-minor bump (v2.0.0 → v2.1.0). Existing callers using shadow `SettlementError::SettlementHashMismatch { expected: String, got: String }` MUST update to wrap with `SettlementHashOpaque::new(...)`.

**C-2.** Per-façade scrubber is purely additive. Existing callers that build `.to_string()` chains continue to work; they are now SUBOPTIMAL but not broken. Migration to `scrub_adapter_error_with` is recommended but not required at compile time.

**C-3.** Adapter-type registry `ADAPTER_TYPES` MUST be declared. Empty registry + non-empty input panics at runtime — guard against silent bypass.

## Test Vectors

### TV-SET-v3-1: `SettlementHashOpaque` Display redacts

```text
input: SettlementHashOpaque::new([0xab; 32])
expect: format!("{}", op) == "<redacted-hash>"
        op.as_bytes() == &[0xab; 32] (programmatic access preserved)
```

### TV-SET-v3-2: `SettlementError::SettlementHashMismatch` Display

```text
input: SettlementHashMismatch { expected: SettlementHashOpaque::new([0x01; 32]), got: SettlementHashOpaque::new([0x02; 32]) }
expect: format!("{}", err) == "settlement hash mismatch: <redacted-hash>"
        no raw hex bytes leaked
```

### TV-SET-v3-3: scrubber Pattern 1 (hex digest)

```text
input: scrub_adapter_error_with("blake3 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", &["StoolapStore"])
expect: output contains "<redacted-hex>"
        no hex substring retained
```

### TV-SET-v3-4: scrubber Pattern 5d (IPv4)

```text
input: scrub_adapter_error_with("connect 192.168.1.42:5432 failed", &["StoolapStore"])
expect: output contains "<redacted-ipv4>"
```

### TV-SET-v3-5: scrubber input cap

```text
input: 5 KiB string
expect: scrub_adapter_error_with returns "<redacted-too-long>"
        no payload retained
```

### TV-SET-v3-6: scrubber Pattern 6 (adapter-type)

```text
input: scrub_adapter_error_with("StoolapStore open_in_memory failed", &["StoolapStore", "StoolapReceiptSink"])
expect: output contains "<redacted-adapter>"
        "StoolapStore" not present in output
```

### TV-SET-v3-7: DOMAIN adapter wraps error chain

```text
input: StoolapStore::open_in_memory() against a malformed path
expect: StorageError::Stoolap variant
        message contains scrubber sentinel(s) where applicable
        raw stoolap error string NOT present (or scrubbed)
```

### TV-SET-v3-8: SinkSpecific unbounded at substrate

```text
input: SettlementError::SinkSpecific("x".repeat(10_000))
expect: Display emits "sink-specific error: <10K x chars>"
        NO substrate-level cap enforced
        scrubber-side cap would emit "<redacted-too-long>" if adapter wraps
```

### TV-SET-v3-9: scrubber Pattern 2 (absolute paths)

```text
input: scrub_adapter_error_with("open /var/lib/db/stoolap/asks failed", &["StoolapStore"])
expect: output contains "<redacted-path>"
        no "/var/lib" substring retained
```

### TV-SET-v3-10: scrubber Pattern 3 (table-name refs)

```text
input: scrub_adapter_error_with("no such table: asks", &["StoolapStore"])
expect: output contains "<redacted-table>"
        "asks" not present in output
```

### TV-SET-v3-11: scrubber Pattern 4 (SQLSTATE prefixes)

```text
input: scrub_adapter_error_with("SQLSTATE_23000 unique violation", &["StoolapStore"])
expect: output contains "<redacted-sql-state>"
```

### TV-SET-v3-12: scrubber Pattern 5 (io error chains)

```text
input: scrub_adapter_error_with("os error 2 (no such file or directory)", &["StoolapStore"])
expect: output contains "<redacted-io>"
```

### TV-SET-v3-13: scrubber Pattern 5b (URL credentials)

```text
input: scrub_adapter_error_with("connect postgres://user:secret@db.example.com/asks failed", &["StoolapStore"])
expect: output contains "<redacted-creds>"
        "secret" not present in output
```

### TV-SET-v3-14: scrubber Pattern 5c (ANSI-CSI escape sequences)

```text
input: scrub_adapter_error_with("color \x1b[31mred\x1b[0m end", &["StoolapStore"])
expect: output contains "<redacted-ansi>"
```

### TV-SET-v3-15: scrubber Pattern 5e (UUIDs)

```text
input: scrub_adapter_error_with("tx 550e8400-e29b-41d4-a716-446655440000 aborted", &["StoolapStore"])
expect: output contains "<redacted-uuid>"
        "550e8400-e29b-41d4-a716-446655440000" not present in output
```

### TV-SET-v3-16: scrubber output cap (under-cap input)

```text
input: 4 KiB - 1 byte string of unmatched non-pattern payload (just under MAX_OUTPUT_BYTES output cap; output stays under cap after scrubbing)
expect: scrub_adapter_error_with returns the input (no "<redacted-too-long>" marker)
        payload retained in full
```

### TV-SET-v3-17: scrubber output cap (over-cap input)

```text
input: 4 KiB - 1 byte string containing a 64-char hex substring (Pattern 1 expansion pushes output over MAX_OUTPUT_BYTES)
expect: scrub_adapter_error_with truncates output at MAX_OUTPUT_BYTES
        "<redacted-too-long>" marker appended at truncation boundary
```

### TV-SET-v3-18: empty-registry precondition panic

```text
input: scrub_adapter_error_with("some error message", &[])
expect: PANIC with message containing "empty ADAPTER_TYPES registry"
        (vs scrub_adapter_error_with("", &[]) which returns "" without panic)
```

### TV-SET-v3-19: `SettlementError::SinkSpecific` substrate-faithful vs scrubber wrap (FIX 3 assertive paired)

```text
input (substrate): SettlementError::SinkSpecific("x".repeat(10_000))
expect (substrate): Display emits full 10K chars (substrate-faithful = no cap) ✓

input (DOMAIN wrap): scrub_adapter_error_with(&err.to_string(), &["StoolapStore"])
expect (DOMAIN wrap): emits "<redacted-too-long>" marker ✓
        (input 4 KiB cap triggered; payload not retained)
```

### TV-SET-v3-20: `SettlementError::AskNotFound` Display format

```text
input: AskNotFound(SettlementHashOpaque::new([0xab; 32]))
expect: format!("{}", err) == "ask not found: <redacted-hash>"
        no raw hex bytes leaked at Display
```

### TV-SET-v3-21: `SettlementError::AlreadyConsumed` Display format

```text
input: AlreadyConsumed(SettlementHashOpaque::new([0xcd; 32]))
expect: format!("{}", err) == "receipt already consumed: <redacted-hash>"
        no raw hex bytes leaked at Display
```

### TV-SET-v3-22: `SettlementError::InvalidTransition` Display format

```text
input: InvalidTransition { from: AskState::Consumed, to: AskState::Minted }
expect: format!("{}", err) contains "invalid transition" + state names
        (no redaction needed; state names are not an oracle — chain-state transitions are public by design)
```

### TV-SET-v3-23: `SettlementError::ReservationNotFound` Display format

```text
input: ReservationNotFound(String::from("res-9f3a"))
expect: format!("{}", err) == "reservation not found: res-9f3a"
        (reservation-id is non-PII opaque token; substrate-faithful String field stays raw;
         scrubber Pattern 1-5 + 5b-5e + 6 redacts at DOMAIN adapter boundary if any PII
         resonance surfaces; §S5.2 migration scope explicitly excludes ReservationNotFound
         — settlement-engine shadow retains String per RFC-0014-v3 §S5.2 + R8-S-3 reviewer note)
```

### TV-SET-v3-24: `SettlementError::ReservationExpired` Display format

```text
input: ReservationExpired(String::from("res-9f3a"))
expect: format!("{}", err) contains "reservation expired" + "res-9f3a"
        (substrate-faithful String field stays raw; expired_at is a separate
         non-PII string per R8-S-4 reviewer note; §S5.2 migration scope explicitly
         excludes ReservationExpired — settlement-engine shadow retains String
         + raw expired_at_millis_unix; RFC-0012-v3 §S6.2 TimestampOpaque applies
         to AuditChainError::TimestampRegression only)
```

### TV-SET-v3-25: `SettlementError::InvalidReservationTransition` Display format

```text
input: InvalidReservationTransition { from: ReservationState::Active, to: ReservationState::Settled }
expect: format!("{}", err) contains "invalid reservation transition" + state names
        (no redaction needed; state names are not an oracle)
```

### TV-SET-v3-26: `SettlementError::Storage` Display format

```text
input: Storage { source: Box<dyn std::error::Error + Send + Sync> } wrapping a Stoolap error
expect: format!("{}", err) contains scrubber sentinel(s) where applicable
        raw stoolap error string NOT present (or scrubbed at DOMAIN adapter boundary)
```

## Alternatives Considered

**Alt-A: Substrate-level scrubber (Layer A frozen).** Rejected — Layer A stays free of adapter concerns. Scrubber lives at façade per RFC-0014-v2 §FW6 — Canonical Scrubber Patterns.

**Alt-B: Drop `expected` + `got` entirely from `SettlementHashMismatch`.** Rejected — programmatic chain-integrity verification needs them. Source-retain + Display-redact is the canonical split.

**Alt-C: Substrate-level cap on `SinkSpecific(String)`.** Rejected — substrate-faithful posture per AC-11 + §S5; cap lives at scrubber.

**Alt-D: Use `secrecy::Secret<[u8; 32]>` for `SettlementHashOpaque`.** Rejected — overkill for this redaction posture; `Debug` + `Display` redacting newtype is sufficient per R48-s.

## Implementation Phases

### Phase 1: Substrate code amendment

- ✅ Bump `octo-settlement-core` to v2.1.0
- ✅ Add doc-comment to `SettlementError::SinkSpecific` documenting substrate-faithful unbounded posture + §S5.3 DEFERRED marker
- ✅ (Layer A) Add `SettlementHashOpaque` newtype at `octo-settlement-core` (frozen substrate newtype)
- ✅ (Layer C) Add `pub use octo_settlement_core::SettlementHashOpaque;` at `quota-router-sm-engine`
- ✅ (Layer C) Migrate shadow `SettlementError::SettlementHashMismatch` + `AskNotFound` + `AlreadyConsumed` to use `SettlementHashOpaque`
- ✅ Update `StoolapStore::settle` + `consume` + `get` callsites to wrap with `SettlementHashOpaque::new(...)`

### Phase 2: Façade scrubber landing

- ✅ Add `octo-settlement::scrub` module with 10-pattern canonical scrubber
- ✅ Re-export `scrub_adapter_error`, `scrub_adapter_error_with`, `scrub_registry_validate`
- ✅ Migrate `StoolapStore` 24 raw `.to_string()` sites to `scrub_adapter_error_with` (Pattern 6 registry); 4 additional sites to bare `scrub_adapter_error` delegate (no-registry)
- ✅ Add `scrub` module unit tests (13 tests covering all 10 patterns + caps + empty-registry precondition)

### Phase 3: Acceptance

- Prettier pass
- Cite sweep
- Promote Draft → Accepted

## Key Files to Modify

- `crates/octo-settlement-core/src/error.rs` — `SinkSpecific` doc-comment update (§S5.3) + `SettlementHashOpaque` newtype addition (§S5.2)
- `crates/octo-settlement-core/Cargo.toml` — version bump 2.0.0 → 2.1.0
- `crates/octo-settlement/src/scrub.rs` — NEW (per-façade 10-pattern scrubber)
- `crates/octo-settlement/src/lib.rs` — re-export `scrub` module
- `crates/octo-settlement/Cargo.toml` — `regex` + `once_cell` workspace deps (see Cargo.toml dep rationale comments)
- `crates/quota-router-sm-engine/src/lib.rs` — `pub use octo_settlement_core::SettlementHashOpaque;` re-export + shadow 8-variant `SettlementError` migration
- `crates/quota-router-sm-engine/src/store.rs` — 24 `.to_string()` → `scrub_adapter_error_with` migrations + 4 bare `scrub_adapter_error` delegate sites + `SettlementHashOpaque::new` callsite updates
- `crates/quota-router-sm-engine/Cargo.toml` — `octo-settlement` dep (see Cargo.toml dep rationale comments)

## Future Work

### §FW1 — Cross-RFC scrubber shared utility

**DEFERRED — lands at acceptance** At v2.1+, extract `octo_audit::scrub` + `octo_settlement::scrub` into `octo-foundation::scrub` (Layer A frozen shared utility). Per-façade duplication accepted at v2.0.0 per R34.5 trade-off; consolidation deferred.

### §FW2 — Substrate-level `[u8; 32]` Debug redaction

**DEFERRED — lands at acceptance** At acceptance, add `impl Debug for [u8; 32]` (or a wrapper newtype) at `octo-settlement-core` to redact raw 32-byte hashes from substrate-level `Debug` formatting. Currently deferred; shadow variants at Layer C use `SettlementHashOpaque` to bypass the gap.

### §FW2a — Substrate `SettlementError::{AskNotFound, AlreadyConsumed}` Display format-string vector (R8-S-1 + R8-S-2 paired-acceptance DEFERRED)

**DEFERRED — lands at acceptance** Substrate format strings `#[error("ask not found: {0:?}")]` (L10) and `#[error("ask {0:?} already consumed")]` (L15) invoke Debug on `[u8; 32]`, so `format!("{}", err)` leaks raw bytes via the Display trait path even though Debug redaction is DEFERRED. Mitigation requires paired-acceptance substrate amendment: migrate `AskNotFound([u8; 32])` + `AlreadyConsumed([u8; 32])` → `SettlementHashOpaque`-wrapped fields + change format strings to `{0}` (uses redacting Display). Will be codified in a future RFC-0014-v3.1 paired-acceptance amendment.

### §FW3 — Substrate-level SinkSpecific cap

**DEFERRED — lands at acceptance** At acceptance, MAY amend §S5 to declare a substrate-side byte cap (e.g. `MAX_SINK_PAYLOAD_BYTES = 4 * 1024`) if 4 KiB scrubber is found insufficient in production. Substrate-faithful = no `debug_assert!`; cap would be a runtime `if s.len() > MAX { s.truncate(MAX); s.push_str(REDACTED_TOO_LONG); }` at the variant's `Display` boundary only.

### §FW4 — Clippy lint for raw `.to_string()` in DOMAIN adapters

**DEFERRED — lands at acceptance** Add clippy lint that flags raw `.to_string()` chains in `crates/*/src/storage/` modules, requiring `scrub_adapter_error_with` wrapping. Deferred to v3.x.

## Rationale

RFC-0014-v3 codifies the paired-acceptance DEFERRED substrate defects from R48-s review. The defects are:

1. **Defect 1b (DOMAIN adapter scrubber-bypass):** raw `.to_string()` chains leak hex digests, paths, SQLSTATE codes, io error numbers, IPv4 literals, UUIDs. Fix: per-façade scrubber + DOMAIN adapter migration contract.
2. **Defect 2 (32-byte hash oracle):** `SettlementError::SettlementHashMismatch` Display leaks 32-byte hashes via hex-encoded `{expected}` + `{got}` strings. Fix: `SettlementHashOpaque` newtype with Display-redact.
3. **Defect 3 (SinkSpecific payload cap):** substrate-faithful posture is verbatim payload retention; cap lives at scrubber. DOMAIN adapters MUST call `scrub_adapter_error_with` which enforces 4 KiB input/output caps.

All three fixes are ADDITIVE (semver-minor), preserve programmatic access via explicit accessor methods, and stay at the right layer (scrubber at Layer B façade, newtype at Layer A frozen substrate for substrate-faithful redaction primitive). Per CLAUDE.md §Architectural Principles, no breaking changes.

## Version History

| Version      | Date       | Author                                | Notes                                                         |
| ------------ | ---------- | ------------------------------------- | ------------------------------------------------------------- |
| v3.0.0       | 2026-09-12 | CipherOcto Architecture Working Group | Promoted Draft → Accepted; paired with RFC-0012-v3            |
| v3.0.0-draft | 2026-09-12 | CipherOcto Architecture Working Group | Initial draft — paired-acceptance DEFERRED defects 1b + 2 + 3 |

## Related RFCs

- **RFC-0012** — defines parent audit substrate; `octo-audit-core` consumer of paired acceptance.
- **RFC-0012-v2** — pins substrate-frozen audit extension pattern; RFC-0014-v2 §FW6 canonical scrubber pattern list referenced by this amendment.
- **RFC-0012-v3** — paired acceptance (audit-side scrubber + `TimestampOpaque` Layer A newtype).
- **RFC-0014** — defines parent settlement substrate; `octo-settlement-core` owns `SettlementError`, `Receipt`, `Reservation`, `Ask`.
- **RFC-0014-v2** — pins substrate-frozen settlement extension pattern; RFC-0014-v2 §FW6 canonical scrubber pattern list (single source of truth for §S5.1).
- **RFC-0855p** — settlement chain-integrity substrate; `compute_receipt_hash` + receipt chain verification referenced in RFC-0855p chain-integrity Appendix.

## Related Use Cases

- **UC-1 — DOMAIN adapter error-chain redaction at audit log emission.** `StoolapStore` + `StoolapReceiptSink` migrate every raw `.to_string()` chain to `scrub_adapter_error_with` (Pattern 6 registry) — eliminates hex/path/SQLSTATE/io-error chain leakage at the DOMAIN adapter boundary (defect 1b).
- **UC-2 — Chain-integrity verification with redacted Display.** `octo-settlement-core::SettlementError::SinkSpecific` retains unbounded payload at substrate (Layer A frozen) per AC-11 + §S5 explicit substrate-faithful posture; DOMAIN adapters MUST call `scrub_adapter_error_with` for 4 KiB input/output cap enforcement (defect 3).
- **UC-3 — SettlementHashMismatch error redacted at Display, raw bytes at source.** Shadow 8-variant `SettlementError::SettlementHashMismatch` + `AskNotFound` + `AlreadyConsumed` migrate `[u8; 32]` → `SettlementHashOpaque` (Layer A frozen substrate newtype); Display emits `<redacted-hash>`, `as_bytes()` retains raw bytes for programmatic chain-integrity verification (defect 2).
- **UC-4 — Substrate-level substrate-faithful posture for unbounded payload.** `SinkSpecific(String)` at substrate remains UNBOUNDED pre-acceptance; cap lives at scrubber (per §FW3 + §S5.3 DEFERRED marker); acceptance mission MAY amend §S5 to declare substrate-side byte cap if 4 KiB scrubber insufficient.

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
- **Pattern 6 (adapter-type names):** per-call registry via `scrub_adapter_error_with(s, ADAPTER_TYPES)`; substring-replace (`String::replace(adapter_type, "<redacted-adapter>")`) — no regex compilation needed. Registry entries are adapter-type names like `StoolapStore`, `StoolapReceiptSink`.

**Caps:** input 4 KiB → `<redacted-too-long>` single token, no payload retained. Output 4 KiB truncates with `<redacted-too-long>` marker on truncation.

**Compilation posture:** Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e pre-compiled via `once_cell::sync::Lazy<regex::Regex>`. Pattern 6 is a substring-replace (registry entries are adapter-type names like `StoolapStore`, `StoolapReceiptSink`); no regex compilation needed.

**Empty-registry precondition:** `scrub_adapter_error_with(s, [])` panics on non-empty `s`. Use `scrub_adapter_error(s)` for no-registry entry point.

### Appendix B — Migration manifest (per-site table)

The following raw `.to_string()` / `&format!(...)` sites in `crates/quota-router-sm-engine/src/store.rs` were migrated to scrub-wrapped callsites. Counts verified by `grep -cE "scrub_adapter_error_with|scrub_adapter_error\("` against the working tree (2026-09-12 R1.5 hard check):

| Migration class                                               | Callsites | Wrapper                                                   | Pattern 6 registry |
| ------------------------------------------------------------- | --------- | --------------------------------------------------------- | ------------------ |
| `Stoolap(e.to_string())` → scrub-wrapped                      | 14        | `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)` | yes                |
| `Decode(e.to_string())` → scrub-wrapped                       | 10        | `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)` | yes                |
| `Stoolap(e.to_string())` (nested in SettlementError::Storage) | 5         | `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)` | yes                |
| `Decode(e.to_string())` (nested in SettlementError::Storage)  | 7         | `scrub_adapter_error_with(&e.to_string(), ADAPTER_TYPES)` | yes                |
| `&format!(...)` raw error chain → bare delegate               | 3         | `scrub_adapter_error(&format!(...))`                      | no                 |
| Literal string "output_hash wrong length" → bare delegate     | 1         | `scrub_adapter_error("output_hash wrong length")`         | no                 |
| **Total**                                                     | **40**    | —                                                         | —                  |

**Note on totals:** The "callsite counts" include reuse in error-context paths (`e.to_string()` is passed through scrubber then re-formatted in `format!`). The total distinct migration sites is 28 (24 registry + 4 bare delegate) per hard check; the table decomposes by error-class.

**Cargo.toml dependency rationale:** `regex` + `once_cell` workspace deps added per RFC-0014-v2 §FW6 substrate-faithful posture. See Cargo.toml dep rationale comments for layer assignment (Layer B façade, not Layer A substrate).
