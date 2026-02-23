//! CipherOcto Core
//!
//! Deterministic core logic for the CipherOcto network.
//!
//! Responsibilities:
//! - Identity management
//! - Role staking (simulated in MVP)
//! - Message routing
//! - Resource accounting
//!
//! This crate contains protocol logic that must be deterministic
//! and secure.

pub mod identity;
pub mod role;
pub mod routing;

pub use identity::Identity;
pub use role::Role;

/// CipherOcto core configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub local_port: u16,
    pub bootstrap_peers: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            local_port: 8765,
            bootstrap_peers: vec![],
        }
    }
}
