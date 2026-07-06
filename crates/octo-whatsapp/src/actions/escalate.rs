//! `Escalate` action. Bumps priority + sends to a named target.

use super::{ActionContext, ActionError};

/// Phase 4 stub. Real implementation lands in Phase 5 once
/// `actions.escalate` has its own transport (PagerDuty, Slack, or
/// custom). For now this is a structural placeholder.
pub async fn execute(target: &str, reason: &str, _ctx: &ActionContext) -> Result<(), ActionError> {
    if target.is_empty() {
        return Err(ActionError::ExecFailed("empty target".into()));
    }
    if reason.is_empty() {
        return Err(ActionError::ExecFailed("empty reason".into()));
    }
    Ok(())
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
        }
    }

    #[tokio::test]
    async fn rejects_empty_target() {
        assert!(execute("", "reason", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn rejects_empty_reason() {
        assert!(execute("oncall", "", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn accepts_valid_args() {
        assert!(execute("oncall", "r", &ctx()).await.is_ok());
    }
}
