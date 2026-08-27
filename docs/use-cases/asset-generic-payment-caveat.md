# Use Case: Asset-Generic Payment Caveat

## Problem

Today, CipherOcto's payment-side substrate is hardcoded to one asset and one decimal scale:

- `PaymentCaveat` (RFC-0965) carries a budget over an LLM model (e.g., "spend up to 100 units on `gpt-4o`") but does not bind the budget to a specific asset. The implicit asset is OCTO-W at scale 0 (one-millionth of an OCTO).
- `AssetId` (RFC-0105 v3.4) is a 32-byte opaque digest with no metadata about the asset's decimal scale.
- Three naming sites still use "micro-OCTO" terminology (`MICRO_PER_OCTOW`, `amount_dqa_micros: i64`, `RELAY_RATE_B_MICRO_OCTO_PER_GB`) even after the 2026-08-17 `MicroOctoW` alias retirement.
- `PermissionKind` (RFC-0965 §3.2) classifies permissions by type (`NativeTokenTransfer | Erc20TokenTransfer | ContractCall | Reservation | VaultMutation`), but does not bind to a specific asset. A "native token transfer" capability is interpreted against the capability's bound vault, not against an explicit asset_id.

This drift is real and breaks three concrete scenarios:

1. **Bridged mirror of Bitcoin on a corporate chain.** A corporate chain issues a 1:1 BTC mirror at 8 decimals as a private asset. The asset_id derives from `BLAKE3("cipherocto/asset/v1/" || "PRIVATE-<chain_id_hex>-BTC-MIRROR")`. A capability that says "spend up to 0.5 BTC-mirror against any model" cannot be expressed — the budget would be implicitly OCTO-W, and the wire form would carry `scale = 0` micro-OCTO counts that mean nothing for an 8-decimal asset.
2. **Multi-asset marketplace.** A marketplace provider offers both OCTO-W and a corporate-chain USDC mirror at 6 decimals. A consumer wants a single capability that authorizes payment in either asset (subject to market price). The substrate cannot express "this budget is denominated in asset X, not asset Y" — there is no asset binding on `PaymentCaveat`.
3. **Cross-asset settlement audit.** When a settlement event records `cost: MicroOCTO_W` (RFC-0959 v2.7), the audit log cannot tell whether the cost was paid in OCTO-W or in a bridged asset. The cost field's scale is implicit (0) and the asset binding is lost.

The drift was not deliberate. The substrate type `Dqa { value: i64, scale: u8 }` already supports per-asset scales; arithmetic primitives `dqa_sub` / `dqa_cmp` are multi-scale-safe. The drift is purely **site-level** (every production call hardcodes `scale = 0`) and **naming-level** (carrying `micro-OCTO` semantics past retirement).

## Stakeholders

- **Primary**: Capability issuer (mints capabilities with budget caps); consumer (presents capabilities to spend against an asset); settlement engine (verifies cost against budget and records receipts).
- **Secondary**: Vault substrate (resolves asset_id → scale + denomination); corporate-chain operators (register private assets and their scales); bridge operators (issue bridged mirror assets).
- **Affected**: Marketplace providers who offer multi-asset settlement; auditors who read settlement receipts and need to know what asset was spent; governance participants who approve new asset registrations.

## Motivation

### Why This Matters for CipherOcto

1. **Asset-generality is the substrate's design intent.** RFC-0105 v3.4 explicitly defines `asset_id_for(role_token: &str) -> [u8;32]` as the canonical substrate for asset identification. The substrate carries the SHAPE for asset-generality (32-byte digest + role_token string + scale field), but the application layer (caveats, settlement, naming) collapsed to a single asset (OCTO-W) at a single scale (0 = micro).
2. **The bridged-asset use case is on the roadmap.** Corporate chains, cross-chain bridges, and multi-asset marketplaces are explicitly listed in RFC-0960 §20.x and the long-horizon plan v1.6 research phase. The drift makes those scenarios unimplementable without re-architecting the substrate.
3. **Naming carries semantic load.** When `MicroOctoW` was retired 2026-08-17, the retirement note (RFC-0965 §v2.0-CanonicalAlias row + RFC-0862 v2.0.3 VH) explicitly flagged `amount_dqa_micros: i64` and `MICRO_PER_OCTOW` as vestigial naming. Three months later, those names are still in the substrate. Cleanup is overdue.
4. **Drift has accumulated, not by design but by accretion.** Each RFC amendment (RFC-0862 v2.0.3, RFC-0959 v2.7, RFC-0965 v2.0, RFC-0960 v3.5, RFC-0105 v3.4) added fields and constants without consolidating the asset-binding question. The drift is emergent; the fix is coordinated.

## Success Metrics

| Metric | Target | Measurement |
| ------ | ------ | ----------- |
| PaymentCaveat asset_id binding coverage | 100% of new payment caveats carry asset_id | substrate grep: `PaymentCaveat { .. }` literal count + audit log of new caveats |
| AssetRegistry side-table coverage | All 9 sovereign role tokens + all corporate-chain private assets registered | RFC-0105 v3.5 §Per-Asset Scale Table = substrate table; cross-check via cross-crate test vector |
| Naming cleanup completion | Zero remaining `MICRO_PER_OCTOW`, `amount_dqa_micros`, `RELAY_RATE_B_MICRO_OCTO_PER_GB` references in production code | substrate grep at the end of the migration cycle |
| Settlement cost migration | 100% of settlement events carry `cost: Dqa` with scale bound to the cost_vault_id's asset | `crates/quota-router-storage/migrations/*.sql` + cross-crate test vector |
| Backward compatibility break | Zero active substrate breaks for capability holders during the deprecation cycle (1 substrate release) | substrate integration tests + capability lifecycle audit |

## Constraints

- **Must not**: widen the frozen `AssetId(pub [u8;32])` substrate struct (semver-major on Layer A per RFC-0105 §Layer A frozen-substrate principle). The fix MUST use a Layer B additive side-table.
- **Must not**: introduce a third-party decimal library that breaks RFC-0104 Dfp determinism.
- **Must not**: add a central enum for asset kinds (use the typed-discriminator + Raw escape hatch pattern per `cipherocto-design-principles.md` §Extension over enumeration).
- **Limited to**: substrate changes that fit within the existing `Dqa { value: i64, scale: u8 }` 16-byte BE wire form (no wire-form change).
- **Limited to**: one substrate release cycle for backward compatibility on the deprecated `"paid-query/v1"` discriminator.

## Non-Goals

- **Not proposing**: a generic bridge contract for cross-chain asset transfers (separate RFC; RFC-0960 §20.x deferred).
- **Not proposing**: replacing `Dqa` with a different monetary substrate.
- **Not proposing**: a multi-token wallet UI or human-facing UX (separate RFC).
- **Not proposing**: federation across multiple cipherocto meshes (separate RFC).
- **Not proposing**: per-region regulatory scale caps (separate RFC).
- **Not proposing**: per-policy-kind asset-class refinement beyond the existing `NativeTokenTransfer | Erc20TokenTransfer` split (deferred).
- **Not proposing**: an asset-class taxonomy beyond `SovereignRoleToken | PrivateCorporateAsset | BridgedExternalAsset | WrappedCrossChainAsset` (RFC-0960 v3.6 additive).

## Impact

If implemented:

- **Capability issuers** can mint asset-bound budgets. A single capability can authorize spend against USDC-mirror at 6 decimals OR OCTO-W at scale 0, with the asset binding explicit in the caveat wire form.
- **Consumers** can present a capability against any supported asset, with the substrate verifying the asset binding at attenuation time. No more "implicit OCTO-W" interpretation.
- **Settlement engine** can record settlement events with `cost: Dqa` carrying the per-asset scale. Audit logs become readable across multi-asset marketplaces.
- **Vault substrate** becomes the canonical source of asset metadata. `AssetRegistry::metadata(asset_id)` resolves scale, denomination, and kind for any asset in the system.
- **Naming substrate** is consistent: no `MICRO_PER_OCTOW`, no `amount_dqa_micros`, no `RELAY_RATE_B_MICRO_OCTO_PER_GB`. Field names carry semantics, not asset-specific terminology.
- **Backwards-compatible migration**: one substrate release cycle accepts both the old and new wire forms. Deprecation warnings flag capabilities that use the old `"paid-query/v1"` discriminator.

## Related RFCs

- RFC-0105 (Deterministic Quant Arithmetic): §Asset ID Derivation — primary RFC for asset_id canonical derivation.
- RFC-0105 v3.4 (Private Asset ID Namespace): latest accepted version; v3.5 amendment proposed for per-asset scale table.
- RFC-0960 (Grand Design: Vaults, Capabilities, Reservations): primary RFC for vault composite key and asset_id binding.
- RFC-0960 v3.5 (Vault Path Taxonomy): latest accepted version; v3.6 amendment proposed for `BurnEventRef.amount: Dqa` migration.
- RFC-0965 (Capability Extension Format): primary RFC for `PaymentCaveat` and `PermissionKind`.
- RFC-0959 (Ask Settlement Chain): primary RFC for `SettlementEvent.cost` wire form.
- RFC-0959-A1 (Market Delivery Envelope): carries `cost_vault_id` for scale resolution.
- RFC-0862 (Writer Election Bootstrap): spends ledger substrate; references `MicroOctoW` alias.
- RFC-0957 (Macaroon Substrate): attenuation invariant — preserved across the amendment chain.
- RFC-0957-A1 (Layer Discipline for Adapters): adds `PaymentCaveat` to the per-extension crate pattern.
- RFC-0008 (Deterministic AI Execution Boundary): execution class mapping for the migration.

## Related Use Cases

- `docs/use-cases/enterprise-private-ai.md` — corporate chains register private assets; this use case carries the substrate-side support.
- `docs/use-cases/hybrid-ai-blockchain-runtime.md` — cross-chain asset flows require asset-generic payment.
- `docs/use-cases/data-marketplace.md` — multi-asset marketplace settlement.
- `docs/use-cases/dual-mode-authorization-workflow.md` — bearer + capability dual auth, where capability budgets are asset-bound.

## Related Research

- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` — adjacent research on vault substrate; this use case builds on it.
- `docs/research/2026-08-26-payment-caveat-asset-generality.md` — primary research doc for this use case.

---

**End of Use Case.**

Pending Use Case → RFC promotion per BLUEPRINT.md §The Core Separation.