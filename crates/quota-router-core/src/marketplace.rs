//! Marketplace — cheapest matching Ask lookup (S04 Step 3 + Phase C).
//!
//! Backed by `octo_core::ask_repo::AskRepository` (cipherocto-side persistence
//! per Phase C). In-memory `Marketplace` kept as a legacy stub for tests that
//! don't want the stoolap dependency.

use serde::{Deserialize, Serialize};

/// Marketplace entry (serialization shape for clients).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub ask_id: [u8; 32],
    pub asker_did: String,
    pub model: String,
    pub cost_per_1k: u128,
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

/// Marketplace backed by cipherocto-side `AskRepository` (Phase C).
///
/// `cheapest()` delegates to `AskRepository::cheapest()` which queries the
/// `asks` table with `idx_model` index + filters expired entries.
///
/// Conversion: `Ask` (full content) → `MarketplaceEntry` (compact wire shape).
/// Cost computation is done in `AskRepository`; here we just snapshot
/// the canonical "asker_did, model, ask_id, cost_per_1k".
pub struct Marketplace {
    repo: AskRepository,
    axes: Vec<PricingAxis>,
}

impl Marketplace {
    /// Open a marketplace backed by an in-memory AskRepository (test/dev).
    /// # Errors
    /// Returns `RepoError` on stoolap open / migration failure.
    pub fn open_in_memory() -> Result<Self, RepoError> {
        Ok(Self {
            repo: AskRepository::open_in_memory()?,
            axes: PricingAxis::standard_axes(),
        })
    }

    /// Open a marketplace backed by a file-backed AskRepository (production).
    /// # Errors
    /// Returns `RepoError` on open / migration failure.
    pub fn open_path(path: &str) -> Result<Self, RepoError> {
        Ok(Self {
            repo: AskRepository::open_path(path)?,
            axes: PricingAxis::standard_axes(),
        })
    }

    /// Wrap an existing AskRepository.
    #[must_use]
    pub fn from_repo(repo: AskRepository) -> Self {
        Self {
            repo,
            axes: PricingAxis::standard_axes(),
        }
    }

    /// Override the pricing-axis set (defaults to `PricingAxis::standard_axes()`).
    #[must_use]
    pub fn with_axes(mut self, axes: Vec<PricingAxis>) -> Self {
        self.axes = axes;
        self
    }

    /// Insert (or replace) an Ask in the underlying repository.
    /// # Errors
    /// Returns `RepoError` on stoolap failure.
    pub fn put(&self, ask: &Ask) -> Result<(), RepoError> {
        self.repo.put(ask)
    }

    /// Find the cheapest Ask matching `model`.
    /// # Errors
    /// Returns `RepoError` on stoolap failure or deserialization.
    pub fn cheapest(&self, model: &str) -> Result<Option<MarketplaceEntry>, RepoError> {
        let now = current_unix();
        let ask = match self.repo.cheapest(model, now, &self.axes)? {
            None => return Ok(None),
            Some(a) => a,
        };
        // Compute cost proxy: 1 unit per known axis.
        let consumed = build_unit_consumed(&ask, &self.axes);
        let cost = settlement_cost(&ask, &consumed, &self.axes);
        Ok(Some(MarketplaceEntry {
            ask_id: compute_ask_id_static(&ask),
            asker_did: ask.asker_did.clone(),
            model: ask.model.clone(),
            cost_per_1k: cost,
        }))
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
    let mut msg = Vec::with_capacity(ask.asker_did.len() + ask.model.len() + 32 + ask.nonce.len());
    msg.extend_from_slice(ask.asker_did.as_bytes());
    msg.extend_from_slice(ask.model.as_bytes());
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
    pub fn attach_org_policy(
        &self,
        policy_id: [u8; 32],
        policy_version: u64,
    ) -> PolicyAttachment {
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
    use quota_router_storage::ask::{AxisRate, ModelRateTable};

    fn sample_ask(asker: &str, model: &str, rate: u128, expires: u64) -> Ask {
        Ask {
            asker_did: asker.to_owned(),
            model: model.to_owned(),
            rates: ModelRateTable {
                model: model.to_owned(),
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
        });
        m.insert(MarketplaceEntry {
            ask_id: [2; 32],
            asker_did: "b".into(),
            model: "gpt-4".into(),
            cost_per_1k: 25_000,
        });
        m.insert(MarketplaceEntry {
            ask_id: [3; 32],
            asker_did: "c".into(),
            model: "other".into(),
            cost_per_1k: 1,
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
            "did:octo:a",
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
            "did:octo:c",
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();
        let cheapest = m.cheapest("openai/gpt-4").unwrap().expect("cheapest");
        assert_eq!(cheapest.asker_did, "did:octo:c");
        assert_eq!(cheapest.cost_per_1k, 10_000);
    }

    #[test]
    fn ask_repo_backed_cheapest_unknown_model_returns_none() {
        let m = Marketplace::open_in_memory().unwrap();
        assert!(m.cheapest("nonexistent").unwrap().is_none());
    }

    #[test]
    fn ask_repo_backed_excludes_expired() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        // Expired (cheap) Ask must NOT be returned even if cheaper.
        m.put(&sample_ask(
            "did:octo:cheap",
            "openai/gpt-4",
            1_000,
            now - 100,
        ))
        .unwrap();
        m.put(&sample_ask(
            "did:octo:active",
            "openai/gpt-4",
            50_000,
            now + 1000,
        ))
        .unwrap();
        let cheapest = m.cheapest("openai/gpt-4").unwrap().expect("cheapest");
        assert_eq!(cheapest.asker_did, "did:octo:active");
        assert_eq!(cheapest.cost_per_1k, 50_000);
    }

    #[test]
    fn ask_repo_backed_list_by_asker() {
        let m = Marketplace::open_in_memory().unwrap();
        let now = current_unix();
        m.put(&sample_ask(
            "did:octo:a",
            "openai/gpt-4",
            10_000,
            now + 1000,
        ))
        .unwrap();
        m.put(&sample_ask(
            "did:octo:a",
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
        let alice_asks = m.list_by_asker("did:octo:a").unwrap();
        assert_eq!(alice_asks.len(), 2);
        let bob_asks = m.list_by_asker("did:octo:b").unwrap();
        assert_eq!(bob_asks.len(), 1);
        let none = m.list_by_asker("did:octo:nonexistent").unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn attach_org_policy_returns_attachment() {
        let m = Marketplace::open_in_memory().unwrap();
        let attachment = m.attach_org_policy([0xab; 32], 1);
        assert_eq!(attachment.policy_id, [0xab; 32]);
        assert_eq!(attachment.policy_version, 1);
    }
}
