//! `AgentRun` action. Invokes a registered trigger by id.

use super::{ActionContext, ActionError};

/// Phase 4 stub: structural validation only. Real invocation
/// requires access to the `TriggerStore`; for handler-level
/// dispatch we route through `ActionContext::caller_uid`'s
/// daemon handle. This stub exists so the dispatcher type-checks
/// for handlers that don't carry the daemon handle (e.g. unit
/// tests).
pub async fn execute(trigger_id: &str, _ctx: &ActionContext) -> Result<(), ActionError> {
    if trigger_id.is_empty() {
        return Err(ActionError::ExecFailed("empty trigger_id".into()));
    }
    if trigger_id.len() > 64 {
        return Err(ActionError::ExecFailed("trigger_id too long".into()));
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
            metrics: None,
        }
    }

    #[tokio::test]
    async fn rejects_empty() {
        assert!(execute("", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn accepts_valid_id() {
        assert!(execute("trigger-1", &ctx()).await.is_ok());
    }
}
