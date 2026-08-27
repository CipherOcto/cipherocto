# Research: Per-Asset DQA Scale + Asset-Generic PaymentCaveat

**Date:** 2026-08-26
**Status:** Draft Research — pending multi-round review per BLUEPRINT.md §Adversarial Review Process
**Version:** v0.1 (initial)
**Author:** research-audit pass against current substrate + accepted RFCs

**Builds on:**

- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` (R1-R7 review trail)
- `docs/research/2026-07-22-grand-design-vaults-capabilities-reservations.md`
- `rfcs/accepted/economics/0105-v34-private-asset-namespace.md` (latest RFC-0105)
- `rfcs/accepted/economics/0965-capability-extension-format.md` (latest RFC-0965)
- `rfcs/accepted/economics/0960-v35-vault-path-taxonomy.md` (latest RFC-0960)
- `rfcs/accepted/economics/0959-ask-settlement-chain.md` (RFC-0959 v2.0/v2.7)
- `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` (numeric parent of RFC-0105)

---

## 0. Executive Summary

CipherOcto's payment-side substrate (`PaymentCaveat`, `AssetId`, `Dqa`) carries the **shape** for asset-generality but has **drifted** to a single global scale (DQA `scale = 0` = micro-OCTO_W for OCTO-W) and OCTO-only naming in three independent places:

1. `AssetId(pub [u8; 32])` is opaque — does not carry the asset's decimal scale.
2. `PaymentCaveat` (`crates/octo-cap-macaroon/src/caveat/payment.rs`) has no `asset_id` field; budget is implicitly tied to a global `scale = 0` micro-OCTO_W frame.
3. Three naming sites still say "micro-OCTO" (`MICRO_PER_OCTOW`, `amount_dqa_micros`, `RELAY_RATE_B_MICRO_OCTO_PER_GB`) and `permission` enum is by class (`NativeTokenTransfer | Erc20TokenTransfer`) not by asset.

This drift was an emergent outcome, not a deliberate design decision. The substrate type `Dqa { value: i64, scale: u8 }` (16-byte BE wire form) and the arithmetic primitives `dqa_sub` / `dqa_cmp` (in `octo_determin`) **already support** per-asset scales. The drift is **site-level** (every production call hardcodes `scale = 0`) and **naming-level** (carrying `micro-OCTO` semantics past the 2026-08-17 `MicroOctoW` retirement).

This research proposes three coordinated changes to fix the drift without re-architecting the substrate:

- **D1** Substrate: add scale-binding to `AssetId` via a side-table (consistent with frozen-substrate principle) OR an explicit accessor.
- **D2** Caveat: `PaymentCaveat` adds `pub asset_id: AssetId`; verify pipeline reads scale from it; invariant that `caveat.budget.scale == asset_id.scale()` enforced at both `verify` and `Builder::attenuate`.
- **D3** Naming: rename `MICRO_PER_OCTOW` → asset-generic constant; rename `amount_dqa_micros: i64` → `amount: Dqa` everywhere; rename `RELAY_RATE_B_MICRO_OCTO_PER_GB` → `RELAY_RATE_PER_GB` with `Dqa` units; rename `PaymentCaveat` discriminator `"paid-query/v1"` → `"payment/v1"` to surface asset-generality.

Decisions are **asset-generic, not OCTO-specific**. A bridged mirror of Bitcoin (e.g., a corporate-chain `AssetId` for a 1:1 BTC mirror at 8 decimals) flows through the same substrate as OCTO-W — only the `asset_id` carries the decimal scale and unit denomination.

All six design axes (D1-D6) have final decisions. Per-asset variation uses **typed-discriminator + Raw escape hatch** pattern from `cipherocto-design-principles.md` §Extension over enumeration — no central enums added.

**R1 resolutions (initial):**

- AssetRegistry side-table on `(asset_id)` → `scale: u8, denomination: &str, kind: AssetKind` (Layer B additive, semver-minor on RFC-0105).
- `PaymentCaveat.asset_id` is a hard requirement; verifier rejects unknown asset_id (fail-closed).
- Discriminator `"payment/v1"` reserves a slot in `0x1A..=0xCF` (RFC-0965 §3); old `"paid-query/v1"` is deprecated but valid for one substrate cycle for cross-crate compatibility.
- RFC-0105 v3.5 amendment carries the per-asset scale table for the 9 sovereign role tokens (assumed scale = 6 to align with the DQA(6) historical convention used in `crates/quota-router-storage/src/ask.rs:MICRO_PER_OCTOW`); private assets carry their own scale via the corporate-chain registry.
- RFC-0965 v2.1 amendment adds the `asset_id` field + invariant; companion mission `payment-caveat-asset-binding` follows.
- RFC-0960 v3.6 amendment changes `BurnEventRef.amount_dqa_micros: i64` → `amount: Dqa`; companion mission `burn-event-dqa-migration` follows.
- RFC-0959 v2.8 amendment migrates settlement cost fields from i64 to `Dqa` and resolves scale via the ask's `cost_vault_id`.

**No code changes proposed in this research** — only the RFC amendment chain. Substrate owner is `octo-determin` (Layer A frozen); the side-table sits in `octo-vault` (Layer B additive).

---

## 1. Problem Statement

### 1.1 Three named sites still say "micro-OCTO" (audit §D)

| Site | Code | Issue |
|---|---|---|
| `crates/quota-router-storage/src/ask.rs:41` | `pub const MICRO_PER_OCTOW: Dqa = Dqa { value: 1_000_000, scale: 0 }` | Constant name bakes "1 OCTO = 1_000_000 micro" framing; per-asset generalization impossible without renaming. |
| `crates/octo-policy/src/policy_kinds.rs:263` + `crates/octo-policy/src/workflow_kind.rs:313,461,477` + `rfcs/accepted/process/0206-v30-value-transfer-surface.md:65-121` | `pub amount_dqa_micros: i64` | Field name encodes the scale; i64 carrier assumes scale = 0 universally. |
| `crates/octo-network/src/porelay/economics.rs:131` | `RELAY_RATE_B_MICRO_OCTO_PER_GB: u64 = 100_000` | Per-OCTO-B-asset rate named for micro-OCTO. |
| `crates/octo-network/src/drs/scoring.rs:8` | "All weights are micro-units (0-1_000_000). Total must equal 1_000_000." | Same framing in scoring weights. |

**User concern surfaced 2026-08-26**: naming must be asset-generic. A corporate-chain mirror of BTC (asset_id derived from `BLAKE3("cipherocto/asset/v1/" || "PRIVATE-<chain_id_hex>-BTC-MIRROR")`) at 8 decimals has no relation to OCTO; "MICRO_PER_OCTOW" makes no sense as a unit for it.

### 1.2 Hardcoded `scale = 0` everywhere in production (audit §C)

- `crates/octo-cap-macaroon/src/caveat/payment.rs:25` "scale = 0 enforced at the substrate boundary"
- `crates/quota-router-storage/src/ask.rs:43,109` `Dqa { value, scale: 0 }`
- `crates/octo-paid-query/src/lib.rs:104,343,493,515,517,717,730,735,736` all `scale: 0`
- `crates/octo-paid-query/src/ledger.rs:229` test helper hardcodes
- `crates/quota-router-storage/src/slash_store.rs:43` "Always stored at `scale = 0`"

The substrate type CAN carry per-asset scale (`Dqa { value: i64, scale: u8 }` per `crates/octo-cap-macaroon/src/dqa_serde.rs:6`, plus `dqa_sub` / `dqa_cmp` primitives in `octo_determin`). Capacity unused. Production treats scale as a single global constant.

### 1.3 `PaymentCaveat` not asset-bound (audit §B)

`crates/octo-cap-macaroon/src/caveat/payment.rs` fields:

| field | type | purpose |
|---|---|---|
| `caveat_name` | `String` | discriminator (`"paid-query/v1"`) |
| `budget` | `Dqa` | spending ceiling |
| `model` | `String` | LLM model allowlist (e.g. `"gpt-4o"`); empty = wildcard |
| `expires_at_unix_ms` | `u64` | expiry |

No `asset_id`. The "Payment" caveat actually binds the budget to an LLM model, not an asset. Naming lies about semantics. A capability for "spend up to 100 units of USDC against gpt-4o" cannot be expressed — the budget is implicitly OCTO-W.

### 1.4 `AssetId` carries no scale metadata (audit §A)

`pub struct AssetId(pub [u8; 32])` opaque (`crates/octo-vault/src/lib.rs:136`). Derivation `BLAKE3("cipherocto/asset/v1/" || role_token)` produces bare 32-byte digest. RFC-0105 v3.4 enumerates 9 sovereign role tokens (OCTO-A/B/D/M/N/O/S/H/W) but never assigns a scale. To realize per-asset scale, either:

- Widen the struct: `AssetId { bytes: [u8;32], scale: u8 }` (semver-major on the frozen substrate, rejected below).
- Side-table: `AssetRegistry::scale(asset_id) -> Result<u8>` (Layer B additive, recommended).

### 1.5 `PermissionKind` enum by class, not by asset (audit §F)

`crates/octo-cap-macaroon/src/caveat/mod.rs:228-229`:

```rust
pub enum PermissionKind {
    NativeTokenTransfer,
    Erc20TokenTransfer,
    ContractCall,
    Reservation,
    VaultMutation,
}
```

Two assets in the same class collapse. `NativeTokenTransfer` on OCTO-W vs `NativeTokenTransfer` on a BTC mirror are treated identically when they shouldn't be. The substrate is class-only; the asset binding is implicit (presumably the capability's bound vault).

---

## 2. Research Scope

**In scope:**

- `AssetRegistry` side-table pattern (owner: `octo-vault`, Layer B additive).
- `PaymentCaveat.asset_id` field + scale-binding invariant.
- Naming cleanup: rename `MICRO_PER_OCTOW`, `amount_dqa_micros`, `RELAY_RATE_B_MICRO_OCTO_PER_GB`, payment caveat discriminator.
- `BurnEventRef.amount: Dqa` migration.
- Settlement cost field migration to `Dqa` with scale resolution via `cost_vault_id`.
- RFC amendment chain: 0105 v3.5 + 0965 v2.1 + 0960 v3.6 + 0959 v2.8.

**Out of scope:**

- Widening `AssetId(pub [u8;32])` to a struct (semver-major on frozen substrate, rejected per RFC-0105 §Layer A frozen-substrate principle).
- Replacing `Dqa` with a third-party decimal library.
- Cross-chain bridge contract (separate RFC).
- Multi-token wallet UI / human-facing UX.
- Federation across multiple cipherocto meshes.

**Not addressing (deferred per `deferred-vs-unspecified` rule):**

- Per-policy-kind asset-class refinement beyond `NativeTokenTransfer | Erc20TokenTransfer` split (e.g., NFT, staking derivatives).
- Per-region regulatory scale caps (separate RFC).

---

## 3. Design Decisions

### D1. AssetRegistry side-table (RECOMMENDED)

```
AssetRegistry = (asset_id: AssetId) -> { scale: u8, denomination: String, kind: AssetKind }
```

- Owner: `octo-vault` (Layer B additive).
- Implementation: `HashMap<AssetId, AssetMetadata>` populated at startup from RFC-0105 v3.5 §2 sovereign namespace table + corporate-chain registry entries.
- Lookup: `AssetRegistry::metadata(asset_id) -> Result<AssetMetadata, AssetError::Unknown>` — fail-closed on unknown asset_id.
- `AssetKind`: `SovereignRoleToken | PrivateCorporateAsset | BridgedExternalAsset | WrappedCrossChainAsset` (RFC-0960 v3.6 §3 new — additive, semver-minor).

**Rationale (rejected alternative)**: widening `AssetId(pub [u8;32])` → `AssetId { bytes, scale }` is semver-major on the frozen Layer A substrate. Per RFC-0105 §Layer A stability rule, the substrate shape is RFC-frozen; widening forces a migration of every consumer (`octo-cap-macaroon`, `octo-policy`, `quota-router-storage`, etc.). The side-table preserves the frozen substrate and adds capability at Layer B.

### D2. PaymentCaveat asset_id field

```
pub struct PaymentCaveat {
    pub caveat_name: String,      // "payment/v1" (deprecated: "paid-query/v1")
    pub asset_id: AssetId,        // NEW; binds budget to a specific asset
    pub budget: Dqa,              // invariant: budget.scale == asset_registry.scale(asset_id)
    pub model: String,            // empty = wildcard
    pub expires_at_unix_ms: u64,
}
```

**Attenuation invariant additions:**

- `attenuate(new_budget, new_expires) -> Result<Self, AttenuationError>`:
  - `AttenuationError::AssetMismatch { current, proposed }` if `new_budget.scale != self.budget.scale` (the proposed scale differs from the parent caveat's scale, which equals `asset_id`'s scale).
  - `AttenuationError::BudgetWidened` unchanged.
  - `AttenuationError::ExpiryWidened` unchanged.
- `verify(query_cost, query_model, now)`:
  - New gate 0: `query_cost.scale == budget.scale` (else `Reject { reason: ScaleMismatch { caveat_scale, query_cost_scale } }`).

### D3. Naming cleanup

| Old | New | Rationale |
|---|---|---|
| `MICRO_PER_OCTOW` (`crates/quota-router-storage/src/ask.rs:41`) | `UNITS_PER_OCTO_W` (or `SCALE_OF_OCTO_W` if scale-only) | "micro" deprecated; per-asset, not OCTO-specific. New name reads as "1 OCTO_W = UNITS_PER_OCTO_W base units" regardless of asset. |
| `amount_dqa_micros: i64` (`crates/octo-policy/src/policy_kinds.rs:263` etc.) | `amount: Dqa` | Field name encodes scale; rename to type-only. |
| `RELAY_RATE_B_MICRO_OCTO_PER_GB: u64 = 100_000` (`crates/octo-network/src/porelay/economics.rs:131`) | `RELAY_BANDWIDTH_RATE_PER_GB: Dqa` | Per-OCTO-B-asset rate named for micro-OCTO; new name is asset-generic with `Dqa` carrier (scale = 6 default). |
| `MicroOctoW` (alias retired 2026-08-17) | `Dqa` | Already done; verify removal. |
| `PaymentCaveat` caveat discriminator `"paid-query/v1"` (`crates/octo-cap-macaroon/src/caveat/payment.rs:40`) | `"payment/v1"` (deprecate `"paid-query/v1"` for one cycle) | Surface asset-generality; the caveat is about payment, not paid-query-specific. |

### D4. BurnEventRef migration (RFC-0960 v3.6)

```
// OLD:
pub struct BurnEventRef {
    pub burn_id: [u8; 16],
    pub chain_id: [u8; 32],
    pub vault_id: [u8; 32],
    pub amount_dqa_micros: i64,  // bounded by i64::MAX
    pub burn_policy_hash: [u8; 32],
    pub finalized_at_unix: i64,
}

// NEW:
pub struct BurnEventRef {
    pub burn_id: [u8; 16],
    pub chain_id: [u8; 32],
    pub vault_id: [u8; 32],
    pub amount: Dqa,             // per-asset scale; vault_id resolves the asset
    pub burn_policy_hash: [u8; 32],
    pub finalized_at_unix: i64,
}
```

### D5. Settlement cost migration (RFC-0959 v2.8)

```
// OLD:
SettlementEvent { cost: MicroOCTO_W }

// NEW:
SettlementEvent { cost: Dqa }
```

Scale resolution: `cost_vault_id` is a required field on the ask envelope; settlement engine looks up the vault, resolves the asset_id via `AssetRegistry`, and asserts `cost.scale == asset_registry.scale(asset_id)` at settlement time.

### D6. PermissionKind asset binding

Two acceptable shapes (RFC-0965 v2.1 selects one):

- **D6.a (RECOMMENDED)**: keep `PermissionKind` class-only; require co-bound `Caveat::Vault(asset_id)` whenever the permission is `NativeTokenTransfer | Erc20TokenTransfer`. Verifier checks co-bound caveat exists at attenuation time.
- **D6.b**: extend `PermissionKind` to `PermissionKind::NativeTokenTransfer(AssetId) | Erc20TokenTransfer(AssetId) | ...` (additive, semver-minor). More explicit; widens the enum variants.

D6.a is preferred — preserves the enum's class-only semantics and uses the existing `Caveat::Vault` binding mechanism for asset identity (RFC-0965 §3).

---

## 4. Substrate Capacity Verification

The substrate already supports per-asset scale:

- **Type**: `Dqa { value: i64, scale: u8 }` — `crates/octo-cap-macaroon/src/dqa_serde.rs:6` defines the 16-byte BE wire form (`value: i64 (8 bytes) + scale: u8 (1 byte) + _reserved: [u8; 7]`).
- **Arithmetic**: `dqa_sub`, `dqa_cmp`, `dqa_add`, `dqa_mul` from `octo_determin` (used by `PaymentCaveat::verify`). Multi-scale-safe (operands may differ in scale; result is the highest operand scale).
- **Construction**: `Dqa::new(value: i64, scale: u8) -> Result<Dqa, DqaError::InvalidScale>` — substrate boundary accepts arbitrary scale.
- **Constraint**: `MAX_SCALE` (per `dqa_serde.rs:33` "Returns `DqaError::InvalidScale` if scale exceeds `MAX_SCALE`").

The drift is purely **site-level** (every production call constructs `Dqa::new(n, 0)`) and **naming-level** (carrying `micro-OCTO` past retirement). No substrate widening needed.

---

## 5. RFC Amendment Chain

| RFC | Amendment | Status | Companion Mission |
|---|---|---|---|
| RFC-0105 v3.5 | Add §Per-Asset Scale Table (sovereign role tokens + private asset class); §AssetRegistry side-table definition. | Draft (this research) | `payment-caveat-asset-binding` (deferred — Mission lifecycle only after Accepted) |
| RFC-0965 v2.1 | `PaymentCaveat.asset_id` field; scale-binding invariant in `attenuate` + `verify`; discriminator `"payment/v1"`; D6.a PermissionKind co-bound caveat rule. | Draft (this research) | Same |
| RFC-0960 v3.6 | `BurnEventRef.amount: Dqa` (replace `amount_dqa_micros: i64`); `AssetKind` enum (additive, semver-minor). | Draft (this research) | `burn-event-dqa-migration` (deferred) |
| RFC-0959 v2.8 | `SettlementEvent.cost: Dqa`; scale resolution via `cost_vault_id`; settlement engine rejects scale mismatch. | Draft (this research) | `settlement-cost-dqa-migration` (deferred) |

**No missions filed in this research.** Missions are claimable work units per BLUEPRINT.md §Mission — they REQUIRE an Accepted RFC. Missions land after the RFC amendment chain promotes to Accepted.

---

## 6. Cross-CRF (Cross-Reference) Impact

### 6.1 RFCs that cite `MICRO_PER_OCTOW` or `MicroOctoW`

| Site | Reference type |
|---|---|
| `crates/quota-router-storage/src/ask.rs:14` | comment cross-ref to RFC-0862 v2.0.3 |
| `crates/quota-router-storage/src/ask.rs:41` | constant definition |
| `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md:912,919,929,2113` | substrate spec (uses `MicroOctoW` extensively) |
| `rfcs/accepted/process/0206-v30-value-transfer-surface.md:65-121` | `amount_dqa_micros` field across 6 structs |
| `rfcs/accepted/economics/0959-ask-settlement-chain.md` | `MicroOCTO_W` in `SettlementEvent.cost` |
| `rfcs/accepted/economics/0965-capability-extension-format.md` | `MicroOctoW` alias cross-ref in v2.0-CanonicalAlias row |

### 6.2 RFCs that reference `PaymentCaveat`

| Site | Reference type |
|---|---|
| `rfcs/accepted/economics/0965-capability-extension-format.md` | Primary RFC for `PaymentCaveat` definition. v2.1 amendment here. |
| `rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md` | Capacity / Vault state machine reference. |
| `rfcs/accepted/process/0871e-f7.md` (if exists) | Paid-query mission wire form. |

### 6.3 RFCs that reference `AssetId` / `asset_id_for`

| Site | Reference type |
|---|---|
| `rfcs/accepted/economics/0105-v34-private-asset-namespace.md` | Primary RFC for sovereign/private boundary. v3.5 amendment here. |
| `rfcs/accepted/economics/0960-v35-vault-path-taxonomy.md` | Vault composite key uses `asset_id`. v3.6 amendment here. |
| `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` | Numeric parent for `asset_id_for`. |
| `rfcs/accepted/economics/0959-a1-market-delivery.md` | Market delivery envelope carries `cost_vault_id`. |
| `rfcs/accepted/process/0010-...` (chain ID RFC) | Chain-bound assets derive via `chain_id` segment in role-token string. |

---

## 7. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| AssetRegistry side-table split-brain across nodes | HIGH | RFC-0105 v3.5 §Per-Asset Scale Table ships as canonical anchor; corporate-chain registries sync via governance_snapshot. |
| Asset_id mismatch at settlement | HIGH | RFC-0959 v2.8 §Settlement scale resolution rejects at settlement time (fail-closed); audit log records the mismatch. |
| Backward compatibility for old PaymentCaveat (no asset_id) | MED | RFC-0965 v2.1 §Discriminator Deprecation Cycle: `"paid-query/v1"` accepted for one substrate release cycle; substrate reject path returns `ScaleUnknown` (verifier treats it as OCTO-W at scale 0 for legacy compatibility, with deprecation warning). |
| Renaming `MICRO_PER_OCTOW` breaks downstream crates | MED | Companion mission exposes new constant; old constant kept as `pub const LEGACY_MICRO_PER_OCTOW: Dqa = ...` with `#[deprecated]` for one cycle. |
| PermissionKind asset co-bound verification overhead | LOW | D6.a verifier check is O(1) — presence of `Caveat::Vault(asset_id)` in caveat list. |

---

## 8. Alternatives Considered

| Alternative | Pros | Cons |
|---|---|---|
| Widen `AssetId(pub [u8;32])` → `AssetId { bytes, scale }` | Self-contained; no side-table | semver-major on frozen Layer A; touches every consumer |
| Per-role-token hardcoded scale in `asset_id_for` | One place to change | Mixes substrate shape with policy metadata; rejected on separation-of-concerns grounds |
| `Caveat::Budget { asset: AssetId, amount: Dqa }` (new variant instead of `PaymentCaveat`) | Cleaner separation | Breaks macaroon attenuation layering; `PaymentCaveat` is single-element composition by design |
| Replace `Dqa` with `rust_decimal::Decimal` | Battle-tested library | Loses RFC-0105 Dfp determinism guarantees; rejected per RFC-0104 determinism principle |

---

## 9. Recommended Path

1. **Promote this research** to multi-round adversarial review per BLUEPRINT.md §Adversarial Review Process (R1 → R_n → DRY).
2. **File RFC amendment chain** as drafts in `rfcs/draft/economics/`:
   - `0105-v35-payment-caveat-asset-generality.md`
   - `0965-v21-payment-caveat-asset-binding.md`
   - `0960-v36-burn-event-dqa-migration.md`
   - `0959-v28-settlement-cost-dqa-migration.md`
3. **Promote RFC amendments** through Draft → Accepted (minimum 7 days per RFC review etiquette + 2 maintainer approvals).
4. **File companion missions** in `missions/open/` AFTER RFC amendments reach Accepted (BLUEPRINT.md §Mission rules).
5. **No code changes** in this research cycle. Substrate owner is `octo-determin` (frozen Layer A); side-table lives in `octo-vault` (Layer B additive).

---

## 10. Next Steps

- [ ] R1 adversarial review (3 parallel lenses: substrate, cross-RFC, naming).
- [ ] File RFC amendment chain drafts (RFC-0105 v3.5 + RFC-0965 v2.1 + RFC-0960 v3.6 + RFC-0959 v2.8).
- [ ] Cross-reference validation: Guard 2 cite validator (no PHANTOM / INVALID / STALE RFC numbers).
- [ ] Draft use case `docs/use-cases/asset-generic-payment-caveat.md` (problem/motivation layer per BLUEPRINT.md).
- [ ] After RFC acceptance: file companion missions.
- [ ] Schedule 90-day re-cert per RFC-0008 §Lifecycle Requirements (ACCEPTED RISK deadline).

---

## 11. Out-of-Scope (Documented but not Proposed)

- Federation across multiple cipherocto meshes.
- Cross-chain bridge contract primitives (no-bridge / atomic-swap / wrapped-representation — already in RFC-0960 v3.5).
- A/B variant empirical baseline establishment for mainnet audit policy.
- Per-policy-kind asset-class refinement beyond `NativeTokenTransfer | Erc20TokenTransfer` split.

---

**End of research draft v0.1.**

Pending R1 adversarial review.