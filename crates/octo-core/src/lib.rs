//! CipherOcto Core
//!
//! Deterministic core logic for the CipherOcto network.
//!
//! Responsibilities:
//! - Identity management
//! - Role staking (simulated in MVP)
//! - Message routing
//!
//! This crate contains protocol logic that must be deterministic
//! and secure.
//!
//! **As of 2026-07-21:** pricing/settlement (`ask`), persistence
//! (`ask_repo`, `migrations`), and sync subscription (`sync`) moved to
//! `quota-router-storage` — they are quota-router domain, not core.
//! octo-core is now lean: identity + role + routing only.

pub mod capability;
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
