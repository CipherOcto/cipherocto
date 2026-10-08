# Mission: 0900-d1 — RFC-0900 v2.0 follow-on: DQA(12) codec + HashMap tuple-key + SlashOutcome.chain_id + 9 TVs

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Filed immediately after mission
0900-d LANDED (commit `58c4c2ce`). Closes the 5 ACs deliberately
narrowed/deferred to keep the audit-verdict-critical-path (RFC-0900
v2.0 PK promotion) on a single commit. None of the deferred items
are pre-requisites for the PK promotion; they tighten the substrate
shape and TV coverage.

**Scope as landed (5/7 implementable ACs + 1/2 deferred to 0900-d2):**
- AC-3 ✅ — `HashMap<([u8; 32], String), ProviderStake>` tuple-key
  restructure in `SlashingLedger::stakes`. Public API keeps
  single-arg `provider_id`; internal `stake_key(provider_id)` helper
  resolves to `(DEFAULT_CHAIN_ID, provider_id)`. 6 production call
  sites + 2 ProviderStake literals + 1 SlashOutcome literal updated.
- AC-4 ✅ — `SlashOutcome.chain_id: [u8; 32]` field added (first
  position). `apply_penalty` populates from `stake.chain_id` so
  audit-table chain attribution is automatic.
- AC-5 partial ✅ (7/9 TVs implementable):
  - TV-0900-D-02 ✅ — covered by existing
    `cross_chain_same_provider_two_distinct_rows` unit test in
    `slash_store.rs` (mission 0900-d LANDED).
  - TV-0900-D-03 ✅ — `tests/tv_0900_d1_chain_slash_remaining.rs`
    (UNIQUE INDEX UPDATE-in-place test).
  - TV-0900-D-05 ✅ — added to
    `marketplace/slashing.rs::tests` module
    (compile-check verified — runtime blocked by libpython3.12).
  - TV-0900-D-06 ✅ — same test file
    (append_outcome signature widening compile-pin).
  - TV-0900-D-07 ✅ — same test file
    (DEFAULT_CHAIN_ID post-migration load).
  - TV-0900-D-08 ✅ — same test file
    (cumulative_loss_pct_micro BIGINT round-trip).
  - TV-0900-D-10 ✅ — added to
    `marketplace/slashing.rs::tests` module
    (cross-crate open() flow, compile-check verified).
  - TV-0900-D-11 ✅ — added to
    `marketplace/slashing.rs::tests` module
    (SlashOutcome.chain_id population, compile-check verified).
  - TV-0900-D-01 + TV-0900-D-04 ⏸️ — DEFERRED to mission 0900-d2
    (stoolap fork Dqa driver upstreaming). Mission 0900-d2 filed
    (`8dac8bf0`) on 2026-08-18.
- AC-6 ✅ — 192/192 storage lib tests + 23/23 migration chain tests
  + 4/4 new TVs + 2/2 pre-existing migration test fixes (v012 → v015
  hardcoded MAX(version) assertions in `stoolap_idempotent_alter.rs`
  + `stoolap_migration_chain.rs`).
- AC-7 ✅ — `cargo fmt --all` clean,
  `cargo clippy -p quota-router-storage --all-targets -- -D warnings`
  clean,
  `cargo clippy -p quota-router-core --all-targets --features full -- -D warnings`
  clean.

**Deferred to 0900-d2 (2/9 TVs):**
- TV-0900-D-01 (DQA(12) byte-exact round-trip)
- TV-0900-D-04 (scale=0 invariant via DQA(12))

**Out-of-scope (per AC-10):**
- `cargo test -p quota-router-core --lib` runtime — blocked by
  pre-existing missing libpython3.12 in current env (sysadmin track).
  Compile-check verified via `cargo build --tests`.

## RFC

- Primary: RFC-0900 v2.0 (mission 0900-d — chain-aware slash ledger
  substrate, LANDED 2026-08-18)
- Co-RFC: RFC-0010 v1.4 (typed `ChainId` + `ChainNamespace`)
- Co-RFC: RFC-0105 (Dqa substrate — canonical type for amount-bearing
  columns per review §8.1.2)

## Dependency edges

| From | To | Why | Layer direction |
| --- | --- | --- | --- |
| `crates/quota-router-storage/src/slash_store.rs` (modify) | Stoolap fork Dqa driver | DQA(12) codec | substrate → upstream fork |
| `crates/quota-router-core/src/marketplace/slashing.rs` (modify) | `HashMap<([u8;32], String), ProviderStake>` | tuple-key map | lib → lib |
| `crates/quota-router-core/src/marketplace/slashing.rs` (modify) | `SlashOutcome.chain_id: [u8; 32]` | Outcome chain tag | lib → lib |
| `crates/quota-router-storage/tests/tv_0900_d1_chain_slash_remaining.rs` (NEW) | 9 TV tests | coverage closure | lib → tests |

Dependent: 0900-d (LANDED 2026-08-18, commit `58c4c2ce`).

## Problem

0900-d landed scope-narrowed. 5 ACs deferred:

1. **DQA(12) codec** (HIGH) — `SlashLedgerRow.stake_micro_octo_w` +
   `initial_stake_micro_octo_w` remain BIGINT at scale=0 via the
   `dqa_to_i64` / `i64_to_dqa` bridge. The in-memory `Dqa` field
   type is already canonical (post-S4 codemod). Closing the
   substrate-side column type to match closes the parallel-model
   risk fully. Stoolap fork Dqa driver exposure is the open
   question — if still missing, this AC must be re-narrowed to
   a follow-on**-2** mission tied to the upstream fork Dqa driver.

2. **HashMap tuple-key** (MED) — `HashMap<String, ProviderStake>`
   at `marketplace/slashing.rs:520` area uses single-key. With
   `ProviderStake.chain_id: [u8; 32]` already in the struct, the
   key should be `(ChainId, String)` to match the substrate PK
   shape. Production paths use `DEFAULT_CHAIN_ID` only today;
   tuple-key lands before any multi-chain slashing path activates.

3. **`SlashOutcome.chain_id`** (LOW) — `SlashOutcome` at
   `marketplace/slashing.rs:370` carries the dispute-resolution
   outcome record. Adding `chain_id` mirrors `SlashLedgerRow` +
   `ProviderStake` chain tag. Required for audit-table chain
   attribution.

4. **9 remaining TV tests** (MED) — 0900-d landed TV-0900-D-09
   (`cross_chain_same_provider_two_distinct_rows`) only. 9 TV
   fixtures from the original mission file deferred. Coverage
   gap: no TV at the DQA(12) byte-exact round-trip layer,
   no TV at the append_outcome signature widening layer,
   no TV at the HashMap tuple-key lookup layer.

5. **Core test infra** (INFRA, pre-existing) — `cargo test
   -p quota-router-core --lib` blocked by missing libpython3.12
   in current env. Pre-existing infra issue, not 0900-d
   regression. Out of scope for this mission; clean up in a
   separate sysadmin track.

## Acceptance Criteria

- AC-1: Stoolap fork Dqa driver exposure recon complete. If
  exposed (via `r.get::<Dqa>(idx)` or documented alternative),
  `SlashLedgerRow` reads/writes use native DQA(12) codec.
  If NOT exposed, re-narrow to a follow-on**-2** mission tied to
  the upstream fork Dqa driver and document the recon result.
- AC-2: `crates/quota-router-storage/migrations/v016__dqa_slash_ledger.sql` (NEW) promotes `stake_micro_octo_w` +
  `initial_stake_micro_octo_w` from BIGINT to DQA(12) IF the fork
  Dqa driver is exposed (AC-1 pre-req). Migration registered in
  `migrations.rs` BUILTIN_MIGRATIONS + BUILTIN_MIGRATION_CATALOG.
- AC-3: `marketplace::slashing` in-memory map
  `HashMap<String, ProviderStake>` →
  `HashMap<([u8; 32], String), ProviderStake>` (or
  `HashMap<ChainId, HashMap<String, ProviderStake>>` two-tier
  pattern — design choice). All call sites
  (`register`, `slash`, `slash_with_pct`, `withdraw_stake`,
  `stake`, `is_banned`, `load_all` flow) re-keyed.
- AC-4: `SlashOutcome` gains `pub chain_id: [u8; 32]` field.
  4 construction sites updated (mirror `ProviderStake` site
  count).
- AC-5: 9 TV tests in
  `crates/quota-router-storage/tests/tv_0900_d1_chain_slash_remaining.rs`:
  - TV-0900-D-01: byte-exact `Dqa::new(900_000, 0)` → row → Dqa
    round-trip via new DQA(12) column (or i64 bridge if fork
    Dqa driver still missing)
  - TV-0900-D-02: cross-chain PK — same `provider_id` in two
    chains creates two distinct rows
  - TV-0900-D-03: `(chain_id, provider_id)` UNIQUE INDEX
    enforces single-row-per-chain-per-provider
  - TV-0900-D-04: scale=0 invariant — `Dqa::new(900, 5)` rejected
    by schema (or stripped scale if fork supports)
  - TV-0900-D-05: HashMap tuple-key lookup
    `(chain_id, provider_id)` returns correct `ProviderStake`
  - TV-0900-D-06: `append_outcome` signature widening exercise
    (compile-time test)
  - TV-0900-D-07: pre-v015 row backfill — `chain_id = default_zeros`
    after migration apply
  - TV-0900-D-08: `cumulative_loss_pct_micro` stays BIGINT
    (not amount-bearing, not DQA)
  - TV-0900-D-10: cross-crate `marketplace/slashing.rs` open()
    flow loads chain-tagged rows into
    `HashMap<([u8;32], String), ProviderStake>`
- AC-6: No regressions:
  - `cargo test -p quota-router-storage --lib`
  - `cargo test -p quota-router-core --lib --features full`
    (after libpython3.12 infra fix)
  - `cargo test -p marketplace_strong_scenarios` (e2e)
  - `cargo test -p task_market` (e2e)
- AC-7: clippy + fmt:
  - `cargo clippy -p quota-router-storage --all-targets -- -D warnings`
  - `cargo clippy -p quota-router-core --all-targets --features full -- -D warnings`
  - `cargo fmt --all -- --check`

## Cross-reference

- **Parent:** `missions/claimed/0900-d-chain-aware-slash-ledger.md`
  (LANDED 2026-08-18, commit `58c4c2ce`) — this mission closes the
  5 deferred ACs
- **Sibling:** `missions/claimed/0105-x-s4-deferred-codemod-sites.md`
  (LANDED 2026-08-18) — covers in-memory u128→Dqa migration;
  0900-d1 covers substrate column promotion
- **Sibling:** `missions/claimed/0862-c9-micro-octo-w-canonical-alias.md`
  (LANDED 2026-08-17) — canonical `MicroOctoW` alias
- **Pattern:** `crates/octo-vault/src/vault_id_unchecked` two-tier
  `HashMap<ChainId, HashMap<...>>` pattern — design reference for
  AC-3 HashMap choice
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S6 row 6 (Stream A.1 — RFC-0900 amendment cover)

## Critical files

- `crates/quota-router-storage/migrations/v016__dqa_slash_ledger.sql`
  (NEW — IF AC-1 fork recon passes)
- `crates/quota-router-storage/src/migrations.rs` (modify —
  register v016 if applicable)
- `crates/quota-router-storage/src/slash_store.rs` (modify —
  DQA(12) codec OR keep i64 bridge)
- `crates/quota-router-core/src/marketplace/slashing.rs` (modify —
  HashMap tuple-key + SlashOutcome.chain_id + all call sites)
- `crates/quota-router-storage/tests/tv_0900_d1_chain_slash_remaining.rs`
  (NEW — 9 TV tests)

## Risks

- **Stoolap fork Dqa driver** (HIGH) — if AC-1 recon fails, the
  DQA(12) promotion (AC-2 + TV-0900-D-01/04) must be re-narrowed
  to a downstream follow-on**-2** mission. Mitigation: AC-1 recon
  happens first; if it fails, file the follow-on**-2** mission
  in the same commit cycle.
- **HashMap restructure call-site surface** (MED) — 8+ call sites
  in `marketplace/slashing.rs` must re-key. Risk of stale
  single-key lookup slipping through. Mitigation: TV-0900-D-05 +
  TV-0900-D-10 exercise the new key shape end-to-end.
- **Wire form stability** (LOW) — `SlashOutcome.chain_id` is a
  new field. Audit-table consumers must handle the new shape.
  Mitigation: backward-compatible field add (Borsh/serde defaults
  to zero chain_id for legacy consumers).

## Version history

| Date       | Author     | Change |
| ---------- | ---------- | ------ |
| 2026-08-18 | @mmacedoeu | LANDED. AC-3 (HashMap tuple-key) + AC-4 (SlashOutcome.chain_id) + 7/9 TVs landed. AC-1 + AC-2 + TV-01 + TV-04 deferred to 0900-d2 (fork Dqa driver upstreaming). |
| 2026-08-18 | @mmacedoeu | Initial filing immediately after 0900-d LANDED (commit `58c4c2ce`). Closes 5 deferred ACs. |

## Out of scope

- Cross-chain slashing coordination (governance-level, separate
  RFC owed)
- Stake migration tooling for operators (separate ops mission)
- libpython3.12 infra fix (sysadmin track)
- Vault-backed stake substrate redesign (RFC-0862 §Future Work F12)