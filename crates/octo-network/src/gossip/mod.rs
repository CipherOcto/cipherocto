//! BIND envelope gossip over libp2p (mission 0850p-c-libp2p-propagation).
//!
//! See `bind.rs` for the implementation. This module re-exports
//! the public API.
//!
//! `reputation.rs` is the reputation gossip substrate for mission
//! 0855p-b / 0968 Phase 4; it consumes the inbound gossipsub channel
//! from `octo-adapter-p2p` and writes to the persisted
//! `ReputationStore`. Topics are `/dot/reputation/{recorder_did}` per
//! RFC-0968-A1 amendment 29.

pub mod bind;
pub mod reputation;

pub use bind::{bind_gossip_topic, BindGossipConfig, BindGossipState};
// Re-export `start_reputation_gossip` for back-compat. It's
// #[deprecated] (Round 2 review C5 — RFC-0968 §12 MUST); the
// `#[allow(deprecated)]` here suppresses the deprecation at the
// re-export site so it fires only at actual callers.
#[allow(deprecated)]
pub use reputation::{
    start_reputation_gossip, IngressOutcome, RawIngress, ReputationGossipHandle,
    ReputationGossipJoin,
};
