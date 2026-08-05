//! Marketplace — cheapest matching Ask lookup (S04 Step 3 + Phase C).
//!
//! Backed by `octo_core::ask_repo::AskRepository` (cipherocto-side persistence
//! per Phase C). In-memory `Marketplace` kept as a legacy stub for tests that
//! don't want the stoolap dependency.
//!
//! Submodules (RFC-0900):
//! - `orderbook` — price-time priority order book (Gap 5.1).
//! - `escrow`    — escrow state machine (Gap 5.2).
//! - `slashing`  — provider slashing model (Gap 5.3).

pub mod escrow;
pub mod orderbook;
pub mod reputation_compat;
pub mod scoring;
pub mod slashing;

use serde::{Deserialize, Serialize};

/// Marketplace entry (serialization shape for clients).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub ask_id: [u8; 32],
    pub asker_did: String,
    pub model: String,
    pub cost_per_1k: u128,
    /// Observed latency (EWMA in ms) when reputation observations
    /// exist. `None` for unknown providers (no recorded outcomes) or
    /// when the latency-aware ranking was not requested.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency_ms: Option<u64>,
    /// Reputation Score 0-100 derived from the persisted RFC-0968
    /// `score_ewma` at read time (RFC-0968-A1 §22, amendment 30,
    /// 0968-b Phase C). `None` when the consumer has not asked the
    /// market to populate reputation (legacy listings omit the field).
    /// `Some(100)` for unknown providers (legacy "unknown = perfect"
    /// semantics; a non-zero presentation keeps the field populated).
    /// Mission 0010-b Phase G (S8): wired through `cheapest` /
    /// `list_by_asker` so listing UIs can render the value.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reputation_score_0_100: Option<u8>,
}

// ============================================================================
// In-memory legacy stub
// ============================================================================

/// In-memory marketplace index (legacy; production uses AskRepository-backed).
#[derive(Debug, Default)]
pub struct InMemoryMarketplace {
    entries: Vec<MarketplaceEntry>,
}

impl InMemoryMarketplace {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, entry: MarketplaceEntry) {
        self.entries.push(entry);
    }

    /// Find the cheapest entry matching `model`.
    #[must_use]
    pub fn cheapest(&self, model: &str) -> Option<&MarketplaceEntry> {
        self.entries
            .iter()
            .filter(|e| e.model == model)
            .min_by_key(|e| e.cost_per_1k)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ============================================================================
// AskRepository-backed marketplace (production)
// ============================================================================

use quota_router_storage::ask::{settlement_cost, Ask, PricingAxis};
use quota_router_storage::ask_repo::{AskRepository, RepoError};

pub use scoring::{LatencyRanking, ProviderScore};

/// Marketplace backed by cipherocto-side `AskRepository` (Phase C).
///
/// `cheapest()` reads through an in-memory `OrderBook<AskSpec>` that is
/// populated write-through by `put()`. The orderbook uses price-time
/// priority (RFC-0900 §Order Book) so cheapest lookup is O(log N) and
/// avoids the previous linear BTreeMap scan. The `AskRepository` remains
/// the canonical persistence layer (Gap 7 still queries it via
/// `list_by_asker`).
///
/// Conversion: `Ask` (full content) → `AskSpec` (compact order payload)
/// → `MarketplaceEntry` (wire shape). Cost computation uses the order's
/// `price` (= cost_per_1k) directly; the full `Ask` cost path is still
/// available via the storage layer.
///
/// Gap 7 extensions:
///
/// - `reputation` — provider-scoring registry with circuit-breaker
///   (RFC-0900 §Reputation System). When the registry's
///   `min_reputation` is positive, providers whose `success_rate` falls
///   below the threshold are skipped by `cheapest()`. When
///   `min_reputation <= 0.0` (the default), the registry is a passive
///   observer and `cheapest()` preserves pre-Gap-7 behavior.
/// - `cheapest_with_ranking(model, ranking)` — latency-aware ranking
///   (RFC-0900 §Market Operations). Scans all matching asks and returns
///   the best by a min-max-normalized weighted blend of price and
///   observed latency.
pub struct Marketplace {
    repo: AskRepository,
    axes: Vec<PricingAxis>,
    book: parking_lot::Mutex<orderbook::OrderBook<AskSpec>>,
    reputation: scoring::ProviderReputationRegistry,
}

/// Order payload carried by `Marketplace`'s internal order book.
///
/// One `AskSpec` per published Ask; the order's `price` field carries
/// the cost-per-1k-token settled against the marketplace's pricing axes
/// at publish time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskSpec {
    pub ask_id: [u8; 32],
    pub asker_did: String,
    pub model: String,
}

impl Marketplace {
    /// Open a marketplace backed by an in-memory AskRepository (test/dev).
    /// # Errors
    /// Returns `RepoError` on stoolap open / migration failure.
    pub fn open_in_memory() -> Result<Self, RepoError> {
        Ok(Self {
            repo: AskRepository::open_in_memory()?,
            axes: PricingAxis::standard_axes(),
            book: parking_lot::Mutex::new(orderbook::OrderBook::new()),
            reputation: scoring::ProviderReputationRegistry::new(),
        })
    }

    /// Open a marketplace backed by a file-backed AskRepository (production).
    /// # Errors
    /// Returns `RepoError` on open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, RepoError> {
        Ok(Self {
            repo: AskRepository::open_path(path)?,
            axes: PricingAxis::standard_axes(),
            book: parking_lot::Mutex::new(orderbook::OrderBook::new()),
            reputation: scoring::ProviderReputationRegistry::new(),
        })
    }

    /// Wrap an existing AskRepository.
    #[must_use]
    pub fn from_repo(repo: AskRepository) -> Self {
        Self {
            repo,
            axes: PricingAxis::standard_axes(),
            book: parking_lot::Mutex::new(orderbook::OrderBook::new()),
            reputation: scoring::ProviderReputationRegistry::new(),
        }
    }

    /// Override the pricing-axis set (defaults to `PricingAxis::standard_axes()`).
    #[must_use]
    pub fn with_axes(mut self, axes: Vec<PricingAxis>) -> Self {
        self.axes = axes;
        self
    }

    // ========================================================================
    // Gap 7.1 — Reputation registry (RFC-0900 §Reputation System)
    // ========================================================================

    /// Set the circuit-breaker threshold (`success_rate` below which a
    /// provider is excluded from `cheapest()`).
    ///
    /// `0.0` (or any negative value) disables the breaker — every
    /// provider remains eligible. Positive values activate the breaker.
    pub fn set_min_reputation(&self, min: f64) {
        self.reputation.set_min_reputation(min);
    }

    /// Current circuit-breaker threshold.
    #[must_use]
    pub fn min_reputation(&self) -> f64 {
        self.reputation.min_reputation()
    }

    /// Override a provider's reputation (operator / test fixture).
    pub fn set_provider_score(&self, score: ProviderScore) {
        self.reputation.set_score(score);
    }

    /// Read a provider's recorded reputation. `None` for unknown
    /// providers (no observations recorded yet).
    #[must_use]
    pub fn provider_score(&self, asker_did: &str) -> Option<ProviderScore> {
        self.reputation.score(asker_did)
    }

    /// Record a transaction outcome. Updates the EWMA-tracked success
    /// rate + latency for the provider.
    ///
    /// Typical call sites: the inference task market settlement path
    /// (after a successful or failed prompt completion), and the mesh
    /// `node::scorer` when it observes an HTTP completion.
    pub fn record_outcome(&self, asker_did: &str, success: bool, latency_ms: u64) {
        self.reputation.record(asker_did, success, latency_ms);
    }

    /// Insert (or replace) an Ask in the underlying repository AND
    /// the in-memory order book. Expired Asks are still persisted but
    /// not indexed in the order book so `cheapest()` never returns them.
    /// # Errors
    /// Returns `RepoError` on stoolap failure.
    pub fn put(&self, ask: &Ask) -> Result<(), RepoError> {
        self.repo.put(ask)?;
        // Write-through to order book; skip expired.
        let now = current_unix();
        if ask.expires_at_unix > now {
            let consumed = build_unit_consumed(ask, &self.axes);
            let cost = settlement_cost(ask, &consumed, &self.axes);
            let ask_id = compute_ask_id_static(ask);
            let mut book = self.book.lock();
            book.place_ask(
                AskSpec {
                    ask_id,
                    asker_did: ask.asker_did.clone(),
                    model: ask.model.to_wire(),
                },
                cost,
                1, // qty 1 per Ask; RFC-0900 markets trade 1 prompt per Ask
                ask.asker_did.clone(),
                now,
            );
        }
        Ok(())
    }

    /// Find the cheapest Ask matching `model` via the in-memory order
    /// book. Returns `None` if no matching ask is active.
    ///
    /// The orderbook is queried in price-time priority. The previous
    /// `Result<Option<...>, RepoError>` signature was dropped: the
    /// order book is in-memory, so there is no I/O failure mode.
    /// Callers needing repository-side errors should still use
    /// `list_by_asker`.
    ///
    /// Circuit-breaker (Gap 7.1): if `min_reputation > 0.0`, asks from
    /// providers whose `success_rate` is below the threshold are
    /// skipped. With `min_reputation <= 0.0` (the default), this is a
    /// pure price-time priority lookup as before. The skip walks the
    /// candidate set so a single excluded provider cannot return `None`
    /// when a non-excluded provider is still in the book.
    ///
    /// This is the price-only specialization of
    /// `cheapest_with_ranking(model, LatencyRanking::cheapest())`.
    #[must_use]
    pub fn cheapest(&self, model: &str) -> Option<MarketplaceEntry> {
        self.cheapest_with_ranking(model, LatencyRanking::cheapest())
    }

    /// Latency-aware ranking (Gap 7.2). Scans every ask matching
    /// `model` and returns the one with the lowest composite score
    /// under `ranking`.
    ///
    /// With `LatencyRanking::cheapest()` the result is identical to
    /// `cheapest(model)`. With `LatencyRanking::prefer_latency()` the
    /// scan blends normalized price + observed latency so a
    /// faster-but-pricier provider can outrank a slower-but-cheaper
    /// one.
    ///
    /// Cost: O(N) over the model's ask set; for the documented
    /// ≤1k-provider target (Gap 5 perf note) this is well under any
    /// per-request budget. If the marketplace grows beyond that,
    /// swap the implementation for a heap-indexed ranking.
    ///
    /// When no latency observations exist for a provider, its
    /// latency contribution defaults to `0` (i.e., the "no data"
    /// provider is treated as the fastest candidate in the min-max
    /// normalization). Operators can avoid this bias by recording at
    /// least one outcome per provider before enabling
    /// `prefer_latency`.
    ///
    /// Circuit-breaker behavior matches `cheapest()`: providers below
    /// `min_reputation` are excluded before ranking runs.
    #[must_use]
    pub fn cheapest_with_ranking(
        &self,
        model: &str,
        ranking: LatencyRanking,
    ) -> Option<MarketplaceEntry> {
        let book = self.book.lock();

        // Collect candidates matching `model` and not circuit-broken.
        let candidates: Vec<&orderbook::Order<AskSpec>> = book
            .asks_matching(|spec| spec.model == model)
            .into_iter()
            .filter(|o| !self.reputation.is_excluded(&o.spec.asker_did))
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Fast path: price-only ranking is already encoded by the
        // orderbook's price-time priority.
        if ranking.latency_weight == 0.0 {
            let order = candidates.into_iter().next()?;
            return Some(self.entry_from_order(order));
        }

        // Latency-aware path: normalize price + latency across the
        // candidate set, then pick the lowest composite. Ties break
        // by price (cheaper wins).
        let (min_price, max_price) = candidates
            .iter()
            .fold((u128::MAX, u128::MIN), |(lo, hi), o| {
                (lo.min(o.price), hi.max(o.price))
            });
        let latencies: Vec<u64> = candidates
            .iter()
            .map(|o| {
                self.reputation
                    .score(&o.spec.asker_did)
                    .map(|s| s.latency_ms)
                    .unwrap_or(0)
            })
            .collect();
        let (min_lat, max_lat) = latencies
            .iter()
            .copied()
            .fold((u64::MAX, u64::MIN), |(lo, hi), l| (lo.min(l), hi.max(l)));

        let mut best: Option<(f64, &orderbook::Order<AskSpec>)> = None;
        for (order, latency_ms) in candidates.iter().zip(latencies.iter()) {
            let score = ranking.composite(
                order.price,
                *latency_ms,
                min_price,
                max_price,
                min_lat,
                max_lat,
            );
            let dominated = match &best {
                None => true,
                Some((best_score, best_order)) => {
                    score < *best_score || (score == *best_score && order.price < best_order.price)
                }
            };
            if dominated {
                best = Some((score, order));
            }
        }
        best.map(|(_, o)| self.entry_from_order(o))
    }

    fn entry_from_order(&self, order: &orderbook::Order<AskSpec>) -> MarketplaceEntry {
        let provider_score = self.reputation.score(&order.spec.asker_did);
        let latency_ms = provider_score.as_ref().map(|s| s.latency_ms);
        // Mission 0010-b Phase G (S8): populate the 0-100 presentation score
        // from the persisted RFC-0968 score_ewma. The legacy
        // `ProviderScore` legacy shape carries `success_rate: f64` directly
        // (in-memory registry); the RFC-0968 compat will eventually replace
        // this with `compat.reputation_score_0_100(asker_did)`. For now we
        // map `success_rate: f64` (already clamped to [-1, 1]) through the
        // `(score + 1) * 50` presentation formula.
        let reputation_score_0_100 = provider_score.map(|s| {
            let clamped = s.success_rate.clamp(-1.0, 1.0);
            let raw = (clamped + 1.0) * 50.0;
            raw.round().clamp(0.0, 100.0) as u8
        });
        MarketplaceEntry {
            ask_id: order.spec.ask_id,
            asker_did: order.spec.asker_did.clone(),
            model: order.spec.model.clone(),
            cost_per_1k: order.price,
            latency_ms,
            reputation_score_0_100,
        }
    }

    /// List Asks published by a single asker (delegates to AskRepository).
    /// # Errors
    /// Returns `RepoError` on stoolap failure.
    pub fn list_by_asker(&self, asker_did: &str) -> Result<Vec<Ask>, RepoError> {
        self.repo.list_by_asker(asker_did, current_unix())
    }
}

/// Compute AskId for serialization. We re-derive from the Ask fields
/// (asker_did + model + axes_hash + nonce) since `Ask::id()` requires a
/// borrow and we already own the Ask by value here.
fn compute_ask_id_static(ask: &Ask) -> [u8; 32] {
    let axes_hash = ask.axes_hash();
    let model_wire = ask.model.to_wire();
    let mut msg = Vec::with_capacity(ask.asker_did.len() + model_wire.len() + 32 + ask.nonce.len());
    msg.extend_from_slice(ask.asker_did.as_bytes());
    msg.extend_from_slice(model_wire.as_bytes());
    msg.extend_from_slice(&axes_hash);
    msg.extend_from_slice(&ask.nonce);
    *blake3::hash(&msg).as_bytes()
}

/// Build a unit-per-axis consumption list for cost computation.
fn build_unit_consumed(ask: &Ask, axes: &[PricingAxis]) -> Vec<(String, u64)> {
    axes.iter()
        .filter(|a| ask.rates.rates.iter().any(|r| r.axis == a.id))
        .map(|a| (a.id.clone(), 1000))
        .collect()
}

fn current_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Org policy attachment (PR-Q4, W5).
///
/// When minting a capability with an org policy, the policy's constraints
/// are intersected with the cap's own constraints. The resulting capability
/// carries a `PolicyReference` caveat (RFC-0965 §3.9) pointing at the
/// policy's id. At verify time, the verifier fetches the policy and
/// checks `capability ⊆ policy` (RFC-0967 §5 subgraph relation).
///
/// This module exposes the registration helper. The actual mint step
/// happens in `octo-wallet::capability::CapabilityToken::mint` (W1).
pub struct PolicyAttachment {
    pub policy_id: [u8; 32],
    pub policy_version: u64,
}

impl Marketplace {
    /// Register an org policy attachment for a marketplace. Returns the
    /// attachment descriptor that downstream capability mint can use.
    ///
    /// In the full implementation, this stores the policy reference in
    /// the `policy_catalog` table (RFC-0967 §8). For now it's a stub
    /// that returns the attachment descriptor.
    #[must_use]
    pub fn attach_org_policy(&self, policy_id: [u8; 32], policy_version: u64) -> PolicyAttachment {
        PolicyAttachment {
            policy_id,
            policy_version,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use quota_router_storage::ask::{AxisRate, ModelRateTable, ModelRef};

    fn sample_ask(asker: &str, model: &str, rate: u128, expires: u64) -> Ask {
        Ask {
            asker_did: asker.to_owned(),
            model: ModelRef::from(model),
            rates: ModelRateTable {
                model: ModelRef::from(model),
                rates: vec![AxisRate {
                    axis: "input_tokens_per_1k".to_owned(),
                    rate_per_1k: rate,
                }],
            },
            nonce: [0x42; 16],
            expires_at_unix: expires,
        }
    }

    #[test]
    fn in_memory_cheapest_returns_min_cost() {
        let mut m = InMemoryMarketplace::new();
        m.insert(MarketplaceEntry {
            ask_id: [1; 32],
            asker_did: "a".into(),
            model: "gpt-4".into(),
            cost_per_1k: 30_000,
            latency_ms: None,
            reputation_score_0_100: None,
        });
        m.insert(MarketplaceEntry {
            ask_id: [2; 32],
            asker_did: "b".into(),
            model: "gpt-4".into(),
            cost_per_1k: 25_000,
            latency_ms: None,
            reputation_score_0_100: None,
        });
        m.insert(MarketplaceEntry {
            ask_id: [3; 32],
            asker_did: "c".into(),
            model: "other".into(),
            cost_per_1k: 1,
            latency_ms: None,
            reputation_score_0_100: None,
        });
        let cheapest = m.cheapest("gpt-4").unwrap();
        assert_eq!(cheapest.ask_id, [2; 32]);
    }

    #[test]
    fn in_memory_empty_marketplace_returns_none() {
        let m = InMemoryMarketplace::new();
        assert!(m.cheapest("gpt-4").is_none());
    }

    #[test]
    fn ask_repo_backed_cheapest_returns_lowest() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(164),
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            "did:octo:b",
            "openai/gpt-4",
            20_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(62),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();
        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(cheapest.asker_did, octo_ident::test_helpers::sample_did(62));
        assert_eq!(cheapest.cost_per_1k, 10_000);
    }

    #[test]
    fn ask_repo_backed_cheapest_unknown_model_returns_none() {
        let m = Marketplace::open_in_memory().unwrap();
        assert!(m.cheapest("nonexistent").is_none());
    }

    #[test]
    fn ask_repo_backed_excludes_expired() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        // Expired (cheap) Ask must NOT be returned even if cheaper.
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(166),
            "openai/gpt-4",
            1_000,
            now - 100,
        ))
        .unwrap();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(106),
            "openai/gpt-4",
            50_000,
            now + 1000,
        ))
        .unwrap();
        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(
            cheapest.asker_did,
            octo_ident::test_helpers::sample_did(106)
        );
        assert_eq!(cheapest.cost_per_1k, 50_000);
    }

    #[test]
    fn ask_repo_backed_list_by_asker() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(164),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(164),
            "anthropic/claude",
            20_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            "did:octo:b",
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();
        let alice_asks = m
            .list_by_asker(&octo_ident::test_helpers::sample_did(164))
            .unwrap();
        assert_eq!(alice_asks.len(), 2);
        let bob_asks = m.list_by_asker("did:octo:b").unwrap();
        assert_eq!(bob_asks.len(), 1);
        let none = m
            .list_by_asker(&octo_ident::test_helpers::sample_did(9))
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn attach_org_policy_returns_attachment() {
        let m = Marketplace::open_in_memory().unwrap();
        let attachment = m.attach_org_policy([0xab; 32], 1);
        assert_eq!(attachment.policy_id, [0xab; 32]);
        assert_eq!(attachment.policy_version, 1);
    }

    #[test]
    fn put_indexes_active_ask_in_orderbook() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(164),
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();
        // The orderbook should now hold 1 active ask matching the model.
        let (ask_count, best_asker) = {
            let book = m.book.lock();
            let best = book
                .best_ask_matching(|spec| spec.model == "openai/gpt-4")
                .expect("matching ask");
            (book.ask_count(), (best.spec.asker_did.clone(), best.price))
        };
        assert_eq!(ask_count, 1);
        assert_eq!(best_asker.0, octo_ident::test_helpers::sample_did(164));
        assert_eq!(best_asker.1, 30_000);
    }

    #[test]
    fn cheapest_via_orderbook_skips_expired() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        // Cheap but expired → should NOT be in the orderbook.
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(204),
            "openai/gpt-4",
            1_000,
            now - 1,
        ))
        .unwrap();
        // Active but expensive → in the orderbook.
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(106),
            "openai/gpt-4",
            50_000,
            now + 1000,
        ))
        .unwrap();
        // Only the active one is indexed.
        {
            let book = m.book.lock();
            assert_eq!(book.ask_count(), 1);
        }
        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(
            cheapest.asker_did,
            octo_ident::test_helpers::sample_did(106)
        );
    }

    #[test]
    fn cheapest_returns_none_when_orderbook_empty_for_model() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(164),
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();
        // Different model has no matching ask.
        assert!(m.cheapest("anthropic/claude").is_none());
    }

    // ========================================================================
    // Gap 7.1 — Provider scoring circuit-breaker (RFC-0900)
    // ========================================================================

    use crate::marketplace::scoring::ProviderScore;

    fn score(asker: &str, success_rate: f64, latency_ms: u64, samples: u64) -> ProviderScore {
        ProviderScore {
            asker_did: asker.to_owned(),
            success_rate,
            latency_ms,
            samples,
        }
    }

    #[test]
    fn cheapest_excludes_provider_below_reputation_threshold() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(35),
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(79),
            "openai/gpt-4",
            10_000, // cheaper but excluded
            now + 1000,
        ))
        .unwrap();

        // Configure circuit-breaker at 0.5 success_rate.
        m.set_min_reputation(0.5);
        m.set_provider_score(score(&octo_ident::test_helpers::sample_did(79), 0.3, 0, 10));

        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(
            cheapest.asker_did,
            octo_ident::test_helpers::sample_did(35),
            "provider below reputation threshold must be excluded"
        );
        assert_eq!(cheapest.cost_per_1k, 30_000);
    }

    #[test]
    fn cheapest_includes_provider_at_or_above_threshold() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(191),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(35),
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();

        m.set_min_reputation(0.5);
        m.set_provider_score(score(
            &octo_ident::test_helpers::sample_did(191),
            0.5,
            0,
            10,
        )); // exactly threshold

        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(
            cheapest.asker_did,
            octo_ident::test_helpers::sample_did(191)
        );
    }

    #[test]
    fn cheapest_no_filter_when_threshold_zero() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(79),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(35),
            "openai/gpt-4",
            30_000,
            now + 1000,
        ))
        .unwrap();
        // No min_reputation set; default is 0.0 → no filtering.
        m.set_provider_score(score(&octo_ident::test_helpers::sample_did(79), 0.0, 0, 10));

        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(
            cheapest.asker_did,
            octo_ident::test_helpers::sample_did(79),
            "with threshold=0.0, no providers are excluded"
        );
    }

    #[test]
    fn cheapest_unknown_provider_not_excluded() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            &octo_ident::test_helpers::sample_did(69),
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();

        m.set_min_reputation(0.5);
        // No score registered for `did:octo:unknown` → treat as max reputation.

        let cheapest = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(cheapest.asker_did, octo_ident::test_helpers::sample_did(69));
    }

    #[test]
    fn record_outcome_updates_ewma_success_rate() {
        let m = Marketplace::open_in_memory().unwrap();
        m.record_outcome(&octo_ident::test_helpers::sample_did(181), true, 100);
        m.record_outcome(&octo_ident::test_helpers::sample_did(181), true, 100);
        let s = m
            .provider_score(&octo_ident::test_helpers::sample_did(181))
            .expect("score registered after two observations");
        assert!(s.success_rate > 0.9, "success_rate={}", s.success_rate);
        assert_eq!(s.latency_ms, 100);
        assert_eq!(s.samples, 2);

        m.record_outcome(&octo_ident::test_helpers::sample_did(181), false, 100);
        let s = m
            .provider_score(&octo_ident::test_helpers::sample_did(181))
            .unwrap();
        assert!(
            s.success_rate < 0.9 && s.success_rate > 0.0,
            "EWMA should decay: success_rate={}",
            s.success_rate
        );
        assert_eq!(s.samples, 3);
    }

    // ========================================================================
    // Gap 7.2 — Latency-aware ranking (RFC-0900 §Market Operations)
    // ========================================================================

    use crate::marketplace::scoring::LatencyRanking;

    fn put_ask_with_latency(
        m: &Marketplace,
        asker: &str,
        model: &str,
        rate: u128,
        expires: u64,
        latency_ms: u64,
    ) {
        m.put(&sample_ask(asker, model, rate, expires)).unwrap();
        // Seed latency directly on the registry. EWMA on a single
        // observation converges to the observed value.
        m.set_provider_score(score(asker, 1.0, latency_ms, 1));
    }

    #[test]
    fn cheapest_with_ranking_prefer_latency_picks_fastest() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        // Cheaper-but-slow (10k, 5s).
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(166),
            "openai/gpt-4",
            10_000,
            now + 1000,
            5_000,
        );
        // Mid-price + fast (30k, 100ms) — should win under prefer_latency.
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(232),
            "openai/gpt-4",
            30_000,
            now + 1000,
            100,
        );
        // Slower + more expensive (50k, 4s).
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(110),
            "openai/gpt-4",
            50_000,
            now + 1000,
            4_000,
        );

        let best = m
            .cheapest_with_ranking("openai/gpt-4", LatencyRanking::prefer_latency())
            .expect("a non-excluded provider must exist");
        assert_eq!(
            best.asker_did,
            octo_ident::test_helpers::sample_did(232),
            "lower latency must beat cheaper price when prefer_latency=true"
        );
        // Latency must surface on the entry.
        assert_eq!(best.latency_ms, Some(100));
    }

    #[test]
    fn cheapest_default_still_picks_cheapest_price() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(166),
            "openai/gpt-4",
            10_000,
            now + 1000,
            5_000,
        );
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(232),
            "openai/gpt-4",
            30_000,
            now + 1000,
            100,
        );

        let best = m.cheapest("openai/gpt-4").expect("cheapest");
        assert_eq!(
            best.asker_did,
            octo_ident::test_helpers::sample_did(166),
            "default cheapest() must preserve price-only ranking"
        );
    }

    #[test]
    fn cheapest_with_ranking_returns_none_when_all_excluded() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(164),
            "openai/gpt-4",
            10_000,
            now + 1000,
            100,
        );
        put_ask_with_latency(&m, "did:octo:b", "openai/gpt-4", 20_000, now + 1000, 200);

        // Override both providers with bad scores so they fall below
        // the threshold.
        m.set_provider_score(score(
            &octo_ident::test_helpers::sample_did(164),
            0.1,
            100,
            10,
        ));
        m.set_provider_score(score("did:octo:b", 0.2, 200, 10));
        m.set_min_reputation(0.5);

        let best = m.cheapest_with_ranking("openai/gpt-4", LatencyRanking::prefer_latency());
        assert!(best.is_none(), "all providers excluded → None");
    }

    #[test]
    fn cheapest_with_ranking_skips_circuit_broken_in_prefer_latency() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        // Cheapest ask from a bad-rep provider should be skipped even
        // under latency-aware ranking.
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(79),
            "openai/gpt-4",
            10_000,
            now + 1000,
            50,
        );
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(35),
            "openai/gpt-4",
            30_000,
            now + 1000,
            100,
        );

        m.set_min_reputation(0.5);
        m.set_provider_score(score(
            &octo_ident::test_helpers::sample_did(79),
            0.1,
            50,
            10,
        ));

        let best = m
            .cheapest_with_ranking("openai/gpt-4", LatencyRanking::prefer_latency())
            .expect("good provider remains eligible");
        assert_eq!(best.asker_did, octo_ident::test_helpers::sample_did(35));
    }

    #[test]
    fn ranking_cheapest_pure_price_path_ignores_latency() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(166),
            "openai/gpt-4",
            10_000,
            now + 1000,
            5_000,
        );
        put_ask_with_latency(
            &m,
            &octo_ident::test_helpers::sample_did(232),
            "openai/gpt-4",
            30_000,
            now + 1000,
            100,
        );

        let best = m
            .cheapest_with_ranking("openai/gpt-4", LatencyRanking::cheapest())
            .expect("cheapest");
        assert_eq!(
            best.asker_did,
            octo_ident::test_helpers::sample_did(166),
            "with price-only ranking, the cheapest ask wins regardless of latency"
        );
    }

    #[test]
    fn ranking_handles_degenerate_single_candidate() {
        let r = LatencyRanking::prefer_latency();
        // Single candidate → range collapses to 0 on both axes →
        // composite pinned at 0.
        let score = r.composite(50, 100, 50, 50, 100, 100);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn ranking_composite_normalizes_axes() {
        let r = LatencyRanking::prefer_latency();
        // Cheaper-but-slow vs expensive-but-fast.
        // price range [10, 100]; latency range [50, 200].
        // slow: price_norm = (10-10)/(100-10) = 0, latency_norm = (200-50)/(200-50) = 1
        //       → 0*0.3 + 1*0.7 = 0.7
        // fast: price_norm = (100-10)/(100-10) = 1, latency_norm = (50-50)/(200-50) = 0
        //       → 1*0.3 + 0*0.7 = 0.3
        let slow = r.composite(10, 200, 10, 100, 50, 200);
        let fast = r.composite(100, 50, 10, 100, 50, 200);
        assert!(slow > fast, "slow={slow} fast={fast}");
        assert!((slow - 0.7).abs() < 1e-9);
        assert!((fast - 0.3).abs() < 1e-9);
    }

    #[test]
    fn ranking_composite_cheapest_cancels_latency_axis() {
        let r = LatencyRanking::cheapest();
        // Same price (100 = min), different latency → identical composite.
        let a = r.composite(100, 50, 100, 200, 50, 200);
        let b = r.composite(100, 200, 100, 200, 50, 200);
        assert_eq!(a, b, "price-only ranking must ignore latency");
    }
}
