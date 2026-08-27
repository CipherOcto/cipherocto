//! Mission B (RFC-0960 v3.7) EventLogProducer substrate.
//!
//! Per RFC-0960 v3.7 §2.4 (invalidation bus) + §2.5 (EventLogProducer
//! trait).
//!
//! ## Layer hosting
//!
//! `octo-vault` is Layer B (RFC-driven, additive only). The trait
//! definitions live in Layer B; concrete impls live in Layer C crates
//! (octo-wallet-node / quota-router-sm-engine / octo-policy) per
//! `cipherocto-design-principles` — impl in Layer C avoids Layer B →
//! Layer C dependency inversion.

#![allow(missing_docs, clippy::double_must_use)]

use std::sync::{Arc, Mutex};

use octo_cap_macaroon::{
    AssetId, AssetRegistry, ChainId, Dqa, NonceRegistry, VaultAssetError, VaultId,
};

use crate::vault_balance_projection::{TransferEventLog, VaultAssetResolver};

/// `TransferEventRef` — canonical 7-field transfer event reference
/// (RFC-0960 v3.7 §2.5). Substrate wire form per §3.1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferEventRef {
    pub event_id: [u8; 32],
    pub chain_id: ChainId,
    pub from_vault_id: VaultId,
    pub to_vault_id: VaultId,
    pub asset_id: AssetId,
    pub amount: Dqa,
    pub occurred_at_unix: i64,
}

/// `EventLogProducer` trait (RFC-0960 v3.7 §2.5 L474-516).
///
/// Default `produce` body bundles: drain_lock → validate_pre_insert →
/// to_transfer_event → log.insert → bus.emit. Subclasses MAY override
/// `produce` but MUST re-call `validate_pre_insert` before any state
/// mutation.
pub trait EventLogProducer: Send + Sync {
    type Input;

    fn drain_lock(&self) -> &Arc<Mutex<()>>;

    fn validate_pre_insert(
        &self,
        input: &Self::Input,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError>;

    fn to_transfer_event(
        &self,
        input: Self::Input,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError>;

    #[allow(clippy::too_many_arguments)]
    fn produce(
        &self,
        input: Self::Input,
        registry: &dyn AssetRegistry,
        asset_resolver: &dyn VaultAssetResolver,
        nonce_registry: &dyn NonceRegistry,
        log: &mut dyn TransferEventLog,
        bus: &dyn VaultProjectionInvalidationEmitter,
        current_unix_seconds: i64,
    ) -> Result<TransferEventRef, ProducerError> {
        let _guard = self
        .drain_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.validate_pre_insert(&input, registry, asset_resolver)?;
        let ev = self.to_transfer_event(input, registry, asset_resolver, nonce_registry)?;
        log.insert(&ev).map_err(ProducerError::from)?;
        let _ = current_unix_seconds; // reserved for downstream envelope consumers
        bus.emit(&VaultProjectionInvalidationEnvelope {
            chain_id: ev.chain_id,
            vault_id: ev.to_vault_id,
            asset_id: ev.asset_id,
            source_kind: crate::vault_balance_projection::ProjectionSource::FreshLogScan,
        });
        Ok(ev)
    }
}

/// `TransferEventLog::insert` errors (re-exported from vault_balance_projection).
pub use crate::vault_balance_projection::TransferEventLogInsertError;

// Marker alias: `insert` is a method on `TransferEventLog` itself per
// Mission B. This empty trait keeps the `TransferEventLogInsert` name
// available for downstream consumers that want a bound name.
impl<T: TransferEventLog + ?Sized> TransferEventLogInsert for T {}

/// `ProducerError` (RFC-0960 v3.7 §2.5).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProducerError {
    /// Tri-invariant (RFC-0105 v3.5 §3.13) producer-side rejection.
    #[error("tri-invariant violation: {0}")]
    TriInvariantViolation(String),
    /// `TransferEventLog::insert` failure.
    #[error("log insert failed: {0}")]
    LogInsertFailed(#[from] TransferEventLogInsertError),
    /// `VaultAssetError` mapping.
    #[error("vault asset error: {0:?}")]
    VaultAsset(#[from] VaultAssetError),
}

/// `VaultProjectionInvalidationEnvelope` (RFC-0960 v3.7 §2.4).
///
/// Wire form: serialized to `cache:projection:<hex(vault_id)>` Stoolap
/// pub/sub channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultProjectionInvalidationEnvelope {
    pub chain_id: ChainId,
    pub vault_id: VaultId,
    pub asset_id: AssetId,
    pub source_kind: crate::vault_balance_projection::ProjectionSource,
}

/// Re-export of the `TransferEventLog::insert` method marker trait
/// alias (insert lives on `TransferEventLog` itself per Mission B;
/// this alias keeps the trait name stable for downstream consumers).
pub trait TransferEventLogInsert: TransferEventLog {}
pub trait VaultProjectionInvalidationEmitter: Send + Sync {
    fn emit(&self, envelope: &VaultProjectionInvalidationEnvelope);
}

/// Cache channel naming convention (RFC-0960 v3.7 §2.4).
/// `cache:projection:<hex(vault_id)>`
#[must_use]
pub fn cache_channel_name(vault_id: &VaultId) -> String {
    format!("cache:projection:{}", hex::encode(vault_id.as_bytes()))
}

// ============================================================================
// Process-wide drain_lock singleton
// ============================================================================

use std::sync::OnceLock;

/// Process-wide drain_lock (RFC-0960 v3.7 §2.5). Shared across all 3
/// producer impls to enforce serial access to `TransferEventLog::insert`.
#[must_use]
pub fn process_drain_lock() -> &'static Arc<Mutex<()>> {
    static LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    LOCK.get_or_init(|| Arc::new(Mutex::new(())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_channel_name_format() {
        let v = VaultId::from_bytes([0xab; 32]);
        let name = cache_channel_name(&v);
        assert!(name.starts_with("cache:projection:"));
        assert_eq!(name.len(), "cache:projection:".len() + 64);
    }

    #[test]
    fn process_drain_lock_singleton() {
        let a = process_drain_lock();
        let b = process_drain_lock();
        assert!(Arc::ptr_eq(a, b));
    }

    #[test]
    fn envelope_eq() {
        let e1 = VaultProjectionInvalidationEnvelope {
            chain_id: ChainId::from_bytes([1u8; 32]),
            vault_id: VaultId::from_bytes([2u8; 32]),
            asset_id: AssetId::from_bytes([3u8; 32]),
            source_kind: crate::vault_balance_projection::ProjectionSource::FreshLogScan,
        };
        let e2 = e1.clone();
        assert_eq!(e1, e2);
    }
}
