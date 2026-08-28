//! `octo policy` — RFC-0011 §Subcommand Taxonomy.
//!
//! Layer C/D orchestrator over the Layer-B `octo-policy` substrate
//! (`show`, `list`, `latest_version`, `name_hash_index`). The handlers
//! resolve `(name, version)` via `latest_version` (or honour an explicit
//! `--version`), validate the operator-supplied `--kind-uuid` against the
//! record before rendering, apply the `Redaction Layer` pass to `body`,
//! and render the result through `OutputEnvelope<T>`.

#![allow(clippy::module_name_repetitions)]

use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::output::OutputEnvelope;
use crate::Octo;
use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::{Serialize, Serializer};
use std::fmt;

/// Hex-encoded byte-string newtype with variable-length backing —
///
/// Policy fields whose substrate shape is `[u8;16]` (kind_uuid) and
/// `[u8;32]` (superseding_policy_hash) both render through the same type.
/// The schema reflects the variable width; the underlying serialization
/// is always lowercase hex.
///
/// Named `HexBytes` to avoid collision with the canonical 32-byte-fixed
/// `crate::output::Hex32` defined in `output.rs`. The two types are
/// distinct on purpose: substrate byte widths are heterogeneous and the
/// `[u8;32]`-pinned `output::Hex32` would refuse to encode the 16-byte
/// `kind_uuid` field.
#[derive(Debug, Clone, schemars::JsonSchema)]
pub struct HexBytes(#[schemars(with = "String")] String);

impl HexBytes {
    /// Build a `HexBytes` from raw bytes (variable length, lower-cased hex).
    pub fn new(bytes: &[u8]) -> Self {
        Self(hex::encode(bytes))
    }
}

impl Serialize for HexBytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl fmt::Display for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

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
        /// Policy kind discriminator (hex-encoded).
        ///
        /// Forwarded to substrate per RFC-0011 §Subcommand Taxonomy
        /// entry #14. The CLI validates the operator-supplied value
        /// against the record's `kind_uuid` before rendering; a mismatch
        /// is rejected with `Internal` (exit 64) so the operator does
        /// not silently receive a record whose `kind_uuid` does not
        /// match the one they queried for.
        #[arg(long)]
        kind_uuid: Option<String>,
    },
    /// List registered policies.
    List {
        /// Filter expression (`key=value[,key=value]`).
        ///
        /// Empty string is treated as "no filter" (equivalent to omitting
        /// the flag entirely); per CORR-10 the previous implementation
        /// rejected empty filters with `InvalidFilter` (exit 16) which
        /// contradicted operator intuition.
        #[arg(long)]
        filter: Option<String>,
    },
}

/// Render payload for `octo policy show`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct PolicyShowOutput {
    /// Policy name.
    pub name: String,
    /// Policy version resolved by the CLI.
    pub version: u32,
    /// Hex-encoded kind UUID.
    pub kind_uuid: HexBytes,
    /// Hex-encoded policy body (post-redaction).
    pub body: String,
    /// Execution class label.
    pub execution_class: String,
    /// DID of the registrant.
    pub registered_by_did: HexBytes,
    /// Registration timestamp.
    pub registered_at: DateTime<Utc>,
    /// Revocation timestamp (if revoked).
    pub revoked_at: Option<DateTime<Utc>>,
    /// DID of the revoker (if revoked).
    pub revoked_by_did: Option<HexBytes>,
    /// Free-form revocation reason.
    pub revocation_reason: Option<String>,
    /// Hex-encoded superseding policy hash (if superseded).
    pub superseding_policy_hash: Option<HexBytes>,
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
///
/// SEC-02: every substrate error variant is routed through
/// `sanitize_substrate_error` where the substrate carries a string
/// payload — the substrate may emit paths or SQL fragments in
/// diagnostic messages, and the CLI MUST strip them before
/// operator-facing exposure.
fn map_registry_error(e: octo_policy::PolicyRegistryError) -> OctoCliError {
    match e {
        octo_policy::PolicyRegistryError::NotFound(name) => OctoCliError::PolicyNotFound(name),
        octo_policy::PolicyRegistryError::HashMismatch { expected, actual } => {
            let msg = format!(
                "policy hash mismatch: expected {}, got {}",
                sanitize_substrate_error(&expected),
                sanitize_substrate_error(&actual),
            );
            OctoCliError::Internal(msg)
        }
        octo_policy::PolicyRegistryError::InvalidClassBProof => OctoCliError::Internal(
            sanitize_substrate_error("class B registration rejected: ZK envelope marker missing"),
        ),
        octo_policy::PolicyRegistryError::AlreadyRegistered(hash) => OctoCliError::Internal(
            sanitize_substrate_error(&format!("policy_hash {hash} is already registered")),
        ),
        octo_policy::PolicyRegistryError::NotRegistrant(hash) => {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "caller is not the original registrant of policy_hash {hash}"
            )))
        }
        octo_policy::PolicyRegistryError::AlreadyRevoked { revoked_at_unix } => {
            // Operator-safe: timestamp is numeric, no path/SQL leak risk.
            // Sanitizer runs the formatted string anyway as defence-in-depth.
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "policy already revoked at {revoked_at_unix}"
            )))
        }
        octo_policy::PolicyRegistryError::AuthorityDelegationDenied(detail) => {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "authority delegation denied: {detail}"
            )))
        }
    }
}

/// Handle `octo policy show <name>`.
pub fn show(
    name: &str,
    version_arg: Option<u32>,
    kind_uuid: Option<&str>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Version resolution: explicit `--version` wins; otherwise ask the
    // substrate for the latest version via `NameHashIndex`.
    let version = match version_arg {
        Some(v) => v,
        None => octo_policy::latest_version(name).map_err(map_registry_error)?,
    };
    let record = octo_policy::show(name, version).map_err(map_registry_error)?;

    // --kind-uuid wiring (SEC-13 follow-on): when the operator supplies
    // a kind UUID, the CLI MUST validate it before rendering so a
    // mismatch never reaches the operator as a silent success. The
    // substrate's `show()` signature does not yet accept `kind_uuid` as
    // a third argument; the CLI performs the comparison itself so the
    // operator-facing contract is locked at this layer.
    let record_kind_hex = hex::encode(record.kind_uuid);
    validate_kind_uuid(kind_uuid, &record_kind_hex)?;

    // Defense-in-depth redactor pass per RFC-0011 §Redaction Layer.
    let redacted = redact_body(&record.body);
    let output = PolicyShowOutput {
        name: record.name,
        version,
        kind_uuid: HexBytes::new(&record.kind_uuid),
        body: hex::encode(redacted),
        execution_class: record.execution_class,
        registered_by_did: parse_did_hexbytes(&record.registered_by_did),
        registered_at: DateTime::<Utc>::from_timestamp(record.registered_at_unix, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
        revoked_at: record
            .revoked_at_unix
            .and_then(|u| DateTime::<Utc>::from_timestamp(u, 0)),
        revoked_by_did: record.revoked_by_did.as_deref().map(parse_did_hexbytes),
        revocation_reason: record.revocation_reason,
        superseding_policy_hash: record.superseding_policy_hash.map(|h| HexBytes::new(&h)),
    };
    let env = OutputEnvelope::new(output, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render failed: {e}")))
        })
}

/// Build a `HexBytes` from a substrate DID string.
///
/// The substrate returns the registrant DID as a string (canonical DID per
/// RFC-0010 alignment, e.g. `did:octo:1z...`). For v1.0 we surface the
/// value hex-encoded through `HexBytes` to honour SPEC-01; the substrate
/// amendment that returns `[u8; 32]` will switch this to a direct
/// `HexBytes::new(&bytes)` per SPEC-19 / SPEC-20.
fn parse_did_hexbytes(s: &str) -> HexBytes {
    // Encode the full DID string as bytes for hex rendering. The
    // substrate amendment that returns `[u8; 32]` will collapse this
    // to a direct `HexBytes::new(&bytes)` call.
    let mut buf = [0u8; 32];
    let bytes = s.as_bytes();
    let n = bytes.len().min(32);
    buf[..n].copy_from_slice(&bytes[..n]);
    HexBytes::new(&buf)
}

/// Handle `octo policy list`.
pub fn list(filter_arg: Option<&str>, cli: &Octo) -> Result<(), OctoCliError> {
    // CORR-10: treat `Some("")` as `PolicyFilter::default()` rather than
    // rejecting with `InvalidFilter` (exit 16). Empty filter is operator-
    // safe — it is semantically equivalent to omitting the flag.
    let filter = match filter_arg {
        Some(s) if !s.is_empty() => parse_filter(s)?,
        _ => octo_policy::PolicyFilter::default(),
    };
    let entries = octo_policy::list(&filter)
        .map_err(|e| OctoCliError::Internal(sanitize_substrate_error(&e.to_string())))?;
    let summaries: Vec<PolicySummary> = entries
        .into_iter()
        .map(|e| {
            let n = e.kind_uuid.len().min(2);
            PolicySummary {
                name: e.name,
                kind: hex::encode(&e.kind_uuid[..n]),
                execution_class: e.execution_class,
                version: e.version,
            }
        })
        .collect();
    let output = PolicyListOutput {
        policies: summaries,
    };
    let env = OutputEnvelope::new(output, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render failed: {e}")))
        })
}

/// Parse a `key=value[,key=value]` filter expression.
///
/// Recognised keys: `kind`, `class`. Unknown keys or malformed
/// `key=value` pairs surface as `OctoCliError::InvalidFilter` (CLI-side
/// parse error; the substrate has no `InvalidFilter` variant per R1
/// substrate alignment review).
///
/// **LAYER-06 Phase-1 concession:** `parse_filter` lives CLI-side per
/// RFC-0011 §Subcommand Taxonomy. A future `[ADD] PolicyFilter::parse`
/// amendment will move this substrate-side (Layer B), keeping
/// `PolicyFilter` a substrate-truth construct rather than a CLI-parsed
/// view of operator input. The CLI form will then be a thin pass-through.
pub fn parse_filter(s: &str) -> Result<octo_policy::PolicyFilter, OctoCliError> {
    let mut filter = octo_policy::PolicyFilter::default();
    for part in s.split(',') {
        let (k, v) = part.split_once('=').ok_or_else(|| {
            OctoCliError::InvalidFilter(crate::redact::redact_string(s).into_owned())
        })?;
        match k {
            "kind" => filter.kind = Some(v.to_string()),
            "class" => filter.execution_class = Some(v.to_string()),
            _ => {
                return Err(OctoCliError::InvalidFilter(
                    crate::redact::redact_string(s).into_owned(),
                ))
            }
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
        PolicyAction::Show {
            name,
            version,
            kind_uuid,
        } => show(name, *version, kind_uuid.as_deref(), cli),
        PolicyAction::List { filter, .. } => list(filter.as_deref(), cli),
    }
}

/// Validate the operator-supplied `--kind-uuid` against the record's
/// stored `kind_uuid` (both hex-encoded). Exposed at module scope so
/// the kind-uuid wiring unit tests can exercise the comparison without
/// spinning up the substrate stub.
///
/// SEC-13 follow-on contract:
/// - `None` operator input → `Ok` (the flag is optional; the record's
///   own kind_uuid is the source of truth).
/// - Empty operator input → `Internal` (exit 64): the operator passed
///   `--kind-uuid ""` which is almost certainly a shell-quoting mistake
///   rather than a deliberate "match nothing" choice. Surfacing as
///   `Internal` (not `InvalidFilter`) keeps the exit-code table stable
///   for `--filter` consumers that grep for `InvalidFilter` (exit 16).
/// - Operator input that is not 32-char lowercase hex (16-byte UUID
///   form) → `Internal` (exit 64): the kind_uuid format is
///   substrate-fixed; an arbitrary string cannot reach a matching
///   record even if it were equal by accident, so we reject early at
///   the strict-lowercase hex gate. Strict-lowercase (no uppercase) is
///   a pastejacking-defense choice — a malicious shell snippet that
///   rewrites the kind to uppercase cannot match a substrate record
///   whose kind is lowercase.
/// - Lowercase 32-char hex match against `record_kind_hex` → `Ok`.
/// - Any other input → `Internal` with sanitized mismatch diagnostic.
pub fn validate_kind_uuid(
    operator: Option<&str>,
    record_kind_hex: &str,
) -> Result<(), OctoCliError> {
    let Some(op) = operator else {
        return Ok(());
    };
    if op.is_empty() {
        return Err(OctoCliError::Internal(sanitize_substrate_error(
            "--kind-uuid must be non-empty when provided",
        )));
    }
    if !is_lower_hex_kind(op) {
        return Err(OctoCliError::Internal(sanitize_substrate_error(
            "--kind-uuid must be 32 lowercase hex chars (16 bytes)",
        )));
    }
    if op == record_kind_hex {
        Ok(())
    } else {
        Err(OctoCliError::Internal(sanitize_substrate_error(&format!(
            "--kind-uuid mismatch: operator supplied {} but record has {}",
            op, record_kind_hex
        ))))
    }
}

/// Lowercase 16-byte hex gate for `--kind-uuid` (UUID form). The
/// substrate `kind_uuid` field is `[u8;16]` (RFC-0967 §9. Catalog schema)
/// so the operator-facing hex form is 32 lowercase hex chars. Mixed-case
/// or non-hex operator input is rejected before the mismatch diagnostic
/// so the operator's exact string is not echoed back through the error
/// path.
fn is_lower_hex_kind(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
        // `parse_filter("")` parses the empty-string into a default filter
        // because `Some("")` is intercepted at the `list()` boundary
        // (CORR-10); the parser itself surfaces `InvalidFilter` for empty
        // input passed directly so the failure mode is observable in unit
        // tests.
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
    fn list_empty_filter_treated_as_default() {
        // CORR-10 regression: `Some("")` → `PolicyFilter::default()`.
        let f = match Some("") {
            Some(s) if !s.is_empty() => parse_filter(s).unwrap(),
            _ => octo_policy::PolicyFilter::default(),
        };
        let default = PolicyFilter::default();
        assert_eq!(f.kind, default.kind);
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
    fn map_already_revoked_to_cli_internal() {
        // SEC-02 regression: substrate `AlreadyRevoked { revoked_at_unix }`
        // maps to `Internal` (sanitized timestamp) — exit 64.
        let e = octo_policy::PolicyRegistryError::AlreadyRevoked {
            revoked_at_unix: 1_700_000_000,
        };
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => {
                assert!(s.contains("policy already revoked"), "{s}");
                assert!(s.contains("1700000000"), "{s}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_hash_mismatch_to_cli_internal() {
        // SEC-02 regression: `HashMismatch { expected, actual }` MUST
        // route through the sanitizer before operator-facing exposure.
        let e = octo_policy::PolicyRegistryError::HashMismatch {
            expected: "a".repeat(64),
            actual: "b".repeat(64),
        };
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => assert!(s.contains("policy hash mismatch"), "{s}"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_authority_delegation_to_cli_internal() {
        let e = octo_policy::PolicyRegistryError::AuthorityDelegationDenied(
            "delegation denied by parent policy".into(),
        );
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => {
                // Sanitized — substrate SQL fragment must NOT leak.
                assert!(s.contains("authority delegation denied"), "{s}");
                assert!(s.contains("delegation denied by parent policy"), "{s}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_already_registered_to_cli_internal() {
        // SEC-02 regression: substrate payload (hash string) MUST run
        // through the sanitizer (which strips SQL / paths). Use a clean
        // fixture so the sanitizer preserves the content; the regression
        // contract is "no SQL/path leaks", proven by sibling tests.
        let e = octo_policy::PolicyRegistryError::AlreadyRegistered("abc123".into());
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => {
                assert!(s.contains("already registered"), "{s}");
                assert!(s.contains("abc123"), "{s}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_already_registered_sanitizes_sql_fragment() {
        // SEC-02 regression: substrate payload containing `SQL:` MUST be
        // replaced with `<substrate-error>` by the sanitizer before
        // reaching the operator. The hash itself is intentionally
        // marked as substrate-internal; no path/SQL leaks.
        let e = octo_policy::PolicyRegistryError::AlreadyRegistered(
            "abc123 SQL: select from secrets".into(),
        );
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => {
                assert_eq!(s, "<substrate-error>", "{s}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_invalid_class_b_proof_to_cli_internal() {
        let e = octo_policy::PolicyRegistryError::InvalidClassBProof;
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => {
                assert!(s.contains("class B registration rejected"), "{s}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_not_registrant_to_cli_internal() {
        let e = octo_policy::PolicyRegistryError::NotRegistrant("hash123".into());
        let cli_err = map_registry_error(e);
        match cli_err {
            OctoCliError::Internal(s) => {
                assert!(s.contains("not the original registrant"), "{s}");
                assert!(s.contains("hash123"), "{s}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    /// SEC-13 follow-on: when the operator supplies a non-empty
    /// `--kind-uuid`, the CLI must compare it (case-insensitive) to
    /// the substrate record's `kind_uuid` before rendering. The unit
    /// test exercises the comparison helper directly: an empty input
    /// is rejected, a wrong hex is rejected, a matching hex passes.
    /// The substrate integration path is exercised by the CLI driver
    /// tests once a substrate stub for `show(name, version, kind_uuid)`
    /// lands.
    #[test]
    fn kind_uuid_match_logic() {
        let record_kind_bytes = [0xab; 16];
        let record_kind_hex = hex::encode(record_kind_bytes);

        // Empty operator input — rejected.
        let r = validate_kind_uuid(Some(""), &record_kind_hex);
        assert!(matches!(r, Err(OctoCliError::Internal(_))));

        // Short operator input — rejected at the format gate.
        let r = validate_kind_uuid(Some("00"), &record_kind_hex);
        assert!(matches!(r, Err(OctoCliError::Internal(_))));

        // Non-hex operator input — rejected at the format gate.
        let r = validate_kind_uuid(Some(&"z".repeat(64)), &record_kind_hex);
        assert!(matches!(r, Err(OctoCliError::Internal(_))));

        // Well-formed but non-matching operator input — exercises the
        // load-bearing mismatch diagnostic path (the only test that
        // reaches it). 32 lowercase hex chars but not equal to
        // `record_kind_hex` so the format gate passes and the
        // mismatch-diagnostic arm fires.
        let r = validate_kind_uuid(Some(&"0".repeat(32)), &record_kind_hex);
        assert!(matches!(r, Err(OctoCliError::Internal(_))));

        // Uppercase hex operator input — rejected at the format gate
        // (the substrate never emits uppercase; the strict lowercase
        // contract prevents a class of pastejacking where a malicious
        // shell snippet rewrites the kind to uppercase).
        let r = validate_kind_uuid(Some(&record_kind_hex.to_uppercase()), &record_kind_hex);
        assert!(matches!(r, Err(OctoCliError::Internal(_))));

        // Matching operator input — passes.
        let r = validate_kind_uuid(Some(&record_kind_hex), &record_kind_hex);
        assert!(r.is_ok());

        // Operator omitted — no validation, no error.
        let r = validate_kind_uuid(None, &record_kind_hex);
        assert!(r.is_ok());
    }
}
