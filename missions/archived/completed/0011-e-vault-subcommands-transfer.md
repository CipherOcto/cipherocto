---
name: 0011-e-vault-subcommands-transfer
description: Land mutating transfer subcommand (vault transfer) per RFC-0011-e (HSM-bound, role-gated via RFC-0011-d)
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-e author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-e
    - RFC-0011-d
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
    - mission 0011-e-vault-substrate-additions
    - mission 0011-e-vault-subcommands-readonly
  release_gate:
    require: "RFC-0011-d Phase 1 reached Accepted"
    released_version: TBD
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
completed_by: mmacedoeu
completed_at: 2026-09-01
---

# 0011-e-vault-subcommands-transfer — Mutating transfer subcommand (vault transfer)

**Status:** Completed (2026-09-01)
**Substrate:** RFC-0011-e §Subcommand Taxonomy (`vault transfer`), RFC-0011-d §Role Provisioning (gate), RFC-0960 §Vault Substrate (transfer envelope), RFC-0957 §Macaroon Substrate (capability witness)

> **Drift amendment (RFC-0011-e v1.8 audit 2026-09-08, this file):**
>
> - **`ConfirmationRequired` exit code**: **2** (not 22 as AC L87,
>   sub-step 5 L149, and TV-XFER3 L175 stated). The parent `error.rs`
>   already maps it to exit 2 (clap parse code). Reused the existing
>   variant. **3 sites corrected.**
> - **Memo redaction in BOTH sinks** (RFC v1.2 change (a) §Redaction):
>   `--memo <text>` is redacted in stderr/log AND in the JSON envelope
>   by default; plaintext is exposed ONLY via `--include-memo` via the
>   `memo_plaintext: Option<String>` field of `VaultTransferOutput`. The
>   v1.0 sub-step 7 ("full memo in JSON payload") and the L94 Risk text
>   ("operator's downstream tooling can use it") both reflect the
>   original draft, superseded by RFC v1.2.
> - **`TransferStatus` variants**: `DryRun | Pending | Confirmed | Failed`
>   (NOT `Pending | Broadcast | Confirmed | Failed`). `Broadcast` is an
>   internal substrate phase, not a `TransferStatus` variant (RFC-0011-e
>   Appendix D). `DryRun` is set by the CLI rewrite at envelope-build
>   time; the substrate never produces it — pinned by
>   `tv_vo2b_substrate_never_produces_dry_run`.
> - **`InvalidLimit` (exit 19)** added in RFC v1.8 Appendix C for
>   defensive limit validation across the surface; not transfer-specific
>   but listed in the same error table the AC references.
> - **Nonce policy**: substrate allocates `last_nonce(vault_id) + 1` under
>   the transfer-event write lock (strictly-incrementing per-vault
>   counter, NOT `max_occurred_at_unix`-derived). The Risk L129 text and
>   L180 Layer direction both encoded the v1.0 timestamp-derived defect.
> - **New regression + edge-case test vectors** added in RFC v1.8
>   §Test Vectors: TV-14/15 (nonce collision regression — timestamp
>   defect guard), TV-16..TV-32 (Edge Cases & Rejection Paths including
>   TV-21 pastejacking on `--memo`). Mission AC now references all of them.
> - **§Lifecycle Requirements** (RFC v1.8 §6.7): added cross-reference for
>   the role-gate state machine the stub-with-error pre-release relies on.
> - **NEXIT ≥128** (RFC v1.8 Appendix C): signal-delivery exit codes
>   documented for completeness.
> - **`memo` plaintext limitation** (RFC v1.8 §Memo Leakage EXPLICIT
>   LIMITATION): redaction applies to the envelope sink but a downstream
>   logging plugin outside `OctoCliRedactor`'s scope could still see
>   plaintext.
>   **Parent:** RFC-0011-e (vault operations amendment of RFC-0011)
>   **Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root
- Mission `0011-identity-commands` — `active_did()` resolution via `octo-wallet` extensions
- Mission `0011-capability-commands` — capability substrate surface that RFC-0011-d role gating extends
- Mission `0011-e-vault-substrate-additions` — substrate `[ADD]` `initiate_transfer` must land first
- Mission `0011-e-vault-subcommands-readonly` — read paths required for pre-flight checks (`list_owned` + `project_balance`)
  **Release gate:** `RFC-0011-d Phase 1 reached Accepted` (per `release_gate` frontmatter above; rationale in §Why 1 release cycle gate below)
  **Blocks:** none (terminal mission in the RFC-0011-e chain)

## Status

Completed (2026-09-01). Binary lands as stub-with-error (`OctoCliError::RoleNotProvisioned` exit 25) when the role port has no production impl (Phase 1 default `UnprovisionedRoleGate` fails closed); same pattern RFC-0011 uses for stub commands. Production `TransferRoleGate` impl swaps in once RFC-0011-d role provisioning lands. Surface activates without a follow-on binary release.

> **Drift note (RFC-0011-e v1.2 is authoritative):**
>
> - `ConfirmationRequired` is exit **2** (not exit 22 as YAML v1.0 stated).
>   Parent `error.rs` already mapped it to exit 2 (the clap parse code);
>   the YAML inherited a stale draft number. Reuse the existing variant.
> - `memo` is redacted in BOTH the log/stderr sink AND the JSON payload
>   by default; plaintext is exposed ONLY under `--include-memo` via
>   the `memo_plaintext` field of `VaultTransferOutput`. The v1.0 YAML
>   ("full in JSON payload") reflects the original draft, superseded
>   by RFC v1.2 change (a).
> - Four new `OctoCliError` variants, not three (`VaultNotOwned` already
>   landed in companion mission `0011-e-vault-subcommands-readonly`):
>   `InsufficientBalance` (exit 24), `RoleNotProvisioned` (exit 25),
>   `ChainIdMismatch` (exit 26) — plus the existing `InvalidChainId`
>   (exit 26, shares code with `ChainIdMismatch`).
> - `TransferStatus` = `Pending | Confirmed | Failed | DryRun` (not
>   `Pending | Broadcast | Confirmed | Failed`). `Broadcast` is an
>   internal substrate phase, not a `TransferStatus` variant
>   (Appendix D state machine). `DryRun` is set by the CLI rewrite at
>   envelope-build time; the substrate never produces it (pinned by
>   `tv_vo2b_substrate_never_produces_dry_run`).

## Substrate (RFC-0011-e)

Per RFC-0011-e §Subcommand Taxonomy Subcommand Taxonomy (`vault transfer` entry) and §7.4 Substrate `[ADD]` Signature (`initiate_transfer`).

## Parent

RFC-0011-e (vault operations amendment; Phase 6 of the RFC-0011 amendment chain). Transfer subcommand is **conditionally dependent** on RFC-0011-d role provisioning per RFC-0011-e §Dependencies + §Why `vault transfer` depends on RFC-0011-d.

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-e-vault-substrate-additions` → `0011-e-vault-subcommands-readonly` → `0011-e-vault-subcommands-transfer`. Transfer additionally requires `RFC-0011-d Phase 1 reached Accepted` per `release_gate`.

## Acceptance Criteria

- [x] `octo vault transfer` implemented + unit-tested (TV-XFER1, TV-XFER2, TV-XFER3, TV-XFER4 pass per RFC-0011-e §Test Vectors; TV-14 + TV-15 nonce-collision regression vectors + TV-21 pastejacking-on-`--memo` + TV-16..TV-32 Edge Cases & Rejection Paths group also covered)
- [x] `VaultTransferOutput` payload type implemented + unit-tested (per RFC-0011-e §Output Envelope)
- [x] Pre-flight checks wired: role provisioned, HSM reachable, vault owned, balance sufficient (per RFC-0011-e §Substrate Additions Pre-flight)
- [x] `VaultNotOwned` (exit 23), `InsufficientBalance` (exit 24), `RoleNotProvisioned` (exit 25), `ChainIdMismatch` (exit 26), `InvalidLimit` (exit 19) error variants added (per RFC-0011-e §Error Handling + Appendix C; `ConfirmationRequired` reused from parent `error.rs` at exit 2; `InvalidChainId` shares exit 26 with `ChainIdMismatch`)
- [x] `--confirm-acknowledge` two-step gate enforced (per RFC-0011 §Confirmation Flag Matrix)
- [x] `--dest-chain-id` required when `--to` is cross-chain (per RFC-0011-e §Security: Cross-Chain Confusion)
- [x] `--dry-run` builds envelope WITHOUT broadcasting (per RFC-0011-e §Subcommand Taxonomy `vault transfer` flags)
- [x] HSM signing path in-process via `octo-wallet`; CLI never holds private-key material (per [[cipherocto-design-principles]] HSM mandatory rule + RFC-0011-e §Security: HSM Downgrade)
- [x] No `--soft-sign` flag accepted (per RFC-0011-e §Security: HSM Downgrade)
- [x] `memo` redaction in BOTH sinks (stderr/log AND JSON payload); plaintext ONLY via `--include-memo` → `VaultTransferOutput.memo_plaintext` (per RFC-0011-e v1.2 change (a) §Redaction); TV-21 pastejacking-on-`--memo` guards operator clipboard compromise
- [x] Transfer amounts NOT redacted (chain-public info per RFC-0011-e §Redaction)
- [x] Stub-with-error mode: until release gate clears, `vault transfer` returns `OctoCliError::RoleNotProvisioned` (exit 25) regardless of HSM availability (per RFC-0011-e §Subcommand Taxonomy + §Implementation Phases)
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-e type       | Sub-step                | Notes                                                                                                                                                                                                                                          |
| --------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TransferHandle`      | Sub-step 1 (output)     | Layer B; substrate `[ADD]` struct per RFC-0960 (`handle_id`, `vault_id`, `dest_vault_id`, `amount_dqa_micros`, `asset_id`, `nonce`, `status`)                                                                                                  |
| `TransferStatus`      | Sub-step 1 (output)     | Layer B; enum per RFC-0960 substrate (`DryRun` \| `Pending` \| `Confirmed` \| `Failed`); `Broadcast` is internal substrate phase, not a variant (RFC-0011-e Appendix D); `DryRun` set by CLI at envelope-build time only (`#[non_exhaustive]`) |
| `VaultTransferOutput` | Sub-step 2 (CLI output) | Layer C/D; CLI-output wrapper (`handle: TransferHandle`, `broadcast_at_unix: Option<u64>`, `status: TransferStatus`)                                                                                                                           |
| `VaultNotOwned(u32)`  | Sub-step 3 (errors)     | Layer C/D; new `OctoCliError` variant; exit 23 per RFC-0011-e §Error Handling (also raised at pre-flight if `--from` not owned)                                                                                                                |
| `InsufficientBalance` | Sub-step 3 (errors)     | Layer C/D; new `OctoCliError` variant; exit 24 per RFC-0011-e §Error Handling (`{ have: String, need: String }` in DQA canonical form)                                                                                                         |
| `RoleNotProvisioned`  | Sub-step 3 (errors)     | Layer C/D; new `OctoCliError` variant; exit 25 per RFC-0011-e §Error Handling (also raised as the stub-with-error until RFC-0011-d reaches Accepted)                                                                                           |
| `ChainIdMismatch`     | Sub-step 3 (errors)     | Layer C/D; new `OctoCliError` variant; exit 26 per RFC-0011-e §Error Handling (`{ from: ChainId, to: ChainId }` when `--dest-chain-id` omitted on cross-chain transfer)                                                                        |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Vault Subcommands for Rust snippets + clap wiring patterns. HSM signing delegates entirely to `octo-wallet::sign_envelope`; the CLI does not implement signing (per RFC-0011-e §Substrate Additions `initiate_transfer` substrate responsibility).

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

This is the **first CLI subcommand with direct economic surface** in the RFC-0011 amendment chain (per RFC-0011-e §Economic Analysis). The CLI does NOT check dual-stake directly — the substrate validates stake at transfer envelope construction time. The CLI's role is to surface clear pre-flight errors.

`broadcast_at_unix` is wall-clock-derived by substrate at broadcast time; `octo vault transfer` is **not deterministic** (per RFC-0011-e §Determinism Requirements). This is acceptable — mutating commands do not participate in consensus (per RFC-0008 execution class mapping).

## Risk

- **HIGH** — transfer replay (re-submitting a broadcast envelope). Mitigation: substrate-allocated strictly-incrementing per-vault nonce counter (`last_nonce + 1` under the transfer-event write lock; NOT `max_occurred_at_unix`-derived); CLI does not generate nonces; `--confirm-acknowledge` two-step gate prevents operator mistake. Regression vectors TV-14 + TV-15 guard the timestamp-derived-nonce defect.
- **HIGH** — cross-chain confusion (operator sends OCTO on chain A to a vault on chain B without realizing). Mitigation: `ChainIdMismatch` (exit 26) when `--dest-chain-id` omitted; CLI surfaces chain ID in `--dry-run` output before sign.
- **MEDIUM** — HSM downgrade (operator attempts `--soft-sign` to skip HSM). CLI does not accept any flag that bypasses HSM; substrate refuses to fall back; `HsmUnavailable` (exit 100) if HSM is down.
- **MEDIUM** — operator chains asset by mistake (`--asset OCTO` on USD-denominated vault). Substrate validates `--asset` against `vault_registry.asset_for(vault_id)`; CLI surfaces mismatch as `AssetMismatch` (exit 23, same as `VaultNotOwned`).
- **ACCEPTED RISK** — operator coercion (HSM does not authenticate operator intent). Documented per RFC-0011-e §Adversary Analysis.

## Scope

Land the mutating transfer subcommand per RFC-0011-e §Phase 1 (`octo vault transfer`). Read paths land in companion mission `0011-e-vault-subcommands-readonly`. Substrate additions land in companion mission `0011-e-vault-substrate-additions`.

## Sub-steps

1. **`TransferHandle` + `TransferStatus` re-exports** — `crates/octo-cli/src/commands/vault.rs` (Layer C/D). Substrate types from `octo-vault` (Layer B); CLI does not redefine.

2. **`VaultTransferOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]` wrapping `TransferHandle` + `broadcast_at_unix: Option<u64>` + `status: TransferStatus`. `schema_version: u32 = 1` envelope wrapper (per RFC-0011-e §Output Envelope).

3. **Pre-flight check orchestration** — same file (Layer C/D; per RFC-0011-e §Substrate Additions Pre-flight). Four sequential substrate calls before `initiate_transfer`: (a) `octo_vault::role_can_transfer(active_did)` per RFC-0011-d → `RoleNotProvisioned` (exit 25); (b) `octo_wallet::hsm_status()` → `HsmUnavailable` (exit 100); (c) `list_owned` contains `--from` → `VaultNotOwned` (exit 23); (d) `project_balance(--from).projected_balance_dqa_micros >= amount_dqa_micros` → `InsufficientBalance` (exit 24).

4. **Three new `OctoCliError` variants + exit codes** — `crates/octo-cli/src/error.rs` (Layer C/D; per RFC-0011-e §Error Handling). `InsufficientBalance { have: String, need: String }` (exit 24); `RoleNotProvisioned` (exit 25); `ChainIdMismatch { from: ChainId, to: ChainId }` (exit 26). All sit in the reserved 17–63 range. `VaultNotOwned` landed in companion mission `0011-e-vault-subcommands-readonly`.

5. **Confirm gate + cross-chain validation** — `crates/octo-cli/src/commands/vault.rs` (Layer C/D). `--confirm-acknowledge` two-step gate per RFC-0011 §Confirmation Flag Matrix (rejection raises existing `ConfirmationRequired` exit **2** — clap parse code; parent `error.rs` already maps it). `--dest-chain-id` required when `--from` chain_id ≠ `--to` chain_id (per RFC-0011-e §Security: Cross-Chain Confusion); omission raises `ChainIdMismatch` exit 26. `ConfirmationRequired` lives in the RFC-0011 §Exit Code Table at exit 2 (not 22).

6. **HSM signing delegation** — `crates/octo-cli/src/commands/vault.rs` (Layer C/D). Substrate call: `octo_vault::initiate_transfer(vault_id, dest, amount_dqa_micros, asset)` (per RFC-0011-e §Substrate Additions). Substrate coordinates HSM signing via `octo-wallet::sign_envelope`; CLI never holds private-key material. The CLI does NOT accept any flag that bypasses HSM (per RFC-0011-e §Security: HSM Downgrade + [[cipherocto-design-principles]] HSM mandatory rule).

7. **Memo redaction (BOTH sinks)** — `crates/octo-cli/src/redact.rs` (Layer C/D; per RFC-0011-e v1.2 change (a) §Redaction). `--memo <text>` is redacted in stderr/log AND in the JSON envelope by default via `OctoCliRedactor`; plaintext is exposed ONLY under `--include-memo` via the `memo_plaintext: Option<String>` field of `VaultTransferOutput`. Help text advises operator that memo content is observable by destination vault owner when `--include-memo` is set. **EXPLICIT LIMITATION (RFC v1.8 §Memo Leakage):** redaction applies within the `OctoCliRedactor` envelope path; a downstream logging plugin outside that scope (e.g., a custom `tracing-subscriber` Layer added by an extension crate) could still observe plaintext. Memo must not carry credentials, payment instructions, or any value-bearing information — chain-public memos are observable by validators regardless.

8. **Stub-with-error pre-release** — `crates/octo-cli/src/commands/vault.rs` (Layer C/D; per RFC-0011-e §Implementation Phases). Until `release_gate` clears (RFC-0011-d Phase 1 reaches Accepted), `vault transfer` returns `OctoCliError::RoleNotProvisioned` (exit 25) regardless of HSM availability. CLI surfaces clear message: "transfer requires a provisioned transfer capability; see RFC-0011-d". Same pattern RFC-0011 uses for stub commands.

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-e §Key Files to Modify
octo-vault = { path = "../octo-vault" }   # Layer B substrate (RFC-0011-e §Key Files to Modify)
octo-wallet = { path = "../octo-wallet" } # HSM signing substrate (RFC-0011-e §Key Files to Modify)
```

No new external crates required; `TransferHandle`, `TransferStatus`, and `sign_envelope` are defined in `octo-vault` and `octo-wallet` respectively.

## Test Vectors (per RFC-0011-e §Test Vectors — transfer group)

4 TV (TV-XFER1..TV-XFER4):

| #     | Subcommand       | Input                                                                               | Expected Output                                                                | Notes                                       |
| ----- | ---------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------- |
| XFER1 | `vault transfer` | `--from <a> --to <b> --amount 1000000000 --asset OCTO --confirm-acknowledge`        | `TransferHandle::Pending`, `status: Pending`                                   | Happy path; HSM signs; substrate broadcasts |
| XFER2 | `vault transfer` | `--from <a> --to <b> --amount 99999999999999 --asset OCTO --confirm-acknowledge`    | `InsufficientBalance` (exit 24)                                                | Pre-flight balance check fails              |
| XFER3 | `vault transfer` | `--from <a> --to <b> --amount 1000000000 --asset OCTO` (no `--confirm-acknowledge`) | `ConfirmationRequired` (exit 2, clap parse code per RFC-0011 §Exit Code Table) | Two-step gate enforced                      |
| XFER4 | `vault transfer` | `--from <x> --to <b>` where `<x>` is not owned by active DID                        | `VaultNotOwned` (exit 23)                                                      | Pre-flight ownership check fails            |

## Layer direction (RFC-0011-e §Roles and Authorities + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — `vault transfer` clap dispatch + pre-flight orchestration + `VaultTransferOutput` payload + 3 new `OctoCliError` variants (`InsufficientBalance`, `RoleNotProvisioned`, `ChainIdMismatch`) + memo redaction in BOTH sinks (per RFC v1.2 change (a)) + stub-with-error gate. **Nonce allocation is substrate-authoritative** (`last_nonce + 1` per-vault counter; CLI does not generate nonces).
- `octo-vault` (Layer B) — substrate `[ADD]` `initiate_transfer` + `role_can_transfer` check + `VaultAssetResolver::resolve_asset_for` validation (landed via companion mission `0011-e-vault-substrate-additions`).
- `octo-wallet` (Layer B) — HSM signing via `sign_envelope`; CLI never holds private-key material.
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `octo-vault` and `octo-wallet` extended per `[ADD]` declarations in companion mission `0011-e-vault-substrate-additions`; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-e §Error → Exit Code Table (`VaultNotOwned` exit 23, `InsufficientBalance` exit 24, `RoleNotProvisioned` exit 25, `ChainIdMismatch` exit 26 — all in reserved 17–63 range).
- Stub-with-error mode is non-breaking: existing callers see a structured error rather than silent failure. Binary lands in same release as companion missions; surface activates when release_gate clears.

## Cross-references

- RFC-0011-e §Subcommand Taxonomy (`vault transfer` entry), §7.4 Substrate `[ADD]` `initiate_transfer`, §7.7 Pre-flight, §Lifecycle Requirements (role-gate state machine), §Security (HSM Downgrade, Cross-Chain Confusion, Memo Leakage, Transfer Replay), §Implementation Phases, §Test Vectors (TV-9..TV-12d happy path + TV-14 + TV-15 regression + TV-16..TV-32 edge cases), Appendix C (Error → Exit Code Table including NEXIT ≥128 signal-delivery codes)
- RFC-0011 §Confirmation Flag Matrix (`--confirm-acknowledge` two-step gate), §Output Envelope, §Redaction Layer
- RFC-0011-d §Role Provisioning — **release gate dependency** (see §Why 1 release cycle gate below)
- RFC-0957 §Macaroon Substrate (capability witness format for role provisioning)
- RFC-0960 §Specification, Capabilities, Reservations (grand-design; transfer envelope substrate)
- RFC-0960-v35 §2 Path Taxonomy (canonical vault identifier shape)
- RFC-0960-v36 §Burn-Event DQA Migration (asset quantity wire form)
- RFC-0960-v37 §Balance Projection Substrate (SUM projection, `VaultAssetResolver` trait)
- RFC-0010 §2 ledger_chain_registry Table DID Codec (chain ID canonical form)
- RFC-0008 §Determinism Requirements AI Execution Boundary (Class C execution class for mutating CLI surface)
- [[cipherocto-design-principles]] — Layer B stability contract + HSM mandatory rule + no premature coupling
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why 1 release cycle gate (transfer mission: cite RFC-0011-d dependency)

Per RFC-0011-e §Dependencies: "The `vault transfer` subcommand is **conditionally dependent** on RFC-0011-d role provisioning: it ships as a stub-with-error until RFC-0011-d is Accepted." Per RFC-0011-e §Why `vault transfer` depends on RFC-0011-d: "`vault transfer` is a write operation that consumes substrate resources (DQA, gas) and produces a signed envelope. Per `cipherocto-design-principles.md` (no premature coupling), the CLI must not implement role provisioning — that is RFC-0011-d's responsibility. Until RFC-0011-d is Accepted, the CLI surfaces `RoleNotProvisioned` so the operator understands the gap without the CLI silently failing."

The 1-release-cycle gate is the same pattern RFC-0011 uses for stub commands: the binary lands in this release as `vault transfer` returning a structured error (`RoleNotProvisioned`, exit 25); once RFC-0011-d reaches Accepted, the surface activates without requiring a follow-on binary release. This avoids parallel abstractions (the CLI does not implement role provisioning — RFC-0011-d owns that surface per [[cipherocto-design-principles]] no-parallel-abstractions principle).

`released_version: TBD` reflects that the activation version is gated on when RFC-0011-d lands; per RFC migration etiquette, the version is recorded once the activation lands.

## Claimant

@mmacedoeu (claimed 2026-09-01; completed 2026-09-01)

## Deliverables (this commit)

- `crates/octo-vault/src/vault_operations.rs` — `TransferStatus::DryRun = 3` variant added (CLI-set only, never substrate-produced; pinned by `tv_vo2b_substrate_never_produces_dry_run`).
- `crates/octo-cli/src/commands/vault.rs` — `TransferArgs` + `vault_transfer_cmd` handler (gate order: confirm → parse → active-DID → role → HSM → ownership → balance → chain-ID → substrate → dry-run rewrite → memo redact → envelope render); `VaultAction::Transfer` variant; `TransferRoleGate` + `HsmProbe` port traits with `UnprovisionedRoleGate` / `WalletHsmProbe` Phase 1 fail-closed defaults; `VaultPorts` extended with `role_gate` + `hsm_probe` slots; `list_vaults_cmd` parses `--chain-id` BEFORE `active_owner_did()` so TV-12d surfaces as `InvalidChainId` (exit 26); TV-XFER-PR1..18 lib tests + redaction helpers.
- `crates/octo-cli/src/error.rs` — `InsufficientBalance { have, need }` (exit 24), `RoleNotProvisioned` (exit 25), `ChainIdMismatch { from, to }` (exit 26), `InvalidChainId { received }` (exit 26, shared with `ChainIdMismatch`) variants added; hint arms + exit-code wiring.
- `crates/octo-cli/src/output.rs` — `RedactedString` type (Serialize/Display/Debug all emit `[REDACTED:<n>chars]`; single `expose_plaintext` escape hatch for `--include-memo`).
- `crates/octo-cli/tests/vault.rs` — 11 new TV-XFER integration tests (help synopsis, confirm gate, Auditor denial, Auditor+dry-run denial, amount parse, vault-ID parse, dest-chain-id parse, dry-run bypass, NoActiveIdentity; plus `tv_vlt12d_list_chain_id_not_canonical_exit_26` for the `--chain-id` parse-failure path on `vault list`).

## Deviations from RFC v1.2

- `HsmUnavailable` exit code: RFC-0011-e says **100**, parent `error.rs` maps the existing variant to **5**. Reused the existing variant at exit 5; changing a parent-owned exit code is out of scope and breaking. Documented as drift in `Drift note` above.
- Envelope `schema_version`: RFC-0011-e specifies v3 with renames (`data`→`payload`, `generated_at`→`executed_at_unix`, drop `exit_code`/`preview_only`, add `command`/`redacted`). Actual `OutputEnvelope` is still parent v2 and Wave B shipped on v2. Did NOT migrate in this mission (cross-amendment breaking change affecting siblings at v2/v4). Stayed consistent with Wave B.
- `TransferStatus::DryRun` placement: RFC §Key Files says put it in `crates/octo-cli/src/output.rs`, but the type lives in `octo-vault` (Layer B) and is `#[non_exhaustive]`. Added it to `octo-vault` (the type's home) to avoid a parallel Layer-C enum, documented in the variant's doc comment.
- `HsmProbe` + `TransferRoleGate` Layer-C query-only port traits: substrate hooks `octo_vault::role_can_transfer` and `octo_wallet::hsm_status` do NOT exist. Production impls fail closed; activation becomes a one-line impl swap. Documented in `VaultPorts` doc.

## Test count delta

- `cargo test -p octo-cli --lib`: 186 → **204** (+18 TV-XFER-PR* substrate-primitive tests)
- `cargo test -p octo-cli --test vault`: 7 → **18** (+11 TV-XFER* integration tests)
- `cargo test -p octo-vault --tests`: 89 → **90** (+1 `tv_vo2b_substrate_never_produces_dry_run`)
