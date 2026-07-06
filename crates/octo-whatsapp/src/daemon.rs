//! Long-lived daemon. Owns the adapter, the unix-socket server, the
//! event router stub, and the shared stoolap handle.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::adapter_trait::OctoWhatsAppAdapter;
use crate::audit::AuditLog;
use crate::config::WhatsAppRuntimeConfig;
use crate::events_persister::EventsBuffer;
use crate::ipc::handlers::clients::McpClientRegistry;
use crate::media_buffer::MediaBuffer;
use crate::rules::{MutationRateLimiter, RuleStore};
use crate::triggers::TriggerStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonPhase {
    Booting,
    Connected,
    SessionLost,
    ShuttingDown,
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
    }

    /// Test/feature helper for binding a mock adapter. Sync write —
    /// call before any await point. Mirrors the sync `RwLock` pattern
    /// of `set_phase`.
    ///
    /// Gated on `#[cfg(any(test, feature = "test-helpers"))]` so the
    /// setter never ships in default builds.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_adapter_for_tests(&self, a: Arc<dyn OctoWhatsAppAdapter>) {
        *self
            .inner
            .adapter
            .write()
            .unwrap_or_else(|p| p.into_inner()) = Some(a);
    }

    /// Phase 3: read access to the in-memory events ring buffer. The
    /// event router populates this; `events.list/show/replay` RPC
    /// handlers consult it.
    pub fn events_buffer(&self) -> &Arc<EventsBuffer> {
        &self.inner.events_buffer
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
    pub fn new(config: WhatsAppRuntimeConfig) -> Self {
        Self {
            config,
            cancel: CancellationToken::new(),
        }
    }

    pub fn handle(&self) -> DaemonHandle {
        let media_buffer = MediaBuffer::new(
            self.config.media_buffer.max_concurrent_uploads,
            self.config.media_buffer.root.clone(),
        );
        let events_buffer = EventsBuffer::new(self.config.events.max_rows);
        let audit_log = AuditLog::new(
            self.config.security.audit_max_rows,
            self.config.security.audit_anchor_every,
        );
        let rule_store = Arc::new(RuleStore::new(self.config.security.auto_approve_rules));
        let mutation_rl = Arc::new(MutationRateLimiter::new(10)); // 10/min per caller
        let trigger_store = Arc::new(TriggerStore::new());
        DaemonHandle {
            inner: Arc::new(DaemonInner {
                config: self.config.clone(),
                cancel: self.cancel.clone(),
                phase: std::sync::RwLock::new(DaemonPhase::Booting),
                media_buffer,
                adapter: std::sync::RwLock::new(None),
                events_buffer,
                clients: McpClientRegistry::new(),
                rules: rule_store,
                mutation_rl,
                triggers: trigger_store,
                audit_log: Arc::new(audit_log),
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

        let registry = std::sync::Arc::new(crate::ipc::handlers::build_registry());
        let sock = self.config.socket_path();
        let server = crate::ipc::server::UnixSocketServer::bind(&sock)?;
        let server_task = {
            let cancel = cancel.clone();
            let handle = handle.clone();
            tokio::spawn(async move { server.serve(handle, registry, cancel).await })
        };

        cancel.cancelled().await;
        info!("daemon: cancel observed; waiting for server to drain");
        let _ = server_task.await;
        info!("daemon: exited");
        Ok(())
    }
}

/// Tests live in their own file so the unit-test surface stays narrow.
#[cfg(test)]
mod tests;
