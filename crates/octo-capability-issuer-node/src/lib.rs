//! Capability-issuer specialized-node adapter (RFC-0871 §Roles and
//! Authorities, mission 0871d-capability-issuer-node).
//!
//! Layer C crate per `docs/architecture/design-principles.md`: per-RFC stability,
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
//! Both handlers operate on the real substrate: they mint and revoke through
//! `HolderRegistry` rather than acknowledging requests.
//!
//! ## Scope
//!
//! `CAPABILITY_ISSUE`:
//! 1. Validates `holder_did` via `octo_ident::CanonicalCodec::parse(s, false)`
//!    (RFC-0010 F4 — canonical form only; legacy bare form rejected).
//! 2. Mints a real `CapabilityToken` signed by the node's identity, and inserts
//!    the `HolderRecord` into the node's registry (RFC-0957 §Algorithms,
//!    RFC-0957-A1 §Roles and Authorities — the Token Issuer registers).
//! 3. Refuses without an identity key, because minting signs with it.
//!
//! `CAPABILITY_REVOKE`:
//! 1. Accepts the 32-byte `cap_root_hash`, the key the registry is indexed by.
//! 2. Marks the record revoked in the node's registry and reports what actually
//!    happened. A capability the registry never held reports `revoked: false`
//!    rather than an acknowledgement it cannot back up.
//!
//! ## Authorization model (production semantics, deferred to 0957 Phase 2)
//!
//! - `CAPABILITY_ISSUE` requires `Authorization::Signature` from issuer
//!   (issuer's HSM key) + holder's pre-signed commitment envelope.
//! - `CAPABILITY_REVOKE` authorization is **unspecified**. The rule previously
//!   cited here — a `RevocationCaveat` from RFC-0965's reserved range — does not
//!   exist: RFC-0965 never mentions revocation, and `Caveat` has no `Revocation`
//!   variant. RFC-0957-A1 gives the Token Issuer the authority to revoke, which
//!   is a role statement and not a request rule. See
//!   `missions/deferred/0969-a1-revocation-authorization.md`.
//!
//! The dispatcher performs envelope-level verification (`envelope_id` dedup +
//! expiry + signature). Fine-grained authorization (holder pre-signed
//! commitment + revocation authorization) is still the substrate layer's
//! responsibility and remains open. Revocation in particular is not gated on who
//! asked: see §Authorization model above.
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
    HandlerOutput, IssueHandler, IssueRequest, RevokeHandler, RevokeRequest, RevokeResponse,
};
pub use node::{
    default_dispatcher, CapabilityIssuerNode, CapabilityIssuerNodeConfig,
    CapabilityIssuerNodeError, CapabilityIssuerNodeHandle,
};

/// Payload kinds served by the capability issuer node.
///
/// **Re-exported from `octo-protocol`, not redeclared.** This crate used to
/// carry its own copy of this list, and the copy was wrong: it named
/// `CAPABILITY_ISSUE` and `CAPABILITY_REVOKE` and omitted `CAPABILITY_LOOKUP`,
/// which `handle_envelope` dispatches. `CapabilityIssuerNode::start` iterates
/// this list to register served kinds, so the node never registered a kind it
/// nonetheless answered — a request the node served and had not declared.
///
/// Two definitions of one set is §No parallel abstractions: the second copy
/// cannot be kept in step with the first, and here it was not.
pub use octo_protocol::payload_kind::{
    is_capability_payload_kind, CAPABILITY_ISSUE, CAPABILITY_LOOKUP, CAPABILITY_PAYLOAD_KINDS,
    CAPABILITY_REVOKE,
};
