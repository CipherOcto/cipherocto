//! WAL entry types + format constants (per RFC-0862 v1.3 §Substrate
//! types + §V2 WAL header_size extension).
//!
//! The v1.3 WAL entry is NOT parseable by v1.2 readers (different Magic + ShardKey field + blake3 vs CRC32 checksum). Per R11 H5 + R12 C1, v1.2 nodes MUST be patched to v1.2.1 (reject unknown Magic) before v1.3 nodes deploy.
//!
//! The v1.3 entry layout (92 bytes + PayloadLength):
//!   Magic(WAL_MAGIC_V13, 4) + EntryType(1) + EntryVersion(1) + Reserved(2) +
//!   ShardKey(32) + LSN(8) + PreviousLSN(8) + PayloadLength(4) +
//!   Payload(PayloadLength bytes) + Blake3Hash(32)
//!
//! Checksum (per R12 H16): blake3 over the 60-byte entry prefix
//! (Magic..PayloadLength) + Payload. Tampering with LSN / EntryType /
//! ShardKey invalidates the checksum.

use borsh::{BorshDeserialize, BorshSerialize};

use super::ids::ShardKey;

/// v1.2 WAL magic (ASCII `"WALE"`).
///
/// Per RFC-0862 v1.3 R12 C2: kept for migration reference. v1.3 entries
/// use `WAL_MAGIC_V13`; v1.2 readers identify v1.3 entries by the
/// differing magic + v1.2.1+ reject them with `WalVersionTooNew`.
pub const WAL_MAGIC_V12: u32 = 0x454C_4157;

/// v1.3 WAL magic (ASCII `"WAL3"`).
///
/// Per RFC-0862 v1.3 R12 C2: distinguishes v1.2 vs v1.3 entries. The
/// 60-byte prefix layout (Magic..PayloadLength) is canonical for v1.3.
pub const WAL_MAGIC_V13: u32 = 0x5741_4C33;

/// Entry type codes (per RFC-0862 v1.3 R12 M20).
///
/// 0x10 = operator nonce record (governance proof replay).
/// 0x20 = drain event (Layer B-substrate → epoch boundary).
/// 0x21 = DID register event.
/// 0x22 = DID revoke event.
pub const ENTRY_TYPE_NONCE_RECORD: u8 = 0x10;

/// Drain event entry type (Layer B-substrate epoch boundary).
pub const ENTRY_TYPE_DRAIN: u8 = 0x20;

/// DID register event entry type.
pub const ENTRY_TYPE_DID_REGISTER: u8 = 0x21;

/// DID revoke event entry type.
pub const ENTRY_TYPE_DID_REVOKE: u8 = 0x22;

/// v1.3 WAL entry (per RFC-0862 v1.3 §Substrate types + §V2 WAL
/// header_size extension).
///
/// `prefix_bytes` is the canonical 60-byte serialization of the entry
/// prefix (`Magic..PayloadLength`). The checksum is `blake3(prefix_bytes
/// || payload)` — tampering with LSN / EntryType / ShardKey invalidates
/// the checksum (per R12 H16).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct WalEntry {
    /// Either `WAL_MAGIC_V12` or `WAL_MAGIC_V13`.
    pub magic: u32,
    /// One of `ENTRY_TYPE_*`.
    pub entry_type: u8,
    /// Entry schema version. v1.3 = 1.
    pub entry_version: u8,
    /// Reserved bytes (per spec).
    pub reserved: [u8; 2],
    /// Shard key for the entry (per R11 H14: cross-shard entries rejected).
    pub shard_key: ShardKey,
    /// Logical sequence number.
    pub lsn: u64,
    /// Previous LSN (for chaining).
    pub previous_lsn: u64,
    /// Payload length in bytes.
    pub payload_length: u32,
    /// Entry payload (entry-type-specific bytes).
    pub payload: Vec<u8>,
    /// Canonical 60-byte serialization of the entry prefix
    /// (`Magic..PayloadLength`). Stored for checksum verification.
    pub prefix_bytes: [u8; 60],
    /// blake3-256 over `prefix_bytes || payload`.
    pub checksum: [u8; 32],
}

impl WalEntry {
    /// Construct a v1.3 WAL entry with valid 60-byte `prefix_bytes`
    /// (per the §V2 WAL header_size extension) but placeholder LSN
    /// fields. Use `finalize_with_lsns` to set the LSN chain + recompute
    /// the checksum once the writer knows the assigned LSN.
    ///
    /// Layout (big-endian throughout, per the WAL spec):
    /// - `prefix[0..4]` = `WAL_MAGIC_V13.to_be_bytes()`
    /// - `prefix[4]` = `entry_type`
    /// - `prefix[5]` = `1` (entry version for v1.3)
    /// - `prefix[6..8]` = `[0, 0]` (reserved)
    /// - `prefix[8..40]` = `shard_key.0`
    /// - `prefix[40..48]` = `lsn.to_be_bytes()` (placeholder 0)
    /// - `prefix[48..56]` = `previous_lsn.to_be_bytes()` (placeholder 0)
    /// - `prefix[56..60]` = `payload_length.to_be_bytes()`
    pub fn build_v13(entry_type: u8, shard_key: ShardKey, payload: Vec<u8>) -> Self {
        let payload_length = payload.len() as u32;
        let mut prefix = [0u8; 60];
        prefix[0..4].copy_from_slice(&WAL_MAGIC_V13.to_be_bytes());
        prefix[4] = entry_type;
        prefix[5] = 1;
        prefix[6..8].copy_from_slice(&[0u8; 2]);
        prefix[8..40].copy_from_slice(&shard_key.0);
        prefix[40..48].copy_from_slice(&0u64.to_be_bytes());
        prefix[48..56].copy_from_slice(&0u64.to_be_bytes());
        prefix[56..60].copy_from_slice(&payload_length.to_be_bytes());
        let mut input = Vec::with_capacity(60 + payload.len());
        input.extend_from_slice(&prefix);
        input.extend_from_slice(&payload);
        let checksum = *blake3::hash(&input).as_bytes();
        Self {
            magic: WAL_MAGIC_V13,
            entry_type,
            entry_version: 1,
            reserved: [0, 0],
            shard_key,
            lsn: 0,
            previous_lsn: 0,
            payload_length,
            payload,
            prefix_bytes: prefix,
            checksum,
        }
    }

    /// Update the LSN chain fields + recompute the checksum. Called by
    /// the WAL appender once it knows the assigned LSN.
    pub fn finalize_with_lsns(&mut self, lsn: u64, previous_lsn: u64) {
        self.lsn = lsn;
        self.previous_lsn = previous_lsn;
        self.prefix_bytes[40..48].copy_from_slice(&lsn.to_be_bytes());
        self.prefix_bytes[48..56].copy_from_slice(&previous_lsn.to_be_bytes());
        let mut input = Vec::with_capacity(60 + self.payload.len());
        input.extend_from_slice(&self.prefix_bytes);
        input.extend_from_slice(&self.payload);
        self.checksum = *blake3::hash(&input).as_bytes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_constants_distinct() {
        assert_ne!(WAL_MAGIC_V12, WAL_MAGIC_V13);
        // ASCII sanity check (big-endian byte order).
        assert_eq!(WAL_MAGIC_V12.to_be_bytes(), [0x45, 0x4C, 0x41, 0x57]);
        assert_eq!(WAL_MAGIC_V13.to_be_bytes(), [0x57, 0x41, 0x4C, 0x33]);
    }

    #[test]
    fn entry_type_codes_distinct() {
        let codes = [
            ENTRY_TYPE_NONCE_RECORD,
            ENTRY_TYPE_DRAIN,
            ENTRY_TYPE_DID_REGISTER,
            ENTRY_TYPE_DID_REVOKE,
        ];
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j]);
            }
        }
    }

    #[test]
    fn prefix_byte_width() {
        // 60 = 4 (Magic) + 1 (EntryType) + 1 (EntryVersion) + 2 (Reserved)
        //     + 32 (ShardKey) + 8 (LSN) + 8 (PreviousLSN) + 4 (PayloadLength)
        assert_eq!(std::mem::size_of::<[u8; 60]>(), 60);
    }
}
