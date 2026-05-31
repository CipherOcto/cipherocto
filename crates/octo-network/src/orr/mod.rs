//! Onion Relay Routing (RFC-0858)
//!
//! Privacy-preserving multi-hop relay architecture with layered encryption,
//! per-relay knowledge isolation, forward secrecy, and cover traffic generation.

pub mod cover_traffic;
pub mod error;
pub mod onion;
pub mod session;
pub mod types;

pub use cover_traffic::{check_replay, generate_cover_payload, ORR_EXECUTION_CLASS_TABLE};
pub use error::OrrError;
pub use onion::{construct_onion, peel_layer, HopConstructionParams, PeeledLayer};
pub use session::{compute_hop_mac, derive_hop_nonce, derive_hop_session_key};
pub use types::{
    CoverEnvelope, CoverPolicy, OnionDomain, OnionHop, OnionRoute, RouteCommitment,
    TransportVector, ROUTE_FLAG_COVER, ROUTE_FLAG_HIGH_LATENCY, ROUTE_FLAG_MISSION_SCOPED,
    ROUTE_FLAG_STEALTH,
};
