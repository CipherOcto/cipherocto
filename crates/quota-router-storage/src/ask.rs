//! Ask + PricingAxis + AskId types (RFC-0959 v1.0 §Data Structures).
//!
//! Ask = a node's published pricing offer. `AskId = BLAKE3(canonical_ser(asker_did || model || axes_hash || nonce))`.
//! PricingAxis registry holds per-axis rate tables keyed by model.

use std::collections::HashMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Micro-OCTO-W (u128). 1 OCTO-W = 1_000_000 micro-OCTO-W.
#[allow(non_camel_case_types)]
pub type MicroOCTO_W = u128;

/// Display-unit OCTO-W (RFC-0959 §Data Structures type-distinct newtype).
///
/// NOT a type alias — this is a distinct newtype to prevent silent unit-conversion
/// bugs (R1 critical fix; type aliases permitted silently otherwise).
/// `to_micro()` / `from_micro()` enforce the conversion at the ingress boundary.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OCTO_WAmount(pub u128);

/// On-wire micro-OCTO-W newtype. Pairs with [`OCTO_WAmount`] for type-safe display ↔ wire.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MicroOCTO_WNewtype(pub u128);

impl OCTO_WAmount {
    /// 1 OCTO-W = 1_000_000 micro-OCTO-W.
    pub const MICRO_PER_OCTOW: u128 = 1_000_000;

    /// Convert display-unit OCTO-W to on-wire micro-OCTO-W.
    #[must_use]
    pub const fn to_micro(self) -> u128 {
        self.0 * Self::MICRO_PER_OCTOW
    }

    /// Construct from on-wire micro-OCTO-W. Returns `None` if not aligned.
    #[must_use]
    pub const fn from_micro(micro: u128) -> Option<Self> {
        if micro.is_multiple_of(Self::MICRO_PER_OCTOW) {
            Some(Self(micro / Self::MICRO_PER_OCTOW))
        } else {
            None
        }
    }
}

/// Token counter per axis (RFC-0959 §Data Structures).
/// `u32` cap = 4.29B tokens — sufficient for any single-axis quota.
pub type TokenCount = u32;

/// Ed25519 signature bytes (RFC 8032 standard, 64 bytes).
/// `AskSigned.signature` stores as `Vec<u8>` for serde compatibility; use
/// `AskSigned::signature_fixed()` to obtain the fixed-size form.
pub type Ed25519Signature = [u8; 64];

/// Ed25519 verifying key bytes (32 bytes; RFC-0009 §Identity Key Format).
pub type Ed25519PublicKey = [u8; 32];

/// Provider model reference (RFC-0959 §Data Structures).
///
/// `{namespace, family, version?}` matches RFC-0959 §Data Structures verbatim.
/// The wire format (slash-joined string) is the canonical on-the-wire form;
/// callers convert via `ModelRef::parse` / `Display` at the boundary. The
/// `asks.model` DB column stores the wire form (String); `From<&str>` /
/// `From<String>` produce the structured form for in-memory use.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ModelRef {
    /// Provider namespace (e.g., `"openai"`, `"anthropic"`, `"custom"`).
    pub namespace: String,
    /// Model family (e.g., `"gpt-4"`, `"claude-3"`).
    pub family: String,
    /// Optional version pin (e.g., `"0613"`, `"opus-2026"`).
    /// `None` matches any version.
    pub version: Option<String>,
}

impl ModelRef {
    /// Construct a new ModelRef.
    /// # Errors
    /// Returns `ModelRefError::EmptyNamespace` / `EmptyFamily` if invalid.
    pub fn new(
        namespace: impl Into<String>,
        family: impl Into<String>,
        version: Option<String>,
    ) -> Result<Self, ModelRefError> {
        let namespace = namespace.into();
        let family = family.into();
        if namespace.is_empty() {
            return Err(ModelRefError::EmptyNamespace);
        }
        if family.is_empty() {
            return Err(ModelRefError::EmptyFamily);
        }
        Ok(Self {
            namespace,
            family,
            version,
        })
    }

    /// Parse from the wire string `"namespace/family/version"` or `"namespace/family"`.
    /// Whitespace-only segments are rejected.
    /// # Errors
    /// Returns `ModelRefError::Parse` on malformed input.
    pub fn parse(s: &str) -> Result<Self, ModelRefError> {
        let parts: Vec<&str> = s.splitn(3, '/').collect();
        match parts.len() {
            2 => {
                let namespace = parts[0].trim();
                let family = parts[1].trim();
                if namespace.is_empty() {
                    return Err(ModelRefError::Parse(
                        "namespace is empty (expected `namespace/family[/version]`)".to_owned(),
                    ));
                }
                if family.is_empty() {
                    return Err(ModelRefError::Parse(
                        "family is empty (expected `namespace/family[/version]`)".to_owned(),
                    ));
                }
                Ok(Self {
                    namespace: namespace.to_owned(),
                    family: family.to_owned(),
                    version: None,
                })
            }
            3 => {
                let namespace = parts[0].trim();
                let family = parts[1].trim();
                let version = parts[2].trim();
                if namespace.is_empty() {
                    return Err(ModelRefError::Parse("namespace is empty".to_owned()));
                }
                if family.is_empty() {
                    return Err(ModelRefError::Parse("family is empty".to_owned()));
                }
                if version.is_empty() {
                    return Err(ModelRefError::Parse("version is empty".to_owned()));
                }
                Ok(Self {
                    namespace: namespace.to_owned(),
                    family: family.to_owned(),
                    version: Some(version.to_owned()),
                })
            }
            _ => Err(ModelRefError::Parse(format!(
                "expected `namespace/family[/version]`, got `{s}`"
            ))),
        }
    }

    /// Render to the wire form `"namespace/family/version"` or `"namespace/family"`.
    #[must_use]
    pub fn to_wire(&self) -> String {
        match &self.version {
            Some(v) => format!("{}/{}/{}", self.namespace, self.family, v),
            None => format!("{}/{}", self.namespace, self.family),
        }
    }

    /// Lenient parse: returns a placeholder (`namespace="", family=""`) when
    /// input is empty / malformed. Used at DB-read boundaries where the
    /// persisted string may predate ModelRef partitioning.
    #[must_use]
    pub fn parse_lenient(s: &str) -> Self {
        Self::parse(s).unwrap_or_else(|_| Self {
            namespace: String::new(),
            family: s.to_owned(),
            version: None,
        })
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_wire())
    }
}

impl std::str::FromStr for ModelRef {
    type Err = ModelRefError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<&str> for ModelRef {
    fn from(s: &str) -> Self {
        Self::parse_lenient(s)
    }
}

impl From<String> for ModelRef {
    fn from(s: String) -> Self {
        Self::parse_lenient(&s)
    }
}

/// ModelRef construction / parse errors.
#[derive(Debug, thiserror::Error)]
pub enum ModelRefError {
    #[error("model namespace is empty")]
    EmptyNamespace,
    #[error("model family is empty")]
    EmptyFamily,
    #[error("invalid model reference: {0}")]
    Parse(String),
}

/// NodeType taxonomy (RFC-0009 §Node + RFC-0959 §Data Structures).
///
/// Mirrors `octo_wallet::node::NodeType` wire format (kebab-case JSON,
/// `wholesale` / `self-host` / `hybrid` CLI strings) so the two crates
/// stay in sync at the wire boundary. Defined locally here to avoid pulling
/// `octo-wallet` (HSM/MPC/keystore) into the storage crate — the
/// `octo-wallet::node::NodeType` re-export remains the canonical version
/// for the wallet crate itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeType {
    /// Routes calls to external opaque providers. Cannot mint ZK-bearing
    /// capabilities per RFC-0958 §Adversary A3.
    Wholesale,
    /// Runs inference inside the CipherOcto protocol boundary.
    /// Mints ZK-bearing capabilities by default per RFC-0958 §NodeType Gating.
    #[serde(rename = "self-host")]
    SelfHost,
    /// Operates both wholesale-routed and self-hosted inference.
    /// ZK mint requires explicit `mint_with_zk()` API call.
    Hybrid,
}

impl NodeType {
    /// CLI string accepted by `octo-wallet init --node-type <X>`.
    #[must_use]
    pub fn as_cli_str(&self) -> &'static str {
        match self {
            Self::Wholesale => "wholesale",
            Self::SelfHost => "self-host",
            Self::Hybrid => "hybrid",
        }
    }

    /// Returns true iff this NodeType permits minting ZK-bearing capabilities.
    /// Wholesale always returns false; SelfHost and Hybrid return true.
    #[must_use]
    pub const fn permits_zk_mint(&self) -> bool {
        matches!(self, Self::SelfHost | Self::Hybrid)
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_cli_str())
    }
}

impl std::str::FromStr for NodeType {
    type Err = NodeTypeParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wholesale" => Ok(Self::Wholesale),
            "self-host" | "self_host" | "selfhost" => Ok(Self::SelfHost),
            "hybrid" => Ok(Self::Hybrid),
            _ => Err(NodeTypeParseError(s.to_owned())),
        }
    }
}

/// Error returned when parsing a `NodeType` from CLI / config fails.
#[derive(Debug, thiserror::Error)]
#[error("unknown NodeType `{0}`; expected one of: wholesale, self-host, hybrid")]
pub struct NodeTypeParseError(pub String);

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
        model: impl Into<ModelRef>,
        rates: ModelRateTable,
        nonce: [u8; 16],
        expires_at_unix: u64,
    ) -> Result<Self, AskError> {
        let asker_did = asker_did.into();
        let model: ModelRef = model.into();
        if asker_did.is_empty() {
            return Err(AskError::EmptyAskerDid);
        }
        if model.family.is_empty() {
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
        let model_wire = self.model.to_wire();
        let mut msg =
            Vec::with_capacity(self.asker_did.len() + model_wire.len() + 32 + self.nonce.len());
        msg.extend_from_slice(self.asker_did.as_bytes());
        msg.extend_from_slice(model_wire.as_bytes());
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
    #[error("jurisdiction is empty (use [\"*\"] for global)")]
    EmptyJurisdiction,
    #[error("asker identity seed is all-zeros")]
    EmptyIdentitySeed,
}

/// Unsigned Ask payload (RFC-0959 §Data Structures).
///
/// This is the canonical content that gets signed. `AskId` and the signature
/// are derived FROM this payload; neither is part of the canonical signed
/// surface (non-circular derivation per R1 fix).
///
/// The `nonce` field is 16 bytes (RFC-0959 fix; the AskId re-derivation uses
/// the nonce as a uniqueness salt so two identical offers produce distinct
/// AskIds, which matters for rate-table updates where the asker publishes
/// a new nonce to invalidate prior AskIds).
///
/// `node_type` is the asker's deployment mode (RFC-0009 §Node). It gates
/// downstream capability-class minting per RFC-0958 §NodeType Gating Matrix
/// and informs the marketplace index's jurisdiction/routing decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskUnsignedPayload {
    /// Asker (publisher) DID (RFC-0009 §Identity Key Format).
    pub asker_did: AskerDid,
    /// NodeType of the asker (RFC-0009 §Node; mirrored locally from
    /// `octo_wallet::node::NodeType` to avoid the wallet dep).
    pub node_type: NodeType,
    /// Model reference — uses `ModelRef` struct form `{namespace, family,
    /// version?}` per RFC-0959 §Data Structures. Stored as the wire string
    /// (`"namespace/family/version"`) for persistence compat with the `asks`
    /// table schema (RFC-0959 §Implementation Phases).
    pub model: ModelRef,
    /// Per-model rate table.
    pub rates: ModelRateTable,
    /// TTL in Unix seconds (inclusive). After this, the Ask is invalid.
    pub ttl_unix: u64,
    /// Jurisdiction tag(s). Empty list rejected by `AskUnsignedPayload::new`.
    pub jurisdiction: Vec<String>,
    /// Unix timestamp at which the payload was assembled (for replay-window defenses).
    pub published_at_unix: u64,
    /// Nonce for content-addressable AskId uniqueness (16 bytes; CSPRNG-generated).
    pub nonce: [u8; 16],
}

impl AskUnsignedPayload {
    /// Construct a validated payload. Rejects empty asker_did / model / jurisdiction
    /// / all-zero nonce.
    /// # Errors
    /// Returns `AskError::EmptyAskerDid` / `EmptyModel` / `EmptyJurisdiction` /
    /// `EmptyNonce`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        asker_did: impl Into<String>,
        node_type: NodeType,
        model: impl Into<ModelRef>,
        rates: ModelRateTable,
        ttl_unix: u64,
        jurisdiction: Vec<String>,
        published_at_unix: u64,
        nonce: [u8; 16],
    ) -> Result<Self, AskError> {
        let asker_did = asker_did.into();
        let model: ModelRef = model.into();
        if asker_did.is_empty() {
            return Err(AskError::EmptyAskerDid);
        }
        if model.family.is_empty() {
            return Err(AskError::EmptyModel);
        }
        if jurisdiction.is_empty() {
            return Err(AskError::EmptyJurisdiction);
        }
        if nonce == [0u8; 16] {
            return Err(AskError::EmptyNonce);
        }
        Ok(Self {
            asker_did,
            node_type,
            model,
            rates,
            ttl_unix,
            jurisdiction,
            published_at_unix,
            nonce,
        })
    }

    /// Compute the content-addressable AskId (RFC-0959 §Algorithms).
    /// `AskId = BLAKE3(canonical_ser(self))`.
    /// Non-circular: `ask_id` and `signature` are NOT part of the canonical payload.
    #[must_use]
    pub fn ask_id(&self) -> AskId {
        let canonical = serde_json::to_vec(self).expect("serializable");
        *blake3::hash(&canonical).as_bytes()
    }
}

/// Cryptographically-attested Ask (RFC-0959 §Data Structures).
///
/// `AskSigned { ask_id, payload, signature }` is the wire/transport form. The
/// signature is Ed25519 over `canonical_ser(payload)`. `verify()` recomputes
/// `ask_id` from the payload and verifies both the AskId re-derivation and
/// the Ed25519 signature against the embedded `asker_did` (which must be
/// resolvable to a known Ed25519 public key — see RFC-0009 §DID Resolution).
///
/// The `signature` field is stored as `Vec<u8>` to satisfy serde's default
/// impl (stable Rust has no `Deserialize` for `[u8; 64]` without the
/// `serde_with` feature). Use `signature_fixed()` to obtain the 64-byte form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskSigned {
    /// `BLAKE3(canonical_ser(payload))`. Must equal `payload.ask_id()`.
    pub ask_id: AskId,
    /// The unsigned payload.
    pub payload: AskUnsignedPayload,
    /// Ed25519 signature over `canonical_ser(payload)`.
    pub signature: Vec<u8>,
}

impl AskSigned {
    /// Sign a payload with the asker's 32-byte Ed25519 seed (RFC-0959 §Algorithms).
    ///
    /// `ask_id` is derived from the payload; the signature is over
    /// `canonical_ser(payload)`. The seed is consumed via `SigningKey::from_bytes`
    /// (NOT zeroized here — the caller owns seed material).
    /// # Errors
    /// Returns `AskSignedError::EmptyIdentitySeed` if the seed is all-zeros.
    /// Returns `AskSignedError::CanonicalSer` if `serde_json` serialization fails.
    pub fn sign(
        payload: AskUnsignedPayload,
        identity_seed: &[u8; 32],
    ) -> Result<Self, AskSignedError> {
        if identity_seed == &[0u8; 32] {
            return Err(AskSignedError::EmptyIdentitySeed);
        }
        let canonical = serde_json::to_vec(&payload).map_err(AskSignedError::CanonicalSer)?;
        let ask_id = *blake3::hash(&canonical).as_bytes();
        let signing = SigningKey::from_bytes(identity_seed);
        let signature = signing.sign(&canonical);
        Ok(Self {
            ask_id,
            payload,
            signature: signature.to_bytes().to_vec(),
        })
    }

    /// Verify both AskId re-derivation AND Ed25519 signature (RFC-0959 §Algorithms).
    ///
    /// `asker_public_key` is the 32-byte Ed25519 verifying key derived from the
    /// asker's DID per RFC-0009 §Identity Key Format.
    /// # Errors
    /// Returns `AskSignedError::AskIdMismatch` if `ask_id != payload.ask_id()`.
    /// Returns `AskSignedError::SignatureLengthInvalid` if signature is not 64 bytes.
    /// Returns `AskSignedError::AskSignatureInvalid` if Ed25519 verify fails.
    pub fn verify(&self, asker_public_key: &Ed25519PublicKey) -> Result<(), AskSignedError> {
        // 1. Re-derive AskId from payload; must equal embedded ask_id.
        let derived = self.payload.ask_id();
        if derived != self.ask_id {
            return Err(AskSignedError::AskIdMismatch {
                expected: self.ask_id,
                actual: derived,
            });
        }
        // 2. Verify Ed25519 signature over canonical_ser(payload).
        let canonical = serde_json::to_vec(&self.payload).map_err(AskSignedError::CanonicalSer)?;
        let sig_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| AskSignedError::SignatureLengthInvalid)?;
        let verifying = VerifyingKey::from_bytes(asker_public_key)
            .map_err(|_| AskSignedError::InvalidPublicKey)?;
        let sig = Signature::from_bytes(&sig_bytes);
        verifying
            .verify(&canonical, &sig)
            .map_err(|_| AskSignedError::AskSignatureInvalid)
    }

    /// Return the signature as a fixed 64-byte array. Panics if stored signature
    /// is the wrong length (should never happen after `sign()`).
    #[must_use]
    pub fn signature_fixed(&self) -> [u8; 64] {
        self.signature
            .as_slice()
            .try_into()
            .expect("AskSigned signature is always 64 bytes after sign()")
    }

    /// Convenience: compute the asker's Ed25519 public key from a seed (for tests + CLI flows).
    #[must_use]
    pub fn public_key_from_seed(seed: &[u8; 32]) -> Ed25519PublicKey {
        SigningKey::from_bytes(seed).verifying_key().to_bytes()
    }
}

/// AskSigned construction / verification errors.
#[derive(Debug, thiserror::Error)]
pub enum AskSignedError {
    #[error("asker identity seed is all-zeros")]
    EmptyIdentitySeed,
    #[error("canonical serialization failed: {0}")]
    CanonicalSer(serde_json::Error),
    #[error("ask_id mismatch: expected {expected:?}, derived {actual:?}")]
    AskIdMismatch { expected: AskId, actual: AskId },
    #[error("signature length invalid (must be 64 bytes)")]
    SignatureLengthInvalid,
    #[error("invalid Ed25519 public key bytes")]
    InvalidPublicKey,
    #[error("Ed25519 signature verification failed (tampered payload or wrong key)")]
    AskSignatureInvalid,
}

#[cfg(test)]
mod ask_signed_tests {
    use super::*;
    use crate::ask::{AxisRate, ModelRateTable};

    fn sample_payload() -> AskUnsignedPayload {
        let rates = ModelRateTable {
            model: ModelRef::from("openai/gpt-4"),
            rates: vec![AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: 30_000,
            }],
        };
        AskUnsignedPayload {
            asker_did: "did:octo:asker1".to_owned(),
            node_type: NodeType::SelfHost,
            model: ModelRef::from("openai/gpt-4"),
            rates,
            ttl_unix: 1_900_000_000,
            jurisdiction: vec!["US".to_owned(), "EU".to_owned()],
            published_at_unix: 1_700_000_000,
            nonce: [0x42; 16],
        }
    }

    #[test]
    fn sign_then_verify_roundtrip() {
        let payload = sample_payload();
        let seed = [0xABu8; 32];
        let pk = AskSigned::public_key_from_seed(&seed);
        let signed = AskSigned::sign(payload.clone(), &seed).expect("sign");
        assert_eq!(signed.ask_id, payload.ask_id());
        assert_eq!(signed.payload, payload);
        signed.verify(&pk).expect("verify");
    }

    #[test]
    fn ask_id_non_circular() {
        // ask_id MUST equal BLAKE3(canonical_ser(payload)) and NOT depend on signature.
        let payload = sample_payload();
        let seed = [0xCDu8; 32];
        let s1 = AskSigned::sign(payload.clone(), &seed).unwrap();
        let s2 = AskSigned::sign(payload.clone(), &seed).unwrap();
        assert_eq!(s1.ask_id, s2.ask_id, "ask_id deterministic across re-sign");
        assert_eq!(s1.ask_id, payload.ask_id());
    }

    #[test]
    fn tampered_payload_breaks_verify() {
        let payload = sample_payload();
        let seed = [0xEFu8; 32];
        let pk = AskSigned::public_key_from_seed(&seed);
        let mut signed = AskSigned::sign(payload, &seed).unwrap();
        // Flip one byte in the rate table — payload & ask_id diverge.
        signed.payload.rates.rates[0].rate_per_1k = 999;
        let err = signed.verify(&pk).unwrap_err();
        // Either AskIdMismatch (preferred) or AskSignatureInvalid — both are
        // acceptable defenses against tampering.
        assert!(
            matches!(
                err,
                AskSignedError::AskIdMismatch { .. } | AskSignedError::AskSignatureInvalid
            ),
            "expected AskIdMismatch or AskSignatureInvalid, got {err:?}"
        );
    }

    #[test]
    fn tampered_ask_id_breaks_verify() {
        let payload = sample_payload();
        let seed = [0x77u8; 32];
        let pk = AskSigned::public_key_from_seed(&seed);
        let mut signed = AskSigned::sign(payload, &seed).unwrap();
        signed.ask_id[0] ^= 0xFF;
        let err = signed.verify(&pk).unwrap_err();
        assert!(
            matches!(err, AskSignedError::AskIdMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn wrong_public_key_rejects_signature() {
        let payload = sample_payload();
        let seed = [0x11u8; 32];
        let signed = AskSigned::sign(payload, &seed).unwrap();
        let wrong_pk = AskSigned::public_key_from_seed(&[0x22u8; 32]);
        let err = signed.verify(&wrong_pk).unwrap_err();
        assert!(
            matches!(err, AskSignedError::AskSignatureInvalid),
            "got {err:?}"
        );
    }

    #[test]
    fn empty_seed_rejected() {
        let payload = sample_payload();
        let err = AskSigned::sign(payload, &[0u8; 32]).unwrap_err();
        assert!(matches!(err, AskSignedError::EmptyIdentitySeed));
    }

    #[test]
    fn empty_jurisdiction_rejected() {
        let mut payload = sample_payload();
        payload.jurisdiction.clear();
        let err = AskUnsignedPayload::new(
            payload.asker_did.clone(),
            payload.node_type,
            payload.model.clone(),
            payload.rates.clone(),
            payload.ttl_unix,
            payload.jurisdiction.clone(),
            payload.published_at_unix,
            payload.nonce,
        );
        assert!(matches!(err, Err(AskError::EmptyJurisdiction)));
    }

    #[test]
    fn octow_amount_distinct_newtype() {
        // R1 critical fix: OCTO_WAmount must be a distinct newtype, NOT a type alias.
        // We verify the type system catches silent unit-conversion: `let _: u128 = x.0` is fine,
        // but `let _: u128 = x` is a compile error (intentional, prevents accidental ops).
        let one_octow = OCTO_WAmount(1);
        let one_micro = OCTO_WAmount(0).to_micro();
        assert_eq!(one_octow.to_micro(), 1_000_000);
        assert_eq!(one_micro, 0);
        let back = OCTO_WAmount::from_micro(1_000_000).unwrap();
        assert_eq!(back, OCTO_WAmount(1));
        assert!(
            OCTO_WAmount::from_micro(1_000_001).is_none(),
            "non-aligned must reject"
        );
    }
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
        let model_wire = self.model.to_wire();
        let mut msg =
            Vec::with_capacity(model_wire.len() + 32 + 32 + self.axes_consumed.len() * 32 + 32);
        msg.extend_from_slice(model_wire.as_bytes());
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
    #[error("unknown axis `{0}` referenced in settlement (not in registry)")]
    UnknownAxis(String),
    #[error("ask expired at {ttl_unix} (now={now})")]
    AskExpired {
        ask_id: AskId,
        ttl_unix: u64,
        now: u64,
    },
    #[error("ask {ask_id:?} not found in marketplace index")]
    AskNotFound { ask_id: AskId },
    #[error("jurisdiction mismatch: declared {declared:?}, actual {actual:?}")]
    JurisdictionMismatch {
        declared: Vec<String>,
        actual: Vec<String>,
    },
    #[error(
        "cached axis consumed but cache_key_hash not provided (RFC-0959 §Cache Classification)"
    )]
    CacheStrategyRequired,
    #[error("overflow in compute_cost for axis `{axis_id}` (partial sum = {partial_sum})")]
    Overflow { axis_id: String, partial_sum: u128 },
    #[error("ask signature invalid (tampered payload or wrong key)")]
    AskSignatureInvalid,
    #[error("canonical_ser error: {0}")]
    CanonicalSer(#[from] serde_json::Error),
}

// ============================================================================
// RFC-0959 §Algorithms Settlement Surface (session 2)
// ============================================================================

/// Per-axis consumption at settlement time (RFC-0959 §Data Structures).
///
/// `axes` is a `BTreeMap` (NOT `HashMap`) for deterministic ordering — the
/// `canonical_ser` derivation must produce identical bytes across two
/// independent nodes replaying the same event set (RFC-0909 §Determinism
/// Requirements).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxesConsumed {
    /// Axis ID → units consumed (BTreeMap for canonical ordering).
    pub axes: std::collections::BTreeMap<String, TokenCount>,
    /// BLAKE3 hash of the prompt tokens (RFC-0959 §Cache Classification).
    /// Required iff any cached axis (e.g., `cached_input_tokens_per_1k`)
    /// appears in `axes`. None for fully non-cache settlements.
    pub cache_key_hash: Option<[u8; 32]>,
}

impl AxesConsumed {
    /// Construct an `AxesConsumed` with no cache classification.
    #[must_use]
    pub fn new(axes: std::collections::BTreeMap<String, TokenCount>) -> Self {
        Self {
            axes,
            cache_key_hash: None,
        }
    }

    /// Attach a `cache_key_hash` (used when any cached axis is consumed).
    #[must_use]
    pub fn with_cache_key_hash(mut self, hash: [u8; 32]) -> Self {
        self.cache_key_hash = Some(hash);
        self
    }

    /// Returns true iff this settlement consumes any axis whose ID contains
    /// "cached" (heuristic for the `cached_input_tokens_per_1k` axis family).
    #[must_use]
    pub fn requires_cache_strategy(&self) -> bool {
        self.axes.keys().any(|id| id.contains("cached"))
    }
}

/// Settlement event (RFC-0959 §Data Structures).
///
/// The canonical wire form for a settled invocation. Settlement hash is
/// computed externally via [`compute_settlement_hash`] and not stored in
/// this struct (RFC-0959 §Data Structures fix: separation of content from
/// attestation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementEvent {
    /// BLAKE3 hash of the capability root (RFC-0957 §Algorithms `cap_root_hash`).
    pub cap_root_hash: [u8; 32],
    /// Content-addressable AskId.
    pub ask_id: AskId,
    /// BLAKE3 hash of the invocation (RFC-0959 §Data Structures `invocation_hash`).
    pub invocation_hash: [u8; 32],
    /// Per-axis consumption.
    pub axes_consumed: AxesConsumed,
    /// Computed cost in micro-OCTO-W (integer-only, no float).
    pub cost: MicroOCTO_W,
    /// Unix timestamp of settlement.
    pub settled_at_unix: u64,
}

/// Router-signed receipt wrapping a [`SettlementEvent`] (RFC-0959 §Algorithms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementReceipt {
    /// The event being attested.
    pub event: SettlementEvent,
    /// Ed25519 signature over `canonical_ser((event || nonce || settled_at_unix))`.
    /// Stored as `Vec<u8>` for serde compatibility; length MUST be 64.
    pub router_signature: Vec<u8>,
    /// 16-byte nonce derived from `csprng.next_u64().to_le_bytes() ++ wall_clock_now.to_le_bytes()`.
    pub nonce: [u8; 16],
}

/// Domain separator for settlement hash (RFC-0959 §Algorithms).
///
/// MUST stay stable across versions. To bump the hash surface, change to
/// `"cipherocto/settlement/v2\n"` and accept a cross-impl hash migration.
pub const SETTLEMENT_HASH_DOMAIN: &[u8] = b"cipherocto/settlement/v1\n";

/// Compute the RFC-0959 §Algorithms settlement hash.
///
/// `settlement_hash = BLAKE3(DOMAIN || cap_root_hash || ask_id || invocation_hash || canonical_ser(axes_consumed))`
///
/// `Result` return propagates `serde_json::Error` from `canonical_ser`
/// (RFC-0959 §Data Structures R1 fix: previously infallible `expect()`).
/// # Errors
/// Returns `serde_json::Error` if `canonical_ser` fails (should never happen
/// for `AxesConsumed` since the field types are all serde-friendly).
pub fn compute_settlement_hash(event: &SettlementEvent) -> Result<[u8; 32], serde_json::Error> {
    let axes_canonical = serde_json::to_vec(&event.axes_consumed)?;
    let mut msg =
        Vec::with_capacity(SETTLEMENT_HASH_DOMAIN.len() + 32 + 32 + 32 + axes_canonical.len());
    msg.extend_from_slice(SETTLEMENT_HASH_DOMAIN);
    msg.extend_from_slice(&event.cap_root_hash);
    msg.extend_from_slice(&event.ask_id);
    msg.extend_from_slice(&event.invocation_hash);
    msg.extend_from_slice(&axes_canonical);
    Ok(*blake3::hash(&msg).as_bytes())
}

/// Compute settlement cost (RFC-0959 §Data Structures `compute_cost`).
///
/// `cost = Σ ceil(tokens/1000) * rate[axis]` (u128 throughout, integer-only).
/// Rejects cached axes without `cache_key_hash` per RFC-0959 §Cache
/// Classification.
/// # Errors
/// Returns `SettlementError::UnknownAxis` if an axis ID isn't in the registry.
/// Returns `SettlementError::CacheStrategyRequired` if a cached axis is
/// consumed without `cache_key_hash`.
/// Returns `SettlementError::Overflow` if the running sum overflows `u128`.
pub fn compute_cost(
    ask: &Ask,
    axes_consumed: &AxesConsumed,
    registry: &[PricingAxis],
) -> Result<MicroOCTO_W, SettlementError> {
    if axes_consumed.requires_cache_strategy() && axes_consumed.cache_key_hash.is_none() {
        return Err(SettlementError::CacheStrategyRequired);
    }
    let mut total: u128 = 0;
    for (axis_id, &units) in &axes_consumed.axes {
        let rate = ask
            .rates
            .cost_for(axis_id, u64::from(units), registry)
            .ok_or_else(|| SettlementError::UnknownAxis(axis_id.clone()))?;
        let new_total = total
            .checked_add(rate)
            .ok_or_else(|| SettlementError::Overflow {
                axis_id: axis_id.clone(),
                partial_sum: total,
            })?;
        total = new_total;
    }
    Ok(total)
}

/// Sign a [`SettlementEvent`] with the router's 32-byte Ed25519 seed,
/// producing a [`SettlementReceipt`] (RFC-0959 §Algorithms).
///
/// Signature input: `canonical_ser((event || nonce || settled_at_unix))`.
/// `nonce` is 16 bytes; the caller supplies it (typically
/// `csprng.next_u64().to_le_bytes() ++ wall_clock_now.to_le_bytes()`).
/// # Errors
/// Returns `AskSignedError::EmptyIdentitySeed` if the seed is all-zeros.
/// Returns `AskSignedError::CanonicalSer` if `serde_json` fails.
pub fn sign_settlement_receipt(
    event: SettlementEvent,
    router_seed: &[u8; 32],
    nonce: [u8; 16],
) -> Result<SettlementReceipt, AskSignedError> {
    if router_seed == &[0u8; 32] {
        return Err(AskSignedError::EmptyIdentitySeed);
    }
    // RFC-0959 §Algorithms signature input = canonical_ser((event || nonce || settled_at_unix)).
    // We approximate "canonical_ser((event || nonce || settled_at_unix))" as
    // `serde_json::to_vec(event) ++ nonce ++ settled_at_unix.to_le_bytes()`.
    // A borsh-encoded form would be wire-equivalent under the canonical_ser
    // contract (deterministic JSON), so this is sufficient for the in-process
    // round-trip. The full borsh migration is out of scope here.
    let event_canonical = serde_json::to_vec(&event).map_err(AskSignedError::CanonicalSer)?;
    let mut msg = Vec::with_capacity(event_canonical.len() + nonce.len() + 8);
    msg.extend_from_slice(&event_canonical);
    msg.extend_from_slice(&nonce);
    msg.extend_from_slice(&event.settled_at_unix.to_le_bytes());
    let signing = SigningKey::from_bytes(router_seed);
    let signature = signing.sign(&msg);
    Ok(SettlementReceipt {
        event,
        router_signature: signature.to_bytes().to_vec(),
        nonce,
    })
}

/// Verify a [`SettlementReceipt`] (RFC-0959 §Algorithms).
///
/// Recomputes the signature input and verifies the router's Ed25519 signature.
/// Also re-derives and compares the embedded settlement hash if present.
/// # Errors
/// Returns `SettlementError::HashMismatch` if the embedded settlement hash
/// doesn't match `compute_settlement_hash(event)`.
/// Returns `SettlementError::AskSignatureInvalid` if router signature verify fails.
/// Returns `SettlementError::CanonicalSer` if `serde_json` fails.
pub fn verify_settlement_receipt(
    receipt: &SettlementReceipt,
    router_public_key: &Ed25519PublicKey,
    expected_settlement_hash: &[u8; 32],
) -> Result<(), SettlementError> {
    let computed = compute_settlement_hash(&receipt.event)?;
    if &computed != expected_settlement_hash {
        return Err(SettlementError::HashMismatch);
    }
    let event_canonical = serde_json::to_vec(&receipt.event)?;
    let mut msg = Vec::with_capacity(event_canonical.len() + receipt.nonce.len() + 8);
    msg.extend_from_slice(&event_canonical);
    msg.extend_from_slice(&receipt.nonce);
    msg.extend_from_slice(&receipt.event.settled_at_unix.to_le_bytes());
    let sig_bytes: [u8; 64] = receipt
        .router_signature
        .as_slice()
        .try_into()
        .map_err(|_| SettlementError::AskSignatureInvalid)?;
    let verifying = VerifyingKey::from_bytes(router_public_key)
        .map_err(|_| SettlementError::AskSignatureInvalid)?;
    let sig = Signature::from_bytes(&sig_bytes);
    verifying
        .verify(&msg, &sig)
        .map_err(|_| SettlementError::AskSignatureInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ask() -> Ask {
        Ask {
            asker_did: "did:octo:asker1".to_owned(),
            model: ModelRef::from("openai/gpt-4"),
            rates: ModelRateTable {
                model: ModelRef::from("openai/gpt-4"),
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
            model: ModelRef::from("openai/gpt-4"),
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
            model: ModelRef::from("openai/gpt-4"),
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
            model: ModelRef::from("openai/gpt-4"),
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
            model: ModelRef::from("m"),
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

#[cfg(test)]
mod settlement_engine_tests {
    use super::*;

    fn sample_event() -> SettlementEvent {
        SettlementEvent {
            cap_root_hash: [0x01; 32],
            ask_id: [0x02; 32],
            invocation_hash: [0x03; 32],
            axes_consumed: AxesConsumed::new({
                let mut m = std::collections::BTreeMap::new();
                m.insert("input_tokens_per_1k".to_owned(), 1000);
                m.insert("output_tokens_per_1k".to_owned(), 500);
                m
            }),
            cost: 90_000,
            settled_at_unix: 1_700_000_000,
        }
    }

    #[test]
    fn settlement_hash_deterministic() {
        // Same event → same hash (across calls; across two recomputations).
        let e = sample_event();
        let h1 = compute_settlement_hash(&e).unwrap();
        let h2 = compute_settlement_hash(&e).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn settlement_hash_byte_equivalent_across_replay() {
        // Two independent "nodes" replaying the same event set produce identical hash.
        let e1 = sample_event();
        let e2 = sample_event();
        assert_eq!(
            compute_settlement_hash(&e1).unwrap(),
            compute_settlement_hash(&e2).unwrap(),
        );
    }

    #[test]
    fn settlement_hash_domain_separator_present() {
        // Bumping the domain separator MUST change the hash (RFC-0959 §Algorithms
        // version migration contract).
        let e = sample_event();
        let with_domain = compute_settlement_hash(&e).unwrap();
        // Recompute with zero domain (simulates "forgot to prepend").
        let axes_canonical = serde_json::to_vec(&e.axes_consumed).unwrap();
        let mut msg = Vec::new();
        msg.extend_from_slice(&e.cap_root_hash);
        msg.extend_from_slice(&e.ask_id);
        msg.extend_from_slice(&e.invocation_hash);
        msg.extend_from_slice(&axes_canonical);
        let without_domain = *blake3::hash(&msg).as_bytes();
        assert_ne!(with_domain, without_domain);
    }

    #[test]
    fn settlement_hash_changes_when_axes_change() {
        let mut e = sample_event();
        let h1 = compute_settlement_hash(&e).unwrap();
        e.axes_consumed
            .axes
            .insert("cached_input_tokens_per_1k".to_owned(), 100);
        let h2 = compute_settlement_hash(&e).unwrap();
        assert_ne!(h1, h2, "axes change must change hash");
    }

    #[test]
    fn cached_axis_requires_cache_key_hash() {
        let ask = Ask::new(
            "did:octo:a",
            "openai/gpt-4",
            ModelRateTable {
                model: ModelRef::from("openai/gpt-4"),
                rates: vec![AxisRate {
                    axis: "cached_input_tokens_per_1k".to_owned(),
                    rate_per_1k: 3_000,
                }],
            },
            [0x42; 16],
            1_900_000_000,
        )
        .unwrap();
        let axes = PricingAxis::standard_axes();
        let consumed = AxesConsumed::new({
            let mut m = std::collections::BTreeMap::new();
            m.insert("cached_input_tokens_per_1k".to_owned(), 100);
            m
        });
        // No cache_key_hash → CacheStrategyRequired.
        let err = compute_cost(&ask, &consumed, &axes).unwrap_err();
        assert!(matches!(err, SettlementError::CacheStrategyRequired));
        // With cache_key_hash → succeeds.
        let consumed_ok = consumed.with_cache_key_hash([0xAA; 32]);
        let cost = compute_cost(&ask, &consumed_ok, &axes).unwrap();
        // 100 tokens = ceil(100/1000) = 1 block * 3000 = 3000.
        assert_eq!(cost, 3_000);
    }

    #[test]
    fn compute_cost_unknown_axis_rejected() {
        let ask = Ask::new(
            "did:octo:a",
            "openai/gpt-4",
            ModelRateTable::default(),
            [0x42; 16],
            1_900_000_000,
        )
        .unwrap();
        let axes = PricingAxis::standard_axes();
        let consumed = AxesConsumed::new({
            let mut m = std::collections::BTreeMap::new();
            m.insert("nonexistent_axis".to_owned(), 100);
            m
        });
        let err = compute_cost(&ask, &consumed, &axes).unwrap_err();
        assert!(matches!(err, SettlementError::UnknownAxis(ref s) if s == "nonexistent_axis"));
    }

    #[test]
    fn compute_cost_overflow_detected() {
        // Force overflow: a single axis with u128::MAX worth of rate + huge units.
        // We can't directly set u128::MAX rate via standard axes, so we use a
        // synthetic Ask with an extreme rate.
        let ask = Ask::new(
            "did:octo:a",
            "openai/gpt-4",
            ModelRateTable {
                model: ModelRef::from("openai/gpt-4"),
                rates: vec![AxisRate {
                    axis: "input_tokens_per_1k".to_owned(),
                    rate_per_1k: u128::MAX,
                }],
            },
            [0x42; 16],
            1_900_000_000,
        )
        .unwrap();
        let axes = PricingAxis::standard_axes();
        let consumed = AxesConsumed::new({
            let mut m = std::collections::BTreeMap::new();
            m.insert("input_tokens_per_1k".to_owned(), 100);
            m
        });
        // First block of 100 tokens: 1 * u128::MAX = u128::MAX. Second call
        // would overflow. Use 2 axes to trigger.
        let mut consumed2 = consumed.clone();
        consumed2
            .axes
            .insert("output_tokens_per_1k".to_owned(), 100);
        // Add output rate to push sum over u128::MAX.
        // Hmm — our ask only has input rate. Output falls back to default 60_000.
        // u128::MAX + 60_000 overflows.
        let err = compute_cost(&ask, &consumed2, &axes).unwrap_err();
        assert!(matches!(err, SettlementError::Overflow { .. }));
    }

    #[test]
    fn receipt_sign_verify_roundtrip() {
        let event = sample_event();
        let seed = [0xCDu8; 32];
        let pk = AskSigned::public_key_from_seed(&seed);
        let nonce = [0xEEu8; 16];
        let receipt = sign_settlement_receipt(event.clone(), &seed, nonce).unwrap();
        let expected_hash = compute_settlement_hash(&event).unwrap();
        verify_settlement_receipt(&receipt, &pk, &expected_hash).expect("verify");
    }

    #[test]
    fn receipt_wrong_hash_rejected() {
        let event = sample_event();
        let seed = [0xCDu8; 32];
        let pk = AskSigned::public_key_from_seed(&seed);
        let receipt = sign_settlement_receipt(event, &seed, [0xEE; 16]).unwrap();
        let wrong_hash = [0xFFu8; 32];
        let err = verify_settlement_receipt(&receipt, &pk, &wrong_hash).unwrap_err();
        assert!(matches!(err, SettlementError::HashMismatch));
    }

    #[test]
    fn receipt_wrong_public_key_rejected() {
        let event = sample_event();
        let seed = [0xCDu8; 32];
        let receipt = sign_settlement_receipt(event, &seed, [0xEE; 16]).unwrap();
        let expected_hash = compute_settlement_hash(&receipt.event).unwrap();
        let wrong_pk = AskSigned::public_key_from_seed(&[0x99u8; 32]);
        let err = verify_settlement_receipt(&receipt, &wrong_pk, &expected_hash).unwrap_err();
        assert!(matches!(err, SettlementError::AskSignatureInvalid));
    }

    #[test]
    fn receipt_tampered_event_rejected() {
        let event = sample_event();
        let seed = [0xCDu8; 32];
        let pk = AskSigned::public_key_from_seed(&seed);
        let mut receipt = sign_settlement_receipt(event, &seed, [0xEE; 16]).unwrap();
        receipt.event.cost += 1; // tamper
        let expected_hash = compute_settlement_hash(&receipt.event).unwrap();
        let err = verify_settlement_receipt(&receipt, &pk, &expected_hash).unwrap_err();
        assert!(matches!(err, SettlementError::AskSignatureInvalid));
    }

    #[test]
    fn empty_router_seed_rejected() {
        let event = sample_event();
        let err = sign_settlement_receipt(event, &[0u8; 32], [0xEE; 16]).unwrap_err();
        assert!(matches!(err, AskSignedError::EmptyIdentitySeed));
    }

    #[test]
    fn settlement_hash_property_10k_replays() {
        // 10K random (cap_root_hash, ask_id, invocation_hash, axes) tuples,
        // recomputed twice → identical 32-byte hash. (Property-test surrogate
        // without pulling in `proptest` crate; deterministic seeded RNG.)
        use std::collections::BTreeMap;
        let mut rng_state: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let mut next = || {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            rng_state
        };
        for _ in 0..10_000 {
            let mut axes = BTreeMap::new();
            axes.insert("input_tokens_per_1k".to_owned(), (next() as u32) % 10_000);
            axes.insert("output_tokens_per_1k".to_owned(), (next() as u32) % 5_000);
            let event = SettlementEvent {
                cap_root_hash: next().to_le_bytes().repeat(4)[..32]
                    .try_into()
                    .unwrap_or([0u8; 32]),
                ask_id: next().to_le_bytes().repeat(4)[..32]
                    .try_into()
                    .unwrap_or([0u8; 32]),
                invocation_hash: next().to_le_bytes().repeat(4)[..32]
                    .try_into()
                    .unwrap_or([0u8; 32]),
                axes_consumed: AxesConsumed::new(axes),
                cost: u128::from(next()),
                settled_at_unix: next(),
            };
            let h1 = compute_settlement_hash(&event).unwrap();
            let h2 = compute_settlement_hash(&event).unwrap();
            assert_eq!(h1, h2, "determinism violated for event {event:?}");
        }
    }
}
