//! `EndpointUri` allowlist-validated URI wrapper (RFC-0011-f
//! §Peer Summary Shape).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::MeshError;

/// Allowlisted endpoint URI schemes (RFC-0011-f §Peer Summary Shape).
pub const ALLOWED_ENDPOINT_SCHEMES: &[&str] = &["tcp://", "quic://", "bluetooth://"];

/// Wrapper around an endpoint URI string.
///
/// Validated against the [`ALLOWED_ENDPOINT_SCHEMES`] allowlist at
/// construction time. Unknown schemes return `MeshError::InvalidEndpointScheme`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EndpointUri(pub String);

impl EndpointUri {
    /// Parse + validate an endpoint URI string against the allowlist.
    ///
    /// # Errors
    ///
    /// Returns [`MeshError::InvalidEndpointScheme`] when the scheme is
    /// not in [`ALLOWED_ENDPOINT_SCHEMES`] or when the URI payload is
    /// empty after the scheme prefix.
    pub fn parse(s: &str) -> Result<Self, MeshError> {
        for prefix in ALLOWED_ENDPOINT_SCHEMES {
            if let Some(rest) = s.strip_prefix(prefix) {
                if rest.is_empty() {
                    return Err(MeshError::InvalidEndpointScheme {
                        scheme: prefix.trim_end_matches("://").to_string(),
                    });
                }
                return Ok(Self(s.to_string()));
            }
        }
        // Extract the scheme up to `://` for the diagnostic.
        let scheme = s
            .split_once("://")
            .map_or_else(|| s.to_string(), |(scheme, _)| scheme.to_string());
        Err(MeshError::InvalidEndpointScheme { scheme })
    }

    /// Return the inner URI string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EndpointUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
