//! Audit log with SHA-256 hash chain and ring-buffer eviction.
//!
//! Phase 4 of `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`
//! §Security. Per-RPC audit row + per-N-anchor external write-once.
//!
//! ## Hash chain
//!
//! Each row records `prev_audit_hash` of the previous row. The chain
//! head is written to an external anchor file every `anchor_every`
//! rows (default 100), providing tamper evidence even if the
//! in-memory ring is wiped.
//!
//! ## Storage
//!
//! Phase 4 uses an in-memory ring buffer (Phase 5 wires to stoolap).
//! Tests are hermetic; production durability is the anchor file +
//! process restart semantics (handlers re-record into the new
//! buffer).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Single audit row. Persisted shape is the in-memory record plus a
/// computed `this_hash`; the disk anchor file appends the same shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    pub seq_no: u64,
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u128,
    pub caller_uid: String,
    pub caller_pid: u32,
    pub method: String,
    pub args_canonical_sha256: String,
    pub result_status: String,
    pub latency_ms: u64,
    pub prev_audit_hash: String,
    pub this_hash: String,
}

/// Input supplied by RPC middleware. `seq_no` and hashes are filled
/// in by `AuditLog::record`.
#[derive(Debug, Clone)]
pub struct AuditEntryInput {
    pub ts_unix_ms: i64,
    pub ts_mono_ns: u128,
    pub caller_uid: String,
    pub caller_pid: u32,
    pub method: String,
    pub args_canonical_sha256: String,
    pub result_status: String,
    pub latency_ms: u64,
}

/// Result of `verify_chain`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChainVerifyResult {
    pub ok: bool,
    pub broken_at_seq: Option<u64>,
    pub verified_count: u64,
    pub last_seq_no: u64,
}

#[derive(Debug)]
pub struct AuditLog {
    inner: Mutex<VecDeque<AuditEntry>>,
    max_rows: usize,
    seq_no: AtomicU64,
    truncated_total: AtomicU64,
    anchor_every: u64,
    anchor_path: Option<PathBuf>,
}

impl AuditLog {
    pub fn new(max_rows: usize, anchor_every: u64) -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(max_rows.min(1024))),
            max_rows,
            seq_no: AtomicU64::new(0),
            truncated_total: AtomicU64::new(0),
            anchor_every: anchor_every.max(1),
            anchor_path: None,
        }
    }

    pub fn with_anchor_path(mut self, path: PathBuf) -> Self {
        self.anchor_path = Some(path);
        self
    }

    pub fn seq_no(&self) -> u64 {
        self.seq_no.load(Ordering::Relaxed)
    }

    pub fn truncated_total(&self) -> u64 {
        self.truncated_total.load(Ordering::Relaxed)
    }

    /// Records an audit row. Returns the assigned `seq_no`.
    ///
    /// Side effects:
    /// - Computes `prev_audit_hash` from the last entry's `this_hash`.
    /// - Computes `this_hash` over the canonical payload.
    /// - Pushes to the ring buffer; evicts the oldest row when over
    ///   `max_rows` (increments `truncated_total`).
    /// - On every `anchor_every`-th row, appends to the anchor file
    ///   (best-effort; errors logged but do not fail the RPC).
    pub fn record(&self, input: AuditEntryInput) -> u64 {
        let seq_no = self.seq_no.fetch_add(1, Ordering::Relaxed) + 1;
        let (prev_hash, this_hash) = {
            let buf = self.inner.lock();
            let prev = buf.back().map(|e| e.this_hash.clone()).unwrap_or_default();
            let this = compute_hash(&prev, seq_no, &input);
            (prev, this)
        };
        let entry = AuditEntry {
            seq_no,
            ts_unix_ms: input.ts_unix_ms,
            ts_mono_ns: input.ts_mono_ns,
            caller_uid: input.caller_uid,
            caller_pid: input.caller_pid,
            method: input.method,
            args_canonical_sha256: input.args_canonical_sha256,
            result_status: input.result_status,
            latency_ms: input.latency_ms,
            prev_audit_hash: prev_hash,
            this_hash,
        };
        {
            let mut buf = self.inner.lock();
            if buf.len() >= self.max_rows {
                buf.pop_front();
                self.truncated_total.fetch_add(1, Ordering::Relaxed);
            }
            buf.push_back(entry.clone());
        }
        if seq_no.is_multiple_of(self.anchor_every) {
            self.write_anchor(&entry);
        }
        seq_no
    }

    /// Returns the last `limit` rows with `seq_no > since_seq`. The
    /// returned `Vec` is ordered by `seq_no` ascending.
    pub fn tail(&self, since_seq: u64, limit: usize) -> Vec<AuditEntry> {
        let limit = limit.min(10_000);
        let buf = self.inner.lock();
        buf.iter()
            .filter(|e| e.seq_no > since_seq)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Walks the chain from `seq_no=1` and verifies that each row's
    /// `prev_audit_hash` matches the previous row's `this_hash`. The
    /// ring buffer's first row should have an empty `prev_audit_hash`.
    /// Returns `ok=true` if the chain verifies; otherwise the first
    /// broken `seq_no`.
    pub fn verify_chain(&self) -> ChainVerifyResult {
        let buf = self.inner.lock();
        let mut prev_hash = String::new();
        let mut count = 0u64;
        let mut last_seq = 0u64;
        for entry in buf.iter() {
            count += 1;
            last_seq = entry.seq_no;
            if entry.prev_audit_hash != prev_hash {
                return ChainVerifyResult {
                    ok: false,
                    broken_at_seq: Some(entry.seq_no),
                    verified_count: count - 1,
                    last_seq_no: last_seq,
                };
            }
            // Recompute hash to detect payload tampering (not just
            // link tampering).
            let input = AuditEntryInput {
                ts_unix_ms: entry.ts_unix_ms,
                ts_mono_ns: entry.ts_mono_ns,
                caller_uid: entry.caller_uid.clone(),
                caller_pid: entry.caller_pid,
                method: entry.method.clone(),
                args_canonical_sha256: entry.args_canonical_sha256.clone(),
                result_status: entry.result_status.clone(),
                latency_ms: entry.latency_ms,
            };
            let recomputed = compute_hash(&prev_hash, entry.seq_no, &input);
            if recomputed != entry.this_hash {
                return ChainVerifyResult {
                    ok: false,
                    broken_at_seq: Some(entry.seq_no),
                    verified_count: count - 1,
                    last_seq_no: last_seq,
                };
            }
            prev_hash = entry.this_hash.clone();
        }
        ChainVerifyResult {
            ok: true,
            broken_at_seq: None,
            verified_count: count,
            last_seq_no: last_seq,
        }
    }

    fn write_anchor(&self, entry: &AuditEntry) {
        let Some(path) = &self.anchor_path else {
            return;
        };
        let line = match serde_json::to_string(entry) {
            Ok(s) => format!("{s}\n"),
            Err(_) => return,
        };
        // Best-effort append. Errors are swallowed because the audit
        // log must not block the RPC path.
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
}

fn compute_hash(prev_hash: &str, seq_no: u64, input: &AuditEntryInput) -> String {
    let mut h = Sha256::new();
    h.update(prev_hash.as_bytes());
    h.update(seq_no.to_le_bytes());
    h.update(input.ts_unix_ms.to_le_bytes());
    h.update(input.caller_uid.as_bytes());
    h.update(input.method.as_bytes());
    h.update(input.args_canonical_sha256.as_bytes());
    h.update(input.result_status.as_bytes());
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(method: &str, status: &str) -> AuditEntryInput {
        AuditEntryInput {
            ts_unix_ms: 1000,
            ts_mono_ns: 999,
            caller_uid: "test".into(),
            caller_pid: 42,
            method: method.into(),
            args_canonical_sha256: "abc".into(),
            result_status: status.into(),
            latency_ms: 1,
        }
    }

    #[test]
    fn empty_chain_verifies() {
        let log = AuditLog::new(100, 100);
        let v = log.verify_chain();
        assert!(v.ok);
        assert_eq!(v.verified_count, 0);
    }

    #[test]
    fn single_row_has_empty_prev_hash() {
        let log = AuditLog::new(100, 100);
        log.record(input("version.get", "ok"));
        let entries = log.tail(0, 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].seq_no, 1);
        assert_eq!(entries[0].prev_audit_hash, "");
        assert_ne!(entries[0].this_hash, "");
    }

    #[test]
    fn chain_links_previous_hash() {
        let log = AuditLog::new(100, 100);
        log.record(input("a", "ok"));
        log.record(input("b", "ok"));
        log.record(input("c", "ok"));
        let entries = log.tail(0, 10);
        assert_eq!(entries[0].prev_audit_hash, "");
        assert_eq!(entries[1].prev_audit_hash, entries[0].this_hash);
        assert_eq!(entries[2].prev_audit_hash, entries[1].this_hash);
    }

    #[test]
    fn chain_verifies_after_many_writes() {
        let log = AuditLog::new(100, 100);
        for i in 0..50 {
            log.record(input(&format!("m{i}"), "ok"));
        }
        let v = log.verify_chain();
        assert!(v.ok);
        assert_eq!(v.verified_count, 50);
        assert_eq!(v.last_seq_no, 50);
    }

    #[test]
    fn chain_detects_link_tamper() {
        let log = AuditLog::new(100, 100);
        log.record(input("a", "ok"));
        log.record(input("b", "ok"));
        log.record(input("c", "ok"));
        // Tamper with row 2's prev_audit_hash.
        {
            let mut buf = log.inner.lock();
            let entry = buf.get_mut(1).expect("row 2");
            entry.prev_audit_hash = "deadbeef".into();
        }
        let v = log.verify_chain();
        assert!(!v.ok);
        assert_eq!(v.broken_at_seq, Some(2));
        assert_eq!(v.verified_count, 1);
    }

    #[test]
    fn chain_detects_payload_tamper() {
        let log = AuditLog::new(100, 100);
        log.record(input("a", "ok"));
        log.record(input("b", "ok"));
        // Tamper with row 2's method field (link still valid but
        // recomputed hash differs).
        {
            let mut buf = log.inner.lock();
            let entry = buf.get_mut(1).expect("row 2");
            entry.method = "tampered".into();
        }
        let v = log.verify_chain();
        assert!(!v.ok);
        assert_eq!(v.broken_at_seq, Some(2));
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let log = AuditLog::new(5, 100);
        for i in 0..10 {
            log.record(input(&format!("m{i}"), "ok"));
        }
        assert_eq!(log.seq_no(), 10);
        assert_eq!(log.truncated_total(), 5);
        let entries = log.tail(0, 100);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].seq_no, 6);
        assert_eq!(entries[4].seq_no, 10);
    }

    #[test]
    fn tail_filters_by_since_seq() {
        let log = AuditLog::new(100, 100);
        for i in 0..5 {
            log.record(input(&format!("m{i}"), "ok"));
        }
        let entries = log.tail(3, 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq_no, 4);
        assert_eq!(entries[1].seq_no, 5);
    }

    #[test]
    fn tail_caps_at_limit() {
        let log = AuditLog::new(100, 100);
        for i in 0..20 {
            log.record(input(&format!("m{i}"), "ok"));
        }
        let entries = log.tail(0, 5);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].seq_no, 1);
        assert_eq!(entries[4].seq_no, 5);
    }

    #[test]
    fn hash_is_deterministic() {
        let i = input("m", "ok");
        let h1 = compute_hash("", 1, &i);
        let h2 = compute_hash("", 1, &i);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_changes_with_seq() {
        let i = input("m", "ok");
        let h1 = compute_hash("", 1, &i);
        let h2 = compute_hash("", 2, &i);
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_changes_with_prev() {
        let i = input("m", "ok");
        let h1 = compute_hash("prev1", 1, &i);
        let h2 = compute_hash("prev2", 1, &i);
        assert_ne!(h1, h2);
    }
}
