---
name: 0011-d-role-subcommands-phase1
description: Implement `octo role {list,show,select}` Phase 1 (7 base roles) per RFC-0011-d; aggregates 9 atomic missions M1-M9 per RFC §Mission Decomposition table.
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.3"
  completed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
    - mission 0011-d-M1-octorole-crate-skeleton
    - mission 0011-d-M2-octorole-types-and-errors
    - mission 0011-d-M3-octorole-list-show
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
    - mission 0011-d-M5-octowallet-nonce-counter
    - mission 0011-d-M6-octocli-role-commands
    - mission 0011-d-M7-octocli-role-error-variants
    - mission 0011-d-M8-octocli-role-tests
    - mission 0011-d-M9-doc-followon-token-design-md-10
status: Completed
---

# 0011-d-role-subcommands-phase1 — Role subcommands Phase 1 (list/show/select, 7 base roles)

**Status:** Completed 2026-09-01 (aggregate) — All 9 Phase 1 atomic missions (M1-M9) closed at `missions/archived/completed/0011-d-M{1..9}-*.md` per audit `docs/audits/2026-09-01-rfc-0011-d-mission-dry-closure.md`. Landing commits `003a7b0f` / `043f8543` / `91c42ab2` / `3842e8c6` / `e07e85b0` / `63ffdf94` / `90ce73bd` (M1-M8 substrate) + M9 doc edit (drift-fix commit `c6f9ab6c`; §10 footnote in `token-design.md` + role-provisioning RFC cite now consistent). M9 closed via policy override per `[[deferred-vs-unspecified]]` "deferred not unspecified" — release gate (first dual-stake role addition) is bookkeeping artifact; substantive work delivered. Gate event re-fires when M10/M11 land (CLOSED 2026-09-03 at commit `96bccc4b` + drift-fix `c6f9ab6c`). Phase 1 fully closed; moved to `missions/archived/completed/` 2026-09-06.
**Substrate:** RFC-0011-d §Specification §7.1–§7.7 (role subcommand group)
**Parent:** RFC-0011-d
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root
- Mission `0011-identity-commands` — `WalletStore::active_signer()` exposes `Arc<dyn CapabilitySigner>` (parent RFC-0011 §Subcommand Taxonomy entry #10); `role select` reuses this helper
- Mission `0011-capability-commands` — `RedactedHex` + `Hex32` newtype wrappers
- Mission `0011-policy-commands` — filter parser pattern + `OutputEnvelope<T>` integration

## Status

Claimed (2026-09-01) by @mmacedoeu — unblocked. Phase 1 of the RFC-0011-d amendment; covers 7 base roles (builder, provider, storage, bandwidth, orchestrator, recorder, wallet). Coordinator + domain-coordinator subcommands are Phase 2 (separate mission `0011-d-role-subcommands-phase2`, gated on RFC-0855p-d AND RFC-0855p-e Accepted).

## Atomic Decomposition (claim units)

This phase-aggregate mission decomposes per RFC-0011-d §Mission Decomposition into 9 atomic claim units (claim these individually, NOT this aggregate):

| #   | Mission file                                   | Substrate scope                                                                            | Acceptance gate                                     |
| --- | ---------------------------------------------- | ------------------------------------------------------------------------------------------ | --------------------------------------------------- |
| M1  | `0011-d-M1-octorole-crate-skeleton.md`         | `crates/octo-role/` scaffold                                                               | `cargo build -p octo-role` clean                    |
| M2  | `0011-d-M2-octorole-types-and-errors.md`       | substrate types + `RoleError` + `RoleAction`                                               | `cargo test -p octo-role` types pass                |
| M3  | `0011-d-M3-octorole-list-show.md`              | `octo_role::list` + `octo_role::show`                                                      | TV-RL-1..3 + TV-RS-1..3 pass                        |
| M4  | `0011-d-M4-octorole-select-with-stoolap-tx.md` | `octo_role::select` (write-path)                                                           | TV-RX-1..4 pass                                     |
| M5  | `0011-d-M5-octowallet-nonce-counter.md`        | wallet `OctoRoleBinding` cached projection + `next_nonce_counter`                          | TV-WAL-1..9 pass                                    |
| M6  | `0011-d-M6-octocli-role-commands.md`           | clap `octo role {list,show,select}`                                                        | M6 unit tests pass                                  |
| M7  | `0011-d-M7-octocli-role-error-variants.md`     | 4 `OctoCliError` variants (exit 31/32/33/35; 34 reserved)                                  | TV-ERR-1..6 pass                                    |
| M8  | `0011-d-M8-octocli-role-tests.md`              | 11 YAML test vectors (TV-RL-1..3 + TV-RS-1..3 + TV-RX-1..4 + TV-RP-1) + assert_cmd harness | TV-RL-1..3 + TV-RS-1..3 + TV-RX-1..4 + TV-RP-1 pass |
| M9  | `0011-d-M9-doc-followon-token-design-md-10.md` | pure-doc `token-design.md` §10                                                             | prettier PASS, cite sweep 0 INVALID                 |

**Workflow:** Claim M1 → M2 → ... → M8 sequentially; M9 may claim in parallel with M4-M8 (pure-doc). Each atomic is a single PR. This aggregate remains Open as the Phase 1 summary; do not claim this aggregate — claim the atomics.

**Cross-reference:** Phase 2 atomics live under `0011-d-M10-phase2-coordinator-domain-coordinator.md` + `0011-d-M11-phase2-domain-coordinator-platform-binding.md` (GATED on RFC-0855p-d + RFC-0855p-e Accepted).

## Substrate (RFC-0011-d)

RFC-0011-d §7.2 Subcommand Taxonomy (`octo role list` / `octo role show` / `octo role select`); §7.3 Output Envelopes (`RoleListOutput`, `RoleShowOutput`, `RoleSelectOutput`); §7.4 Substrate `[ADD]` signatures (`octo_role::list`, `octo_role::show`, `octo_role::select`); §7.5 Role Summary; §7.6 Role Select — HSM Signing Flow; §7.7 Redaction.

## Parent

RFC-0011-d (process — `octo role` provisioning subcommands).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: core → identity → capability → policy → role (Phase 1).

## Acceptance Criteria

- [ ] `octo role list` implemented + unit-tested (TV-RL-1, TV-RL-2, TV-RL-3 pass per RFC-0011-d §Test Vectors)
- [ ] `octo role show <role>` implemented + unit-tested (TV-RS-1, TV-RS-2, TV-RS-3 pass)
- [ ] `octo role select <role>` for `builder`, `provider`, `storage`, `bandwidth`, `orchestrator`, `recorder`, `wallet` implemented + unit-tested (TV-RX-1, TV-RX-2, TV-RX-3, TV-RX-4 pass per RFC-0011-d §11)
- [ ] Partial-prereq guard implemented + unit-tested (TV-RP-1 pass: `coordinator` + `domain-coordinator` return exit 33 + prereq RFC names in error message)
- [ ] `RoleListOutput`, `RoleShowOutput`, `RoleSelectOutput` envelopes implemented with `#[derive(Serialize, Deserialize)]` + `schemars::JsonSchema`
- [ ] `--filter <field=value>` parser (kind/class/requires_octo_min) implemented + unit-tested
- [ ] `--with-slashing-rules` flag on `role show` implemented with `#[serde(skip_serializing_if = "Option::is_none")]`
- [ ] `--dry-run` preview envelope on `role select` (slashing rules table embedded) implemented + unit-tested
- [ ] `--confirm` + `--confirm-acknowledge` two-step gate on `role select` enforced via `#[arg(requires = "confirm")]`; CI tests assert gate
- [ ] HSM downgrade matrix asserted across `OCTO_ENV` ∈ {unset, production, staging, test, development}; exit 5 if `--dev` requested outside development
- [ ] Auditor mode denial on `role select` (exit 33, `RoleNotSelectable { reason: "auditor mode" }`, does NOT echo role_id) implemented + unit-tested
- [ ] Redaction: `signature_proof` rendered as `[REDACTED:sig]`; `stake_octo` / `stake_role_token` / `role_binding_hash` rendered verbatim
- [ ] `OctoCliError` variants `RoleNotFound` (31), `StakeInsufficient` (32), `RoleNotSelectable` (33), `SignerMismatch` (35) added per RFC-0011-d §Error Handling exit-code table (exit 34 reserved/freed per F-16)
- [ ] `RoleFilter`, `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleBinding`, `RoleError` substrate types added to `octo-role` (or extension to `octo-coordinator` if substrate team prefers co-location)
- [ ] `[ADD] octo_role::list`, `octo_role::show`, `octo_role::select` substrate entrypoints implemented
- [ ] `OctoRoleBinding` persistence added to `WalletStore` (RFC-0011 §Subcommand Taxonomy entry #1 — 0700 enforcement)
- [ ] 7 role slugs added to role registry (builder, provider, storage, bandwidth, orchestrator, recorder, wallet)
- [ ] RFC-0855-namespaced UUIDv5 discriminator verified for all 7 role slugs (integration test asserts distinct UUIDs)
- [ ] Dual-stake sufficiency check is substrate-authoritative (CLI does NOT pre-check; substrate returns `RoleError::StakeInsufficient { required, available }`)
- [ ] Cross-chain partition invariant enforced: slash-ledger row keyed by `(chain_id, operator_did)` per RFC-0900 §Slash Ledger Substrate
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli -p octo-role --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli -p octo-role --lib --tests green
- [ ] No new INVALID cites introduced (cite validator runs clean per `docs/07-developers/octo-cli-implementation-guide.md` Guard 2)

### Type Coverage

| RFC-0011-d type                   | Sub-step                     | Notes                                                                                                                                            |
| --------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `RoleSummary`                     | Sub-step 1 (output types)    | Layer B; substrate `[ADD]` struct in `octo-role`; field-aligned to RFC-0011-d §7.5 Role Summary table                                            |
| `RoleRecord`                      | Sub-step 1 (output types)    | Layer B; substrate `[ADD]` struct returned by `show`; includes `slashing_rules` + `allowed_actions` + `registry_ref`                             |
| `RoleFilter`                      | Sub-step 4 (filter parsing)  | Layer B; `[ADD]` struct parsed CLI-side, mirrors `capability list` filter pattern                                                                |
| `SlashingRule`                    | Sub-step 1 (output types)    | Layer B; per-rule slashing entry per RFC-0900 §Slashing Model (`reason_code`, `description`, `penalty_pct_micro`, `escalation_multiplier_micro`) |
| `RoleBinding`                     | Sub-step 1 (output types)    | Layer B; persisted to wallet store on `select`; includes `signature_proof` (redacted in CLI) + `role_binding_hash`                               |
| `RoleError`                       | Sub-step 5 (substrate error) | Layer B; typed error enum; CLI translates to `OctoCliError::Role*` variants per RFC-0011-d §Error Handling                                       |
| `RoleListOutput`                  | Sub-step 2 (CLI envelope)    | Layer C/D; `OutputEnvelope<RoleListOutput>` wrapper                                                                                              |
| `RoleShowOutput`                  | Sub-step 2 (CLI envelope)    | Layer C/D; `OutputEnvelope<RoleShowOutput>` wrapper; `slashing_rules` field uses `skip_serializing_if = "Option::is_none"`                       |
| `RoleSelectOutput`                | Sub-step 2 (CLI envelope)    | Layer C/D; `OutputEnvelope<RoleSelectOutput>` wrapper; `signature_proof: RedactedHex`, `role_binding_hash: Hex32`                                |
| `OctoCliError::RoleNotFound`      | Sub-step 6 (CLI error)       | Layer C; exit 31                                                                                                                                 |
| `OctoCliError::StakeInsufficient` | Sub-step 6 (CLI error)       | Layer C; exit 32                                                                                                                                 |
| `OctoCliError::RoleNotSelectable` | Sub-step 6 (CLI error)       | Layer C; exit 33 (incl. Auditor mode denial + Phase 2 prereq block)                                                                              |
| `OctoCliError::SignerMismatch`    | Sub-step 6 (CLI error)       | Layer C; exit 35 (F-16 dedicated slot)                                                                                                           |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Role Subcommands (companion guide amended per RFC-0011-d §Key Files to Modify). Rust snippets + clap wiring patterns mirror `capability list` / `capability mint` pattern (parent RFC-0011 §Subcommand Taxonomy).

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Risk

- **HSM downgrade via env var manipulation** — mitigated by `InMemorySigner` gated to `OCTO_ENV == "development"` (exact match); CLI exits 5 if `--dev` requested outside development; CI asserts across full matrix (RFC-0011-d §Security 1 + §Adversarial Review).
- **Role squatting via repeated `role select`** — mitigated by substrate last-writer-wins semantics per `(chain_id, operator_did)` (RFC-0011-d §Security 2; no `RoleBindingConflict` variant — substrate enforces atomically inside envelope-build); race window is closed by `BEGIN IMMEDIATE` tx serialization (M4).
- **Stake slashing risk undisclosed to operator** — mitigated by `--dry-run` showing slashing rules table + `--confirm-acknowledge` two-step gate; CI tests assert rules appear before submit (RFC-0011-d §Security 3).
- **Cross-chain role replay** — mitigated by `chain_id: [u8; 32]` in binding envelope (RFC-0010 ChainId); substrate rejects envelopes whose `chain_id` doesn't match active chain.
- **Typed discriminator collision** — mitigated by RFC-0855-namespaced UUIDv5; integration test asserts all 7 role slugs produce distinct UUIDs.
- **Pastejacking on `--role` argument** — mitigated by `--confirm-acknowledge` two-step gate (parent RFC-0011 §Security 1a) + `#[arg(requires = "confirm")]` on `--confirm-acknowledge`.

## Notes

- `RedactedHex` (from mission `0011-capability-commands`) is reused for `signature_proof`; `Hex32` (BLAKE3-256 digest, public material) is reused for `role_binding_hash`.
- `RoleKind` is NOT a substrate enum; the substrate uses a typed 128-bit UUID discriminator (`role_kind_uuid: [u8; 16]`) per the RFC-0855 namespace pattern (`urn:octo:role:0855:<slug>`). New roles land by adding a row to the role registry; no central enum edit (mirror RFC-0855 §Mission types extension pattern + parent RFC-0011 §Caveat Catalog caveat envelope parsing).
- Dual-stake sufficiency check is performed substrate-side (NOT CLI-side) per RFC-0011-d §7.4 substrate-truth notes; the CLI surfaces `RoleError::StakeInsufficient { required, available }` verbatim.
- The two new role slugs (`recorder`, `wallet`) are introduced in RFC-0011-d §7.5; they are OCTO-only roles (`None` for `role_token_ticker` + `stake_role_token`).
- `docs/04-tokenomics/token-design.md` §10 Dual-Stake table MUST be amended to add `recorder` and `wallet` role rows (or note their absence as OCTO-only).

## Scope

Land 3 role subcommands per RFC-0011-d §7.2 Subcommand Taxonomy Phase 1 (list/show/select for 7 base roles only). Coordinator + domain-coordinator subcommands are explicitly OUT OF SCOPE — they are Phase 2 (separate mission `0011-d-role-subcommands-phase2`).

## Sub-steps

1. **Substrate type scaffolding** — add `RoleFilter`, `RoleSummary`, `RoleRecord`, `SlashingRule`, `RoleBinding`, `RoleError` to `octo-role` (or `octo-coordinator` if co-located).
2. **CLI output envelopes** — add `RoleListOutput`, `RoleShowOutput`, `RoleSelectOutput` to `crates/octo-cli/src/commands/role.rs`; derive `Serialize`, `Deserialize`, `schemars::JsonSchema`.
3. **clap subcommand group** — add `Octo::Role { action: RoleAction }` to clap tree; route `list` / `show` / `select` per RFC-0011-d §7.2.
4. **Filter parser** — port `--filter <field=value>` parser from `capability list` pattern; validate field names (kind/class/requires_octo_min).
5. **Substrate `[ADD]` wiring** — implement `octo_role::list`, `octo_role::show`, `octo_role::select` (HSM-bound); substrate owns dual-stake sufficiency check.
6. **CLI error mapping** — add 4 `OctoCliError` variants per RFC-0011-d §Error Handling; map to exit codes 31, 32, 33, 35 (exit 34 reserved per F-16); mirror parent's `sanitize_substrate_error` pass.
7. **HSM + confirmation gate** — enforce `--confirm` + `--confirm-acknowledge` two-step gate via `#[arg(requires = "confirm")]`; HSM downgrade matrix asserted in CI per RFC-0011-d §Security 1.
8. **Persistence** — add `OctoRoleBinding` persistence to `WalletStore` (parent RFC-0011 §Subcommand Taxonomy entry #1 — 0700 enforcement applies).
9. **Test vectors** — 9+ test vectors per RFC-0011-d §Test Vectors (3 per subcommand + 1 partial-prereq guard); assert exit codes + envelope shape + redaction + cross-chain partition.
10. **Role registry rows** — add 7 role slugs to role registry (builder, provider, storage, bandwidth, orchestrator, recorder, wallet) with RFC-0855-namespaced UUIDv5 discriminators.

### Cargo deps

```toml
# crates/octo-cli/Cargo.toml (Layer C; per RFC-0011-d §Key Files to Modify)
# Role substrate (Layer B; new crate per RFC-0011-d §7.4)
octo-role = { path = "../octo-role" }
# Slash ledger substrate (Layer B; per RFC-0900 §Slash Ledger Substrate)
octo-slash-ledger = { path = "../octo-slash-ledger" }
# CapabilitySigner trait (Layer B; from octo-cap-macaroon per RFC-0011 §Subcommand Taxonomy entry #10)
# (already a dependency)
```

### Test Vectors (per RFC-0011-d §Test Vectors — Phase 1)

At least 9 test vectors required per RFC-0011-d §Test Vectors:

| Vector ID | Description                                                                                                                       | Group                |
| --------- | --------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| TV-RL-1   | `role list` (all) — 7 entries; exit 0; envelope schema v2                                                                         | Role list            |
| TV-RL-2   | `role list --filter kind=provider` — single match; exit 0                                                                         | Role list            |
| TV-RL-3   | `role list --filter requires_octo_min=10000` — empty match; exit 0                                                                | Role list            |
| TV-RS-1   | `role show provider` — success; envelope shape; `role_token_ticker: "OCTO-A"`; `quorum: 3`                                        | Role show            |
| TV-RS-2   | `role show provider --with-slashing-rules` — slashing table embedded                                                              | Role show            |
| TV-RS-3   | `role show nonexistent` — exit 31; `RoleNotFound` variant                                                                         | Role show            |
| TV-RX-1   | `role select builder --dry-run --json` — success dry-run; exit 0; `signature_proof` redacted; no side effects                     | Role select          |
| TV-RX-2   | `role select builder --confirm --json` — success; exit 0; deterministic `body_hash`; `signature_proof` redacted; slash-ledger row | Role select          |
| TV-RX-3   | `role select nonexistent-role --confirm --json` — exit 31; `RoleNotFound` variant                                                 | Role select          |
| TV-RX-4   | `role select builder --confirm --json` with signer.did ≠ operator_did — exit 35; `SignerMismatch` (F-16 dedicated slot)           | Role select          |
| TV-RP-1   | `role select coordinator` (Phase 2 blocked) — exit 33; error message names prereq RFC (RFC-0855p-e)                               | Partial-prereq guard |

### Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `RoleAction` dispatch + 3 output envelopes + 4 error variants + filter parser + HSM gate + redaction pass
- `octo-role` (Layer B; new crate per RFC-0011-d §7.4) — `RoleSummary` / `RoleRecord` / `RoleFilter` / `SlashingRule` / `RoleBinding` / `RoleError` substrate types + `[ADD]` `list` / `show` / `select` entrypoints
- `octo-slash-ledger` (Layer B; per RFC-0900 §Slash Ledger Substrate) — slash-ledger row creation on `select`; `(chain_id, operator_did)` primary key partition
- `octo-wallet` (Layer B) — `OctoRoleBinding` persistence extension to `WalletStore` (parent RFC-0011 §Subcommand Taxonomy entry #1 — 0700 enforcement)
- `octo-cap-macaroon` (Layer B) — `CapabilitySigner` trait (already exposed via `active_signer()`)

The CLI depends on Layer-B substrate crates; substrate crates do NOT depend on the CLI. No reverse deps.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli -p octo-role --all-targets -- -D warnings  # clean
cargo test -p octo-cli -p octo-role --lib --tests  # green
```

## Backward compat

- Additive only: new clap subcommand group + new `[ADD]` substrate entrypoints + new `OctoCliError` variants; no existing API modified per RFC migration etiquette.
- CLI exit codes: 31-35 reserved for role subcommands; 17-30 reserved by parent amendment chain; 36-63 reserved for future amendment additions (RFC-0011-d §Exit Codes table).
- Output schema: `RoleShowOutput.slashing_rules` field uses `#[serde(skip_serializing_if = "Option::is_none")]` so the field is absent when `--with-slashing-rules` is not passed; no `schema_version` bump for additive field.
- `OutputEnvelope.schema_version` stays at 2 (current); bump to 3 only on breaking change per RFC-0011-d §Compatibility
- Deprecated stub `octo role {builder,provider,...}` (RFC-0011 §Stub command compatibility) preserved as-is; deprecation removal timeline (v1.0 → v1.1 → v2.0) unchanged.

## Cross-references

- RFC-0011-d §7.2 Subcommand Taxonomy (role list/show/select tables)
- RFC-0011-d §7.3 Output Envelopes (`RoleListOutput`, `RoleShowOutput`, `RoleSelectOutput`)
- RFC-0011-d §7.4 Substrate `[ADD]` signatures (`octo_role::list`, `show`, `select`)
- RFC-0011-d §7.5 Role Summary (canonical role → role-token mapping)
- RFC-0011-d §7.6 Role Select — HSM Signing Flow (sequence diagram + confirmation gate + downgrade rules)
- RFC-0011-d §7.7 Redaction (`signature_proof` redacted; `stake_octo`/`stake_role_token`/`role_binding_hash` verbatim)
- RFC-0011-d §Implementation Phases (Phase 1 = unblocked; Phase 2 = gated)
- RFC-0011-d §Compatibility (partial-prereq caveat: RFC-0855p-d + RFC-0855p-e Draft)
- RFC-0011-d §Test Vectors (9+ vectors distributed across 4 groups)
- RFC-0011-d §Error Handling exit-code table (31-34 reserved)
- RFC-0011-d §Security Considerations (1-6) + §Adversarial Review
- RFC-0011 — `octo` CLI substrate (parent amendment chain host)
- RFC-0900 — AI Quota Marketplace (slash ledger substrate; first-offense + escalation slashing model)
- RFC-0855 — Mission Overlay Networks (role namespace; dual-stake model; participant flag bits)
- RFC-0855p-b — Mission Coordinator Lifecycle (slash tally; lifecycle states inherited by DomainCoordinator specialization)
- RFC-0855p-c — DomainCoordinator Role (physical-platform binding authority; platform-mediated handover pattern)
- RFC-0009 — Identity Management (DID derivation for the role-binding signature)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping; role commands are class C)
- RFC-0010 — Canonical DID Codec (ChainId typing for slash ledger partition)
- RFC-0957 — Macaroon Substrate (signer trait cross-reference)
- [[cipherocto-design-principles]] — Layer B stability contract; no central enum; typed discriminator

## Why 1 release cycle gate

N/A — Phase 1 is unblocked per RFC-0011-d §Implementation Phases Phase 1. Coordinator + domain-coordinator subcommands are Phase 2 (separate mission `0011-d-role-subcommands-phase2` with `release_gate` block).

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
