//! Wallet specialized-node adapter (RFC-0871 §Wallet Node Lifecycle).
//!
//! Layer C crate per [[cipherocto-design-principles]]: per-RFC stability,
//! additive only. Consumes Layer A (`octo-protocol`) + Layer B
//! (`octo-wallet`, `octo-ident`) and registers as a `NetworkReceiver`
//! via the Layer D transport (`octo-transport::NodeTransport`).
//!
//! ## Mission 0871a-wallet-node (RFC-0871 Phase 2)
//!
//! `WalletNode` advertises four payload kinds from the RFC-0871
//! `WALLET_*` namespace:
//! `WALLET_*` namespace:
//!
//! - `WALLET_SIGN_ED25519` (UUID `0x0009:0002:0000:0000:0000:0000:0000:0001`)
//! - `WALLET_MINT_CAPABILITY` (UUID `0x0009:0002:0000:0000:0000:0000:0000:0002`)
//! - `WALLET_ATTENUATE_CAPABILITY` (UUID `0x0009:0002:0000:0000:0000:0000:0000:0003`)
//! - `WALLET_RESOLVE_DID` (UUID `0x0009:0002:0000:0000:0000:0000:0000:0004`)
//!
//! Each handler verifies `Vec<Authorization>` per RFC-0871 §Adversary
//! Analysis A6 (logical AND, no shortcut). HSM-routed signing goes
//! through `Arc<dyn HsmAdapter>` (no direct `ed25519_dalek` access in
//! this crate per [[cipherocto-design-principles]] layer boundary).
//!
//! ## Replay + authorization
//!
//! All inbound envelopes route through `octo_protocol::EnvelopeDispatcher`
//! for envelope_id dedup + expiry check + signature verification. The
//! dispatcher reference is injectable so production code uses a
//! `ReferenceDispatcher` (full verification + cache) and tests use
//! `test_dispatcher` (in-memory cache + system clock).
//!
//! ## Backward compat
//!
//! Existing in-wallet APIs (`CapabilityToken::mint`, `IdentityKey::sign`)
//! are preserved as direct-call wrappers. Envelope handlers are additive
//! — no breaking change for existing callers.
//!
//! ## Wire format
//!
//! Outbound `WALLET_*` envelopes are borsh-encoded `NodeEnvelope` per
//! RFC-0871 §Data Structures. Inbound envelopes are borsh-decoded and
//! dispatched via `payload_kind` UUID lookup. Legacy discriminator-byte
//! envelopes are NOT accepted (mission 0871a runs in parallel with
//! mission 0870-b call-site migration; no production nodes exist).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod handlers;
pub mod node;

pub use handlers::{
    AttenuateHandler, AttenuateRequest, HandlerOutput, MintHandler, MintRequest, ResolveDIDHandler,
    ResolveDIDRequest, SignHandler, SignRequest,
};
pub use node::{WalletNode, WalletNodeConfig, WalletNodeError, WalletNodeHandle};

use octo_protocol::PayloadKindId;

/// All payload kinds served by `WalletNode` (RFC-0871 §Wallet Node Lifecycle).
///
/// Public so callers can register handlers for these UUIDs on other
/// dispatchers (e.g. quota-router's `EnvelopeDispatcher` for interop).
pub const WALLET_PAYLOAD_KINDS: &[PayloadKindId] = &[
    octo_protocol::payload_kind::WALLET_SIGN_ED25519,
    octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
    octo_protocol::payload_kind::WALLET_ATTENUATE_CAPABILITY,
    octo_protocol::payload_kind::WALLET_RESOLVE_DID,
];

/// True if `kind` is a wallet payload kind.
#[must_use]
pub fn is_wallet_payload_kind(kind: &PayloadKindId) -> bool {
    WALLET_PAYLOAD_KINDS.contains(kind)
}
