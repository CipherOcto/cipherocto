//! `octo` operator CLI — RFC-0011.

#![warn(missing_docs)]

pub mod commands;
pub mod error;
pub mod flags;
pub mod output;
pub mod redact;

pub use error::{sanitize_substrate_error, OctoCliError};
pub use flags::{OperatorMode, OperatorModeFlags, OutputFlags};
pub use output::{Hex32, OutputEnvelope};

use clap::{Parser, Subcommand};
use commands::{capability::CapabilityAction, identity::IdentityAction, policy::PolicyAction};

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
#[derive(Subcommand, Debug)]
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
    /// Deprecated — see RFC-0011 §Compatibility.
    #[command(hide = true)]
    Role {
        /// Deprecated role subcommand.
        #[command(subcommand)]
        action: RoleActionStub,
    },
    /// Deprecated — see RFC-0011 §Compatibility.
    #[command(hide = true)]
    Agent {
        /// Deprecated agent subcommand.
        #[command(subcommand)]
        action: AgentActionStub,
    },
}

/// Deprecated `role` subcommands.
#[derive(Subcommand, Debug)]
pub enum RoleActionStub {
    /// Deprecated.
    Builder,
    /// Deprecated.
    Provider,
    /// Deprecated.
    Storage,
    /// Deprecated.
    Bandwidth,
    /// Deprecated.
    Orchestrator,
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
