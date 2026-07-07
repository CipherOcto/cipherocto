//! `Shell` action. Phase 4 of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`
//! §Security: "Trigger runner sandboxing".
//!
//! Cross-platform: on Linux, delegates to `runner::shell_linux::run`
//! with full Landlock+seccomp+rlimit+pidfd sandbox. On other
//! platforms, returns `NotSupported` and refuses to exec (fail
//! closed, design §Security).

use super::{ActionContext, ActionError};
use crate::actions::runner;

pub async fn execute(
    argv: &[String],
    timeout_ms: u64,
    env_passthrough: &[String],
    _ctx: &ActionContext,
) -> Result<(), ActionError> {
    if argv.is_empty() {
        return Err(ActionError::ExecFailed("empty argv".into()));
    }
    if timeout_ms == 0 {
        return Err(ActionError::ExecFailed("timeout_ms must be > 0".into()));
    }
    runner::run_shell(argv, timeout_ms, env_passthrough).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionContext;
    use crate::events::{InboundEvent, MessageKind};
    use std::sync::Arc;

    fn ctx() -> ActionContext {
        ActionContext {
            rule_id: "r".into(),
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
                is_group: false,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
            metrics: None,
        }
    }

    #[tokio::test]
    async fn empty_argv_errors() {
        let err = execute(&[], 1000, &[], &ctx()).await.unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    async fn zero_timeout_errors() {
        let err = execute(&["echo".into()], 0, &[], &ctx()).await.unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    #[cfg(not(target_os = "linux"))]
    async fn non_linux_returns_not_supported() {
        let err = execute(&["echo".into()], 1000, &[], &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::NotSupported(_)));
    }
}
