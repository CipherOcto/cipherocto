//! Cache classification (PR-Q5 + 11-step exercise Step 9).
//!
//! Classifies a provider response into cache classes (exact / fuzzy / miss)
//! and computes per-axis consumption. PR-Q5 wraps this in
//! `ReceiptEnvelope` for signed commitment.

use serde::{Deserialize, Serialize};

use crate::receipt::CacheClassifyMeta;

/// Provider cache class (RFC-0959 §Cache classification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheClass {
    /// Exact match — request body hash matched a previous cached response.
    Exact,
    /// Fuzzy match — semantic similarity above threshold.
    Fuzzy,
    /// Miss — no cached response.
    Miss,
}

impl CacheClass {
    /// Wire-stable identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Fuzzy => "fuzzy",
            Self::Miss => "miss",
        }
    }
}

/// Provider cache hit indicator (RFC-0959 §Cache classification approach).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheHit {
    pub class: CacheClass,
    /// BLAKE3 32-byte cache key; None on Miss.
    pub cache_key_hash: Option<[u8; 32]>,
    /// Per-axis consumption (axis name → unit count).
    pub axes_consumed: Vec<(String, u64)>,
}

impl CacheHit {
    /// Build from a prompt + response pair (deterministic).
    ///
    /// `prompt_hash` is the BLAKE3 hash of the canonical request body.
    /// `response_hash` is the BLAKE3 hash of the canonical response body.
    /// `provider_flag` is the provider-reported cache indicator (when
    /// available); local cache lookup uses prompt_hash as the key.
    #[must_use]
    pub fn from_prompt_response(
        prompt_hash: [u8; 32],
        response_hash: [u8; 32],
        provider_flag: Option<bool>,
        axes_consumed: Vec<(String, u64)>,
    ) -> Self {
        let _ = prompt_hash; // local cache lookup would key on this
        let class = if let Some(true) = provider_flag {
            // Provider confirms cache hit → trust provider.
            CacheClass::Exact
        } else {
            CacheClass::Miss
        };
        let cache_key_hash = if matches!(class, CacheClass::Miss) {
            None
        } else {
            Some(response_hash)
        };
        Self { class, cache_key_hash, axes_consumed }
    }

    /// Convert to the proxy-level `CacheClassifyMeta` (PR-Q5).
    #[must_use]
    pub fn to_meta(&self) -> CacheClassifyMeta {
        CacheClassifyMeta {
            cache_class: self.class.as_str().to_owned(),
            cache_key_hash: self.cache_key_hash,
            axes_consumed: self.axes_consumed.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_class_wire_strings() {
        assert_eq!(CacheClass::Exact.as_str(), "exact");
        assert_eq!(CacheClass::Fuzzy.as_str(), "fuzzy");
        assert_eq!(CacheClass::Miss.as_str(), "miss");
    }

    #[test]
    fn from_prompt_response_provider_hit_yields_exact() {
        let prompt = [0xab; 32];
        let response = [0xcd; 32];
        let hit = CacheHit::from_prompt_response(
            prompt,
            response,
            Some(true),
            vec![("input_tokens_per_1k".to_owned(), 100)],
        );
        assert_eq!(hit.class, CacheClass::Exact);
        assert_eq!(hit.cache_key_hash, Some(response));
        assert_eq!(hit.axes_consumed.len(), 1);
    }

    #[test]
    fn from_prompt_response_provider_miss_yields_miss() {
        let prompt = [0xab; 32];
        let response = [0xcd; 32];
        let hit = CacheHit::from_prompt_response(
            prompt,
            response,
            Some(false),
            vec![],
        );
        assert_eq!(hit.class, CacheClass::Miss);
        assert_eq!(hit.cache_key_hash, None);
    }

    #[test]
    fn from_prompt_response_no_provider_flag_defaults_miss() {
        let prompt = [0xab; 32];
        let response = [0xcd; 32];
        let hit = CacheHit::from_prompt_response(prompt, response, None, vec![]);
        assert_eq!(hit.class, CacheClass::Miss);
    }

    #[test]
    fn to_meta_preserves_fields() {
        let hit = CacheHit {
            class: CacheClass::Fuzzy,
            cache_key_hash: Some([0x99; 32]),
            axes_consumed: vec![("output_tokens".to_owned(), 50)],
        };
        let meta = hit.to_meta();
        assert_eq!(meta.cache_class, "fuzzy");
        assert_eq!(meta.cache_key_hash, Some([0x99; 32]));
        assert_eq!(meta.axes_consumed.len(), 1);
    }
}
