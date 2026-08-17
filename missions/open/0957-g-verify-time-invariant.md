# Mission: 0957-g verify-time invariant (S5 of storage restructure)

## Status

**Claimed (2026-08-17, claimant @mmacedoeu).** S5 deliverable per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 6 (Stream C.2). Pre-reqs verified landed: S2 (octo-storage
split), S3 (octo-vault substrate + TV-V1 10), S4 (DFP codemod — `Dqa`
canonical). Work in progress.

## RFC

- Primary: RFC-0957 (verify-time bump) per review §20.6.1.
- Co-RFC: RFC-0870 (additive) — `NodeEnvelope.version_tag: u8` field
  per review §14.1.
- Co-RFC: RFC-0965 §3.5 + §3.7 — `WrappedOnly` caveat + parent-no-
  Vault-binding reject per review §20.6.1 line 1328.
- Source review: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 (envelope.version_tag) + §20.6.1 (verify-time chain invariant)
  + §8.10 (TV-C1 anchor).

## Summary

Three pillars of plan §C.2 land in one crate family:

1. **`NodeEnvelope.version_tag: u8` field** in `crates/octo-protocol/src/envelope.rs`
   per §14.1. `VERSION_TAG_V2 = 0xA1` post-cutover; `VERSION_TAG_V1 = 0xA0`.
   V1 receipts (or absent version_tag) hard-rejected at verify
   deterministically. Wire-format break per §14.1; consumers must
   rebuild.

2. **`Caveat::Vault(vault_id)` verify-time UNIQUE INDEX lookup** per
   §20.6.1 option (b) (adopted). `octo-cap-macaroon` gains a new
   `VaultLookup` trait (`fn lookup_vault_row(&[u8;32]) -> Option<VaultRowRef>`)
   consumed by `CapabilityTokenV2::verify_vault_bound()`; `octo-vault`
   implements via the substrate-backed `vaults_vault_id_idx` UNIQUE
   index. Layer-B → Layer-B dep edge (octo-vault at Layer B; macaroon
   at Layer B per `cipherocto-design-principles.md`).

3. **`WrappedOnly` intra-chain-only rule + parent-no-Vault-binding
   reject** per §20.6.1 line 1328. When `Macaroon::verify_full` walks a
   `WrappedOnly` chain for a `VaultOperation` target, the ancestor
   chain must contain at least one `Caveat::Vault(vault_id)` — chainless
   parent (e.g., pure `AmountMax` cap with no vault binding) yields
   `MacaroonError::WrappedChainHasNoVault`. Vault chain_id must match
   operation target.

4. **TV-C1 fixtures (4)** written + passing in
   `crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs` per §8.10
   central registry.

### Why option (b) over option (c) (§20.6.1)

Option (b) (vault row UNIQUE INDEX lookup) adopted as DEFAULT per
§20.6.1 — no V2 wire-format bump required, +1 DB round-trip per
redemption (~1-3ms SSD). Option (c) (chain_id in payload) deferred to
v3.0 if perf data demands.

### Why VaultLookup trait not direct dep

`octo-cap-macaroon` is a Layer B extension crate (per
`cipherocto-design-principles.md`). Per **Stable Abstractions
Principle** + **Dependency Inversion**: trait lives in
`octo-cap-macaroon` (consumer), impl lives in `octo-vault` (owner of
the data). Mirrors existing `CapabilityCatalog` pattern
(`crates/octo-cap-macaroon/src/macaroon.rs:425`). Avoids premature
`octo-vault` direct dep at the macaroon verify path.

## Acceptance Criteria

1. `octo-protocol/src/envelope.rs`: `NodeEnvelope.version_tag: u8` field
   added. `VERSION_TAG_V2 = 0xA1`, `VERSION_TAG_V1 = 0xA0` exported as
   `pub const`.
2. `NodeEnvelope::build()` accepts `version_tag: u8`; rejects values
   other than `0xA0`/`0xA1` with `ProtocolError::UnsupportedVersion(u8)`.
3. `NodeEnvelope::verify_version()` helper returns `Err` for V1
   (`0xA0`) and absent-version-tag, `Ok` for V2 (`0xA1`).
4. `compute_envelope_id()` inputs include `version_tag` (post-cutover
   V2 receipts deterministically land at different `envelope_id`s
   than V1 — prevents replay across cutover).
5. `octo-cap-macaroon/src/vault_lookup.rs` (NEW): trait `VaultLookup`
   `pub trait VaultLookup: Send + Sync { fn lookup_vault_row(vault_id:
   &[u8; 32]) -> Option<VaultRowRef>; }` where `VaultRowRef { chain_id:
   [u8; 32], state: octo_vault::VaultState }`. Re-export at
   `crate::vault_lookup`.
6. `octo-vault/src/vault_lookup_impl.rs` (NEW): `OctoVaultLookup` wraps
   `Arc<dyn Storage>` + implements `VaultLookup`. UNIQUE INDEX lookup
   against `vaults.vaults_vault_id_idx` (per §20.6.1 algorithm step 2).
7. `CapabilityTokenV2::verify_vault_bound(&self, op_chain_id: &[u8;32],
   lookup: &dyn VaultLookup) -> Result<VaultBoundProof, CapabilityError>`
   implements §20.6.1 5-step algorithm verbatim:
   - step 1: extract `cap.caveats.Vault.vault_id`
   - step 2: `vault_row = lookup.lookup_vault_row(vault_id)?`
   - step 3: assert `vault_row.chain_id == op_chain_id`
   - step 4: assert `vault_row.state == Active`
   - step 5: return VaultBoundProof(vault_id, vault_row.chain_id)
8. `Macaroon::verify_full` extension: when verifying for a
   `VaultOperation`, walk ancestor `WrappedOnly` chain. If no
   ancestor carries `Caveat::Vault(_)`, return
   `MacaroonError::WrappedChainHasNoVault`. Otherwise locate deepest
   ancestor Vault caveat → assert its chain matches op_chain_id.
9. `octo-protocol::envelope.rs` borsh round-trip: forward-compatible
   deserialization of pre-V2 bytes (no `version_tag`) returns
   `ProtocolError::MissingVersionTag` (loud reject, not silent
   fallback).
10. `crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs` (NEW):
    - **TV-C1-01**: `Caveat::Vault(vault_id)` verifies when vault row
      exists and state=Active.
    - **TV-C1-02**: `Caveat::Vault(vault_id)` rejects with
      `CapabilityError::VaultRowMissing(vault_id)` when
      `lookup_vault_row` returns `None`.
    - **TV-C1-03**: `WrappedOnly` chain WITH parent's `Caveat::Vault`
      verifies when vault chain matches op chain.
    - **TV-C1-04**: `WrappedOnly` chain WITHOUT parent's `Caveat::Vault`
      (chainless) rejects with
      `MacaroonError::WrappedChainHasNoVault`.
11. Verification gate (plan §4 S5):
    ```bash
    cargo test -p octo-vault --lib           # TV-C1-01..02 + existing 5
    cargo test -p octo-cap-macaroon --lib    # TV-C1-01..04 + existing WrappedOnly tests (no regression)
    cargo test -p octo-protocol --lib        # new version_tag tests + no regression
    cargo build --workspace --all-targets    # no breakage elsewhere
    cargo clippy --all-targets --all-features -- -D warnings
    cargo fmt --all -- --check
    npx prettier --write missions/open/0957-g-verify-time-invariant.md
    ```
12. Memory card written: `memory/mission-0957-g-verify-time-invariant-status.md`
    following `MEMORY.md` template (after S5 LANDED).

## Out of scope (deferred)

- 7 RFC §22 atomic-blocker bundle (S6).
- RFC-0957 v3.0 option (c) wire-format bump (deferred per §20.6.1 last
  row, conditionally triggered by perf data).
- 7 marketplace_strong_scenarios TV-D9 + TV-D10 fixtures (RFC-0105 +
  RFC-0965 amendment territory per S6).
- borsh-schema-version migration for V1 receipt drain (cutover plan
  in B0 PR bundle).
- New `OctoVaultLookup` wrapper in `octo-vault-node` Layer C crate
  (S6+ follow-on).

## Verification (per plan §4 S5 gate)

```bash
# Plan §4 S5 prescribed gates
cargo test -p octo-vault --lib
cargo test -p octo-cap-macaroon --lib
cargo test --workspace --lib   # no regressions in 8 surrounding crates

# Project-mandatory gates (per CLAUDE.md + memory cards)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib   # any leftover lib regressions
```

## Dependency edges

| From | To | Why | Layer direction |
|---|---|---|---|
| `octo-protocol` | (none new) | `NodeEnvelope` self-contained | Layer B → Layer A only |
| `octo-cap-macaroon` | `octo-protocol::WireDid` (existing) + new `octo-vault::VaultState` import (state enum only, no impl) | Reuse state enum; no data coupling | Layer B → Layer B |
| `octo-vault` | (none new) | `OctoVaultLookup` impl uses existing substrate handle | Layer B → Layer A |
| consumers | `octo-cap-macaroon::VaultLookup` trait | Trait injection at config time | Layer C → Layer B |

No new cyclic edges. No new upward deps (Layer B does NOT gain dep
on Layer C).

## Critical files (proposed)

- `crates/octo-protocol/src/envelope.rs` (modify — add `version_tag`,
  bump `compute_envelope_id`, add `verify_version` helper + tests)
- `crates/octo-protocol/src/lib.rs` (re-export VERSION_TAG_V1/V2)
- `crates/octo-cap-macaroon/src/vault_lookup.rs` (NEW — trait + types)
- `crates/octo-cap-macaroon/src/bundle_v2.rs` (modify — add
  `verify_vault_bound` method + tests)
- `crates/octo-cap-macaroon/src/macaroon.rs` (modify — WrappedOnly
  parent-Vault-binding check + tests + new error variant)
- `crates/octo-cap-macaroon/src/lib.rs` (re-export `vault_lookup`)
- `crates/octo-vault/src/vault_lookup_impl.rs` (NEW — `OctoVaultLookup`
  struct + `VaultLookup` impl)
- `crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs` (NEW — TV-C1
  fixtures)
- `crates/octo-protocol/tests/envelope_version.rs` (NEW or extend
  existing — V1 reject / V2 accept / absent-version reject tests)
- `memory/mission-0957-g-verify-time-invariant-status.md` (NEW —
  LANDED status card)

## Existing patterns reused

- `CapabilityCatalog` trait (`crates/octo-cap-macaroon/src/macaroon.rs:425`)
  → exact topology for `VaultLookup`: trait in consumer crate, impl
  in owner crate, injected at config time.
- `BUILTIN_MIGRATION_CATALOG` migration tuple/catalog duality pattern
  (`crates/octo-vault/src/migrations.rs:18+32`) → if/when
  `OctoVaultLookup` carries a static config struct, mirror the pattern.
- NodeEnvelope `build()` boundary validation
  (`crates/octo-protocol/src/envelope.rs:56-80`) → extend with
  version_tag boundary check (rejects invalid at construction, not at
  verify, so debug logs catch the error loud).

## Risks (per plan §5)

- **B.3 verify-time invariant is load-bearing** (HIGH per plan §5):
  pre-deploy gate at §4 S5 must pass; if any test fails, §22 B0
  atomic-blocker rule holds.
- **Backward compat on `NodeEnvelope`**: V2 receipt drain requires
  coordinated consumer rebuild. RFC-0870 (additive) amendment text
  (S6 territory) MUST include consumer migration path.
- **`Caveat::Vault` collision**: macaroon crate currently has
  `Caveat::Vault([u8;32])` (per caveat/mod.rs:1139-1141, 1340, 1517).
  Verify-time wiring is incremental — no enum change required.

## Critical files reference at commit time

```text
crates/octo-protocol/src/envelope.rs
crates/octo-protocol/src/lib.rs
crates/octo-cap-macaroon/src/lib.rs
crates/octo-cap-macaroon/src/vault_lookup.rs       (NEW)
crates/octo-cap-macaroon/src/bundle_v2.rs
crates/octo-cap-macaroon/src/macaroon.rs
crates/octo-vault/src/lib.rs
crates/octo-vault/src/vault_lookup_impl.rs        (NEW)
crates/octo-cap-macaroon/tests/tv_c1_verify_time.rs (NEW)
crates/octo-protocol/tests/envelope_version.rs     (NEW or extended)
memory/mission-0957-g-verify-time-invariant-status.md (NEW)
missions/open/0957-g-verify-time-invariant.md      (this file)
```
