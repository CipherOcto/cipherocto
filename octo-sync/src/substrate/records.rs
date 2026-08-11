//! Substrate records + error enums (per RFC-0862 v1.3 §Supporting
//! types + error enums).
//!
//! `PeerIdentity` (bootstrap phase peer discovery), `NonceRecord` (WAL
//! entry type 0x10 payload), `ActualDrained` (drain result), and the
//! error enums covering the writer-election / drain / bootstrap /
//! coordinator surfaces.

use borsh::{BorshDeserialize, BorshSerialize};

use super::ids::ShardKey;
use super::ids::ShardMissionId;
use super::ids::WriterNodeId;

/// Peer identity returned by `BootstrapOrchestrator::acquire_peers`
/// (per RFC-0862 v1.3 §BootstrapOrchestrator trait).
///
/// `node_id` is the writer node id; `overlay_id` is the underlying
/// transport overlay (libp2p / custom) identity for the peer;
/// `mission_id` is the shared mission scope.
pub struct PeerIdentity {
    /// Writer node id.
    pub node_id: WriterNodeId,
    /// Overlay-identity (transport-layer) for the peer.
    pub overlay_id: [u8; 32],
    /// Mission shared with the peer.
    pub mission_id: ShardMissionId,
}

/// Nonce record (per RFC-0862 v1.3 §Substrate types).
///
/// Stored in WAL as entry type `ENTRY_TYPE_NONCE_RECORD` (0x10). The
/// `NonceTracker` replays these on init and subsequently uses the
/// `(term, nonce)` tuple set for replay-resistance (per RFC-0862 v1.3
/// R11 M3 + R13 M3).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct NonceRecord {
    /// Shard key the nonce is bound to.
    pub shard_key: ShardKey,
    /// Election term in which the nonce was issued.
    pub term: u64,
    /// 32-byte nonce.
    pub nonce: [u8; 32],
}

/// Result of a successful drain (per RFC-0862 v1.3 §Supporting types).
///
/// `receipt_lsn` is the WAL LSN at which the drain entry was appended;
/// callers use this for cross-instance proof-of-payment replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActualDrained {
    /// Holder DID whose balance was drained.
    pub holder_did: String,
    /// 32-byte macaroon identifier (typically the LSN-derived key).
    pub macaroon_id: Vec<u8>,
    /// Drained amount in base units.
    pub drained_amount: u128,
    /// WAL LSN at which the drain entry was appended.
    pub receipt_lsn: u64,
}

/// Writer-election errors (per RFC-0862 v1.3 §Supporting types + error enums).
#[derive(Debug, thiserror::Error)]
pub enum WriterElectionError {
    /// WAL corruption detected (gap, non-monotonic LSN, checksum mismatch).
    #[error("WAL corruption detected")]
    WalCorruption,
    /// WAL magic indicates a newer version than this reader supports.
    #[error("WAL version too new for this reader")]
    WalVersionTooNew,
    /// Replay attempt: nonce already used in this `(term, nonce)` space.
    #[error("nonce already used (replay)")]
    NonceReplayed,
    /// `shard_key` mismatch between attestation and writer state.
    #[error("shard_key mismatch")]
    ShardKeyMismatch,
    /// `chain_id` mismatch (deployment-binding per R12 M23).
    #[error("chain_id mismatch (deployment-binding)")]
    ChainIdMismatch,
    /// Threshold mismatch between attestation and configured operator set.
    #[error("threshold mismatch")]
    ThresholdMismatch,
    /// Signature count exceeds the bound (per R12 M23: DoS cap).
    #[error("too many signatures: count={count}, max={max}")]
    TooManySignatures {
        /// Actual count.
        count: usize,
        /// Cap.
        max: usize,
    },
    /// Duplicate signer (per R12 M23: each operator signs at most once).
    #[error("duplicate signer")]
    DuplicateSigner,
    /// Signer not in the configured operator set.
    #[error("unauthorized signer")]
    UnauthorizedSigner,
    /// ed25519 signature verification failed.
    #[error("invalid signature")]
    InvalidSignature,
    /// Below-threshold valid signatures.
    #[error("insufficient signatures")]
    InsufficientSignatures,
    /// Lease TTL expired before relinquish / heartbeat.
    #[error("lease expired")]
    LeaseExpired,
    /// `relinquish_writer` already issued but not yet acknowledged.
    #[error("relinquish already pending")]
    RelinquishPending,
}

/// DID write coordinator errors (per RFC-0862 v1.3 §Supporting types).
#[derive(Debug, thiserror::Error)]
pub enum DidWriteCoordinatorError {
    /// No elected writer available for the shard.
    #[error("writer unavailable")]
    WriterUnavailable,
    /// Caller-supplied `canonical_did_hash` does not match the
    /// `canonical_hash(document)`.
    #[error("hash/document mismatch")]
    HashDocumentMismatch,
    /// `chain_id` does not match the configured deployment.
    #[error("chain_id mismatch")]
    ChainIdMismatch,
    /// WAL corruption detected at the coordinator layer.
    #[error("WAL corruption detected")]
    WalCorruption,
}

/// Drain coordinator errors (per RFC-0862 v1.3 §Supporting types).
#[derive(Debug, thiserror::Error)]
pub enum DrainCoordinatorError {
    /// No elected writer available for the shard.
    #[error("writer unavailable")]
    WriterUnavailable,
    /// Holder DID not found in the balance table.
    #[error("unknown holder did")]
    UnknownHolder,
    /// Holder balance below the requested cost.
    #[error("insufficient balance")]
    InsufficientBalance,
}

/// Bootstrap errors (per RFC-0862 v1.3 §Supporting types).
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    /// Peer acquisition timed out.
    #[error("peer acquisition timed out after {0} ms")]
    Timeout(u64),
    /// No peers discovered.
    #[error("no peers discovered")]
    NoPeers,
    /// Overlay identity verification failed (TLS / Noise handshake).
    #[error("overlay identity verification failed")]
    OverlayIdentityVerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_record_borsh_round_trip() {
        let rec = NonceRecord {
            shard_key: ShardKey([7u8; 32]),
            term: 42,
            nonce: [9u8; 32],
        };
        let bytes = borsh::to_vec(&rec).unwrap();
        let decoded: NonceRecord = NonceRecord::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.shard_key, rec.shard_key);
        assert_eq!(decoded.term, rec.term);
        assert_eq!(decoded.nonce, rec.nonce);
    }

    #[test]
    fn actual_drained_equality() {
        let a = ActualDrained {
            holder_did: "did:oct:abc".to_string(),
            macaroon_id: vec![1, 2, 3],
            drained_amount: 1000,
            receipt_lsn: 7,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn writer_election_error_messages() {
        let e = WriterElectionError::TooManySignatures {
            count: 100,
            max: 32,
        };
        let msg = format!("{e}");
        assert!(msg.contains("100"));
        assert!(msg.contains("32"));
    }
}
