# settlement-id-substrate-move — promote `SettlementId` to octo-cap-macaroon Layer A substrate

**Status:** claimed (2026-08-27)
**Substrate:** RFC-0959 (settlement event) + `cipherocto-design-principles` §Canonical home rule (newtypes in Layer A)
**Parent:** R3 review follow-on (substrate-hosting finding — `SettlementId` currently in Layer C, should be in Layer A)
**Depends on:**

- Mission G (`0959-v28-settlement-cost-dqa-migration-substrate.md`) — `SettlementId` decl currently at `crates/quota-router-sm-engine/src/settlement_event.rs`
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `SettlementProducerInput.settlement_id: [u8; 32]` consumer site at `crates/quota-router-sm-engine/src/event_log_producer.rs` (currently raw bytes; promotion will use the newtype)

## Motivation

`pub struct SettlementId(pub [u8; 32]);` is declared at `crates/quota-router-sm-engine/src/settlement_event.rs` (Layer C — quota-router-sm-engine). Per `cipherocto-design-principles` §Canonical home rule, identity-bearing newtypes belong in Layer A (`octo-cap-macaroon`) so all downstream crates (Layer B / C / D / E) can consume a single canonical type. Currently, the `SettlementProducerInput.settlement_id: [u8; 32]` field uses raw bytes (not the newtype), confirming the newtype is not yet universally adopted.

Layer A hosting also unlocks Layer A-frozen semantics: PQC migration, Borsh canonical encoding, and cross-protocol signature verification are easier when the newtype lives alongside `ChainId`, `VaultId`, `AssetId` in Layer A.

## Scope

Move `pub struct SettlementId(pub [u8; 32]);` from `crates/quota-router-sm-engine/src/settlement_event.rs` to `crates/octo-cap-macaroon/src/lib.rs` (or a dedicated `crates/octo-cap-macaroon/src/settlement.rs` module). Update all consumers.

### Sub-steps

1. **Move decl** — relocate the `SettlementId` struct to `crates/octo-cap-macaroon/src/lib.rs` (or new module). Add `#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]` to match the derives on `ChainId` / `VaultId` / `AssetId` (canonical Layer A derives).

2. **Re-export from quota-router-sm-engine** — `crates/quota-router-sm-engine/src/settlement_event.rs` does `pub use octo_cap_macaroon::SettlementId;` (re-export) so existing `use crate::settlement_event::SettlementId;` paths continue to compile. Re-export is removed once all consumers migrate (Cycle 2).

3. **Convert raw-bytes consumers** — `SettlementProducerInput.settlement_id` becomes `SettlementId` (was raw `[u8; 32]`). Test sites that construct via `SettlementId([1u8; 32])` etc. are already newtype-shaped — no change needed. **Audit at landing time:** `grep -n "settlement_id:" crates/quota-router-sm-engine/src/event_log_producer.rs` (1 expected match) + `grep -n "SettlementId(" crates/quota-router-sm-engine/src/settlement_event.rs` (11 construction sites at the 11 lines listed in TV-SI-3; the 12th grep match is the `pub struct SettlementId` declaration at the substrate-hosting site itself, not a construction site — total 12 grep matches, 11 construction sites, 1 decl).

4. **Borsh wire form** — confirm Borsh derives added; serialize-then-deserialize round-trip preserves the inner `[u8; 32]`. Cross-protocol signature verification (RFC-0105 §3.12 — verify_governance_signature canonical home) MUST operate on the same bytes regardless of newtype wrapper.

5. **Migration window** — Cycle 1 (this mission): decl moves; re-export retained for backward compat. Cycle 2 (follow-on): re-export removed; all consumers MUST use `octo_cap_macaroon::SettlementId` directly. Mirror the 3-cycle deprecation pattern per RFC-0960 §5.1.

6. **RFC-0959 update** — bump Version History with v2.9 (or appropriate) noting `SettlementId` moved to Layer A substrate. Cite per CLAUDE.md §RFC Reference Conventions (bare RFC number only).

## Out of Scope

- Adding fields to `SettlementId` (the newtype is intentionally minimal: a 32-byte identifier)
- Changing the inner byte representation (PQC migration tracked separately per Layer A migration path)
- Moving other newtypes (e.g., `AskId`, `CostVaultId`, `EvidenceRef`) — same pattern applies but tracked as separate missions
- Renaming `SettlementId` (the name is canonical)

## Test Vectors

- TV-SI-1: `crates/octo-cap-macaroon/src/lib.rs` (or new module) declares `pub struct SettlementId(pub [u8; 32])` with the canonical Layer A derives
- TV-SI-2: `crates/quota-router-sm-engine/src/settlement_event.rs` re-exports `pub use octo_cap_macaroon::SettlementId;` (Cycle 1)
- TV-SI-3: All 11 construction sites in `crates/quota-router-sm-engine/src/settlement_event.rs` continue to use `SettlementId(...)` construction without modification (the 12th grep match is the `pub struct SettlementId` decl at the substrate-hosting site itself — 11 construction sites + 1 decl = 12 total grep matches)
- TV-SI-4: `SettlementProducerInput.settlement_id: SettlementId` (was `[u8; 32]`); `produce_settlement` constructor call sites pass `SettlementId` directly
- TV-SI-5: Borsh round-trip: `SettlementId([1u8; 32])` → Borsh bytes → `SettlementId([1u8; 32])`
- TV-SI-6: Layer A hosting — `octo-policy` can `use octo_cap_macaroon::SettlementId;` without depending on `quota-router-sm-engine` (Layer A → Layer C inversion risk avoided)
- TV-SI-7: `cargo doc --no-deps --workspace` shows `SettlementId` listed under `octo_cap_macaroon` docs

## Layer direction (per `cipherocto-design-principles`)

- `octo-cap-macaroon` (Layer A, RFC-frozen) — `SettlementId` decl + canonical derives
- `quota-router-sm-engine` (Layer C) — re-export `pub use octo_cap_macaroon::SettlementId;` (Cycle 1); remove re-export at Cycle 2
- All downstream consumers migrate from `crate::settlement_event::SettlementId` to `octo_cap_macaroon::SettlementId` over the migration window
- Layer direction: Layer C → Layer A (allowed per §Canonical home rule: implementations import from substrate)
- Layer A → Layer C inversion is forbidden — verified: `octo_cap_macaroon` does not gain a dep on `quota-router-sm-engine`

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib

# Layer A hosting verified
grep -rn "pub struct SettlementId" crates/
# expect: ONE match in crates/octo-cap-macaroon/src/

grep -rn "pub use octo_cap_macaroon::SettlementId" crates/
# expect: re-export in crates/quota-router-sm-engine/src/settlement_event.rs (Cycle 1)

# No Layer A → Layer C inversion
grep "quota-router-sm-engine" crates/octo-cap-macaroon/Cargo.toml
# expect: zero matches

# All consumers compile
cargo build --workspace --all-targets --features full
```

## Backward compat

- **Cycle 1:** Source-compatible (re-export retained). Substrate move is transparent to consumers.
- **Cycle 2:** Source-breaking for consumers that use `crate::settlement_event::SettlementId` directly (re-export removed). Justified per `cipherocto-design-principles` §Canonical home rule.
- **Wire form (Borsh):** UNCHANGED. The newtype wrapper is transparent to Borsh (single field, fixed size).

**Semver impact:** Layer A semver-MAJOR per §Layer stability (a new public type in Layer A is a new public API surface, even though the wire form is unchanged).

## Risk

- HIGH: Cycle 2 re-export-removal blast radius — when `pub use octo_cap_macaroon::SettlementId` is dropped from `quota-router-sm-engine::settlement_event`, every consumer crate still using `crate::settlement_event::SettlementId` fails to compile; workspace-wide build failure spans every Layer B / C / D / E consumer. Mitigation: pre-landing grep gate verifying `pub use octo_cap_macaroon::SettlementId` is the only `SettlementId` export path in every consumer crate; cross-crate review per `cipherocto-design-principles` §Layer stability; 1-cycle deprecation window between Cycle 1 (decl + re-export) and Cycle 2 (re-export removal).
- LOW: migration is mechanical — re-export + sed-style consumer rewrite
- MEDIUM: cross-crate `cargo` cache invalidation — every consumer crate's fingerprint changes because the type's module path changes. First build after the move is slow (recompiles affected crates cold).
- LOW: signature verification cross-protocol — verify Borsh-derive is identical to the Layer A pattern (see canonical derives on `ChainId` / `VaultId`).

## Cross-references

- `cipherocto-design-principles` §Canonical home rule (newtypes in Layer A)
- `cipherocto-design-principles` §Layer stability table — Layer A semver-major
- RFC-0959 §2.1 — SettlementEvent substrate (Mission G)
- RFC-0959 §2.3 — audit invariant (verify_settlement_against_payment_caveat)
- RFC-0105 §3.12 — verify_governance_signature canonical home (relies on inner bytes)
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `SettlementProducerInput` consumer
- Mission G (`0959-v28-settlement-cost-dqa-migration-substrate.md`) — `SettlementId` decl source
- RFC-0960 §5 (Single Timeline — 3-cycle deprecation pattern described within) — template for Cycle 1/2 migration
- R3 review substrate-hosting finding

## Claimant

@mmacedoeu

## Pull Request

#