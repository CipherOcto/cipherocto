//! Command dispatch — RFC-0011 §Binary Surface.

pub mod agent;
pub mod audit;
pub mod capability;
pub mod governance;
pub mod identity;
pub mod mesh;
pub mod peer;
pub mod policy;
pub mod reputation;
pub mod role;
pub mod vault;

pub use agent::AgentAction;
pub use audit::AuditAction;
pub use governance::GovernanceAction;
pub use mesh::MeshAction;
pub use peer::PeerAction;
pub use reputation::ReputationAction;
pub use role::RoleAction;
pub use vault::VaultAction;

use crate::error::OctoCliError;
use crate::{Commands, Octo};

/// Route a parsed invocation to its command handler.
pub fn dispatch(cli: &Octo) -> Result<(), OctoCliError> {
    match &cli.command {
        Commands::Whoami => identity::whoami(cli),
        Commands::Identity { action } => identity::dispatch(action, cli),
        Commands::Capability { action } => capability::dispatch(action, cli),
        Commands::Policy { action } => policy::dispatch(action, cli),
        Commands::Role { action } => role::dispatch(action, cli),
        Commands::Reputation { action } => reputation::dispatch(action, cli),
        Commands::Mesh { action } => mesh::dispatch(action, cli),
        Commands::Vault { action } => vault::dispatch(action, cli),
        Commands::Agent { action } => agent::dispatch(action, cli),
        Commands::Governance { action } => governance::dispatch(action, cli),
        Commands::Audit { action } => audit::dispatch(action, cli),
    }
}
