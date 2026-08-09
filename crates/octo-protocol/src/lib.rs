//! Specialized Node Protocol Envelope — RFC-0871.
//!
//! Layer 1 stable crate owning the canonical envelope types + dispatch logic:
//! [`NodeEnvelope`], [`PayloadKindId`], [`Authorization`], [`RecipientRef`],
//! [`ProtocolError`]. Implementations of `envelope_id` derivation via BLAKE3-256
//! and the domain-separated signing preimage
//! `blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE", envelope_id || from_did_wire || payload)`
//! live in [`signing`]. Canonical DID validation via
//! `octo_ident::CanonicalCodec::parse()` is enforced on every `from_did` field
//! at envelope construction.
//!
//! ## Design intent (RFC-0871 §Design Goals)
//!
//! - **No central `PayloadKind` enum** — 128-bit `PayloadKindId` UUID with
//!   RFC-allocated namespace + Raw escape hatch.
//! - **No central `Authorization` enum without escape hatch** — `Authorization`
//!   has `Raw { discriminator: [u8; 16], body: Vec<u8> }` for forward-compat.
//! - **All identities use canonical DID** — `from_did` validated via
//!   `octo_ident::CanonicalCodec::parse()` at every boundary.
//! - **Replay defense + TTL** — `envelope_id` uniqueness + per-sender nonce
//!   + `expires_at_unix_ms` (millisecond resolution per RFC-0970 §TV11).
//!
//! ## Wire format (RFC-0871 §Algorithms)
//!
//! 1. Build envelope with `from_did = signer.canonical_did()`.
//! 2. Compute `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`.
//! 3. Compute signature preimage:
//!    `preimage = blake3::derive_key("OCTO_NODEENVELOPE_V1_SIGNATURE", envelope_id || from_did_wire || payload).as_bytes()` (32 bytes, domain-separated).
//! 4. Sign via `HsmAdapter::sign(preimage)` (Layer B wallet-core owns HSM routing;
//!    this crate only consumes `ed25519_dalek` directly for Layer-1 test fixtures).
//! 5. Set `nonce`, `expires_at_unix_ms = clock.now_unix_ms() + TTL`.
//! 6. Serialize via borsh; ship via `octo_transport::NodeTransport`.
//!
//! ## Clock injection
//!
//! Sender (TTL computation) and receiver (TTL enforcement) MUST obtain current
//! time via the injected [`time::Clock`] trait, not via direct
//! `SystemTime::now()` calls. Required for byte-exact reproducibility of test
//! vectors and for deterministic simulation runs.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod authorization;
pub mod dispatch;
pub mod envelope;
pub mod error;
pub mod payload_kind;
pub mod recipient;
pub mod signing;
pub mod time;

pub use authorization::{Authorization, BlsSignature, CapabilityToken, ProofBundle};
pub use dispatch::{DispatcherConfig, EnvelopeDispatcher, HandlerOutput, ValidationCache};
pub use envelope::NodeEnvelope;
pub use error::ProtocolError;
pub use payload_kind::PayloadKindId;
pub use recipient::RecipientRef;
pub use signing::{compute_envelope_id, signature_preimage, DOMAIN_ENVELOPE_ID, DOMAIN_SIGNATURE};
pub use time::{Clock, SystemClock};

/// Re-export of `octo_ident::WireDid` for downstream crate convenience.
/// Envelope types embed `WireDid` directly; this re-export lets consumers
/// `use octo_protocol::WireDid` without taking a transitive dep on octo-ident.
pub use octo_ident::WireDid;

use octo_ident::CanonicalCodec;
use octo_ident::DidCodec;

/// Validate that a wire-form DID string matches the canonical shape.
///
/// Per RFC-0871 §Specification, every `from_did` field MUST be validated via
/// `octo_ident::CanonicalCodec::parse()`. Legacy `did:octo:b<base32>` form is
/// rejected post-deprecation window (RFC-0010 §`parse` step 3).
pub fn validate_canonical_did(input: &str) -> Result<WireDid, ProtocolError> {
    CanonicalCodec::parse(input, false).map_err(ProtocolError::from)
}

/// Convenience constructor that validates canonical DID before wrapping.
pub fn wire_did(input: &str) -> Result<WireDid, ProtocolError> {
    validate_canonical_did(input)
}
