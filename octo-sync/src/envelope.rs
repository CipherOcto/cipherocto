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

impl WalTailChunk {
    /// Encode to binary wire format (little-endian, length-prefixed entries).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.from_lsn.to_le_bytes());
        buf.extend_from_slice(&self.to_lsn.to_le_bytes());
        buf.push(self.is_last as u8);
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            buf.extend_from_slice(&(entry.len() as u32).to_le_bytes());
            buf.extend_from_slice(entry);
        }
        buf
    }

    /// Decode from binary wire format.
    pub fn decode(data: &[u8]) -> Result<Self, SyncError> {
        if data.len() < 21 {
            return Err(SyncError::BackendNotReady("WalTailChunk too short".into()));
        }
        let mut off = 0;
        let from_lsn = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        off += 8;
        let to_lsn = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        off += 8;
        let is_last = data[off] != 0;
        off += 1;
        let count = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if off + 4 > data.len() {
                return Err(SyncError::BackendNotReady(
                    "WalTailChunk entry length truncated".into(),
                ));
            }
            let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            if off + len > data.len() {
                return Err(SyncError::BackendNotReady(
                    "WalTailChunk entry data truncated".into(),
                ));
            }
            entries.push(data[off..off + len].to_vec());
            off += len;
        }
        Ok(WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last,
        })
    }
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

impl SummaryResponse {
    /// Encode to binary wire format.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.writer_lsn.to_le_bytes());
        buf.extend_from_slice(&(self.summaries.len() as u32).to_le_bytes());
        for s in &self.summaries {
            buf.extend_from_slice(&s.table_id.to_le_bytes());
            buf.extend_from_slice(&s.segment_count.to_le_bytes());
            buf.extend_from_slice(&s.segment_root);
            buf.extend_from_slice(&s.lsn_watermark.to_le_bytes());
            buf.extend_from_slice(&s.hmac);
        }
        buf
    }

    /// Decode from binary wire format.
    pub fn decode(data: &[u8]) -> Result<Self, SyncError> {
        if data.len() < 8 {
            return Err(SyncError::BackendNotReady(
                "SummaryResponse too short".into(),
            ));
        }
        let mut off = 0;
        let writer_lsn = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        off += 8;
        let count = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        let mut summaries = Vec::with_capacity(count);
        for _ in 0..count {
            if off + 52 > data.len() {
                return Err(SyncError::BackendNotReady(
                    "SummaryResponse truncated".into(),
                ));
            }
            let table_id = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
            off += 4;
            let segment_count = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
            off += 4;
            let mut segment_root = [0u8; 32];
            segment_root.copy_from_slice(&data[off..off + 32]);
            off += 32;
            let lsn_watermark = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
            off += 8;
            let mut hmac = [0u8; 32];
            hmac.copy_from_slice(&data[off..off + 32]);
            off += 32;
            summaries.push(crate::summary::SyncSummary {
                table_id,
                segment_count,
                segment_root,
                lsn_watermark,
                hmac,
            });
        }
        Ok(SummaryResponse {
            writer_lsn,
            summaries,
        })
    }
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

impl SegmentRequest {
    /// Encode to binary wire format (little-endian).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.table_id.to_le_bytes());
        buf.extend_from_slice(&self.segment_index.to_le_bytes());
        buf.extend_from_slice(&self.expected_root);
        buf
    }

    /// Decode from binary wire format.
    pub fn decode(data: &[u8]) -> Result<Self, SyncError> {
        if data.len() < 40 {
            return Err(SyncError::BackendNotReady("SegmentRequest too short".into()));
        }
        let table_id = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let segment_index = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let mut expected_root = [0u8; 32];
        expected_root.copy_from_slice(&data[8..40]);
        Ok(SegmentRequest {
            table_id,
            segment_index,
            expected_root,
        })
    }
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

impl SegmentNotFound {
    /// Encode to binary wire format (little-endian).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.table_id.to_le_bytes());
        buf.extend_from_slice(&self.segment_index.to_le_bytes());
        buf.push(self.regenerated as u8);
        buf
    }

    /// Decode from binary wire format.
    pub fn decode(data: &[u8]) -> Result<Self, SyncError> {
        if data.len() < 9 {
            return Err(SyncError::BackendNotReady("SegmentNotFound too short".into()));
        }
        let table_id = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let segment_index = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let regenerated = data[8] != 0;
        Ok(SegmentNotFound {
            table_id,
            segment_index,
            regenerated,
        })
    }
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

    #[test]
    fn wal_tail_chunk_encode_decode_roundtrip() {
        let chunk = WalTailChunk {
            from_lsn: 10,
            to_lsn: 20,
            entries: vec![vec![1, 2, 3], vec![4, 5, 6, 7]],
            is_last: true,
        };
        let encoded = chunk.encode();
        let decoded = WalTailChunk::decode(&encoded).unwrap();
        assert_eq!(chunk, decoded);
    }

    #[test]
    fn wal_tail_chunk_encode_decode_empty_entries() {
        let chunk = WalTailChunk {
            from_lsn: 0,
            to_lsn: 0,
            entries: vec![],
            is_last: false,
        };
        let encoded = chunk.encode();
        let decoded = WalTailChunk::decode(&encoded).unwrap();
        assert_eq!(chunk, decoded);
    }

    #[test]
    fn wal_tail_chunk_decode_too_short() {
        let err = WalTailChunk::decode(&[0u8; 5]).unwrap_err();
        assert!(matches!(err, SyncError::BackendNotReady(_)));
    }

    #[test]
    fn wal_tail_chunk_decode_truncated_entry() {
        let mut data = vec![0u8; 21]; // header only
        data[20] = 1; // is_last
                      // count = 0 (from bytes 17-20)
                      // Add count=1 but no entry data
        data.extend_from_slice(&1u32.to_le_bytes()); // count = 1
        let err = WalTailChunk::decode(&data).unwrap_err();
        assert!(matches!(err, SyncError::BackendNotReady(_)));
    }

    #[test]
    fn summary_response_encode_decode_roundtrip() {
        let response = SummaryResponse {
            writer_lsn: 42,
            summaries: vec![crate::summary::SyncSummary {
                table_id: 1,
                segment_count: 3,
                segment_root: [0xAAu8; 32],
                lsn_watermark: 40,
                hmac: [0xBBu8; 32],
            }],
        };
        let encoded = response.encode();
        let decoded = SummaryResponse::decode(&encoded).unwrap();
        assert_eq!(response.writer_lsn, decoded.writer_lsn);
        assert_eq!(response.summaries.len(), decoded.summaries.len());
        assert_eq!(response.summaries[0], decoded.summaries[0]);
    }

    #[test]
    fn summary_response_encode_decode_empty() {
        let response = SummaryResponse {
            writer_lsn: 0,
            summaries: vec![],
        };
        let encoded = response.encode();
        let decoded = SummaryResponse::decode(&encoded).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn segment_request_encode_decode_roundtrip() {
        let req = SegmentRequest {
            table_id: 42,
            segment_index: 7,
            expected_root: [0xCCu8; 32],
        };
        let encoded = req.encode();
        let decoded = SegmentRequest::decode(&encoded).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn segment_request_decode_too_short() {
        assert!(SegmentRequest::decode(&[0u8; 10]).is_err());
    }

    #[test]
    fn segment_not_found_encode_decode_roundtrip() {
        let snf = SegmentNotFound {
            table_id: 99,
            segment_index: 3,
            regenerated: true,
        };
        let encoded = snf.encode();
        let decoded = SegmentNotFound::decode(&encoded).unwrap();
        assert_eq!(snf, decoded);
    }

    #[test]
    fn segment_not_found_decode_too_short() {
        assert!(SegmentNotFound::decode(&[0u8; 5]).is_err());
    }
}
