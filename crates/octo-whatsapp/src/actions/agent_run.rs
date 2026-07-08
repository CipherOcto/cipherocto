//! `AgentRun` action. Phase 5 Part F: invokes a registered trigger
//! by id, returning the trigger's run record on success.

use super::{ActionContext, ActionError};

/// Phase 4 stub: kept for hermetic tests where the dispatcher
/// doesn't carry a daemon handle. Real invocation happens via
/// [`dispatch`].
pub async fn execute(trigger_id: &str, _ctx: &ActionContext) -> Result<(), ActionError> {
    if trigger_id.is_empty() {
        return Err(ActionError::ExecFailed("empty trigger_id".into()));
    }
    if trigger_id.len() > 64 {
        return Err(ActionError::ExecFailed("trigger_id too long".into()));
    }
    Ok(())
}

/// Phase 5 Part F: production invocation. Calls
/// `ctx.daemon.triggers().run(trigger_id, &event, ctx.now_ms)` and
/// surfaces errors as `ActionError`.
pub async fn dispatch(trigger_id: &str, ctx: &ActionContext) -> Result<(), ActionError> {
    if trigger_id.is_empty() {
        return Err(ActionError::ExecFailed("empty trigger_id".into()));
    }
    if trigger_id.len() > 64 {
        return Err(ActionError::ExecFailed("trigger_id too long".into()));
    }
    ctx.daemon
        .triggers()
        .run(trigger_id, &ctx.event, ctx.now_ms)
        .await
        .map(|_record| ())
        .map_err(|e| ActionError::ExecFailed(format!("trigger {trigger_id}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionContext;
    use crate::daemon::Daemon;
    use crate::events::{InboundEvent, MessageKind};
    use std::sync::Arc;

    fn handle() -> crate::daemon::DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn ctx() -> ActionContext {
        ActionContext {
            rule_id: "r".into(),
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
                is_group: false,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
            daemon: handle(),
            metrics: None,
        }
    }

    #[tokio::test]
    async fn rejects_empty_id_dispatch() {
        let err = dispatch("", &ctx()).await.unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    async fn rejects_overlong_id_dispatch() {
        let long = "x".repeat(65);
        let err = dispatch(&long, &ctx()).await.unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    async fn missing_trigger_id_errors_dispatch() {
        let err = dispatch("not-a-real-trigger", &ctx()).await.unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    async fn rejects_empty_id_execute() {
        assert!(execute("", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn accepts_valid_id_execute() {
        assert!(execute("trigger-1", &ctx()).await.is_ok());
    }
}
