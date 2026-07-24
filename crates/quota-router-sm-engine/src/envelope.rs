//! ExecutionEnvelope object protocol (RFC-0962 §4 + RFC-0960 §10).
//!
//! Signed, hash-committed, replayable database transaction object. An
//! `ExecutionEnvelope` bundles an ordered set of SQL statements (compiled
//! to WAL entries by the deterministic SQL engine per RFC-0961) into a
//! single signed object the network certifies.
//!
//! ## Caps
//!
//! - 1000 statements per single envelope (RFC-0962 §4).
//! - 1 MB total envelope size (RFC-0962 §4).
//! - MultiEnvelope ladder triggered above the cap (RFC-0962 §7).
//!
//! ## Signature
//!
//! Capability-holder signs over `canonical_ser(envelope_unsigned)`. The
//! 32-byte nonce + `block_height` provide replay protection (RFC-0962 §6.2).

#![warn(missing_debug_implementations)]

use serde::{Deserialize, Serialize};

/// Maximum statements per envelope (RFC-0962 §4).
pub const MAX_STATEMENTS: usize = 1000;
/// Maximum envelope size in bytes (RFC-0962 §4).
pub const MAX_ENVELOPE_BYTES: usize = 1_048_576; // 1 MB
/// Maximum MultiEnvelope nesting depth (RFC-0962 §7 R8-F5).
pub const MAX_NESTING_DEPTH: u8 = 4;
/// Nonce size (RFC-0962 §4 R4-F3 — 256-bit collision resistance).
pub const NONCE_SIZE: usize = 32;
/// Version tag (RFC-0962 §4 — v2 = ExecutionEnvelope rename + DETERMINISTIC mode).
pub const VERSION_TAG: u8 = 2;
/// Domain separator for `sql_statements_hash` (RFC-0962 §9 R6-F6).
pub const SQL_STATEMENTS_HASH_PREFIX: u8 = 0xA3;

/// ExecutionEnvelope (RFC-0962 §4).
///
/// Wire-format ordering: `version_tag, session_id, capability, capability_holder,
/// sql_statements, stored_procs, ddl_changes, wal_segment_hash, block_height,
/// nonce, mode, timestamp, signature`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEnvelope {
    /// Version tag (currently 2).
    pub version: u8,
    /// Session identifier (32-byte content-addressed).
    pub session_id: [u8; 32],
    /// Capability bound (32-byte macaroon root_id).
    pub capability: [u8; 32],
    /// Capability holder DID (RFC-0009). Distinct from `capability`: the
    /// capability id identifies the macaroon; the holder is the signer.
    pub capability_holder: String,
    /// Ordered SQL statements (canonical_text form).
    pub sql_statements: Vec<String>,
    /// Stored procedure invocations.
    pub stored_procs: Vec<ProcInvocation>,
    /// DDL operations.
    pub ddl_changes: Vec<DdlOperation>,
    /// Hash of the WAL segment this envelope appends to (RFC-0862).
    pub wal_segment_hash: [u8; 32],
    /// Block height at which envelope was signed.
    pub block_height: u64,
    /// 256-bit nonce (replay defense).
    pub nonce: [u8; NONCE_SIZE],
    /// Session mode (RFC-0962 §4 — DETERMINISTIC | OFF_CHAIN | AUDIT_ONLY).
    pub mode: EnvelopeMode,
    /// Unix timestamp at sign time.
    pub timestamp: u64,
    /// Ed25519 signature over `canonical_ser(envelope_unsigned)`.
    #[serde(with = "ed25519_sig_serde")]
    pub signature: ed25519_dalek::Signature,
}

/// ExecutionEnvelope session mode (RFC-0962 §4).
///
/// Renamed from `CONSENSUS_SAFE` to `DETERMINISTIC` per RFC-0962 v2.0
/// strategic reframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeMode {
    /// Production mutations entering consensus; CIPHERO_SQL enforced.
    Deterministic,
    /// Local-only execution; no consensus impact.
    OffChain,
    /// Read-only sessions that produce audit trail without mutation.
    AuditOnly,
}

/// Stored procedure invocation (RFC-0962 §4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcInvocation {
    pub proc_name: String,
    pub args: Vec<String>,
}

/// DDL operation (RFC-0962 §4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdlOperation {
    CreateTable { name: String, columns: Vec<String> },
    DropTable { name: String },
    AlterTable { name: String, alteration: String },
}

/// MultiEnvelope (RFC-0962 §7) — chains sub-envelopes when one envelope
/// exceeds the 1000-stmt / 1MB cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiEnvelope {
    pub sub_envelopes: Vec<ExecutionEnvelope>,
    pub completion_rule: CompletionRule,
    pub completion_quorum_n: Option<u32>,
    pub parent_sessions: Vec<[u8; 32]>,
    /// Hard deadline; on expiry, `fallback_action` runs (RFC-0962 §7).
    pub timeout_unix_ms: u64,
    /// Action on timeout / partial commit (RFC-0962 §7).
    pub fallback_action: FallbackAction,
    /// Optional recursive child `MultiEnvelope`. Enforces the R8-F5
    /// nesting cap via [`check_nesting_depth`]. `serde(default)` keeps
    /// wire-format back-compat for v2.0 envelopes that pre-date the
    /// field.
    #[serde(default)]
    pub nested: Option<Box<MultiEnvelope>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionRule {
    AllRequired,
    Quorum,
    AnyOne,
}

/// Fallback action when a `MultiEnvelope` does not complete within
/// `timeout_unix_ms` (RFC-0962 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackAction {
    /// Roll back every committed sub-envelope. Default.
    RollbackAll,
    /// Commit whatever sub-envelopes reached Replayed; abort the rest.
    CommitPartial,
    /// Abort everything; no partial commit.
    Abort,
}

/// Envelope errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvelopeError {
    #[error("statement count {0} exceeds cap {MAX_STATEMENTS}")]
    TooManyStatements(usize),
    #[error("envelope size {0} bytes exceeds cap {MAX_ENVELOPE_BYTES}")]
    TooLarge(usize),
    #[error("nested MultiEnvelope depth {0} exceeds {MAX_NESTING_DEPTH} (R8-F5)")]
    NestingDepthExceeded(u8),
    #[error("envelope signature invalid")]
    SignatureInvalid,
    #[error("envelope replay detected: nonce {0:?}")]
    ReplayDetected([u8; NONCE_SIZE]),
    #[error("envelope not yet applicable: block_height {block_height} exceeds local head")]
    FutureBlock { block_height: u64 },
    #[error("storage error: {0}")]
    Storage(String),
    /// RFC-0962 §6.4: tentative→final `envelope_id` mismatch.
    #[error("WAL hash mismatch: envelope_id {envelope_id:?} ≠ final_id {final_id:?}")]
    WalHashMismatch {
        envelope_id: [u8; 32],
        final_id: [u8; 32],
    },
    /// RFC-0962 §6.4: `wal_segment_hash_final` not present in local WAL chain.
    #[error("WAL segment missing: {0:?}")]
    WalSegmentMissing([u8; 32]),
    /// RFC-0962 §6.4: referenced segment committed after validator's height.
    #[error("WAL out of order: segment_height {segment_height} > local_height {local_height}")]
    WalOutOfOrder {
        segment_height: u64,
        local_height: u64,
    },
}

impl From<StorageError> for EnvelopeError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e.to_string())
    }
}

use crate::StorageError;

/// Build an `ExecutionEnvelope` (RFC-0962 §4).
///
/// Enforces the 1000-stmt + 1MB caps. Caller provides the signature
/// (which depends on the holder's signing key). `capability` is the
/// macaroon root_id; `block_height` is the local chain head at sign time.
#[allow(clippy::too_many_arguments)]
pub fn build_envelope(
    session_id: [u8; 32],
    capability: [u8; 32],
    capability_holder: String,
    sql_statements: Vec<String>,
    stored_procs: Vec<ProcInvocation>,
    ddl_changes: Vec<DdlOperation>,
    wal_segment_hash: [u8; 32],
    block_height: u64,
    nonce: [u8; NONCE_SIZE],
    mode: EnvelopeMode,
    timestamp: u64,
    signature: ed25519_dalek::Signature,
) -> Result<ExecutionEnvelope, EnvelopeError> {
    if sql_statements.len() > MAX_STATEMENTS {
        return Err(EnvelopeError::TooManyStatements(sql_statements.len()));
    }

    let envelope = ExecutionEnvelope {
        version: VERSION_TAG,
        session_id,
        capability,
        capability_holder,
        sql_statements,
        stored_procs,
        ddl_changes,
        wal_segment_hash,
        block_height,
        nonce,
        mode,
        timestamp,
        signature,
    };

    let bytes = serialize_envelope(&envelope)?;
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(EnvelopeError::TooLarge(bytes.len()));
    }
    Ok(envelope)
}

/// Canonical serialization of the unsigned envelope (everything except
/// the signature). Used for both sign + verify inputs.
#[must_use]
pub fn unsigned_canonical_ser(envelope: &ExecutionEnvelope) -> Vec<u8> {
    let clone = ExecutionEnvelope {
        signature: ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        ..envelope.clone()
    };
    let mut buf = vec![clone.version];
    buf.extend_from_slice(&clone.session_id);
    buf.extend_from_slice(&clone.capability);
    // capability_holder (DID) — length-prefixed UTF-8.
    let holder_bytes = clone.capability_holder.as_bytes();
    buf.extend_from_slice(&(holder_bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(holder_bytes);
    let stmts_json = serde_json::to_string(&clone.sql_statements).expect("serializable");
    buf.extend_from_slice(&(stmts_json.len() as u32).to_be_bytes());
    buf.extend_from_slice(stmts_json.as_bytes());
    buf.extend_from_slice(&clone.wal_segment_hash);
    buf.extend_from_slice(&clone.block_height.to_be_bytes());
    buf.extend_from_slice(&clone.nonce);
    buf.push(clone.mode as u8);
    buf.extend_from_slice(&clone.timestamp.to_be_bytes());
    buf
}

/// Serialize the full envelope to bytes (for size check).
pub fn serialize_envelope(envelope: &ExecutionEnvelope) -> Result<Vec<u8>, EnvelopeError> {
    let mut buf = unsigned_canonical_ser(envelope);
    buf.extend_from_slice(&envelope.signature.to_bytes());
    Ok(buf)
}

/// `sql_statements_hash = BLAKE3(0xA3 || canonical_ser(sql_statements))`
/// (RFC-0962 §9 R6-F6).
#[must_use]
pub fn sql_statements_hash(statements: &[String]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[SQL_STATEMENTS_HASH_PREFIX]);
    let json = serde_json::to_string(statements).expect("serializable");
    hasher.update(json.as_bytes());
    *hasher.finalize().as_bytes()
}

// ===== RFC-0962 §6.4 WAL two-phase hash binding =====

/// Tentative `envelope_id` at sign time.
///
/// Per RFC-0962 §6.4 (sign-time): the signer fills `wal_segment_hash`
/// with a deterministic placeholder derived from the SQL operations, so
/// `envelope_id = BLAKE3(0xA3 || canonical_ser(envelope_unsigned_placeholder))`
/// is computable before the WAL segment is committed.
#[must_use]
pub fn tentative_envelope_id(envelope: &ExecutionEnvelope) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[SQL_STATEMENTS_HASH_PREFIX]); // 0xA3
    hasher.update(&unsigned_canonical_ser(envelope));
    *hasher.finalize().as_bytes()
}

/// Final `envelope_id` at commit time.
///
/// Per RFC-0962 §6.4 (commit-time): when the WAL segment containing the
/// envelope's entries is appended, the validator recomputes
/// `envelope_final_id = BLAKE3(0xA3 || canonical_ser(envelope_final))`
/// where `envelope_final.wal_segment_hash = wal_segment_hash_final`.
/// Mismatch → `E_WAL_HASH_MISMATCH`.
#[must_use]
pub fn finalize_envelope_id(envelope: &ExecutionEnvelope) -> [u8; 32] {
    tentative_envelope_id(envelope) // placeholder hash = final hash in this scaffold;
                                    // real impl recomputes with the final wal_segment_hash.
}

/// Final WAL segment hash (RFC-0960 §1.1 + RFC-0962 §10):
/// `BLAKE3(prev_segment_id || canonical_ser(segment_body))`.
#[must_use]
pub fn finalize_wal_hash(prev_segment_id: &[u8; 32], segment_body: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(prev_segment_id);
    hasher.update(segment_body);
    *hasher.finalize().as_bytes()
}

/// Verify WAL hash binding (RFC-0962 §6.4 + §6.2 step 4).
///
/// Caller provides the local-chain lookups so the verifier stays pure
/// (no I/O inside `envelope.rs`).
pub fn verify_wal_hash_binding(
    envelope: &ExecutionEnvelope,
    tentative_id: [u8; 32],
    final_id: [u8; 32],
    segment_present: bool,
    segment_height: u64,
    local_chain_height: u64,
) -> Result<(), EnvelopeError> {
    if tentative_id != final_id {
        return Err(EnvelopeError::WalHashMismatch {
            envelope_id: tentative_id,
            final_id,
        });
    }
    if !segment_present {
        return Err(EnvelopeError::WalSegmentMissing(envelope.wal_segment_hash));
    }
    if segment_height > local_chain_height {
        return Err(EnvelopeError::WalOutOfOrder {
            segment_height,
            local_height: local_chain_height,
        });
    }
    Ok(())
}

/// Verify an envelope's signature against the holder's public key.
pub fn verify_envelope_signature(
    envelope: &ExecutionEnvelope,
    holder_pub: &[u8; 32],
) -> Result<(), EnvelopeError> {
    let vk = ed25519_dalek::VerifyingKey::from_bytes(holder_pub)
        .map_err(|_| EnvelopeError::SignatureInvalid)?;
    let msg = unsigned_canonical_ser(envelope);
    vk.verify_strict(&msg, &envelope.signature)
        .map_err(|_| EnvelopeError::SignatureInvalid)
}

/// Check a `MultiEnvelope` for nesting depth (RFC-0962 §7 R8-F5).
///
/// Recursively walks the `nested` chain. Returns
/// `Err(NestingDepthExceeded(current_depth))` the moment the depth
/// reaches `MAX_NESTING_DEPTH`. `current_depth` is the depth of the
/// caller-supplied `multi` itself; the function increments it by 1
/// before descending into `nested`.
pub fn check_nesting_depth(multi: &MultiEnvelope, current_depth: u8) -> Result<(), EnvelopeError> {
    if current_depth >= MAX_NESTING_DEPTH {
        return Err(EnvelopeError::NestingDepthExceeded(current_depth));
    }
    if let Some(nested) = &multi.nested {
        check_nesting_depth(nested, current_depth + 1)?;
    }
    Ok(())
}

/// Replay-defense check (RFC-0962 §6.2 step 6 + §6.3 composite key).
///
/// Verifies the `(signer_did, nonce)` pair has not been seen in the
/// ConsumedEnvelopeIndex. `signer_did` = `envelope.capability_holder.as_bytes()`
/// (DID utf-8).
pub fn check_replay<S: ReplayIndex>(
    envelope: &ExecutionEnvelope,
    index: &S,
) -> Result<(), EnvelopeError> {
    let signer_did = envelope.capability_holder.as_bytes();
    if index.consumed_contains_for(signer_did, &envelope.nonce) {
        return Err(EnvelopeError::ReplayDetected(envelope.nonce));
    }
    Ok(())
}

/// Replay index abstraction (RFC-0962 §6.3).
///
/// Composite key `(signer_did, nonce)` prevents replay across distinct
/// signers reusing the same nonce.
pub trait ReplayIndex {
    fn consumed_contains_for(&self, signer_did: &[u8], nonce: &[u8; NONCE_SIZE]) -> bool;

    /// Legacy single-key lookup. Default delegates to composite with empty
    /// `signer_did`. New implementations should override `consumed_contains_for`
    /// directly.
    fn consumed_contains(&self, nonce: &[u8; NONCE_SIZE]) -> bool {
        self.consumed_contains_for(&[], nonce)
    }
}

/// Reserve a nonce slot (RFC-0962 §6.2 step 6 + §6.3).
pub fn mark_consumed<S: ReplayIndexMut>(
    envelope: &ExecutionEnvelope,
    index: &mut S,
) -> Result<(), EnvelopeError> {
    let signer_did = envelope.capability_holder.as_bytes().to_vec();
    if index.consumed_contains_for(&signer_did, &envelope.nonce) {
        return Err(EnvelopeError::ReplayDetected(envelope.nonce));
    }
    index.mark_consumed_for(signer_did, envelope.nonce);
    Ok(())
}

pub trait ReplayIndexMut: ReplayIndex {
    fn mark_consumed_for(&mut self, signer_did: Vec<u8>, nonce: [u8; NONCE_SIZE]);

    /// Legacy single-key insert. Default delegates with empty signer_did.
    fn mark_consumed(&mut self, nonce: [u8; NONCE_SIZE]) {
        self.mark_consumed_for(Vec::new(), nonce);
    }
}

/// Build a `MultiEnvelope` from sub-envelopes (RFC-0962 §7).
///
/// Default `timeout_unix_ms` = 5_000 (5 seconds; matches the worked
/// example in RFC-0962 §5 + RFC-0963 §9). Default `fallback_action`
/// = `Abort` (no partial commit).
pub fn build_multi_envelope(
    sub_envelopes: Vec<ExecutionEnvelope>,
    completion_rule: CompletionRule,
    completion_quorum_n: Option<u32>,
    parent_sessions: Vec<[u8; 32]>,
) -> MultiEnvelope {
    MultiEnvelope {
        sub_envelopes,
        completion_rule,
        completion_quorum_n,
        parent_sessions,
        timeout_unix_ms: 5_000,
        fallback_action: FallbackAction::Abort,
        nested: None,
    }
}

/// Builder with explicit timeout + fallback (RFC-0962 §7).
pub fn build_multi_envelope_with(
    sub_envelopes: Vec<ExecutionEnvelope>,
    completion_rule: CompletionRule,
    completion_quorum_n: Option<u32>,
    parent_sessions: Vec<[u8; 32]>,
    timeout_unix_ms: u64,
    fallback_action: FallbackAction,
) -> MultiEnvelope {
    MultiEnvelope {
        sub_envelopes,
        completion_rule,
        completion_quorum_n,
        parent_sessions,
        timeout_unix_ms,
        fallback_action,
        nested: None,
    }
}

// Ed25519 signature serde shim.
mod ed25519_sig_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &Signature, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_bytes(&sig.to_bytes())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Signature, D::Error> {
        let bytes: Vec<u8> = Deserialize::deserialize(de)?;
        Signature::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn make_key() -> SigningKey {
        SigningKey::from_bytes(&[0x42; 32])
    }

    fn sign_envelope(envelope: &mut ExecutionEnvelope, key: &SigningKey) {
        let msg = unsigned_canonical_ser(envelope);
        let sig = key.sign(&msg);
        envelope.signature = sig;
    }

    #[test]
    fn build_envelope_under_cap_succeeds() {
        let key = make_key();
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        sign_envelope(&mut env, &key);
        let bytes = serialize_envelope(&env).unwrap();
        assert!(bytes.len() <= MAX_ENVELOPE_BYTES);
    }

    #[test]
    fn build_envelope_too_many_statements_rejected() {
        let stmts: Vec<String> = (0..MAX_STATEMENTS + 1)
            .map(|i| format!("SELECT {i}"))
            .collect();
        let err = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            stmts,
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap_err();
        assert_eq!(err, EnvelopeError::TooManyStatements(MAX_STATEMENTS + 1));
    }

    #[test]
    fn verify_envelope_signature_accepts_valid_sig() {
        let key = make_key();
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        sign_envelope(&mut env, &key);
        let pub_bytes = key.verifying_key().to_bytes();
        verify_envelope_signature(&env, &pub_bytes).unwrap();
    }

    #[test]
    fn verify_envelope_signature_rejects_tampered() {
        let key = make_key();
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        sign_envelope(&mut env, &key);
        let mut env2 = env.clone();
        env2.block_height = 999; // tamper
        let pub_bytes = key.verifying_key().to_bytes();
        let err = verify_envelope_signature(&env2, &pub_bytes).unwrap_err();
        assert_eq!(err, EnvelopeError::SignatureInvalid);
    }

    #[test]
    fn sql_statements_hash_deterministic() {
        let stmts = vec!["SELECT 1".to_owned(), "SELECT 2".to_owned()];
        let h1 = sql_statements_hash(&stmts);
        let h2 = sql_statements_hash(&stmts);
        assert_eq!(h1, h2);
    }

    #[test]
    fn sql_statements_hash_differs_for_different_statements() {
        let a = sql_statements_hash(&["SELECT 1".to_owned()]);
        let b = sql_statements_hash(&["SELECT 2".to_owned()]);
        assert_ne!(a, b);
    }

    #[test]
    fn check_replay_detects_seen_nonce() {
        struct InMemoryIndex {
            seen: Vec<(Vec<u8>, [u8; NONCE_SIZE])>,
        }
        impl ReplayIndex for InMemoryIndex {
            fn consumed_contains_for(&self, signer_did: &[u8], nonce: &[u8; NONCE_SIZE]) -> bool {
                self.seen
                    .iter()
                    .any(|(d, n)| d.as_slice() == signer_did && n == nonce)
            }
        }
        let nonce = [0xab; NONCE_SIZE];
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            nonce,
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let key = make_key();
        sign_envelope(&mut env, &key);
        let index = InMemoryIndex {
            seen: vec![(b"did:octo:test".to_vec(), nonce)],
        };
        let err = check_replay(&env, &index).unwrap_err();
        assert_eq!(err, EnvelopeError::ReplayDetected(nonce));
    }

    #[test]
    fn check_replay_accepts_unseen_nonce() {
        struct InMemoryIndex;
        impl ReplayIndex for InMemoryIndex {
            fn consumed_contains_for(&self, _signer_did: &[u8], _nonce: &[u8; NONCE_SIZE]) -> bool {
                false
            }
        }
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0xab; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let key = make_key();
        sign_envelope(&mut env, &key);
        check_replay(&env, &InMemoryIndex).unwrap();
    }

    #[test]
    fn mark_consumed_adds_nonce() {
        struct InMemoryIndex {
            seen: Vec<(Vec<u8>, [u8; NONCE_SIZE])>,
        }
        impl ReplayIndex for InMemoryIndex {
            fn consumed_contains_for(&self, signer_did: &[u8], nonce: &[u8; NONCE_SIZE]) -> bool {
                self.seen
                    .iter()
                    .any(|(d, n)| d.as_slice() == signer_did && n == nonce)
            }
        }
        impl ReplayIndexMut for InMemoryIndex {
            fn mark_consumed_for(&mut self, signer_did: Vec<u8>, nonce: [u8; NONCE_SIZE]) {
                self.seen.push((signer_did, nonce));
            }
        }
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0xcd; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let key = make_key();
        sign_envelope(&mut env, &key);
        let mut index = InMemoryIndex { seen: vec![] };
        mark_consumed(&env, &mut index).unwrap();
        let err = check_replay(&env, &index).unwrap_err();
        assert_eq!(err, EnvelopeError::ReplayDetected([0xcd; NONCE_SIZE]));
    }

    #[test]
    fn check_nesting_depth_under_cap_accepts() {
        let multi = build_multi_envelope(vec![], CompletionRule::AllRequired, None, vec![]);
        check_nesting_depth(&multi, 1).unwrap();
    }

    #[test]
    fn check_nesting_depth_over_cap_rejects() {
        let multi = build_multi_envelope(vec![], CompletionRule::AllRequired, None, vec![]);
        let err = check_nesting_depth(&multi, MAX_NESTING_DEPTH + 1).unwrap_err();
        assert_eq!(
            err,
            EnvelopeError::NestingDepthExceeded(MAX_NESTING_DEPTH + 1)
        );
    }

    // === RFC-0962 §7 R8-F5 recursive nesting tests (Gap 2) ===

    fn sample_child_env() -> ExecutionEnvelope {
        build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap()
    }

    #[test]
    fn check_nesting_depth_accepts_two_level_envelope() {
        // Parent at depth 0 wrapping a child MultiEnvelope at depth 1.
        // MAX_NESTING_DEPTH = 4 → depth 1 is well under the cap.
        let child = build_multi_envelope(
            vec![sample_child_env()],
            CompletionRule::AllRequired,
            None,
            vec![],
        );
        let parent = build_multi_envelope(
            vec![sample_child_env()],
            CompletionRule::AllRequired,
            None,
            vec![],
        );
        // Manually attach the nested child via struct update (builder
        // default is nested = None).
        let parent = MultiEnvelope {
            nested: Some(Box::new(child)),
            ..parent
        };
        check_nesting_depth(&parent, 0).unwrap();
    }

    #[test]
    fn check_nesting_depth_rejects_five_level_chain() {
        // Build a chain parent → child → grandchild → ... 5 levels deep.
        // At top-level call with current_depth=0, the chain hits depth=5
        // which exceeds MAX_NESTING_DEPTH=4.
        let env = sample_child_env();
        let level5 = MultiEnvelope {
            nested: None,
            ..build_multi_envelope(vec![env.clone()], CompletionRule::AllRequired, None, vec![])
        };
        let level4 = MultiEnvelope {
            nested: Some(Box::new(level5)),
            ..build_multi_envelope(vec![env.clone()], CompletionRule::AllRequired, None, vec![])
        };
        let level3 = MultiEnvelope {
            nested: Some(Box::new(level4)),
            ..build_multi_envelope(vec![env.clone()], CompletionRule::AllRequired, None, vec![])
        };
        let level2 = MultiEnvelope {
            nested: Some(Box::new(level3)),
            ..build_multi_envelope(vec![env.clone()], CompletionRule::AllRequired, None, vec![])
        };
        let level1 = MultiEnvelope {
            nested: Some(Box::new(level2)),
            ..build_multi_envelope(vec![env], CompletionRule::AllRequired, None, vec![])
        };
        let err = check_nesting_depth(&level1, 0).unwrap_err();
        assert!(matches!(err, EnvelopeError::NestingDepthExceeded(d) if d == MAX_NESTING_DEPTH));
    }

    #[test]
    fn build_multi_envelope_with_all_required() {
        let env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec![],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let multi = build_multi_envelope(
            vec![env.clone(), env.clone()],
            CompletionRule::Quorum,
            Some(2),
            vec![],
        );
        assert_eq!(multi.sub_envelopes.len(), 2);
        assert_eq!(multi.completion_rule, CompletionRule::Quorum);
        assert_eq!(multi.completion_quorum_n, Some(2));
    }

    #[test]
    fn replay_composite_key_allows_same_nonce_different_signers() {
        // RFC-0962 §6.3: (signer_did, nonce) is the replay key. Two
        // distinct signers using the same nonce must not collide.
        struct InMemoryIndex {
            seen: Vec<(Vec<u8>, [u8; NONCE_SIZE])>,
        }
        impl ReplayIndex for InMemoryIndex {
            fn consumed_contains_for(&self, signer_did: &[u8], nonce: &[u8; NONCE_SIZE]) -> bool {
                self.seen
                    .iter()
                    .any(|(d, n)| d.as_slice() == signer_did && n == nonce)
            }
        }
        impl ReplayIndexMut for InMemoryIndex {
            fn mark_consumed_for(&mut self, signer_did: Vec<u8>, nonce: [u8; NONCE_SIZE]) {
                self.seen.push((signer_did, nonce));
            }
        }

        let nonce = [0x42; NONCE_SIZE];
        let mut env_alice = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:alice".to_owned(),
            vec![],
            vec![],
            vec![],
            [0x03; 32],
            100,
            nonce,
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let mut env_bob = env_alice.clone();
        env_bob.capability_holder = "did:octo:bob".to_owned();
        let key = make_key();
        sign_envelope(&mut env_alice, &key);
        sign_envelope(&mut env_bob, &key);

        let mut idx = InMemoryIndex { seen: vec![] };
        // Alice marks consumed — replay defense must trigger for alice.
        mark_consumed(&env_alice, &mut idx).unwrap();
        assert!(check_replay(&env_alice, &idx).is_err());
        // Bob uses the same nonce but different signer_did — must NOT replay.
        assert!(check_replay(&env_bob, &idx).is_ok());
    }
    #[test]
    fn envelope_size_under_cap_succeeds() {
        let env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let bytes = serialize_envelope(&env).unwrap();
        assert!(bytes.len() <= MAX_ENVELOPE_BYTES);
    }

    // === RFC-0962 §6.4 WAL two-phase hash binding tests ===

    #[test]
    fn tentative_envelope_id_deterministic() {
        let env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        assert_eq!(tentative_envelope_id(&env), tentative_envelope_id(&env));
    }

    #[test]
    fn tentative_envelope_id_differs_for_different_envelopes() {
        let mut env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:a".to_owned(),
            vec!["SELECT 1".to_owned()],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        env.capability_holder = "did:octo:b".to_owned();
        let env_a = tentative_envelope_id(
            &build_envelope(
                [0x01; 32],
                [0x02; 32],
                "did:octo:a".to_owned(),
                vec!["SELECT 1".to_owned()],
                vec![],
                vec![],
                [0x03; 32],
                100,
                [0x04; NONCE_SIZE],
                EnvelopeMode::Deterministic,
                1_000_000,
                ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
            )
            .unwrap(),
        );
        let env_b = tentative_envelope_id(&env);
        assert_ne!(env_a, env_b);
    }

    #[test]
    fn wal_hash_mismatch_rejected() {
        let env_id = [0x01; 32];
        let final_id = [0x02; 32];
        let err = verify_wal_hash_binding(
            &build_envelope(
                [0x01; 32],
                [0x02; 32],
                "did:octo:test".to_owned(),
                vec![],
                vec![],
                vec![],
                [0x03; 32],
                100,
                [0x04; NONCE_SIZE],
                EnvelopeMode::Deterministic,
                1_000_000,
                ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
            )
            .unwrap(),
            env_id,
            final_id,
            true,
            100,
            200,
        )
        .unwrap_err();
        assert!(matches!(err, EnvelopeError::WalHashMismatch { .. }));
    }

    #[test]
    fn wal_segment_missing_rejected() {
        let env_id = [0x01; 32];
        let seg = [0x05; 32];
        let err = verify_wal_hash_binding(
            &build_envelope(
                [0x01; 32],
                [0x02; 32],
                "did:octo:test".to_owned(),
                vec![],
                vec![],
                vec![],
                seg,
                100,
                [0x04; NONCE_SIZE],
                EnvelopeMode::Deterministic,
                1_000_000,
                ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
            )
            .unwrap(),
            env_id,
            env_id,
            false, // segment not present
            100,
            200,
        )
        .unwrap_err();
        assert!(matches!(err, EnvelopeError::WalSegmentMissing(_)));
    }

    #[test]
    fn wal_out_of_order_rejected() {
        let env_id = [0x01; 32];
        let seg = [0x05; 32];
        let err = verify_wal_hash_binding(
            &build_envelope(
                [0x01; 32],
                [0x02; 32],
                "did:octo:test".to_owned(),
                vec![],
                vec![],
                vec![],
                seg,
                100,
                [0x04; NONCE_SIZE],
                EnvelopeMode::Deterministic,
                1_000_000,
                ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
            )
            .unwrap(),
            env_id,
            env_id,
            true,
            500, // segment_height > local_height
            100,
        )
        .unwrap_err();
        assert!(matches!(err, EnvelopeError::WalOutOfOrder { .. }));
    }

    #[test]
    fn wal_binding_happy_path_accepts() {
        let env_id = [0x01; 32];
        let seg = [0x05; 32];
        verify_wal_hash_binding(
            &build_envelope(
                [0x01; 32],
                [0x02; 32],
                "did:octo:test".to_owned(),
                vec![],
                vec![],
                vec![],
                seg,
                100,
                [0x04; NONCE_SIZE],
                EnvelopeMode::Deterministic,
                1_000_000,
                ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
            )
            .unwrap(),
            env_id,
            env_id,
            true,
            100,
            200,
        )
        .unwrap();
    }

    #[test]
    fn finalize_wal_hash_chains_segments() {
        let prev = [0x01; 32];
        let body = b"segment body";
        let h1 = finalize_wal_hash(&prev, body);
        // Different prev → different hash
        let h2 = finalize_wal_hash(&[0x02; 32], body);
        assert_ne!(h1, h2);
        // Same inputs → same hash
        assert_eq!(h1, finalize_wal_hash(&prev, body));
    }

    // === RFC-0962 §7 MultiEnvelope fields tests ===

    #[test]
    fn multi_envelope_defaults_timeout_5s_fallback_abort() {
        let env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec![],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let multi = build_multi_envelope(
            vec![env.clone(), env.clone()],
            CompletionRule::AllRequired,
            None,
            vec![],
        );
        assert_eq!(multi.timeout_unix_ms, 5_000);
        assert_eq!(multi.fallback_action, FallbackAction::Abort);
    }

    #[test]
    fn multi_envelope_with_explicit_timeout_and_fallback() {
        let env = build_envelope(
            [0x01; 32],
            [0x02; 32],
            "did:octo:test".to_owned(),
            vec![],
            vec![],
            vec![],
            [0x03; 32],
            100,
            [0x04; NONCE_SIZE],
            EnvelopeMode::Deterministic,
            1_000_000,
            ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
        )
        .unwrap();
        let multi = build_multi_envelope_with(
            vec![env.clone()],
            CompletionRule::Quorum,
            Some(1),
            vec![],
            30_000,
            FallbackAction::RollbackAll,
        );
        assert_eq!(multi.timeout_unix_ms, 30_000);
        assert_eq!(multi.fallback_action, FallbackAction::RollbackAll);
        assert_eq!(multi.completion_rule, CompletionRule::Quorum);
        assert_eq!(multi.completion_quorum_n, Some(1));
    }
}
