# Mission: 0900-d — RFC-0900 amendment: chain-aware slash ledger substrate + DQA(12) columns

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Filed per audit verdict 2026-08-17
(storage restructure hard-recommendation: S6d RFC-0900 amendment +
10 TV per pending task #443). Closes parallel-model risk surfaced by
audit: slash_ledger substrate (BIGINT `stake_micro_octo_w` + no
`chain_id` column) diverges from §20.3 Model B chain-aware PK
vault substrate.

## RFC

- Primary: RFC-0900 (slash ledger substrate — currently §Slashing
  Model does not specify storage column types or chain dimension;
  v2.0 row will introduce §Slash Ledger Substrate subsection)
- Co-RFC: RFC-0010 v1.4 (typed `ChainId` + `ChainNamespace` per
  R15-F9 — provides the wire form)
- Co-RFC: RFC-0105 (Dqa substrate — canonical type for amount-bearing
  columns per review §8.1.2)

## Dependency edges

| From                                                                        | To                                         | Why             | Layer direction |
| --------------------------------------------------------------------------- | ------------------------------------------ | --------------- | --------------- |
| `crates/quota-router-storage/migrations/v015__slash_ledger_chain.sql` (NEW) | Stoolap native DQA(12) + BLOB(32) chain_id | Schema          | lib → schema    |
| `crates/quota-router-storage/src/slash_store.rs` (modify)                   | `SlashLedgerRow.chain_id: [u8; 32]`        | Chain-aware row | lib → lib       |
| `crates/quota-router-core/src/marketplace/slashing.rs` (modify)             | `ProviderStake.chain_id: [u8; 32]`         | Chain-aware PK  | lib → lib       |
| `crates/quota-router-core/src/marketplace/slashing.rs` (modify)             | HashMap key → `(ChainId, String)`          | Chain-aware map | lib → lib       |
| `rfcs/accepted/economics/0900-ai-quota-marketplace.md` (modify)             | §Slash Ledger Substrate + v2.0 row         | Spec coherence  | RFC → RFC       |

No new cyclic edges. Migration co-deployed with in-memory field
migration as single commit (cross-layer coupling requires atomic
landing).

## Problem

Audit (2026-08-17) found slash_ledger substrate diverges from §20.3
Model B in two distinct ways:

1. **Column type drift** (HIGH) — `v012__create_slash_ledger.sql`
   declares `stake_micro_octo_w BIGINT NOT NULL` +
   `initial_stake_micro_octo_w BIGINT NOT NULL`. Per review §8.3.3
   - §8.4.1, amount-bearing columns at scale=0 should use
     `DQA(12)`. The in-memory field type IS `Dqa` (post-S4 codemod
     LANDED 2026-08-17), but the column stays `BIGINT`. Stoolap fork
     supports native `DQA(12)` per §8.1.3 — migration owed.

2. **Missing chain_id column** (CRITICAL) — `v012` PK = `(row_id,
provider_id UNIQUE)`. No `chain_id` column. Vault substrate
   (`v013__create_vaults.sql`) PK = `(chain_id, owner_did,
asset_id)` per §20.3 Model B. Slash ledger should be parallel —
   the same provider can carry stakes in multiple chains (S3
   chain-aware vault + S6d chain-aware slash ledger form a coherent
   chain-partitioned stake model).

3. **`ProviderStake` no chain_id field** (HIGH) — `pub struct
ProviderStake { pub provider_id: String, ... }` at
   `crates/quota-router-core/src/marketplace/slashing.rs:346`. In-
   memory state mirrors storage schema. If storage moves to
   chain-aware, in-memory must follow.

4. **Open flow cross-coupling** (MED) — `SlashLedgerRow.provider_id:
String` used as `HashMap` key at
   `crates/quota-router-core/src/marketplace/slashing.rs:520`.
   Restructure to `HashMap<(ChainId, String), ProviderStake>` or
   `HashMap<ChainId, HashMap<String, ProviderStake>>`. Mirror the
   vault substrate's two-tier mapping pattern.

5. **RFC text silent on substrate** (MED) — RFC-0900 §Slashing
   Model specifies penalty percentages (10%, 1.5× escalation,
   50% permanent ban) but NO column types, NO chain dimension, NO
   substrate choice. LANDED v012 schema was an implementation
   detail without spec backing. v2.0 row + §Slash Ledger Substrate
   subsection close the spec gap.

**Parallel model risk:** same provider_id in two chains produces
silently-overwritten stakes in v012 (PK = `provider_id UNIQUE`).
With vault substrate chain-aware per §20.3 + slash ledger global,
two parallel stake substrates diverge on PK partition. Audit
verdict Risk #2 (CRITICAL) — 4 column types for "amount" includes
slash_ledger `BIGINT`.

## Acceptance Criteria

- AC-1: New migration
  `crates/quota-router-storage/migrations/v015__chain_aware_slash_ledger.sql`
  adds:
  - `ALTER TABLE slash_ledger ADD COLUMN chain_id BLOB(32) NOT NULL DEFAULT '\x00...'` (32 bytes of zero, matches `ChainNamespace::default()` per RFC-0010 v1.4)
  - `stake_micro_octo_w BIGINT NOT NULL` → `stake_micro_octo_w DQA(12) NOT NULL` (Dqa-promotion)
  - `initial_stake_micro_octo_w BIGINT NOT NULL` → `initial_stake_micro_octo_w DQA(12) NOT NULL`
  - `cumulative_loss_pct_micro BIGINT NOT NULL` (kept as BIGINT — not amount-bearing, it's a percentage)
  - `last_updated_unix BIGINT NOT NULL` (kept as BIGINT — timestamp)
  - `CREATE UNIQUE INDEX slash_ledger_chain_provider_idx ON slash_ledger (chain_id, provider_id)` — replaces `provider_id UNIQUE` constraint
  - `DROP INDEX slash_ledger_provider_id_unique` if it exists
  - Backfill: existing rows get `chain_id = '\x00...'` (the R15-F9 additive default namespace)
- AC-2: `SlashLedgerRow` (at `crates/quota-router-storage/src/slash_store.rs:19`) gains:
  ```rust
  pub chain_id: [u8; 32],
  ```
  Reorders PK-equivalent fields: chain_id first. Reads/writes
  adopt native DQA(12) codec (`r.get::<Dqa>(1)` style — verify
  stoolap Dqa driver support; if missing, fall back to
  `dqa_to_i64` / `i64_to_dqa` bridge at scale=0 with documented
  invariant).
- AC-3: `ProviderStake` (at `crates/quota-router-core/src/marketplace/slashing.rs:346`) gains `pub chain_id: [u8; 32]` field. Matches Row.site shape.
- AC-4: `SlashOutcome` (at `crates/quota-router-core/src/marketplace/slashing.rs:370`) gains `pub chain_id: [u8; 32]` field.
- AC-5: In-memory `HashMap<String, ProviderStake>` →
  `HashMap<([u8; 32], String), ProviderStake>` (tuple-keyed map;
  mirror vault substrate's `vault_id_unchecked` two-tier pattern).
  All call sites: `register`, `slash`, `slash_with_pct`, `withdraw_stake`,
  `stake`, `is_banned`, `load_all` flow at line 520 area re-key.
- AC-6: `append_outcome` (SlashStore trait at
  `crates/quota-router-storage/src/slash_store.rs:66`) gains
  `chain_id: [u8; 32]` parameter. NO-OP default impl keeps
  signature widening for audit-table extension.
- AC-7: RFC-0900 §Slash Ledger Substrate subsection added:
  - PK = `(chain_id, provider_id)` per §20.3 Model B parallel to
    vault substrate
  - `stake_micro_octo_w` + `initial_stake_micro_octo_w` columns at
    `DQA(12)` scale=0 per review §8.4.1
  - `chain_id` BLOB(32) typed `ChainId` per RFC-0010 v1.4
  - `cumulative_loss_pct_micro` BIGINT (percentage, not amount-
    bearing)
  - Provider may carry one row per chain (cross-chain stake
    partitioning)
  - §Version History v2.0 row documenting: chain-aware substrate,
    DQA(12) promotion, v015 migration, SlashLedgerRow.chain_id
    field, ProviderStake.chain_id field, HashMap restructure
- AC-8: 10 TV in `crates/quota-router-storage/tests/tv_0900_d_chain_slash.rs`:
  - TV-0900-D-01: byte-exact `Dqa::new(900_000, 0)` → row → Dqa round-trip via new DQA(12) column
  - TV-0900-D-02: cross-chain reject — same `provider_id` in two chains creates two distinct rows; verify `(chain_id_1, provider_id)` and `(chain_id_2, provider_id)` both readable
  - TV-0900-D-03: PK promotion — `(chain_id, provider_id)` UNIQUE INDEX enforces single-row-per-chain-per-provider
  - TV-0900-D-04: scale=0 invariant — `Dqa::new(900, 5)` rejected by schema (non-zero scale column write fails or strips scale)
  - TV-0900-D-05: HashMap tuple-key lookup `(chain_id, provider_id)` returns correct `ProviderStake`
  - TV-0900-D-06: append_outcome signature widens to include chain_id parameter (compile-time test)
  - TV-0900-D-07: existing row backfill — pre-v015 row gets `chain_id = default_zeros` after migration apply
  - TV-0900-D-08: cumulative_loss_pct_micro stays BIGINT (not amount-bearing, not DQA)
  - TV-0900-D-09: chain_id discriminator — rows in different chains do NOT collapse on `load_all`
  - TV-0900-D-10: cross-crate `marketplace/slashing.rs` open() flow loads chain-tagged rows correctly into `HashMap<([u8;32], String), ProviderStake>`
- AC-9: Existing TV in `crates/quota-router-storage/src/slash_store.rs` tests (lines 270-322 — `load_all_empty_on_fresh_db`, `upsert_then_load_round_trips_row`, `upsert_overwrites_existing_provider`) updated to include `chain_id` field. Tests migrate to `Dqa` schema codec.
- AC-10: No regressions:
  - `cargo test -p quota-router-storage --lib`
  - `cargo test -p quota-router-core --lib`
  - `cargo test -p marketplace_strong_scenarios` (e2e)
  - `cargo test -p task_market` (e2e)
- AC-11: clippy + fmt:
  - `cargo clippy --workspace --all-targets --features full -- -D warnings`
  - `cargo fmt --all -- --check`

## Cross-reference

- **Parent:** RFC-0900 §Slashing Model (penalties LANDED as
  RFC-0900 v1.1) — this mission closes the substrate-coherence gap
- **Pattern:** `crates/octo-vault/migrations/v013__create_vaults.sql`
  PK = `(chain_id, owner_did, asset_id)` per §20.3 Model B — exact
  shape for new slash_ledger UNIQUE INDEX
- **Pattern:** `crates/octo-vault/src/lib.rs::vault_id_unchecked`
  uses `b"cipherocto/vault/v1/" + chain_id + owner_did + asset_id`
  BLAKE3 prefix per §20.3 — same shape applies to slash ledger
  identity (slash ledger intentionally does NOT derive an ID; the
  composite key IS the identity per §20.3 lattice)
- **Sibling:** `missions/claimed/0862-c1-dqa-vault-bump-amendment.md`
  (LANDED 2026-08-17) — same DQA(12) + chain_id promotion pattern
  for vault transfers; this mission applies it to slash ledger
- **Co-mission (parallel):**
  `missions/open/0862-c9-micro-octow-type-unification.md` (filed
  2026-08-17) — closes audit-verdict Risk #1 CRITICAL (MicroOctoW
  alias); this mission closes Risk #2 CRITICAL (4 column types)
  for the slash_ledger portion
- **Co-mission (parallel):**
  `missions/open/0105-x-s4-deferred-codemod-sites.md` (filed
  2026-08-17) — in-memory field type migration for
  marketplace/slashing.rs etc.; this mission covers migration
  column promotion + chain_id PK
- **Co-mission (parallel):**
  `missions/open/0959-c1-wire-format-amendment.md` (filed
  2026-08-17) — settlement wire format DQA(12); this mission is
  parallel for slash_ledger chain-aware
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S6 row 6 (Stream A.1 — RFC-0900 amendment cover)
- **Audit source:** 2026-08-17 audit verdict, Risk #2 (CRITICAL)

## Critical files

- `crates/quota-router-storage/migrations/v015__chain_aware_slash_ledger.sql`
  (NEW — schema migration)
- `crates/quota-router-storage/migrations.rs` (modify — register
  v015 in `apply_pending`)
- `crates/quota-router-storage/src/slash_store.rs` (modify —
  `SlashLedgerRow.chain_id` + DQA(12) codec + read/write site
  updates + native DQA driver OR i64 bridge)
- `crates/quota-router-core/src/marketplace/slashing.rs` (modify —
  `ProviderStake.chain_id` + `SlashOutcome.chain_id` +
  `HashMap<([u8;32], String), ProviderStake>` + `register`,
  `slash`, `slash_with_pct`, `withdraw_stake`, `stake`, `is_banned`,
  `load_all`, `append_outcome` callers)
- `crates/quota-router-storage/src/lib.rs` (modify — re-export
  `ChainId` if needed)
- `crates/quota-router-storage/tests/tv_0900_d_chain_slash.rs`
  (NEW — TV-0900-D-01..10)
- `crates/quota-router-storage/src/slash_store.rs` (modify —
  existing tests `load_all_empty_on_fresh_db`,
  `upsert_then_load_round_trips_row`,
  `upsert_overwrites_existing_provider` gain chain_id field)
- `rfcs/accepted/economics/0900-ai-quota-marketplace.md`
  (modify — §Version History v2.0 row + new §Slash Ledger
  Substrate subsection)

## Existing patterns reused

- `crates/octo-vault/migrations/v013__create_vaults.sql` — PK =
  `(chain_id, owner_did, asset_id)` + UNIQUE INDEX shape — direct
  template for v015
- `crates/octo-vault/src/vault_id_unchecked` — chain-partitioned
  derivation pattern (not used here; PK is composite without BLAKE3
  per §20.3 lattice note)
- `octo_determin::Dqa` Display + scale=0 invariant — same pattern
  as `crates/quota-router-storage/src/stoolap_spend_ledger.rs:74`
  reference for `MicroOctoW` + scale=0 (x-mission/0862-c9 will
  centralize the alias)
- `crates/quota-router-storage/src/dqa_serde.rs` — DQA wire
  serialization for in-memory type
- `crate::marketplace::slashing::SlashReason` (typed
  discriminator per marketplace-round-1 review) — preserved
  through chain-aware restructure

## Risks

- **Schema migration atomicity** (HIGH): v015 chain_id backfill +
  BIGINT → DQA(12) promotion must land atomically. Splitting
  across multiple migrations risks double-write window. Mitigation:
  single migration script + idempotent run via
  `migrations::run_one` (LANDED 2026-08-11 per
  `mission-0871b-storage-idempotent-alter-hardening-status.md`).
- **Stoolap native DQA driver support** (MED): verify stoolap fork
  exposes `r.get::<Dqa>(1)` + `tx.set::<Dqa>(1, value)`. If
  missing, fall back to `i64` bridge at scale=0 with
  `dqa_to_i64` / `i64_to_dqa` helpers (current pattern). Document
  the fallback path in §Slash Ledger Substrate.
- **In-memory tuple-key ergonomics** (LOW): `HashMap<([u8;32],
String), ProviderStake>` lookup syntax is verbose. Mitigation:
  introduce `type StakeKey = ([u8; 32], String);` alias.
- **Slashing scope ambiguity** (HIGH): does slasher from chain X
  have authority over provider's stake in chain Y? Per §20.3 Model
  B + RFC-0010 chain-aware substrate, slash events within a chain
  ONLY affect that chain's stake. Cross-chain slashing requires
  explicit governance coordination. v2.0 row + §Slash Ledger
  Substrate must document this scope boundary.
- **Forward-compatibility with append_outcome** (MED): audit-table
  extension (e.g., dispute-resolution subsystem). Signature widening
  to include `chain_id` is breaking. Mitigation: extension authors
  already implement `SlashStore` trait; widen signature in single
  commit.
- **Cargo workspace dep graph** (LOW): no new crate deps. Per layer
  model, slash_ledger migration is in-storage; no impact on
  `octo-cap-macaroon` layer B.

## Version history

| Date       | Author     | Change                                                                                                                                                                             |
| ---------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation: S6d RFC-0900 amendment per pending task #443). Co-filed with `0862-c9`, `0105-x`, `0959-c1`. |

## Out of scope

- Cross-chain slashing coordination (governance-level, separate
  RFC owed)
- Stake migration tooling for operators (separate ops mission
  owed; v015 applies forward only — existing rows backfilled to
  default namespace per R15-F9)
- Stoolap fork DQA(12) driver surface (verified in AC scope; if
  missing, fall back to i64 bridge)
- Vault-backed stake substrate redesign (current slash ledger
  remains a separate substrate per RFC-0862 §Future Work F12
  parallel-balance pattern)
- Reputation `Auth.chain_id: u32` semantic audit (separate audit
  pending per audit verdict Risk #7 LOW)
- Slash event cross-chain dispute coordination (governance RFC
  owed)
