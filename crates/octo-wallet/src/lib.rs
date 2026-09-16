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

pub mod agent;
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
pub mod role_nonce;
pub mod vault;
pub mod vault_rotation;

pub use agent::{
    list_owned_agents, lookup_agent, read_agent_state, transition_agent, validate_reason,
    AgentFilter, AgentManifest, AgentState, AgentSummary, CapabilityId, TransitionReceipt,
};
pub use cli_fns::{
    active_identity, begin_rotation, identity_record as identity_record_fn, register_agent, revoke,
};
pub use error::WalletError;
// Re-export the Layer A `ed25519-dalek` crate at the wallet boundary
// so `octo-runtime` can verify signatures without taking a direct
// dep on `ed25519-dalek` (per RFC-0011-c §F.5 Layer A frozen contract
// + [[cipherocto-design-principles]] §Stable Abstractions Principle).
// `octo-runtime::handle::signing::verify_attach_handle_payload` and
// `octo-runtime::handle::encoding::decode_token` follow the same
// `verify_successor_proof` / `verify_revocation_proof` static-helper
// shape — pure helpers that verify against arbitrary `pubkey: &[u8;
// 32]` rather than binding to a `&self` receiver — but they live in
// the runtime substrate, not the wallet. Routing the primitive
// through the wallet re-export preserves the layer direction.
pub use ed25519_dalek;
pub use identity::{derive_capability_key, AudienceId, CapabilityKey, ChannelId, IdentityKey};
pub use identity_record::{Did, IdentityRecord, IdentityRotationEvent, WalletStore};
pub use key_hierarchy::{AxisSubkey, KeyHierarchy, MissionId, MissionKey};
pub use lifecycle::LifecycleState;
pub use node::{NodeType, NodeTypeParseError};
pub use role_nonce::next_nonce_counter;

#[cfg(test)]
mod phase1_tv_json;
