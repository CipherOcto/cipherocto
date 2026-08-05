//! Persistent audit log storage for decommission events (RFC-0850p-f v0.3).
//!
//! `AuditLog` (`decommission.rs`) is the in-memory BTreeMap used at runtime
//! for cheap O(1) lookups. `AuditLogStore` is the on-disk persistence layer
//! that survives node restart — entries are written through to disk as
//! NDJSON (one JSON object per line) and rotated by size or time threshold.
//!
//! Rotation policy (RFC-0850p-f v0.3 §F-4):
//! - **Size-based**: rotate when the current segment exceeds
//!   `max_segment_bytes` (default 1 MiB).
//! - **Time-based**: rotate when the current segment's oldest entry is
//!   older than `max_segment_age_secs` (default 86_400 = 24 h).
//! - **Naming**: `<dir>/audit-<seq_start>.ndjson`. `seq_end` is implicit
//!   from the file's contents (line count).
//!
//! Persistence format: NDJSON with one `PersistableAuditEntry` per line.
//! Crypto fields (`audit_hash`, `signature`, `nonce`, `unbind_hash`) are
//! intentionally NOT persisted — they live on the in-memory envelope and
//! the audit log stores the *metadata* (who, when, why, witness count,
//! group_jid) for forensic reconstruction.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::decommission::AuditEntry;

/// Reason byte carried by `UnbindReason` (re-declared here to avoid pulling
/// serde onto the canonical enum — this DTO owns its own wire format).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum PersistedUnbindReason {
    Scheduled = 0x00,
    MassKick = 0x01,
    MissionTerminated = 0x02,
    CoordinatorResign = 0x03,
    SafetyShutdown = 0x04,
}

impl PersistedUnbindReason {
    pub fn from_canonical(r: super::dc_envelopes::UnbindReason) -> Self {
        match r {
            super::dc_envelopes::UnbindReason::Scheduled => Self::Scheduled,
            super::dc_envelopes::UnbindReason::MassKick => Self::MassKick,
            super::dc_envelopes::UnbindReason::MissionTerminated => Self::MissionTerminated,
            super::dc_envelopes::UnbindReason::CoordinatorResign => Self::CoordinatorResign,
            super::dc_envelopes::UnbindReason::SafetyShutdown => Self::SafetyShutdown,
        }
    }

    pub fn to_canonical(self) -> super::dc_envelopes::UnbindReason {
        use super::dc_envelopes::UnbindReason;
        match self {
            Self::Scheduled => UnbindReason::Scheduled,
            Self::MassKick => UnbindReason::MassKick,
            Self::MissionTerminated => UnbindReason::MissionTerminated,
            Self::CoordinatorResign => UnbindReason::CoordinatorResign,
            Self::SafetyShutdown => UnbindReason::SafetyShutdown,
        }
    }
}

/// On-disk representation of a single audit entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistableAuditEntry {
    pub timestamp_secs: u64,
    pub domain_id: [u8; 32],
    pub group_jid: String,
    pub platform: String,
    pub initiator_id: [u8; 32],
    pub reason: PersistedUnbindReason,
    pub reason_text: String,
    pub initiated_at_epoch: u64,
    pub completed_at_epoch: u64,
    pub witness_count: u32,
}

impl PersistableAuditEntry {
    pub fn from_entry(entry: &AuditEntry) -> Self {
        Self {
            timestamp_secs: entry.timestamp_secs,
            domain_id: entry.envelope.domain_id,
            group_jid: entry.envelope.group_jid.clone(),
            platform: entry.envelope.platform.clone(),
            initiator_id: entry.envelope.initiator_id,
            reason: PersistedUnbindReason::from_canonical(entry.envelope.reason),
            reason_text: entry.envelope.reason_text.clone(),
            initiated_at_epoch: entry.envelope.initiated_at_epoch,
            completed_at_epoch: entry.envelope.completed_at_epoch,
            witness_count: entry.envelope.witness_count,
        }
    }

    pub fn to_entry(&self) -> AuditEntry {
        use super::decommission::UnbindAllAuditEnvelope;
        AuditEntry {
            timestamp_secs: self.timestamp_secs,
            envelope: UnbindAllAuditEnvelope {
                domain_id: self.domain_id,
                group_jid: self.group_jid.clone(),
                platform: self.platform.clone(),
                initiator_id: self.initiator_id,
                reason: self.reason.to_canonical(),
                reason_text: self.reason_text.clone(),
                initiated_at_epoch: self.initiated_at_epoch,
                completed_at_epoch: self.completed_at_epoch,
                witness_count: self.witness_count,
                unbind_hash: [0u8; 32],
                nonce: [0u8; 32],
                audit_hash: [0u8; 32],
                signature: [0u8; 64],
            },
        }
    }
}

/// Trait abstracting audit log persistence.
pub trait AuditLogStore: std::fmt::Debug {
    fn append(&mut self, seq: u64, entry: &AuditEntry) -> Result<(), AuditStoreError>;
    fn max_seq(&self) -> Result<Option<u64>, AuditStoreError>;
    fn read_range(
        &self,
        seq_start: u64,
        seq_end: u64,
    ) -> Result<Vec<(u64, AuditEntry)>, AuditStoreError>;
    fn rotate(&mut self) -> Result<PathBuf, AuditStoreError>;
}

/// Errors from `AuditLogStore` operations.
#[derive(Debug, thiserror::Error)]
pub enum AuditStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("corrupt NDJSON line: {0}")]
    CorruptLine(String),
}

/// Default maximum segment size before rotation (1 MiB).
pub const DEFAULT_MAX_SEGMENT_BYTES: u64 = 1024 * 1024;

/// Default maximum segment age before rotation (24 hours).
pub const DEFAULT_MAX_SEGMENT_AGE_SECS: u64 = 86_400;

/// NDJSON-on-disk implementation of `AuditLogStore`.
///
/// Each segment is `<dir>/audit-<seq_start>.ndjson`. The current segment
/// is tracked separately to avoid scanning the directory on each append.
#[derive(Debug)]
pub struct NdjsonAuditLogStore {
    dir: PathBuf,
    current: Option<CurrentSegment>,
    max_segment_bytes: u64,
    max_segment_age_secs: u64,
}

#[derive(Debug)]
struct CurrentSegment {
    file: File,
    path: PathBuf,
    /// Sequence number of the first entry in this segment. Tracked but
    /// not currently read in this file (the filename encodes it for
    /// debugging); reserved for future rotation policy decisions.
    #[allow(dead_code)]
    seq_start: u64,
    /// Sequence number of the last entry written so far. Tracked
    /// in-memory; the file itself only contains the entries.
    seq_end: u64,
    /// Timestamp of the first entry in this segment (for age-based rotation).
    first_entry_ts: u64,
    /// Bytes written to this segment so far.
    bytes: u64,
}

impl NdjsonAuditLogStore {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, AuditStoreError> {
        Self::open_with_limits(dir, DEFAULT_MAX_SEGMENT_BYTES, DEFAULT_MAX_SEGMENT_AGE_SECS)
    }

    pub fn open_with_limits(
        dir: impl Into<PathBuf>,
        max_segment_bytes: u64,
        max_segment_age_secs: u64,
    ) -> Result<Self, AuditStoreError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let mut store = Self {
            dir,
            current: None,
            max_segment_bytes,
            max_segment_age_secs,
        };
        store.reopen_current()?;
        Ok(store)
    }

    /// Read every existing segment and return entries in order. Used by
    /// `AuditLog::open` to rehydrate the in-memory BTreeMap.
    pub fn read_all(&self) -> Result<Vec<(u64, AuditEntry)>, AuditStoreError> {
        let mut out = Vec::new();
        let mut seq = 0u64;
        for seg_path in self.segment_paths()? {
            let (_start, _end) = parse_segment_filename(&seg_path)?;
            let file = File::open(&seg_path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                if line.is_empty() {
                    continue;
                }
                let dto: PersistableAuditEntry = serde_json::from_str(&line)
                    .map_err(|e| AuditStoreError::CorruptLine(format!("{e}")))?;
                out.push((seq, dto.to_entry()));
                seq += 1;
            }
        }
        Ok(out)
    }

    /// All segment paths in lexical (chronological) order.
    pub fn segment_paths(&self) -> Result<Vec<PathBuf>, AuditStoreError> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && is_segment_file(&&path) {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn reopen_current(&mut self) -> Result<(), AuditStoreError> {
        let paths = self.segment_paths()?;
        if let Some(last) = paths.last() {
            let (start, _end) = parse_segment_filename(last)?;
            // Re-seek the file contents to compute `seq_end` and
            // `first_entry_ts` so subsequent appends don't unnecessarily
            // rotate. `seq_end = highest seq in file` (0 if empty).
            let entries = self.read_all()?;
            let seq_end = entries.last().map(|(s, _)| *s).unwrap_or(0);
            let first_ts = entries.first().map(|(_, e)| e.timestamp_secs).unwrap_or(0);
            let file = OpenOptions::new().append(true).open(last)?;
            let bytes = fs::metadata(last).map(|m| m.len()).unwrap_or(0);
            self.current = Some(CurrentSegment {
                file,
                path: last.clone(),
                seq_start: start,
                seq_end,
                first_entry_ts: first_ts,
                bytes,
            });
        } else {
            self.start_new_segment(0)?;
        }
        Ok(())
    }

    fn start_new_segment(&mut self, seq_start: u64) -> Result<(), AuditStoreError> {
        let path = self.dir.join(format!("audit-{seq_start}.ndjson"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.current = Some(CurrentSegment {
            file,
            path,
            seq_start,
            seq_end: seq_start.saturating_sub(1),
            first_entry_ts: 0,
            bytes: 0,
        });
        Ok(())
    }

    fn rotation_needed(&self, cur: &CurrentSegment, now_secs: u64) -> bool {
        if cur.bytes >= self.max_segment_bytes {
            return true;
        }
        if cur.first_entry_ts != 0
            && now_secs.saturating_sub(cur.first_entry_ts) >= self.max_segment_age_secs
        {
            return true;
        }
        false
    }
}

impl AuditLogStore for NdjsonAuditLogStore {
    fn append(&mut self, seq: u64, entry: &AuditEntry) -> Result<(), AuditStoreError> {
        let needs_rotate = {
            let cur = self.current.as_ref().expect("segment must be open");
            cur.seq_end + 1 != seq || self.rotation_needed(cur, entry.timestamp_secs)
        };
        if needs_rotate {
            // Close the current segment. The filename `audit-<seq_start>` is
            // already stable — no rename needed.
            self.current = None;
            self.start_new_segment(seq)?;
        }
        let cur = self
            .current
            .as_mut()
            .expect("segment must be open after rotate");
        let dto = PersistableAuditEntry::from_entry(entry);
        let line = serde_json::to_string(&dto)?;
        let bytes = line.len() + 1; // +1 for newline
        cur.file.write_all(line.as_bytes())?;
        cur.file.write_all(b"\n")?;
        cur.file.sync_data()?;
        if cur.first_entry_ts == 0 {
            cur.first_entry_ts = entry.timestamp_secs;
        }
        cur.seq_end = seq;
        cur.bytes += bytes as u64;
        Ok(())
    }

    fn max_seq(&self) -> Result<Option<u64>, AuditStoreError> {
        // Re-read all entries to find the highest seq. Avoids the
        // `seq_end` drift problem on reopen.
        let all = self.read_all()?;
        Ok(all.last().map(|(s, _)| *s))
    }

    fn read_range(
        &self,
        seq_start: u64,
        seq_end: u64,
    ) -> Result<Vec<(u64, AuditEntry)>, AuditStoreError> {
        let all = self.read_all()?;
        Ok(all
            .into_iter()
            .filter(|(s, _)| *s >= seq_start && *s <= seq_end)
            .collect())
    }

    fn rotate(&mut self) -> Result<PathBuf, AuditStoreError> {
        let cur = self.current.take().ok_or_else(|| {
            AuditStoreError::Io(std::io::Error::other("no current segment to rotate"))
        })?;
        let next_seq = cur.seq_end + 1;
        self.start_new_segment(next_seq)?;
        Ok(cur.path)
    }
}

fn is_segment_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("audit-") & n.ends_with(".ndjson"))
}

fn parse_segment_filename(path: &Path) -> Result<(u64, u64), AuditStoreError> {
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AuditStoreError::Io(std::io::Error::other("invalid segment filename")))?;
    let stem = stem
        .strip_prefix("audit-")
        .and_then(|s| s.strip_suffix(".ndjson"))
        .ok_or_else(|| AuditStoreError::Io(std::io::Error::other("invalid segment filename")))?;
    let start: u64 = stem
        .parse()
        .map_err(|e| AuditStoreError::Io(std::io::Error::other(format!("bad seq: {e}"))))?;
    // `seq_end` is implicit from file contents; caller rehydrates by reading.
    Ok((start, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::dc_envelopes::UnbindReason;
    use ed25519_dalek::SigningKey;

    fn test_audit_envelope() -> super::super::decommission::UnbindAllAuditEnvelope {
        let key = SigningKey::from_bytes(&[1u8; 32]);
        super::super::decommission::UnbindAllAuditEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
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
        }
    }

    fn entry(seq_seed: u8, ts: u64) -> (u64, AuditEntry) {
        let mut env = test_audit_envelope();
        env.domain_id = [seq_seed; 32];
        env.nonce = [seq_seed; 32];
        env.witness_count = seq_seed as u32;
        (
            seq_seed as u64,
            AuditEntry {
                timestamp_secs: ts,
                envelope: env,
            },
        )
    }

    #[test]
    fn append_and_read_round_trip() {
        let dir = tempdir();
        let mut store = NdjsonAuditLogStore::open(&dir).unwrap();
        for s in 0u8..5 {
            let (_, e) = entry(s, 1_700_000_000 + s as u64);
            store.append(s as u64, &e).unwrap();
        }
        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].0, 0);
        assert_eq!(all[4].0, 4);
        assert_eq!(all[0].1.envelope.domain_id, [0u8; 32]);
        assert_eq!(all[4].1.envelope.domain_id, [4u8; 32]);
    }

    #[test]
    fn size_rotation_creates_new_segment() {
        let dir = tempdir();
        let mut store = NdjsonAuditLogStore::open_with_limits(&dir, 256, u64::MAX).unwrap();
        for s in 0u8..5 {
            let (_, e) = entry(s, 1_700_000_000 + s as u64);
            store.append(s as u64, &e).unwrap();
        }
        let segments = store.segment_paths().unwrap();
        assert!(
            segments.len() >= 2,
            "expected >=2 segments, got {segments:?}"
        );
    }

    #[test]
    fn rotate_returns_path_and_starts_new_segment() {
        let dir = tempdir();
        let mut store = NdjsonAuditLogStore::open(&dir).unwrap();
        let (_, e) = entry(0, 1_700_000_000);
        store.append(0, &e).unwrap();
        let rotated = store.rotate().unwrap();
        assert!(rotated.exists());
        assert!(rotated.to_string_lossy().contains("audit-0.ndjson"));
        let (_, e1) = entry(1, 1_700_000_001);
        store.append(1, &e1).unwrap();
        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].0, 1);
    }

    #[test]
    fn open_after_restart_rehydrates_entries() {
        let dir = tempdir();
        {
            let mut store = NdjsonAuditLogStore::open(&dir).unwrap();
            for s in 0u8..3 {
                let (_, e) = entry(s, 1_700_000_000 + s as u64);
                store.append(s as u64, &e).unwrap();
            }
        }
        let store2 = NdjsonAuditLogStore::open(&dir).unwrap();
        let all = store2.read_all().unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(store2.max_seq().unwrap(), Some(2));
    }

    #[test]
    fn time_rotation_creates_new_segment() {
        let dir = tempdir();
        let mut store = NdjsonAuditLogStore::open_with_limits(&dir, u64::MAX, 1).unwrap();
        let (_, e1) = entry(0, 1_700_000_000);
        store.append(0, &e1).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let (_, e2) = entry(1, 1_700_000_002);
        store.append(1, &e2).unwrap();
        let segments = store.segment_paths().unwrap();
        assert!(
            segments.len() >= 2,
            "expected time-based rotation, got {segments:?}"
        );
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "audit-store-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
