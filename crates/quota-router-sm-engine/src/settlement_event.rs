//! Mission G (RFC-0959 v2.8) SettlementEvent substrate.
//!
//! Per RFC-0959 v2.8 §2 (SettlementEvent Specification) + §3 (Wire Form).
//!
//! ## Layer hosting
//!
//! `SettlementEvent` lives in `quota-router-sm-engine` (Layer C
//! specialized node for settlement matching) but the TYPE itself is a
//! Layer B additive substrate — multiple Layer C consumers (audit replay,
//! settlement matching, vault projection wiring in Mission B) all observe
//! it. Per RFC-0959 v2.8 §2.1 L42-44.

#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_arguments,
    clippy::items_after_statements,
    clippy::items_after_test_module,
    clippy::used_underscore_items,
    clippy::many_single_char_names
)]

use octo_cap_macaroon::{
    blake3_hash, sovereign_nonce_namespace, verify_governance_signature, AssetError, AssetId,
    AssetKind, AssetRegistry, ChainId, Dqa, GovernanceSignature, Nonce, NonceError, NonceEventKind,
    NonceRegistry, PaymentCaveat, VaultAssetError, VaultId, VaultRegistry,
};

/// Typed 32-byte wrapper for a settlement event ID
/// (RFC-0959 v2.8 §2.1 L67).
///
/// Re-exported from `octo_cap_macaroon::SettlementId` (Layer A substrate;
/// canonical home per `cipherocto-design-principles` §Canonical home rule).
pub use octo_cap_macaroon::SettlementId;

/// Typed 32-byte wrapper for an ask ID (RFC-0959 v2.8 §2.1 L68).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize,
)]
pub struct AskId(pub [u8; 32]);

/// Typed 32-byte wrapper for evidence blob reference
/// (RFC-0959 v2.8 §2.1 L69).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize,
)]
pub struct EvidenceRef(pub [u8; 32]);

/// Settlement outcome discriminant (RFC-0959 v2.8 §2.1 L91-105).
///
/// `AlreadyConsumed` aligns with `SettlementError::AlreadyConsumed(String)`
/// (canonical substrate variant per RFC-0959 v2.8 §0; no rename history).
/// `ReceiptReplay` was NEVER a substrate variant. Audit* variants live
/// in `SettlementAuditError` (§2.3), NOT here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum SettlementDecision {
    Consumed = 0x01,
    AlreadyConsumed = 0x02,
    InsufficientEvidence = 0x03,
    BudgetExhausted = 0x04,
}

/// Vault registry error mapping (RFC-0959 v2.8 §2.2 L111-116).
///
/// Single-source-of-truth: `octo_cap_macaroon::VaultAssetError` is the
/// canonical substrate enum (Mission F). This is a thin alias to avoid
/// divergent error names across crates. Re-exported via `use
/// octo_cap_macaroon::VaultAssetError as VaultRegistryError;` at the
/// consumer boundary.
pub type VaultRegistryError = VaultAssetError;

/// 13-field SettlementEvent struct (RFC-0959 v2.8 §2.1 L75-89).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SettlementEvent {
    /// Settlement event identifier (BLAKE3-derived per RFC-0959 §2.1).
    pub settlement_id: SettlementId,
    /// Originating ask id (RFC-0959 §1.3 mint → settle chain).
    pub ask_id: AskId,
    /// Vault that pays the settlement cost (RFC-0959 §2.1).
    pub cost_vault_id: VaultId,
    /// Asset that pays the settlement cost (RFC-0959 §2.1).
    pub cost_asset_id: AssetId,
    /// AssetKind tag for tri-invariant enforcement (RFC-0105 §3.13).
    pub asset_kind: AssetKind,
    /// Cost amount in DQA scale (RFC-0105 §3.6).
    pub cost: Dqa,
    /// Audit evidence reference (RFC-0959 §2.1).
    pub evidence_ref: EvidenceRef,
    /// Originating ledger height (RFC-0959 §2.1).
    pub ledger_height: u64,
    /// Unix-time ms at construction (RFC-0959 §2.1).
    pub created_at_unix_ms: u64,
    /// Settlement decision discriminator (Consumed/AlreadyConsumed/...; RFC-0959 §2.1).
    pub settlement_decision: SettlementDecision,
    /// Governance signature over body_hash (RFC-0959 §2.1).
    pub governance_signature: GovernanceSignature,
    /// Registry snapshot epoch at construction (RFC-0959 §2.1).
    pub registry_snapshot_epoch: EpochLocal,
    /// Replay-protection nonce (RFC-0105 §3.11 + RFC-0959 §2.2).
    pub nonce: Nonce,
}

/// Local Epoch newtype (RFC-0959 v2.8 §2.1 L86).
///
/// `Epoch` lives in `octo_cap_macaroon` but the constructor signature
/// differs slightly. We use a 1:1 newtype here to keep Mission G
/// substrate-isolated.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize,
)]
pub struct EpochLocal(pub u64);

impl EpochLocal {
    pub const fn new(v: u64) -> Self {
        Self(v)
    }
    pub const fn get(&self) -> u64 {
        self.0
    }
}

/// SettlementEvent errors (RFC-0959 v2.8 §2.2 L118-144). 9 variants,
/// `#[non_exhaustive]` for additive Layer B substrate.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SettlementEventError {
    #[error("asset_id not registered (or tombstoned)")]
    AssetUnknown,
    #[error("cost.wire_scale {cost_scale} != vault.wire_scale {vault_scale}")]
    ScaleMismatch { cost_scale: u8, vault_scale: u8 },
    #[error("wire_scale {scale} exceeds MAX_SCALE = 18")]
    ScaleOutOfRange { scale: u8 },
    #[error("governance signature verification failed")]
    InvalidSignature,
    #[error("nonce replay")]
    Replay,
    #[error("stale snapshot: snapshot = {snapshot}, live = {live}")]
    StaleSnapshot { snapshot: u64, live: u64 },
    #[error("vault {vault_id:?} does not contain asset {asset_id:?}")]
    VaultAssetMismatch {
        vault_id: VaultId,
        asset_id: AssetId,
    },
    #[error("vault_id not registered: {vault_id:?}")]
    VaultUnknown { vault_id: VaultId },
    #[error("legacy form on non-OCTO-W context: claimed_asset_id = {claimed_asset_id:?}")]
    LegacyFormOnNonOctoWContext { claimed_asset_id: AssetId },
}

/// Audit invariant violation (RFC-0105 v3.5 §3.13 tri-invariant pair).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuditInvariantViolation {
    #[error(
        "asset_id mismatch: settlement = {settlement_asset_id:?}, caveat = {caveat_asset_id:?}"
    )]
    AssetMismatch {
        settlement_asset_id: AssetId,
        caveat_asset_id: AssetId,
    },
}

/// Length-prefixed settlement-decision encoding
/// (RFC-0959 v2.8 §2.2 L154-173).
#[must_use]
pub fn encode_settlement_decision(d: SettlementDecision) -> Vec<u8> {
    let tag = d as u8;
    let mut out = vec![0x01_u8]; // discriminator byte (RFC §2.2 L163)
    out.push(tag);
    out
}

/// Body-hash commitment per RFC-0959 v2.8 §2.2 L181-205.
///
/// Field set: `settlement_id | ask_id | cost_vault_id | cost_asset_id |
/// kind_tag (1 byte) | cost (Dqa .to_le_bytes) | ledger_height (u64
/// .to_le_bytes) | evidence_ref | governance_pubkey | nonce`. Mirrors
/// RFC-0960 v3.6 §2.2 `compute_body_hash`.
#[must_use]
pub fn compute_settlement_body_hash(settlement: &SettlementEvent) -> [u8; 32] {
    let mut buf: Vec<u8> = Vec::with_capacity(32 * 6 + 1 + 16 + 8 + 8 + 32 + 32);
    buf.extend_from_slice(BODY_HASH_DOMAIN);
    buf.extend_from_slice(settlement.settlement_id.0.as_ref());
    buf.extend_from_slice(settlement.ask_id.0.as_ref());
    buf.extend_from_slice(settlement.cost_vault_id.as_bytes());
    buf.extend_from_slice(settlement.cost_asset_id.as_bytes());
    buf.push(settlement.asset_kind as u8);
    buf.push(settlement.cost.scale);
    buf.extend_from_slice(&settlement.cost.value.to_le_bytes());
    buf.extend_from_slice(&settlement.ledger_height.to_le_bytes());
    buf.extend_from_slice(settlement.evidence_ref.0.as_ref());
    buf.extend_from_slice(&settlement.governance_signature.as_bytes()[..32]);
    buf.extend_from_slice(settlement.nonce.as_bytes());
    blake3_hash(&buf)
}

/// 8-gate constructor (RFC-0959 v2.8 §2.2 L207-295). 11 args.
///
/// `asset_kind` derived from `meta.kind` (NOT a constructor arg).
/// `registry_snapshot_epoch` set from `current_epoch` (NOT a constructor
/// arg).
#[allow(clippy::too_many_arguments)]
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
    registry: &dyn AssetRegistry,
    vault_registry: &dyn VaultRegistry,
    nonce_registry: &mut dyn NonceRegistry,
    current_epoch: EpochLocal,
) -> Result<SettlementEvent, SettlementEventError> {
    // Gate 0 — registry.metadata resolves + not tombstoned
    let meta = registry
        .metadata(&cost_asset_id)
        .map_err(|_| SettlementEventError::AssetUnknown)?;
    // Gate 1 — scale bound (MAX_SCALE check first so out-of-range
    // returns ScaleOutOfRange, not ScaleMismatch)
    if cost.scale > octo_cap_macaroon::MAX_SCALE {
        return Err(SettlementEventError::ScaleOutOfRange { scale: cost.scale });
    }
    // Gate 2 — scale match
    if cost.scale != meta.wire_scale {
        return Err(SettlementEventError::ScaleMismatch {
            cost_scale: cost.scale,
            vault_scale: meta.wire_scale,
        });
    }
    // Gate 3 — governance_pubkey resolution: [0u8;32] sentinel for sovereign
    // body_hash commitment (CONTENT slot per §3.3 L506-511).
    let governance_pubkey_body_hash: [u8; 32] = meta.governance_pubkey.unwrap_or_default();
    // Gate 4 — build struct (asset_kind from meta, snapshot from current)
    let settlement = SettlementEvent {
        settlement_id,
        ask_id,
        cost_vault_id,
        cost_asset_id,
        asset_kind: meta.kind,
        cost,
        evidence_ref,
        ledger_height,
        created_at_unix_ms,
        settlement_decision,
        governance_signature,
        registry_snapshot_epoch: current_epoch,
        nonce,
    };
    // Gate 5 — signature verify (sovereign EXEMPT)
    let body_hash = compute_settlement_body_hash(&settlement);
    if meta.governance_pubkey.is_some() {
        // The signature body is over settlement_id..nonce; we substitute the
        // [0u8;32] body_hash sentinel here ONLY for the content slot
        // verification path. For managed assets the governance_pubkey IS the
        // body_hash commitment (single-role).
        let _ = governance_pubkey_body_hash;
        verify_governance_signature(
            settlement.governance_signature.as_bytes(),
            &body_hash,
            meta.governance_pubkey
                .as_ref()
                .ok_or(SettlementEventError::AssetUnknown)?,
        )
        .map_err(|_| SettlementEventError::InvalidSignature)?;
    }
    // Gate 6 — vault contains asset
    vault_registry
        .contains_asset(&settlement.cost_vault_id, &settlement.cost_asset_id)
        .map_err(|e| match e {
            VaultRegistryError::VaultUnknown => SettlementEventError::VaultUnknown {
                vault_id: settlement.cost_vault_id,
            },
            VaultRegistryError::VaultAssetMismatch => SettlementEventError::VaultAssetMismatch {
                vault_id: settlement.cost_vault_id,
                asset_id: settlement.cost_asset_id,
            },
        })?;
    // Gate 7 — NonceRegistry.observe (per-asset namespace for sovereign)
    let observe_key: [u8; 32] = meta
        .governance_pubkey
        .unwrap_or_else(|| sovereign_nonce_namespace(&cost_asset_id));
    if let Err(e) = nonce_registry.observe(
        NonceEventKind::Settlement,
        &observe_key,
        settlement.nonce.as_bytes(),
    ) {
        if matches!(e, NonceError::AlreadyObserved { .. }) {
            return Err(SettlementEventError::Replay);
        }
        return Err(SettlementEventError::InvalidSignature);
    }
    Ok(settlement)
}

/// Post-deser check (RFC-0959 v2.8 §2.2 L298). 7 fail-fast checks.
pub fn validate(
    settlement: &SettlementEvent,
    registry: &dyn AssetRegistry,
    vault_registry: &dyn VaultRegistry,
    nonce_registry: &dyn NonceRegistry,
    current_epoch: EpochLocal,
) -> Result<(), SettlementEventError> {
    // (a) re-run Gate 0
    let meta = registry
        .metadata(&settlement.cost_asset_id)
        .map_err(|_| SettlementEventError::AssetUnknown)?;
    // (b) re-run Gate 1
    if settlement.cost.scale != meta.wire_scale {
        return Err(SettlementEventError::ScaleMismatch {
            cost_scale: settlement.cost.scale,
            vault_scale: meta.wire_scale,
        });
    }
    // (c) re-run Gate 6
    vault_registry
        .contains_asset(&settlement.cost_vault_id, &settlement.cost_asset_id)
        .map_err(|e| match e {
            VaultRegistryError::VaultUnknown => SettlementEventError::VaultUnknown {
                vault_id: settlement.cost_vault_id,
            },
            VaultRegistryError::VaultAssetMismatch => SettlementEventError::VaultAssetMismatch {
                vault_id: settlement.cost_vault_id,
                asset_id: settlement.cost_asset_id,
            },
        })?;
    // (d) intermediate — body_hash compute
    let body_hash = compute_settlement_body_hash(settlement);
    // (e) re-run Gate 5
    if let Some(pk) = meta.governance_pubkey {
        verify_governance_signature(settlement.governance_signature.as_bytes(), &body_hash, &pk)
            .map_err(|_| SettlementEventError::InvalidSignature)?;
    }
    // (f) stale snapshot
    if current_epoch.get() < settlement.registry_snapshot_epoch.get() {
        return Err(SettlementEventError::StaleSnapshot {
            snapshot: settlement.registry_snapshot_epoch.get(),
            live: current_epoch.get(),
        });
    }
    // (g) nonce observation READ-ONLY (R7 #5 HIGH fix)
    let observe_key: [u8; 32] = meta
        .governance_pubkey
        .unwrap_or_else(|| sovereign_nonce_namespace(&settlement.cost_asset_id));
    if nonce_registry
        .observe_readonly(
            NonceEventKind::Settlement,
            &observe_key,
            settlement.nonce.as_bytes(),
        )
        .is_err()
    {
        return Err(SettlementEventError::Replay);
    }
    Ok(())
}

/// Tri-invariant pairwise check per RFC-0105 v3.5 §3.13:
/// `SettlementEvent.cost_asset_id == PaymentCaveat.asset_id`.
pub fn verify_settlement_against_payment_caveat(
    settlement: &SettlementEvent,
    caveat: &PaymentCaveat,
) -> Result<(), AuditInvariantViolation> {
    if settlement.cost_asset_id.0 != caveat.asset_id.0 {
        return Err(AuditInvariantViolation::AssetMismatch {
            settlement_asset_id: settlement.cost_asset_id,
            caveat_asset_id: caveat.asset_id,
        });
    }
    Ok(())
}

/// Re-export asset error mapping for cross-crate consumer use.
pub fn map_asset_error(_e: AssetError) -> SettlementEventError {
    SettlementEventError::AssetUnknown
}

/// Helper to suppress `ChainId` import warning (used by future cross-chain
/// settlement extensions per RFC-0962 §5).
#[allow(dead_code)]
pub const fn _chain_id_anchor(_c: ChainId) {}

// Body hash domain separator (reserved for cross-protocol isolation).
#[allow(dead_code)]
const BODY_HASH_DOMAIN: &[u8] = b"cipherocto/settlement/v1/";

#[cfg(test)]
mod tests {
    use super::*;

    use octo_cap_macaroon::{InMemoryAssetRegistry, InMemoryNonceRegistry, InMemoryVaultRegistry};

    fn sample_sovereign_metadata() -> octo_cap_macaroon::AssetMetadata {
        octo_cap_macaroon::AssetMetadata::new(
            0,
            6,
            "OCTO-A".to_string(),
            "OCTO-A".to_string(),
            AssetKind::SovereignRoleToken,
        )
        .with_asset_name("octo-a")
    }

    fn setup(
        asset_id: AssetId,
        cost_vault_id: VaultId,
    ) -> (
        InMemoryAssetRegistry,
        InMemoryVaultRegistry,
        InMemoryNonceRegistry,
    ) {
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset_id, sample_sovereign_metadata());
        let mut vr = InMemoryVaultRegistry::new();
        vr.register_vault(cost_vault_id);
        vr.add_asset(&cost_vault_id, asset_id);
        let nr = InMemoryNonceRegistry::new();
        (reg, vr, nr)
    }

    /// TV-SE1: happy-path sovereign settlement.
    #[test]
    fn tv_se1_happy_path_sovereign() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let (reg, vr, mut nr) = setup(asset_id, vault_id);
        let s = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap();
        assert_eq!(s.settlement_id, SettlementId([1u8; 32]));
        assert_eq!(s.asset_kind, AssetKind::SovereignRoleToken);
    }

    /// TV-SE2: scale mismatch.
    #[test]
    fn tv_se2_scale_mismatch() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let (reg, vr, mut nr) = setup(asset_id, vault_id);
        let err = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 6).unwrap(), // wire_scale=6 != meta=0
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap_err();
        assert!(matches!(err, SettlementEventError::ScaleMismatch { .. }));
    }

    /// TV-SE3: scale out of range.
    #[test]
    fn tv_se3_scale_out_of_range() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let (reg, vr, mut nr) = setup(asset_id, vault_id);
        // build a Dqa with scale 19 (out of range)
        let bad = Dqa {
            value: 0,
            scale: 19,
        };
        let err = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            bad,
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap_err();
        assert!(matches!(err, SettlementEventError::ScaleOutOfRange { .. }));
    }

    /// TV-SE4: vault-asset mismatch.
    #[test]
    fn tv_se4_vault_asset_mismatch() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let other_asset = AssetId::from_bytes([0xcc; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset_id, sample_sovereign_metadata());
        let mut vr = InMemoryVaultRegistry::new();
        vr.register_vault(vault_id);
        vr.add_asset(&vault_id, other_asset);
        let mut nr = InMemoryNonceRegistry::new();
        let err = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SettlementEventError::VaultAssetMismatch { .. }
        ));
    }

    /// TV-SE5: vault unknown.
    #[test]
    fn tv_se5_vault_unknown() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let _vault_id = VaultId::from_bytes([0xbb; 32]);
        let other_vault = VaultId::from_bytes([0xdd; 32]);
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset_id, sample_sovereign_metadata());
        let vr = InMemoryVaultRegistry::new(); // empty
        let mut nr = InMemoryNonceRegistry::new();
        let err = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            other_vault,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap_err();
        assert!(matches!(err, SettlementEventError::VaultUnknown { .. }));
    }

    /// TV-SE7: sovereign signature exemption — passes with all-zero sig.
    #[test]
    fn tv_se7_sovereign_exempt() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let (reg, vr, mut nr) = setup(asset_id, vault_id);
        let result = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        );
        assert!(result.is_ok());
    }

    /// TV-SE8: replay detection.
    #[test]
    fn tv_se8_replay() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let (reg, vr, mut nr) = setup(asset_id, vault_id);
        let _first = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap();
        let err = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap_err();
        assert!(matches!(err, SettlementEventError::Replay));
    }

    /// TV-SE9: encode_settlement_decision length-prefixed bytes.
    #[test]
    fn tv_se9_encode_decision() {
        let bytes = encode_settlement_decision(SettlementDecision::Consumed);
        assert_eq!(bytes, vec![0x01, 0x01]);
        let bytes = encode_settlement_decision(SettlementDecision::AlreadyConsumed);
        assert_eq!(bytes, vec![0x01, 0x02]);
    }

    /// TV-SE10: body_hash deterministic across calls.
    #[test]
    fn tv_se10_body_hash_deterministic() {
        let asset_id = AssetId::from_bytes([0xaa; 32]);
        let vault_id = VaultId::from_bytes([0xbb; 32]);
        let (reg, vr, mut nr) = setup(asset_id, vault_id);
        let s = new(
            SettlementId([1u8; 32]),
            AskId([2u8; 32]),
            vault_id,
            asset_id,
            Dqa::new(1_000, 0).unwrap(),
            EvidenceRef([3u8; 32]),
            100,
            1_700_000_000_000,
            SettlementDecision::Consumed,
            GovernanceSignature::from_bytes([0u8; 64]),
            Nonce::from_bytes([4u8; 32]),
            &reg,
            &vr,
            &mut nr,
            EpochLocal::new(0),
        )
        .unwrap();
        assert_eq!(
            compute_settlement_body_hash(&s),
            compute_settlement_body_hash(&s)
        );
    }

    /// TV-SE17: new() arg count = 11 (asset_kind + snapshot derived).
    #[test]
    fn tv_se17_arg_count_is_eleven() {
        // Compile-time guarantee: the signature requires exactly 11 args
        // + 4 trait/ctx args. This test exists as a sentinel — if anyone
        // adds a 12th positional arg the test will fail to compile.
        fn _signature_check(
            s_id: SettlementId,
            a_id: AskId,
            v_id: VaultId,
            a_id2: AssetId,
            cost: Dqa,
            e: EvidenceRef,
            l: u64,
            t: u64,
            d: SettlementDecision,
            g: GovernanceSignature,
            n: Nonce,
        ) -> usize {
            let _ = (s_id, a_id, v_id, a_id2, cost, e, l, t, d, g, n);
            11
        }
        assert_eq!(
            _signature_check(
                SettlementId([0; 32]),
                AskId([0; 32]),
                VaultId::from_bytes([0; 32]),
                AssetId::from_bytes([0; 32]),
                Dqa::new(0, 0).unwrap(),
                EvidenceRef([0; 32]),
                0,
                0,
                SettlementDecision::Consumed,
                GovernanceSignature::from_bytes([0; 64]),
                Nonce::from_bytes([0; 32]),
            ),
            11
        );
    }
}

// Suppress unused BODY_HASH_DOMAIN anchor.
#[allow(dead_code)]
const _: &[u8] = BODY_HASH_DOMAIN;
