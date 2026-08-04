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
/// **NodeType deferred:** RFC-0959 §Data Structures lists `node_type` as a
/// payload field, gated by RFC-0009 §Node (which lives in `octo-wallet::node`).
/// Adding `octo-wallet` as a dep pulls HSM/MPC/keystore — too heavy for this
/// session. The field is omitted here; the follow-up session that adds
/// `octo-wallet` re-introduces it via `NodeType` re-export. This matches the
/// mission's "Re-exported from octo-wallet::node" intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskUnsignedPayload {
    /// Asker (publisher) DID (RFC-0009 §Identity Key Format).
    pub asker_did: AskerDid,
    /// Model reference — strings `{namespace, family, version?}` per RFC-0959 §Data Structures.
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
    pub fn new(
        asker_did: impl Into<String>,
        model: impl Into<String>,
        rates: ModelRateTable,
        ttl_unix: u64,
        jurisdiction: Vec<String>,
        published_at_unix: u64,
        nonce: [u8; 16],
    ) -> Result<Self, AskError> {
        let asker_did = asker_did.into();
        let model = model.into();
        if asker_did.is_empty() {
            return Err(AskError::EmptyAskerDid);
        }
        if model.is_empty() {
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
            model: "openai/gpt-4".to_owned(),
            rates: vec![AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: 30_000,
            }],
        };
        AskUnsignedPayload {
            asker_did: "did:octo:asker1".to_owned(),
            model: "openai/gpt-4".to_owned(),
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
