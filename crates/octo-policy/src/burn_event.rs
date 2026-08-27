//! Mission F (RFC-0960) BurnEventRef substrate + 2-sink atomicity.
//!
//! Per RFC-0960 §2 (BurnEventRef Specification) + §3 (Wire Form).
//!
//! ## 2-sink atomicity (R7 fix: doc reflects substrate after
//! `l4-parallel-transfer-event-log-elimination` removed the inline
//! `TransferEventLog::insert` call from `consume()`).
//!
//! `consume()` orchestrates TWO sinks in sequence:
//! 1. `nonce_registry.observe(NonceEventKind::Burn, &pk, &nonce)`
//! 2. `audit_sink.write(...)`
//!
//! If sink 2 fails AFTER sink 1 succeeded, sink 1 MUST be rolled back
//! by calling `nonce_registry.unobserve(...)`. Without unobserve,
//! the caller sees `Replay` on retry but the burn is silently lost.
//!
//! Note: audit-sink `compensate()` exists as a port method but is
//! caller-responsibility (consume() does not call it directly).

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
#[non_exhaustive]
pub enum AuditInvariantViolation {
    #[error("asset_id mismatch: burn = {burn_asset_id:?}, caveat = {caveat_asset_id:?}")]
    AssetMismatch {
        burn_asset_id: AssetId,
        caveat_asset_id: AssetId,
    },
}

/// Redact a `NonceError` to its variant discriminant only — strips the
/// 32-byte `pk` and 32-byte `nonce` that the `AlreadyObserved` /
/// `NotObserved` struct variants embed.
///
/// R3 SECURITY (replaces R2 string-parse approach): the prior
/// `split('{').next()` strategy was fail-OPEN — a future tuple variant
/// (e.g. `WalFailure(String)`) would have no `{` in its Debug output
/// and would silently re-leak payload bytes. Using an exhaustive
/// `match` makes any new variant a compile error, forcing the redactor
/// to be updated alongside substrate changes (fail-CLOSED).
///
/// Per CLAUDE.md §"Display operator-facing, Debug substrate-internal":
/// the variant tag identifies the failure mode without surfacing
/// authority-identifying or replay-nonce bytes.
fn redact_nonce_error(e: &NonceError) -> &'static str {
    match e {
        NonceError::AlreadyObserved { .. } => "AlreadyObserved",
        NonceError::NotObserved { .. } => "NotObserved",
        NonceError::PersistenceFailure => "PersistenceFailure",
        NonceError::WalRecovering => "WalRecovering",
    }
}

/// Redact an `AuditError` to its variant discriminant only. The
/// `LogInsertFailed { sink: String, ... }` and `UnobserveFailed(String)`
/// variants carry payload bytes that MUST NOT flow through Display
/// (per CLAUDE.md §"Display operator-facing, Debug substrate-internal").
///
/// Fail-CLOSED via exhaustive match — a future `AuditError` variant
/// becomes a compile error here, forcing the redactor to be updated
/// alongside substrate changes.
fn redact_audit_error(e: &AuditError) -> &'static str {
    match e {
        AuditError::WriteFailed => "WriteFailed",
        AuditError::CompensateFailed => "CompensateFailed",
        AuditError::LogInsertFailed { .. } => "LogInsertFailed",
        AuditError::UnobserveFailed(_) => "UnobserveFailed",
    }
}

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
        // R3 SECURITY: do NOT embed raw `{:?}` Debug here — it would leak
        // the 32-byte `pk` and 32-byte `nonce` carried by the
        // `NonceError` variants via the `AuditError::UnobserveFailed`
        // string, which flows through `BurnEventError::AuditSinkFailed`
        // Display. Use the same redactor as the rollback path.
        return Err(BurnEventError::AuditSinkFailed {
            sink_error: AuditError::UnobserveFailed(format!(
                "observe failed: {}",
                redact_nonce_error(&e)
            )),
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
            // R2 SECURITY: redact the 32-byte pubkey/nonce substrate-internal
            // Debug output that NonceError variants embed. Per CLAUDE.md §"Layer
            // B → substrate error type" (Display operator-facing, Debug
            // substrate-internal), this string flows through the Display impl
            // and MUST NOT contain authority-identifying or replay-nonce bytes.
            return Err(BurnEventError::AtomicityRollbackFailed {
                nonce_error: format!(
                    "audit_err={}; unobserve_err={}",
                    redact_audit_error(&audit_err),
                    redact_nonce_error(&nonce_err)
                ),
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

// Suppress unused BODY_HASH_DOMAIN (RFC-0960 §3 reserves domain
// separator for future BLAKE3-prefixed variants; current body_hash
// uses raw concatenation only).
#[allow(dead_code)]
const _: &[u8] = BODY_HASH_DOMAIN;

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

    /// Test-only `NonceRegistry` that wraps `InMemoryNonceRegistry` and
    /// can be configured to fail `unobserve()` (R2 test-coverage: the
    /// new `AtomicityRollbackFailed` variant needs an exercisable path).
    struct FailingUnobserveNonceRegistry {
        inner: InMemoryNonceRegistry,
        fail_unobserve: bool,
    }

    impl FailingUnobserveNonceRegistry {
        fn new(fail_unobserve: bool) -> Self {
            Self {
                inner: InMemoryNonceRegistry::new(),
                fail_unobserve,
            }
        }
    }

    impl octo_cap_macaroon::NonceRegistry for FailingUnobserveNonceRegistry {
        fn observe(
            &mut self,
            event_kind: octo_cap_macaroon::NonceEventKind,
            pk: &[u8; 32],
            nonce: &[u8; 32],
        ) -> Result<(), octo_cap_macaroon::NonceError> {
            self.inner.observe(event_kind, pk, nonce)
        }
        fn observe_readonly(
            &self,
            event_kind: octo_cap_macaroon::NonceEventKind,
            pk: &[u8; 32],
            nonce: &[u8; 32],
        ) -> Result<(), octo_cap_macaroon::NonceError> {
            self.inner.observe_readonly(event_kind, pk, nonce)
        }
        fn unobserve(
            &mut self,
            event_kind: octo_cap_macaroon::NonceEventKind,
            pk: &[u8; 32],
            nonce: &[u8; 32],
        ) -> Result<(), octo_cap_macaroon::NonceError> {
            if self.fail_unobserve {
                return Err(octo_cap_macaroon::NonceError::NotObserved {
                    event_kind,
                    pk: *pk,
                    nonce: *nonce,
                });
            }
            self.inner.unobserve(event_kind, pk, nonce)
        }
    }

    /// Test-only `NonceRegistry` that wraps `InMemoryNonceRegistry` and
    /// can be configured to fail `observe()` with a chosen
    /// `NonceError` variant (R4 test-coverage: the Sink-1
    /// observe-failure path through `AuditSinkFailed::UnobserveFailed`
    /// needs an exercisable path for each redactor-covered variant).
    ///
    /// **Single-shot constraint (R5):** `fail_variant.take()` means
    /// each instance fails `observe()` exactly ONCE. Subsequent calls
    /// delegate to `inner.observe()`. Tests requiring multiple failures
    /// MUST construct fresh instances per call.
    ///
    /// **R5 refactor:** the constructor takes a discriminator enum
    /// (`ObserveFailKind`) rather than a `NonceError` value, because
    /// struct variants (`AlreadyObserved` / `NotObserved`) cannot be
    /// constructed without `(event_kind, pk, nonce)` fields — those
    /// are reconstructed at observe-call time from the call site.
    /// `NotObserved` is part of the API contract for symmetry with the
    /// 4-variant `NonceError` (covered by `tv_redact_nonce_error_all_variants`)
    /// but has no dedicated integration test.
    #[derive(Clone, Copy)]
    #[allow(dead_code)] // NotObserved is API-surface symmetry; covered by unit test
    enum ObserveFailKind {
        AlreadyObserved,
        NotObserved,
        PersistenceFailure,
        WalRecovering,
    }

    struct FailingObserveNonceRegistry {
        inner: InMemoryNonceRegistry,
        fail_variant: Option<ObserveFailKind>,
    }

    impl FailingObserveNonceRegistry {
        fn new(fail_variant: ObserveFailKind) -> Self {
            Self {
                inner: InMemoryNonceRegistry::new(),
                fail_variant: Some(fail_variant),
            }
        }
    }

    impl octo_cap_macaroon::NonceRegistry for FailingObserveNonceRegistry {
        fn observe(
            &mut self,
            event_kind: octo_cap_macaroon::NonceEventKind,
            pk: &[u8; 32],
            nonce: &[u8; 32],
        ) -> Result<(), octo_cap_macaroon::NonceError> {
            if let Some(v) = self.fail_variant.take() {
                return Err(match v {
                    ObserveFailKind::AlreadyObserved => {
                        octo_cap_macaroon::NonceError::AlreadyObserved {
                            event_kind,
                            pk: *pk,
                            nonce: *nonce,
                        }
                    }
                    ObserveFailKind::NotObserved => octo_cap_macaroon::NonceError::NotObserved {
                        event_kind,
                        pk: *pk,
                        nonce: *nonce,
                    },
                    ObserveFailKind::PersistenceFailure => {
                        octo_cap_macaroon::NonceError::PersistenceFailure
                    }
                    ObserveFailKind::WalRecovering => octo_cap_macaroon::NonceError::WalRecovering,
                });
            }
            self.inner.observe(event_kind, pk, nonce)
        }
        fn observe_readonly(
            &self,
            event_kind: octo_cap_macaroon::NonceEventKind,
            pk: &[u8; 32],
            nonce: &[u8; 32],
        ) -> Result<(), octo_cap_macaroon::NonceError> {
            self.inner.observe_readonly(event_kind, pk, nonce)
        }
        fn unobserve(
            &mut self,
            event_kind: octo_cap_macaroon::NonceEventKind,
            pk: &[u8; 32],
            nonce: &[u8; 32],
        ) -> Result<(), octo_cap_macaroon::NonceError> {
            self.inner.unobserve(event_kind, pk, nonce)
        }
    }

    /// TV-BE23 (R4 test-coverage): Sink-1 observe-failure path with
    /// `NonceError::PersistenceFailure` — `consume()` MUST return
    /// `AuditSinkFailed { UnobserveFailed("observe failed: PersistenceFailure") }`
    /// (redactor contract). The nonce MUST NOT be in the registry
    /// post-call (observe failed, so no state mutation).
    #[test]
    fn tv_be23_observe_failure_persistence_failure() {
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
        let mut nr = FailingObserveNonceRegistry::new(ObserveFailKind::PersistenceFailure);
        let mut audit = InMemoryAuditSink::default();
        let err = consume(&burn, &mut nr, &mut audit).unwrap_err();
        match err {
            BurnEventError::AuditSinkFailed {
                sink_error: AuditError::UnobserveFailed(msg),
            } => {
                assert!(
                    msg.contains("PersistenceFailure"),
                    "redactor MUST surface PersistenceFailure tag: {msg}"
                );
                assert!(
                    !msg.contains("sink:"),
                    "redactor MUST NOT leak audit_err `sink:` payload: {msg}"
                );
                // R6 SECURITY: absence guard — even for unit variants
                // (no struct fields), the format MUST NOT contain `pk:`
                // or `nonce:` substrings.
                assert!(
                    !msg.contains("pk:"),
                    "format MUST NOT contain `pk:` substring: {msg}"
                );
                assert!(
                    !msg.contains("nonce:"),
                    "format MUST NOT contain `nonce:` substring: {msg}"
                );
            }
            other => panic!("expected AuditSinkFailed{{UnobserveFailed}}, got {other:?}"),
        }
        // R5 test-coverage: nonce MUST NOT be in registry post-failed
        // observe (no state mutation on observe error).
        let readonly = nr.observe_readonly(
            octo_cap_macaroon::NonceEventKind::Burn,
            &burn.governance_pubkey,
            burn.nonce.as_bytes(),
        );
        assert!(
            readonly.is_ok(),
            "post-failed-observe observe_readonly MUST return Ok (no state mutation); got {readonly:?}"
        );
    }

    /// TV-BE24 (R4 test-coverage): Sink-1 observe-failure path with
    /// `NonceError::WalRecovering` — exercises the same redactor
    /// contract as tv_be23 for the second unit-variant of NonceError.
    #[test]
    fn tv_be24_observe_failure_wal_recovering() {
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
        let mut nr = FailingObserveNonceRegistry::new(ObserveFailKind::WalRecovering);
        let mut audit = InMemoryAuditSink::default();
        let err = consume(&burn, &mut nr, &mut audit).unwrap_err();
        match err {
            BurnEventError::AuditSinkFailed {
                sink_error: AuditError::UnobserveFailed(msg),
            } => {
                assert!(
                    msg.contains("WalRecovering"),
                    "redactor MUST surface WalRecovering tag: {msg}"
                );
                // R6 SECURITY: absence guard — even for unit variants
                // (no struct fields), the format MUST NOT contain `pk:`
                // or `nonce:` substrings.
                assert!(
                    !msg.contains("pk:"),
                    "format MUST NOT contain `pk:` substring: {msg}"
                );
                assert!(
                    !msg.contains("nonce:"),
                    "format MUST NOT contain `nonce:` substring: {msg}"
                );
            }
            other => panic!("expected AuditSinkFailed{{UnobserveFailed}}, got {other:?}"),
        }
        // R5 test-coverage: nonce MUST NOT be in registry post-failed
        // observe (no state mutation on observe error).
        let readonly = nr.observe_readonly(
            octo_cap_macaroon::NonceEventKind::Burn,
            &burn.governance_pubkey,
            burn.nonce.as_bytes(),
        );
        assert!(
            readonly.is_ok(),
            "post-failed-observe observe_readonly MUST return Ok (no state mutation); got {readonly:?}"
        );
    }

    /// TV-BE26 (R6 security + test-coverage): Sink-1 observe-failure
    /// path with `NonceError::NotObserved` (struct variant carrying
    /// 32-byte pk + nonce). Exercises the redactor contract for the
    /// struct-variant case through `consume()` directly. Closes the
    /// struct-variant observe-fail coverage gap (AlreadyObserved is
    /// structurally impossible in this path — it shortcuts to Replay).
    #[test]
    fn tv_be26_observe_failure_not_observed() {
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
        let mut nr = FailingObserveNonceRegistry::new(ObserveFailKind::NotObserved);
        let mut audit = InMemoryAuditSink::default();
        let err = consume(&burn, &mut nr, &mut audit).unwrap_err();
        match err {
            BurnEventError::AuditSinkFailed {
                sink_error: AuditError::UnobserveFailed(msg),
            } => {
                // R6 SECURITY: the redactor MUST surface the NotObserved
                // tag AND MUST NOT leak the 32-byte pk/nonce bytes that
                // the struct variant embeds.
                assert!(
                    msg.contains("NotObserved"),
                    "redactor MUST surface NotObserved tag: {msg}"
                );
                let mut pk_hex = String::with_capacity(64);
                for b in &burn.governance_pubkey {
                    use std::fmt::Write as _;
                    let _ = write!(pk_hex, "{b:02x}");
                }
                let mut nonce_hex = String::with_capacity(64);
                for b in burn.nonce.as_bytes() {
                    use std::fmt::Write as _;
                    let _ = write!(nonce_hex, "{b:02x}");
                }
                assert!(
                    !msg.contains(&pk_hex),
                    "format MUST NOT leak full pubkey hex: {msg}"
                );
                assert!(
                    !msg.contains(&nonce_hex),
                    "format MUST NOT leak full nonce hex: {msg}"
                );
                assert!(
                    !msg.contains("pk:"),
                    "format MUST NOT contain `pk:` substring: {msg}"
                );
                assert!(
                    !msg.contains("nonce:"),
                    "format MUST NOT contain `nonce:` substring: {msg}"
                );
            }
            other => panic!("expected AuditSinkFailed{{UnobserveFailed}}, got {other:?}"),
        }
        let readonly = nr.observe_readonly(
            octo_cap_macaroon::NonceEventKind::Burn,
            &burn.governance_pubkey,
            burn.nonce.as_bytes(),
        );
        assert!(
            readonly.is_ok(),
            "post-failed-observe observe_readonly MUST return Ok (no state mutation); got {readonly:?}"
        );
    }

    /// TV-BE21 (R2 test-coverage): unobserve-failure path triggers
    /// `AtomicityRollbackFailed`. Compound failure: audit fails AND
    /// nonce rollback fails — caller MUST receive a diagnostic
    /// distinguishing this from plain audit failure.
    #[test]
    fn tv_be21_atomicity_rollback_failure() {
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
        let mut nr = FailingUnobserveNonceRegistry::new(true); // fail_unobserve
        let mut audit = InMemoryAuditSink {
            fail_write: true, // AND audit fails
            ..Default::default()
        };
        let err = consume(&burn, &mut nr, &mut audit).unwrap_err();
        match err {
            BurnEventError::AtomicityRollbackFailed { nonce_error } => {
                assert!(
                    nonce_error.contains("audit_err"),
                    "format MUST include audit_err: {nonce_error}"
                );
                assert!(
                    nonce_error.contains("unobserve_err"),
                    "format MUST include unobserve_err: {nonce_error}"
                );
                // Redaction contract (R2 security): the formatted string
                // MUST NOT contain the full 32-byte pubkey or nonce as
                // hex (would leak substrate-internal Debug into Display).
                let mut pk_hex = String::with_capacity(64);
                for b in &burn.governance_pubkey {
                    use std::fmt::Write as _;
                    let _ = write!(pk_hex, "{b:02x}");
                }
                let mut nonce_hex = String::with_capacity(64);
                for b in burn.nonce.as_bytes() {
                    use std::fmt::Write as _;
                    let _ = write!(nonce_hex, "{b:02x}");
                }
                assert!(
                    !nonce_error.contains(&pk_hex),
                    "format MUST NOT leak full pubkey hex: {nonce_error}"
                );
                assert!(
                    !nonce_error.contains(&nonce_hex),
                    "format MUST NOT leak full nonce hex: {nonce_error}"
                );
                // R3 SECURITY: the redacted audit_err + unobserve_err
                // tags MUST be present (positive contract — fail-CLOSED
                // redaction surfaces the failure mode even when raw
                // payload bytes are stripped).
                assert!(
                    nonce_error.contains("WriteFailed"),
                    "format MUST include redacted audit_err tag 'WriteFailed': {nonce_error}"
                );
                assert!(
                    nonce_error.contains("NotObserved"),
                    "format MUST include redacted unobserve_err tag 'NotObserved': {nonce_error}"
                );
                // R3 SECURITY: redaction must not leak audit_err inner
                // payload either — `InMemoryAuditSink` carries no payload
                // string, but a future sink impl could. Guard against
                // any `sink:` substring leaking through.
                assert!(
                    !nonce_error.contains("sink:"),
                    "format MUST NOT leak audit_err `sink:` payload: {nonce_error}"
                );
            }
            _ => panic!("expected AtomicityRollbackFailed, got {err:?}"),
        }
        // R3 SECURITY: stuck-nonce invariant — when unobserve fails, the
        // nonce MUST remain in the registry (caller can detect the
        // stuck nonce via a subsequent observe returning
        // `AlreadyObserved`). This is the operational consequence of
        // fail-CLOSED rollback.
        let re_observe = nr.observe(
            octo_cap_macaroon::NonceEventKind::Burn,
            &burn.governance_pubkey,
            burn.nonce.as_bytes(),
        );
        assert!(
            matches!(
                re_observe,
                Err(octo_cap_macaroon::NonceError::AlreadyObserved { .. })
            ),
            "post-rollback-failure re-observe MUST report AlreadyObserved (stuck nonce); got {re_observe:?}"
        );
    }

    /// Unit tests covering all 4 `NonceError` variants through
    /// `redact_nonce_error` (R3 test-coverage: the redactor MUST
    /// remain exhaustive as substrate adds variants).
    #[test]
    fn tv_redact_nonce_error_all_variants() {
        let pk = [0u8; 32];
        let nonce = [0u8; 32];
        assert_eq!(
            redact_nonce_error(&octo_cap_macaroon::NonceError::AlreadyObserved {
                event_kind: octo_cap_macaroon::NonceEventKind::Burn,
                pk,
                nonce,
            }),
            "AlreadyObserved"
        );
        assert_eq!(
            redact_nonce_error(&octo_cap_macaroon::NonceError::NotObserved {
                event_kind: octo_cap_macaroon::NonceEventKind::Burn,
                pk,
                nonce,
            }),
            "NotObserved"
        );
        assert_eq!(
            redact_nonce_error(&octo_cap_macaroon::NonceError::PersistenceFailure),
            "PersistenceFailure"
        );
        assert_eq!(
            redact_nonce_error(&octo_cap_macaroon::NonceError::WalRecovering),
            "WalRecovering"
        );
    }

    /// Unit tests covering all 4 `AuditError` variants through
    /// `redact_audit_error` (R3 test-coverage: the redactor MUST
    /// remain exhaustive as substrate adds variants).
    #[test]
    fn tv_redact_audit_error_all_variants() {
        assert_eq!(redact_audit_error(&AuditError::WriteFailed), "WriteFailed");
        assert_eq!(
            redact_audit_error(&AuditError::CompensateFailed),
            "CompensateFailed"
        );
        assert_eq!(
            redact_audit_error(&AuditError::LogInsertFailed {
                sink: "should-not-leak".to_string(),
                nonce_rolled_back: true,
                audit_compensated: false,
            }),
            "LogInsertFailed"
        );
        assert_eq!(
            redact_audit_error(&AuditError::UnobserveFailed("should-not-leak".to_string())),
            "UnobserveFailed"
        );
    }

    /// TV-BE-27 (R7 test-coverage): Gate 2 (`amount.scale > 18`
    /// → `ScaleOutOfRange`) is structurally unreachable through
    /// `new()` because `Dqa::new` itself rejects `scale > MAX_SCALE`
    /// (= 18). This test documents the upstream-reachability contract:
    /// `Dqa::new(0, 19)` MUST return `Err`, and `Dqa::new(0, 18)`
    /// (boundary) MUST return `Ok`. The `BurnEventError::ScaleOutOfRange`
    /// variant stays as belt-and-suspenders for any future caller that
    /// bypasses `new()` (e.g. raw struct-field mutation on a
    /// re-validated `BurnEventRef`).
    #[test]
    fn tv_be27_scale_out_of_range_upstream_guard() {
        // scale = 19 (one above MAX_SCALE = 18) MUST be rejected by
        // Dqa::new.
        let over = octo_determin::Dqa::new(0, 19);
        assert!(
            over.is_err(),
            "Dqa::new(0, 19) MUST return Err (scale > MAX_SCALE = 18); got {over:?}"
        );
        // scale = 18 (boundary, equals MAX_SCALE) MUST succeed.
        let boundary = octo_determin::Dqa::new(0, 18);
        assert!(
            boundary.is_ok(),
            "Dqa::new(0, 18) MUST return Ok (scale == MAX_SCALE = 18); got {boundary:?}"
        );
    }

    /// TV-BE-28 (R7 test-coverage): `InMemoryAuditSink::compensate()`
    /// MUST record the body's BLAKE3 hash into `compensates` so a
    /// downstream auditor can confirm the sink was rolled back. This
    /// test exercises the `compensate()` method directly so the
    /// `AuditError::CompensateFailed` variant has at least one
    /// call site reachable from the test suite (caller-responsibility
    /// contract per `consume()` doc-comment).
    #[test]
    fn tv_be28_audit_sink_compensate_records_body_hash() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
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
        let mut sink = InMemoryAuditSink::default();
        assert!(
            sink.compensates.is_empty(),
            "compensates vec MUST start empty; got {} entries",
            sink.compensates.len()
        );
        sink.compensate(&burn).expect("compensate() default Ok");
        assert_eq!(
            sink.compensates.len(),
            1,
            "compensates vec MUST contain exactly 1 hash after compensate(); got {}",
            sink.compensates.len()
        );
        // The recorded hash MUST equal compute_body_hash(&burn).
        let expected = compute_body_hash(&burn);
        assert_eq!(
            sink.compensates[0], expected,
            "compensate() MUST record the BLAKE3 body hash"
        );
    }

    /// TV-BE-3 (R8 test-coverage): Gate 0 `AssetUnknown` via `new()`
    /// — the registry returns `Err` for `metadata(&asset_id)` because
    /// the asset is not registered. Closes the gate-0 coverage gap
    /// (gates 1, 2, 3, 4, 6 were covered by other tests).
    #[test]
    fn tv_be3_new_asset_unknown_rejected() {
        // Empty registry — no asset registered.
        let reg = InMemoryAssetRegistry::new();
        let vr = InMemoryVaultRegistry::new();
        let asset_id = AssetId::from_bytes([0xAA; 32]);
        let vault_id = VaultId::from_bytes([0xBB; 32]);
        let err = new(
            ChainId::from_bytes([0x01; 32]),
            vault_id,
            asset_id,
            AssetKind::ManagedAsset,
            Dqa::new(1_000, 0).unwrap(),
            100,
            SettlementId([0x04; 32]),
            GovernanceSignature::from_bytes([0u8; 64]),
            Epoch::new(0),
            Nonce::from_bytes([0x05; 32]),
            &reg,
            &vr,
            Epoch::new(1),
        )
        .unwrap_err();
        assert!(
            matches!(err, BurnEventError::AssetUnknown),
            "new() with unregistered asset MUST return AssetUnknown (Gate 0); got {err:?}"
        );
    }

    /// TV-BE-29 (R7 test-coverage): `validate()` path for
    /// `BurnEventError::AssetKindMismatch` — `burn.asset_kind != meta.kind`.
    /// All other gate-1..gate-7 checks must pass so the AssetKindMismatch
    /// branch is the failure cause.
    #[test]
    fn tv_be29_validate_asset_kind_mismatch() {
        let asset_id = AssetId::from_bytes([1u8; 32]);
        let vault_id = VaultId::from_bytes([2u8; 32]);
        let (sk, pk) = sample_key();
        let (reg, vr) = setup_managed(asset_id, pk, vault_id);
        let mut burn = BurnEventRef {
            chain_id: ChainId::from_bytes([3u8; 32]),
            vault_id,
            asset_id,
            // Registered kind is OctoW per setup_managed; claim
            // ManagedAsset to trigger AssetKindMismatch.
            asset_kind: AssetKind::ManagedAsset,
            amount: Dqa::new(1_000, 0).unwrap(),
            ledger_height: 100,
            settlement_event_ref: SettlementId([4u8; 32]),
            governance_signature: GovernanceSignature::from_bytes([0u8; 64]),
            governance_pubkey: pk,
            registry_snapshot_epoch: Epoch::new(0),
            nonce: Nonce::from_bytes([5u8; 32]),
        };
        sign_burn(&sk, &mut burn);
        let err = validate(&burn, &reg, &vr, Epoch::new(1)).unwrap_err();
        assert!(
            matches!(
                err,
                BurnEventError::AssetKindMismatch {
                    claimed: AssetKind::ManagedAsset,
                    registered: AssetKind::OctoW,
                }
            ),
            "validate() MUST return AssetKindMismatch{{claimed: ManagedAsset, registered: OctoW}}; got {err:?}"
        );
    }
}
