//! `octo role {list, show, select}` — RFC-0011-d §7.4 subcommand surface.
//!
//! Thin Layer C wrapper over the `octo-role` substrate crate. Operator
//! invocation → clap parse → substrate call → JSON envelope render. No
//! business logic in this module; all decisions live in
//! `octo_role::{list, show, select}`.

use std::sync::Arc;

use clap::Subcommand;
use octo_cap_macaroon::signer::CapabilitySigner;
use octo_network::dc::admin_attest::PlatformAdminProof;
use octo_network::dot::binding::{GroupBinding, GroupState};
use octo_role::{
    select as substrate_select, select_coordinator as substrate_select_coordinator,
    select_domain_coordinator as substrate_select_domain_coordinator, BindingStore, RoleBinding,
    RoleError, RoleFilter, RoleRecord, RoleSummary,
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
    ///
    /// M10 (RFC-0011-d §7.4): pass `--coordinator` for generic coordinator
    /// role-binding (emits `HandoverRequestEnvelope`); pass
    /// `--domain-coordinator` for atomic domain-coordinator role-binding
    /// + group binding ceremony (RFC-0855p-c §5a). Both flags require
    ///   `--mission-id` + `--current-epoch`. `--domain-coordinator` also
    ///   requires `--group-jid` + `--platform` + `--platform-admin-proof`.
    #[allow(clippy::struct_excessive_bools)]
    Select {
        /// Role slug to bind to.
        role_id: String,
        /// Select as generic coordinator (emits `HandoverRequestEnvelope`
        /// per RFC-0855p-e). Mutually exclusive with `--domain-coordinator`.
        #[arg(long, conflicts_with = "domain_coordinator")]
        coordinator: bool,
        /// Select as domain coordinator (atomic with RFC-0855p-c §5a
        /// `PlatformAdminProof` verification + group binding update).
        #[arg(long)]
        domain_coordinator: bool,
        /// Mission ID as 64-char lowercase hex (32-byte BLAKE3-256).
        /// Required when `--coordinator` or `--domain-coordinator` is set.
        #[arg(long, value_parser = parse_hash32_hex)]
        mission_id: Option<[u8; 32]>,
        /// Current network consensus epoch (required for coordinator flows).
        #[arg(long)]
        current_epoch: Option<u64>,
        /// Group JID (e.g., `120363@g.us`). Required for
        /// `--domain-coordinator`. Phase 1: constructs a fresh
        /// `GroupBinding`; production loads the existing one from
        /// `GroupRegistry` (RFC-0850p-c §4).
        #[arg(long)]
        group_jid: Option<String>,
        /// Platform string (`whatsapp`, `telegram`, `matrix`, ...).
        /// Required for `--domain-coordinator`.
        #[arg(long)]
        platform: Option<String>,
        /// Platform admin proof as JSON (RFC-0855p-c §5a). Required for
        /// `--domain-coordinator`. Pass the proof object emitted by
        /// the platform admin attestation step.
        #[arg(long)]
        platform_admin_proof: Option<String>,
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
        RoleAction::Select {
            role_id,
            coordinator,
            domain_coordinator,
            mission_id,
            current_epoch,
            group_jid,
            platform,
            platform_admin_proof,
        } => select_role(
            role_id,
            *coordinator,
            *domain_coordinator,
            *mission_id,
            *current_epoch,
            group_jid.clone(),
            platform.clone(),
            platform_admin_proof.clone(),
            cli,
        ),
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

/// `octo role select <role_id> [--coordinator | --domain-coordinator] --confirm`
///
/// M10 (RFC-0011-d §7.4) — three invocation modes:
///   * Plain:    `octo role select <role>` → `substrate_select`
///   * `--coordinator`:    → `substrate_select_coordinator` (emits HORQ)
///   * `--domain-coordinator`: → `substrate_select_domain_coordinator`
///     (atomic with RFC-0855p-c §5a group binding ceremony)
///
/// `signature_proof` and `role_binding_hash` are substrate truth — the
/// CLI renders them through `RedactedHex` at the envelope boundary so
/// neither byte nor hex escape into operator-visible output. The
/// substrate types are wrapped in a CLI-local projection (`RoleSelectOutput`)
/// before the envelope is rendered; the substrate `RoleBinding`
/// (with raw `Vec<u8>` signature) never reaches the renderer.
#[allow(clippy::too_many_arguments)]
fn select_role(
    role_id: &str,
    coordinator: bool,
    domain_coordinator: bool,
    mission_id: Option<[u8; 32]>,
    current_epoch: Option<u64>,
    group_jid: Option<String>,
    platform: Option<String>,
    platform_admin_proof_json: Option<String>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    require_confirm(cli, "role select")?;
    let signer_arc = active_signer_for_did(cli)?;
    let operator_did = signer_arc.did();
    let signer: &dyn CapabilitySigner = signer_arc.as_ref();

    let binding = if domain_coordinator {
        // Validate the domain-coordinator flag bundle.
        let mission_id = mission_id.ok_or_else(|| {
            OctoCliError::InvalidFilter("--mission-id is required for --domain-coordinator".into())
        })?;
        let current_epoch = current_epoch.ok_or_else(|| {
            OctoCliError::InvalidFilter(
                "--current-epoch is required for --domain-coordinator".into(),
            )
        })?;
        let group_jid = group_jid.ok_or_else(|| {
            OctoCliError::InvalidFilter("--group-jid is required for --domain-coordinator".into())
        })?;
        let platform = platform.ok_or_else(|| {
            OctoCliError::InvalidFilter("--platform is required for --domain-coordinator".into())
        })?;
        let proof_json = platform_admin_proof_json.ok_or_else(|| {
            OctoCliError::InvalidFilter(
                "--platform-admin-proof is required for --domain-coordinator".into(),
            )
        })?;
        // Parse the platform admin proof JSON (RFC-0855p-c §5a envelope).
        let proof: PlatformAdminProof = serde_json::from_str(&proof_json).map_err(|e| {
            OctoCliError::InvalidFilter(format!("--platform-admin-proof JSON invalid: {e}"))
        })?;
        // Construct a fresh `GroupBinding` for Phase 1. Production wires
        // the binding registry lookup per RFC-0850p-c §4.
        let group_binding = GroupBinding {
            group_jid: group_jid.clone(),
            platform: platform.clone(),
            mission_id,
            domain_id: [0u8; 32],
            domain_coordinator_id: [0u8; 32],
            bound_at_epoch: current_epoch,
            renewed_at_epoch: current_epoch,
            state: GroupState::Bound,
            binding_hash: [0u8; 32],
        };
        let (role_binding, _updated_binding) = substrate_select_domain_coordinator(
            role_id,
            &operator_did,
            signer,
            &octo_role::default_chain_id(),
            binding_store(),
            &group_binding,
            &proof,
            current_epoch,
        )
        .map_err(map_role_error)?;
        role_binding
    } else if coordinator {
        let mission_id = mission_id.ok_or_else(|| {
            OctoCliError::InvalidFilter("--mission-id is required for --coordinator".into())
        })?;
        let current_epoch = current_epoch.ok_or_else(|| {
            OctoCliError::InvalidFilter("--current-epoch is required for --coordinator".into())
        })?;
        substrate_select_coordinator(
            role_id,
            &operator_did,
            signer,
            &octo_role::default_chain_id(),
            binding_store(),
            mission_id,
            current_epoch,
        )
        .map_err(map_role_error)?
    } else {
        // Plain `octo role select <role>` path.
        substrate_select(role_id, &operator_did, signer, binding_store()).map_err(map_role_error)?
    };

    let projected = RoleSelectOutput::from_binding(binding);
    render_envelope("octo.role.select.v1", projected, cli)
}

/// Parse 64-char lowercase hex string into a 32-byte array. Used by
/// `--mission-id` per M10 (RFC-0011-d §7.4 + RFC-0855p-e).
///
/// Canonical lowercase alphabet per RFC-0010 §OctoID Codec; matches
/// `octo_cap_macaroon::signer::did_from_pubkey` output. Uppercase hex
/// is rejected (fails closed) — see `hex_nibble` below for the
/// shared rationale.
fn parse_hash32_hex(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!(
            "expected 64 hex chars (32 bytes), got {} chars",
            s.len()
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0]).ok_or_else(|| {
            format!(
                "invalid hex char at position {}: {:?}",
                i * 2,
                chunk[0] as char
            )
        })?;
        let lo = hex_nibble(chunk[1]).ok_or_else(|| {
            format!(
                "invalid hex char at position {}: {:?}",
                i * 2 + 1,
                chunk[1] as char
            )
        })?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

/// Lowercase hex nibble. Alphabet matches
/// `octo_cap_macaroon::signer::hex_nibble` (lowercase-only canonical
/// form per RFC-0010 §OctoID Codec). Uppercase rejected to fail closed.
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
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
        RoleError::GroupBindingRejected { reason } => OctoCliError::GroupBindingRejected { reason },
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
        let _ = map_role_error(RoleError::GroupBindingRejected {
            reason: "stale proof".into(),
        });
    }

    #[test]
    fn map_role_error_6_variants() {
        // M10 adds `GroupBindingRejected` per RFC-0011-d §Error Handling.
        // 6 substrate variants now; CLI maps each to a typed exit code.
        assert!(matches!(
            map_role_error(RoleError::RoleNotFound {
                role_id: "x".into()
            }),
            OctoCliError::RoleNotFound(_)
        ));
        assert!(matches!(
            map_role_error(RoleError::StakeInsufficient {
                required: 1,
                available: 0
            }),
            OctoCliError::StakeInsufficient { .. }
        ));
        assert!(matches!(
            map_role_error(RoleError::RoleNotSelectable {
                role_id: "x".into(),
                reason: "y".into()
            }),
            OctoCliError::RoleNotSelectable { .. }
        ));
        assert!(matches!(
            map_role_error(RoleError::SignerMismatch {
                signer_did: "a".into(),
                operator_did: "b".into()
            }),
            OctoCliError::SignerMismatch { .. }
        ));
        assert!(matches!(
            map_role_error(RoleError::SigningFailed { reason: "x".into() }),
            OctoCliError::SigningFailed(_)
        ));
        assert!(matches!(
            map_role_error(RoleError::GroupBindingRejected { reason: "x".into() }),
            OctoCliError::GroupBindingRejected { .. }
        ));
    }

    #[test]
    fn parse_hash32_hex_round_trip() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let arr = parse_hash32_hex(hex).expect("valid hex");
        assert_eq!(arr[0], 0x00);
        assert_eq!(arr[1], 0x11);
        assert_eq!(arr[31], 0xff);
    }

    #[test]
    fn parse_hash32_hex_rejects_wrong_length() {
        assert!(parse_hash32_hex("abcd").is_err());
        assert!(parse_hash32_hex("").is_err());
    }

    #[test]
    fn parse_hash32_hex_rejects_non_hex() {
        let bad = "zz112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        assert!(parse_hash32_hex(bad).is_err());
    }

    #[test]
    fn parse_hash32_hex_rejects_uppercase() {
        // M10 parser is canonical lowercase per RFC-0010 §OctoID Codec.
        // Uppercase hex chars MUST be rejected (fails closed) to prevent
        // ambiguous canonical forms from leaking into substrate.
        let upper = "00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF";
        assert!(parse_hash32_hex(upper).is_err());
    }

    #[test]
    fn hex_nibble_round_trip() {
        for b in 0u8..16 {
            let c = if b < 10 { b'0' + b } else { b'a' + b - 10 };
            assert_eq!(hex_nibble(c), Some(b));
        }
        assert_eq!(hex_nibble(b'Z'), None);
    }
}
