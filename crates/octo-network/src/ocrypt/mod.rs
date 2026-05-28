//! Overlay Cryptography (OCrypt) — RFC-0853
//!
//! Sovereign cryptographic layer for CipherOcto overlay networking.
//!
//! Provides:
//! - Sovereign overlay identity (platform-independent)
//! - Deterministic envelope encryption (ChaCha20-Poly1305)
//! - Session key establishment (X25519 + HKDF-BLAKE3)
//! - Mission-scoped key hierarchy
//! - Gateway attestation
//! - Onion relay extension
//! - Deterministic randomness derivation
//!
//! Core invariant: External platforms MUST NEVER be trusted for
//! confidentiality, authenticity, ordering, or integrity.

pub mod attestation;
pub mod envelope;
pub mod error;
pub mod identity;
pub mod mission;
pub mod onion;
pub mod randomness;
pub mod session;
pub mod suite;

pub use attestation::GatewayAttestation;
pub use envelope::{EncryptedEnvelope, EncryptionContext};
pub use error::CryptoError;
pub use identity::{OverlayIdentity, PlatformBinding};
pub use mission::MissionKeyHierarchy;
pub use randomness::{derive_deterministic_nonce, derive_deterministic_random};
pub use session::{
    decrypt, derive_consensus_nonce, derive_envelope_key, derive_nonce, derive_session_key,
    encrypt, x25519_shared_secret, SessionKeyMaterial, KEY_SIZE, NONCE_SIZE, TAG_SIZE,
};
pub use suite::{CryptoSuiteId, DEFAULT_SUITE};
