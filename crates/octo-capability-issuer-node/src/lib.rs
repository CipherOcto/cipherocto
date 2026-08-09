//! Capability-issuer specialized-node adapter (RFC-0871 §Roles and
//! Authorities, mission 0871d-capability-issuer-node).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-wallet`, `octo-ident`) + Layer 4 (`octo-cap-macaroon`) and
//! registers as a `NetworkReceiver` via the Layer D transport
//! (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871d-capability-issuer-node (RFC-0871 Phase 3)
//!
//! `CapabilityIssuerNode` advertises two Phase 3 MVP payload kinds
//! from the RFC-0871 `CAPABILITY_*` namespace (sub-namespace `0x0005`):
//!
//! - `CAPABILITY_ISSUE` (UUID `0x0009:0005:0000:0000:0000:0000:0000:0001`)
//! - `CAPABILITY_REVOKE` (UUID `0x0009:0005:0000:0000:0000:0000:0000:0002`)
//!
//! Each handler validates `holder_did` / `token_id` shape and returns
//! a placeholder wire form. The full macaroon substrate
//! (`CapabilityToken::mint` + `HolderRegistry` registration per RFC-0957
//! §Algorithms + RFC-0957-A1 §Data Structures, and the RFC-0965
//! `RevocationCaveat`) lands in mission 0957 Phase 2 follow-on.
//!
//! ## Phase 3 MVP scope
//!
//! `CAPABILITY_ISSUE`:
//! 1. Validates `holder_did` via `octo_ident::CanonicalCodec::parse(s, false)`
//!    (RFC-0010 v1.2 F4 — canonical form only; legacy bare form rejected).
//! 2. Returns a placeholder wire form `CIPHEROCTO_ISSUE_V1:<holder_did>:<token_id>`.
//! 3. No macaroon minting, no holder signature, no `HolderRegistry`
//!    registration — substrate lands in 0957 Phase 2.
//!
//! `CAPABILITY_REVOKE`:
//! 1. Accepts the 16-byte `token_id` (= `MacaroonId` per RFC-0957 §Wire Format).
//! 2. Returns an acknowledgement stub (no `HolderRegistry` mutation,
//!    no event emission — substrate lands in 0957 Phase 2).
//!
//! ## Authorization model (production semantics, deferred to 0957 Phase 2)
//!
//! - `CAPABILITY_ISSUE` requires `Authorization::Signature` from issuer
//!   (issuer's HSM key) + holder's pre-signed commitment envelope.
//! - `CAPABILITY_REVOKE` requires `Authorization::Capability(token)` with
//!   `RevocationCaveat` (RFC-0965 reserved range).
//!
//! Phase 3 MVP: the dispatcher performs envelope-level verification
//! (`envelope_id` dedup + expiry + signature). Fine-grained
//! authorization (holder pre-signed commitment + revocation caveat
//! validation) is the substrate layer's responsibility and lands in
//! mission 0957 Phase 2.
//!
//! ## Replay + authorization
//!
//! All inbound envelopes route through `octo_protocol::EnvelopeDispatcher`
//! for envelope_id dedup + expiry check + signature verification. The
//! dispatcher reference is injectable so production code uses a
//! `ReferenceDispatcher` (full verification + cache) and tests use the
//! default dispatcher (in-memory cache + system clock).
//!
//! ## Wire format
//!
//! Outbound `CAPABILITY_*` envelopes are borsh-encoded `NodeEnvelope` per
//! RFC-0871 §Data Structures. Inbound envelopes are borsh-decoded and
//! dispatched via `payload_kind` UUID lookup.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod handlers;
pub mod node;

pub use handlers::{
    HandlerOutput, IssueHandler, IssueRequest, IssueResponse, RevokeHandler, RevokeRequest,
    RevokeResponse,
};
pub use node::{
    default_dispatcher, CapabilityIssuerNode, CapabilityIssuerNodeConfig,
    CapabilityIssuerNodeError, CapabilityIssuerNodeHandle,
};

use octo_protocol::PayloadKindId;

/// All payload kinds served by `CapabilityIssuerNode` (RFC-0871 §Roles
/// and Authorities, mission 0871d-capability-issuer-node).
///
/// Phase 3 MVP exposes `CAPABILITY_ISSUE` + `CAPABILITY_REVOKE`.
/// Follow-on missions add `CAPABILITY_LOOKUP` + `CAPABILITY_ATTENUATE`
/// once the macaroon substrate (mission 0957 Phase 2) + `HolderRegistry`
/// (RFC-0957-A1) are wired in production.
///
/// Public so callers can register handlers for these UUIDs on other
/// dispatchers (e.g. quota-router's `EnvelopeDispatcher` for interop).
pub const CAPABILITY_PAYLOAD_KINDS: &[PayloadKindId] = &[
    octo_protocol::payload_kind::CAPABILITY_ISSUE,
    octo_protocol::payload_kind::CAPABILITY_REVOKE,
];

/// True if `kind` is a capability-issuer payload kind.
#[must_use]
pub fn is_capability_payload_kind(kind: &PayloadKindId) -> bool {
    CAPABILITY_PAYLOAD_KINDS.contains(kind)
}
