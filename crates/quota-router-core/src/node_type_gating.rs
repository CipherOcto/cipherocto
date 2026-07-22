//! NodeType gating for ZK capability class (S05 — RFC-0958).
//!
//! Per RFC-0958 §NodeType Gating Matrix:
//! - Wholesale → ZK mint REJECTED (fail-closed)
//! - SelfHost   → ZK mint DEFAULT
//! - Hybrid     → ZK mint OPT-IN

use crate::zk_verify::ZkMintError;

/// NodeType taxonomy (re-exported from octo-wallet via thin newtype for
/// S05 separation of concerns; RFC-0958 §NodeType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Wholesale,
    SelfHost,
    Hybrid,
}

impl NodeType {
    /// Whether this NodeType permits minting ZK-bearing capabilities.
    /// Wholesale always returns false (fail-closed per RFC-0958 §Adversary A3).
    #[must_use]
    pub const fn permits_zk_mint(&self) -> bool {
        matches!(self, Self::SelfHost | Self::Hybrid)
    }
}

/// Try to mint a ZK capability. Returns Ok if permitted by NodeType gating,
/// Err if not.
///
/// Wholesale → `Err(NodeTypeCannotMintZKCap)` regardless of other inputs.
/// SelfHost + Hybrid → `Ok(())`.
///
/// # Errors
/// Returns `ZkMintError::NodeTypeCannotMintZKCap` if `node_type == Wholesale`.
pub fn check_zk_mint_allowed(node_type: NodeType) -> Result<(), ZkMintError> {
    if node_type.permits_zk_mint() {
        Ok(())
    } else {
        Err(ZkMintError::NodeTypeCannotMintZKCap)
    }
}

/// Try to mint a ZK capability with full pre-flight checks.
///
/// Checks (in order):
/// 1. NodeType gating (Wholesale REJECTED)
/// 2. Capability class == ZKBearing
/// 3. SelfHost NodeType requires inference trace (caller supplies flag)
///
/// # Errors
/// Returns appropriate `ZkMintError` variant for each failure mode.
pub fn check_zk_mint_preflight(
    node_type: NodeType,
    class_is_zkbearing: bool,
    selfhost_has_inference_trace: bool,
) -> Result<(), ZkMintError> {
    check_zk_mint_allowed(node_type)?;
    if !class_is_zkbearing {
        return Err(ZkMintError::ClassMismatch);
    }
    if matches!(node_type, NodeType::SelfHost) && !selfhost_has_inference_trace {
        return Err(ZkMintError::MissingInferenceTrace);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wholesale_rejects_zk_mint() {
        let err = check_zk_mint_allowed(NodeType::Wholesale).unwrap_err();
        assert!(matches!(err, ZkMintError::NodeTypeCannotMintZKCap));
    }

    #[test]
    fn selfhost_permits_zk_mint() {
        check_zk_mint_allowed(NodeType::SelfHost).unwrap();
    }

    #[test]
    fn hybrid_permits_zk_mint() {
        check_zk_mint_allowed(NodeType::Hybrid).unwrap();
    }

    #[test]
    fn preflight_rejects_class_mismatch() {
        let err = check_zk_mint_preflight(NodeType::SelfHost, false, true).unwrap_err();
        assert!(matches!(err, ZkMintError::ClassMismatch));
    }

    #[test]
    fn preflight_rejects_missing_inference_trace_for_selfhost() {
        let err = check_zk_mint_preflight(NodeType::SelfHost, true, false).unwrap_err();
        assert!(matches!(err, ZkMintError::MissingInferenceTrace));
    }

    #[test]
    fn preflight_passes_for_selfhost_with_trace() {
        check_zk_mint_preflight(NodeType::SelfHost, true, true).unwrap();
    }

    #[test]
    fn preflight_passes_for_hybrid_with_class() {
        check_zk_mint_preflight(NodeType::Hybrid, true, true).unwrap();
    }

    #[test]
    fn permits_zk_mint_const() {
        assert!(!NodeType::Wholesale.permits_zk_mint());
        assert!(NodeType::SelfHost.permits_zk_mint());
        assert!(NodeType::Hybrid.permits_zk_mint());
    }
}
