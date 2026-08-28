//! CipherOcto Wallet.
//!
//! User-facing wallet layer: identity keys, capability key derivation,
//! encrypted provider-key vault, starkli-compatible keystore import/export.
//!
//! Architectural role: the **root** of the dependency graph for cryptographic
//! types in CipherOcto. `octo-core` re-exports these types via thin newtype
//! wrappers so downstream code (router, adapters) never sees the substrate
//! directly.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]

pub mod capability;
pub mod cli_fns;
pub mod error;
pub mod hsm;
pub mod identity;
pub mod identity_record;
pub mod key_hierarchy;
pub mod keystore;
pub mod lifecycle;
pub mod mpc;
pub mod node;
pub mod vault;
pub mod vault_rotation;

pub use cli_fns::{active_identity, begin_rotation, identity_record as identity_record_fn, revoke};
pub use error::WalletError;
pub use identity::{derive_capability_key, AudienceId, CapabilityKey, ChannelId, IdentityKey};
pub use identity_record::{Did, IdentityRecord, IdentityRotationEvent, WalletStore};
pub use key_hierarchy::{AxisSubkey, KeyHierarchy, MissionId, MissionKey};
pub use lifecycle::LifecycleState;
pub use node::{NodeType, NodeTypeParseError};

#[cfg(test)]
mod phase1_tv_json;
