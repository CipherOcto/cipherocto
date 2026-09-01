---
name: 0011-e-vault-subcommands-readonly
description: Land read-only vault subcommands (vault list, vault balance) per RFC-0011-e
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-e author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-e
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-e-vault-subcommands-readonly — Read-only vault subcommands (vault list, vault balance)

**Status:** Open
**Substrate:** RFC-0011-e §Subcommand Taxonomy (`vault list`, `vault balance`), RFC-0960-v37 §Balance Projection Substrate, RFC-0960-v35 §2 Path Taxonomy
**Parent:** RFC-0011-e (vault operations amendment of RFC-0011)
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root
- Mission `0011-identity-commands` — `active_did()` resolution via `octo-wallet` extensions
- Mission `0011-e-vault-substrate-additions` — substrate API declarations (`list_owned`, `project_balance`) must land first
  **Blocks:** `0011-e-vault-subcommands-transfer` (transfer requires read paths to exist for pre-flight checks)

## Status

Open (RFC-0011-e §Phase 1 read surface; transfer surface is a separate mission gated on RFC-0011-d role provisioning per `0011-e-vault-subcommands-transfer` release_gate).

## Substrate (RFC-0011-e)

Per RFC-0011-e §Subcommand Taxonomy Subcommand Taxonomy (entries for `vault list` and `vault balance`) and §7.4 Substrate `[ADD]` Signatures (`list_owned`, `project_balance`).

## Parent

RFC-0011-e (vault operations amendment; Phase 6 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-e-vault-substrate-additions` → `0011-e-vault-subcommands-readonly` → `0011-e-vault-subcommands-transfer` (transfer subcommand is gated on role provisioning per RFC-0011-d).

## Acceptance Criteria

- [ ] `octo vault list` implemented + unit-tested (TV-VLT1, TV-VLT2, TV-VLT3, TV-VLT4 pass per RFC-0011-e §Test Vectors)
- [ ] `octo vault balance <vault-id>` implemented + unit-tested (TV-VLT5, TV-VLT6, TV-VLT7, TV-VLT8 pass per RFC-0011-e §Test Vectors)
- [ ] `VaultListOutput`, `VaultBalanceOutput` payload types implemented + unit-tested
- [ ] `OctoCliRedactor` vault-specific patterns applied (vault_id truncation, owner_did redaction per RFC-0011-e §Redaction)
- [ ] `--chain-id`, `--asset-symbol`, `--cursor`, `--limit` flags wired (per RFC-0011-e §Subcommand Taxonomy `vault list` flags)
- [ ] `--no-cache`, `--history` flags wired (per RFC-0011-e §Subcommand Taxonomy `vault balance` flags)
- [ ] `VaultNotOwned` (exit 23) wired for unknown vault on `vault balance` (per RFC-0011-e §Error Handling)
- [ ] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set (per RFC-0011-e §Output Envelope)
- [ ] Cache hit warning surfaced when projection older than cache TTL (per RFC-0011-e §Security: Balance Projection Staleness)
- [ ] Cross-mission AC: read paths integrate with core mission's envelope + identity mission's active_did resolution
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-e type      | Sub-step                | Notes                                                                                                                                                         |
| -------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `VaultSummary`       | Sub-step 1 (output)     | Layer B; substrate `[ADD]` struct per RFC-0960-v37 §2.1 (`vault_id`, `chain_id`, `owner_did`, `asset_symbol`, `balance_projected`, `last_updated_unix`)       |
| `BalanceRecord`      | Sub-step 2 (output)     | Layer B; substrate `[ADD]` struct per RFC-0960-v37 §2.1 (`chain_id`, `vault_id`, `asset_id`, `projected_balance_dqa_micros`, `last_projection_unix_seconds`)  |
| `ProjectionSource`   | Sub-step 2 (output)     | Layer B; enum per RFC-0960-v37 §2.1 (`CacheHit` \| `LogQuery` \| `LogQueryFailed { error }`); CLI surfaces discriminant via Display + `serde(tag = "source")` |
| `VaultListOutput`    | Sub-step 3 (CLI output) | Layer C/D; CLI-output wrapper (`vaults: Vec<VaultSummary>`, `next_cursor: Option<String>`, `resolved_at_unix: u64`)                                           |
| `VaultBalanceOutput` | Sub-step 3 (CLI output) | Layer C/D; CLI-output wrapper (`record: BalanceRecord`, `cache_hit: bool`, `projection_source: ProjectionSource`)                                             |
| `VaultNotOwned(u32)` | Sub-step 4 (errors)     | Layer C/D; new `OctoCliError` variant; exit 23 per RFC-0011-e §Error Handling (reserved 17–63 range)                                                          |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Vault Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `VaultAction` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`last_updated_unix` is **not** wall-clock time — it is `max_occurred_at_unix(chain_id, vault_id)` from substrate. The CLI must surface this distinction in help text and warning messages (per RFC-0011-e §Security: Balance Projection Staleness).

`ProjectionSource` is delegated to the substrate; the CLI does not pattern-match on variants (forward-compatible per RFC-0011-e §Forward Compatibility).

## Risk

- **HIGH** — balance manipulation via stale projection. Mitigation per RFC-0011-e §Adversarial Review: `last_updated_unix` shown; `>TTL seconds old` warning emitted when projection age exceeds cache TTL; `--no-cache` forces re-projection; `cache_hit` and `projection_source` exposed in `VaultBalanceOutput`.
- **LOW** — concurrent CLI invocations on `vault list`. Substrate is read-only for `list`; concurrent reads safe; substrate `vault_registry` is single-writer.

## Scope

Land the 2 read-only vault subcommands per RFC-0011-e §Phase 1 (`octo vault list`, `octo vault balance <vault-id>`). Transfer subcommand is out of scope for this mission — see `0011-e-vault-subcommands-transfer`.

## Sub-steps

1. **`VaultAction` dispatch + clap wiring** — `crates/octo-cli/src/commands/vault.rs` (NEW; Layer C/D per [[cipherocto-design-principles]]; substrate reference RFC-0011-e §Subcommand Taxonomy). `enum VaultAction { List(VaultListArgs), Balance(VaultBalanceArgs) }`. Wire into existing clap root from mission `0011-core-output-envelope-redaction`.

2. **`VaultListOutput` + `VaultBalanceOutput` payload types** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. `schema_version: u32 = 1` envelope wrapper (per RFC-0011-e §Output Envelope).

3. **Read-path CLI handlers** — same file. `vault list` calls `octo_vault::list_owned(active_did)` (substrate `[ADD]` per `0011-e-vault-substrate-additions`); applies `--chain-id` (canonical form per RFC-0010) and `--asset-symbol` (client-side filter); respects `--limit`/`--cursor` defensively. `vault balance <vault-id>` calls `octo_vault::project_balance(vault_id)`; respects `--no-cache` (force `projection_source = LogQuery`); respects `--history <n>` (per RFC-0960-v37 §2.4 invalidation bus).

4. **`VaultNotOwned` error variant + exit 23 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Add `VaultNotOwned(u32)` to the `#[non_exhaustive] OctoCliError` enum; map to exit 23 per RFC-0011-e §Error Handling. Exit code 23 sits in the reserved 17–63 range.

5. **Vault-specific redaction patterns** — `crates/octo-cli/src/redact.rs` (Layer C/D; per RFC-0011-e §Redaction). Add: `vault_id` truncation (first 8 hex chars + `...` per RFC-0011 §Hex32 newtype redaction); `owner_did` redaction unless `owner_did == active_did` (per RFC-0011 §Redaction Layer). `chain_id`, `asset_symbol`, `balance_projected`, `last_updated_unix` are NOT redacted (chain-public info).

6. **Cache staleness warning** — `crates/octo-cli/src/commands/vault.rs` (Layer C/D; per RFC-0011-e §Security). When `last_projection_unix_seconds` is older than the cache TTL (substrate exposes via `ProjectionSource::CacheHit` cache age), emit a warning to stderr: `>N seconds old; consider --no-cache`. Staleness is informational, not an error.

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-e §Key Files to Modify
octo-vault = { path = "../octo-vault" }   # Layer B substrate (RFC-0011-e §Key Files to Modify)
```

No new external crates required; all substrate types (`VaultSummary`, `BalanceRecord`, `ProjectionSource`) are defined in `octo-vault` and re-used by the CLI.

## Test Vectors (per RFC-0011-e §Test Vectors — read group)

8 TV (TV-VLT1..TV-VLT8) covering `vault list` (4) and `vault balance` (4):

| #    | Subcommand      | Input                                    | Expected Output                                                      | Notes                                                                             |
| ---- | --------------- | ---------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| VLT1 | `vault list`    | `--chain-id chain-a --asset-symbol OCTO` | 3 vaults filtered by chain + asset                                   | Filter applied client-side; envelope includes all matching `VaultSummary` records |
| VLT2 | `vault list`    | No flags                                 | All vaults owned by active DID across all chains                     | Output envelope includes `next_cursor` if more than `--limit`                     |
| VLT3 | `vault list`    | `--limit 0`                              | Empty `vaults` array + warning                                       | Defensive: substrate validates `limit >= 1`; CLI surfaces `InvalidLimit`          |
| VLT4 | `vault list`    | `--json`                                 | Single-line JSON envelope                                            | TTY-aware override (per RFC-0011 §Output Envelope)                                |
| VLT5 | `vault balance` | `<vault-id>` (cache-hit)                 | `cache_hit: true`, `projection_source: CacheHit`                     | Cache hit returns in `<50ms`                                                      |
| VLT6 | `vault balance` | `<vault-id> --no-cache`                  | `cache_hit: false`, `projection_source: LogQuery`                    | Forces substrate re-projection                                                    |
| VLT7 | `vault balance` | `<vault-id>` for vault with no events    | `projected_balance_dqa_micros: 0`, `last_projection_unix_seconds: 0` | Vault exists but no transfers yet                                                 |
| VLT8 | `vault balance` | `<vault-id>` for unknown vault           | `VaultNotOwned` (exit 23)                                            | Substrate rejects; CLI surfaces structured error                                  |

## Layer direction (RFC-0011-e §Roles and Authorities + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `VaultAction` dispatch + 2 output structs (`VaultListOutput`, `VaultBalanceOutput`) + `VaultNotOwned` error variant + vault-specific redaction patterns.
- `octo-vault` (Layer B) — substrate `[ADD]` `list_owned`, `project_balance`, `VaultSummary`, `BalanceRecord`, `ProjectionSource` (landed via companion mission `0011-e-vault-substrate-additions`).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `octo-vault` extended per `[ADD]` declarations in companion mission `0011-e-vault-substrate-additions`; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-e §Error → Exit Code Table (only `VaultNotOwned` exit 23 added in this mission).
- `OutputEnvelope<T>::schema_version = 1` pinned (RFC-0011-e §Forward Compatibility); future amendments bump to 2; old CLI ignores unknown fields.
- `ProjectionSource` enum may grow new variants (e.g., `LogQueryDegraded`); CLI surfaces discriminant via Display + `serde(tag = "source")` without pattern-matching on variants.

## Cross-references

- RFC-0011-e §Subcommand Taxonomy (`vault list`, `vault balance` entries)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0011-d §Role Provisioning — gates the transfer subcommand (out of scope here; companion mission `0011-e-vault-subcommands-transfer`)
- RFC-0960-v37 §Balance Projection Substrate (canonical SUM projection, `ZERO_VAULT_ID` sentinel, `max_occurred_at_unix`, `VaultAssetResolver` trait)
- RFC-0960-v35 §2 Path Taxonomy (canonical vault identifier shape)
- RFC-0960-v36 §Burn-Event DQA Migration (asset quantity wire form)
- RFC-0010 §2 ledger_chain_registry Table (chain ID canonical form)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Claimant

@unassigned
