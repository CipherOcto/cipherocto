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

use crate::commands::identity::{active_signer_for_did, require_confirm};
use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::output::OutputEnvelope;
use crate::Octo;

/// CLI-facing role subcommand enum (Layer C; delegates to `octo_role`
/// substrate for decisions). `#[non_exhaustive]` per F-14.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
///
/// `signature_proof` and `role_binding_hash` are substrate truth — the
/// CLI renders them through `RedactedHex` at the envelope boundary so
/// neither byte nor hex escape into operator-visible output. The
/// substrate types are wrapped in a CLI-local projection (`RoleSelectOutput`)
/// before the envelope is rendered; the substrate `RoleBinding`
/// (with raw `Vec<u8>` signature) never reaches the renderer.
fn select_role(role_id: &str, cli: &Octo) -> Result<(), OctoCliError> {
    // Wave 2 HIGH-1 / HIGH-2: route through the canonical confirmation
    // gate (`require_confirm`) instead of the inline `--confirm` only
    // check. `require_confirm` enforces:
    //   * Auditor is denied regardless of `--confirm` / `--dry-run`
    //   * Human mode requires BOTH `--confirm` AND `--confirm-acknowledge`
    //     (pastejacking defense, two-step gate per RFC-0011 §Security 1a)
    //   * Ci / Dev modes require `--allow-write`
    // Without this routing, `octo role select --confirm` (no
    // `--confirm-acknowledge`) bypassed the pastejacking defense, and
    // Auditor mode could fire the slash-ledger write.
    require_confirm(cli, "role select")?;
    // Resolve operator DID + signer from active identity (dev-mode gated
    // by `active_signer_for_did`).
    let signer_arc = active_signer_for_did(cli)?;
    let operator_did = signer_arc.did();
    let signer: &dyn CapabilitySigner = signer_arc.as_ref();
    let binding: RoleBinding = substrate_select(role_id, &operator_did, signer, binding_store())
        .map_err(map_role_error)?;
    // Wrap the substrate binding in the CLI-local projection before the
    // envelope render — keeps the redaction boundary at the CLI layer
    // (the substrate stays substrate-truth; the CLI owns operator output).
    let projected = RoleSelectOutput::from_binding(binding);
    render_envelope("octo.role.select.v1", projected, cli)
}

/// CLI-local projection of [`RoleBinding`].
///
/// R12 CRITICAL-2: the substrate `RoleBinding.signature_proof` is a raw
/// `Vec<u8>` (64-byte Ed25519 signature) — surfacing it through the
/// envelope as raw bytes leaks the substrate secret. The CLI owns the
/// redaction boundary; this projection wraps the bytes in `RedactedHex`
/// so the rendered JSON / pretty output is `[REDACTED:sig]` regardless
/// of inner contents. `role_binding_hash` is hashed material (NOT a
/// secret) but is also wrapped in `RedactedHex` for symmetry with the
/// substrate RFC-0011-d §Key Files redaction contract.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub(crate) struct RoleSelectOutput {
    pub role_id: String,
    pub operator_did: String,
    #[schemars(with = "String")]
    pub chain_id: crate::redact::RedactedHex,
    #[schemars(with = "String")]
    pub role_kind_uuid: crate::redact::RedactedHex,
    pub stake_octo: u64,
    pub stake_role_token: Option<u64>,
    #[schemars(with = "String")]
    pub body_hash: crate::redact::RedactedHex,
    #[schemars(with = "String")]
    pub signature_proof: crate::redact::RedactedHex,
    #[schemars(with = "String")]
    pub role_binding_hash: crate::redact::RedactedHex,
    pub nonce: u64,
}

impl RoleSelectOutput {
    fn from_binding(b: RoleBinding) -> Self {
        Self {
            role_id: b.role_id,
            operator_did: b.operator_did,
            chain_id: crate::redact::RedactedHex(b.chain_id.to_vec()),
            role_kind_uuid: crate::redact::RedactedHex(b.role_kind_uuid.to_vec()),
            stake_octo: b.stake_octo,
            stake_role_token: b.stake_role_token,
            body_hash: crate::redact::RedactedHex(b.body_hash.to_vec()),
            signature_proof: crate::redact::RedactedHex(b.signature_proof),
            role_binding_hash: crate::redact::RedactedHex(b.role_binding_hash.to_vec()),
            nonce: b.nonce,
        }
    }
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
        RoleError::StakeInsufficient {
            required,
            available,
        } => OctoCliError::StakeInsufficient {
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
        RoleError::SigningFailed { reason } => {
            // Map substrate-level signing failure to the existing CLI
            // exit-11 variant (`SigningFailed`). The reason string is
            // sanitized via the redaction layer at the envelope render
            // boundary; do not echo the raw substrate error here.
            OctoCliError::SigningFailed(reason)
        }
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
        let _ = map_role_error(RoleError::SigningFailed {
            reason: "hsm timeout".into(),
        });
    }
}
