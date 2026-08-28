---
name: 0011-policy-commands
description: Land policy subcommands (policy show/list) per RFC-0011
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0967
    - RFC-0964
    - mission 0011-core-output-envelope-redaction
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
spec_cycle_dry_closed: 2026-08-28
---

# 0011-policy-commands — Policy subcommands (policy show, policy list)

**Status:** Claimed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28 (5-round loop-until-DRY closure). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]].
**Substrate:** RFC-0011 §Subcommand Taxonomy (PolicyAction), RFC-0967 Policy Object Graph
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — substrate
  **Blocks:** `0011-deprecation-stub-removal` (final integration)

## Status

Claimed 2026-08-28 (mmacedoeu) — spec cycle DRY-closed 2026-08-28

## RFC

RFC-0011 §Subcommand Taxonomy (rfcs/draft/process/0011-octo-cli-substrate.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [ ] `octo policy show` implemented + unit-tested (TV-POL1, TV-POL2, TV-POL4 pass)
- [ ] `octo policy list` implemented + unit-tested (TV-POL3, TV-POL5 pass)
- [ ] `body` redaction pass implemented + unit-tested (over `Vec<u8>` per R1 substrate alignment review)
- [ ] Version resolution implemented + unit-tested (CLI distinguishes name-not-found from version-not-found via `NameHashIndex`)
- [ ] Filter parsing implemented + unit-tested (CLI-side `InvalidFilter` only — no substrate `PolicyError::InvalidFilter`)
- [ ] Cross-mission AC: policy commands integrate with core mission's `OutputEnvelope<T>` + `OctoCliRedactor`
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test --workspace --lib green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011 type      | Sub-step                        | Notes                                                                                                                   |
| ------------------ | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `PolicyRecord`     | Sub-step 1 (output types)       | Layer B; `[ADD]` struct in octo-policy substrate, field-aligned to `RegisteredPolicy` per R1 substrate alignment review |
| `PolicyListEntry`  | Sub-step 1 (output types)       | Layer B; substrate return type for `list()`                                                                             |
| `PolicyFilter`     | Sub-step 4 (filter parsing)     | Layer B; `[ADD]` struct parsed CLI-side                                                                                 |
| `NameHashIndex`    | Sub-step 3 (version resolution) | Layer B; `[ADD]` extension to octo-policy substrate                                                                     |
| `PolicyShowOutput` | Sub-step 1 (output types)       | Layer C/D; CLI-output wrapper around `PolicyRecord`                                                                     |
| `PolicyListOutput` | Sub-step 1 (output types)       | Layer C/D; CLI-output wrapper around `Vec<PolicySummary>`                                                               |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Policy Subcommands for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Notes

`body` redaction is a defense-in-depth pass — substrate enforces redaction at write time per RFC-0967, CLI does a final sweep before display (over hex-decoded bytes, re-encodes post-redaction).

## Scope

Land 2 policy subcommands per RFC-0011 §Subcommand Taxonomy:

1. **`octo policy show <name>`** — `commands/policy.rs::show`. Args:
   `<name>` REQUIRED. Flags: `--version <n:u32>` (default latest),
   `--kind-uuid <uuid>` (filter), `--json`. Substrate call:
   `[ADD] octo_policy::show(name: &str, version: u32) -> Result<PolicyRecord, PolicyRegistryError>`
   (substrate has a `PolicyRegistry` trait keyed on content hash; the
   `(name, version)` form is the CLI-friendly `[ADD]` wrapper; `PolicyRecord`
   is a new `[ADD]` struct in octo-policy Layer B field-aligned to substrate
   `RegisteredPolicy` per R1 substrate alignment review). Output:
   `PolicyShowOutput { name, kind_uuid, body, execution_class,
registered_by_did: Hex32, registered_at_unix, revoked_at_unix?,
revoked_by_did?, revocation_reason?, superseding_policy_hash? }`. Exit 0
   / 13 (not found) / 14 (no such version) / 64. Read-only. (`signer_set: Vec<Did>`
   from R0 was reduced to a single `registered_by_did: Hex32` for v1.0;
   multi-signer governance sets defer to the governance amendment per Status
   header amendment chain.)

2. **`octo policy list`** — `commands/policy.rs::list`. Flags: `--json`,
   `--filter <kind>`. Substrate call:
   `[ADD] octo_policy::list(filter: &PolicyFilter) -> Result<Vec<PolicyListEntry>, PolicyRegistryError>`
   (returns the new `[ADD]` `PolicyListEntry` type from octo-policy substrate,
   distinct from the CLI-output `PolicySummary` type that lives in octo-cli).
   Output:
   `PolicyListOutput { policies: Vec<PolicySummary> }` where
   `PolicySummary { name, kind, execution_class, version }` (CLI-output type;
   substrate returns `PolicyListEntry`, CLI maps to `PolicySummary`). Exit 0
   / 64. Read-only.

### Sub-steps

1. **Output types** — `crates/octo-cli/src/commands/policy.rs`. Three
   `#[derive(Serialize, schemars::JsonSchema)]` structs: `PolicyShowOutput`,
   `PolicyListOutput`, `PolicySummary`. `body` (in `PolicyShowOutput`) is
   `Vec<u8>` mirroring substrate `RegisteredPolicy.body` (canonical CBOR /
   trait-spec bytes). The CLI renders `body` as a hex-encoded string in
   JSON output (or YAML-like in TTY mode); substrate body structure is
   substrate-defined, CLI does NOT introspect beyond hex rendering +
   redactor pass.

2. **Redactor pass on `body`** — `commands/policy.rs::redact_body`. Before
   rendering, walk the hex-decoded `body` and apply redaction rules to
   nested fields named `private_key`, `holder_sig`, `password`, `seed`,
   `mnemonic`, `seed_phrase`, `passphrase`, `pin`, `api_key`, `secret`,
   `token` (per RFC-0011 §Redaction Layer). This is a defense-in-depth
   pass — substrate enforces redaction at write time per RFC-0967; CLI does
   a final sweep before display.

3. **Version resolution** — `commands/policy.rs::resolve_version(name, version:
Option<u32>) -> Result<u32, OctoCliError>`. If `None`, query substrate for
   latest version via `[ADD] octo_policy::latest_version(name: &str) -> Result<u32, PolicyRegistryError>`.
   If substrate returns `PolicyRegistryError::NotFound(name)` with no
   versions registered, surface as `OctoCliError::PolicyNotFound(name)`
   (exit 13). If substrate returns `PolicyRegistryError::NotFound` for a
   version mismatch (CLI resolves the policy_hash via
   `[ADD] NameHashIndex::resolve(name, Some(version))` and the resolved
   policy_hash is not in `PolicyRegistry`), surface as
   `OctoCliError::PolicyVersionNotFound { policy: name.to_string(), version }`
   (exit 14). Substrate `PolicyRegistryError` does NOT have a
   `VersionNotFound` variant per R1 substrate alignment review (substrate truth per
   `octo-policy::PolicyRegistryError` shape); CLI distinguishes
   via the `NameHashIndex` lookup before surfacing the substrate error.

4. **Filter parsing** — `commands/policy.rs::parse_filter(s: &str) ->
Result<PolicyFilter, OctoCliError>`. `s` is `kind=<value>` or
   `class=<value>`. Unknown filter form → `OctoCliError::InvalidFilter(s)`
   (CLI-side parse error, NOT a substrate variant; substrate has no
   `InvalidFilter` per R1 substrate alignment review; exit code 16).

5. **Dispatch** — `dispatch(action, &Octo) -> Result<(), OctoCliError>`.
   Matches on `PolicyAction` enum. Each arm resolves version, builds output,
   applies redaction, renders envelope.

### Substrate API consumed (per RFC-0967 + R1 substrate alignment amendment)

```rust
// crates/octo-policy/src/lib.rs (substrate, [ADD] forms per RFC-0011)
#[ADD] pub fn show(name: &str, version: u32) -> Result<PolicyRecord, PolicyRegistryError>;
#[ADD] pub fn list(filter: &PolicyFilter) -> Result<Vec<PolicyListEntry>, PolicyRegistryError>;
#[ADD] pub fn latest_version(name: &str) -> Result<u32, PolicyRegistryError>;
```

`PolicyError` is NOT extended (per R1 substrate alignment review). Substrate truth per
`octo-policy::PolicyRegistryError` shape: substrate today exposes
`PolicyRegistryError` with variants `NotFound(String)`, `HashMismatch`,
`InvalidClassBProof`, `AlreadyRegistered`, `NotRegistrant`, `AlreadyRevoked`,
`AuthorityDelegationDenied`. No `EmptyIntersection` variant (the previous
mission text was substrate-truth drift — substrate uses
`AuthorityDelegationDenied` for empty-intersection-class failures). CLI maps
the substrate variants directly:

| Substrate `PolicyRegistryError`      | CLI `OctoCliError`                                | Exit | Sanitization          |
| ------------------------------------ | ------------------------------------------------- | ---- | --------------------- |
| `NotFound(name)` (no policy by name) | `PolicyNotFound(name)`                            | 13   | none (name is opaque) |
| `NotFound(name)` (version mismatch)  | `PolicyVersionNotFound { policy: name, version }` | 14   | none                  |
| `HashMismatch { .. }`                | `Internal(sanitize_substrate_error(...))`         | 64   | sanitizer applied     |
| `InvalidClassBProof`                 | `Internal(sanitize_substrate_error(...))`         | 64   | sanitizer applied     |
| `AlreadyRegistered(..)`              | `Internal(sanitize_substrate_error(...))`         | 64   | sanitizer applied     |
| `NotRegistrant(..)`                  | `Internal(sanitize_substrate_error(...))`         | 64   | sanitizer applied     |
| `AlreadyRevoked { .. }`              | `Internal(sanitize_substrate_error(...))`         | 64   | sanitizer applied     |
| `AuthorityDelegationDenied(..)`      | `Internal(sanitize_substrate_error(...))`         | 64   | sanitizer applied     |
| (CLI-side filter parse failure)      | `InvalidFilter(s)` (CLI-side, exit 16)            | 16   | none (operator-owned) |

## Test Vectors (per RFC-0011 §Test Vectors — policy group)

5 new TV (TV-POL1..TV-POL5):

- `tv_pol1_show_success` — `octo policy show rate_limit` → exit 0; stdout
  contains `name`, `kind_uuid`, `body` (hex-encoded bytes), `execution_class`,
  `registered_at_unix`, `registered_by_did`
- `tv_pol2_show_not_found` — `octo policy show no_such_policy` → exit 13;
  stderr `policy not found`
- `tv_pol3_list_filter` — `octo policy list --filter kind=rate_limit` → exit 0;
  stdout `policies` array contains only `kind=rate_limit` entries
- `tv_pol4_show_version_mismatch` — `octo policy show rate_limit --version 999`
  → exit 14; stderr `policy version not found`
- `tv_pol5_list_empty` — substrate has 0 policies; `octo policy list` →
  exit 0; stdout `"policies":[]`

### Cargo deps added

None new. All deps added by mission `0011-core-output-envelope-redaction`.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `PolicyAction` dispatch + 3 output structs +
  filter parser + body redactor + `PolicySummary` CLI-output type
- `octo-policy` (Layer B) — extended with `[ADD]` `PolicyRecord` (field-aligned
  to substrate `RegisteredPolicy` per R1 substrate alignment review: `kind_uuid: [u8;16]`,
  `body: Vec<u8>`, `registered_at_unix: i64`, plus revocation fields
  `revoked_at_unix` / `revoked_by_did` / `revocation_reason` /
  `superseding_policy_hash`) + `PolicyListEntry` + `PolicyFilter` types, NO
  new `PolicyError` variants (substrate `PolicyRegistryError` already has
  `NotFound(String)` etc.; CLI maps directly), and a name→hash index
  `NameHashIndex { by_name: BTreeMap<String, Vec<(u32, [u8;32])>> }`
  keyed by name with values `(version, policy_hash)` for `(name, version)`
  resolution. Substrate today exposes `PolicyRegistry::lookup_policy` keyed
  on content hash only; the `(name, version)` + name→hash-index + `list()` +
  `latest_version()` wrappers are the new additions (each is the `[ADD]`
  form per RFC-0011 §Subcommand Taxonomy entries #14, #15, #16, #17).

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cli --lib --all-features
cargo test -p octo-cli --test policy --all-features
```

## Backward compat

- Additive only: `octo-policy` extended per `[ADD]` declarations above; no
  breaking changes to existing public API per RFC migration etiquette. The
  existing `PolicyRegistryError` enum (per `octo-policy::PolicyRegistryError` shape)
  is unchanged — the [ADD] amendments are pure additions (`PolicyRecord`,
  `PolicyListEntry`, `PolicyFilter`, `NameHashIndex`, `show` / `list` /
  `latest_version` wrappers).
- CLI exit codes match RFC-0011 §Exit Code table
- `body` redactor pass is internal to CLI; downstream consumers see
  post-redaction bytes (hex-encoded in JSON output)

## Cross-references

- RFC-0011 §Subcommand Taxonomy PolicyAction table
- RFC-0967 — Policy Object Graph substrate
- RFC-0965 — Caveat envelope (policy body may embed caveats; redactor applies
  to nested fields)
- (Future work, no RFC filed yet): WhatsApp/Telegram Auth Onboarding redaction
  pattern (applied to nested secret fields in `body_json`) — see
  whatsapp/telegram CLI substrate RFC when filed
- [[cipherocto-design-principles]] — Layer B stability contract

## Claimant

@unassigned
