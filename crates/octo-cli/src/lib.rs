//! `octo` operator CLI — RFC-0011.

#![warn(missing_docs)]

pub mod commands;
pub mod error;
pub mod flags;
pub mod home;
pub mod output;
pub mod redact;

pub use commands::agent::AgentAction;
pub use commands::audit::AuditAction;
pub use commands::governance::GovernanceAction;
pub use commands::network::NetworkAction;
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
    /// Agent lifecycle subcommands (RFC-0011-c §Subcommand Taxonomy).
    ///
    /// Phase 1 lands `octo agent create` (mission
    /// `0011-c-agent-create-subcommand`); sibling subcommands (`run`,
    /// `list`, `destroy`, `attach`) land in follow-on missions.
    Agent {
        /// Agent subcommand.
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Governance read/write subcommands (RFC-0011-g §Subcommand Taxonomy).
    ///
    /// Phase 1 lands `octo governance snapshot` (mission
    /// `0011-g-governance-snapshot`); the `attest` + `vote`
    /// surface waits for the RFC-0855p-d + RFC-0855p-e + RFC-0011-d
    /// Phase 1 conjunction (mission
    /// `0011-g-governance-attest-vote`).
    Governance {
        /// Governance subcommand.
        #[command(subcommand)]
        action: GovernanceAction,
    },
    /// Settlement-receipt read surface (RFC-0011-a §Subcommand Taxonomy).
    ///
    /// Phase 1 lands `octo audit list` + `octo audit show`
    /// (mission `0011-a-audit-commands`); the `redact | export |
    /// watch` surface lands in follow-on amendments per RFC-0011-a
    /// §Future Work.
    Audit {
        /// Audit subcommand.
        #[command(subcommand)]
        action: AuditAction,
    },
    /// Network read surface (RFC-0011-i §Subcommand Taxonomy Phase 1).
    ///
    /// Phase 1 lands 5 read-only subcommands: `peers list`, `peers get`,
    /// `identity show`, `trust-graph render`, `governance rotation
    /// status`. `octo network bootstrap` + `octo network status` are
    /// DEFERRED per RFC-0011-h row 543 footnote pending user decision
    /// on write-path gate surface (G25 amendment).
    Network {
        /// Network subcommand.
        #[command(subcommand)]
        action: NetworkAction,
    },
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
