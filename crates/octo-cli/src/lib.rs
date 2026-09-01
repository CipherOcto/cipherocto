//! `octo` operator CLI — RFC-0011.

#![warn(missing_docs)]

pub mod commands;
pub mod error;
pub mod flags;
pub mod home;
pub mod output;
pub mod redact;

pub use commands::peer::PeerAction;
pub use error::{sanitize_substrate_error, OctoCliError};
pub use flags::{OperatorMode, OperatorModeFlags, OutputFlags};
pub use output::{Hex32, OutputEnvelope};

use clap::{Parser, Subcommand};
use commands::{
    capability::CapabilityAction, identity::IdentityAction, mesh::MeshAction, policy::PolicyAction,
    ReputationAction, RoleAction, VaultAction,
};

/// The `octo` operator CLI root.
#[derive(Parser, Debug)]
#[command(name = "octo", version, about = "CipherOcto operator CLI")]
pub struct Octo {
    /// Output-shaping flags.
    #[command(flatten)]
    pub output: OutputFlags,
    /// Operator-mode + write-gating flags.
    #[command(flatten)]
    pub mode: OperatorModeFlags,
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands.
///
/// `#[non_exhaustive]` per F-14: future amendments add subcommand
/// variants (e.g., `octo reputation list` per RFC-0011-b §Future
/// Work) without requiring central-enum edits across the workspace.
#[derive(Subcommand, Debug)]
#[non_exhaustive]
pub enum Commands {
    /// Show the active identity.
    Whoami,
    /// Identity lifecycle commands.
    Identity {
        /// Identity subcommand.
        #[command(subcommand)]
        action: IdentityAction,
    },
    /// Capability lifecycle commands.
    Capability {
        /// Capability subcommand.
        #[command(subcommand)]
        action: CapabilityAction,
    },
    /// Policy inspection commands.
    Policy {
        /// Policy subcommand.
        #[command(subcommand)]
        action: PolicyAction,
    },
    /// Deprecated — see RFC-0011 §Compatibility.
    #[command(hide = true)]
    Init,
    /// Deprecated — see RFC-0011 §Compatibility.
    #[command(hide = true)]
    Join,
    /// Deprecated — see RFC-0011 §Compatibility.
    #[command(hide = true)]
    Status,
    /// Role provisioning subcommands (RFC-0011-d §7.4).
    Role {
        /// Role subcommand.
        #[command(subcommand)]
        action: RoleAction,
    },
    /// Reputation read surface (RFC-0011-b §Specification).
    Reputation {
        /// Reputation subcommand.
        #[command(subcommand)]
        action: ReputationAction,
    },
    /// Mesh operations subcommands (RFC-0011-f §Subcommand Taxonomy).
    Mesh {
        /// Mesh subcommand.
        #[command(subcommand)]
        action: MeshAction,
    },
    /// Vault read surface (RFC-0011-e §Subcommand Taxonomy).
    ///
    /// Phase 1 lands the two read-only subcommands (`vault list`,
    /// `vault balance`); the transfer surface lands in the
    /// follow-on `0011-e-vault-subcommands-transfer` mission.
    Vault {
        /// Vault subcommand.
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Deprecated — see RFC-0011 §Compatibility.
    #[command(hide = true)]
    Agent {
        /// Deprecated agent subcommand.
        #[command(subcommand)]
        action: AgentActionStub,
    },
}

/// Deprecated `agent` subcommands.
#[derive(Subcommand, Debug)]
pub enum AgentActionStub {
    /// Deprecated.
    Create {
        /// Agent name.
        name: String,
    },
    /// Deprecated.
    Run {
        /// Agent name.
        name: String,
    },
    /// Deprecated.
    List,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_surface_is_valid() {
        Octo::command().debug_assert();
    }
}
