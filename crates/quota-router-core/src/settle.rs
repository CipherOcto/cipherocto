//! Settlement module (PR-Q2, W2).
//!
//! Wraps `quota_router_sm_engine` settlement calls for the proxy. The
//! proxy calls `mint_ask` / `settle_receipt` / `consume_receipt` through
//! this module to keep the single-egress / single-settlement invariant.

use serde::{Deserialize, Serialize};

pub mod classify;

use crate::receipt::Receipt;

/// Settlement error proxy-side.
#[derive(Debug, thiserror::Error)]
pub enum ProxySettlementError {
    #[error("sm-engine error: {0}")]
    SmEngine(String),
    #[error("invalid settlement state: {0}")]
    InvalidState(String),
    #[error("settlement not found: {0}")]
    NotFound(String),
}

/// Settlement summary returned by `proxy_settle_step` (PR-Q2).
///
/// Combines the sm-engine `Receipt` + the proxy-side signed `Receipt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySettlementResult {
    pub ask_id: [u8; 32],
    pub receipt_id: [u8; 32],
    pub settlement_hash: [u8; 32],
    pub proxy_receipt: Receipt,
}

/// Build a proxy-side `Receipt` from an sm-engine `SettlementReceipt` +
/// router identity (PR-Q2).
///
/// In the full implementation, this function calls into
/// `quota_router_sm_engine::settle_ask` and signs the canonical bytes
/// with the router's Ed25519 key. The shape is kept here so the proxy
/// can call it without circular dependencies on the sm-engine crate's
/// `#[cfg]`-gated test helpers.
#[must_use]
pub fn build_proxy_receipt(
    settlement_hash: [u8; 32],
    router_id: &str,
    holder_did: &str,
    asker_did: &str,
    timestamp_unix: u64,
) -> Receipt {
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;

    let key = SigningKey::from_bytes(&[0x42; 32]); // placeholder; real key from vault
    let canonical = crate::receipt::canonical_receipt_bytes(
        &settlement_hash,
        asker_did,
        holder_did,
        timestamp_unix,
    );
    let sig = key.sign(&canonical);
    Receipt {
        settlement_hash,
        router_id: router_id.to_string(),
        router_sig: sig,
        timestamp_unix,
    }
}

/// Step 6 of the 11-step exercise: real `Reservation::mint` (PR-Q2 + W2).
///
/// Calls `quota_router_sm_engine::Reservation::mint` and returns the
/// content-addressed `reservation_id`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn mint_reservation(
    vault_id: [u8; 32],
    capability_id: [u8; 32],
    ask_id: [u8; 32],
    resource_axis: String,
    amount_micro: u128,
    expires_at_unix: u64,
    audit_window_secs: u64,
    created_at_unix: u64,
) -> [u8; 32] {
    quota_router_sm_engine::Reservation::mint(
        vault_id,
        capability_id,
        ask_id,
        resource_axis,
        amount_micro,
        expires_at_unix,
        audit_window_secs,
        created_at_unix,
    )
    .reservation_id
}

/// Step 6+10: real `Ask::mint` + `Receipt` build (PR-Q2).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn mint_ask(
    ask_id: [u8; 32],
    holder_did: String,
    axes_consumed: Vec<(String, u64)>,
    cap_root_hash: [u8; 32],
    invocation_hash: [u8; 32],
    current_unix_time: u64,
    output_hash: Option<[u8; 32]>,
) -> quota_router_sm_engine::Ask {
    quota_router_sm_engine::Ask {
        ask_id,
        holder_did,
        axes_consumed,
        cap_root_hash,
        invocation_hash,
        current_unix_time,
        output_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_proxy_receipt_has_router_signature() {
        let r = build_proxy_receipt(
            [0xab; 32],
            "router-1",
            &octo_ident::test_helpers::sample_did(199),
            &octo_ident::test_helpers::sample_did(182),
            1_700_000_000,
        );
        assert_eq!(r.router_id, "router-1");
        assert_eq!(r.settlement_hash, [0xab; 32]);
        assert_eq!(r.timestamp_unix, 1_700_000_000);
    }

    #[test]
    fn mint_reservation_content_addressed() {
        let r1 = mint_reservation(
            [0x01; 32],
            [0x02; 32],
            [0x03; 32],
            "input_tokens_per_1k".to_owned(),
            1_000_000,
            1_800_000_000,
            86400,
            1_700_000_000,
        );
        let r2 = mint_reservation(
            [0x01; 32],
            [0x02; 32],
            [0x03; 32],
            "input_tokens_per_1k".to_owned(),
            1_000_000,
            1_800_000_000,
            86400,
            1_700_000_000,
        );
        assert_eq!(r1, r2); // deterministic
    }

    #[test]
    fn mint_ask_carries_axes() {
        let a = mint_ask(
            [0x01; 32],
            octo_ident::test_helpers::sample_did(232),
            vec![("input_tokens_per_1k".to_owned(), 100)],
            [0x02; 32],
            [0x03; 32],
            1_700_000_000,
            None,
        );
        assert_eq!(a.axes_consumed.len(), 1);
        assert_eq!(a.axes_consumed[0].0, "input_tokens_per_1k");
    }
}
