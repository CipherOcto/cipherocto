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
