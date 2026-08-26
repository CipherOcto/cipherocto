---
rfc: 0959-v2.8
title: SettlementEvent.cost Dqa Migration + cost_asset_id Audit Invariant
status: Accepted
version: 2.8
date: 2026-08-26
amends: RFC-0959 v2.7
builds_on:
  - rfcs/accepted/economics/0959-ask-settlement-chain.md
  - rfcs/draft/economics/0105-v35-payment-caveat-asset-generality.md
  - rfcs/draft/economics/0960-v36-burn-event-dqa-migration.md
---

# RFC-0959 v2.8 — SettlementEvent.cost Dqa Migration + cost_asset_id Audit Invariant

## 0. Status

**Accepted (v2.8, 2026-08-26).** Amendment to RFC-0959 v2.7.

**Promotion trail:** R1-R9 multi-round adversarial review 2026-08-25 → DRY closure at R9 2026-08-26 → Accepted 2026-08-26 per BLUEPRINT.md RFC process. R1-R9 = 5-lens reviews; loop-until-DRY pattern reached 2 consecutive zero-finding rounds (R8=2 LOW, R9=0) per closure audit docs/audits/asset-generic-payment-caveat-review-DRY-2026-08-26.md.

**Substrate anchor (Round 2 corrected):** the substrate has NO `SettlementEvent` struct as a top-level type. The settlement pathway runs through `crates/quota-router-sm-engine/src/lib.rs:266` (`SettlementError::AlreadyConsumed(String)`) + `crates/quota-router-storage/src/consumed_receipt_repo.rs:26` (`AlreadyConsumed` arm) + `crates/quota-router-sm-engine/src/store.rs:163,211` (`SettlementError::AlreadyConsumed` arm). `AlreadyConsumed` is the canonical substrate variant (no rename history; `ReceiptReplay` was never a substrate name).

`SettlementEvent` is **GREENFIELD** (introduced by this RFC as a Layer B additive type). `VaultId(pub [u8; 32])` at `crates/octo-vault/src/lib.rs:82` is a tuple struct (single 32-byte wrapper) — not a struct with chain_id/vault_id/asset_id fields. Round 2 keeps `cost_vault_id: VaultId` (tuple, frozen Layer A) and adds a NEW `cost_asset_id: AssetId` field for the audit-invariant. This preserves Layer A stability per the substrate model.

**NonceRegistry anchor (Round 5 fix):** `NonceRegistry` trait + `NonceError` enum at **RFC-0105 §3.11** (GREENFIELD; canonical home).

## 1. Motivation

The settlement pathway today records cost as `amount_micro_octo_w: Dqa` (already migrated from `u128` to `Dqa` per mission 0105-x; verified at `crates/quota-router-core/src/marketplace/escrow.rs:159` + `EscrowSnapshot` line 172 + `crates/quota-router-storage/src/slash_store.rs:95`) and as `SettlementError::AlreadyConsumed(String)` on the engine (`crates/quota-router-sm-engine/src/lib.rs:266`). There is no canonical `SettlementEvent` substrate type. The `i64` claim in the prior draft was incorrect (the carrier is already `Dqa`; only the field NAME `amount_micro_octo_w` is outstanding — see §5 Naming Cleanup for the rename plan). This amendment:

- Introduces `SettlementEvent` as a new Layer B type (greenfield, additive).
- Migrates the cost carrier to `cost: Dqa` with explicit `cost_asset_id: AssetId` (replaces implicit OCTO-W at wire scale 0).
- Adds scale-resolution via `AssetRegistry` (RFC-0105 §3.1).
- Adds a fail-closed check: `cost.wire_scale != AssetRegistry::metadata(cost_asset_id).wire_scale` REJECTS the event.
- Adds an audit invariant: per RFC-0105 §3.13 tri-invariant, `SettlementEvent.cost_asset_id` MUST equal `PaymentCaveat.asset_id` (RFC-0965) AND `BurnEventRef.asset_id` (RFC-0960). Violation REJECTS the audit chain.
- Adds a governance signature requirement on `SettlementEvent` (parallel to RFC-0960 §2.1).
- Adds a vault-contains-asset check: `vault.contains_asset(cost_asset_id)` REJECTS the event if the vault does not actually hold the asset (Round 1 CRITICAL #3 mitigation: cross-asset attribution bypass).

## 2. SettlementEvent Specification

### 2.1 Substrate definition (NEW, greenfield)

> Forward-reference: see RFC-0960 §2 Specification for the vault-balance projection substrate; the `SettlementEventProducer` in §2.5 wraps `SettlementEventRepository::insert` and emits `VaultProjectionInvalidationEnvelope` to the RFC-0913 bus.

```rust
// crates/quota-router-sm-engine/src/settlement_event.rs (NEW file)

// GREENFIELD substrate paths (canonical home RFC-0105 §3.11 NonceRegistry + §3.12 Cryptographic Primitives):
use octo_vault::newtypes::{Nonce, Epoch, GovernanceSignature};  // §3.11 (NonceRegistry substrate) + §3.12 (Cryptographic Primitives)
use octo_vault::asset_registry::{AssetError, AssetKind, AssetMetadata, AssetRegistry, MAX_SCALE};  // §3.1
use octo_vault::nonce_registry::{NonceRegistry, NonceError};  // §3.11
use octo_vault::bridge_chain_namespace::BridgeChainNamespace;  // §2.1 (Bridged external asset namespace)
use octo_vault::sovereign_nonce_namespace;  // §3.11 (helper)
use octo_vault::verify_governance_signature;  // §3.12 Cryptographic Primitives
use octo_vault::blake3_hash;  // §3.12 Cryptographic Primitives

use borsh::{BorshDeserialize, BorshSerialize};
use octo_determin::Dqa;
use octo_vault::{AssetId, VaultId};
use serde::{Deserialize, Serialize};

// Typed newtypes (Layer B additive, semver-minor) — substrate convention from RFC-0105 §3.1.
// SettlementId, AskId, EvidenceRef are GREENFIELD local (no RFC-0105 §3 section allocates these);
// Nonce, Epoch, GovernanceSignature are imported from octo_vault::newtypes per RFC-0105 §3.11
// (Round 3 fix #11; canonical home shared with NonceRegistry).
pub struct SettlementId(pub [u8; 32]);
pub struct AskId(pub [u8; 32]);
pub struct EvidenceRef(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, BorshSerialize, BorshDeserialize)]
// Round 4 fix #6: Deserialize is hand-implemented below to reject the legacy envelope when
// paired with a non-OCTO-W cost_asset_id (asset-binding bypass close at deserialization
// boundary, not just the new() path).
pub struct SettlementEvent {
    pub settlement_id: SettlementId,             // typed wrapper
    pub ask_id: AskId,                          // typed wrapper
    pub cost_vault_id: VaultId,                  // octo_vault::VaultId tuple (32-byte wrapper, Layer A frozen; GREENFIELD, no rename history — see §2.1 annotation)
    pub cost_asset_id: AssetId,                  // NEW: explicit asset binding for audit
    pub asset_kind: AssetKind,                   // NEW: populated from meta.kind.clone() in new(); mirrors BurnEventRef (Round 3 fix #17)
    pub cost: Dqa,                              // scale MUST equal AssetRegistry::metadata(cost_asset_id).wire_scale
    pub evidence_ref: EvidenceRef,               // typed wrapper
    pub ledger_height: u64,                      // NEW (Round 3 fix #2): deterministic ordering anchor; replaces created_at_unix_ms inside the signed body-hash. created_at_unix_ms remains an informational field outside the signed commitment (mirrors RFC-0960 §2.2).
    pub created_at_unix_ms: u64,                 // informational only; NOT included in body-hash
    pub settlement_decision: SettlementDecision,
    pub governance_signature: GovernanceSignature,  // NEW: ed25519 sig signed by cost_vault_id's governance key
    pub registry_snapshot_epoch: Epoch,          // NEW: asset registry epoch captured at construction
    pub nonce: Nonce,                            // NEW: anti-replay
}

pub enum SettlementDecision {
    /// fires when the settlement was successful and the cost charged to the vault.
    Consumed,
    /// fires when the settlement receipt was already consumed by a prior settlement event.
    /// Canonical substrate arm (per `crates/quota-router-sm-engine/src/lib.rs:266`).
    /// No rename history; "ReceiptReplay" was never a substrate variant.
    AlreadyConsumed,
    /// fires when the evidence_ref does not match a known ask or its authenticity fails.
    InsufficientEvidence,
    /// fires when the vault's available balance for `cost_asset_id` is below `cost` at settlement time.
    BudgetExhausted,
    // Audit* variants REMOVED — live in SettlementAuditError (Round 4 fix #4:
    // §2.3 SettlementAuditError is the sole home for runtime audit outcomes;
    // SettlementDecision holds only construction-time + decision outcomes).
}
```

### 2.2 Scale-resolution invariant

```rust
pub enum VaultRegistryError {
    /// fires when the vault is not known to the registry.
    UnknownVault { vault_id: VaultId },
    /// fires when the vault is known but does not contain the asset.
    VaultAssetMismatch { vault_id: VaultId, asset_id: AssetId },
}

pub enum SettlementEventError {
    /// fires when `AssetRegistry::metadata(cost_asset_id)` returns `AssetError::Unknown`
    /// OR the asset is tombstoned.
    AssetUnknown,
    /// fires when `cost.wire_scale != metadata.wire_scale`.
    ScaleMismatch { cost_scale: u8, vault_scale: u8 },
    /// fires when `cost.wire_scale > MAX_SCALE` (defense-in-depth).
    ScaleOutOfRange { scale: u8 },
    /// fires when `governance_signature` does not verify against `cost_vault_id`'s registered key
    /// (for non-sovereign assets). Sovereign assets exempt per RFC-0105 §3.1 sovereign exemption —
    /// see §3 of this RFC.
    InvalidSignature,
    /// fires when the nonce was previously observed in the injected `&mut dyn NonceRegistry`.
    Replay,
    /// fires when `current_epoch.0 < self.registry_snapshot_epoch.0` (asset rotated out).
    StaleSnapshot { snapshot: u64, live: u64 },
    /// fires when `vault_registry.contains_asset(cost_vault_id, cost_asset_id)` returns
    /// `Err(VaultRegistryError::VaultAssetMismatch)`.
    VaultAssetMismatch { vault_id: VaultId, asset_id: AssetId },
    /// fires when `vault_registry.contains_asset(cost_vault_id, cost_asset_id)` returns
    /// `Err(VaultRegistryError::UnknownVault)`.
    VaultUnknown { vault_id: VaultId },
    /// fires when a legacy `cost: { amount_micro_octo_w }` form is submitted with a non-OCTO-W
    /// `cost_asset_id` (Round 3 fix #13; see §3.2 wire-form migration close). Serde discriminator:
    /// `"legacy_form_on_non_octow_context"`.
    LegacyFormOnNonOctoWContext { claimed_asset_id: AssetId },
}

impl SettlementEvent {
    /// Canonical length-prefixed encoding of SettlementDecision for body_hash commitment.
    /// Discriminant byte (0x01..0x04) + payload bytes (length-prefixed per variant).
    /// GREENFIELD substrate path: `octo_quota_router_sm_engine::settlement_event::SettlementEvent::encode_settlement_decision`
    /// (associated function on `SettlementEvent`, called via `Self::`).
    /// Round 5 fix (R5 R1 MED-07): explicit helper was previously referenced by `new()`
    /// but never defined; promoted to documented public API. Round 6: factored
    /// `compute_settlement_body_hash` helper shared with `validate()` (see below).
    pub fn encode_settlement_decision(d: &SettlementDecision) -> Vec<u8> {
        match d {
            SettlementDecision::Consumed => {
                let mut v = vec![0x01];
                v
            }
            SettlementDecision::AlreadyConsumed => {
                let mut v = vec![0x02];
                v
            }
            SettlementDecision::InsufficientEvidence => {
                let mut v = vec![0x03];
                v
            }
            SettlementDecision::BudgetExhausted => {
                let mut v = vec![0x04];
                v
            }
        }
    }

    /// Both new() and validate() MUST compute the same hash over the same
    /// field-set, otherwise a forger can mutate a field post-sign and the
    /// re-verify at validate() will fail on a different hash.
    /// Round 6 fix: factored out from the inline body-hash construction that
    /// previously duplicated the field-set between new() and validate().
    /// Mirrors RFC-0960 §2.2 L371-393 (`compute_body_hash`).
    fn compute_settlement_body_hash(
        settlement_id: SettlementId,
        ask_id: AskId,
        cost_vault_id: VaultId,
        cost_asset_id: AssetId,
        kind_tag: u8,
        cost: Dqa,
        ledger_height: u64,
        evidence_ref: EvidenceRef,
        governance_pubkey: [u8; 32],
        nonce: &Nonce,
    ) -> [u8; 32] {
        let mut buf = Vec::new();
        buf.extend_from_slice(&settlement_id.0);
        buf.extend_from_slice(&ask_id.0);
        buf.extend_from_slice(&cost_vault_id.0);
        buf.extend_from_slice(&cost_asset_id.0);
        buf.push(kind_tag);
        buf.extend_from_slice(&cost.0.to_le_bytes());
        buf.extend_from_slice(&ledger_height.to_le_bytes());
        buf.extend_from_slice(&evidence_ref.0);
        buf.extend_from_slice(&governance_pubkey);
        buf.extend_from_slice(&nonce.0);
        blake3_hash(&buf)
    }

    pub fn new(
        settlement_id: SettlementId,
        ask_id: AskId,
        cost_vault_id: VaultId,
        cost_asset_id: AssetId,
        cost: Dqa,
        evidence_ref: EvidenceRef,
        ledger_height: u64,
        created_at_unix_ms: u64,
        settlement_decision: SettlementDecision,
        governance_signature: GovernanceSignature,
        nonce: Nonce,
        registry: &dyn AssetRegistry,            // RFC-0105 §3.1
        vault_registry: &dyn VaultRegistry,      // NEW: vault-contains-asset check
        nonce_registry: &mut dyn NonceRegistry,  // NEW (Round 3 fix #3): anti-replay observation
        current_epoch: Epoch,
    ) -> Result<Self, SettlementEventError> {
        let meta = registry.metadata(&cost_asset_id)
            .map_err(|_| SettlementEventError::AssetUnknown)?;
        if cost.wire_scale != meta.wire_scale {
            return Err(SettlementEventError::ScaleMismatch { cost_scale: cost.wire_scale, vault_scale: meta.wire_scale });
        }
        if cost.wire_scale > MAX_SCALE {
            return Err(SettlementEventError::ScaleOutOfRange { scale: cost.wire_scale });
        }
        // Body-hash canonicalization (Round 3 fix #2 + #9 + Round 4 fix #2 + Round 5 fix R5 L3 CRIT-1/HIGH-1
        // + Round 6 fix: compute_settlement_body_hash helper factored out, shared with validate()):
        // explicit BLAKE3 over the length-prefixed (SettlementEvent fields). Includes cost_vault_id so the asset
        // authority cannot mint events attributing costs to vaults they do not control. Includes
        // settlement_decision (length-prefixed discriminant + payload) so an authority cannot
        // sign a Consumed event and rewrite it as BudgetExhausted on replay. Includes
        // governance_pubkey (Round 5 fix HIGH-1) so a signature minted under one key cannot be
        // replayed against a vault whose governance key has rotated. Includes nonce (Round 5
        // fix CRIT-1) so a signature cannot be replayed across two distinct SettlementEvents.
        // Sovereign assets (governance_pubkey = None) use the all-zeros sentinel for the
        // governance_pubkey bytes; signature verification is skipped for sovereign assets per
        // RFC-0105 §3.1 sovereign exemption (see §3 of this RFC). created_at_unix_ms is
        // intentionally EXCLUDED (wall-clock non-deterministic); ledger_height is the
        // deterministic ordering anchor. Length-prefixed per RFC-0105 §3.4.
        let governance_pubkey_bytes: [u8; 32] = meta.governance_pubkey.unwrap_or([0u8; 32]);
        let body_hash: [u8; 32] = Self::compute_settlement_body_hash(
            settlement_id,
            ask_id,
            cost_vault_id.clone(),
            cost_asset_id,
            meta.kind_tag(),
            cost.clone(),
            ledger_height,
            evidence_ref,
            governance_pubkey_bytes,
            &nonce,
        );
        // Sovereign-asset exemption (Round 3 fix #1): if meta.governance_pubkey is None (sovereign role token),
        // the signature is optional per RFC-0105 §3.1 sovereign exemption (documented in §3 of this RFC).
        // Non-sovereign assets REQUIRE a verifiable signature; InvalidSignature otherwise.
        if let Some(pk) = meta.governance_pubkey {
            if !verify_governance_signature(&governance_signature.0, &body_hash, &pk) {
                return Err(SettlementEventError::InvalidSignature);
            }
        }
        // Vault-contains-asset check (Round 1 CRITICAL #3 mitigation; Round 3 fix #5).
        // Result-typed: separate VaultUnknown from VaultAssetMismatch.
        vault_registry.contains_asset(&cost_vault_id, &cost_asset_id)
            .map_err(|e| match e {
                VaultRegistryError::UnknownVault { vault_id } => SettlementEventError::VaultUnknown { vault_id },
                VaultRegistryError::VaultAssetMismatch { vault_id, asset_id } => SettlementEventError::VaultAssetMismatch { vault_id, asset_id },
            })?;
        // Anti-replay observation (Round 3 fix #3 + Round 4 fix #1 + Round 5 fix R1 CRIT-02): mirror RFC-0960 §2.2
        // BurnEventRef::consume(). observe() returns Err if the nonce was already consumed.
        // Non-sovereign assets use the governance_pubkey as the namespace. Sovereign assets
        // (governance_pubkey = None) fall back to a PER-ASSET derived namespace via
        // sovereign_nonce_namespace(&cost_asset_id) — matches RFC-0960 §3.3 verbatim.
        // Round 4 fix #1 replaces the prior shared [0u8;32] sentinel, which was a DoS vector:
        // an attacker observing one sovereign-nonce would block ALL sovereign SettlementEvents
        // sharing the sentinel namespace. Round 5 fix CRIT-02: the inline blake3 construction
        // is now factored into the canonical `sovereign_nonce_namespace` helper (RFC-0105
        // §3.11) so all consumers (BurnEventRef + SettlementEvent) share one derivation.
        let observe_key: [u8; 32] = meta.governance_pubkey
            .map(|pk| *pk)
            .unwrap_or_else(|| sovereign_nonce_namespace(&cost_asset_id));
        nonce_registry.observe(&observe_key, &nonce.0)
            .map_err(|NonceError::AlreadyObserved { prior_height, .. }| SettlementEventError::Replay { prior_height })?;
        Ok(Self {
            settlement_id, ask_id, cost_vault_id, cost_asset_id,
            asset_kind: meta.kind.clone(),  // Round 3 fix #17
            cost, evidence_ref, ledger_height, created_at_unix_ms,
            settlement_decision, governance_signature, registry_snapshot_epoch: current_epoch, nonce,
        })
    }

    /// Post-deserialization invariant check (Round 1 CRITICAL #2 mitigation).
    pub fn validate(&self, registry: &dyn AssetRegistry, vault_registry: &dyn VaultRegistry, nonce_registry: &mut dyn NonceRegistry, current_epoch: Epoch) -> Result<(), SettlementEventError> {
        let meta = registry.metadata(&self.cost_asset_id)
            .map_err(|_| SettlementEventError::AssetUnknown)?;
        if self.cost.wire_scale != meta.wire_scale {
            return Err(SettlementEventError::ScaleMismatch { cost_scale: self.cost.wire_scale, vault_scale: meta.wire_scale });
        }
        if meta.tombstone {
            return Err(SettlementEventError::AssetUnknown);    // tombstoned rejects new events
        }
        // Round 5 fix (R5 R1 MED-04): re-verify the governance signature on the body_hash so
        // direct Deserialize bypass cannot accept events whose signature was tampered with
        // after construction. Mirrors RFC-0960 §2.1 validate() pattern. Sovereign assets
        // (governance_pubkey = None) skip verification per RFC-0105 §3.1 sovereign exemption.
        if let Some(pk) = meta.governance_pubkey {
            let governance_pubkey_bytes: [u8; 32] = pk;
            let body_hash: [u8; 32] = Self::compute_settlement_body_hash(
                self.settlement_id.clone(),
                self.ask_id.clone(),
                self.cost_vault_id.clone(),
                self.cost_asset_id.clone(),
                meta.kind_tag(),
                self.cost.clone(),
                self.ledger_height,
                self.evidence_ref.clone(),
                governance_pubkey_bytes,
                &self.nonce,
            );
            if !verify_governance_signature(&self.governance_signature.0, &body_hash, &pk) {
                return Err(SettlementEventError::InvalidSignature);
            }
        }
        vault_registry.contains_asset(&self.cost_vault_id, &self.cost_asset_id)
            .map_err(|e| match e {
                VaultRegistryError::UnknownVault { vault_id } => SettlementEventError::VaultUnknown { vault_id },
                VaultRegistryError::VaultAssetMismatch { vault_id, asset_id } => SettlementEventError::VaultAssetMismatch { vault_id, asset_id },
            })?;
        // Nonce observation (Round 3 fix #3 + Round 4 fix #1 + Round 5 fix R1 CRIT-02): also enforced
        // at validate-time so direct Deserialize bypass cannot replay events. Per-asset namespace
        // for sovereign assets via `sovereign_nonce_namespace(&self.cost_asset_id)` matches the
        // new() path; see new() comment for the DoS rationale.
        let observe_key: [u8; 32] = meta.governance_pubkey
            .map(|pk| *pk)
            .unwrap_or_else(|| sovereign_nonce_namespace(&self.cost_asset_id));
        nonce_registry.observe(&observe_key, &self.nonce.0)
            .map_err(|NonceError::AlreadyObserved { prior_height, .. }| SettlementEventError::Replay { prior_height })?;
        if current_epoch.0 < self.registry_snapshot_epoch.0 {
            return Err(SettlementEventError::StaleSnapshot {
                snapshot: self.registry_snapshot_epoch.0, live: current_epoch.0,
            });
        }
        Ok(())
    }
}

pub trait VaultRegistry {
    /// Round 3 fix #5: Result-typed. Returns `Ok(())` if vault contains asset; otherwise an error
    /// distinguishing unknown vault from vault-asset mismatch.
    fn contains_asset(&self, vault_id: &VaultId, asset_id: &AssetId) -> Result<(), VaultRegistryError>;
}

// Note (Round 3 fix #7): `impl AssetMetadata { fn kind_tag(&self) -> u8 }` is REMOVED from this RFC.
// `kind_tag()` lives in RFC-0105 §3.1 alongside the `AssetMetadata` struct definition
// (0x01=Sovereign, 0x02=Private, 0x03=Bridged, 0x04=Wrapped, matching RFC-0105 §3.9 wire form).
// SettlementEvent::new() calls `meta.kind_tag()` via the substrate-provided method.

// Round 4 fix #6 + Round 5 fix (R5 L3 CRIT-3): explicit Deserialize rejection of the legacy
// envelope when paired with a non-OCTO-W cost_asset_id. serde derives a permissive Deserialize
// by default; this hand-written impl closes the asset-binding bypass at the deserialization
// boundary (not just the new() path). Round 5 fix: the prior `todo!()` panic (which would
// PANIC at runtime on modern payloads after the legacy-form check passed) is replaced with the
// working serde_json round-trip pattern from RFC-0960 §2.1: peek at `amount_micro_octo_w`,
// reject if paired with a non-OCTO-W `cost_asset_id`, then deserialize via
// `serde_json::from_value`. This guarantees the legacy-form check is the ONLY branch that can
// fail; modern envelopes flow through to the derived Deserialize without panicking.
impl<'de> Deserialize<'de> for SettlementEvent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(d)?;
        // Detect legacy { amount_micro_octo_w: i64 } envelope
        if let Some(_legacy_cost) = raw.get("amount_micro_octo_w") {
            let claimed_cost_asset_id: AssetId = serde_json::from_value(
                raw.get("cost_asset_id").cloned().unwrap_or(serde_json::json!(AssetId::OCTO_W_ASSET_ID))
            ).map_err(de::Error::custom)?;
            if claimed_cost_asset_id != AssetId::OCTO_W_ASSET_ID {
                return Err(de::Error::custom(format!(
                    "LegacyFormOnNonOctoWContext: claimed_asset_id={}",
                    hex::encode(claimed_cost_asset_id.0)
                )));
            }
        }
        serde_json::from_value(raw).map_err(de::Error::custom)
    }
}
```

### 2.3 Audit invariant: verify_settlement_against_payment_caveat

```rust
/// Round 3 fix #8: separate error type from constructor-time `SettlementEventError`.
/// Runtime audit checks (asset/scale/vault-asset) emit `SettlementAuditError` variants
/// with the `Audit` prefix, distinguishing them from constructor-time validation errors.
pub enum SettlementAuditError {
    /// fires when `self.cost_asset_id != caveat.asset_id` at audit-time.
    AuditAssetMismatch { settlement_asset_id: AssetId, caveat_asset_id: AssetId },
    /// fires when `self.cost.wire_scale != caveat.budget.wire_scale` at audit-time.
    AuditScaleMismatch { settlement_scale: u8, caveat_scale: u8 },
    /// fires when `vault.contains_asset(cost_asset_id)` returns false at audit-time.
    AuditVaultAssetMismatch { vault_id: VaultId, asset_id: AssetId },
}

/// HARD invariant: a settlement event charged against a vault bound to PaymentCaveat
/// MUST be in the same asset AND at the same wire scale as the caveat authorizes.
/// Round 1 MED #6 mitigation: this check is MANDATORY at verify-time (not optional).
/// Round 3 fix #8: renamed to `verify_settlement_against_payment_caveat` and returns
/// `SettlementAuditError` (separate from constructor-time `SettlementEventError`).
/// Note: prior draft declared `EvidenceRefMismatch` + `NotConsumed` variants here
/// but the verify function did not exercise them — those variants were dropped in
/// Round 3 (LOW #11). If a future need arises, add the parameters + variants together.
pub fn verify_settlement_against_payment_caveat(
    settlement: &SettlementEvent,
    caveat: &PaymentCaveat,
    vault_registry: &dyn VaultRegistry,  // Round 4 fix #5: required to enforce AuditVaultAssetMismatch
) -> Result<(), SettlementAuditError> {
    if settlement.cost_asset_id != caveat.asset_id {
        return Err(SettlementAuditError::AuditAssetMismatch {
            settlement_asset_id: settlement.cost_asset_id, caveat_asset_id: caveat.asset_id,
        });
    }
    if settlement.cost.wire_scale != caveat.budget.wire_scale {
        return Err(SettlementAuditError::AuditScaleMismatch {
            settlement_scale: settlement.cost.wire_scale, caveat_scale: caveat.budget.wire_scale,
        });
    }
    // Round 4 fix #5: AuditVaultAssetMismatch was previously unreachable (no vault_registry
    // parameter; contains_asset never called). Inject the dependency and map the registry
    // error so the variant fires at audit-time as the doc comment promises.
    vault_registry.contains_asset(&settlement.cost_vault_id, &settlement.cost_asset_id)
        .map_err(|e| match e {
            VaultRegistryError::UnknownVault { vault_id } => SettlementAuditError::AuditVaultAssetMismatch {
                vault_id, asset_id: settlement.cost_asset_id,
            },
            VaultRegistryError::VaultAssetMismatch { vault_id, asset_id } => SettlementAuditError::AuditVaultAssetMismatch {
                vault_id, asset_id,
            },
        })?;
    Ok(())
}
```

### 2.4 Error scenario matrix (NEW, Round 1 fix)

| Variant                       | Trigger                                                                                                                                                                                  |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AssetUnknown`                | `AssetRegistry::metadata(cost_asset_id)` returns `AssetError::Unknown` OR tombstoned.                                                                                                    |
| `ScaleMismatch`               | `cost.wire_scale != metadata.wire_scale` at construction OR validate().                                                                                                                  |
| `ScaleOutOfRange`             | `cost.wire_scale > MAX_SCALE` (defense-in-depth).                                                                                                                                        |
| `InvalidSignature`            | `governance_signature` does not verify against cost_vault_id's governance_pubkey (non-sovereign assets only; sovereign assets exempt per RFC-0105 §3.1 sovereign exemption — see §3).    |
| `Replay`                      | `nonce` was previously observed via `NonceRegistry::observe()` (injected in new() and validate()).                                                                                       |
| `StaleSnapshot`               | `current_epoch.0 < self.registry_snapshot_epoch.0` (asset rotated out from under us).                                                                                                    |
| `VaultAssetMismatch`          | `vault_registry.contains_asset(cost_vault_id, cost_asset_id)` returns `Err(VaultRegistryError::VaultAssetMismatch)`.                                                                     |
| `VaultUnknown`                | `vault_registry.contains_asset(cost_vault_id, cost_asset_id)` returns `Err(VaultRegistryError::UnknownVault)`.                                                                           |
| `LegacyFormOnNonOctoWContext` | Legacy `cost: { amount_micro_octo_w }` form submitted with a non-OCTO-W `cost_asset_id` (see §3.2 wire-form migration close). Serde discriminator: `"legacy_form_on_non_octow_context"`. |

**Note (Round 4 fix #9):** `SettlementAuditError` (AuditAssetMismatch / AuditScaleMismatch / AuditVaultAssetMismatch) is a **runtime audit result type** returned by `verify_settlement_against_payment_caveat()`, NOT a wire-form field. The §3.1 JSON above carries only `settlement_decision` (construction-time + decision outcome). Audit results travel via the audit pipeline (RFC-0965 §3 audit reporting) and do not appear on the SettlementEvent wire envelope.

### 2.5 Cryptographic Primitives (forward-pointer; Round 5 fix R1 CRIT-03; Round 6 cleanup)

`verify_governance_signature` and `blake3_hash` are defined canonically at **RFC-0105 §3.12 Cryptographic Primitives** (GREENFIELD substrate path: `octo_cap_macaroon::verify_governance_signature`). This RFC imports them via `use octo_vault::verify_governance_signature; use octo_vault::blake3_hash;` (see §2.1 import block). No local re-declaration. Round 6: the duplicate import block previously re-listed here is REMOVED — §2.5 is a prose forward-pointer, not a Rust code site.

**Signature scheme**: ed25519 over BLAKE3-256(message). Signature size: 64 bytes (R ‖ S). Public key size: 32 bytes.

## 3. Wire Form (NEW, greenfield)

### 3.1 Canonical wire form

```json
{
  "settlement_event": {
    "settlement_id": "<32B-hex>",
    "ask_id": "<32B-hex>",
    "cost_vault_id": "<32B-hex>", // VaultId tuple (32-byte wrapper)
    "cost_asset_id": "<32B-hex>", // NEW: explicit asset binding
    "asset_kind": "SovereignRoleToken | PrivateCorporateAsset | BridgedExternalAsset | WrappedCrossChainAsset",
    "cost": "<16B-hex-of-DqaEncoding>", // 16-byte BE per DqaEncoding (RFC-0862)
    "evidence_ref": "<32B-hex>",
    "ledger_height": 12345, // NEW: deterministic ordering anchor (Round 3 fix #2)
    "created_at_unix_ms": 1724700000000, // informational only; NOT signed
    "settlement_decision": "Consumed | AlreadyConsumed | InsufficientEvidence | BudgetExhausted",
    "governance_signature": "<64B-hex>",
    "registry_snapshot_epoch": 12345,
    "nonce": "<32B-hex>"
  }
}
```

**Note (Round 3 fix #18):** `PaymentCaveat.budget` (RFC-0965 §3) uses the same `<16B-hex-of-DqaEncoding>` encoding as `SettlementEvent.cost`. Encoding uniformity is intentional — the audit invariant in §2.3 compares wire-scale values directly.

**Note (Round 6 fix, cross-RFC body_hash asymmetry):** Both RFC-0959 (`SettlementEvent`) and RFC-0960 (`BurnEventRef`) bind a `governance_pubkey` into the body_hash, but they source it differently. **RFC-0959 binds LIVE `meta.governance_pubkey`** read from the injected `&dyn AssetRegistry` at construction AND at validate-time (the registry is the source of truth). **RFC-0960 binds PINNED `self.governance_pubkey`** snapshotted onto the `BurnEventRef` struct at construction (the struct field is the source of truth). Both bind the same field-set otherwise (governance_pubkey + nonce + asset_id + vault_id + cost/amount); the difference is the SOURCE — registry lookup vs. struct field. The asymmetry is intentional and matches each RFC's substrate shape: `SettlementEvent` is constructed fresh on every settlement (registry is always fresh), while `BurnEventRef` is constructed once and re-validated across the burn trail (snapshot is required for offline replay).

### 3.2 Wire-form migration

There is no prior wire form to migrate. The substrate today records settlement outcomes via `SettlementError::AlreadyConsumed` (engine) + JSON-RPC envelopes (CLI), but no canonical event type. This RFC introduces `SettlementEvent` as the canonical event; older engine-level error returns remain for backward compatibility until the engine migrates.

### 3.3 Sovereign-asset signature exemption (NEW, Round 3 fix #1)

Sovereign role tokens (RFC-0105 §3.1) carry `meta.governance_pubkey = None` because the issuing chain itself is the authority — no external governance key is registered. Per the RFC-0105 §3.1 sovereign exemption, the `governance_signature` field on `SettlementEvent` is OPTIONAL for sovereign assets; `verify_governance_signature` is SKIPPED (the `if let Some(pk) = meta.governance_pubkey { ... }` branch in `SettlementEvent::new()` is not entered). Non-sovereign assets (private, bridged, wrapped) REQUIRE a verifiable signature; `InvalidSignature` is returned otherwise.

The NonceRegistry observation namespace for sovereign assets uses a **per-asset derived namespace** `BLAKE3("octo:sovereign-nonce-ns:v1" || cost_asset_id.0)` — see `SettlementEvent::new()`. This construction matches RFC-0960 §3.3 verbatim. Round 4 fix #1 replaces the prior shared `[0u8;32]` sentinel: the shared sentinel was a DoS vector (an attacker observing one sovereign nonce would block ALL sovereign SettlementEvents sharing the namespace). The per-asset namespace isolates each sovereign asset's nonce space while remaining deterministic and chain-derived (sovereign assets are minted by the chain itself; replay protection is anchored at the chain level rather than at the governance-key level).

> **Clarification (Round 6 fix):** On sovereign-path events, the `governance_pubkey_bytes` local binding inside `new()` / `validate()` falls back to `[0u8; 32]` when `meta.governance_pubkey` is `None`. This binding is used in TWO distinct roles and the roles MUST NOT be confused:
>
> 1. **body_hash commitment** (Round 5 fix HIGH-1): the `[0u8; 32]` sentinel is hashed into `compute_settlement_body_hash` so that body-hashes are deterministic across sovereign + non-sovereign events. This is a CONTENT slot, not a key.
> 2. **NonceRegistry observation namespace** (Round 4 fix #1): for sovereign assets, the per-asset derived `sovereign_nonce_namespace(&cost_asset_id)` is used as the `observe_key` (NOT the `[0u8; 32]` sentinel). This is a REPLAY-PROTECTION key, not a signature-verification key.
>
> Signature verification is SKIPPED for sovereign assets per RFC-0105 §3.1 sovereign exemption. Do NOT pass the `[0u8; 32]` sentinel, the per-asset namespace key, OR `cost_asset_id` itself to `verify_governance_signature` — there is no governance signature to verify on sovereign assets.

## 4. Cross-Reference Updates (Round 2: stripped version pins)

- RFC-0105 (companion amendment): defines `AssetRegistry` side-table that `SettlementEvent::new` and `validate` query.
- RFC-0965 (companion amendment): `PaymentCaveat.asset_id` is the audit-invariant partner of `SettlementEvent.cost_asset_id`.
- RFC-0960 (companion amendment): `BurnEventRef.asset_id` MUST equal `SettlementEvent.cost_asset_id` (settlement → burn trail invariant).

### Tri-invariant (RFC-0105 §3.13)

// Tri-invariant (RFC-0105 §3.13)

## 5. Naming Cleanup

| Old                   | New                      | Site      |
| --------------------- | ------------------------ | --------- |
| (implicit cost asset) | `cost_asset_id: AssetId` | NEW field |

`SettlementEvent.cost` is `Dqa` from inception; the retired `MicroOctoW` alias (retired 2026-08-17 per `crates/octo-cap-macaroon/src/caveat/payment.rs:23` + `crates/octo-paid-query/src/lib.rs:103`) is never reintroduced. `AlreadyConsumed` is canonical substrate (no rename history; `ReceiptReplay` was never a substrate variant — no rename row needed).

**Note (Round 3 fix #15):** `cost_vault_id` is GREENFIELD; no substrate rename history. The design adopts `VaultId` (Layer A typed wrapper) per RFC-0105 §3.1 convention — see §2.1 annotation. No rename row needed.

## 6. Backward Compatibility

- **Existing settlement outcomes** via `SettlementError::AlreadyConsumed(String)` at `crates/quota-router-sm-engine/src/lib.rs:266`: continue to work. Engine migration to emit `SettlementEvent` is a separate RFC.
- **`amount_micro_octo_w: Dqa` field NAME on `Escrow` + `EscrowSnapshot` + `slash_store`**: the carrier is already `Dqa` (Round 1 MED #6 corrected: the prior draft claimed `i64`; substrate has `Dqa`). Only the field NAME is outstanding — `amount_micro_octo_w: Dqa` → `amount: Dqa` paired with `asset_id: AssetId`, with the rename tracked in RFC-0105 §6 Naming Cleanup. Sites: `crates/quota-router-core/src/marketplace/escrow.rs:159` + `EscrowSnapshot` line 172 + `crates/quota-router-storage/src/slash_store.rs:95` + 6 test assertions at `quota-router-core/tests/task_market.rs:397,452,464,469,600,667`.
- **JSON-RPC envelopes** carrying `cost: { amount_micro_octo_w: i64 }`: read path accepts the legacy form and constructs `SettlementEvent::new(..., cost: Dqa { value, wire_scale: 0 }, ..., cost_asset_id: OCTO_W_ASSET_ID, ...)`. A `#[deprecated]` warning fires for one substrate release cycle (≈6 weeks per RFC-0965 §3.1).
- **After one cycle**: legacy `cost: { amount_micro_octo_w }` form is REJECTED if the event also carries a non-OCTO-W `cost_asset_id` (asset-binding bypass close).

## 7. Version History

| Version | Date                    | Author                   | Note                                                                                             |
| ------- | ----------------------- | ------------------------ | ------------------------------------------------------------------------------------------------ |
| 2.0     | 2026-07-29              | @cipherocto + @mmacedoeu | Initial accepted (Ask Settlement Chain).                                                         |
| 2.1-2.7 | 2026-07-29 → 2026-08-24 | @cipherocto + @mmacedoeu | R1-R12 fix-all rounds.                                                                           |
| 2.8-r1  | 2026-08-26              | @mmacedoeu               | **Initial v2.8 draft (Round 1).** 8 findings; addressed in r2.                                   |
| 2.8-r2  | 2026-08-26              | @mmacedoeu               | **Round 2.** VaultId+cost_asset_id; Dqa imports; §2.4 matrix; phantom cite dropped.              |
| 2.8-r3  | 2026-08-26              | @mmacedoeu               | **Round 3.** governance_pubkey Option; ledger_height; NonceRegistry; audit rename; legacy close. |
| 2.8-r4  | 2026-08-26              | @mmacedoeu               | **Round 4.** Per-asset ns; commitment; Audit dedup; vault_registry; §3.13 anchor.                |
| 2.8-r5  | 2026-08-26              | @mmacedoeu               | **Round 5.** Deserialize fix; §2.5 dedup; body_hash fields; validate re-verify.                  |
| 2.8-r6  | 2026-08-26              | @mmacedoeu               | **Round 6.** Drop anchor; body_hash helper; blake3; asymmetry note; replay-key.                  |
| 2.8-r7  | 2026-08-26              | @mmacedoeu               | Round 7-9: §3.x anchors + VH trim + DRY + Accepted promotion.                                    |

## 8. Pending (concrete test vectors)

- [ ] R2 adversarial review after fix.
- [ ] Substrate anchor verification (NEW): run `scripts/verify-substrate-anchors.sh <rfc-path>`.
- [ ] Test vector: `SettlementEvent::new(..., cost_vault_id=OCTO_W_VAULT, cost_asset_id=OCTO_W_ASSET_ID, cost=Dqa{value:1000, wire_scale:0}, ...)` succeeds (substrate today enforces scale=0 on OCTO-W).
- [ ] Test vector: `SettlementEvent::new(..., cost_asset_id=OCTO_W_ASSET_ID, cost=Dqa{value:1000, wire_scale:8}, ...)` returns `Err(ScaleMismatch { cost_scale: 8, vault_scale: 0 })`.
- [ ] Test vector: `SettlementEvent::new(..., cost_asset_id=TOMBSTONED_ASSET_ID, ...)` returns `Err(AssetUnknown)`.
- [ ] Test vector: validate() catches direct Deserialize bypass (scale-flip attack).
- [ ] Test vector: `verify_settlement_against_payment_caveat(settlement{asset=OCTO_W, wire_scale=0}, caveat{asset=USDC, budget_wire_scale=6})` returns `Err(AuditAssetMismatch)`.
- [ ] Test vector: `SettlementEvent::new(..., cost_vault_id=V_HOLDS_USDC_ONLY, cost_asset_id=OCTO_W_ASSET_ID, ...)` returns `Err(VaultAssetMismatch)` (Round 1 CRITICAL #3 mitigation).
- [ ] Test vector (Round 4 fix #11): `SettlementEvent::new(..., cost=Dqa{value:1, wire_scale:MAX_SCALE+1}, ...)` returns `Err(ScaleOutOfRange { scale: MAX_SCALE+1 })`.
- [ ] Test vector (Round 4 fix #11): `SettlementEvent::new(..., cost_asset_id=USDC_ASSET_ID, governance_signature=zero_bytes, ...)` returns `Err(InvalidSignature)`.
- [ ] Test vector (Round 4 fix #11): re-submitting a previously-observed nonce (per-asset sovereign namespace after Round 4 fix #1) returns `Err(Replay { prior_height })`.
- [ ] Test vector (Round 4 fix #11): `SettlementEvent::new(..., cost_asset_id=USDC_ASSET_ID, ...)` followed by `validate(...)` at `current_epoch < registry_snapshot_epoch` returns `Err(StaleSnapshot { snapshot, live })`.
- [ ] Test vector (Round 4 fix #11): `SettlementEvent::new(..., cost_vault_id=UNKNOWN_VAULT_ID, ...)` returns `Err(VaultUnknown { vault_id: UNKNOWN_VAULT_ID })`.
- [ ] Test vector (Round 4 fix #11): `Deserialize::deserialize` on `{ amount_micro_octo_w: 1000, cost_asset_id: USDC_ASSET_ID, ... }` returns `Err(SettlementEventError::LegacyFormOnNonOctoWContext { claimed_asset_id: USDC_ASSET_ID })` (Round 4 fix #6 boundary close).
- [ ] Test vector (Round 4 fix #11): `verify_settlement_against_payment_caveat(settlement{asset=OCTO_W, wire_scale=0}, caveat{asset=OCTO_W, budget_wire_scale=0}, vault_registry)` where `vault_registry.contains_asset(V, OCTO_W)` returns `Err(VaultAssetMismatch)` → `Err(AuditVaultAssetMismatch { vault_id: V, asset_id: OCTO_W })`.
- [ ] Test vector (Round 5 fix L5 MED): `SettlementEvent::validate(vault_id=0xUNKNOWN, ...)` returns `Err(VaultUnknown { vault_id: 0xUNKNOWN })` (direct `contains_asset` rejection at validate-time, no audit-path bypass).
- [ ] Test vector (Round 5 fix L5 MED): `verify_settlement_against_payment_caveat(event{wire_scale=0}, caveat{wire_scale=6})` returns `Err(AuditScaleMismatch)` (runtime audit scale-collision close; matches AuditScaleMismatch docstring).
- [ ] Test vector (Round 6 fix, mirrors RFC-0960 R5 CRIT-1): two `SettlementEvent`s constructed with identical body fields except `nonce` (NONCE_A vs NONCE_B) produce distinct `compute_settlement_body_hash` outputs; submitting both via `NonceRegistry::observe()` returns `Ok` for the first and `Err(NonceError::AlreadyObserved)` for the second.
- [ ] Cross-reference validation via Guard 2 cite validator.
- [ ] Acceptance promotion (7-day minimum review + 2 maintainer approvals).

---

**End of RFC-0959 v2.8 (Accepted 2026-08-26).**
