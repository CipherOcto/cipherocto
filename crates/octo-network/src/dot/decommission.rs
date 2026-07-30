//! Transport Group Decommission envelopes & audit log — RFC-0850p-f
//!
//! Implements the 2 new envelope types and the local audit log:
//!
//! - `UnbindAllDoneEnvelope` (subtype `b"UADN"`) — emitted when all
//!   members have left the platform; the group is fully decommissioned
//! - `UnbindAllAuditEnvelope` (subtype `b"UAAU"`) — signed audit entry
//!   recording who initiated, when, why, witness count, `group_jid`
//!
//! The basic `UnbindAllEnvelope` (subtype `b"UALL"`) and
//! `UnbindAllAckEnvelope` (subtype `b"UAAC"`) are defined in
//! `super::dc_envelopes` (RFC-0850p-d §F) and re-used here.
//!
//! See mission `missions/claimed/0850p-f-group-decommission.md` for the
//! full requirements. The mission is "preliminary" — most of the
//! advanced scenarios (DC rotation, platform-side leave race, quorum
//! semantics) are pending RFC-0850p-f elaboration.

use std::collections::BTreeMap;

use blake3;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::binding::header;
use super::dc_envelopes::UnbindReason;
use super::error::DotError;

/// 4-byte ASCII subtype tags for 0850p-f envelopes.
pub mod tag {
    /// `UnbindAllDoneEnvelope` — all members have left; group is
    /// decommissioned.
    pub const UNBIND_ALL_DONE: [u8; 4] = *b"UADN";
    /// `UnbindAllAuditEnvelope` — signed audit entry.
    pub const UNBIND_ALL_AUDIT: [u8; 4] = *b"UAAU";
}

// -----------------------------------------------------------------------------
// UnbindAllDoneEnvelope
// -----------------------------------------------------------------------------

/// `UnbindAllDoneEnvelope` — emitted when all members have left the
/// platform; the group is fully decommissioned.
///
/// R17 R1-HIGH-1 fix: added `nonce: [u8; 32]` field (was missing entirely).
/// Without a nonce, an attacker could replay the same decommission envelope
/// to forge multiple decommission events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbindAllDoneEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// The `unbind_hash` of the original `UnbindAllEnvelope` (correlation).
    pub unbind_hash: [u8; 32],
    /// Number of member ACKs collected.
    pub ack_count: u32,
    /// Epoch at which decommission completed.
    pub completed_at_epoch: u64,
    /// 32-byte random nonce (R17 R1-HIGH-1 fix: added for replay protection).
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub done_hash: [u8; 32],
    /// Ed25519 signature over `done_hash`.
    pub signature: [u8; 64],
}

impl UnbindAllDoneEnvelope {
    /// Compute `done_hash`.
    pub fn compute_done_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&header(tag::UNBIND_ALL_DONE));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.unbind_hash);
        buf.extend_from_slice(&self.ack_count.to_be_bytes());
        buf.extend_from_slice(&self.completed_at_epoch.to_be_bytes());
        // R17 R1-HIGH-1 fix: nonce is INCLUDED so swapping or stripping the
        // nonce changes the hash and breaks the signature.
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.done_hash = self.compute_done_hash();
        self.signature = key.sign(&self.done_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_done_hash();
        if computed != self.done_hash {
            return Err(DotError::Serialization(
                "UnbindAllDoneEnvelope: done_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.done_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.done_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// UnbindAllAuditEnvelope
// -----------------------------------------------------------------------------

/// `UnbindAllAuditEnvelope` — signed audit entry recording the full
/// decommission event.
///
/// R17 R1-HIGH-1 fix: added `nonce: [u8; 32]` field (was missing entirely).
/// Without a nonce, an attacker could replay the same audit envelope to
/// forge audit log entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbindAllAuditEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Public key of the initiator (the DC that emitted the
    /// `UnbindAllEnvelope`).
    pub initiator_id: [u8; 32],
    /// Reason for the decommission.
    pub reason: UnbindReason,
    /// Free-form reason text (UTF-8).
    pub reason_text: String,
    /// Epoch at which the UNBIND_ALL was issued.
    pub initiated_at_epoch: u64,
    /// Epoch at which the decommission completed (or aborted).
    pub completed_at_epoch: u64,
    /// Number of member ACKs collected.
    pub witness_count: u32,
    /// The `unbind_hash` of the original `UnbindAllEnvelope`.
    pub unbind_hash: [u8; 32],
    /// 32-byte random nonce (R17 R1-HIGH-1 fix: added for replay protection).
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub audit_hash: [u8; 32],
    /// Ed25519 signature over `audit_hash`.
    pub signature: [u8; 64],
}

impl UnbindAllAuditEnvelope {
    /// Compute `audit_hash`.
    pub fn compute_audit_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&header(tag::UNBIND_ALL_AUDIT));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.initiator_id);
        buf.push(self.reason.as_byte());
        write_string(&mut buf, &self.reason_text);
        buf.extend_from_slice(&self.initiated_at_epoch.to_be_bytes());
        buf.extend_from_slice(&self.completed_at_epoch.to_be_bytes());
        buf.extend_from_slice(&self.witness_count.to_be_bytes());
        buf.extend_from_slice(&self.unbind_hash);
        // R17 R1-HIGH-1 fix: nonce is INCLUDED so swapping or stripping the
        // nonce changes the hash and breaks the signature.
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.audit_hash = self.compute_audit_hash();
        self.signature = key.sign(&self.audit_hash).to_bytes();
    }

    /// Verify against the initiator's public key.
    pub fn verify(&self, initiator_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_audit_hash();
        if computed != self.audit_hash {
            return Err(DotError::Serialization(
                "UnbindAllAuditEnvelope: audit_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        initiator_pubkey
            .verify(&self.audit_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.audit_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Local audit log
// -----------------------------------------------------------------------------

/// A single audit log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// Wall-clock timestamp (seconds since UNIX epoch).
    pub timestamp_secs: u64,
    /// The audit envelope.
    pub envelope: UnbindAllAuditEnvelope,
}

/// Local audit log per node.
///
/// Persists a chronological record of all decommission events. The
/// default size is 1024 entries; the log is bounded to keep memory
/// usage predictable. Older entries are evicted on overflow (FIFO).
///
/// When constructed via `open_with_ndjson_store`, an
/// `AuditLogStore` is attached: every `append` writes through to disk
/// (RFC-0850p-f v0.3 §F-4).
#[derive(Debug)]
pub struct AuditLog {
    entries: BTreeMap<u64, AuditEntry>,
    max_entries: usize,
    next_seq: u64,
    store: Option<Box<dyn super::audit_store::AuditLogStore>>,
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl AuditLog {
    /// Create a new audit log with the given maximum number of entries.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            max_entries,
            next_seq: 0,
            store: None,
        }
    }

    /// Attach a persistent audit log store. When set, every `append`
    /// call writes through to `store` after the in-memory update.
    /// Sequence numbers in the store must be contiguous with
    /// `next_seq`; callers opening an existing store should seed
    /// `next_seq` via `with_store_rehydrated`.
    pub fn with_store(mut self, store: Box<dyn super::audit_store::AuditLogStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Open an audit log with a fresh `NdjsonAuditLogStore` rooted at
    /// `dir`, rehydrating the in-memory BTreeMap from any existing
    /// segments. Sequence numbers are seeded to `max_persisted_seq + 1`.
    pub fn open_with_ndjson_store(
        max_entries: usize,
        dir: impl Into<std::path::PathBuf>,
    ) -> Result<Self, super::audit_store::AuditStoreError> {
        use super::audit_store::NdjsonAuditLogStore;
        let store = Box::new(NdjsonAuditLogStore::open(dir)?);
        let mut log = Self::new(max_entries).with_store(store);
        // Rehydrate entries from disk.
        let rehydrated = log
            .store
            .as_ref()
            .unwrap()
            .read_range(0, u64::MAX)
            .unwrap_or_default();
        for (seq, entry) in &rehydrated {
            log.entries.insert(*seq, entry.clone());
        }
        let max_seq = rehydrated.last().map(|(s, _)| *s);
        log.next_seq = max_seq.map(|s| s + 1).unwrap_or(0);
        Ok(log)
    }

    /// Append an entry to the log. Returns the sequence number assigned.
    /// If the log is full, the oldest entry is evicted.
    ///
    /// R17 R1-LOW-4 fix: the timestamp is now taken from the caller
    /// (via `timestamp_secs`) instead of `SystemTime::now()`. This
    /// makes the function deterministic and testable. Production
    /// callers should pass `SystemTime::now()...as_secs()` (or the
    /// wall-clock from their clock-source-of-record).
    pub fn append(&mut self, envelope: UnbindAllAuditEnvelope, timestamp_secs: u64) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let entry = AuditEntry {
            timestamp_secs,
            envelope,
        };
        self.entries.insert(seq, entry.clone());
        // Evict oldest entries to keep size under max_entries
        while self.entries.len() > self.max_entries {
            let oldest_key = *self.entries.keys().next().expect("non-empty");
            self.entries.remove(&oldest_key);
        }
        // Write through to the persistent store if one is attached.
        if let Some(store) = self.store.as_mut() {
            // Persistence failures are non-fatal — log them but keep
            // the in-memory entry. Production callers should monitor
            // the error via a tracing event (TODO: wire tracing here).
            let _ = store.append(seq, &entry);
        }
        seq
    }

    /// Number of entries in the log.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get an entry by sequence number.
    pub fn get(&self, seq: u64) -> Option<&AuditEntry> {
        self.entries.get(&seq)
    }

    /// Iterate over all entries in chronological (sequence) order.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &AuditEntry)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// Look up entries by `domain_id`. Returns an iterator of
    /// `(seq, entry)` pairs.
    pub fn find_by_domain<'a>(
        &'a self,
        domain_id: &'a [u8; 32],
    ) -> impl Iterator<Item = (u64, &'a AuditEntry)> + 'a {
        self.entries
            .iter()
            .filter(move |(_, e)| &e.envelope.domain_id == domain_id)
            .map(|(k, v)| (*k, v))
    }
}

// -----------------------------------------------------------------------------
// UnbindAllAckCollector — RFC-0850p-f v0.3 §F-1 (Quorum Semantics)
// -----------------------------------------------------------------------------

/// Minimum number of distinct witness ACKs required to advance from
/// `UnboundAllPending` to `UnboundAllDone`. See RFC-0850p-f v0.3 §F-1 for
/// the trade-off table (1 ACK vs N-of-M). 1 is chosen: decommission is a
/// defensive action; false-negatives (no decommission when needed) are
/// more harmful than false-positives (decommission a healthy group,
/// recoverable via REBIND).
pub const UNBIND_ALL_MIN_ACKS: u32 = 1;

/// Collects witness ACKs against an in-flight `UnbindAllEnvelope`.
///
/// `witness_id`s are deduplicated — a witness who re-ACKs (e.g. after
/// receiving the rebroadcast for a late joiner per RFC-0850p-f v0.3 §F-3)
/// is counted once. Quorum reached when `unique_witness_count >=
/// UNBIND_ALL_MIN_ACKS`.
#[derive(Debug, Default, Clone)]
pub struct UnbindAllAckCollector {
    /// Unique `witness_id`s that have ACK'd this `unbind_hash`.
    witnesses: BTreeMap<[u8; 32], u64>,
    /// Total ACKs received (including duplicates); useful for metrics.
    total_acks: u64,
}

impl UnbindAllAckCollector {
    /// Create a fresh collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a witness ACK. Returns `true` if quorum was just reached
    /// (i.e. this ACK crossed the `UNBIND_ALL_MIN_ACKS` threshold).
    pub fn record_ack(&mut self, witness_id: [u8; 32]) -> bool {
        self.total_acks += 1;
        let was_new = self.witnesses.insert(witness_id, self.total_acks).is_none();
        let count = self.witnesses.len() as u32;
        was_new && count >= UNBIND_ALL_MIN_ACKS
    }

    /// Number of distinct witnesses who have ACK'd.
    pub fn unique_witness_count(&self) -> u32 {
        self.witnesses.len() as u32
    }

    /// Total ACKs received (including duplicates).
    pub fn total_ack_count(&self) -> u64 {
        self.total_acks
    }

    /// `true` if the quorum threshold has been reached.
    pub fn is_quorum_reached(&self) -> bool {
        self.unique_witness_count() >= UNBIND_ALL_MIN_ACKS
    }
}

// -----------------------------------------------------------------------------
// Serialization helpers
// -----------------------------------------------------------------------------

/// Write a length-prefixed string (u32 BE length, then UTF-8 bytes).
fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dc_key() -> SigningKey {
        SigningKey::from_bytes(&[1u8; 32])
    }

    #[test]
    fn unbind_all_done_sign_verify() {
        let key = test_dc_key();
        let mut env = UnbindAllDoneEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            unbind_hash: [2u8; 32],
            ack_count: 5,
            completed_at_epoch: 100,
            nonce: [3u8; 32],
            done_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn unbind_all_done_mutation_rejected() {
        let key = test_dc_key();
        let mut env = UnbindAllDoneEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            unbind_hash: [2u8; 32],
            ack_count: 5,
            completed_at_epoch: 100,
            nonce: [3u8; 32],
            done_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        env.ack_count = 10;
        assert!(env.verify(&key.verifying_key()).is_err());
    }

    #[test]
    fn unbind_all_audit_sign_verify() {
        let key = test_dc_key();
        let mut env = UnbindAllAuditEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            initiator_id: key.verifying_key().to_bytes(),
            reason: UnbindReason::Scheduled,
            reason_text: "scheduled decommission".into(),
            initiated_at_epoch: 100,
            completed_at_epoch: 150,
            witness_count: 5,
            unbind_hash: [2u8; 32],
            nonce: [3u8; 32],
            audit_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn audit_log_append_and_get() {
        let key = test_dc_key();
        let mut log = AuditLog::new(10);
        assert!(log.is_empty());
        for i in 0..3 {
            let env = UnbindAllAuditEnvelope {
                domain_id: [i as u8; 32],
                group_jid: format!("g{}@g.us", i),
                platform: "whatsapp".into(),
                initiator_id: key.verifying_key().to_bytes(),
                reason: UnbindReason::Scheduled,
                reason_text: format!("entry {}", i),
                initiated_at_epoch: 100 + i,
                completed_at_epoch: 150 + i,
                witness_count: 1,
                unbind_hash: [0u8; 32],
                nonce: [i as u8; 32],
                audit_hash: [0u8; 32],
                signature: [0u8; 64],
            };
            log.append(env, 1_700_000_000);
        }
        assert_eq!(log.len(), 3);
        assert!(log.get(0).is_some());
        assert!(log.get(1).is_some());
        assert!(log.get(2).is_some());
        assert!(log.get(3).is_none());
    }

    #[test]
    fn audit_log_evicts_oldest() {
        let key = test_dc_key();
        let mut log = AuditLog::new(2);
        for i in 0..5 {
            let env = UnbindAllAuditEnvelope {
                domain_id: [i as u8; 32],
                group_jid: format!("g{}", i),
                platform: "whatsapp".into(),
                initiator_id: key.verifying_key().to_bytes(),
                reason: UnbindReason::Scheduled,
                reason_text: String::new(),
                initiated_at_epoch: i,
                completed_at_epoch: i,
                witness_count: 0,
                unbind_hash: [0u8; 32],
                nonce: [i as u8; 32],
                audit_hash: [0u8; 32],
                signature: [0u8; 64],
            };
            log.append(env, 1_700_000_000);
        }
        // Should have only the last 2 entries (sequences 3 and 4)
        assert_eq!(log.len(), 2);
        assert!(log.get(0).is_none());
        assert!(log.get(1).is_none());
        assert!(log.get(2).is_none());
        assert!(log.get(3).is_some());
        assert!(log.get(4).is_some());
    }

    #[test]
    fn audit_log_records_caller_supplied_timestamp() {
        // R17 R1-LOW-4 regression: AuditLog::append must record the
        // timestamp the caller passed in, not a wall-clock value
        // (which would make the test non-deterministic).
        let key = test_dc_key();
        let mut log = AuditLog::new(10);
        let env = UnbindAllAuditEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            initiator_id: key.verifying_key().to_bytes(),
            reason: UnbindReason::Scheduled,
            reason_text: String::new(),
            initiated_at_epoch: 100,
            completed_at_epoch: 150,
            witness_count: 1,
            unbind_hash: [0u8; 32],
            nonce: [0u8; 32],
            audit_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        log.append(env, 1_234_567_890);
        let entry = log.get(0).expect("entry 0 must exist");
        assert_eq!(entry.timestamp_secs, 1_234_567_890);
    }

    #[test]
    fn audit_log_find_by_domain() {
        let key = test_dc_key();
        let mut log = AuditLog::new(10);
        for i in 0..3 {
            let env = UnbindAllAuditEnvelope {
                domain_id: [1u8; 32], // all same domain
                group_jid: format!("g{}", i),
                platform: "whatsapp".into(),
                initiator_id: key.verifying_key().to_bytes(),
                reason: UnbindReason::Scheduled,
                reason_text: String::new(),
                initiated_at_epoch: i,
                completed_at_epoch: i,
                witness_count: 0,
                unbind_hash: [0u8; 32],
                nonce: [i as u8; 32],
                audit_hash: [0u8; 32],
                signature: [0u8; 64],
            };
            log.append(env, 1_700_000_000);
        }
        // Different domain
        let env = UnbindAllAuditEnvelope {
            domain_id: [2u8; 32],
            group_jid: "other".into(),
            platform: "whatsapp".into(),
            initiator_id: key.verifying_key().to_bytes(),
            reason: UnbindReason::Scheduled,
            reason_text: String::new(),
            initiated_at_epoch: 0,
            completed_at_epoch: 0,
            witness_count: 0,
            unbind_hash: [0u8; 32],
            nonce: [9u8; 32],
            audit_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        log.append(env, 1_700_000_000);
        let found: Vec<_> = log.find_by_domain(&[1u8; 32]).collect();
        assert_eq!(found.len(), 3);
    }

    // R17 R1-HIGH-1 regression test: changing the nonce must change the
    // hash (so an attacker cannot swap a stored envelope's nonce to bypass
    // replay protection).
    #[test]
    fn unbind_all_done_nonce_changes_hash() {
        let key = test_dc_key();
        let mut env = UnbindAllDoneEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            unbind_hash: [2u8; 32],
            ack_count: 5,
            completed_at_epoch: 100,
            nonce: [3u8; 32],
            done_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        let original_hash = env.done_hash;
        // Swap the nonce; verify must fail because the hash changed.
        env.nonce = [4u8; 32];
        env.sign(&key);
        assert_ne!(env.done_hash, original_hash);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn unbind_all_audit_nonce_changes_hash() {
        let key = test_dc_key();
        let mut env = UnbindAllAuditEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            initiator_id: key.verifying_key().to_bytes(),
            reason: UnbindReason::Scheduled,
            reason_text: "scheduled decommission".into(),
            initiated_at_epoch: 100,
            completed_at_epoch: 150,
            witness_count: 5,
            unbind_hash: [2u8; 32],
            nonce: [3u8; 32],
            audit_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        let original_hash = env.audit_hash;
        env.nonce = [4u8; 32];
        env.sign(&key);
        assert_ne!(env.audit_hash, original_hash);
    }

    #[test]
    fn header_subtypes_distinct() {
        // Verify the 2 new tags are distinct from existing ones.
        let new_tags = [tag::UNBIND_ALL_DONE, tag::UNBIND_ALL_AUDIT];
        let existing = [
            super::super::dc_envelopes::tag::UNBIND_ALL,
            super::super::dc_envelopes::tag::UNBIND_ALL_ACK,
        ];
        for nt in &new_tags {
            for et in &existing {
                assert_ne!(nt, et, "new tag {:?} collides with existing {:?}", nt, et);
            }
        }
        assert_eq!(&tag::UNBIND_ALL_DONE, b"UADN");
        assert_eq!(&tag::UNBIND_ALL_AUDIT, b"UAAU");
    }

    // RFC-0850p-f v0.3 §F-1 (Quorum): 1 distinct witness ACK reaches quorum.
    #[test]
    fn ack_collector_quorum_reached_after_first_ack() {
        let mut c = UnbindAllAckCollector::new();
        assert!(!c.is_quorum_reached());
        assert_eq!(c.unique_witness_count(), 0);
        assert_eq!(c.total_ack_count(), 0);
        // First ACK crosses threshold.
        assert!(c.record_ack([1u8; 32]));
        assert!(c.is_quorum_reached());
        assert_eq!(c.unique_witness_count(), 1);
        assert_eq!(c.total_ack_count(), 1);
    }

    // RFC-0850p-f v0.3 §F-1: duplicate ACKs from same witness are deduped.
    #[test]
    fn ack_collector_dedupes_duplicate_witness() {
        let mut c = UnbindAllAckCollector::new();
        // First ACK crosses threshold (returns true).
        assert!(c.record_ack([1u8; 32]));
        // Same witness re-ACKs after a rebroadcast — must NOT re-trigger.
        assert!(!c.record_ack([1u8; 32]));
        assert_eq!(c.unique_witness_count(), 1);
        assert_eq!(c.total_ack_count(), 2);
        assert!(c.is_quorum_reached());
    }

    // RFC-0850p-f v0.3 §F-4: AuditLog + NdjsonAuditLogStore wiring.
    // Appends write through to disk; reopening rehydrates entries.
    #[test]
    fn audit_log_persists_to_ndjson_and_rehydrates() {
        let dir = std::env::temp_dir().join(format!(
            "audit-log-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        // Open a fresh log with a persistent store.
        let mut log = AuditLog::open_with_ndjson_store(1024, &dir).unwrap();
        let key = SigningKey::from_bytes(&[1u8; 32]);
        let env = UnbindAllAuditEnvelope {
            domain_id: [7u8; 32],
            group_jid: "g@g.us".into(),
            platform: "whatsapp".into(),
            initiator_id: key.verifying_key().to_bytes(),
            reason: UnbindReason::Scheduled,
            reason_text: String::new(),
            initiated_at_epoch: 0,
            completed_at_epoch: 0,
            witness_count: 1,
            unbind_hash: [0u8; 32],
            nonce: [0u8; 32],
            audit_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        let seq0 = log.append(env.clone(), 1_700_000_000);
        let seq1 = log.append(env, 1_700_000_001);
        assert_eq!(seq0, 0);
        assert_eq!(seq1, 1);

        // Reopen: rehydration loads both entries.
        let log2 = AuditLog::open_with_ndjson_store(1024, &dir).unwrap();
        assert_eq!(log2.len(), 2);
        assert_eq!(log2.get(0).unwrap().envelope.domain_id, [7u8; 32]);
        assert_eq!(log2.get(1).unwrap().envelope.domain_id, [7u8; 32]);

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }
}
