//! Sync-peer slash codes + emission mapping (mission 0862m).
//!
//! Defines the canonical slash code constants used by RFC-0862 v1.1.0
//! Phase 4 ("Slashing for misbehaving sync peers") + the `SyncSlash`
//! enum that maps `SyncError` variants onto the slash-code space.
//! DomainCoordinator consumers (mission 0855p-c) emit
//! `SlashEnvelope`s carrying these codes to the reputation substrate.
//!
//! ## Slash code allocation
//!
//! Reserved range starts at `0x0020` to avoid collision with the
//! `PlatformType` range (`0x0001..=0x0015`) per `crates/octo-network/src/dot/domain.rs`.
//! Codes `0x0020..=0x0023` are the four sync-peer misbehavior triggers.

use crate::error::SyncError;

/// WAL entry fails CRC32 verification on the receiving side.
pub const SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY: u16 = 0x0020;

/// `SyncSummary.hmac` does not match the published `transport_key`.
pub const SLASH_CODE_SYNC_FAKE_SUMMARY: u16 = 0x0021;

/// An incoming LSN is below the per-peer watermark (LSN went backwards).
pub const SLASH_CODE_SYNC_LSN_REGRESSION: u16 = 0x0022;

/// Peer exceeded the rate limit repeatedly within the rolling window.
pub const SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION: u16 = 0x0023;

/// All slash reasons for the sync-peer category, in code order.
pub const ALL_SYNC_SLASH_CODES: &[u16] = &[
    SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY,
    SLASH_CODE_SYNC_FAKE_SUMMARY,
    SLASH_CODE_SYNC_LSN_REGRESSION,
    SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION,
];

/// Friendly name per slash code. Stable across replicas so audit
/// logs and Prometheus counters can be greppable.
pub fn slash_code_name(code: u16) -> Option<&'static str> {
    match code {
        SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY => Some("SyncCorruptedWalEntry"),
        SLASH_CODE_SYNC_FAKE_SUMMARY => Some("SyncFakeSummary"),
        SLASH_CODE_SYNC_LSN_REGRESSION => Some("SyncLsnRegression"),
        SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION => Some("SyncRateLimitViolation"),
        _ => None,
    }
}

/// True iff `code` is a reserved sync slash code (the four codes
/// above). Useful for "should this `WireError` be reported as a
/// peer slash?" pre-checks.
pub fn is_sync_slash_code(code: u16) -> bool {
    ALL_SYNC_SLASH_CODES.contains(&code)
}

/// What a sync-engine slash event carries when handed off to the
/// DomainCoordinator for emission.
///
/// The struct is local to `octo-sync`; the DC transcodes it into a
/// canonical `octo_network::mon::slash::SlashEnvelope` (with
/// `slash_reason` field set to the corresponding code and
/// `slash_reason_data` carrying any sub-fields). This keeps the
/// `octo-network` dependency direction one-way: sync → network,
/// never network → sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSlash {
    /// Slash code from `0x0020..=0x0023`. Stable across replicas.
    pub code: u16,
    /// Human-readable label, set from [`slash_code_name`].
    pub reason: &'static str,
    /// Identifier of the misbehaving peer (SubjectKeyId byte form).
    pub peer_id: [u8; 32],
    /// Optional sub-code. Two layouts share this slot depending on
    /// the slash reason:
    ///
    /// - For `SyncCorruptedWalEntry` / `SyncFakeSummary` /
    ///   `SyncRateLimitViolation`: the low 16 bits hold a
    ///   reason-specific sub-code (mirroring `BootstrapMisbehavior`'s
    ///   `(reason << 16) | sub` layout); the high 16 bits are zero.
    /// - For `SyncLsnRegression`: the high 16 bits hold the expected
    ///   LSN and the low 16 bits hold the offending (regressed) LSN,
    ///   so `sub_code = (expected << 16) | actual`. Callers decoding
    ///   this slot must branch on `code == SLASH_CODE_SYNC_LSN_REGRESSION`
    ///   before interpreting the bits as reason+sub.
    pub sub_code: u32,
}

impl SyncSlash {
    /// Build a slash event from a `SyncError`. Returns `None` if the
    /// error is not a sync-peer misbehavior (e.g., generic backend
    /// error, schema drift, etc.) — those are NOT slashed.
    ///
    /// **Rate-limit violation (0x0023) does not surface as a `SyncError`**
    /// — the rate limiter lives in the carrier/stream layer (`octo-sync/src/stream.rs::RateLimiter`)
    /// and emits a `WireError::RateLimit` from the transport layer. Slash
    /// emission for that case is wired at the carrier layer, not via this
    /// function. See `crates/octo-sync/src/stream.rs` for the
    /// `SyncSlash::rate_limit_for(peer_id)` helper.
    pub fn from_sync_error(peer_id: [u8; 32], err: &SyncError) -> Option<Self> {
        match err {
            SyncError::CorruptedWalEntry => Some(Self {
                code: SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY,
                reason: "SyncCorruptedWalEntry",
                peer_id,
                sub_code: 0,
            }),
            SyncError::FakeSummary => Some(Self {
                code: SLASH_CODE_SYNC_FAKE_SUMMARY,
                reason: "SyncFakeSummary",
                peer_id,
                sub_code: 0,
            }),
            SyncError::LsnRegression { expected, actual } => Some(Self {
                code: SLASH_CODE_SYNC_LSN_REGRESSION,
                reason: "SyncLsnRegression",
                peer_id,
                sub_code: ((*expected as u32) << 16) | (*actual as u32),
            }),
            _ => None,
        }
    }

    /// Build a slash event for the rate-limit violation case
    /// (`0x0023`). Called from `stream::RateLimiter` once a peer has
    /// exceeded the rolling-window budget by a configurable count.
    /// `excess_count` is the over-budget count that triggered the
    /// slash, useful for audit logs.
    pub fn rate_limit_for(peer_id: [u8; 32], excess_count: u32) -> Self {
        Self {
            code: SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION,
            reason: "SyncRateLimitViolation",
            peer_id,
            sub_code: excess_count,
        }
    }
}

/// Compute a CRC32 over a WAL entry payload (mission 0862m AC item 2).
///
/// This is the verification helper that the `apply_wal_entry` adapter
/// implementations call BEFORE handing the entry bytes to the engine.
/// The CRC32 is computed over the entire entry (header + body) using
/// the IEEE polynomial (`crc32fast::IEEE_TABLE`-backed).
///
/// Callers compare the returned hash to a header field that the
/// emitter populated. On mismatch the entry is rejected with
/// `SyncError::CorruptedWalEntry` and the peer is slashed per
/// `SyncSlash::from_sync_error`.
pub fn crc32_of_entry(entry: &[u8]) -> u32 {
    crc32fast::hash(entry)
}

/// Verify a WAL entry's CRC32 against an expected value. Returns
/// `Ok(())` on match, `Err(SyncError::CorruptedWalEntry)` on mismatch.
/// The actual CRC32 verification at each adapter site is performed
/// by calling this function once per inbound `apply_wal_entry`; the
/// adapter impl is responsible for slicing the expected CRC32 out of
/// the entry's header format.
pub fn verify_wal_crc32(entry: &[u8], expected_crc32: u32) -> Result<(), SyncError> {
    let actual = crc32_of_entry(entry);
    if actual == expected_crc32 {
        Ok(())
    } else {
        Err(SyncError::CorruptedWalEntry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{SyncError, WireError};

    #[test]
    fn slash_code_constants_are_distinct_and_in_reserved_range() {
        // 0x0020..=0x0023 reserved for sync, above PlatformType 0x0015.
        let mut codes = ALL_SYNC_SLASH_CODES.to_vec();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(
            codes.len(),
            4,
            "must have exactly 4 distinct sync slash codes"
        );
        for c in &codes {
            assert!(*c >= 0x0020, "below PlatformType range");
            assert!(*c <= 0x0023, "above reserved 4-slot");
        }
    }

    #[test]
    fn slash_code_names_match_constants() {
        assert_eq!(
            slash_code_name(SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY),
            Some("SyncCorruptedWalEntry")
        );
        assert_eq!(
            slash_code_name(SLASH_CODE_SYNC_FAKE_SUMMARY),
            Some("SyncFakeSummary")
        );
        assert_eq!(
            slash_code_name(SLASH_CODE_SYNC_LSN_REGRESSION),
            Some("SyncLsnRegression")
        );
        assert_eq!(
            slash_code_name(SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION),
            Some("SyncRateLimitViolation")
        );
        assert_eq!(slash_code_name(0xFFFF), None);
        assert_eq!(slash_code_name(0x0001), None); // PlatformType range — not sync
    }

    #[test]
    fn is_sync_slash_code_only_true_for_reserved_range() {
        for c in ALL_SYNC_SLASH_CODES {
            assert!(is_sync_slash_code(*c));
        }
        assert!(!is_sync_slash_code(0x0015)); // PlatformType::Quic
        assert!(!is_sync_slash_code(0x0024)); // outside the four-slot reservation
    }

    #[test]
    fn from_sync_error_maps_each_slash_reason() {
        let peer = [0xAAu8; 32];
        assert_eq!(
            SyncSlash::from_sync_error(peer, &SyncError::CorruptedWalEntry).map(|s| s.code),
            Some(SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY)
        );
        assert_eq!(
            SyncSlash::from_sync_error(peer, &SyncError::FakeSummary).map(|s| s.code),
            Some(SLASH_CODE_SYNC_FAKE_SUMMARY)
        );
        assert_eq!(
            SyncSlash::from_sync_error(
                peer,
                &SyncError::LsnRegression {
                    expected: 10,
                    actual: 7,
                },
            )
            .map(|s| (s.code, s.sub_code)),
            Some((SLASH_CODE_SYNC_LSN_REGRESSION, (10u32 << 16) | 7u32))
        );
        // Non-slash errors return None (e.g., generic backend errors).
        assert!(
            SyncSlash::from_sync_error(peer, &SyncError::BackendNotReady("x".into())).is_none()
        );
    }

    #[test]
    fn rate_limit_for_helpers_use_reserved_code() {
        let peer = [0xBBu8; 32];
        let s = SyncSlash::rate_limit_for(peer, 5);
        assert_eq!(s.code, SLASH_CODE_SYNC_RATE_LIMIT_VIOLATION);
        assert_eq!(s.sub_code, 5);
    }

    #[test]
    fn crc32_matches_reference_implementation_on_known_vectors() {
        // Reference vectors from IEEE 802.3 / ITU-T V.42 (the IEEE-802.3
        // CRC-32, polynomial 0x04C11DB7). The well-known check value
        // for the ASCII string "123456789" is 0xCBF43926. (RFC 3309
        // and CRC-32C use polynomial 0x1EDC6F41 with a different
        // check value — that polynomial is NOT what crc32fast::hash
        // computes, so do not cite RFC 3309 here.)
        // Empty input → CRC32 = 0x00000000.
        assert_eq!(crc32_of_entry(b""), 0x00000000);
        // "123456789" → 0xCBF43926 (the canonical IEEE polynomial check).
        assert_eq!(crc32_of_entry(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn verify_wal_crc32_ok_on_match() {
        let entry = b"abc";
        let crc = crc32_of_entry(entry);
        assert_eq!(verify_wal_crc32(entry, crc), Ok(()));
    }

    #[test]
    fn verify_wal_crc32_err_on_mismatch() {
        let entry = b"abc";
        let result = verify_wal_crc32(entry, 0xDEADBEEF);
        assert_eq!(result, Err(SyncError::CorruptedWalEntry));
        // Mapping to a slash event picks the right code.
        let slash = SyncSlash::from_sync_error([0u8; 32], &result.unwrap_err())
            .expect("CorruptedWalEntry must be slashable");
        assert_eq!(slash.code, SLASH_CODE_SYNC_CORRUPTED_WAL_ENTRY);
    }

    #[test]
    fn wire_error_code_stable_for_fake_summary() {
        // WireError::FakeSummary is the on-wire counterpart for the
        // slash code. They MUST move together so the DC consumer and
        // the remote peer agree.
        assert_eq!(WireError::FakeSummary.code(), 0x0B);
        // The slash code is the high-level mission constant; the
        // wire-error code is the transport-layer encoding. They are
        // intentionally distinct but referenced together so the
        // consumer's slash event points back at `FakeSummary`.
        assert_eq!(
            slash_code_name(SLASH_CODE_SYNC_FAKE_SUMMARY),
            Some("SyncFakeSummary")
        );
    }
}
