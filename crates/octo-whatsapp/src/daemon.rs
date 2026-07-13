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
use crate::events_buffer::EventsBuffer;
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

/// 8-variant BotState mirror (spec compliance F18 — R1 review, plus
/// Session 4 of wacore-webauthn plan). The runtime does NOT own a
/// `wacore` adapter; this enum is a runtime-side mirror updated by
/// the connection watcher when the adapter transitions. `status.get`
/// reads this and returns the variant name verbatim per design
/// §Readiness "7-variant BotState verbatim" — the 8th variant is
/// `AwaitingPasskey` for SHORTCAKE_PASSKEY link flows (RFC-0909).
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
    /// Phone-side second-verification required. wacore 0.6.0 does
    /// not surface WebAuthn / passkey / 2FA-PIN events, so we detect
    /// the stall heuristically: 45s after a pairing prompt with no
    /// terminal event. Hint is a const; the operator must complete
    /// the prompt on the phone (security key, passkey, or 2FA PIN)
    /// to advance the state machine.
    AwaitingUserAction,
    /// Server requested a WebAuthn assertion (SHORTCAKE_PASSKEY
    /// link flow, RFC-0909). The phone must scan the displayed QR
    /// to drive `PasskeyAuthenticator::get_assertion`; if no
    /// authenticator is registered the operator must complete the
    /// assertion manually on the phone. Stays in this state until
    /// either `PairPasskeyConfirmation` (still waiting) or
    /// `PairPasskeyError` (terminal: `LoggedOut`) arrives.
    AwaitingPasskey,
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
        BotStateMirror::AwaitingUserAction => 7,
        BotStateMirror::AwaitingPasskey => 8,
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
        7 => BotStateMirror::AwaitingUserAction,
        8 => BotStateMirror::AwaitingPasskey,
        _ => BotStateMirror::Disconnected,
    }
}

/// Hint surfaced to operators when `BotStateMirror::AwaitingUserAction`
/// fires. Const so we never allocate; the message is the same for
/// every second-verification case the wacore SDK hides from us
/// (WebAuthn, security key, 2FA PIN, multi-device toggle, etc.).
pub const AWAITING_USER_ACTION_HINT: &str = "pairing stalled: check phone for a second-verification prompt (passkey, security key, 2FA PIN, or multi-device toggle)";

/// Hint surfaced to operators when `BotStateMirror::AwaitingPasskey`
/// fires. Shorter than `AWAITING_USER_ACTION_HINT` because the
/// SHORTCAKE_PASSKEY flow has a specific resolution path: a phone-
/// side WA app scan of the displayed QR (or a registered
/// `PasskeyAuthenticator` driving the assertion in-Rust).
pub const AWAITING_PASSKEY_HINT: &str = "server requested WebAuthn assertion (SHORTCAKE_PASSKEY): scan the QR displayed in the CLI/daemon logs with your phone's WhatsApp app to complete the link";

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
    /// Phase 8: the active [`EventsRouter`], populated by `start`
    /// (live path) or `build_event_router` (test-helpers path).
    /// Read by `status.get` to surface per-sink lagged counts.
    events_router: parking_lot::RwLock<Option<Arc<crate::events_router::EventsRouter>>>,
    /// Phase 3 Part D: optional upstream ingress to the
    /// EventsPersister actor. `None` when persistence is disabled.
    /// Holds ONLY the upstream `Sender` + stats accessor + the
    /// cancellation token used by the shutdown drain. Cloning is
    /// cheap so `bind_adapter` can wire the router sink without
    /// taking ownership of the actor's JoinHandle.
    events_persister: Option<crate::events_persister::PersisterIngress>,
    /// Phase 0 (query layer): test seam that overrides the default
    /// embedder. `None` => `LocalCandleEmbedder` scaffold. Tests
    /// inject a deterministic `MockEmbedder` via
    /// `set_query_embedder_for_tests`.
    #[cfg(all(feature = "query", any(test, feature = "test-helpers")))]
    test_embedder: std::sync::RwLock<Option<Arc<dyn crate::query::embedder::Embedder>>>,
    /// Phase 1 (query layer): lazily-populated handle to the
    /// `QuerySubsystem` so RPC handlers (daemon.search,
    /// messages.context, events.find) can read from the derived
    /// SQL + Tantivy views. Populated on first access by
    /// `query_subsystem()` if the `query` feature is on; never
    /// constructed when the feature is off.
    #[cfg(feature = "query")]
    query_subsystem: std::sync::OnceLock<Arc<crate::query::QuerySubsystem>>,
    #[cfg(feature = "query")]
    query_service: std::sync::OnceLock<Arc<crate::query::QueryService>>,
    /// Spec compliance F18 (R1 review) + Session 4 (RFC-0909):
    /// 8-variant `BotState` mirror, encoded as `AtomicU8` for
    /// lock-free reads. Encoding: 0=Disconnected, 1=PairingQr,
    /// 2=PairingCode, 3=Connected, 4=Replaced, 5=LoggedOut,
    /// 6=SessionExpired, 7=AwaitingUserAction, 8=AwaitingPasskey.
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

    /// Read access to the boot-time runtime config. Read-only borrow; the
    /// config is set once at `Daemon::new` and not mutated thereafter.
    /// Callers that need the *active* account_id should consult
    /// `accounts().info(active_id)` instead — the boot-time value here
    /// does not reflect runtime account switches.
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
    ///
    /// Phase 6.1.1.1: this is an *atomic replace* — any prior
    /// connection-watcher join-handle is aborted before the new one
    /// is stored, so successive calls (including `rebind_adapter_for`)
    /// do not leak watcher tasks. Callers that need the
    /// pre-replacement adapter reference (e.g. to log which account
    /// was just unbound) should consult `adapter()` first.
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

        // Phase 3 Part C: spawn the EventsRouter. This consumes a
        // SECOND receiver from the same broadcast (broadcast
        // supports multiple consumers) and pipes parsed
        // InboundEvents into the handle's `events_buffer` (which
        // the in-memory `events.list/show/replay` RPCs serve from)
        // and to every `EventsSink` subscriber (MCP clients,
        // rules, the persister, etc.). Without this, the buffer
        // stays empty even though raw events fire.
        if let Some(router_rx) = a.subscribe_raw_events() {
            let router_buffer = self.inner.events_buffer.clone();
            let router_cancel = self.inner.cancel.clone();
            let router = Arc::new(crate::events_router::EventsRouter::from_parts(
                router_buffer,
                router_cancel,
            ));
            // Expose the router to status.get so per-sink lagged
            // counts are visible. The router outlives this function;
            // the Arc is cheap to clone.
            *self.inner.events_router.write() = Some(router.clone());
            // Subscribe the EventsPersister as a sink before we start
            // the router so we don't lose the first events. The
            // sink forwards via the persister's `push` (try_send,
            // non-blocking).
            if let Some(persister_ingress) = self.events_persister_handle() {
                let mut sub = router.subscribe_named(4096, "persister");
                tokio::spawn(async move {
                    while let Some(ev) = sub.recv().await {
                        persister_ingress.push(ev);
                    }
                });
            }
            // Phase 0 of `docs/plans/2026-07-11-whatsapp-query-layer-design.md`:
            // wire the query subsystem (SQL mirror + Tantivy FTS +
            // embedder queue) into the same broadcast. Built only when
            // the `query` cargo feature is on.
            #[cfg(feature = "query")]
            {
                let base = match self.query_base_dir() {
                    Some(b) => b,
                    None => {
                        tracing::warn!(
                            "query feature on but no base dir configured; \
                             skipping query subsystem wiring"
                        );
                        tokio::spawn(async move {
                            router.run(router_rx).await;
                        });
                        return;
                    }
                };
                let embedder: Arc<dyn crate::query::embedder::Embedder> = self.query_embedder();
                match crate::query::open_subsystem(
                    &base,
                    embedder,
                    crate::query::JobConfig {
                        queue_capacity: self.inner.config.query.queue_capacity,
                        batch_size: self.inner.config.query.batch_size,
                        batch_window_ms: self.inner.config.query.batch_window_ms,
                    },
                ) {
                    Ok(subsystem) => {
                        tracing::info!(
                            base = %base.display(),
                            "query subsystem online"
                        );
                        let arc = Arc::new(subsystem);
                        // Install on the handle so RPC handlers
                        // (daemon.search / messages.context /
                        // events.find) can read from it. Idempotent
                        // — `install` is a no-op when something is
                        // already wired.
                        self.install_query_subsystem(arc.clone());
                        // Phase 1 task 16: hydrate derived views
                        // from the events NDJSON canonical log so
                        // a fresh daemon boots with all previously
                        // persisted events searchable. Configurable
                        // via `query.rebuild_on_boot` (default on).
                        if self.inner.config.query.rebuild_on_boot {
                            let ndjson_path = self
                                .inner
                                .config
                                .events
                                .resolved_persistence_path(&self.inner.config.data_dir);
                            match crate::query::replay_ndjson(arc.as_ref(), &ndjson_path) {
                                Ok(stats) => tracing::info!(
                                    read = stats.lines_read,
                                    handled = stats.lines_handled,
                                    failed_parse = stats.lines_failed_parse,
                                    path = %ndjson_path.display(),
                                    "query layer hydrated from NDJSON"
                                ),
                                Err(e) => tracing::warn!(
                                    error = %e,
                                    path = %ndjson_path.display(),
                                    "NDJSON replay failed; derived views start empty"
                                ),
                            }
                        } else {
                            tracing::info!("query.rebuild_on_boot = false; skipping NDJSON replay");
                        }
                        let cancel = self.inner.cancel.clone();
                        let sub = router.subscribe_named(16384, "query");
                        arc.run(sub, cancel);
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "query subsystem failed to open; broadcast \
                             continues without it"
                        );
                    }
                }
            }
            tokio::spawn(async move {
                router.run(router_rx).await;
            });
        }
        // prior connection-watcher if any, so re-binding under a
        // new account does not leak tasks. The `blocking_lock`
        // here is fine because the call site is a synchronous
        // setup function, not on a hot RPC path.
        let mut slot = self.inner.connection_watcher.lock();
        if let Some(prev) = slot.take() {
            prev.abort();
        }
        *slot = Some(join);
    }

    /// Bind an adapter AND spawn its `start_bot` on the daemon's runtime,
    /// in that order. This is the **chokepoint** that prevents the
    /// connection-watcher's subscription race: the raw-event broadcast
    /// channel emits boot-time events (Connected / LoggedOut /
    /// Replaced / SessionExpired) from inside `start_bot`'s on_event
    /// callback. If the watcher isn't subscribed yet, those events are
    /// dropped and `bot_state` / `DaemonPhase` stay at their defaults
    /// (Disconnected / Booting) regardless of what the bot actually
    /// reached.
    ///
    /// Call sites: `cli.rs::daemon` (production), and the two
    /// `live_daemon_test.rs` fixtures (`init_fixture`, `bad_fixture`).
    /// No other entry point should pair an adapter with the daemon.
    ///
    /// `start` is a closure that returns a `Future<Output = ()>`. The
    /// caller is responsible for error handling — failures are
    /// fire-and-forget, the watcher observes whatever state the bot
    /// ends up in. This is intentional: Phase 6.12.3's
    /// `live_chain_i_bad_shape_session` exercises the case where
    /// `start_bot` returns Err (or never reaches Connected); the
    /// watcher needs to surface that as `phase = session_lost` and
    /// `bot_state = LoggedOut | Replaced | SessionExpired | Disconnected`.
    ///
    /// The closure runs on the current tokio runtime (the same one
    /// that will run `Daemon::run()`). `bind_adapter` is synchronous —
    /// it spawns the watcher task before this method returns, so by
    /// the time `start` is spawned, the watcher is already in its
    /// `rx.recv()` loop awaiting the first event.
    pub fn bind_adapter_and_start<F, Fut>(&self, a: Arc<dyn OctoWhatsAppAdapter>, start: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        // Step 1: bind — spawns the connection-watcher, which calls
        // `subscribe_raw_events()` on the adapter and waits on the
        // returned broadcast::Receiver.
        self.bind_adapter(a);

        // Step 2: spawn start. By this point a Receiver is subscribed
        // to `raw_event_tx`, so any event the WA client emits during
        // the handshake is delivered to the watcher.
        tokio::spawn(start());
    }

    /// Rebind the running adapter to a new account's session path.
    ///
    /// Constructs a fresh `WhatsAppWebAdapter` from `new_session_path`
    /// (taken from the just-activated `AccountEntry`) + the current runtime
    /// config's `groups` / `sender_allowlist`, then atomically swaps the
    /// adapter slot via `bind_adapter` (which aborts the prior
    /// connection-watcher).
    ///
    /// The new adapter is NOT `start_bot()`-ed. The caller is expected
    /// to invoke `reconnect.now` afterwards to establish a fresh
    /// connection under the new account.
    pub fn rebind_adapter_for(&self, account_id: &str, new_session_path: &std::path::Path) {
        use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
        let cfg = self.config();
        let new_adapter_cfg = WhatsAppConfig {
            session_path: new_session_path.to_string_lossy().into_owned(),
            ws_url: None,
            pair_phone: None,
            pair_code: None,
            groups: cfg.groups.clone(),
            sender_allowlist: cfg.sender_allowlist.clone(),
            passkey_authenticator: None,
        };
        let new_adapter = std::sync::Arc::new(WhatsAppWebAdapter::new(new_adapter_cfg));
        tracing::info!(
            account_id,
            session = %new_session_path.display(),
            "rebinding adapter to new account"
        );
        self.bind_adapter(new_adapter);
    }

    /// Deprecated alias. Use `bind_adapter` instead.
    #[deprecated(note = "renamed to bind_adapter; will be removed in Phase 6.4")]
    pub fn set_adapter_for_tests(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
        self.bind_adapter(a);
    }

    /// Phase 3: read access to the in-memory events ring buffer. The
    /// event router populates this; `events.list/show/replay` RPC
    /// handlers consult it.
    /// Phase 3 Part D: clone of the events persister handle (or
    /// `None` if persistence is disabled). Used by `bind_adapter`
    /// to wire the router's per-sink fan-out into the persister's
    /// upstream channel. Cloning is cheap (one `mpsc::Sender`).
    pub fn events_persister_handle(&self) -> Option<crate::events_persister::PersisterIngress> {
        self.inner.events_persister.clone()
    }

    pub fn events_buffer(&self) -> &Arc<EventsBuffer> {
        &self.inner.events_buffer
    }

    /// Resolve the directory under which the query subsystem keeps
    /// its derived stores (`<data_dir>/query/`). Returns `None`
    /// when no data dir is configured (e.g. ephemeral test
    /// daemons).
    #[cfg(feature = "query")]
    fn query_base_dir(&self) -> Option<std::path::PathBuf> {
        let dir = self.inner.config.data_dir.clone();
        if dir.as_os_str().is_empty() {
            None
        } else {
            Some(dir.join("query"))
        }
    }

    /// Construct the runtime embedder. Default is the local candle
    /// scaffold (returns a fatal EmbedError until Phase 1 task 9
    /// wires the forward pass); tests inject a `MockEmbedder` via
    /// `set_query_embedder_for_tests`.
    #[cfg(feature = "query")]
    fn query_embedder(&self) -> Arc<dyn crate::query::embedder::Embedder> {
        #[cfg(all(test, feature = "test-helpers"))]
        {
            if let Ok(guard) = self.inner.test_embedder.read() {
                if let Some(forced) = guard.as_ref() {
                    return forced.clone();
                }
            }
        }
        // `LocalCandleEmbedder::new()` resolves the cache directory
        // but does not download weights — the actual forward pass
        // arrives in Phase 1 task 9. If construction fails entirely
        // (e.g. missing HOME), fall back to a `MockEmbedder` so the
        // broadcast path still functions.
        match crate::query::embedder::LocalCandleEmbedder::new() {
            Ok(e) => Arc::new(e),
            Err(_) => Arc::new(crate::query::embedder::MockEmbedder::ok("fallback", 384)),
        }
    }

    /// Test seam: inject a deterministic embedder.
    #[cfg(all(feature = "query", any(test, feature = "test-helpers")))]
    pub fn set_query_embedder_for_tests(&self, e: Arc<dyn crate::query::embedder::Embedder>) {
        if let Ok(mut g) = self.inner.test_embedder.write() {
            *g = Some(e);
        }
    }

    /// Install the QuerySubsystem handle that RPC handlers read
    /// from. Called by the live wiring at boot; tests inject a
    /// hermetic instance directly. Returns `false` if a subsystem
    /// was already installed (OnceLock semantics — first call wins).
    #[cfg(feature = "query")]
    pub fn install_query_subsystem(&self, s: Arc<crate::query::QuerySubsystem>) -> bool {
        let inserted = self.inner.query_subsystem.set(s.clone()).is_ok();
        if inserted {
            let svc = Arc::new(crate::query::QueryService::new(
                s.tantivy_arc(),
                s.ingester_arc(),
            ));
            let _ = self.inner.query_service.set(svc);
        }
        inserted
    }

    /// Borrow the QuerySubsystem (live or test-injected). Returns
    /// `None` if no subsystem has been installed.
    #[cfg(feature = "query")]
    pub fn query_subsystem(&self) -> Option<Arc<crate::query::QuerySubsystem>> {
        self.inner.query_subsystem.get().cloned()
    }

    /// Borrow the QueryService view (Tantivy BM25 + SQL filters).
    /// Returns `None` if no subsystem is installed.
    #[cfg(feature = "query")]
    pub fn query_service(&self) -> Option<Arc<crate::query::QueryService>> {
        self.inner.query_service.get().cloned()
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

    /// Phase 8: borrow the active [`EventsRouter`] (or `None` if
    /// the daemon hasn't bound an adapter yet). Read by
    /// `status.get` to surface per-sink lagged counts.
    pub fn events_router(&self) -> Option<Arc<crate::events_router::EventsRouter>> {
        self.inner.events_router.read().clone()
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
    handle: DaemonHandle,
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
    /// Production constructor. Opens the default multi-account store
    /// (best-effort; logs warning on failure, store stays None).
    pub fn new(config: WhatsAppRuntimeConfig) -> Self {
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
        Self::new_internal(config, accounts)
    }

    /// Hermetic test constructor. Builds a Daemon whose filesystem
    /// paths (data_dir, socket_dir, MultiAccountStore index, rules.toml
    /// + wal, media buffer root, observability logs) all live inside
    ///   `tmpdir`. Returns `(Daemon, DaemonHandle)`.
    ///
    /// The returned Daemon is fully usable; no adapter is bound. Tests
    /// that need an adapter call `handle.bind_adapter(...)` after
    /// construction.
    pub fn new_for_tests(tmpdir: &std::path::Path) -> (Self, DaemonHandle) {
        use crate::config::*;
        let data_dir = tmpdir.join("data");
        let _ = std::fs::create_dir_all(&data_dir);
        let socket_dir = tmpdir.join("sock");
        let _ = std::fs::create_dir_all(&socket_dir);

        let cfg = WhatsAppRuntimeConfig {
            name: "test".into(),
            data_dir,
            log_dir: tmpdir.join("logs"),
            socket_dir,
            media_buffer: MediaBufferConfig {
                root: tmpdir.join("media"),
                ..Default::default()
            },
            events: EventsConfig::default(),
            security: SecurityConfig {
                grace_path: Some(tmpdir.join("data/grace.json")),
                ..Default::default()
            },
            observability: ObservabilityConfig::default(),
            rules: RulesConfig {
                storage_path: tmpdir.join("data/rules.toml"),
                wal_path: Some(tmpdir.join("data/rules.wal")),
                ..Default::default()
            },
            account_id: "default".into(),
            groups: Vec::new(),
            sender_allowlist: std::collections::BTreeMap::new(),
            query: crate::config::QueryConfig::default(),
        };

        // Open the store at tmpdir/data/index.json — NOT via open_default().
        // `MultiAccountStore::open` only materializes the file on the
        // first mutation; tests assert the path exists immediately
        // (hermetic invariant: no global filesystem side-effects), so
        // touch an empty-but-valid index file up-front.
        let index_path = tmpdir.join("data/index.json");
        if !index_path.exists() {
            let _ = std::fs::write(&index_path, br#"{"accounts":{}}"#);
        }
        let accounts =
            MultiAccountStore::open(&index_path).expect("MultiAccountStore::open(tmpdir)");

        let daemon = Self::new_internal(cfg, Some(accounts));
        let handle = daemon.handle();
        (daemon, handle)
    }

    /// Phase 5 Part B: canonical API version string. Bumped from
    /// `1.0.0+phase4` when Part A landed. The phase suffix
    /// communicates to operators which observability/security
    /// surfaces are guaranteed to exist.
    pub const fn version() -> &'static str {
        "1.0.0+phase5"
    }

    /// Private constructor — takes a pre-opened `MultiAccountStore` to
    /// bypass the `open_default()` filesystem read. Used by both
    /// `new` (production) and `new_for_tests` (hermetic).
    fn new_internal(config: WhatsAppRuntimeConfig, accounts: Option<MultiAccountStore>) -> Self {
        let cancel = CancellationToken::new();
        let handle = Self::build_handle(&config, &cancel, accounts);
        Self {
            config,
            cancel,
            handle,
        }
    }

    /// Build the [`DaemonHandle`] for `config` + `cancel` + optional
    /// `accounts` store. All filesystem writes/reads (data dir,
    /// socket dir, media buffer, observability logs, rules.toml +
    /// WAL, MultiAccountStore index) are derived from `config` — so
    /// `new_for_tests` simply passes a config whose paths are rooted
    /// under `tmpdir`.
    fn build_handle(
        config: &WhatsAppRuntimeConfig,
        cancel: &CancellationToken,
        accounts: Option<MultiAccountStore>,
    ) -> DaemonHandle {
        let media_buffer = MediaBuffer::new(
            config.media_buffer.max_concurrent_uploads,
            config.media_buffer.root.clone(),
        );
        let events_buffer = EventsBuffer::new(config.events.max_rows);
        // Phase 3 Part D: spawn the disk persister. The actor
        // owns a tokio task that mirrors in-memory events to
        // `$data_dir/events/events.ndjson` (override via
        // `events.persistence_path`). Cold-start reload is
        // synchronous inside `spawn` so the buffer is hydrated
        // before this function returns.
        let mut events_persister: Option<crate::events_persister::PersisterIngress> = None;
        if config.events.persistence_enabled {
            let path = config.events.resolved_persistence_path(&config.data_dir);
            match crate::events_persister::EventsPersisterHandle::spawn(
                events_buffer.clone(),
                Some(path),
                std::time::Duration::from_millis(config.events.flush_interval_ms),
                cancel.clone(),
            ) {
                Ok(handle) => {
                    let persister_token = cancel.clone();
                    let ingress = handle.ingress();
                    events_persister = Some(ingress.clone());
                    EVENTS_PERSISTER.with(|cell| {
                        *cell.borrow_mut() = Some((persister_token, handle));
                    });
                    tracing::info!(
                        path = %config.events.resolved_persistence_path(&config.data_dir).display(),
                        "events_persister: spawned"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e,
                        "events_persister: spawn failed; running with in-memory only");
                }
            }
        }
        // Phase 5 Part B: Prometheus registry materialized first so
        // we can attach it to AuditLog / RuleStore / TriggerStore.
        let label_secret = config
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
            config.security.audit_max_rows,
            config.security.audit_anchor_every,
        )
        .with_metrics(metrics.clone());
        let mutation_rl = Arc::new(MutationRateLimiter::new(10)); // 10/min per caller
        let trigger_store = Arc::new(TriggerStore::new().with_metrics(metrics.clone()));
        // Phase 5 Part A: TokenStore. Default grace_path is
        // `$data_dir/tokens/grace.json` if the user did not override.
        let grace_path = config
            .security
            .grace_path
            .clone()
            .unwrap_or_else(|| config.data_dir.join("tokens").join("grace.json"));
        let tokens = Arc::new(TokenStore::new(
            Some(grace_path),
            config.security.grace_period_ms,
        ));
        // Best-effort initial load: env var unset leaves the store empty
        // (hermetic tests). Env var set with malformed contents logs a
        // warning via the descriptor's `label`.
        let _ = tokens.load_from_env(&config.security.bearer_token_env, Some("bootstrap"));
        let _ = tokens.load_grace();
        // Phase 5 Part C: rules persistence. The persister writes
        // the ruleset atomically to `rules.toml` with a SHA-256
        // chained WAL. The JoinHandle lives outside `DaemonInner`
        // (in `Daemon`) so the supervisor can await the actor's
        // exit during shutdown drain.
        let storage_path = config.rules.resolved_storage_path();
        let wal_path = config.rules.resolved_wal_path();
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
            RulesPersister::spawn(storage_path, wal_path, config.rules.debounce_ms);
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
            config.rules.resolved_storage_path(),
            rules_persister.clone(),
        );
        let rs = RuleStore::new(config.security.auto_approve_rules)
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
                config: config.clone(),
                cancel: cancel.clone(),
                phase: std::sync::RwLock::new(DaemonPhase::Booting),
                media_buffer,
                adapter: std::sync::RwLock::new(None),
                events_buffer,
                events_router: parking_lot::RwLock::new(None),
                events_persister,
                #[cfg(all(feature = "query", any(test, feature = "test-helpers")))]
                test_embedder: std::sync::RwLock::new(None),
                #[cfg(feature = "query")]
                query_subsystem: std::sync::OnceLock::new(),
                #[cfg(feature = "query")]
                query_service: std::sync::OnceLock::new(),
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

    /// Return the cached [`DaemonHandle`]. The handle is built once
    /// at construction time (in [`Daemon::new_internal`]) so callers
    /// may invoke `handle()` repeatedly without re-running the
    /// expensive boot sequence (rules persister spawn, metrics
    /// init, audit log anchor, MultiAccountStore open, etc.).
    pub fn handle(&self) -> DaemonHandle {
        self.handle.clone()
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
        // Phase 3 Part D: drain the events persister so the last
        // batch of in-flight events gets a final fsync.
        crate::daemon::drain_events_persister().await;
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
    /// `Daemon` instance (during `Daemon::new` / `new_internal`);
    /// the supervisor awaits each handle during shutdown so the
    /// persister can drain.
    ///
    /// We use a thread_local rather than storing the handle inside
    /// `DaemonInner` because `Arc<DaemonInner>` is shared with the
    /// IPC handlers; moving the handle there would force `'static`
    /// bound on every handler that needs to read it. The thread
    /// local is owned by the supervisor thread that called
    /// `Daemon::new` (typically the runtime entry point).
    static PERSISTER_HANDLES: std::cell::RefCell<Vec<tokio::task::JoinHandle<()>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Phase 3 Part D: optional events disk persister. `Some`
    /// when `events.persistence_enabled` is true (the default).
    /// Drains on shutdown via `drain_events_persister`.
    static EVENTS_PERSISTER: std::cell::RefCell<
        Option<(
            tokio_util::sync::CancellationToken,
            crate::events_persister::EventsPersisterHandle,
        )>,
    > = const { std::cell::RefCell::new(None) };
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

/// Phase 3 Part D: drain the events persister if one was spawned.
/// Cancels its cancellation token (triggers the actor's drain + final
/// fsync) and waits up to 5s for the join handle. Logs but does not
/// propagate a timeout.
pub(crate) async fn drain_events_persister() {
    let pair = EVENTS_PERSISTER.with(|cell| cell.borrow_mut().take());
    let Some((token, handle)) = pair else {
        return;
    };
    token.cancel();
    // Allow up to 5s for the actor to drain + flush. The actor's
    // cancel arm does a final sync_all(); if it doesn't return in
    // 5s the worst case is we lose the last few seconds of events
    // (we already lost at most flush_interval_ms anyway).
    if tokio::time::timeout(std::time::Duration::from_secs(5), handle.join())
        .await
        .is_err()
    {
        tracing::warn!("events_persister: drain timed out after 5s");
    }
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
    // The adapter broadcasts `format!("{:?}", event)` for every WA
    // lifecycle event. The actual format depends on the wacore
    // `Event` enum's Debug derive. Empirically, in current wacore
    // versions, the Debug impl produces `LoggedOut(LoggedOut { ... })`
    // WITHOUT a leading `Event::` prefix (i.e. only the variant name,
    // not the qualified type path). Other libraries or future
    // versions may produce `Event::LoggedOut(...)`. Accept both.
    //
    // Anchor: split on the first `(`, ` `, `{`, or `<` to get just
    // the identifier; strip the optional `Event::` prefix first.
    let without_prefix = raw.strip_prefix("Event::").unwrap_or(raw);
    let ident = without_prefix.split(['(', ' ', '{', '<']).next()?.trim();
    match ident {
        // `phase_changed = true` means the caller should also flip
        // `DaemonPhase` (Connected ↔ SessionLost). Pairing/PairingQr
        // don't move phase — boot is in progress, daemon is already
        // in Booting/SessionLost.
        "Connected" => Some((BotStateMirror::Connected, true)),
        "Disconnected" => Some((BotStateMirror::Disconnected, true)),
        // The wacore variants are `PairingQrCode` and `PairingCode`
        // (note the `Code` suffix on the QR variant). Earlier phases
        // had this wrong as `"PairingQr"` and the arm silently never
        // matched in production — only `LoggedOut` / `Connected` /
        // `Replaced` / `SessionExpired` paths were reachable. Phase
        // 6.12.5's hermetic test (`pairing_stall_timer_fires_*`)
        // exposed the bug.
        "PairingQrCode" => Some((BotStateMirror::PairingQr, false)),
        "PairingCode" => Some((BotStateMirror::PairingCode, false)),
        "LoggedOut" => Some((BotStateMirror::LoggedOut, true)),
        "Replaced" => Some((BotStateMirror::Replaced, true)),
        "SessionExpired" => Some((BotStateMirror::SessionExpired, true)),
        // Session 4 (RFC-0909, wacore-webauthn plan): the three
        // SHORTCAKE_PASSKEY events. The server asked for a WebAuthn
        // assertion (Request) or reached the final verification
        // stage (Confirmation); both keep us in `AwaitingPasskey`
        // until the assertion resolves. `PairPasskeyError` is
        // terminal — the link failed and the operator must restart,
        // so we advance to `LoggedOut` and move the daemon phase to
        // `SessionLost`.
        "PairPasskeyRequest" => Some((BotStateMirror::AwaitingPasskey, false)),
        "PairPasskeyConfirmation" => Some((BotStateMirror::AwaitingPasskey, false)),
        "PairPasskeyError" => Some((BotStateMirror::LoggedOut, true)),
        _ => None,
    }
}

/// Default stall threshold: if no terminal pairing event arrives
/// within this window after the QR/code is rendered, fire
/// `BotStateMirror::AwaitingUserAction`. wacore 0.6.0 does not surface
/// WebAuthn / passkey / 2FA-PIN events; the operator must complete the
/// prompt on the phone before the state advances.
pub const PAIRING_STALL_SECS: u64 = 45;

/// Long-running task spawned at adapter-bind time. Loops over the
/// broadcast receiver, applies `BotStateMirror` transitions, and
/// observes cancellation. Survives `Event::Lagged` (channel overflow)
/// by continuing — events lost during overflow are non-critical.
///
/// Maintains a per-task pairing stall timer: set when a `PairingQr` /
/// `PairingCode` event is observed, cleared by any terminal event
/// (`Connected` / `Disconnected` / `LoggedOut` / `Replaced` /
/// `SessionExpired`). If the timer fires before a terminal event, the
/// state machine advances to `AwaitingUserAction` so operators see a
/// truthful `status.get` even when the SDK has stalled behind a
/// phone-side second-verification prompt.
async fn run_connection_watcher(
    rx: tokio::sync::broadcast::Receiver<String>,
    handle: DaemonHandle,
    cancel: CancellationToken,
) {
    run_connection_watcher_inner(
        rx,
        handle,
        cancel,
        std::time::Duration::from_secs(PAIRING_STALL_SECS),
    )
    .await
}

/// Testable inner: takes the stall threshold explicitly so unit
/// tests can use a sub-second timeout (the production const is 45s,
/// which would dominate test wall-clock).
async fn run_connection_watcher_inner(
    mut rx: tokio::sync::broadcast::Receiver<String>,
    handle: DaemonHandle,
    cancel: CancellationToken,
    stall_after: std::time::Duration,
) {
    use tokio::sync::broadcast::error::RecvError;
    use tokio::time::Instant;

    tracing::info!("connection watcher: task spawned, awaiting events");

    // `None` when no pairing is in progress; `Some(t)` otherwise. Set
    // by `PairingQr` / `PairingCode` classifier arms, cleared by any
    // terminal classifier arm. The watcher fires `AwaitingUserAction`
    // when `now - t >= stall_after`.
    let mut pairing_started_at: Option<Instant> = None;

    loop {
        // Compute the timeout for the current `select!` so the stall
        // timer fires deterministically per iteration (not via a
        // separate spawned timer that would race against `rx.recv`).
        let stall_deadline = pairing_started_at.map(|t| t + stall_after);
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("connection watcher: cancel observed");
                break;
            }
            _ = async {
                match stall_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                tracing::warn!(
                    stall_secs = ?stall_after,
                    "connection watcher: pairing stalled (no terminal event); \
                     firing AwaitingUserAction; hint: {hint}",
                    hint = AWAITING_USER_ACTION_HINT,
                );
                handle.set_bot_state(BotStateMirror::AwaitingUserAction);
                // Phase is unchanged — daemon remains in `Booting`
                // until either Connected (happy path) or a terminal
                // SessionLost-class event arrives. Pairing stall
                // doesn't move phase because the bot hasn't been
                // linked yet.
                pairing_started_at = None;
            }
            recv = rx.recv() => {
                match recv {
                    Ok(raw) => {
                        tracing::debug!(
                            raw = %raw,
                            "connection watcher: raw event received"
                        );
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
                            // Manage the pairing stall timer in lockstep
                            // with the state machine.
                            match mirror {
                                BotStateMirror::PairingQr
                                | BotStateMirror::PairingCode => {
                                    pairing_started_at = Some(Instant::now());
                                }
                                BotStateMirror::Connected
                                | BotStateMirror::Disconnected
                                | BotStateMirror::Replaced
                                | BotStateMirror::LoggedOut
                                | BotStateMirror::SessionExpired
                                | BotStateMirror::AwaitingUserAction
                                | BotStateMirror::AwaitingPasskey => {
                                    pairing_started_at = None;
                                }
                            }
                        } else {
                            // DEBUG (Phase 6.12.5): log unclassified events so
                            // we can diagnose format mismatches between the
                            // adapter's `format!("{:?}", event)` and the
                            // classify_event prefix-based matcher.
                            tracing::warn!(
                                raw = %raw,
                                "connection watcher: received event did not classify"
                            );
                        }
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
