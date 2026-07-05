//! Long-lived daemon. Owns the adapter, the unix-socket server, the
//! event router stub, and the shared stoolap handle.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::WhatsAppRuntimeConfig;
use crate::media_buffer::MediaBuffer;

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

#[derive(Debug)]
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

    /// Async-marked for API symmetry with future async setters, but the
    /// underlying lock is sync (`std::sync::RwLock`) so this only does a
    /// single instantaneous write. The crate's
    /// `#![warn(clippy::await_holding_lock)]` does not bite: this is a
    /// terminal op, not a held-across-await pattern.
    pub async fn set_phase(&self, p: DaemonPhase) {
        let mut g = self.inner.phase.write().unwrap_or_else(|p| p.into_inner());
        *g = p;
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
        let mb_cfg = self.config.media_buffer.clone().unwrap_or_default();
        let media_buffer = MediaBuffer::new(mb_cfg.max_concurrent_uploads, mb_cfg.root);
        DaemonHandle {
            inner: Arc::new(DaemonInner {
                config: self.config.clone(),
                cancel: self.cancel.clone(),
                phase: std::sync::RwLock::new(DaemonPhase::Booting),
                media_buffer,
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
