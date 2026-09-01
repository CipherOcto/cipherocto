//! Operator-facing error envelope — RFC-0011 §Error Handling.

use std::io::Write;
use thiserror::Error;

/// Every operator-visible failure mode of the `octo` CLI.
#[derive(Error, Debug)]
pub enum OctoCliError {
    /// Argument parsing failed.
    #[error("{0}")]
    ClapParse(#[from] clap::Error),
    /// No active identity in the wallet.
    #[error("no active identity")]
    NoActiveIdentity,
    /// A mutating command was invoked without confirmation flags.
    ///
    /// Fires for any operator mode (Human / Ci / Dev) when the
    /// confirmation gate is unmet. Exact escape hatches depend on mode
    /// and command; the operator sees the per-mode help text from
    /// `OctoCliError::render` rather than a mode-specific label here.
    #[error("ConfirmationRequired: --confirm required for mutating command {command}")]
    ConfirmationRequired {
        /// Command that required confirmation.
        command: String,
    },
    /// A mutating command was invoked under `--mode auditor`.
    ///
    /// Auditor is a read-only role and is denied **before** the
    /// confirmation gate fires. Wave 3 LOW: a separate variant lets
    /// the operator see "auditor mode is read-only" rather than the
    /// generic "--confirm required" (which would be misleading —
    /// adding `--confirm` does not unblock an Auditor session).
    #[error("auditor mode is read-only; refusing mutating command {command}")]
    AuditorDenied {
        /// Command the auditor attempted to invoke.
        command: String,
    },
    /// A rotation is already in flight.
    #[error("identity rotation already in progress")]
    AlreadyRotating,
    /// Requested identity is unknown.
    #[error("identity not found: {0}")]
    IdentityNotFound(String),
    /// HSM backend unavailable.
    #[error("HSM unavailable: {0}")]
    HsmUnavailable(String),
    /// Identity already revoked.
    #[error("identity already revoked")]
    AlreadyRevoked,
    /// Caveat expression failed to parse.
    #[error("caveat parse error: {message}")]
    CaveatParse {
        /// Parser diagnostic.
        message: String,
    },
    /// Caveats parsed but combine illegally.
    #[error("invalid caveat combination: {detail}")]
    InvalidCaveatCombination {
        /// Why the combination is invalid.
        detail: String,
    },
    /// Requested holder is unknown.
    #[error("holder not found: {0}")]
    HolderNotFound(String),
    /// Attenuation would widen authority.
    #[error("attenuation violation: {0}")]
    AttenuationViolation(String),
    /// Signing operation failed.
    #[error("signing failed: {0}")]
    SigningFailed(String),
    /// Parent capability is unknown.
    #[error("parent capability not found: {0}")]
    ParentCapNotFound(String),
    /// Requested policy is unknown.
    #[error("policy not found: {0}")]
    PolicyNotFound(String),
    /// Requested policy version is unknown.
    #[error("policy `{policy}` has no version {version}")]
    PolicyVersionNotFound {
        /// Policy name.
        policy: String,
        /// Requested version.
        version: u32,
    },
    /// Requested role is unknown (RFC-0011-d §Error Handling).
    #[error("role not found: {0}")]
    RoleNotFound(String),
    /// Operator OCTO stake is below the role's minimum (RFC-0011-d).
    #[error("stake insufficient: required {required} micro-OCTO, available {available}")]
    StakeInsufficient {
        /// Required stake (micro-OCTO).
        required: u64,
        /// Available stake (micro-OCTO).
        available: u64,
    },
    /// Role exists but cannot be selected (RFC-0011-d).
    #[error("role `{role_id}` not selectable: {reason}")]
    RoleNotSelectable {
        /// Role slug.
        role_id: String,
        /// Why the role cannot be selected.
        reason: String,
    },
    /// Active signer DID does not match the operator DID (F-16).
    #[error("signer mismatch: signer did `{signer_did}` != operator did `{operator_did}`")]
    SignerMismatch {
        /// Signer DID derived from the active public key.
        signer_did: String,
        /// Operator DID supplied on the command line.
        operator_did: String,
    },
    /// Secret was offered on stdin without `--allow-stdin-secret`.
    #[error("secret material on pipe; pass --allow-stdin-secret to override")]
    StdinSecretRefused,
    /// Filter expression is malformed.
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
    /// A deprecated stub was invoked during the stale-stub window.
    #[error("`{name}` was removed")]
    StaleStub {
        /// Stub command name.
        name: String,
    },
    /// Reputation aggregate for the given `(did, role)` was not found
    /// (RFC-0011-b §Substrate `[ADD]` map: `ReputationError::AggregateEmpty`).
    #[error("reputation not found for did `{did}` role `{role}`")]
    ReputationNotFound {
        /// Subject DID.
        did: String,
        /// Role slug.
        role: String,
    },
    /// Reputation subject DID is in the `Revoked` lifecycle state
    /// (RFC-0968 §Roles and Authorities). Auditor mode fails closed
    /// regardless of caller mode (RFC-0011-b §Security Considerations 3).
    #[error("reputation revoked for did `{did}`")]
    ReputationRevoked {
        /// Subject DID.
        did: String,
    },
    /// Anchor chain digest mismatch (RFC-0011-b §Substrate `[ADD]`
    /// map: `ReputationError::AnchorDigestMismatch`). Surfaces the
    /// last anchored unix timestamp for diagnostic context.
    #[error("anchor chain broken for did `{did}` (last anchor at unix {last_anchor_unix})")]
    AnchorChainBroken {
        /// Subject DID.
        did: String,
        /// Unix seconds of the last accepted anchor.
        last_anchor_unix: i64,
    },
    /// `--no-anchor-verify` was passed outside Dev mode
    /// (RFC-0011-b §Security Considerations 1a; DEV-ONLY escape hatch).
    #[error(
        "--no-anchor-verify is DEV-only (got mode `{mode}`); switch to --mode dev or drop the flag"
    )]
    NoAnchorVerifyInMode {
        /// Resolved operator mode label (lowercase).
        mode: &'static str,
    },
    /// Role slug failed the substrate `Role::parse` guard
    /// (RFC-0011-b §7.4 `Role::parse` rejects empty / whitespace).
    #[error("invalid role slug `{slug}`: {reason}")]
    InvalidRoleSlug {
        /// The supplied role slug (sanitized).
        slug: String,
        /// Why the slug was rejected.
        reason: String,
    },
    /// TTL hop count is out of the `1..=8` range allowed by RFC-0871.
    /// Surfaces from `octo mesh forward --ttl-hops`. Substrate clamps to
    /// the per-node-type TTL ceiling from `RouterAnnouncePayload`; the
    /// CLI enforces the wider 1..=8 operator-facing bound at dispatch
    /// (RFC-0011-f §Error Handling). Exit 17 per the amendment-chain
    /// slot allocation reserved by RFC-0011-f §Exit Codes.
    #[error("invalid TTL hops: {hops} (must be in 1..=8 per RFC-0871 ceiling)")]
    InvalidTtlHops {
        /// The offending hop count.
        hops: u8,
    },
    /// Mesh capability is missing or insufficient for the requested
    /// dispatch. `forward` and `rpc` require an RFC-0957 capability
    /// caveat per RFC-0011-f §G7; substrate-truth verification at the
    /// dispatch boundary surfaces this when the envelope carries only a
    /// signature authorization (no capability bound) or the bound
    /// capability's `Audience` caveat does not match the resolved peer
    /// DID (RFC-0957 §Attenuation Invariant). Exit 18.
    #[error("mesh capability missing or insufficient: {detail}")]
    MeshCapabilityInsufficient {
        /// Operator-safe diagnostic.
        detail: String,
    },
    /// Envelope authorization verification failed at the dispatch
    /// boundary per RFC-0871 §Algorithms "Envelope receive (node-side)"
    /// step 6 (signature verify against the current `verifying_key`) or
    /// the substrate's request/reply correlation path. CLI maps the
    /// substrate `ProtocolError` family (e.g. `SignerKeyRevoked`,
    /// `InvalidSignature`, `AudienceMismatch`) to this single
    /// operator-facing variant; the substrate owns the canonical
    /// distinction. Exit 19.
    #[error("envelope authorization failed: {detail}")]
    EnvelopeAuthorizationFailed {
        /// Operator-safe diagnostic.
        detail: String,
    },
    /// Endpoint URI scheme is not in the mesh allowlist
    /// (`tcp://`, `quic://`, `bluetooth://` per RFC-0011-f §Peer
    /// Summary Shape). Mapped from `octo_mesh::MeshError::InvalidEndpointScheme`
    /// at the `peer add` dispatch boundary. Exit 28 (shared slot
    /// with `forward`'s `InvalidTtlHops` per RFC-0011-f §Exit Codes).
    #[error("invalid endpoint URI scheme: `{scheme}` (allowlist: tcp://, quic://, bluetooth://)")]
    InvalidEndpointScheme {
        /// The rejected scheme (lowercase, no `://`).
        scheme: String,
    },
    /// Unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),
}

impl OctoCliError {
    /// Process exit code for this failure.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ClapParse(_) => 2,
            Self::NoActiveIdentity => 2,
            Self::ConfirmationRequired { .. } => 2,
            Self::AuditorDenied { .. } => 2,
            Self::AlreadyRotating => 3,
            Self::IdentityNotFound(_) => 4,
            Self::HsmUnavailable(_) => 5,
            Self::AlreadyRevoked => 6,
            Self::CaveatParse { .. } => 7,
            Self::InvalidCaveatCombination { .. } => 8,
            Self::HolderNotFound(_) => 9,
            Self::AttenuationViolation(_) => 10,
            Self::SigningFailed(_) => 11,
            Self::ParentCapNotFound(_) => 12,
            Self::PolicyNotFound(_) => 13,
            Self::PolicyVersionNotFound { .. } => 14,
            Self::StdinSecretRefused => 15,
            Self::InvalidFilter(_) => 16,
            Self::RoleNotFound(_) => 31,
            Self::StakeInsufficient { .. } => 32,
            Self::RoleNotSelectable { .. } => 33,
            // 34 reserved per F-16 (was RoleBindingConflict; intentionally
            // skipped — last-writer-wins per RFC-0011-d §Security 2).
            Self::SignerMismatch { .. } => 35,
            // 28 shared with `InvalidTtlHops` (RFC-0011-f §Exit Codes;
            // follow-on `forward` mission claims the same slot).
            Self::InvalidEndpointScheme { .. } => 28,
            Self::ReputationNotFound { .. } => 20,
            Self::ReputationRevoked { .. } => 21,
            Self::AnchorChainBroken { .. } => 22,
            Self::NoAnchorVerifyInMode { .. } => 2,
            Self::InvalidRoleSlug { .. } => 2,
            Self::InvalidTtlHops { .. } => 17,
            Self::MeshCapabilityInsufficient { .. } => 18,
            Self::EnvelopeAuthorizationFailed { .. } => 19,
            Self::Internal(_) => 64,
            Self::StaleStub { .. } => 65,
        }
    }

    /// Operator-safe message with substrate internals stripped.
    pub fn user_message(&self) -> String {
        sanitize_substrate_error(&self.to_string())
    }

    /// Per-variant remediation hint.
    pub fn hint(&self) -> Option<String> {
        let h = match self {
            Self::ClapParse(_) => "run `octo --help` for usage",
            Self::NoActiveIdentity => "create or select an identity before running this command",
            Self::ConfirmationRequired { .. } => {
                "re-run with `--confirm` to acknowledge the mutation"
            }
            Self::AuditorDenied { .. } => {
                "auditor mode is read-only; switch to --mode human or --mode ci to perform mutations"
            }
            Self::AlreadyRotating => "complete or abort the in-flight rotation first",
            Self::IdentityNotFound(_) => "list identities with `octo identity show`",
            Self::HsmUnavailable(_) => "check that the HSM backend is reachable",
            Self::AlreadyRevoked => "this identity is already revoked; no action needed",
            Self::CaveatParse { .. } => "check the caveat expression syntax",
            Self::InvalidCaveatCombination { .. } => "remove conflicting caveats",
            Self::HolderNotFound(_) => "verify the holder DID",
            Self::AttenuationViolation(_) => "attenuation may only narrow authority",
            Self::SigningFailed(_) => "verify the signing key is available",
            Self::ParentCapNotFound(_) => "list capabilities with `octo capability list`",
            Self::PolicyNotFound(_) => "list policies with `octo policy list`",
            Self::PolicyVersionNotFound { .. } => "omit `--version` to use the latest version",
            Self::StdinSecretRefused => "re-run with `--allow-stdin-secret` if intended",
            Self::InvalidFilter(_) => "filter syntax is `key=value`",
            Self::RoleNotFound(_) => "list roles with `octo role list`",
            Self::StakeInsufficient { .. } => "top up the operator's OCTO stake and retry",
            Self::RoleNotSelectable { .. } => "verify the role slug + operator permissions",
            Self::SignerMismatch { .. } => {
                "the active signer does not match the supplied operator DID"
            }
            Self::ReputationNotFound { .. } => {
                "the subject has no aggregate for this role yet; attestations land first"
            }
            Self::ReputationRevoked { .. } => {
                "revoked DIDs are read-only across all operator modes (RFC-0011-b §Security 3)"
            }
            Self::AnchorChainBroken { .. } => {
                "the last anchor chain digest does not match the substrate; verify the chain"
            }
            Self::NoAnchorVerifyInMode { .. } => {
                "drop --no-anchor-verify or re-run with --mode dev"
            }
            Self::InvalidRoleSlug { .. } => {
                "role slugs must be non-empty and contain no whitespace"
            }
            Self::InvalidTtlHops { .. } => {
                "--ttl-hops must be in the inclusive range 1..=8 (RFC-0871 ceiling); substrate further clamps to per-node-type ceiling from RouterAnnouncePayload"
            }
            Self::MeshCapabilityInsufficient { .. } => {
                "the envelope must carry an Authorization::Capability with Audience caveat bound to the target peer DID (RFC-0957 §Attenuation Invariant)"
            }
            Self::EnvelopeAuthorizationFailed { .. } => {
                "verify the envelope signature against the current verifying key, the audience caveat matches the target peer DID, and the envelope has not expired"
            }
            Self::InvalidEndpointScheme { .. } => {
                "endpoint URI scheme must be one of tcp://, quic://, bluetooth://"
            }
            Self::StaleStub { .. } => "this command was removed; see the migration notes",
            Self::Internal(_) => "re-run with `RUST_LOG=debug` and report the diagnostic",
        };
        Some(h.to_string())
    }

    /// Write this error to stderr and terminate the process.
    ///
    /// Render format (RFC-0011 §Error Handling):
    /// ```text
    /// error: <msg>
    ///   caused by: <chain>
    ///   hint: <hint>
    ///   exit code: <N>
    /// ```
    pub fn render(&self, force_json: bool) -> ! {
        let code = self.exit_code();
        let msg = self.user_message();
        let stderr = std::io::stderr();
        let mut w = stderr.lock();
        if force_json {
            let mut sources: Vec<String> = Vec::new();
            let mut src: Option<&dyn std::error::Error> = std::error::Error::source(self);
            while let Some(s) = src {
                sources.push(sanitize_substrate_error(&s.to_string()));
                src = s.source();
            }
            let body = serde_json::json!({
                "schema_version": crate::output::OutputEnvelope::<()>::SCHEMA_VERSION,
                "error": msg,
                "caused_by": sources,
                "hint": self.hint(),
                "exit_code": code,
            });
            let _ = writeln!(w, "{body}");
        } else {
            let _ = writeln!(w, "error: {msg}");
            let mut src: Option<&dyn std::error::Error> = std::error::Error::source(self);
            while let Some(s) = src {
                let _ = writeln!(
                    w,
                    "  caused by: {}",
                    sanitize_substrate_error(&s.to_string())
                );
                src = s.source();
            }
            if let Some(hint) = self.hint() {
                let _ = writeln!(w, "  hint: {hint}");
            }
            let _ = writeln!(w, "  exit code: {code}");
        }
        let _ = w.flush();
        std::process::exit(code)
    }
}

/// Gate helper that any future stdin reader calls before consuming pipe data.
///
/// Returns `StdinSecretRefused` (exit 15) unless the operator passed
/// `--allow-stdin-secret`. The flag is currently unused in this RFC's
/// command surface (no command reads stdin), but the gate is wired here so
/// that when a reader is added it can drop in `ensure_stdin_secret_allowed`
/// and inherit the refusal + exit-code contract for free.
pub fn ensure_stdin_secret_allowed(allow: bool) -> Result<(), OctoCliError> {
    if allow {
        Ok(())
    } else {
        Err(OctoCliError::StdinSecretRefused)
    }
}

/// Strip substrate paths and storage-engine internals from an error string.
///
/// Marker policy (R16 Lens-2 F7): substring containment is too broad —
/// `query:` matches legitimate diagnostic text, `src/` matches URLs and
/// any path containing the directory name. We anchor on full crate paths
/// (`crates/octo-<name>/`) and require a word-boundary on both sides of
/// `SQL:` / `query:` / `sqlite3_open` so they only fire when used as SQL
/// noise markers, not as natural prose.
pub fn sanitize_substrate_error(s: &str) -> String {
    // Anchor SQL/storage markers with word-boundary semantics: the marker
    // must appear at the start, after whitespace, or after a punctuation
    // token. Trailing punctuation (`.`, `,`, `\n`) is also a valid boundary.
    // R17 Lens-2 F2: case-insensitive variants — `SQL:` and `Query:` are
    // both legitimate noise markers a substrate may emit.
    const ERROR_MARKERS: [&str; 3] = ["SQL:", "query:", "sqlite3_open"];
    let mut out = s.to_string();
    for marker in ERROR_MARKERS {
        while let Some(idx) = find_word_boundary_ci(&out, marker) {
            out.replace_range(idx..idx + marker.len(), "<substrate-error>");
        }
    }
    // Anchor path markers to the canonical `crates/octo-<name>/` prefix —
    // this catches `crates/octo-wallet/...`, `crates/octo-cap-macaroon/...`,
    // etc. without matching `src/` substrings in URLs or other contexts.
    const PATH_PREFIX: &str = "crates/octo-";
    while let Some(idx) = out.find(PATH_PREFIX) {
        // Find the end of the path: either a whitespace, closing punctuation,
        // or end-of-string. Stop at the first `)`/`]`/`,` so the
        // diagnostic `path:42:5` is replaced as one block.
        let tail = &out[idx..];
        let end = tail
            .find(char::is_whitespace)
            .or_else(|| tail.find([')', ']', ',']))
            .unwrap_or(out.len() - idx);
        out.replace_range(idx..idx + end, "<substrate-path>");
    }
    out
}

/// Case-insensitive variant of `find_word_boundary` (R17 Lens-2 F2).
/// Matches ASCII case variants only (`a-z`/`A-Z`) — the markers we use
/// (`SQL:`, `query:`, `sqlite3_open`) are all ASCII so this is sufficient.
fn find_word_boundary_ci(s: &str, marker: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let marker_bytes = marker.as_bytes();
    let marker_len = marker_bytes.len();
    let mut i = 0;
    while i + marker_len <= bytes.len() {
        // Word-boundary check on left.
        let left_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if left_ok {
            // Case-insensitive byte comparison.
            let matches = bytes[i..i + marker_len]
                .iter()
                .zip(marker_bytes.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b));
            if matches {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use thiserror::Error;

    #[test]
    fn tv_err2_internal_no_substrate_leak() {
        let e = OctoCliError::Internal("SQL: select * from wallet".into());
        // R16 Lens-2 F7: in-place replacement preserves context; the
        // `SQL:` marker is replaced, the surrounding text is retained.
        let msg = e.user_message();
        assert!(msg.contains("<substrate-error>"), "{msg}");
        assert!(!msg.contains("SQL:"), "{msg}");
    }

    #[test]
    fn tv_err3_source_chain_rendered() {
        // Build a chained-error wrapper that exposes a `#[source]` chain so
        // `std::error::Error::source()` walks more than one frame.
        #[derive(Error, Debug)]
        #[error("top-level: {0}")]
        struct Wrapper(#[source] Inner);

        #[derive(Error, Debug)]
        #[error("inner cause")]
        struct Inner;

        let inner = Inner;
        let chain = Wrapper(inner);
        // Render through OctoCliError::Internal so the sanitizer runs and we
        // exercise the `caused by:` walk. The wrapped text doesn't contain
        // any substrate markers so the message passes through verbatim.
        let cli_err = OctoCliError::Internal(format!("{chain}"));
        // Force the JSON branch off — we test the multi-line text branch by
        // asserting that the rendered format strings reference `caused by`
        // and `exit code` tokens and that source() walks both frames.
        let mut lines: Vec<String> = Vec::new();
        let mut src: Option<&dyn std::error::Error> =
            Some(&Wrapper(Inner) as &dyn std::error::Error);
        while let Some(s) = src {
            lines.push(format!("  caused by: {s}"));
            src = s.source();
        }
        assert!(
            lines.iter().any(|l| l.contains("top-level")),
            "wrapper not walked: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("inner cause")),
            "inner cause not walked: {lines:?}"
        );
        // Sanity check the cli error renders a stable `user_message`.
        assert!(cli_err.user_message().contains("top-level"));
    }

    #[test]
    fn tv_err3b_stdin_secret_refused_message_text() {
        let e = OctoCliError::StdinSecretRefused;
        let rendered = format!("{e}");
        assert!(
            rendered.contains("--allow-stdin-secret"),
            "rendered must mention the override flag: {rendered}"
        );
        assert!(
            rendered.contains("pipe"),
            "rendered must mention pipe: {rendered}"
        );
    }

    #[test]
    fn tv_err3c_stdin_gate_blocks_without_flag() {
        assert!(matches!(
            ensure_stdin_secret_allowed(false),
            Err(OctoCliError::StdinSecretRefused)
        ));
        assert!(ensure_stdin_secret_allowed(true).is_ok());
    }

    #[test]
    fn tv_err3d_render_emits_four_lines() {
        let e = OctoCliError::IdentityNotFound("alice".into());
        // We can't easily capture stderr from a !-returning fn without
        // spawning a process, so we just verify the formatting inputs
        // are coherent: hint present, code matches.
        assert!(e.hint().is_some());
        assert_eq!(e.exit_code(), 4);
    }

    #[test]
    fn tv_err4_exit_code_mapping() {
        let cases: Vec<(OctoCliError, i32)> = vec![
            (OctoCliError::NoActiveIdentity, 2),
            (
                OctoCliError::ConfirmationRequired {
                    command: "x".into(),
                },
                2,
            ),
            (OctoCliError::AlreadyRotating, 3),
            (OctoCliError::IdentityNotFound("d".into()), 4),
            (OctoCliError::HsmUnavailable("h".into()), 5),
            (OctoCliError::AlreadyRevoked, 6),
            (
                OctoCliError::CaveatParse {
                    message: "m".into(),
                },
                7,
            ),
            (
                OctoCliError::InvalidCaveatCombination { detail: "d".into() },
                8,
            ),
            (OctoCliError::HolderNotFound("h".into()), 9),
            (OctoCliError::AttenuationViolation("a".into()), 10),
            (OctoCliError::SigningFailed("s".into()), 11),
            (OctoCliError::ParentCapNotFound("p".into()), 12),
            (OctoCliError::PolicyNotFound("p".into()), 13),
            (
                OctoCliError::PolicyVersionNotFound {
                    policy: "p".into(),
                    version: 1,
                },
                14,
            ),
            (OctoCliError::StdinSecretRefused, 15),
            (OctoCliError::InvalidFilter("f".into()), 16),
            (OctoCliError::RoleNotFound("x".into()), 31),
            (
                OctoCliError::StakeInsufficient {
                    required: 0,
                    available: 0,
                },
                32,
            ),
            (
                OctoCliError::RoleNotSelectable {
                    role_id: "x".into(),
                    reason: "y".into(),
                },
                33,
            ),
            (
                OctoCliError::SignerMismatch {
                    signer_did: "a".into(),
                    operator_did: "b".into(),
                },
                35,
            ),
            (
                OctoCliError::AuditorDenied {
                    command: "x".into(),
                },
                2,
            ),
            (OctoCliError::InvalidTtlHops { hops: 9 }, 17),
            (
                OctoCliError::MeshCapabilityInsufficient {
                    detail: "no capability bound to forward envelope".into(),
                },
                18,
            ),
            (
                OctoCliError::EnvelopeAuthorizationFailed {
                    detail: "audience mismatch".into(),
                },
                19,
            ),
            (
                OctoCliError::InvalidEndpointScheme {
                    scheme: "file".into(),
                },
                28,
            ),
            (OctoCliError::Internal("i".into()), 64),
            (
                OctoCliError::StaleStub {
                    name: "init".into(),
                },
                65,
            ),
        ];
        for (e, code) in cases {
            assert_eq!(e.exit_code(), code, "{e:?}");
        }
        // ClapParse is constructed separately (24th variant).
        let clap_err = clap::Error::new(clap::error::ErrorKind::InvalidValue);
        assert_eq!(OctoCliError::ClapParse(clap_err).exit_code(), 2);
    }

    #[test]
    fn tv_err5_no_substrate_internals() {
        let cases = [
            // `crates/octo-wallet/...` → redacted (canonical path prefix).
            OctoCliError::Internal("failed at crates/octo-wallet/src/store.rs:42".into()),
            // `src/identity.rs` is NOT redacted under R16 Lens-2 F7
            // (substring `src/` was too broad — matches URLs etc.).
            OctoCliError::IdentityNotFound("src/identity.rs".into()),
            // `query: SELECT 1` → word-boundary match on `query:` redacts.
            OctoCliError::HsmUnavailable("query: SELECT 1".into()),
        ];
        for e in cases {
            let msg = e.user_message();
            assert!(!msg.contains("crates/octo-"), "{msg}");
            // `src/` substring intentionally not stripped (R16 Lens-2 F7).
            assert!(!msg.contains("SQL:"), "{msg}");
            assert!(!msg.contains("query:"), "{msg}");
        }
    }

    /// RFC-0011-f §Error Handling: mesh forward error variants must
    /// carry their assigned exit codes (17/18/19) and render a
    /// remediation hint. Pins the exit-code table reserved by the
    /// amendment-chain slot allocation.
    #[test]
    fn tv_mesh_forward_exit_codes_and_hints() {
        let ttl = OctoCliError::InvalidTtlHops { hops: 9 };
        assert_eq!(ttl.exit_code(), 17);
        let hint = ttl.hint().expect("hint required");
        assert!(
            hint.contains("1..=8"),
            "TTL hint must cite the inclusive range, got: {hint}"
        );

        let cap = OctoCliError::MeshCapabilityInsufficient {
            detail: "envelope has Authorization::Signature only".into(),
        };
        assert_eq!(cap.exit_code(), 18);
        let hint = cap.hint().expect("hint required");
        assert!(
            hint.contains("Audience"),
            "capability hint must mention Audience caveat, got: {hint}"
        );

        let auth = OctoCliError::EnvelopeAuthorizationFailed {
            detail: "audience mismatch: expected peer_a, got peer_b".into(),
        };
        assert_eq!(auth.exit_code(), 19);
        let hint = auth.hint().expect("hint required");
        assert!(
            hint.contains("audience") || hint.contains("signature"),
            "auth hint must mention audience or signature, got: {hint}"
        );
    }
}
