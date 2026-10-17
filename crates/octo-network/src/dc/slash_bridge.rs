//! Sync-peer slash transcoding bridge (mission 0862m1).
//!
//! Translates canonical `octo_sync::slash::SyncSlash` events into
//! the network-layer `SlashEnvelope` so the DomainCoordinator can
//! emit them through `octo_network::mon::slash_aggregation`. This
//! keeps the `octo-sync → octo-network` dep direction one-way:
//! `octo-network` already depends on `octo-sync`; the bridge lives
//! in `octo-network/src/dc/` and `octo-sync` MUST NOT depend on
//! `octo-network`.
//!
//! ## Field mapping
//!
//! | `SyncSlash` | `SlashEnvelope` | Notes |
//! |---|---|---|
//! | `code: u16` | `slash_reason: u16` | Pass-through if known sync code |
//! | `sub_code: u32` | `slash_reason_data: u32` | `SyncLsnRegression`: `(expected << 16) \| actual` |
//! | `peer_id: [u8; 32]` | `target_peer: String` | Hex-encoded; DC stores per-target counter |
//! | `reason: &'static str` | `domain_id: String` | DC domain scope is derived from the slash code |
//! | n/a | `slash_id: String` | Derived from `(peer_hex, code)` so duplicate emissions are dedup'd |
//! | n/a | `signature: Vec<u8>` | Empty for sync-side; witness layer adds it later |
//! | n/a | `cast_at: u64` | Caller-supplied epoch (tests pass deterministic value) |
//!
//! ## peer_id → RecorderDid mapping (RFC-0968-A1 amendment 29)
//!
//! Sync's `peer_id` is a 32-byte SubjectKeyId; the canonical
//! `RecorderDid` is 52 bytes (32-byte `blake3(pubkey)` prefix +
//! 20-byte version discriminator). The bridge zero-pads the
//! 32-byte SubjectKeyId into the 52-byte `RecorderDid` bytes. This
//! is intentionally NOT a re-encoding — the bridge is a one-way
//! handoff to the reputation layer; identity resolution (if
//! needed) happens at the DC boundary, not here.

use octo_reputation::types::RecorderDid;
use octo_sync::slash::SyncSlash;

use crate::mon::slash::SlashEnvelope;

/// Errors from the sync-slash bridge.
#[derive(Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// The sync slash code is not in the reserved sync range
    /// (`0x0020..=0x0023`). Bridge refuses to map unknown codes
    /// (RFC-0855p-c §9 forward-compatibility: never silently
    /// re-interpret unknown slash reasons).
    UnknownSlashCode(u16),
    /// The peer id bytes are not representable as a `RecorderDid`.
    /// Currently unreachable (the bridge zero-pads), but reserved
    /// for the future 52-byte sync-side representation (RFC-0968-A1
    /// amendment 29 alignment).
    PeerIdMappingFailed,
}

/// Manual redacting `Debug` per RFC-0957-A1 §Security (defense in
/// depth: never leak slash reason context via `Debug`).
impl std::fmt::Debug for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSlashCode(_) => f
                .debug_tuple("UnknownSlashCode")
                .field(&"[REDACTED code]")
                .finish(),
            Self::PeerIdMappingFailed => f.write_str("PeerIdMappingFailed"),
        }
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSlashCode(_) => f.write_str("unknown sync slash code"),
            Self::PeerIdMappingFailed => f.write_str("peer id mapping to RecorderDid failed"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Translate a `SyncSlash` into the canonical network-layer
/// `SlashEnvelope`. The bridge is the single canonical translation
/// point; downstream DC code consumes `SlashEnvelope` only.
///
/// `domain_id` is the receiving DC's domain scope (e.g. `"sync"` or
/// the gossip platform tag — caller-supplied because the sync
/// engine doesn't know its DC topology). `slash_id` is derived
/// deterministically from `(peer_hex, code)` so duplicate
/// `record_slash` calls are idempotent on the gossip substrate's
/// dedup table.
///
/// `cast_at_unix` is the witness-layer epoch timestamp; the
/// signature field is left empty because sync-engine slashes are
/// NOT yet witness-signed at this layer (the DC adds signatures
/// when relaying the envelope to mission-level witnesses).
pub fn encode_sync_slash(
    sync_slash: &SyncSlash,
    domain_id: impl Into<String>,
    cast_at_unix: u64,
) -> Result<SlashEnvelope, BridgeError> {
    if !octo_sync::slash::is_sync_slash_code(sync_slash.code) {
        return Err(BridgeError::UnknownSlashCode(sync_slash.code));
    }

    let peer_hex = hex::encode(sync_slash.peer_id);
    let slash_id = format!("sync:{peer_hex}:{:04x}", sync_slash.code);

    Ok(SlashEnvelope {
        domain_id: domain_id.into(),
        slash_id,
        slash_reason: sync_slash.code,
        slash_reason_data: sync_slash.sub_code,
        target_peer: peer_hex,
        signature: Vec::new(),
        cast_at: cast_at_unix,
    })
}

/// Translate a `SyncSlash` into a canonical `RecorderDid`. The
/// bridge zero-pads the 32-byte sync SubjectKeyId into the 52-byte
/// canonical DID bytes. The 20-byte version discriminator is
/// filled with zeros (sync-engine peers do not yet carry an
/// RFC-0010 discriminator; the bridge is a one-way handoff).
pub fn sync_peer_to_recorder_did(peer_id: &[u8; 32]) -> RecorderDid {
    let mut arr = [0u8; 52];
    arr[..32].copy_from_slice(peer_id);
    RecorderDid::from_array(arr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_sync::error::SyncError;
    use octo_sync::slash::{
        SyncSlash, SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY, SLASH_CODE_SYNC_FAKE_SUMMARY,
        SLASH_CODE_SYNC_LSN_REGRESSION, SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION,
    };

    #[test]
    fn unknown_code_rejected() {
        let slash = SyncSlash {
            code: 0x9999, // not in 0x0020..=0x0023
            reason: "Unknown",
            peer_id: [0xAA; 32],
            sub_code: 0,
        };
        let result = encode_sync_slash(&slash, "sync", 1700000000);
        assert_eq!(result, Err(BridgeError::UnknownSlashCode(0x9999)));
    }

    #[test]
    fn fake_summary_maps_to_0x0021_with_peer_hex() {
        let peer = [0xABu8; 32];
        let slash = SyncSlash::from_sync_error(peer, &SyncError::FakeSummary)
            .expect("FakeSummary must be slashable");
        let env = encode_sync_slash(&slash, "sync", 1700000000).unwrap();
        assert_eq!(env.slash_reason, SLASH_CODE_SYNC_FAKE_SUMMARY);
        assert_eq!(env.slash_reason_data, 0);
        assert_eq!(env.target_peer, hex::encode(peer));
        assert_eq!(env.domain_id, "sync");
        assert!(env.signature.is_empty());
        assert_eq!(env.cast_at, 1700000000);
        assert!(env.slash_id.starts_with("sync:"));
        assert!(env.slash_id.ends_with(":0021"));
    }

    #[test]
    fn corrupted_wal_entry_maps_to_0x0020() {
        let peer = [0xCDu8; 32];
        let slash = SyncSlash::from_sync_error(peer, &SyncError::CorruptedWalEntry).unwrap();
        let env = encode_sync_slash(&slash, "sync", 1700000001).unwrap();
        assert_eq!(env.slash_reason, SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY);
        assert_eq!(env.target_peer, hex::encode(peer));
    }

    #[test]
    fn lsn_regression_preserves_expected_actual_layout() {
        let peer = [0xEFu8; 32];
        let slash = SyncSlash::from_sync_error(
            peer,
            &SyncError::LsnRegression {
                expected: 100,
                actual: 50,
            },
        )
        .unwrap();
        let env = encode_sync_slash(&slash, "sync", 1700000002).unwrap();
        assert_eq!(env.slash_reason, SLASH_CODE_SYNC_LSN_REGRESSION);
        // (expected << 16) | actual
        assert_eq!(env.slash_reason_data, (100u32 << 16) | 50u32);
    }

    #[test]
    fn rate_limit_violation_round_trips() {
        let peer = [0x11u8; 32];
        let slash = SyncSlash::rate_limit_for(peer, 5);
        let env = encode_sync_slash(&slash, "sync", 1700000003).unwrap();
        assert_eq!(env.slash_reason, SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION);
        assert_eq!(env.slash_reason_data, 5);
    }

    #[test]
    fn slash_id_is_deterministic_for_same_input() {
        let peer = [0xABu8; 32];
        let slash = SyncSlash::from_sync_error(peer, &SyncError::FakeSummary).unwrap();
        let env_a = encode_sync_slash(&slash, "sync", 1700000000).unwrap();
        let env_b = encode_sync_slash(&slash, "sync", 1700000000).unwrap();
        assert_eq!(env_a.slash_id, env_b.slash_id);
    }

    #[test]
    fn sync_peer_to_recorder_did_zero_pads_to_52() {
        let peer = [0xABu8; 32];
        let did = sync_peer_to_recorder_did(&peer);
        let bytes = did.as_bytes();
        // First 32 bytes = peer_id
        assert_eq!(&bytes[..32], &peer[..]);
        // Last 20 bytes = zeros
        assert_eq!(&bytes[32..], &[0u8; 20]);
    }

    #[test]
    fn bridge_error_debug_redacts_code() {
        let err = BridgeError::UnknownSlashCode(0x9999);
        let dbg = format!("{err:?}");
        // Redacting Debug: must NOT include the raw code value.
        assert!(
            !dbg.contains("0x9999"),
            "Debug must redact code value: {dbg}"
        );
        assert!(dbg.contains("UnknownSlashCode"));
        assert!(dbg.contains("REDACTED"));
    }

    #[test]
    fn bridge_error_display_hides_specifics() {
        let err = BridgeError::UnknownSlashCode(0x0021);
        let s = format!("{err}");
        assert!(!s.contains("0x0021"));
        assert!(s.contains("unknown"));
    }
}
