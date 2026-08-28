//! Command dispatch — RFC-0011 §Binary Surface.

pub mod capability;
pub mod identity;
pub mod policy;
pub mod stub;

use crate::error::OctoCliError;
use crate::{Commands, Octo};

/// Route a parsed invocation to its command handler.
pub fn dispatch(cli: &Octo) -> Result<(), OctoCliError> {
    match &cli.command {
        Commands::Whoami => identity::whoami(cli),
        Commands::Identity { action } => identity::dispatch(action, cli),
        Commands::Capability { action } => capability::dispatch(action, cli),
        Commands::Policy { action } => policy::dispatch(action, cli),
        Commands::Init => {
            stub::print_deprecated("init", "use octo-wallet init (out of scope for this RFC)")
        }
        Commands::Join => stub::print_deprecated(
            "join",
            "use octo network bootstrap (out of scope for this RFC)",
        ),
        Commands::Status => stub::print_deprecated(
            "status",
            "use octo network status (per Status header amendment chain)",
        ),
        Commands::Role { action } => stub::print_role_deprecated(action),
        Commands::Agent { action } => stub::print_agent_deprecated(action),
    }
}
