//! Test fixtures for the octo-vault substrate (Layer B test-only).
//!
//! Reusable across Layer C test sites that need to wire
//! `produce_burn` / `produce_payment` / `produce_settlement` per mission
//! `producer-wrapper-consumer-wiring.md` sub-step 6.
//!
//! Composition-over-inheritance: each fixture implements a port trait,
//! not an enum. New variants land as new structs without touching the
//! substrate (per `cipherocto-design-principles` §Extension over
//! enumeration).

#![allow(missing_docs)]

use std::sync::Mutex;

use octo_cap_macaroon::{AssetId, ChainId, Dqa, VaultId};

use crate::event_log_producer::{
    TransferEventLogInsertError, TransferEventRef, VaultProjectionInvalidationEmitter,
    VaultProjectionInvalidationEnvelope,
};
use crate::vault_balance_projection::{
    ProjectionError, TransferEventLog, VaultAssetResolver, VaultAssetResolverError,
};

/// In-memory `TransferEventLog` fixture — records every `insert` call.
///
/// Default: `Default` impl produces an empty fixture. Tests can
/// pre-seed via [`StubTransferEventLog::with_events`] for
/// `sum_to_vault` / `sum_from_vault` projections.
#[derive(Debug, Default)]
pub struct StubTransferEventLog {
    events: Mutex<Vec<TransferEventRef>>,
    fail_insert: bool,
}

impl StubTransferEventLog {
    /// Construct a fixture pre-seeded with events.
    #[must_use]
    pub fn with_events(events: Vec<TransferEventRef>) -> Self {
        Self {
            events: Mutex::new(events),
            fail_insert: false,
        }
    }

    /// Construct a fixture that fails every insert with the supplied error
    /// (TV-PW-6 negative path).
    #[must_use]
    pub fn failing_insert() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            fail_insert: true,
        }
    }

    /// Snapshot of all events recorded so far.
    #[must_use]
    pub fn events(&self) -> Vec<TransferEventRef> {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Number of events recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Is the fixture empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl TransferEventLog for StubTransferEventLog {
    fn sum_to_vault(
        &self,
        _chain_id: &ChainId,
        _vault_id: &VaultId,
        _asset_id: &AssetId,
        _occurred_at_unix_floor: i64,
    ) -> Result<Dqa, ProjectionError> {
        Ok(Dqa::new(0, 0).unwrap())
    }

    fn sum_from_vault(
        &self,
        _chain_id: &ChainId,
        _vault_id: &VaultId,
        _asset_id: &AssetId,
        _occurred_at_unix_floor: i64,
    ) -> Result<Dqa, ProjectionError> {
        Ok(Dqa::new(0, 0).unwrap())
    }

    fn max_occurred_at_unix(
        &self,
        _chain_id: &ChainId,
        _vault_id: &VaultId,
        _asset_id: &AssetId,
    ) -> Result<Option<i64>, ProjectionError> {
        Ok(None)
    }

    fn insert(&mut self, event: &TransferEventRef) -> Result<(), TransferEventLogInsertError> {
        if self.fail_insert {
            return Err(TransferEventLogInsertError::InsertFailed);
        }
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(event.clone());
        Ok(())
    }
}

/// In-memory `VaultProjectionInvalidationEmitter` fixture — records every
/// `emit` call (TV-PW-1 / TV-PW-2 / TV-PW-4 envelope observation).
#[derive(Debug, Default)]
pub struct StubEmitter {
    envelopes: Mutex<Vec<VaultProjectionInvalidationEnvelope>>,
}

impl StubEmitter {
    /// Snapshot of all envelopes emitted so far.
    #[must_use]
    pub fn envelopes(&self) -> Vec<VaultProjectionInvalidationEnvelope> {
        self.envelopes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Number of envelopes recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.envelopes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }

    /// Is the fixture empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl VaultProjectionInvalidationEmitter for StubEmitter {
    fn emit(&self, envelope: &VaultProjectionInvalidationEnvelope) {
        self.envelopes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(envelope.clone());
    }
}

/// In-memory `VaultAssetResolver` fixture — single-vault mapping
/// `(chain_id, vault_id) → asset_id`.
#[derive(Debug, Default)]
pub struct StubVaultAssetResolver {
    mapping: Mutex<Vec<(ChainId, VaultId, AssetId)>>,
}

impl StubVaultAssetResolver {
    /// Construct a fixture pre-seeded with `(chain_id, vault_id, asset_id)` triples.
    #[must_use]
    pub fn with_mapping(mapping: Vec<(ChainId, VaultId, AssetId)>) -> Self {
        Self {
            mapping: Mutex::new(mapping),
        }
    }

    /// Add a `(chain_id, vault_id, asset_id)` triple to the fixture.
    pub fn add(&self, chain_id: ChainId, vault_id: VaultId, asset_id: AssetId) {
        self.mapping
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((chain_id, vault_id, asset_id));
    }
}

impl VaultAssetResolver for StubVaultAssetResolver {
    fn resolve_asset_for(
        &self,
        chain_id: &ChainId,
        vault_id: &VaultId,
    ) -> Result<AssetId, VaultAssetResolverError> {
        let m = self.mapping.lock().unwrap_or_else(|p| p.into_inner());
        m.iter()
            .find(|(c, v, _)| c == chain_id && v == vault_id)
            .map(|(_, _, a)| *a)
            .ok_or(VaultAssetResolverError::UnknownVault {
                vault_id: *vault_id,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log_producer::TransferEventRef;

    fn sample_event(seed: u8) -> TransferEventRef {
        TransferEventRef {
            event_id: [seed; 32],
            chain_id: ChainId::from_bytes([seed; 32]),
            from_vault_id: VaultId::from_bytes([seed.wrapping_add(1); 32]),
            to_vault_id: VaultId::from_bytes([seed.wrapping_add(2); 32]),
            asset_id: AssetId::from_bytes([seed.wrapping_add(3); 32]),
            amount: Dqa::new(100, 0).unwrap(),
            occurred_at_unix: 1_700_000_000 + i64::from(seed),
        }
    }

    /// TV-TS-1: StubTransferEventLog records inserts.
    #[test]
    fn tv_ts1_transfer_log_records_inserts() {
        let mut log = StubTransferEventLog::default();
        log.insert(&sample_event(1)).unwrap();
        log.insert(&sample_event(2)).unwrap();
        assert_eq!(log.len(), 2);
        assert!(!log.is_empty());
    }

    /// TV-TS-2: failing_insert fixture returns Err on insert.
    #[test]
    fn tv_ts2_failing_insert() {
        let mut log = StubTransferEventLog::failing_insert();
        assert!(log.insert(&sample_event(1)).is_err());
    }

    /// TV-TS-3: StubEmitter records emits.
    #[test]
    fn tv_ts3_emitter_records_envelopes() {
        let emitter = StubEmitter::default();
        emitter.emit(&VaultProjectionInvalidationEnvelope::v1_legacy(
            ChainId::from_bytes([1u8; 32]),
            VaultId::from_bytes([2u8; 32]),
            AssetId::from_bytes([3u8; 32]),
            crate::vault_balance_projection::ProjectionSource::FreshLogScan,
        ));
        assert_eq!(emitter.len(), 1);
    }

    /// TV-TS-4: StubVaultAssetResolver resolves known triples.
    #[test]
    fn tv_ts4_resolver_known_triple() {
        let chain = ChainId::from_bytes([1u8; 32]);
        let vault = VaultId::from_bytes([2u8; 32]);
        let asset = AssetId::from_bytes([3u8; 32]);
        let r = StubVaultAssetResolver::with_mapping(vec![(chain, vault, asset)]);
        assert_eq!(r.resolve_asset_for(&chain, &vault).unwrap(), asset);
    }

    /// TV-TS-5: StubVaultAssetResolver rejects unknown triples.
    #[test]
    fn tv_ts5_resolver_unknown_triple() {
        let r = StubVaultAssetResolver::default();
        let chain = ChainId::from_bytes([1u8; 32]);
        let vault = VaultId::from_bytes([2u8; 32]);
        let err = r.resolve_asset_for(&chain, &vault).unwrap_err();
        assert!(matches!(err, VaultAssetResolverError::UnknownVault { .. }));
    }
}
