//! Long-lived daemon. Owns the adapter, the unix-socket server, the
//! event router stub, and the shared stoolap handle.

use std::sync::Arc;

use octo_adapter_whatsapp::WhatsAppWebAdapter;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::WhatsAppRuntimeConfig;

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
    phase: tokio::sync::RwLock<DaemonPhase>,
}

impl DaemonHandle {
    pub fn phase(&self) -> DaemonPhase {
        *self.inner.phase.blocking_read()
    }

    pub fn config(&self) -> &WhatsAppRuntimeConfig {
        &self.inner.config
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
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
        DaemonHandle {
            inner: Arc::new(DaemonInner {
                config: self.config.clone(),
                cancel: self.cancel.clone(),
                phase: tokio::sync::RwLock::new(DaemonPhase::Booting),
            }),
        }
    }

    /// Clone of the daemon's cancellation token. Used by tests and by
    /// supervisor code to trigger shutdown without holding `&Daemon`.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub async fn run(self, _adapter: WhatsAppWebAdapter) -> anyhow::Result<()> {
        info!(
            name = self.config.name.as_str(),
            "daemon stub: exiting immediately"
        );
        // Phase 1 stub: real boot arrives in Task 26.
        Ok(())
    }
}

/// Tests live in their own file so the unit-test surface stays narrow.
#[cfg(test)]
mod tests;
