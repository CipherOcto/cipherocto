//! BIND envelope gossip over libp2p (mission 0850p-c-libp2p-propagation).
//!
//! See `bind.rs` for the implementation. This module re-exports
//! the public API.

pub mod bind;

pub use bind::{bind_gossip_topic, BindGossipConfig, BindGossipState};
