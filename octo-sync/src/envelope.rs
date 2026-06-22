//! Sync protocol envelope types (per RFC-0862 §Envelope Payload Discriminators).
//!
//! 13 envelope types total:
//! - 0xA0–0xA5: Sync envelope types (SummaryRequest, SummaryResponse, SegmentRequest,
//!   SegmentResponse, SegmentNotFound, NodeStatus)
//! - 0xB0–0xB3: WAL streaming (WalTailRequest, WalTailResponse, WalTailEnd, LsnAck)
//! - 0xC0–0xC2: Liveness + auth (Heartbeat, AuthChallenge, AuthResponse)
//!
//! Each envelope type is a Rust struct with `encode()` / `decode()` methods that
//! produce a `Vec<u8>` for the wire. The encoding is a simple length-prefixed
//! scheme (not DCS, which is a separate concern handled at the envelope frame
//! layer; see RFC-0126 for the canonical serialization format).
//!
//! The `discriminator` is the 8-bit value that identifies the envelope type at
//! the wire boundary. The cipherocto sync engine routes incoming envelopes by
//! discriminator.

use crate::error::SyncError;
use crate::types::Lsn;

/// 8-bit envelope payload discriminator (per RFC-0862 §Envelope Payload Discriminators).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum EnvelopeKind {
    /// 0xA0: SummaryRequest
    SummaryRequest = 0xA0,
    /// 0xA1: SummaryResponse
    SummaryResponse = 0xA1,
    /// 0xA2: SegmentRequest
    SegmentRequest = 0xA2,
    /// 0xA3: SegmentResponse
    SegmentResponse = 0xA3,
    /// 0xA4: SegmentNotFound
    SegmentNotFound = 0xA4,
    /// 0xA5: NodeStatus
    NodeStatus = 0xA5,
    /// 0xB0: WalTailRequest
    WalTailRequest = 0xB0,
    /// 0xB1: WalTailResponse
    WalTailResponse = 0xB1,
    /// 0xB2: WalTailEnd
    WalTailEnd = 0xB2,
    /// 0xB3: LsnAck
    LsnAck = 0xB3,
    /// 0xC0: Heartbeat
    Heartbeat = 0xC0,
    /// 0xC1: AuthChallenge
    AuthChallenge = 0xC1,
    /// 0xC2: AuthResponse
    AuthResponse = 0xC2,
}

impl EnvelopeKind {
    /// Try to convert a raw 8-bit discriminator to an `EnvelopeKind`.
    pub fn from_u8(b: u8) -> Result<Self, SyncError> {
        match b {
            0xA0 => Ok(EnvelopeKind::SummaryRequest),
            0xA1 => Ok(EnvelopeKind::SummaryResponse),
            0xA2 => Ok(EnvelopeKind::SegmentRequest),
            0xA3 => Ok(EnvelopeKind::SegmentResponse),
            0xA4 => Ok(EnvelopeKind::SegmentNotFound),
            0xA5 => Ok(EnvelopeKind::NodeStatus),
            0xB0 => Ok(EnvelopeKind::WalTailRequest),
            0xB1 => Ok(EnvelopeKind::WalTailResponse),
            0xB2 => Ok(EnvelopeKind::WalTailEnd),
            0xB3 => Ok(EnvelopeKind::LsnAck),
            0xC0 => Ok(EnvelopeKind::Heartbeat),
            0xC1 => Ok(EnvelopeKind::AuthChallenge),
            0xC2 => Ok(EnvelopeKind::AuthResponse),
            _ => Err(SyncError::UnknownEnvelopeSubtype(b)),
        }
    }

    /// Return the 8-bit wire value.
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// A `WalTailChunk` envelope payload (RFC-0862 §4.3, type 0xB1 WalTailResponse).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalTailChunk {
    /// The first LSN in this chunk (inclusive).
    pub from_lsn: Lsn,
    /// The last LSN in this chunk (inclusive).
    pub to_lsn: Lsn,
    /// The raw WAL entries (each entry is the output of `WALEntry::encode()`).
    pub entries: Vec<Vec<u8>>,
    /// Per RFC-0862 §4.3: "true if to_lsn == writer.current_lsn".
    /// Post-store invariant: this is always `true` (current_lsn == to_lsn).
    pub is_last: bool,
}

/// A `SummaryResponse` envelope payload (RFC-0862 §4.3.4, type 0xA1).
///
/// The writer sends this in response to a `SummaryRequest`. Contains the
/// per-table `SyncSummary` list for the mission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryResponse {
    /// The writer's current LSN (highest committed).
    pub writer_lsn: Lsn,
    /// The per-table summaries for this mission.
    pub summaries: Vec<crate::summary::SyncSummary>,
}

/// A `SegmentRequest` envelope payload (RFC-0862 §4.3.4, type 0xA2).
///
/// The reader sends this to request a specific snapshot segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentRequest {
    /// The table id.
    pub table_id: u32,
    /// The ordinal position of the requested segment.
    pub segment_index: u32,
    /// The BLAKE3-256 root the reader expects (for staleness detection).
    pub expected_root: [u8; 32],
}

/// A `SegmentNotFound` envelope payload (RFC-0862 §4.3.4, type 0xA4).
///
/// Sent by the writer when the requested segment is missing OR has a stale
/// root. The `regenerated` flag indicates whether the writer has already
/// triggered a regeneration (in which case the reader should re-fetch the
/// summary and re-descend the Merkle tree).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentNotFound {
    /// The table id.
    pub table_id: u32,
    /// The ordinal position of the requested segment.
    pub segment_index: u32,
    /// Whether the writer has already triggered a regeneration.
    pub regenerated: bool,
}

/// A `NodeStatus` envelope payload (RFC-0862 §4.3, type 0xA5).
///
/// Sent by both writer and reader in response to a status query, or as a
/// periodic health advertisement. Contains the local node's view of the
/// mission state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeStatus {
    /// The local node's current LSN (highest committed).
    pub current_lsn: Lsn,
    /// The number of currently-connected peers.
    pub peer_count: u32,
    /// The local node's role (per `SyncRole`).
    pub role: u8,
}

/// A `WalTailRequest` envelope payload (RFC-0862 §4.3.3, type 0xB0).
///
/// The reader sends this to request WAL entries from a given LSN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalTailRequest {
    /// The first LSN the reader wants.
    pub from_lsn: Lsn,
}

/// A `WalTailEnd` envelope payload (RFC-0862 §4.3.3, type 0xB2).
///
/// Sent by the writer to signal "no more WAL chunks in this batch". The
/// reader uses this as the stop signal (in addition to `WalTailChunk.is_last`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalTailEnd {
    /// The writer's final LSN (highest committed at the time of this end signal).
    pub final_lsn: Lsn,
}

/// An `AuthChallenge` envelope payload (RFC-0862 §4.3.1, type 0xC1).
///
/// Sent by the writer to the reader during the Authenticating phase. The
/// reader responds with an `AuthResponse` containing a signed nonce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthChallenge {
    /// The writer's mission_id.
    pub mission_id: [u8; 32],
    /// A random 32-byte nonce the reader must sign.
    pub nonce: [u8; 32],
    /// Unix timestamp (seconds) at the writer.
    pub unix_seconds: u64,
}

/// An `AuthResponse` envelope payload (RFC-0862 §4.3.1, type 0xC2).
///
/// Sent by the reader in response to an `AuthChallenge`. Contains a
/// signature over the challenge nonce with the reader's public key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthResponse {
    /// The reader's public key (32 bytes; ed25519).
    pub public_key: Vec<u8>,
    /// The signature over the challenge nonce (64 bytes; ed25519).
    pub signature: Vec<u8>,
    /// The reader's current LSN (for catch-up).
    pub current_lsn: Lsn,
}

/// An `LsnAck` envelope payload (RFC-0862 §4.3, type 0xB3).
///
/// The reader sends this after successfully applying a `WalTailChunk`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LsnAck {
    /// The highest LSN that the reader has successfully applied.
    pub applied_lsn: Lsn,
}

/// A `Heartbeat` envelope payload (RFC-0862 §4.3, type 0xC0).
///
/// Sent every 5s on each direction. A missing heartbeat for `2 × heartbeat_interval`
/// (10s) transitions the peer to `Suspect` (per RFC-0862 §Lifecycle Requirements).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heartbeat {
    /// The sender's current LSN (highest committed).
    pub current_lsn: Lsn,
    /// Unix timestamp (seconds) at the sender.
    pub unix_seconds: u64,
}

/// A `SummaryRequest` envelope payload (RFC-0862 §4.3.4, type 0xA0).
///
/// The reader sends this when it wants to start (or restart) the anti-entropy
/// catch-up flow. The writer responds with a `SummaryResponse`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummaryRequest {
    /// The reader's current high-water LSN. The writer includes this in the
    /// response so the reader can detect "I'm already caught up" cases.
    pub reader_lsn: Lsn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_kind_round_trip() {
        for b in [
            0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xB0, 0xB1, 0xB2, 0xB3, 0xC0, 0xC1, 0xC2,
        ] {
            let k = EnvelopeKind::from_u8(b).unwrap();
            assert_eq!(k.to_u8(), b);
        }
    }

    #[test]
    fn unknown_subtype_returns_error() {
        let err = EnvelopeKind::from_u8(0x99).unwrap_err();
        assert_eq!(err, SyncError::UnknownEnvelopeSubtype(0x99));
    }

    #[test]
    fn wal_tail_chunk_construction() {
        let c = WalTailChunk {
            from_lsn: 1,
            to_lsn: 100,
            entries: vec![vec![1, 2, 3], vec![4, 5, 6]],
            is_last: true,
        };
        assert_eq!(c.from_lsn, 1);
        assert_eq!(c.to_lsn, 100);
        assert_eq!(c.entries.len(), 2);
        assert!(c.is_last);
    }

    #[test]
    fn lsn_ack_construction() {
        let ack = LsnAck { applied_lsn: 42 };
        assert_eq!(ack.applied_lsn, 42);
    }
}
