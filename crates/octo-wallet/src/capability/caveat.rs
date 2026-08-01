//! Caveat DSL for macaroon capability tokens (RFC-0957 §3.1 + §3.5).
//!
//! Strongly-typed enum for common caveats + `Raw` escape hatch for unknown axes.
//! `canonical_ser` per RFC-0126 deterministic serialization so HMAC inputs are
//! stable across implementations.

use std::collections::HashSet;

use cipherocto_encoding::{encode as encode_constraint, Constraint};
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

    // RFC-0965 §3 — caveat types added by the Acceptance v1.1 bump.
    /// Bind capability to a specific vault (RFC-0960 §2.1).
    #[serde(rename = "vault")]
    Vault([u8; 32]),

    /// Permission kind (RFC-0960 §2.2 + RFC-0965 §3.2).
    #[serde(rename = "permission")]
    Permission(PermissionKind),

    /// Valid time range (RFC-0960 §2.2 + RFC-0965 §3.3; supersedes single
    /// `Before` for ranges).
    #[serde(rename = "valid_range")]
    ValidRange {
        valid_after_unix: u64,
        valid_until_unix: u64,
    },

    /// Per-transaction cap (RFC-0960 §2.2 + RFC-0965 §3.4; distinct from
    /// `AmountMax` which is total budget).
    #[serde(rename = "max_per_tx")]
    MaxPerTx(u128),

    /// Audit window duration (RFC-0960 §2.2 + RFC-0965 §3.5). 0 = instant.
    #[serde(rename = "audit_window")]
    AuditWindow { duration_secs: u64 },

    /// Max number of uses (RFC-0960 §2.2 + RFC-0965 §3.6; 0 = unlimited).
    #[serde(rename = "max_uses")]
    MaxUses { count: u32 },

    /// Wrapped-only (RFC-0960 §2.2 + RFC-0965 §3.7): capability only usable
    /// through a parent capability. Chain depth bounded to 16 per RFC-0965
    /// §3.7 R7-F1.
    #[serde(rename = "wrapped_only")]
    WrappedOnly { parent_capability: [u8; 32] },

    /// Factory vet (RFC-0960 §2.2 + RFC-0965 §3.8): pre-validated invocation
    /// (target + selector + arg template). NOT raw bytes (phishing vector).
    #[serde(rename = "factory")]
    Factory(FactoryVet),

    /// Policy reference (RFC-0960 §2.2 + RFC-0965 §3.9 + RFC-0967).
    /// Carries the policy_id hash + the policy version_seq + a witness
    /// signature binding the attenuation per RFC-0967 §8.2.
    #[serde(rename = "policy_reference")]
    PolicyReference {
        policy_id: [u8; 32],
        policy_version_seq: u64,
        #[serde(with = "serde_bytes_arr64")]
        attenuation_witness: [u8; 64],
    },

    /// Valid-after time bound (RFC-0965 §3.3). Single timestamp; for ranges
    /// use the RFC-0964 `Constraint::ValidRange` instead.
    #[serde(rename = "valid_after")]
    ValidAfter { not_before_unix: u64 },

    /// Redemption context (RFC-0965 §3.6). Anti-replay domain separator.
    /// `context_hash = BLAKE3(0xA2 || canonical_ser(context))` per RFC-0965 §3.6.
    #[serde(rename = "redemption_context")]
    RedemptionContext { context_hash: [u8; 32] },

    /// Shard pin (RFC-0965 §1.2 + RFC-0963 §6). Restricts capability to a
    /// specific shard.
    #[serde(rename = "sharded")]
    Sharded { shard_id: u32 },
}

/// Permission kind enum (RFC-0960 §2.2 + RFC-0965 §3.2).
///
/// Adding new kinds is a backwards-compatible variant add per RFC-0960 §R1-F6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    NativeTokenTransfer,
    Erc20TokenTransfer,
    ContractCall,
    Reservation,
    VaultMutation,
}

/// Canonical action template (RFC-0960 §10.7 + RFC-0965 §3.8).
///
/// `action_template` is a typed invocation shape (selector + ordered args),
/// NOT opaque bytes. The verifier runs the same constraint pipeline
/// against the deployed target before redeeming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionTemplate {
    /// Selector (function/method identifier).
    pub selector: String,
    /// Ordered canonical args (DIDs, amounts, asset_ids, …).
    pub args: Vec<String>,
}

/// Factory vet (RFC-0960 §2.2 + RFC-0965 §3.8).
///
/// Canonicalised by RFC-0126. NOT opaque bytes — the verifier runs the same
/// constraint pipeline against the deployed target before redeeming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryVet {
    /// Target vault for the pre-validated invocation.
    pub target_vault_id: [u8; 32],
    /// Typed invocation shape (selector + ordered args).
    pub action_template: ActionTemplate,
    /// Who must invoke (default = capability holder).
    pub required_caller: Option<String>,
    /// Pre-conditions that must all hold at redemption time (RFC-0964
    /// `Constraint` set).
    pub pre_conditions: Vec<Constraint>,
    /// Hard deadline for deploying + redeeming.
    pub expiry_for_deploy_unix: u64,
}

/// Escape-hatch caveat (name + value bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawCaveat {
    pub name: String,
    #[serde(with = "serde_bytes")]
    pub value: Vec<u8>,
}

/// Caveat name string (used as `info` parameter to HMAC-BLAKE3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Vault,
    Permission,
    ValidRange,
    MaxPerTx,
    AuditWindow,
    MaxUses,
    WrappedOnly,
    Factory,
    PolicyReference,
    ValidAfter,
    RedemptionContext,
    Sharded,
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
            Self::Vault => "cipherocto/cap/v1/caveat/vault",
            Self::Permission => "cipherocto/cap/v1/caveat/permission",
            Self::ValidRange => "cipherocto/cap/v1/caveat/valid_range",
            Self::MaxPerTx => "cipherocto/cap/v1/caveat/max_per_tx",
            Self::AuditWindow => "cipherocto/cap/v1/caveat/audit_window",
            Self::MaxUses => "cipherocto/cap/v1/caveat/max_uses",
            Self::WrappedOnly => "cipherocto/cap/v1/caveat/wrapped_only",
            Self::Factory => "cipherocto/cap/v1/caveat/factory",
            Self::PolicyReference => "cipherocto/cap/v1/caveat/policy_reference",
            Self::ValidAfter => "cipherocto/cap/v1/caveat/valid_after",
            Self::RedemptionContext => "cipherocto/cap/v1/caveat/redemption_context",
            Self::Sharded => "cipherocto/cap/v1/caveat/sharded",
        }
    }
}

impl PermissionKind {
    /// Wire-stable identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NativeTokenTransfer => "cipherocto/cap/v1/permission/native_token_transfer",
            Self::Erc20TokenTransfer => "cipherocto/cap/v1/permission/erc20_token_transfer",
            Self::ContractCall => "cipherocto/cap/v1/permission/contract_call",
            Self::Reservation => "cipherocto/cap/v1/permission/reservation",
            Self::VaultMutation => "cipherocto/cap/v1/permission/vault_mutation",
        }
    }
}

/// Monotonic subsumption (RFC-0957 §3.5).
///
/// `parent ⊇ child` iff every caveat in `child` is implied by some caveat in
/// `parent`. Used at attenuation verification: a child capability may only
/// add caveats (or narrow existing ones), never remove or widen.
///
/// Per-variant subsumption rules (child narrows parent):
/// - `AmountMax`: child ≤ parent (child budget ≤ parent budget)
/// - `PerAxisMax`: same-axis child ≤ parent
/// - `Model`: parent model == child model
/// - `Provider`: child set ⊆ parent set
/// - `Before`: child deadline ≤ parent deadline (child expires earlier)
/// - `Audience`: parent DID == child DID
/// - `RateLimit`: child rpm ≤ parent rpm AND child tpm ≤ parent tpm
/// - `CacheStrategy`: parent more permissive (Always › OptIn › Off)
/// - `Jurisdiction`: child set ⊆ parent set
/// - `InvocationHashBind`: parent hash == child hash
/// - `AskBinding`: parent ask_id == child ask_id
/// - `ThirdParty`: parent channel == child channel
/// - `Raw`: parent name == child name AND parent value == child value
#[must_use]
pub fn set_subsumes(parent: &[Caveat], child: &[Caveat]) -> bool {
    child.iter().all(|c| parent_caveat_implies(parent, c))
}

/// Catalog-aware subsumption check. Adds the Raw caveat fail-closed
/// invariant: any `Caveat::Raw` whose `name` is not registered with
/// the catalog is rejected (returns `false`). Mission 0957-a AC #13.
///
/// `is_raw_registered` is the catalog's registration predicate
/// (see `CapabilityCatalog::is_raw_name_registered`). Pass
/// `|_| false` for the legacy fail-closed default (reject all Raw).
#[must_use]
pub fn set_subsumes_with_registry<F: Fn(&str) -> bool>(
    parent: &[Caveat],
    child: &[Caveat],
    is_raw_registered: F,
) -> bool {
    child.iter().all(|c| {
        // Fail-closed for unregistered Raw caveat names.
        if let Caveat::Raw(r) = c {
            if !is_raw_registered(&r.name) {
                return false;
            }
        }
        parent_caveat_implies(parent, c)
    })
}

#[allow(clippy::too_many_lines)]
fn parent_caveat_implies(parent: &[Caveat], child: &Caveat) -> bool {
    match child {
        Caveat::AmountMax(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::AmountMax(p) if *c <= *p)),
        Caveat::PerAxisMax(c) => parent.iter().any(|p| match p {
            Caveat::PerAxisMax(p_inner) => {
                p_inner.axis == c.axis && c.max_per_1k <= p_inner.max_per_1k
            }
            _ => false,
        }),
        Caveat::Model(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::Model(p) if p == c)),
        Caveat::Provider(c) => parent.iter().any(|p| match p {
            Caveat::Provider(p) => {
                let c_set: HashSet<&String> = c.iter().collect();
                let p_set: HashSet<&String> = p.iter().collect();
                c_set.is_subset(&p_set)
            }
            _ => false,
        }),
        Caveat::Before(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::Before(p) if *c <= *p)),
        Caveat::Audience(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::Audience(p) if p == c)),
        Caveat::RateLimit(c) => parent.iter().any(|p| match p {
            Caveat::RateLimit(p_inner) => c.rpm <= p_inner.rpm && c.tpm <= p_inner.tpm,
            _ => false,
        }),
        Caveat::CacheStrategy(c) => parent.iter().any(|p| match p {
            Caveat::CacheStrategy(p_inner) => cache_policy_subsumes(p_inner, c),
            _ => false,
        }),
        Caveat::Jurisdiction(c) => parent.iter().any(|p| match p {
            Caveat::Jurisdiction(p) => c.is_subset(p),
            _ => false,
        }),
        Caveat::InvocationHashBind(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::InvocationHashBind(p) if p == c)),
        Caveat::AskBinding(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::AskBinding(p) if p == c)),
        Caveat::ThirdParty(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::ThirdParty(p) if p == c)),
        Caveat::Raw(c) => parent.iter().any(|p| match p {
            Caveat::Raw(p_inner) => p_inner.name == c.name && p_inner.value == c.value,
            _ => false,
        }),
        // RFC-0965 §3 — new types. Subsumption rules:
        // - Vault: parent vault_id == child vault_id (cannot change vault
        //   via attenuation).
        // - Permission: parent kind == child kind (cannot widen kind).
        // - ValidRange: child range ⊆ parent range (valid_after >= parent,
        //   valid_until <= parent).
        // - MaxPerTx: child amount <= parent amount.
        // - AuditWindow: child duration >= parent duration (upgrade from
        //   high-trust to auditable; R7-F8).
        // - MaxUses: child count <= parent count (0 = unlimited is most
        //   permissive — parent 0 subsumes child N).
        // - WrappedOnly: parent_capability == child parent_capability.
        // - Factory: requires full canonical-vector equality (the vet is a
        //   signed assertion; cannot change target/template).
        // - PolicyReference: hash equality.
        Caveat::Vault(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::Vault(p) if p == c)),
        Caveat::Permission(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::Permission(p) if p == c)),
        Caveat::ValidRange { valid_after_unix: c_after, valid_until_unix: c_until } => {
            parent.iter().any(|p| match p {
                Caveat::ValidRange { valid_after_unix: p_after, valid_until_unix: p_until } => {
                    c_after >= p_after && c_until <= p_until
                }
                _ => false,
            })
        }
        Caveat::MaxPerTx(c) => parent
            .iter()
            .any(|p| matches!(p, Caveat::MaxPerTx(p) if c <= p)),
        Caveat::AuditWindow { duration_secs: c_dur } => parent.iter().any(|p| match p {
            Caveat::AuditWindow { duration_secs: p_dur } => c_dur >= p_dur,
            _ => false,
        }),
        Caveat::MaxUses { count: c_count } => parent.iter().any(|p| match p {
            // Parent unlimited: subsumes any child (including unlimited).
            Caveat::MaxUses { count: 0 } => true,
            // Bounded parent, unlimited child: not subsumed (widening).
            Caveat::MaxUses { count: _ } if *c_count == 0 => false,
            // Bounded parent, bounded child: child count <= parent count.
            Caveat::MaxUses { count: p_count } => c_count <= p_count,
            _ => false,
        }),
        Caveat::WrappedOnly { parent_capability: c_parent } => parent.iter().any(|p| match p {
            Caveat::WrappedOnly { parent_capability: p_parent } => p_parent == c_parent,
            _ => false,
        }),
        Caveat::Factory(c) => parent.iter().any(|p| match p {
            Caveat::Factory(p) => {
                p.target_vault_id == c.target_vault_id
                    && p.action_template == c.action_template
                    && p.required_caller == c.required_caller
                    && p.pre_conditions == c.pre_conditions
                    && p.expiry_for_deploy_unix == c.expiry_for_deploy_unix
            }
            _ => false,
        }),
        Caveat::PolicyReference { policy_id, policy_version_seq, attenuation_witness } => {
            parent.iter().any(|p| matches!(
                p,
                Caveat::PolicyReference {
                    policy_id: p_id,
                    policy_version_seq: p_seq,
                    attenuation_witness: p_wit,
                } if p_id == policy_id && p_seq == policy_version_seq && p_wit == attenuation_witness
            ))
        }
        Caveat::ValidAfter { not_before_unix: c_after } => parent.iter().any(|p| match p {
            Caveat::ValidAfter { not_before_unix: p_after } => c_after >= p_after,
            _ => false,
        }),
        Caveat::RedemptionContext { context_hash: c_hash } => parent.iter().any(|p| match p {
            Caveat::RedemptionContext { context_hash: p_hash } => p_hash == c_hash,
            _ => false,
        }),
        Caveat::Sharded { shard_id: c_shard } => parent.iter().any(|p| match p {
            Caveat::Sharded { shard_id: p_shard } => p_shard == c_shard,
            _ => false,
        }),
    }
}

/// Cache policy subsumption: parent must be more permissive than child.
///
/// Permissiveness order: `Off` (no cache, most restrictive) ⊂ `OptIn`
/// (opt-in caching) ⊂ `Always` (cache always, least restrictive).
/// `parent ⊇ child` iff parent's policy permits all caching that child does.
#[allow(clippy::match_same_arms, clippy::unnested_or_patterns)]
fn cache_policy_subsumes(parent: &CachePolicy, child: &CachePolicy) -> bool {
    match (parent, child) {
        // Off ↔ Off: same restriction level.
        (CachePolicy::Off, CachePolicy::Off) => true,

        // Always ↔ Always: parent TTL must cover child TTL (child no longer).
        (CachePolicy::Always { ttl_secs: p_ttl }, CachePolicy::Always { ttl_secs: c_ttl }) => {
            c_ttl <= p_ttl
        }

        // OptIn ↔ OptIn: parent bound to key requires child to be too.
        (
            CachePolicy::OptIn {
                cache_key_hash: p_kh,
            },
            CachePolicy::OptIn {
                cache_key_hash: c_kh,
            },
        ) => match (p_kh, c_kh) {
            (None, _) => true, // parent allows any key
            (Some(p), Some(c)) => p == c,
            (Some(_), None) => false, // parent bound to specific key, child not
        },

        // Cross-variant: parent must be more permissive.
        (CachePolicy::Always { .. }, CachePolicy::OptIn { .. }) => true, // Always allows OptIn
        (CachePolicy::Always { .. }, CachePolicy::Off) => true,          // Always allows Off
        (CachePolicy::OptIn { .. }, CachePolicy::Off) => true,           // OptIn allows Off
        // Off is most restrictive; cannot permit OptIn or Always.
        // OptIn is more restrictive than Always.
        (CachePolicy::Off, CachePolicy::OptIn { .. }) => false,
        (CachePolicy::Off, CachePolicy::Always { .. }) => false,
        (CachePolicy::OptIn { .. }, CachePolicy::Always { .. }) => false,
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
            Self::Vault(_) => CaveatName::Vault,
            Self::Permission(_) => CaveatName::Permission,
            Self::ValidRange { .. } => CaveatName::ValidRange,
            Self::MaxPerTx(_) => CaveatName::MaxPerTx,
            Self::AuditWindow { .. } => CaveatName::AuditWindow,
            Self::MaxUses { .. } => CaveatName::MaxUses,
            Self::WrappedOnly { .. } => CaveatName::WrappedOnly,
            Self::Factory(_) => CaveatName::Factory,
            Self::PolicyReference { .. } => CaveatName::PolicyReference,
            Self::ValidAfter { .. } => CaveatName::ValidAfter,
            Self::RedemptionContext { .. } => CaveatName::RedemptionContext,
            Self::Sharded { .. } => CaveatName::Sharded,
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
            Caveat::Vault(id) => serde_json::json!({"type": "vault", "value": hex::encode(id)}),
            Caveat::Permission(k) => serde_json::json!({"type": "permission", "value": k.as_str()}),
            Caveat::ValidRange {
                valid_after_unix,
                valid_until_unix,
            } => serde_json::json!({
                "type": "valid_range",
                "value": {
                    "valid_after_unix": valid_after_unix,
                    "valid_until_unix": valid_until_unix,
                }
            }),
            Caveat::MaxPerTx(amount) => serde_json::json!({"type": "max_per_tx", "value": amount}),
            Caveat::AuditWindow { duration_secs } => {
                serde_json::json!({"type": "audit_window", "value": duration_secs})
            }
            Caveat::MaxUses { count } => serde_json::json!({"type": "max_uses", "value": count}),
            Caveat::WrappedOnly { parent_capability } => {
                serde_json::json!({
                    "type": "wrapped_only",
                    "value": hex::encode(parent_capability),
                })
            }
            Caveat::Factory(vet) => {
                // Canonicalise each constraint via RFC-0964 wire format.
                let pre_conditions: Vec<String> = vet
                    .pre_conditions
                    .iter()
                    .map(|c| hex::encode(encode_constraint(c).expect("constraint encoding")))
                    .collect();
                serde_json::json!({
                    "type": "factory",
                    "value": {
                        "target_vault_id": hex::encode(vet.target_vault_id),
                        "action_template": {
                            "selector": vet.action_template.selector,
                            "args": vet.action_template.args,
                        },
                        "required_caller": vet.required_caller,
                        "pre_conditions": pre_conditions,
                        "expiry_for_deploy_unix": vet.expiry_for_deploy_unix,
                    }
                })
            }
            Caveat::PolicyReference {
                policy_id,
                policy_version_seq,
                attenuation_witness,
            } => {
                serde_json::json!({
                    "type": "policy_reference",
                    "value": {
                        "policy_id": hex::encode(policy_id),
                        "policy_version_seq": policy_version_seq,
                        "attenuation_witness": hex::encode(attenuation_witness),
                    }
                })
            }
            Caveat::ValidAfter { not_before_unix } => {
                serde_json::json!({"type": "valid_after", "value": not_before_unix})
            }
            Caveat::RedemptionContext { context_hash } => {
                serde_json::json!({"type": "redemption_context", "value": hex::encode(context_hash)})
            }
            Caveat::Sharded { shard_id } => {
                serde_json::json!({"type": "sharded", "value": shard_id})
            }
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

    // RFC-0957 §3.5 + 0957-a mission AC: set_subsumes enforces monotonic
    // attenuation — child capability may only narrow, never widen.

    #[test]
    fn subsumes_empty_parent_allows_empty_child() {
        assert!(set_subsumes(&[], &[]));
    }

    #[test]
    fn subsumes_empty_parent_rejects_nonempty_child() {
        let child = vec![Caveat::Model("gpt-4".to_owned())];
        assert!(!set_subsumes(&[], &child));
    }

    #[test]
    fn subsumes_model_requires_same_value() {
        let parent = vec![Caveat::Model("gpt-4".to_owned())];
        assert!(set_subsumes(&parent, &[Caveat::Model("gpt-4".to_owned())]));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::Model("claude-3".to_owned())]
        ));
    }

    #[test]
    fn subsumes_amount_max_narrows() {
        // parent 1000 OCTO-W; child 500 OCTO-W (narrowing) ⇒ child ⊆ parent
        let parent = vec![Caveat::AmountMax(1_000_000_000)];
        assert!(set_subsumes(&parent, &[Caveat::AmountMax(500_000_000)]));
        // child 1500 OCTO-W (widening) ⇒ reject
        assert!(!set_subsumes(&parent, &[Caveat::AmountMax(1_500_000_000)]));
    }

    #[test]
    fn subsumes_before_earlier_in_child() {
        // parent expires at ts 200; child expires at ts 100 (earlier = narrower) ⇒ child ⊆ parent
        let parent = vec![Caveat::Before(200)];
        assert!(set_subsumes(&parent, &[Caveat::Before(100)]));
        // child expires later (ts 300) ⇒ reject (widening)
        assert!(!set_subsumes(&parent, &[Caveat::Before(300)]));
    }

    #[test]
    fn subsumes_provider_subset() {
        let parent = vec![Caveat::Provider(vec![
            "openai".to_owned(),
            "anthropic".to_owned(),
            "google".to_owned(),
        ])];
        let child_narrow = vec![Caveat::Provider(vec![
            "openai".to_owned(),
            "anthropic".to_owned(),
        ])];
        assert!(set_subsumes(&parent, &child_narrow));
        let child_wider = vec![Caveat::Provider(vec![
            "openai".to_owned(),
            "cohere".to_owned(),
        ])];
        assert!(!set_subsumes(&parent, &child_wider));
    }

    #[test]
    fn subsumes_jurisdiction_subset() {
        let parent = vec![Caveat::Jurisdiction(
            ["US".to_owned(), "DE".to_owned()].into_iter().collect(),
        )];
        let child_narrow = vec![Caveat::Jurisdiction(
            ["US".to_owned()].into_iter().collect(),
        )];
        assert!(set_subsumes(&parent, &child_narrow));
        let child_wider = vec![Caveat::Jurisdiction(
            ["US".to_owned(), "FR".to_owned()].into_iter().collect(),
        )];
        assert!(!set_subsumes(&parent, &child_wider));
    }

    #[test]
    fn subsumes_rate_limit_narrows() {
        let parent = vec![Caveat::RateLimit(RateLimit {
            rpm: 100,
            tpm: 10_000,
        })];
        let child_narrow = vec![Caveat::RateLimit(RateLimit {
            rpm: 50,
            tpm: 5_000,
        })];
        assert!(set_subsumes(&parent, &child_narrow));
        let child_wider = vec![Caveat::RateLimit(RateLimit {
            rpm: 200,
            tpm: 5_000,
        })];
        assert!(!set_subsumes(&parent, &child_wider));
    }

    #[test]
    fn subsumes_per_axis_same_axis_narrows() {
        let parent = vec![Caveat::PerAxisMax(PerAxisMax {
            axis: "input_tokens".to_owned(),
            max_per_1k: 1_000,
        })];
        let child = vec![Caveat::PerAxisMax(PerAxisMax {
            axis: "input_tokens".to_owned(),
            max_per_1k: 500,
        })];
        assert!(set_subsumes(&parent, &child));
    }

    #[test]
    fn subsumes_per_axis_different_axis_rejects() {
        let parent = vec![Caveat::PerAxisMax(PerAxisMax {
            axis: "input_tokens".to_owned(),
            max_per_1k: 1_000,
        })];
        let child = vec![Caveat::PerAxisMax(PerAxisMax {
            axis: "output_tokens".to_owned(),
            max_per_1k: 500,
        })];
        assert!(!set_subsumes(&parent, &child));
    }

    #[test]
    fn subsumes_must_match_all_child_caveats() {
        // parent has 2 caveats; child needs both implied
        let parent = vec![Caveat::Model("gpt-4".to_owned()), Caveat::Before(2_000)];
        let child_ok = vec![Caveat::Model("gpt-4".to_owned()), Caveat::Before(1_500)];
        assert!(set_subsumes(&parent, &child_ok));
        let child_missing = vec![Caveat::Model("gpt-4".to_owned()), Caveat::Before(2_500)];
        assert!(!set_subsumes(&parent, &child_missing));
    }

    #[test]
    fn subsumes_extra_parent_caveats_allowed() {
        // Parent can have caveats not present in child — child just runs
        // within parent's full constraint set.
        let parent = vec![Caveat::Model("gpt-4".to_owned()), Caveat::Before(2_000)];
        let child = vec![Caveat::Model("gpt-4".to_owned())];
        assert!(set_subsumes(&parent, &child));
    }

    #[test]
    fn subsumes_audience_must_match() {
        let parent = vec![Caveat::Audience("did:octo:alice".to_owned())];
        assert!(set_subsumes(
            &parent,
            &[Caveat::Audience("did:octo:alice".to_owned())]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::Audience("did:octo:bob".to_owned())]
        ));
    }

    #[test]
    fn subsumes_invocation_hash_bind_must_match() {
        let h = [0xab; 32];
        let parent = vec![Caveat::InvocationHashBind(h)];
        assert!(set_subsumes(&parent, &[Caveat::InvocationHashBind(h)]));
        let different = [0xcd; 32];
        assert!(!set_subsumes(
            &parent,
            &[Caveat::InvocationHashBind(different)]
        ));
    }

    #[test]
    fn subsumes_cache_policy_always_is_most_permissive() {
        // Always ⊇ OptIn ⊇ Off — parent Always allows any child cache policy.
        let parent = vec![Caveat::CacheStrategy(CachePolicy::Always {
            ttl_secs: 3600,
        })];
        assert!(set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::Off)]
        ));
        assert!(set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::OptIn {
                cache_key_hash: None
            })]
        ));
        assert!(set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::Always { ttl_secs: 60 })]
        ));
    }

    #[test]
    fn subsumes_cache_policy_off_is_most_restrictive() {
        // Off only permits Off child — cannot widen to OptIn or Always.
        let parent = vec![Caveat::CacheStrategy(CachePolicy::Off)];
        assert!(set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::Off)]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::OptIn {
                cache_key_hash: None
            })]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::Always { ttl_secs: 60 })]
        ));
    }

    #[test]
    fn subsumes_cache_policy_opt_in_permits_off() {
        let parent = vec![Caveat::CacheStrategy(CachePolicy::OptIn {
            cache_key_hash: None,
        })];
        // OptIn allows Off (no cache) since Off is more restrictive.
        assert!(set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::Off)]
        ));
        // OptIn does not allow Always (Always is less restrictive).
        assert!(!set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::Always { ttl_secs: 60 })]
        ));
    }

    #[test]
    fn subsumes_cache_policy_opt_in_key_match() {
        let key = [0x42; 32];
        let parent = vec![Caveat::CacheStrategy(CachePolicy::OptIn {
            cache_key_hash: Some(key),
        })];
        // child must specify the same key
        assert!(set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::OptIn {
                cache_key_hash: Some(key)
            })]
        ));
        // child without key ⇒ reject (parent bound to specific key)
        assert!(!set_subsumes(
            &parent,
            &[Caveat::CacheStrategy(CachePolicy::OptIn {
                cache_key_hash: None
            })]
        ));
    }

    #[test]
    fn subsumes_raw_matches_name_and_value() {
        let parent = vec![Caveat::Raw(RawCaveat {
            name: "custom".to_owned(),
            value: vec![0x01, 0x02],
        })];
        let child_ok = vec![Caveat::Raw(RawCaveat {
            name: "custom".to_owned(),
            value: vec![0x01, 0x02],
        })];
        let child_bad = vec![Caveat::Raw(RawCaveat {
            name: "custom".to_owned(),
            value: vec![0x03],
        })];
        assert!(set_subsumes(&parent, &child_ok));
        assert!(!set_subsumes(&parent, &child_bad));
    }

    #[test]
    fn subsumes_disjoint_types_reject() {
        // Parent has Model, child has Provider — different caveat types.
        let parent = vec![Caveat::Model("gpt-4".to_owned())];
        let child = vec![Caveat::Provider(vec!["openai".to_owned()])];
        assert!(!set_subsumes(&parent, &child));
    }

    // RFC-0965 §3 — new variant subsumption rules.

    #[test]
    fn subsumes_vault_must_match_id() {
        let v1 = [0x01; 32];
        let v2 = [0x02; 32];
        let parent = vec![Caveat::Vault(v1)];
        assert!(set_subsumes(&parent, &[Caveat::Vault(v1)]));
        assert!(!set_subsumes(&parent, &[Caveat::Vault(v2)]));
    }

    #[test]
    fn subsumes_permission_must_match_kind() {
        let parent = vec![Caveat::Permission(PermissionKind::NativeTokenTransfer)];
        assert!(set_subsumes(
            &parent,
            &[Caveat::Permission(PermissionKind::NativeTokenTransfer)]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::Permission(PermissionKind::Reservation)]
        ));
    }

    #[test]
    fn subsumes_valid_range_narrows() {
        let parent = vec![Caveat::ValidRange {
            valid_after_unix: 100,
            valid_until_unix: 1000,
        }];
        // child narrower on both sides ⇒ accepted
        let child = vec![Caveat::ValidRange {
            valid_after_unix: 200,
            valid_until_unix: 800,
        }];
        assert!(set_subsumes(&parent, &child));
        // child widens on either side ⇒ rejected
        let widens_after = vec![Caveat::ValidRange {
            valid_after_unix: 50,
            valid_until_unix: 800,
        }];
        assert!(!set_subsumes(&parent, &widens_after));
        let widens_until = vec![Caveat::ValidRange {
            valid_after_unix: 200,
            valid_until_unix: 2000,
        }];
        assert!(!set_subsumes(&parent, &widens_until));
    }

    #[test]
    fn subsumes_max_per_tx_narrows() {
        let parent = vec![Caveat::MaxPerTx(1_000)];
        assert!(set_subsumes(&parent, &[Caveat::MaxPerTx(500)]));
        assert!(!set_subsumes(&parent, &[Caveat::MaxPerTx(2_000)]));
    }

    #[test]
    fn subsumes_audit_window_upgrade_resolves_r7_f8() {
        // R7-F8: parent 0 (high-trust, instant) can upgrade to non-zero
        // (auditable). `c_dur >= p_dur` rule admits this.
        let parent = vec![Caveat::AuditWindow { duration_secs: 0 }];
        let child = vec![Caveat::AuditWindow {
            duration_secs: 86400,
        }];
        assert!(set_subsumes(&parent, &child));
        // Reverse: parent 24h, child 0 (releaseaudit) — disallowed (downgrade).
        let reverse_parent = vec![Caveat::AuditWindow {
            duration_secs: 86400,
        }];
        let reverse_child = vec![Caveat::AuditWindow { duration_secs: 0 }];
        assert!(!set_subsumes(&reverse_parent, &reverse_child));
    }

    #[test]
    fn subsumes_max_uses_zero_means_unlimited() {
        let parent = vec![Caveat::MaxUses { count: 0 }];
        // 0 = unlimited: subsumes any count.
        assert!(set_subsumes(&parent, &[Caveat::MaxUses { count: 100 }]));
        assert!(set_subsumes(&parent, &[Caveat::MaxUses { count: 0 }]));
        // Bounded parent: child must be ≤ parent.
        let bounded = vec![Caveat::MaxUses { count: 50 }];
        assert!(set_subsumes(&bounded, &[Caveat::MaxUses { count: 25 }]));
        assert!(!set_subsumes(&bounded, &[Caveat::MaxUses { count: 75 }]));
        // Cannot widen to unlimited via attenuation.
        assert!(!set_subsumes(&bounded, &[Caveat::MaxUses { count: 0 }]));
    }

    #[test]
    fn subsumes_wrapped_only_must_match_parent() {
        let p1 = [0xaa; 32];
        let p2 = [0xbb; 32];
        let parent = vec![Caveat::WrappedOnly {
            parent_capability: p1,
        }];
        assert!(set_subsumes(
            &parent,
            &[Caveat::WrappedOnly {
                parent_capability: p1
            }]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::WrappedOnly {
                parent_capability: p2
            }]
        ));
    }

    #[test]
    fn subsumes_factory_requires_full_equality() {
        let vet1 = FactoryVet {
            target_vault_id: [0x01; 32],
            action_template: ActionTemplate {
                selector: "transfer_v1".to_owned(),
                args: vec!["arg1".to_owned()],
            },
            required_caller: Some("did:octo:caller".to_owned()),
            pre_conditions: vec![Constraint::SingleUse],
            expiry_for_deploy_unix: 1_000_000,
        };
        let mut vet2 = vet1.clone();
        vet2.action_template = ActionTemplate {
            selector: "transfer_v1".to_owned(),
            args: vec!["arg99".to_owned()],
        };
        let parent = vec![Caveat::Factory(vet1.clone())];
        assert!(set_subsumes(&parent, &[Caveat::Factory(vet1)]));
        assert!(!set_subsumes(&parent, &[Caveat::Factory(vet2)]));
    }

    #[test]
    fn subsumes_policy_reference_must_match_hash() {
        let p1 = [0x11; 32];
        let p2 = [0x22; 32];
        let parent = vec![Caveat::PolicyReference {
            policy_id: p1,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        }];
        assert!(set_subsumes(
            &parent,
            &[Caveat::PolicyReference {
                policy_id: p1,
                policy_version_seq: 1,
                attenuation_witness: [0u8; 64],
            }]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::PolicyReference {
                policy_id: p2,
                policy_version_seq: 1,
                attenuation_witness: [0u8; 64],
            }]
        ));
    }

    #[test]
    fn subsumes_valid_after_narrows() {
        let parent = vec![Caveat::ValidAfter {
            not_before_unix: 100,
        }];
        assert!(set_subsumes(
            &parent,
            &[Caveat::ValidAfter {
                not_before_unix: 100
            }]
        ));
        assert!(set_subsumes(
            &parent,
            &[Caveat::ValidAfter {
                not_before_unix: 200
            }]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::ValidAfter {
                not_before_unix: 50
            }]
        ));
    }

    #[test]
    fn subsumes_redemption_context_must_match() {
        let h = [0x42; 32];
        let h2 = [0x99; 32];
        let parent = vec![Caveat::RedemptionContext { context_hash: h }];
        assert!(set_subsumes(
            &parent,
            &[Caveat::RedemptionContext { context_hash: h }]
        ));
        assert!(!set_subsumes(
            &parent,
            &[Caveat::RedemptionContext { context_hash: h2 }]
        ));
    }

    #[test]
    fn subsumes_sharded_must_match_shard_id() {
        let parent = vec![Caveat::Sharded { shard_id: 3 }];
        assert!(set_subsumes(&parent, &[Caveat::Sharded { shard_id: 3 }]));
        assert!(!set_subsumes(&parent, &[Caveat::Sharded { shard_id: 5 }]));
    }

    #[test]
    fn new_variants_canonical_ser_distinct() {
        // Each new variant must produce a distinct canonical_ser output.
        let v1 = Caveat::Vault([0x01; 32]);
        let v2 = Caveat::Permission(PermissionKind::NativeTokenTransfer);
        let v3 = Caveat::ValidRange {
            valid_after_unix: 0,
            valid_until_unix: 100,
        };
        let v4 = Caveat::MaxPerTx(100);
        let v5 = Caveat::AuditWindow { duration_secs: 60 };
        let v6 = Caveat::MaxUses { count: 5 };
        let v7 = Caveat::WrappedOnly {
            parent_capability: [0xab; 32],
        };
        let v8 = Caveat::Factory(FactoryVet {
            target_vault_id: [0x01; 32],
            action_template: ActionTemplate {
                selector: "s".to_owned(),
                args: Vec::new(),
            },
            required_caller: None,
            pre_conditions: vec![],
            expiry_for_deploy_unix: 0,
        });
        let v9 = Caveat::PolicyReference {
            policy_id: [0xff; 32],
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        };
        let serials = [
            v1.canonical_ser(),
            v2.canonical_ser(),
            v3.canonical_ser(),
            v4.canonical_ser(),
            v5.canonical_ser(),
            v6.canonical_ser(),
            v7.canonical_ser(),
            v8.canonical_ser(),
            v9.canonical_ser(),
        ];
        let unique: std::collections::HashSet<_> = serials.iter().collect();
        assert_eq!(
            unique.len(),
            9,
            "RFC-0965 variants must have distinct canonical_ser"
        );
    }

    #[test]
    fn new_variants_caveat_name_distinct() {
        // Each variant name() must yield a distinct CaveatName.
        use std::collections::HashSet;
        let names = vec![
            Caveat::Vault([0; 32]).name(),
            Caveat::Permission(PermissionKind::NativeTokenTransfer).name(),
            Caveat::ValidRange {
                valid_after_unix: 0,
                valid_until_unix: 0,
            }
            .name(),
            Caveat::MaxPerTx(0).name(),
            Caveat::AuditWindow { duration_secs: 0 }.name(),
            Caveat::MaxUses { count: 0 }.name(),
            Caveat::WrappedOnly {
                parent_capability: [0; 32],
            }
            .name(),
            Caveat::Factory(FactoryVet {
                target_vault_id: [0; 32],
                action_template: ActionTemplate {
                    selector: String::new(),
                    args: Vec::new(),
                },
                required_caller: None,
                pre_conditions: vec![],
                expiry_for_deploy_unix: 0,
            })
            .name(),
            Caveat::PolicyReference {
                policy_id: [0; 32],
                policy_version_seq: 0,
                attenuation_witness: [0; 64],
            }
            .name(),
        ];
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), 9);
    }

    #[test]
    fn subsumes_factory_typed_preconditions_must_match_set() {
        // Per RFC-0965 §3.8: Factory subsumption requires full canonical
        // equality of the pre_conditions set (the vet is a signed
        // assertion; cannot change constraints via attenuation).
        let vet_a = FactoryVet {
            target_vault_id: [0x01; 32],
            action_template: ActionTemplate {
                selector: "x".to_owned(),
                args: Vec::new(),
            },
            required_caller: None,
            pre_conditions: vec![Constraint::SingleUse, Constraint::MaxUses { count: 5 }],
            expiry_for_deploy_unix: 1_000_000,
        };
        let parent = vec![Caveat::Factory(vet_a.clone())];
        assert!(set_subsumes(&parent, &[Caveat::Factory(vet_a.clone())]));
        // Different constraint type → no subsume
        let mut vet_b = vet_a.clone();
        vet_b.pre_conditions = vec![Constraint::CallerBound("did:octo:other".to_owned())];
        assert!(!set_subsumes(&parent, &[Caveat::Factory(vet_b)]));
    }

    #[test]
    fn factory_action_template_typed_not_opaque() {
        // action_template is ActionTemplate { selector, args }, not Vec<u8>.
        let t1 = ActionTemplate {
            selector: "transfer".to_owned(),
            args: vec!["100".to_owned(), "alice".to_owned()],
        };
        let t2 = t1.clone();
        assert_eq!(t1, t2);
        let t3 = ActionTemplate {
            selector: "transfer".to_owned(),
            args: vec!["200".to_owned(), "alice".to_owned()],
        };
        assert_ne!(t1, t3);
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

/// `serde_bytes` shim for `[u8; 64]` — hex-string representation.
mod serde_bytes_arr64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<[u8; 64], D::Error> {
        let s = String::deserialize(de)?;
        let v = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if v.len() != 64 {
            return Err(serde::de::Error::custom("expected 64 bytes"));
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(&v);
        Ok(out)
    }
}
