# 0960-v37-c-legacy-balance-deprecation — 3-cycle deprecation of legacy Balance substrate

**Status:** Open
**Substrate:** RFC-0960 §5.1 (3-cycle deprecation timeline)
**Parent:** RFC-0960 §6 Implementation Path Mission C
**Depends on:** Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — provides the replacement `VaultBalanceProjection` substrate this mission deprecates against; Mission B (`0960-v37-b-event-log-producer-wiring.md`) — producer wiring for `VaultProjectionInvalidationEnvelope` over `cache:projection:<hex(vault_id)>` channel (RFC-0913 pub/sub); Mission B has zero `KeyStorage` references — R7 finding removed fabricated claim

## Scope

Apply the §5.1 3-cycle deprecation timeline to retire the legacy `Balance` struct,
`octo_w_balances` table, and `Vault.balance_dqa_micros` field. Mission A lands the
substrate; Mission B wires the producers; Mission C deletes the legacy.

### Mission C sub-steps (3 cycles, gated per RFC-0960 §5.1)

**Cycle 1 — Deprecation stub (default ON):**

1. **`Balance { amount: u64 }` struct** (`crates/quota-router-core/src/balance.rs:9`).
   Add `#[deprecated(note = "use VaultBalanceProjection per RFC-0960 v3.7 §2.1")]` to
   the struct + all methods (`new`, `add`, `subtract`, `as_u64`, etc.). Verify
   5 callers of `Balance::new` migrate to `VaultBalanceProjection::get_or_compute`
   in same PR OR carry `#[allow(deprecated)]` with TODO comment + ticket.

2. **`Vault.balance_dqa_micros: i64` field** (`crates/octo-vault/src/lib.rs:278`).
   Add `#[deprecated]` attribute. Column kept on `vaults` table. grep-verified
   0 production write sites remain (stranded field per §5.2).

3. **`octo_w_balances` table** (`crates/quota-router-core/src/schema.rs:182-186`).
   Init retained; reads serve legacy callers. No flag gating yet (Cycle 2
   introduces gating).

**Cycle 2 — Core deletion (default OFF, gated behind `legacy_octo_w` feature flag):**

1. **`Balance` struct + all methods** — REMOVED from
   `crates/quota-router-core/src/balance.rs`. All 5 callers MUST have migrated
   by this point (verified via grep `Balance::new` = 0 matches).
   `cargo build` MUST succeed with zero callers using `Balance`.

2. **`Vault.balance_dqa_micros: i64`** — REMOVED from `Vault` struct.
   Column kept on `vaults` table (Cycle 3 drops).

3. **`octo_w_balances` init** — GATED behind `legacy_octo_w = "off"`
   Cargo feature flag (default OFF). When OFF: init code path skipped;
   reads fail-fast with clear error message pointing at the migration
   target.

**Cycle 3 — Column drop (reserved right to refuse):**

1. **`vaults.balance` column** — Migration drops the column.
   `cargo test --workspace` migration test passes.

2. **`octo_w_balances` table** — Init REMOVED. Table dropped via migration.

3. **RESERVED RIGHT TO REFUSE:** RFC-0904's lifecycle status (per its
   current §0 Status header — see `rfcs/accepted/economics/0904-real-time-cost-tracking.md`)
   determines external-adoption risk — downstream consumers may have
   pinned the table. Mission C Cycle-3 table drop RESERVES the right to
   refuse if external adoption is detected (verification via 3rd-party
   registry at landing time). Refusal documented in mission file via
   `## Refusal log` blockquote + RFC-0960 §5.3 update.

### Migration window per cycle

Each cycle spans 1 release. Total 3-cycle window: 3 releases from Mission A
landing. RFC-0960 §5.1 table is canonical for what each cycle touches.

## Test Vectors

- TV-LD1: Cycle 1 — `Balance::new` produces `#[deprecated]` warning
  (clippy `deprecated_semver` lint); callers with `#[allow(deprecated)]`
  build cleanly
- TV-LD2: Cycle 1 — `Vault.balance_dqa_micros` field access produces
  `#[deprecated]` warning; column still readable from `vaults` table
- TV-LD3: Cycle 2 — `--features full` build (default features) passes
  with `Balance` removed; no callers compile
- TV-LD4: Cycle 2 — `--features legacy_octo_w` build passes with
  `octo_w_balances` init retained
- TV-LD5: Cycle 3 — migration drops `vaults.balance` column; existing
  projections from `VaultBalanceProjection` continue working
- TV-LD6: Cycle 3 — `octo_w_balances` table dropped; `--features legacy_octo_w`
  build fails cleanly with migration-required error
- TV-LD7: external adoption check — grep across `crates/`, `agents/`,
  `use-cases/`, `docs/` for `octo_w_balances` references at Cycle-3
  landing time (refusal trigger if non-zero matches found outside RFC-0960

## Layer direction (per [[cipherocto-design-principles]])

- `octo-vault` (Layer B) — `Vault.balance_dqa_micros` field removal
- `quota-router-core` (Layer B substrate) — `Balance` struct removal,
  `octo_w_balances` init/table removal
- All removals = Layer B-breaking, JUSTIFIED by source-of-truth migration
  (event-sourced vault balance projection replaces stored balance)
- No cross-layer inversion

**Semver impact:**

- `octo-vault` (Layer B) = **semver-MAJOR** for `balance_dqa_micros`
  field removal at Cycle 3 (column drop).
- `quota-router-core` (Layer B) = **semver-MAJOR** for `Balance` struct
  removal at Cycle 2.
- 3-cycle migration window per RFC-0960 §5.1 = 3 releases.

## Validation

```bash
# Cycle 1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings  # deprecation warnings allowed

# Cycle 2
cargo build --workspace --features full
cargo build --workspace --features full,legacy_octo_w

# Cycle 3 (after external adoption check)
cargo test --workspace --lib
cargo test --workspace  # migration test passes
```

### Cycle 1 grep gate (AC)

```bash
# Verify stranded-field evidence before Cycle 1 deprecation:
grep -rn 'balance_dqa_micros' crates/ --include='*.rs'  # expect 0 production write sites
```

### Cycle 3 fail-fast error message (AC, TV-LD6)

When `--features legacy_octo_w` is enabled after Cycle 3 table drop:

```
error: octo_w_balances table dropped in Cycle 3; use VaultBalanceProjection per RFC-0960 §2.1
```

## Backward compat

- **Cycle 1**: Source-compatible; legacy callers compile with deprecation warnings
- **Cycle 2**: Source-breaking for `Balance` callers; `--features legacy_octo_w` retains `octo_w_balances` reads
- **Cycle 3**: Source-breaking for `octo_w_balances` callers (now no feature flag); DB schema breaking (column + table dropped)

The 3-cycle window gives downstream consumers 3 releases to migrate.

## Cross-references

- RFC-0960 §5.1 — 3-cycle deprecation table (canonical)
- RFC-0960 §5.2 — `Vault.balance_dqa_micros` stranded-field grep evidence
- RFC-0960 §5.3 — `octo_w_balances` external-adoption-risk mitigation
- RFC-0960 §6 Mission C — canonical scope
- RFC-0904 — `octo_w_balances` external consumer (Cycle-3 refusal trigger if external adoption detected)
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — provides replacement `VaultBalanceProjection`
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — provides replacement `EventLogProducer` wiring for vault-balance cache invalidation

## Refusal log

(empty at creation; Cycle-3 landing fills this blockquote if external
adoption detected, per §5.3 reserved right)

## Claimant

@unassigned

## Pull Request

#
