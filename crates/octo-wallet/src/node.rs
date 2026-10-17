//! NodeType taxonomy.
//!
//! Per RFC-0009 §Node, every CipherOcto node declares one of three NodeTypes.
//! The choice gates capability-class minting per RFC-0958 §NodeType Gating Matrix:
//! - Wholesale → ZK-bearing capabilities REJECTED (fail-closed)
//! - SelfHost   → ZK-bearing capabilities DEFAULT
//! - Hybrid     → ZK-bearing capabilities OPT-IN

use serde::{Deserialize, Serialize};

/// CipherOcto node deployment mode.
///
/// Coarse taxonomy; finer sub-types (e.g., per-provider routing) live in
/// downstream configuration (RFC-0900 marketplace) and do not affect
/// capability-class gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeType {
    /// Routes calls to external opaque providers (OpenAI, Anthropic, etc.).
    /// Cannot mint ZK-bearing capabilities per RFC-0958 §Adversary A3.
    Wholesale,
    /// Runs inference inside the CipherOcto protocol boundary.
    /// Mints ZK-bearing capabilities by default per RFC-0958 §NodeType Gating.
    #[serde(rename = "self-host")]
    SelfHost,
    /// Operates both wholesale-routed and self-hosted inference.
    /// ZK mint requires explicit `mint_with_zk()` API call.
    Hybrid,
}

impl NodeType {
    /// CLI string accepted by `octo-wallet init --node-type <X>`.
    #[must_use]
    pub fn as_cli_str(&self) -> &'static str {
        match self {
            Self::Wholesale => "wholesale",
            Self::SelfHost => "self-host",
            Self::Hybrid => "hybrid",
        }
    }

    /// Returns true iff this NodeType permits minting ZK-bearing capabilities.
    /// Wholesale always returns false; SelfHost and Hybrid return true.
    #[must_use]
    pub const fn permits_zk_mint(&self) -> bool {
        matches!(self, Self::SelfHost | Self::Hybrid)
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_cli_str())
    }
}

impl std::str::FromStr for NodeType {
    type Err = NodeTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wholesale" => Ok(Self::Wholesale),
            "self-host" | "self_host" | "selfhost" => Ok(Self::SelfHost),
            "hybrid" => Ok(Self::Hybrid),
            _ => Err(NodeTypeParseError(s.to_owned())),
        }
    }
}

/// Error returned when parsing a `NodeType` from CLI / config fails.
#[derive(Debug, thiserror::Error)]
#[error("unknown NodeType `{0}`; expected one of: wholesale, self-host, hybrid")]
pub struct NodeTypeParseError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_roundtrip() {
        for nt in [NodeType::Wholesale, NodeType::SelfHost, NodeType::Hybrid] {
            let s = nt.as_cli_str();
            let parsed: NodeType = s.parse().expect("parse");
            assert_eq!(nt, parsed);
        }
    }

    #[test]
    fn permits_zk_mint() {
        assert!(!NodeType::Wholesale.permits_zk_mint());
        assert!(NodeType::SelfHost.permits_zk_mint());
        assert!(NodeType::Hybrid.permits_zk_mint());
    }

    #[test]
    fn unknown_rejected() {
        let err = "garbage".parse::<NodeType>().unwrap_err();
        assert_eq!(err.0, "garbage");
    }

    #[test]
    fn json_roundtrip() {
        for nt in [NodeType::Wholesale, NodeType::SelfHost, NodeType::Hybrid] {
            let j = serde_json::to_string(&nt).unwrap();
            let back: NodeType = serde_json::from_str(&j).unwrap();
            assert_eq!(nt, back);
        }
    }
}
