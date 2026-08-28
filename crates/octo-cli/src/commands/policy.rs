//! `octo policy` — RFC-0011 §Policy Commands.
//!
//! Wave 2 placeholder: the clap surface is final; the handlers land in Wave 3.

use crate::error::OctoCliError;
use crate::Octo;
use clap::Subcommand;

/// Policy subcommands.
#[derive(Subcommand, Debug)]
pub enum PolicyAction {
    /// Show a policy record.
    Show {
        /// Policy name.
        name: String,
        /// Specific version (defaults to latest).
        #[arg(long)]
        version: Option<u32>,
        /// Policy kind discriminator.
        #[arg(long)]
        kind_uuid: Option<String>,
    },
    /// List registered policies.
    List {
        /// Filter expression (`key=value`).
        #[arg(long)]
        filter: Option<String>,
    },
}

/// Dispatch a policy subcommand.
pub fn dispatch(_action: &PolicyAction, _cli: &Octo) -> Result<(), OctoCliError> {
    Err(OctoCliError::Internal(
        "policy commands pending Wave 3 implementation".into(),
    ))
}
