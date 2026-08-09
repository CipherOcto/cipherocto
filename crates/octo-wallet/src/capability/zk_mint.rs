//! Re-export shim — ZK capability substrate now lives in `octo-cap-zk`
//! (Layer 4 extension crate per RFC-0965 per-extension crate layout
//! mandate).
//!
//! Mission 0957-ext-zk-crate migration moved the canonical implementation
//! into `octo_cap_zk`. This shim preserves the existing
//! `octo_wallet::capability::zk_mint::*` import paths for backward
//! compatibility. No new code lives here — all behavior lives in the
//! extension crate.

pub use octo_cap_zk::*;

/// Convert the wallet's canonical `NodeType` (Layer B substrate in
/// `crates/octo-wallet/src/node.rs`) to the local `octo_cap_zk::NodeType`
/// mirror enum. The mirror-enum approach is the Layer 4 boundary
/// strategy: `NodeType` is a 3-variant enum with a single boolean method,
/// and a trait abstraction would add indirection for zero benefit.
///
/// **Orphan rule:** we cannot write `impl From<crate::node::NodeType>
/// for octo_cap_zk::NodeType` because both types are foreign to either
/// crate. The free function form is the clean alternative.
#[must_use]
pub fn to_zk_node_type(node_type: &crate::node::NodeType) -> octo_cap_zk::NodeType {
    use crate::node::NodeType as WalletNodeType;
    match node_type {
        WalletNodeType::Wholesale => octo_cap_zk::NodeType::Wholesale,
        WalletNodeType::SelfHost => octo_cap_zk::NodeType::SelfHost,
        WalletNodeType::Hybrid => octo_cap_zk::NodeType::Hybrid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_zk_node_type_wholesale() {
        let wallet = crate::node::NodeType::Wholesale;
        assert_eq!(to_zk_node_type(&wallet), octo_cap_zk::NodeType::Wholesale);
    }

    #[test]
    fn to_zk_node_type_self_host() {
        let wallet = crate::node::NodeType::SelfHost;
        assert_eq!(to_zk_node_type(&wallet), octo_cap_zk::NodeType::SelfHost);
    }

    #[test]
    fn to_zk_node_type_hybrid() {
        let wallet = crate::node::NodeType::Hybrid;
        assert_eq!(to_zk_node_type(&wallet), octo_cap_zk::NodeType::Hybrid);
    }

    #[test]
    fn to_zk_node_type_preserves_permits_zk_mint_semantics() {
        // Mirror the RFC-0958 §NodeType Gating Rule (Wholesale fail-closed,
        // SelfHost default, Hybrid opt-in). If the wallet's NodeType gating
        // rules drift from the ZK crate's, this test surfaces the mismatch.
        let cases = [
            (crate::node::NodeType::Wholesale, false),
            (crate::node::NodeType::SelfHost, true),
            (crate::node::NodeType::Hybrid, true),
        ];
        for (wallet_nt, expected) in cases {
            let zk_nt = to_zk_node_type(&wallet_nt);
            assert_eq!(
                zk_nt.permits_zk_mint(),
                expected,
                "NodeType::{wallet_nt:?} gating mismatch",
            );
        }
    }
}
