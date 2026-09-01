//! Substrate error envelope — RFC-0011-f §Error Handling.
//!
//! The CLI maps each substrate variant to the operator-facing
//! [`crate::OctoCliError`] variant per RFC-0011-f §Error Handling.
//! New `MeshError` variants must be mirrored in `octo-cli::error.rs`
//! to preserve the exit-code contract.

use thiserror::Error;

/// Errors the `octo-mesh` substrate can surface.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MeshError {
    /// The supplied peer DID is not in canonical RFC-0010 wire form.
    ///
    /// Surfaced as `OctoCliError::IdentityNotFound` (exit 4) at the CLI
    /// dispatch boundary per RFC-0011-f §Error Handling. Includes the
    /// `did:octo:` prefix but the inner payload is not the canonical
    /// `did:octo:z<base58btc>` form (legacy `did:octo:b<base32>` is the
    /// common case — `allow_legacy_bare=false` rejects with this
    /// variant).
    #[error("invalid peer DID shape: {0}")]
    InvalidDidShape(String),

    /// The supplied endpoint URI scheme is not in the allowlist
    /// (`tcp://`, `quic://`, `bluetooth://` per RFC-0011-f §Peer
    /// Summary Shape).
    ///
    /// Surfaced as `OctoCliError::InvalidEndpointScheme` (exit 28) at
    /// the CLI dispatch boundary.
    #[error("invalid endpoint URI scheme: `{scheme}` (allowlist: tcp://, quic://, bluetooth://)")]
    InvalidEndpointScheme {
        /// The rejected scheme (lowercase, no `://`).
        scheme: String,
    },

    /// The target peer does not serve the requested RPC method per
    /// its `payload_kind` UUID (RFC-0011-f §RPC Surface "no central
    /// enum" rationale + RFC-0871 §Specialized Node Lifecycle). The
    /// substrate's method registry is the canonical answer; the CLI
    /// maps this to exit 17 (shared with `InvalidTtlHops` per
    /// amendment-chain slot allocation, RFC-0011-f §Exit Codes).
    #[error("unknown RPC method `{method}` (target peer does not serve this method per its `payload_kind` UUID)")]
    UnknownMethod {
        /// The rejected method name (verbatim operator input).
        method: String,
    },

    /// The RPC reply did not arrive within the substrate timeout
    /// ceiling (default 30s per RFC-0011-f §Performance Targets). The
    /// CLI maps this to exit 20 per RFC-0011-f §Error Handling +
    /// §Subcommand Taxonomy `rpc` "Exit codes" row.
    #[error("RPC timeout after {timeout_ms}ms: peer `{peer}` method `{method}`")]
    RpcTimeout {
        /// Target peer DID (RFC-0010 canonical wire form).
        peer: String,
        /// Method name (verbatim operator input).
        method: String,
        /// Timeout ceiling in milliseconds (substrate-defined;
        /// CLI default 30_000).
        timeout_ms: u64,
    },

    /// Filesystem error during peer-table read / write.
    #[error("peer table I/O failure: {0}")]
    Io(String),

    /// TOML parse error during peer-table load.
    #[error("peer table TOML parse error: {0}")]
    TomlParse(String),

    /// TOML serialisation error during peer-table write.
    #[error("peer table TOML serialise error: {0}")]
    TomlSerialise(String),
}
