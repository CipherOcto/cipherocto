//! Canonical `ReceiptSummary` projection (RFC-0016-a §6.5 + RFC-0014-v2 §Extension).
//!
//! Compact summary projection of canonical `Receipt` for CLI list output
//! and downstream readers that do not need every canonical `Receipt`
//! field. Pairs with the RFC-0014-v2 + RFC-0016-a `Receipt` field
//! extensions (model, cost_dqa, capability_root, subject_did, status).
//!
//! ## Layer discipline
//!
//! This module lives in `octo-audit` (Layer B façade). It depends on
//! `octo-settlement` (Layer B → Layer B per RFC-0014 §Module Layout)
//! which re-exports `octo-settlement-core` (Layer A frozen). The
//! projection maps canonical substrate fields to the summary struct
//! 1:1 — no per-field derivation, no calls into the registry. The
//! DOMAIN adapter (Layer C) owns the Stoolap-backed filter / list
//! wiring; this façade is the read-side projection.
//!
//! ## Substrate-faithfulness
//!
//! `from_canonical` preserves the substrate sort order (the CLI sorts
//! by `executed_at_unix DESC` with `receipt_id ASC` tiebreaker —
//! `executed_at_unix` aliases `Receipt::timestamp_unix` byte-for-byte
//! per RFC-0014 §Data Structures + RFC-0014-v2 §Extension).

use serde::{Deserialize, Serialize};

use octo_settlement::ReceiptId;
use octo_settlement::ReceiptStatus;

/// Compact summary projection of canonical `Receipt`
/// (RFC-0016-a §6.5).
///
/// Pairs with RFC-0014-v2 `Receipt` field extensions. The
/// `receipt_id: ReceiptId` field uses the canonical newtype (paired
/// with `receipt_id_for_digest` reverse-mapping per RFC-0014-v2 +
/// RFC-0016-a §6.4). The `subject_did` field carries the canonical
/// DID wire form as `String` (the substrate-faithful `octo-settlement`
/// façade boundary — `Did` type lives in `octo-ident`, this façade
/// stays free of that Layer B dep).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptSummary {
    /// Canonical `ReceiptId` newtype (paired with
    /// `receipt_id_for_digest` reverse-mapping).
    pub receipt_id: ReceiptId,
    /// Hex-form `ask_id` digest (32-byte BLAKE3 derived).
    pub ask_id: String,
    /// Model identifier (default = empty for pre-RFC-0014-v2
    /// receipts).
    #[serde(default)]
    pub model: String,
    /// Cost denominator-quanta-amount (default = 0 for pre-RFC-0014-v2
    /// receipts).
    #[serde(default)]
    pub cost_dqa: u64,
    /// Capability root hash (32-byte BLAKE3 digest; default = zeroed
    /// for pre-RFC-0014-v2 receipts).
    #[serde(default)]
    pub capability_root: [u8; 32],
    /// Subject DID (canonical wire form; default = empty for
    /// pre-RFC-0014-v2 receipts).
    #[serde(default)]
    pub subject_did: String,
    /// Execution timestamp in UNIX seconds — canonical substrate
    /// field per RFC-0014 §Data Structures.
    pub executed_at_unix: u64,
    /// Receipt status (paired with RFC-0014-v2 `ReceiptStatus` enum).
    pub status: ReceiptStatus,
}

impl ReceiptSummary {
    /// Substrate-faithful projection from canonical `Receipt`
    /// (RFC-0016-a §6.5).
    ///
    /// Maps every canonical `Receipt` field 1:1 — no per-field
    /// derivation, no calls into the registry. `executed_at_unix`
    /// aliases `Receipt::timestamp_unix` byte-for-byte per RFC-0014
    /// §Data Structures; `subject_did` carries the canonical DID wire
    /// form (as held by the substrate) as `String`.
    pub fn from_canonical(receipt: octo_settlement::Receipt) -> Self {
        Self {
            receipt_id: ReceiptId(receipt.receipt_id),
            ask_id: receipt.ask_id.iter().map(|b| format!("{b:02x}")).collect(),
            model: receipt.model,
            cost_dqa: receipt.cost_dqa,
            capability_root: receipt.capability_root,
            subject_did: receipt.subject_did,
            executed_at_unix: receipt.timestamp_unix,
            status: receipt.status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_settlement::Receipt;

    fn sample_receipt(id: u64, ts: u64) -> Receipt {
        Receipt {
            receipt_id: id,
            ask_id: [0xAA; 32],
            settlement_hash: [0xBB; 32],
            router_id: "did:octo:router-a".to_string(),
            router_sig: vec![0xCC; 64],
            timestamp_unix: ts,
            ..Default::default()
        }
    }

    #[test]
    fn from_canonical_preserves_receipt_id() {
        let r = sample_receipt(42, 1_700_000_042);
        let s = ReceiptSummary::from_canonical(r);
        assert_eq!(s.receipt_id, ReceiptId(42));
        assert_eq!(s.executed_at_unix, 1_700_000_042);
        assert_eq!(s.status, ReceiptStatus::Unknown);
        assert_eq!(s.model, "");
        assert_eq!(s.cost_dqa, 0);
        assert_eq!(s.capability_root, [0u8; 32]);
        assert_eq!(s.subject_did, "");
    }

    #[test]
    fn from_canonical_hex_encodes_ask_id() {
        let r = sample_receipt(1, 100);
        let s = ReceiptSummary::from_canonical(r);
        // ask_id is [0xAA; 32]; hex form = 64 `aa` chars
        assert_eq!(s.ask_id.len(), 64);
        assert!(s.ask_id.chars().all(|c| c == 'a'));
    }
}
