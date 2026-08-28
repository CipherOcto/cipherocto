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
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
spec_cycle_dry_closed: 2026-08-28
---

# 0011-core-output-envelope-redaction — CLI substrate (output + error + redaction + clap root)

**Status:** Claimed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28 (5-round loop-until-DRY closure: R1 → R2 → R3 fix-all → R4 verify → R5 verify; 122+ fixes applied across 7 files). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]].
**Substrate:** RFC-0011 §Output Envelope (Output Envelope), §Redaction Layer (Redaction Layer), §Error Handling (Error Handling), §Binary Surface (Binary Surface)
**Parent:** RFC-0011
**Depends on:**

- RFC-0011: own substrate — this RFC's clap derive + output envelope + redaction layer
- RFC-0009: Identity substrate — `WalletStore` / `IdentityKey` consumed by identity commands
- RFC-0957: Macaroon substrate — `CapabilityToken` consumed by capability commands
- RFC-0964: Caveat Envelope canonical caveat JSON shape
- RFC-0008: Execution Class operator class mapping for CLI commands
  **Blocks:** `0011-identity-commands`, `0011-capability-commands`, `0011-policy-commands`

## Status

Claimed 2026-08-28 (mmacedoeu) — spec cycle DRY-closed 2026-08-28

## RFC

RFC-0011 §Binary Surface (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [ ] `OutputEnvelope<T>` implemented + unit-tested (TV-ENV1..5 pass)
- [ ] `OctoCliError` 19-variant enum implemented + unit-tested (TV-ERR1..5 pass)
- [ ] `OctoCliRedactor` implemented + unit-tested (TV-RED1..8 pass)
- [ ] Clap root struct implemented + unit-tested
- [ ] Stub deprecation banners implemented + unit-tested (TV-DEP1..2)
- [ ] Cross-mission AC: mission's output is the substrate that identity/capability/policy missions build on
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test --workspace --lib green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

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

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Notes

This is the substrate mission; every later identity/capability/policy mission depends on the `OutputEnvelope<T>` + `OctoCliError` + clap root that this mission lands.

## Scope

Land the CLI substrate that every identity / capability / policy subcommand
builds on:

1. **`OutputEnvelope<T>`** — `crates/octo-cli/src/output.rs` per RFC-0011 §Output Envelope
   with `schema_version: u32 = 2`, `generated_at: DateTime<Utc>` (RFC 3339 with
   `Z`), `data: T`, `exit_code: i32`, `preview_only: bool`. TTY-aware renderer using
   `std::io::IsTerminal`. Two orthogonal JSON-forcing mechanisms:
   - `--json` flag forces JSON output (overrides TTY detection) per RFC §Output Envelope
   - `OCTO_FORCE_JSON` environment variable forces JSON output regardless of TTY
     detection (for scripted environments that cannot pass `--json`)
     `--no-color` disables ANSI.

2. **`OctoCliError`** — `crates/octo-cli/src/error.rs` per RFC-0011 §Error Handling.
   19-variant `thiserror` enum (matches impl-guide OctoCliError enum; full list at
   impl-guide §OctoCliError: ClapParse, NoActiveIdentity, ConfirmationRequired,
   AlreadyRotating, IdentityNotFound, HsmUnavailable, AlreadyRevoked, CaveatParse,
   InvalidCaveatCombination, HolderNotFound, AttenuationViolation, SigningFailed,
   ParentCapNotFound, PolicyNotFound, PolicyVersionNotFound, StdinSecretRefused,
   InvalidFilter, StaleStub, Internal). Each variant maps to fixed exit code per
   RFC-0011 §Exit Code table; `render(force_json)` writes to stderr.

3. **`OctoCliRedactor`** — `crates/octo-cli/src/redact.rs` per RFC-0011 §Redaction Layer.
   `tracing_subscriber::Layer` impl + `redact_string(s)` helper + field-name
   redaction table (seed / key / sig / pair / password / bearer / mnemonic /
   passphrase / pin / api_key / secret). Field-name redactor covers 11 names;
   value-pattern redactor covers 8 standalone patterns (`password=`,
   `seed_bytes=`, etc.). Plus the `Hex32` / `RedactedHex` newtypes used by
   capability subcommands.

4. **Clap root struct** — `crates/octo-cli/src/main.rs` REPLACE 213-line stub
   with full `Octo { output: OutputFlags, mode: OperatorModeFlags, command:
Commands }` per RFC-0011 §Binary Surface. `OutputFlags` (--json, --no-color) +
   `OperatorModeFlags` (--mode, --allow-write, --confirm, --dry-run,
   --allow-stdin-secret) per RFC-0011 §Binary Surface.

5. **`flags.rs`** — `crates/octo-cli/src/flags.rs` with `OutputFlags`,
   `OperatorModeFlags`, `OperatorMode` enum (Human / Ci / Auditor).

6. **`commands/mod.rs`** — `crates/octo-cli/src/commands/mod.rs` with
   `dispatch` stubs that wire to identity/capability/policy modules.

7. **Stub deprecation banners** — `crates/octo-cli/src/commands/stub.rs` per
   RFC-0011 §Compatibility `init` / `join` / `role` / `agent` / `status` print
   deprecation warning + hint pointing to replacement subcommand (which lands in
   role-provisioning / agent-lifecycle / mesh-operations amendments per Status
   header amendment chain).

### Cargo deps added

```toml
# === Substrate (Layer B) — RFC-0011 §Dependencies ===
octo-wallet = { path = "../octo-wallet", version = "0.1.0" }       # RFC-0009
octo-cap-macaroon = { path = "../octo-cap-macaroon", version = "0.1.0" }  # RFC-0957
octo-policy = { path = "../octo-policy", version = "0.1.0" }       # RFC-0967

# === Output + TTY ===
chrono = { version = "0.4", default-features = false, features = ["serde", "clock"] }

# === Error envelope ===
thiserror = "2.0"

# === Hex32 newtype serialization ===
hex = "0.4"

# === Redaction ===
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# === Schemars for OutputEnvelope<T> ===
schemars = { version = "0.8", features = ["chrono"] }
```

## Test Vectors (per RFC-0011 §Test Vectors — output envelope + error + redaction groups)

21 new TV total (15 baseline + tv_red6/tv_red7/tv_red8 NEW redaction vectors + tv_env6/tv_env7/tv_env8 NEW environment-error vectors):

**Envelope group (5):**

- `tv_env1_schema_version_present` — `OutputEnvelope::new(..).render(--json, false)` contains `"schema_version":2,"preview_only":false`
- `tv_env2_generated_at_rfc3339_utc` — `generated_at` ends in `Z`; roundtrip parses
- `tv_env3_json_toggle` — `--json` flag → JSON output; no flag + TTY → pretty
- `tv_env4_tty_detected` — when `is_terminal()` returns true and no `--json`, output is multi-line
- `tv_env5_no_color_honored` — `NO_COLOR=1` env → no ANSI escape codes in output

**Error group (5):**

- `tv_err1_clap_parse_propagated` — invalid arg → exit 2 + clap usage (POSIX convention)
- `tv_err2_internal_no_substrate_leak` — `Internal("SQL: select * from wallet...")` is sanitized to `Internal("wallet store error")` via `sanitize_substrate_error`
- `tv_err3_source_chain_rendered` — `#[source]` chain printed under `caused by:`
- `tv_err4_exit_code_mapping` — each variant returns fixed code per §Exit Code table
- `tv_err5_no_substrate_internals` — file paths + SQL fragments never appear in user-facing message

**Redaction group (8):**

- `tv_red1_holder_sig_stripped` — log line containing 128-hex holder_sig → `[REDACTED:sig]`
- `tv_red2_pair_code_stripped` — stderr `pair_code=ABC123` → `pair_code=[REDACTED:pair]`
- `tv_red3_bearer_token_stripped` — `Authorization: Bearer eyJhbGc...` → `Bearer [REDACTED:bearer]` (case-insensitive on `Bearer`)
- `tv_red4_password_field_stripped` — `password=hunter2` → `password=[REDACTED:pw]`
- `tv_red5_seed_bytes_stripped` — `seed_bytes=abc123...` → `seed_bytes=[REDACTED:seed]`
- `tv_red6_mnemonic_stripped` — `mnemonic word1 word2 ...` → `mnemonic=[REDACTED:mnemonic]` — NEW
- `tv_red7_pin_stripped` — `pin=1234` → `pin=[REDACTED:pin]` — NEW
- `tv_red8_api_key_stripped` — `api_key=sk-abc123` → `api_key=[REDACTED:api_key]` — NEW

**Environment-error group (3, owned by core mission):**

- `tv_env6_internal_error_path` — substrate returns `Internal("SQL: SELECT ...")`;
  `sanitize_substrate_error` invoked via `user_message()`; stderr shows
  sanitized message `wallet store error` (per RFC §Error Handling)
  with SQL fragment stripped; exit 64.
- `tv_env7_stdin_secret_refused` — operator pipes private key to stdin without
  `--allow-stdin-secret`; stderr shows `secret material on pipe`; exit 15.
  With `--allow-stdin-secret`: warning + audit log entry tagged
  `stdin_secret_override=true`; exit 0.
- `tv_env8_concurrent_lock` — second CLI instance tries to acquire wallet lock
  while first instance holds it; without lock → exit 101; wallet mutex
  contention recorded in audit log.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `OutputEnvelope<T>` + `OctoCliError` +
  `OctoCliRedactor` + clap structs. All Layer C operator UX.
- NO new Layer A or Layer B types introduced.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cli --lib --all-features
cargo test -p octo-cli --test '*' --all-features
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

## Claimant

@unassigned
