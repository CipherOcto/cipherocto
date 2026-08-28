//! Layer B [ADD] substrate for RFC-0011 §Subcommand Taxonomy.
//!
//! New types consumed by the `octo-cli` (Layer C/D) `identity` subcommand
//! tree. Pure additions to the existing identity substrate — does NOT modify
//! any existing `IdentityKey` fields, methods, or behavior.
//!
//! Per RFC-0010 alignment, the canonical DID form is `did:octo:<encoded-pubkey>`.
//! This module exposes a `Did` newtype wrapper that the CLI composes into
//! `IdentityShowOutput` rows.

use serde::{Deserialize, Serialize};

/// Serde adapter for `[u8; 32]` fields (matches the `market_delivery`
/// pattern; serde's derive does not auto-implement for fixed arrays).
mod serde_bytes_32 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::Bytes::new(bytes).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let v: serde_bytes::ByteArray<32> = serde_bytes::ByteArray::deserialize(d)?;
        Ok(v.into_array())
    }
}

/// Serde adapter for `[u8; 64]` fields.
mod serde_bytes_64 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::Bytes::new(bytes).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: serde_bytes::ByteArray<64> = serde_bytes::ByteArray::deserialize(d)?;
        Ok(v.into_array())
    }
}

/// Serde adapter for `LifecycleState`.
///
/// `LifecycleState` is `#[repr(u8)]` with cross-implementation-deterministic
/// discriminants (RFC-0009 Appendix A). We serialize/deserialize as the raw
/// `u8` byte so the wire form matches the RFC-0009 Appendix A table exactly
/// (forward-compat: unknown discriminants fail closed via `from_u8` returning
/// `None`).
mod serde_lifecycle_state {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::lifecycle::{from_u8, LifecycleState};

    // Serde's `#[serde(with = ...)]` calls `serialize(&value, serializer)`;
    // the reference is required by serde's contract. Clippy's
    // `trivially_copy_pass_by_ref` lint fires because `LifecycleState` is a
    // single-byte `Copy` enum — the signature is dictated by serde.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(state: &LifecycleState, s: S) -> Result<S::Ok, S::Error> {
        // SAFETY: `LifecycleState` is `#[repr(u8)]` with defined variants
        // (Designated=0x00, Active=0x01, Rotating=0x02, Revoked=0x03).
        let byte = *state as u8;
        byte.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<LifecycleState, D::Error> {
        let byte = u8::deserialize(d)?;
        from_u8(byte).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown LifecycleState discriminant: 0x{byte:02x}"))
        })
    }
}

/// Canonical DID per RFC-0010 alignment. Newtype wrapper for `String`.
///
/// The wrapped string is the wire-format DID (e.g. `did:octo:z<base58btc>...`).
/// Validation against `octo_ident::CanonicalCodec` is intentionally NOT done
/// here — the CLI decides whether to accept legacy forms based on its own
/// policy. The substrate just carries the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Did(pub String);

impl Did {
    /// Borrow the inner DID string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Did {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Did {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Identity record (Layer B [ADD] — exposed by `identity_record`).
///
/// Snapshot of an identity as stored in the on-disk wallet. Distinct from
/// the live `IdentityKey` (which holds the HSM adapter + lifecycle fields);
/// this is the persisted, serializable view that `octo-cli identity show`
/// reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityRecord {
    /// Canonical DID (RFC-0010 form).
    pub did: Did,
    /// 32-byte Ed25519 public key.
    #[serde(with = "serde_bytes_32")]
    pub pubkey_bytes: [u8; 32],
    /// Lifecycle state at snapshot time.
    #[serde(with = "serde_lifecycle_state")]
    pub lifecycle: crate::lifecycle::LifecycleState,
    /// HSM slot id (None for `InMemorySigner`-backed identities).
    pub hsm_slot: Option<u32>,
    /// Unix timestamp (seconds) of registration.
    pub registered_at_unix: i64,
    /// Rotation history (newest first or insertion order — TBD by consumer).
    /// Distinct from the live `IdentityKey::successor_key` linkage.
    pub rotation_history: Vec<IdentityRotationEvent>,
}

/// Identity rotation event (Layer B [ADD] — distinct from `RotationEvent`
/// in `vault_rotation` which is a different domain).
///
/// One rotation in an identity's history. Carries the 32-byte rotation id,
/// the successor DID, and the 64-byte signature proof (Ed25519 over
/// `b"rotate" || successor_pubkey`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityRotationEvent {
    /// 32-byte rotation id (opaque identifier for this rotation event).
    #[serde(with = "serde_bytes_32")]
    pub rotation_id: [u8; 32],
    /// Unix timestamp (seconds) when rotation started.
    pub started_at_unix: i64,
    /// Unix timestamp (seconds) when the grace period expires.
    pub grace_expires_at_unix: i64,
    /// DID of the successor identity (RFC-0010 form).
    pub successor_did: Did,
    /// 64-byte Ed25519 signature: `Ed25519(self, b"rotate" || successor_pubkey)`.
    #[serde(with = "serde_bytes_64")]
    pub signature_proof: [u8; 64],
}

/// Wallet store handle (Layer B [ADD] — explicit reference, NO ambient global).
///
/// `open()` returns the canonical store; CLI consumes `&WalletStore`
/// everywhere. The handle is intentionally cheap to clone (currently a
/// zero-sized type — the real impl will hold a connection / lock).
#[derive(Debug, Clone, Copy)]
pub struct WalletStore;

impl WalletStore {
    /// Open the on-disk wallet store at `$OCTO_HOME/wallet` (0700 perms).
    /// Returns the canonical handle; fails if the directory does not exist
    /// or has wrong permissions.
    ///
    /// # Errors
    /// Returns `WalletError::Io` if the directory is missing / unwritable,
    /// `WalletError::Config` if `$OCTO_HOME` is unset / empty.
    pub fn open() -> Result<Self, crate::error::WalletError> {
        // Stub: returns empty store. Real impl reads `$OCTO_HOME/wallet/keystore.json`.
        Ok(Self)
    }

    /// Return the active identity (None maps to canonical "no active
    /// identity" semantics — CLI exit 2).
    ///
    /// # Errors
    /// Returns `WalletError::NotActive` when no identity is currently active.
    pub fn try_active_identity(
        &self,
    ) -> Result<crate::identity::IdentityKey, crate::error::WalletError> {
        // Stub: returns NotActive. Real impl reads active_did pointer from store.
        Err(crate::error::WalletError::NotActive {
            current_state: crate::lifecycle::LifecycleState::Designated,
        })
    }

    /// Look up an identity record by DID.
    ///
    /// # Errors
    /// Returns `WalletError::NotActive` when the DID is not registered
    /// (stub behavior — the real impl will use a dedicated
    /// `IdentityNotFound` variant in a follow-on).
    pub fn lookup_identity_record(
        &self,
        _did: &Did,
    ) -> Result<IdentityRecord, crate::error::WalletError> {
        // Stub: returns NotActive. Real impl walks identity index.
        Err(crate::error::WalletError::NotActive {
            current_state: crate::lifecycle::LifecycleState::Designated,
        })
    }
}
