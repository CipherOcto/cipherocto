//! `octo capability` — RFC-0011 §Capability Commands.
//!
//! Wave 2 placeholder: the clap surface is final; the handlers land in Wave 3.

use crate::error::OctoCliError;
use crate::Octo;
use clap::Subcommand;

/// Capability subcommands.
#[derive(Subcommand, Debug)]
pub enum CapabilityAction {
    /// List active capabilities.
    List,
    /// Mint a new capability.
    Mint {
        /// Caveat expression.
        #[arg(long)]
        caveats: String,
        /// Holder DID.
        #[arg(long)]
        holder: String,
        /// Root capability identifier.
        #[arg(long)]
        root: Option<String>,
        /// Acknowledge that minting grants authority.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
    /// Attenuate an existing capability.
    Attenuate {
        /// Parent capability identifier.
        cap_id: String,
        /// Additional caveats to apply.
        #[arg(long)]
        caveats: String,
        /// Acknowledge that attenuation issues a new capability.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
}

/// Dispatch a capability subcommand.
pub fn dispatch(_action: &CapabilityAction, _cli: &Octo) -> Result<(), OctoCliError> {
    Err(OctoCliError::Internal(
        "capability commands pending Wave 3 implementation".into(),
    ))
}
