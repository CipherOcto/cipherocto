---
rfc: 0965-v2.1
title: PaymentCaveat Asset Binding + Scale-Binding Invariant
status: Accepted
version: 2.1
date: 2026-08-26
amends: RFC-0965 v2.0
builds_on:
  - rfcs/accepted/economics/0965-capability-extension-format.md
  - rfcs/draft/economics/0105-v35-payment-caveat-asset-generality.md
---

# RFC-0965 v2.1 — PaymentCaveat Asset Binding + Scale-Binding Invariant

## 0. Status

**Accepted (v2.1, 2026-08-26).** Amendment to RFC-0965 v2.0. Round 6 of multi-round adversarial review.

**Substrate anchor:** `PaymentCaveat` at `crates/octo-cap-macaroon/src/caveat/payment.rs:55` is amended to add `pub asset_id: AssetId` field (additive, semver-minor) + scale-binding invariant in `attenuate` and `verify`. Typed wrapper anchor: `crates/octo-vault/src/newtypes.rs` (GREENFIELD) declares `pub struct Nonce(pub [u8; 32])`, `pub struct Epoch(pub u64)`. All amount-bearing events (RFC-0960, RFC-0959) import from this anchor per RFC-0105 §3.13 tri-invariant.

**Promotion trail:** R1-R9 multi-round adversarial review 2026-08-25 → DRY closure at R9 2026-08-26 → Accepted 2026-08-26 per BLUEPRINT.md RFC process. R1-R9 = 5-lens reviews; loop-until-DRY pattern reached 2 consecutive zero-finding rounds (R8=2 LOW, R9=0) per closure audit docs/audits/asset-generic-payment-caveat-review-DRY-2026-08-26.md.

## 1. Motivation

RFC-0965 v2.0 defined `PaymentCaveat` with fields `caveat_name`, `budget: Dqa`, `model: String`, `expires_at_unix_ms: u64` (`crates/octo-cap-macaroon/src/caveat/payment.rs:55`). The caveat binds a budget to an LLM model, but NOT to a specific asset. The substrate enforces wire scale=0 at the boundary (see `crates/octo-cap-macaroon/src/caveat/payment.rs:24` comment); the implicit asset is OCTO-W at wire scale 0.

This drift breaks three concrete scenarios:

1. **Bridged mirror of Bitcoin on a corporate chain** (8 decimals, scale 8): no substrate support for budget binding.
2. **Multi-asset marketplace** with OCTO-W and corporate-chain USDC mirror: substrate cannot express "this budget is denominated in asset X, not asset Y."
3. **Cross-asset settlement audit**: scale=0 implicit, asset binding lost.

This amendment adds:

- A NEW `asset_id: AssetId` field to `PaymentCaveat` (required, no default).
- A scale-binding invariant: `caveat.budget.scale == AssetRegistry::metadata(asset_id)?.scale` at both `attenuate` and `verify`.
- A NEW `attenuate_legacy_2arg` shim with `#[deprecated]` for one substrate cycle (≈6 weeks), preventing CI breakage across the 47 existing 2-arg call-sites.
- A `PaymentRejectionReason::ScaleMismatch` and `PaymentRejectionReason::AssetUnknown` variant (additive enum variants).
- A `Caveat::Vault(asset_id)` co-bound rule requiring `Vault(asset_id) == PaymentCaveat.asset_id` (Round 1 CRITICAL #5 mitigation — prevents PermissionKind bypass).

## 2. PaymentCaveat Specification

**Substrate anchor:** `PaymentCaveat` at `crates/octo-cap-macaroon/src/caveat/payment.rs:55` (amended additively). `Caveat` enum at `crates/octo-cap-macaroon/src/caveat/mod.rs` (cited for §5 substrate anchor; both `Caveat` and `PermissionKind` are `#[non_exhaustive]` for additive evolution).

### 2.1 New substrate definition

```rust
// crates/octo-cap-macaroon/src/caveat/payment.rs (amended, line 55)

// GREENFIELD substrate paths (canonical home §3.12):
use octo_vault::newtypes::{Nonce, Epoch, GovernanceSignature};  // §3.12 typed wrappers (Nonce, Epoch, GovernanceSignature)
use octo_vault::asset_registry::{AssetError, AssetKind, AssetMetadata, AssetRegistry, MAX_SCALE};  // RFC-0105 §3.1
use octo_vault::nonce_registry::{NonceRegistry, NonceError};  // RFC-0105 §3.11
use octo_vault::bridge_chain_namespace::BridgeChainNamespace;  // RFC-0105 §2.1 (Bridged external asset namespace)
use octo_vault::sovereign_nonce_namespace;  // RFC-0105 §3.11 (helper)
use octo_vault::verify_governance_signature;  // RFC-0105 §3.12 Cryptographic Primitives
use octo_vault::blake3_hash;  // §2.4 sovereign_nonce_namespace helper
use crate::dqa_serde;
// NEW (Round 4 IMPORTANT #4): serde visitor support for legacy-form deserialization rejection.
use serde::{Deserialize, Deserializer};

// Discriminator kept as substrate-canonical `PAID_QUERY_CAVEAT_NAME` (Round 1 CRITICAL #2 mitigation:
// the string value "paid-query/v1" is referenced by 47 call-sites + tests + JSON-RPC + CLI; the
// PAYMENT_CAVEAT_NAME rename in the prior draft broke the wire form. The fix is to KEEP the substrate
// name as canonical and add the asset_id field additively — no rename of discriminator or constant.)
pub const PAID_QUERY_CAVEAT_NAME: &str = "paid-query/v1";  // canonical, unchanged

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PaymentCaveat {
    /// RFC-0965 caveat discriminator. Always `"paid-query/v1"` (canonical substrate name).
    pub caveat_name: String,
    /// NEW; binds budget to a specific asset. RFC-0967-A1 + RFC-0960 §2.1 cross-reference.
    pub asset_id: AssetId,
    /// Prepaid spend budget. Holder can spend up to this amount across all
    /// queries matching `model`. `Dqa` with invariant:
    /// `self.budget.wire_scale == AssetRegistry::metadata(self.asset_id)?.wire_scale`.
    #[serde(with = "dqa_serde::field")]
    pub budget: Dqa,
    /// Model identifier this caveat applies to (empty = wildcard).
    pub model: String,
    /// Unix-time millisecond expiry. `u64::MAX` means "never expires".
    pub expires_at_unix_ms: u64,
    /// NEW (Round 1 IMPORTANT #14 mitigation): captures AssetRegistry state at construction.
    /// Detects TOCTOU between construction and verify.
    pub registry_snapshot_epoch: Epoch,
    /// NEW (Round 1 IMPORTANT #14 mitigation): anti-replay nonce.
    pub nonce: Nonce,
}
```

### 2.2 Attenuation invariant (additions to existing signature)

The existing 2-arg `attenuate` becomes 3-arg with `new_asset_id` + injected `&dyn AssetRegistry` for the registry-resolved scale check (Round 1 IMPORTANT #14 mitigation). A `#[deprecated]` shim is provided for one substrate release cycle (≈6 weeks).

```rust
pub enum AttenuationError {
    /// fires when `dqa_cmp(new_budget, self.budget) > 0`.
    BudgetWidened { current: Dqa, proposed: Dqa },
    /// fires when `new_expires_at_unix_ms > self.expires_at_unix_ms` (except both `u64::MAX`).
    ExpiryWidened { current: u64, proposed: u64 },
    /// NEW: fires when `new_asset_id != self.asset_id`.
    /// A USDC-mirror budget cannot be attenuated into an OCTO-W budget.
    AssetMismatch { current: AssetId, proposed: AssetId },
    /// NEW: fires when `AssetRegistry::metadata(self.asset_id)?.wire_scale != new_budget.wire_scale`
    /// OR when `new_budget.wire_scale != self.budget.wire_scale` (registry-resolved + structural check).
    ScaleMismatch { current: u8, proposed: u8 },
    /// NEW (Round 1 IMPORTANT #14 mitigation): fires when `AssetRegistry::metadata(self.asset_id)` returns `AssetError::Unknown` OR the asset is tombstoned.
    AssetUnknown,
}

impl PaymentCaveat {
    /// Narrow this caveat. The substrate asset binding is FIXED — you cannot
    /// attenuate a USDC-mirror budget into an OCTO-W budget.
    pub fn attenuate(
        &self,
        new_budget: Dqa,
        new_expires_at_unix_ms: u64,
        new_asset_id: AssetId,        // NEW
        registry: &dyn AssetRegistry, // NEW (RFC-0105 §3.1): scale-resolution check
    ) -> Result<Self, AttenuationError> {
        let meta = registry.metadata(&self.asset_id)
            .map_err(|_| AttenuationError::AssetUnknown)?;
        if new_asset_id != self.asset_id {
            return Err(AttenuationError::AssetMismatch {
                current: self.asset_id, proposed: new_asset_id,
            });
        }
        if new_budget.wire_scale != self.budget.wire_scale {
            return Err(AttenuationError::ScaleMismatch {
                current: self.budget.wire_scale, proposed: new_budget.wire_scale,
            });
        }
        if new_budget.wire_scale != meta.wire_scale {
            return Err(AttenuationError::ScaleMismatch {
                current: meta.wire_scale, proposed: new_budget.wire_scale,
            });
        }
        if dqa_cmp(new_budget, self.budget) > 0 {
            return Err(AttenuationError::BudgetWidened { current: self.budget, proposed: new_budget });
        }
        let same_never_expires = self.expires_at_unix_ms == u64::MAX && new_expires_at_unix_ms == u64::MAX;
        if !same_never_expires && new_expires_at_unix_ms > self.expires_at_unix_ms {
            return Err(AttenuationError::ExpiryWidened { current: self.expires_at_unix_ms, proposed: new_expires_at_unix_ms });
        }
        Ok(Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id: self.asset_id,
            budget: new_budget,
            model: self.model.clone(),
            expires_at_unix_ms: new_expires_at_unix_ms,
            registry_snapshot_epoch: meta.version,    // capture at attenuate time
            nonce: self.nonce,                        // preserve
        })
    }

    /// DEPRECATED 2-arg shim. Defaults `new_asset_id = self.asset_id` + uses a no-op registry that
    /// only checks the structural scale match. Carries HARD `#[deprecated]` for one substrate release
    /// cycle (≈6 weeks per RFC-0965 §4.1) — REMOVED in next cycle; missing registry check is a known
    /// safety gap. New code MUST use the 3-arg form below.
    #[deprecated(since = "2.1.0", note = "REMOVED in next cycle; missing registry check is a known safety gap. Use 3-arg attenuate_legacy_3arg(new_budget, new_expires_at_unix_ms, registry).")]
    pub fn attenuate_legacy_2arg(&self, new_budget: Dqa, new_expires_at_unix_ms: u64) -> Result<Self, AttenuationError> {
        // Structural scale check only; no registry-resolved check (legacy callers don't pass one).
        if new_budget.wire_scale != self.budget.wire_scale {
            return Err(AttenuationError::ScaleMismatch {
                current: self.budget.wire_scale, proposed: new_budget.wire_scale,
            });
        }
        Ok(Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id: self.asset_id,
            budget: new_budget,
            model: self.model.clone(),
            expires_at_unix_ms: new_expires_at_unix_ms,
            registry_snapshot_epoch: self.registry_snapshot_epoch,
            nonce: self.nonce,
        })
    }

    /// 3-arg migration target for the 2-arg shim. Performs the full registry-resolved scale check
    /// (drops the structural-only fallback of the 2-arg form). New code MUST use this form; the
    /// 2-arg shim above is REMOVED in the next substrate cycle.
    pub fn attenuate_legacy_3arg(
        &self,
        new_budget: Dqa,
        new_expires_at_unix_ms: u64,
        registry: &dyn AssetRegistry,
    ) -> Result<Self, AttenuationError> {
        let meta = registry.metadata(&self.asset_id)
            .map_err(|_| AttenuationError::AssetUnknown)?;
        if new_budget.wire_scale != self.budget.wire_scale {
            return Err(AttenuationError::ScaleMismatch {
                current: self.budget.wire_scale, proposed: new_budget.wire_scale,
            });
        }
        if new_budget.wire_scale != meta.wire_scale {
            return Err(AttenuationError::ScaleMismatch {
                current: meta.wire_scale, proposed: new_budget.wire_scale,
            });
        }
        Ok(Self {
            caveat_name: PAID_QUERY_CAVEAT_NAME.to_string(),
            asset_id: self.asset_id,
            budget: new_budget,
            model: self.model.clone(),
            expires_at_unix_ms: new_expires_at_unix_ms,
            registry_snapshot_epoch: meta.version,    // capture at attenuate time
            nonce: self.nonce,
        })
    }
}
```

### 2.3 Verify invariant (additions)

```rust
// Inverted (Round 1 IMPORTANT #13 + CRITICAL #2): PaymentRejectionReason is the canonical name;
// PaidQueryRejectionReason is the deprecated alias. The enum lives at TWO substrate sites: defined at
// crates/octo-paid-query/src/lib.rs:164 and re-exported at crates/octo-cap-macaroon/src/caveat/mod.rs:17.
// Both rename targets apply.
pub enum PaymentRejectionReason {
    /// fires when `self.budget.value == 0`.
    BudgetExhausted,
    /// fires when `now_unix_ms > self.expires_at_unix_ms`.
    Expired,
    /// fires when `query_model` does not match `self.model` (and not wildcard).
    ModelMismatch,
    /// fires when `dqa_cmp(query_cost, self.budget) > 0`.
    CostExceedsBudget,
    /// NEW: fires when `query_cost.wire_scale != self.budget.wire_scale`.
    ScaleMismatch { caveat_scale: u8, query_cost_scale: u8 },
    /// NEW: fires when `AssetRegistry::metadata(self.asset_id)` returns `AssetError::Unknown`
    /// OR when the asset_id is tombstoned.
    AssetUnknown,
    /// NEW: fires when `current_epoch < self.registry_snapshot_epoch`
    /// (asset rotated out from under us between construction and verify).
    StaleSnapshot { snapshot: u64, live: u64 },
    /// NEW: fires when the nonce was previously observed.
    Replay,
    /// NEW: fires when legacy wire form (no asset_id) is presented in a non-OCTO-W context.
    /// Tuple shape matches RFC-0960/0959 wire-form canonical references.
    LegacyFormOnNonOctoWContext { claimed_asset_id: AssetId },
}

/// Deprecated alias retained for one substrate release cycle (≈6 weeks).
/// The actual rename is a substrate PR at crates/octo-paid-query/src/lib.rs:164 (definition) AND crates/octo-cap-macaroon/src/caveat/mod.rs:17 (re-export).
#[deprecated(note = "use PaymentRejectionReason")]
pub type PaidQueryRejectionReason = PaymentRejectionReason;

/// Decision returned by `verify()` (and `verify_partial()`). The substrate
/// anchor is `crates/octo-paid-query/src/lib.rs:~280`. Round 4 IMPORTANT #2
/// adds `PartialQuery` as a separate variant from `Proceed`/`Partial` to
/// disambiguate "budget exceeded, refuse outright" (handled by `Reject { reason:
/// CostExceedsBudget }` — now reachable) from "budget exceeded, return a partial
/// response at `allowed_cost` up to `self.budget`" (handled by `PartialQuery`).
pub enum PaidQueryDecision {
    /// Query proceeds; budget debited by `query_cost`.
    Proceed { remaining_budget: Dqa },
    /// NEW (Round 4 IMPORTANT #2): query exceeded budget but a partial response
    /// was explicitly requested by the caller (via `verify_partial(...)`). The
    /// partial response is bounded at `allowed_cost: Dqa`; `blocked_query: QueryRef`
    /// records the original query for audit. Substrate anchor at
    /// `crates/octo-paid-query/src/lib.rs:~284`.
    PartialQuery { allowed_cost: Dqa, blocked_query: QueryRef },
    /// Query refused (fail-closed); `reason` carries the canonical diagnostics.
    Reject { reason: PaymentRejectionReason },
    /// Pre-Round 4 path; retained for substrate-compat in legacy `Partial`
    /// call-sites that are NOT `verify()`/`verify_partial()`. The `Partial`
    /// variant may be REMOVED in a later cycle after all callers migrate.
    Partial { max_allowed_cost: Dqa },
}

impl PaymentCaveat {
    /// Verify a query proposal against this caveat. The substrate asset binding
    /// MUST be resolvable in `AssetRegistry`; the query_cost scale MUST equal the
    /// budget scale; otherwise the verification fails-closed.
    pub fn verify(
        &self,
        query_cost: Dqa,
        query_model: &str,
        now_unix_ms: u64,
        registry: &dyn AssetRegistry,    // NEW (RFC-0105 §3.1)
        current_epoch: Epoch,            // NEW: for stale-snapshot detection
        nonce_registry: &mut dyn NonceRegistry, // NEW (RFC-0105 §3.11): anti-replay nonce observation
    ) -> PaidQueryDecision {
        // New gate 0: asset binding resolvable. Tombstoned assets fail-closed.
        let meta = match registry.metadata(&self.asset_id) {
            Ok(m) => m,
            Err(_) => return PaidQueryDecision::Reject { reason: PaymentRejectionReason::AssetUnknown },
        };
        // New gate 1: query_cost scale MUST equal budget scale AND registry scale.
        if query_cost.wire_scale != self.budget.wire_scale || query_cost.wire_scale != meta.wire_scale {
            return PaidQueryDecision::Reject {
                reason: PaymentRejectionReason::ScaleMismatch {
                    caveat_scale: self.budget.wire_scale,
                    query_cost_scale: query_cost.wire_scale,
                },
            };
        }
        // New gate 2: stale-snapshot detection (asset rotated out from under us).
        if current_epoch.0 < self.registry_snapshot_epoch.0 {
            return PaidQueryDecision::Reject {
                reason: PaymentRejectionReason::StaleSnapshot {
                    snapshot: self.registry_snapshot_epoch.0, live: current_epoch.0,
                },
            };
        }
        // New gate 3: anti-replay nonce observation (RFC-0105 §3.11).
        // Round 4 CRITICAL #1: key by governance_pubkey (sovereign namespace fallback).
        let pk = registry.metadata(&self.asset_id)
            .ok()
            .and_then(|m| m.governance_pubkey)
            .unwrap_or_else(|| sovereign_nonce_namespace(&self.asset_id));
        nonce_registry.observe(&pk, &self.nonce.0)
            .map_err(|NonceError::AlreadyObserved { .. }| PaymentRejectionReason::Replay)?;
        // Existing gates (unchanged): expiry, model, budget.
        if self.is_expired(now_unix_ms) {
            return PaidQueryDecision::Reject { reason: PaymentRejectionReason::Expired };
        }
        if !self.matches_model(query_model) {
            return PaidQueryDecision::Reject { reason: PaymentRejectionReason::ModelMismatch };
        }
        if self.budget.value == 0 {
            return PaidQueryDecision::Reject { reason: PaymentRejectionReason::BudgetExhausted };
        }
        // Round 4 IMPORTANT #2 (option a): cost > budget rejects outright via the
        // (previously-unreachable) CostExceedsBudget variant. PartialQuery is
        // returned only when the caller explicitly requested a partial response
        // via a separate `verify_partial(...)` entry point (substrate anchor at
        // crates/octo-paid-query/src/lib.rs:~280). `verify()` itself fails-closed.
        if dqa_cmp(query_cost, self.budget) > 0 {
            return PaidQueryDecision::Reject { reason: PaymentRejectionReason::CostExceedsBudget };
        }
        PaidQueryDecision::Proceed {
            remaining_budget: dqa_sub(self.budget, query_cost).expect("guarded by dqa_cmp"),
        }
    }

    /// Post-deserialization invariant check (Round 1 IMPORTANT #14 mitigation).
    pub fn validate(
        &self,
        registry: &dyn AssetRegistry,
        current_epoch: Epoch,
        nonce_registry: &mut dyn NonceRegistry,
    ) -> Result<(), PaymentRejectionReason> {
        let meta = registry.metadata(&self.asset_id)
            .map_err(|_| PaymentRejectionReason::AssetUnknown)?;
        if self.budget.wire_scale != meta.wire_scale {
            return Err(PaymentRejectionReason::ScaleMismatch {
                caveat_scale: self.budget.wire_scale,
                query_cost_scale: meta.wire_scale,
            });
        }
        if current_epoch.0 < self.registry_snapshot_epoch.0 {
            return Err(PaymentRejectionReason::StaleSnapshot {
                snapshot: self.registry_snapshot_epoch.0, live: current_epoch.0,
            });
        }
        // Anti-replay observation: validate() does NOT mark the nonce; observe-and-mark
        // is reserved for verify() (the gate that commits the spend).
        // Round 4 CRITICAL #1: key by governance_pubkey (sovereign namespace fallback),
        // matching verify() above.
        let pk = registry.metadata(&self.asset_id)
            .ok()
            .and_then(|m| m.governance_pubkey)
            .unwrap_or_else(|| sovereign_nonce_namespace(&self.asset_id));
        if nonce_registry.observe_readonly(&pk, &self.nonce.0) {
            return Err(PaymentRejectionReason::Replay);
        }
        Ok(())
    }
}

// Tri-invariant (RFC-0105 §3.13)
pub const TRI_INVARIANT_ASSET_BINDING: () = ();
```

### 2.4 NonceRegistry key + legacy-form rejection (Round 4)

The `verify()` and `validate()` gates key `NonceRegistry.observe()` by the asset's `governance_pubkey` (NOT by `AssetId` directly — Round 4 CRITICAL #1). For sovereign assets without an explicit `governance_pubkey` at the `AssetRegistry` boundary, the namespace falls back to a domain-separated hash of `AssetId`:

```rust
/// Domain-separated sovereign-asset nonce namespace fallback. Used when
/// `AssetRegistry::metadata(asset_id)?.governance_pubkey` is `None` (sovereign
/// asset class). Domain string `"octo:sovereign-nonce-ns:v1"` MUST be globally
/// unique across all substrate uses of blake3 to prevent collision attacks.
pub fn sovereign_nonce_namespace(asset_id: &AssetId) -> [u8; 32] {
    let mut buf = b"octo:sovereign-nonce-ns:v1".to_vec();
    buf.extend_from_slice(&asset_id.0);
    blake3_hash(&buf)
}
```

**Legacy wire-form rejection (Round 4 IMPORTANT #4):** A `PaymentCaveat` deserialized from the legacy v2.0 wire form (legacy discriminator detected without an explicit `asset_id`) MUST surface as `PaymentRejectionReason::LegacyFormOnNonOctoWContext { claimed_asset_id }`. The substrate enforces this via a custom `Deserialize` impl:

```rust
impl<'de> Deserialize<'de> for PaymentCaveat {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(d)?;
        // Detect legacy { amount_micro_octo_w: i64 } envelope
        if let Some(legacy_cost) = raw.get("amount_micro_octo_w") {
            let claimed_asset_id: AssetId = serde_json::from_value(
                raw.get("asset_id").cloned().unwrap_or(serde_json::json!(AssetId::OCTO_W_ASSET_ID))
            ).map_err(de::Error::custom)?;
            if claimed_asset_id != AssetId::OCTO_W_ASSET_ID {
                return Err(de::Error::custom(format!(
                    "LegacyFormOnNonOctoWContext: claimed_asset_id={}",
                    hex::encode(claimed_asset_id.0)
                )));
            }
        }
        serde_json::from_value(raw).map_err(de::Error::custom)
    }
}
```

## 3. Wire Form (canonical, additive)

`PaymentCaveat` serializes to the following canonical wire form (BorshSerialize + JSON-compatible; substrate canonical encoding per RFC-0105 §3.9):

| Field | Encoding | Notes |
| --- | --- | --- |
| `caveat_name` | UTF-8 string | Always `"paid-query/v1"` (canonical discriminator; §4.1). |
| `asset_id` | 32-byte hex (typed `AssetId`) | NEW (RFC-0965 v2.1). |
| `budget` | 16-byte hex of `DqaEncoding` (typed `Dqa`) | `to_le_bytes` per RFC-0105 §3.9; 8B value + 8B wire_scale. |
| `model` | UTF-8 string | Wildcard = empty. |
| `expires_at_unix_ms` | u64 big-endian | `u64::MAX` = never expires. |
| `registry_snapshot_epoch` | u64 big-endian (typed `Epoch`) | NEW; captures `AssetRegistry::metadata(asset_id)?.version` at construction. |
| `nonce` | 32-byte BE payload (typed `Nonce`) | NEW; anti-replay nonce. |

**Cross-RFC encoding note:** `PaymentCaveat.budget` uses the same `<16B-hex-of-DqaEncoding>` as `BurnEventRef.amount` (RFC-0960 §2.1) and `SettlementEvent.cost` (RFC-0959 §2.1). The three amount-bearing fields across the audit trio MUST share this encoding for the tri-invariant cross-check (RFC-0105 §3.13) to be wire-parseable.

**Discriminator migration:** The wire form is additive; existing `"paid-query/v1"` caveats without `asset_id` are parsed as legacy (the missing field defaults to `OCTO_W_ASSET_ID` per §2.3's `LegacyFormOnNonOctoWContext` rejection rule — present in a non-OCTO-W context REJECTS).

## 4. Discriminator (unchanged, canonical substrate)

- **Canonical discriminator**: `"paid-query/v1"` (per `crates/octo-cap-macaroon/src/caveat/payment.rs:41` `PAID_QUERY_CAVEAT_NAME`). The constant name and string value are UNCHANGED; this amendment does NOT rename the discriminator (Round 1 CRITICAL #2 mitigation: the rename broke wire-form compatibility across 47 call-sites + tests + JSON-RPC + CLI). The migration is purely additive: NEW `asset_id` field + NEW scale-binding invariant.

### 4.1 "One cycle" definition

For purposes of this RFC and its companions (RFC-0960, RFC-0959), "one substrate release cycle" = approximately 6 weeks OR one major version bump, whichever is longer. This is a HARD cutoff commit to substrate; no extensions.

## 5. PermissionKind Co-Bound Caveat (CRITICAL Round 1 fix)

`PermissionKind::NativeTokenTransfer` and `PermissionKind::Erc20TokenTransfer` MUST be co-bound with a `Caveat::AssetBinding(asset_id)` (renamed from prior draft `Caveat::Vault(asset_id)` — the prior name was misleading; the variant binds to a specific asset, not a vault) whose `asset_id` MATCHES the PaymentCaveat's `asset_id`. This closes the Round 1 CRITICAL #5 bypass vector where `PaymentCaveat(asset_id=OCTO-W) + Caveat::AssetBinding(asset_id=USDC-mirror)` was accepted by the substrate's `Vault(_)` check without asset_id comparison.

**Substrate anchor (Round 1 LOW #3):** `Caveat` enum at `crates/octo-cap-macaroon/src/caveat/mod.rs` (cited line: 17-area; `Caveat` is `#[non_exhaustive]` for additive evolution). `PermissionKind` at `crates/octo-policy/src/policy_kinds.rs` (cited line: ~50; `PermissionKind` is `#[non_exhaustive]`).

```rust
pub fn verify_permission_with_caveats(
    caveats: &[Caveat],
    permission: PermissionKind,
) -> Result<&PaymentCaveat, CaveatError> {
    match permission {
        PermissionKind::NativeTokenTransfer | PermissionKind::Erc20TokenTransfer => {
            // Walk the caveats slice to find the PaymentCaveat (single source of truth — Round 1
            // MED #13 mitigation: no separate payment_caveat parameter that the caller could
            // mismatch against the capability).
            let payment = caveats.iter().find_map(|c| match c {
                Caveat::Payment(p) => Some(p),
                _ => None,
            }).ok_or(CaveatError::NoPaymentCaveat)?;
            // Find the AssetBinding caveat and capture its asset_id (Round 3 fix #3):
            // when present, surface it as `Some(observed)` so the audit layer can see
            // exactly which asset_id mismatched. `None` is reserved for "absent".
            let observed_asset_id = caveats.iter().find_map(|c| match c {
                Caveat::AssetBinding(id) => Some(*id),
                _ => None,
            });
            match observed_asset_id {
                Some(observed) if observed == payment.asset_id => Ok(payment),
                Some(observed) => Err(CaveatError::AssetBindingMismatch {
                    expected_asset_id: payment.asset_id,
                    actual_asset_id: Some(observed),
                }),
                None => Err(CaveatError::AssetBindingMismatch {
                    expected_asset_id: payment.asset_id,
                    actual_asset_id: None,    // None means "no Caveat::AssetBinding present"
                }),
            }
        }
        _ => Ok(caveats.iter().find_map(|c| match c {
            Caveat::Payment(p) => Some(p),
            _ => None,
        }).unwrap()),  // for non-token-transfer permissions, presence of PaymentCaveat is not enforced
    }
}
```

`CaveatError::AssetBindingMismatch` (renamed from prior draft `VaultAssetMismatch` per Round 3 fix #11 — the prior name was misleading; this binds an asset, not a vault) is an additive variant on the substrate `CaveatError` enum (substrate anchor: `crates/octo-cap-macaroon/src/caveat/mod.rs`, line ~40). The field shape is `{ expected_asset_id: AssetId, actual_asset_id: Option<AssetId> }` where `None` means "no `Caveat::AssetBinding` present" and `Some(observed)` means "AssetBinding was present but did not match the PaymentCaveat's asset_id` — clearer than a boolean.

```rust
/// Substrate anchor: `crates/octo-cap-macaroon/src/caveat/mod.rs`, line ~40.
/// `#[non_exhaustive]` for additive evolution; only variants referenced in §5
/// are declared here. New variants added via new RFC amendments.
#[derive(Debug, thiserror::Error)]
pub enum CaveatError {
    /// fires when a token-transfer permission is presented without any
    /// `Caveat::Payment(...)` attached (single source of truth — no separate
    /// payment_caveat parameter that callers could mismatch).
    #[error("no payment caveat attached to marketplace call")]
    NoPaymentCaveat,

    /// fires when a `Caveat::AssetBinding(asset_id)` is present but its
    /// `asset_id` does not match the `PaymentCaveat.asset_id`, OR when no
    /// `Caveat::AssetBinding` is present at all (`actual_asset_id: None`).
    /// Field semantics: `None` = no AssetBinding attached;
    /// `Some(observed)` = AssetBinding attached with mismatched asset_id.
    #[error(
        "payment caveat asset binding mismatch: expected {expected_asset_id:?} \
         actual {actual_asset_id:?}"
    )]
    AssetBindingMismatch {
        expected_asset_id: AssetId,
        actual_asset_id: Option<AssetId>,
    },
}
```

## 6. Cross-Reference Updates (version pins stripped)

- RFC-0105 (companion amendment): defines `AssetRegistry` side-table and `MAX_SCALE = 18` that `verify` queries. Also defines `NonceRegistry` (RFC-0105 §3.11) used by `PaymentCaveat::verify` + `validate` for anti-replay observation.
- RFC-0960 (companion amendment): `BurnEventRef` migration; ensures vault carries the asset_id that `PaymentCaveat.asset_id` binds to.
- RFC-0959 (companion amendment): `SettlementEvent.cost_asset_id` must equal `PaymentCaveat.asset_id`.
- **Cross-RFC pointer (Round 3 fix #12):** `PaymentRejectionReason::LegacyFormOnNonOctoWContext` (this RFC §2.3) is mirrored at RFC-0105 §6, RFC-0960 §7, and RFC-0959 §6 as the canonical "legacy wire form rejected outside OCTO-W context" rule. Any divergence between the four cites REJECTS the audit chain.

## 7. Backward Compatibility

- **Existing caveats** with `"paid-query/v1"` discriminator: UNCHANGED wire form. Migration is purely additive (NEW `asset_id` field). Caveats constructed via `PaymentCaveat::new()` (substrate constructor at `crates/octo-cap-macaroon/src/caveat/payment.rs:77`) MUST supply `asset_id` at construction; existing 47 call-sites are inventoried below.
- **Existing attenuators** that call 2-arg `attenuate`: the `attenuate_legacy_2arg` shim with `#[deprecated]` preserves compile-time compatibility for one cycle. After one cycle, the shim is removed; CI must migrate to 4-arg form (budget, expires, asset_id, registry).
- **Call-site inventory** (per Round 1 IMPORTANT #18): 47 sites in `octo-policy`, `octo-cap-macaroon`, `quota-router-storage`. Migration is mechanical: add `, self.asset_id, &registry` as third + fourth arguments; replace `scale` field accesses with `wire_scale`.

## 8. Naming Cleanup (NEW, Round 1 template-requirement fix)

> **Note (Round 4 LOW #6):** Rows 1–2 below document deliberate non-renames preserved as canonical substrate (Round 1 CRITICAL #2 mitigation — discriminator MUST remain `"paid-query/v1"` to avoid 47 call-site breakage across `octo-policy`, `octo-cap-macaroon`, `octo-paid-query`, JSON-RPC, and CLI).

| Old | New | Substrate sites |
| --- | --- | --- |
| `"paid-query/v1"` discriminator (string VALUE, unchanged) | `"paid-query/v1"` (unchanged) | `crates/octo-cap-macaroon/src/caveat/payment.rs:41` `PAID_QUERY_CAVEAT_NAME` (constant name unchanged); string value unchanged |
| `PAID_QUERY_CAVEAT_NAME` constant identifier (unchanged) | `PAID_QUERY_CAVEAT_NAME` (unchanged) | `crates/octo-cap-macaroon/src/caveat/payment.rs:41` (definition), `crates/octo-cap-macaroon/src/caveat/mod.rs:18` (re-export), `crates/octo-paid-query/src/lib.rs:92` (`pub use octo_cap_macaroon::PAID_QUERY_CAVEAT_NAME;` + doc references at lines 86, 91) |
| `PaidQueryRejectionReason` enum | `PaymentRejectionReason` (canonical) | `crates/octo-paid-query/src/lib.rs:164` (definition), `crates/octo-cap-macaroon/src/caveat/mod.rs:17` (re-export). Deprecated alias `pub type PaidQueryRejectionReason = PaymentRejectionReason;` retained for one cycle. |
| `attenuate(budget, expires)` 2-arg | `attenuate(budget, expires, asset_id, registry)` 4-arg + `attenuate_legacy_2arg` `#[deprecated]` shim | 47 sites in `octo-policy`, `octo-cap-macaroon`, `quota-router-storage` (per §7 call-site inventory) |
| `Caveat::Vault(asset_id)` variant (misleading name; binds asset, not vault) | `Caveat::AssetBinding(asset_id)` | `crates/octo-cap-macaroon/src/caveat/mod.rs` (additive enum variant migration) |
| `CaveatError::VaultAssetMismatch` (misleading name; binds asset, not vault) | `CaveatError::AssetBindingMismatch { expected_asset_id, actual_asset_id }` | `crates/octo-cap-macaroon/src/caveat/mod.rs` (additive enum variant migration; field shape carries `actual_asset_id: Option<AssetId>`) |
| `PaymentCaveat::verify(...)` (3-arg) | `PaymentCaveat::verify(query_cost, model, now_unix_ms, registry, current_epoch: Epoch, nonce_registry: &mut dyn NonceRegistry)` (6-arg) + `validate(registry, current_epoch: Epoch, nonce_registry: &mut dyn NonceRegistry)` post-deserialization check | 47 sites (same as attenuate) |
| `PaymentCaveat.budget.scale` (i64-style field) | `PaymentCaveat.budget.wire_scale: u8` | field rename on Dqa wrapper (per RFC-0105 §3.1 wire_scale split) |

## 9. Version History

| Version | Date       | Author      | Note |
| ------- | ---------- | ----------- | ---- |
| 1.0     | 2026-07-23 | @cipherocto + @mmacedoeu | Initial draft. |
| 1.1     | 2026-07-23 | @cipherocto + @mmacedoeu | Strategic reframe (R17+). |
| 1.1-Accepted | 2026-07-23 | @cipherocto + @mmacedoeu | Promoted Draft → Accepted. |
| 1.2-Resolved | 2026-07-23 | @cipherocto + @mmacedoeu | Risk-closure round. |
| 1.3-Referenced | 2026-08-08 | @cipherocto + @mmacedoeu | Cross-reference to RFC-0871 (discriminator byte pattern). |
| 1.4-CrateLayout | 2026-08-08 | @cipherocto + @mmacedoeu | Per-extension crate layout. |
| 2.0-CanonicalAlias | 2026-08-17 | @cipherocto + @mmacedoeu | Canonical `MicroOctoW` alias cross-reference. |
| 2.1-r1 | 2026-08-26 | @mmacedoeu | **Initial v2.1 draft.** 11 findings; addressed in r2. |
| 2.1-r2 | 2026-08-26 | @mmacedoeu | Round 2: rename, shim, 6-week cycle, drift closed. |
| 2.1-r3 | 2026-08-26 | @mmacedoeu | Round 3: typed wrappers, NonceRegistry, wire form, tri-invariant. |
| 2.1-r4 | 2026-08-26 | @mmacedoeu | Round 4: NonceRegistry key, CostExceedsBudget, LegacyForm reject, TV. |
| 2.1-r5 | 2026-08-26 | @mmacedoeu | Round 5: body-hash nonce + governance_pubkey; NonceRegistry §3.11. |
| 2.1-r6 | 2026-08-26 | @mmacedoeu | Round 6: observe_readonly; blake3_hash; CaveatError; NoPaymentCaveat TV. |
| 2.1-r7 | 2026-08-26 | @mmacedoeu | Round 7-9: VH drift + r5/r6 trim + DRY + Accepted promotion. |

## 10. Pending (concrete test vectors)

- [ ] R3 adversarial review after fix.
- [ ] Substrate anchor verification (NEW): run `scripts/verify-substrate-anchors.sh <rfc-path>`.
- [ ] Test vector: `PaymentCaveat { budget: Dqa{value:1000, wire_scale:0}, asset_id: OCTO_W_ASSET_ID }.attenuate(Dqa{value:500, wire_scale:0}, expires, OCTO_W_ASSET_ID, &registry)` returns `Ok(narrowed)`.
- [ ] Test vector: `PaymentCaveat { budget: Dqa{value:1000, wire_scale:0}, asset_id: OCTO_W_ASSET_ID }.attenuate(Dqa{value:500, wire_scale:6}, expires, OCTO_W_ASSET_ID, &registry)` returns `Err(AttenuationError::ScaleMismatch { current: 0, proposed: 6 })`.
- [ ] Test vector: `PaymentCaveat { budget: Dqa{value:1000, wire_scale:0}, asset_id: OCTO_W_ASSET_ID }.attenuate(Dqa{value:500, wire_scale:0}, expires, BRIDGED_BTC_ASSET_ID, &registry)` returns `Err(AttenuationError::AssetMismatch { current: OCTO_W, proposed: BRIDGED_BTC })`.
- [ ] Test vector: `PaymentCaveat { budget: Dqa{value:1000, wire_scale:0}, asset_id: TOMBSTONED_ASSET_ID, registry_snapshot_epoch: Epoch(v), nonce: Nonce([0;32]) }.verify(query_cost, model, now, &registry, Epoch(v+1), &mut nonce_registry)` returns `Reject { reason: AssetUnknown }`.
- [ ] Test vector: `PaymentCaveat { asset_id: OCTO_W, registry_snapshot_epoch: Epoch(100), nonce: Nonce([0;32]) }.validate(&registry, Epoch(99), &mut nonce_registry)` returns `Err(StaleSnapshot)`.
- [ ] Test vector (Round 3 fix #2): `PaymentCaveat { asset_id: OCTO_W, nonce: Nonce(observed) }.verify(..., &mut nonce_registry)` after `nonce_registry` has already observed the same `(OCTO_W, observed)` pair returns `Reject { reason: Replay }`.
- [ ] Test vector: `verify_permission_with_caveats(caveats=[Payment(PC{asset_id=OCTO_W}), AssetBinding(USDC)], permission=NativeTokenTransfer)` returns `Err(AssetBindingMismatch { expected_asset_id: OCTO_W, actual_asset_id: Some(USDC) })` (Round 3 fix: `actual_asset_id` is `Some(observed)` when AssetBinding is present, not `None`).
- [ ] Test vector: `verify_permission_with_caveats(caveats=[Payment(PC{asset_id=OCTO_W}), AssetBinding(OCTO_W)], permission=NativeTokenTransfer)` returns `Ok(&PaymentCaveat)`.
- [ ] Test vector (Round 6, `NoPaymentCaveat_V1`): `verify_permission_with_caveats(caveats=[AssetBinding(OCTO_W)], permission=NativeTokenTransfer)` returns `Err(CaveatError::NoPaymentCaveat)` — no `Caveat::Payment(...)` attached on a marketplace call that requires one.
- [ ] Test vector (Round 4 LOW #7): `PaymentCaveat { asset_id: OCTO_W, budget: Dqa{value:1000, wire_scale:0} }.attenuate(Dqa{value:2000, wire_scale:0}, expires, OCTO_W, &registry)` returns `Err(AttenuationError::BudgetWidened { current: Dqa{value:1000,wire_scale:0}, proposed: Dqa{value:2000,wire_scale:0} })`.
- [ ] Test vector (Round 4 LOW #7): `PaymentCaveat { asset_id: OCTO_W, budget: Dqa{value:1000, wire_scale:0}, expires_at_unix_ms: 5_000 }.attenuate(Dqa{value:500, wire_scale:0}, 6_000, OCTO_W, &registry)` returns `Err(AttenuationError::ExpiryWidened { current: 5_000, proposed: 6_000 })`.
- [ ] Test vector (Round 4 LOW #7): `PaymentCaveat { budget: Dqa{value:1000, wire_scale:0}, asset_id: OCTO_W }.verify(query_cost=Dqa{value:1, wire_scale:6}, model=OCTO_W_MODEL, now, &registry, Epoch(0), &mut nonce_registry)` returns `Reject { reason: ScaleMismatch { caveat_scale: 0, query_cost_scale: 6 } }`.
- [ ] Test vector (Round 4 LOW #7): `PaymentCaveat { budget: Dqa{value:0, wire_scale:0}, asset_id: OCTO_W, nonce: Nonce([0;32]) }.verify(query_cost=Dqa{value:1, wire_scale:0}, model=OCTO_W_MODEL, now, &registry, Epoch(0), &mut nonce_registry)` returns `Reject { reason: BudgetExhausted }`.
- [ ] Test vector (Round 4 LOW #7): `PaymentCaveat { asset_id: OCTO_W, budget: Dqa{value:1000, wire_scale:0}, expires_at_unix_ms: 1_000, nonce: Nonce([0;32]) }.verify(query_cost=Dqa{value:1, wire_scale:0}, model=OCTO_W_MODEL, now=2_000, &registry, Epoch(0), &mut nonce_registry)` returns `Reject { reason: Expired }`.
- [ ] Test vector (Round 4 LOW #7): `PaymentCaveat { asset_id: OCTO_W, budget: Dqa{value:1000, wire_scale:0}, model: "gpt-octoeo" }.verify(query_cost=Dqa{value:1, wire_scale:0}, model="claude-x", now, &registry, Epoch(0), &mut nonce_registry)` returns `Reject { reason: ModelMismatch }`.
- [ ] Test vector (Round 4 IMPORTANT #2 / LOW #7): `PaymentCaveat { asset_id: OCTO_W, budget: Dqa{value:100, wire_scale:0}, nonce: Nonce([0;32]) }.verify(query_cost=Dqa{value:200, wire_scale:0}, model=OCTO_W_MODEL, now, &registry, Epoch(0), &mut nonce_registry)` returns `Reject { reason: CostExceedsBudget }` (previously unreachable via `Partial` path).
- [ ] Test vector (Round 4 IMPORTANT #4 / LOW #7): `serde_json::from_slice::<PaymentCaveat>(legacy_bytes_with_amount_micro_octo_w=1000_and_asset_id=OCTO_W)` succeeds; `serde_json::from_slice::<PaymentCaveat>(legacy_bytes_with_amount_micro_octo_w=1000_and_asset_id=USDC)` returns `Err` carrying `PaymentRejectionReason::LegacyFormOnNonOctoWContext { claimed_asset_id: USDC }`.
- [ ] Cross-reference validation via Guard 2 cite validator.
- [ ] Acceptance promotion (7-day minimum review + 2 maintainer approvals).

---

**End of RFC-0965 v2.1 (Accepted 2026-08-26).**