//! Marketplace — cheapest matching Ask lookup (S04 Step 3).

use serde::{Deserialize, Serialize};

/// Marketplace lookup query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceQuery {
    pub model: String,
    pub min_capacity_per_min: u32,
}

/// Ask entry in marketplace index (RFC-0959 v1.0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub ask_id: [u8; 32],
    pub asker_did: String,
    pub model: String,
    pub cost_per_1k: u128,
}

/// Marketplace index (in-memory; production backed by stoolap).
#[derive(Debug, Default)]
pub struct Marketplace {
    entries: Vec<MarketplaceEntry>,
}

impl Marketplace {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cheapest_returns_min_cost() {
        let mut m = Marketplace::new();
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
    fn empty_marketplace_returns_none() {
        let m = Marketplace::new();
        assert!(m.cheapest("gpt-4").is_none());
    }
}
