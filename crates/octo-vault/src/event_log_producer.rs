//! Mission B (RFC-0960) EventLogProducer substrate.
//!
//! Per RFC-0960 §2.4 (invalidation bus) + §2.5 (EventLogProducer
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
/// (RFC-0960 §2.5). Substrate wire form per §3.1.
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

/// `EventLogProducer` trait (RFC-0960 §2.5).
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
        bus.emit(&VaultProjectionInvalidationEnvelope::v1_legacy(
            ev.chain_id,
            ev.to_vault_id,
            ev.asset_id,
            crate::vault_balance_projection::ProjectionSource::FreshLogScan,
        ));
        Ok(ev)
    }
}

/// `TransferEventLog::insert` errors (re-exported from vault_balance_projection).
pub use crate::vault_balance_projection::TransferEventLogInsertError;

// Marker alias: `insert` is a method on `TransferEventLog` itself per
// Mission B. This empty trait keeps the `TransferEventLogInsert` name
// available for downstream consumers that want a bound name.
impl<T: TransferEventLog + ?Sized> TransferEventLogInsert for T {}

/// `ProducerError` (RFC-0960 §2.5).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProducerError {
    /// Tri-invariant (RFC-0105 §3.13) producer-side rejection.
    #[error("tri-invariant violation: {0}")]
    TriInvariantViolation(String),
    /// `TransferEventLog::insert` failure.
    #[error("log insert failed: {0}")]
    LogInsertFailed(#[from] TransferEventLogInsertError),
    /// `VaultAssetError` mapping.
    #[error("vault asset error: {0:?}")]
    VaultAsset(#[from] VaultAssetError),
}

/// `VaultProjectionInvalidationEnvelope` (RFC-0960 §2.4 +
/// `cache-bus-auth` Mission).
///
/// Wire form: serialized to `cache:projection:<hex(vault_id)>` Stoolap
/// pub/sub channel. v2 (post-`cache-bus-auth`) wire form carries
/// producer identity + signature + monotonic per-producer sequence.
///
/// Field layout (7 + 1 version tag):
/// - `version`: wire-form version (1 = pre-auth legacy, 2 = signed v2)
/// - `chain_id`, `vault_id`, `asset_id`, `source_kind`: payload
/// - `producer_did`: `OverlayIdentity` (RFC-0853 §3 Sovereign Identity
///   Model) identifying the producer node
/// - `sequence`: monotonic per-producer sequence number (replay defense)
/// - `producer_signature`: 64-byte ed25519 signature over the canonical
///   serialization of the prior 6 fields + cross-protocol domain
///   separator `b"cipherocto/cache-bus/invalidation/v2\0"`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VaultProjectionInvalidationEnvelope {
    pub version: u8,
    pub chain_id: ChainId,
    pub vault_id: VaultId,
    pub asset_id: AssetId,
    pub source_kind: crate::vault_balance_projection::ProjectionSource,
    pub producer_did: octo_cap_macaroon::OverlayIdentity,
    pub sequence: u64,
    pub producer_signature: [u8; 64],
}

/// Wire-form version: v1 (pre-auth legacy, signature = `[0u8; 64]`,
/// sequence = 0, producer_did = empty string) — used by tests that
/// pre-date the `cache-bus-auth` mission and by the 1-cycle warn-only
/// window per Mission `cache-bus-auth` §Risk row 1.
pub const ENVELOPE_VERSION_V1: u8 = 1;
/// Wire-form version: v2 (signed). Mandatory for production emitters
/// post-`cache-bus-auth`.
pub const ENVELOPE_VERSION_V2: u8 = 2;

/// Cross-protocol domain separator prepended to the signed preimage per
/// `cache-bus-auth` §Sub-step 3. Prevents replay across structurally
/// similar protocols that share field layout (e.g., the
/// macaroon-issuance bus or any other `chain_id || vault_id || ...`
/// concatenation).
pub const CACHE_BUS_DOMAIN_SEPARATOR: &[u8] = b"cipherocto/cache-bus/invalidation/v2\0";

impl VaultProjectionInvalidationEnvelope {
    /// Construct a v1 (legacy, unauthenticated) envelope — used by
    /// pre-`cache-bus-auth` test sites + the 1-cycle warn-only window.
    /// Producer-side signing lives at `sign_v2` (post-auth).
    #[must_use]
    pub fn v1_legacy(
        chain_id: ChainId,
        vault_id: VaultId,
        asset_id: AssetId,
        source_kind: crate::vault_balance_projection::ProjectionSource,
    ) -> Self {
        Self {
            version: ENVELOPE_VERSION_V1,
            chain_id,
            vault_id,
            asset_id,
            source_kind,
            producer_did: String::new(),
            sequence: 0,
            producer_signature: [0u8; 64],
        }
    }

    /// Canonical preimage: domain-separator || version || chain_id ||
    /// vault_id || asset_id || source_kind || producer_did || sequence.
    /// This is what `producer_signature` MUST cover (per `cache-bus-auth`
    /// §Sub-step 3).
    #[must_use]
    pub fn preimage(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(CACHE_BUS_DOMAIN_SEPARATOR.len() + 200);
        buf.extend_from_slice(CACHE_BUS_DOMAIN_SEPARATOR);
        buf.push(self.version);
        buf.extend_from_slice(self.chain_id.as_bytes());
        buf.extend_from_slice(self.vault_id.as_bytes());
        buf.extend_from_slice(self.asset_id.as_bytes());
        // source_kind discriminant (u8) — stable across versions.
        buf.push(self.source_kind as u8);
        buf.extend_from_slice(self.producer_did.as_bytes());
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf
    }
}

/// Re-export of the `TransferEventLog::insert` method marker trait
/// alias (insert lives on `TransferEventLog` itself per Mission B;
/// this alias keeps the trait name stable for downstream consumers).
pub trait TransferEventLogInsert: TransferEventLog {}
pub trait VaultProjectionInvalidationEmitter: Send + Sync {
    fn emit(&self, envelope: &VaultProjectionInvalidationEnvelope);
}

/// Cache channel naming convention (RFC-0960 §2.4).
/// `cache:projection:<hex(vault_id)>`
#[must_use]
pub fn cache_channel_name(vault_id: &VaultId) -> String {
    format!("cache:projection:{}", hex::encode(vault_id.as_bytes()))
}

// ============================================================================
// Process-wide drain_lock singleton
// ============================================================================

use std::sync::OnceLock;

/// Process-wide drain_lock (RFC-0960 §2.5). Shared across all 3
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
        let e1 = VaultProjectionInvalidationEnvelope::v1_legacy(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
            crate::vault_balance_projection::ProjectionSource::FreshLogScan,
        );
        let e2 = e1.clone();
        assert_eq!(e1, e2);
        assert_eq!(e1.version, ENVELOPE_VERSION_V1);
    }

    #[test]
    fn envelope_preimage_starts_with_domain_separator() {
        let e = VaultProjectionInvalidationEnvelope::v1_legacy(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
            crate::vault_balance_projection::ProjectionSource::FreshLogScan,
        );
        let p = e.preimage();
        assert!(
            p.starts_with(CACHE_BUS_DOMAIN_SEPARATOR),
            "preimage MUST start with cross-protocol domain separator"
        );
        assert!(p.len() > CACHE_BUS_DOMAIN_SEPARATOR.len());
    }
}
