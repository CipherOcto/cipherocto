//! Legacy reputation store implementations.
//!
//! These stubs mirror the public API surface of the pre-RFC-0968
//! `SlashReputationStore` and `DcRootedSlashReputationStore` in
//! `quota-router-core::marketplace`. The compat adapter invokes
//! `shadow_record` on every canonical write; the parity binary (Session 4)
//! reads via `success_rate` and compares against the canonical Dfp aggregate.
//!
//! The implementations use plain f64 EWMA in this crate. They are NOT used
//! for canonical reads — the canonical path is the persisted `ReputationStore`.
//! They exist solely for parity reconciliation.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::compat::LegacyShadowError;
use crate::types::{RecorderDid, ReputationLayer, SignalKind};

/// Legacy trait — every legacy backend implements this so the compat
/// adapter can write through a uniform interface.
pub trait LegacyReputationStore: Send + Sync {
    fn shadow_record(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
        score_delta: f64,
        ts: u64,
    ) -> Result<(), LegacyShadowError>;

    fn success_rate(&self, did: &RecorderDid) -> f64;

    fn sample_count(&self, did: &RecorderDid) -> u64;
}

#[derive(Default)]
struct LegacyRow {
    samples: u64,
    success_sum: f64,
    ewma: f64,
}

impl LegacyRow {
    fn record(&mut self, delta: f64) {
        let n = self.samples as f64;
        let alpha = 1.0 / (n + 1.0);
        self.ewma = self.ewma * (1.0 - alpha) + delta * alpha;
        if delta > 0.0 {
            self.success_sum += delta;
        }
        self.samples += 1;
    }
}

#[derive(Default)]
pub struct SlashReputationStore {
    rows: RwLock<HashMap<[u8; 52], LegacyRow>>,
}

impl SlashReputationStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn did_key(d: &RecorderDid) -> [u8; 52] {
        *d.as_bytes()
    }
}

impl LegacyReputationStore for SlashReputationStore {
    fn shadow_record(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
        score_delta: f64,
        _ts: u64,
    ) -> Result<(), LegacyShadowError> {
        if !matches!(kind, SignalKind::Outcome) {
            return Err(LegacyShadowError::UnsupportedKind);
        }
        if !matches!(layer, ReputationLayer::Market) {
            return Err(LegacyShadowError::UnsupportedLayer);
        }
        let k = Self::did_key(did);
        let mut g = self.rows.write().unwrap();
        g.entry(k).or_default().record(score_delta);
        Ok(())
    }

    fn success_rate(&self, did: &RecorderDid) -> f64 {
        let g = self.rows.read().unwrap();
        g.get(&Self::did_key(did)).map(|r| r.ewma).unwrap_or(0.0)
    }

    fn sample_count(&self, did: &RecorderDid) -> u64 {
        let g = self.rows.read().unwrap();
        g.get(&Self::did_key(did)).map(|r| r.samples).unwrap_or(0)
    }
}

/// DC-rooted variant. Identical semantics — separate type for mission 0968-b
/// dual-read surface.
#[derive(Default)]
pub struct DcRootedSlashReputationStore {
    rows: RwLock<HashMap<[u8; 52], LegacyRow>>,
}

impl DcRootedSlashReputationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LegacyReputationStore for DcRootedSlashReputationStore {
    fn shadow_record(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
        score_delta: f64,
        _ts: u64,
    ) -> Result<(), LegacyShadowError> {
        if !matches!(kind, SignalKind::Outcome) {
            return Err(LegacyShadowError::UnsupportedKind);
        }
        if !matches!(layer, ReputationLayer::Coordinator) {
            return Err(LegacyShadowError::UnsupportedLayer);
        }
        let k = *did.as_bytes();
        let mut g = self.rows.write().unwrap();
        g.entry(k).or_default().record(score_delta);
        Ok(())
    }

    fn success_rate(&self, did: &RecorderDid) -> f64 {
        let g = self.rows.read().unwrap();
        g.get(did.as_bytes()).map(|r| r.ewma).unwrap_or(0.0)
    }

    fn sample_count(&self, did: &RecorderDid) -> u64 {
        let g = self.rows.read().unwrap();
        g.get(did.as_bytes()).map(|r| r.samples).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_ewma_is_deterministic() {
        let s = SlashReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        for i in 0..10u64 {
            s.shadow_record(
                &did,
                SignalKind::Outcome,
                ReputationLayer::Market,
                1.0,
                1000 + i,
            )
            .unwrap();
        }
        // EWMA of 10 × 1.0 should approach 1.0; first sample = 1.0/1 = 1.0,
        // subsequent samples reduce alpha → still ~1.0.
        assert!((s.success_rate(&did) - 1.0).abs() < 1e-9);
        assert_eq!(s.sample_count(&did), 10);
    }

    #[test]
    fn slash_rejects_unsupported_layer() {
        let s = SlashReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        let err = s
            .shadow_record(
                &did,
                SignalKind::Outcome,
                ReputationLayer::Coordinator,
                1.0,
                1000,
            )
            .unwrap_err();
        assert_eq!(err, LegacyShadowError::UnsupportedLayer);
    }

    #[test]
    fn dc_rooted_rejects_unsupported_kind() {
        let s = DcRootedSlashReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        let err = s
            .shadow_record(
                &did,
                SignalKind::Latency,
                ReputationLayer::Coordinator,
                1.0,
                1000,
            )
            .unwrap_err();
        assert_eq!(err, LegacyShadowError::UnsupportedKind);
    }
}
