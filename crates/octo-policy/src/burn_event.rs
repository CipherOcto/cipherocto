//! Mission F (RFC-0960) BurnEventRef substrate + 3-sink atomicity.
//!
//! Per RFC-0960 §2 (BurnEventRef Specification) + §3 (Wire Form).
//!
//! ## 3-sink atomicity (R7 CRITICAL #3)
//!
//! `consume()` orchestrates THREE sinks in sequence:
//! 1. `nonce_registry.observe(NonceEventKind::Burn, &pk, &nonce)`
//! 2. `audit_sink.write(...)`
//! 3. `producer.log.insert(...)` (TransferEventLog write)
//!
//! If ANY sink fails AFTER a prior sink succeeded, ALL prior sinks
//! MUST be rolled back atomically. Without rollback, a log-insert
//! failure would burn the nonce bucket + write audit + skip the
//! TransferEventLog — caller sees `Replay` on retry but
//! `VaultBalanceProjection` is silently short by the burn amount.

use std::collections::HashSet;

use octo_cap_macaroon::{
    blake3_hash, sovereign_nonce_namespace, verify_governance_signature, AssetId, AssetKind,
    AssetRegistry, ChainId, Dqa, Epoch, GovernanceSignature, Nonce, NonceError, NonceEventKind,
    NonceRegistry, PaymentCaveat, VaultAssetError, VaultId, VaultRegistry,
};
use thiserror::Error;

/// Sentinel `VaultId` used for sovereign role-token burns (no vault
/// containment — burned by chain rule per RFC-0960 §3.3).
pub const ZERO_VAULT_ID: VaultId = VaultId::from_bytes([0u8; 32]);

/// 32-byte typed wrapper for a `SettlementEvent` reference
/// (RFC-0960 §2.2). Deprecated alias `SettlementRef`
/// preserved for one substrate cycle per L182.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize,
)]
pub struct SettlementId(pub [u8; 32]);

/// Deprecated alias (RFC-0960 §2.2 — 1 cycle).
#[deprecated(note = "use SettlementId per RFC-0960 §2.2")]
pub type SettlementRef = SettlementId;

/// 11-field BurnEventRef struct per RFC-0960 §2.1.
///
/// Wire form per §3.1: Borsh field order. JSON/serde canonical-serial
/// form is available via `octo_determin::Dqa` + substrate adapters
/// (see `PaymentCaveat` Mission E for the equivalent pattern).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct BurnEventRef {
    /// Chain identifier.
    pub chain_id: ChainId,
    /// Vault identifier (or ZERO_VAULT_ID for sovereign burns).
    pub vault_id: VaultId,
    /// Asset identifier.
    pub asset_id: AssetId,
    /// Asset kind (must match registered kind at validate()).
    pub asset_kind: AssetKind,
    /// Burn amount (Dqa, scale bound to asset's wire_scale).
    pub amount: Dqa,
    /// Ledger height of the burn event.
    pub ledger_height: u64,
    /// Reference to the settlement event that authorized the burn.
    pub settlement_event_ref: SettlementId,
    /// Ed25519 signature (all-zeros for sovereign burns).
    pub governance_signature: GovernanceSignature,
    /// PINNED governance pubkey (managed: meta.governance_pubkey;
    /// sovereign: sovereign_nonce_namespace(asset_id)).
    pub governance_pubkey: [u8; 32],
    /// Registry snapshot epoch at mint time.
    pub registry_snapshot_epoch: Epoch,
    /// Anti-replay nonce.
    pub nonce: Nonce,
}

/// BurnEventRef errors (RFC-0960 §2.1). 11 variants.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum BurnEventError {
    #[error("asset_id not registered (or tombstoned)")]
    AssetUnknown,
    #[error("amount.wire_scale {amount_wire_scale} != asset.wire_scale {asset_wire_scale}")]
    ScaleMismatch {
        amount_wire_scale: u8,
        asset_wire_scale: u8,
    },
    #[error("wire_scale {scale} exceeds MAX_SCALE = 18")]
    ScaleOutOfRange { scale: u8 },
    #[error("governance signature verification failed")]
    InvalidSignature,
    #[error("nonce replay (prior_height = {prior_height})")]
    Replay { prior_height: u64 },
    #[error("stale snapshot: snapshot = {snapshot}, live = {live}")]
    StaleSnapshot { snapshot: u64, live: u64 },
    #[error("asset_kind mismatch: claimed = {claimed:?}, registered = {registered:?}")]
    AssetKindMismatch {
        claimed: AssetKind,
        registered: AssetKind,
    },
    #[error("vault_id not registered: {vault_id:?}")]
    VaultUnknown { vault_id: VaultId },
    #[error("vault {vault_id:?} does not contain asset {asset_id:?}")]
    VaultAssetMismatch {
        vault_id: VaultId,
        asset_id: AssetId,
    },
    #[error("audit sink failed: {sink_error:?}")]
    AuditSinkFailed { sink_error: AuditError },
    #[error("atomicity rollback failed (nonce unobserve): {nonce_error}")]
    AtomicityRollbackFailed { nonce_error: String },
    #[error("legacy form on non-OCTO-W context: claimed_asset_id = {claimed_asset_id:?}")]
    LegacyFormOnNonOctoWContext { claimed_asset_id: AssetId },
}

/// AuditSink errors (RFC-0960 §2.2 + R7 CRITICAL #3
/// `LogInsertFailed` extension).
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuditError {
    #[error("audit sink write failed")]
    WriteFailed,
    #[error("audit sink compensate failed")]
    CompensateFailed,
    #[error(
        "transfer event log insert failed (sink = {sink:?}, nonce_rolled_back = {nonce_rolled_back}, audit_compensated = {audit_compensated})"
    )]
    LogInsertFailed {
        sink: String,
        nonce_rolled_back: bool,
        audit_compensated: bool,
    },
    #[error("unobserve failed during rollback")]
    UnobserveFailed(String),
}

/// AuditSink trait (RFC-0960 §2.2).
pub trait AuditSink: Send + Sync {
    fn write(&mut self, event: &BurnEventRef) -> Result<(), AuditError>;
    fn compensate(&mut self, event: &BurnEventRef) -> Result<(), AuditError>;
}

/// TransferEventLog port — re-exported from `octo_vault` (canonical Layer B
/// substrate per `cipherocto-design-principles` §No parallel abstractions).
pub use octo_vault::TransferEventLog;

/// Audit invariant violation (RFC-0105 §3.13 tri-invariant pair).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditInvariantViolation {
    #[error("asset_id mismatch: burn = {burn_asset_id:?}, caveat = {caveat_asset_id:?}")]
    AssetMismatch {
        burn_asset_id: AssetId,
        caveat_asset_id: AssetId,
    },
}

/// Compute the body_hash per RFC-0960 §2.2.
///
/// Hash input is the concatenation of:
/// `chain_id || vault_id || asset_id || asset_kind_tag ||
/// amount.wire_scale (u8 BE) || amount.value (i64 BE) ||
/// ledger_height (u64 BE) || settlement_event_ref ||
/// registry_snapshot_epoch.0 (u64 BE) || nonce`
#[must_use]
pub fn compute_body_hash(burn: &BurnEventRef) -> [u8; 32] {
    let mut buf: Vec<u8> = Vec::with_capacity(32 * 6 + 1 + 8 + 8 + 8 + 32 + 8 + 32);
    buf.extend_from_slice(BODY_HASH_DOMAIN);
    buf.extend_from_slice(burn.chain_id.as_bytes());
    buf.extend_from_slice(burn.vault_id.as_bytes());
    buf.extend_from_slice(burn.asset_id.as_bytes());
    buf.push(burn.asset_kind as u8);
    buf.push(burn.amount.scale);
    buf.extend_from_slice(&burn.amount.value.to_be_bytes());
    buf.extend_from_slice(&burn.ledger_height.to_be_bytes());
    buf.extend_from_slice(burn.settlement_event_ref.0.as_ref());
    buf.extend_from_slice(&burn.registry_snapshot_epoch.get().to_be_bytes());
    buf.extend_from_slice(burn.nonce.as_bytes());
    blake3_hash(&buf)
}

/// Body hash domain separator prefix for cross-protocol isolation.
const BODY_HASH_DOMAIN: &[u8] = b"cipherocto/burn/v1/";

/// Build the canonical 7-gate constructor (RFC-0960 §2.2).
///
/// Gates (canonical order):
/// - Gate 0: `registry.metadata(&asset_id)` resolves + not tombstoned
/// - Gate 1: `amount.wire_scale == meta.wire_scale`
/// - Gate 2: `amount.wire_scale <= MAX_SCALE`
/// - Gate 3: `vault_registry.contains_asset(&vault_id, &asset_id)`
/// - Gate 4: resolve `governance_pubkey` (sovereign fallback to
///   `sovereign_nonce_namespace(&asset_id)`)
/// - Gate 5: `compute_body_hash(...)`
/// - Gate 6: `verify_governance_signature(...)` (sovereign EXEMPT)
pub fn new(
    chain_id: ChainId,
    vault_id: VaultId,
    asset_id: AssetId,
    asset_kind: AssetKind,
    amount: Dqa,
    ledger_height: u64,
    settlement_event_ref: SettlementId,
    governance_signature: GovernanceSignature,
    registry_snapshot_epoch: Epoch,
    nonce: Nonce,
    registry: &dyn AssetRegistry,
    vault_registry: &dyn VaultRegistry,
    current_epoch: Epoch,
) -> Result<BurnEventRef, BurnEventError> {
    // Gate 0
    let meta = registry
        .metadata(&asset_id)
        .map_err(|_| BurnEventError::AssetUnknown)?;
    // Gate 1
    if amount.scale != meta.wire_scale {
        return Err(BurnEventError::ScaleMismatch {
            amount_wire_scale: amount.scale,
            asset_wire_scale: meta.wire_scale,
        });
    }
    // Gate 2
    if amount.scale > 18 {
        return Err(BurnEventError::ScaleOutOfRange {
            scale: amount.scale,
        });
    }
    // Gate 3 (sovereign burns skip — no vault containment)
    if vault_id != ZERO_VAULT_ID {
        vault_registry
            .contains_asset(&vault_id, &asset_id)
            .map_err(|e| match e {
                VaultAssetError::VaultUnknown => BurnEventError::VaultUnknown { vault_id },
                VaultAssetError::VaultAssetMismatch => {
                    BurnEventError::VaultAssetMismatch { vault_id, asset_id }
                }
            })?;
    }
    // Gate 4 — resolve governance_pubkey
    let governance_pubkey: [u8; 32] = match meta.governance_pubkey {
        Some(pk) => pk,
        None => sovereign_nonce_namespace(&asset_id),
    };
    // Gate 5 — compute body_hash
    let burn = BurnEventRef {
        chain_id,
        vault_id,
        asset_id,
        asset_kind,
        amount,
        ledger_height,
        settlement_event_ref,
        governance_signature,
        governance_pubkey,
        registry_snapshot_epoch,
        nonce,
    };
    let body_hash = compute_body_hash(&burn);
    // Gate 6 — sovereign EXEMPT (no governance_pubkey in registry means
    // chain rule, not vault governance)
    if meta.governance_pubkey.is_some() {
        verify_governance_signature(
            governance_signature.as_bytes(),
            &body_hash,
            &governance_pubkey,
        )
        .map_err(|_| BurnEventError::InvalidSignature)?;
    }
    // Stale-snapshot detection (per §2.2)
    if registry_snapshot_epoch.get() > current_epoch.get() {
        return Err(BurnEventError::StaleSnapshot {
            snapshot: registry_snapshot_epoch.get(),
            live: current_epoch.get(),
        });
    }
    Ok(burn)
}

/// Validate a `BurnEventRef` against current registry + vault state
/// (RFC-0960 §2.2). 7 checks for offline audit integrity.
pub fn validate(
    burn: &BurnEventRef,
    registry: &dyn AssetRegistry,
    vault_registry: &dyn VaultRegistry,
    current_epoch: Epoch,
) -> Result<(), BurnEventError> {
    let meta = registry
        .metadata(&burn.asset_id)
        .map_err(|_| BurnEventError::AssetUnknown)?;
    if burn.amount.scale != meta.wire_scale {
        return Err(BurnEventError::ScaleMismatch {
            amount_wire_scale: burn.amount.scale,
            asset_wire_scale: meta.wire_scale,
        });
    }
    if burn.vault_id != ZERO_VAULT_ID {
        vault_registry
            .contains_asset(&burn.vault_id, &burn.asset_id)
            .map_err(|e| match e {
                VaultAssetError::VaultUnknown => BurnEventError::VaultUnknown {
                    vault_id: burn.vault_id,
                },
                VaultAssetError::VaultAssetMismatch => BurnEventError::VaultAssetMismatch {
                    vault_id: burn.vault_id,
                    asset_id: burn.asset_id,
                },
            })?;
    }
    if burn.asset_kind != meta.kind {
        return Err(BurnEventError::AssetKindMismatch {
            claimed: burn.asset_kind,
            registered: meta.kind,
        });
    }
    if burn.registry_snapshot_epoch.get() > current_epoch.get() {
        return Err(BurnEventError::StaleSnapshot {
            snapshot: burn.registry_snapshot_epoch.get(),
            live: current_epoch.get(),
        });
    }
    let body_hash = compute_body_hash(burn);
    if meta.governance_pubkey.is_some() {
        verify_governance_signature(
            burn.governance_signature.as_bytes(),
            &body_hash,
            &burn.governance_pubkey,
        )
        .map_err(|_| BurnEventError::InvalidSignature)?;
    }
    Ok(())
}

/// Tri-invariant pairwise check per RFC-0105 §3.13:
/// `BurnEventRef.asset_id == PaymentCaveat.asset_id`.
pub fn verify_burn_against_caveat(
    burn: &BurnEventRef,
    caveat: &PaymentCaveat,
) -> Result<(), AuditInvariantViolation> {
    if burn.asset_id.0 != caveat.asset_id.0 {
        return Err(AuditInvariantViolation::AssetMismatch {
            burn_asset_id: burn.asset_id,
            caveat_asset_id: caveat.asset_id,
        });
    }
    Ok(())
}

/// Atomic 2-sink orchestration per RFC-0960 §2.2 +
/// R7 CRITICAL #3 cross-sink rollback. The third sink (transfer log
/// insert) is owned by `produce_burn` (Layer B `TransferEventLog`); the
/// parallel `octo-policy::TransferEventLog` trait was eliminated under
/// mission `l4-parallel-transfer-event-log-elimination`.
pub fn consume(
    burn: &BurnEventRef,
    nonce_registry: &mut dyn NonceRegistry,
    audit_sink: &mut dyn AuditSink,
) -> Result<(), BurnEventError> {
    // Sink (1) — nonce observe
    if let Err(e) = nonce_registry.observe(
        NonceEventKind::Burn,
        &burn.governance_pubkey,
        burn.nonce.as_bytes(),
    ) {
        if matches!(e, NonceError::AlreadyObserved { .. }) {
            return Err(BurnEventError::Replay {
                prior_height: burn.ledger_height,
            });
        }
        return Err(BurnEventError::AuditSinkFailed {
            sink_error: AuditError::UnobserveFailed(format!("observe failed: {e:?}")),
        });
    }
    // Sink (2) — audit sink write
    if let Err(audit_err) = audit_sink.write(burn) {
        // Rollback (1) — propagate unobserve failure (R1 SECURITY: do NOT
        // silently swallow; caller must distinguish "audit failed + nonce
        // rolled back" from "audit failed + nonce stuck").
        let rollback = nonce_registry.unobserve(
            NonceEventKind::Burn,
            &burn.governance_pubkey,
            burn.nonce.as_bytes(),
        );
        if let Err(nonce_err) = rollback {
            return Err(BurnEventError::AtomicityRollbackFailed {
                nonce_error: format!("audit_err={audit_err:?}; unobserve_err={nonce_err:?}"),
            });
        }
        return Err(BurnEventError::AuditSinkFailed {
            sink_error: audit_err,
        });
    }
    Ok(())
}

/// In-memory `AuditSink` for tests (RFC-0960 §2.2).
#[derive(Debug, Default)]
pub struct InMemoryAuditSink {
    pub writes: Vec<[u8; 32]>,
    pub compensates: Vec<[u8; 32]>,
    pub fail_write: bool,
}

impl AuditSink for InMemoryAuditSink {
    fn write(&mut self, event: &BurnEventRef) -> Result<(), AuditError> {
        if self.fail_write {
            return Err(AuditError::WriteFailed);
        }
        let h = compute_body_hash(event);
        self.writes.push(h);
        Ok(())
    }
    fn compensate(&mut self, event: &BurnEventRef) -> Result<(), AuditError> {
        let h = compute_body_hash(event);
        self.compensates.push(h);
        Ok(())
    }
}

/// In-memory `TransferEventLog` fixture placeholder (deleted under mission
/// `l4-parallel-transfer-event-log-elimination` — the parallel trait was
/// eliminated in favour of `octo_vault::TransferEventLog`; per-test fixtures
/// live in `octo-vault/src/testing.rs` per follow-on cycle).
#[derive(Debug, Default)]
pub struct InMemoryTransferEventLog {
    pub inserts: Vec<[u8; 32]>,
    pub fail_insert: bool,
}

// Suppress unused BODY_HASH_DOMAIN (RFC-0960 §3 reserves domain
// separator for future BLAKE3-prefixed variants; current body_hash
// uses raw concatenation only).
#[allow(dead_code)]
const _: &[u8] = BODY_HASH_DOMAIN;

// Mark HashSet as used for the InMemoryVaultRegistry import chain.
#[allow(dead_code)]
fn _hashset_anchor() -> HashSet<u8> {
    HashSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use octo_cap_macaroon::{
        AssetMetadata, InMemoryAssetRegistry, InMemoryNonceRegistry, InMemoryVaultRegistry,
        PaymentCaveat,
    };
    use octo_determin::Dqa;
    use rand::RngCore;

    fn sample_key() -> (SigningKey, [u8; 32]) {
        let mut secret = [0u8; 32];
        rand::rng().fill_bytes(&mut secret);
        let sk = SigningKey::from_bytes(&secret);
        let pk_bytes = sk.verifying_key().to_bytes();
        (sk, pk_bytes)
    }

    fn octow_metadata(pk: [u8; 32]) -> AssetMetadata {
        AssetMetadata::new(
            0,
            6,
            "OCTO-W".to_string(),
            "OCTO-W".to_string(),
            AssetKind::OctoW,
        )
        .with_asset_name("octo-w")
        .with_governance_pubkey(pk)
    }

    fn sovereign_metadata(_asset_id: AssetId) -> AssetMetadata {
        AssetMetadata::new(
            0,
            6,
            "OCTO-A".to_string(),
            "OCTO-A".to_string(),
            AssetKind::SovereignRoleToken,
        )
        .with_asset_name("octo-a")
    }

    fn setup_managed(
        asset_id: AssetId,
        pk: [u8; 32],
        vault_id: VaultId,
    ) -> (InMemoryAssetRegistry, InMemoryVaultRegistry) {
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset_id, octow_metadata(pk));
        let mut vr = InMemoryVaultRegistry::new();
        vr.register_vault(vault_id);
        vr.add_asset(&vault_id, asset_id);
        (reg, vr)
    }

    fn sign_burn(sk: &SigningKey, burn: &mut BurnEventRef) {
        let body_hash = compute_body_hash(burn);
        let sig = sk.sign(&body_hash).to_bytes();
        burn.governance_signature = GovernanceSignature::from_bytes(sig);
    }

    /// TV-BE1: happy-path managed asset burn.
    #[test]
    fn tv_be1_happy_path_managed() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let mut burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: pk,
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        sign_burn(&sk, &mut burn);
        let out = new(
            burn.chain_id,
            burn.vault_id,
            burn.asset_id,
            burn.asset_kind,
            burn.amount,
            burn.ledger_height,
            burn.settlement_event_ref,
            burn.governance_signature,
            burn.registry_snapshot_epoch,
            burn.nonce,
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap();
        assert_eq!(out.amount.value, 1_000);
    }

    /// TV-BE2: Gate 1 violation — scale mismatch.
    #[test]
    fn tv_be2_scale_mismatch() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (_sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let err = new(
            ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            AssetKind::OctoW,
            Dqa::new(1_000, 6).unwrap(), // wire_scale 6 != registered 0
            100,
            SettlementId([4u8; 32]),
            GovernanceSignature::from_bytes([0u8; 64]),
            Epoch::new(0),
            Nonce::from_bytes([5u8; 32]),
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap_err();
        assert!(matches!(err, BurnEventError::ScaleMismatch { .. }));
    }

    /// TV-BE4: Gate 3 — vault unknown.
    #[test]
    fn tv_be4_vault_unknown() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (_sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        // Remove vault from registry
        let _ = vr;
        let vr2 = InMemoryVaultRegistry::new(); // empty
        let err = new(
            ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            AssetKind::OctoW,
            Dqa::new(1_000, 0).unwrap(),
            100,
            SettlementId([4u8; 32]),
            GovernanceSignature::from_bytes([0u8; 64]),
            Epoch::new(0),
            Nonce::from_bytes([5u8; 32]),
            &reg,
            &vr2,
            Epoch::new(1),
        )
        .unwrap_err();
        assert!(matches!(err, BurnEventError::VaultUnknown { .. }));
    }

    /// TV-BE5: Gate 3 — vault-asset mismatch.
    #[test]
    fn tv_be5_vault_asset_mismatch() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let other_asset = AssetId::from_bytes([99u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (_sk, pk) = sample_key();
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset_id, octow_metadata(pk));
        let mut vr = InMemoryVaultRegistry::new();
        vr.register_vault(vault_id);
        vr.add_asset(&vault_id, other_asset); // vault has other_asset, not asset_id
        let err = new(
            ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            AssetKind::OctoW,
            Dqa::new(1_000, 0).unwrap(),
            100,
            SettlementId([4u8; 32]),
            GovernanceSignature::from_bytes([0u8; 64]),
            Epoch::new(0),
            Nonce::from_bytes([5u8; 32]),
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap_err();
        assert!(matches!(err, BurnEventError::VaultAssetMismatch { .. }));
    }

    /// TV-BE6: Gate 6 — forged signature.
    #[test]
    fn tv_be6_invalid_signature() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (_sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        // Use a zero signature — invalid
        let err = new(
            ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            AssetKind::OctoW,
            Dqa::new(1_000, 0).unwrap(),
            100,
            SettlementId([4u8; 32]),
            GovernanceSignature::from_bytes([0u8; 64]),
            Epoch::new(0),
            Nonce::from_bytes([5u8; 32]),
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap_err();
        assert!(matches!(err, BurnEventError::InvalidSignature));
    }

    /// TV-BE7: sovereign burn — no governance signature needed.
    #[test]
    fn tv_be7_sovereign_exemption() {
        let asset_id = AssetId::from_bytes([7u8; 32]);
        let mut reg = InMemoryAssetRegistry::new();
        reg.register(asset_id, sovereign_metadata(asset_id));
        let vr = InMemoryVaultRegistry::new();
        let burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id: ZERO_VAULT_ID, // sovereign — no vault
            asset_id,
            asset_kind: AssetKind::SovereignRoleToken,
            amount: Dqa::new(500, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: sovereign_nonce_namespace(&asset_id),
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        let out = new(
            burn.chain_id,
            burn.vault_id,
            burn.asset_id,
            burn.asset_kind,
            burn.amount,
            burn.ledger_height,
            burn.settlement_event_ref,
            burn.governance_signature,
            burn.registry_snapshot_epoch,
            burn.nonce,
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap();
        assert_eq!(out.governance_pubkey, sovereign_nonce_namespace(&asset_id));
    }

    /// TV-BE8: nonce replay — second observation rejected.
    #[test]
    fn tv_be8_replay() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let mut burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: pk,
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        sign_burn(&sk, &mut burn);
        let burn = new(
            burn.chain_id,
            burn.vault_id,
            burn.asset_id,
            burn.asset_kind,
            burn.amount,
            burn.ledger_height,
            burn.settlement_event_ref,
            burn.governance_signature,
            burn.registry_snapshot_epoch,
            burn.nonce,
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap();
        let mut nr = InMemoryNonceRegistry::new();
        let mut audit = InMemoryAuditSink::default();
        // First consume succeeds
        consume(&burn, &mut nr, &mut audit).unwrap();
        // Second consume fails with Replay
        let err = consume(&burn, &mut nr, &mut audit).unwrap_err();
        assert!(matches!(err, BurnEventError::Replay { .. }));
    }

    /// TV-BE12: consume() happy-path.
    #[test]
    fn tv_be12_consume_happy() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let mut burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: pk,
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        sign_burn(&sk, &mut burn);
        let burn = new(
            burn.chain_id,
            burn.vault_id,
            burn.asset_id,
            burn.asset_kind,
            burn.amount,
            burn.ledger_height,
            burn.settlement_event_ref,
            burn.governance_signature,
            burn.registry_snapshot_epoch,
            burn.nonce,
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap();
        let mut nr = InMemoryNonceRegistry::new();
        let mut audit = InMemoryAuditSink::default();
        consume(&burn, &mut nr, &mut audit).unwrap();
        assert_eq!(audit.writes.len(), 1);
    }

    /// TV-BE14: verify_burn_against_caveat matches.
    #[test]
    #[allow(deprecated)]
    fn tv_be14_verify_burn_against_caveat_match() {
        let asset_id = octo_cap_macaroon::octo_w_asset_id();
        let burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id,
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: [0u8; 32],
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        let caveat = PaymentCaveat::legacy_3arg(Dqa::new(1_000, 0).unwrap(), "gpt-4", u64::MAX);
        assert!(verify_burn_against_caveat(&burn, &caveat).is_ok());
    }

    /// TV-BE15: verify_burn_against_caveat rejects mismatch.
    #[test]
    #[allow(deprecated)]
    fn tv_be15_verify_burn_against_caveat_mismatch() {
        let burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id: AssetId::from_bytes([0xfeu8; 32]),
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: [0u8; 32],
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        let caveat = PaymentCaveat::legacy_3arg(Dqa::new(1_000, 0).unwrap(), "gpt-4", u64::MAX);
        let err = verify_burn_against_caveat(&burn, &caveat).unwrap_err();
        assert!(matches!(err, AuditInvariantViolation::AssetMismatch { .. }));
    }

    /// TV-BE16: borsh wire form round-trip.
    #[test]
    fn tv_be16_borsh_round_trip() {
        use borsh::BorshDeserialize;
        let burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id: AssetId::from_bytes([1u8; 32]),
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([9u8; 64]),
            governance_pubkey: [0xab; 32],
            registry_snapshot_epoch: Epoch::new(7),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        let bytes = borsh::to_vec(&burn).unwrap();
        let back = BurnEventRef::try_from_slice(&bytes).unwrap();
        assert_eq!(burn, back);
    }

    /// TV-BE18: 2-sink atomicity — audit-sink write failure rolls back nonce.
    ///
    /// Under mission `l4-parallel-transfer-event-log-elimination` Sink 3
    /// (transfer log) was removed from `consume`; the 3-sink rollback path
    /// became a 2-sink rollback path. This test guards the surviving
    /// cross-sink rollback invariant: Sink 2 (audit) failure MUST roll back
    /// Sink 1 (nonce).
    #[test]
    fn tv_be18_2sink_atomicity_rollback() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let mut burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: pk,
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        sign_burn(&sk, &mut burn);
        let burn = new(
            burn.chain_id,
            burn.vault_id,
            burn.asset_id,
            burn.asset_kind,
            burn.amount,
            burn.ledger_height,
            burn.settlement_event_ref,
            burn.governance_signature,
            burn.registry_snapshot_epoch,
            burn.nonce,
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap();
        let mut nr = InMemoryNonceRegistry::new();
        let mut audit = InMemoryAuditSink {
            fail_write: true,
            ..Default::default()
        };
        let err = consume(&burn, &mut nr, &mut audit).unwrap_err();
        match err {
            BurnEventError::AuditSinkFailed {
                sink_error: AuditError::WriteFailed,
            } => {}
            _ => panic!("expected AuditSinkFailed/WriteFailed, got {err:?}"),
        }
        // Nonce was rolled back — second consume (with audit restored) succeeds.
        audit.fail_write = false;
        consume(&burn, &mut nr, &mut audit).unwrap();
        assert_eq!(audit.writes.len(), 1);
    }

    /// TV-BE19: body_hash stability under perturbation.
    #[test]
    fn tv_be19_body_hash_stability() {
        let a = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id: AssetId::from_bytes([1u8; 32]),
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: [0u8; 32],
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        let h_a = compute_body_hash(&a);
        let h_b = compute_body_hash(&a);
        assert_eq!(h_a, h_b, "deterministic");
        // Perturb ledger_height
        let mut c = a.clone();
        c.ledger_height = 101;
        assert_ne!(h_a, compute_body_hash(&c));
    }

    /// TV-BE20: distinct nonces produce distinct observation triples.
    #[test]
    fn tv_be20_nonce_replay_by_variation() {
        let burn1 = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id: AssetId::from_bytes([1u8; 32]),
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: [0u8; 32],
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        let burn2 = BurnEventRef {
            nonce: Nonce::from_bytes([6u8; 32]),
            ..burn1.clone()
        };
        let mut nr = InMemoryNonceRegistry::new();
        let mut audit = InMemoryAuditSink::default();
        consume(&burn1, &mut nr, &mut audit).unwrap();
        consume(&burn2, &mut nr, &mut audit).unwrap();
        assert_eq!(audit.writes.len(), 2);
    }

    /// TV-BE22: stale snapshot detection.
    #[test]
    fn tv_be22_stale_snapshot() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let mut burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            asset_kind: AssetKind::OctoW,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: pk,
            registry_snapshot_epoch: Epoch::new(10), // snapshot 10 > current 5
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        sign_burn(&sk, &mut burn);
        let err = validate(&burn, &reg, &vr, Epoch::new(5)).unwrap_err();
        assert!(matches!(
            err,
            BurnEventError::StaleSnapshot {
                snapshot: 10,
                live: 5
            }
        ));
    }
}
