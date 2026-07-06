//! TriggerStore — ArcSwap-backed registry with optimistic
//! concurrency. Phase 4.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use parking_lot::Mutex;

use super::trigger::{RunRecord, RunnerSpec, Trigger};
use crate::events::InboundEvent;
use crate::rules::canonical_etag;
use sha2::{Digest, Sha256};

#[derive(Debug, Default)]
pub struct Triggerset {
    pub triggers: Vec<Arc<Trigger>>,
    pub by_id: HashMap<String, usize>,
    pub version: u64,
}

impl Triggerset {
    pub fn empty() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Clone)]
pub struct TriggerDraft {
    pub id: String,
    pub enabled: bool,
    pub runner: RunnerSpec,
    pub rate_limit: Option<super::trigger::RateLimit>,
    pub timeout_ms: u64,
    pub retries: u32,
    pub history_cap: u32,
    pub created_by: String,
    pub now_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct TriggerPatch {
    pub runner: Option<RunnerSpec>,
    pub rate_limit: Option<Option<super::trigger::RateLimit>>,
    pub timeout_ms: Option<u64>,
    pub retries: Option<u32>,
    pub history_cap: Option<u32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TriggerError {
    #[error("trigger not found: {id}")]
    NotFound { id: String },
    #[error("trigger already exists: {id}")]
    AlreadyExists { id: String },
    #[error(
        "etag conflict on {id} (current_version={current_version}, current_etag={current_etag})"
    )]
    Conflict {
        id: String,
        current_etag: String,
        current_version: u64,
    },
    #[error("invalid trigger id: {reason}")]
    InvalidId { reason: String },
    #[error("trigger disabled: {id}")]
    Disabled { id: String },
    #[error("execution failed: {0}")]
    ExecFailed(String),
    #[error("runner not supported: {0}")]
    NotSupported(String),
}

#[derive(Debug)]
pub struct TriggerStore {
    state: ArcSwap<Triggerset>,
    last_swap_generation: AtomicU64,
    last_fire_ms: Mutex<HashMap<String, i64>>,
}

impl TriggerStore {
    pub fn new() -> Self {
        Self {
            state: ArcSwap::from_pointee(Triggerset::default()),
            last_swap_generation: AtomicU64::new(0),
            last_fire_ms: Mutex::new(HashMap::new()),
        }
    }

    pub fn swap_generation(&self) -> u64 {
        self.last_swap_generation.load(Ordering::Relaxed)
    }

    pub fn list(&self) -> Vec<Arc<Trigger>> {
        self.state.load().triggers.clone()
    }

    pub fn get(&self, id: &str) -> Option<Arc<Trigger>> {
        let s = self.state.load();
        s.by_id.get(id).and_then(|&i| s.triggers.get(i).cloned())
    }

    pub fn create(&self, draft: TriggerDraft) -> Result<Arc<Trigger>, TriggerError> {
        validate_id(&draft.id)?;
        let mut trigger = Trigger {
            id: draft.id,
            version: 1,
            enabled: draft.enabled,
            runner: draft.runner,
            rate_limit: draft.rate_limit,
            timeout_ms: draft.timeout_ms,
            retries: draft.retries,
            last_run: None,
            history_cap: draft.history_cap,
            created_by: draft.created_by,
            created_at: draft.now_ms,
            updated_at: draft.now_ms,
            etag: String::new(),
        };
        trigger.etag = canonical_etag(&trigger.etag_payload());
        let trigger = Arc::new(trigger);
        let new_snapshot = {
            let s = self.state.load();
            if s.by_id.contains_key(&trigger.id) {
                return Err(TriggerError::AlreadyExists {
                    id: trigger.id.clone(),
                });
            }
            let mut triggers = s.triggers.clone();
            triggers.push(trigger.clone());
            let by_id = rebuild_by_id(&triggers);
            Arc::new(Triggerset {
                triggers,
                by_id,
                version: s.version + 1,
            })
        };
        self.state.store(new_snapshot);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
        Ok(trigger)
    }

    pub fn update(
        &self,
        id: &str,
        caller_etag: &str,
        patch: TriggerPatch,
        now_ms: i64,
    ) -> Result<Arc<Trigger>, TriggerError> {
        let mut new_trigger = {
            let current = self
                .get(id)
                .ok_or(TriggerError::NotFound { id: id.to_string() })?;
            if current.etag != caller_etag {
                return Err(TriggerError::Conflict {
                    id: id.to_string(),
                    current_etag: current.etag.clone(),
                    current_version: current.version,
                });
            }
            let mut t: Trigger = (*current).clone();
            t.version += 1;
            t.updated_at = now_ms;
            if let Some(r) = patch.runner {
                t.runner = r;
            }
            if let Some(rl) = patch.rate_limit {
                t.rate_limit = rl;
            }
            if let Some(to) = patch.timeout_ms {
                t.timeout_ms = to;
            }
            if let Some(r) = patch.retries {
                t.retries = r;
            }
            if let Some(h) = patch.history_cap {
                t.history_cap = h;
            }
            if let Some(e) = patch.enabled {
                t.enabled = e;
            }
            t
        };
        new_trigger.etag = canonical_etag(&new_trigger.etag_payload());
        self.replace(new_trigger)
    }

    pub fn delete(&self, id: &str, caller_etag: &str) -> Result<(), TriggerError> {
        let current = self
            .get(id)
            .ok_or(TriggerError::NotFound { id: id.to_string() })?;
        if current.etag != caller_etag {
            return Err(TriggerError::Conflict {
                id: id.to_string(),
                current_etag: current.etag.clone(),
                current_version: current.version,
            });
        }
        let new_snapshot = {
            let s = self.state.load();
            let idx = *s.by_id.get(id).expect("present");
            let mut triggers: Vec<Arc<Trigger>> = Vec::with_capacity(s.triggers.len() - 1);
            for (i, t) in s.triggers.iter().enumerate() {
                if i != idx {
                    triggers.push(t.clone());
                }
            }
            let by_id = rebuild_by_id(&triggers);
            Arc::new(Triggerset {
                triggers,
                by_id,
                version: s.version + 1,
            })
        };
        self.state.store(new_snapshot);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Records the outcome of a run. Bumps version, updates
    /// `last_run`, and bumps `updated_at`. Idempotent: caller can
    /// invoke multiple times for retries.
    pub fn record_run(&self, id: &str, record: RunRecord) -> Result<Arc<Trigger>, TriggerError> {
        let mut new_trigger = {
            let current = self
                .get(id)
                .ok_or(TriggerError::NotFound { id: id.to_string() })?;
            let mut t: Trigger = (*current).clone();
            t.last_run = Some(record);
            t.version += 1;
            t
        };
        new_trigger.etag = canonical_etag(&new_trigger.etag_payload());
        self.replace(new_trigger)
    }

    /// Returns true if the trigger is fireable and outside its
    /// cooldown window. Updates the cooldown map on a positive
    /// answer.
    pub fn check_fireable(&self, id: &str, now_ms: i64) -> bool {
        let t = match self.get(id) {
            Some(t) => t,
            None => return false,
        };
        if !t.is_fireable() {
            return false;
        }
        let mut cooldown = self.last_fire_ms.lock();
        let last = cooldown.get(id).copied().unwrap_or(i64::MIN);
        if now_ms.saturating_sub(last) < (t.timeout_ms as i64).max(1) {
            return false;
        }
        cooldown.insert(id.to_string(), now_ms);
        true
    }

    /// Invokes the trigger. Phase 4 stub: returns
    /// `NotImplemented` for non-Agent runners; for `Agent` runner it
    /// records a synthetic `RunRecord` so callers can exercise the
    /// full chain. Real runners (shell, http) are wired in Part C.
    pub async fn run(
        &self,
        id: &str,
        _event: &InboundEvent,
        now_ms: i64,
    ) -> Result<RunRecord, TriggerError> {
        let t = self
            .get(id)
            .ok_or(TriggerError::NotFound { id: id.to_string() })?;
        if !t.is_fireable() {
            return Err(TriggerError::Disabled { id: id.to_string() });
        }
        if !self.check_fireable(id, now_ms) {
            return Err(TriggerError::ExecFailed("trigger in cooldown".into()));
        }
        // Synthetic record for Phase 4 Part B. Real dispatch comes
        // from `actions::run_for_trigger` (Part C).
        let record = RunRecord {
            started_at: now_ms,
            finished_at: now_ms,
            exit_code: 0,
            stdout_sha256: hex::encode(Sha256::digest(b"stub")),
            stderr_sha256: hex::encode(Sha256::digest(b"")),
            truncated: false,
            bytes_stdout: 0,
            bytes_stderr: 0,
        };
        self.record_run(id, record.clone())?;
        Ok(record)
    }

    fn replace(&self, new_trigger: Trigger) -> Result<Arc<Trigger>, TriggerError> {
        let new_arc = Arc::new(new_trigger);
        let new_snapshot = {
            let s = self.state.load();
            if !s.by_id.contains_key(&new_arc.id) {
                return Err(TriggerError::NotFound {
                    id: new_arc.id.clone(),
                });
            }
            let mut triggers = s.triggers.clone();
            let idx = *s.by_id.get(&new_arc.id).expect("present");
            triggers[idx] = new_arc.clone();
            Arc::new(Triggerset {
                triggers,
                by_id: s.by_id.clone(),
                version: s.version + 1,
            })
        };
        self.state.store(new_snapshot);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
        Ok(new_arc)
    }
}

impl Default for TriggerStore {
    fn default() -> Self {
        Self::new()
    }
}

fn rebuild_by_id(triggers: &[Arc<Trigger>]) -> HashMap<String, usize> {
    let mut m = HashMap::with_capacity(triggers.len());
    for (i, t) in triggers.iter().enumerate() {
        m.insert(t.id.clone(), i);
    }
    m
}

fn validate_id(id: &str) -> Result<(), TriggerError> {
    if id.is_empty() {
        return Err(TriggerError::InvalidId {
            reason: "empty".into(),
        });
    }
    if id.len() > 64 {
        return Err(TriggerError::InvalidId {
            reason: "longer than 64 chars".into(),
        });
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(TriggerError::InvalidId {
            reason: "must be [A-Za-z0-9_-] only".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{InboundEvent, MessageKind};

    fn dummy_draft(id: &str) -> TriggerDraft {
        TriggerDraft {
            id: id.into(),
            enabled: true,
            runner: RunnerSpec::Agent {
                agent_id: "a1".into(),
                input_template: "t".into(),
            },
            rate_limit: None,
            timeout_ms: 1000,
            retries: 0,
            history_cap: 10,
            created_by: "test".into(),
            now_ms: 1000,
        }
    }

    fn msg_event() -> InboundEvent {
        InboundEvent::Message {
            id: "M".into(),
            mentions_truncated: false,
            peer: "p".into(),
            sender: "s".into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
            kind: MessageKind::Text,
            text: "x".into(),
            media_token: None,
            reply_to: None,
            mentions: Vec::new(),
            is_group: false,
        }
    }

    #[tokio::test]
    async fn new_store_is_empty() {
        let s = TriggerStore::new();
        assert!(s.list().is_empty());
        assert_eq!(s.swap_generation(), 0);
    }

    #[tokio::test]
    async fn create_inserts_with_etag() {
        let s = TriggerStore::new();
        let t = s.create(dummy_draft("t1")).unwrap();
        assert_eq!(t.id, "t1");
        assert_eq!(t.version, 1);
        assert!(!t.etag.is_empty());
        assert_eq!(s.list().len(), 1);
    }

    #[tokio::test]
    async fn create_rejects_duplicate() {
        let s = TriggerStore::new();
        s.create(dummy_draft("dup")).unwrap();
        let err = s.create(dummy_draft("dup")).unwrap_err();
        assert!(matches!(err, TriggerError::AlreadyExists { .. }));
    }

    #[tokio::test]
    async fn create_rejects_invalid_id() {
        let s = TriggerStore::new();
        assert!(s.create(dummy_draft("")).is_err());
        assert!(s.create(dummy_draft("bad id!")).is_err());
    }

    #[tokio::test]
    async fn update_increments_version() {
        let s = TriggerStore::new();
        let t = s.create(dummy_draft("t1")).unwrap();
        let new = s
            .update(
                "t1",
                &t.etag,
                TriggerPatch {
                    timeout_ms: Some(5_000),
                    ..Default::default()
                },
                2000,
            )
            .unwrap();
        assert_eq!(new.version, 2);
        assert_eq!(new.timeout_ms, 5_000);
    }

    #[tokio::test]
    async fn update_stale_etag_returns_conflict() {
        let s = TriggerStore::new();
        let t = s.create(dummy_draft("t1")).unwrap();
        let _ = s
            .update(
                "t1",
                &t.etag,
                TriggerPatch {
                    timeout_ms: Some(5_000),
                    ..Default::default()
                },
                2000,
            )
            .unwrap();
        let err = s
            .update(
                "t1",
                &t.etag,
                TriggerPatch {
                    timeout_ms: Some(9_000),
                    ..Default::default()
                },
                3000,
            )
            .unwrap_err();
        assert!(matches!(err, TriggerError::Conflict { .. }));
    }

    #[tokio::test]
    async fn delete_with_correct_etag_succeeds() {
        let s = TriggerStore::new();
        let t = s.create(dummy_draft("t1")).unwrap();
        s.delete("t1", &t.etag).unwrap();
        assert!(s.list().is_empty());
    }

    #[tokio::test]
    async fn run_records_synthetic_outcome() {
        let s = TriggerStore::new();
        s.create(dummy_draft("t1")).unwrap();
        let rec = s.run("t1", &msg_event(), 1000).await.unwrap();
        assert_eq!(rec.exit_code, 0);
        let t = s.get("t1").unwrap();
        assert!(t.last_run.is_some());
    }

    #[tokio::test]
    async fn run_disabled_trigger_errors() {
        let s = TriggerStore::new();
        let _t = s
            .create(TriggerDraft {
                enabled: false,
                ..dummy_draft("t1")
            })
            .unwrap();
        let err = s.run("t1", &msg_event(), 1000).await.unwrap_err();
        assert!(matches!(err, TriggerError::Disabled { .. }));
    }

    #[tokio::test]
    async fn run_missing_trigger_errors() {
        let s = TriggerStore::new();
        let err = s.run("nope", &msg_event(), 0).await.unwrap_err();
        assert!(matches!(err, TriggerError::NotFound { .. }));
    }

    #[tokio::test]
    async fn check_fireable_respects_cooldown() {
        let s = TriggerStore::new();
        s.create(TriggerDraft {
            timeout_ms: 5_000,
            ..dummy_draft("t1")
        })
        .unwrap();
        assert!(s.check_fireable("t1", 1000));
        assert!(!s.check_fireable("t1", 2000));
        assert!(s.check_fireable("t1", 7000));
    }
}
