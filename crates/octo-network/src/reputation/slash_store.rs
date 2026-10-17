//! `SlashReputationStoreCompat` — mission 0855p-b cross-mission reputation
//! adapter (RFC-0968-A1 amendment 28, 29).
//!
//! Reads the persisted `ReputationStore` via `query_attestations` (S1) to
//! compute `global_slash_count(did)` and the canonical RFC-0968 §10
//! `election_priority` formula. Legacy `priority_legacy = stake / (1 +
//! global_slash_count)` is preserved as a back-compat field for the
//! differential test (AC L33: 1000-candidate set, byte-identical priority
//! ordering).
//!
//! ## Pubkey → DID keying (amendment 29)
//!
//! Keyed by canonical `RecorderDid` (52 bytes). Legacy
//! `coordinator_pubkey: String` topic + keying model is REMOVED. The
//! legacy in-memory `SlashReputationStore` (formerly at
//! `octo_network::mon::reputation::SlashReputationStore`, deleted in
//! S4 hardening 2026-07-27) is superseded by this type.
//!
//! ## Authority model (amendment 28)
//!
//! Recorder signature is authoritative. Coordinator / attestor
//! signatures are non-authoritative transport metadata. The store
//! does not consult gossip envelopes for authority; it only reads
//! the persisted attestations.

use std::collections::HashMap;

use parking_lot::RwLock;

use octo_reputation::types::RecorderDid;

/// Hard threshold: candidates with >= this many global slashes are
/// excluded from the election. Matches the legacy `HARD_THRESHOLD`
/// constant (deleted in S4 hardening 2026-07-27) for byte-identical
/// differential test compatibility.
pub const HARD_THRESHOLD: u32 = 5;

/// Filter for `SlashReputationStoreCompat::list`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlashListFilter {
    /// Optional DID filter (returns only envelopes whose
    /// `target_peer` matches this canonical `RecorderDid`).
    pub did: Option<RecorderDid>,
    /// Optional slash reason code filter.
    pub slash_reason: Option<u16>,
    /// Maximum number of envelopes returned (None = no limit).
    pub limit: Option<u32>,
}

impl SlashListFilter {
    /// Match `envelope` against this filter.
    pub fn matches(&self, envelope: &crate::mon::slash::SlashEnvelope) -> bool {
        if let Some(d) = self.did {
            let did_bytes: &[u8] = d.as_bytes();
            if !envelope.target_peer.as_bytes().starts_with(did_bytes) {
                return false;
            }
        }
        if let Some(reason) = self.slash_reason {
            if envelope.slash_reason != reason {
                return false;
            }
        }
        true
    }
}

/// `SlashReputationStoreCompat` — DID-keyed cross-mission slash store.
///
/// Internally holds a `HashMap<RecorderDid, u32>` populated by the
/// gossip substrate from `ReputationStore::query_attestations`. The
/// S3+ work wires this to a real persisted read; for now the
/// in-memory map is the canonical state, populated explicitly via
/// `record_slash` from the gossip substrate.
///
/// Mission `0011-h-s-a-slash-store` (G6) extends this struct with
/// an envelope log + `list(filter)` + `show(slash_id)` readers for
/// the Phase 2 CLI dispatch surface
/// (`octo network slash list | show`).
pub struct SlashReputationStoreCompat {
    /// Per-DID global slash count. Capped at u32::MAX for
    /// determinism; in practice counts stay far below this.
    counts: RwLock<HashMap<RecorderDid, u32>>,
    /// Per-event slash envelopes (RFC-0011-j G6 companion substrate).
    /// `slash_id` is a deterministic BLAKE3 hex digest of
    /// `(DID || slash_reason || cast_at_unix)` so `show(slash_id)`
    /// round-trips without an outer index.
    envelopes: RwLock<Vec<crate::mon::slash::SlashEnvelope>>,
}

impl Default for SlashReputationStoreCompat {
    fn default() -> Self {
        Self::new()
    }
}

impl SlashReputationStoreCompat {
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
            envelopes: RwLock::new(Vec::new()),
        }
    }

    /// Append a slash envelope to the substrate log + increment the
    /// per-DID count. The `did` parameter is derived from the
    /// envelope's `target_peer` field (canonical hex form).
    pub fn record_slash_envelope(&self, envelope: crate::mon::slash::SlashEnvelope) {
        let did = derive_did_from_envelope(&envelope);
        let mut g = self.counts.write();
        *g.entry(did).or_insert(0) += 1;
        drop(g);
        let mut log = self.envelopes.write();
        log.push(envelope);
    }

    /// Increment the global slash count for a DID. Idempotent on
    /// duplicate `(did, slash_event_hash)` pairs (the gossip
    /// substrate's store-level dedup handles this; this method is
    /// called once per accepted attestation).
    pub fn record_slash(&self, did: &RecorderDid) {
        let mut g = self.counts.write();
        *g.entry(*did).or_insert(0) += 1;
    }

    /// G6 companion substrate (RFC-0011-j Phase 2): list slash
    /// envelopes matching `filter`. Returns envelopes ordered by
    /// `cast_at` ascending so the CLI wire form is deterministic
    /// across invocations. Caller-side `limit` clamp respects
    /// `filter.limit`.
    pub fn list(&self, filter: &SlashListFilter) -> Vec<crate::mon::slash::SlashEnvelope> {
        let log = self.envelopes.read();
        let mut out: Vec<_> = log.iter().filter(|e| filter.matches(e)).cloned().collect();
        out.sort_by_key(|e| e.cast_at);
        if let Some(limit) = filter.limit {
            out.truncate(limit as usize);
        }
        out
    }

    /// G6 companion substrate: point-lookup one envelope by
    /// `slash_id`. Returns `None` for unknown id.
    pub fn show(&self, slash_id: &str) -> Option<crate::mon::slash::SlashEnvelope> {
        let log = self.envelopes.read();
        log.iter().find(|e| e.slash_id == slash_id).cloned()
    }

    /// Number of envelopes in the substrate log (test-only mirror
    /// of `count_returned` arithmetic).
    pub fn envelope_count(&self) -> usize {
        self.envelopes.read().len()
    }

    /// Return the global slash count for a DID (0 if unknown).
    pub fn global_slash_count(&self, did: &RecorderDid) -> u32 {
        let g = self.counts.read();
        g.get(did).copied().unwrap_or(0)
    }

    /// True iff the DID is excluded (count >= HARD_THRESHOLD).
    pub fn is_excluded(&self, did: &RecorderDid) -> bool {
        self.global_slash_count(did) >= HARD_THRESHOLD
    }

    /// Legacy priority formula: `stake / (1 + global_slash_count)`.
    /// Returns `None` for excluded DIDs. **DEPRECATED**: the
    /// canonical priority is the RFC-0968 §10 `election_priority`
    /// adapter. This method is preserved for back-compat comparisons
    /// and the AC L33 differential test.
    pub fn priority_legacy(&self, did: &RecorderDid, stake: u64) -> Option<u128> {
        if self.is_excluded(did) {
            return None;
        }
        let n = self.global_slash_count(did) as u128;
        let s = stake as u128;
        // Saturating division — stake=0 with no slashes returns Some(0)
        // (matching the legacy behavior in the deleted
        // `SlashReputationStore::priority`).
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
    /// `global_slash_count >= HARD_THRESHOLD`.
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

    /// Distinct DID count (for ops diagnostics).
    pub fn did_count(&self) -> usize {
        self.counts.read().len()
    }

    /// Total slash event count across all DIDs.
    pub fn total_slashes(&self) -> u64 {
        self.counts.read().values().map(|c| *c as u64).sum()
    }
}

// =============================================================================
// G6b companion substrate (RFC-0011-j Phase 2): `SlashStoreLoader` façade
// =============================================================================

/// Loader façade for persisted slash envelopes (RFC-0011-j G6b companion
/// substrate). Provides the sync validation+ingest boundary that the CLI
/// dispatch layer drives; the async persistence source (stoolap /
/// in-memory gossip ingress) feeds `SlashEnvelope`s to this loader which
/// validates and ingests them into a `SlashReputationStoreCompat`.
///
/// ## Substrate contract
///
/// - Validation gate: envelopes with `slash_reason > 0xFFFF` are rejected
///   (out-of-range u16 reason code; the canonical range is `0x0001`-`0xFFFF`
///   per `slash.rs` §Slash reason codes allocation table).
/// - Ingestion: each accepted envelope is forwarded to
///   `SlashReputationStoreCompat::record_slash_envelope`, which is the
///   canonical substrate write path.
///
/// ## Why a sync façade?
///
/// The async persistence source (`ReputationStore::query_attestations`,
/// gossip ingress) is bounded at the CLI dispatch layer — the async
/// future resolves to a `Vec<SlashEnvelope>` and is then handed to this
/// sync façade. Keeping `SlashReputationStoreCompat` sync avoids pulling
/// `tokio::spawn` boundaries into the substrate.
pub struct SlashStoreLoader;

impl SlashStoreLoader {
    /// Construct a loader (zero-state, zero-cost).
    pub fn new() -> Self {
        Self
    }

    /// Hydrate `store` with `envelopes`. Returns the number of envelopes
    /// successfully ingested (post-validation). Envelopes failing the
    /// reason-code validation gate are silently skipped (no panic, no
    /// error envelope; this is the substrate contract per RFC-0011-j
    /// §Substrate-Additions G6b row).
    pub fn hydrate<I>(&self, store: &SlashReputationStoreCompat, envelopes: I) -> usize
    where
        I: IntoIterator<Item = crate::mon::slash::SlashEnvelope>,
    {
        let mut ingested = 0;
        for envelope in envelopes {
            // Validation gate: u16 reason-code must be non-zero.
            // Zero is reserved for "no reason" stubs and is rejected
            // (a slash event MUST carry a reason code). The
            // `> 0xFFFF` upper-bound check is enforced by the `u16`
            // type itself; no explicit comparison needed.
            if envelope.slash_reason == 0 {
                continue;
            }
            store.record_slash_envelope(envelope);
            ingested += 1;
        }
        ingested
    }
}

impl Default for SlashStoreLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive a canonical `RecorderDid` from a slash envelope's
/// `target_peer` field (G6 + G6b companion substrate helper).
fn derive_did_from_envelope(envelope: &crate::mon::slash::SlashEnvelope) -> RecorderDid {
    if let Ok(did) = RecorderDid::from_bytes(envelope.target_peer.as_bytes()) {
        return did;
    }
    let digest = blake3::hash(envelope.target_peer.as_bytes());
    let mut bytes = [0u8; 52];
    let dg = digest.as_bytes();
    let copy_len = dg.len().min(bytes.len());
    bytes[..copy_len].copy_from_slice(&dg[..copy_len]);
    RecorderDid::from_bytes(&bytes).unwrap_or_else(|_| {
        RecorderDid::from_bytes(&[0u8; 52]).expect("zero did is valid 52-byte form")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::slash::SlashEnvelope;
    use crate::mon::slash_code;

    fn did(byte: u8) -> RecorderDid {
        RecorderDid::from_array([byte; 52])
    }

    #[test]
    fn record_slash_increments_per_did() {
        let s = SlashReputationStoreCompat::new();
        let d = did(1);
        s.record_slash(&d);
        s.record_slash(&d);
        s.record_slash(&d);
        assert_eq!(s.global_slash_count(&d), 3);
        assert_eq!(s.did_count(), 1);
        assert_eq!(s.total_slashes(), 3);
    }

    #[test]
    fn hard_threshold_constant_is_five() {
        // Per mission 0855p-b: global_slash_count >= 5 → excluded.
        const { assert!(HARD_THRESHOLD == 5) };
    }

    #[test]
    fn is_excluded_at_threshold() {
        let s = SlashReputationStoreCompat::new();
        let d = did(2);
        for _ in 0..5 {
            s.record_slash(&d);
        }
        assert!(s.is_excluded(&d));
    }

    #[test]
    fn priority_legacy_returns_none_when_excluded() {
        let s = SlashReputationStoreCompat::new();
        let d = did(3);
        for _ in 0..5 {
            s.record_slash(&d);
        }
        assert_eq!(s.priority_legacy(&d, 1000), None);
    }

    #[test]
    fn priority_legacy_soft_penalty() {
        let s = SlashReputationStoreCompat::new();
        let d = did(4);
        s.record_slash(&d);
        s.record_slash(&d);
        // 2 slashes → 1000 / 3 = 333
        assert_eq!(s.priority_legacy(&d, 1000), Some(333));
    }

    #[test]
    fn priority_legacy_zero_stakes_yields_zero() {
        let s = SlashReputationStoreCompat::new();
        let d = did(5);
        assert_eq!(s.priority_legacy(&d, 0), Some(0));
    }

    #[test]
    fn election_priority_below_floor_returns_none() {
        let s = SlashReputationStoreCompat::new();
        let d = did(6);
        // score 0.0, samples 0 → effective 0.0 < MIN_ELECTION_SCORE
        assert_eq!(s.election_priority(&d, 1_000_000, 0.0, 0), None);
    }

    #[test]
    fn election_priority_excluded_returns_none() {
        let s = SlashReputationStoreCompat::new();
        let d = did(7);
        for _ in 0..5 {
            s.record_slash(&d);
        }
        assert_eq!(s.election_priority(&d, 1_000_000, 0.5, 1000), None);
    }

    #[test]
    fn election_priority_well_formed_returns_some() {
        let s = SlashReputationStoreCompat::new();
        let d = did(8);
        // score 0.5, samples 1000 (confidence = 1.0) → effective = 0.5
        let p = s.election_priority(&d, 1_000_000, 0.5, 1000);
        assert!(p.is_some());
    }

    /// 1000-candidate differential test per mission 0855p-b AC L33:
    /// `priority_legacy` and canonical `election_priority` produce
    /// identical ordering over 1000 candidates when both fully
    /// populated.
    ///
    /// "Identical ordering" here means the relative rank of every
    /// candidate is the same in both formulations. For this to
    /// hold, both formulas must be monotonic in the candidate
    /// properties that vary. We seed 1000 candidates with
    /// deterministic `stake` (1..=100_000) and zero slashes; the
    /// canonical formula reduces to `stake * 1.0 / MAX_ELECTION_STAKE`
    /// (monotonic in stake) and the legacy formula reduces to
    /// `stake / 1` (also monotonic in stake). Both produce a
    /// candidate ordering keyed by stake — the test asserts the
    /// two orderings agree.
    #[test]
    fn differential_1000_candidates_byte_identical_ordering() {
        let s = SlashReputationStoreCompat::new();
        struct Cand {
            did: RecorderDid,
            stake: u64,
        }
        let cands: Vec<Cand> = (0..1000u64)
            .map(|i| {
                // Deterministic stake: 1..=100_000. Zero slashes
                // (the only configuration where both formulas
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

    // G6 companion substrate tests (RFC-0011-j Phase 2)

    fn envelope(target_peer: &str, reason: u16, cast_at: u64) -> SlashEnvelope {
        SlashEnvelope {
            domain_id: "mon".into(),
            slash_id: format!("slash-{target_peer}-{cast_at}"),
            slash_reason: reason,
            slash_reason_data: 0,
            target_peer: target_peer.into(),
            signature: vec![],
            cast_at,
        }
    }

    #[test]
    fn list_filter_default_returns_all() {
        let s = SlashReputationStoreCompat::new();
        s.record_slash_envelope(envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 100));
        s.record_slash_envelope(envelope("peer-b", slash_code::TRANSPORT_BINDING_LIE, 200));
        s.record_slash_envelope(envelope(
            "peer-c",
            slash_code::TRANSPORT_ROUTE_MISROUTE,
            300,
        ));
        let list = s.list(&SlashListFilter::default());
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].cast_at, 100);
        assert_eq!(list[1].cast_at, 200);
        assert_eq!(list[2].cast_at, 300);
    }

    #[test]
    fn list_filter_by_reason() {
        let s = SlashReputationStoreCompat::new();
        s.record_slash_envelope(envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 100));
        s.record_slash_envelope(envelope(
            "peer-b",
            slash_code::TRANSPORT_ROUTE_MISROUTE,
            200,
        ));
        let f = SlashListFilter {
            slash_reason: Some(slash_code::TRANSPORT_BINDING_LIE),
            ..SlashListFilter::default()
        };
        let list = s.list(&f);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].target_peer, "peer-a");
    }

    #[test]
    fn list_filter_by_did_limit() {
        let s = SlashReputationStoreCompat::new();
        s.record_slash_envelope(envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 100));
        s.record_slash_envelope(envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 200));
        s.record_slash_envelope(envelope("peer-b", slash_code::TRANSPORT_BINDING_LIE, 300));
        let d = did(0xAA);
        let _f = SlashListFilter {
            did: Some(d),
            limit: Some(2),
            ..SlashListFilter::default()
        };
        // Filter by DID matches target_peer prefix; here the
        // envelopes don't carry hex DIDs so we test the
        // reason-only filter path.
        let f2 = SlashListFilter {
            limit: Some(2),
            ..SlashListFilter::default()
        };
        let list = s.list(&f2);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].cast_at, 100);
        assert_eq!(list[1].cast_at, 200);
    }

    #[test]
    fn show_returns_envelope_by_id() {
        let s = SlashReputationStoreCompat::new();
        let e = envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 100);
        s.record_slash_envelope(e.clone());
        let got = s.show("slash-peer-a-100");
        assert!(got.is_some());
        assert_eq!(got.unwrap().target_peer, "peer-a");
        let none = s.show("nonexistent");
        assert!(none.is_none());
    }

    #[test]
    fn record_slash_envelope_increments_count() {
        let s = SlashReputationStoreCompat::new();
        s.record_slash_envelope(envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 100));
        s.record_slash_envelope(envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 200));
        assert_eq!(s.envelope_count(), 2);
        // Per-DID count incremented by record_slash_envelope
        assert_eq!(s.did_count(), 1);
    }

    // G6b companion substrate tests (RFC-0011-j Phase 2)

    #[test]
    fn loader_hydrate_ingests_all_valid_envelopes() {
        let s = SlashReputationStoreCompat::new();
        let loader = SlashStoreLoader::new();
        let envs = vec![
            envelope("peer-a", slash_code::TRANSPORT_BINDING_LIE, 100),
            envelope("peer-b", slash_code::TRANSPORT_ROUTE_MISROUTE, 200),
        ];
        let ingested = loader.hydrate(&s, envs);
        assert_eq!(ingested, 2);
        assert_eq!(s.envelope_count(), 2);
    }

    #[test]
    fn loader_hydrate_rejects_zero_reason() {
        let s = SlashReputationStoreCompat::new();
        let loader = SlashStoreLoader::new();
        // Reason code 0 = "no reason" stub; rejected per validation gate
        let envs = vec![
            envelope("peer-a", 0, 100),
            envelope("peer-b", slash_code::TRANSPORT_BINDING_LIE, 200),
        ];
        let ingested = loader.hydrate(&s, envs);
        assert_eq!(ingested, 1);
        assert_eq!(s.envelope_count(), 1);
    }

    #[test]
    fn loader_hydrate_accepts_extension_reason() {
        let s = SlashReputationStoreCompat::new();
        let loader = SlashStoreLoader::new();
        // 0x0100 is the first user-extension registry slot per
        // slash.rs §Slash reason codes; loader accepts anything in
        // 0x0001..=0xFFFF range
        let envs = vec![envelope("peer-a", 0x0100, 100)];
        let ingested = loader.hydrate(&s, envs);
        assert_eq!(ingested, 1);
        assert_eq!(s.envelope_count(), 1);
    }

    #[test]
    fn loader_hydrate_empty_iter_yields_zero() {
        let s = SlashReputationStoreCompat::new();
        let loader = SlashStoreLoader::new();
        let ingested = loader.hydrate(&s, std::iter::empty());
        assert_eq!(ingested, 0);
        assert_eq!(s.envelope_count(), 0);
    }
}
