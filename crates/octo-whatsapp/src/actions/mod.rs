//! Action dispatchers. Phase 5 Part F of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §Triggers
//! + §Security.
//!
//! Each rule carries an ordered list of `ActionSpec` values. When a
//! rule fires, the rule engine dispatches each action via the
//! appropriate submodule:
//! - [`webhook`] — HTTP POST with HMAC signature, TLS-only.
//! - [`agent_run`] — invoke a registered trigger.
//! - [`shell`] — sandboxed subprocess.
//! - [`mcp_notify`] — push event to MCP clients.
//! - [`escalate`] — bump priority + send to a named target.
//!
//! All dispatchers emit an audit row on success and failure, and
//! support per-action timeouts via `tokio::time::timeout`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::daemon::DaemonHandle;
use crate::events::InboundEvent;
use crate::observability::metrics::Metrics;
use crate::rules::ActionSpec;

pub mod agent_run;
pub mod escalate;
pub mod mcp_notify;
pub mod runner;
pub mod shell;
pub mod webhook;

/// Per-action execution context. The rule engine builds one of these
/// and passes it to the dispatcher.
///
/// Phase 5 Part F: carries a `DaemonHandle` clone so dispatchers can
/// reach the `reqwest::Client` (webhook), the `TriggerStore` (agent_run),
/// the MCP client registry (mcp_notify), the audit log, and the events
/// buffer. The handle is cheap to clone (Arc-bumped inner).
#[derive(Debug, Clone)]
pub struct ActionContext {
    pub rule_id: String,
    /// Phase 5 Part F: monotonically increasing version of the rule
    /// that fired this action. Captured for audit purposes.
    pub rule_version: u64,
    pub event: Arc<InboundEvent>,
    pub caller_uid: String,
    pub now_ms: i64,
    /// Phase 5 Part F: cheap-clone handle to the daemon. Set by
    /// `EventsRouter`/rule dispatcher in production; tests may
    /// pass a minimal handle from `Daemon::new(test_cfg).handle()`.
    pub daemon: DaemonHandle,
    /// Phase 5 Part B: optional Prometheus registry. When set,
    /// `dispatch()` increments `outbound_messages_total{kind,result}`.
    /// Existing callers (tests) pass `None`.
    pub metrics: Option<Arc<Metrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub status: String, // "ok" | "error"
    pub detail: Option<String>,
    pub latency_ms: u64,
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("action disabled: {0}")]
    Disabled(String),
    #[error("not supported on this platform: {0}")]
    NotSupported(String),
    #[error("webhook not configured: {0}")]
    WebhookNotConfigured(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("timeout after {0}ms")]
    Timeout(u64),
    #[error("execution failed: {0}")]
    ExecFailed(String),
    #[error("rate limited")]
    RateLimited,
}

/// Executes a single `ActionSpec`. The rule engine drives this in a
/// `tokio::time::timeout(deadline)` wrapper.
///
/// Phase 5 Part F: dispatchers now read & mutate daemon state via
/// `ctx.daemon` — webhook hits the shared `reqwest::Client`,
/// `agent_run` invokes `TriggerStore::run`, `shell` runs the
/// Linux/Other sandbox, `mcp_notify` fans out to subscribed MCP
/// clients, `escalate` bumps the rule's priority and emits an audit
/// row.
pub async fn dispatch(spec: &ActionSpec, ctx: &ActionContext) -> Result<ActionResult, ActionError> {
    let start = std::time::Instant::now();
    let res = match spec {
        ActionSpec::Webhook {
            url,
            signing_secret_env,
            allowed_domains,
        } => webhook::dispatch(url, signing_secret_env.as_deref(), allowed_domains, ctx).await,
        ActionSpec::AgentRun { trigger_id } => agent_run::dispatch(trigger_id, ctx).await,
        ActionSpec::Shell {
            argv,
            timeout_ms,
            env_passthrough,
        } => shell::dispatch(argv, *timeout_ms, env_passthrough, ctx).await,
        ActionSpec::McpNotify { template } => mcp_notify::dispatch(template, ctx).await,
        ActionSpec::Escalate { target, reason } => escalate::dispatch(target, reason, ctx).await,
    };
    let latency_ms = start.elapsed().as_millis() as u64;
    // Phase 5 Part B: increment outbound metric once per dispatch.
    if let Some(m) = &ctx.metrics {
        let kind = match spec {
            ActionSpec::Webhook { .. } => "webhook",
            ActionSpec::AgentRun { .. } => "agent_run",
            ActionSpec::Shell { .. } => "shell",
            ActionSpec::McpNotify { .. } => "mcp_notify",
            ActionSpec::Escalate { .. } => "escalate",
        };
        let result_label = if res.is_ok() { "ok" } else { "error" };
        m.inc_outbound(kind, result_label);
    }
    match res {
        Ok(()) => Ok(ActionResult {
            status: "ok".into(),
            detail: None,
            latency_ms,
        }),
        Err(e) => Err(e),
    }
}

/// Backwards-compatible wrapper for tests/handlers that haven't been
/// updated to thread a `DaemonHandle`. Builds an `ActionContext`
/// without a daemon handle — only valid for dispatch paths that
/// don't touch shared state (e.g. unit tests with structural-only
/// dispatchers).
#[cfg(any(test, feature = "test-helpers"))]
pub async fn dispatch_structural(
    spec: &ActionSpec,
    ctx: &ActionContext,
) -> Result<ActionResult, ActionError> {
    let start = std::time::Instant::now();
    let res = match spec {
        ActionSpec::Webhook {
            url,
            signing_secret_env,
            allowed_domains,
        } => webhook::execute(url, signing_secret_env.as_deref(), allowed_domains, ctx).await,
        ActionSpec::AgentRun { trigger_id } => agent_run::execute(trigger_id, ctx).await,
        ActionSpec::Shell {
            argv,
            timeout_ms,
            env_passthrough,
        } => shell::execute(argv, *timeout_ms, env_passthrough, ctx).await,
        ActionSpec::McpNotify { template } => mcp_notify::execute(template, ctx).await,
        ActionSpec::Escalate { target, reason } => escalate::execute(target, reason, ctx).await,
    };
    let latency_ms = start.elapsed().as_millis() as u64;
    match res {
        Ok(()) => Ok(ActionResult {
            status: "ok".into(),
            detail: None,
            latency_ms,
        }),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::events::MessageKind;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn ctx() -> ActionContext {
        ActionContext {
            rule_id: "r1".into(),
            rule_version: 1,
            event: Arc::new(InboundEvent::Message {
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
                from_me: false,
                is_group: false,
                view_once: false,
                ephemeral_expires_at_seconds: None,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
            daemon: handle(),
            metrics: None,
        }
    }

    #[tokio::test]
    async fn mcp_notify_production_dispatch_with_no_clients() {
        let r = dispatch(
            &ActionSpec::McpNotify {
                template: "msg".into(),
            },
            &ctx(),
        )
        .await
        .unwrap();
        assert_eq!(r.status, "ok");
    }

    #[tokio::test]
    async fn escalate_production_dispatch_succeeds() {
        let r = dispatch(
            &ActionSpec::Escalate {
                target: "oncall".into(),
                reason: "x".into(),
            },
            &ctx(),
        )
        .await
        .unwrap();
        assert_eq!(r.status, "ok");
    }

    #[tokio::test]
    async fn webhook_without_secret_refused() {
        let err = dispatch(
            &ActionSpec::Webhook {
                url: "https://example.com/h".into(),
                signing_secret_env: None,
                allowed_domains: vec!["example.com".into()],
            },
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::WebhookNotConfigured(_)));
    }

    #[tokio::test]
    async fn shell_non_linux_returns_not_supported() {
        #[cfg(not(target_os = "linux"))]
        {
            let err = dispatch(
                &ActionSpec::Shell {
                    argv: vec!["echo".into()],
                    timeout_ms: 1000,
                    env_passthrough: vec![],
                },
                &ctx(),
            )
            .await
            .unwrap_err();
            assert!(matches!(err, ActionError::NotSupported(_)));
        }
    }
}
