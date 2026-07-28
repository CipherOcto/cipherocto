//! Cross-layer query helper (RFC-0968 §3, mission 0968 Phase 3).
//!
//! Wraps `ReputationStore::cross_layer_query` with two extras:
//!
//! 1. **Deduplication** — repeated `ReputationLayer` entries in the input
//!    slice collapse to a single query.
//! 2. **Maximum fan-out** — at most `MAX_CROSS_LAYER_FANOUT` distinct layers
//!    may be requested in one query (defaults to `ReputationLayer::COUNT`).
//! 3. **Index** — the result is keyed by `ReputationLayer` discriminant for
//!    O(1) lookup; absent layers get `None`.

use std::collections::BTreeMap;

use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{RecorderDid, ReputationAggregate, ReputationLayer, SignalKind};

/// Maximum distinct layers per query. Sized to the canonical layer set
/// (Consensus / Market / Coordinator / Slash / Governance). Larger requests
/// are rejected — operators paginate via multiple queries.
pub const MAX_CROSS_LAYER_FANOUT: usize = 5;

/// Per-layer query result — `None` indicates the layer has no aggregate yet.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossLayerResult {
    pub recorder_did: RecorderDid,
    pub signal_kind: SignalKind,
    pub layers_requested: Vec<ReputationLayer>,
    /// Layer discriminant → aggregate (or absent).
    pub by_layer: BTreeMap<u8, Option<ReputationAggregate>>,
}

/// Deduplicate a layer slice preserving first-seen order.
pub fn dedup_layers(layers: &[ReputationLayer]) -> Vec<ReputationLayer> {
    let mut seen: [bool; 256] = [false; 256];
    let mut out = Vec::with_capacity(layers.len());
    for l in layers {
        let d = l.discriminant();
        if !seen[d as usize] {
            seen[d as usize] = true;
            out.push(*l);
        }
    }
    out
}

/// Run a dedup'd cross-layer query and shape the result.
pub async fn cross_layer_query<S: ReputationStore + ?Sized>(
    store: &S,
    did: &RecorderDid,
    kind: SignalKind,
    layers: &[ReputationLayer],
) -> StoreResult<CrossLayerResult> {
    let dedupd = dedup_layers(layers);
    if dedupd.is_empty() {
        return Err(ReputationError::CrossLayerEmpty);
    }
    if dedupd.len() > MAX_CROSS_LAYER_FANOUT {
        return Err(ReputationError::AnchorTupleFanoutExceeded(
            dedupd.len() as u64
        ));
    }
    let aggs = store.cross_layer_query(did, kind, &dedupd).await?;
    let mut by_layer: BTreeMap<u8, Option<ReputationAggregate>> = BTreeMap::new();
    for l in &dedupd {
        let d = l.discriminant();
        // The store returns aggregates in the order they exist; we map by
        // matching the (kind, layer) pair. In the in-memory impl the order
        // matches the dedup'd input, so we index by position. To be robust
        // against store reordering, look up by composite key.
        let m: Option<ReputationAggregate> = aggs
            .iter()
            .find(|a| a.signal_kind == kind && a.layer == *l)
            .cloned();
        by_layer.insert(d, m);
    }
    Ok(CrossLayerResult {
        recorder_did: *did,
        signal_kind: kind,
        layers_requested: dedupd,
        by_layer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryReputationStore;
    use octo_determin::Dfp;

    #[test]
    fn dedup_layers_preserves_first_seen_order() {
        let layers = vec![
            ReputationLayer::Market,
            ReputationLayer::Market,
            ReputationLayer::Coordinator,
            ReputationLayer::Market,
        ];
        let d = dedup_layers(&layers);
        assert_eq!(
            d,
            vec![ReputationLayer::Market, ReputationLayer::Coordinator]
        );
    }

    #[test]
    fn dedup_layers_idempotent() {
        let layers = vec![
            ReputationLayer::Consensus,
            ReputationLayer::Market,
            ReputationLayer::Coordinator,
        ];
        let d = dedup_layers(&layers);
        assert_eq!(dedup_layers(&d), d);
    }

    #[tokio::test]
    async fn cross_layer_query_empty_input_rejected() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        let err = cross_layer_query(&store, &did, SignalKind::Outcome, &[])
            .await
            .unwrap_err();
        assert_eq!(err, ReputationError::CrossLayerEmpty);
    }

    #[tokio::test]
    async fn cross_layer_query_returns_per_layer_aggregates() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        for layer in [ReputationLayer::Market, ReputationLayer::Coordinator] {
            let ev = crate::types::SignalEvent {
                event_id: crate::types::EventId::from_u64(0),
                recorder_did: did,
                controller_id: crate::types::ControllerId::from_array([0u8; 32]),
                signal_kind: SignalKind::Outcome,
                layer,
                score_delta: Dfp::from_f64(1.0),
                recorded_at_unix: 1_000,
                rotation_provenance: None,
                audit_ref: None,
                anchor_tx_hash: None,
            };
            store.record_signal(ev).await.unwrap();
        }
        let layers = vec![
            ReputationLayer::Market,
            ReputationLayer::Coordinator,
            ReputationLayer::Governance, // absent
        ];
        let r = cross_layer_query(&store, &did, SignalKind::Outcome, &layers)
            .await
            .unwrap();
        assert!(r
            .by_layer
            .get(&ReputationLayer::Market.discriminant())
            .unwrap()
            .is_some());
        assert!(r
            .by_layer
            .get(&ReputationLayer::Coordinator.discriminant())
            .unwrap()
            .is_some());
        assert!(r
            .by_layer
            .get(&ReputationLayer::Governance.discriminant())
            .unwrap()
            .is_none());
    }
}
