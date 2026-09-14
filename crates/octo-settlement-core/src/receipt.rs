//! Canonical `Receipt` struct per RFC-0014 §Module Layout `receipt` +
//! RFC-0959 §Data Structures.
//!
//! RFC-0014-v2 substrate amendment + RFC-0016-a §6.5: `Receipt` gains
//! additive fields (`model`, `cost_dqa`, `capability_root`,
//! `subject_did`, `status`). All new fields carry `Default` defaults
//! to preserve backwards-compat with existing `Receipt` literals
//! (test sites updated with `..Default::default()`). The canonical
//! `receipt_id_for` hash continues over the ORIGINAL 6 fields only
//! so existing receipts (written before RFC-0016-a) remain
/// chain-valid.
use serde::{Deserialize, Serialize};

/// Canonical settlement receipt struct. Field shape byte-identical to
/// RFC-0959 §Data Structures + RFC-0014 §Module Layout `receipt`,
/// extended per RFC-0014-v2 (Layer A frozen extension under
/// [[cipherocto-design-principles]] §Extension over enumeration).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Model identifier (RFC-0014-v2 extension; default empty string
    /// preserves chain validity for receipts written before
    /// RFC-0016-a acceptance).
    #[serde(default)]
    pub model: String,
    /// Settlement cost in DQA (RFC-0014-v2 extension; default 0
    /// preserves chain validity).
    #[serde(default)]
    pub cost_dqa: u64,
    /// Capability root hash (RFC-0014-v2 extension; default `[0; 32]`
    /// preserves chain validity).
    #[serde(default)]
    pub capability_root: [u8; 32],
    /// Subject DID (RFC-0014-v2 extension; raw canonical wire form;
    /// default empty string preserves chain validity).
    #[serde(default)]
    pub subject_did: String,
    /// Receipt status (RFC-0014-v2 extension; default `Unknown`
    /// preserves chain validity — receipts written before
    /// RFC-0016-a acceptance deserialize to `Unknown`).
    #[serde(default)]
    pub status: ReceiptStatus,
}

/// Canonical receipt status enum (RFC-0014-v2 §Extension +
/// RFC-0016-a §6.5 `ReceiptSummary::from_canonical`).
///
/// `#[non_exhaustive]` per [[cipherocto-design-principles]]
/// §Extension over enumeration: future status variants land
/// additively (e.g., `Expired`, `Refunded`) without breaking
/// downstream matchers. The `Unknown` variant is the
/// RFC-0014-v2-paired-decode default for receipts written before
/// `ReceiptStatus` landed (serde `#[serde(default)]` round-trip
/// compatibility).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReceiptStatus {
    /// Pre-RFC-0014-v2 receipts decode to this; never written by
    /// RFC-0016-a-aware sinks.
    #[default]
    Unknown,
    /// Settled successfully — full amount transferred.
    Ok,
    /// Settled partially — partial amount transferred (the
    /// settlement reached a checkpoint before full transfer).
    Partial,
    /// Settlement rejected — the underlying settlement transaction
    /// failed; no funds transferred.
    Reject,
}

impl ReceiptStatus {
    /// Canonical lowercase wire form for CLI / canonical bytes
    /// emission (RFC-0016-a §6.5 `ReceiptSummary::from_canonical`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::Reject => "reject",
        }
    }
}

/// Canonical receipt primary-key newtype (RFC-0016-a §6.4 —
/// `ReceiptId(pub u64)` paired with `receipt_id_for_digest`
/// reverse-mapping function).
///
/// Wraps the raw `u64` monotonic receipt_id. The newtype is
/// `Copy + Hash + Ord` so it composes with `BTreeMap<ReceiptId,
/// Receipt>` ordering invariants. `Display` emits the canonical
/// decimal `u64` form per RFC-0016-a §6.4 substrate contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReceiptId(pub u64);

impl ReceiptId {
    /// Wrap a raw `u64` receipt_id into the canonical newtype.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Access the raw `u64` (programmatic callers only; CLI surfaces
    /// the newtype via `Display`).
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ReceiptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for ReceiptId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}
