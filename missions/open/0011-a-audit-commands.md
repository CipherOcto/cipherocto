---
name: 0011-a-audit-commands
description: Implement `octo audit {list,show}` per RFC-0011-a — read-only substrate consumers over RFC-0959 settlement receipt store
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-a author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-a
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
status: Open
---

# 0011-a-audit-commands — Implement `octo audit {list,show}` per RFC-0011-a

**Status:** Open — DOC-ONLY amendment (single coherent substrate wiring per RFC-0011-a §Implementation Phases). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] once the parent RFC-0011 lands per RFC-0011-a §Compatibility No `schema_version` bump required (additive per parent §Compatibility).
**Substrate:** RFC-0011-a §Specification (`octo audit` Subcommands)
**Parent:** RFC-0011-a
**Depends on:**

- Mission `0011-core-output-envelope-redaction` (`OutputEnvelope<T>` + `OctoCliRedactor` substrate landed)
- Mission `0011-identity-commands` (`octo_wallet::Did` newtype + RFC-0010 canonical DID codec landed)
- Mission `0011-capability-commands` (`Hex32` newtype + parent §Filters grammar landed)
- Mission `0011-policy-commands` (read-only dispatch + `require_confirm` gate landed)
- RFC-0011-a (canonical amendment text accepted)
- Substrate `[ADD]` surface lands separately (per RFC-0011-a §Key Files to Modify — SUBSTRATE; out of scope for this RFC cycle)

**Blocks:** none

## Status

Open — DOC-ONLY amendment per RFC-0011-a §Implementation Phases; both subcommands land together in a single RFC cycle (atomic per §Implementation Phases). Substrate `[ADD]` signatures (`list_receipts`, `get_receipt`, `AuditFilter`, `AuditError`, `ReceiptId`, `audit_home`) are filed separately per RFC-0011-a §Key Files to Modify — SUBSTRATE and land alongside this amendment via substrate-side RFCs.

## Substrate (RFC-0011-a)

RFC-0011-a §Specification (rfcs/draft/process/0011-a-audit-commands.md).

## Parent

RFC-0011-a — `octo audit` Subcommands amendment to parent RFC-0011 per Status header amendment chain. Inherits every contract from parent RFC-0011: `OutputEnvelope<T>` (§Output Envelope), `OctoCliRedactor` (§Redaction Layer), `OctoCliError` (§Error Handling), exit-code table (§Exit Codes), confirmation flag matrix (§Confirmation Flag Matrix), TTY-aware rendering (§Output Envelope).

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing per parent RFC-0011 §Implementation Phases: mission 1 → 2 → 3 → 4 → 5 (output envelope / identity / capability / policy landed) → RFC-0011-a (audit amendment, this mission). The audit substrate (`octo-audit`) depends on the settlement substrate (`octo-settlement` per RFC-0959) for the `ReceiptRecord` projection per RFC-0011-a §Substrate-truth dependency.

## Acceptance Criteria

- [ ] `commands/audit.rs` created (NEW dispatch module per RFC-0011-a §Key Files to Modify — SUBSTRATE)
- [ ] `Commands::Audit { List, Show }` variant added to clap derive struct in `crates/octo-cli/src/main.rs` (parent §Binary Surface unchanged otherwise)
- [ ] `OctoCliError::ReceiptNotFound`, `AuditReadFailed`, `AuditResponseTooLarge` variants added (exit 17/18/19 per RFC-0011-a §Error Handling; FIRST occupants of parent-reserved range 17-63)
- [ ] `InvalidFilter(String)` variant wired through parent §Error Handling (exit 16; parent reserved "first user" code)
- [ ] CLI-side projection types declared: `AuditFilter`, `ReceiptSummary`, `AuditListOutput`, `AuditShowOutput`, `ReceiptRecord`, `ReceiptStatus` per RFC-0011-a §Output Envelope (declaration order canonical)
- [ ] `octo audit list` implemented with all flags per RFC-0011-a §Filters (`--since`, `--until`, `--capability-root`, `--model`, `--status`, `--limit`, `--json`); default `--limit 100`, max `10000`
- [ ] `octo audit show <receipt_id>` implemented with `--json` flag per RFC-0011-a §Subcommand Taxonomy (`show` Args)
- [ ] `--status` parser is case-insensitive (`OK` / `Ok` / `ok` all normalize to lowercase per RFC-0011-a §Adversarial Review)
- [ ] `parse_status` value_parser returns lowercase token per RFC-0011-a §Appendix A
- [ ] Duration parser (`<n>d|<n>h|<n>m|<n>s`) implemented for `--since` per RFC-0011-a §Filters grammar
- [ ] `OctoCliRedactor` value-pattern sweep applied to `reject_reason` (test vector `audit-show-reject-reason-with-pw-substr` per RFC-0011-a §Redaction)
- [ ] TTY-aware rendering per parent §Output Envelope: pretty-printed table on TTY + no `--json`; JSON on non-TTY or `--json`; `--no-color` disables ANSI; `OCTO_FORCE_JSON` forces JSON
- [ ] Auditor-mode constraint enforced at CLI dispatch (no `--status` reject-hiding filter; `--status` flag silently no-op'd in Auditor mode per RFC-0011-a §Security Considerations row 2)
- [ ] `require_confirm` gate returns success without consulting mode flags for audit commands (read-only commands are unconditional per RFC-0011-a §Confirmation gate matrix)
- [ ] `has_more: true` hint emitted on stdout when `total_matched > returned_count` per RFC-0011-a §Subcommand Taxonomy (`list`) Notes + §Adversarial Review
- [ ] JSON field order matches RFC-0011-a §Output Envelope declaration order (parent §Determinism Requirements)
- [ ] Test vectors implemented per RFC-0011-a §Test Vectors (12 vectors; 8-floor satisfied)
- [ ] CI regression: G1 (no INSERT/UPDATE SQL hit the receipt store during `octo audit {list,show}` invocation per RFC-0011-a §Design Goals)
- [ ] CI regression: G5 (byte-equivalent output across `auditor` / `ci` / `dev` / `human` modes for same canonical fixture per RFC-0011-a §Design Goals)
- [ ] Substrate `[ADD]` surface landed via substrate-side RFC (filed separately per RFC-0011-a §Key Files to Modify — SUBSTRATE; out of scope for this RFC cycle but mission cannot Claim without substrate amendment acceptance)
- [ ] Layer direction verified (octo-audit is Layer C; depends on Layer B octo-settlement; no reverse deps per [[cipherocto-design-principles]])
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy -p octo-cli --all-targets --all-features -- -D warnings` zero warnings
- [ ] `cargo test -p octo-cli --all-features` green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011-a type                                                | Sub-step                      | Notes                                                                                                                |
| -------------------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `OutputEnvelope<AuditListOutput>` (NEW)                        | Sub-step 1 (dispatch module)  | Layer C/D; `schema_version = 2` per parent §Output Envelope (parent-bumped value carrying `preview_only`)            |
| `OutputEnvelope<AuditShowOutput>` (NEW)                        | Sub-step 1 (dispatch module)  | Layer C/D; `preview_only: false` always (no `--dry-run` per RFC-0011-a §Output Envelope)                             |
| `AuditListOutput { receipts, total_matched, has_more }` (NEW)  | Sub-step 1 (projection types) | Layer C/D; `total_matched` is count BEFORE `--limit` truncation per RFC-0011-a §Subcommand Taxonomy (`list`)         |
| `ReceiptSummary` (NEW)                                         | Sub-step 1 (projection types) | Layer C/D; strict subset of `ReceiptRecord` (omits `prompt_hash`, `executed_by`, `reject_reason`) per §Receipt Shape |
| `AuditShowOutput { receipt }` (NEW)                            | Sub-step 1 (projection types) | Layer C/D; full `ReceiptRecord` projection                                                                           |
| `ReceiptRecord` (NEW CLI projection)                           | Sub-step 1 (projection types) | Layer C/D; mirrors RFC-0959 `ReceiptRecord` (substrate-truth) — see RFC-0011-a §Substrate-truth dependency           |
| `ReceiptStatus { Ok, Partial, Reject }` (NEW enum)             | Sub-step 1 (projection types) | Layer C/D; `#[serde(rename_all = "lowercase")]`; NOT a central enum per CLAUDE.md §Architectural Principles          |
| `AuditFilter` (NEW CLI projection)                             | Sub-step 1 (projection types) | Layer C/D; mirrors substrate `[ADD] AuditFilter` per RFC-0011-a §Substrate entry #3                                  |
| `ReceiptId(pub [u8;32])` (NEW newtype)                         | Sub-step 2 (substrate wiring) | Layer C; substrate-side per RFC-0011-a §Substrate entry #5; CLI surfaces `octo_audit::ReceiptId`                     |
| `OctoCliError::InvalidFilter(String)`                          | Sub-step 3 (error envelope)   | Layer C/D; exit 16 (parent reserved "first user" code)                                                               |
| `OctoCliError::ReceiptNotFound(String)` (NEW)                  | Sub-step 3 (error envelope)   | Layer C/D; exit 17 (NEW; first occupant of parent-reserved 17-63 range)                                              |
| `OctoCliError::AuditReadFailed(String)` (NEW)                  | Sub-step 3 (error envelope)   | Layer C/D; exit 18 (NEW)                                                                                             |
| `OctoCliError::AuditResponseTooLarge { matched, limit }` (NEW) | Sub-step 3 (error envelope)   | Layer C/D; exit 19 (NEW)                                                                                             |
| `[ADD] octo_audit::list_receipts(filter)`                      | Sub-step 2 (substrate wiring) | Layer C (substrate-side RFC; OUT OF SCOPE for this RFC cycle per RFC-0011-a §Key Files to Modify — SUBSTRATE)        |
| `[ADD] octo_audit::get_receipt(id)`                            | Sub-step 2 (substrate wiring) | Layer C (substrate-side RFC; OUT OF SCOPE)                                                                           |
| `[ADD] octo_audit::AuditFilter` struct                         | Sub-step 2 (substrate wiring) | Layer C (substrate-side RFC; OUT OF SCOPE)                                                                           |
| `[ADD] octo_audit::AuditError` enum                            | Sub-step 2 (substrate wiring) | Layer C (substrate-side RFC; OUT OF SCOPE)                                                                           |
| `[ADD] octo_audit::ReceiptId` newtype                          | Sub-step 2 (substrate wiring) | Layer C (substrate-side RFC; OUT OF SCOPE)                                                                           |
| `[ADD] octo_audit::audit_home()` discovery helper              | Sub-step 2 (substrate wiring) | Layer C (substrate-side RFC; OUT OF SCOPE; diagnostic / substrate-internal testing only)                             |
| `Commands::Audit { List, Show }` clap variants                 | Sub-step 4 (clap wiring)      | Layer C/D; appends to parent `Commands` enum (no reverse deps per [[cipherocto-design-principles]])                  |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Audit Subcommand Fixtures + §Audit Substrate `[ADD]` Mapping for Rust snippets + clap wiring patterns. The audit substrate depends on `octo-settlement` (Layer B per RFC-0959) for the `ReceiptRecord` projection per RFC-0011-a §Substrate-truth dependency.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Risk

- Substrate `[ADD]` surface (filed separately per RFC-0011-a §Key Files to Modify — SUBSTRATE) MUST land before this mission's Claim phase. If substrate amendment does not land alongside this RFC cycle, `list_receipts` / `get_receipt` calls fail at runtime with `SettlementStore` errors → `AuditReadFailed` exit 18. Mitigation: substrate amendment acceptance blocks mission Claim transition.
- `ReceiptRecord` shape drift between RFC-0959 substrate-truth and CLI projection per RFC-0011-a §Substrate compatibility (if RFC-0959 amends `ReceiptRecord`, the audit amendment amends in lockstep). Mitigation: integration test asserts field-for-field match against canonical 1,000-receipt fixture per §Performance Targets.
- Auditor-mode constraint (no `--status` reject-hiding filter per RFC-0011-a §Security Considerations row 2) requires dispatch-layer enforcement — if accidentally delegated to `require_confirm`, operators could still hide reject rows in Auditor mode. Mitigation: CI test asserts dispatch-layer constraint with explicit mode flag injection.
- Performance target `octo audit list` p95 <100ms (per RFC-0011-a §Performance Targets) requires substrate-level index on `(subject_did, executed_at_unix DESC)`. Mitigation: substrate amendment MUST include the index; benchmark test on canonical 1,000-receipt fixture asserts target.
- Exit code 17/18/19 are FIRST occupants of parent-reserved range 17-63 (per RFC-0011-a §Error Handling). Following amendments (RFC-0011-b/c/d/e/f/g) MUST NOT reuse 17-19. Mitigation: amendment chain Status headers cite the reservation; pre-commit cite validation flags any reuse.
- `--since` duration parsing (`<n>d|<n>h|<n>m|<n>s`) must reject empty unit suffix or whitespace. Mitigation: parser returns `InvalidFilter` (exit 16) on parse failure per RFC-0011-a §Filters
- Redactor value-pattern sweep must catch `password=<value>` substrings in `reject_reason` regardless of field name (per RFC-0011-a §Redaction). Mitigation: test vector `audit-show-reject-reason-with-pw-substr` asserts the sweep end-to-end.
- `total_matched` semantics (count BEFORE `--limit` truncation) must not be conflated with returned receipt count (per RFC-0011-a §Subcommand Taxonomy `list` Notes). Mitigation: integration test asserts `total_matched > returned_count` triggers `has_more: true`.

## Notes

This mission covers both `octo audit list` + `octo audit show` as a single coherent substrate wiring per RFC-0011-a §Implementation Phases (the amendment is ATOMIC — both subcommands land together in a single RFC cycle). The substrate `[ADD]` surface is filed separately per RFC-0011-a §Key Files to Modify — SUBSTRATE; this mission is the CLI-side consumer. RFC-0011-a is the canonical amendment text per parent RFC-0011 Status header amendment chain.

## Scope

Implement two read-only CLI subcommands that consume the canonical
substrate `[ADD]` surface defined by RFC-0011-a:

1. **`octo audit list`** — list settlement receipts matching a typed
   filter set (per RFC-0011-a §Filters). Emits
   `OutputEnvelope<AuditListOutput>` with `Vec<ReceiptSummary>` summaries
   sorted by `executed_at_unix DESC`. Default `--limit 100`, max `10000`.
   `total_matched` carries the count BEFORE `--limit` truncation;
   `has_more: true` signals "narrow the filter and re-invoke" rather than
   cursor-walk (per parent bounded-result-set pattern).

2. **`octo audit show <receipt_id>`** — point lookup by canonical receipt
   ID (32-byte blake3 digest, lowercase hex form). Emits
   `OutputEnvelope<AuditShowOutput>` with the full `ReceiptRecord`
   projection. Dashed-UUID form REJECTED at CLI parser (per parent
   §Binary Surface convention; format violations route through
   `OctoCliError::InvalidFilter` for scripting-consumer-domain-error
   semantics).

Both subcommands are READ-ONLY by construction (per RFC-0011-a §Security
Considerations row 1, G1, §Substrate `[ADD]` surface — every substrate
function returns `Result<_, AuditError>` with no mutation, and the
parent §Confirmation Flag Matrix pattern extends cleanly: read commands
require no `--confirm` / `--allow-write` in any mode). Audit subcommands
do NOT introduce new persistence; every read translates to a substrate
call against `octo_settlement::ReceiptStore` per RFC-0011-a §Substrate-truth
dependency.

Mutating audit operations (`octo audit {redact,export,watch}`) are
explicitly OUT OF SCOPE per RFC-0011-a §Future Work — deferred until
concrete requirements surface.

### Sub-steps

1. **Dispatch module scaffold** — create `crates/octo-cli/src/commands/audit.rs` (NEW) with:
   - `Commands::Audit { action: AuditAction }` clap variant wired into `crates/octo-cli/src/main.rs` (parent §Binary Surface unchanged otherwise)
   - `AuditAction::List(ListArgs)` + `AuditAction::Show(ShowArgs)` per RFC-0011-a §Appendix A
   - `ListArgs` with all filter flags per RFC-0011-a §Filters (`--since`, `--until`, `--capability-root`, `--model`, `--status`, `--limit`, `--json`)
   - `ShowArgs` with positional `receipt_id` + `--json` flag
   - `parse_status` value_parser (case-insensitive, lowercase normalize) per RFC-0011-a §Appendix A
   - Duration parser (`<n>d|<n>h|<n>m|<n>s`) for `--since`
   - CLI-side projection types: `AuditFilter`, `ReceiptSummary`, `AuditListOutput`, `AuditShowOutput`, `ReceiptRecord`, `ReceiptStatus` per RFC-0011-a §Output Envelope (declaration order canonical)

2. **Substrate wiring** — call substrate `[ADD]` surface per RFC-0011-a §Substrate:
   - `octo_audit::list_receipts(&filter)` for `AuditAction::List`
   - `octo_audit::get_receipt(&id)` for `AuditAction::Show`
   - Map substrate errors (`AuditError::ReceiptNotFound` → exit 17, `InvalidFilter` → exit 16, `Internal` → exit 64, `SettlementStore` → exit 64 → `AuditReadFailed` exit 18)
   - Apply `require_confirm` gate (returns success unconditionally for read-only audit commands per RFC-0011-a §Confirmation gate matrix)
   - Enforce Auditor-mode constraint at dispatch layer (no `--status` reject-hiding filter per RFC-0011-a §Security Considerations row 2)

3. **Error envelope** — extend `crates/octo-cli/src/error.rs`:
   - `InvalidFilter(String)` (exit 16; parent reserved "first user" code per RFC-0011-a §Error Handling)
   - `ReceiptNotFound(String)` (exit 17; NEW; first occupant of 17-63 reserved range)
   - `AuditReadFailed(String)` (exit 18; NEW)
   - `AuditResponseTooLarge { matched: usize, limit: usize }` (exit 19; NEW)
   - All variants pass through `sanitize_substrate_error` before display (parent §Error Handling pattern verbatim)

4. **Clap wiring** — register `Audit` subcommand in `crates/octo-cli/src/commands/mod.rs`. Append `Commands::Audit` variant to parent `Commands` enum (existing variants unchanged per RFC-0011-a §Compatibility row 3).

5. **Output envelope rendering** — implement TTY-aware rendering per parent §Output Envelope:
   - TTY + no `--json` → pretty-printed table (`list`) / key-value (`show`); ANSI colors when stdout is TTY AND `--no-color` / `OCTO_FORCE_JSON` is not set; gated by `std::io::IsTerminal`
   - Non-TTY OR `--json` → JSON via `serde_json::to_string_pretty`
   - `--json` forces JSON regardless of TTY
   - `--no-color` disables ANSI
   - `OCTO_FORCE_JSON` forces JSON output

6. **Redaction** — apply `OctoCliRedactor` value-pattern sweep to `reject_reason` (per RFC-0011-a §Redaction). Canonical receipt fields are redactor-clean by construction (blake3 digests, RFC-0010 canonical DIDs, RFC-0959 Dqa wire forms, enum tags, monotonic unix timestamps); no new patterns required.

7. **Tests** — implement test vectors per RFC-0011-a §Test Vectors (12 vectors; 8-floor satisfied):
   - `audit-list-empty`, `audit-list-with-filter`, `audit-list-invalid-status`, `audit-list-bad-capability-root`
   - `audit-show-success`, `audit-show-not-found`, `audit-show-bad-id-format`, `audit-show-reject-reason-with-pw-substr`
   - `envelope-pretty-tty`, `envelope-json-pipe`, `auditor-mode-reject-unfiltered`, `redaction-clean-canonical-fields`

8. **CI regression** — add tests asserting G1 (no INSERT/UPDATE SQL hit the receipt store during `octo audit {list,show}` invocation) and G5 (byte-equivalent output across `auditor` / `ci` / `dev` / `human` modes for same canonical fixture) per RFC-0011-a §Design Goals.

9. **Mission state** — mission transitions Claimed → In Progress → Completed per BLUEPRINT.md §Mission Lifecycle. Cite landed commit stack in completion log.

### Cargo deps

| Crate                                           | Why                                                                                                                                                              | Layer |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| `octo-audit`                                    | Audit substrate (Layer C; NEW per RFC-0011-a §Substrate `[ADD]`); exposes `list_receipts`, `get_receipt`, `AuditFilter`, `AuditError`, `ReceiptId`, `audit_home` | C     |
| `octo-settlement` (transitive via `octo-audit`) | Settlement substrate (Layer B per RFC-0959); source of `ReceiptRecord` projection                                                                                | B     |

No new direct deps in `crates/octo-cli` — audit substrate depends on
`octo-settlement` transitively. The CLI depends on the audit substrate
alone.

## Test Vectors

12 vectors per RFC-0011-a §Test Vectors (8-floor satisfied):

- `audit-list-empty` — Receipt store contains zero receipts; no filters. `OutputEnvelope<AuditListOutput> { receipts: [], total_matched: 0, has_more: false }`; exit 0.
- `audit-list-with-filter` — 1,000 receipts; `--since 7d --model llama-3.1-8b --limit 10`. Up to 10 summaries matching filter; `total_matched` is count BEFORE `--limit` truncation; `has_more: true` if `total_matched > 10`; exit 0.
- `audit-list-invalid-status` — `--status invalid`. `OctoCliError::InvalidFilter("status: must be one of ok|partial|reject")`; exit 16.
- `audit-list-bad-capability-root` — `--capability-root 01ab` (too short). `OctoCliError::InvalidFilter("capability-root: expected 32 lowercase hex chars, got 4")`; exit 16.
- `audit-show-success` — `octo audit show 01ab..cd34`. `OutputEnvelope<AuditShowOutput> { receipt: <full ReceiptRecord> }`; exit 0.
- `audit-show-not-found` — `octo audit show deadbeef..0001`. `OctoCliError::ReceiptNotFound("deadbeef..0001")`; exit 17.
- `audit-show-bad-id-format` — `octo audit show not-hex`. `OctoCliError::InvalidFilter("receipt_id: expected 32 lowercase hex chars, got 7")`; exit 16.
- `audit-show-reject-reason-with-pw-substr` — `reject_reason = "auth failed: password=hunter2 invalid"`; `audit show`. Receipt surfaces with `reject_reason` rendered; `password=hunter2` REPLACED with `[REDACTED:pw]` per `OctoCliRedactor` value-pattern sweep; exit 0.
- `envelope-pretty-tty` — TTY mode + no `--json`; `octo audit list`. Pretty-printed table (columns: `RECEIPT_ID`, `SUBJECT_DID`, `MODEL`, `EXECUTED_AT`, `STATUS`, `COST_DQA`); ANSI color when stdout is a TTY and `--no-color` is not set.
- `envelope-json-pipe` — Non-TTY (pipe to `cat`); `octo audit list --json`. JSON via `serde_json::to_string_pretty`; `schema_version: 2`; `preview_only: false`; field order matches §Output Envelope.
- `auditor-mode-reject-unfiltered` — `octo --mode auditor audit list --status reject` on mixed fixture. Response is the FULL set (not reject-only); `--status reject` is silently no-op'd in Auditor mode per §Security Considerations row 2; exit 0.
- `redaction-clean-canonical-fields` — `prompt_hash = "01ab..cd34"`, `cost_dqa = "1234.567890"`; redactor runs. Both fields emitted verbatim (no redaction); redactor value-pattern sweep does NOT match canonical shapes per §Redaction.

Canonical fixture: `docs/07-developers/octo-cli-implementation-guide.md` §Audit Subcommand Fixtures.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — appends `Commands::Audit { List, Show }` clap variants; NEW dispatch module `commands/audit.rs`; extends `OctoCliError` with 4 audit-specific variants (per RFC-0011-a §Error Handling); NO new Layer A or Layer B types
- `octo-audit` (Layer C, NEW per RFC-0011-a §Substrate `[ADD]`) — thin read-only projection layer over `octo-settlement`; exposes `list_receipts`, `get_receipt`, `AuditFilter`, `AuditError`, `ReceiptId`, `audit_home`; owns NO persistence
- `octo-settlement` (Layer B, RFC-0959) — source of truth for `ReceiptRecord`; audit substrate is a thin read-only projection per RFC-0011-a §Substrate-truth dependency

Layer direction: C (CLI) → C (audit substrate) → B (settlement substrate). NO reverse deps. The audit substrate MUST NOT introduce new persistence — every read translates to a substrate call against `octo_settlement::ReceiptStore` per RFC-0011-a §Substrate-truth dependency.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cli --all-features
# G1 regression (no receipt-store mutation):
grep -rE 'INSERT|UPDATE|DELETE' crates/octo-cli/src/commands/audit.rs  # expect 0 hits
# G5 regression (cross-mode consistency):
cargo test -p octo-cli --all-features --test audit_cross_mode_consistency  # exit 0
# Audit fixture smoke:
octo audit list --since 7d --limit 10 --json
octo audit show <receipt_id> --json
```

## Backward compat

This mission lands ADDITIVELY across every contract boundary per RFC-0011-a §Compatibility:

1. **No `schema_version` bump required.** New `AuditListOutput` and `AuditShowOutput` are NEW data types and do NOT modify parent `OutputEnvelope<T>` (already at `schema_version = 2` carrying the additive `preview_only` field per parent §Output Envelope). Parent's existing data types unchanged.
2. **No new exit codes break parent semantics.** Parent §Exit Codes reserves 17-63 for amendment additions; this mission occupies 17, 18, 19 per RFC-0011-a §Error Handling. Parent's existing exit codes (0-16, 64-78, 100-127) unchanged.
3. **No new clap variants break parent dispatch.** Adds one new variant to parent `Commands` enum (`Commands::Audit`) and a new dispatch module (`commands/audit.rs`); existing variants and modules unchanged.
4. **No new redaction patterns required.** Per RFC-0011-a §Redaction, canonical receipt shape is redactor-clean by construction. `OctoCliRedactor` reused verbatim from parent.
5. **Confirmation flag matrix extends cleanly.** Per RFC-0011-a §Confirmation gate matrix: read commands carry `(no flag)` in every column; no mode-specific confirmation logic added.
6. **Stub command compatibility unaffected.** Parent's stub deprecation schedule (v1.0 banner, v1.1 hard-error, v2.0 removal) is unchanged; this mission introduces NO new stubs.

## Cross-references

- RFC-0011-a — `octo audit` Subcommands (parent of this mission)
- RFC-0011 — `octo` CLI Substrate (parent of RFC-0011-a; substrate contracts: output envelope, redaction, error envelope, exit codes, confirmation gate matrix, TTY-aware rendering, stub command compatibility)
- RFC-0965 — Capability Extension Format (receipt `capability_root` shape per RFC-0965 §2 Specification for receipt cost field)
- RFC-0959 — Ask Settlement Chain (primary producer of audit receipts per RFC-0959; `ReceiptRecord` source of truth; cost-dqa-migration wire form; burn-event linkage; market-delivery receipts subset)
- RFC-0010 — Canonical DID Codec (DID rendering in audit output per parent §Hex32 + `octo_wallet::Did` newtype)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping for the new operations per RFC-0011-a §RFC-0008 Execution Class Mapping)
- RFC-0917 — HTTP Proxy + Python SDK (programmatic counterpart to this CLI amendment; out of scope but related per RFC-0011-a §Related RFCs)
- [[cipherocto-design-principles]] — Layer C/D per-RFC evolution; extension over central enum (per RFC-0011-a §Rationale — Why extension over central enum)

## Why 1 release cycle gate

N/A — this mission is a READ-ONLY amendment per RFC-0011-a §Compatibility No deprecation timeline applies (no commands are removed; no exit codes are reclaimed; no `schema_version` is bumped). The mission lands atomically in a single RFC cycle per RFC-0011-a §Implementation Phases (no Phase 2 for this amendment). Follow-on amendments (RFC-0011-b/c/d/e/f/g) follow their own amendment chains and reservation rules.

## Claimant

@unassigned
