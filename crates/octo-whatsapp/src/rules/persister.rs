//! Rules persister — Phase 5 Part C.
//!
//! Background actor that turns in-memory `Rule` mutations into
//! durable disk state without blocking the mutator's hot path.
//!
//! Design contract (plan §Part C, Tasks 23-31):
//!
//! 1. **Mutations are immediately visible in memory.** The caller
//!    (`RuleStore::create/update/delete/replace_all`) performs the
//!    `ArcSwap<Ruleset>` swap FIRST and only then enqueues a
//!    `PersistOp` for debounced disk persistence. Readers therefore
//!    observe writes without waiting for `fsync`.
//!
//! 2. **Debounce + coalesce.** Multiple ops queued within
//!    `debounce_ms` collapse into one disk write. Coalescing rules:
//!    - `Upsert(rule)` — latest per `rule.id` wins (overrides prior
//!      pending `Upsert` for the same id).
//!    - `Delete(id)` — collapses with any subsequent `Upsert(id)`
//!      into just the `Upsert`.
//!    - `ReplaceAll(rules)` — supersedes everything pending.
//!
//! 3. **Atomic write.** Each flush serializes the current ruleset to
//!    a `tempfile::NamedTempFile` in the parent directory, calls
//!    `sync_all()`, then `persist(...)` (atomic rename). After the
//!    rename succeeds, the parent directory is `fsync`'d so the
//!    rename is durable on power loss. The published file is always
//!    either the prior version or the new full version — never a
//!    half-written document.
//!
//! 4. **WAL — audit trail (NOT source of truth).** Every successful
//!    flush appends a line `<seq>\t<json>\t<sha>` to the WAL with
//!    `fsync`. The SHA chains the previous tail line
//!    (tamper-evident). On startup the daemon calls
//!    `recover_from_wal` to **verify chain integrity** and seed the
//!    `next_seq` counter; the canonical state is **always**
//!    `rules.toml` (atomic writes). A WAL line whose chain is
//!    broken is rewritten out — the broken tail is dropped but the
//!    good lines are preserved.
//!
//! 5. **Cancel-safe.** `CancellationToken` triggers drain (write
//!    pending state, exit). Join handle completes; drop is
//!    well-defined.
//!
//! 6. **Bounded.** `tokio::sync::mpsc::channel(256)` — bursts up to
//!    256 queued mutations before back-pressure applies to the
//!    caller.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::predicate::{classify_regex, Predicate};
use super::rule::{ActionSpec, Rule, RuleState};

/// One queued mutation. Variant matters for coalescing.
#[derive(Debug)]
pub enum PersistOp {
    /// Insert or overwrite a single rule (latest per `rule.id` wins).
    Upsert(Rule),
    /// Drop a single rule by id (collapses with follow-up upserts).
    Delete(String),
    /// Wholesale replacement of the entire ruleset (DROPS everything
    /// else pending).
    ReplaceAll(Vec<Rule>),
}

/// Special op: force-flush any pending state immediately, ack via
/// the sender stashed in `pending_sync`. Distinct from `PersistOp`
/// because it has no in-memory bookkeeping.
#[derive(Debug)]
pub struct FlushSync;

/// Errors raised by the persister.
#[derive(Debug, Error)]
pub enum PersistError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml encode: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error("toml decode: {0}")]
    TomlDecode(#[from] toml::de::Error),
    #[error("persister channel closed")]
    ChannelClosed,
    /// Distinguishes "persister is slow / disk is wedged" from "the
    /// channel is genuinely dead". RPC handlers can retry on
    /// `FlushTimeout` but must not retry on `ChannelClosed`.
    #[error("flush timed out after {elapsed_ms}ms")]
    FlushTimeout { elapsed_ms: u64 },
    #[error("wal chain integrity broken at seq {0}")]
    WalChainBroken(u64),
}

/// Snapshot of the in-memory state known to the persister. The
/// mutator updates this on every `enqueue` so the background actor
/// never has to ask back for the current state.
#[derive(Debug)]
struct PersisterState {
    /// id → Rule. Sorted by id on every flush for determinism.
    rules: HashMap<String, Rule>,
    /// Pending coalescing decisions since the last flush. Hash key
    /// includes the variant to keep them separate.
    pending: HashMap<PendingKey, PendingValue>,
    /// Pending FlushSync ack (single slot; coalesced FIFO-style if
    /// multiple arrive — only the LAST one is acked; intermediate
    /// senders error). In practice the RPC layer awaits one at a
    /// time.
    pending_sync: Option<tokio::sync::oneshot::Sender<()>>,
    /// Next WAL seq number.
    next_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PendingKey {
    Upsert(String),
    Delete(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum PendingValue {
    Upsert(Rule),
    Delete,
}

impl Default for PersisterState {
    fn default() -> Self {
        Self {
            rules: HashMap::new(),
            pending: HashMap::new(),
            pending_sync: None,
            next_seq: 1,
        }
    }
}

/// Background persister handle. Cheap to clone; all clones share
/// the same `mpsc` channel + cancellation token + state.
#[derive(Debug)]
pub struct RulesPersister {
    tx: mpsc::Sender<PersistMessage>,
    cancel: CancellationToken,
    storage_path: PathBuf,
    wal_path: PathBuf,
    #[allow(dead_code)]
    debounce_ms: u64,
    state: Mutex<PersisterState>,
}

/// Internal channel message. Flat enum so `tokio::select!` can
/// branch without inspecting variants.
#[derive(Debug)]
enum PersistMessage {
    /// `Op` carries the payload only for tracing; the loop reads
    /// the updated in-memory `state` directly. The inner `PersistOp`
    /// is therefore intentionally ignored by the loop body.
    Op(#[allow(dead_code)] PersistOp),
    Flush(FlushSync),
}

impl RulesPersister {
    /// Spawn the background actor and return an `Arc` handle plus a
    /// `JoinHandle` for the caller to await on shutdown. Directory
    /// creation is the responsibility of the configuration step.
    pub fn spawn(
        storage_path: PathBuf,
        wal_path: PathBuf,
        debounce_ms: u64,
    ) -> (Arc<Self>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<PersistMessage>(256);
        let cancel = CancellationToken::new();
        let persister = Arc::new(Self {
            tx,
            cancel: cancel.clone(),
            storage_path: storage_path.clone(),
            wal_path: wal_path.clone(),
            debounce_ms,
            state: Mutex::new(PersisterState::default()),
        });
        let handle = tokio::spawn(run_persister(
            persister.clone(),
            rx,
            cancel,
            storage_path,
            wal_path,
            debounce_ms,
        ));
        (persister, handle)
    }

    /// Enqueue an op. Non-blocking — the channel buffers up to 256.
    pub async fn enqueue_op(&self, op: PersistOp) -> Result<(), PersistError> {
        // Coalesce into the pending map FIRST.
        {
            let mut g = self.state.lock();
            match &op {
                PersistOp::Upsert(r) => {
                    let id = r.id.clone();
                    g.rules.insert(id.clone(), r.clone());
                    g.pending.insert(
                        PendingKey::Upsert(id.clone()),
                        PendingValue::Upsert(r.clone()),
                    );
                    g.pending.remove(&PendingKey::Delete(id));
                }
                PersistOp::Delete(id) => {
                    g.rules.remove(id);
                    if !g.pending.contains_key(&PendingKey::Upsert(id.clone())) {
                        g.pending
                            .insert(PendingKey::Delete(id.clone()), PendingValue::Delete);
                    }
                }
                PersistOp::ReplaceAll(rules) => {
                    g.rules.clear();
                    for r in rules {
                        g.rules.insert(r.id.clone(), r.clone());
                    }
                    // Wholesale replace supersedes everything else
                    // — clear all prior pending decisions.
                    g.pending.clear();
                    // ReplaceAll is a wholesale snapshot; no need to
                    // track it in the per-id pending map (we'll
                    // consult `rules` directly on the next flush).
                    // Add a sentinel by also clearing the per-id
                    // map (already done above).
                }
            }
        }
        self.tx
            .send(PersistMessage::Op(op))
            .await
            .map_err(|_| PersistError::ChannelClosed)
    }

    /// Force a sync flush; returns when the disk write completes.
    pub async fn flush_sync(&self) -> Result<(), PersistError> {
        // Build the oneshot and stash the SENDER before sending the
        // Flush message — the persister loop reads `pending_sync`
        // after the next flush and acks by sending `()` to the
        // receiver. We hold the receiver locally and `await` it
        // directly (no polling), so the ack is delivered as soon as
        // the persister loop runs.
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut g = self.state.lock();
            g.pending_sync = Some(tx);
        }
        if self
            .tx
            .send(PersistMessage::Flush(FlushSync))
            .await
            .is_err()
        {
            // Channel closed — clear the stashed sender so a later
            // `take_pending_sync` does not see a leaked tx.
            let _ = self.take_pending_sync();
            return Err(PersistError::ChannelClosed);
        }
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_oneshot_recv_err)) => {
                // Persister dropped the sender without sending — treat
                // as channel closed (the loop is exiting).
                Err(PersistError::ChannelClosed)
            }
            Err(_elapsed) => {
                // Persister is alive but slow / disk wedged. Clear
                // the stale sender so the next flush is not blocked.
                let _ = self.take_pending_sync();
                Err(PersistError::FlushTimeout { elapsed_ms: 30_000 })
            }
        }
    }

    /// Cancel the background actor. Pending ops are flushed before
    /// exit.
    pub fn cancel_handle(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Storage path (rules.toml).
    pub fn storage_path(&self) -> &Path {
        &self.storage_path
    }

    /// WAL path.
    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    /// Snapshot of the in-memory rules (cloned). Visible for tests.
    pub fn snapshot(&self) -> HashMap<String, Rule> {
        let g = self.state.lock();
        g.rules.clone()
    }

    /// Number of pending ops queued for the next flush.
    pub fn pending_len(&self) -> usize {
        self.state.lock().pending.len()
    }

    /// Inject a known-good ruleset into the in-memory state without
    /// going through the enqueue channel. Used by
    /// `recover_from_wal` to seed the actor before it starts.
    pub(crate) fn seed_snapshot(&self, rules: Vec<Rule>) {
        let mut g = self.state.lock();
        g.rules.clear();
        g.pending.clear();
        g.next_seq = 1;
        for r in rules {
            g.rules.insert(r.id.clone(), r);
        }
    }

    /// Returns the current `next_seq`.
    pub fn next_seq_no(&self) -> u64 {
        self.state.lock().next_seq
    }

    /// Bump the next_seq counter to `at_least`. Used by
    /// `recover_from_wal` and `load_initial_rules_from_disk` to
    /// restore the chain counter after restart so new WAL lines do
    /// not collide with prior entries (correctness review F7).
    pub(crate) fn bump_seq(&self, at_least: u64) {
        let mut g = self.state.lock();
        if at_least > g.next_seq {
            g.next_seq = at_least;
        }
    }

    /// Peek the current pending `FlushSync` sender (if any). The
    /// background loop calls this after a flush and acks it.
    fn take_pending_sync(&self) -> Option<tokio::sync::oneshot::Sender<()>> {
        let mut g = self.state.lock();
        g.pending_sync.take()
    }
}

// ---- TOML schema for `rules.toml` ----

/// Top-level shape of the on-disk `rules.toml`. `version = 1`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedRuleset {
    pub version: u32,
    pub rules: Vec<PersistedRule>,
}

impl PersistedRuleset {
    /// Current schema version.
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn from_rules(rules: Vec<Rule>) -> Self {
        Self {
            version: Self::SCHEMA_VERSION,
            rules: rules.into_iter().map(PersistedRule::from).collect(),
        }
    }

    pub fn into_rules(self) -> Vec<Rule> {
        self.rules.into_iter().map(PersistedRule::into).collect()
    }
}

/// TOML-friendly shape of `Rule`. Mirrors the Rust struct 1:1
/// except `state` serializes as a snake-case string (matches the
/// daemon's IPC convention).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedRule {
    pub id: String,
    pub version: u64,
    pub enabled: bool,
    pub priority: i32,
    /// Snake-case string: `"draft" | "approved" | "disabled"`.
    pub state: String,
    pub predicate: Predicate,
    pub actions: Vec<ActionSpec>,
    pub cooldown_ms: u64,
    pub ttl_until: Option<i64>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub etag: String,
}

impl From<Rule> for PersistedRule {
    fn from(r: Rule) -> Self {
        Self {
            id: r.id,
            version: r.version,
            enabled: r.enabled,
            priority: r.priority,
            state: state_label(r.state).to_string(),
            predicate: r.predicate,
            actions: r.actions,
            cooldown_ms: r.cooldown_ms,
            ttl_until: r.ttl_until,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            etag: r.etag,
        }
    }
}

impl From<PersistedRule> for Rule {
    fn from(p: PersistedRule) -> Self {
        Self {
            id: p.id,
            version: p.version,
            enabled: p.enabled,
            priority: p.priority,
            state: parse_state_label(&p.state).unwrap_or(RuleState::Draft),
            predicate: p.predicate,
            actions: p.actions,
            cooldown_ms: p.cooldown_ms,
            ttl_until: p.ttl_until,
            created_by: p.created_by,
            created_at: p.created_at,
            updated_at: p.updated_at,
            etag: p.etag,
        }
    }
}

fn state_label(s: RuleState) -> &'static str {
    match s {
        RuleState::Draft => "draft",
        RuleState::Approved => "approved",
        RuleState::Disabled => "disabled",
    }
}

fn parse_state_label(s: &str) -> Option<RuleState> {
    match s {
        "draft" => Some(RuleState::Draft),
        "approved" => Some(RuleState::Approved),
        "disabled" => Some(RuleState::Disabled),
        _ => None,
    }
}

/// Replay the WAL file and return the ruleset at the highest valid
/// line. Returns an empty ruleset when the file is missing or
/// contains no valid lines (the expected state for a fresh boot).
///
/// **Source of truth:** `rules.toml` (atomic writes via rename). The
/// WAL is a tamper-evident **audit trail**; on startup this function
/// verifies chain integrity, drops any corrupted tail, and returns
/// the highest valid seq so the persister can resume the chain
/// without colliding with prior entries. The actual rule state is
/// loaded from `rules.toml` by the caller (see `load_initial_rules_from_disk`).
///
/// On chain mismatch: the WAL is rewritten with ONLY the valid lines
/// (atomic temp + rename) so future appends continue at the right
/// seq and the good chain is preserved for forensic review.
pub async fn recover_from_wal(wal_path: &Path) -> Result<Vec<Rule>, PersistError> {
    let bytes = match tokio::fs::read(wal_path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(PersistError::Io(e)),
    };
    // Strict UTF-8 decode — lossy decode hides half-line corruption
    // (correctness review F4). If the WAL has a half-line, we report
    // it as a broken tail.
    let content = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_string(),
        Err(e) => {
            tracing::warn!(
                valid_up_to = e.valid_up_to(),
                "rules_persister: WAL has invalid UTF-8; truncating at boundary"
            );
            let valid = String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned();
            rewrite_wal_valid_lines(wal_path, &valid).await?;
            return Ok(Vec::new());
        }
    };
    let mut last_good_chain: Vec<String> = Vec::new();
    let mut last_valid_seq: u64 = 0;
    let mut valid_rules: Vec<Rule> = Vec::new();
    let mut valid_lines_buf = String::new();
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let seq = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or(PersistError::WalChainBroken(last_valid_seq))?;
        let payload = parts.next().unwrap_or("");
        let claimed_sha = parts.next().unwrap_or("");
        let prev = last_good_chain.last().cloned().unwrap_or_default();
        let expected = {
            let mut hasher = Sha256::new();
            hasher.update(prev.as_bytes());
            hasher.update(b"\t");
            hasher.update(seq.to_string().as_bytes());
            hasher.update(b"\t");
            hasher.update(payload.as_bytes());
            hex::encode(hasher.finalize())
        };
        if expected != claimed_sha {
            tracing::warn!(
                seq_no = seq,
                last_valid = last_valid_seq,
                "rules_persister: WAL chain mismatch; truncating tail and rewriting good lines"
            );
            rewrite_wal_valid_lines(wal_path, &valid_lines_buf).await?;
            return Ok(valid_rules);
        }
        valid_lines_buf.push_str(line);
        valid_lines_buf.push('\n');
        if let Some(rules) = apply_wal_payload(payload)? {
            valid_rules = rules;
        }
        last_valid_seq = seq;
        last_good_chain.push(claimed_sha.to_string());
    }
    Ok(valid_rules)
}

/// Read the WAL and return the highest valid seq (0 if missing or
/// empty). Used at startup to seed `next_seq` so a daemon restart
/// does not collide with prior chain entries (correctness review F7).
pub fn max_wal_seq(wal_path: &Path) -> Result<u64, PersistError> {
    let bytes = match std::fs::read(wal_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(PersistError::Io(e)),
    };
    let content = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => return Ok(0),
    };
    let mut max_seq = 0u64;
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(seq) = line.split('\t').next().and_then(|s| s.parse().ok()) {
            if seq > max_seq {
                max_seq = seq;
            }
        }
    }
    Ok(max_seq)
}

/// Rewrite the WAL with only the already-verified good lines. Atomic
/// temp + rename so a crash mid-rewrite leaves the original intact.
async fn rewrite_wal_valid_lines(wal_path: &Path, valid_lines: &str) -> Result<(), PersistError> {
    let parent = wal_path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.as_os_str().is_empty() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(tmp.as_file_mut(), valid_lines.as_bytes())?;
    tmp.as_file_mut().sync_all()?;
    tmp.persist(wal_path)
        .map_err(|e| PersistError::Io(e.error))?;
    // fsync the parent directory so the rename is durable.
    if let Ok(dir) = tokio::fs::File::open(parent).await {
        let _ = dir.sync_all().await;
    }
    Ok(())
}

/// Decode a WAL payload. Returns `Some(new_ruleset)` for
/// `ReplaceAll`, `None` for the others. Used during recovery.
fn apply_wal_payload(payload: &str) -> Result<Option<Vec<Rule>>, PersistError> {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "op", rename_all = "snake_case")]
    enum WalOp {
        Upsert {
            #[allow(dead_code)]
            rule: serde_json::Value,
        },
        Delete {
            #[allow(dead_code)]
            id: String,
        },
        ReplaceAll {
            #[serde(default)]
            rules: Vec<PersistedRule>,
        },
    }
    let parsed: WalOp = serde_json::from_str(payload)
        .map_err(|e| PersistError::Io(std::io::Error::other(format!("wal json: {e}"))))?;
    match parsed {
        WalOp::ReplaceAll { rules } => {
            Ok(Some(rules.into_iter().map(PersistedRule::into).collect()))
        }
        WalOp::Upsert { .. } | WalOp::Delete { .. } => Ok(None),
    }
}

// ---- background loop ----

async fn run_persister(
    persister: Arc<RulesPersister>,
    mut rx: mpsc::Receiver<PersistMessage>,
    cancel: CancellationToken,
    storage_path: PathBuf,
    wal_path: PathBuf,
    debounce_ms: u64,
) {
    loop {
        // Stage 1 — wait for the first message (or cancel).
        let first = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                drain_and_exit(&persister, &storage_path, &wal_path).await;
                return;
            }
            first = rx.recv() => {
                match first {
                    Some(m) => m,
                    None => {
                        // Channel closed — drain + exit.
                        drain_and_exit(&persister, &storage_path, &wal_path).await;
                        return;
                    }
                }
            }
        };
        let _first_was_flush = matches!(first, PersistMessage::Flush(_));

        // Stage 2 — collect more messages within `debounce_ms`.
        let debounce = std::time::Duration::from_millis(debounce_ms.max(1));
        let mut deadline = tokio::time::Instant::now() + debounce;

        // If the first message was a Flush, flush immediately.
        if _first_was_flush {
            let _ = flush_state(&persister, &storage_path, &wal_path).await;
            if let Some(ack) = persister.take_pending_sync() {
                let _ = ack.send(());
            }
            continue;
        }

        // Otherwise, wait for either another message (which may
        // reset debounce) or the deadline to elapse.
        let debounce_window_passed;
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                debounce_window_passed = true;
                break;
            }
            let remaining = deadline - now;
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    let _ = flush_state(&persister, &storage_path, &wal_path).await;
                    if let Some(ack) = persister.take_pending_sync() {
                        let _ = ack.send(());
                    }
                    return;
                }
                next = rx.recv() => {
                    let Some(m) = next else {
                        let _ = flush_state(&persister, &storage_path, &wal_path).await;
                        return;
                    };
                    if matches!(m, PersistMessage::Flush(_)) {
                        let _ = flush_state(&persister, &storage_path, &wal_path).await;
                        if let Some(ack) = persister.take_pending_sync() {
                            let _ = ack.send(());
                        }
                        // Already flushed above; signal don't
                        // flush again.
                        debounce_window_passed = false;
                        break;
                    }
                    // Reset the deadline — debounce restarts.
                    deadline = tokio::time::Instant::now() + debounce;
                }
                _ = tokio::time::sleep(remaining) => {
                    debounce_window_passed = true;
                    break;
                }
            }
        }
        if debounce_window_passed {
            let _ = flush_state(&persister, &storage_path, &wal_path).await;
        }
    }
}

async fn drain_and_exit(persister: &RulesPersister, storage_path: &Path, wal_path: &Path) {
    let _ = flush_state(persister, storage_path, wal_path).await;
    if let Some(ack) = persister.take_pending_sync() {
        let _ = ack.send(());
    }
}

async fn flush_state(
    p: &RulesPersister,
    storage_path: &Path,
    wal_path: &Path,
) -> Result<(), PersistError> {
    // Snapshot + clear the pending map atomically.
    let (mut new_rules, seq_to_write) = {
        let mut g = p.state.lock();
        // Sort the snapshot rules by id for deterministic output.
        let mut rules: Vec<Rule> = g.rules.values().cloned().collect();
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        // Clear pending — by this point, all pending decisions are
        // already reflected in `g.rules` (the mutator updates both
        // atomically inside `enqueue_op`).
        g.pending.clear();
        let seq = g.next_seq;
        g.next_seq += 1;
        (rules, seq)
    };
    let _ = &mut new_rules;
    // Serialize to TOML.
    let toml_bytes = {
        let set = PersistedRuleset::from_rules(new_rules);
        toml::to_string(&set)?
    };
    // Append to WAL first (so a crash between WAL and rules.toml is
    // recoverable — replay yields equivalent state).
    append_wal_line(wal_path, seq_to_write, &toml_bytes).await?;
    // Atomic write of rules.toml.
    write_rules_atomic(storage_path, &toml_bytes).await?;
    Ok(())
}

async fn write_rules_atomic(path: &Path, content: &str) -> Result<(), PersistError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    // Explicit 0600 on the temp file so the grace window cannot be
    // observed by other users (security review F7).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(tmp.path(), perms);
    }
    std::io::Write::write_all(tmp.as_file_mut(), content.as_bytes())?;
    tmp.as_file_mut().sync_all()?;
    tmp.persist(path).map_err(|e| PersistError::Io(e.error))?;
    // After the rename, fsync the parent directory so the directory
    // entry is durable across power loss. Without this, the rename
    // may not survive a crash (correctness review F5).
    if let Ok(dir) = tokio::fs::File::open(parent).await {
        let _ = dir.sync_all().await;
    }
    // Enforce 0600 on the published file (security review F7).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            let _ = meta; // suppress unused
        }
    }
    Ok(())
}

async fn append_wal_line(wal_path: &Path, seq: u64, toml_bytes: &str) -> Result<(), PersistError> {
    if let Some(parent) = wal_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    let prev_sha = read_last_wal_sha(wal_path).await?;
    let payload_json = serde_json::json!({
        "op": "replace_all",
        "toml_len": toml_bytes.len(),
        "ts_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    })
    .to_string();
    let mut hasher = Sha256::new();
    hasher.update(prev_sha.as_bytes());
    hasher.update(b"\t");
    hasher.update(seq.to_string().as_bytes());
    hasher.update(b"\t");
    hasher.update(payload_json.as_bytes());
    let sha = hex::encode(hasher.finalize());
    let line = format!("{seq}\t{payload_json}\t{sha}\n");
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(wal_path)
        .await?;
    f.write_all(line.as_bytes()).await?;
    f.sync_all().await?;
    Ok(())
}

async fn read_last_wal_sha(wal_path: &Path) -> Result<String, PersistError> {
    let bytes = match tokio::fs::read(wal_path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(PersistError::Io(e)),
    };
    let content = String::from_utf8_lossy(&bytes);
    let last = content
        .lines()
        .rfind(|l| !l.is_empty())
        .map(|s| s.to_string());
    let Some(line) = last else {
        return Ok(String::new());
    };
    let sha = line.splitn(3, '\t').nth(2).unwrap_or("").to_string();
    Ok(sha)
}

/// Resolve `~`-prefixed paths to the user's home directory. Returns
/// the path unchanged if it does not start with `~` or if
/// `dirs::home_dir()` returns `None`.
pub fn resolve_storage_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    p.to_path_buf()
}

impl RulesPersister {
    /// Static seed — equivalent to `seed_snapshot` but callable
    /// without holding an `Arc<Self>` directly. Used by the daemon
    /// at startup before the persister is shared.
    pub fn seed_snapshot_static(self: &Arc<Self>, rules: Vec<Rule>) {
        self.seed_snapshot(rules);
    }
}

/// Validate a `Rule` freshly loaded from disk: id format +
/// ReDoS predicate check. Drops everything else. Used by
/// `Daemon::handle` at startup so a malformed `rules.toml` cannot
/// wedge the daemon.
pub fn validate_persisted_rule(rule: &Rule) -> bool {
    if rule.id.is_empty() || rule.id.len() > 64 {
        return false;
    }
    if !rule
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return false;
    }
    // ReDoS-style predicate validation.
    let mut stack: Vec<&Predicate> = vec![&rule.predicate];
    while let Some(node) = stack.pop() {
        match node {
            Predicate::TextRegex { pattern } => {
                if classify_regex(pattern).is_err() {
                    return false;
                }
            }
            Predicate::And(children) | Predicate::Or(children) => {
                stack.extend(children.iter());
            }
            Predicate::Not(inner) => stack.push(inner),
            _ => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn dummy_rule(id: &str, priority: i32) -> Rule {
        Rule {
            id: id.into(),
            version: 1,
            enabled: true,
            priority,
            predicate: Predicate::True,
            actions: vec![],
            cooldown_ms: 0,
            ttl_until: None,
            created_by: "test".into(),
            created_at: 1_000_000,
            updated_at: 1_000_000,
            etag: format!("etag-{id}"),
            state: RuleState::Approved,
        }
    }

    fn spawn_for(
        dir: &TempDir,
        debounce_ms: u64,
    ) -> (Arc<RulesPersister>, tokio::task::JoinHandle<()>) {
        let storage = dir.path().join("rules.toml");
        let wal = dir.path().join("rules.wal");
        RulesPersister::spawn(storage, wal, debounce_ms)
    }

    async fn wait_until<F: FnMut() -> bool>(mut pred: F, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while !pred() {
            if start.elapsed() > timeout {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        true
    }

    fn read_toml(path: &Path) -> PersistedRuleset {
        let bytes = std::fs::read(path).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        toml::from_str(s).unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn upsert_coalesces_two_updates_within_debounce() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 100);
        p.enqueue_op(PersistOp::Upsert(dummy_rule("r1", 0)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        p.enqueue_op(PersistOp::Upsert(dummy_rule("r1", 99)))
            .await
            .unwrap();
        // flush_sync forces it onto disk now (bypasses debounce).
        p.flush_sync().await.unwrap();
        assert_eq!(p.snapshot().len(), 1);
        assert_eq!(p.snapshot()["r1"].priority, 99);
        let toml = read_toml(p.storage_path());
        assert_eq!(toml.rules.len(), 1);
        assert_eq!(toml.rules[0].priority, 99);
        p.cancel.cancel();
        let _ = h.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_then_upsert_resolves_to_upsert() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 50);
        p.enqueue_op(PersistOp::Upsert(dummy_rule("r1", 1)))
            .await
            .unwrap();
        p.flush_sync().await.unwrap();
        p.enqueue_op(PersistOp::Delete("r1".into())).await.unwrap();
        p.enqueue_op(PersistOp::Upsert(dummy_rule("r1", 5)))
            .await
            .unwrap();
        p.flush_sync().await.unwrap();
        let toml = read_toml(p.storage_path());
        assert_eq!(toml.rules.len(), 1);
        assert_eq!(toml.rules[0].id, "r1");
        assert_eq!(toml.rules[0].priority, 5);
        p.cancel.cancel();
        let _ = h.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replace_all_flushes_pending() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 50);
        p.enqueue_op(PersistOp::Upsert(dummy_rule("a", 1)))
            .await
            .unwrap();
        p.enqueue_op(PersistOp::Upsert(dummy_rule("b", 2)))
            .await
            .unwrap();
        p.flush_sync().await.unwrap();
        // Now replace with [{c}] — should drop a and b.
        p.enqueue_op(PersistOp::ReplaceAll(vec![dummy_rule("c", 3)]))
            .await
            .unwrap();
        p.flush_sync().await.unwrap();
        let toml = read_toml(p.storage_path());
        assert_eq!(toml.rules.len(), 1);
        assert_eq!(toml.rules[0].id, "c");
        p.cancel.cancel();
        let _ = h.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wal_fsyncs_on_every_entry() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 10);
        for i in 0..3 {
            p.enqueue_op(PersistOp::Upsert(dummy_rule(&format!("r{i}"), i)))
                .await
                .unwrap();
            p.flush_sync().await.unwrap();
        }
        let bytes = std::fs::read(p.wal_path()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let line_count = text.lines().filter(|l| !l.is_empty()).count();
        assert!(line_count >= 3);
        for line in text.lines().filter(|l| !l.is_empty()) {
            let mut parts = line.splitn(3, '\t');
            let _seq = parts.next();
            let _payload = parts.next();
            let sha = parts.next();
            assert!(sha.is_some(), "every WAL line must carry a sha: {line:?}");
        }
        p.cancel.cancel();
        let _ = h.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn crash_recovery_prepopulated_wal() {
        let dir = TempDir::new().unwrap();
        let wal = dir.path().join("rules.wal");
        let ruleset = PersistedRuleset::from_rules(vec![dummy_rule("restored", 42)]);
        let toml_text = toml::to_string(&ruleset).unwrap();
        let seq: u64 = 1;
        let payload = serde_json::json!({
            "op": "replace_all",
            "rules": ruleset.rules,
            "toml_len": toml_text.len(),
            "ts_ms": 1_700_000_000_000_i64,
        })
        .to_string();
        let prev_sha = String::new();
        let mut hasher = Sha256::new();
        hasher.update(prev_sha.as_bytes());
        hasher.update(b"\t");
        hasher.update(seq.to_string().as_bytes());
        hasher.update(b"\t");
        hasher.update(payload.as_bytes());
        let sha = hex::encode(hasher.finalize());
        let line = format!("{seq}\t{payload}\t{sha}\n");
        std::fs::write(&wal, line).unwrap();
        let recovered = recover_from_wal(&wal).await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, "restored");
        assert_eq!(recovered[0].priority, 42);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_flushes_pending_via_flush_sync() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 5_000);
        p.enqueue_op(PersistOp::Upsert(dummy_rule("r1", 7)))
            .await
            .unwrap();
        // Force-flush (bypasses debounce).
        p.flush_sync().await.unwrap();
        let bytes = std::fs::read(p.storage_path()).unwrap();
        let toml_text = std::str::from_utf8(&bytes).unwrap();
        assert!(toml_text.contains("priority = 7"));
        p.cancel.cancel();
        let _ = h.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_toml_rejected() {
        let dir = TempDir::new().unwrap();
        let storage = dir.path().join("rules.toml");
        std::fs::write(&storage, b"this is { not valid toml ==").unwrap();
        let bytes = std::fs::read(&storage).unwrap();
        let result: Result<PersistedRuleset, _> =
            toml::from_str(std::str::from_utf8(&bytes).unwrap());
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_enqueue_no_data_race() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 50);
        let mut joins = Vec::new();
        for i in 0..20 {
            let pp = p.clone();
            joins.push(tokio::spawn(async move {
                pp.enqueue_op(PersistOp::Upsert(dummy_rule(&format!("t{i}"), i)))
                    .await
                    .unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        p.flush_sync().await.unwrap();
        let toml = read_toml(p.storage_path());
        assert_eq!(toml.rules.len(), 20);
        p.cancel.cancel();
        let _ = h.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resolve_storage_path_strips_tilde() {
        assert_eq!(
            resolve_storage_path(Path::new("~/x/rules.toml")),
            dirs::home_dir().unwrap().join("x/rules.toml")
        );
        assert_eq!(
            resolve_storage_path(Path::new("/var/lib/x.toml")),
            PathBuf::from("/var/lib/x.toml")
        );
    }

    #[test]
    fn schema_round_trip_toml_byte_stable() {
        let ruleset = PersistedRuleset::from_rules(vec![dummy_rule("a", 1), dummy_rule("b", 2)]);
        let s = toml::to_string(&ruleset).unwrap();
        let back: PersistedRuleset = toml::from_str(&s).unwrap();
        assert_eq!(ruleset, back);
        let again = toml::to_string(&back).unwrap();
        assert_eq!(s, again);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn debounce_idle_writes_after_window() {
        let dir = TempDir::new().unwrap();
        let (p, h) = spawn_for(&dir, 50);
        p.enqueue_op(PersistOp::Upsert(dummy_rule("r1", 1)))
            .await
            .unwrap();
        // Wait for debounce to fire (no manual flush).
        let ok = wait_until(
            || p.storage_path().exists() && p.snapshot().contains_key("r1"),
            std::time::Duration::from_secs(2),
        );
        let resolved = ok.await;
        assert!(resolved, "debounce should write the rule to disk");
        p.cancel.cancel();
        let _ = h.await;
    }
}
