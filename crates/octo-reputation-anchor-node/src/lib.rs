//! Reputation-anchor specialized-node adapter (RFC-0871 §Roles and
//! Authorities, mission 0871c-reputation-anchor-node).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-ident`) and registers as a `NetworkReceiver` via the Layer D
//! transport (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871c-reputation-anchor-node (RFC-0871 Phase 3)
//!
//! `ReputationAnchorNode` advertises the Phase 3 MVP payload kind
//! `REPUTATION_ANCHOR_QUERY` (UUID
//! `0x0009:0004:0000:0000:0000:0000:0000:0001`). The handler validates a
//! canonical DID via `octo_ident::CanonicalCodec::parse(s, false)` and
//! returns a stub `(anchor_score = 0, attestation_count = 0)` response.
//!
//! The full RFC-0968 / RFC-0955-R1 reputation surface
//! (`REPUTATION_QUERY`, `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`)
//! lands in mission 0968a-reputation-anchoring follow-on once the
//! reputation registry substrate + anchoring substrate are production-ready.
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
//! Outbound `REPUTATION_*` envelopes are borsh-encoded `NodeEnvelope` per
//! RFC-0871 §Data Structures. Inbound envelopes are borsh-decoded and
//! dispatched via `payload_kind` UUID lookup.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod handlers;
pub mod node;

pub use handlers::{HandlerOutput, QueryAnchorHandler, QueryAnchorRequest, QueryAnchorResponse};
pub use node::{
    default_dispatcher, ReputationAnchorNode, ReputationAnchorNodeConfig,
    ReputationAnchorNodeError, ReputationAnchorNodeHandle,
};

use octo_protocol::PayloadKindId;

/// All payload kinds served by `ReputationAnchorNode` (RFC-0871 §Roles
/// and Authorities, mission 0871c-reputation-anchor-node).
///
/// Phase 3 MVP exposes only `REPUTATION_ANCHOR_QUERY`. Follow-on
/// missions add `REPUTATION_QUERY` / `REPUTATION_UPDATE` /
/// `REPUTATION_ANCHOR` once the RFC-0968 registry + RFC-0955-R1 anchoring
/// substrate are wired (mission 0968a-reputation-anchoring).
///
/// Public so callers can register handlers for these UUIDs on other
/// dispatchers (e.g. quota-router's `EnvelopeDispatcher` for interop).
pub const REPUTATION_PAYLOAD_KINDS: &[PayloadKindId] =
    &[octo_protocol::payload_kind::REPUTATION_ANCHOR_QUERY];

/// True if `kind` is a reputation-anchor payload kind.
#[must_use]
pub fn is_reputation_payload_kind(kind: &PayloadKindId) -> bool {
    REPUTATION_PAYLOAD_KINDS.contains(kind)
}
