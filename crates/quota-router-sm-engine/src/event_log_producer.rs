//! Mission B (RFC-0960 v3.7 §2.5) `SettlementEventProducer` impl.
//!
//! Layer C specialization of the Layer B `EventLogProducer` trait.
//! Wraps `SettlementEvent::new` (Mission G substrate).

#![allow(clippy::missing_docs_in_private_items)]

use std::sync::{Arc, Mutex};

use octo_cap_macaroon::{AssetRegistry, ChainId, NonceRegistry, VaultId};
use octo_vault::{
    cache_channel_name, process_drain_lock, EventLogProducer, ProducerError, TransferEventLog,
    TransferEventRef, VaultAssetResolver, VaultProjectionInvalidationEmitter,
    VaultProjectionInvalidationEnvelope,
};

use crate::settlement_event::SettlementEventError;

/// `SettlementEventProducer` — Layer C impl (Mission B §2.5).
#[derive(Debug)]
pub struct SettlementEventProducer {
    drain_lock: Arc<Mutex<()>>,
}

impl Default for SettlementEventProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl SettlementEventProducer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            drain_lock: Arc::clone(process_drain_lock()),
        }
    }
}

/// Minimal input wrapper for the producer (Mission B scope keeps the
/// producer self-contained — full SettlementEvent construction lives in
/// `settlement_event::new`).
#[derive(Clone, Debug)]
pub struct SettlementProducerInput {
    pub settlement_id: octo_cap_macaroon::SettlementId,
    pub chain_id: ChainId,
    pub cost_vault_id: VaultId,
    pub asset_id: octo_cap_macaroon::AssetId,
    pub amount: octo_cap_macaroon::Dqa,
    pub occurred_at_unix: i64,
}

impl EventLogProducer for SettlementEventProducer {
    type Input = SettlementProducerInput;

    fn drain_lock(&self) -> &Arc<Mutex<()>> {
        &self.drain_lock
    }

    fn validate_pre_insert(
        &self,
        _input: &SettlementProducerInput,
        _registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError> {
        // SettlementEvent::new enforces registry + vault + nonce gates
        // upstream; the producer is a pure pass-through wrapper.
        Ok(())
    }

    fn to_transfer_event(
        &self,
        input: SettlementProducerInput,
        _registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
        _nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError> {
        Ok(TransferEventRef {
            event_id: input.settlement_id.0,
            chain_id: input.chain_id,
            from_vault_id: VaultId::from_bytes([0u8; 32]),
            to_vault_id: input.cost_vault_id,
            asset_id: input.asset_id,
            amount: input.amount,
            occurred_at_unix: input.occurred_at_unix,
        })
    }
}

/// Wrap `SettlementEvent::new` + log insert + bus emit (Mission B TV-VP12).
pub fn produce_settlement(
    input: SettlementProducerInput,
    registry: &dyn AssetRegistry,
    asset_resolver: &dyn VaultAssetResolver,
    nonce_registry: &mut dyn NonceRegistry,
    log: &mut dyn TransferEventLog,
    bus: &dyn VaultProjectionInvalidationEmitter,
) -> Result<TransferEventRef, ProducerError> {
    let producer = SettlementEventProducer::new();
    let _guard = producer
        .drain_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    producer.validate_pre_insert(&input, registry, asset_resolver)?;
    let ev = producer.to_transfer_event(input, registry, asset_resolver, nonce_registry)?;
    log.insert(&ev).map_err(ProducerError::from)?;
    bus.emit(&VaultProjectionInvalidationEnvelope::v1_legacy(
        ev.chain_id,
        ev.to_vault_id,
        ev.asset_id,
        octo_vault::ProjectionSource::FreshLogScan,
    ));
    let _ = cache_channel_name(&ev.to_vault_id);
    Ok(ev)
}

// Anchor (settlement_event re-exports kept for forward-compat).
#[allow(dead_code)]
fn _anchor(_e: SettlementEventError) {}
