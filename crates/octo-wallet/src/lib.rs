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
pub mod agent_persist;
pub mod capability;
pub mod cli_fns;
pub mod error;
pub mod fs_perms;
pub mod hsm;
pub mod identity;
pub mod identity_record;
pub mod identity_store;
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
    AgentFilter, AgentManifest, AgentRegistry, AgentState, AgentSummary, CapabilityId,
    InMemoryAgentRegistry, TransitionReceipt,
};
pub use cli_fns::{
    abort_rotation, begin_rotation, complete_rotation, identity_record as identity_record_fn,
    register_agent, revoke,
};
pub use error::WalletError;
// Re-export the Layer A `ed25519-dalek` crate at the wallet boundary
// so `octo-runtime` can verify signatures without taking a direct
// dep on `ed25519-dalek` (per RFC-0011-c §F.5 Layer A frozen contract
// + `docs/architecture/design-principles.md` §Stable Abstractions Principle).
// `octo-runtime::handle::signing::verify_attach_handle_payload` and
// `octo-runtime::handle::encoding::decode_token` follow the same
// `verify_successor_proof` / `verify_revocation_proof` static-helper
// shape — pure helpers that verify against arbitrary `pubkey: &[u8;
// 32]` rather than binding to a `&self` receiver — but they live in
// the runtime substrate, not the wallet. Routing the primitive
// through the wallet re-export preserves the layer direction.
pub use ed25519_dalek;
pub use identity::{derive_capability_key, AudienceId, CapabilityKey, ChannelId, IdentityKey};
pub use identity_record::{Did, IdentityRecord, IdentityRotationEvent};
pub use identity_store::{WalletIndex, WalletStore, WALLET_INDEX_VERSION};

// Algorithm dispatch, re-exported alongside `ed25519_dalek` for the
// same reason: crates that already treat the wallet as their crypto
// gateway (`octo-runtime`) can dispatch through the registry without
// growing a dependency on the Layer B identity substrate and the Layer
// E algorithm crate directly. Mission 0009-c.
pub use key_hierarchy::{AxisSubkey, KeyHierarchy, MissionId, MissionKey};
pub use lifecycle::LifecycleState;
pub use node::{NodeType, NodeTypeParseError};
pub use octo_algo_ed25519::{
    ed25519_algorithm_id, verify_dispatch, DispatchError, Ed25519Algorithm,
};
pub use octo_ident::AlgorithmId;
// The RFC-0009 prefix substrate, re-exported alongside `AlgorithmId` so a
// downstream crate that must *carry* a prefix on the wire (an attach-handle
// token, for instance) reaches it through the same path as the identity
// operations that produce one. Without this, every such crate grows its own
// `octo-ident` edge to reach a type `octo-wallet` already owns semantics for —
// two import paths to one concept (principle §No parallel abstractions).
pub use octo_ident::{
    CanonicalPayload, SignableError, SignableForm, SignablePrefix, SignedMessage,
    VerificationMethodId,
};
pub use role_nonce::next_nonce_counter;

#[cfg(test)]
mod phase1_tv_json;
