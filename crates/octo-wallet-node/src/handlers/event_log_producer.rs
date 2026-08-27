//! Mission B (RFC-0960 v3.7 §2.5) `PaymentEventProducer` impl.
//!
//! Layer C specialization of the Layer B `EventLogProducer` trait.
//! Reads `PaymentCaveat::asset_id` to derive the asset-generic
//! `TransferEventRef` (RFC-0965 v2.1 §AssetBinding → RFC-0960 v3.7
//! §3.6).

#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]

use std::sync::{Arc, Mutex};

use octo_cap_macaroon::{AssetRegistry, ChainId, NonceRegistry, PaymentCaveat, VaultId};
use octo_vault::{
    cache_channel_name, process_drain_lock, EventLogProducer, ProducerError, TransferEventLog,
    TransferEventRef, VaultAssetResolver, VaultProjectionInvalidationEmitter,
    VaultProjectionInvalidationEnvelope,
};

/// `PaymentEventProducer` — Layer C impl (Mission B §2.5).
#[derive(Debug)]
pub struct PaymentEventProducer {
    drain_lock: Arc<Mutex<()>>,
}

impl Default for PaymentEventProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl PaymentEventProducer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            drain_lock: Arc::clone(process_drain_lock()),
        }
    }
}

/// Producer input wrapper — carries the bound caveat + counter-party
/// vault.
#[derive(Clone, Debug)]
pub struct PaymentProducerInput {
    pub payment_id: [u8; 32],
    pub chain_id: ChainId,
    pub from_vault_id: VaultId,
    pub to_vault_id: VaultId,
    pub caveat: PaymentCaveat,
    pub amount: octo_cap_macaroon::Dqa,
    pub occurred_at_unix: i64,
}

impl EventLogProducer for PaymentEventProducer {
    type Input = PaymentProducerInput;

    fn drain_lock(&self) -> &Arc<Mutex<()>> {
        &self.drain_lock
    }

    fn validate_pre_insert(
        &self,
        input: &PaymentProducerInput,
        _registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError> {
        // Caveat asset_id must be a registered asset. The substrate
        // registry handles full tri-invariant enforcement upstream;
        // the producer narrows to "asset_id is non-zero".
        if input.caveat.asset_id.as_bytes() == &[0u8; 32] {
            return Err(ProducerError::TriInvariantViolation(
                "payment caveat asset_id is zero".into(),
            ));
        }
        Ok(())
    }

    fn to_transfer_event(
        &self,
        input: PaymentProducerInput,
        _registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
        _nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError> {
        Ok(TransferEventRef {
            event_id: input.payment_id,
            chain_id: input.chain_id,
            from_vault_id: input.from_vault_id,
            to_vault_id: input.to_vault_id,
            asset_id: input.caveat.asset_id,
            amount: input.amount,
            occurred_at_unix: input.occurred_at_unix,
        })
    }
}

/// Wrap validation + log insert + bus emit (Mission B TV-VP11).
pub fn produce_payment(
    input: PaymentProducerInput,
    registry: &dyn AssetRegistry,
    asset_resolver: &dyn VaultAssetResolver,
    nonce_registry: &mut dyn NonceRegistry,
    log: &mut dyn TransferEventLog,
    bus: &dyn VaultProjectionInvalidationEmitter,
) -> Result<TransferEventRef, ProducerError> {
    let producer = PaymentEventProducer::new();
    let _guard = producer
        .drain_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    producer.validate_pre_insert(&input, registry, asset_resolver)?;
    let ev = producer.to_transfer_event(input, registry, asset_resolver, nonce_registry)?;
    log.insert(&ev).map_err(ProducerError::from)?;
    bus.emit(&VaultProjectionInvalidationEnvelope {
        chain_id: ev.chain_id,
        vault_id: ev.from_vault_id,
        asset_id: ev.asset_id,
        source_kind: octo_vault::ProjectionSource::FreshLogScan,
    });
    let _ = cache_channel_name(&ev.from_vault_id);
    Ok(ev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_lock_singleton() {
        let p1 = PaymentEventProducer::new();
        let p2 = PaymentEventProducer::new();
        assert!(Arc::ptr_eq(p1.drain_lock(), p2.drain_lock()));
    }

    #[test]
    fn cache_channel_name_format() {
        let v = VaultId::from_bytes([0xab; 32]);
        let name = cache_channel_name(&v);
        assert!(name.starts_with("cache:projection:"));
        assert_eq!(name.len(), "cache:projection:".len() + 64);
    }
}
