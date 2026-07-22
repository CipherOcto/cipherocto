//! Caveat DSL for macaroon capability tokens (RFC-0957 §3.1 + §3.5).
//!
//! Strongly-typed enum for common caveats + `Raw` escape hatch for unknown axes.
//! `canonical_ser` per RFC-0126 deterministic serialization so HMAC inputs are
//! stable across implementations.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// OCTO-W micro-denomination (u128). 1 OCTO-W = 1_000_000 micro-OCTO-W.
pub type MicroOctoW = u128;

/// Provider identifier (opaque string).
pub type ProviderId = String;

/// Model reference (provider-specific model string).
pub type ModelRef = String;

/// Unix epoch seconds.
pub type UnixTimeSecs = u64;

/// Overlay identity (did:octo:...).
pub type OverlayIdentity = String;

/// ISO-3166 country code (2-letter).
pub type ISO3166 = String;

/// Ask identifier (RFC-0959 v1.0 `AskId` — content-addressable hash).
pub type AskId = [u8; 32];

/// BLAKE3 32-byte digest.
pub type Blake3 = [u8; 32];

/// Cache policy attached to a capability token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy")]
pub enum CachePolicy {
    /// Cache disabled.
    #[serde(rename = "off")]
    Off,
    /// Cache opt-in; holder may attach a specific cache key hash.
    #[serde(rename = "opt-in")]
    OptIn { cache_key_hash: Option<Blake3> },
    /// Cache always; TTL in seconds.
    #[serde(rename = "always")]
    Always { ttl_secs: u32 },
}

/// Per-axis upper bound on settlement cost.
///
/// Caveat format: `PerAxisMax { axis, max_per_1k }` where `axis` is a string
/// (e.g., "input_tokens", "output_tokens", "cached_input_tokens") and
/// `max_per_1k` is the maximum micro-OCTO-W per 1000 units of that axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerAxisMax {
    pub axis: String,
    pub max_per_1k: MicroOctoW,
}

/// Rate-limit bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub rpm: u32,
    pub tpm: u32,
}

/// Strongly-typed caveat enum + `Raw` escape hatch.
///
/// **Attenuation invariant (RFC-0957 §3.5):** Attenuators MAY add caveats
/// but MUST NOT remove caveats. The verify routine enforces this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Caveat {
    /// Total budget cap (implied sum over all axes at settlement time).
    #[serde(rename = "amount_max")]
    AmountMax(MicroOctoW),

    /// Per-axis cap. Holder may settle up to `max_per_1k` per 1000 units.
    #[serde(rename = "per_axis_max")]
    PerAxisMax(PerAxisMax),

    /// Allowed model.
    #[serde(rename = "model")]
    Model(ModelRef),

    /// Allowed providers (any-of).
    #[serde(rename = "provider")]
    Provider(Vec<ProviderId>),

    /// Capability expires at this Unix time (inclusive).
    #[serde(rename = "before")]
    Before(UnixTimeSecs),

    /// Audience (DID) the capability is bound to.
    #[serde(rename = "audience")]
    Audience(OverlayIdentity),

    /// Rate-limit envelope.
    #[serde(rename = "rate_limit")]
    RateLimit(RateLimit),

    /// Bind capability to a specific request body hash (anti-replay).
    #[serde(rename = "invocation_hash_bind")]
    InvocationHashBind(Blake3),

    /// Jurisdiction whitelist.
    #[serde(rename = "jurisdiction")]
    Jurisdiction(HashSet<ISO3166>),

    /// Cache policy.
    #[serde(rename = "cache_strategy")]
    CacheStrategy(CachePolicy),

    /// Bind capability to a specific Ask by id.
    #[serde(rename = "ask_binding")]
    AskBinding(AskId),

    /// Third-party caveat requiring a discharge macaroon.
    #[serde(rename = "third_party")]
    ThirdParty(String),

    /// Escape hatch for unknown / forward-compat caveat names.
    #[serde(rename = "raw")]
    Raw(RawCaveat),
}

/// Escape-hatch caveat (name + value bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawCaveat {
    pub name: String,
    #[serde(with = "serde_bytes")]
    pub value: Vec<u8>,
}

/// Caveat name string (used as `info` parameter to HMAC-BLAKE3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaveatName {
    AmountMax,
    PerAxisMax,
    Model,
    Provider,
    Before,
    Audience,
    RateLimit,
    InvocationHashBind,
    Jurisdiction,
    CacheStrategy,
    AskBinding,
    ThirdParty,
    Raw,
}

impl CaveatName {
    /// Wire-stable identifier used as HMAC info string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AmountMax => "cipherocto/cap/v1/caveat/amount_max",
            Self::PerAxisMax => "cipherocto/cap/v1/caveat/per_axis_max",
            Self::Model => "cipherocto/cap/v1/caveat/model",
            Self::Provider => "cipherocto/cap/v1/caveat/provider",
            Self::Before => "cipherocto/cap/v1/caveat/before",
            Self::Audience => "cipherocto/cap/v1/caveat/audience",
            Self::RateLimit => "cipherocto/cap/v1/caveat/rate_limit",
            Self::InvocationHashBind => "cipherocto/cap/v1/caveat/invocation_hash_bind",
            Self::Jurisdiction => "cipherocto/cap/v1/caveat/jurisdiction",
            Self::CacheStrategy => "cipherocto/cap/v1/caveat/cache_strategy",
            Self::AskBinding => "cipherocto/cap/v1/caveat/ask_binding",
            Self::ThirdParty => "cipherocto/cap/v1/caveat/third_party",
            Self::Raw => "cipherocto/cap/v1/caveat/raw",
        }
    }
}

impl Caveat {
    /// Wire-stable name (used as HMAC info).
    #[must_use]
    pub fn name(&self) -> CaveatName {
        match self {
            Self::AmountMax(_) => CaveatName::AmountMax,
            Self::PerAxisMax(_) => CaveatName::PerAxisMax,
            Self::Model(_) => CaveatName::Model,
            Self::Provider(_) => CaveatName::Provider,
            Self::Before(_) => CaveatName::Before,
            Self::Audience(_) => CaveatName::Audience,
            Self::RateLimit(_) => CaveatName::RateLimit,
            Self::InvocationHashBind(_) => CaveatName::InvocationHashBind,
            Self::Jurisdiction(_) => CaveatName::Jurisdiction,
            Self::CacheStrategy(_) => CaveatName::CacheStrategy,
            Self::AskBinding(_) => CaveatName::AskBinding,
            Self::ThirdParty(_) => CaveatName::ThirdParty,
            Self::Raw(_) => CaveatName::Raw,
        }
    }

    /// Canonical serialization per RFC-0126 (deterministic JSON).
    ///
    /// Sort keys alphabetically; `serde_json` with `preserve_order = false`
    /// (default) gives non-deterministic output. We use a custom serializer
    /// that produces stable output: tagged variant → `tag || value`.
    #[must_use]
    pub fn canonical_ser(&self) -> Vec<u8> {
        // Deterministic JSON: serialize each variant as `{"type": "...", "value": <payload>}`.
        // serde_json cannot serialize tagged newtype variants directly, so we
        // build the JSON value manually. HashSet + Vec<ProviderId> are sorted
        // for determinism (HMAC input stability across orderings).
        let value = match self {
            Caveat::AmountMax(v) => serde_json::json!({"type": "amount_max", "value": v}),
            Caveat::PerAxisMax(p) => serde_json::json!({"type": "per_axis_max", "value": p}),
            Caveat::Model(m) => serde_json::json!({"type": "model", "value": m}),
            Caveat::Provider(p) => {
                let mut sorted: Vec<&String> = p.iter().collect();
                sorted.sort();
                serde_json::json!({"type": "provider", "value": sorted})
            }
            Caveat::Before(t) => serde_json::json!({"type": "before", "value": t}),
            Caveat::Audience(a) => serde_json::json!({"type": "audience", "value": a}),
            Caveat::RateLimit(r) => serde_json::json!({"type": "rate_limit", "value": r}),
            Caveat::InvocationHashBind(h) => {
                serde_json::json!({"type": "invocation_hash_bind", "value": hex::encode(h)})
            }
            Caveat::Jurisdiction(set) => {
                let mut sorted: Vec<&String> = set.iter().collect();
                sorted.sort();
                serde_json::json!({"type": "jurisdiction", "value": sorted})
            }
            Caveat::CacheStrategy(c) => serde_json::json!({"type": "cache_strategy", "value": c}),
            Caveat::AskBinding(id) => {
                serde_json::json!({"type": "ask_binding", "value": hex::encode(id)})
            }
            Caveat::ThirdParty(channel) => {
                serde_json::json!({"type": "third_party", "value": channel})
            }
            Caveat::Raw(r) => serde_json::json!({
                "type": "raw",
                "value": {"name": r.name, "value": hex::encode(&r.value)}
            }),
        };
        serde_json::to_vec(&value).expect("serializable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caveat_name_stable() {
        // Wire-stable identifier MUST NOT change between releases without
        // bumping version constant in RFC-0957.
        assert_eq!(
            CaveatName::AmountMax.as_str(),
            "cipherocto/cap/v1/caveat/amount_max"
        );
        assert_eq!(
            CaveatName::AskBinding.as_str(),
            "cipherocto/cap/v1/caveat/ask_binding"
        );
    }

    #[test]
    fn canonical_ser_deterministic_for_jurisdiction() {
        let a = Caveat::Jurisdiction(["US".to_owned(), "DE".to_owned()].into_iter().collect());
        let b = Caveat::Jurisdiction(["DE".to_owned(), "US".to_owned()].into_iter().collect());
        assert_eq!(a.canonical_ser(), b.canonical_ser());
    }

    #[test]
    fn canonical_ser_stable_across_runs() {
        let c = Caveat::AmountMax(1_000_000);
        assert_eq!(c.canonical_ser(), c.canonical_ser());
    }

    #[test]
    fn canonical_ser_provider_order_independent() {
        let a = Caveat::Provider(vec!["openai".to_owned(), "anthropic".to_owned()]);
        let b = Caveat::Provider(vec!["anthropic".to_owned(), "openai".to_owned()]);
        assert_eq!(a.canonical_ser(), b.canonical_ser());
    }
}

// `serde_bytes` shim — Json representation: hex string.
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(de)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}
