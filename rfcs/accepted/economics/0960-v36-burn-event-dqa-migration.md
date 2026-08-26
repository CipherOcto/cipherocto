---
rfc: 0960-v3.6
title: BurnEventRef Dqa Migration + Asset-Binding Audit Invariant (Greenfield BurnEventRef)
status: Accepted
version: 3.6
date: 2026-08-26
amends: RFC-0960 v3.5
builds_on:
  - rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md
  - rfcs/draft/economics/0105-v35-payment-caveat-asset-generality.md
---

# RFC-0960 v3.6 — BurnEventRef Dqa Migration + Asset-Binding Audit Invariant

## 0. Status

**Accepted (v3.6, 2026-08-26).** Amendment to RFC-0960 v3.5. Round 6 review fixes applied (revision r7).

**Promotion trail:** R1-R9 multi-round adversarial review 2026-08-25 → DRY closure at R9 2026-08-26 → Accepted 2026-08-26 per BLUEPRINT.md RFC process. R1-R9 = 5-lens reviews; loop-until-DRY pattern reached 2 consecutive zero-finding rounds (R8=2 LOW, R9=0) per closure audit docs/audits/asset-generic-payment-caveat-review-DRY-2026-08-26.md.

**Substrate anchor (Round 2 corrected):** **No `BurnEventRef` struct exists in substrate today.** The Round 1 review confirmed this via grep across `crates/octo-policy/` and other crates. This RFC is **GREENFIELD** — it introduces `BurnEventRef` as a new Layer B type. The drift sites at `crates/octo-policy/src/policy_kinds.rs:263` (SelectorContext.amount_dqa_micros), `crates/octo-policy/src/workflow_kind.rs:313,461,477` (test fixtures + ByAmountSelector), and `rfcs/accepted/process/0206-v30-value-transfer-surface.md:65,73,93,101,110,121` (wire-form draft) are upstream entities that motivate this RFC but are NOT the BurnEventRef itself.

`Dqa` lives in `octo_determin` (path: `./determin/`); import path is `octo_determin::Dqa`.

**Shared-substrate anchors (Round 3 fix #5/#6/#13):** `NonceRegistry` lives at **RFC-0105 §3.11 (GREENFIELD)**; RFC-0960 §2.2 + RFC-0959 §2.2 **import** from this anchor per the single-source-of-truth rule — neither RFC re-declares the trait. The generic newtypes `Nonce`, `Epoch`, and `GovernanceSignature` share that same anchor. Only **domain-specific** wrappers (`SettlementId`, `EvidenceRef`, …) are declared locally in this RFC.

## 1. Motivation

The substrate drift identified in Round 1:

- `amount_dqa_micros: i64` at `crates/octo-policy/src/policy_kinds.rs:263` (on `SelectorContext`) is an i64 carrier with implicit wire scale=0.
- 3 sites at `crates/octo-policy/src/workflow_kind.rs:313,461,477` carry the same field on test/selector machinery.
- 6 sites at `rfcs/accepted/process/0206-v30-value-transfer-surface.md:65,73,93,101,110,121` document wire-form occurrences.

None of these is `BurnEventRef`. The substrate has no canonical "burn event" type today. This amendment introduces `BurnEventRef` (greenfield, Layer B additive, semver-minor) and binds it to the asset-binding chain via `AssetRegistry` (RFC-0105 §3.1).

## 2. BurnEventRef Specification

### 2.1 Substrate definition (NEW, greenfield)

> Forward-reference: see RFC-0960 §2 Specification for the vault-balance projection substrate; the `BurnEventProducer` in §2.5 wraps `BurnEventRef::consume` and emits `VaultProjectionInvalidationEnvelope` after nonce observation.

```rust
// crates/octo-policy/src/burn_event.rs (NEW file)

use borsh::{BorshDeserialize, BorshSerialize};
use octo_determin::{dqa_cmp, Dqa};
use serde::{Deserialize, Serialize};

// Single-source-of-truth: AssetKind / AssetRegistry / AssetError / MAX_SCALE are defined ONCE in
// octo-vault (RFC-0105 §3.1). Consumer crates MUST import, never re-declare.

// GREENFIELD substrate paths (canonical homes cited concretely per RFC-0105):
use octo_vault::newtypes::{Nonce, Epoch, GovernanceSignature};  // §3.11 typed newtypes
use octo_vault::asset_registry::{AssetError, AssetKind, AssetMetadata, AssetRegistry, MAX_SCALE};  // §3.1
use octo_vault::nonce_registry::{NonceRegistry, NonceError};  // §3.11
use octo_vault::bridge_chain_namespace::BridgeChainNamespace;  // §2.1 (Bridged external asset namespace)
use octo_vault::sovereign_nonce_namespace;  // §3.11 (helper)
use octo_vault::verify_governance_signature;  // §3.12 Cryptographic Primitives
use octo_vault::blake3_hash;  // §3.12 Cryptographic Primitives — canonical BLAKE3-256 -> [u8; 32]

// Additional substrate paths (typed wrappers + vault↔asset containment; RFC-0105 §3.1):
use octo_vault::{AssetId, ChainId, VaultId};
use octo_vault::vault_registry::VaultRegistry;  // Round 3 IMPORTANT #2 — symmetrized with RFC-0959 §2.2

/// BurnEventRef — immutable audit record of a vault asset burn.
///
/// Carries `amount: Dqa` (post-migration) bound to a specific `asset_id` via
/// `AssetRegistry`. Signature on the event is signed by the vault's governance
/// key per RFC-0105 §3.3 (governance rotation + revocation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, BorshSerialize, BorshDeserialize)]
pub struct BurnEventRef {
    pub chain_id: ChainId,                    // typed wrapper (Layer A; RFC-0105 §3.1 substrate convention)
    pub vault_id: VaultId,                    // typed wrapper (Layer A frozen tuple struct)
    pub asset_id: AssetId,
    pub asset_kind: AssetKind,                // RFC-0105 §3.1 — bundled for offline audit
    pub amount: Dqa,
    pub ledger_height: u64,
    pub settlement_event_ref: SettlementId,   // typed [u8; 32] wrapper (Layer B additive; RFC-0959 §2.1 naming)
    pub governance_signature: GovernanceSignature,  // typed [u8; 64] wrapper (RFC-0105 §3.11 newtypes anchor)
    pub governance_pubkey: [u8; 32],          // Round 3 CRITICAL #1: THE NonceRegistry key. Parallel to
                                              // `AssetMetadata.governance_pubkey` (RFC-0105 §3.1). Pinned at
                                              // construction so `consume()` never derives a replay key from
                                              // signature bytes (a signature prefix is attacker-malleable).
    pub registry_snapshot_epoch: Epoch,        // typed u64 wrapper (RFC-0105 §3.11 newtypes anchor)
    pub nonce: Nonce,                          // typed [u8; 32] wrapper (RFC-0105 §3.11 newtypes anchor; consumed via NonceRegistry §3.11)
}

pub enum BurnEventError {
    /// fires when `AssetRegistry::metadata(asset_id)` returns `AssetError::Unknown`
    /// OR the asset is tombstoned (revoked but historical events still resolve).
    AssetUnknown,
    /// fires when `amount.wire_scale != metadata.wire_scale`.
    ScaleMismatch { amount_wire_scale: u8, asset_wire_scale: u8 },
    /// fires when `amount.wire_scale > MAX_SCALE` (defense-in-depth).
    ScaleOutOfRange { scale: u8 },
    /// fires when `governance_signature` does not verify against `metadata.governance_pubkey`.
    InvalidSignature,
    /// fires when the nonce was previously observed in the substrate NonceRegistry (replay attempt).
    Replay { prior_height: u64 },
    /// fires when the live registry epoch < `self.registry_snapshot_epoch` (asset rotated out from under us).
    StaleSnapshot { snapshot: u64, live: u64 },
    /// fires when a stored `asset_kind` field on the deserialized BurnEventRef does NOT match the
    /// `AssetRegistry::metadata(asset_id).kind` lookup (Round 1 IMPORTANT #9 mitigation:
    /// forger mutation must not bypass kind-specific routing).
    AssetKindMismatch { claimed: Box<AssetKind>, registered: Box<AssetKind> },
    /// fires when the vault_registry.contains_asset() returns VaultRegistryError::UnknownVault — vault_id not present in the registry.
    VaultUnknown { vault_id: VaultId },
    /// fires when `VaultRegistry::contains_asset(vault_id, asset_id)` returns false — i.e. the burn
    /// claims to charge an asset the vault never held (Round 3 IMPORTANT #2; symmetrized with
    /// RFC-0959 §2.2 which performs the identical containment check on `SettlementEvent`).
    VaultAssetMismatch { vault_id: VaultId, asset_id: AssetId },
    /// fires when `AuditSink::write` returns `Err(AuditError)` during `consume()` — i.e. at
    /// audit-time sink write, AFTER validate() has passed and AFTER the nonce was observed.
    /// This variant is **BurnEvent-scoped**: it is the only tri-invariant error whose trigger is
    /// the consume-path audit sink, so it has no counterpart in the settlement-side error set
    /// (RFC-0959 has no consume() sink write). Distinct from `InvalidSignature`: a sink failure is
    /// an infrastructure fault, NOT a cryptographic rejection, and callers MUST be able to retry it
    /// without treating the event as forged (Round 3 MED #7).
    AuditSinkFailed { sink_error: Box<AuditError> },
    /// fires when the legacy `{ amount_micro_octo_w: i64 }` envelope is detected via the serde
    /// discriminator field (§2.2 Legacy-form discriminator) while `cost_asset_id != OCTO_W_ASSET_ID`.
    /// The legacy scalar form has NO asset binding, so it is only ever admissible in an OCTO-W
    /// context; any other asset means the envelope was replayed across an asset boundary.
    LegacyFormOnNonOctoWContext { claimed_asset_id: AssetId },
}

// Round 4 IMPORTANT #7: custom Deserialize that enforces the legacy-form discriminator at the
// serde layer (NOT post-deserialization). The derived Deserialize was previously used and
// silently accepted the legacy envelope, deferring the rejection to a downstream caller — which
// inverts the substrate-mandated invariant. The visitor inspects the raw envelope BEFORE
// constructing a BurnEventRef, so a legacy envelope on a non-OCTO-W asset is rejected BEFORE it
// ever reaches `validate()`.
//
// Happy path: when the envelope carries the modern `{ amount: DqaEncoding }` shape (no
// `amount_micro_octo_w` key), delegate to the derived Deserialize via a manual rebuild.
//
// Error mapping: the custom Deserialize returns `BurnEventError::LegacyFormOnNonOctoWContext
// { claimed_asset_id }` as the inner error; the caller-facing surface is
// `serde::de::Error::custom` so the failure is observable to Borsh/JSON deserializers uniformly.
impl<'de> serde::Deserialize<'de> for BurnEventRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // 1. Inspect the raw envelope (JSON object or Borsh-map equivalent) for the legacy key.
        //    The legacy discriminator is the presence of `amount_micro_octo_w: i64`.
        let raw: serde_json::Value = serde_json::Value::deserialize(deserializer)
            .map_err(serde::de::Error::custom)?;
        let obj = raw.as_object()
            .ok_or_else(|| serde::de::Error::custom("BurnEventRef: expected JSON object envelope"))?;
        if obj.contains_key("amount_micro_octo_w") {
            // 2. Reject with LegacyFormOnNonOctoWContext unless the surrounding context resolves
            //    to OCTO_W_ASSET_ID. The claimed asset_id is the `cost_asset_id` field if present;
            //    absent cost_asset_id is treated as "default to OCTO-W" only when the surrounding
            //    envelope context explicitly says so — without that context we reject with the
            //    sentinel `AssetId([0u8; 32])` so the caller knows the field was missing.
            let claimed_asset_id = obj.get("cost_asset_id")
                .and_then(|v| v.as_str())
                .and_then(|s| hex::decode(s).ok())
                .and_then(|bytes| if bytes.len() == 32 { Some(AssetId(<[u8; 32]>::try_from(bytes).unwrap())) } else { None })
                .unwrap_or(AssetId([0u8; 32]));
            return Err(serde::de::Error::custom(
                BurnEventError::LegacyFormOnNonOctoWContext { claimed_asset_id }
            ));
        }
        // 3. Happy path: re-serialize through JSON and delegate to the derived impl. This is the
        //    standard "manually rebuild via serde_json" pattern when removing a field from a
        //    derived Deserialize — the performance cost is acceptable because BurnEventRef is
        //    deserialized at audit boundaries, not on a hot path.
        serde_json::from_value(raw).map_err(serde::de::Error::custom)
    }
}
```

### 2.2 Construction + scale-binding invariant

```rust
// Domain-specific newtype (Layer B additive, semver-minor) — declared HERE because it is
// burn/settlement-domain vocabulary, not generic vault substrate.
// Round 3 LOW #9: canonical spelling is `SettlementId` (matches RFC-0959 §2.1). The old
// `SettlementRef` spelling survives as an alias for ONE substrate cycle, then is deleted —
// the same deprecation pattern as `PaidQueryRejectionReason = PaymentRejectionReason`.
pub struct SettlementId(pub [u8; 32]);
pub type SettlementRef = SettlementId;

// Round 3 MED #6: `Nonce`, `Epoch`, `GovernanceSignature` are NOT re-declared here. They are
// generic vault substrate with a single anchor at RFC-0105 §3.11 and are imported in §2.1:
//     use octo_vault::newtypes::{Epoch, GovernanceSignature, Nonce};
//
// Round 3 MED #5: `NonceRegistry` / `NonceError` are likewise NOT re-declared here.
// The trait's single source of truth is RFC-0105 §3.11 (GREENFIELD); this RFC and RFC-0959 §2.2 both
// import it:
//     use octo_vault::nonce_registry::{NonceError, NonceRegistry};
// Contract recap (normative text lives at the anchor, not here): the registry maintains a
// (governance_pubkey, nonce) -> ledger_height mapping, and anti-replay is substrate-mandated via
// `consume()` rather than call-site-mandated.

impl BurnEventRef {
    pub fn new(
        chain_id: ChainId,
        vault_id: VaultId,
        asset_id: AssetId,
        amount: Dqa,
        ledger_height: u64,
        settlement_event_ref: SettlementId,
        governance_signature: GovernanceSignature,
        nonce: Nonce,
        registry: &dyn AssetRegistry,    // RFC-0105 §3.1
        vault_registry: &dyn VaultRegistry,  // Round 3 IMPORTANT #2 — symmetrized with RFC-0959 §2.2
        current_epoch: Epoch,
    ) -> Result<Self, BurnEventError> {
        let meta = registry.metadata(&asset_id)
            .map_err(|_| BurnEventError::AssetUnknown)?;
        if amount.wire_scale != meta.wire_scale {
            return Err(BurnEventError::ScaleMismatch {
                amount_wire_scale: amount.wire_scale, asset_wire_scale: meta.wire_scale,
            });
        }
        if amount.wire_scale > MAX_SCALE {
            return Err(BurnEventError::ScaleOutOfRange { scale: amount.wire_scale });
        }
        // Round 3 IMPORTANT #2 + Round 4 CRITICAL #1: vault_registry.contains_asset returns
        // Result<(), VaultRegistryError> (per RFC-0959 R3 fix #5). The bool pattern that the prior
        // draft used is GONE; both error variants are mapped to their corresponding BurnEventError
        // variants. VaultUnknown fires when the vault_id is not registered at all — a different
        // failure mode from VaultAssetMismatch (which fires when the vault IS registered but does
        // not hold this asset). Callers can distinguish "unknown vault" from "vault-asset mismatch".
        vault_registry.contains_asset(&vault_id, &asset_id)
            .map_err(|e| match e {
                VaultRegistryError::UnknownVault { vault_id } => BurnEventError::VaultUnknown { vault_id },
                VaultRegistryError::VaultAssetMismatch { vault_id, asset_id } => BurnEventError::VaultAssetMismatch { vault_id, asset_id },
            })?;
        // Round 3 IMPORTANT #3 + Round 4 CRITICAL #2: governance_pubkey MUST be resolved BEFORE
        // body_hash, because body_hash binds governance_pubkey into the signed message (so an
        // attacker cannot mutate the verifier key to redirect the replay namespace without also
        // forging the signature). Sovereign assets carry `meta.governance_pubkey == None` — a
        // sovereign role token is burned by chain rule, not by a vault governance key (RFC-0105
        // §3.1 sovereign exemption; the normative statement for this RFC is §3.3 below).
        let governance_pubkey = if let Some(pk) = meta.governance_pubkey {
            pk
        } else {
            // Sovereign path: no governance key exists, so there is nothing to verify against.
            // `NonceRegistry` still requires a non-colliding namespace key, so derive a
            // domain-separated sentinel from the asset itself:
            //     blake3("octo:sovereign-nonce-ns:v1" || asset_id.0)
            // Sovereign nonces therefore occupy a per-asset namespace that can never collide with a
            // governed vault's namespace. This branch is what DISCHARGES the
            // `.expect("guard at new()")` obligation referenced by `consume()`: after `new()` returns,
            // `self.governance_pubkey` is unconditionally populated.
            sovereign_nonce_namespace(&asset_id)
        };
        // Round 3 MED #4 + Round 4 CRITICAL #2: `body_hash` is now DEFINED, not elided, AND it
        // binds `governance_pubkey` (Round 4 addition — prevents pubkey-swap replay attacks where
        // a forger redirects the message to a different verifier's nonce bucket without breaking
        // the signature). Every input is fixed-width, so plain concatenation is unambiguous (no
        // field can borrow bytes from its neighbour) — this is the length-prefixed discipline of
        // RFC-0105 §3.4 chain-of-trust applied to a fixed-width tuple. `governance_signature` is
        // EXCLUDED (it signs this hash) and `registry_snapshot_epoch` is EXCLUDED (it is assigned
        // by the verifier, not the signer). The helper is SHARED with `validate()` so the
        // field-set cannot drift between construction and post-deser re-verify.
        let body_hash = compute_body_hash(
            &chain_id, &vault_id, &asset_id,
            meta.kind_tag(), &amount, ledger_height,
            &settlement_event_ref, &governance_pubkey, &nonce,
        );
        // Governed path: signature is MANDATORY and verified against the registry pubkey.
        // Sovereign path: no verification (chain rule, not governance key, authorizes the burn).
        if let Some(pk) = meta.governance_pubkey {
            if !verify_governance_signature(&governance_signature.0, &body_hash[..], &pk) {
                return Err(BurnEventError::InvalidSignature);
            }
        }
        Ok(Self {
            chain_id, vault_id, asset_id, asset_kind: meta.kind.clone(), amount, ledger_height, settlement_event_ref,
            governance_signature, governance_pubkey, registry_snapshot_epoch: current_epoch, nonce,
        })
    }

    /// Post-deserialization invariant check (Round 1 CRITICAL #2 mitigation).
    /// Catches scale-flip attacks via direct Deserialize bypass.
    pub fn validate(
        &self,
        registry: &dyn AssetRegistry,
        vault_registry: &dyn VaultRegistry,  // Round 3 IMPORTANT #2 — same check as new(), re-run post-deser
        current_epoch: Epoch,
    ) -> Result<(), BurnEventError> {
        let meta = registry.metadata(&self.asset_id)
            .map_err(|_| BurnEventError::AssetUnknown)?;
        if self.amount.wire_scale != meta.wire_scale {
            return Err(BurnEventError::ScaleMismatch {
                amount_wire_scale: self.amount.wire_scale, asset_wire_scale: meta.wire_scale,
            });
        }
        // Round 3 IMPORTANT #2 + Round 4 CRITICAL #1: re-checked here using the same Result pattern
        // as new() (RFC-0959 R3 fix #5). Direct `Deserialize` bypass can substitute an arbitrary
        // `vault_id` on an otherwise well-formed event, so the check MUST run again post-deser.
        vault_registry.contains_asset(&self.vault_id, &self.asset_id)
            .map_err(|e| match e {
                VaultRegistryError::UnknownVault { vault_id } => BurnEventError::VaultUnknown { vault_id },
                VaultRegistryError::VaultAssetMismatch { vault_id, asset_id } => BurnEventError::VaultAssetMismatch { vault_id, asset_id },
            })?;
        if meta.tombstone {
            return Err(BurnEventError::AssetUnknown);    // tombstoned rejects new events
        }
        if self.asset_kind != meta.kind {
            return Err(BurnEventError::AssetKindMismatch {
                claimed: Box::new(self.asset_kind.clone()),
                registered: Box::new(meta.kind.clone()),
            });
        }
        if current_epoch.0 < self.registry_snapshot_epoch.0 {
            return Err(BurnEventError::StaleSnapshot {
                snapshot: self.registry_snapshot_epoch.0, live: current_epoch.0,
            });
        }
        // Round 4 IMPORTANT #6: re-verify the signature on body_hash. The signature was checked at
        // new() against whatever `meta.governance_pubkey` was at construction time, but a stale
        // BurnEventRef (deserialized long after creation) might be replayed through a registry that
        // has since rotated the governance key (RFC-0105 §3.3 governance rotation + revocation).
        // The current `meta.governance_pubkey` is the source of truth for the post-rotation
        // verify; if it is `Some`, the signature MUST verify against THAT pk over the same
        // body_hash field-set. Sovereign path: `meta.governance_pubkey == None` so no re-verify.
        let body_hash = compute_body_hash(
            &self.chain_id, &self.vault_id, &self.asset_id,
            meta.kind_tag(), &self.amount, self.ledger_height,
            &self.settlement_event_ref, &self.governance_pubkey, &self.nonce,
        );
        if let Some(pk) = meta.governance_pubkey {
            if !verify_governance_signature(&self.governance_signature.0, &body_hash[..], &pk) {
                return Err(BurnEventError::InvalidSignature);
            }
        }
        Ok(())
    }

    /// Substrate-mandated consumption (Round 1 MED #6 mitigation: TOCTOU between validate and consume).
    /// Bundles validate() + NonceRegistry observe() + audit log write into a single transaction.
    /// All downstream consumers MUST go through this function.
    pub fn consume(
        &self,
        registry: &dyn AssetRegistry,
        vault_registry: &dyn VaultRegistry,   // Round 3 IMPORTANT #2
        nonce_registry: &mut dyn NonceRegistry,
        audit_sink: &mut dyn AuditSink,
        current_epoch: Epoch,
        current_ledger_height: u64,
    ) -> Result<(), BurnEventError> {
        // 1. Validate (scale, vault↔asset containment, tombstone, asset_kind, stale snapshot,
        //    signature re-verify — Round 4 IMPORTANT #6).
        self.validate(registry, vault_registry, current_epoch)?;
        // 2. Register the nonce. Substrate-mandated, NOT call-site (Round 1 CRITICAL #4 mitigation).
        //    Round 3 CRITICAL #1: the key is `self.governance_pubkey` — the ACTUAL pubkey pinned at
        //    construction. The prior draft keyed on `self.governance_signature.0[..32]`, which is the
        //    first 32 bytes of an ed25519 signature (the `R` point), NOT a pubkey. That is
        //    attacker-malleable: a forger who can produce any second valid signature over the same
        //    body lands in a DIFFERENT nonce bucket and replays the burn for free. Keying on the
        //    pinned pubkey makes the replay namespace immutable for a given signer.
        //    Round 4 CRITICAL #3: `governance_pubkey` is non-optional (sovereign sentinel via blake3
        //    for the sovereign path), so we key DIRECTLY on the struct field — no `.as_ref()` of an
        //    Option<>. The error variant carries `prior_height` so callers can distinguish the
        //    replay from a fresh observation failure (which never reaches AlreadyObserved).
        nonce_registry.observe(&self.governance_pubkey, &self.nonce.0)
            .map_err(|NonceError::AlreadyObserved { prior_height, .. }| BurnEventError::Replay { prior_height })?;
        // 3. Write to audit log with the snapshot epoch pinned (Round 1 MED #6 mitigation).
        //    Round 3 MED #7: a sink failure maps to `AuditSinkFailed`, NOT `InvalidSignature`. The prior
        //    draft reused `InvalidSignature`, which told callers a well-signed event was forged and made
        //    a retriable infrastructure fault indistinguishable from a cryptographic rejection.
        audit_sink.write(self, self.registry_snapshot_epoch.clone())
            .map_err(|e| BurnEventError::AuditSinkFailed { sink_error: Box::new(e) })
    }
}

/// Round 4 IMPORTANT #6 + Round 5 CRIT-1: shared body_hash helper. Both `new()` and `validate()`
/// MUST compute the same hash over the same field-set, otherwise a forger can mutate a field
/// post-sign and the re-verify at `validate()` will fail on a different hash than the signature
/// was produced over.
/// Field-set (fixed order, fixed width): chain_id, vault_id, asset_id, kind_tag, amount,
/// ledger_height, settlement_event_ref, governance_pubkey, nonce. Including `nonce` in the signed
/// message prevents a replay-by-nonce-variation attack: the same body signed twice with two
/// distinct nonces yields two distinct hashes, so a forger cannot reuse a signature to consume
/// a different nonce.
/// Round 6: the hash is computed via the canonical `octo_vault::blake3_hash` (RFC-0105 §3.12
/// Cryptographic Primitives), which returns the 32-byte form directly. Call sites MUST NOT use
/// `blake3::hash(..).as_bytes()` — the canonical helper is the single source of the digest form,
/// and the return type is `[u8; 32]` so signature verification passes `&body_hash[..]`.
fn compute_body_hash(
    chain_id: &ChainId,
    vault_id: &VaultId,
    asset_id: &AssetId,
    kind_tag: u8,
    amount: &Dqa,
    ledger_height: u64,
    settlement_event_ref: &SettlementId,
    governance_pubkey: &[u8; 32],
    nonce: &Nonce,
) -> [u8; 32] {
    blake3_hash(&[
        &chain_id.0,
        &vault_id.0,
        &asset_id.0,
        &[kind_tag],
        &amount.to_be_bytes(),
        &ledger_height.to_be_bytes(),
        &settlement_event_ref.0,
        governance_pubkey.as_ref(),
        &nonce.0,
    ].concat())
}

pub trait AuditSink {
    fn write(&mut self, burn: &BurnEventRef, snapshot_epoch: Epoch) -> Result<(), AuditError>;
}
pub enum AuditError { /* substrate-defined sink-side errors */ }
```

**NonceRegistry size bound (Round 3 LOW #13).** An unbounded `(governance_pubkey, nonce) -> ledger_height` map is a memory-exhaustion surface: any signer can mint fresh nonces indefinitely. The substrate implementation MUST therefore be a **bounded LRU keyed per `governance_pubkey`** — on the order of 10^6 nonces per pubkey — so one signer's nonce churn cannot evict another signer's replay protection. Entry **TTL is tied to the asset revocation grace period** (RFC-0105 §3.3 governance rotation + revocation): once an asset can no longer be revoked-and-disputed, its nonces can no longer be usefully replayed, so eviction is safe. Both the bound and the TTL are normatively specified at the substrate anchor, **RFC-0105 §3.11**; this RFC only states the requirement it depends on.

**Legacy-form serde discriminator (Round 3 MED #10).** The pre-Dqa envelope carried a bare scalar `{ amount_micro_octo_w: i64 }` with **no asset binding** — the asset was implicit in the field name. Deserializers MUST treat the presence of the `amount_micro_octo_w` key as a **legacy-form discriminator**: it is admissible only when the surrounding context resolves to `OCTO_W_ASSET_ID`. If the discriminator field is present alongside `cost_asset_id != OCTO_W_ASSET_ID`, the envelope was replayed across an asset boundary and deserialization MUST fail with `BurnEventError::LegacyFormOnNonOctoWContext { claimed_asset_id }`. The two forms are mutually exclusive: an envelope carrying both `amount_micro_octo_w` and `amount` is malformed and rejected.

### 2.3 Audit-invariant: verify_burn_against_caveat (NEW)

```rust
pub enum BurnCaveatError {
    /// fires when `burn.asset_id != caveat.asset_id`.
    /// `Audit*` prefix per the RFC-0959 §2.3 `SettlementAuditError` convention: audit-time
    /// invariant errors carry the prefix; constructor-time errors (`BurnEventError`) use bare names.
    AuditAssetMismatch { burn_asset_id: AssetId, caveat_asset_id: AssetId },
    /// fires when `burn.amount.wire_scale != caveat.budget.wire_scale`.
    AuditScaleMismatch { burn_scale: u8, caveat_scale: u8 },
}

/// HARD invariant: a burn event charged against a vault bound to PaymentCaveat
/// MUST be in the same asset AND at the same wire scale as the caveat authorizes.
pub fn verify_burn_against_caveat(
    burn: &BurnEventRef,
    caveat: &PaymentCaveat,
) -> Result<(), BurnCaveatError> {
    if burn.asset_id != caveat.asset_id {
        return Err(BurnCaveatError::AuditAssetMismatch {
            burn_asset_id: burn.asset_id, caveat_asset_id: caveat.asset_id,
        });
    }
    if burn.amount.wire_scale != caveat.budget.wire_scale {
        return Err(BurnCaveatError::AuditScaleMismatch {
            burn_scale: burn.amount.wire_scale, caveat_scale: caveat.budget.wire_scale,
        });
    }
    Ok(())
}
```

### 2.4 AssetKind cryptographic commitment (Round 1 IMPORTANT #11 mitigation)

`asset_kind` is BOTH bundled on the struct AND verified at validate-time: `registry.metadata(asset_id)?.kind == self.asset_kind`. If a forger tampers with `asset_kind` without also forging the registry entry, validate() catches it.

### 2.5 Error scenario matrix (NEW, Round 1 fix)

| Variant                       | Trigger                                                                                                                                          |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `AssetUnknown`                | `AssetRegistry::metadata(asset_id)` returns `AssetError::Unknown` OR asset is tombstoned.                                                        |
| `ScaleMismatch`               | `amount.wire_scale != metadata.wire_scale` at construction OR at validate().                                                                     |
| `ScaleOutOfRange`             | `amount.wire_scale > MAX_SCALE` (defense-in-depth).                                                                                              |
| `InvalidSignature`            | `governance_signature` does not verify against `metadata.governance_pubkey` (governed path only — sovereign assets are exempt per §3.3).         |
| `Replay`                      | `NonceRegistry::observe(self.governance_pubkey, nonce)` returns `NonceError::AlreadyObserved`.                                                   |
| `StaleSnapshot`               | `current_epoch.0 < self.registry_snapshot_epoch.0` (asset rotated out from under us).                                                            |
| `AssetKindMismatch`           | stored `asset_kind` on the deserialized event != `AssetRegistry::metadata(asset_id).kind` (forger mutation mitigation).                          |
| `VaultUnknown`                | `VaultRegistry::contains_asset(vault_id, asset_id)` returns `VaultRegistryError::UnknownVault` — vault_id not present in the registry.           |
| `VaultAssetMismatch`          | `VaultRegistry::contains_asset(vault_id, asset_id)` returns `VaultRegistryError::VaultAssetMismatch` — burn names an asset the vault never held. |
| `AuditSinkFailed`             | `AuditSink::write` returns `Err(AuditError)` in `consume()` — BurnEvent-scoped, retriable infrastructure fault, NOT a crypto rejection.          |
| `LegacyFormOnNonOctoWContext` | legacy `{ amount_micro_octo_w: i64 }` envelope detected via the serde discriminator while `cost_asset_id != OCTO_W_ASSET_ID`.                    |

## 3. Wire Form (NEW, greenfield)

### 3.1 Canonical wire form

```json
{
  "burn_event_ref": {
    "chain_id": "<32B-hex>",
    "vault_id": "<32B-hex>",
    "asset_id": "<32B-hex>",
    "asset_kind": "SovereignRoleToken | PrivateCorporateAsset | BridgedExternalAsset | WrappedCrossChainAsset",
    "amount": "<16B-hex-of-DqaEncoding>", // RFC-0862 §Substrate types: 16-byte BE (value:i64 + wire_scale:u8 + _reserved:7)
    "ledger_height": 42,
    "settlement_event_ref": "<32B-hex>",
    "governance_signature": "<64B-hex>", // ed25519 over body_hash (§2.2); absent-but-present-as-zeros on the sovereign path (§3.3)
    "governance_pubkey": "<32B-hex>", // Round 3 CRITICAL #1: the NonceRegistry key, pinned at construction
    "registry_snapshot_epoch": 12345,
    "nonce": "<32B-hex>"
  }
}
```

**Encoding note (Round 2 fix):** the canonical on-wire form is **16-byte BE** per `crates/octo-cap-macaroon/src/dqa_serde.rs:5` (`DqaEncoding`). JSON display form `{ "value": ..., "scale": ... }` is for human inspection only; serde_bytes converts to/from the 16-byte BE form. Reviewers and implementers must NOT confuse the JSON display form with the wire form.

**Cross-reference (Round 3 LOW #12):** `PaymentCaveat.budget` (RFC-0965 §3) uses the same `<16B-hex-of-DqaEncoding>` encoding as `BurnEventRef.amount`. This is what makes the `verify_burn_against_caveat` scale comparison (§2.3) a byte-level comparison of like-encoded values rather than a cross-format conversion.

**Cross-RFC body_hash note (Round 6 correction; supersedes the Round 5 HIGH-1 + LOW-13 rationale):** Both RFC-0959 §2.2 and this RFC bind `governance_pubkey` into `body_hash`. RFC-0959 binds the **LIVE** `meta.governance_pubkey` (the current registry meta, read at construction and again at validate()); `BurnEventRef` binds the **PINNED** `self.governance_pubkey` (snapshotted into the struct at construction). The asymmetry between the two RFCs is a **SOURCE** difference (live vs pinned), **NOT** an inclusion/exclusion difference — both events bind `governance_pubkey` into `body_hash`. The prior draft of this note claimed RFC-0959 intentionally _excludes_ `governance_pubkey`; that claim was stale as of the RFC-0959 R5 HIGH-1 fix, which added `governance_pubkey` to the settlement body_hash field-set. Both RFCs likewise include `nonce` in the body_hash so that replay-by-nonce-variation is uniformly prevented across the two event types.

Why pinned on the burn side: `BurnEventRef` verifies at-construction (`new()`) AND at post-deser re-verify (`validate()`), and `consume()` keys the NonceRegistry on the same pinned pubkey — so the replay namespace and the signed message must agree on one immutable key. Rebinding the burn body_hash against the live key would let a governance rotation retroactively invalidate well-signed historical burns and silently move them into a different nonce bucket.

### 3.2 Wire-form migration

There is no prior wire form to migrate. The 6 sites at `rfcs/accepted/process/0206-v30-value-transfer-surface.md:65,73,93,101,110,121` are wire-form draft prose that was never finalized. RFC-0206 will incorporate `BurnEventRef` as the canonical burn-event wire form.

### 3.3 Sovereign exemption (Round 3 IMPORTANT #3)

`AssetMetadata.governance_pubkey` is `Option<[u8; 32]>`, and for **sovereign assets it is `None`**. A sovereign role token has no vault governance key to sign with: its burns are authorized by chain rule, not by a delegated governance signer. Consequently:

1. **Signature is optional on the sovereign path.** When `meta.governance_pubkey == None`, `BurnEventRef::new()` MUST NOT attempt verification and MUST NOT return `InvalidSignature`. This is the **RFC-0105 §3.1 sovereign exemption**.
2. **Signature is mandatory on the governed path.** When `meta.governance_pubkey == Some(pk)`, verification against `pk` is required and failure is `InvalidSignature`. There is no "best-effort" middle state.
3. **The replay namespace is still populated.** `BurnEventRef.governance_pubkey` is a non-optional `[u8; 32]` because `consume()` requires a stable `NonceRegistry` key. On the sovereign path it holds the domain-separated sentinel `blake3("octo:sovereign-nonce-ns:v1" || asset_id.0)`, giving each sovereign asset its own nonce namespace that cannot collide with any governed vault's namespace.
4. **The exemption is asset-scoped, not caller-scoped.** It follows from registry metadata alone, so a caller cannot elect into it; presenting a sovereign `asset_id` is the only way to reach the branch, and `AssetKindMismatch` (§2.4) prevents forging the kind to get there.

## 4. Cross-Reference Updates (Round 2: stripped version pins)

- RFC-0105 (companion amendment): defines `AssetRegistry` side-table that `BurnEventRef::new` and `validate` query. Also the single-source anchor for `NonceRegistry` + the shared `Nonce` / `Epoch` / `GovernanceSignature` newtypes (§0).
- RFC-0965 (companion amendment): `PaymentCaveat.asset_id` binds to the same `BurnEventRef.asset_id`; the `verify_burn_against_caveat` invariant guarantees the equality.
- RFC-0959 (companion amendment): `SettlementEvent.cost_asset_id` matches `BurnEventRef.asset_id` (settlement → burn trail invariant).

// Tri-invariant (RFC-0105 §3.13)

## 5. Backward Compatibility

- **Existing sites at `crates/octo-policy/src/policy_kinds.rs:263` + `workflow_kind.rs:313,461,477`**: these are on `SelectorContext` (InteropSelector) and test fixtures, NOT on `BurnEventRef`. They are out of scope for this RFC; they remain on `amount_dqa_micros: i64` until RFC-0967-A1 (InteropSelector) is amended separately.
- **Wire-form draft sites at `rfcs/accepted/process/0206-v30-value-transfer-surface.md`**: not yet finalized; RFC-0206 + this RFC together close them.

## 6. Migration of `amount_dqa_micros` Sites (out-of-scope but documented)

The `amount_dqa_micros` carrier at `crates/octo-policy/src/policy_kinds.rs:263` + `workflow_kind.rs:313,461,477` is on `SelectorContext`, not on `BurnEventRef`. Migration of those sites is the responsibility of RFC-0967-A1 (InteropSelector amendment). This RFC documents them as out-of-scope to avoid the Round 1 CRITICAL #1 confusion where the anchor was claimed to point at BurnEventRef.

## 7. Naming Cleanup

| Old (out-of-scope, owned by RFC-0967-A1)      | New                                     | Site                                                                          |
| --------------------------------------------- | --------------------------------------- | ----------------------------------------------------------------------------- |
| `amount_dqa_micros: i64` (on SelectorContext) | (no change — out of scope for this RFC) | `crates/octo-policy/src/policy_kinds.rs:263` + `workflow_kind.rs:313,461,477` |

`BurnEventRef.amount` is `Dqa` from inception; the retired `MicroOctoW` alias (retired 2026-08-17 per `crates/octo-cap-macaroon/src/caveat/payment.rs:23` + `crates/octo-paid-query/src/lib.rs:103`) is never reintroduced. The prior draft's "CHANGED from `cost: MicroOCTO_W`" framing was dropped: BurnEventRef is GREENFIELD (Round 2 §0 corrected), so there is no prior `MicroOCTO_W`-typed field to migrate from. (The substrate alias was spelled `MicroOctoW`, retired 2026-08-17; `MicroOCTO_W` only survives in a slashing error string at `crates/quota-router-core/src/marketplace/slashing.rs:435`.)

## 8. Version History

| Version | Date                    | Author                   | Note                                                                        |
| ------- | ----------------------- | ------------------------ | --------------------------------------------------------------------------- |
| 3.0     | 2026-08-15              | @cipherocto + @mmacedoeu | Initial accepted (Vault Path Taxonomy).                                     |
| 3.1-3.5 | 2026-08-15 → 2026-08-22 | @cipherocto + @mmacedoeu | R1-R7 fix-all rounds.                                                       |
| 3.6-r1  | 2026-08-26              | @mmacedoeu               | Initial v3.6 draft (Round 1). 11 findings → r2.                             |
| 3.6-r2  | 2026-08-26              | @mmacedoeu               | GREENFIELD anchor; DqaEncoding wire form; scope reduction.                  |
| 3.6-r3  | 2026-08-26              | @mmacedoeu               | Single-source imports; typed newtypes; NonceRegistry trait; consume().      |
| 3.6-r4  | 2026-08-26              | @mmacedoeu               | governance_pubkey nonce key; VaultRegistry; body_hash defined.              |
| 3.6-r5  | 2026-08-26              | @mmacedoeu               | Result pattern; VaultUnknown; body_hash pk; signature re-verify.            |
| 3.6-r6  | 2026-08-26              | @mmacedoeu               | nonce in body_hash; GREENFIELD imports; tri-invariant → §3.13.              |
| 3.6-r7  | 2026-08-26              | @mmacedoeu               | Audit\* prefix; blake3_hash; concrete cites; body_hash rationale corrected. |
| 3.6-r8  | 2026-08-26              | @mmacedoeu               | Round 7-9: r7 trim + DRY closure + Accepted promotion.                      |

## 9. Pending (concrete test vectors)

- [ ] R4 adversarial review after fix.
- [ ] Substrate anchor verification (NEW): run `scripts/verify-substrate-anchors.sh <rfc-path>`.
- [ ] Test vector: `BurnEventRef::new(chain, vault, OCTO_W_ASSET_ID, Dqa{value:1000, wire_scale:0}, 42, ...)` succeeds (substrate today enforces `wire_scale=0` on OCTO-W).
- [ ] Test vector: `BurnEventRef::new(chain, vault, OCTO_W_ASSET_ID, Dqa{value:1000, wire_scale:8}, 42, ...)` returns `Err(ScaleMismatch { amount_wire_scale: 8, asset_wire_scale: 0 })`.
- [ ] Test vector: `BurnEventRef::new(chain, vault, TOMBSTONED_ASSET_ID, ...)` returns `Err(AssetUnknown)`.
- [ ] Test vector: validate() catches direct Deserialize bypass: deserialize `{ asset_id: OCTO_W, amount: { wire_scale: 8 } }` then validate() against live registry returns `Err(ScaleMismatch)`.
- [ ] Test vector: validate() catches asset_kind mutation: deserialize `{ asset_id: OCTO_W, asset_kind: BridgedExternalAsset, ...}` then validate() returns `Err(AssetKindMismatch { claimed: BridgedExternalAsset, registered: SovereignRoleToken })`.
- [ ] Test vector (Round 3 IMPORTANT #2): `BurnEventRef::new(chain, VAULT_A, ASSET_HELD_ONLY_BY_VAULT_B, ...)` returns `Err(VaultAssetMismatch { vault_id: VAULT_A, asset_id: ASSET_HELD_ONLY_BY_VAULT_B })`; the same event mutated post-deserialization is caught again by `validate()`.
- [ ] Test vector: consume() rejects replay: `nonce_registry.observe(pk, nonce)` succeeds; second observe() with same `(pk, nonce)` returns `BurnEventError::Replay`.
- [ ] Test vector (Round 3 CRITICAL #1): two DISTINCT valid signatures over the same body land in the SAME nonce bucket — i.e. consume() on the second returns `Err(Replay)`. Keying on `governance_signature.0[..32]` would have let it through; keying on `governance_pubkey` does not.
- [ ] Test vector (Round 3 IMPORTANT #3, sovereign exemption): `BurnEventRef::new(chain, vault, SOVEREIGN_ASSET_ID, ...)` with `meta.governance_pubkey == None` succeeds WITHOUT signature verification, and `self.governance_pubkey == blake3("octo:sovereign-nonce-ns:v1" || SOVEREIGN_ASSET_ID.0)`.
- [ ] Test vector (Round 3 MED #7): `audit_sink` that returns `Err(AuditError)` makes consume() return `Err(AuditSinkFailed { .. })`, NOT `Err(InvalidSignature)`.
- [ ] Test vector (Round 3 MED #10): envelope carrying `{ amount_micro_octo_w: 1000 }` with `cost_asset_id = USDC_ASSET_ID` returns `Err(LegacyFormOnNonOctoWContext { claimed_asset_id: USDC_ASSET_ID })`; the same envelope with `cost_asset_id = OCTO_W_ASSET_ID` deserializes.
- [ ] Test vector (Round 3 MED #4 + Round 4 CRITICAL #2): `body_hash` is stable across re-encode and CHANGES when any one contributing field (`chain_id`, `vault_id`, `asset_id`, `kind_tag`, `amount`, `ledger_height`, `settlement_event_ref`, `governance_pubkey`) is perturbed by one byte — `governance_pubkey` is in the field-set as of v3.6-r5.
- [ ] Test vector (Round 5 CRIT-1, replay-by-nonce-variation prevented): two `BurnEventRef` events with **identical** body fields but **distinct** `nonce` values produce **distinct** `body_hash` outputs. Concretely, if a signer produces signature `sig_1` over `body_hash(body, nonce_1)` and an attacker tries to replay the same body under `nonce_2 != nonce_1`, `verify_governance_signature(sig_1, body_hash(body, nonce_2), pk)` MUST return `false` — i.e. the signature does NOT verify against the nonce-shifted hash. The replay attempt then fails on two independent grounds: (a) signature does not verify, and (b) `nonce_registry.observe(pk, nonce_2)` succeeds (because `nonce_2` is fresh). Binding `nonce` into the signed message closes the attack surface; prior drafts that excluded `nonce` from `body_hash` would have allowed a forger to reuse `sig_1` to consume `nonce_2` (defeating the anti-replay substrate mandate).
- [ ] Test vector: `verify_burn_against_caveat(burn{asset=OCTO_W, amount_wire_scale=0}, caveat{asset=USDC, budget_wire_scale=6})` returns `Err(AuditAssetMismatch)`.
- [ ] Test vector (Round 6): `verify_burn_against_caveat(burn{asset=OCTO_W, amount_wire_scale=0}, caveat{asset=OCTO_W, budget_wire_scale=6})` returns `Err(AuditScaleMismatch { burn_scale: 0, caveat_scale: 6 })` — same asset, divergent wire scale.
- [ ] Test vector (Round 4 LOW #11): `BurnEventRef::new(chain, vault, OCTO_W_ASSET_ID, Dqa{value:1000, wire_scale:99}, 42, ...)` returns `Err(ScaleOutOfRange { scale: 99 })` — defense-in-depth bound above `MAX_SCALE`.
- [ ] Test vector (Round 4 LOW #11): `validate()` re-verifies signature after governance rotation — a `BurnEventRef` signed by `pk_old` is rejected with `Err(InvalidSignature)` when the registry's current `meta.governance_pubkey` is `Some(pk_new)` and the signature does not verify against `pk_new` over the same body_hash field-set (Round 4 IMPORTANT #6).
- [ ] Test vector (Round 4 LOW #11): `validate()` returns `Err(StaleSnapshot { snapshot: 100, live: 50 })` when a `BurnEventRef` carrying `registry_snapshot_epoch = 100` is presented against a current `Epoch(50)` — the snapshot epoch is from the future relative to the live registry.
- [ ] Test vector (Round 4 LOW #11): `BurnEventRef::new(chain, UNKNOWN_VAULT_ID, OCTO_W_ASSET_ID, ...)` returns `Err(VaultUnknown { vault_id: UNKNOWN_VAULT_ID })` — vault is not present in the registry at all (distinct from `VaultAssetMismatch`).
- [ ] Cross-reference validation via Guard 2 cite validator.
- [ ] Acceptance promotion (7-day minimum review + 2 maintainer approvals).

---

**End of RFC-0960 v3.6 (revision r7 — Accepted 2026-08-26).**
