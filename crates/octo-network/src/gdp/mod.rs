//! Gateway Discovery Protocol (GDP) — RFC-0851
//!
//! Gateway discovery with advertisements, capability Merkle commitments,
//! heartbeat monitoring, and deterministic cache eviction.

pub mod advertisement;
pub mod anti_sybil;
pub mod cache;
pub mod discovery;
pub mod discovery_gossip;
pub mod error;
pub mod heartbeat;
pub mod identity;
pub mod overlay_endpoint;
pub mod types;

pub use advertisement::GatewayAdvertisement;
pub use cache::GatewayCache;
pub use error::GdpError;
pub use heartbeat::GatewayHeartbeat;
pub use identity::GdpGatewayIdentity;
pub use overlay_endpoint::OverlayEndpoint;
pub use types::{
    AdvertisementExpiration, DiscoveryLifecycle, DiscoveryScope, GatewayCapability,
    StakeRequirement,
};
