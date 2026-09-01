//! `octo role {list, show, select}` — RFC-0011-d §7.4 subcommand surface.
//!
//! Thin Layer C wrapper over the `octo-role` substrate crate. Operator
//! invocation → clap parse → substrate call → JSON envelope render. No
//! business logic in this module; all decisions live in
//! `octo_role::{list, show, select}`.

use std::sync::Arc;

use clap::Subcommand;
use octo_cap_macaroon::signer::CapabilitySigner;
use octo_role::{
    select as substrate_select, BindingStore, RoleBinding, RoleError, RoleFilter, RoleRecord,
    RoleSummary,
};

use crate::commands::identity::active_signer_for_did;
use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::output::OutputEnvelope;
use crate::Octo;

/// CLI-facing role subcommand enum (Layer C; delegates to `octo_role::RoleAction`
/// for substrate decisions).
#[derive(Subcommand, Debug)]
pub enum RoleAction {
    /// List registered roles.
    List {
        /// Filter by role kind (`builder`, `provider`, ...).
        #[arg(long)]
        kind: Option<String>,
        /// Filter by classification (`infrastructure`, `economic`, ...).
        #[arg(long)]
        class: Option<String>,
        /// Filter by minimum OCTO stake (micro-OCTO).
        #[arg(long)]
        requires_octo_min: Option<u64>,
    },
    /// Show a single role record.
    Show {
        /// Role slug (e.g., `builder`).
        role_id: String,
    },
    /// Bind the active operator DID to a role (writes to the slash ledger).
    Select {
        /// Role slug to bind to.
        role_id: String,
    },
}

/// Shared in-process binding store (Phase 1; substrate replaces with
/// slash-ledger-backed implementation per RFC-0900).
fn binding_store() -> &'static BindingStore {
    use std::sync::OnceLock;
    static STORE: OnceLock<BindingStore> = OnceLock::new();
    STORE.get_or_init(BindingStore::new)
}

/// Dispatch a parsed `octo role ...` invocation to its handler.
pub fn dispatch(action: &RoleAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        RoleAction::List {
            kind,
            class,
            requires_octo_min,
        } => list_roles(kind.clone(), class.clone(), *requires_octo_min, cli),
        RoleAction::Show { role_id } => show_role(role_id, cli),
        RoleAction::Select { role_id } => select_role(role_id, cli),
    }
}

/// `octo role list [--kind <slug>] [--class <tag>] [--requires-octo-min <N>]`
fn list_roles(
    kind: Option<String>,
    class: Option<String>,
    requires_octo_min: Option<u64>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let filter = RoleFilter {
        kind,
        class,
        requires_octo_min,
    };
    let summaries: Vec<RoleSummary> = octo_role::list(&filter);
    render_envelope("octo.role.list.v1", summaries, cli)
}

/// `octo role show <role_id>`
fn show_role(role_id: &str, cli: &Octo) -> Result<(), OctoCliError> {
    let record: RoleRecord = octo_role::show(role_id).map_err(map_role_error)?;
    render_envelope("octo.role.show.v1", record, cli)
}

/// `octo role select <role_id> --confirm`
fn select_role(role_id: &str, cli: &Octo) -> Result<(), OctoCliError> {
    if !cli.mode.confirm {
        return Err(OctoCliError::ConfirmationRequired {
            command: format!("role select {role_id}"),
        });
    }
    // Resolve operator DID + signer from active identity.
    let signer_arc = active_signer_for_did()?;
    let operator_did = signer_arc.did();
    let signer: &dyn CapabilitySigner = signer_arc.as_ref();
    let binding: RoleBinding =
        substrate_select(role_id, &operator_did, signer, binding_store()).map_err(map_role_error)?;
    render_envelope("octo.role.select.v1", binding, cli)
}

/// Render an output envelope for the given payload (serializable).
fn render_envelope<T: serde::Serialize>(
    _schema: &str,
    data: T,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Schema string is currently captured in test-vector documentation
    // only; OutputEnvelope carries the schema via `SCHEMA_VERSION`.
    let env = OutputEnvelope::new(data, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// Map [`RoleError`] from `octo-role` to the operator-facing [`OctoCliError`].
fn map_role_error(e: RoleError) -> OctoCliError {
    match e {
        RoleError::RoleNotFound { role_id } => OctoCliError::RoleNotFound(role_id),
        RoleError::StakeInsufficient { required, available } => OctoCliError::StakeInsufficient {
            required,
            available,
        },
        RoleError::RoleNotSelectable { role_id, reason } => {
            OctoCliError::RoleNotSelectable { role_id, reason }
        }
        RoleError::SignerMismatch {
            signer_did,
            operator_did,
        } => OctoCliError::SignerMismatch {
            signer_did,
            operator_did,
        },
    }
}

// ---------------------------------------------------------------------------
// Helpers: hide substrate signatures from this module's callers.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct SignerHandle {
    pub(crate) inner: Arc<dyn CapabilitySigner>,
    pub(crate) did: String,
}

impl SignerHandle {
    pub(crate) fn did(&self) -> String {
        self.did.clone()
    }
    pub(crate) fn as_ref(&self) -> &dyn CapabilitySigner {
        self.inner.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_role_error_all_variants() {
        let _ = map_role_error(RoleError::RoleNotFound {
            role_id: "r".into(),
        });
        let _ = map_role_error(RoleError::StakeInsufficient {
            required: 1,
            available: 0,
        });
        let _ = map_role_error(RoleError::RoleNotSelectable {
            role_id: "r".into(),
            reason: "x".into(),
        });
        let _ = map_role_error(RoleError::SignerMismatch {
            signer_did: "a".into(),
            operator_did: "b".into(),
        });
    }
}
