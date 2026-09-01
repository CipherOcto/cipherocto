---
name: 0011-e-vault-substrate-additions
description: Land [ADD] substrate API declarations for RFC-0011-e (list_owned, project_vault_balance, initiate_transfer) in octo-vault
metadata:
  node_type: substrate-additions
  type: substrate-additions
  originSessionId: RFC-0011-e author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-e
    - RFC-0960
    - RFC-0960-v35
    - RFC-0960-v36
    - RFC-0960-v37
    - RFC-0010
    - mission 0011-core-output-envelope-redaction
status: Open
---

# 0011-e-vault-substrate-additions — Layer B `[ADD]` substrate API for RFC-0011-e (list_owned, project_vault_balance, initiate_transfer)

**Status:** Open
**Substrate:** RFC-0011-e §Substrate Additions, RFC-0960-v37 §Balance Projection Substrate, RFC-0960-v35 §2 Path Taxonomy, RFC-0960-v36 §Burn-Event DQA Migration
**Parent:** RFC-0011-e (vault operations amendment of RFC-0011)
**Depends on:**

- RFC-0960 §Specification, Capabilities, Reservations (vault substrate root)
- RFC-0960-v35 §2 Path Taxonomy (canonical vault identifier shape)
- RFC-0960-v36 §Burn-Event DQA Migration (asset quantity wire form)
- RFC-0960-v37 §Balance Projection Substrate (SUM projection, `ZERO_VAULT_ID`, `max_occurred_at_unix`, `VaultAssetResolver`)
- RFC-0010 §2 ledger_chain_registry Table DID Codec (chain identifier namespace)
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` substrate consumed by CLI missions
  **Blocks:** `0011-e-vault-subcommands-readonly`, `0011-e-vault-subcommands-transfer` (CLI subcommand missions consume this substrate)

## Status

Open. This is the **substrate [ADD] mission** for the RFC-0011-e amendment chain — NOT a CLI mission. CLI missions (`0011-e-vault-subcommands-readonly`, `0011-e-vault-subcommands-transfer`) depend on the substrate declarations landed here.

## Substrate (RFC-0011-e)

Per RFC-0011-e §Substrate Additions — three function declarations in `crates/octo-vault/src/lib.rs`:

```rust
// [ADD] RFC-0011-e §Substrate Additions
pub fn list_owned(owner_did: &Did) -> Result<Vec<VaultSummary>, VaultError>;

// [ADD] RFC-0011-e §Substrate Additions
// 7-param substrate signature per RFC-0960-v37 §2.2; `ProjectionError` is the substrate
// error type (Layer B), distinct from the CLI-side `VaultError` (`VaultSummary` and
// `TransferHandle` paths use `VaultError` because they are CLI-facing wrappers).
pub fn project_vault_balance(
    chain_id: &ChainId,
    vault_id: &VaultId,
    registry: &dyn AssetRegistry,
    asset_resolver: &dyn VaultAssetResolver,
    log: &impl TransferEventLog,
    current_registry_epoch: u64,
    current_unix_seconds: i64,
) -> Result<VaultBalanceProjection, ProjectionError>;

// [ADD] RFC-0011-e §Substrate Additions
pub fn initiate_transfer(
    vault_id: &VaultId,
    dest: &VaultId,
    amount_dqa_micros: i64,
    asset: &AssetId,
) -> Result<TransferHandle, VaultError>;
```

## Parent

RFC-0011-e (vault operations amendment; Phase 6 of the RFC-0011 amendment chain). This mission lands the Layer B additions; companion CLI missions land the Layer C/D operator UX on top.

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-e-vault-substrate-additions` (this mission) → `0011-e-vault-subcommands-readonly` + `0011-e-vault-subcommands-transfer`.

## Acceptance Criteria

- [ ] `list_owned(owner_did: &Did) -> Result<Vec<VaultSummary>, VaultError>` declared + unit-tested (per RFC-0011-e §Substrate Additions)
- [ ] `project_vault_balance(chain_id: &ChainId, vault_id: &VaultId, registry: &dyn AssetRegistry, asset_resolver: &dyn VaultAssetResolver, log: &impl TransferEventLog, current_registry_epoch: u64, current_unix_seconds: i64) -> Result<VaultBalanceProjection, ProjectionError>` declared + unit-tested (per RFC-0011-e §Substrate Additions)
- [ ] `initiate_transfer(vault_id: &VaultId, dest: &VaultId, amount_dqa_micros: i64, asset: &AssetId) -> Result<TransferHandle, VaultError>` declared + unit-tested (per RFC-0011-e §Substrate Additions)
- [ ] `VaultSummary` struct aligned to RFC-0960-v37 §2.1 (`vault_id`, `chain_id`, `owner_did`, `asset_symbol`, `balance_projected`, `last_updated_unix: Option<i64>`)
- [ ] `VaultBalanceProjection` struct aligned to RFC-0960-v37 §2.1 (`chain_id`, `vault_id`, `asset_id`, `projected_balance: Dqa`, `projected_at_unix_seconds: Option<i64>`, `projection_source`)
- [ ] `ProjectionSource` enum per RFC-0960-v37 §2.1 (`Cache` \| `FreshLogScan` \| `EpochRebuild`)
- [ ] `TransferHandle` struct + `TransferStatus` enum per RFC-0960 substrate (transfer envelope substrate)
- [ ] `VaultBalanceCache` (LRU + TTL) wired and exercised (per RFC-0960-v37 §2.3)
- [ ] `VaultAssetResolver::resolve_asset_for(vault_id) -> AssetId` wired (per RFC-0960-v37 §2.1)
- [ ] `ZERO_VAULT_ID` sentinel exclusion applied to SUM projection (per RFC-0960-v37 §2.2)
- [ ] `max_occurred_at_unix(chain_id, vault_id)` monotonic per `(chain_id, vault_id)` (per RFC-0960-v37 §2.2)
- [ ] Nonce derivation per `(vault_id, max_occurred_at_unix)` for transfer replay protection (per RFC-0011-e §Security: Transfer Replay)
- [ ] HSM signing via `octo-wallet::sign_envelope` (no parallel signing abstraction; per [[cipherocto-design-principles]] no-parallel-abstractions principle)
- [ ] Substrate compatibility: all `[ADD]` entries are additive — no existing function signature changes (per RFC-0011-e §Substrate Compatibility)
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-vault --all-targets --features full -- -D warnings clean
- [ ] Cargo test -p octo-vault --lib green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-e type     | Sub-step                | Notes                                                                                                                                                                                                                   |
| ------------------- | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `VaultSummary`      | Sub-step 1 (read)       | Layer B; new struct in `octo-vault/src/lib.rs` per RFC-0960-v37 §2.1 (`vault_id: Hex32`, `chain_id: ChainId`, `owner_did: Did`, `asset_symbol: String`, `balance_projected: String`, `last_updated_unix: Option<i64>`)          |
| `VaultBalanceProjection`     | Sub-step 2 (read)       | Layer B; new struct in `octo-vault/src/lib.rs` per RFC-0960-v37 §2.1 (`chain_id`, `vault_id`, `asset_id`, `projected_balance: Dqa`, `projected_at_unix_seconds: Option<i64>`)                                               |
| `ProjectionSource`  | Sub-step 2 (read)       | Layer B; enum per RFC-0960-v37 §2.1; `#[non_exhaustive]` (Layer B additive; downstream consumers MUST handle the wildcard arm)                                                                                          |
| `TransferHandle`    | Sub-step 3 (write)      | Layer B; new struct in `octo-vault/src/lib.rs` per RFC-0960 substrate (`handle_id`, `vault_id`, `dest_vault_id`, `amount_dqa_micros`, `asset_id`, `nonce`, `status`)                                                    |
| `TransferStatus`    | Sub-step 3 (write)      | Layer B; enum per RFC-0960 substrate (`Pending` \| `Confirmed` \| `Failed`)                                                                                                                              |
| `list_owned`        | Sub-step 4 (entrypoint) | Layer B; `[ADD]` fn signature per RFC-0011-e §Substrate Additions; reads `vault_registry` (PK `(chain_id, owner_did, asset_id)` + UNIQUE INDEX on `vault_id`)                                                                           |
| `project_vault_balance`   | Sub-step 5 (entrypoint) | Layer B; `[ADD]` fn signature per RFC-0011-e §Substrate Additions; canonical SUM projection per RFC-0960-v37 §2.2; routes through `VaultBalanceCache` (LRU + TTL); falls back to `transfer_events` (RFC-0960 v014 schema) on cache miss |
| `initiate_transfer` | Sub-step 6 (entrypoint) | Layer B; `[ADD]` fn signature per RFC-0011-e §Substrate Additions; builds transfer envelope; calls `octo-wallet::sign_envelope` (HSM-bound); broadcasts to chain adapter                                                                |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Vault Subcommands for substrate wiring patterns. The CLI does NOT implement signing; this is a substrate responsibility per RFC-0011-e §Substrate Additions (`initiate_transfer` description).

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

This is the **Layer B [ADD] mission** for RFC-0011-e — the substrate crate `octo-vault` gains three new function signatures and supporting types. CLI missions consume these substrate declarations; the CLI does not redefine or shadow substrate types (per [[cipherocto-design-principles]] no-parallel-abstractions principle).

Per RFC-0011-e §No central enums for vault kinds: vault kinds are an extension surface (Layer E); new vault types are added via a `VaultKind` registry, NOT a central enum. This mission exposes the canonical `VaultSummary` shape but does not introduce a central vault-kind enum.

Per RFC-0011-e §Substrate Compatibility: all `[ADD]` entries are backward-compatible. `list_owned`, `project_vault_balance`, `initiate_transfer` are NEW functions; no existing function signature changes. `VaultSummary`, `VaultBalanceProjection`, `TransferHandle`, `ProjectionSource` are NEW types or align with existing RFC-0960-v37 substrate types.

## Risk

- **HIGH** — transfer replay (re-submitting a broadcast envelope). Mitigation: substrate-derived nonce per `(vault_id, max_occurred_at_unix)`.
- **MEDIUM** — operator chains asset by mistake. Mitigation: substrate validates `asset` against `vault_registry.asset_for(vault_id)` (no `AssetMismatch` exposed to CLI; CLI surfaces substrate rejection as `VaultNotOwned` exit 23).
- **ACCEPTED RISK** — operator coercion (HSM does not authenticate operator intent). Documented per RFC-0011-e §Adversary Analysis.

## Scope

Land the three `[ADD]` substrate API declarations per RFC-0011-e §Substrate Additions in `crates/octo-vault/src/lib.rs`. CLI surface lands in companion missions (`0011-e-vault-subcommands-readonly`, `0011-e-vault-subcommands-transfer`).

## Sub-steps

1. **`VaultSummary` struct** — `crates/octo-vault/src/lib.rs` (Layer B; per RFC-0960-v37 §2.1). `#[derive(Serialize, Deserialize, Debug, Clone)]`. Fields: `vault_id: Hex32`, `chain_id: ChainId`, `owner_did: Did`, `asset_symbol: String`, `balance_projected: String` (DQA canonical form per RFC-0960-v36), `last_updated_unix: Option<i64>` (= projection.projected_at_unix_seconds).

2. **`VaultBalanceProjection` struct + `ProjectionSource` enum** — same file (Layer B; per RFC-0960-v37 §2.1). `VaultBalanceProjection` fields: `chain_id`, `vault_id`, `asset_id` (resolved via `VaultAssetResolver`), `projected_balance: Dqa`, `projected_at_unix_seconds: Option<i64>`, `projection_source`. `ProjectionSource` enum: `Cache` \| `FreshLogScan` \| `EpochRebuild` — annotated `#[non_exhaustive]`.

3. **`TransferHandle` struct + `TransferStatus` enum** — same file (Layer B; per RFC-0960 substrate). `TransferHandle` fields: `handle_id`, `vault_id`, `dest_vault_id`, `amount_dqa_micros`, `asset_id`, `nonce` (substrate-derived), `status`. `TransferStatus` enum: `Pending` \| `Confirmed` \| `Failed`.

4. **`list_owned` fn** — same file (Layer B; per RFC-0011-e §Substrate Additions). Reads `vault_registry` (PK `(chain_id, owner_did, asset_id)` + UNIQUE INDEX on `vault_id`). Resolves each entry to a `VaultSummary`. Returns `Result<Vec<VaultSummary>, VaultError>`.

5. **`project_vault_balance` fn** — same file (Layer B; per RFC-0011-e §Substrate Additions + RFC-0960-v37 §2.2). Canonical 7-param substrate signature: `(chain_id: &ChainId, vault_id: &VaultId, registry: &dyn AssetRegistry, asset_resolver: &dyn VaultAssetResolver, log: &impl TransferEventLog, current_registry_epoch: u64, current_unix_seconds: i64) -> Result<VaultBalanceProjection, ProjectionError>`. Canonical SUM projection: `SUM(in) - SUM(out)` filtered by `ZERO_VAULT_ID` sentinel exclusion. Routes through `VaultBalanceCache` (LRU + TTL); falls back to `transfer_events` (RFC-0960 v014 schema) on cache miss. `asset_id` derived from `vault_id` via `VaultAssetResolver::resolve_asset_for`. Enforces `max_occurred_at_unix` monotonicity per `(chain_id, vault_id)` (substrate returns `Option<i64>` per RFC-0960-v37 L121).

6. **`initiate_transfer` fn** — same file (Layer B; per RFC-0011-e §Substrate Additions). Builds the transfer envelope; derives nonce per `(vault_id, max_occurred_at_unix)` (replay protection per RFC-0011-e §Security: Transfer Replay); calls `octo-wallet::sign_envelope` (HSM-bound; no parallel signing abstraction per [[cipherocto-design-principles]]); broadcasts to chain adapter. Returns `Result<TransferHandle, VaultError>`.

7. **`VaultBalanceCache` wiring verification** — `crates/octo-vault/src/cache.rs` (Layer B; per RFC-0960-v37 §2.3). Verify `VaultBalanceCache` is wired and exercised by `project_vault_balance`. Bounded LRU + unix-seconds TTL. TTL bounded per RFC-0960-v37 §2.3.

8. **`VaultAssetResolver` trait wiring** — `crates/octo-vault/src/lib.rs` (Layer B; per RFC-0960-v37 §2.1). Verify `VaultAssetResolver::resolve_asset_for(vault_id) -> AssetId` is invoked by `project_vault_balance`. Already-landed substrate trait per RFC-0960-v37.

## Cargo deps

```toml
# crates/octo-vault/Cargo.toml — additive per RFC-0011-e §Substrate Compatibility
octo-wallet = { path = "../octo-wallet" }   # Layer B HSM signing substrate (RFC-0011-e §Substrate Additions initiate_transfer)
```

No new external crates required; substrate types (`Hex32`, `ChainId`, `Did`, `AssetId`) already exist in `octo-vault` per RFC-0960-v35 + RFC-0010 alignment.

## Test Vectors (per RFC-0011-e §Test Vectors — envelope schema group)

1 TV (TV-VLT9) — envelope schema parity test verifying the substrate shape matches RFC-0011-e §Appendices B JSON Output Schemas:

| #    | Substrate surface | Input                                        | Expected Output                                                                                                   | Notes                                                            |
| ---- | ----------------- | -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| VLT9 | All 3 `[ADD]` fns | Any substrate call + downstream CLI `--json` | `OutputEnvelope<T>` with `schema_version: 3` for `VaultListOutput` / `VaultBalanceOutput` / `VaultTransferOutput` (CLI envelope; substrate `T` payload versioning is independent) | Substrate shape parity test (per RFC-0011-e §Test Vectors TV-13) |

The remaining 12 test vectors (TV-VLT1..TV-VLT8 + TV-XFER1..TV-XFER4) live in the companion CLI missions (`0011-e-vault-subcommands-readonly`, `0011-e-vault-subcommands-transfer`) where the substrate calls are exercised end-to-end through the CLI surface.

## Layer direction (RFC-0011-e §Roles and Authorities + per [[cipherocto-design-principles]])

- `octo-vault` (Layer B) — three `[ADD]` fn signatures (`list_owned`, `project_vault_balance`, `initiate_transfer`) + four new types (`VaultSummary`, `VaultBalanceProjection`, `ProjectionSource`, `TransferHandle`, `TransferStatus`). `VaultBalanceCache` wired and verified.
- `octo-wallet` (Layer B) — `sign_envelope` invoked by `initiate_transfer`; HSM-bound signing path.
- NO new Layer A types introduced.
- This is a Layer B mission; companion CLI missions are Layer C/D.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-vault --all-targets --features full -- -D warnings  # clean
cargo test -p octo-vault --lib  # green
```

## Backward compat

- Additive only per RFC-0011-e §Substrate Compatibility: `list_owned`, `project_vault_balance`, `initiate_transfer` are NEW functions; no existing function signature changes.
- `VaultSummary`, `VaultBalanceProjection`, `TransferHandle` are NEW types or align with existing RFC-0960-v37 substrate types.
- `ProjectionSource` enum is `#[non_exhaustive]`; downstream consumers MUST handle the wildcard arm (per RFC migration etiquette).
- `TransferStatus` enum may grow new variants (e.g., `Reorged`); substrate consumers MUST handle the wildcard arm.
- Per RFC-0011-e §Forward Compatibility: `OutputEnvelope<T>::schema_version = 3` is pinned (CLI envelope; substrate `T` payload versioning is independent); future amendments bump to 4.

## Cross-references

- RFC-0011-e §Substrate Additions — the canonical declarations this mission lands
- RFC-0960 §Specification, Capabilities, Reservations (grand-design; vault substrate root)
- RFC-0960-v35 §2 Path Taxonomy (canonical vault identifier shape)
- RFC-0960-v36 §Burn-Event DQA Migration (asset quantity wire form)
- RFC-0960-v37 §Balance Projection Substrate (canonical SUM projection, `ZERO_VAULT_ID` sentinel, `max_occurred_at_unix`, `VaultAssetResolver` trait)
- RFC-0010 §2 ledger_chain_registry Table DID Codec (chain identifier namespace; `ChainId` canonical form)
- RFC-0957 §Macaroon Substrate (capability witness format for RFC-0011-d role provisioning; consumed indirectly via `initiate_transfer`)
- RFC-0011 §Specification (`OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`) — consumed by companion CLI missions
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle + HSM mandatory rule
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Claimant

@unassigned
