//! Canonical `Receipt` struct per RFC-0014 §Module Layout `receipt` +
//! RFC-0959 §Data Structures.

use serde::{Deserialize, Serialize};

/// Canonical settlement receipt struct. Field shape byte-identical to
/// RFC-0959 §Data Structures + RFC-0014 §Module Layout `receipt`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Monotonic receipt ID (per-stream; sink enforces monotonicity).
    pub receipt_id: u64,
    /// Ask ID this receipt settles (32-byte BLAKE3-256 digest).
    pub ask_id: [u8; 32],
    /// Settlement hash (32-byte BLAKE3-256 digest over canonical
    /// ask + settlement amount + counterparty DIDs).
    pub settlement_hash: [u8; 32],
    /// Router node DID (raw canonical wire form).
    pub router_id: String,
    /// Router signature over `settlement_hash` (variable length; raw
    /// signature bytes — domain crate parses per crypto scheme).
    pub router_sig: Vec<u8>,
    /// Wall-clock timestamp in seconds since UNIX epoch.
    pub timestamp_unix: u64,
}
