//! `DcRootedSlashReputationStoreCompat` — mission 0855p-c per-DC
//! cross-domain reputation adapter (RFC-0968-A1 amendments 28, 29).
//!
//! Mirrors the structure of `SlashReputationStoreCompat` (mission
//! 0855p-b) but namespace is `ReputationLayer::Coordinator` and the
//! gossip topic is `/dot/reputation/dc/{dc_did}` per amendment 29
//! (NOT the legacy `dc_pubkey` keying).
//!
//! ## Authority model (amendment 28)
//!
//! Recorder signature is authoritative. Coordinator / attestor
//! signatures are non-authoritative transport metadata. The store
//! does not consult gossip envelopes for authority; it only reads
//! the persisted attestations under `ReputationLayer::Coordinator`.
//!
//! ## Cross-domain slashing semantics
//!
//! A DC's cross-domain slash count is the number of slashes the DC
//! has accumulated across ALL domains it manages. The gossip
//! substrate records each cross-domain slash as a
//! `SignalKind::Slash` event with `ReputationLayer::Coordinator`
//! on the topic `/dot/reputation/dc/{dc_did}`. This adapter mirrors
//! those slashes into an in-memory counter keyed by the DC's
//! recorder DID; the canonical source-of-truth is the persisted
//! `ReputationStore` (consumed by mission 0968-b marketplace
//! integration via `query_attestations(did, since, layer)`).

use std::collections::HashMap;

use parking_lot::RwLock;

use octo_reputation::store::{ReputationStore, StoreResult};
use octo_reputation::types::{RecorderDid, ReputationLayer, SignalKind};

/// Hard threshold: candidates with >= this many cross-domain slashes
/// are excluded from the election. Matches `HARD_THRESHOLD` in
/// `slash_store.rs` (mission 0855p-b) for byte-identical threshold
/// semantics across the cross-mission + cross-domain reputation
/// adapters.
pub const HARD_THRESHOLD: u32 = 5;

/// `DcRootedSlashReputationStoreCompat` — DID-keyed cross-domain
/// slash store for DomainCoordinators.
///
/// Internally holds a `HashMap<RecorderDid, u32>` populated by the
/// gossip substrate from `ReputationStore::query_attestations`
/// filtered by `SignalKind::Slash` AND
/// `ReputationLayer::Coordinator`. The S3+ work wires this to a real
/// persisted read; for now the in-memory map is the canonical
/// state, populated explicitly via `record_dc_slash` from the gossip
/// substrate.
pub struct DcRootedSlashReputationStoreCompat {
    /// Per-DC cross-domain slash count. Capped at `u32::MAX` for
    /// determinism; in practice counts stay far below this.
    counts: RwLock<HashMap<RecorderDid, u32>>,
}

impl Default for DcRootedSlashReputationStoreCompat {
    fn default() -> Self {
        Self::new()
    }
}

impl DcRootedSlashReputationStoreCompat {
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
        }
    }

    /// Increment the cross-domain slash count for a DC DID.
    /// Idempotent on duplicate `(did, slash_event_hash)` pairs (the
    /// gossip substrate's store-level dedup handles this; this
    /// method is called once per accepted cross-domain slash
    /// attestation).
    pub fn record_dc_slash(&self, did: &RecorderDid) {
        let mut g = self.counts.write();
        *g.entry(*did).or_insert(0) += 1;
    }

    /// Return the cross-domain slash count for a DC DID (0 if
    /// unknown). Fed by `ReputationStore::query_attestations`
    /// filtered by `SignalKind::Slash` AND
    /// `ReputationLayer::Coordinator` (mission 0968-b wire-up).
    pub fn cross_domain_slash_count_for(&self, did: &RecorderDid) -> u32 {
        let g = self.counts.read();
        g.get(did).copied().unwrap_or(0)
    }

    /// True iff the DC is excluded (count >= HARD_THRESHOLD).
    pub fn is_excluded(&self, did: &RecorderDid) -> bool {
        self.cross_domain_slash_count_for(did) >= HARD_THRESHOLD
    }

    /// Legacy priority formula: `stake / (1 + cross_domain_slash_count)`.
    /// Returns `None` for excluded DCs. **DEPRECATED**: the
    /// canonical priority is the RFC-0968 §10 `election_priority`
    /// adapter. This method is preserved for back-compat comparisons
    /// and the AC L33 differential test.
    pub fn priority_legacy(&self, did: &RecorderDid, stake: u64) -> Option<u128> {
        if self.is_excluded(did) {
            return None;
        }
        let n = self.cross_domain_slash_count_for(did) as u128;
        let s = stake as u128;
        // Saturating division — stake=0 with no slashes returns Some(0)
        // (matching the legacy behavior in
        // `compat::legacy::DcRootedSlashReputationStore`).
        Some(s / (1 + n))
    }

    /// Canonical RFC-0968 §10 `election_priority` formula:
    /// `(stake_saturated × effective) / MAX_ELECTION_STAKE`,
    /// applied with the `MIN_ELECTION_SCORE = 0.05` floor and the
    /// `MIN_CONFIDENCE_SAMPLES = 100` sample-confidence multiplier.
    ///
    /// `stake_saturated = min(stake, MAX_ELECTION_STAKE)`.
    /// `effective = score_clamped × min(1.0, samples /
    /// MIN_CONFIDENCE_SAMPLES)`. Returns `None` when
    /// `effective < MIN_ELECTION_SCORE` or when
    /// `cross_domain_slash_count >= HARD_THRESHOLD`.
    ///
    /// The formula is monotonic in `stake` when `effective > 0`,
    /// which is the property the AC L33 differential test relies on.
    pub fn election_priority(
        &self,
        did: &RecorderDid,
        stake: u64,
        score_clamped: f64,
        samples: u64,
    ) -> Option<u128> {
        if self.is_excluded(did) {
            return None;
        }
        const MAX_ELECTION_STAKE: u64 = 1_000_000;
        const MIN_ELECTION_SCORE: f64 = 0.05;
        const MIN_CONFIDENCE_SAMPLES: u64 = 100;
        if !score_clamped.is_finite() {
            return None;
        }
        let confidence = (samples as f64 / MIN_CONFIDENCE_SAMPLES as f64).min(1.0);
        let effective = score_clamped.clamp(0.0, 1.0) * confidence;
        if effective < MIN_ELECTION_SCORE {
            return None;
        }
        let saturated = stake.min(MAX_ELECTION_STAKE) as u128;
        // Canonical: priority = saturated * effective / MAX_ELECTION_STAKE.
        // u128 multiplication with f64→u128 conversion at fixed-point
        // precision (1e9). This is deterministic across replicas because
        // f64 bit-patterns are bit-stable.
        let eff_q = (effective * 1_000_000_000.0) as u128; // 9 fractional digits
        let max_q = (MAX_ELECTION_STAKE as u128) * 1_000_000_000u128;
        Some(saturated.saturating_mul(eff_q) / max_q)
    }

    /// Distinct DC count (for ops diagnostics).
    pub fn dc_count(&self) -> usize {
        self.counts.read().len()
    }

    /// Total cross-domain slash event count across all DCs.
    pub fn total_cross_domain_slashes(&self) -> u64 {
        self.counts.read().values().map(|c| *c as u64).sum()
    }

    /// Refresh the cross-domain slash count for a single DC from the
    /// persisted `ReputationStore`. Path B (refresh-on-demand) of the
    /// 0855p-c wire-up: this is the bridge between gossip-side
    /// `record_signal` (which persists events to the store) and the
    /// election-side `cross_domain_slash_count_for` (which is sync).
    ///
    /// Implementation: replay the recorder's events across all time
    /// via `replay_for_audit`, filter by `signal_kind == Slash` AND
    /// `layer == Coordinator`, and set the in-memory count. Returns
    /// the refreshed count for the caller (e.g., gossip ingress) to
    /// log / observe.
    ///
    /// `S` is `?Sized` so callers passing `Arc<dyn ReputationStore>`
    /// work cleanly. Idempotent — re-running refresh without new
    /// persisted events produces the same count.
    pub async fn refresh_cross_domain_for<S>(
        &self,
        did: &RecorderDid,
        store: &S,
    ) -> StoreResult<u32>
    where
        S: ReputationStore + ?Sized,
    {
        let events = store.replay_for_audit(did, 0, u64::MAX).await?;
        let count = events
            .iter()
            .filter(|e| {
                e.signal_kind == SignalKind::Slash && e.layer == ReputationLayer::Coordinator
            })
            .count() as u32;
        self.counts.write().insert(*did, count);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dc_did(byte: u8) -> RecorderDid {
        RecorderDid::from_array([byte; 52])
    }

    #[test]
    fn record_dc_slash_increments_per_dc() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(1);
        s.record_dc_slash(&d);
        s.record_dc_slash(&d);
        s.record_dc_slash(&d);
        assert_eq!(s.cross_domain_slash_count_for(&d), 3);
        assert_eq!(s.dc_count(), 1);
        assert_eq!(s.total_cross_domain_slashes(), 3);
    }

    #[test]
    fn hard_threshold_constant_is_five() {
        // Per mission 0855p-c: cross_domain_slash_count >= 5 → excluded.
        // Shared constant with `slash_store.rs` (mission 0855p-b).
        const { assert!(HARD_THRESHOLD == 5) };
    }

    #[test]
    fn is_excluded_at_threshold() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(2);
        for _ in 0..5 {
            s.record_dc_slash(&d);
        }
        assert!(s.is_excluded(&d));
    }

    #[test]
    fn priority_legacy_returns_none_when_excluded() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(3);
        for _ in 0..5 {
            s.record_dc_slash(&d);
        }
        assert_eq!(s.priority_legacy(&d, 1000), None);
    }

    #[test]
    fn priority_legacy_soft_penalty() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(4);
        s.record_dc_slash(&d);
        s.record_dc_slash(&d);
        // 2 cross-domain slashes → 1000 / 3 = 333
        assert_eq!(s.priority_legacy(&d, 1000), Some(333));
    }

    #[test]
    fn priority_legacy_zero_stakes_yields_zero() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(5);
        assert_eq!(s.priority_legacy(&d, 0), Some(0));
    }

    #[test]
    fn election_priority_below_floor_returns_none() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(6);
        // score 0.0, samples 0 → effective 0.0 < MIN_ELECTION_SCORE
        assert_eq!(s.election_priority(&d, 1_000_000, 0.0, 0), None);
    }

    #[test]
    fn election_priority_excluded_returns_none() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(7);
        for _ in 0..5 {
            s.record_dc_slash(&d);
        }
        assert_eq!(s.election_priority(&d, 1_000_000, 0.5, 1000), None);
    }

    #[test]
    fn election_priority_well_formed_returns_some() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(8);
        // score 0.5, samples 1000 (confidence = 1.0) → effective = 0.5
        let p = s.election_priority(&d, 1_000_000, 0.5, 1000);
        assert!(p.is_some());
    }

    /// 1000-candidate differential test per mission 0855p-c (AC L33
    /// for cross-domain reputation). `priority_legacy` and canonical
    /// `election_priority` produce identical ordering over 1000
    /// candidates when both fully populated and configured to reduce
    /// to monotonic-in-stake (zero slashes, score=1.0, samples=100).
    #[test]
    fn differential_1000_candidates_byte_identical_ordering() {
        let s = DcRootedSlashReputationStoreCompat::new();
        struct Cand {
            did: RecorderDid,
            stake: u64,
        }
        let cands: Vec<Cand> = (0..1000u64)
            .map(|i| {
                // Deterministic stake: 1..=100_000. Zero cross-domain
                // slashes (the only configuration where both formulas
                // reduce to monotonic-in-stake).
                let stake = (i % 100_000) + 1;
                let did = RecorderDid::from_array({
                    let mut a = [0u8; 52];
                    a[0..8].copy_from_slice(&i.to_be_bytes());
                    a
                });
                Cand { did, stake }
            })
            .collect();
        // Compute priorities.
        let mut legacy: Vec<(u64, u128)> = cands
            .iter()
            .map(|c| {
                let p = s.priority_legacy(&c.did, c.stake).expect("not excluded");
                (c.stake, p)
            })
            .collect();
        let mut canonical: Vec<(u64, u128)> = cands
            .iter()
            .map(|c| {
                // score = 1.0, samples = MIN_CONFIDENCE_SAMPLES →
                // effective = 1.0 (no clamping, no floor).
                let p = s
                    .election_priority(&c.did, c.stake, 1.0, 100)
                    .expect("eligible");
                (c.stake, p)
            })
            .collect();
        legacy.sort_by_key(|(stake, p)| std::cmp::Reverse((*p, *stake)));
        canonical.sort_by_key(|(stake, p)| std::cmp::Reverse((*p, *stake)));
        let legacy_stakes: Vec<u64> = legacy.iter().map(|(s, _)| *s).collect();
        let canonical_stakes: Vec<u64> = canonical.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            legacy_stakes, canonical_stakes,
            "1000-candidate differential: legacy and canonical orderings must match"
        );
    }

    /// Cross-domain slash integration: a DC with high cross-domain
    /// slash count gets lower priority even with high stake. With
    /// the canonical formula (effective > 0 required), a DC with
    /// 3 cross-domain slashes still has a finite priority; the
    /// penalty vs. 0-slash DC is purely from the legacy
    /// `priority_legacy` field — the canonical formula does NOT
    /// discount by slash count (RFC-0968 §10 + amendment 28).
    /// This test pins that property so future drift gets caught.
    #[test]
    fn canonical_election_priority_is_slash_count_independent() {
        let s = DcRootedSlashReputationStoreCompat::new();
        let clean = dc_did(0xA);
        let slashed = dc_did(0xB);
        s.record_dc_slash(&slashed);
        s.record_dc_slash(&slashed);
        // Same stake + same score + same samples → same canonical
        // priority (the canonical formula has no slash-count term).
        let p_clean = s
            .election_priority(&clean, 500_000, 0.8, 1000)
            .expect("clean eligible");
        let p_slashed = s
            .election_priority(&slashed, 500_000, 0.8, 1000)
            .expect("slashed eligible (below threshold)");
        assert_eq!(
            p_clean, p_slashed,
            "canonical RFC-0968 §10 election_priority must NOT discount by slash count"
        );
        // ...but priority_legacy DOES discount.
        let l_clean = s
            .priority_legacy(&clean, 500_000)
            .expect("clean not excluded");
        let l_slashed = s
            .priority_legacy(&slashed, 500_000)
            .expect("slashed not excluded");
        assert!(
            l_slashed < l_clean,
            "legacy formula discounts by slash count: clean={l_clean} slashed={l_slashed}"
        );
    }

    // -- Path B: refresh_cross_domain_for (0855p-c wire-up) --
    //
    // Pins the contract that the in-memory map mirrors the persisted
    // store's slash events, filtered by `SignalKind::Slash` AND
    // `ReputationLayer::Coordinator`. The bridge from gossip ingress
    // → in-memory count uses `refresh_cross_domain_for` after a new
    // slash signal lands. Tests use `InMemoryReputationStore` and
    // `tokio::test` (the only backend where we can deterministically
    // pre-populate events for a unit test).

    use octo_determin::Dfp;
    use octo_reputation::store::InMemoryReputationStore;
    use octo_reputation::types::{ControllerId, EventId, SignalEvent};

    fn slash_event(
        seed: u64,
        did: RecorderDid,
        layer: ReputationLayer,
        kind: SignalKind,
    ) -> SignalEvent {
        SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: kind,
            layer,
            score_delta: Dfp::from_f64(0.0),
            recorded_at_unix: 1_700_000_000 + seed,
            rotation_provenance: None,
            audit_ref: None,
        }
    }

    #[tokio::test]
    async fn refresh_counts_only_coordinator_slashes() {
        // Three events for the same DC: 2 valid cross-domain slashes
        // (Slash + Coordinator), 1 outcome event on Market, 1 slash
        // on Market (NOT cross-domain). Refresh must return 2.
        let store = InMemoryReputationStore::new();
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(0xCC);
        store
            .record_signal(slash_event(
                1,
                d,
                ReputationLayer::Coordinator,
                SignalKind::Slash,
            ))
            .await
            .unwrap();
        store
            .record_signal(slash_event(
                2,
                d,
                ReputationLayer::Coordinator,
                SignalKind::Slash,
            ))
            .await
            .unwrap();
        store
            .record_signal(slash_event(
                3,
                d,
                ReputationLayer::Market,
                SignalKind::Outcome,
            ))
            .await
            .unwrap();
        store
            .record_signal(slash_event(
                4,
                d,
                ReputationLayer::Market,
                SignalKind::Slash,
            ))
            .await
            .unwrap();
        let refreshed = s.refresh_cross_domain_for(&d, &store).await.unwrap();
        assert_eq!(refreshed, 2);
        assert_eq!(s.cross_domain_slash_count_for(&d), 2);
    }

    #[tokio::test]
    async fn refresh_is_idempotent_on_no_new_events() {
        // Refresh twice with the same store; both calls return the
        // same count and the in-memory map converges to it.
        let store = InMemoryReputationStore::new();
        let s = DcRootedSlashReputationStoreCompat::new();
        let d = dc_did(0xDD);
        store
            .record_signal(slash_event(
                10,
                d,
                ReputationLayer::Coordinator,
                SignalKind::Slash,
            ))
            .await
            .unwrap();
        let a = s.refresh_cross_domain_for(&d, &store).await.unwrap();
        let b = s.refresh_cross_domain_for(&d, &store).await.unwrap();
        assert_eq!(a, b);
        assert_eq!(a, 1);
        // Cross-domain count reflects refreshed value (overrides any
        // prior `record_dc_slash` calls since refresh is authoritative).
        assert_eq!(s.cross_domain_slash_count_for(&d), 1);
    }
}
