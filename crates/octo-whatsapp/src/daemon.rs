//! Long-lived daemon. Owns the adapter, the unix-socket server, the
//! event router stub, and the shared stoolap handle.

use parking_lot::Mutex as SyncMutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::adapter_trait::OctoWhatsAppAdapter;
use crate::audit::AuditLog;
use crate::config::WhatsAppRuntimeConfig;
use crate::events_persister::EventsBuffer;
use crate::ipc::handlers::clients::McpClientRegistry;
use crate::media_buffer::MediaBuffer;
use crate::observability::metrics::Metrics;
use crate::rules::{
    persister as rules_persister_mod, MutationRateLimiter, RuleStore, RulesPersister,
};
use crate::security::TokenStore;
use crate::triggers::TriggerStore;

use octo_whatsapp_onboard_core::{AccountEntry, CoreError, MultiAccountStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonPhase {
    Booting,
    Connected,
    SessionLost,
    ShuttingDown,
}

/// 7-variant BotState mirror (spec compliance F18 — R1 review).
/// The runtime does NOT own a `wacore` adapter; this enum is a
/// runtime-side mirror updated by the connection watcher when the
/// adapter transitions. `status.get` reads this and returns the
/// variant name verbatim per design §Readiness "7-variant BotState
/// verbatim".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BotStateMirror {
    #[default]
    Disconnected,
    PairingQr,
    PairingCode,
    Connected,
    Replaced,
    LoggedOut,
    SessionExpired,
}

fn encode_bot_state(bs: BotStateMirror) -> u8 {
    match bs {
        BotStateMirror::Disconnected => 0,
        BotStateMirror::PairingQr => 1,
        BotStateMirror::PairingCode => 2,
        BotStateMirror::Connected => 3,
        BotStateMirror::Replaced => 4,
        BotStateMirror::LoggedOut => 5,
        BotStateMirror::SessionExpired => 6,
    }
}

fn decode_bot_state(v: u8) -> BotStateMirror {
    match v {
        1 => BotStateMirror::PairingQr,
        2 => BotStateMirror::PairingCode,
        3 => BotStateMirror::Connected,
        4 => BotStateMirror::Replaced,
        5 => BotStateMirror::LoggedOut,
        6 => BotStateMirror::SessionExpired,
        _ => BotStateMirror::Disconnected,
    }
}

/// Shared, cheaply-cloneable handle to daemon state.
#[derive(Clone, Debug)]
pub struct DaemonHandle {
    inner: Arc<DaemonInner>,
}

struct DaemonInner {
    config: WhatsAppRuntimeConfig,
    cancel: CancellationToken,
    /// Synchronous `std::sync::RwLock<DaemonPhase>`.
    ///
    /// Reads from RPC handlers (status/health/etc.) are always under
    /// `try_read` so they are non-blocking; writes from the supervisor and
    /// the `shutdown` handler are instantaneous. We deliberately avoid
    /// `tokio::sync::RwLock` here because:
    ///
    /// 1. RPC handlers must be callable from a tokio runtime context but
    ///    not block it (`blocking_read` panics inside `#[tokio::test]`),
    /// 2. the daemon's status reply path needs a snapshot, not a future.
    phase: std::sync::RwLock<DaemonPhase>,
    /// Concurrency-capped scratch disk for outbound media uploads.
    media_buffer: MediaBuffer,
    /// Bound adapter (live or `MockAdapter`).
    ///
    /// Dispatch path: RPC handlers call
    /// `h.adapter()?.send_audio_checked(...)`. `send_audio_checked`
    /// exists as both an inherent method on `WhatsAppWebAdapter` and an
    /// `OctoWhatsAppAdapter` trait method here, so handlers compile
    /// unchanged against either the concrete type or this trait object.
    ///
    /// `std::sync::RwLock` (not `tokio::sync::RwLock`) for the same
    /// reasons as `phase` above — RPC handlers must not block a tokio
    /// runtime, and writes (bind / unbind) are instantaneous.
    adapter: std::sync::RwLock<Option<Arc<dyn OctoWhatsAppAdapter>>>,
    /// Phase 3: in-memory events ring buffer. Populated by the event
    /// router (sinks receive InboundEvent) and queried by
    /// `events.list/show/replay` handlers.
    events_buffer: Arc<EventsBuffer>,
    /// Spec compliance F18 (R1 review): 7-variant `BotState` mirror,
    /// encoded as `AtomicU8` for lock-free reads. Encoding:
    /// 0=Disconnected, 1=PairingQr, 2=PairingCode, 3=Connected,
    /// 4=Replaced, 5=LoggedOut, 6=SessionExpired. 7+ are reserved.
    bot_state: std::sync::atomic::AtomicU8,
    /// Phase 3: MCP client registry for agent discovery
    /// (`clients.list` returns a snapshot of this).
    clients: McpClientRegistry,
    /// Phase 4: rules engine. `ArcSwap<Ruleset>` + cooldown map +
    /// mutation rate-limiter (per-caller 10/min).
    rules: Arc<RuleStore>,
    /// Per-caller rate-limiter for `rules.create|update|patch|delete`.
    mutation_rl: Arc<MutationRateLimiter>,
    /// Phase 4: triggers registry (ArcSwap-backed, same shape as rules).
    triggers: Arc<TriggerStore>,
    /// Phase 4: audit log with SHA-256 hash chain + ring buffer.
    audit_log: Arc<AuditLog>,
    /// Phase 5 Part A: bearer-token store with rotation, grace period,
    /// and revocation list. Initialized from `[security] bearer_token_env`
    /// (best-effort: missing env var leaves the store empty) and
    /// persists grace entries to `[security] grace_path`.
    tokens: Arc<TokenStore>,
    /// Phase 5 Part C: disk persister for rules (debounced atomic
    /// writes to `rules.toml` + WAL). Optional — `None` only when
    /// the persister was disabled at startup. The `JoinHandle` is
    /// owned separately by `Daemon` (not stored here).
    rules_persister: Option<Arc<RulesPersister>>,
    /// Phase 5 Part B: Prometheus registry (14 metrics). Cheap to
    /// share via `Arc`; handlers increment counters via the helper
    /// accessors below.
    metrics: Arc<Metrics>,
    /// Phase 5 Part B: liveness flag for HTTP `/health`. Set true
    /// after the daemon has finished the bind phase + spawned the
    /// IPC server; cleared on shutdown. Read by the axum
    /// `/health` route.
    is_live: Arc<AtomicBool>,
    /// Phase 5 Part B: readiness flag for HTTP `/ready`. Reflects
    /// `connected && session_valid`. Set/cleared by the connection
    /// watcher at the same boundary as `DaemonPhase`.
    is_ready: Arc<AtomicBool>,
    /// Phase 5 Part B: Unix-epoch millis when the daemon started —
    /// used for `daemon_uptime_seconds` updates.
    started_at_unix_ms: AtomicI64,
    /// Phase 5 Part F: shared `reqwest::Client` for the webhook action
    /// dispatcher. Constructed once at boot with a 10s timeout and a
    /// shared connection pool; all `Webhook` action dispatches reuse
    /// this client to amortize TLS handshakes and respect the
    /// Rustls-only default features.
    http_client: reqwest::Client,
    /// Phase 6.12.4: connection-watcher task handle. Spawned when an
    /// adapter is bound (live test fixture or production `start`
    /// path); awaitable during shutdown so the watcher doesn't outlive
    /// the daemon. `None` when no adapter is bound — typical during
    /// very early boot or hermetic tests that never bind.
    connection_watcher: SyncMutex<Option<tokio::task::JoinHandle<()>>>,
    /// Phase 6.1 T6.1.2: multi-account index store. Opened at
    /// startup via `MultiAccountStore::open_default()` —
    /// best-effort: if the call fails (e.g. HOME unset and
    /// `dirs::home_dir()` returns `None`), the daemon still starts
    /// with `None` here, and `accounts().use_account()` will return
    /// a `CoreError` from the guard. Read-only accessors (`list`,
    /// `info`) degrade silently to empty Vec / `None` so handlers
    /// never panic on a missing index file.
    accounts: parking_lot::Mutex<Option<MultiAccountStore>>,
}

impl std::fmt::Debug for DaemonInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let adapter_bound = self.adapter.read().map(|g| g.is_some()).unwrap_or(false);
        f.debug_struct("DaemonInner")
            .field("config", &self.config)
            .field("cancel", &self.cancel)
            .field("phase", &"*DaemonPhase (locked)")
            .field("media_buffer", &"MediaBuffer { .. }")
            .field(
                "adapter",
                &if adapter_bound {
                    "Some(Arc<dyn OctoWhatsAppAdapter>)"
                } else {
                    "None"
                },
            )
            .field(
                "events_buffer",
                &format_args!("EventsBuffer{{ len={} }}", self.events_buffer.len()),
            )
            .field("clients", &self.clients.count())
            .finish()
    }
}

impl DaemonHandle {
    /// Snapshot read of the current lifecycle phase. Falls back to
    /// `Booting` only if the underlying lock is contended AND poisoned
    /// — under normal operation the read always succeeds.
    pub fn phase(&self) -> DaemonPhase {
        match self.inner.phase.try_read() {
            Ok(g) => *g,
            Err(std::sync::TryLockError::WouldBlock) => {
                // A writer is mid-transition (microsecond-scale). Retry
                // once with the blocking reader: writers are instantaneous
                // so this never stalls a tokio runtime in practice.
                *self.inner.phase.read().unwrap_or_else(|p| p.into_inner())
            }
            Err(std::sync::TryLockError::Poisoned(p)) => *p.into_inner(),
        }
    }

    pub fn config(&self) -> &WhatsAppRuntimeConfig {
        &self.inner.config
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }

    /// Media buffer (concurrency-capped scratch disk) used by all
    /// outbound media RPC handlers. Acquired slots live as long as the
    /// returned `MediaSlot`, releasing back to the pool on drop.
    pub fn media_buffer(&self) -> &MediaBuffer {
        &self.inner.media_buffer
    }

    /// Bound adapter, if any. Runtime RPC handlers consult this for
    /// every outbound call; pre-flight checks happen BEFORE this lookup
    /// so ceiling tests don't need a live adapter.
    ///
    /// Returns a `dyn OctoWhatsAppAdapter` trait object so callers can
    /// swap in a `MockAdapter` (under `feature = "test-helpers"`) without
    /// instantiating a live WhatsApp Web session. The concrete
    /// `WhatsAppWebAdapter` impl in `crate::adapter_trait` satisfies the
    /// trait, so production code is unchanged.
    pub fn adapter(&self) -> Option<Arc<dyn OctoWhatsAppAdapter>> {
        self.inner
            .adapter
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Async-marked for API symmetry with future async setters, but the
    /// underlying lock is sync (`std::sync::RwLock`) so this only does a
    /// single instantaneous write. The crate's
    /// `#![warn(clippy::await_holding_lock)]` does not bite: this is a
    /// terminal op, not a held-across-await pattern.
    pub async fn set_phase(&self, p: DaemonPhase) {
        let mut g = self.inner.phase.write().unwrap_or_else(|p| p.into_inner());
        *g = p;
        // Phase 5 Part B: keep `bot_state` and `is_live` in sync
        // with the canonical phase. Cheap & instantaneous.
        let state_label = match p {
            DaemonPhase::Booting => "booting",
            DaemonPhase::Connected => "connected",
            DaemonPhase::SessionLost => "reconnecting",
            DaemonPhase::ShuttingDown => "shutting_down",
        };
        self.inner.metrics.set_bot_state(state_label);
    }

    /// Phase 5 Part B: set the readiness flag (HTTP `/ready`).
    /// True when the adapter is bound AND session_valid.
    pub fn set_ready(&self, ready: bool) {
        self.inner.is_ready.store(ready, Ordering::SeqCst);
        self.inner.metrics.set_connected(ready);
    }

    /// Phase 5 Part B: set the liveness flag (HTTP `/health`).
    /// True while the process is up and the IPC listener is bound.
    pub fn set_live(&self, live: bool) {
        self.inner.is_live.store(live, Ordering::SeqCst);
    }

    /// Bind an adapter to the daemon. Sync write — call before any
    /// await point. Mirrors the sync `RwLock` pattern of `set_phase`.
    ///
    /// Phase 6.12.4: de-gated from `cfg(test)` so production can use
    /// it too. The companion connection-watcher task is spawned here
    /// when the adapter exposes `subscribe_raw_events()` (default
    /// `None` on `MockAdapter`, real broadcast on `WhatsAppWebAdapter`).
    pub fn bind_adapter(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
        *self
            .inner
            .adapter
            .write()
            .unwrap_or_else(|p| p.into_inner()) = Some(a.clone());

        // Spawn the connection-watcher if the adapter exposes a raw
        // event stream. `MockAdapter` returns `None` (default trait
        // impl) — the watcher silently no-ops in that case, which is
        // exactly what hermetic tests want (no real WA lifecycle).
        let Some(rx) = a.subscribe_raw_events() else {
            return;
        };
        let cancel = self.inner.cancel.clone();
        let handle = self.clone();
        let join = tokio::spawn(async move {
            run_connection_watcher(rx, handle, cancel).await;
        });
        // Replace any prior watcher (single-bind-per-daemon assumption;
        // multi-bind would leak old tasks — documented limitation).
        // The `blocking_lock` here is fine because the call site is a
        // synchronous setup function, not on a hot RPC path.
        let mut slot = self.inner.connection_watcher.lock();
        if let Some(prev) = slot.take() {
            prev.abort();
        }
        *slot = Some(join);
    }

    /// Deprecated alias. Use `bind_adapter` instead.
    #[deprecated(note = "renamed to bind_adapter; will be removed in Phase 6.4")]
    pub fn set_adapter_for_tests(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
        self.bind_adapter(a);
    }

    /// Phase 3: read access to the in-memory events ring buffer. The
    /// event router populates this; `events.list/show/replay` RPC
    /// handlers consult it.
    pub fn events_buffer(&self) -> &Arc<EventsBuffer> {
        &self.inner.events_buffer
    }

    /// Spec compliance F18 (R1 review): 7-variant BotState mirror.
    /// Updated by `set_bot_state`; read by `status.get` and the
    /// `BotState → error code` mapping for the 3-way SessionLost
    /// split (Findings F4 — Spec). Defaults to `Disconnected` until
    /// the connection watcher fires.
    pub fn bot_state(&self) -> BotStateMirror {
        decode_bot_state(
            self.inner
                .bot_state
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// Set the BotState mirror. Called by the connection watcher on
    /// each `Connection` event.
    pub fn set_bot_state(&self, bs: BotStateMirror) {
        self.inner
            .bot_state
            .store(encode_bot_state(bs), std::sync::atomic::Ordering::Relaxed);
    }

    /// Phase 3: read access to the MCP client registry. `clients.list`
    /// RPC handler snapshots this; future `clients.subscribe` will
    /// register/unregister entries here.
    pub fn clients(&self) -> &McpClientRegistry {
        &self.inner.clients
    }

    /// Phase 4: rules engine. `rules.list|get|create|update|patch|
    /// delete|enable|disable|reload|test|flush|approve` all read or
    /// mutate this store.
    pub fn rules(&self) -> &Arc<RuleStore> {
        &self.inner.rules
    }

    /// Phase 4: per-caller rate-limiter for rule mutations
    /// (`create|update|patch|delete`).
    pub fn mutation_rl(&self) -> &Arc<MutationRateLimiter> {
        &self.inner.mutation_rl
    }

    /// Phase 4: triggers registry. `triggers.list|get|create|update|
    /// delete|run` read or mutate this store.
    pub fn triggers(&self) -> &Arc<TriggerStore> {
        &self.inner.triggers
    }

    /// Phase 4: audit log with hash chain + ring buffer.
    pub fn audit_log(&self) -> &Arc<AuditLog> {
        &self.inner.audit_log
    }

    /// Phase 5 Part A: bearer-token store. `security.rotate_token`,
    /// `security.revoke_all_tokens`, and `security.list_tokens`
    /// handlers consult this store.
    pub fn tokens(&self) -> &Arc<TokenStore> {
        &self.inner.tokens
    }

    /// Phase 5 Part C: rules persister. `rules.reload` and
    /// shutdown-drain paths consult this directly.
    pub fn rules_persister(&self) -> Option<&Arc<RulesPersister>> {
        self.inner.rules_persister.as_ref()
    }

    /// Phase 5 Part B: Prometheus metrics registry. Handlers use
    /// this to increment counters; the HTTP `/metrics` endpoint
    /// renders from the same registry.
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.inner.metrics
    }

    /// Phase 5 Part B: HTTP `/health` liveness flag.
    pub fn is_live_flag(&self) -> Arc<AtomicBool> {
        self.inner.is_live.clone()
    }

    /// Phase 5 Part B: HTTP `/ready` readiness flag.
    pub fn is_ready_flag(&self) -> Arc<AtomicBool> {
        self.inner.is_ready.clone()
    }

    /// Phase 5 Part B: Unix-epoch millis at daemon boot — used to
    /// drive `daemon_uptime_seconds`.
    pub fn started_at_unix_ms(&self) -> i64 {
        self.inner.started_at_unix_ms.load(Ordering::Relaxed)
    }

    pub fn now_unix_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Phase 5 Part F: shared `reqwest::Client` used by the
    /// `Webhook` action dispatcher. Returned by `Arc` clone so the
    /// dispatcher can borrow without holding a `&DaemonHandle`.
    pub fn http_client(&self) -> &reqwest::Client {
        &self.inner.http_client
    }

    /// Phase 3: build an `EventsRouter` that subscribes to the
    /// bound adapter's `raw_event_tx` and pipes events into this
    /// handle's `events_buffer`. Returns `None` if no adapter is
    /// bound. Caller spawns `router.run(rx)` on a tokio task.
    ///
    /// Gated on `#[cfg(any(test, feature = "test-helpers"))]` for
    /// now — production wiring happens via `Daemon::start` after the
    /// adapter connects.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn build_event_router(&self) -> Option<crate::events_router::EventsRouter> {
        // Adapter may not be bound; if it is, take a broadcast::Receiver
        // for the raw event stream. (Production wiring in
        // `Daemon::start` will use a different mechanism.)
        let _adapter = self.adapter()?;
        // For the test-helpers path we don't have a real adapter
        // handle. Production callers should drive the router off
        // `adapter.subscribe_raw_events()`. Returning a router
        // without a bound source is fine for tests that exercise
        // the subscribe/fanout surface but not the broadcast source.
        Some(crate::events_router::EventsRouter::from_parts(
            self.inner.events_buffer.clone(),
            self.inner.cancel.clone(),
        ))
    }

    /// Phase 6.1 T6.1.2: borrow the multi-account index store.
    /// Returns a guard that exposes `list` / `info` / `use_account`
    /// without leaking the underlying `parking_lot::Mutex`. The
    /// lock is held for the lifetime of the returned guard; inner
    /// ops do blocking filesystem I/O (the index file is read/written
    /// synchronously by `MultiAccountStore`), so handlers should NOT
    /// hold the guard across an `.await`.
    pub fn accounts(&self) -> AccountStoreGuard<'_> {
        AccountStoreGuard {
            inner: self.inner.accounts.lock(),
        }
    }
}

/// Phase 6.1 T6.1.2: thin wrapper that exposes `MultiAccountStore`
/// methods through `&self` and `&mut self`, without leaking the
/// `parking_lot::Mutex` internals to handlers. Read-only methods
/// (`list`, `info`) degrade silently when the underlying store
/// failed to initialize; mutating methods (`use_account`) return a
/// `CoreError::InvalidSessionPath` so callers can react uniformly.
pub struct AccountStoreGuard<'a> {
    inner: parking_lot::MutexGuard<'a, Option<MultiAccountStore>>,
}

impl std::fmt::Debug for AccountStoreGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountStoreGuard")
            .field("initialized", &self.inner.is_some())
            .finish()
    }
}

impl<'a> AccountStoreGuard<'a> {
    /// List every account in the index, sorted by `account_id`.
    /// Returns an empty Vec if the store failed to initialize at
    /// boot (e.g. unwriteable `~/.local/share/octo/whatsapp/`).
    pub fn list(&self) -> Vec<AccountEntry> {
        self.inner.as_ref().map(|s| s.list()).unwrap_or_default()
    }

    /// Look up one account by `account_id`. Returns `None` if not in
    /// the index OR if the store failed to initialize — handlers
    /// cannot tell the two apart and should treat both identically.
    pub fn info(&self, account_id: &str) -> Option<AccountEntry> {
        self.inner.as_ref().and_then(|s| s.get(account_id).cloned())
    }

    /// Mark `account_id` as the active account. Returns the entry on
    /// success; returns `CoreError::InvalidSessionPath` if the store
    /// failed to initialize or `MultiAccountStore::use_account`
    /// rejects the id (unknown / session path missing).
    pub fn use_account(&mut self, account_id: &str) -> Result<AccountEntry, CoreError> {
        let store = self
            .inner
            .as_mut()
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: std::path::PathBuf::from("(no MultiAccountStore)"),
                reason: "store not initialized (MultiAccountStore::open_default failed at boot)"
                    .to_string(),
            })?;
        store.use_account(account_id)
    }
}

pub struct Daemon {
    config: WhatsAppRuntimeConfig,
    cancel: CancellationToken,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon")
            .field("name", &self.config.name)
            .field("cancelled", &self.cancel.is_cancelled())
            .finish()
    }
}

impl Daemon {
    /// Build a new `Daemon` from a validated [`WhatsAppRuntimeConfig`].
    pub fn new(config: WhatsAppRuntimeConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
        }
    }

    /// Phase 5 Part B: canonical API version string. Bumped from
    /// `1.0.0+phase4` when Part A landed. The phase suffix
    /// communicates to operators which observability/security
    /// surfaces are guaranteed to exist.
    pub const fn version() -> &'static str {
        "1.0.0+phase5"
    }

    pub fn handle(&self) -> DaemonHandle {
        let media_buffer = MediaBuffer::new(
            self.config.media_buffer.max_concurrent_uploads,
            self.config.media_buffer.root.clone(),
        );
        let events_buffer = EventsBuffer::new(self.config.events.max_rows);
        // Phase 5 Part B: Prometheus registry materialized first so
        // we can attach it to AuditLog / RuleStore / TriggerStore.
        let label_secret = self
            .config
            .observability
            .metrics
            .label_hash_secret
            .as_deref()
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_else(random_label_secret);
        let metrics = Metrics::new(&label_secret).expect("Metrics::new is infallible in practice");
        // Initial bot_state = booting so the metric has a sample even
        // before the first `set_phase(Connected)`.
        metrics.set_bot_state("booting");
        metrics.set_connected(false);
        let audit_log = AuditLog::new(
            self.config.security.audit_max_rows,
            self.config.security.audit_anchor_every,
        )
        .with_metrics(metrics.clone());
        let mutation_rl = Arc::new(MutationRateLimiter::new(10)); // 10/min per caller
        let trigger_store = Arc::new(TriggerStore::new().with_metrics(metrics.clone()));
        // Phase 5 Part A: TokenStore. Default grace_path is
        // `$data_dir/tokens/grace.json` if the user did not override.
        let grace_path = self
            .config
            .security
            .grace_path
            .clone()
            .unwrap_or_else(|| self.config.data_dir.join("tokens").join("grace.json"));
        let tokens = Arc::new(TokenStore::new(
            Some(grace_path),
            self.config.security.grace_period_ms,
        ));
        // Best-effort initial load: env var unset leaves the store empty
        // (hermetic tests). Env var set with malformed contents logs a
        // warning via the descriptor's `label`.
        let _ = tokens.load_from_env(&self.config.security.bearer_token_env, Some("bootstrap"));
        let _ = tokens.load_grace();
        // Phase 5 Part C: rules persistence. The persister writes
        // the ruleset atomically to `rules.toml` with a SHA-256
        // chained WAL. The JoinHandle lives outside `DaemonInner`
        // (in `Daemon`) so the supervisor can await the actor's
        // exit during shutdown drain.
        let storage_path = self.config.rules.resolved_storage_path();
        let wal_path = self.config.rules.resolved_wal_path();
        if let Some(parent) = storage_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        if let Some(parent) = wal_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let (rules_persister, persister_handle) =
            RulesPersister::spawn(storage_path, wal_path, self.config.rules.debounce_ms);
        // Side-channel: stash the JoinHandle so we can await it on
        // shutdown. The handle is light (one tokio JoinHandle) —
        // owned by the supervisor task spawned by `Daemon::run`.
        PERSISTER_HANDLES.with(|cell| {
            let mut g = cell.borrow_mut();
            g.push(persister_handle);
        });
        // Seed: load any pre-existing rules.toml from disk and
        // inject into both the RuleStore (swap) and the
        // persister's snapshot map.
        let loaded_rules = load_initial_rules_from_disk(
            self.config.rules.resolved_storage_path(),
            rules_persister.clone(),
        );
        let rs = RuleStore::new(self.config.security.auto_approve_rules)
            .with_metrics(metrics.clone())
            .with_persister(rules_persister.clone());
        if !loaded_rules.is_empty() {
            let arcs: Vec<Arc<crate::rules::Rule>> =
                loaded_rules.into_iter().map(Arc::new).collect();
            rs.replace_all(arcs);
        }
        let rule_store = Arc::new(rs);
        let started_at_unix_ms = unix_epoch_ms_now();
        let is_live = Arc::new(AtomicBool::new(false));
        let is_ready = Arc::new(AtomicBool::new(false));
        // Phase 6.1 T6.1.2: open the multi-account index store.
        // Best-effort: `open_default()` resolves
        // `~/.local/share/octo/whatsapp/index.json` via
        // `dirs::home_dir()` and creates an empty in-memory index
        // when the file is absent. If the call errors out (e.g.
        // HOME unset, unwritable data dir), the daemon still
        // starts: handlers will report a `CoreError` for mutating
        // ops and empty results for read-only ops.
        let accounts = match MultiAccountStore::open_default() {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "MultiAccountStore::open_default failed; daemon starts without accounts API"
                );
                None
            }
        };
        // Phase 5 Part F: build a shared `reqwest::Client` with
        // conservative defaults suitable for webhook dispatches.
        // 10s connect timeout, 30s request timeout. Constructed once
        // at boot; rule handlers MUST reuse this via
        // `DaemonHandle::http_client()`.
        let http_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!("octo-whatsapp/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest::Client::builder build is infallible in practice");
        DaemonHandle {
            inner: Arc::new(DaemonInner {
                config: self.config.clone(),
                cancel: self.cancel.clone(),
                phase: std::sync::RwLock::new(DaemonPhase::Booting),
                media_buffer,
                adapter: std::sync::RwLock::new(None),
                events_buffer,
                bot_state: std::sync::atomic::AtomicU8::new(0), // Disconnected
                clients: McpClientRegistry::new(),
                rules: rule_store,
                mutation_rl,
                triggers: trigger_store,
                audit_log: Arc::new(audit_log),
                tokens,
                rules_persister: Some(rules_persister),
                metrics,
                is_live,
                is_ready,
                started_at_unix_ms: AtomicI64::new(started_at_unix_ms),
                http_client,
                connection_watcher: SyncMutex::new(None),
                accounts: parking_lot::Mutex::new(accounts),
            }),
        }
    }

    /// Clone of the daemon's cancellation token. Used by tests and by
    /// supervisor code to trigger shutdown without holding `&Daemon`.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub async fn run(self) -> anyhow::Result<()> {
        info!(name = self.config.name.as_str(), "daemon: starting");

        let cancel = self.cancel.clone();
        let handle = self.handle();

        // Correctness review NIT F34: append a `daemon.started` audit
        // row so operators tailing the audit log can see the boot
        // event. Caller uid/pid are recorded as the supervisor's
        // process info (not a real RPC caller).
        handle.audit_log().record(crate::audit::AuditEntryInput {
            ts_unix_ms: crate::security::tokens::now_unix_ms(),
            ts_mono_ns: 0,
            caller_uid: format!("supervisor:{}", std::process::id()),
            caller_pid: std::process::id(),
            method: "daemon.started".to_string(),
            args_canonical_sha256: String::new(),
            result_status: "ok".to_string(),
            latency_ms: 0,
        });

        let registry = std::sync::Arc::new(crate::ipc::handlers::build_registry());
        let sock = self.config.socket_path();
        let server = crate::ipc::server::UnixSocketServer::bind(&sock)?;
        let server_task = {
            let cancel = cancel.clone();
            let handle = handle.clone();
            tokio::spawn(async move { server.serve(handle, registry, cancel).await })
        };

        // Phase 5 Part B: spin up the HTTP health server (if
        // configured) + flip `is_live = true` once the daemon has
        // both the unix socket bound AND, optionally, an HTTP
        // surface answering.
        let _health_handle = match self.config.observability.health.http_listen.as_deref() {
            Some(addr_str) => {
                let addr: std::net::SocketAddr = addr_str.parse().map_err(|e| {
                    anyhow::anyhow!("observability.health.http_listen {addr_str:?} invalid: {e}")
                })?;
                let bearer = crate::observability::health_server::BearerConfig::from_env_or_config(
                    &self.config.observability.metrics.bearer_token_env,
                    self.config.security.bearer_required,
                );
                let is_live = handle.is_live_flag();
                let is_ready = handle.is_ready_flag();
                let metrics = handle.metrics().clone();
                let cancel_clone = cancel.clone();
                let bind_result = crate::observability::run_health_server(
                    addr,
                    metrics,
                    is_ready,
                    is_live,
                    bearer,
                    cancel_clone,
                )
                .await;
                match bind_result {
                    Ok(h) => Some(h),
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "daemon: health server failed to start; continuing without /health/ready/metrics"
                        );
                        None
                    }
                }
            }
            None => None,
        };
        handle.set_live(true);
        info!("daemon: liveness set; server accepting");

        // Phase 5 Part C: spawn a small task that listens for
        // SIGHUP and triggers a non-blocking `rules.reload`. On
        // non-unix targets the watcher is a no-op.
        let sighup_handle = spawn_sighup_watcher(cancel.clone(), handle.clone());

        cancel.cancelled().await;
        info!("daemon: cancel observed; waiting for server to drain");
        handle.set_live(false);
        handle.set_ready(false);
        // Phase 5 Part C: drain the rules persister before
        // returning so any pending writes hit disk.
        if let Some(p) = handle.rules_persister() {
            let _ = p.flush_sync().await;
        }
        let _ = sighup_handle.await;
        crate::daemon::drain_persister_handles().await;
        let _ = server_task.await;
        info!("daemon: exited");
        Ok(())
    }
}

/// Spawn the SIGHUP watcher task. On unix platforms we install a
/// `tokio::signal::unix` listener for SIGHUP and call
/// `DaemonHandle::rules()` reload on each. On non-unix the watcher
/// is a no-op future.
#[cfg(unix)]
fn spawn_sighup_watcher(
    cancel: tokio_util::sync::CancellationToken,
    handle: DaemonHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sig = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "daemon: cannot install SIGHUP listener");
                return;
            }
        };
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = sig.recv() => {
                    tracing::info!("daemon: SIGHUP received — reloading rules");
                    // Synthesize a `rules.reload` invocation by
                    // calling the handler logic directly. This
                    // avoids requiring the full RPC roundtrip.
                    let previous = handle.rules().list();
                    let storage_path = handle.config().rules.resolved_storage_path();
                    let bytes = match tokio::fs::read(&storage_path).await {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!(error = %e, "sighup: cannot read rules.toml");
                            continue;
                        }
                    };
                    let text = match std::str::from_utf8(&bytes) {
                        Ok(s) => s,
                        Err(_) => {
                            tracing::warn!("sighup: rules.toml not UTF-8");
                            continue;
                        }
                    };
                    let set: crate::rules::PersistedRuleset = match toml::from_str(text) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(error = %e, "sighup: rules.toml parse");
                            continue;
                        }
                    };
                    let rules = set.into_rules();
                    let valid: Vec<std::sync::Arc<crate::rules::Rule>> = rules
                        .into_iter()
                        .filter(crate::rules::validate_persisted_rule)
                        .map(std::sync::Arc::new)
                        .collect();
                    handle.rules().replace_all(valid.clone());
                    tracing::info!(
                        prev = previous.len(),
                        new = valid.len(),
                        "sighup: rules reloaded"
                    );
                }
            }
        }
    })
}

#[cfg(not(unix))]
fn spawn_sighup_watcher(
    _cancel: tokio_util::sync::CancellationToken,
    _handle: DaemonHandle,
) -> tokio::task::JoinHandle<()> {
    // Non-unix: no-op watcher.
    tokio::spawn(async move {})
}

/// Generate a per-process random 32-byte secret for label-hash
/// isolation across restart cycles. Operationally this means label
/// hashes change across restarts (intended: bounded cardinality is
/// the only invariant; operators pinning `label_hash_secret` get a
/// stable secret instead).
fn random_label_secret() -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut bytes = [0u8; 32];
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    bytes[..16].copy_from_slice(&nanos.to_le_bytes());
    bytes[16..].copy_from_slice(&pid.to_le_bytes());
    bytes.to_vec()
}

fn unix_epoch_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- Phase 5 Part C: persister plumbing ----

thread_local! {
    /// Per-thread stash of `JoinHandle`s for rules-persister
    /// background tasks. The daemon fires off one persister per
    /// `Daemon::handle()` call; the supervisor awaits each handle
    /// during shutdown so the persister can drain.
    ///
    /// We use a thread_local rather than storing the handle inside
    /// `DaemonInner` because `Arc<DaemonInner>` is shared with the
    /// IPC handlers; moving the handle there would force `'static`
    /// bound on every handler that needs to read it. The thread
    /// local is owned by the supervisor thread that called
    /// `Daemon::handle()` (typically the runtime entry point).
    static PERSISTER_HANDLES: std::cell::RefCell<Vec<tokio::task::JoinHandle<()>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Drain the per-thread `PERSISTER_HANDLES`. Cancels each handle's
/// cancellation token and awaits its completion. Called by the
/// daemon's `run` shutdown sequence.
pub(crate) async fn drain_persister_handles() {
    PERSISTER_HANDLES.with(|cell| {
        let mut g = cell.borrow_mut();
        for h in g.drain(..) {
            h.abort();
        }
    });
}

/// Read `rules.toml` from disk (if present) and return the parsed
/// rules. Used at startup to seed the in-memory store. Validation
/// follows the same path as `rules.reload`. Missing/empty/corrupt
/// files yield an empty `Vec` (logged, not failed).
fn load_initial_rules_from_disk(
    storage_path: std::path::PathBuf,
    persister: Arc<crate::rules::RulesPersister>,
) -> Vec<crate::rules::Rule> {
    use crate::rules::{PersistedRuleset, RulesPersister};
    let bytes = match std::fs::read(&storage_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!(error = %e, "rules_persister: cannot read rules.toml at startup");
            return Vec::new();
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("rules_persister: rules.toml is not UTF-8; treating as empty");
            return Vec::new();
        }
    };
    let set: PersistedRuleset = match toml::from_str(text) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "rules_persister: rules.toml is malformed; ignoring");
            return Vec::new();
        }
    };
    let rules = set.into_rules();
    // Re-validate every predicate (ReDoS classification) before
    // admitting — a malformed entry should not crash the daemon.
    let validated: Vec<crate::rules::Rule> = rules
        .into_iter()
        .filter(crate::rules::validate_persisted_rule)
        .collect();
    // Seed the persister's in-memory snapshot so subsequent upserts
    // reflect the loaded baseline.
    RulesPersister::seed_snapshot_static(&persister, validated.clone());
    // Correctness review F7/F32/F37: restore `next_seq` to the
    // highest valid seq in the existing WAL so a restart does not
    // collide with prior chain entries. `seed_snapshot` resets the
    // counter to 1; we bump it back from disk.
    let wal_path = storage_path.with_file_name(
        storage_path
            .file_name()
            .map(|s| {
                let mut s = s.to_os_string();
                s.push(".wal");
                s
            })
            .unwrap_or_default(),
    );
    if let Ok(max) = rules_persister_mod::max_wal_seq(&wal_path) {
        if max > 0 {
            persister.bump_seq(max + 1);
        }
    }
    validated
}

// ===========================================================================
// Connection watcher (Phase 6.12.4)
// ===========================================================================
//
// Consumes the adapter's raw event stream (typically `format!("{:?}",
// event)` from the underlying SDK) and translates lifecycle variants
// into `BotStateMirror` transitions on the daemon's atomic state. This
// is what makes `status.get` truthful when the bot gets logged out,
// replaced, or expires mid-life — without the watcher the cached
// "Connected" state would mask the failure.
//
// The classifier matches the first identifier after `Event::` because
// the SDK's `Debug` impl is stable for the 7 lifecycle variants we
// care about. Non-lifecycle events (Message, Receipt, ...) fall
// through with `None` and the watcher simply ignores them.

/// Map a raw `format!("{:?}", event)` string to a `BotStateMirror`
/// transition. Returns `None` for non-lifecycle events.
fn classify_event(raw: &str) -> Option<(BotStateMirror, bool)> {
    // `Event::Connected(_)`, `Event::LoggedOut(LoggedOutCause { ... })`, ...
    let rest = raw.strip_prefix("Event::")?;
    let ident = rest.split(['(', ' ', '{', '<']).next()?;
    match ident {
        // `phase_changed = true` means the caller should also flip
        // `DaemonPhase` (Connected ↔ SessionLost). Pairing/PairingQr
        // don't move phase — boot is in progress, daemon is already
        // in Booting/SessionLost.
        "Connected" => Some((BotStateMirror::Connected, true)),
        "Disconnected" => Some((BotStateMirror::Disconnected, true)),
        "PairingQr" => Some((BotStateMirror::PairingQr, false)),
        "PairingCode" => Some((BotStateMirror::PairingCode, false)),
        "LoggedOut" => Some((BotStateMirror::LoggedOut, true)),
        "Replaced" => Some((BotStateMirror::Replaced, true)),
        "SessionExpired" => Some((BotStateMirror::SessionExpired, true)),
        _ => None,
    }
}

/// Long-running task spawned at adapter-bind time. Loops over the
/// broadcast receiver, applies `BotStateMirror` transitions, and
/// observes cancellation. Survives `Event::Lagged` (channel overflow)
/// by continuing — events lost during overflow are non-critical.
async fn run_connection_watcher(
    mut rx: tokio::sync::broadcast::Receiver<String>,
    handle: DaemonHandle,
    cancel: CancellationToken,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("connection watcher: cancel observed");
                break;
            }
            recv = rx.recv() => {
                match recv {
                    Ok(raw) => {
                        if let Some((mirror, phase_changed)) = classify_event(&raw) {
                            tracing::info!(
                                bot_state = ?mirror,
                                raw = %raw,
                                "connection watcher: bot state transition"
                            );
                            handle.set_bot_state(mirror);
                            if phase_changed {
                                let phase = match mirror {
                                    BotStateMirror::Connected => DaemonPhase::Connected,
                                    _ => DaemonPhase::SessionLost,
                                };
                                handle.set_phase(phase).await;
                            }
                        }
                        // Non-lifecycle events are deliberately ignored.
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(
                            dropped = n,
                            "connection watcher: broadcast lag; some events skipped"
                        );
                        // Continue — next iteration resumes from current tip.
                    }
                    Err(RecvError::Closed) => {
                        tracing::info!(
                            "connection watcher: broadcast channel closed; exiting"
                        );
                        break;
                    }
                }
            }
        }
    }
}

/// Tests live in their own file so the unit-test surface stays narrow.
#[cfg(test)]
mod tests;
