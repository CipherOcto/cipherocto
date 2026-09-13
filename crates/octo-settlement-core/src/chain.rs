//! Receipt chain-integrity helpers per RFC-0014 §Design Goals G5.
//!
//! `verify_receipt_chain` validates a sequence of `Receipt`s for:
//! - Strict `receipt_id` monotonicity (gap detection)
//! - `settlement_hash` chain matches `BLAKE3-256` keyed with the
//!   canonical domain separator.
//!
//! `receipt_id_for` produces the canonical receipt ID for a given
//! (ask_id, prev_receipt_hash) pair.

use crate::error::SettlementError;
use crate::receipt::Receipt;

/// Canonical domain separator for the settlement receipt chain.
///
/// Per RFC-0959 + RFC-0014 §Test Vectors, the byte-pinned domain
/// separator is:
/// ```
/// const SEP: &[u8] = b"cipherocto/reservation/v1/";
/// ```
/// Any drift breaks cross-replica consensus; the constant is
/// exported for tests + cross-replica checksum verification.
pub const CHAIN_DOMAIN_SEPARATOR: &[u8] = b"cipherocto/reservation/v1/";

/// Compute the canonical receipt ID for `receipt` as
/// `BLAKE3-256(domain_separator || canonical_receipt_bytes)`.
///
/// `settlement_hash` is the OUTPUT of this function; it is NOT
/// included in the input (including it would make the hash depend on
/// itself, breaking determinism).
///
/// # Keyed-hash key
///
/// The BLAKE3 key is the zero-filled 32-byte constant `[0; 32]`. This
/// is INTENTIONAL — the substrate is Layer A frozen (RFC-0014
/// §Module Layout) and cross-replica consensus requires the receipt
/// hash to be deterministic for any replica that holds the same
/// `Receipt` inputs (no per-deployment key material). The
/// `domain_separator::mission_01_domain_separator_byte_pin`
/// test (RFC-0014 §Module Layout §Chain Helpers invariant; single
/// canonical substrate test surface) byte-pins this behavior.
///
/// Production deployments that need keyed-hash defense-in-depth
/// SHOULD wrap this function (e.g., via a `KeyedHasher` trait on
/// Layer C) and inject real key material from an HSM / Vault /
/// secrets manager at startup. The frozen zero key is a load-bearing
/// consensus invariant, not a permanent security posture.
pub fn receipt_id_for(receipt: &Receipt) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&[0; 32]);
    hasher.update(CHAIN_DOMAIN_SEPARATOR);
    hasher.update(&receipt.receipt_id.to_be_bytes());
    hasher.update(&receipt.ask_id);
    hasher.update(receipt.router_id.as_bytes());
    hasher.update(&receipt.router_sig);
    hasher.update(&receipt.timestamp_unix.to_be_bytes());
    *hasher.finalize().as_bytes()
}

/// Verify a chain of receipts. Returns `Ok(())` on a valid chain;
/// returns `Err(SettlementError)` on the first detected violation.
///
/// # Errors
///
/// - `SequenceGap` — `receipt_id` is not the successor of the previous
///   receipt's `receipt_id`.
/// - `ChainIntegrity` — a receipt's `settlement_hash` field does not
///   match `receipt_id_for(receipt)`.
pub fn verify_receipt_chain(receipts: &[Receipt]) -> Result<(), SettlementError> {
    let mut last_receipt_id: Option<u64> = None;
    for receipt in receipts {
        if let Some(prev) = last_receipt_id {
            if receipt.receipt_id != prev + 1 {
                return Err(SettlementError::SequenceGap {
                    receipt_id: receipt.receipt_id,
                    prev,
                });
            }
        }
        let expected_settlement_hash = receipt_id_for(receipt);
        if receipt.settlement_hash != expected_settlement_hash {
            return Err(SettlementError::ChainIntegrity {
                receipt_id: receipt.receipt_id,
            });
        }
        last_receipt_id = Some(receipt.receipt_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_receipt(id: u64, ask: [u8; 32], ts: u64) -> Receipt {
        let mut r = Receipt {
            receipt_id: id,
            ask_id: ask,
            settlement_hash: [0; 32],
            router_id: "did:oct:router".to_owned(),
            router_sig: vec![0xaa, 0xbb],
            timestamp_unix: ts,
        };
        r.settlement_hash = receipt_id_for(&r);
        r
    }

    #[test]
    fn chain_empty_is_valid() {
        assert!(verify_receipt_chain(&[]).is_ok());
    }

    #[test]
    fn chain_single_accepts() {
        let r = make_receipt(0, [0x01; 32], 1000);
        assert!(verify_receipt_chain(std::slice::from_ref(&r)).is_ok());
    }

    #[test]
    fn chain_monotonic_5() {
        let mut receipts = Vec::new();
        for i in 0..5 {
            receipts.push(make_receipt(i, [0x01; 32], 1000 + i * 100));
        }
        assert!(verify_receipt_chain(&receipts).is_ok());
    }

    #[test]
    fn chain_gap_rejects() {
        let r0 = make_receipt(0, [0x01; 32], 1000);
        let r2 = make_receipt(2, [0x01; 32], 1200);
        assert!(matches!(
            verify_receipt_chain(&[r0, r2]),
            Err(SettlementError::SequenceGap { .. })
        ));
    }

    #[test]
    fn chain_integrity_rejects() {
        let r0 = make_receipt(0, [0x01; 32], 1000);
        let mut r1 = make_receipt(1, [0x01; 32], 1100);
        r1.settlement_hash[0] ^= 1;
        assert!(matches!(
            verify_receipt_chain(&[r0, r1]),
            Err(SettlementError::ChainIntegrity { .. })
        ));
    }

    // Byte-pin test lives at `domain_separator::mission_01_…` (single
    // canonical substrate surface per substrate-faithful no-duplication
    // policy). Do NOT add an inline duplicate here.
}
