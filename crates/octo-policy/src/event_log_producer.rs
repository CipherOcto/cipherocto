//! Mission B (RFC-0960 v3.7 §2.5) `BurnEventProducer` impl.
//!
//! Layer C specialization of the Layer B `EventLogProducer` trait.
//! Wraps `BurnEventRef::consume` (Mission F substrate).

#![allow(clippy::missing_docs_in_private_items)]

use std::sync::{Arc, Mutex};

use octo_cap_macaroon::{AssetRegistry, ChainId, Dqa, Nonce, NonceRegistry, VaultId};
use octo_vault::{
    cache_channel_name, process_drain_lock, EventLogProducer, ProducerError, TransferEventLog,
    TransferEventRef, VaultAssetResolver, VaultProjectionInvalidationEmitter,
    VaultProjectionInvalidationEnvelope,
};

use crate::burn_event::BurnEventRef;

/// `BurnEventProducer` — Layer C impl (Mission B §2.5).
#[derive(Debug)]
pub struct BurnEventProducer {
    drain_lock: Arc<Mutex<()>>,
}

impl Default for BurnEventProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl BurnEventProducer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            drain_lock: Arc::clone(process_drain_lock()),
        }
    }
}

impl EventLogProducer for BurnEventProducer {
    type Input = BurnEventRef;

    fn drain_lock(&self) -> &Arc<Mutex<()>> {
        &self.drain_lock
    }

    fn validate_pre_insert(
        &self,
        _input: &BurnEventRef,
        _registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
    ) -> Result<(), ProducerError> {
        // Tri-invariant + signature checks are performed by
        // BurnEventRef::new() / validate() upstream; the producer is a
        // pure pass-through wrapper.
        Ok(())
    }

    fn to_transfer_event(
        &self,
        input: BurnEventRef,
        _registry: &dyn AssetRegistry,
        _asset_resolver: &dyn VaultAssetResolver,
        _nonce_registry: &dyn NonceRegistry,
    ) -> Result<TransferEventRef, ProducerError> {
        Ok(TransferEventRef {
            event_id: *input.nonce.as_bytes(),
            chain_id: input.chain_id,
            from_vault_id: VaultId::from_bytes([0u8; 32]), // ZERO for burn
            to_vault_id: input.vault_id,
            asset_id: input.asset_id,
            amount: input.amount,
            occurred_at_unix: input.ledger_height.try_into().unwrap_or(i64::MAX),
        })
    }
}

/// Wrap `BurnEventRef::consume` + log insert + bus emit
/// (Mission B TV-VP13).
///
/// The nonce + audit sinks are upstream of `TransferEventLog::insert`
/// (Mission F 3-sink atomicity). The producer's `produce` default body
/// performs the log.insert + bus.emit steps AFTER the nonce + audit
/// sinks have been observed.
pub fn produce_burn(
    burn: BurnEventRef,
    registry: &dyn AssetRegistry,
    asset_resolver: &dyn VaultAssetResolver,
    nonce_registry: &mut dyn NonceRegistry,
    audit_sink: &mut dyn crate::burn_event::AuditSink,
    consume_log: &mut dyn crate::burn_event::TransferEventLog,
    log: &mut dyn TransferEventLog,
    bus: &dyn VaultProjectionInvalidationEmitter,
) -> Result<TransferEventRef, ProducerError> {
    let producer = BurnEventProducer::new();
    let _guard = producer
        .drain_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    producer.validate_pre_insert(&burn, registry, asset_resolver)?;
    // CRITICAL FIX (review C1): 3-sink atomicity — Sink 1 (nonce) + Sink 2
    // (audit) + Sink 3-audit-mirror (local log) run inside `consume` with
    // full rollback. `log` (Layer B `TransferEventLog`) is the canonical
    // projection source carrying the TransferEventRef envelope. Parallel
    // TransferEventLog traits (octo-policy::burn_event::TransferEventLog
    // vs octo-vault::TransferEventLog) are tracked for elimination under
    // L4 CRITICAL #2.
    crate::burn_event::consume(&burn, nonce_registry, audit_sink, consume_log)
        .map_err(|e| ProducerError::TriInvariantViolation(format!("burn_consume: {e:?}")))?;
    let ev = producer.to_transfer_event(burn, registry, asset_resolver, nonce_registry)?;
    log.insert(&ev).map_err(ProducerError::from)?;
    bus.emit(&VaultProjectionInvalidationEnvelope {
        chain_id: ev.chain_id,
        vault_id: ev.to_vault_id,
        asset_id: ev.asset_id,
        source_kind: octo_vault::ProjectionSource::FreshLogScan,
    });
    let _ = cache_channel_name(&ev.to_vault_id);
    Ok(ev)
}

// Re-export so consumers don't need separate imports.
pub use crate::burn_event::TransferEventLog as BurnTransferEventLog;

// Helper to make `produce_burn` compile without unused-import errors when
// downstream callers do not use Dqa / Nonce / ChainId directly.
#[allow(dead_code)]
fn _anchor(_d: Dqa, _n: Nonce, _c: ChainId) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// TV-VP13-prep: BurnEventProducer drain_lock singleton + acquire/release.
    /// The 3-sink atomicity contract is exercised end-to-end by Mission F's
    /// 15 TV-BE tests in `burn_event.rs` (which cover `BurnEventRef::consume`
    /// directly); this test guards the producer wrapper's lock plumbing.
    #[test]
    fn tv_vp13_producer_drain_lock_singleton() {
        let producer = BurnEventProducer::new();
        assert!(Arc::ptr_eq(
            producer.drain_lock(),
            BurnEventProducer::new().drain_lock(),
        ));
        let g = producer
            .drain_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(g);
    }
}
