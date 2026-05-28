//! Onion Relay Routing (RFC-0858)
//!
//! Privacy-preserving multi-hop relay architecture with layered encryption,
//! per-relay knowledge isolation, forward secrecy, and cover traffic generation.

pub mod error;
pub mod session;
pub mod types;

pub use error::OrrError;
pub use session::{compute_hop_mac, derive_hop_nonce, derive_hop_session_key};
pub use types::{
    CoverEnvelope, CoverPolicy, OnionDomain, OnionHop, OnionRoute, RouteCommitment,
    TransportVector, ROUTE_FLAG_COVER, ROUTE_FLAG_HIGH_LATENCY, ROUTE_FLAG_MISSION_SCOPED,
    ROUTE_FLAG_STEALTH,
};
