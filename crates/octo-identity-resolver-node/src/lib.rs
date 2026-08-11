//! Identity-resolver specialized-node adapter (RFC-0871 §Roles and Authorities).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B (`octo-ident`)
//! and registers as a `NetworkReceiver` via the Layer D transport
//! (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871b-identity-resolver-node (RFC-0871 Phase 3)
//!
//! `IdentityResolverNode` advertises one payload kind from the RFC-0871
//! `IDENTITY_*` namespace:
//!
//! - `IDENTITY_RESOLVE` (UUID `0x0009:0001:0000:0000:0000:0000:0000:0001`)
//!
//! The handler validates the canonical DID shape via
//! `octo_ident::CanonicalCodec::parse(s, false)` and returns the canonical
//! DID + the resolver's placeholder public key (storage-pubkey form).
//! Real storage-backed lookup (RFC-0010 dual storage/wire split) is wired
//! in a follow-on mission; this crate is the network adapter only.
//!
//! ## Replay + authorization
//!
//! All inbound envelopes route through `octo_protocol::ReferenceDispatcher`
//! for envelope_id dedup + expiry check + signature verification. The
//! dispatcher reference is injectable so production code uses a
//! `ReferenceDispatcher` (full verification + cache) and tests use
//! `test_dispatcher` (in-memory cache + system clock).
//!
//! ## Wire format
//!
//! Outbound `IDENTITY_RESOLVE` envelopes are borsh-encoded `NodeEnvelope`
//! per RFC-0871 §Data Structures. Inbound envelopes are borsh-decoded
//! and dispatched via `payload_kind` UUID lookup.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod handlers;
pub mod node;

pub use handlers::{
    HandlerOutput, IdentityResolveError, RegisterHandler, RegisterRequest, RegisterResponse,
    ResolveHandler, ResolveRequest, ResolveResponse, RevokeHandler, RevokeRequest, RevokeResponse,
};
pub use node::{
    IdentityResolverNode, IdentityResolverNodeConfig, IdentityResolverNodeError,
    IdentityResolverNodeHandle, DEFAULT_CHAIN_ID,
};

use octo_protocol::PayloadKindId;

/// All payload kinds served by `IdentityResolverNode` (RFC-0871 §Roles and
/// Authorities, RFC-0862 v1.3 §DidWriteCoordinator).
///
/// Public so callers can register handlers for these UUIDs on other
/// dispatchers (e.g. quota-router's `EnvelopeDispatcher` for interop).
///
/// Mission 0871e-f7-impl-resolver-mediation: extends the read-only
/// `IDENTITY_RESOLVE` with the cross-instance write paths
/// `IDENTITY_REGISTER` + `IDENTITY_REVOKE` that consult an injected
/// `DidWriteCoordinator` (RFC-0862 v1.3) before delegating to the local
/// `DidRegistry` backend.
pub const IDENTITY_RESOLVER_PAYLOAD_KINDS: &[PayloadKindId] = &[
    octo_protocol::payload_kind::IDENTITY_RESOLVE,
    octo_protocol::payload_kind::IDENTITY_REGISTER,
    octo_protocol::payload_kind::IDENTITY_REVOKE,
];

/// True if `kind` is an identity-resolver payload kind.
#[must_use]
pub fn is_identity_resolver_payload_kind(kind: &PayloadKindId) -> bool {
    IDENTITY_RESOLVER_PAYLOAD_KINDS.contains(kind)
}
