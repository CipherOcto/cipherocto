//! In-memory marketplace index (RFC-0959 §Roles).
//!
//! `BTreeMap<(namespace, family, Option<version>), BTreeSet<AskId>>` — preserves
//! ordered, deterministic scans (NOT `HashMap`). The BTreeSet is keyed by
//! `AskId` itself, giving O(log n) tie-break by `ask_id` ASC (lowest ask_id wins).
//!
//! Cache invalidation: asks with `expires_at_unix < now` are evicted lazily
//! (on `select_ask`) and via explicit `prune_expired(now_unix)`. Total active
//! count is capped at 100K; an over-cap `insert` prunes by earliest
//! `expires_at_unix` (the closest available proxy for publication order — see
//! [`MarketplaceIndex::prune_expired_and_overflow`]).
//!
//! Synced from RFC-0862 sync events (this module is the in-memory mirror;
//! the `asks` table in stoolap is the durable source).

use std::collections::{BTreeMap, BTreeSet};

use crate::ask::{Ask, AskId, AskerDid, ModelRef};

/// Maximum number of active asks in the index before FIFO pruning kicks in.
/// RFC-0959 §Roles: `pruned in published_at_unix ASC order at cap 100K`.
pub const ACTIVE_ASK_CAP: usize = 100_000;

/// In-memory marketplace index (RFC-0959 §Roles).
///
/// Backed by `BTreeMap` for deterministic, ordered scans. The key is the
/// `(namespace, family, version)` triple; the value is a `BTreeSet<AskId>`
/// which itself orders by `AskId` for tie-break during `select_ask`.
#[derive(Debug)]
pub struct MarketplaceIndex {
    /// Active asks keyed by model triple.
    by_model: BTreeMap<(String, String, Option<String>), BTreeSet<AskId>>,
    /// Mirror of all active asks for pruning / iteration.
    ///
    /// The value is a bare `Ask`, which does **not** carry
    /// `published_at_unix` (that lives on `AskUnsignedPayload`, the pre-mint
    /// form). Overflow eviction therefore approximates publication order with
    /// `expires_at_unix`; see [`Self::prune_expired_and_overflow`].
    indexed: BTreeMap<AskId, Ask>,
    /// Total active count (mirror of `indexed.len()`; kept for cheap cap check).
    count: usize,
    /// Maximum entries before [`Self::insert`] prunes. Always
    /// [`ACTIVE_ASK_CAP`] outside tests; a private field so the over-cap
    /// path is reachable from a unit test without a 100K-iteration loop.
    cap: usize,
}

impl Default for MarketplaceIndex {
    /// Delegates to [`Self::new`] so the derived-default `cap == 0` can never
    /// appear: a zero cap would make `insert` prune on every call and silently
    /// leave the index empty.
    fn default() -> Self {
        Self::new()
    }
}

impl MarketplaceIndex {
    /// Construct an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_model: BTreeMap::new(),
            indexed: BTreeMap::new(),
            count: 0,
            cap: ACTIVE_ASK_CAP,
        }
    }

    /// Construct an empty index with a lowered capacity.
    ///
    /// Test-only: [`ACTIVE_ASK_CAP`] is large enough that reaching it in a
    /// unit test is impractical, and the over-cap branch of [`Self::insert`]
    /// is where the pruning policy actually lives.
    #[cfg(test)]
    #[must_use]
    fn with_cap(cap: usize) -> Self {
        Self { cap, ..Self::new() }
    }

    /// Insert an Ask. If `ask_id` already present, replaces the entry.
    /// Returns `true` if the index grew (new ask_id); `false` if replaced.
    /// Triggers pruning if the active count exceeds `ACTIVE_ASK_CAP`.
    pub fn insert(&mut self, ask: Ask) -> bool {
        let id = ask.id();
        let key = model_key(&ask.model);
        let already_present = self.indexed.contains_key(&id);
        self.by_model.entry(key).or_default().insert(id);
        self.indexed.insert(id, ask);
        if !already_present {
            self.count += 1;
        }
        // Prune if over cap.
        //
        // The cutoff is `0`, not `u64::MAX`: `prune_expired` removes asks
        // with `expires_at_unix < now_unix`, so passing `u64::MAX` here
        // marked **every** ask expired and emptied the whole index on the
        // over-cap insert. This path is deliberately time-agnostic — `insert`
        // has no clock argument, and expiry is applied by `select_ask` — so
        // `0` disables the time filter and leaves only the count cap.
        if self.count > self.cap {
            self.prune_expired_and_overflow(0);
        }
        !already_present
    }

    /// Remove an Ask by id (no-op if not present).
    pub fn remove(&mut self, ask_id: &AskId) -> bool {
        if let Some(ask) = self.indexed.remove(ask_id) {
            let key = model_key(&ask.model);
            if let Some(set) = self.by_model.get_mut(&key) {
                set.remove(ask_id);
                if set.is_empty() {
                    self.by_model.remove(&key);
                }
            }
            self.count = self.count.saturating_sub(1);
            return true;
        }
        false
    }

    /// Number of active asks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Fetch an Ask by id (clone).
    #[must_use]
    pub fn get(&self, ask_id: &AskId) -> Option<&Ask> {
        self.indexed.get(ask_id)
    }

    /// Total entries in the model index (for tests / introspection).
    #[must_use]
    pub fn model_bucket_count(&self) -> usize {
        self.by_model.len()
    }

    /// Evict asks with `expires_at_unix < now_unix`. Returns the number of
    /// entries removed.
    pub fn prune_expired(&mut self, now_unix: u64) -> usize {
        let expired: Vec<AskId> = self
            .indexed
            .iter()
            .filter(|(_, a)| a.expires_at_unix < now_unix)
            .map(|(id, _)| *id)
            .collect();
        let n = expired.len();
        for id in expired {
            self.remove(&id);
        }
        n
    }

    /// Evict expired asks; if still over cap, prune oldest by `expires_at_unix`
    /// (RFC-0959 §Roles: "pruned in `published_at_unix` ASC order at cap 100K").
    /// Since `Ask` does not store `published_at_unix` (it lives in
    /// `AskUnsignedPayload` and the `created_at_unix` DB row), use the earliest
    /// `expires_at_unix` first — TTL is monotonic with publication time within
    /// reasonable operational envelopes (operators mint 30d TTLs).
    ///
    /// Callers that mean "enforce the count cap without applying a time
    /// filter" must pass `0`. Passing `u64::MAX` is the opposite: it matches
    /// every ask and clears the index.
    ///
    /// Returns the number of entries removed.
    pub fn prune_expired_and_overflow(&mut self, now_unix: u64) -> usize {
        let mut removed = self.prune_expired(now_unix);
        while self.count > self.cap {
            // Find ask with smallest expires_at_unix.
            let oldest = self
                .indexed
                .iter()
                .min_by_key(|(_, a)| a.expires_at_unix)
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => {
                    self.remove(&id);
                    removed += 1;
                }
                None => break,
            }
        }
        removed
    }

    /// List all **non-expired** Asks published by `asker_did`.
    ///
    /// `now_unix` is the expiry cutoff, matching the sibling
    /// `AskRead::list_by_asker` contract and this type's own
    /// [`Self::select_ask`], which evicts expired asks before selecting.
    ///
    /// This previously took no cutoff and returned expired asks, which
    /// contradicted both its own doc and the storage-level reader of the
    /// same name. The index retains expired asks until they are pruned
    /// (on insert-overflow or an explicit [`Self::prune_expired`]), so
    /// the filter belongs here rather than in `insert`.
    pub fn list_by_asker(&self, asker_did: &AskerDid, now_unix: u64) -> Vec<Ask> {
        self.indexed
            .values()
            .filter(|a| &a.asker_did == asker_did && a.expires_at_unix > now_unix)
            .cloned()
            .collect()
    }

    /// Select the cheapest Ask matching `(model, jurisdiction, budget_ceiling)`.
    ///
    /// "Cheapest" is evaluated only against asks that survive the two gates
    /// this function actually applies: **expiry** (evicted lazily via
    /// `prune_expired`) and **`budget_ceiling`**. RFC-0959 §Ask state machine
    /// defines further gates that are **not** applied here — in particular the
    /// `Published -> Active` `soft_delay` warm-up. That gate cannot be applied
    /// as things stand: `Ask` does not carry `published_at_unix`, which lives
    /// on `AskUnsignedPayload`, the pre-mint form this index does not hold.
    /// The same loss already makes the jurisdiction filter inert (see below).
    /// See mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`.
    ///
    /// Tie-break: lowest `ask_id` wins (deterministic across calls / nodes).
    ///
    /// `budget_ceiling` is the maximum cost in `Dqa` the caller is
    /// willing to pay for a synthetic `1k-token` consumption per axis (matches
    /// `AskerRepository::cheapest` proxy).
    /// `axes` supplies the standard pricing axes for cost computation.
    /// `now_unix` is the cutoff for expiry eviction.
    /// Returns `None` if no Ask matches.
    #[must_use]
    pub fn select_ask(
        &mut self,
        model: &str,
        jurisdiction: &[String],
        budget_ceiling: crate::ask::Dqa,
        axes: &[crate::ask::PricingAxis],
        now_unix: u64,
    ) -> Option<Ask> {
        // Best-effort eviction (cheap when few expired).
        self.prune_expired(now_unix);
        let key = parse_model_key(model);
        let candidates = self.by_model.get(&key)?;
        // BTreeSet already ordered by AskId (which is byte-lexicographic).
        // Filter by cost + jurisdiction; pick lowest. The `Ask` post-mint
        // form does not carry jurisdiction (lives on `AskUnsignedPayload`),
        // so jurisdiction_matches() returns true for all entries; the
        // filter is preserved for the future case where the marketplace
        // holds the payload form.
        let mut best: Option<(crate::ask::Dqa, Ask)> = None;
        for id in candidates {
            let ask = self.indexed.get(id)?;
            if !jurisdiction_matches(&[], jurisdiction) {
                continue;
            }
            let consumed: Vec<_> = ask
                .rates
                .rates
                .iter()
                .map(|r| (r.axis.clone(), 1000u64))
                .collect();
            let cost = crate::ask::settlement_cost(ask, &consumed, axes);
            if octo_determin::dqa_cmp(cost, budget_ceiling) > 0 {
                continue;
            }
            let replace = match &best {
                None => true,
                Some((best_cost, best_ask)) => {
                    octo_determin::dqa_cmp(cost, *best_cost) < 0
                        || (cost == *best_cost && ask.id() < best_ask.id())
                }
            };
            if replace {
                best = Some((cost, ask.clone()));
            }
        }
        best.map(|(_, a)| a)
    }

    /// Iterate over all active asks (for sync rebuild / audit).
    pub fn iter(&self) -> impl Iterator<Item = &Ask> {
        self.indexed.values()
    }
}

/// Parse a `model` string into the `(namespace, family, version)` key.
///
/// Accepts `"namespace/family/version"` or `"namespace/family"`.
/// Returns `(namespace, family, None)` for 2-segment; full triple for 3-segment.
/// Empty / malformed inputs are coerced to `("", "", None)` so the lookup
/// remains consistent (no panic).
fn parse_model_key(model: &str) -> (String, String, Option<String>) {
    let parts: Vec<&str> = model.splitn(3, '/').collect();
    match parts.len() {
        1 => (parts[0].to_owned(), String::new(), None),
        2 => (parts[0].to_owned(), parts[1].to_owned(), None),
        3 => (
            parts[0].to_owned(),
            parts[1].to_owned(),
            Some(parts[2].to_owned()),
        ),
        _ => (String::new(), String::new(), None),
    }
}

fn model_key(model: &ModelRef) -> (String, String, Option<String>) {
    parse_model_key(&model.to_wire())
}

/// Jurisdiction match: empty `ask.jurisdiction` = wildcard ("`*`") accept any;
/// otherwise caller must declare at least one jurisdiction that matches.
fn jurisdiction_matches(declared: &[String], actual: &[String]) -> bool {
    if declared.is_empty() {
        return true; // wildcard
    }
    if actual.is_empty() {
        return false; // caller declared nothing but ask restricts
    }
    declared.iter().any(|d| actual.iter().any(|a| a == d))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ask::{AxisRate, ModelRateTable, PricingAxis};

    fn sample_ask(asker: &str, model: &str, rate: u128, ttl: u64) -> Ask {
        Ask::new(
            asker,
            ModelRef::from(model),
            ModelRateTable {
                model: ModelRef::from(model),
                rates: vec![AxisRate {
                    axis: "input_tokens_per_1k".to_owned(),
                    rate_per_1k: octo_determin::Dqa::new(
                        i64::try_from(rate).expect("rate fits in i64"),
                        0,
                    )
                    .expect("non-overflow"),
                }],
            },
            [0x42; 16],
            ttl,
        )
        .unwrap()
    }

    #[test]
    fn empty_index_has_zero_len() {
        let idx = MarketplaceIndex::new();
        assert_eq!(idx.len(), 0);
        assert!(idx.is_empty());
        assert_eq!(idx.model_bucket_count(), 0);
    }

    #[test]
    fn insert_records_ask_and_lookup() {
        let mut idx = MarketplaceIndex::new();
        let a = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            1_900_000_000,
        );
        let id = a.id();
        assert!(idx.insert(a.clone()));
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.get(&id).unwrap().asker_did, a.asker_did);
        assert_eq!(idx.model_bucket_count(), 1);
    }

    #[test]
    fn insert_same_id_twice_replaces() {
        let mut idx = MarketplaceIndex::new();
        let a = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            1_900_000_000,
        );
        let id = a.id();
        assert!(idx.insert(a.clone()));
        // Re-insert EXACT same ask (same rate → same id) — count unchanged.
        assert!(!idx.insert(a.clone()));
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.get(&id).unwrap().id(), id);
    }

    #[test]
    fn remove_drops_ask_and_clears_empty_bucket() {
        let mut idx = MarketplaceIndex::new();
        let a = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            1_900_000_000,
        );
        let id = a.id();
        idx.insert(a);
        assert!(idx.remove(&id));
        assert!(idx.get(&id).is_none());
        assert_eq!(idx.model_bucket_count(), 0);
        assert_eq!(idx.len(), 0);
        // Remove missing returns false.
        assert!(!idx.remove(&id));
    }

    #[test]
    fn prune_expired_drops_only_expired() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        let active = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            now + 1000,
        );
        let expired = sample_ask("did:octo:b", "openai/gpt-4", 30_000, now - 1);
        idx.insert(active);
        idx.insert(expired);
        let removed = idx.prune_expired(now);
        assert_eq!(removed, 1);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn select_ask_picks_cheapest_within_budget() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        let cheap = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            10_000,
            now + 1000,
        );
        let mid = sample_ask("did:octo:b", "openai/gpt-4", 30_000, now + 1000);
        let expensive = sample_ask(
            &octo_ident::test_helpers::sample_did(191),
            "openai/gpt-4",
            100_000,
            now + 1000,
        );
        idx.insert(cheap);
        idx.insert(mid);
        idx.insert(expensive);
        let axes = PricingAxis::standard_axes();
        let picked = idx
            .select_ask(
                "openai/gpt-4",
                &[],
                octo_determin::Dqa::new(50_000, 0).expect("non-overflow"),
                &axes,
                now,
            )
            .expect("pick");
        assert_eq!(picked.asker_did, octo_ident::test_helpers::sample_did(111));
    }

    #[test]
    fn select_ask_deterministic_tie_break_by_ask_id() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        let a = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            now + 1000,
        );
        let b = sample_ask("did:octo:b", "openai/gpt-4", 30_000, now + 1000);
        let id_a = a.id();
        let id_b = b.id();
        // Two asks at the same rate; tie-break by lowest ask_id.
        idx.insert(a);
        idx.insert(b);
        let axes = PricingAxis::standard_axes();
        let picked = idx
            .select_ask(
                "openai/gpt-4",
                &[],
                octo_determin::Dqa::new(100_000, 0).expect("non-overflow"),
                &axes,
                now,
            )
            .expect("pick");
        let expected = if id_a < id_b { id_a } else { id_b };
        assert_eq!(picked.id(), expected);
    }

    #[test]
    fn select_ask_filters_by_budget() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        let mut ask = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            now + 1000,
        );
        ask.rates.rates[0].rate_per_1k = octo_determin::Dqa::new(200_000, 0).expect("non-overflow");
        idx.insert(ask);
        let axes = PricingAxis::standard_axes();
        let picked = idx.select_ask(
            "openai/gpt-4",
            &[],
            octo_determin::Dqa::new(100_000, 0).expect("non-overflow"),
            &axes,
            now,
        );
        assert!(picked.is_none(), "exceeds budget");
    }

    #[test]
    fn select_ask_jurisdiction_filter_structural() {
        // `Ask` (in-memory post-mint) does not carry jurisdiction — that
        // field lives on `AskUnsignedPayload`. The marketplace index uses
        // `Ask` for storage, so jurisdiction_matches() sees empty declared
        // and accepts all (wildcard). This test documents the structural
        // behavior; future versions that store jurisdiction on the
        // post-mint form will tighten the filter.
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        let ask = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            30_000,
            now + 1000,
        );
        idx.insert(ask);
        let axes = PricingAxis::standard_axes();
        let picked = idx.select_ask(
            "openai/gpt-4",
            &["EU".to_owned()],
            octo_determin::Dqa::new(100_000, 0).expect("non-overflow"),
            &axes,
            now,
        );
        assert!(picked.is_some());
    }

    #[test]
    fn select_ask_unknown_model_returns_none() {
        let mut idx = MarketplaceIndex::new();
        let axes = PricingAxis::standard_axes();
        let picked = idx.select_ask(
            "nonexistent/model",
            &[],
            octo_determin::Dqa::new(100_000, 0).expect("non-overflow"),
            &axes,
            1_700_000_000,
        );
        assert!(picked.is_none());
    }

    #[test]
    fn select_ask_evicts_expired_lazily() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        let expired = sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            10_000,
            now - 1,
        );
        idx.insert(expired);
        let axes = PricingAxis::standard_axes();
        let picked = idx.select_ask(
            "openai/gpt-4",
            &[],
            octo_determin::Dqa::new(100_000, 0).expect("non-overflow"),
            &axes,
            now,
        );
        assert!(
            picked.is_none(),
            "expired ask must be evicted + not returned"
        );
        assert_eq!(idx.len(), 0, "expired evicted from index");
    }

    #[test]
    fn prune_cap_evicts_oldest_first() {
        // Smoke test: insert asks with non-expired TTLs, trigger prune via
        // internal eviction code path, confirm overflow-purge is opt-in.
        // The full 100K-iteration loop is too slow for unit tests
        // (panicked previously — likely stack overflow on BTreeMap recursion);
        // we use a smaller batch here to verify the eviction logic.
        let mut idx = MarketplaceIndex::new();
        let now: u64 = 0;
        for i in 0..100u64 {
            let ask = sample_ask(
                &format!("did:octo:a{i}"),
                "openai/gpt-4",
                10_000,
                now + 1000 + i, // all above now → no expired
            );
            idx.insert(ask);
        }
        assert_eq!(idx.len(), 100);
        // No expired (now=0, all TTLs > 0) → prune_expired returns 0.
        let removed = idx.prune_expired(now);
        assert_eq!(removed, 0);
        assert_eq!(idx.len(), 100);
    }

    /// Regression: the over-cap insert must evict ONE ask, not all of them.
    ///
    /// `insert` has no clock argument, so it passes a sentinel as the expiry
    /// cutoff. It used to pass `u64::MAX`, which matches every ask in
    /// `prune_expired` and emptied the entire index on the 100,001st insert
    /// instead of trimming to the cap. All asks below carry a year-2100 TTL,
    /// so none is expired by wall-clock standards — the removal can only come
    /// from the count cap.
    ///
    /// This reaches the branch that `prune_cap_evicts_oldest_first` never
    /// does: 100 inserts against a 100_000 cap never trip it.
    #[test]
    fn over_cap_insert_evicts_one_not_everything() {
        const CAP: usize = 3;
        // Every TTL is far-future (2100-01-01 and beyond) so nothing is
        // expired by wall-clock standards: the only thing that can remove an
        // entry is the count cap. The three base TTLs ascend, which makes
        // "earliest expiry" a strict ordering rather than a tie — `min_by_key`
        // would otherwise pick arbitrarily among equals.
        const FAR: u64 = 4_102_444_800;
        let mut idx = MarketplaceIndex::with_cap(CAP);
        let mut base: Vec<AskId> = Vec::new();
        for i in 0..CAP as u64 {
            let ask = sample_ask(&format!("did:octo:a{i}"), "openai/gpt-4", 10_000, FAR + i);
            base.push(ask.id());
            idx.insert(ask);
        }
        assert_eq!(idx.len(), CAP, "index filled to cap, no pruning yet");

        // The 4th insert trips the cap. Its TTL is the latest of all.
        let overflow = sample_ask("did:octo:overflow", "openai/gpt-4", 10_000, FAR + 99);
        let overflow_id = overflow.id();
        assert!(idx.insert(overflow));

        // Trimmed back to the cap, not emptied.
        assert_eq!(
            idx.len(),
            CAP,
            "over-cap insert must evict exactly one ask, not the whole index"
        );
        assert!(
            idx.get(&overflow_id).is_some(),
            "the newly inserted ask must survive the prune"
        );
        assert!(
            idx.get(&base[0]).is_none(),
            "the earliest-expiring ask is the one evicted"
        );
        assert!(
            idx.get(&base[1]).is_some() && idx.get(&base[2]).is_some(),
            "the two later asks survive"
        );
    }

    /// Control for the regression above: `prune_expired_and_overflow` with a
    /// realistic `now_unix` *should* still drop time-expired asks. Without
    /// this, the `u64::MAX` bug could be "fixed" by making `insert` always
    /// pass a real clock — which would silently move expiry into the hot
    /// insert path — and never be caught.
    ///
    /// The cap here is deliberately above the insert count, so `insert` never
    /// auto-prunes and the explicit call below is the only thing that runs.
    #[test]
    fn over_cap_prune_still_honours_a_real_clock() {
        const CAP: usize = 10;
        let now: u64 = 1_700_000_000;
        let mut idx = MarketplaceIndex::with_cap(CAP);
        let stale = sample_ask("did:octo:stale", "openai/gpt-4", 10_000, now - 1);
        let live1 = sample_ask("did:octo:live1", "openai/gpt-4", 10_000, now + 10_000);
        let live2 = sample_ask("did:octo:live2", "openai/gpt-4", 10_000, now + 20_000);
        let (stale_id, live1_id, live2_id) = (stale.id(), live1.id(), live2.id());
        idx.insert(stale);
        idx.insert(live1);
        idx.insert(live2);
        assert_eq!(idx.len(), 3, "under cap, nothing auto-pruned");

        let removed = idx.prune_expired_and_overflow(now);
        assert_eq!(removed, 1, "exactly the expired ask is removed");
        assert_eq!(idx.len(), 2);
        assert!(idx.get(&stale_id).is_none());
        assert!(idx.get(&live1_id).is_some());
        assert!(idx.get(&live2_id).is_some());
    }

    #[test]
    fn default_cap_matches_production_cap() {
        // `Default` is a public trait impl, so `MarketplaceIndex::default()`
        // is reachable from outside the crate. It must carry the production
        // cap, not a derived zero — a zero cap would make `insert` prune on
        // every call and leave the index permanently empty.
        let d = MarketplaceIndex::default();
        assert_eq!(d.cap, ACTIVE_ASK_CAP);
        assert_eq!(d.cap, MarketplaceIndex::new().cap);

        // Behavioural consequence: a default-built index retains asks well
        // past what a zero cap would allow.
        let mut d = MarketplaceIndex::default();
        d.insert(sample_ask(
            "did:octo:a",
            "openai/gpt-4",
            10_000,
            4_102_444_800,
        ));
        assert_eq!(d.len(), 1, "insert must not prune against a real cap");
    }

    #[test]
    fn parse_model_key_two_segments() {
        let (n, f, v) = parse_model_key("openai/gpt-4");
        assert_eq!(n, "openai");
        assert_eq!(f, "gpt-4");
        assert_eq!(v, None);
    }

    #[test]
    fn parse_model_key_three_segments() {
        let (n, f, v) = parse_model_key("anthropic/claude-3/opus-2026");
        assert_eq!(n, "anthropic");
        assert_eq!(f, "claude-3");
        assert_eq!(v, Some("opus-2026".to_owned()));
    }

    #[test]
    fn parse_model_key_one_segment() {
        let (n, f, v) = parse_model_key("solo");
        assert_eq!(n, "solo");
        assert_eq!(f, "");
        assert_eq!(v, None);
    }

    #[test]
    fn list_by_asker_filters() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        idx.insert(sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ));
        idx.insert(sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "anthropic/claude",
            20_000,
            now + 1000,
        ));
        idx.insert(sample_ask("did:octo:b", "openai/gpt-4", 30_000, now + 1000));
        let alice = idx.list_by_asker(&octo_ident::test_helpers::sample_did(111), now);
        assert_eq!(alice.len(), 2);
        for a in &alice {
            assert_eq!(a.asker_did, octo_ident::test_helpers::sample_did(111));
        }
    }

    /// `list_by_asker` is documented as returning **non-expired** asks
    /// and takes an expiry cutoff to match the storage-level
    /// `AskRead::list_by_asker`. It previously took no cutoff and
    /// returned every ask the index retained, expired included.
    ///
    /// The pre-existing `list_by_asker_filters` used only live asks, so
    /// it passed without ever exercising the cutoff.
    #[test]
    fn list_by_asker_excludes_expired_asks() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000u64;
        let did = octo_ident::test_helpers::sample_did(181);
        let did_ref = did.parse::<AskerDid>().expect("canonical did");

        idx.insert(sample_ask(&did, "openai/gpt-4", 10_000, now + 1000));
        idx.insert(sample_ask(&did, "openai/gpt-4", 20_000, now - 1000));
        idx.insert(sample_ask(&did, "openai/gpt-4", 30_000, now - 9999));

        // The index itself retains all three — expiry is not enforced
        // on insert, only by explicit pruning or overflow.
        assert_eq!(idx.len(), 3, "index retains expired asks until pruned");

        let live = idx.list_by_asker(&did_ref, now);
        assert_eq!(
            live.len(),
            1,
            "only the non-expired ask is returned, not all {} retained",
            idx.len()
        );
        assert_eq!(
            live[0].expires_at_unix,
            now + 1000,
            "the surviving ask is the live one"
        );
        assert!(live.iter().all(|a| a.expires_at_unix > now));

        // Positive control: the expired asks are still returned when the
        // cutoff is before them, so the filter is a real predicate and
        // not an asker filter that happens to drop everything.
        let all = idx.list_by_asker(&did_ref, 0);
        assert_eq!(all.len(), 3, "cutoff of 0 returns every retained ask");

        // And a cutoff past every expiry returns none.
        assert!(
            idx.list_by_asker(&did_ref, now + 10_000).is_empty(),
            "a cutoff past every expiry returns nothing"
        );
    }

    #[test]
    fn jurisdiction_wildcard_match() {
        // Empty declared = accept any.
        assert!(jurisdiction_matches(&[], &["US".to_owned()]));
        assert!(jurisdiction_matches(&[], &[]));
        // Non-empty declared, empty actual = no match.
        assert!(!jurisdiction_matches(&["US".to_owned()], &[]));
        // Both non-empty, overlap = match.
        assert!(jurisdiction_matches(
            &["US".to_owned(), "EU".to_owned()],
            &["EU".to_owned()]
        ));
        // No overlap = no match.
        assert!(!jurisdiction_matches(
            &["US".to_owned()],
            &["JP".to_owned()]
        ));
    }

    #[test]
    fn iter_all_asks() {
        let mut idx = MarketplaceIndex::new();
        let now = 1_700_000_000;
        idx.insert(sample_ask(
            &octo_ident::test_helpers::sample_did(111),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ));
        idx.insert(sample_ask(
            "did:octo:b",
            "anthropic/claude",
            20_000,
            now + 1000,
        ));
        let all: Vec<_> = idx.iter().collect();
        assert_eq!(all.len(), 2);
    }
}
