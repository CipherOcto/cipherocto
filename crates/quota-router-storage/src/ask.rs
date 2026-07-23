//! Ask + PricingAxis + AskId types (RFC-0959 v1.0 §Data Structures).
//!
//! Ask = a node's published pricing offer. `AskId = BLAKE3(canonical_ser(asker_did || model || axes_hash || nonce))`.
//! PricingAxis registry holds per-axis rate tables keyed by model.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Micro-OCTO-W (u128). 1 OCTO-W = 1_000_000 micro-OCTO-W.
#[allow(non_camel_case_types)]
pub type MicroOCTO_W = u128;

/// Provider model reference (e.g., "openai/gpt-4", "anthropic/claude-3-opus").
pub type ModelRef = String;

/// Asker (publisher) DID.
pub type AskerDid = String;

/// Ask identifier (BLAKE3 32-byte content hash).
pub type AskId = [u8; 32];

/// Pricing axis identifier (e.g., "input_tokens_per_1k", "output_tokens_per_1k", "cached_input_tokens_per_1k").
pub type AxisId = String;

/// Pricing axis — semantically meaningful unit of consumption.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PricingAxis {
    /// Stable axis identifier (RFC-0959 §Data Structures).
    pub id: AxisId,
    /// Human-readable name.
    pub name: String,
    /// Default rate (micro-OCTO-W per 1000 units) when no per-model override exists.
    pub default_rate_per_1k: MicroOCTO_W,
}

impl PricingAxis {
    /// Built-in axes per RFC-0959 §3.3 default registry.
    #[must_use]
    pub fn standard_axes() -> Vec<Self> {
        vec![
            Self {
                id: "input_tokens_per_1k".to_owned(),
                name: "Input tokens per 1K".to_owned(),
                default_rate_per_1k: 30_000, // 0.03 OCTO-W
            },
            Self {
                id: "output_tokens_per_1k".to_owned(),
                name: "Output tokens per 1K".to_owned(),
                default_rate_per_1k: 60_000, // 0.06 OCTO-W
            },
            Self {
                id: "cached_input_tokens_per_1k".to_owned(),
                name: "Cached input tokens per 1K".to_owned(),
                default_rate_per_1k: 3_000, // 0.003 OCTO-W
            },
        ]
    }

    /// Look up axis by id (linear scan; small N).
    #[must_use]
    pub fn find_by_id<'a>(axes: &'a [Self], id: &str) -> Option<&'a Self> {
        axes.iter().find(|a| a.id == id)
    }
}

/// Per-axis rate override for a specific model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxisRate {
    pub axis: AxisId,
    pub rate_per_1k: MicroOCTO_W,
}

/// Per-model rate table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelRateTable {
    /// Model this table applies to (e.g., "openai/gpt-4").
    pub model: ModelRef,
    /// Per-axis rates; falls back to `PricingAxis::default_rate_per_1k` if axis missing.
    pub rates: Vec<AxisRate>,
}

impl ModelRateTable {
    /// Compute cost for a (model, axis_id, units) tuple. Returns `None` if model unknown.
    #[must_use]
    pub fn cost_for(&self, axis_id: &str, units: u64, axes: &[PricingAxis]) -> Option<MicroOCTO_W> {
        let rate = self
            .rates
            .iter()
            .find(|r| r.axis == axis_id)
            .map(|r| r.rate_per_1k)
            .or_else(|| PricingAxis::find_by_id(axes, axis_id).map(|a| a.default_rate_per_1k))?;
        // cost = ceil(units / 1000) * rate_per_1k
        let blocks = units.div_ceil(1000);
        Some(blocks as u128 * rate)
    }
}

/// Published Ask (per-node pricing offer) per RFC-0959 v1.0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    /// Asker (publisher) DID.
    pub asker_did: AskerDid,
    /// Model this Ask covers.
    pub model: ModelRef,
    /// Per-model rate table (axes + rates).
    pub rates: ModelRateTable,
    /// Nonce (random per mint; ensures AskId uniqueness).
    pub nonce: [u8; 16],
    /// Unix timestamp at which this Ask expires (inclusive).
    pub expires_at_unix: u64,
}

impl Ask {
    /// Construct a validated `Ask`. Rejects empty asker_did / model / all-zero nonce.
    /// # Errors
    /// Returns `AskError::EmptyAskerDid` / `EmptyModel` / `EmptyNonce`.
    pub fn new(
        asker_did: impl Into<String>,
        model: impl Into<String>,
        rates: ModelRateTable,
        nonce: [u8; 16],
        expires_at_unix: u64,
    ) -> Result<Self, AskError> {
        let asker_did = asker_did.into();
        let model = model.into();
        if asker_did.is_empty() {
            return Err(AskError::EmptyAskerDid);
        }
        if model.is_empty() {
            return Err(AskError::EmptyModel);
        }
        if nonce == [0u8; 16] {
            return Err(AskError::EmptyNonce);
        }
        Ok(Self {
            asker_did,
            model,
            rates,
            nonce,
            expires_at_unix,
        })
    }

    /// Compute the content-addressable AskId.
    /// `AskId = BLAKE3(canonical_ser(asker_did || model || axes_hash || nonce))`
    /// where `axes_hash = BLAKE3(canonical_ser(rates.rates))`.
    #[must_use]
    pub fn id(&self) -> AskId {
        let rates_canonical = serde_json::to_vec(&self.rates).expect("serializable");
        let axes_hash = blake3::hash(&rates_canonical);
        let mut msg =
            Vec::with_capacity(self.asker_did.len() + self.model.len() + 32 + self.nonce.len());
        msg.extend_from_slice(self.asker_did.as_bytes());
        msg.extend_from_slice(self.model.as_bytes());
        msg.extend_from_slice(axes_hash.as_bytes());
        msg.extend_from_slice(&self.nonce);
        *blake3::hash(&msg).as_bytes()
    }

    /// Axes-hash (intermediate; exposed for tests).
    #[must_use]
    pub fn axes_hash(&self) -> [u8; 32] {
        let rates_canonical = serde_json::to_vec(&self.rates).expect("serializable");
        *blake3::hash(&rates_canonical).as_bytes()
    }
}

/// Ask construction errors.
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    #[error("asker_did is empty")]
    EmptyAskerDid,
    #[error("model is empty")]
    EmptyModel,
    #[error("nonce is all-zeros (use a real CSPRNG-generated 16-byte nonce)")]
    EmptyNonce,
}

/// Per-axis consumption tuple: `(axis_id, units_consumed)`.
pub type AxisConsumption = (AxisId, u64);

/// Compute settlement cost for an `Ask` given per-axis consumption.
#[must_use]
pub fn settlement_cost(
    ask: &Ask,
    consumed: &[AxisConsumption],
    axes: &[PricingAxis],
) -> MicroOCTO_W {
    let mut total: MicroOCTO_W = 0;
    for (axis_id, units) in consumed {
        if let Some(c) = ask.rates.cost_for(axis_id, *units, axes) {
            total += c;
        }
    }
    total
}

/// PricingAxis registry (default 3 axes + custom additions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingAxisRegistry {
    pub axes: Vec<PricingAxis>,
}

impl Default for PricingAxisRegistry {
    fn default() -> Self {
        Self {
            axes: PricingAxis::standard_axes(),
        }
    }
}

impl PricingAxisRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&PricingAxis> {
        PricingAxis::find_by_id(&self.axes, id)
    }

    /// Register a new axis. Returns Err on duplicate id.
    /// # Errors
    /// Returns `AxisRegistryError::Duplicate` if axis id already registered.
    pub fn register(&mut self, axis: PricingAxis) -> Result<(), AxisRegistryError> {
        if self.get(&axis.id).is_some() {
            return Err(AxisRegistryError::Duplicate(axis.id));
        }
        self.axes.push(axis);
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AxisRegistryError {
    #[error("axis `{0}` already registered")]
    Duplicate(String),
}

/// Cache classification by cache_key_hash (RFC-0959 v1.0 §Cache Classification).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheClassification {
    /// BLAKE3 hash of the request body.
    pub cache_key_hash: [u8; 32],
    /// True if this is a cache hit (provider returned cached response).
    pub is_hit: bool,
    /// Time-to-live of this cache entry (seconds).
    pub ttl_secs: u32,
}

/// Compute cache_key_hash for a request body.
#[must_use]
pub fn cache_key_hash(body: &[u8]) -> [u8; 32] {
    *blake3::hash(body).as_bytes()
}

/// Cache policy attached to capability caveats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy")]
pub enum CachePolicy {
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "opt-in")]
    OptIn { cache_key_hash: Option<[u8; 32]> },
    #[serde(rename = "always")]
    Always { ttl_secs: u32 },
}

impl CachePolicy {
    /// Whether this policy permits caching a response with `cache_key_hash`.
    #[must_use]
    pub fn permits(&self, hash: &[u8; 32]) -> bool {
        match self {
            Self::Off => false,
            Self::Always { .. } => true,
            Self::OptIn { cache_key_hash } => cache_key_hash.as_ref() == Some(hash),
        }
    }
}

/// Settlement envelope (RFC-0959 v1.0 canonical wire).
///
/// `envelope = borsh_compat(model || axes_consumed || ask_id || nonce || timestamp || cost_micro_octo_w || settlement_hash)`
/// where `settlement_hash = BLAKE3(canonical_ser(model || axes_consumed || ask_id || nonce || timestamp))`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementEnvelope {
    /// Settlement hash (BLAKE3 32 bytes).
    pub settlement_hash: [u8; 32],
    /// Asker DID.
    pub asker_did: AskerDid,
    /// Holder DID.
    pub holder_did: String,
    /// Resolved Ask model.
    pub model: ModelRef,
    /// Per-axis consumption at settlement time.
    pub axes_consumed: Vec<AxisConsumption>,
    /// AskId.
    pub ask_id: AskId,
    /// Replay-defense nonce.
    pub nonce: [u8; 32],
    /// Settlement timestamp (Unix seconds).
    pub timestamp_unix: u64,
    /// Cost in micro-OCTO-W.
    pub cost: MicroOCTO_W,
}

impl SettlementEnvelope {
    /// Compute settlement hash from canonical fields.
    #[must_use]
    pub fn compute_settlement_hash(&self) -> [u8; 32] {
        let mut msg =
            Vec::with_capacity(self.model.len() + 32 + 32 + self.axes_consumed.len() * 32 + 32);
        msg.extend_from_slice(self.model.as_bytes());
        let axes_canonical = serde_json::to_vec(&self.axes_consumed).expect("serializable");
        msg.extend_from_slice(&axes_canonical);
        msg.extend_from_slice(&self.ask_id);
        msg.extend_from_slice(&self.nonce);
        msg.extend_from_slice(&self.timestamp_unix.to_le_bytes());
        *blake3::hash(&msg).as_bytes()
    }

    /// Verify self-consistent settlement hash + replay-defense index.
    /// # Errors
    /// Returns `SettlementError::HashMismatch` if embedded hash != computed.
    /// Returns `SettlementError::AlreadyConsumed` if `nonce` already in `consumed_index`.
    pub fn verify(&self, consumed_index: &mut ConsumedReceiptIndex) -> Result<(), SettlementError> {
        let computed = self.compute_settlement_hash();
        if computed != self.settlement_hash {
            return Err(SettlementError::HashMismatch);
        }
        if consumed_index.contains(&self.nonce) {
            return Err(SettlementError::AlreadyConsumed);
        }
        consumed_index.insert(self.nonce);
        Ok(())
    }
}

/// Replay-defense index (in-memory; production backed by stoolap).
#[derive(Debug, Default)]
pub struct ConsumedReceiptIndex {
    seen: HashMap<[u8; 32], ()>,
}

impl ConsumedReceiptIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, nonce: &[u8; 32]) -> bool {
        self.seen.contains_key(nonce)
    }

    pub fn insert(&mut self, nonce: [u8; 32]) {
        self.seen.insert(nonce, ());
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// Settlement errors.
#[derive(Debug, thiserror::Error)]
pub enum SettlementError {
    #[error("settlement hash mismatch (envelope tampered or canonicalization drift)")]
    HashMismatch,
    #[error("nonce already consumed (replay attempt)")]
    AlreadyConsumed,
    #[error("axes_consumed exceeds ask max_total (anti-fraud)")]
    AxesExceededMaxTotal,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ask() -> Ask {
        Ask {
            asker_did: "did:octo:asker1".to_owned(),
            model: "openai/gpt-4".to_owned(),
            rates: ModelRateTable {
                model: "openai/gpt-4".to_owned(),
                rates: vec![
                    AxisRate {
                        axis: "input_tokens_per_1k".to_owned(),
                        rate_per_1k: 30_000,
                    },
                    AxisRate {
                        axis: "output_tokens_per_1k".to_owned(),
                        rate_per_1k: 60_000,
                    },
                ],
            },
            nonce: [0x42; 16],
            expires_at_unix: 1_900_000_000,
        }
    }

    #[test]
    fn ask_id_deterministic() {
        let a = sample_ask();
        let b = sample_ask();
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn ask_id_changes_with_nonce() {
        let a = sample_ask();
        let mut b = sample_ask();
        b.nonce[0] = 0x99;
        assert_ne!(a.id(), b.id());
        // Touch a so it's "used"
        let _ = a.id();
    }

    #[test]
    fn settlement_cost_basic() {
        let ask = sample_ask();
        let axes = PricingAxis::standard_axes();
        let consumed = vec![
            ("input_tokens_per_1k".to_owned(), 1000),
            ("output_tokens_per_1k".to_owned(), 500),
        ];
        let cost = settlement_cost(&ask, &consumed, &axes);
        // 1 * 30_000 + 1 * 60_000 = 90_000 (500 rounds up to 1 block)
        assert_eq!(cost, 90_000);
    }

    #[test]
    fn cache_policy_opt_in_only_specific_hash() {
        let h = [0xab; 32];
        let p = CachePolicy::OptIn {
            cache_key_hash: Some(h),
        };
        assert!(p.permits(&h));
        let mut other = h;
        other[0] = 0xff;
        assert!(!p.permits(&other));
        let p_off = CachePolicy::OptIn {
            cache_key_hash: None,
        };
        assert!(!p_off.permits(&h));
    }

    #[test]
    fn cache_policy_always_permits() {
        let p = CachePolicy::Always { ttl_secs: 60 };
        assert!(p.permits(&[0u8; 32]));
    }

    #[test]
    fn settlement_hash_stable() {
        let env = SettlementEnvelope {
            settlement_hash: [0u8; 32],
            asker_did: "did:octo:a".to_owned(),
            holder_did: "did:octo:h".to_owned(),
            model: "openai/gpt-4".to_owned(),
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            ask_id: [1u8; 32],
            nonce: [2u8; 32],
            timestamp_unix: 1_700_000_000,
            cost: 30_000,
        };
        let h = env.compute_settlement_hash();
        let h2 = env.compute_settlement_hash();
        assert_eq!(h, h2);
    }

    #[test]
    fn settlement_verify_rejects_hash_mismatch() {
        let mut env = SettlementEnvelope {
            settlement_hash: [0xab; 32],
            asker_did: "did:octo:a".to_owned(),
            holder_did: "did:octo:h".to_owned(),
            model: "openai/gpt-4".to_owned(),
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            ask_id: [1u8; 32],
            nonce: [2u8; 32],
            timestamp_unix: 1_700_000_000,
            cost: 30_000,
        };
        let mut idx = ConsumedReceiptIndex::new();
        // Verify self-consistent first.
        env.settlement_hash = env.compute_settlement_hash();
        env.verify(&mut idx).unwrap();
        // Tamper with axes_consumed (affects settlement_hash).
        env.axes_consumed[0].1 = 999;
        let err = env.verify(&mut idx).unwrap_err();
        assert!(matches!(err, SettlementError::HashMismatch));
    }

    #[test]
    fn settlement_replay_defense() {
        let mut env = SettlementEnvelope {
            settlement_hash: [0u8; 32],
            asker_did: "did:octo:a".to_owned(),
            holder_did: "did:octo:h".to_owned(),
            model: "openai/gpt-4".to_owned(),
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            ask_id: [1u8; 32],
            nonce: [2u8; 32],
            timestamp_unix: 1_700_000_000,
            cost: 30_000,
        };
        env.settlement_hash = env.compute_settlement_hash();
        let mut idx = ConsumedReceiptIndex::new();
        env.verify(&mut idx).unwrap();
        let err = env.verify(&mut idx).unwrap_err();
        assert!(matches!(err, SettlementError::AlreadyConsumed));
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn axis_registry_register_and_get() {
        let mut reg = PricingAxisRegistry::new();
        assert_eq!(reg.axes.len(), 3);
        assert!(reg.get("input_tokens_per_1k").is_some());
        let err = reg.register(PricingAxis {
            id: "input_tokens_per_1k".to_owned(),
            name: "dup".to_owned(),
            default_rate_per_1k: 0,
        });
        assert!(matches!(err, Err(AxisRegistryError::Duplicate(_))));
    }

    #[test]
    fn axes_consumed_sorted_in_settlement_hash() {
        // Same axes in different order → same hash (axes_consumed serialized as JSON Vec;
        // order matters in serde_json::to_vec). Documented limitation: consumers
        // should sort before constructing envelope.
        let mut env1 = SettlementEnvelope {
            settlement_hash: [0u8; 32],
            asker_did: "a".to_owned(),
            holder_did: "h".to_owned(),
            model: "m".to_owned(),
            axes_consumed: vec![("a_per_1k".to_owned(), 1), ("b_per_1k".to_owned(), 2)],
            ask_id: [0u8; 32],
            nonce: [0u8; 32],
            timestamp_unix: 0,
            cost: 0,
        };
        let mut env2 = env1.clone();
        env2.axes_consumed = vec![("b_per_1k".to_owned(), 2), ("a_per_1k".to_owned(), 1)];
        env1.settlement_hash = env1.compute_settlement_hash();
        env2.settlement_hash = env2.compute_settlement_hash();
        // Different order → different hash (current contract).
        assert_ne!(env1.settlement_hash, env2.settlement_hash);
    }
}
