// RFC-0910: Pricing Table Registry
// Canonical implementation of deterministic pricing tables and tokenizer registry.
// Feeds into RFC-0909 event_id computation and RFC-0904 cost tracking.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// =============================================================================
// PricingTable (fields 1-8 per DCS Entry 16 for compute_pricing_hash)
// =============================================================================

/// Pricing table for a specific provider/model combination.
/// Uses BTreeMap for deterministic field ordering (RFC-0126 compliance).
///
/// **Field ordering (1-8):** This struct has exactly 8 fields. Adding a 9th field
/// would break `compute_pricing_hash` determinism. For optional data like
/// `tokenizer_version_expiry`, use `metadata` (field 8) with a key like
/// `"tokenizer_version_expiry"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTable {
    /// Unique identifier for this table (e.g., "openai-gpt4-v3")
    pub table_id: String,
    /// Version number (increments per provider/model)
    pub version: u32,
    /// Provider name (e.g., "openai")
    pub provider: String,
    /// Model name (e.g., "gpt-4")
    pub model: String,
    /// Price per 1K prompt tokens (in deterministic micro-units)
    pub prompt_cost_per_1k: u64,
    /// Price per 1K completion tokens (in deterministic micro-units)
    pub completion_cost_per_1k: u64,
    /// Timestamp when this pricing becomes effective (Unix epoch).
    pub effective_from: i64,
    /// Additional metadata (reserved for future use).
    /// Key `tokenizer_version_expiry` (i64, Unix epoch) MAY be stored here.
    pub metadata: BTreeMap<String, String>,
}

impl PricingTable {
    /// Compute deterministic SHA256 hash of the pricing table.
    ///
    /// **Merkle leaf requirement:** RFC-0126 §JSON Allowed Contexts explicitly forbids JSON
    /// serialization for Merkle tree leaves. Since `pricing_hash` is used in `event_id` (a Merkle
    /// leaf input per RFC-0909 §Event Identity), this function MUST use DCS (Entry 16, Part 3)
    /// binary encoding — NOT JSON serialization.
    ///
    /// **DCS Entry 16 struct serialization:**
    /// - Fields serialized in **declaration order** (field_id 1-8)
    /// - Each field: `u32_be(field_id) || value_bytes`
    /// - String value: `u32_be(byte_length) || UTF-8 bytes` (no quotes)
    /// - Integer values: binary big-endian (u32_be for u32, u64_be for u64, i64_be for i64)
    /// - BTreeMap: `u32_be(count) || for each (key, value) in sorted order: serialize_string(key) || serialize_string(value)`
    pub fn compute_pricing_hash(&self) -> [u8; 32] {
        let mut buf = Vec::new();

        // Field 1: table_id (String)
        buf.extend_from_slice(&1u32.to_be_bytes());
        let table_id_bytes = self.table_id.as_bytes();
        buf.extend_from_slice(&(table_id_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(table_id_bytes);

        // Field 2: version (u32)
        buf.extend_from_slice(&2u32.to_be_bytes());
        buf.extend_from_slice(&self.version.to_be_bytes());

        // Field 3: provider (String)
        buf.extend_from_slice(&3u32.to_be_bytes());
        let provider_bytes = self.provider.as_bytes();
        buf.extend_from_slice(&(provider_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(provider_bytes);

        // Field 4: model (String)
        buf.extend_from_slice(&4u32.to_be_bytes());
        let model_bytes = self.model.as_bytes();
        buf.extend_from_slice(&(model_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(model_bytes);

        // Field 5: prompt_cost_per_1k (u64)
        buf.extend_from_slice(&5u32.to_be_bytes());
        buf.extend_from_slice(&self.prompt_cost_per_1k.to_be_bytes());

        // Field 6: completion_cost_per_1k (u64)
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(&self.completion_cost_per_1k.to_be_bytes());

        // Field 7: effective_from (i64)
        buf.extend_from_slice(&7u32.to_be_bytes());
        buf.extend_from_slice(&self.effective_from.to_be_bytes());

        // Field 8: metadata (BTreeMap<String, String>)
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(&(self.metadata.len() as u32).to_be_bytes());
        for (key, value) in &self.metadata {
            let key_bytes = key.as_bytes();
            let value_bytes = value.as_bytes();
            buf.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&(value_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(value_bytes);
        }

        let mut hasher = Sha256::new();
        hasher.update(&buf);
        hasher.finalize().into()
    }
}

// =============================================================================
// RegistryError
// =============================================================================

/// Registry operation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateVersion {
        provider: String,
        model: String,
        version: u32,
    },
    VersionNotIncrement {
        provider: String,
        model: String,
        existing_version: u32,
        attempted_version: u32,
    },
    EffectiveFromNotIncrement {
        provider: String,
        model: String,
        existing_effective_from: i64,
        attempted_effective_from: i64,
    },
    TableIdTooLong {
        table_id: String,
        length: usize,
    },
    MetadataTooLarge {
        size: usize,
        max: usize,
    },
    TooManyVersions {
        provider: String,
        model: String,
        current_count: usize,
        max: usize,
    },
}

const MAX_TABLE_ID_LEN: usize = 128;
const MAX_METADATA_SIZE: usize = 4096;
const MAX_VERSIONS_PER_MODEL: usize = 1000;

// =============================================================================
// PricingRegistry
// =============================================================================

/// Global pricing registry using BTreeMap for deterministic iteration.
/// Maps (provider, model) → Vec<PricingTable> (all versions, sorted desc by version).
#[derive(Default)]
pub struct PricingRegistry {
    tables: BTreeMap<(String, String), Vec<PricingTable>>,
    by_hash: HashMap<[u8; 32], Arc<PricingTable>>,
}

impl PricingRegistry {
    /// Register a new pricing table (immutable after registration).
    /// Returns the computed pricing_hash for use in spend events.
    pub fn register(&mut self, table: PricingTable) -> Result<[u8; 32], RegistryError> {
        let table_id_len = table.table_id.len();
        if table_id_len > MAX_TABLE_ID_LEN {
            return Err(RegistryError::TableIdTooLong {
                table_id: table.table_id,
                length: table_id_len,
            });
        }

        let metadata_size = table
            .metadata
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>();
        if metadata_size > MAX_METADATA_SIZE {
            return Err(RegistryError::MetadataTooLarge {
                size: metadata_size,
                max: MAX_METADATA_SIZE,
            });
        }

        let key = (table.provider.clone(), table.model.clone());
        if let Some(entries) = self.tables.get(&key) {
            if entries.len() >= MAX_VERSIONS_PER_MODEL {
                return Err(RegistryError::TooManyVersions {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    current_count: entries.len(),
                    max: MAX_VERSIONS_PER_MODEL,
                });
            }
        }

        let hash = table.compute_pricing_hash();
        let entries = self.tables.entry(key).or_default();

        if let Some(latest) = entries.first() {
            if latest.version == table.version {
                return Err(RegistryError::DuplicateVersion {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    version: table.version,
                });
            }
            if table.version < latest.version {
                return Err(RegistryError::VersionNotIncrement {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    existing_version: latest.version,
                    attempted_version: table.version,
                });
            }
            if table.effective_from < latest.effective_from {
                return Err(RegistryError::EffectiveFromNotIncrement {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    existing_effective_from: latest.effective_from,
                    attempted_effective_from: table.effective_from,
                });
            }
        }

        entries.push(table);
        entries.sort_by(|a, b| b.version.cmp(&a.version));
        self.by_hash.insert(hash, Arc::new(entries[0].clone()));
        Ok(hash)
    }

    /// Get the active (latest version) pricing for a provider/model.
    pub fn get(&self, provider: &str, model: &str) -> Option<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .and_then(|v| v.first())
    }

    /// Get pricing by exact pricing_hash for verification.
    pub fn get_by_hash(&self, hash: &[u8; 32]) -> Option<&PricingTable> {
        self.by_hash.get(hash).map(|arc| &**arc)
    }

    /// Returns all registered versions for a (provider, model) pair, newest first.
    pub fn get_versions(&self, provider: &str, model: &str) -> Vec<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Verify that a provider-reported tokenizer matches the canonical assignment.
    pub fn verify_tokenizer(
        &self,
        _provider: &str,
        model: &str,
        provider_tokenizer: &str,
    ) -> Result<(), (&'static str, String)> {
        let canonical = get_canonical_tokenizer(model);
        if canonical == provider_tokenizer {
            Ok(())
        } else {
            Err((canonical, provider_tokenizer.to_string()))
        }
    }

    /// List all registered (provider, model) pairs.
    pub fn list_models(&self) -> impl Iterator<Item = (&str, &str)> {
        self.tables.keys().map(|(p, m)| (p.as_str(), m.as_str()))
    }
}

// =============================================================================
// CostError
// =============================================================================

/// Error for cost computation overflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostError {
    Overflow {
        prompt_cost: u64,
        completion_cost: u64,
    },
}

// =============================================================================
// compute_cost — canonical per RFC-0910
// =============================================================================

/// Compute cost deterministically using integer arithmetic.
/// Receives `&PricingTable` (RFC-0910 struct).
pub fn compute_cost(
    pricing: &PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<u64, CostError> {
    let prompt_cost = match (input_tokens as u64).checked_mul(pricing.prompt_cost_per_1k) {
        Some(v) => v / 1000,
        None => {
            return Err(CostError::Overflow {
                prompt_cost: u64::MAX,
                completion_cost: 0,
            })
        }
    };
    let completion_cost = match (output_tokens as u64).checked_mul(pricing.completion_cost_per_1k) {
        Some(v) => v / 1000,
        None => {
            return Err(CostError::Overflow {
                prompt_cost: 0,
                completion_cost: u64::MAX,
            })
        }
    };
    match prompt_cost.checked_add(completion_cost) {
        Some(v) => Ok(v),
        None => Err(CostError::Overflow {
            prompt_cost,
            completion_cost,
        }),
    }
}

// =============================================================================
// Canonical Tokenizer Registry
// =============================================================================

/// Get canonical tokenizer version for a model.
///
/// **Case-sensitive:** Model names must be lowercase. Callers MUST normalize
/// model names to lowercase before calling this function.
pub fn get_canonical_tokenizer(model: &str) -> &'static str {
    const DEFAULT_TOKENIZER: &str = "tiktoken-cl100k_base-v1.2.3";

    // Exact-match table: (model_name, tokenizer_version)
    const EXACT_TABLE: &[(&str, &str)] = &[
        // OpenAI GPT family
        ("gpt-3.5-turbo", "tiktoken-cl100k_base-v1.2.3"),
        ("gpt-4", "tiktoken-cl100k_base-v1.2.3"),
        ("gpt-4-turbo", "tiktoken-cl100k_base-v1.2.3"),
        ("gpt-4o", "tiktoken-o200k_base"),
        ("gpt-4o-mini", "tiktoken-o200k_base"),
        // OpenAI o-series (o200k_base vocab)
        ("o1", "tiktoken-o200k_base"),
        ("o1-mini", "tiktoken-o200k_base"),
        ("o1-preview", "tiktoken-o200k_base"),
        ("o3", "tiktoken-o200k_base"),
        ("o3-mini", "tiktoken-cl100k_base-v1.2.3"),
        ("o3-pro", "tiktoken-cl100k_base-v1.2.3"),
        // Anthropic Claude family
        ("claude-3-5-haiku", "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-5-opus", "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-5-sonnet", "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-haiku", "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-opus", "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-sonnet", "tiktoken-cl100k_base-v1.2.3"),
        // Google Gemini family
        ("gemini-1.5-flash", "tiktoken-cl100k_base-v1.2.3"),
        ("gemini-1.5-pro", "tiktoken-cl100k_base-v1.2.3"),
        ("gemini-2.0-flash", "tiktoken-cl100k_base-v1.2.3"),
        ("gemini-2.0-pro", "tiktoken-cl100k_base-v1.2.3"),
        // Mistral family
        ("mistral-7b", "tiktoken-cl100k_base-v1.2.3"),
        ("mistral-large", "tiktoken-cl100k_base-v1.2.3"),
        ("mistral-small", "tiktoken-cl100k_base-v1.2.3"),
        // Meta LLaMA family
        ("llama-3-8b", "tiktoken-cl100k_base-v1.2.3"),
        ("llama-3-70b", "tiktoken-cl100k_base-v1.2.3"),
    ];

    // 1. Exact match lookup (case-sensitive)
    if let Some((_, tokenizer)) = EXACT_TABLE.iter().find(|(m, _)| *m == model) {
        return tokenizer;
    }

    // 2. Case-insensitive prefix fallback for unknown variants of known families
    let model_lower = model.to_lowercase();
    let o200k = "tiktoken-o200k_base";
    if model_lower.starts_with("gemini-") {
        DEFAULT_TOKENIZER
    } else if model_lower.starts_with("gpt-")
        || model_lower.starts_with("claude-")
        || model_lower.starts_with("mistral-")
        || model_lower.starts_with("llama-")
    {
        "tiktoken-cl100k_base-v1.2.3"
    } else if model_lower.starts_with("o1-m")
        || model_lower.starts_with("o1-p")
        || model_lower.starts_with("o1")
        || model_lower.starts_with("o3")
    {
        o200k
    } else {
        DEFAULT_TOKENIZER
    }
}

// =============================================================================
// tokenizer_version_to_id
// =============================================================================

/// Convert tokenizer version string to tokenizer_id for BLOB(16) storage.
/// Uses BLAKE3 truncated to 16 bytes (per RFC-0909 §tokenizer_id).
pub fn tokenizer_version_to_id(version: &str) -> [u8; 16] {
    use blake3::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(version.as_bytes());
    let hash: blake3::Hash = hasher.finalize();
    let bytes: [u8; 32] = hash.into();
    bytes[..16]
        .try_into()
        .expect("BLAKE3 output always yields at least 16 bytes")
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod compute_pricing_hash_tests {
    use super::*;

    #[test]
    fn test_pricing_hash_tv1() {
        // Test vector from RFC-0910 §Test Vectors
        let table = PricingTable {
            table_id: "openai-gpt4-v1".into(),
            version: 1,
            provider: "openai".into(),
            model: "gpt-4".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
            effective_from: 1_704_067_200,
            metadata: BTreeMap::new(),
        };
        let hash = table.compute_pricing_hash();
        let hash_hex = hex::encode(hash);
        assert_eq!(
            hash_hex, "4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60",
            "pricing_hash mismatch — DCS Entry 16 encoding broken"
        );
    }

    #[test]
    fn test_pricing_hash_empty_metadata() {
        let table = PricingTable {
            table_id: "test-v1".into(),
            version: 1,
            provider: "test".into(),
            model: "test".into(),
            prompt_cost_per_1k: 10_000,
            completion_cost_per_1k: 20_000,
            effective_from: 1_700_000_000,
            metadata: BTreeMap::new(),
        };
        let hash = table.compute_pricing_hash();
        assert_eq!(hash.len(), 32, "hash should be 32 bytes");
    }

    #[test]
    fn test_pricing_hash_with_metadata() {
        let mut metadata = BTreeMap::new();
        metadata.insert("tokenizer_version_expiry".into(), "1735689600".into());
        let table = PricingTable {
            table_id: "test-v2".into(),
            version: 2,
            provider: "test".into(),
            model: "test".into(),
            prompt_cost_per_1k: 15_000,
            completion_cost_per_1k: 30_000,
            effective_from: 1_704_067_200,
            metadata,
        };
        let hash = table.compute_pricing_hash();
        assert_eq!(hash.len(), 32, "hash should be 32 bytes");
    }
}

#[cfg(test)]
mod compute_cost_tests {
    use super::*;

    #[test]
    fn test_compute_cost_tv1() {
        // Test vector from RFC-0910 §Test Vectors
        let pricing = PricingTable {
            table_id: "test".into(),
            version: 1,
            provider: "test".into(),
            model: "test".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
            effective_from: 1_704_067_200,
            metadata: BTreeMap::new(),
        };
        assert_eq!(
            compute_cost(&pricing, 100, 50),
            Ok(6000),
            "TV1: 100 prompt + 50 completion tokens at 30k/60k per 1K = 6000 micro-units"
        );
    }

    #[test]
    fn test_compute_cost_zero_tokens() {
        let pricing = minimal_table(10_000, 20_000);
        assert_eq!(compute_cost(&pricing, 0, 0), Ok(0));
    }

    #[test]
    fn test_compute_cost_input_only() {
        let pricing = minimal_table(30_000, 60_000);
        assert_eq!(compute_cost(&pricing, 1000, 0), Ok(30_000));
    }

    #[test]
    fn test_compute_cost_output_only() {
        let pricing = minimal_table(30_000, 60_000);
        assert_eq!(compute_cost(&pricing, 0, 1000), Ok(60_000));
    }

    #[test]
    fn test_compute_cost_large_tokens() {
        let pricing = minimal_table(30_000, 60_000);
        assert_eq!(
            compute_cost(&pricing, 1_000_000, 1_000_000),
            Ok(90_000_000),
            "1M tokens each direction = 90M micro-units"
        );
    }

    fn minimal_table(prompt: u64, completion: u64) -> PricingTable {
        PricingTable {
            table_id: "t".into(),
            version: 1,
            provider: "t".into(),
            model: "t".into(),
            prompt_cost_per_1k: prompt,
            completion_cost_per_1k: completion,
            effective_from: 0,
            metadata: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tokenizer_tests {
    use super::*;

    #[test]
    fn test_tokenizer_exact_gpt4() {
        assert_eq!(
            get_canonical_tokenizer("gpt-4"),
            "tiktoken-cl100k_base-v1.2.3"
        );
    }

    #[test]
    fn test_tokenizer_exact_gpt4o() {
        assert_eq!(get_canonical_tokenizer("gpt-4o"), "tiktoken-o200k_base");
    }

    #[test]
    fn test_tokenizer_exact_o1_mini() {
        assert_eq!(get_canonical_tokenizer("o1-mini"), "tiktoken-o200k_base");
    }

    #[test]
    fn test_tokenizer_exact_o1_preview() {
        assert_eq!(get_canonical_tokenizer("o1-preview"), "tiktoken-o200k_base");
    }

    #[test]
    fn test_tokenizer_exact_o3() {
        assert_eq!(get_canonical_tokenizer("o3"), "tiktoken-o200k_base");
    }

    #[test]
    fn test_tokenizer_exact_o3_mini() {
        assert_eq!(
            get_canonical_tokenizer("o3-mini"),
            "tiktoken-cl100k_base-v1.2.3"
        );
    }

    #[test]
    fn test_tokenizer_exact_claude() {
        assert_eq!(
            get_canonical_tokenizer("claude-3-opus"),
            "tiktoken-cl100k_base-v1.2.3"
        );
    }

    #[test]
    fn test_tokenizer_unknown_fallback() {
        assert_eq!(
            get_canonical_tokenizer("nonexistent-model-v2"),
            "tiktoken-cl100k_base-v1.2.3"
        );
    }

    #[test]
    fn test_tokenizer_version_to_id() {
        let id = tokenizer_version_to_id("tiktoken-cl100k_base-v1.2.3");
        assert_eq!(hex::encode(id), "e3c8e8ff724411c6416dd4fb135368e3");
        let id2 = tokenizer_version_to_id("tiktoken-o200k_base");
        assert_eq!(hex::encode(id2), "be1b3be0a2698c863b31edc1b7809a9c");
    }
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    fn make_table(provider: &str, model: &str, version: u32, effective_from: i64) -> PricingTable {
        PricingTable {
            table_id: format!("{}-{}-v{}", provider, model, version),
            version,
            provider: provider.into(),
            model: model.into(),
            prompt_cost_per_1k: 10_000,
            completion_cost_per_1k: 20_000,
            effective_from,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn test_register_basic() {
        let mut registry = PricingRegistry::default();
        let table = make_table("openai", "gpt-4", 1, 1_700_000_000);
        let hash = registry.register(table).unwrap();
        assert_eq!(hash.len(), 32);
        assert_eq!(registry.get("openai", "gpt-4").unwrap().version, 1);
    }

    #[test]
    fn test_register_duplicate_version() {
        let mut registry = PricingRegistry::default();
        registry
            .register(make_table("openai", "gpt-4", 1, 1_700_000_000))
            .unwrap();
        let result = registry.register(make_table("openai", "gpt-4", 1, 1_700_000_001));
        assert!(matches!(
            result,
            Err(RegistryError::DuplicateVersion { .. })
        ));
    }

    #[test]
    fn test_register_version_not_increment() {
        let mut registry = PricingRegistry::default();
        registry
            .register(make_table("openai", "gpt-4", 2, 1_700_000_000))
            .unwrap();
        let result = registry.register(make_table("openai", "gpt-4", 1, 1_700_000_001));
        assert!(matches!(
            result,
            Err(RegistryError::VersionNotIncrement { .. })
        ));
    }

    #[test]
    fn test_register_effective_from_not_increment() {
        let mut registry = PricingRegistry::default();
        registry
            .register(make_table("openai", "gpt-4", 1, 1_700_000_001))
            .unwrap();
        let result = registry.register(make_table("openai", "gpt-4", 2, 1_700_000_000));
        assert!(matches!(
            result,
            Err(RegistryError::EffectiveFromNotIncrement { .. })
        ));
    }

    #[test]
    fn test_register_get_latest() {
        let mut registry = PricingRegistry::default();
        registry
            .register(make_table("openai", "gpt-4", 1, 1_700_000_000))
            .unwrap();
        registry
            .register(make_table("openai", "gpt-4", 2, 1_700_000_100))
            .unwrap();
        let latest = registry.get("openai", "gpt-4").unwrap();
        assert_eq!(latest.version, 2, "should return latest version");
    }

    #[test]
    fn test_get_by_hash() {
        let mut registry = PricingRegistry::default();
        let table = make_table("openai", "gpt-4", 1, 1_700_000_000);
        let hash = table.compute_pricing_hash();
        registry.register(table).unwrap();
        let retrieved = registry.get_by_hash(&hash).unwrap();
        assert_eq!(retrieved.version, 1);
    }

    #[test]
    fn test_table_id_too_long() {
        let mut registry = PricingRegistry::default();
        let mut table = make_table("openai", "gpt-4", 1, 1_700_000_000);
        table.table_id = "a".repeat(129);
        let result = registry.register(table);
        assert!(matches!(result, Err(RegistryError::TableIdTooLong { .. })));
    }
}
