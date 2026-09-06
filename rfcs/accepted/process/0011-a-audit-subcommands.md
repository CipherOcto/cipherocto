# RFC-0011-a: `octo audit` Subcommands

## Status

Accepted (2026-08-31)

## Authors

- Authored by `@cipherocto` and `@mmacedoeu` per the amendment chain enumerated in RFC-0011 Status header (audit, reputation, agent lifecycle, role provisioning, vault operations, mesh operations, governance).

## Maintainers

- Maintainer: `@cipherocto` per the amendment chain enumerated in RFC-0011 Status header.

## Summary

This amendment extends the parent RFC-0011 (`octo` CLI substrate) by adding the
audit subcommand slice — `octo audit list` and `octo audit show` — for inspecting
settlement receipts emitted by the RFC-0959 settlement substrate. The amendment
REUSES every contract the parent RFC establishes: `OutputEnvelope<T>` (parent
§Output Envelope), `OctoCliRedactor` (parent §Redaction Layer), `OctoCliError`
(parent §Error Handling), the exit-code table (parent §Exit Codes), the
confirmation flag matrix (parent §Confirmation Flag Matrix), and TTY-aware
rendering (parent §Output Envelope). It defines NO new contract on Layer A/B;
the only new substrate API is the `[ADD]` surface that lands on the audit
substrate via substrate-side amendments (parent §Subcommand Taxonomy
`[ADD]` pattern). No `schema_version` bump is required because every new field
on the output types is additive per parent §Compatibility.

## Dependencies

**Requires:**

- RFC-0011: `octo` CLI Substrate — substrate contracts (output envelope, redaction, error envelope, exit codes, confirmation gate matrix, TTY-aware rendering, stub command compatibility)
- RFC-0965: Capability Extension Format — `capability_root` on a receipt is a `cap_extension_root` per RFC-0965 §Capability Extension
- RFC-0965: Payment Caveat Asset Binding — receipt cost field references the asset-bound `Dqa` wire form per the same RFC
- RFC-0959: Ask Settlement Chain — primary producer of audit receipts (the substrate that mints `SettlementReceipt` entries on capability consumption)
- RFC-0959: Settlement Cost DQA Migration — receipt `cost_dqa` field is a `Dqa` per this migration
- RFC-0959: Burn Event Wire Form — receipt burn-event references for cost-event linkage
- RFC-0959: Market Delivery — market-delivery receipts are a subset of the audit surface (a receipt with `subject_did == market_contract_id`)
- RFC-0008: Deterministic AI Execution Boundary — execution class mapping for the new operations

> **Dependency Validation Rules:**
>
> 1. DAG (no cycles); Requires listed as mission prereqs; optional separated; Planned dependencies noted
> 2. No 2-cycle sibling required — RFC-0011-a is acyclic against all Required dependencies (each cited RFC is reachable via the parent RFC-0011 which is the canonical RFC-0011-chain anchor)

## Design Goals

| Goal | Target                   | Metric                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---- | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1   | Read-only guarantee      | `octo audit {list,show}` MUST NOT mutate the receipt store in any mode (Human / Ci / Dev / Auditor); CI regression test asserts no row INSERT/UPDATE                                                                                                                                                                                                                                                                                                      |
| G2   | Redaction completeness   | `OctoCliRedactor` covers every secret-shaped field emitted by audit output (`prompt_hash` is not redacted — it is a public digest; `subject_did`, `capability_root`, `model`, `executed_by` are public too; only the redactor's secret-pattern sweep applies, and there are zero secret-pattern matches against the canonical receipt shape per §Redaction)                                                                                               |
| G3   | Deterministic exit codes | Same input → same exit code across runs (table in §Error Handling + §Exit Code Reservation); `list` and `show` always exit 0 on success and never exit with a substrate-error code (substrate errors map to `Internal` exit 64 per parent §Error Handling)                                                                                                                                                                                                |
| G4   | Sub-100ms `list` latency | p95 < 100ms on the canonical 1,000-receipt fixture (per §Performance Targets); a substrate-level index on `(subject_did, executed_at_unix DESC)` is the access path                                                                                                                                                                                                                                                                                       |
| G5   | Cross-mode consistency   | `auditor`, `ci`, `dev`, `human` modes MUST surface the same receipt set for the same filter (no hidden rejection-silencing filter per §Security Considerations row 2); CI test asserts byte-equivalent output across modes for the same canonical fixture **WITHOUT** `--status` filter (Auditor mode silently no-ops all `--status` values per H5 finding; filtered-mode equivalence is verified separately via §Test Vectors `reject-hiding-gate` rows) |

## Motivation

The parent RFC-0011 §Implementation Phases §Phase 2 explicitly defers audit subcommands to this amendment:

> Phase 2 (audit, future amendment per Status header amendment chain) — Audit subcommands: `audit {list,show}`. Depends on RFC-0965 audit-window caveat + RFC-0959 settlement substrate wiring.

The gap this amendment closes:

1. **No operator visibility into own settlement history.** Operator mints a capability with `oc audit_window` and consumes via RFC-0959 settlement has no CLI surface to inspect the resulting receipt. Receipt exists in substrate (`octo_settlement::SettlementReceipt`) but reachable only via direct substrate API or HTTP proxy + Python SDK (RFC-0917).
2. **No filterable query surface.** Substrate exposes `SettlementReceipt` as a struct; iterating the full receipt log for a filter match requires a Rust binary or Python script.
3. **No TTY-aware presentation.** Substrate returns raw bytes; no pretty-print path on terminal and no canonical JSON envelope for scripting consumers.
4. **No mode gating.** Substrate API is single layer; no per-mode distinction and no `--allow-write` gate for read commands.

This amendment is the read-only operator surface for the RFC-0959 settlement substrate. Mutating audit operations (`octo audit redact`, `octo audit export`) are explicitly OUT OF SCOPE — they land per a follow-on amendment if/when a mutating requirement surfaces.

## Roles and Authorities

> **The "Nothing should be implied" rule (specification layer):** Every actor affecting correctness, security, accountability, or consensus MUST be named with a stable identifier, defined authority scope, and typed lifecycle. Cross-reference: parent RFC-0011 §Roles and Authorities and BLUEPRINT.md "Human vs Agent Roles" table.

Audit subcommands add NO new operator-facing roles. Parent RFC-0011 §Roles and Authorities establishes three roles — `OperatorKind::Human`, `OperatorKind::CiBot`, `OperatorKind::Auditor` — which fully cover this amendment's surface. The amendment only REFINES the `Auditor` role behavior:

| Role           | Identifier              | Authority Scope                                       | Lifecycle | Source/Ref                             |
| -------------- | ----------------------- | ----------------------------------------------------- | --------- | -------------------------------------- |
| Human Operator | `OperatorKind::Human`   | read-only audit surface (no `--confirm` required)     | stateless | parent RFC-0011 §Roles and Authorities |
| CI Bot         | `OperatorKind::CiBot`   | read-only audit surface (no `--allow-write` required) | stateless | parent RFC-0011 §Roles and Authorities |
| Auditor (RO)   | `OperatorKind::Auditor` | read-only audit surface + audit-trail access          | stateless | parent RFC-0011 §Roles and Authorities |

### Role-specific behavior on audit commands

Parent §Roles and Authorities defines Auditor as "read-only + audit-trail access". This amendment REFINES that contract:

- **Human / CI / Dev / Auditor modes (read freely):** No `--confirm` / `--allow-write` needed (read-only command set, no write surface in this amendment).
- **All modes (no-reject-hiding constraint):** Filtering to a non-reject status (`--status ok` or `--status partial`) hides rejected receipts from output. This is a SECRECY gate that applies in ALL modes (Human, Ci, Dev, Auditor) — a compromised operator session in default `human` mode must not be able to conceal reject rows via `--status ok` either. The gate: any `--status` value excluding `reject` requires either `--include-reject` (UNION semantics: `result_set = filter-result ∪ reject-rows` per M4 finding) OR `--confirm-acknowledge` (explicit confirmation of the reject-hiding intent). Without either flag, the CLI exits 16 `InvalidFilter`. Auditor mode additionally enforces silent no-op on ALL `--status` values (`--status ok` / `--status partial` / `--status reject` are all treated as "all" — symmetric semantics per H5 finding) per §Security Considerations. See §Security Considerations row 2 for rationale and §Adversary Analysis row 1 for threat model.

### Role transitions

None. Audit subcommands are stateless read operations; no role transition triggered.

### Out-of-scope roles

Parent RFC §Roles and Authorities out-of-scope roles (Node Operators, AI Agents) remain out of scope. Node operators access receipts via direct substrate API + dedicated RPC; AI agents access via HTTP proxy + Python SDK (RFC-0917). CLI is the operator UX layer, not node admin or agent runtime layer.

## Specification

### §7.1 System architecture

The `octo audit {list,show}` commands are read-only substrate consumers. They
do NOT introduce new persistence; they read from the receipt store owned by
the RFC-0959 settlement substrate (`octo_settlement::ReceiptStore`).

```mermaid
graph TB
    subgraph Bin["octo binary"]
        A[clap parser: Octo]
        B[Subcommand dispatch<br/>Commands::Audit]
        C[OctoCliError + redaction]
        D[OutputEnvelope&lt;AuditListOutput | AuditShowOutput&gt;]
        E[TTY detection]
    end

    subgraph AuditSub["Audit substrate (Layer C/D)"]
        F["octo-audit<br/>[ADD] list_receipts(filter)<br/>[ADD] get_receipt(id)"]
    end

    subgraph Settle["Settlement substrate (RFC-0959, Layer B)"]
        G["octo_settlement<br/>SettlementReceipt (read-only projection)"]
    end

    subgraph Persist["Persistence"]
        H[(receipt store<br/>append-only log)]
    end

    A --> B
    B --> C
    B --> D
    D --> E
    B -->|audit list| F
    B -->|audit show| F
    F --> G
    G --> H
```

Audit substrate (`octo-audit`) is a thin read-only projection layer over the RFC-0959 settlement substrate. It exposes two functions (see §Substrate for `[ADD]` signatures) and owns NO persistence of its own — every read translates to a substrate call against `octo_settlement::ReceiptStore`.

The `octo` binary is the same Layer-C orchestrator described in parent §Binary Surface. Audit subcommands do NOT modify the binary root struct (`Octo`); they add one variant to the `Commands` enum and a new `commands/audit.rs` dispatch module.

### §7.2 Subcommand taxonomy

The parent RFC §Subcommand Taxonomy establishes the table format (Args /
Flags / Output / Substrate / Exit codes / Redaction / Side effects / Dry-run).
The two new subcommands follow the same format verbatim.

#### `octo audit list`

| Aspect       | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | none                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Flags        | `--since <duration \| unix>` (RFC-0011-a §7.5 Filters grammar), `--until <unix>` (RFC-0011-a §7.5 Filters), `--capability-root <hex32>` (RFC-0011-a §7.5 Filters), `--model <ref>` (RFC-0011-a §7.5 Filters), `--status <ok \| partial \| reject>` (RFC-0011-a §7.5 Filters), `--limit <n:usize>` (default 100, max 10000), `--include-reject` (UNION: forward reject rows into result set; `requires = "status"`), `--confirm-acknowledge` (explicit confirmation of reject-hiding intent; `requires = "status"`), `--json` (force JSON output regardless of TTY), `--no-color` (disable ANSI) |
| Output       | `AuditListOutput { receipts: Vec<ReceiptSummary>, total_matched: usize, has_more: bool }` where `ReceiptSummary { receipt_id: Hex32, subject_did: Did, capability_root: Hex32, model: String, executed_at_unix: u64, status: ReceiptStatus, cost_dqa: String }`                                                                                                                                                                                                                                                                                                                                 |
| Substrate    | `[ADD] octo_audit::list_receipts(filter: &AuditFilter) -> Result<Vec<ReceiptSummary>, AuditError>` (see §Substrate)                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Exit codes   | 0, 16 (`InvalidFilter`), 64                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Redaction    | none on the canonical fields (per §Redaction; all canonical fields are public digests / public DIDs / public cost)                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Side effects | none (read-only)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Dry-run      | n/a                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

Notes:

- `--limit` defaults to 100, max 10000 (parent bounded-result-set pattern; `has_more: true` signals "narrow the filter and re-invoke" rather than cursor-walk).
- `subject_did` is the canonical RFC-0010 DID codec form (`did:octo:` + base58btc of 32-byte pubkey, 43-44 chars; RFC-0010 §Canonical DID Codec); rendered via parent's `octo_wallet::Did` newtype.
- `--status <ok|partial|reject>` is exclusive (single value). Combining statuses requires two invocations (substrate filter is single-valued; see §Filters).

#### `octo audit show <receipt_id>`

| Aspect       | Value                                                                                                                                                                                                                                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Args         | `<receipt_id>` (REQUIRED, positional; format `<32-lowercase-hex>` only — dashed UUID form NOT accepted per RFC-0010 §Canonical DID Codec; format violation → exit 16 `InvalidFilter`)                                                                                                                                                      |
| Flags        | `--json` (force JSON output regardless of TTY), `--no-color` (disable ANSI)                                                                                                                                                                                                                                                                |
| Output       | `AuditShowOutput { receipt: SettlementReceipt }` where `SettlementReceipt { receipt_id: Hex32, subject_did: Did, capability_root: Hex32, model: String, prompt_hash: Hex32, cost_dqa: String, executed_by: Did, executed_at_unix: u64, status: ReceiptStatus, reject_reason: Option<String> }`                                             |
| Substrate    | `[ADD] octo_audit::get_receipt(id: &ReceiptId) -> Result<SettlementReceipt, AuditError>` (see §Substrate)                                                                                                                                                                                                                                  |
| Exit codes   | 0, 16 (`InvalidFilter` on format violation), 17 (`ReceiptNotFound`), 64                                                                                                                                                                                                                                                                    |
| Redaction    | `prompt_hash` is a public blake3 digest (NOT redacted); `reject_reason` is operator-facing prose (NOT redacted; if the substrate ever emits a reject_reason that contains secret-shaped content, `OctoCliRedactor` applies — but per §Implicit Assumptions Audit "Receipts contain no PII" the canonical shape carries no secret material) |
| Side effects | none (read-only)                                                                                                                                                                                                                                                                                                                           |
| Dry-run      | n/a                                                                                                                                                                                                                                                                                                                                        |

Notes:

- `receipt_id` is the canonical 32-byte blake3 digest of the receipt body, rendered lowercase hex. Dashed-UUID form rejected at the CLI parser (parent §Binary Surface convention; format violations route through `OctoCliError::InvalidFilter` rather than `OctoCliError::ClapParse` so scripting consumers see a domain error, not a usage error).
- `reject_reason` is `Option<String>`, populated when `status == ReceiptStatus::Reject` (RFC-0959 substrate shape). CLI does NOT log this field at INFO level; rendered only in the operator-facing output envelope on explicit `octo audit show`.

### §7.3 Output envelope

Both subcommands emit the parent's `OutputEnvelope<T>` (parent §Output Envelope) with `schema_version = 2` (parent-bumped value carrying the `preview_only` field) and TTY-aware rendering (parent §Output Envelope):

```text
OutputEnvelope<AuditListOutput>  (octo audit list)
OutputEnvelope<AuditShowOutput>  (octo audit show)
```

Field declaration order is canonical and MUST match the order below (parent §Determinism Requirements §JSON field order — consumers MUST NOT rely on JSON key order, but the CLI emits fields in declaration order on the Rust struct):

```rust
//! crates/octo-cli/src/commands/audit.rs (declaration order; canonical)

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputEnvelope<T> {
    pub schema_version: u32,    // value: 2 (parent RFC §Output Envelope)
    pub generated_at: DateTime<Utc>,
    pub data: T,
    pub exit_code: i32,
    pub preview_only: bool,     // always `false` for audit commands (no --dry-run)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuditListOutput {
    pub receipts: Vec<ReceiptSummary>,
    pub total_matched: usize,    // count BEFORE --limit truncation
    pub has_more: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReceiptSummary {
    pub receipt_id: Hex32,
    pub subject_did: Did,         // rendered as `did:octo:<base58btc>`
    pub capability_root: Hex32,
    pub model: String,
    pub executed_at_unix: u64,
    pub status: ReceiptStatus,
    pub cost_dqa: String,         // RFC-0959 cost-dqa-migration wire form
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuditShowOutput { pub receipt: SettlementReceipt }

// CLI-side receipt projection re-exports the canonical substrate types.
// Source of truth: RFC-0959 `octo_settlement::SettlementReceipt` and
// `octo_settlement::ReceiptStatus`. CLI MUST NOT declare parallel types
// because every CLI-side field drift becomes a substrate-truth mismatch
// (per CLAUDE.md §Architectural Principles: "extension over enumeration,
// no central enums, no parallel abstractions"). The substrate's
// `SettlementReceipt` derives the same `Serialize, Deserialize, Debug, Clone`
// traits; CLI consumes it verbatim via the audit substrate's re-export.
pub use octo_audit::SettlementReceipt;
pub use octo_audit::ReceiptStatus;
```

The `ReceiptStatus` Raw escape hatch (`Unknown` arm) is owned by the
settlement substrate (RFC-0959 §Receipt Status); the canonical `Unknown` arm shape is defined by the substrate-side amendment to RFC-0959 §Receipt Status.
NOT a central enum per
CLAUDE.md §Architectural Principles — closure of the audit receipt
projection; new statuses (e.g., TimedOut, RateLimited) are added via
substrate-side amendments (see §Rationale).

> **Substrate re-export note (Wave 3 H1 fix):** `octo-audit` is the
> canonical re-export boundary for both `SettlementReceipt` and
> `ReceiptStatus` so the CLI consumes Layer C → Layer C only. The
> `pub use octo_audit::ReceiptStatus;` re-export on the audit substrate
> crate does NOT exist yet in substrate; companion mission files it
> alongside the `[ADD]` surface (out of scope for this DOC-ONLY RFC
> cycle per the §Substrate-truth disclaimer).

TTY-aware rendering (parent §Output Envelope) applies unchanged:

| Condition           | Output format                                                                                                                                                                                              |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TTY + no `--json`   | Pretty-printed (table mode for `list`; key-value for `show`); ANSI colors when stdout is a TTY AND `--no-color` / `OCTO_FORCE_JSON` is not set; gated by `std::io::IsTerminal` per parent §Output Envelope |
| Non-TTY OR `--json` | JSON (`serde_json::to_string_pretty`)                                                                                                                                                                      |

`--json` forces JSON output regardless of TTY. `--no-color` disables ANSI. `OCTO_FORCE_JSON` (per parent §Output Envelope) forces JSON output.

### §7.4 Substrate `[ADD]` signatures

The parent RFC §Subcommand Taxonomy establishes the `[ADD]` pattern for
declaring substrate API additions that the CLI consumes. This amendment
declares the following `[ADD]` surface on the audit substrate
(`crates/octo-audit/`):

1. **`list_receipts(filter: &AuditFilter) -> Result<Vec<ReceiptSummary>, AuditError>`** — filters on §Filters, returns summaries sorted by `executed_at_unix DESC`; substrate enforces `--limit` upper bound (default 100, max 10000) and returns `total_matched`/`has_more` for client pagination.

2. **`get_receipt(id: &ReceiptId) -> Result<SettlementReceipt, AuditError>`** — point lookup by canonical receipt ID (32-byte blake3 digest). Returns `AuditError::ReceiptNotFound` (exit 17) when absent.

3. **`AuditFilter`** — newtype struct:

   ```rust
   #[ADD]
   pub struct AuditFilter {
       pub since_unix: Option<u64>,
       pub until_unix: Option<u64>,
       pub capability_root: Option<[u8;32]>,
       pub model: Option<String>,
       pub status: Option<ReceiptStatus>,
       pub limit: usize,             // default 100, max 10000
   }
   ```

   `since_unix` / `until_unix` are inclusive bounds expressed as `u64` unix seconds (canonical non-negative per parent §Implicit Assumptions Audit "executed_at_unix is monotonic"; matches `executed_at_unix: u64` on `SettlementReceipt` per §Receipt Shape). Negative values are rejected at the CLI parser (exit 16 `InvalidFilter`). CLI converts `--since <duration>` (`7d`, `24h`, `3600s`) to absolute unix at dispatch; `--since <unix>` / `--until <unix>` pass through verbatim.

4. **`AuditError`** — new error enum:

   ```rust
   #[ADD]
   #[derive(thiserror::Error, Debug)]
   pub enum AuditError {
       #[error("receipt not found: {0}")]
       ReceiptNotFound(String),                 // -> CLI exit 17
       #[error("invalid filter: {0}")]
       InvalidFilter(String),                   // -> CLI exit 16 (parent reserved)
       #[error("internal error: {0}")]
       Internal(String),                        // -> CLI exit 64
       #[error(transparent)]
       SettlementStore(#[from] octo_settlement::SettlementError),  // -> CLI exit 64
   }
   ```

   `AuditError::ReceiptNotFound` carries the canonical receipt ID hex form so scripting consumers see the exact ID not found (the substring is a hex digest and contains no secret material).

5. **`ReceiptId(pub [u8;32])`** — newtype mirroring parent §Hex32. CLI surfaces `octo_audit::ReceiptId`; substrate owns `octo_settlement::ReceiptId`; `From` impl at substrate boundary.

6. **`[ADD] octo_audit::audit_home() -> Result<PathBuf, AuditError>`** — canonical discovery helper. Resolves to `$OCTO_HOME/audit/receipts` if `OCTO_HOME` is set, otherwise `~/.config/octo/audit/receipts` per parent RFC §Implicit Assumptions Audit "Operator config dir" row; surfaces `AuditError::Internal` (exit 64) on filesystem errors (permission denied, missing parent directory that cannot be created). CLI does NOT call directly; substrate resolves internally for diagnostic / substrate-internal testing per §Key Files to Modify (out-of-band substrate use).

> **Substrate-truth disclaimer:** Substrate today may have different signatures or names. The `[ADD]` entries above are the canonical forms this amendment mandates; the substrate amendment landing alongside this RFC MUST produce substrate-truth matching these signatures. Substrate amendments filed separately per parent §Subcommand Taxonomy convention.

### §7.5 Filters

The filter set for `octo audit list` is a strict subset of what the
substrate's `AuditFilter` accepts (per §Substrate entry #3):

| Flag                             | Type             | Constraint                                                                | Maps to `AuditFilter` field        |
| -------------------------------- | ---------------- | ------------------------------------------------------------------------- | ---------------------------------- |
| `--since <duration\|unix>`       | duration or unix | duration form: `<n>(d\|h\|m\|s)` where n>0; unix form: <u64>              | `since_unix: Option<u64>`          |
| `--until <unix>`                 | unix             | <u64>; if both `--since` and `--until` are set, `since <= until` required | `until_unix: Option<u64>`          |
| `--capability-root <hex32>`      | 32-lowercase-hex | format violation → exit 16                                                | `capability_root: Option<[u8;32]>` |
| `--model <ref>`                  | string           | non-empty; no wildcard (`*`)                                              | `model: Option<String>`            |
| `--status <ok\|partial\|reject>` | enum             | single value; case-insensitive                                            | `status: Option<ReceiptStatus>`    |
| `--limit <n:usize>`              | usize            | `1 <= n <= 10000`; default 100                                            | `limit: usize`                     |
| `--include-reject`               | flag             | requires `--status`; UNION semantics per §Filters                         | client-side (no substrate field)   |
| `--confirm-acknowledge`          | flag             | requires `--status`; explicit confirmation of reject-hiding intent        | client-side (no substrate field)   |
| `--json`                         | flag             | force JSON output regardless of TTY                                       | client-side (no substrate field)   |
| `--no-color`                     | flag             | disable ANSI color output                                                 | client-side (no substrate field)   |

**Filter grammar (formal EBNF):**

```text
filter      := since-clause? until-clause? cap-clause? model-clause? status-clause? include-reject-clause? confirm-acknowledge-clause? limit-clause? json-clause? no-color-clause?
since       := '--since' ( DURATION | UNIX )
until       := '--until' UNIX
cap         := '--capability-root' HEX32
model       := '--model' REF
status      := '--status' STATUS
include-reject := '--include-reject'  (requires --status; UNION semantics)
confirm-acknowledge := '--confirm-acknowledge'  (requires --status; explicit confirmation)
limit       := '--limit' NAT
json        := '--json'
no-color    := '--no-color'
DURATION    := NAT ('d'|'h'|'m'|'s'); UNIX := NAT; HEX32 := 32 × [0-9a-f]
REF         := non-empty UTF-8 string (no leading/trailing whitespace)
STATUS      := 'ok' | 'partial' | 'reject' (case-insensitive)
NAT         := positive integer
```

**Filter ordering:** Flags MAY appear in any order. The CLI parses all
flags into `AuditFilter` before calling the substrate; `--limit` always
wins over substrate-side default when set.

**Mutual exclusion:** `--status` accepts exactly one value (substrate-truth:
`[ADD] AuditFilter.status` is `Option<ReceiptStatus>`, not
`Vec<ReceiptStatus>`). Combining statuses requires two invocations.

**`--include-reject` UNION semantics (per M4 finding):** When
`--status <ok|partial>` is combined with `--include-reject`, the result
set is `filter-result ∪ reject-rows` (reject rows are FORWARDED into the
result set, not filtered out). The current substrate-truth
`AuditFilter.status: Option<ReceiptStatus>` is single-valued (per
§Substrate entry #3) — a single substrate call with `status = Ok` (or
`Partial`) would EXCLUDE reject rows, producing an empty union. **Two
implementation paths (substrate-fix path is canonical):** the
forward-looking path is `RFC-0959 [SUBSTRATE-FIX: §Data Structures
AuditFilter.status: Vec<ReceiptStatus>]` — once that substrate amendment
lands, the CLI performs one substrate call with
`status: vec![<ok|partial>, Reject]` and the union is enforced
server-side. Until that amendment lands, the CLI MUST perform **two**
substrate calls (one with `--status <ok|partial>`, one with
`--status reject`), union the result sets client-side, and surface
`total_matched` as the post-union count. The selector between paths
is owned by the substrate amendment; the CLI implementation must
support whichever substrate shape is current. This is the
operator-friendly alternative to `--confirm-acknowledge`: operators
who legitimately want a non-reject-only view declare either intent.

### §7.6 Receipt shape

The canonical receipt shape carried by `AuditShowOutput.receipt` is the **CLI projection layer** over the RFC-0959 settlement substrate's `SettlementReceipt`. Of the 10 fields below, only `receipt_id` is substrate-direct (from the settlement envelope). The remaining 9 fields are `[CLI-PROJECTED]` from envelope + capability/ask registries + operator/cost wire forms per RFC-0959; the substrate's actual `SettlementReceipt` shape is `{envelope, router_signature}` where `envelope = {receipt_id, event, nonce, settled_at_unix}` (RFC-0959 §Data Structures). All ten field names, types, and ordering are stable on the CLI surface; future substrate amendments that change the projection source MUST preserve the CLI field shape per parent §Compatibility (additive-only).

| Field              | Type             | Source                                                                    | Notes                                                                                                                                         |
| ------------------ | ---------------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `receipt_id`       | `Hex32`          | blake3(receipt body)                                                      | Canonical 32-byte digest; lowercase hex per parent §Hex32 newtype                                                                             |
| `subject_did`      | `Did`            | `[CLI-PROJECTED]` from envelope + capability registry per RFC-0959        | RFC-0010 canonical DID codec form (base58btc wire form)                                                                                       |
| `capability_root`  | `Hex32`          | `[CLI-PROJECTED]` from envelope + capability/ask registry per RFC-0959    | The root ID of the consumed capability                                                                                                        |
| `model`            | `String`         | `[CLI-PROJECTED]` from envelope + ask registry per RFC-0959               | Free-form model reference; CLI renders canonical `{namespace}/{family}@{version}` form per RFC-0959 `ModelRef` wire form (F-4 rendering rule) |
| `prompt_hash`      | `Hex32`          | `[CLI-PROJECTED]` from envelope + ask registry per RFC-0959               | blake3 of input prompt; PUBLIC digest (NOT prompt content); safe to log                                                                       |
| `cost_dqa`         | `String`         | `[CLI-PROJECTED]` from envelope + cost wire form per RFC-0959             | RFC-0959 cost-dqa-migration canonical 16-byte hex wire form (32 lowercase hex chars); String for forward compat                               |
| `executed_by`      | `Did`            | `[CLI-PROJECTED]` from envelope + operator registry per RFC-0959          | RFC-0010 canonical form (base58btc wire form)                                                                                                 |
| `executed_at_unix` | `u64`            | `[CLI-PROJECTED]` from `envelope.settled_at_unix` per RFC-0959            | monotonic per §Implicit Assumptions Audit; canonical unix seconds (non-negative)                                                              |
| `status`           | `ReceiptStatus`  | `[CLI-PROJECTED]` from envelope + settlement wire form per RFC-0959       | `Ok` / `Partial` / `Reject`                                                                                                                   |
| `reject_reason`    | `Option<String>` | `[CLI-PROJECTED]` from envelope + settlement error wire form per RFC-0959 | operator-facing prose; populated when `status == Reject`; redactor applies if secret-shaped content ever slips in                             |

`ReceiptSummary` (the `list` projection) is a strict subset of
`SettlementReceipt`, omitting `prompt_hash`, `executed_by`, and `reject_reason`
because these are high-cardinality or voluminous and not actionable in the
list view. Operators retrieve full detail via `octo audit show <receipt_id>`.

| Field              | Type            | Source                                                                 | Notes                                                                                                                                         |
| ------------------ | --------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `receipt_id`       | `Hex32`         | blake3(receipt body)                                                   | Canonical 32-byte digest; lowercase hex per parent §Hex32 newtype                                                                             |
| `subject_did`      | `Did`           | `[CLI-PROJECTED]` from envelope + capability registry per RFC-0959     | RFC-0010 canonical DID codec form (base58btc wire form)                                                                                       |
| `capability_root`  | `Hex32`         | `[CLI-PROJECTED]` from envelope + capability/ask registry per RFC-0959 | The root ID of the consumed capability                                                                                                        |
| `model`            | `String`        | `[CLI-PROJECTED]` from envelope + ask registry per RFC-0959            | Free-form model reference; CLI renders canonical `{namespace}/{family}@{version}` form per RFC-0959 `ModelRef` wire form (F-4 rendering rule) |
| `executed_at_unix` | `u64`           | `[CLI-PROJECTED]` from `envelope.settled_at_unix` per RFC-0959         | monotonic per §Implicit Assumptions Audit; canonical unix seconds (non-negative)                                                              |
| `status`           | `ReceiptStatus` | `[CLI-PROJECTED]` from envelope + settlement wire form per RFC-0959    | `Ok` / `Partial` / `Reject`                                                                                                                   |
| `cost_dqa`         | `String`        | `[CLI-PROJECTED]` from envelope + cost wire form per RFC-0959          | RFC-0959 cost-dqa-migration canonical 16-byte hex wire form (32 lowercase hex chars); String for forward compat                               |

### §7.7 Redaction

The parent §Redaction Layer establishes `OctoCliRedactor` with the canonical patterns (`seed_bytes`, `private_key`, `holder_sig`, `pair_code`, `password`, `mnemonic`, `passphrase`, `pin`, `api_key`, `secret`/`token`, `Bearer <token>` in headers). The canonical receipt shape carries NO field whose name matches any pattern; all fields are blake3 digests, RFC-0010 canonical DIDs, RFC-0959 Dqa wire forms, enum tags, or monotonic unix timestamps. `OctoCliRedactor` REQUIRES NO extension for this amendment.

Only conditional sweep target is `reject_reason` VALUE content: if a substrate bug ever surfaces a `password=<value>` substring into the reject reason, the redactor's value-pattern sweep catches it regardless of field name per parent §Redaction Layer dual-pass semantics. Test vector `audit-show-reject-reason-with-pw-substr` asserts the sweep end-to-end.

### §7.8 Confirmation gate matrix

Read-only subcommands do NOT require `--confirm` or `--allow-write` in ANY mode (Human, Ci, Dev, or Auditor). The parent §Confirmation Flag Matrix pattern extends to this amendment: only mutating commands require confirmation. Audit subcommands are purely read-only; the table extends the parent matrix with one new read-only column:

| Operator mode     | Audit list / show (read) | Identity rotate / revoke (write) | Capability mint / attenuate (write)   | Policy show / list (read) |
| ----------------- | ------------------------ | -------------------------------- | ------------------------------------- | ------------------------- |
| `human` (default) | (no flag)                | `--confirm`                      | `--confirm` + `--confirm-acknowledge` | (no flag)                 |
| `ci`              | (no flag)                | `--allow-write`                  | `--allow-write`                       | (no flag)                 |
| `dev`             | (no flag)                | `--allow-write`                  | `--allow-write`                       | (no flag)                 |
| `auditor`         | (no flag)                | denied (exit 2)                  | denied (exit 2)                       | (no flag)                 |

The `require_confirm` gate at `crates/octo-cli/src/commands/audit.rs` returns success without consulting mode flags (read-only commands are unconditional). The Auditor constraint (no filter combinator that excludes reject rows) is enforced at the dispatch layer in `audit list`, not in `require_confirm`.

> **Clarification on `--confirm-acknowledge` reuse:** The
> `--confirm-acknowledge` flag used by the no-reject-hiding gate
> (§Roles and Authorities) is the SAME flag name as the parent's
> capability-mint confirmation flow, but it carries DIFFERENT semantics
> here — it is an INTENT DECLARATION for filter scope, not a mutation
> authorization. The two usages do NOT interfere: the no-reject-hiding
> gate fires only when `--status <ok|partial>` is set (a CLI filter
> argument); the parent's `--confirm-acknowledge` for capability mint
> fires only when minting a capability (a different subcommand). The
> matrix above documents the MUTATION-authorization semantics; the
> filter-intent semantics are documented in §Roles and Authorities and
> §Filters.

## RFC-0008 Execution Class Mapping

| Operation         | Class | Rationale                                                                                               |
| ----------------- | ----- | ------------------------------------------------------------------------------------------------------- |
| `octo audit list` | C     | Operator UX; reads local receipt store via settlement substrate; no consensus impact                    |
| `octo audit show` | C     | Operator UX; point lookup; no consensus impact                                                          |
| Redaction layer   | A     | Affects log emission; deterministic pattern matching (per parent RFC §RFC-0008 Execution Class Mapping) |
| Output envelope   | A     | Deterministic JSON/YAML serialization (per parent RFC §RFC-0008 Execution Class Mapping)                |

All operations are Class C or A per the mapping above. There are NO
Class B operations in this amendment (Class B would require proof
verification for consensus-critical use; audit is purely an
operator-facing read surface).

## Error Handling

The amendment extends `OctoCliError` (parent RFC §Error Handling) with
audit-specific variants and reserves exit codes 17-18 per parent RFC
§Exit Codes ("17-63 reserved per Status header amendment chain"; this
amendment is the first to occupy 17-18 after Wave 6.5 R2 H1 removed
`AuditResponseTooLarge` which previously occupied 19):

```rust
#[derive(thiserror::Error, Debug)]
pub enum OctoCliError {
    // ... existing parent variants ...

    #[error("invalid filter: {0}")]
    InvalidFilter(String),                                  // exit 16 (parent reserved)

    #[error("receipt not found: {0}")]
    ReceiptNotFound(String),                                // exit 17 (NEW)

    #[error("audit read failed: {0}")]
    AuditReadFailed(String),                                // exit 18 (NEW; substrate-level failure)

    // ... existing parent variants continue ...
}
```

The variant map also requires the clap value_parser error → `InvalidFilter`
translation (per Wave 5 R1 finding: `--status invalid` MUST exit 16, not
clap's default exit 2). The dispatch layer `main.rs` performs the
translation via the `From<clap::Error> for OctoCliError` impl below:

```rust
//! §Error Handling — clap value_parser error → OctoCliError mapping
//! (parent §Error Handling rule: domain errors → InvalidFilter (exit 16),
//! NOT clap's default exit-2 usage error). The `parse_status`
//! value_parser emits `clap::Error` with `ErrorKind::ValueValidation` on
//! rejection; this `From` impl catches that kind and translates to
//! `OctoCliError::InvalidFilter` so that `audit list --status invalid`
//! exits 16 (per §Error Handling variant table), not 2.
//!
//! Parser entry point: parent RFC §Binary Surface establishes clap
//! derive dispatch (clap `Parser` derive on every `Commands` variant
//! per parent L174-232), so this `From<clap::Error>` impl IS reached on
//! value_parser rejection — the dispatch layer propagates
//! `Result<_, clap::Error>` from the clap derive into this impl via
//! `?`, and value_parser-emitted `ErrorKind::ValueValidation` is
//! translated to `OctoCliError::InvalidFilter` (exit 16) per §Error
//! Handling variant table — NOT clap's default exit-2 usage error.

impl From<clap::Error> for OctoCliError {
    fn from(e: clap::Error) -> Self {
        if e.kind() == clap::error::ErrorKind::ValueValidation {
            // value_parser-emitted domain error (e.g., `--status invalid`);
            // map to InvalidFilter (exit 16) per §Error Handling variant table.
            OctoCliError::InvalidFilter(e.to_string())
        } else {
            // Generic clap parse failure → usage error (exit 2).
            OctoCliError::ClapParse(e.to_string())
        }
    }
}
```

Variant map:

| Variant           | Exit code | Trigger                                                                                                                                         |
| ----------------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `InvalidFilter`   | 16        | `--capability-root` not 32 hex; `--status` not in `{ok,partial,reject}`; `--limit` out of range; `--since`/`--until` malformed; `since > until` |
| `ReceiptNotFound` | 17        | `octo audit show <id>` and no receipt with that ID exists in the receipt store                                                                  |
| `AuditReadFailed` | 18        | Substrate-level read error (e.g., store locked, schema mismatch, integrity check failure)                                                       |
| `Internal`        | 64        | Catch-all per parent §Error Handling; sanitized via `sanitize_substrate_error`                                                                  |

`InvalidFilter` (exit 16) is the parent-reserved "first user" code;
this amendment is the first amendment to occupy it per parent RFC §Exit
Codes ("16: InvalidFilter (RFC-0011 Phase 1 first user; reserved range
starts at 17)"). `ReceiptNotFound` and `AuditReadFailed` are NEW variants
occupying 17 and 18 — the FIRST occupants of the parent-reserved range
17-63. Following amendments MUST NOT reuse 17-19 (parent RFC §Exit Codes
reservation rule). Note: `AuditResponseTooLarge` was removed per Wave 6.5
R2 H1 finding — substrate enforces `--limit` upper bound (10000) and
truncation surfaces `has_more: true` per §7.2; no overflow variant
needed at the CLI layer.

The variant sanitization pattern from parent RFC §Error Handling applies
verbatim: every variant's display string passes through
`sanitize_substrate_error` before display. The mapping is lossy on
purpose — the operator sees the category, not the substrate signature.

## Performance Targets

| Metric                | Target            | Notes                                                                                                 |
| --------------------- | ----------------- | ----------------------------------------------------------------------------------------------------- |
| `octo audit list` p95 | <100ms            | Canonical 1,000-receipt fixture; substrate-level index on `(subject_did, executed_at_unix DESC)`      |
| `octo audit show` p95 | <50ms             | Point lookup by primary key (`receipt_id`)                                                            |
| Output serialization  | <5ms              | `OutputEnvelope<T>` + serde_json (mirrors parent RFC §Performance Targets)                            |
| Redaction overhead    | <1ms per log line | Pattern matching is bounded; canonical receipt fields are redactor-clean by construction (§Redaction) |
| Memory                | <10 MiB           | Bounded by `--limit` (max 10,000 summaries × ~256B ≈ 2.5 MiB worst case)                              |

The CLI is operator-facing; throughput is not a primary concern. The
10,000 receipt cap (per `--limit` max) prevents accidental OOM on a busy
node with millions of historical receipts.

## Implicit Assumptions Audit

> **The "Nothing should be implied" rule (validation layer):** Every
> assumption the design relies on that is not enforced by types, runtime
> validation, or test coverage MUST be listed here.

| Assumption                                                     | Where Relied Upon                                | Blast Radius if False                                                                                                                     | Mitigation / Status                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------------------------- | ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Receipt store is append-only**                               | §Substrate                                       | If rows can be modified/deleted, the audit trail loses integrity (attacker erases evidence)                                               | Substrate `octo_settlement` enforces append-only per RFC-0959; CLI inherits; integration test asserts no DELETE on receipt store                                                                                                                                                                                                                       |
| **`executed_at_unix` is monotonic**                            | §Filters, §Substrate                             | Non-monotonic timestamps break `--since`/`--until` semantics and degrade `executed_at_unix DESC` index p95                                | Substrate uses system monotonic clock at write time per RFC-0959; integration test asserts monotonicity on canonical fixture                                                                                                                                                                                                                           |
| **Receipts contain no PII / secret material**                  | §Redaction                                       | Substrate bug emits a `reject_reason` containing a password → CLI surfaces it on stdout/stderr                                            | `OctoCliRedactor` value-pattern sweep catches `password=<value>` substrings per parent RFC §Redaction Layer dual-pass semantics; test vector `audit-show-reject-reason-with-pw-substr` asserts the sweep                                                                                                                                               |
| **Receipt store is local**                                     | §Substrate                                       | Remote store → every read is a network round-trip → <50ms / <100ms targets break                                                          | Parent RFC §Implicit Assumptions Audit "Operator config dir" row establishes local-first; substrate-side remote store is a future RFC                                                                                                                                                                                                                  |
| **`subject_did` is canonical RFC-0010 DID**                    | §Receipt Shape, §Filters                         | Non-canonical DID fails validation downstream (RFC-0965 §capability extension requires canonical DID)                                     | Substrate rejects non-canonical DIDs at capability creation per RFC-0965; audit surface only sees DIDs that have passed RFC-0965 validation                                                                                                                                                                                                            |
| **`capability_root` references exist in capability store**     | §Filters, §Substrate                             | Receipt references a pruned capability root → audit read still succeeds but join to capability state would orphan                         | Substrate `octo_cap_macaroon` retains capability metadata for audit window per RFC-0965 §Audit Window Caveat; this amendment does NOT depend on join capability state                                                                                                                                                                                  |
| **No-reject-hiding gate in ALL modes**                         | §Roles and Authorities, §Security Considerations | Compromised operator session in any mode could run `--status ok` (or `--status partial`) to hide rejected receipts from an auditor's view | Gate enforced at CLI dispatch: `--status ok\|partial` requires `--include-reject` OR `--confirm-acknowledge`; without either, exit 16 `InvalidFilter`. CI test asserts the gate fires in Human, Ci, Dev, AND Auditor modes per §Security Considerations row 2 (Wave 5 R1 finding: prior Auditor-mode-only scope left default `human` mode unprotected) |
| **`AuditFilter` semantics stable across substrate amendments** | §Filters, §Substrate entry #3                    | Substrate amendment changes `AuditFilter` field semantics → silently breaks CLI filter behavior                                           | `[ADD] AuditFilter` shape is substrate-truth; substrate amendments that change it MUST bump `octo_audit::AUDIT_FILTER_SCHEMA_VERSION`; CLI parses substrate-side schema version at startup                                                                                                                                                             |

## Security Considerations

1. **Read-only guarantee at substrate layer.** Audit subcommands MUST NOT mutate the receipt store in any mode. The `octo_audit` substrate exposes read-only APIs (`list_receipts`, `get_receipt`); the `octo_settlement` substrate owns write APIs that the CLI does NOT call. CI regression test asserts no INSERT/UPDATE SQL hit the receipt store during `octo audit {list,show}` invocation (per G1).

2. **No-reject-hiding gate in ALL modes.** An attacker who compromised the operator's CLI session could run `octo audit list --status ok` (or `--status partial`) to conceal failed capability consumptions from an auditor. The original Wave 5 R1 design scoped this defense to Auditor mode only, but that left default `human` mode unprotected: a compromised human-mode operator could run `--status ok` and hide reject rows before the auditor arrived. The corrected constraint applies in ALL modes (Human, Ci, Dev, Auditor): any `--status` value excluding `reject` requires either `--include-reject` (UNION semantics: forward reject rows into the result set so the operator still sees them) OR `--confirm-acknowledge` (explicit confirmation of the reject-hiding intent). Without either flag, the CLI exits 16 `InvalidFilter`. The CI test asserts the gate fires across all four modes. Auditor mode additionally enforces silent no-op on ALL `--status` values (`--status ok` / `--status partial` / `--status reject` are all treated as "all") for symmetric semantics per H5 finding — the Auditor MUST see every receipt regardless of which status filter is requested (audit-trail invariant).

3. **Receipt body disclosure.** The `audit show` command surfaces the full `SettlementReceipt`, which includes `prompt_hash` (a public blake3 digest, NOT the prompt content), `executed_by`, `cost_dqa`, and optionally `reject_reason`. None are secret material per the RFC-0959 canonical substrate contract; the redactor's value-pattern sweep provides defense in depth if a substrate bug ever emits secret-shaped content inside `reject_reason`. Operators who need to redact their prompt content must hash it client-side before submission; the audit surface never sees prompt content.

4. **Pseudonymous DIDs and public digests are not secret material.** `capability_root` (public blake3 per RFC-0965 §Capability Extension), `subject_did`, and `executed_by` (RFC-0010 canonical DIDs) are pseudonymous identifiers, not secrets. Surfacing them in `octo audit {list,show}` is required for the audit purpose ("who consumed what"); suppressing them would defeat the audit.

5. **No stdin secret surface.** Audit commands take no secret-shaped flags. The `--allow-stdin-secret` mechanism (parent §Redaction Layer) does NOT apply (no secret input to protect against). `StdinSecretRefused` (exit 15) is never emitted by audit commands.

6. **Substrate write-failure propagation.** Per §Adversary Analysis "Receipt store disk-full silent truncation" row, the audit surface MUST surface substrate write failures (e.g., `SettlementError::StoreFull` from RFC-0959 §Append-Only Contract) as `AuditReadFailed` (exit 18) — never as silent success or as the catch-all `Internal` (exit 64). The `#[from] octo_settlement::SettlementError` impl on `AuditError` (per §Substrate entry #4) propagates substrate errors verbatim; the CLI sanitizes the display string via `sanitize_substrate_error` per parent §Error Handling. CI integration test asserts that a write-failure fixture produces `AuditReadFailed` (exit 18), not silent success — preventing a substrate regression on the append-only contract from breaking the audit-trail integrity guarantee at the CLI surface. The failure mode (substrate silently truncating the append-only log) is substrate-engineering territory per RFC-0959; this CLI constraint is the failure-detection boundary that surfaces the substrate bug before audit evidence is lost.

## Adversarial Review

| Threat                                                                                                       | Impact                                                                               | Mitigation                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Attacker who compromises the operator CLI session runs `audit list --status ok` to conceal rejected receipts | High — auditor sees only successful receipts and misses failed activity              | Auditor mode constraint (§Security Considerations row 2): ALL `--status` values silently no-op'd in Auditor mode (symmetric semantics per H5); CI test asserts the constraint        |
| Substrate bug emits a `reject_reason` containing a `password=<value>` substring                              | Medium — secret leaks to stdout/stderr                                               | `OctoCliRedactor` value-pattern sweep catches `password=<value>` substrings regardless of field name per parent RFC §Redaction Layer dual-pass semantics                             |
| Operator runs `audit list --limit 1` to reduce noise, accidentally hiding most receipts                      | Low — operator UX confusion                                                          | `has_more: true` flag in `AuditListOutput` explicitly signals "more available"; CLI emits hint on stdout when `has_more == true` (parent RFC §Output Envelope pattern)               |
| Attacker exploits `--status` case-sensitivity mismatch between CLI and substrate                             | Low-Medium — operator sees empty result set instead of an error                      | `--status` is case-insensitive at the CLI parser; CLI normalizes to lowercase before substrate call; CI test asserts `OK` / `Ok` / `ok` all behave identically                       |
| Network or filesystem failure during substrate read                                                          | Medium — operator sees exit 64 instead of actionable error                           | `AuditReadFailed` (exit 18) carries a sanitized message naming the substrate layer; CI test asserts substrate `SettlementStore` errors map to `AuditReadFailed`, not `Internal`      |
| Receipt store grows unbounded over time                                                                      | Low — `--limit` cap protects CLI memory but operators cannot see historical receipts | Substrate-side retention policy is out of scope for this amendment (RFC-0959 owns the store); follow-on amendment may add `--since <duration>` default and retention-policy guidance |

## Adversary Analysis

> **The 5-Question Adversary Test:** For every design decision with security
> implications, enumerate: (1) who benefits, (2) what it costs them,
> (3) what they gain if successful, (4) what's our defense and its cost,
> (5) what's the residual risk.

### Reject-hiding filter (all modes)

1. **Who benefits?** — Attacker who has compromised the operator's CLI session (any mode) and runs `octo audit list --status ok` (or `--status partial`) before the auditor arrives; wants to conceal failed capability consumptions.
2. **What does it cost them?** — Crafting `octo audit list --status ok`. Trivial — the flag is documented in `--help`. The original Wave 5 R1 finding flagged that the prior defense (Auditor-mode-only constraint) left default `human` mode unprotected: a compromised human-mode operator could run `--status ok` and hide reject rows before the auditor arrived.
3. **What do they gain if successful?** — Auditor sees only successful receipts (or successful + partial); failed consumptions that should have triggered an investigation are hidden. Audit integrity compromised.
4. **What's our defense?** — Cross-mode reject-hiding gate. ANY `--status` value excluding `reject` requires either `--include-reject` (forward reject rows into the result set so the operator still sees them) OR `--confirm-acknowledge` (explicit confirmation of the reject-hiding intent). Without either flag, the CLI exits 16 `InvalidFilter`. Auditor mode additionally enforces silent no-op on ALL `--status` values (symmetric semantics per H5 finding — Auditor MUST see every receipt regardless of which status filter is requested). Verified by CI test across all four modes (Human, Ci, Dev, Auditor). Cost: operators who genuinely want a non-reject-only view must declare intent; this is the correct trade-off because the audit purpose is visibility, not convenience.
5. **Residual risk?** — An attacker who controls the operator's shell can still delete the receipt store directly (filesystem access) or pass `--confirm-acknowledge` programmatically (e.g., via shell script). OUT OF SCOPE for the CLI layer — the substrate owns the store integrity contract per RFC-0959. Programmatic `--confirm-acknowledge` requires shell access that already enables store deletion; ACCEPTED RISK per §Implicit Assumptions Audit.

### `reject_reason` substring exfiltration

1. **Who benefits?** — Substrate engineer or external attacker who can inject a `reject_reason` containing a `password=<value>` substring.
2. **What does it cost them?** — Crafting the substring. Moderate — requires write access to the receipt store (already a substrate-level compromise).
3. **What do they gain if successful?** — Secret leaks via CLI output, recoverable from shell history or terminal scrollback.
4. **What's our defense?** — `OctoCliRedactor` value-pattern sweep detects `password=<value>` substrings regardless of field name per parent §Redaction Layer dual-pass semantics. The redactor runs on every `tracing` event AND on the final stdout envelope before emission. Cost: redaction overhead is <1ms per log line (parent §Performance Targets), negligible.
5. **Residual risk?** — A substring that does NOT match any canonical redaction pattern (e.g., `pincode=1234` instead of `pin=1234`) would not be caught. ACCEPTED RISK: canonical patterns cover documented secret shapes; novel shapes are substrate-bug territory and surface as follow-on redactor extensions.

### Auditor `--confirm-acknowledge` programmatic bypass

1. **Who benefits?** — Compromised operator session that wishes to bypass the no-reject-hiding gate (Threat 1) without an interactive human in the loop.
2. **What does it cost them?** — Crafting `octo audit list --status ok --confirm-acknowledge` (or invoking via shell script / CI pipeline). Trivial — the flag is documented in `--help` and `--confirm-acknowledge` is a single CLI token.
3. **What do they gain if successful?** — The reject rows are excluded (no UNION), and the audit trail is hidden from any downstream consumer that does not separately query for rejects. This is the programmatic equivalent of an interactive `--confirm-acknowledge` press.
4. **What's our defense?** — The `--confirm-acknowledge` flag is an INTENT declaration, not a SECRET. Its cost is that the option appears in shell history (which is recorded) and in process listings (which are observable) — both of which are auditable. The substrate retains the receipt log independently (per RFC-0959 append-only contract); even if the operator never runs `--include-reject`, the receipts exist and can be queried through the HTTP proxy + Python SDK (RFC-0917) or direct substrate API. ACCT: CLI surfaces a structured warning to stderr every time `--confirm-acknowledge` is used (`WARN: --confirm-acknowledge flags reject-hiding filter; receipts remain queryable via RFC-0917 and substrate API`). Cost: one extra stderr line per invocation; negligible.
5. **Residual risk?** — A sufficiently motivated attacker script will both pass `--confirm-acknowledge` AND suppress stderr (e.g., `2>/dev/null`). The CLI cannot prevent that, but the receipt log is INDEPENDENT of the operator's view — the substrate still holds every row, and the audit amendment's threat model is "operator UX surface, not the audit log itself". The audit-log integrity guarantee is substrate-owned per RFC-0959; this amendment merely provides a structured operator view. ACCEPTED RISK: confirmed by §Security Considerations row 2 ("Attacker who controls the operator's shell can still delete the receipt store directly").

### Clock skew breaking monotonic ordering

1. **Who benefits?** — Attacker who can manipulate the wall clock on a settlement-emitting node (e.g., NTP drift, VM clock-rewind, manual `date -s`) to inject out-of-order `executed_at_unix` timestamps.
2. **What does it cost them?** — Compromising a node's clock source (NTP, hardware RTC, hypervisor clock). Moderate-to-high — depends on the operational surface; usually requires root on the node.
3. **What do they gain if successful?** — A receipt with `executed_at_unix` outside the canonical monotonic window can confuse `--since` / `--until` filter semantics (per §Implicit Assumptions Audit "executed_at_unix is monotonic" row): a timestamp that is set backwards could place a recent receipt OUTSIDE the operator's "last 7 days" window, effectively hiding it. Or set forwards to fall off the end of a bounded query.
4. **What's our defense?** — The substrate uses monotonic clock (`CLOCK_MONOTONIC` on Linux, or equivalent) at write time per RFC-0959; integration test asserts monotonicity on the canonical fixture. Wall-clock skew does NOT translate to monotonic-clock drift. Even if the system clock is rewound, the monotonic clock advances. CI test asserts that monotonic-clock values are non-decreasing under clock-rewind attack. Cost: requires RFC-0959 substrate guarantee; CLI inherits the substrate monotonic discipline and does not need to re-implement.
5. **Residual risk?** — A substrate that uses `CLOCK_REALTIME` instead of `CLOCK_MONOTONIC` would be vulnerable. ACCEPTED RISK: this is a substrate-engineering contract enforced at the RFC-0959 substrate layer; if a future substrate amendment changes the clock source, the §Implicit Assumptions Audit "executed_at_unix is monotonic" row breaks and must be re-validated. The CLI surface remains the same; the substrate integrity contract is what protects us.

### Receipt store disk-full silent truncation

1. **Who benefits?** — Operator whose host disk fills up mid-receipt-write; or an attacker who can fill the disk to provoke truncation behavior.
2. **What does it cost them?** — Filling the disk (operator: app-level growth; attacker: requires local or container write access). Moderate.
3. **What do they gain if successful?** — If the substrate silently truncates the append-only log on write failure, an attacker who can fill the disk before the next write can effectively erase historical evidence. Audit-trail integrity breaks.
4. **What's our defense?** — Per RFC-0959 §Append-Only Contract, the substrate MUST treat any write failure as a HARD ERROR and surface it via `SettlementError::StoreFull` (or equivalent); the substrate MUST NOT silently truncate or skip writes. The CLI's `AuditError::SettlementStore` (per §Substrate entry #4, `#[from] octo_settlement::SettlementError`) propagates that substrate error verbatim; the operator sees `exit 64 Internal` with the sanitized store-full message per §Error Handling. CLI MUST NOT swallow `StoreFull`. Cost: one substrate-side append-only enforcement + one CLI error-mapping test; trivial.
5. **Residual risk?** — A substrate bug that swallows the write failure (instead of propagating it) is the failure mode. ACCEPTED RISK: the substrate is the integrity guarantor; the CLI can only surface substrate errors it observes. CI integration test asserts that a write-failure fixture produces `AuditReadFailed` (exit 18), not silent success. If the substrate regresses on the append-only contract, the integration test fails at substrate-CI, not CLI-CI — the right layer boundary.

## Economic Analysis

Audit subcommands have NO direct economic surface. They do not create, transfer, or burn value; they expose substrate-receipt data for operator inspection. Economic implications (vault creation, settlement creation, governance vote, dual-stake model) land per parent §Economic Analysis ("This RFC defers any reference to the dual-stake model until those amendments"); this amendment inherits that deferral.

The receipt `cost_dqa` field is a DISPLAY field (RFC-0959 cost-dqa-migration wire form); it surfaces a historical cost without affecting the operator's economic state. Operators cannot transfer, refund, or recalculate the cost via this CLI surface — the audit amendment is read-only by construction.

## Compatibility

### Backward compatibility with parent RFC-0011

This amendment is ADDITIVE to parent RFC-0011 v1.0 across every contract boundary:

1. **No `schema_version` bump required.** Per parent RFC §Compatibility ("Adding a field to `OutputEnvelope<T>` or to a subcommand's data type is a non-breaking change"), the new `AuditListOutput` and `AuditShowOutput` are NEW data types and do NOT modify the parent's `OutputEnvelope` (already at `schema_version = 2` carrying the additive `preview_only` field per parent §Output Envelope). The parent's existing data types are unchanged.
2. **No new exit codes break parent semantics.** Parent RFC §Exit Codes reserves 17-63 for amendment additions; this amendment occupies 17 and 18 per §Error Handling (exit 19's `AuditResponseTooLarge` removed per Wave 6.5 R2 H1 — substrate `--limit` bound surfaces `has_more: true` instead). Parent's existing exit codes (0-16, 64-78, 100-127) are unchanged.
3. **No new clap variants break parent dispatch.** Adds one new variant to the parent's `Commands` enum (`Commands::Audit`) and a new dispatch module (`commands/audit.rs`); existing variants and modules are unchanged.
4. **No new redaction patterns required.** Per §Redaction, the canonical receipt shape is redactor-clean by construction. The `OctoCliRedactor` is reused verbatim from the parent.
5. **Confirmation flag matrix extends cleanly.** This amendment adds one row (audit list / show) with `(no flag)` in every column per §Confirmation gate matrix. No mode-specific confirmation logic is added.
6. **Stub command compatibility is unaffected.** Parent's stub deprecation schedule (v1.0 banner, v1.1 hard-error, v2.0 removal) is unchanged; this amendment introduces NO new stubs.

### Substrate compatibility

The audit substrate `octo-audit` is NEW (does not exist today). The `[ADD]` entries in §Substrate declare the canonical substrate API this amendment consumes. Substrate amendments that produce substrate-truth matching these signatures are filed separately per parent §Subcommand Taxonomy `[ADD]` convention.

The settlement substrate `octo_settlement` (RFC-0959) is the source of truth for `SettlementReceipt`; the audit substrate is a thin read-only projection. If RFC-0959 amends `SettlementReceipt`, the audit amendment amends in lockstep — the field types in §Receipt Shape MUST match the latest RFC-0959 substrate-truth.

## Test Vectors

The amendment defines the canonical set below (14 vectors (≥8 parent floor; amendment floor 12 satisfied)).

| ID                                        | Group        | Scenario                                                                                                                                                                                                                                                                                                                                                                                                                   | Expected                                                                                                                                                                           |
| ----------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `audit-list-empty`                        | `list`       | Receipt store contains zero receipts; no filters                                                                                                                                                                                                                                                                                                                                                                           | `OutputEnvelope<AuditListOutput> { receipts: [], total_matched: 0, has_more: false }`; exit 0                                                                                      |
| `audit-list-with-filter`                  | `list`       | 1,000 receipts; `--since 7d --model llama-3.1-8b --limit 10`                                                                                                                                                                                                                                                                                                                                                               | Up to 10 summaries matching filter; `total_matched` is count BEFORE `--limit` truncation; `has_more: true` if `total_matched > 10`; exit 0                                         |
| `audit-list-invalid-status`               | `list` (err) | `--status invalid`                                                                                                                                                                                                                                                                                                                                                                                                         | `OctoCliError::InvalidFilter("status: must be one of `{ok,partial,reject}`"); exit 16                                                                                              |
| `audit-list-bad-capability-root`          | `list` (err) | `--capability-root 01ab` (too short)                                                                                                                                                                                                                                                                                                                                                                                       | `OctoCliError::InvalidFilter("capability-root: expected 32 lowercase hex chars, got 4")`; exit 16                                                                                  |
| `audit-show-success`                      | `show`       | `octo audit show 01ab23cd45ef6789a1b2c3d4e5f60718`                                                                                                                                                                                                                                                                                                                                                                         | `OutputEnvelope<AuditShowOutput> { receipt: <full SettlementReceipt> }`; exit 0                                                                                                    |
| `audit-show-not-found`                    | `show`       | `octo audit show deadbeef00000000000000000000000001`                                                                                                                                                                                                                                                                                                                                                                       | `OctoCliError::ReceiptNotFound("deadbeef00000000000000000000000001")`; exit 17                                                                                                     |
| `audit-show-bad-id-format`                | `show` (err) | `octo audit show not-hex`                                                                                                                                                                                                                                                                                                                                                                                                  | `OctoCliError::InvalidFilter("receipt_id: expected 32 lowercase hex chars, got 7")`; exit 16                                                                                       |
| `audit-show-reject-reason-with-pw-substr` | `show`       | `reject_reason = "auth failed: password=hunter2 invalid"`; `audit show`                                                                                                                                                                                                                                                                                                                                                    | Receipt surfaces with `reject_reason` rendered; `password=hunter2` REPLACED with `[REDACTED:pw]` per `OctoCliRedactor` value-pattern sweep; exit 0                                 |
| `envelope-pretty-tty`                     | envelope     | TTY mode + no `--json`; `octo audit list`                                                                                                                                                                                                                                                                                                                                                                                  | Pretty-printed table (columns: `RECEIPT_ID`, `SUBJECT_DID`, `MODEL`, `EXECUTED_AT`, `STATUS`, `COST_DQA`); ANSI color when stdout is a TTY and `--no-color` is not set             |
| `envelope-json-pipe`                      | envelope     | Non-TTY (pipe to `cat`); `octo audit list --json`                                                                                                                                                                                                                                                                                                                                                                          | JSON via `serde_json::to_string_pretty`; `schema_version: 2`; `preview_only: false`; field order matches §Output Envelope                                                          |
| `auditor-mode-reject-unfiltered`          | envelope     | `octo --mode auditor audit list --status reject` on mixed fixture                                                                                                                                                                                                                                                                                                                                                          | Response is the FULL set (not reject-only); ALL `--status` values silently no-op'd in Auditor mode per §Security Considerations row 2 (symmetric semantics per H5 finding); exit 0 |
| `redaction-clean-canonical-fields`        | envelope     | Canonical fixture: `receipt_id = "01ab23cd45ef6789a1b2c3d4e5f60718"`, `subject_did = "did:octo:abc..."`, `capability_root = "01ab23cd45ef6789a1b2c3d4e5f60718"`, `model = "llama-3.1-8b"`, `prompt_hash = "01ab23cd45ef6789a1b2c3d4e5f60718"`, `cost_dqa = "0123456789abcdef0123456789abcdef"`, `executed_by = "did:octo:def..."`, `executed_at_unix = 1722470400`, `status = "ok"`, `reject_reason = None`; redactor runs | All 10 fields emitted verbatim (no redaction); redactor value-pattern sweep does NOT match canonical shapes per §Redaction                                                         |
| `include-reject-union-semantics`          | `list`       | Mixed fixture: 5 ok + 3 partial + 2 reject; `octo audit list --status ok --include-reject`                                                                                                                                                                                                                                                                                                                                 | Result set contains 5 ok + 2 reject (UNION: filter-result ∪ reject-rows); `total_matched = 7`; `has_more = false`; exit 0                                                          |
| `confirm-acknowledge-reject-hiding`       | `list`       | Mixed fixture: 5 ok + 3 partial + 2 reject; `octo audit list --status ok --confirm-acknowledge`                                                                                                                                                                                                                                                                                                                            | Result set contains 5 ok ONLY (reject rows excluded per operator's explicit confirmation); `total_matched = 5`; `has_more = false`; exit 0                                         |

Mission-level tests will add the running tally per parent RFC §Test Vectors pattern. Canonical fixture: `docs/07-developers/octo-cli-implementation-guide.md` §Audit Subcommand Fixtures (prerequisite per §Implementation Phases Phase 1 per M12 finding).

## Alternatives Considered

| Approach                                                     | Pros                                                          | Cons                                                                                                                                                                                                                                       |
| ------------------------------------------------------------ | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **REST API on settlement substrate**                         | Standard tooling (curl, Postman); trivially scriptable        | Loses operator UX (auto-completion, hint messages, TTY-aware pretty-print); HTTP proxy + Python SDK (RFC-0917) already covers programmatic access; duplicate surface                                                                       |
| **gRPC endpoint with reflection**                            | Strong typing; streaming for large result sets                | Operator UX is worse (no shell-completable commands); heavier dep footprint (tonic + protobuf) for two read commands; no precedent in parent RFC-0011                                                                                      |
| **SQL pass-through (`octo audit query "SELECT ...")`)**      | Maximum operator flexibility; full substrate surface          | Loses typed filter set; reintroduces substrate schema as CLI contract (defeats typed-discriminator principle per CLAUDE.md §Architectural Principles); `DELETE` SQL injection surface; not aligned with parent RFC's typed-output envelope |
| **Mutating audit operations (`octo audit {redact,export}`)** | Operator can clean up PII or export to long-term storage      | Requires new substrate write APIs (RFC-0959 would need new error variants and a new confirmation-gate row); requires redactor changes; out of scope for this amendment; deferred to a follow-on amendment                                  |
| **Receipt streaming over WebSocket**                         | Real-time audit visibility (operators see receipts as minted) | Requires a new substrate transport; pushes CLI toward long-running process model (defeats per-invocation operator UX); out of scope for this amendment; deferred to a follow-on amendment                                                  |

The chosen approach (typed read-only CLI commands with the parent RFC's
output envelope) maximizes alignment with the parent RFC substrate while
delivering the operator visibility this amendment motivates. REST and gRPC
are explicitly DEFERRED — the HTTP proxy + Python SDK (RFC-0917) already
covers programmatic access; the CLI is the operator UX layer, not the
programmatic API layer.

## Implementation Phases

This amendment is ATOMIC — both subcommands land together in a single
RFC cycle. The substrate amendments that produce the `[ADD]` surface
(per §Substrate) are filed separately and land alongside this amendment
via substrate-side RFCs / amendments. There is no Phase 2 for this
amendment; follow-on amendments (reputation, agent lifecycle, etc.)
land per parent RFC §Implementation Phases §Phase 3+.

### Phase 1 (this amendment)

- [ ] `octo audit list` + `octo audit show <receipt_id>` (read-only)
- [ ] `crates/octo-cli/src/commands/audit.rs` (NEW dispatch module)
- [ ] `OctoCliError::ReceiptNotFound`, `AuditReadFailed` variants (exit 17/18)
- [ ] CLI-side projection types (`AuditFilter`, `ReceiptSummary`, `AuditShowOutput`, `SettlementReceipt`, `ReceiptStatus`)
- [ ] Test vectors per §Test Vectors (14 vectors, 8-floor satisfied)
- [ ] Substrate `[ADD]` surface (filed separately, lands alongside)
- [ ] Mission(s) to claim the implementation work
- [ ] Pre-commit cite validation (parent RFC-0011 + 4 substrate RFCs: RFC-0959, RFC-0965, RFC-0010, RFC-0008)
- [ ] CI regression: G1 (no receipt-store mutation), G5 (cross-mode consistency)
- [ ] CLIPPY zero warnings; `cargo fmt --all` clean; Prettier formatting

**Prerequisite:** `docs/07-developers/octo-cli-implementation-guide.md`
§Audit Subcommand Fixtures MUST exist before Phase 1 implementation begins
(per M12 finding). The fixtures define the canonical mixed-fixture shape
used by test vectors `include-reject-union-semantics` and
`confirm-acknowledge-reject-hiding` (5 ok + 3 partial + 2 reject). If the
doc stub is absent at implementation kickoff, the substrate implementation
mission MUST create it as the first task.

### Out of scope for this amendment

- `octo audit {redact,export,watch}` — mutating / long-running; deferred
- `--format csv`, `--fields <list>` — deferred (default full projection is canonical)
- Pagination cursors (`--cursor <hex>`); this amendment uses `--limit` +
  `has_more` + filter-narrow per the parent RFC §Subcommand Taxonomy convention
- Multi-status combinator (`--status ok,reject`); substrate-truth is
  single-valued; deferred until substrate amendment adds `Vec<ReceiptStatus>` to `AuditFilter`

## Mission Decomposition

This RFC is **atomic** — a single mission YAML files the entire implementation
surface (both `octo audit list` and `octo audit show` subcommands plus their
`[ADD]` substrate surface and `OctoCliError` exit-code extensions land
together in a single RFC cycle per §Implementation Phases §Phase 1):

- **Mission:** `missions/claimed/0011-a-audit-commands.md` (NEW; RFC-Accept
  gated per `docs/BLUEPRINT.md` §RFC Acceptance Process; atomicity statement
  per §Implementation Phases §Phase 1).
- **Substrate amendments:** filed separately per parent RFC §Subcommand
  Taxonomy `[ADD]` convention and the §Substrate-truth disclaimer
  (substrate amendments that produce `[ADD]` signatures matching §Substrate
  entries #1-#6 are out of scope for THIS RFC; companion missions are filed
  alongside per the `[ADD]` pattern).

**Decomposition threshold acknowledgment:** Per
`docs/BLUEPRINT.md` §Multi-Mission Decomposition, missions exceeding the
canonical size budget (~1000 LoC of substrate change) MUST be decomposed into
child missions. This RFC's atomic mission is well within budget: total
substrate delta is `crates/octo-audit/` (new crate, ~600 LoC estimate) +
`crates/octo-cli/src/commands/audit.rs` (new module, ~300 LoC estimate) +
4-line `OctoCliError` extension (2 new variants: `ReceiptNotFound`,
`AuditReadFailed`) + 14 test vectors per §Test Vectors. No decomposition
required at this RFC's scope.

## Key Files to Modify

### DOC-ONLY (this RFC cycle)

- `rfcs/draft/process/0011-a-audit-subcommands.md` — this file (Draft)
- `docs/07-developers/octo-cli-implementation-guide.md` — add
  §Audit Subcommand Fixtures + §Audit Substrate `[ADD]` Mapping
- `missions/claimed/0011-a-audit-commands.md` — NEW mission (RFC-Accept
  gated per BLUEPRINT.md §RFC Acceptance Process)

### SUBSTRATE (follow-on substrate amendments, NOT this RFC cycle)

- `crates/octo-cli/Cargo.toml` — add dep `octo-audit` (Layer C substrate)
- `crates/octo-cli/src/commands/mod.rs` — register `Audit` subcommand
- `crates/octo-cli/src/commands/audit.rs` — NEW, audit subcommand impls
- `crates/octo-cli/src/error.rs` — add `ReceiptNotFound`,
  `AuditReadFailed` variants
- `crates/octo-audit/` — NEW Layer C substrate crate (per parent RFC
  `[ADD]` pattern); exposes `list_receipts`, `get_receipt`, `AuditFilter`,
  `AuditError`, `ReceiptId`, `audit_home` per §Substrate
- `crates/octo-audit/Cargo.toml` — depends on `octo-settlement` (Layer B
  per RFC-0959) for the `SettlementReceipt` projection; `octo-audit` re-exports
  `pub use octo_settlement::SettlementReceipt;` so the CLI consumes
  `octo_audit::SettlementReceipt` (Layer C → Layer C) rather than reaching
  directly into Layer B (per H6 finding)

### Substrate-truth dependency

The audit substrate depends on the settlement substrate (`octo-settlement`
per RFC-0959). The audit substrate MUST NOT introduce new persistence —
every read translates to a substrate call against
`octo_settlement::ReceiptStore`. The settlement substrate's
`SettlementReceipt` is the source of truth; the audit substrate's
`AuditShowOutput.receipt` is a CLI-friendly projection (DID rendering
per RFC-0010 + Hex32 newtype per parent RFC §Hex32 + cost_dqa String
per RFC-0959 cost-dqa-migration).

## Future Work

- `octo audit {redact,export,watch}` — mutating / long-running; deferred until concrete requirements surface (current audit surface is redactor-clean by RFC-0959 substrate contract per §Redaction)
- `--format csv`, `--fields <list>` — deferred (parent RFC's JSON envelope is the canonical machine-readable format; default full projection is canonical per §Output Envelope)
- Multi-status combinator (`--status ok,reject`) — substrate-truth is single-valued; deferred until substrate amendment adds `Vec<ReceiptStatus>` to `AuditFilter`
- Pagination cursors (`--cursor <hex>`) — current approach is `--limit` + `has_more` + filter-narrow; deferred until substrate amendment adds cursor support to `AuditFilter`
- RFC-0011-b — reputation subcommands amendment
- RFC-0011-c — agent lifecycle subcommands amendment
- RFC-0011-d — role provisioning subcommands amendment
- RFC-0011-e — vault operations subcommands amendment
- RFC-0011-f — mesh operations subcommands amendment
- RFC-0011-g — governance subcommands amendment

## Rationale

### Why a separate amendment RFC (not folded into parent RFC-0011)

Per parent §Implementation Phases, audit subcommands are explicitly deferred to a Phase 2 amendment. Folding audit into the parent RFC would have (1) exceeded the 1000-line decomposition threshold in `docs/BLUEPRINT.md` §Multi-Mission Decomposition, (2) coupled the audit substrate to the parent RFC's review cycle (RFC-0959 + RFC-0965 each have their own review history), and (3) blocked the parent RFC's promotion on audit substrate readiness — the wrong dependency direction. The amendment chain (parent RFC + RFC-0011-a + ...) is the canonical CipherOcto extension mechanism per the Status header amendment chain convention.

### Why extension over central enum (per CLAUDE.md §Architectural Principles)

The audit subcommands use typed discriminators (`ReceiptStatus` enum re-exported from `octo_settlement` per M5 finding, the `--status <ok|partial|reject>` flag pattern, the RFC-0010 canonical `Did` newtype, and the parent's `Hex32` newtype), NOT a central string-or-enum that other subcommands also carry. This follows CLAUDE.md §Architectural Principles: "For types with infinite extension surface, use typed-discriminator + Raw escape hatch, not central enums."

Specifically:

- `ReceiptStatus` is the CLOSURE of the audit receipt projection (owned by the settlement substrate per RFC-0959; CLI re-exports via `pub use octo_audit::ReceiptStatus;` per §Output Envelope Wave 3 H1 fix — Layer C → Layer C re-export boundary). New statuses (e.g., `TimedOut`, `RateLimited`) are added via substrate-side amendments (RFC-0959 §Receipt Status); the CLI bumps its `AuditShowOutput.receipt.status` field shape with a new enum variant. No "raw string" fallback for unknown statuses (substrate amendment is the canonical path).
- `subject_did` is RFC-0010 canonical; parsed via parent's `octo_wallet::Did` newtype pattern.
- `capability_root` is a 32-byte blake3 digest; parsed via parent's `Hex32` newtype pattern.
- `cost_dqa` is RFC-0959 cost-dqa-migration wire form; rendered as a String for forward compatibility with substrate schema changes.

A new RFC that adds a new `ReceiptStatus` variant requires NO edits to the parent RFC, the audit amendment, the CLI clap derive, or any sibling amendment — only the substrate amendment that adds the variant and the CLI surface that needs to render it.

### Why Layer C/D placement

Per CLAUDE.md §Architectural Principles (Layer Stability table):

- `octo-audit` is a Layer C specialized substrate (one per substrate dependency). It pulls in Layer B substrate (`octo-settlement` per RFC-0959) and re-exports `SettlementReceipt` so the CLI consumes `octo_audit::SettlementReceipt` (Layer C → Layer C) per H6 finding. It does NOT modify Layer A or Layer B.
- The CLI (`crates/octo-cli`) is a Layer C orchestrator per parent RFC §System Architecture. The audit subcommands add NO new Layer A or Layer B types; they only add CLI-side projections (per §Output Envelope) that are pure Layer C operator UX. The CLI MUST NOT `pub use octo_settlement::*` directly — it MUST route through `octo_audit` per the Layer C → Layer C boundary (no CLI → Layer B direct dep).
- The audit substrate is read-only; it owns NO new persistence. The Layer B substrate (RFC-0959) owns the receipt store. This is the canonical Layer C → Layer B dependency direction.

### Why no `--confirm` for audit commands

Audit subcommands are READ-ONLY by construction (per §Security Considerations row 1, G1, and the §Substrate `[ADD]` surface — every substrate function returns `Result<_, AuditError>` with no mutation). The parent §Confirmation Flag Matrix establishes that confirmation flags are required for MUTATING commands only. Adding `--confirm` to read-only audit commands would be operator UX friction with no security gain. The Auditor mode constraint (no reject-hiding filter per §Security Considerations row 2) is enforced at CLI dispatch, NOT via the confirmation flag mechanism — the correct boundary: read access is unconditional, but Auditor-mode visibility is constrained to prevent evidence concealment.

## Version History

| Version | Date       | Status   | Changes                                                                                                                                                                     |
| ------- | ---------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-08-31 | Draft    | Initial draft (audit subcommand amendment to parent RFC-0011 per Status header amendment chain). Full BLUEPRINT template.                                                   |
| 1.1     | 2026-08-31 | Draft    | v1.1 — Wave 3.5 cross-mode reject-hiding gate + audit_home helper                                                                                                           |
| 1.2     | 2026-08-31 | Draft    | v1.2 — Wave 3.5 C1/H/M-series surgical fixes (13 total)<br>13 surgical fixes: 1 CRITICAL (ReceiptStatus re-export), 6 HIGH (H1/H3/H4/H5/H6/H7), 6 MEDIUM (M-series grouped) |
| 1.3     | 2026-08-31 | Draft    | Wave 4.5: 2 MED + 3 LOW — VH row drift, TV expected-value re-paste, L243 fragment, L273 truncation, L603 severity                                                           |
| 1.4     | 2026-08-31 | Accepted | Promoted Draft → Accepted after W1-W6.5 multi-round adversarial review loop + DRY closure (W5+W6 zero-finding) + 28 cite hygiene fixes                                      |

## Related RFCs

- RFC-0011: `octo` CLI Substrate — parent substrate contracts (output
  envelope, redaction, error envelope, exit codes, confirmation gate
  matrix, TTY-aware rendering, stub command compatibility)
- RFC-0965: Capability Extension Format — receipt `capability_root`
  shape (RFC-0965 §capability extension)
- RFC-0965: Payment Caveat Asset Binding — receipt cost field references
  the asset-bound `Dqa` wire form
- RFC-0959: Ask Settlement Chain — primary producer of audit receipts
- RFC-0959: Settlement Cost DQA Migration — receipt `cost_dqa` field
  wire form
- RFC-0959: Burn Event Wire Form — receipt burn-event linkage
- RFC-0959: Market Delivery — market-delivery receipts are a subset
- RFC-0010: Canonical DID Codec — DID rendering in audit output
- RFC-0008: Deterministic AI Execution Boundary — execution class
  mapping for the new operations
- RFC-0917: HTTP Proxy + Python SDK — programmatic counterpart to this
  CLI amendment (out of scope but related)

## Related Use Cases

- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — the audit surface
  is the canonical operator visibility into runtime settlement activity

## Appendices

### Appendix A: clap tree fragment

```rust
//! crates/octo-cli/src/commands/audit.rs (fragment; full module per §Key Files to Modify)

#[derive(Parser, Debug)]
pub struct Audit { #[command(subcommand)] pub action: AuditAction }

#[derive(Subcommand, Debug)]
pub enum AuditAction {
    /// List settlement receipts with optional filters.
    List(ListArgs),
    /// Show one receipt in full detail.
    Show(ShowArgs),
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    /// Filter: receipts executed since <duration> (e.g., 7d, 24h, 3600s) or <unix-seconds>.
    #[arg(long, value_name = "DURATION|UNIX")] pub since: Option<String>,
    /// Filter: receipts executed before <unix-seconds> (inclusive).
    #[arg(long, value_name = "UNIX")] pub until: Option<u64>,
    /// Filter: receipts whose capability_root matches <32-lowercase-hex>.
    #[arg(long, value_name = "HEX32")] pub capability_root: Option<String>,
    /// Filter: receipts whose model reference matches <ref>.
    #[arg(long, value_name = "REF")] pub model: Option<String>,
    /// Filter: receipts with status <ok|partial|reject> (case-insensitive).
    #[arg(long, value_name = "STATUS", value_parser = parse_status)] pub status: Option<String>,
    /// Forward reject rows into result set (UNION semantics):
    /// `result_set = filter-result ∪ reject-rows`. Requires --status.
    #[arg(long, requires = "status")] pub include_reject: bool,
    /// Explicit confirmation of reject-hiding intent. Requires --status.
    /// Without --include-reject or --confirm-acknowledge, --status ok|partial
    /// exits 16 InvalidFilter (no-reject-hiding gate per §Roles and Authorities).
    #[arg(long, requires = "status")] pub confirm_acknowledge: bool,
    /// Max receipts to return (1..=10000; default 100).
    #[arg(long, value_name = "NAT", default_value_t = 100)] pub limit: usize,
    /// Force JSON output regardless of TTY.
    #[arg(long)] pub json: bool,
    /// Disable ANSI color output.
    #[arg(long)] pub no_color: bool,
}

#[derive(Parser, Debug)]
pub struct ShowArgs {
    /// The receipt ID to show (32 lowercase hex chars).
    #[arg(value_name = "RECEIPT_ID")] pub receipt_id: String,
    /// Force JSON output regardless of TTY.
    #[arg(long)] pub json: bool,
    /// Disable ANSI color output.
    #[arg(long)] pub no_color: bool,
}

/// Parse `--status` flag value. Returns `clap::Error` with
/// `ErrorKind::ValueValidation` on rejection; the dispatch layer maps
/// this to `OctoCliError::InvalidFilter` (exit 16) per §Error Handling,
/// ensuring scripting consumers see a domain error (exit 16), NOT
/// clap's default exit-2 usage error. The Raw `clap::Error::raw` carries
/// the same message string the dispatch layer surfaces verbatim.
fn parse_status(s: &str) -> Result<String, clap::Error> {
    match s.to_ascii_lowercase().as_str() {
        "ok" | "partial" | "reject" => Ok(s.to_ascii_lowercase()),
        other => Err(clap::Error::raw(
            clap::error::ErrorKind::ValueValidation,
            format!("status: must be one of ok|partial|reject, got {other}"),
        )),
    }
}
```

The full module is in `crates/octo-cli/src/commands/audit.rs` per
§Key Files to Modify. The fragment above shows the canonical clap
structure; the full module adds dispatch (`AuditAction::List` →
`list_receipts`, `AuditAction::Show` → `get_receipt`), error mapping
(§Error Handling), and output envelope rendering (§Output Envelope).

### Appendix B: JSON output schemas

The `AuditListOutput`, `AuditShowOutput`, `ReceiptSummary`, and
`SettlementReceipt` types each derive `Serialize, Deserialize,
schemars::JsonSchema` per parent RFC §Appendices B. Schemas are
published to `docs/schemas/octo-cli/audit-list.json` and
`docs/schemas/octo-cli/audit-show.json` at build time.

Canonical schema fragment for `AuditListOutput`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "AuditListOutput",
  "type": "object",
  "required": ["receipts", "total_matched", "has_more"],
  "properties": {
    "receipts": {
      "type": "array",
      "items": { "$ref": "ReceiptSummary.json" }
    },
    "total_matched": {
      "type": "integer",
      "minimum": 0,
      "description": "Total receipts matching the filter, BEFORE --limit truncation."
    },
    "has_more": {
      "type": "boolean",
      "description": "True when total_matched exceeds the returned receipt count (filter narrow to see more)."
    }
  }
}
```

Canonical schema fragment for `AuditShowOutput`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "AuditShowOutput",
  "type": "object",
  "required": ["receipt"],
  "properties": {
    "receipt": { "$ref": "SettlementReceipt.json" }
  }
}
```

Canonical schema fragment for `OutputEnvelope<T>`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "OutputEnvelope",
  "type": "object",
  "required": [
    "schema_version",
    "generated_at",
    "data",
    "exit_code",
    "preview_only"
  ],
  "properties": {
    "schema_version": {
      "type": "integer",
      "const": 2,
      "description": "Parent RFC §Output Envelope; audit commands emit schema_version=2 (parent-bumped value carrying the additive `preview_only` field per parent §Compatibility)"
    },
    "generated_at": {
      "type": "string",
      "format": "date-time",
      "description": "RFC-3339 timestamp of envelope emission"
    },
    "data": {
      "description": "Subcommand-specific output (AuditListOutput for `octo audit list`, AuditShowOutput for `octo audit show`); concrete schema varies per subcommand"
    },
    "exit_code": {
      "type": "integer",
      "description": "Mirrors the process exit code; surface for scripting consumers that prefer JSON over process exit-status inspection"
    },
    "preview_only": {
      "type": "boolean",
      "const": false,
      "description": "Always `false` for audit commands (no --dry-run surface per §Subcommand Taxonomy)"
    }
  }
}
```

Canonical schema fragment for `ReceiptSummary`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "ReceiptSummary",
  "type": "object",
  "required": [
    "receipt_id",
    "subject_did",
    "capability_root",
    "model",
    "executed_at_unix",
    "status",
    "cost_dqa"
  ],
  "properties": {
    "receipt_id": {
      "type": "string",
      "pattern": "^[0-9a-f]{32}$",
      "description": "Canonical 32-byte blake3 digest of the receipt body; lowercase hex per parent §Hex32 newtype"
    },
    "subject_did": {
      "type": "string",
      "pattern": "^did:octo:z[a-zA-HJ-NP-Z1-9]{43,44}$",
      "description": "RFC-0010 canonical DID codec form (base58btc wire form); pseudonymous identifier, not secret material per §Security Considerations row 4"
    },
    "capability_root": {
      "type": "string",
      "pattern": "^[0-9a-f]{32}$",
      "description": "The root ID of the consumed capability; pseudonymous identifier per RFC-0965 §Capability Extension, not secret material per §Security Considerations row 4"
    },
    "model": {
      "type": "string",
      "minLength": 1,
      "description": "Free-form model reference; CLI renders canonical `{namespace}/{family}@{version}` form per RFC-0959 `ModelRef` wire form (F-4 rendering rule)"
    },
    "executed_at_unix": {
      "type": "integer",
      "minimum": 0,
      "description": "Canonical unix seconds (u64; non-negative)."
    },
    "status": {
      "type": "string",
      "enum": ["ok", "partial", "reject"],
      "description": "Settlement outcome. Unknown variants (substrate additions like TimedOut, RateLimited) are added via substrate-side amendments per §Output Envelope Raw escape hatch pattern (CLAUDE.md §Architectural Principles: typed-discriminator + Raw escape hatch, not central enum); CLI MUST fail-closed on unknown substrate variants until renderer updated."
    },
    "cost_dqa": {
      "type": "string",
      "pattern": "^[0-9a-f]{32}$",
      "description": "RFC-0959 Dqa canonical 16-byte hex wire form (32 lowercase hex chars)"
    }
  }
}
```

Canonical schema fragment for `SettlementReceipt`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "SettlementReceipt",
  "type": "object",
  "required": [
    "receipt_id",
    "subject_did",
    "capability_root",
    "model",
    "prompt_hash",
    "cost_dqa",
    "executed_by",
    "executed_at_unix",
    "status"
  ],
  "properties": {
    "receipt_id": {
      "type": "string",
      "pattern": "^[0-9a-f]{32}$",
      "description": "Canonical 32-byte blake3 digest of the receipt body; lowercase hex per parent §Hex32 newtype"
    },
    "subject_did": {
      "type": "string",
      "pattern": "^did:octo:z[a-zA-HJ-NP-Z1-9]{43,44}$"
    },
    "capability_root": { "type": "string", "pattern": "^[0-9a-f]{32}$" },
    "model": { "type": "string", "minLength": 1 },
    "prompt_hash": {
      "type": "string",
      "pattern": "^[0-9a-f]{32}$",
      "description": "blake3 digest of input prompt; PUBLIC digest (NOT prompt content); safe to log per §Redaction and §Implicit Assumptions Audit 'Receipts contain no PII'"
    },
    "cost_dqa": {
      "type": "string",
      "pattern": "^[0-9a-f]{32}$",
      "description": "RFC-0959 Dqa canonical 16-byte hex wire form (32 lowercase hex chars)"
    },
    "executed_by": {
      "type": "string",
      "pattern": "^did:octo:z[a-zA-HJ-NP-Z1-9]{43,44}$"
    },
    "executed_at_unix": {
      "type": "integer",
      "minimum": 0,
      "description": "Canonical unix seconds (u64; non-negative)."
    },
    "status": {
      "type": "string",
      "enum": ["ok", "partial", "reject"],
      "description": "Settlement outcome. Unknown variants (substrate additions like TimedOut, RateLimited) are added via substrate-side amendments per §Output Envelope Raw escape hatch pattern (CLAUDE.md §Architectural Principles: typed-discriminator + Raw escape hatch, not central enum); CLI MUST fail-closed on unknown substrate variants until renderer updated."
    },
    "reject_reason": { "type": "string" }
  }
}
```

The `reject_reason` field is OPTIONAL (`required` array does not include
it); it is present only when `status == "reject"`.

### Appendix C: filter grammar

The filter grammar is fully specified in §Filters. The CLI parser
(`crates/octo-cli/src/commands/audit.rs` `parse_list_args`) produces an
`AuditFilter` struct on success or an `OctoCliError::InvalidFilter` on
failure (mapped to exit 16). EBNF grammar is in §Filters.

### Appendix D: error → exit code table

| Exit code | `OctoCliError` variant | Trigger                                                                                                                                                                  |
| --------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 0         | (success)              | Command succeeded                                                                                                                                                        |
| 16        | `InvalidFilter`        | `--capability-root` not 32 hex; `--status` not in `{ok,partial,reject}`; `--limit` out of range; `--since`/`--until` malformed; `since > until`; `receipt_id` not 32 hex |
| 17        | `ReceiptNotFound`      | `octo audit show <id>` and no receipt with that ID exists                                                                                                                |
| 18        | `AuditReadFailed`      | Substrate-level read error (store locked, schema mismatch, integrity check failure)                                                                                      |
| 64        | `Internal`             | Catch-all substrate error (sanitized via `sanitize_substrate_error`)                                                                                                     |
| 100-127   | (env errors)           | Missing config dir, permission mismatch, etc. (parent RFC reservation; not used by this amendment)                                                                       |

Codes 17-63 are reserved per parent RFC §Exit Codes ("17-63 reserved per
Status header amendment chain"). This amendment is the FIRST occupant
of 17 and 18. Following amendments MUST NOT reuse 17-19 per the
parent reservation rule. Codes 19-63 remain available for
RFC-0011-b/c/d/e/f/g.

---

**Submission Date:** 2026-08-31
**Acceptance Date:** 2026-08-31
**Last Updated:** 2026-08-31
**Changes:**

- 2026-08-31 — Promoted Draft → Accepted per BLUEPRINT.md §RFC Acceptance Process (file moved to `rfcs/accepted/process/` via cp+rm for untracked source; Status header updated to Accepted; VH row v1.4 appended documenting W1-W6.5 multi-round adversarial review loop + DRY closure; Authorship Note placeholder stripped per BLUEPRINT §RFC Process; cite hygiene sweep PASS). Review cycle satisfied: W1-W6.5 multi-round review loop with W5 + W6 = 2 consecutive zero-finding rounds → DRY closure; 28 cite hygiene fixes applied.
