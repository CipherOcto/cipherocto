//! CipherOcto Network — Deterministic Overlay Networking Stack
//!
//! Multi-module deterministic overlay networking for the CipherOcto protocol.
//!
//! Modules:
//! - DOT (RFC-0850): Deterministic Overlay Transport
//! - GDP (RFC-0851): Gateway Discovery Protocol
//! - DGP (RFC-0852): Deterministic Gossip Protocol
//! - OCrypt (RFC-0853): Overlay Cryptography
//! - DPS (RFC-0854): Deterministic Proof Substrate
//! - MON (RFC-0855): Mission Overlay Networks
//! - DRS (RFC-0856): Deterministic Route Selection
//! - DOM (RFC-0857): Deterministic Overlay Mempool
//! - ORR (RFC-0858): Onion Relay Routing
//! - PCE (RFC-0859): Proof-Carrying Envelopes (under DOT)
//! - PoRelay (RFC-0860): Proof-of-Relay

/// Common utilities (shared Merkle, etc.).
pub mod common;

/// Deterministic Overlay Transport module — RFC-0850.
pub mod dot;

/// Deterministic Gossip Protocol — RFC-0852.
pub mod dgp;
/// Deterministic Proof Substrate (DPS) — RFC-0854.
pub mod dps;
/// Gateway Discovery Protocol — RFC-0851.
pub mod gdp;
/// Mission 0850p-c-libp2p-propagation: BIND envelope gossip.
pub mod gossip;
/// Overlay Cryptography (OCrypt) module — RFC-0853.
pub mod ocrypt;

/// DomainCoordinator role — RFC-0855p-c + missions 0855p-c-*.
pub mod dc;
/// Deterministic Overlay Mempool (DOM) — RFC-0857.
pub mod dom;
/// Deterministic Route Selection (DRS) — RFC-0856.
pub mod drs;
/// Mission Overlay Networks (MON) — RFC-0855.
pub mod mon;
/// Federation reputation (mission 0855p-b / 0968 Phase 4).
pub mod reputation;
// Re-export the reputation gossip substrate so callers can
// `octo_network::gossip::start_reputation_gossip` without
// descending into the inner module.
pub use crate::gossip::reputation::{
    gossip_catch_up, start_reputation_gossip, start_reputation_gossip_with_rate_limit,
    IngressOutcome, RawIngress, ReputationGossipHandle, ReputationGossipJoin,
};
pub use crate::reputation::{
    DcRootedSlashReputationStoreCompat, SlashReputationStoreCompat,
    HARD_THRESHOLD as SLASH_HARD_THRESHOLD,
};
/// Onion Relay Routing (ORR) — RFC-0858.
pub mod orr;
/// Proof-of-Relay (PoRelay) — RFC-0860.
pub mod porelay;

/// Data sync transport bridge — RFC-0862 carrier integration.
pub mod sync;
