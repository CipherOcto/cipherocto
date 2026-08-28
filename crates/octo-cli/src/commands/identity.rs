//! `octo identity` — RFC-0011 §Identity Commands.
//!
//! Wave 2 placeholder: the clap surface is final; the handlers land in Wave 3.

use crate::error::OctoCliError;
use crate::Octo;
use clap::Subcommand;

/// Identity subcommands.
#[derive(Subcommand, Debug)]
pub enum IdentityAction {
    /// Show an identity record (defaults to the active identity).
    Show {
        /// Target DID.
        did: Option<String>,
    },
    /// Begin a key rotation.
    Rotate {
        /// Acknowledge the irreversible effect of rotation.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
    /// Revoke the active identity.
    Revoke {
        /// Revocation reason recorded in the identity log.
        #[arg(long)]
        reason: String,
        /// Acknowledge the irreversible effect of revocation.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
}

/// Dispatch an identity subcommand.
pub fn dispatch(_action: &IdentityAction, _cli: &Octo) -> Result<(), OctoCliError> {
    Err(OctoCliError::Internal(
        "identity commands pending Wave 3 implementation".into(),
    ))
}

/// `octo whoami` — show the active identity.
pub fn whoami(_cli: &Octo) -> Result<(), OctoCliError> {
    Err(OctoCliError::Internal(
        "whoami pending Wave 3 implementation".into(),
    ))
}
