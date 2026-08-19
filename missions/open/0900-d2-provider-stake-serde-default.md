---
id: 0900-d2
title: ProviderStake serde-default hardening (chain_id field)
status: OPEN
opened: 2026-08-19
priority: LOW
parent: 0900-d
type: hardening
depends_on:
  - 0900-d
  - 0900-d1
---

# 0900-d2 — ProviderStake serde-default hardening

## Context

Round-3 inline adversarial review surfaced a `MISSING SERDE DEFAULT` finding
(N4) on `ProviderStake.chain_id` at
`crates/quota-router-core/src/marketplace/slashing.rs:354`. The struct gained
the `chain_id` field in mission `0900-d1` (per RFC-0010 v1.4 typed `ChainId`)
without `#[serde(default)]`, so any pre-`0900-d1` JSON payload missing
`chain_id` would fail to deserialize.

## Severity assessment

**Severity DOWNGRADED to LOW hardening:**

- No production code path serializes `ProviderStake` via serde today.
  All stake mutation flows through an in-memory `HashMap<(chain_id,
  provider_id), ProviderStake>` in `SlashingLedger` (no DB round-trip;
  persistence is via `SlashLedgerRow` which is a SEPARATE struct in
  `quota-router-storage`).
- The `#[derive(Deserialize)]` on `ProviderStake` exists for forward-
  compat (future wire protocols, snapshot import, RPC envelopes) —
  not exercised today.
- No caller currently constructs a `ProviderStake` JSON payload. The
  field is written exclusively via `..Default::default()` patterns
  in production paths, all using `DEFAULT_CHAIN_ID` sentinel.

This is a defensive hardening — the field is required at compile time
(serde-required == type-required by default), but a future caller that
constructs JSON via `serde_json::to_value(&stake)` then re-deserializes
after a hypothetical schema-rev would crash without `#[serde(default)]`.

## Acceptance criteria

- **AC-1:** `#[serde(default)]` decorator present above
  `pub chain_id: [u8; 32]` at `crates/quota-router-core/src/marketplace/slashing.rs:354`.
- **AC-2:** Default value matches production `DEFAULT_CHAIN_ID` sentinel
  (`[0_u8; 32]`). The `<[u8; 32] as Default>::default()` impl returns
  `[0_u8; 32]`, which equals the `DEFAULT_CHAIN_ID` const in
  `quota-router-storage/src/slash_store.rs:25`. Verified via const-eval.
- **AC-3:** TV test added: JSON round-trip without `chain_id` field
  deserializes with `chain_id = [0_u8; 32]`. Test name:
  `provider_stake_json_without_chain_id_defaults_to_zero_sentinel`
  in `crates/quota-router-core/tests/tv_provider_stake_serde_default.rs`
  (new file).
- **AC-4:** TV test for explicit `chain_id` round-trips byte-exact (no
  defaulting behavior regression). Test name:
  `provider_stake_json_with_chain_id_round_trips_byte_exact` in same
  file.
- **AC-5:** All other `ProviderStake` fields still REQUIRED after the
  `#[serde(default)]` patch (default only applies to the annotated
  field). Verified by TV-3-style missing-field test on
  `provider_id` (must fail).
- **AC-6:** `cargo test -p quota-router-core --features full --tests` —
  full green (no regressions in slashing/marketplace tests).
- **AC-7:** `cargo clippy --all-targets -p quota-router-core --features
  full -- -D warnings` — clean.
- **AC-8:** `cargo fmt --all -- --check` — clean.

## Risks

- **LOW**: `#[serde(default)]` on `[u8; 32]` uses `<[u8; 32] as
  Default>::default()` which is `[0_u8; 32]`. The `ChainId::default()`
  impl (in `octo-ident/src/chain.rs:167`) returns the non-zero
  `CIPHEROCTO_MAINNET` namespace — DIFFERENT from `DEFAULT_CHAIN_ID`.
  Mitigation: the field type is `[u8; 32]` (raw bytes), not `ChainId`;
  sentinel-zero is correct here per production usage at all 9 call
  sites in `slashing.rs` (lines 597/609/681/849/1203 + keys 476/1468).
- **LOW**: serde attribute ordering matters — `#[serde(default)]` must
  precede the field declaration. Convention is to place it directly
  above the field, similar to other `#[serde(with = "...")]`
  decorators on neighboring fields.

## Out of scope

- Renaming `chain_id` to `chain_namespace` (separate consideration;
  field is byte-typed not typed-ChainId by design).
- Adding `ChainId`-typed alias struct (would require RFC-0010 v1.5
  cross-crate refactor; defer).
- Other serde hardening on neighboring `ProviderStake` fields (no
  schema break pending; defer).

## Verification

```bash
cd /home/mmacedoeu/_w/ai/cipherocto

# After patch:
cargo test -p quota-router-core --features full --test tv_provider_stake_serde_default
# Expected: 3/3 green (missing-chain_id defaults, explicit round-trip,
#           missing-other-field still errors)

cargo test -p quota-router-core --features full --tests
# Expected: full green incl. existing slashing/marketplace tests

cargo clippy --all-targets -p quota-router-core --features full -- -D warnings
cargo fmt --all -- --check

# Confirm decorator placement:
grep -B1 'pub chain_id: \[u8; 32\]' crates/quota-router-core/src/marketplace/slashing.rs
# Expected: #\[serde(default)\] line directly above pub chain_id line
```

## Files

- `crates/quota-router-core/src/marketplace/slashing.rs` — 1 decorator added
- `crates/quota-router-core/tests/tv_provider_stake_serde_default.rs` — new file (3 tests)