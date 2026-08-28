# RFC-0011 (Process): `octo` CLI Substrate

## Status

Draft

> **Amendment chain:** This document covers identity, capability, and
> authorization subcommands. Follow-on amendments will extend to audit
> (RFC-0011-a), reputation (RFC-0011-b), agent lifecycle (RFC-0011-c),
> role provisioning (RFC-0011-d), vault operations (RFC-0011-e), mesh
> operations (RFC-0011-f), and governance (RFC-0011-g).

## Authorship Note

> **Placeholder disclosure (per BLUEPRINT §RFC Process for unreviewed drafts):**
> Initial draft authored by CipherOcto maintainers; specific handles pending
> community review. Once RFC-0011 enters Review status, this section is replaced
> with named handles per git log (the substrate-amendment RFC that follows
> RFC-0011 inherits the same authorship chain).

## Summary

This RFC defines the substrate of the `octo` command-line interface (`crates/octo-cli`,
binary name `octo`) for identity, capability, and authorization operations. The
specification fixes the binary surface (clap derive), the output envelope (TTY-aware
pretty/JSON), the redaction layer (zero plaintext secret emission), the lifecycle
mapping to RFC-0009 identity state machine, the caveat catalog binding to RFC-0964,
and the policy object graph binding to RFC-0967. It also defines the compatibility
window for the existing stub commands (`init`, `join`, `role`, `agent`, `status`) and
the exit-code table that all subcommands MUST honor.

## Dependencies

**Requires:**

- RFC-0009 (Process): Identity Management — lifecycle state machine + rotation grace
- RFC-0957 (Process): Macaroon Substrate — capability token structure + holder signature
- RFC-0964 (Process): Constraint Encoding — caveat envelope canonical form
- RFC-0960 (Economics): Vaults, Capabilities, Reservations — caveat catalog root
- RFC-0967 (Process): Policy Object Graph — policy show/list substrate
- RFC-0958 (Process): ZK Capability Subclass — `HolderKind::ZKBearing` flag handling
- RFC-0008 (Process): Deterministic AI Execution Boundary — execution class mapping

**Optional:**

- RFC-0003 (Process): Deterministic Execution Standard — for `--deterministic-time` flag
- (Future work, no RFC filed yet): WhatsApp/Telegram Auth Onboarding clap surface pattern + redaction layer — adapter CLI substrate RFC to be filed when those adapters land

> **Dependency Validation Rules:**
>
> 1. Dependencies MUST form a DAG (no cycles)
> 2. All "Requires" RFCs MUST be listed as mission prerequisites
> 3. Optional dependencies MUST be documented separately from required
> 4. Dependencies on "Planned" RFCs MUST note the assumption they will be Accepted
> 5. No 2-cycle sibling required — RFC-0011 is acyclic against all Required dependencies

## Design Goals

| Goal | Target                             | Metric                                                                                                                          |
| ---- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| G1   | Deterministic exit codes           | Same input → same exit code across runs (table in §Exit Codes)                                                                  |
| G2   | Zero plaintext secret emission     | Redaction layer test passes; no `seed`, `private_key`, `holder_sig`, `pair_code`, `password` in any log line, stderr, or stdout |
| G3   | HSM-bound signing path             | `--dev` flag required for `InMemorySigner`; default path rejects `--dev`; CI asserts default-HSM-only                           |
| G4   | Cross-platform                     | Linux + macOS + Windows; `std::io::IsTerminal` for TTY detection (Rust 1.70+)                                                   |
| G5   | Zero state mutation on `--dry-run` | Every mutating command accepts `--dry-run`; substrate call wrapped in dry-run gate; CI regression test asserts no state change  |
| G6   | TTY-aware output                   | Pretty-print when TTY + no `--json`; JSON when `--json` or non-TTY (pipe, redirect)                                             |

## Motivation

`crates/octo-cli/src/main.rs` is currently a 213-line stub. Of the five subcommands
exposed (`init`, `join`, `role`, `agent`, `status`), only `init()` calls real substrate
code (`octo_registry::init()`); the remainder is `println!` text. The four planned
subcommand groups — identity, capability, audit, reputation — have **no substrate
wiring at all**. The user-facing scratchpad notes the gap explicitly:

> The current `octo-cli` binary is a stub exposing only `init`, `join`, `role`,
> `agent`, `status`. The identity, capability, audit, and reputation commands below
> are on the roadmap and will land as their respective substrate crates stabilize.

The substrate crates (`octo-wallet`, `octo-cap-macaroon`, `octo-policy`) are stable
per their Layer-B years-stable contract. The binding layer between them and the
operator — the CLI — has no specification. This RFC closes that gap for the
identity/capability/authorization slice; subsequent amendments per Status
header amendment chain close the remaining slices.

RFC-0009 already names `octo-cli` as an identity consumer with no formal interface:

> `octo-cli` — creates identities, displays them to users
> Cross-crate contracts are implicit — no formal interface between `octo-core`,
> `octo-cli`, and `octo-registry`

This RFC makes that interface formal: clap derive structs, output schemas, exit
codes, error envelopes, and redaction contracts.

## Roles and Authorities

> **The "Nothing should be implied" rule (specification layer):** Every actor that
> affects correctness, security, accountability, or consensus MUST be named with a
> stable identifier, a defined authority scope, and a typed lifecycle. Inference is a
> defect.

The CLI introduces three operator-facing roles. None of these roles affect consensus
directly; the CLI is a thin operator UX layer over Layer-B substrate crates.

| Role           | Identifier              | Authority Scope                                      | Lifecycle | Source/Ref                   |
| -------------- | ----------------------- | ---------------------------------------------------- | --------- | ---------------------------- |
| Human Operator | `OperatorKind::Human`   | read + write (with `--confirm` for mutating)         | stateless | RFC-0009 §Identity Lifecycle |
| CI Bot         | `OperatorKind::CiBot`   | read-only by default; write requires `--allow-write` | stateless | This RFC §Operator Modes     |
| Auditor (RO)   | `OperatorKind::Auditor` | read-only + audit-trail access                       | stateless | This RFC §Operator Modes     |

### Role Transitions

The CLI does not manage role state. Operator-mode selection is per-invocation via
`--mode <kind>` or detected from environment (`CI=true` → `CiBot`, `OCTO_AUDIT=1` →
`Auditor`). No persistent role state. No signing requirement.

### Out-of-scope Roles

- **Node Operators** (full node, wallet node, gateway) — these interact via dedicated
  RPC + config, not this CLI. The CLI is the user/operator tool, not the node admin
  tool.
- **AI Agents** — programmatic substrate access is via the Python SDK + HTTP proxy,
  not this CLI. See RFC-0917.

## Specification

### System Architecture

```mermaid
graph TB
    subgraph Bin["octo binary"]
        A[clap parser: Octo]
        B[Subcommand dispatch]
        C[OctoCliError + redaction]
        D[OutputEnvelope&lt;T&gt;]
        E[TTY detection]
    end

    subgraph Sub["Substrate crates (Layer B)"]
        F["octo-wallet (B)"]
        G["octo-cap-macaroon (B)"]
        H["octo-policy (B)"]
    end

    subgraph Persist["Persistence"]
        I[(wallet store)]
        J[(policy registry)]
        K[(macaroon store)]
    end

    A --> B
    B -->|whoami| F
    B -->|identity| F
    B -->|capability| G
    B -->|policy| H
    B --> C
    B --> D
    D --> E
    F --> I
    G --> K
    H --> J
```

The `octo` binary is a Layer-C orchestrator. It owns:

- clap parsing (operator UX)
- output envelope rendering (operator UX)
- redaction layer (operator UX)
- error envelope (operator UX)

It does NOT own:

- identity substrate (Layer B — `octo-wallet`)
- capability substrate (Layer B — `octo-cap-macaroon`)
- policy substrate (Layer B — `octo-policy`)
- persistence (Layer A/B — substrate crates own their stores)

### Binary Surface

The `octo` binary uses clap derive. The root `Octo` struct carries one `command` field
of type `Commands`. Stub commands from the prior `main.rs` are preserved as
deprecated wrappers (§Compatibility).

```rust
#[derive(Parser, Debug)]
#[command(name = "octo", version, about)]
struct Octo {
    #[command(flatten)]
    output: OutputFlags,           // --json, --no-color

    #[command(flatten)]
    mode: OperatorModeFlags,       // --mode <kind>, --allow-write, --confirm

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show the active identity (DID, key, lifecycle state).
    Whoami,

    /// Identity operations.
    Identity {
        #[command(subcommand)]
        action: IdentityAction,
    },

    /// Capability operations.
    Capability {
        #[command(subcommand)]
        action: CapabilityAction,
    },

    /// Policy inspection (read-only).
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },

    // --- Deprecated stub commands (kept in v1.0, hard-error in v1.1, removed in v2.0) ---
    // Per RFC migration etiquette: 1 release deprecation window + 1 release hard-error
    // window before removal. v2.0 is the canonical removal target.
    #[command(hide = true)]
    Init,         // DEPRECATED: prints init banner + warning
    #[command(hide = true)]
    Join,         // DEPRECATED: prints join banner + warning
    #[command(hide = true)]
    Role { #[command(subcommand)] action: RoleActionStub },  // DEPRECATED
    #[command(hide = true)]
    Agent { #[command(subcommand)] action: AgentActionStub }, // DEPRECATED
    #[command(hide = true)]
    Status,       // DEPRECATED: prints network banner + warning
}
```

### Subcommand Taxonomy

Each subcommand below specifies: args, flags, output schema, substrate call, exit
codes, and redaction requirements.

> **Substrate API note:** Calls prefixed with `[ADD]` denote Layer-B substrate
> additions gated on RFC-0011 acceptance. They are added by the RFC-0011
> substrate work (a separate amendment) and are required for the CLI commands in
> this RFC to compile. Where the substrate already exposes an equivalent, the
> `[ADD]` entry references the existing form (e.g., `IdentityKey::begin_rotation`
> instance method hard-codes the 24h grace period).
>
> **Complete `[ADD]` surface (consumed by Phase 1 commands):**
>
> 1. `octo_wallet::WalletStore` — new struct in `octo-wallet` Layer B. Substrate today does NOT expose `WalletStore`. Required API:
>    - `pub struct WalletStore`
>    - `WalletStore::open() -> Result<Self, WalletError>` (open the on-disk wallet store at `$OCTO_HOME/wallet` or `~/.config/octo/wallet`)
>    - `active_identity(&self) -> Result<IdentityKey, WalletError>` — returns the active `IdentityKey`. Substrate today returns `WalletError::NotActive { current_state }` (no `IdentityKey` on success); the [ADD] form changes success to `IdentityKey` and reserves `WalletError::NoActiveIdentity` for the no-active-state case (CLI exit 2).
>    - `identity_record(&self, did: &Did) -> Result<IdentityRecord, WalletError>` — full record by DID
>    - `WalletStore` enforces 0700 permissions on store creation (per §Implicit Assumptions Audit row "Local file permissions on config dir are 0700" — currently substrate does NOT enforce; this [ADD] makes enforcement substrate-truth).
> 2. `octo_wallet::Did` — new newtype in `octo-wallet` Layer B. Substrate today has no `Did` type. CLI-side construction: `did:octo:` prefix + base32 encoding of `key.public_key_bytes()`. `[ADD] pub struct Did(pub String)` + `IdentityKey::did(&self) -> Did`.
> 3. `octo_wallet::IdentityRecord` — new struct in `octo-wallet` Layer B. Fields: `{ did: Did, pubkey_bytes: [u8;32], lifecycle: LifecycleState, hsm_slot: Option<u32>, registered_at_unix: i64, rotation_history: Vec<IdentityRotationEvent> }`. Substrate today exposes only `IdentityKey`; `IdentityRecord` is the (did, lifecycle, hsm_slot, history) projection.
> 4. `octo_wallet::IdentityRotationEvent` — new struct in `octo-wallet` Layer B (distinct from the existing `RotationEvent` in `crates/octo-wallet/src/vault_rotation.rs`). Fields: `{ rotation_id: [u8;32], started_at_unix: i64, grace_expires_at_unix: i64, successor_did: Did, signature_proof: [u8;64] }`.
> 5. `octo_wallet::begin_rotation(&mut IdentityKey, successor: IdentityKey, now_unix_secs: u64) -> Result<[u8;64], WalletError>` — `octo identity rotate` (thin wrapper around the existing `IdentityKey::begin_rotation(&mut self, ...)` instance method per substrate signature; substrate hard-codes 24h grace)
> 6. `octo_wallet::revoke(&mut IdentityKey, now_unix_secs: u64) -> Result<(), WalletError>` — `octo identity revoke` (thin wrapper around the existing `IdentityKey::revoke(&mut self, ...)` instance method per substrate signature). NOTE: substrate `IdentityKey::revoke` is idempotent from `Revoked` state — no `AlreadyRevoked` error fires. CLI exit 6 (`AlreadyRevoked`) is achieved via CLI-level pre-check on `record.lifecycle() == LifecycleState::Revoked` BEFORE calling `revoke()`. See §Subcommand Taxonomy → `octo identity revoke`.
> 7. `octo_cap_macaroon::list_active(filter: &CapabilityFilter) -> Result<Vec<CapabilitySummary>, MacaroonError>` — `octo capability list`. Substrate today has neither `list_active` nor `CapabilitySummary` nor `CapabilityFilter`; all three are `[ADD]` (see #8 below).
> 8. `octo_cap_macaroon::CapabilitySummary` — new struct. Fields: `{ cap_id: [u8;32], root_id: [u8;32], caveats: Vec<CaveatSummary>, expires_at_unix: Option<i64> }`. Plus `CapabilityFilter { holder_did: Option<String>, root_id: Option<[u8;32]>, expires_before_unix: Option<i64> }` for filter parsing.
> 9. `octo_cap_macaroon::CaveatSummary` — new struct. Fields: `{ caveat_type: String, constraint_json: serde_json::Value }`. (No `display_label` field — Layer B must not carry presentation per §Architectural Principles; the CLI renders presentation.)
> 10. `octo_cap_macaroon::mint(root_secret: &[u8;32], holder: &dyn CapabilitySigner, holder_did: &str, caveats: &[Caveat]) -> Result<CapabilityToken, MintError>` — `octo capability mint`. The CLI form is a thin wrapper that constructs `CapabilityToken::mint(root_secret, holder, holder_did, caveats)` per substrate signature. NOTE: substrate signature uses `&dyn CapabilitySigner` (cross-ref `octo_cap_macaroon::signer`), NOT the phantom `HolderKey` type. The CLI obtains the signer via `WalletStore::active_signer() -> Result<Arc<dyn CapabilitySigner>, WalletError>` (new [ADD] helper that wraps the HSM-backed signer).
> 11. `octo_cap_macaroon::attenuate(parent: &CapabilityToken, caveats: &[Caveat], holder: &dyn CapabilitySigner, catalog: &dyn CapabilityCatalog) -> Result<CapabilityToken, MintError>` — `octo capability attenuate`. Substrate `CapabilityToken::attenuate` takes a SINGLE caveat and does NOT re-sign; `attenuate_with_signer` is the re-signing variant. The CLI form loops over `caveats.iter()` and calls `parent.attenuate_with_signer(caveat, holder, catalog)` per caveat, threading the result forward.
> 12. `octo_cap_macaroon::set_subsumes(parent_caveats: &[Caveat], child_caveats: &[Caveat]) -> bool` (or `set_subsumes_with_registry` if a registry is in scope) — helper for attenuation validation; replaces the would-be `is_narrowing` since the substrate already exposes `set_subsumes` in `crates/octo-cap-macaroon/src/caveat/mod.rs`. Substrate exposes this on caveat slices; CLI calls it with `caveat_set_of(parent)` and `caveat_set_of(child)` helpers that extract the `&[Caveat]` view from each `CapabilityToken`.
> 13. Caveat envelope check: substrate today has NO `caveat::validate_canonical_form` and NO `CatalogError`. RFC-0011 proposes the [ADD] form `octo_cap_macaroon::caveat::validate_canonical_form(caveats: &[Caveat]) -> Result<(), CatalogError>` where `CatalogError` is a new error enum (e.g., `UnknownCaveatName`, `DuplicateCaveat`, `ConstraintShapeMismatch`). Until the substrate amendment lands, CLI delegates caveat envelope parsing to `Caveat::canonical_ser` and verifies the round-trippable JSON shape per RFC-0964; failures map to `OctoCliError::CaveatParse` (exit 7).
> 14. `octo_policy::show(name: &str, version: u32) -> Result<PolicyRecord, PolicyRegistryError>` — CLI-friendly wrapper over the RFC-0967 substrate interface (currently `PolicyRegistry::lookup_policy` is keyed on content hash; the `(name, version)` form is the new `[ADD]`). Returns `PolicyRegistryError` (NOT a new `PolicyError`) — substrate-truth: substrate has only `PolicyRegistryError` today.
> 15. `octo_policy::list(filter: &PolicyFilter) -> Result<Vec<PolicyListEntry>, PolicyRegistryError>` — returns `PolicyListEntry { name: String, kind: String, execution_class: ExecutionClass, version: u32 }` (a new type distinct from `PolicySummary` which lives in `octo-cli` as the CLI output type).
> 16. `octo_policy::latest_version(name: &str) -> Result<u32, PolicyRegistryError>` — version resolution helper for `octo policy show`.
> 17. `octo_policy::PolicyFilter` — new struct. Fields: `{ name_prefix: Option<String>, kind: Option<String>, execution_class: Option<ExecutionClass> }`. CLI parses `--filter kind=<v>` / `--filter class=<v>` into this struct.
>
> Additionally, the following [ADD] substrate extensions are required:
>
> - `PolicyRecord` (new struct in octo-policy Layer B; **field-aligned to substrate `RegisteredPolicy` per R1 substrate alignment review**): `{ name: String, kind_uuid: [u8;16], body: Vec<u8>, execution_class: ExecutionClass, registered_at_unix: i64, registered_by_did: [u8;32], revoked_at_unix: Option<i64>, revoked_by_did: Option<[u8;32]>, revocation_reason: Option<String>, superseding_policy_hash: Option<[u8;32]> }`. Substrate today has only `RegisteredPolicy` keyed on `policy_hash: &[u8;32]`; `PolicyRecord` is the (name, version) projection. Field-type alignment per substrate `PolicyRegistry` shape: `kind_uuid` is `[u8;16]` (UUIDv5, NOT `[u8;32]`); `body` is `Vec<u8>` (canonical bytes, NOT `body_json: serde_json::Value`); `registered_at_unix: i64` (NOT `last_updated: u64`); revocation fields (`revoked_at_unix`, `revoked_by_did`, `revocation_reason`, `superseding_policy_hash`) are added to `PolicyRecord` to mirror the substrate fields.
> - `octo-policy` name → hash index: `pub struct NameHashIndex { by_name: BTreeMap<String, Vec<(u32, [u8;32])>> }` keyed by name, value `Vec<(version, policy_hash)>`. Methods: `register(policy: &RegisteredPolicy)`, `resolve(name: &str, version: Option<u32>) -> Option<[u8;32]>`, `latest_version(name: &str) -> Option<u32>`, `list_all() -> Vec<PolicyListEntry>`. New `[ADD]` extension to octo-policy substrate; allows `show(name, version)` and `list()` to resolve names without iterating the full registry.
> - `PolicyError` is NOT extended. Substrate-truth: substrate today has `PolicyRegistryError` with `NotFound(String)`, `HashMismatch`, `InvalidClassBProof`, `AlreadyRegistered`, `NotRegistrant`, `AlreadyRevoked`, `AuthorityDelegationDenied`. CLI maps substrate `PolicyRegistryError::NotFound(name)` → `OctoCliError::PolicyNotFound(name)` directly (exit 13); CLI parses filter strings CLI-side; no substrate `InvalidFilter` variant is proposed (CLI-side `OctoCliError::InvalidFilter` covers it).

#### `octo whoami`

| Aspect       | Value                                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Args         | none                                                                                                                                       |
| Flags        | `--json`                                                                                                                                   |
| Output       | `WhoamiOutput { did, pubkey_hex, lifecycle_state, hsm_slot, registered_at }`                                                               |
| Substrate    | `[ADD] octo_wallet::active_identity(&WalletStore) -> Result<IdentityKey, WalletError>` (explicit `&WalletStore` handle; NO ambient global) |
| Exit codes   | 0 (success), 2 (no active identity), 64 (internal error)                                                                                   |
| Redaction    | none (no secret material in output)                                                                                                        |
| Side effects | none (read-only)                                                                                                                           |
| Dry-run      | n/a                                                                                                                                        |

#### `octo identity show`

| Aspect       | Value                                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Args         | `[DID]` (optional; defaults to active identity)                                                                                            |
| Flags        | `--json`                                                                                                                                   |
| Output       | `IdentityShowOutput { did, pubkey_hex, lifecycle_state, rotation_history: Vec<IdentityRotationEvent>, hsm_slot, governance_snapshot_ref }` |
| Substrate    | `[ADD] octo_wallet::identity_record(&WalletStore, did: &Did) -> Result<IdentityRecord, WalletError>`                                       |
| Exit codes   | 0, 4 (no such identity), 64                                                                                                                |
| Redaction    | none                                                                                                                                       |
| Side effects | none                                                                                                                                       |
| Dry-run      | n/a                                                                                                                                        |

#### `octo identity rotate`

| Aspect       | Value                                                                                                                                                                                                                                                      |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | none                                                                                                                                                                                                                                                       |
| Flags        | `--confirm` (REQUIRED in human mode), `--dry-run` (no `--grace-hours` flag — substrate hard-codes 24h grace internally; not operator-configurable; see note) |
| Output       | `IdentityRotateOutput { new_did, old_did, grace_expires_at, signature_proof: RedactedHex }`                                                                                                                                                                |
| Substrate    | `[ADD] octo_wallet::begin_rotation(&mut IdentityKey, successor: IdentityKey, now_unix_secs: u64) -> Result<[u8;64], WalletError>` (wraps `IdentityKey::begin_rotation(&mut self, ...)`; substrate hard-codes 24h grace — no `--grace-hours` flag accepted) |
| Exit codes   | 0, 3 (already rotating), 4 (no active identity), 5 (HSM missing), 11 (signing failed), 64                                                                                                                                                                  |
| Redaction    | `signature_proof` MUST be redacted (`RedactedHex` placeholder); `--confirm` required in human mode                                                                                                                                                         |
| Side effects | new DID minted; old key valid for grace period (RFC-0009)                                                                                                                                                                                                  |
| Dry-run      | substrate call wrapped; no state change                                                                                                                                                                                                                    |

#### `octo identity revoke`

| Aspect       | Value                                                                                                                                                      |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | none                                                                                                                                                       |
| Flags        | `--confirm`, `--reason <str>` (required)                                                                                                                   |
| Output       | `IdentityRevokeOutput { did, revoked_at, terminal: true }`                                                                                                 |
| Substrate    | `[ADD] octo_wallet::revoke(&mut IdentityKey, now_unix_secs: u64) -> Result<(), WalletError>` (wraps `IdentityKey::revoke(&mut self, ...)` instance method) |
| Exit codes   | 0, 4 (no active identity), 6 (already revoked — CLI-level pre-check, NOT substrate error), 64                                                              |
| Redaction    | none                                                                                                                                                       |
| Side effects | DID state → `Revoked` (RFC-0009 §Identity Lifecycle — terminal state; revoke during rotation invalidates both old and new keys immediately)                |
| Dry-run      | substrate call wrapped; no state change                                                                                                                    |

#### `octo capability list`

| Aspect       | Value                                                                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Args         | none                                                                                                                                                   |
| Flags        | `--json`, `--filter <field=value>` (repeatable)                                                                                                        |
| Output       | `CapabilityListOutput { capabilities: Vec<CapabilitySummary> }` where `CapabilitySummary { cap_id, root_id, caveats: Vec<CaveatSummary>, expires_at }` |
| Substrate    | `[ADD] octo_cap_macaroon::list_active(filter) -> Result<Vec<CapabilitySummary>, MacaroonError>` (v1.0 Phase-1 concession: CLI parses `--filter <field=value>` strings via `parse_filters` + `matches_filters`; follow-on `[ADD] CapabilityFilter::parse` substrate-side move collapses the CLI-side parser to a thin newtype) |
| Exit codes   | 0, 64                                                                                                                                                  |
| Redaction    | none (caveat names + IDs only; no holder_sig)                                                                                                          |
| Side effects | none                                                                                                                                                   |
| Dry-run      | n/a                                                                                                                                                    |

> **v1.0 reduction note:** `remaining_budget_dqa` deferred to the audit-window sub-amendment per Status header amendment chain (per capability mission amendment chain rationale; octo-cap-macaroon dropped storage dependency in Phase 2c-2 substrate refactor).

#### `octo capability mint`

| Aspect       | Value                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | none                                                                                                                                                                                                                                                                                                                                                                                                  |
| Flags        | `--caveats <json>` (REQUIRED), `--holder <did>` (REQUIRED), `--root <cap_id>` (optional; defaults to wallet root), `--confirm` (REQUIRED in human mode), `--confirm-acknowledge` (REQUIRED for complex-payload mutating commands per §Security 1a), `--dry-run`                                                                                                                                                                                                          |
| Output       | `CapabilityMintOutput { capability_id: Hex32, body_hash: Hex32, caveats: Vec<CaveatSummary>, holder_sig: RedactedHex }`                                                                                                                                                                                                                                                                               |
| Substrate    | `[ADD] octo_cap_macaroon::mint(root_secret: &[u8;32], holder: &dyn CapabilitySigner, holder_did: &str, caveats: &[Caveat]) -> Result<CapabilityToken, MintError>` (thin wrapper around substrate `CapabilityToken::mint` per substrate signature; `holder` is `&dyn CapabilitySigner`, NOT the phantom `HolderKey`; `root_secret` is the 32-byte root signing secret, NOT the 16-byte root public id) |
| Exit codes   | 0, 7 (`--caveats` parse error), 8 (invalid caveat combination per RFC-0960 catalog), 9 (`--holder` not found), 10 (attenuation violation), 5 (HSM missing), 11 (signing failed), 64                                                                                                                                                                                                                   |
| Redaction    | `holder_sig` MUST be redacted (`RedactedHex`)                                                                                                                                                                                                                                                                                                                                                         |
| Side effects | new capability record persisted                                                                                                                                                                                                                                                                                                                                                                       |
| Dry-run      | substrate call wrapped; preview only                                                                                                                                                                                                                                                                                                                                                                  |

#### `octo capability attenuate <id>`

| Aspect       | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | `<cap_id>` (REQUIRED)                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Flags        | `--caveats <json>` (REQUIRED), `--confirm` (REQUIRED in human mode), `--confirm-acknowledge` (REQUIRED for complex-payload mutating commands per §Security 1a), `--dry-run`                                                                                                                                                                                                                                                                                                                                       |
| Output       | `CapabilityAttenuateOutput { child_cap_id, narrowed_from: cap_id, caveats: Vec<CaveatSummary> }`                                                                                                                                                                                                                                                                                                                                                                                   |
| Substrate    | `[ADD] octo_cap_macaroon::attenuate(parent: &CapabilityToken, caveats: &[Caveat], holder: &dyn CapabilitySigner, catalog: &dyn CapabilityCatalog) -> Result<CapabilityToken, MintError>` (substrate `CapabilityToken::attenuate` takes a SINGLE caveat and does NOT re-sign; `attenuate_with_signer` is the re-signing variant; the CLI form loops over `caveats.iter()` calling `parent.attenuate_with_signer(caveat, holder, catalog)` per caveat, threading the result forward) |
| Exit codes   | 0, 7 (`--caveats` parse error), 10 (attenuation violation — widens or caveat removal without narrowing replacement), 12 (parent not found), 64                                                                                                                                                                                                                                                                                                                                     |
| Redaction    | none                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Side effects | new child capability record persisted                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Dry-run      | substrate call wrapped; preview only                                                                                                                                                                                                                                                                                                                                                                                                                                               |

#### `octo policy show <name>`

| Aspect       | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | `<name>` (REQUIRED)                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Flags        | `--version <n:u32>` (default latest), `--kind-uuid <32-lowercase-hex>` (filter — 32 lowercase hex chars = 16-byte UUID form; dashed UUID form NOT accepted; format violation → exit 64), `--json`                                                                                                                                                                                                                                                                       |
| Output       | `PolicyShowOutput { name, kind_uuid, body, execution_class, registered_by_did: Hex32, registered_at_unix, revoked_at_unix?, revoked_by_did?, revocation_reason?, superseding_policy_hash? }`                                                                                                                                                                                                                                                                                |
| Substrate    | `[ADD] octo_policy::show(name, version) -> Result<PolicyRecord, PolicyRegistryError>` (CLI-friendly wrapper returning the new `PolicyRecord` struct; `PolicyRecord` is declared as [ADD] in §Subcommand Taxonomy — substrate today exposes `PolicyRegistry::lookup_policy(policy_hash)` keyed on content hash, the `(name, version)` form is the new addition; error type is substrate's `PolicyRegistryError` per §Subcommand Taxonomy entry #14, NOT a new `PolicyError`) |
| Exit codes   | 0, 13 (not found), 14 (no such version), 64                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Redaction    | `body` content governs; redactor applies to any nested secret fields                                                                                                                                                                                                                                                                                                                                                                                                        |
| Side effects | none                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Dry-run      | n/a                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |

> **v1.0 reduction note:** `signer_set: Vec<Did>` from R0 was reduced to a
> single `registered_by_did: Hex32` for v1.0 — `PolicyRecord.registered_by_did`
> is the substrate's authoritative signer field (added by RFC-0011 substrate
> work alongside the [ADD] `PolicyRecord` struct). Multi-signer governance
> sets (committee signing, quorum attestations) defer to the governance
> amendment per Status header amendment chain.

#### `octo policy list`

| Aspect       | Value                                                                                                                                                                                                                                                                                          |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Args         | none                                                                                                                                                                                                                                                                                           |
| Flags        | `--json`, `--filter <kind>`                                                                                                                                                                                                                                                                    |
| Output       | `PolicyListOutput { policies: Vec<PolicySummary> }` where `PolicySummary { name, kind, execution_class, version }`                                                                                                                                                                             |
| Substrate    | `[ADD] octo_policy::list(filter: &PolicyFilter) -> Result<Vec<PolicyListEntry>, PolicyRegistryError>` (returns new `PolicyListEntry` from octo-policy substrate; distinct from CLI-output `PolicySummary`; error type is substrate's `PolicyRegistryError` per §Subcommand Taxonomy entry #15) |
| Exit codes   | 0, 64                                                                                                                                                                                                                                                                                          |
| Redaction    | none                                                                                                                                                                                                                                                                                           |
| Side effects | none                                                                                                                                                                                                                                                                                           |
| Dry-run      | n/a                                                                                                                                                                                                                                                                                            |

### Output Envelope

Every subcommand emits `OutputEnvelope<T>`:

```rust
#[derive(Serialize, Deserialize, Debug)]
pub struct OutputEnvelope<T> {
    /// Output schema version. Bumped on breaking field changes.
    /// Add-only changes within a major version are permitted.
    pub schema_version: u32,

    /// RFC 3339 UTC timestamp of envelope generation.
    pub generated_at: DateTime<Utc>,

    /// Subcommand-specific data payload.
    pub data: T,

    /// Process exit code (mirrors shell exit code for scripting).
    pub exit_code: i32,

    /// Preview-only / dry-run marker — set to `true` when the command was
    /// invoked with `--dry-run` (or equivalent) and produced a non-mutating
    /// preview. Consumed by renderers and downstream tooling to suppress
    /// commit-style language. Bumped `schema_version` to 2 in this RFC
    /// amendment for this additive field (per §Compatibility — additive
    /// fields are non-breaking but the version bump is the explicit signal
    /// to consumers gating on `schema_version == 1`).
    pub preview_only: bool,
}
```

**Initial `schema_version`: 2** (bumped from 1 in this RFC amendment for the
additive `preview_only` field per §Compatibility).

**`preview_only` semantics:** When `true`, the envelope records that the
command was invoked under `--dry-run` (or an equivalent dry-run surface, see
`OperatorModeFlags.dry_run` in impl-guide §Clap Root Struct) and produced a
non-mutating preview. All mutating subcommands accept `--dry-run` (per
§Subcommand Taxonomy tables); CLI-side dispatch wraps the substrate call in a
dry-run gate that bypasses the substrate write while still returning a
fully-populated `data` payload. Renderers and downstream tooling use this flag
to:

- Suppress commit-style language ("minted", "revoked", "rotated") in favor of
  preview-style language ("preview: would mint").
- Tag the audit-trail entry differently (per Status header amendment chain).

The orthogonal signal is structured (boolean, not derived from `exit_code`)
because a successful dry-run still exits 0 and a successful non-dry-run
mutation also exits 0 — the dry-run provenance cannot be recovered from
`exit_code` alone.

TTY-aware rendering:

| Condition           | Output format                                                                                                                                                                                                                                        |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TTY + no `--json`   | Pretty-printed YAML-like (line per field, colorized via ANSI escape codes when stdout is a TTY AND `--no-color` / `OCTO_FORCE_JSON` is not set; ANSI codes gated by `std::io::IsTerminal` per RFC-0011 §Output Flags). Bumped `schema_version` to 2. |
| Non-TTY OR `--json` | JSON (`serde_json::to_string_pretty`)                                                                                                                                                                                                                |

The `--json` flag forces JSON output regardless of TTY. The `--no-color` flag
disables ANSI color codes (CI environments set `NO_COLOR=1` per convention; CLI
honors this).

### Caveat Catalog

The 9 caveats from the user-facing scratchpad MUST be supported with the following
canonical forms (RFC-0964 envelope):

| Caveat       | Canonical form                                                     | Constraint check                                                                                                                                                |
| ------------ | ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Budget       | `{ "type": "amount_max", "value": "<16-byte-DqaEncoding-hex>" }`   | `Dqa.amount > 0`; `Dqa.scale` is encoded in the wire form (substrate `Caveat::AmountMax(Dqa)`); currency is a SEPARATE `Caveat::AssetBinding(...)` (NOT nested) |
| Expiry       | `{ "type": "before", "value": <u64> }`                             | `value > now_unix` (substrate `Caveat::Before(UnixTimeSecs)`; serde tag `"before"`)                                                                             |
| Vesting      | `{ "type": "valid_after", "value": { "not_before_unix": <u64> } }` | `not_before_unix <= before` if both present (substrate `Caveat::ValidAfter { not_before_unix: u64 }`)                                                           |
| Max uses     | `{ "type": "max_uses", "value": { "count": <u32> } }`              | `count > 0`; `count == 0` means unlimited per substrate (RFC-0965 §3.4 MaxUses entry)                                                                         |
| Model        | `{ "type": "model", "value": "<model_ref>" }`                      | non-empty `model_ref`; multiple `Caveat::Model` compose via logical-AND (no allowlist variant)                                                                  |
| Provider     | `{ "type": "provider", "value": [<ProviderId>, ...] }`             | non-empty list (substrate `Caveat::Provider(Vec<ProviderId>)`; CLI sorts the list for determinism)                                                              |
| Audience     | `{ "type": "audience", "value": "<OverlayIdentity>" }`             | non-empty overlay identity (substrate `Caveat::Audience(OverlayIdentity)` — tuple variant, use `value` not `overlay`)                                           |
| Single use   | `{ "type": "max_uses", "value": { "count": 1 } }`                  | nested `count` form per substrate `Caveat::MaxUses { count: 1 }` (no dedicated `SingleUse` variant; per RFC-0965 §3.4 MaxUses entry)                                       |
| Audit window | `{ "type": "audit_window", "value": { "duration_secs": <u64> } }`  | `1 <= duration_secs <= 86400` (substrate `Caveat::AuditWindow { duration_secs: u64 }` — u64, NOT u32)                                                           |

> **Substrate envelope (RFC-0964 → `Caveat::canonical_ser`):** All variants
> serialize as `{"type": "<tag>", "value": <payload>}` via the substrate's
> tagged-envelope form (`#[serde(tag="type", content="value")]`). Field names below match
> the substrate canonical serializer (`canonical_ser`) — `canonical_ser`
> uses the same `type`/`value` envelope shape regardless of whether the
> payload is a bare scalar or a struct. The `Budget` caveat's value is a
> 16-byte `DqaEncoding` (substrate `Dqa` wire form), NOT a JSON object
> with `amount_dqa`/`scale`/`currency` fields. The `currency` semantic is
> carried by a separate `Caveat::AssetBinding(...)`, not nested inside
> `AmountMax`.
>
> **Scale-binding (Budget caveat):** The substrate `Dqa` type is
> `{ amount: i64, scale: u8 }` (Layer A `octo_determin` substrate, re-exported
> via `octo_policy` per `octo_determin::Dqa` as used in `octo-policy/src/burn_event.rs`).
> The canonical form MUST carry `scale` — without it, a CLI form guessing
> `scale=0` against a parent `scale=6` would yield a child worth 1,000,000x
> the intended amount that still passes narrowing. `PaymentCaveat::attenuate`
> in `crates/octo-cap-macaroon/src/caveat/payment.rs` rejects
> `new_budget.scale != self.budget.scale`; the CLI MUST surface the same
> invariant.

**Parser clamps (RFC-0964 caveat envelope parsing):**

| Limit                    | Value        | Rationale                                                     |
| ------------------------ | ------------ | ------------------------------------------------------------- |
| Total `--caveats` bytes  | ≤ 64 KiB     | DoS guard — limit a single invocation's payload               |
| JSON nesting depth       | ≤ 32         | Stack-bomb guard; RFC-0964 envelopes are flat-by-construction |
| Caveat array length      | ≤ 16 caveats | Catalog exhausts at 9 types; 16 is a sanity upper bound       |
| Per-caveat payload bytes | ≤ 4 KiB      | Largest legitimate use case (large `allow` allowlist)         |

Violations of these clamps produce `OctoCliError::CaveatParse { message }`
(exit 7) with the offending limit named. The clamps are enforced BEFORE the
RFC-0964 catalog validation runs.

The CLI does NOT define new caveat variants — it consumes the RFC-0960 catalog.
Validation errors (exit codes 7, 8, 10) report the specific constraint violation
plus the field name + offending value (redacted if it contains secret material).
Validation error messages pass through the `OctoCliRedactor` before display so
that error output never carries offending secrets verbatim.

### Hex32 newtype

`Hex32` is a `crates/octo-cli/src/output/types.rs` newtype used to render
32-byte digests (capability IDs, body hashes, payload blake3 hashes) in JSON
output as lowercase hex:

```rust
//! crates/octo-cli/src/output/types.rs

#[derive(Serialize, Debug, Clone)]
pub struct Hex32(#[serde(with = "hex::serde")] pub [u8; 32]);
```

`Hex32` is distinct from `RedactedHex` (which renders any inner value as the
`[REDACTED:sig]` placeholder) because 32-byte digests are PUBLIC material —
not secret — and must round-trip through JSON consumers (signature verifiers,
body-hash auditors, payload-fingerprint correlators) without redaction.

The `hex` crate (`hex = "0.4"`) provides `hex::serde` for the lowercase-hex
serialization; no custom `Serialize` impl is needed.

### Redaction Layer

The `OctoCliRedactor` is a `tracing_subscriber::Layer` that strips sensitive substrings
from any log event's `fmt::Display` and `fmt::Debug` output before emission. The
following patterns are redacted:

| Pattern                                     | Redaction placeholder                                                                 |
| ------------------------------------------- | ------------------------------------------------------------------------------------- |
| `seed_bytes` field values                   | `[REDACTED:seed]`                                                                     |
| `private_key` field values                  | `[REDACTED:key]`                                                                      |
| `holder_sig` (Ed25519 64-byte hex)          | `[REDACTED:sig]`                                                                      |
| `pair_code` (6-character alnum)             | `[REDACTED:pair]`                                                                     |
| `password` field values                     | `[REDACTED:pw]`                                                                       |
| `mnemonic` / `seed_phrase` field values     | `[REDACTED:mnemonic]`                                                                 |
| `passphrase` field values                   | `[REDACTED:passphrase]`                                                               |
| `pin` field values (4-8 digit numeric)      | `[REDACTED:pin]`                                                                      |
| `api_key` field values                      | `[REDACTED:api_key]`                                                                  |
| `secret` / `token` field values             | `[REDACTED:secret]`                                                                   |
| `Bearer <token>` in `Authorization` headers | `Bearer [REDACTED:bearer]` (case-insensitive match on `Bearer` / `bearer` / `BEARER`) |

**Dual-pass semantics:** The redactor runs in two passes: (1) field-name pass
that maps `seed_bytes`, `private_key`, `holder_sig`, etc. to placeholders; (2)
value-pattern pass that detects standalone secret-shaped strings (128-hex Ed25519
sig, `password=...` substrings, `seed_bytes=...` substrings) independent of field
name. Both passes must complete before emission. A value that matches BOTH
patterns is redacted only once (the field-name placeholder takes precedence for
consistency).

**Note on placement:** `OctoCliRedactor` is currently staged in
`crates/octo-cli/src/redact.rs`. When the HTTP proxy / Python SDK land per
RFC-0917, this module is candidate for promotion to a shared `octo-redact`
crate. The redactor will NOT be duplicated in those adapters — a single
canonical implementation lives in `octo-redact` and is reused.

The redactor is registered on every `tracing::subscriber` initialization in
`octo-cli`. CLI tests assert that no redacted pattern appears in captured stdout or
stderr (sanitized per §Substrate Error Sanitization).

Stdin secret exposure: when a flag accepting secret material is fed via pipe (non-TTY
stdin), the CLI MUST:

1. Print a warning to stderr: `DANGER: secret material received via pipe. Shell
history may capture this. Use --allow-stdin-secret to override.`
2. Default-refuse (exit code 15) unless `--allow-stdin-secret` is set.
3. Emit an audit log entry tagged `stdin_secret_override=true`. The audit
   entry MUST include a `payload_blake3_hash: Hex32` field that fingerprints the
   stdin payload (blake3, 32-byte digest) so operators can correlate overrides
   without the payload itself leaking into the audit trail.

### Lifecycle Requirements

The CLI is the operator-facing surface for the RFC-0009 identity lifecycle state
machine. Mapping:

| State               | CLI command                                         | Transition direction |
| ------------------- | --------------------------------------------------- | -------------------- |
| `Designated` (0x00) | (no CLI command — substrate-only transition)        | n/a                  |
| `Active` (0x01)     | `octo whoami`, `octo identity show`                 | source               |
| `Rotating` (0x02)   | `octo whoami`, `octo identity rotate` (post-rotate) | target               |
| `Revoked` (0x03)    | `octo whoami`, `octo identity revoke` (post-revoke) | terminal (RFC-0009)  |

```mermaid
stateDiagram-v2
    [*] --> Designated: wallet init
    Designated --> Active: first activation
    Active --> Rotating: octo identity rotate --confirm
    Rotating --> Active: grace period elapses OR abort
    Rotating --> Active: octo identity rotate --abort (out of scope v1.0)
    Active --> Revoked: octo identity revoke --confirm
    Rotating --> Revoked: octo identity revoke --confirm
    Revoked --> [*]: terminal
```

Liveness: no heartbeat (operator-driven). Recovery: `Revoked` is terminal; no
recovery (RFC-0009). Time bounds: rotation grace 1-168 hours (default 24); revoke
is immediate.

The CLI does NOT define a new state machine — it exposes RFC-0009's. The state
diagram above mirrors RFC-0009 §Identity Lifecycle.

### Determinism Requirements

1. **Exit codes** — every subcommand has a fixed exit-code table (§Exit Codes). Same
   input → same exit code across runs, architectures, and timezones.
2. **JSON field order** — `serde_json` uses `preserve_order = false` by default; we
   do NOT depend on field order in tests. `OutputEnvelope<T>` field declaration order
   is the canonical order; consumers MUST NOT rely on JSON key order.
3. **Timestamps** — all `generated_at` values are RFC 3339 UTC with `Z` suffix. No
   local-timezone display.
4. **`--deterministic-time`** flag (optional, gated on RFC-0003 acceptance) — pins
   `Utc::now()` to a fixture for reproducible test runs.

### RFC-0008 Execution Class Mapping

| Operation                   | Class | Rationale                                                               |
| --------------------------- | ----- | ----------------------------------------------------------------------- |
| `octo whoami`               | C     | Operator UX; reads local wallet store only                              |
| `octo identity show`        | C     | Read-only; no consensus impact                                          |
| `octo identity rotate`      | C     | Local keypair rotation; governance snapshot sync is separate (RFC-0009) |
| `octo identity revoke`      | C     | Local state transition; broadcast handled by substrate                  |
| `octo capability list`      | C     | Read-only                                                               |
| `octo capability mint`      | C     | Local signing; no consensus impact                                      |
| `octo capability attenuate` | C     | Local derivation; no consensus impact                                   |
| `octo policy show`          | C     | Read-only                                                               |
| `octo policy list`          | C     | Read-only                                                               |
| Redaction layer             | A     | Affects log emission; deterministic pattern matching                    |
| Output envelope rendering   | A     | Deterministic JSON/YAML serialization                                   |

All operations touching consensus-critical state (settlement, vault mutation,
governance vote) are OUT OF SCOPE for this RFC. They land per Status header
amendment chain.

### Error Handling

`OctoCliError` is a `thiserror` enum. Each variant maps to a fixed exit code (§Exit
Codes) and a user-facing message format.

```rust
#[derive(thiserror::Error, Debug)]
pub enum OctoCliError {
    #[error("clap parse error: {0}")]
    ClapParse(#[from] clap::Error),                         // exit 2 (POSIX usage-error convention)

    #[error("no active identity")]
    NoActiveIdentity,                                       // exit 2

    #[error("--confirm required for mutating command {command} in human mode")]
    ConfirmationRequired { command: String },               // exit 2 (POSIX usage-error)

    #[error("already rotating")]
    AlreadyRotating,                                        // exit 3

    #[error("identity not found: {0}")]
    IdentityNotFound(String),                               // exit 4

    #[error("HSM unavailable: {0}")]
    HsmUnavailable(String),                                 // exit 5

    #[error("identity already revoked")]
    AlreadyRevoked,                                         // exit 6

    #[error("caveat parse error: {message}")]
    CaveatParse { message: String },                        // exit 7

    #[error("invalid caveat combination: {detail}")]
    InvalidCaveatCombination { detail: String },            // exit 8

    #[error("holder not found: {0}")]
    HolderNotFound(String),                                 // exit 9

    #[error("attenuation violation: {0}")]
    AttenuationViolation(String),                           // exit 10

    #[error("signing failed: {0}")]
    SigningFailed(String),                                  // exit 11

    #[error("parent capability not found: {0}")]
    ParentCapNotFound(String),                              // exit 12

    #[error("policy not found: {0}")]
    PolicyNotFound(String),                                 // exit 13

    #[error("policy version not found: {policy}@{version}")]
    PolicyVersionNotFound { policy: String, version: u32 }, // exit 14

    #[error("secret material on pipe; pass --allow-stdin-secret to override")]
    StdinSecretRefused,                                     // exit 15

    #[error("invalid filter: {0}")]
    InvalidFilter(String),                                  // exit 16 (reserved per Status header amendment chain)

    #[error("stub command {name} is stale; use the replacement documented in RFC-0011 §Compatibility")]
    StaleStub { name: String },                             // exit 65

    #[error("internal error: {0}")]
    Internal(String),                                       // exit 64
}
```

User-facing message format:

```
error: <short message>
  caused by: <chain from #[source] if present>
  hint: <remediation hint if available>
  exit code: <N>
```

**Variant sanitization (defense in depth):** Every variant's display string
MUST be passed through `sanitize_substrate_error(s)` before display — not only
`Internal`. The `user_message()` helper on `OctoCliError` runs `to_string()`
output through the sanitizer so any substrate string that slipped through is
caught. Substrate crates SHOULD NOT emit internals-bearing errors at the
boundary, but the sanitizer catches any that slip through. The mapping is
lossy on purpose — the operator sees the category, not the substrate signature.

```rust
pub fn sanitize_substrate_error(s: &str) -> String {
    let mut out = s.to_string();
    // Strip absolute paths after common file:line prefixes
    for prefix in ["src/", "crates/octo-"] {
        // Replace `prefix...` runs with `<substrate-path>`
        while let Some(idx) = out.find(prefix) {
            let end = out[idx..].find(|c: char| c == ' ' || c == '\n' || c == ':' || c == ')')
                .map(|e| idx + e)
                .unwrap_or(out.len());
            out.replace_range(idx..end, "<substrate-path>");
        }
    }
    // Strip SQL fragments after "SQL:" or "query:"
    for marker in ["SQL:", "query:", "sqlite3_open"] {
        while let Some(idx) = out.find(marker) {
            // Replace from marker to next semicolon or newline
            let end = out[idx..].find(|c: char| c == ';' || c == '\n')
                .map(|e| idx + e + 1)
                .unwrap_or(out.len());
            out.replace_range(idx..end, "<substrate-error>");
        }
    }
    // Collapse runs of "<substrate-error>" to a single occurrence
    while out.contains("<substrate-error><substrate-error>") {
        out = out.replace("<substrate-error><substrate-error>", "<substrate-error>");
    }
    out
}
```

This is a defense-in-depth pass: substrate crates SHOULD NOT emit
internals-bearing errors at the boundary, but the sanitizer catches any that slip
through. The mapping is lossy on purpose — the operator sees the category, not
the substrate signature.

### Exit Codes

| Range   | Meaning                                                                                           |
| ------- | ------------------------------------------------------------------------------------------------- |
| 0       | Success                                                                                           |
| 1       | (reserved) — POSIX generic-failure; canonical `SigningFailed` is code 11 (see Appendix D)         |
| 2-15    | Per-`OctoCliError` variant (see table above); includes `ClapParse` (POSIX usage-error convention) |
| 16      | `InvalidFilter` (RFC-0011 Phase 1 first user; reserved range starts at 17)                        |
| 17-63   | Reserved per Status header amendment chain                                                        |
| 64      | Internal / substrate errors (`OctoCliError::Internal`)                                            |
| 65-78   | Reserved for substrate-error sub-discriminator expansion (current: `StaleStub` = 65)              |
| 79-99   | Reserved for future amendment additions per Status header amendment chain                         |
| 100-127 | Reserved for environment errors (missing HSM, missing config dir, etc.)                           |

Exit codes MUST stay in 0-127 (POSIX shell constraint). Exit codes 64-127 are
reserved; never use 128+ (those carry signal info).

## Performance Targets

| Metric               | Target            | Notes                                                     |
| -------------------- | ----------------- | --------------------------------------------------------- |
| Cold start           | <100ms            | Substrate crate load; lazy initialization of wallet store |
| Per-command latency  | <50ms p95         | For read-only commands on warm cache                      |
| Output serialization | <5ms              | `OutputEnvelope<T>` + serde_json                          |
| Redaction overhead   | <1ms per log line | Pattern matching is bounded                               |

The CLI is operator-facing; throughput is not a primary concern. Latency targets
exist to keep the operator experience responsive.

## Implicit Assumptions Audit

> **The "Nothing should be implied" rule (validation layer):** Every assumption the
> design relies on that is not enforced by types, runtime validation, or test
> coverage MUST be listed here.

| Assumption                                               | Where Relied Upon                                                                             | Blast Radius if False                                                                                                  | Mitigation / Status                                                                                                                                                                                                                |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HSM slot is reachable                                    | §Subcommand Taxonomy → `octo identity rotate`, `octo identity revoke`, `octo capability mint` | Mutating commands fail with exit code 5; operator cannot rotate/revoke/mint without HSM                                | Substrate `octo-wallet` enforces HSM path; CLI returns clear error                                                                                                                                                                 |
| Operator config dir is `$OCTO_HOME` or `~/.config/octo/` | §Binary Surface, §Output Envelope                                                             | Commands cannot find wallet/policy/macaroon store; exit code 100 (env error)                                           | `OCI` env var honored; `OCTO_HOME` overrides; `dirs` crate for cross-platform default                                                                                                                                              |
| No concurrent CLI instances per identity                 | All mutating commands                                                                         | Last-writer-wins on rotation/revoke; nonce race on capability mint                                                     | Substrate uses single-writer lock; CLI exit code 101 if another instance detected                                                                                                                                                  |
| Monotonic clock for nonce derivation                     | §Caveat Catalog → RFC-0964 caveat envelope body hash                                          | Replay defense rejects legitimate retries                                                                              | Substrate enforces; CLI inherits                                                                                                                                                                                                   |
| Operator trusts the shell environment (no pastejacking)  | `octo capability mint --caveats <paste>`                                                      | Caveat JSON could be modified by clipboard hijacker                                                                    | CLI shows parsed caveats + asks `--confirm`; CI test asserts confirmation prompt for mutating commands                                                                                                                             |
| Local file permissions on config dir are 0700            | §Binary Surface                                                                               | Other local users can read wallet/capability material                                                                  | `octo init` (deprecated stub) checks; in RFC-0011, the `[ADD] octo-wallet::WalletStore` (per §Subcommand Taxonomy entry #1) enforces 0700 on store creation — substrate today does NOT enforce; the [ADD] makes it substrate-truth |
| RFC-0009 identity substrate is stable                    | All identity subcommands                                                                      | Any breaking change in `octo-wallet` cascades                                                                          | `octo-cli` pins to a major-versioned `octo-wallet`; CI runs against lockfile                                                                                                                                                       |
| Monotonic nonce counter persists across process restarts | §Caveat Catalog → RFC-0964 caveat envelope body hash, §Caveat Catalog → nonce derivation      | Replay defense rejects legitimate retries after restart (counter resets to 0 → legitimate nonces look like duplicates) | Substrate `octo-wallet` persists the counter in the wallet store on disk (Layer B); CLI inherits; integration test asserts counter survives process restart                                                                        |

## Security Considerations

1. **Secret material exposure surface** — CLI accepts `--holder <did>`,
   `--caveats <json>`, and (in `--dev` mode) holder keys via stdin. The redaction
   layer (§Redaction Layer) MUST strip these from all log output. Test vectors
   assert no leak via stderr or log files.

1a. **Atomic pastejacking defense** — When `--confirm` and `--caveats` (or any
flag accepting a complex structured payload) are passed on the SAME
invocation of a mutating command, the CLI MUST print the parsed payload to
stderr in canonical form and require an additional `--confirm-acknowledge`
flag (distinct from `--confirm`) before proceeding. This breaks the
paste-then-spray class of attacks where a clipboard hijacker swaps the
payload after the operator has confirmed. The clap schema enforces this via
a `#[arg(requires = "confirm")]` attribute on `--confirm-acknowledge`: clap
rejects any invocation where `--confirm-acknowledge` is set without
`--confirm` (exit 2). Additionally, dispatch-time checks in `require_confirm`
enforce mutating-action semantics (see §Security 1a). Test vectors assert the
two-step confirmation for `capability mint` and `capability attenuate`.

2. **Command injection via caveat names** — `--caveats <json>` is parsed by
   `serde_json` then validated against the RFC-0964 catalog. Parser errors yield
   exit code 7 with the offending field. Catalog validation yields exit code 8.

3. **Replay via cloned nonce** — `octo capability mint` derives its nonce from
   `body_hash + holder_did + monotonic_counter`. The CLI MUST NOT accept `--nonce`
   as a flag (no operator override path). Substrate `octo-cap-macaroon` rejects
   duplicate nonces with `SettlementError::AlreadyConsumed`.

4. **Downgrade to `--dev`** — `InMemorySigner` is dev-only. Default-deny:
   `InMemorySigner` is selected ONLY when `OCTO_ENV` is explicitly set to
   `development`. Any other value (including unset) routes through the HSM
   signer. The CLI refuses to start `--dev` mode otherwise and exits with code 5
   (`HsmUnavailable`). CI tests assert default-HSM-only across the full matrix
   (unset, `production`, `staging`, `test`, `dev`).

5. **Capability replay via `--root <cap_id>` reuse** — `--root` is an explicit
   operator choice. The CLI surfaces the root's caveats in `--dry-run` output; the
   operator confirms. No silent reuse.

6. **Policy body disclosure** — `octo policy show` may print `body_json` containing
   sensitive fields (e.g., approver DIDs, governance set hashes). The redactor
   applies to any nested secret fields per §Redaction Layer.

7. **Stdin secret exposure** — see §Redaction Layer exit code 15.

## Adversarial Review

| Threat                                                                    | Impact                                                           | Mitigation                                                                                                                                                                       |
| ------------------------------------------------------------------------- | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pastejacking modifies `--caveats` JSON between paste and submission       | High — attacker widens budget on legitimate capability           | `--dry-run` shows parsed caveats before `--confirm`; CI asserts confirmation prompt for mutating commands                                                                        |
| Log scraping via `tracing-subscriber` file layer                          | High — full holder_sig could leak                                | `OctoCliRedactor` strips patterns before file write; test asserts no secret in log fixtures                                                                                      |
| Operator runs CLI on shared host with file permissions 0755 on config dir | High — other local users can read wallet                         | `[ADD] octo-wallet::WalletStore` enforces 0700 on store creation (per §Subcommand Taxonomy entry #1); CLI fails fast on permission mismatch                                      |
| Attacker injects `Bearer` header via crafted error message                | Medium — token leaks to logs                                     | Redactor strips `Bearer <token>` pattern from all output                                                                                                                         |
| Operator pastes private key into `--holder-key` flag value by accident    | Medium — key in shell history                                    | `--dev` mode requires explicit flag; `InMemorySigner` refuses `--holder-key` from non-TTY stdin without `--allow-stdin-secret`                                                   |
| Attacker exploits TTY detection to downgrade `--json` output              | Low — operator receives pretty JSON instead of machine-readable  | `--json` flag overrides TTY detection; CI tests both paths                                                                                                                       |
| Concurrent CLI invocation races on nonce generation                       | Medium — capability mint collides                                | Substrate uses single-writer lock per identity; CLI exit code 101 if another instance detected                                                                                   |
| Caveat removal widens capability surface (attenuation widens via subset)  | High — child capability with FEWER caveats grants more authority | Attenuation check MUST verify every parent caveat is present in the child set OR the child carries a strictly narrower form of the same caveat (per RFC-0957 §Attenuation Rules) |
| Atomic pastejacking swaps payload between confirm and submit              | High — caveat JSON replaced after operator reviewed              | `--confirm-acknowledge` two-step gate; CI asserts the gate for `capability mint` + `capability attenuate`                                                                        |

## Adversary Analysis

> **The 5-Question Adversary Test:** For every design decision with security
> implications, enumerate: (1) who benefits, (2) what it costs them, (3) what they
> gain if successful, (4) what's our defense and its cost, (5) what's the residual
> risk.

### Pastejacking on `--caveats` JSON

1. **Who benefits?** — Attacker on the same physical/logical clipboard plane as the
   operator. Likely malware or compromised browser extension.
2. **What does it cost them?** — Crafting a JSON payload that respects RFC-0964
   catalog while widening a caveat (e.g., removing `before`, increasing
   `amount_dqa`). Moderate technical effort.
3. **What do they gain if successful?** — Persistent capability with widened
   budget or extended expiry. Operational cost only (no funds at risk in this RFC;
   mitigated downstream by `--root` governance).
4. **What's our defense?** — `--dry-run` shows parsed caveats; `--confirm` requires
   explicit operator approval. Cost: extra interaction per command (operator UX
   friction).
5. **Residual risk?** — Operator may skip `--dry-run` or auto-confirm. ACCEPTED
   RISK: documented in §Out-of-scope roles (operator trust model).

### Stdin secret exfiltration via `--holder-key`

1. **Who benefits?** — Local-process attacker (e.g., malware on dev box) that can
   read stdin or hijack pipe.
2. **What does it cost them?** — Knowing the CLI accepts `--holder-key` via stdin.
   Trivial — observable from `octo capability mint --help`.
3. **What do they gain if successful?** — Holder private key for offline signing.
4. **What's our defense?** — `--dev` mode required for `InMemorySigner`; default
   rejection of pipe-stdin secrets (exit code 15); `--allow-stdin-secret` requires
   explicit override + audit log entry. Cost: requires operator education (documented
   in release notes).
5. **Residual risk?** — Operator enables `--allow-stdin-secret` reflexively. ACCEPTED
   RISK: documented in §Things you must NOT do.

### Concurrent CLI invocation on capability mint

1. **Who benefits?** — Operator running two CLI windows in parallel (legitimate) or
   attacker forcing race (unusual).
2. **What does it cost them?** — N/A for legitimate case; trivial for attacker.
3. **What do they gain if successful?** — Duplicate capability minted with colliding
   nonce (rejected by substrate); no actual gain.
4. **What's our defense?** — Substrate single-writer lock per identity; CLI exits 101
   if another instance detected via filesystem lock. Cost: operators cannot
   trivially parallelize mutating commands.
5. **Residual risk?** — Filesystem lock can be bypassed on read-only mounts. ACCEPTED
   RISK: documented in §Implicit Assumptions Audit (config dir permissions).

### Downgrade to `--dev`

1. **Who benefits?** — Attacker controlling the operator's environment variables
   (e.g., a malicious `~/.bashrc` snippet, a compromised CI step, or
   process-level env injection via a desktop session). Goal: force
   `InMemorySigner` so the holder key materializes in process memory and is
   readable via `/proc/<pid>/mem` or core dumps.
2. **What does it cost them?** — Writing one env-var assignment. Trivial.
3. **What do they gain if successful?** — Holder private key in cleartext in
   process memory; recoverable via memory dump or post-mortem. With it, the
   attacker can mint capabilities as the operator offline.
4. **What's our defense?** — Default-deny: `InMemorySigner` ONLY when
   `OCTO_ENV == "development"` (exact match). All other values, including unset,
   route to HSM. The CLI emits a one-line startup banner stating the active
   signer, and exits with code 5 if `--dev` is requested in a non-dev
   environment. Cost: operators must explicitly opt-in to dev mode even in
   dev environments (1-line `.env` setup).
5. **Residual risk?** — Operator explicitly sets `OCTO_ENV=development` in
   their interactive shell and forgets. ACCEPTED RISK: documented in
   §Things you must NOT do; mitigated by shell rcfile review process.

## Economic Analysis

The CLI has no direct economic surface. It does not create or transfer value; it
exposes substrate capabilities (identity, capability, policy) for operator
inspection and modification. Economic implications (vault creation, settlement,
governance vote) land per Status header amendment chain. This RFC defers any
reference to the dual-stake model until those amendments.

## Compatibility

### Stub command compatibility

The current stub commands (`init`, `join`, `role`, `agent`, `status`) are preserved
in v1.0 as deprecated wrappers:

- `init` — prints init banner + deprecation warning pointing to
  `octo-wallet init` (out of scope for this RFC; lands via RFC-0011 wallet
  substrate amendment).
- `join` — prints join banner + deprecation warning pointing to
  `octo network bootstrap` (out of scope for v1.0).
- `role {builder,provider,storage,bandwidth,orchestrator}` — prints role banner +
  deprecation warning pointing to the role-provisioning amendment per Status
  header amendment chain.
- `agent {create,run,list}` — prints agent banner + deprecation warning pointing
  to the agent-lifecycle amendment per Status header amendment chain.
- `status` — prints status banner + deprecation warning.

Removal timeline:

| Version           | Stub command behavior                               |
| ----------------- | --------------------------------------------------- |
| v1.0 (this RFC)   | Hide from `--help`; deprecation warning on use      |
| v1.1 (next minor) | Emit hard error (exit code 65 (`StaleStub`)) on use |
| v2.0 (next major) | Remove entirely                                     |

Per RFC migration etiquette: 1 release cycle deprecation window + 1 release cycle
hard-error window before removal.

#### Stale-stub window env-var override (v1.1 hard-error opt-in)

The v1.1 release turns the v1.0 deprecation banners into hard errors
(exit code 65 `StaleStub`). Operators MAY opt into the v1.1 hard-error
behaviour on a v1.0 binary by setting the environment variable
`OCTO_STALE_STUB_WINDOW=1` (or `=true`, case-insensitive). The v1.1
release compile flag (set at build time) takes precedence; the env-var
override is the v1.0 operator escape hatch. Other values (absent,
empty, or any other string) leave the v1.0 banner-only behaviour
unchanged.

The canonical banner prefix is `DEPRECATED:` (uppercase, with trailing
colon). All stub command output is emitted on **stderr** (not stdout)
so JSON consumers piping `octo ... --json` through `jq` see a clean
parseable stdout stream. The banner MUST NOT leak to stdout under any
configuration.

### Backward compatibility for output schemas

Adding a field to `OutputEnvelope<T>` or to a subcommand's data type is a non-breaking
change (consumers ignore unknown fields by default). Removing a field, renaming a
field, or changing a field's type requires:

1. Bump `OutputEnvelope.schema_version`
2. Companion RFC amendment documenting the breaking change

## Test Vectors

At least 30 test vectors are required (current count exceeds the floor by ~70% — see missions + impl-guide for the running tally), distributed as:

| Group               | Count | Examples                                                                                                                                                 |
| ------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identity commands   | 5     | whoami-success, identity-show-not-found, identity-rotate-confirm-required, identity-rotate-grace-hours-flag-absent, identity-revoke-already-revoked      |
| Capability commands | 5     | capability-list-empty, capability-mint-success, capability-mint-bad-caveats, capability-attenuate-widens-rejected, capability-attenuate-parent-not-found |
| Policy commands     | 5     | policy-show-success, policy-show-not-found, policy-list-filter, policy-show-version-mismatch, policy-list-empty                                          |
| Error envelope      | 5     | Internal error message format, exit code mapping, clap parse error propagation, source chain rendering, no-substrate-internals leak                      |
| Output envelope     | 5     | schema_version present, generated_at format RFC 3339 UTC, json-vs-pretty toggle, TTY detected vs not, --no-color honored                                 |
| Redaction layer     | 5     | holder_sig stripped from log, pair_code stripped from stderr, bearer token stripped, password field stripped, seed_bytes stripped                        |
| Stub command banner | 5     | init-banner-on-stderr, join-banner-on-stderr, role-builder-banner-on-stderr, agent-list-banner-on-stderr, status-banner-on-stderr (each asserts `DEPRECATED:` prefix on stderr + stdout cleanliness; see §Compatibility stub-banner contract) |

Test vectors are specified in YAML form in the companion implementation guide
(`docs/07-developers/octo-cli-implementation-guide.md`). The CLI integration tests
use `assert_cmd` for binary invocation + `assert_json` for output schema validation.

## Alternatives Considered

| Approach                                                                 | Pros                                                   | Cons                                                                                                          |
| ------------------------------------------------------------------------ | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| REST API instead of CLI                                                  | Standard tooling (curl, Postman); trivially scriptable | Loses operator UX (auto-completion, hint messages); loses offline-first property                              |
| TUI (interactive) instead of CLI                                         | Better UX for complex workflows                        | Not scriptable; defeats automation use case; harder to embed in CI                                            |
| Separate binary per command group (`octo-id`, `octo-cap`, `octo-policy`) | Smaller surface per binary                             | Violates "one canonical CLI" principle; operator must remember 5+ binary names; CI scripts grow               |
| YAML/JSON config files instead of CLI flags                              | Declarative; git-friendly                              | Loses interactive validation; requires upfront schema knowledge                                               |
| Plugin-based architecture (`octo` + `octo-plugin-identity` etc.)         | Extensibility                                          | Premature; not justified by current scope; Layer E pattern documented in CLAUDE.md but not yet needed for CLI |

## Implementation Phases

This RFC covers Phase 1 only. Follow-on amendments cover Phases 2-8.

### Phase 1 (this RFC)

- Identity subcommands: `whoami`, `identity {show,rotate,revoke}`
- Capability subcommands: `capability {list,mint,attenuate}` (`capability list` carries a Phase-1 concession: CLI parses `--filter <field=value>` strings via `parse_filters` + `matches_filters`; follow-on `[ADD] CapabilityFilter::parse` substrate-side move collapses the CLI-side parser to a thin newtype — see §Subcommand Taxonomy `capability list` Substrate row)
- Policy subcommands: `policy {show,list}`
- Output envelope (`OutputEnvelope<T>` + TTY-aware rendering)
- Redaction layer (`OctoCliRedactor` tracing layer)
- Error envelope (`OctoCliError` enum + exit code table)
- Stub command deprecation warnings

### Phase 2 (audit, future amendment per Status header amendment chain)

- Audit subcommands: `audit {list,show}`
- Depends on RFC-0965 audit-window caveat + RFC-0959 settlement substrate wiring

### Phase 3 (reputation, future amendment per Status header amendment chain)

- Reputation subcommands: `reputation show`
- Depends on RFC-0968 reputation substrate

### Phase 4 (agent lifecycle, future amendment per Status header amendment chain)

- Agent lifecycle subcommands: `agent {create,run,list,destroy,attach}`
- Depends on `octo-runtime` + agent manifest substrate (RFC-0002 territory)

### Phase 5 (role provisioning, future amendment per Status header amendment chain)

- Role provisioning subcommands: `role {select,list,show}`
- Depends on RFC-0900+ economic substrate + RFC-0855+ governance

### Phase 6 (vault operations, future amendment per Status header amendment chain)

- Vault operations subcommands: `vault {list,balance,transfer}`
- Depends on RFC-0960 (vault balance projection)

### Phase 7 (mesh operations, future amendment per Status header amendment chain)

- Mesh operations subcommands: `mesh {peer,forward,rpc}`
- Depends on RFC-0871 node protocol wiring

### Phase 8 (governance, future amendment per Status header amendment chain)

- Governance subcommands: `governance {snapshot,attest,vote}`
- Depends on RFC-0855 governance

## Key Files to Modify

### DOC-ONLY (this RFC cycle)

- `rfcs/draft/process/0011-octo-cli-substrate.md` — this file
- `docs/07-developers/octo-cli-implementation-guide.md` — companion guide
- `missions/open/0011-core-output-envelope-redaction.md` — mission 1
- `missions/open/0011-identity-commands.md` — mission 2
- `missions/open/0011-capability-commands.md` — mission 3
- `missions/open/0011-policy-commands.md` — mission 4
- `missions/open/0011-deprecation-stub-removal.md` — mission 5

### SUBSTRATE (follow-on missions, NOT this RFC cycle)

- `crates/octo-cli/Cargo.toml` — add deps (octo-wallet, octo-cap-macaroon, octo-policy, chrono, schemars, dirs)
- `crates/octo-cli/src/main.rs` — REPLACE 213-line stub with full clap struct
- `crates/octo-cli/src/output.rs` — NEW, `OutputEnvelope<T>` + TTY-aware rendering
- `crates/octo-cli/src/redact.rs` — NEW, `OctoCliRedactor` tracing layer
- `crates/octo-cli/src/error.rs` — NEW, `OctoCliError` enum
- `crates/octo-cli/src/commands/mod.rs` — NEW, subcommand dispatch
- `crates/octo-cli/src/commands/identity.rs` — NEW, identity subcommand impls
- `crates/octo-cli/src/commands/capability.rs` — NEW, capability subcommand impls
- `crates/octo-cli/src/commands/policy.rs` — NEW, policy subcommand impls
- `crates/octo-cli/src/commands/stub.rs` — NEW, deprecated stub wrappers

## Future Work

- `octo-cli trust graph` — proposed in mission 0851p-a; depends on `octo-bootstrap-cli`
  substrate (does not yet exist)
- `octo-cli whatsapp-onboard` — adapter CLI pattern reference; see whatsapp CLI
  substrate RFC when filed (CLI substrate prerequisite established here)
- `octo-cli telegram-onboard` — adapter CLI pattern reference; see telegram CLI
  substrate RFC when filed (CLI substrate prerequisite established here)
- `--deterministic-time` flag — gated on RFC-0003 acceptance
- `OutputEnvelope<T>` JSON Schema export — add `schemars` derive on output structs;
  publish schemas to `docs/schemas/octo-cli/` for consumer tooling

## Rationale

### Why a single RFC with amendment chain

The CLI surface spans 8 substrate dependencies (identity, capability, audit,
reputation, agent, role, vault, mesh, governance). Folding all of them into one
RFC would exceed the 1000-line decomposition threshold in BLUEPRINT.md §Multi-Mission
Decomposition. The amendment chain mirrors the existing pattern (RFC-0960 + RFC-0010 + RFC-0959
multi-amendment precedents) and keeps each amendment focused on one substrate
dependency.

### Why Draft status (not Planned)

Per BLUEPRINT.md §The RFC Process: "1. Draft RFC in `rfcs/draft/{category}/XXXX-title.md`.
Draft = full template, open for discussion, 7-day minimum review." User explicitly
requested "full spec". Draft is the correct landing.

### Why Layer C/D placement

Per CLAUDE.md §Architectural Principles:

- `octo-cli` is an **operator-facing orchestrator** that pulls in Layer-B substrate
  crates (`octo-wallet`, `octo-cap-macaroon`, `octo-policy`). It depends on them;
  they do not depend on it.
- The CLI does NOT introduce new Layer-A or Layer-B types. All new types
  (`OutputEnvelope<T>`, `OctoCliRedactor`, `OctoCliError`, clap structs) are pure
  Layer-C operator UX.
- The redaction layer is D-adjacent (transport-aware) because it must handle stdout
  vs stderr vs log-file vs pipe differently.

### Why TTY-aware output

Operators use the CLI interactively (TTY → pretty) and in automation (CI, scripts
→ JSON). The TTY detection + `--json` override pattern is the industry standard
(used by `gh`, `kubectl`, `cargo`, `rustc`). It keeps operator UX excellent while
preserving machine readability.

### Why deprecated stub commands

The current stub (`init`, `join`, `role`, `agent`, `status`) is the only "working"
CLI today. Breaking it on RFC Accept would block operators. Deprecation + 1-release
grace + hard-error window = standard migration etiquette.

## Version History

| Version | Date       | Changes                                                                      |
| ------- | ---------- | ---------------------------------------------------------------------------- |
| 1.0     | 2026-08-27 | Initial draft (process — identity/capability/policy slice)                   |
| 1.1     | TBD        | Amendment: audit subcommands (per Status header amendment chain)             |
| 1.2     | TBD        | Amendment: reputation subcommands (per Status header amendment chain)        |
| 1.3     | TBD        | Amendment: agent lifecycle subcommands (per Status header amendment chain)   |
| 1.4     | TBD        | Amendment: role provisioning subcommands (per Status header amendment chain) |
| 1.5     | TBD        | Amendment: vault operations subcommands (per Status header amendment chain)  |
| 1.6     | TBD        | Amendment: mesh operations subcommands (per Status header amendment chain)   |
| 1.7     | TBD        | Amendment: governance subcommands (per Status header amendment chain)        |

## Related RFCs

- RFC-0009 — Identity Management (lifecycle state machine + rotation grace)
- RFC-0957 — Macaroon Substrate (capability token structure)
- RFC-0964 — Constraint Encoding (caveat envelope canonical form)
- RFC-0960 — Vaults, Capabilities, Reservations (caveat catalog root)
- RFC-0967 — Policy Object Graph (policy show/list substrate)
- RFC-0958 — ZK Capability Subclass (`HolderKind::ZKBearing` flag handling)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping)

## Related Use Cases

- `docs/use-cases/hybrid-ai-blockchain-runtime.md`

## Appendices

### A. Subcommand Reference

Full clap tree with help strings:

```mermaid
graph TD
    root[octo] --> whoami["whoami<br/>Show the active identity"]
    root --> identity[identity]
    root --> capability[capability]
    root --> policy[policy]
    root --> init["init<br/>[DEPRECATED]"]
    root --> join["join<br/>[DEPRECATED]"]
    root --> role["role<br/>[DEPRECATED]"]
    root --> agent["agent<br/>[DEPRECATED]"]
    root --> status["status<br/>[DEPRECATED]"]

    identity --> id_show["show [DID]<br/>Show identity details"]
    identity --> id_rotate["rotate<br/>Begin key rotation (24h grace default)"]
    identity --> id_revoke["revoke<br/>Revoke the active identity (terminal)"]

    capability --> cap_list["list<br/>List active capabilities"]
    capability --> cap_mint["mint<br/>Mint a new capability"]
    capability --> cap_attenuate["attenuate &lt;cap_id&gt;<br/>Narrow a capability"]

    policy --> pol_show["show &lt;name&gt;<br/>Inspect a policy"]
    policy --> pol_list["list<br/>List all policies"]

    role --> r_builder[builder]
    role --> r_provider[provider]
    role --> r_storage[storage]
    role --> r_bandwidth[bandwidth]
    role --> r_orchestrator[orchestrator]

    agent --> a_create["create &lt;name&gt;"]
    agent --> a_run["run &lt;name&gt;"]
    agent --> a_list2[list]

    classDef deprecated fill:#999,stroke:#666,color:#fff,stroke-dasharray: 5 5
    class init,join,role,agent,status,r_builder,r_provider,r_storage,r_bandwidth,r_orchestrator,a_create,a_run,a_list2 deprecated
```

Flags (root-level, global):

| Flag                    | Effect                                                      |
| ----------------------- | ----------------------------------------------------------- |
| `--json`                | Force JSON output (overrides TTY)                           |
| `--no-color`            | Disable ANSI color codes                                    |
| `--mode <kind>`         | Operator mode: `human` / `ci` / `auditor`                   |
| `--allow-write`         | Required for mutating commands in `ci` mode                 |
| `--confirm`             | Required for mutating commands in `human` mode              |
| `--dry-run`             | Preview without state mutation                              |
| `--allow-stdin-secret`  | Required for secret material via pipe                       |
| `--confirm-acknowledge` | Two-step confirmation for complex-payload mutating commands |

### B. JSON Output Schemas

Per-subcommand JSON schemas are specified in the companion implementation guide
(`docs/07-developers/octo-cli-implementation-guide.md`). Each output type derives
`schemars::JsonSchema` so the schema can be exported at build time.

### C. Caveat Catalog

See §Caveat Catalog in the main body. 9 caveat types supported (user-facing
names): `budget`, `before` (expiry), `valid_after` (vesting), `max_uses`,
`model`, `provider` (any-of provider list — not `destinations`), `audience`,
`single_use` (canonical form `max_uses` with `count=1` per RFC-0965 §3.4
MaxUses entry — not a separate wire tag), `audit_window`. Canonical
forms in RFC-0964 envelope.

### D. Exit Code Table

| Code    | Variant                                                   | Meaning                                                                                                                                                                     |
| ------- | --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0       | (success)                                                 | Command succeeded                                                                                                                                                           |
| 1       | (reserved)                                                | Reserved (POSIX convention reserves 1 for generic-failure; canonical `SigningFailed` is code 11)                                                                            |
| 2       | `ClapParse` / `NoActiveIdentity` / `ConfirmationRequired` | Clap usage-error OR no active identity in the wallet store OR mutating command invoked without `--confirm` in human mode (POSIX convention: clap parse → 2)                 |
| 3       | `AlreadyRotating`                                         | Identity is already in `Rotating` state                                                                                                                                     |
| 4       | `IdentityNotFound`                                        | Identity with given DID not found                                                                                                                                           |
| 5       | `HsmUnavailable`                                          | HSM slot not reachable                                                                                                                                                      |
| 6       | `AlreadyRevoked`                                          | Identity is already `Revoked` (CLI-level pre-check, NOT substrate error — substrate `IdentityKey::revoke` is idempotent from `Revoked` and does NOT raise `AlreadyRevoked`) |
| 7       | `CaveatParse`                                             | `--caveats` JSON parse error                                                                                                                                                |
| 8       | `InvalidCaveatCombination`                                | Caveat violates RFC-0964 catalog                                                                                                                                            |
| 9       | `HolderNotFound`                                          | `--holder` DID not found                                                                                                                                                    |
| 10      | `AttenuationViolation`                                    | Child caveat widens parent                                                                                                                                                  |
| 11      | `SigningFailed`                                           | HSM signing failed during capability mint/attenuate                                                                                                                         |
| 12      | `ParentCapNotFound`                                       | Attenuation parent capability not found                                                                                                                                     |
| 13      | `PolicyNotFound`                                          | Policy with given name not found                                                                                                                                            |
| 14      | `PolicyVersionNotFound`                                   | Policy version out of range                                                                                                                                                 |
| 15      | `StdinSecretRefused`                                      | Secret on pipe without `--allow-stdin-secret`                                                                                                                               |
| 16      | `InvalidFilter`                                           | Invalid `--filter` form (reserved range per Status header amendment chain)                                                                                                  |
| 17-63   | (reserved)                                                | Future amendments per Status header amendment chain                                                                                                                         |
| 64      | `Internal`                                                | Substrate error (sanitized)                                                                                                                                                 |
| 65      | `StaleStub`                                               | Deprecated stub command invoked past v1.1 hard-error window                                                                                                                 |
| 66-78   | (reserved)                                                | Future substrate-error sub-discriminator expansion                                                                                                                          |
| 79-99   | (reserved)                                                | Future amendment additions                                                                                                                                                  |
| 100-127 | (env errors)                                              | Missing config dir, permission mismatch, etc.                                                                                                                               |

### E. Redaction Pattern Examples

Sample log lines before and after redactor:

```text
BEFORE:
  INFO octo_cli::commands::capability: minted cap_id=01ab.. holder_sig=4e7f..9d2a body_hash=ab12..cd34

AFTER:
  INFO octo_cli::commands::capability: minted cap_id=01ab.. holder_sig=[REDACTED:sig] body_hash=ab12..cd34
```

```text
BEFORE:
  ERROR octo_cli::commands::identity: HSM slot returned password=hunter2 for slot=0

AFTER:
  ERROR octo_cli::commands::identity: HSM slot returned password=[REDACTED:pw] for slot=0
```

```text
BEFORE:
  TRACE octo_cli::auth: Authorization: Bearer eyJhbGciOiJIUzI1NiJ9...

AFTER:
  TRACE octo_cli::auth: Authorization: Bearer [REDACTED:bearer]
```

The redactor is a `tracing_subscriber::Layer` that runs on every event before the
formatter. Implementation detail in the companion implementation guide.

---

**Submission Date:** 2026-08-27
**Last Updated:** 2026-08-27
**Changes:**

- Initial draft (this version)
