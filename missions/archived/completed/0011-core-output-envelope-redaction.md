---
name: 0011-core-output-envelope-redaction
description: Land CLI substrate (output envelope, error envelope, redaction layer, clap root) per RFC-0011
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0009
    - RFC-0957
    - RFC-0964
    - RFC-0008
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
completed_at: 2026-08-28
completed_by: mmacedoeu
landing_commit: bc228500
spec_cycle_dry_closed: 2026-08-28
review_dry_closed: 2026-08-28
---

# 0011-core-output-envelope-redaction — CLI substrate (output + error + redaction + clap root)

**Status:** Completed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28. Implementation review loop R1-R32 closed 2026-08-28 (R31+R32 = 2 consecutive zero-finding rounds). User owns push/PR/promotion per [[feedback_initiation_user_only]] + [[git-workflow]].
**Landing commit:** `bc228500 feat(octo-cli): M1+M5 substrate — OutputEnvelope + OctoCliError + OctoCliRedactor + clap root + deprecation stubs` (12 files, +1233 lines)
**Substrate:** RFC-0011 §Output Envelope (Output Envelope), §Redaction Layer (Redaction Layer), §Error Handling (Error Handling), §Binary Surface (Binary Surface)
**Parent:** RFC-0011
**Depends on:**

- RFC-0011: own substrate — this RFC's clap derive + output envelope + redaction layer
- RFC-0009: Identity substrate — `WalletStore` / `IdentityKey` consumed by identity commands
- RFC-0957: Macaroon substrate — `CapabilityToken` consumed by capability commands
- RFC-0965: Caveat Envelope canonical caveat JSON shape
- RFC-0008: Execution Class operator class mapping for CLI commands
  **Blocks:** `0011-identity-commands`, `0011-capability-commands`, `0011-policy-commands`

## Status

Completed 2026-08-28 (mmacedoeu) — landed `bc228500`. Spec cycle DRY-closed 2026-08-28; review loop R1-R32 closed 2026-08-28.

## RFC

RFC-0011 §Binary Surface, §Output Envelope, §Redaction Layer, §Error Handling (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [x] `OutputEnvelope<T>` implemented + unit-tested (TV-ENV1..5 pass)
- [x] `OctoCliError` 19-variant enum implemented + unit-tested (TV-ERR1..5 pass)
- [x] `OctoCliRedactor` implemented + unit-tested (TV-RED1..8 pass)
- [x] Clap root struct implemented + unit-tested
- [x] Stub deprecation banners implemented + unit-tested (TV-DEP1..2)
- [x] Cross-mission AC: mission's output is the substrate that identity/capability/policy missions build on
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [x] Cargo test --workspace --lib green
- [x] No new INVALID cites introduced (Guard 2 cite validator 84/84 PASS)

### Type Coverage

| RFC-0011 type       | Sub-step   | Notes                                                                                             |
| ------------------- | ---------- | ------------------------------------------------------------------------------------------------- |
| `OutputEnvelope<T>` | Sub-step 1 | Layer C/D; carries `schema_version: u32 = 2`, `generated_at`, `data`, `exit_code`, `preview_only` |
| `OctoCliError`      | Sub-step 2 | Layer C/D; 19-variant `thiserror` enum; exit code per variant                                     |
| `OctoCliRedactor`   | Sub-step 3 | Layer C/D; `tracing_subscriber::Layer` impl + `redact_string` helper + field-name redaction table |
| Clap root struct    | Sub-step 4 | Layer C/D; `Octo { output, mode, command }` per RFC-0011 §Binary Surface                          |

### Sub-steps

1. **`OutputEnvelope<T>`** — `crates/octo-cli/src/output.rs` (Layer C/D per [[cipherocto-design-principles]]; substrate reference RFC-0011 §Output Envelope). `#[derive(Serialize, Deserialize, Debug, Clone)]` generic struct with `schema_version: u32 = 2`, `generated_at: DateTime<Utc>` (RFC 3339 with `Z`), `data: T`, `exit_code: i32`, `preview_only: bool`. TTY-aware renderer via `std::io::IsTerminal`.

2. **`OctoCliError`** — `crates/octo-cli/src/error.rs` (Layer C/D; substrate reference RFC-0011 §Error Handling). 19-variant `thiserror` enum (full list at impl-guide §OctoCliError). Each variant maps to fixed exit code per §Exit Code table; `render(force_json)` writes to stderr + exits.

3. **`OctoCliRedactor`** — `crates/octo-cli/src/redact.rs` (Layer C/D; substrate reference RFC-0011 §Redaction Layer). `tracing_subscriber::Layer` impl + `redact_string(s)` helper + `redact_by_field(name, value)` table. Field-name redactor covers 11 names (seed/key/sig/pair/password/bearer/mnemonic/passphrase/pin/api_key/secret); value-pattern redactor covers 8 standalone patterns.

4. **Clap root struct** — `crates/octo-cli/src/main.rs` (Layer C/D; substrate reference RFC-0011 §Binary Surface). REPLACE 213-line stub with full `Octo { output: OutputFlags, mode: OperatorModeFlags, command: Commands }`. `OutputFlags` (--json, --no-color) + `OperatorModeFlags` (--mode, --allow-write, --confirm, --dry-run, --allow-stdin-secret).

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §CLI Substrate for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

This is the substrate mission; every later identity/capability/policy mission depends on the `OutputEnvelope<T>` + `OctoCliError` + clap root that this mission lands.

## Scope

Land the CLI substrate that every identity / capability / policy subcommand
builds on (full scope per original mission YAML — preserved verbatim).

## Test Vectors (per RFC-0011 §Test Vectors — output envelope + error + redaction groups)

21 new TV total (15 baseline + tv_red6/tv_red7/tv_red8 NEW redaction vectors + tv_env6/tv_env7/tv_env8 NEW environment-error vectors) — all passing per `cargo test -p octo-cli --lib`.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `OutputEnvelope<T>` + `OctoCliError` +
  `OctoCliRedactor` + clap structs. All Layer C operator UX.
- NO new Layer A or Layer B types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- 213-line stub's existing subcommands (`init`, `join`, `role`, `agent`,
  `status`) preserved as `commands/stub.rs` wrappers emitting deprecation
  warnings. Same exit code (0) and same visible behavior.
- Hidden from `--help` (`#[command(hide = true)]`); deprecation banner on
  direct invocation.
- Removal: gated on 1 release cycle per RFC migration etiquette; Mission
  `0011-deprecation-stub-removal` handles it.

## Cross-references

- RFC-0011 §Binary Surface, §Output Envelope, §Redaction Layer, §Error Handling — the substrate sections
- (Future work, no RFC filed yet): WhatsApp/Telegram Auth Onboarding redaction
  layer pattern (proven) — see whatsapp/telegram CLI substrate RFC when filed
- RFC-0009 §Identity Lifecycle — substrate that identity commands consume
- [[cipherocto-design-principles]] — Layer A/B stability contract
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure (R31+R32 zero-finding rounds)

## Claimant

@unassigned (mission lifecycle now Completed; archived 2026-08-28 per [[mission-close-up-2026-08-27]] pattern)
