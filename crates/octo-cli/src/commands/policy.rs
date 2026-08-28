//! `octo policy` — RFC-0011 §Policy Commands.
//!
//! Layer C/D orchestrator over the Layer-B `octo-policy` substrate
//! (`show`, `list`, `latest_version`, `name_hash_index`). The handlers
//! resolve `(name, version)` via `latest_version` (or honour an explicit
//! `--version`), apply the `Redaction Layer` pass to `body`, and render
//! the result through `OutputEnvelope<T>`.

#![allow(clippy::module_name_repetitions)]

use crate::error::OctoCliError;
use crate::output::OutputEnvelope;
use crate::Octo;
use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::Serialize;

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
        /// Filter expression (`key=value[,key=value]`).
        #[arg(long)]
        filter: Option<String>,
    },
}

/// Render payload for `octo policy show`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PolicyShowOutput {
    /// Policy name.
    pub name: String,
    /// Hex-encoded kind UUID.
    pub kind_uuid: String,
    /// Hex-encoded policy body (post-redaction).
    pub body: String,
    /// Execution class label.
    pub execution_class: String,
    /// DID of the registrant.
    pub registered_by_did: String,
    /// Registration timestamp.
    pub registered_at: DateTime<Utc>,
    /// Revocation timestamp (if revoked).
    pub revoked_at: Option<DateTime<Utc>>,
    /// DID of the revoker (if revoked).
    pub revoked_by_did: Option<String>,
    /// Free-form revocation reason.
    pub revocation_reason: Option<String>,
    /// Hex-encoded superseding policy hash (if superseded).
    pub superseding_policy_hash: Option<String>,
}

/// Render payload for `octo policy list`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PolicyListOutput {
    /// Policies matching the filter.
    pub policies: Vec<PolicySummary>,
}

/// One row of a `policy list` render.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PolicySummary {
    /// Policy name.
    pub name: String,
    /// Kind tag (v1 stub: hex prefix of the kind UUID).
    pub kind: String,
    /// Execution class label.
    pub execution_class: String,
    /// Version of the registered policy.
    pub version: u32,
}

/// Map a substrate `PolicyRegistryError` to the CLI error envelope.
fn map_registry_error(e: octo_policy::PolicyRegistryError) -> OctoCliError {
    match e {
        octo_policy::PolicyRegistryError::NotFound(name) => OctoCliError::PolicyNotFound(name),
        octo_policy::PolicyRegistryError::VersionMismatch { policy, version } => {
            OctoCliError::PolicyVersionNotFound { policy, version }
        }
        octo_policy::PolicyRegistryError::Internal(s) => OctoCliError::Internal(s),
    }
}

/// Handle `octo policy show <name>`.
pub fn show(name: &str, version_arg: Option<u32>, cli: &Octo) -> Result<(), OctoCliError> {
    // Version resolution: explicit `--version` wins; otherwise ask the
    // substrate for the latest version via `NameHashIndex`.
    let version = match version_arg {
        Some(v) => v,
        None => octo_policy::latest_version(name).map_err(map_registry_error)?,
    };
    let record = octo_policy::show(name, version).map_err(map_registry_error)?;
    // Defense-in-depth redactor pass per RFC-0011 §Redaction Layer.
    let redacted = redact_body(&record.body);
    let output = PolicyShowOutput {
        name: record.name,
        kind_uuid: hex::encode(record.kind_uuid),
        body: hex::encode(redacted),
        execution_class: record.execution_class,
        registered_by_did: record.registered_by_did,
        registered_at: DateTime::<Utc>::from_timestamp(record.registered_at_unix, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
        revoked_at: record
            .revoked_at_unix
            .and_then(|u| DateTime::<Utc>::from_timestamp(u, 0)),
        revoked_by_did: record.revoked_by_did,
        revocation_reason: record.revocation_reason,
        superseding_policy_hash: record.superseding_policy_hash.map(hex::encode),
    };
    let env = OutputEnvelope::new(output, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render failed: {e}")))
}

/// Handle `octo policy list`.
pub fn list(filter_arg: Option<&str>, cli: &Octo) -> Result<(), OctoCliError> {
    let filter = match filter_arg {
        Some(s) => parse_filter(s)?,
        None => octo_policy::PolicyFilter::default(),
    };
    let entries = octo_policy::list(&filter).map_err(|e| OctoCliError::Internal(e.to_string()))?;
    let summaries: Vec<PolicySummary> = entries
        .into_iter()
        .map(|e| PolicySummary {
            name: e.name,
            kind: hex::encode(&e.kind_uuid[..2]),
            execution_class: e.execution_class,
            version: e.version,
        })
        .collect();
    let output = PolicyListOutput {
        policies: summaries,
    };
    let env = OutputEnvelope::new(output, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render failed: {e}")))
}

/// Parse a `key=value[,key=value]` filter expression.
///
/// Recognised keys: `kind`, `class`. Unknown keys or malformed
/// `key=value` pairs surface as `OctoCliError::InvalidFilter` (CLI-side
/// parse error; the substrate has no `InvalidFilter` variant per R1
/// substrate alignment review).
pub fn parse_filter(s: &str) -> Result<octo_policy::PolicyFilter, OctoCliError> {
    let mut filter = octo_policy::PolicyFilter::default();
    for part in s.split(',') {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| OctoCliError::InvalidFilter(s.to_string()))?;
        match k {
            "kind" => filter.kind = Some(v.to_string()),
            "class" => filter.execution_class = Some(v.to_string()),
            _ => return Err(OctoCliError::InvalidFilter(s.to_string())),
        }
    }
    Ok(filter)
}

/// Apply the `Redaction Layer` pass to a policy body.
///
/// Defense in depth per RFC-0011 §Redaction Layer: substrate enforces
/// redaction at write time (RFC-0967), and the CLI does a final sweep
/// before display. The body is interpreted as UTF-8 (lossy for binary
/// bodies — acceptable for v1; substrate-defined structure).
pub fn redact_body(body: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(body);
    let redacted = crate::redact::redact_string(&s);
    redacted.as_bytes().to_vec()
}

/// Dispatch a `PolicyAction` to its handler.
pub fn dispatch(action: &PolicyAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        PolicyAction::Show { name, version, .. } => show(name, *version, cli),
        PolicyAction::List { filter, .. } => list(filter.as_deref(), cli),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_policy::PolicyFilter;

    #[test]
    fn parse_filter_kind_only() {
        let f = parse_filter("kind=rate_limit").unwrap();
        assert_eq!(f.kind.as_deref(), Some("rate_limit"));
        assert_eq!(f.execution_class, None);
    }

    #[test]
    fn parse_filter_class_only() {
        let f = parse_filter("class=high").unwrap();
        assert_eq!(f.kind, None);
        assert_eq!(f.execution_class.as_deref(), Some("high"));
    }

    #[test]
    fn parse_filter_kind_and_class() {
        let f = parse_filter("kind=rate_limit,class=high").unwrap();
        assert_eq!(f.kind.as_deref(), Some("rate_limit"));
        assert_eq!(f.execution_class.as_deref(), Some("high"));
    }

    #[test]
    fn parse_filter_unknown_key_rejected() {
        let err = parse_filter("bogus=1").unwrap_err();
        assert!(matches!(err, OctoCliError::InvalidFilter(_)));
    }

    #[test]
    fn parse_filter_missing_equals_rejected() {
        let err = parse_filter("bogus").unwrap_err();
        assert!(matches!(err, OctoCliError::InvalidFilter(_)));
    }

    #[test]
    fn parse_filter_empty_rejected() {
        let err = parse_filter("").unwrap_err();
        assert!(matches!(err, OctoCliError::InvalidFilter(_)));
    }

    #[test]
    fn parse_filter_default_when_omitted() {
        let f = parse_filter("kind=rate_limit").unwrap();
        // Default-constructed PolicyFilter has both fields None.
        let default = PolicyFilter::default();
        assert_eq!(f.execution_class, default.execution_class);
    }

    #[test]
    fn redact_body_password_value_replaced() {
        let body = b"password=hunter2 rest=ok";
        let out = redact_body(body);
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("[REDACTED:"), "{s}");
        assert!(!s.contains("hunter2"), "{s}");
    }

    #[test]
    fn redact_body_safe_string_unchanged() {
        let body = b"kind=rate_limit version=3";
        let out = redact_body(body);
        let s = std::str::from_utf8(&out).unwrap();
        assert_eq!(s, "kind=rate_limit version=3");
    }

    #[test]
    fn map_not_found_to_cli_policy_not_found() {
        let e = octo_policy::PolicyRegistryError::NotFound("rate_limit".into());
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::PolicyNotFound(n) => assert_eq!(n, "rate_limit"),
            other => panic!("expected PolicyNotFound, got {other:?}"),
        }
    }

    #[test]
    fn map_version_mismatch_to_cli_version_not_found() {
        let e = octo_policy::PolicyRegistryError::VersionMismatch {
            policy: "rate_limit".into(),
            version: 999,
        };
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::PolicyVersionNotFound { policy, version } => {
                assert_eq!(policy, "rate_limit");
                assert_eq!(version, 999);
            }
            other => panic!("expected PolicyVersionNotFound, got {other:?}"),
        }
    }

    #[test]
    fn map_internal_to_cli_internal() {
        let e = octo_policy::PolicyRegistryError::Internal("boom".into());
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => assert_eq!(s, "boom"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
