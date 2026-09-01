//! `RoleAction` — CLI-side dispatch enum per RFC-0011-d §7.2.
//!
//! `#[non_exhaustive]` per F-14 (preserves upgrade path; new CLI
//! actions land without central enum edit).

use clap::Subcommand;

/// CLI-side dispatch enum for `octo role {action}`.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoleAction {
    /// `octo role list` — enumerate roles in the registry.
    List,
    /// `octo role show <role_id>` — fetch a single role record.
    Show {
        /// Role slug to display.
        role_id: String,
    },
    /// `octo role select <role_id>` — bind the active identity to a role.
    Select {
        /// Role slug to select.
        role_id: String,
    },
}